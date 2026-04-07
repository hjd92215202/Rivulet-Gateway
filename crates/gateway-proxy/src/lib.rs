//! proxy 层负责把“路由结果”真正变成一次转发行为。
//! 第一阶段只支持保守的 HTTP/1.1 代理语义，重点是把边界条件和错误路径做扎实。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Instant;

use gateway_admin::AdminService;
use gateway_filters::FilterRegistry;
use gateway_router::Router;
use gateway_types::{
    GatewayError, HttpMethod, RequestContext, ResolvedProxyPolicy, ResponseContext, Result,
    RuntimeSettings,
};
use gateway_upstream::{EndpointState, UpstreamRegistry};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub struct ProxyService {
    /// 路由器负责把请求映射到路由定义。
    router: Router,
    /// 过滤器注册表负责执行请求前后钩子。
    filters: FilterRegistry,
    /// upstream 注册表负责选出可用节点。
    upstreams: UpstreamRegistry,
    /// 运行时超时、重试和协议限制。
    timeouts: RuntimeSettings,
    /// 只读管理面，优先拦截内置管理路径。
    admin: Option<AdminService>,
    /// 路由级内存限流器，先用于公开页面保护和保守型入口防刷。
    rate_limiter: RequestRateLimiter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedRequest {
    /// 完成转发后的请求上下文。
    pub request: RequestContext,
    /// 完成转发后的响应上下文。
    pub response: ResponseContext,
    /// 本次请求实际发生的额外重试次数。
    pub retries: usize,
    /// 响应回写完成后是否应该关闭当前 downstream 连接。
    pub close_downstream: bool,
}

#[derive(Debug)]
pub struct ProxyConnectionError {
    /// 具体错误。
    pub error: GatewayError,
    /// 如果请求已经解析出来，就把上下文带给日志层。
    pub request: Option<RequestContext>,
    /// 错误发生前已经消耗的重试次数。
    pub retries: usize,
}

/// 第一版限流器只维护固定窗口计数。
/// 这里刻意不引入外部存储，让生产前期先把语义、观测和错误边界做稳。
struct RequestRateLimiter {
    buckets: Mutex<HashMap<String, RateLimitBucket>>,
}

#[derive(Clone, Debug)]
struct RateLimitBucket {
    /// 当前计数窗口的结束时刻。
    window_ends_at: Instant,
    /// 当前窗口内已经接收的请求数。
    observed_requests: usize,
}

impl RequestRateLimiter {
    fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    fn check(
        &self,
        route_name: &str,
        route: &gateway_types::RouteRateLimitPolicy,
        request: &RequestContext,
    ) -> Result<()> {
        if !route.is_enabled() {
            return Ok(());
        }

        let allowed_requests = route
            .requests
            .expect("enabled rate limit should carry requests");
        let window = route
            .window
            .expect("enabled rate limit should carry window");
        let bucket_key = format!("{}:{}", route_name, route.key_for(request));
        let now = Instant::now();

        let mut buckets = self
            .buckets
            .lock()
            .expect("rate limit buckets mutex poisoned");
        // 当桶数量增长到一定规模时，先清理已经过期的窗口，避免入口保护反过来造成无界状态膨胀。
        if buckets.len() >= 16_384 {
            buckets.retain(|_, bucket| bucket.window_ends_at > now);
        }

        let bucket = buckets
            .entry(bucket_key)
            .or_insert_with(|| RateLimitBucket {
                window_ends_at: now + window,
                observed_requests: 0,
            });

        if now >= bucket.window_ends_at {
            bucket.window_ends_at = now + window;
            bucket.observed_requests = 0;
        }

        if bucket.observed_requests >= allowed_requests {
            return Err(GatewayError::RateLimited(format!(
                "route {} exceeded {} requests per {:?}",
                route_name, allowed_requests, window
            )));
        }

        bucket.observed_requests += 1;
        Ok(())
    }
}

impl ProxyService {
    pub fn new(
        router: Router,
        filters: FilterRegistry,
        upstreams: UpstreamRegistry,
        timeouts: RuntimeSettings,
    ) -> Self {
        Self::with_admin(router, filters, upstreams, timeouts, None)
    }

    pub fn with_admin(
        router: Router,
        filters: FilterRegistry,
        upstreams: UpstreamRegistry,
        timeouts: RuntimeSettings,
        admin: Option<AdminService>,
    ) -> Self {
        Self {
            router,
            filters,
            upstreams,
            timeouts,
            admin,
            rate_limiter: RequestRateLimiter::new(),
        }
    }

    pub async fn handle(&self, mut request: RequestContext) -> Result<ResponseContext> {
        // 这个入口主要给内存级测试和后续管理面复用。
        let route = self.router.resolve(&request)?;
        self.filters
            .run_before(&route.filter_names, &mut request)
            .await?;
        // 当前顺序先保留“请求过滤器 -> 鉴权”，
        // 这样未授权请求也能拿到 request id 等最小观测上下文。
        route.auth_policy.authorize(&request)?;
        self.rate_limiter
            .check(&route.route_name, &route.rate_limit_policy, &request)?;
        // 分享访问隔离在入口放行后立即落到请求上下文里，
        // 这样后面的日志、转发和上游识别都能看到稳定 share 元数据。
        route.share_policy.authorize(&mut request)?;

        // 纯内存入口只验证选路是否正确，不做真实网络转发。
        let selected = self
            .upstreams
            .cluster(&route.upstream_name)?
            .next_endpoint()?;

        let mut response = ResponseContext::new(200);
        response.upstream = Some(selected.address);

        // 响应后过滤器仍然要跑，保证这条路径和真实流量路径语义一致。
        self.filters
            .run_after(&route.filter_names, &mut response)
            .await?;

        Ok(response)
    }

    pub async fn handle_connection(
        &self,
        listener_name: &str,
        downstream: &mut TcpStream,
        client_addr: SocketAddr,
    ) -> std::result::Result<CompletedRequest, ProxyConnectionError> {
        // 这条链路是“真实网络请求”的主路径：
        // 读请求 -> 路由 -> 过滤器 -> 选上游 -> 转发 -> 回写响应。
        let request = HttpRequest::read_from(downstream, &self.timeouts)
            .await
            .map_err(|error| ProxyConnectionError {
                // 请求还没解析出来前，只能记录连接级错误。
                error,
                request: None,
                retries: 0,
            })?;
        // 一旦解析成功，就立刻构造标准请求上下文，后续所有逻辑都基于它运行。
        let mut request_context = RequestContext::new(
            listener_name,
            request.host_header(),
            request.path_for_route(),
            request.method,
        );
        // 把下游请求头和 query 快照带进统一上下文，给鉴权、限流、分享隔离类策略复用。
        request_context.query = request.query_for_policy();
        request_context.headers = request.headers.clone();
        // 客户端地址主要用于日志和补充 X-Forwarded-For。
        request_context.client_addr = Some(client_addr);

        // 内置管理面走只读短路路径，不再进入用户路由和上游转发逻辑。
        if let Some(admin) = &self.admin {
            if let Some(admin_response) = admin.maybe_handle(&request_context) {
                downstream
                    .write_all(&admin_response.bytes)
                    .await
                    .map_err(|err| ProxyConnectionError {
                        error: GatewayError::Io(format!("write admin response: {}", err)),
                        request: Some(request_context.clone()),
                        retries: 0,
                    })?;
                downstream
                    .flush()
                    .await
                    .map_err(|err| ProxyConnectionError {
                        error: GatewayError::Io(format!("flush admin response: {}", err)),
                        request: Some(request_context.clone()),
                        retries: 0,
                    })?;

                let mut response = ResponseContext::new(admin_response.status_code);
                response.upstream = Some("internal://admin-ui".into());

                return Ok(CompletedRequest {
                    request: request_context,
                    response,
                    retries: 0,
                    close_downstream: true,
                });
            }
        }

        // 路由失败时仍然保留请求上下文，方便 access log 告诉我们“为什么没命中”。
        let route =
            self.router
                .resolve(&request_context)
                .map_err(|error| ProxyConnectionError {
                    error,
                    request: Some(request_context.clone()),
                    retries: 0,
                })?;
        // 请求前过滤器统一放在转发前执行。
        self.filters
            .run_before(&route.filter_names, &mut request_context)
            .await
            .map_err(|error| ProxyConnectionError {
                error,
                request: Some(request_context.clone()),
                retries: 0,
            })?;
        route
            .auth_policy
            .authorize(&request_context)
            .map_err(|error| ProxyConnectionError {
                error,
                request: Some(request_context.clone()),
                retries: 0,
            })?;
        self.rate_limiter
            .check(
                &route.route_name,
                &route.rate_limit_policy,
                &request_context,
            )
            .map_err(|error| ProxyConnectionError {
                error,
                request: Some(request_context.clone()),
                retries: 0,
            })?;
        route
            .share_policy
            .authorize(&mut request_context)
            .map_err(|error| ProxyConnectionError {
                error,
                request: Some(request_context.clone()),
                retries: 0,
            })?;

        // 命中路由后，把目标 upstream 集群取出来作为后续所有尝试的候选集合。
        let cluster = self
            .upstreams
            .cluster(&route.upstream_name)
            .map_err(|error| ProxyConnectionError {
                error,
                request: Some(request_context.clone()),
                retries: 0,
            })?;
        // 记录这次请求已经尝试失败过的地址，避免重试回到同一个坏节点。
        // 这里是本次请求真正生效的转发策略，
        // 优先级固定为 route override -> upstream override -> runtime default。
        let policy = route
            .proxy_policy
            .or_else(cluster.proxy_policy())
            .resolve(&self.timeouts);
        let mut excluded_addresses = Vec::new();
        // 所有尝试都失败时，最终会从这里返回最后一个可解释错误。
        let mut final_outcome: Option<std::result::Result<CompletedRequest, ProxyConnectionError>> =
            None;

        for attempt in 0..policy.upstream_retry_attempts {
            // 每轮都从“健康且这次没失败过”的节点里选一个地址。
            let endpoint_state = match cluster.select_endpoint(&excluded_addresses) {
                Ok(endpoint) => endpoint,
                Err(error) if attempt > 0 => break,
                Err(error) => {
                    return Err(ProxyConnectionError {
                        error,
                        request: Some(request_context.clone()),
                        retries: attempt,
                    });
                }
            };

            let endpoint = endpoint_state.endpoint().clone();
            // 与单个 upstream 的交互细节都收口到独立函数里。
            let outcome = self
                .forward_to_endpoint(&endpoint_state, &request, &request_context, policy)
                .await;

            match outcome {
                Ok(upstream_response) => {
                    let status_code = upstream_response.status_code;
                    // 第一阶段只对少数典型 5xx 做重试，先避免把非幂等请求重试面铺得太大。
                    if is_retryable_status(status_code)
                        && attempt + 1 < policy.upstream_retry_attempts
                    {
                        endpoint_state.record_failure(cluster.passive_failure_threshold());
                        excluded_addresses.push(endpoint.address.clone());
                        final_outcome = Some(Err(ProxyConnectionError {
                            error: GatewayError::Io(format!(
                                "retryable upstream status {}",
                                status_code
                            )),
                            request: Some(request_context.clone()),
                            retries: attempt + 1,
                        }));
                        continue;
                    }

                    endpoint_state.record_success(cluster.passive_success_threshold());

                    // 当前模型仍然是“读完整个 upstream 响应，再一次性回写”。
                    downstream
                        .write_all(&upstream_response.bytes)
                        .await
                        .map_err(|err| ProxyConnectionError {
                            error: GatewayError::Io(format!("write downstream response: {}", err)),
                            request: Some(request_context.clone()),
                            retries: attempt,
                        })?;
                    downstream
                        .flush()
                        .await
                        .map_err(|err| ProxyConnectionError {
                            error: GatewayError::Io(format!("flush downstream response: {}", err)),
                            request: Some(request_context.clone()),
                            retries: attempt,
                        })?;

                    // 回写完成后再生成响应上下文，保证记录的是最终状态。
                    let mut response = ResponseContext::new(status_code);
                    response.upstream = Some(endpoint.address);

                    self.filters
                        .run_after(&route.filter_names, &mut response)
                        .await
                        .map_err(|error| ProxyConnectionError {
                            error,
                            request: Some(request_context.clone()),
                            retries: attempt,
                        })?;

                    return Ok(CompletedRequest {
                        request: request_context,
                        response,
                        retries: attempt,
                        close_downstream: request_connection_close(
                            &request.version,
                            &request.headers,
                        ) || upstream_response.connection_close,
                    });
                }
                Err(error) => {
                    // 连接失败、读超时或协议错误都先按一次失败记入节点状态。
                    endpoint_state.record_failure(cluster.passive_failure_threshold());
                    excluded_addresses.push(endpoint.address.clone());
                    final_outcome = Some(Err(ProxyConnectionError {
                        error,
                        request: Some(request_context.clone()),
                        retries: attempt + 1,
                    }));
                }
            }
        }

        final_outcome.unwrap_or_else(|| {
            // 正常情况下走到这里意味着已经没有可重试节点。
            Err(ProxyConnectionError {
                error: GatewayError::NoHealthyUpstream(route.upstream_name.clone()),
                request: Some(request_context),
                retries: policy.upstream_retry_attempts.saturating_sub(1),
            })
        })
    }

    async fn forward_to_endpoint(
        &self,
        endpoint_state: &EndpointState,
        request: &HttpRequest,
        request_context: &RequestContext,
        // 这里传入的已经是最终确定好的策略，
        // 所以与单个 upstream 的交互过程不用再关心策略优先级问题。
        policy: ResolvedProxyPolicy,
    ) -> Result<UpstreamResponse> {
        // 这里把“与单个 upstream 交互”收口成独立函数，
        // 方便后面接入更细的错误分类、重试预算和连接池。
        let endpoint = endpoint_state.endpoint();
        let keepalive_enabled = self.timeouts.upstream_idle_pool_size > 0;
        let mut force_fresh_connect = false;

        loop {
            // 先尝试借用空闲连接；只有池里没有可用连接时才真正发起新建连。
            let (mut upstream, reused_idle_connection) = if !force_fresh_connect {
                match endpoint_state.checkout_idle_connection().await {
                    Some(stream) => (stream, true),
                    None => (
                        self.connect_upstream(endpoint.address.as_str(), policy)
                            .await?,
                        false,
                    ),
                }
            } else {
                (
                    self.connect_upstream(endpoint.address.as_str(), policy)
                        .await?,
                    false,
                )
            };

            // 空闲连接有可能已经被上游静默关掉；第一版先允许在同一次 endpoint 尝试里补一次新建连。
            if let Err(error) = request
                .write_to_upstream(&mut upstream, request_context, keepalive_enabled)
                .await
            {
                if reused_idle_connection {
                    force_fresh_connect = true;
                    continue;
                }
                return Err(error);
            }

            let response = read_upstream_response(
                &mut upstream,
                policy,
                &self.timeouts,
                request_context.method,
            )
            .await;
            match response {
                Ok(response) => {
                    // 只有边界明确且上游允许 keep-alive 的连接才回收，避免把脏连接放回池里。
                    if response.reusable_connection {
                        endpoint_state
                            .store_idle_connection(upstream, self.timeouts.upstream_idle_pool_size)
                            .await;
                    }
                    return Ok(response);
                }
                Err(error) => {
                    if reused_idle_connection {
                        force_fresh_connect = true;
                        continue;
                    }
                    return Err(error);
                }
            }
        }
    }

    async fn connect_upstream(
        &self,
        endpoint_address: &str,
        policy: ResolvedProxyPolicy,
    ) -> Result<TcpStream> {
        // 建连阶段受 connect timeout 保护，避免坏节点无限拖住请求。
        timeout(
            policy.upstream_connect_timeout,
            TcpStream::connect(endpoint_address),
        )
        .await
        .map_err(|_| {
            GatewayError::Io(format!(
                "connect upstream {} timed out after {:?}",
                endpoint_address, policy.upstream_connect_timeout
            ))
        })?
        .map_err(|err| GatewayError::Io(format!("connect upstream {}: {}", endpoint_address, err)))
    }
}

#[derive(Clone, Debug)]
struct UpstreamResponse {
    /// 完整上游响应字节，会被原样回写给下游。
    bytes: Vec<u8>,
    /// 已经校验过的上游状态码，供重试策略直接使用。
    status_code: u16,
    /// 只有响应边界明确且上游未声明关闭时，连接才允许回收到池里。
    reusable_connection: bool,
    /// 上游是否显式要求关闭当前响应连接。
    connection_close: bool,
}

#[derive(Clone, Debug)]
struct ValidatedUpstreamResponseHead {
    /// 状态码会继续参与重试判定和日志记录。
    status_code: u16,
    /// 如果上游给出了精确的 `Content-Length`，这里就记录解析结果。
    content_length: Option<usize>,
    /// 某些响应按协议语义本身就不允许携带 body，例如 `HEAD`、`204`、`304`。
    body_allowed: bool,
    /// 只有在上游没有声明关闭连接时，连接才有资格进入复用判定。
    reusable_connection: bool,
    /// 上游响应是否显式带了 `Connection: close` 语义。
    connection_close: bool,
}

async fn read_upstream_response(
    upstream: &mut TcpStream,
    policy: ResolvedProxyPolicy,
    settings: &RuntimeSettings,
    request_method: HttpMethod,
) -> Result<UpstreamResponse> {
    let mut buffer = Vec::with_capacity(4096);
    let mut temp = [0_u8; 2048];

    // 第一段只负责把响应头读完整，并在边界超限前及时失败。
    let header_end = loop {
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() >= settings.max_upstream_header_bytes {
            return Err(GatewayError::Protocol(
                "upstream response headers exceed configured maximum size".into(),
            ));
        }
        let read = timeout(policy.upstream_read_timeout, upstream.read(&mut temp))
            .await
            .map_err(|_| {
                GatewayError::Io(format!(
                    "read upstream response timed out after {:?}",
                    policy.upstream_read_timeout
                ))
            })?
            .map_err(|err| GatewayError::Io(format!("read upstream response: {}", err)))?;
        if read == 0 {
            return Err(GatewayError::Protocol(
                "upstream closed connection before response headers completed".into(),
            ));
        }
        buffer.extend_from_slice(&temp[..read]);
    };

    // 头部一旦读完整，就立刻把状态行、连接语义和 body 边界规则算出来。
    // 后续 body 读取严格按这里的结论执行，避免“读多了”或“读少了”。
    let response_head =
        validate_upstream_response_head(&buffer[..header_end], settings, request_method)?;

    // 头部之后已经落入缓冲区的剩余字节就属于响应体，需要立刻做一次限额检查。
    let body_start = header_end + 4;
    let mut body_bytes = buffer.len().saturating_sub(body_start);

    // 如果响应按协议不允许带 body，那头部后面出现任何多余字节都说明上游越界了。
    if !response_head.body_allowed {
        if body_bytes != 0 {
            return Err(GatewayError::Protocol(
                "upstream returned a body for a response that must not contain one".into(),
            ));
        }
    } else if let Some(content_length) = response_head.content_length {
        // 有精确长度时，先检查已读到缓冲区里的字节是否已经越界。
        if content_length > settings.max_upstream_body_bytes {
            return Err(GatewayError::Protocol(
                "upstream response body exceeds configured maximum size".into(),
            ));
        }
        if body_bytes > content_length {
            return Err(GatewayError::Protocol(
                "upstream response body exceeds declared content-length".into(),
            ));
        }

        // 再把剩余的固定长度 body 读满，确保连接边界被完整消费。
        while body_bytes < content_length {
            let read = timeout(policy.upstream_read_timeout, upstream.read(&mut temp))
                .await
                .map_err(|_| {
                    GatewayError::Io(format!(
                        "read upstream response timed out after {:?}",
                        policy.upstream_read_timeout
                    ))
                })?
                .map_err(|err| GatewayError::Io(format!("read upstream response: {}", err)))?;
            if read == 0 {
                return Err(GatewayError::Protocol(
                    "upstream closed connection before response body completed".into(),
                ));
            }
            body_bytes += read;
            if body_bytes > content_length {
                return Err(GatewayError::Protocol(
                    "upstream response body exceeds declared content-length".into(),
                ));
            }
            buffer.extend_from_slice(&temp[..read]);
        }
    } else {
        // 没有精确长度但按协议允许 body 时，只能退回到 EOF 作为结束边界。
        // 这种模式能保证功能正确，但由于边界依赖对端关连接，所以绝不复用。
        if body_bytes > settings.max_upstream_body_bytes {
            return Err(GatewayError::Protocol(
                "upstream response body exceeds configured maximum size".into(),
            ));
        }
        loop {
            let read = timeout(policy.upstream_read_timeout, upstream.read(&mut temp))
                .await
                .map_err(|_| {
                    GatewayError::Io(format!(
                        "read upstream response timed out after {:?}",
                        policy.upstream_read_timeout
                    ))
                })?
                .map_err(|err| GatewayError::Io(format!("read upstream response: {}", err)))?;
            if read == 0 {
                break;
            }
            body_bytes += read;
            if body_bytes > settings.max_upstream_body_bytes {
                return Err(GatewayError::Protocol(
                    "upstream response body exceeds configured maximum size".into(),
                ));
            }
            buffer.extend_from_slice(&temp[..read]);
        }
    }

    Ok(UpstreamResponse {
        bytes: buffer,
        status_code: response_head.status_code,
        // 只有“边界明确且上游未要求关闭”时才允许池化。
        reusable_connection: response_head.reusable_connection
            && (response_head.content_length.is_some() || !response_head.body_allowed),
        connection_close: response_head.connection_close,
    })
}

fn validate_upstream_response_head(
    head_bytes: &[u8],
    settings: &RuntimeSettings,
    request_method: HttpMethod,
) -> Result<ValidatedUpstreamResponseHead> {
    let head = String::from_utf8(head_bytes.to_vec())
        .map_err(|_| GatewayError::Protocol("upstream response head is not valid utf-8".into()))?;
    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| GatewayError::Protocol("missing upstream status line".into()))?;
    if status_line.len() > settings.max_upstream_status_line_bytes {
        return Err(GatewayError::Protocol(
            "upstream status line exceeds configured maximum size".into(),
        ));
    }

    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| GatewayError::Protocol("missing upstream http version".into()))?;
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(GatewayError::Unsupported(format!(
            "upstream http version {}",
            version
        )));
    }
    let status_code = parts
        .next()
        .ok_or_else(|| GatewayError::Protocol("missing upstream status code".into()))?
        .parse::<u16>()
        .map_err(|_| GatewayError::Protocol("invalid upstream status code".into()))?;
    if !(100..=599).contains(&status_code) {
        return Err(GatewayError::Protocol(
            "upstream status code is outside supported range".into(),
        ));
    }
    if parts.next().is_none() {
        return Err(GatewayError::Protocol(
            "upstream reason phrase must not be empty".into(),
        ));
    }

    let mut headers = Vec::new();
    let mut header_count = 0;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        header_count += 1;
        if header_count > settings.max_upstream_headers {
            return Err(GatewayError::Protocol(
                "upstream response contains too many headers".into(),
            ));
        }
        let (name, value) = line.split_once(':').ok_or_else(|| {
            GatewayError::Protocol(format!("invalid upstream header line {}", line))
        })?;
        let name = name.trim();
        if !is_valid_header_name(name) {
            return Err(GatewayError::Protocol(format!(
                "invalid upstream header name {}",
                name
            )));
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }

    let content_length = parse_optional_content_length(&headers)?;

    // 第一版显式拒绝 `Transfer-Encoding`，避免在尚未实现 chunked 解码前误读边界。
    // 同时拒绝 `Transfer-Encoding + Content-Length` 组合，减少上下游边界解释分歧。
    if header_has_transfer_encoding(&headers) && content_length.is_some() {
        return Err(GatewayError::Unsupported(
            "upstream transfer-encoding with content-length is not supported in the first kernel cut"
                .into(),
        ));
    }
    if header_has_transfer_encoding(&headers) {
        return Err(GatewayError::Unsupported(
            "upstream transfer-encoding is not supported in the first kernel cut".into(),
        ));
    }
    let connection_close = response_connection_close(version, &headers);

    Ok(ValidatedUpstreamResponseHead {
        status_code,
        content_length,
        body_allowed: upstream_response_body_allowed(status_code, request_method),
        reusable_connection: !connection_close,
        connection_close,
    })
}

#[derive(Clone, Debug)]
struct HttpRequest {
    /// 归一化后的请求方法。
    method: HttpMethod,
    /// 原始 request target，保留 query string 以便转发时原样带过去。
    target: String,
    /// 协议版本字符串，目前只接受 HTTP/1.1。
    version: String,
    /// 按原始顺序保留的请求头列表。
    headers: Vec<(String, String)>,
    /// 已经读入内存的请求体。
    body: Vec<u8>,
}

impl HttpRequest {
    async fn read_from(stream: &mut TcpStream, settings: &RuntimeSettings) -> Result<Self> {
        const MAX_HEADER_BYTES: usize = 64 * 1024;
        let read_timeout = settings.downstream_read_timeout;
        let mut buffer = Vec::with_capacity(2048);
        let mut temp = [0_u8; 2048];

        let header_end = loop {
            if let Some(position) = find_header_end(&buffer) {
                break position;
            }
            if buffer.len() >= MAX_HEADER_BYTES {
                return Err(GatewayError::Protocol(
                    "request headers exceed maximum size".into(),
                ));
            }
            let waiting_for_first_byte = buffer.is_empty();
            let wait_timeout = if waiting_for_first_byte {
                settings.downstream_keepalive_idle_timeout
            } else {
                read_timeout
            };
            let read = timeout(wait_timeout, stream.read(&mut temp))
                .await
                .map_err(|_| {
                    if waiting_for_first_byte {
                        GatewayError::Io(format!(
                            "downstream keepalive idle timeout after {:?}",
                            wait_timeout
                        ))
                    } else {
                        GatewayError::Io(format!(
                            "read downstream request timed out after {:?}",
                            read_timeout
                        ))
                    }
                })?
                .map_err(|err| GatewayError::Io(format!("read downstream request: {}", err)))?;
            if read == 0 {
                if waiting_for_first_byte {
                    return Err(GatewayError::Io(
                        "downstream connection closed by peer".into(),
                    ));
                }
                return Err(GatewayError::Protocol(
                    "connection closed before request completed".into(),
                ));
            }
            buffer.extend_from_slice(&temp[..read]);
        };

        // 请求头目前按可见 ASCII/UTF-8 文本处理。
        // 这样实现足够简单，但也意味着后面要继续补更严格的 header 校验。
        let head = String::from_utf8(buffer[..header_end].to_vec())
            .map_err(|_| GatewayError::Protocol("request head is not valid utf-8".into()))?;
        let mut lines = head.split("\r\n");
        let request_line = lines
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request line".into()))?;
        if request_line.len() > settings.max_request_line_bytes {
            return Err(GatewayError::Protocol(
                "request line exceeds configured maximum size".into(),
            ));
        }
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request method".into()))
            .and_then(HttpMethod::try_from)?;
        let target = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request target".into()))?
            .to_string();
        if !target.starts_with('/') {
            return Err(GatewayError::Unsupported(
                "only origin-form request targets are supported".into(),
            ));
        }
        let version = request_parts
            .next()
            .ok_or_else(|| GatewayError::Protocol("missing request version".into()))?
            .to_string();
        if request_parts.next().is_some() {
            return Err(GatewayError::Protocol(
                "request line contains unexpected trailing tokens".into(),
            ));
        }

        if version != "HTTP/1.1" {
            return Err(GatewayError::Unsupported(format!(
                "http version {}",
                version
            )));
        }

        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if headers.len() >= settings.max_request_headers {
                return Err(GatewayError::Protocol(
                    "request contains too many headers".into(),
                ));
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| GatewayError::Protocol(format!("invalid header line {}", line)))?;
            let name = name.trim();
            if !is_valid_header_name(name) {
                return Err(GatewayError::Protocol(format!(
                    "invalid header name {}",
                    name
                )));
            }
            headers.push((name.to_string(), value.trim().to_string()));
        }

        let content_length = parse_content_length(&headers)?;
        if content_length > settings.max_request_body_bytes {
            return Err(GatewayError::Protocol(
                "request body exceeds configured maximum size".into(),
            ));
        }

        // 对 `Transfer-Encoding` 相关路径统一显式拒绝，避免把 chunked 语义误当固定长度。
        if header_has_transfer_encoding(&headers) && header_has_content_length(&headers) {
            return Err(GatewayError::Unsupported(
                "transfer-encoding with content-length is not supported in the first kernel cut"
                    .into(),
            ));
        }
        if header_has_transfer_encoding(&headers) {
            return Err(GatewayError::Unsupported(
                "transfer-encoding is not supported in the first kernel cut".into(),
            ));
        }
        if header_has_expect_100_continue(&headers) {
            return Err(GatewayError::Unsupported(
                "expect: 100-continue is not supported in the first kernel cut".into(),
            ));
        }

        let mut body = buffer[(header_end + 4)..].to_vec();

        if body.len() < content_length {
            let remaining = content_length - body.len();
            let mut extra = vec![0_u8; remaining];
            timeout(read_timeout, stream.read_exact(&mut extra))
                .await
                .map_err(|_| {
                    GatewayError::Io(format!(
                        "read request body timed out after {:?}",
                        read_timeout
                    ))
                })?
                .map_err(|err| GatewayError::Io(format!("read request body: {}", err)))?;
            body.extend_from_slice(&extra);
        } else if body.len() > content_length {
            return Err(GatewayError::Unsupported(
                "downstream request pipelining is not supported in the first kernel cut".into(),
            ));
        }

        // HTTP/1.1 的 Host 头是强约束，缺失时直接拒绝，
        // 这样可以减少后续路由歧义和请求走私风险。
        validate_host_header(&headers)?;

        Ok(Self {
            method,
            target,
            version,
            headers,
            body,
        })
    }

    fn host_header(&self) -> String {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("host"))
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    }

    fn path_for_route(&self) -> String {
        self.target
            .split('?')
            .next()
            .unwrap_or(self.target.as_str())
            .to_string()
    }

    fn query_for_policy(&self) -> Option<String> {
        self.target
            .split_once('?')
            .map(|(_, query)| query.to_string())
            .filter(|query| !query.is_empty())
    }

    async fn write_to_upstream(
        &self,
        upstream: &mut TcpStream,
        request_context: &RequestContext,
        keepalive_enabled: bool,
    ) -> Result<()> {
        // 转发时会清理 hop-by-hop 头，并补齐代理侧需要的最小公共头。
        let mut request_bytes = Vec::with_capacity(1024 + self.body.len());
        request_bytes.extend_from_slice(
            format!(
                "{} {} {}\r\n",
                request_context.method, self.target, self.version
            )
            .as_bytes(),
        );

        let mut has_host = false;
        let mut has_x_forwarded_for = false;
        let mut has_content_length = false;

        for (name, value) in &self.headers {
            if is_hop_by_hop_header(name) {
                continue;
            }
            // 这类头只能由网关根据已认证/已隔离的内部上下文注入，
            // 不能信任客户端原样透传，避免伪造分享身份。
            if is_gateway_managed_upstream_header(name) {
                continue;
            }
            if name.eq_ignore_ascii_case("host") {
                has_host = true;
            }
            if name.eq_ignore_ascii_case("x-forwarded-for") {
                has_x_forwarded_for = true;
            }
            if name.eq_ignore_ascii_case("content-length") {
                has_content_length = true;
            }
            request_bytes.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }

        // 把网关内部策略生成的附加头放在这里统一写入，
        // 保证上游拿到的是“经过入口策略裁决后的稳定语义”。
        for (name, value) in &request_context.upstream_headers {
            request_bytes.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }

        if !has_host {
            request_bytes
                .extend_from_slice(format!("Host: {}\r\n", request_context.host).as_bytes());
        }
        if !has_x_forwarded_for {
            if let Some(client_addr) = request_context.client_addr {
                request_bytes.extend_from_slice(
                    format!("X-Forwarded-For: {}\r\n", client_addr.ip()).as_bytes(),
                );
            }
        }

        // 只有启用了空闲连接池时才主动声明 keep-alive；否则继续用 close 收紧语义边界。
        if keepalive_enabled {
            request_bytes.extend_from_slice(b"Connection: keep-alive\r\n");
        } else {
            request_bytes.extend_from_slice(b"Connection: close\r\n");
        }
        if !has_content_length {
            request_bytes
                .extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        request_bytes.extend_from_slice(b"\r\n");
        request_bytes.extend_from_slice(&self.body);

        upstream
            .write_all(&request_bytes)
            .await
            .map_err(|err| GatewayError::Io(format!("write upstream request: {}", err)))?;
        upstream
            .flush()
            .await
            .map_err(|err| GatewayError::Io(format!("flush upstream request: {}", err)))?;
        Ok(())
    }
}

fn parse_content_length(headers: &[(String, String)]) -> Result<usize> {
    let mut content_length = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value.parse::<usize>().map_err(|_| {
                GatewayError::Protocol(format!("invalid content-length header value {}", value))
            })?;
            match content_length {
                // 这里从严处理重复 Content-Length，
                // 先优先规避请求走私和上下游解析不一致的问题。
                Some(existing) if existing != parsed => {
                    return Err(GatewayError::Protocol(
                        "conflicting content-length headers are not allowed".into(),
                    ));
                }
                Some(_) => {
                    return Err(GatewayError::Protocol(
                        "duplicate content-length headers are not allowed".into(),
                    ));
                }
                None => content_length = Some(parsed),
            }
        }
    }
    Ok(content_length.unwrap_or(0))
}

fn parse_optional_content_length(headers: &[(String, String)]) -> Result<Option<usize>> {
    let mut content_length = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value.parse::<usize>().map_err(|_| {
                GatewayError::Protocol(format!("invalid content-length header value {}", value))
            })?;
            match content_length {
                // 响应方向同样从严禁止重复或冲突的 `Content-Length`，
                // 否则连接复用时会把边界安全建立在不可靠前提上。
                Some(existing) if existing != parsed => {
                    return Err(GatewayError::Protocol(
                        "conflicting upstream content-length headers are not allowed".into(),
                    ));
                }
                Some(_) => {
                    return Err(GatewayError::Protocol(
                        "duplicate upstream content-length headers are not allowed".into(),
                    ));
                }
                None => content_length = Some(parsed),
            }
        }
    }
    Ok(content_length)
}

fn validate_host_header(headers: &[(String, String)]) -> Result<()> {
    let host_values: Vec<_> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.as_str())
        .collect();

    if host_values.is_empty() {
        return Err(GatewayError::Protocol(
            "host header is required for http/1.1 requests".into(),
        ));
    }
    if host_values.len() > 1 {
        return Err(GatewayError::Protocol(
            "duplicate host headers are not allowed".into(),
        ));
    }
    if host_values[0].is_empty() {
        return Err(GatewayError::Protocol(
            "host header must not be empty".into(),
        ));
    }

    Ok(())
}

fn header_has_transfer_encoding(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
}

fn header_has_content_length(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"))
}

fn header_has_expect_100_continue(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("expect")
            && value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("100-continue"))
    })
}

fn response_connection_close(version: &str, headers: &[(String, String)]) -> bool {
    if version == "HTTP/1.0" {
        // HTTP/1.0 默认是短连接，只有显式 `keep-alive` 才把它当成可持久连接。
        !headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("keep-alive"))
        })
    } else {
        // HTTP/1.1 默认允许持久连接，只有显式 `close` 才强制本次连接不可复用。
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("close"))
        })
    }
}

fn request_connection_close(version: &str, headers: &[(String, String)]) -> bool {
    if version == "HTTP/1.0" {
        !headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("keep-alive"))
        })
    } else {
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection")
                && value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("close"))
        })
    }
}

fn upstream_response_body_allowed(status_code: u16, request_method: HttpMethod) -> bool {
    // `HEAD` 响应、1xx、204、304 都没有语义 body；
    // 只要碰到这些状态，我们就按“无 body”边界处理响应。
    request_method != HttpMethod::Head
        && !(100..200).contains(&status_code)
        && status_code != 204
        && status_code != 304
}

fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes().iter().all(|byte| {
            matches!(
                *byte,
                b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.'
                    | b'^' | b'_' | b'`' | b'|' | b'~'
                    | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'
            )
        })
}

fn is_hop_by_hop_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("connection")
        || name.eq_ignore_ascii_case("keep-alive")
        || name.eq_ignore_ascii_case("proxy-connection")
        || name.eq_ignore_ascii_case("transfer-encoding")
        || name.eq_ignore_ascii_case("upgrade")
}

fn is_gateway_managed_upstream_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("x-rivulet-share-id")
        || name.eq_ignore_ascii_case("x-rivulet-share-scope")
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn is_retryable_status(status_code: u16) -> bool {
    matches!(status_code, 500 | 502 | 503 | 504)
}

/// 运行时在请求失败后会回落到这里，把内部错误映射成最小可读 HTTP 响应。
pub fn error_response(error: &GatewayError) -> Vec<u8> {
    let (status, reason) = status_and_reason_for_error(error);

    let body = format!("{} {}\n", status, reason);
    format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: text/plain\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    )
    .into_bytes()
}

pub fn status_code_for_error(error: &GatewayError) -> u16 {
    status_and_reason_for_error(error).0
}

pub fn is_graceful_downstream_close(error: &GatewayError) -> bool {
    matches!(
        error,
        GatewayError::Io(message)
            if message == "downstream connection closed by peer"
                || message.starts_with("downstream keepalive idle timeout after ")
    )
}

fn status_and_reason_for_error(error: &GatewayError) -> (u16, &'static str) {
    match error {
        GatewayError::RouteNotMatched => (404_u16, "Not Found"),
        GatewayError::Unauthorized(_) => (401_u16, "Unauthorized"),
        GatewayError::Forbidden(_) => (403_u16, "Forbidden"),
        GatewayError::RateLimited(_) => (429_u16, "Too Many Requests"),
        GatewayError::Unsupported(_) => (501_u16, "Not Implemented"),
        GatewayError::Protocol(_)
        | GatewayError::InvalidConfig(_)
        | GatewayError::FilterRejected(_) => (400_u16, "Bad Request"),
        GatewayError::NoHealthyUpstream(_) | GatewayError::NotFound(_) => {
            (503_u16, "Service Unavailable")
        }
        GatewayError::Io(_) => (502_u16, "Bad Gateway"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gateway_admin::{
        AdminListener, AdminOverview, AdminOverviewProvider, AdminRoute, AdminRuntime,
        AdminService, AdminStats, AdminSummary, AdminUpstream,
    };
    use gateway_config::{
        EndpointConfig, GatewayConfigFile, ListenerConfig, LoadBalanceConfig, ProtocolConfig,
        ProxyPolicyConfig, RouteAuthConfig, RouteConfig, RouteShareConfig, RouteShareGrantConfig,
        RuntimeConfig, UpstreamConfig,
    };
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{Duration, sleep, timeout};

    struct FakeAdminProvider;

    impl AdminOverviewProvider for FakeAdminProvider {
        fn overview(&self) -> AdminOverview {
            AdminOverview {
                summary: AdminSummary {
                    listeners: 1,
                    routes: 1,
                    upstreams: 1,
                    worker_threads: 4,
                },
                runtime: AdminRuntime {
                    graceful_shutdown_secs: 30,
                    downstream_read_timeout_ms: 5000,
                    downstream_keepalive_idle_timeout_ms: 5000,
                    downstream_keepalive_max_requests: 100,
                    upstream_connect_timeout_ms: 3000,
                    upstream_read_timeout_ms: 5000,
                    upstream_retry_attempts: 2,
                    upstream_idle_pool_size: 1,
                },
                stats: AdminStats {
                    total_requests: 0,
                    completed_requests: 0,
                    active_connections: 0,
                    successful_responses: 0,
                    client_error_responses: 0,
                    server_error_responses: 0,
                    upstream_retries: 0,
                },
                listeners: vec![AdminListener {
                    name: "edge".into(),
                    address: "127.0.0.1:8080".into(),
                    protocol: "http1".into(),
                }],
                routes: vec![AdminRoute {
                    name: "admin".into(),
                    listener: "edge".into(),
                    hosts: vec!["localhost".into()],
                    path_prefixes: vec!["/__admin".into()],
                    methods: vec!["GET".into()],
                    upstream: "internal".into(),
                }],
                upstreams: vec![AdminUpstream {
                    name: "internal".into(),
                    load_balance: "round_robin".into(),
                    endpoints: vec!["127.0.0.1:9000".into()],
                }],
            }
        }
    }

    fn fake_admin_service() -> AdminService {
        AdminService::new(Arc::new(FakeAdminProvider))
    }

    #[tokio::test]
    async fn proxies_http_request_to_upstream() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            assert_eq!(request.path_for_route(), "/hello");
            assert_eq!(request.host_header(), "example.test");

            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                )
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            let completed = service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
                .expect("proxy request");
            assert_eq!(completed.response.status_code, 200);
            assert_eq!(completed.retries, 0);
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /hello HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        server.await.expect("gateway task");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.ends_with("hello"));
    }

    #[tokio::test]
    async fn rejects_request_without_valid_route_auth_token() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "private".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/private".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: RouteAuthConfig {
                    bearer_tokens: vec!["gateway-secret".into()],
                    query_tokens: Vec::new(),
                    query_token_name: "access_token".into(),
                },
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /private HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Unauthorized(message),
                request: Some(request),
                retries,
            }) => {
                assert_eq!(
                    request.request_id.as_deref(),
                    Some("edge:example.test:/private")
                );
                assert!(message.contains("valid bearer token"));
                assert_eq!(retries, 0);
            }
            other => panic!(
                "expected unauthorized route auth rejection, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn allows_request_with_valid_query_share_token() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            assert_eq!(request.path_for_route(), "/share/view");
            let raw = String::from_utf8_lossy(&request.body);
            assert!(raw.is_empty());
            assert_eq!(
                request
                    .headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("x-rivulet-share-id"))
                    .map(|(_, value)| value.as_str()),
                Some("share-001")
            );
            assert_eq!(
                request
                    .headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("x-rivulet-share-scope"))
                    .map(|(_, value)| value.as_str()),
                Some("preview")
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "share".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/share/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: RouteShareConfig {
                    query_token_name: "share_token".into(),
                    grants: vec![RouteShareGrantConfig {
                        token: "share-secret".into(),
                        share_id: "share-001".into(),
                        scope: "preview".into(),
                        resource_prefixes: vec!["/share/view".into()],
                    }],
                },
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /share/view?share_token=share-secret HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task").expect("proxy request");
        assert_eq!(outcome.response.status_code, 200);
        assert_eq!(
            outcome.request.request_id.as_deref(),
            Some("edge:example.test:/share/view")
        );
        assert_eq!(outcome.request.share_id.as_deref(), Some("share-001"));
        assert_eq!(outcome.request.share_scope.as_deref(), Some("preview"));
    }

    #[tokio::test]
    async fn rejects_request_without_valid_query_share_token() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "share".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/share/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: RouteShareConfig {
                    query_token_name: "share_token".into(),
                    grants: vec![RouteShareGrantConfig {
                        token: "share-secret".into(),
                        share_id: "share-001".into(),
                        scope: "preview".into(),
                        resource_prefixes: vec!["/share/view".into()],
                    }],
                },
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /share/view HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Unauthorized(message),
                request: Some(request),
                retries,
            }) => {
                assert!(message.contains("share_token"));
                assert_eq!(
                    request.request_id.as_deref(),
                    Some("edge:example.test:/share/view")
                );
                assert_eq!(request.share_id, None);
                assert_eq!(retries, 0);
            }
            other => panic!("expected shared access rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_request_outside_share_resource_prefix_before_upstream() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "share".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/share/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: RouteShareConfig {
                    query_token_name: "share_token".into(),
                    grants: vec![RouteShareGrantConfig {
                        token: "share-secret".into(),
                        share_id: "share-001".into(),
                        scope: "preview".into(),
                        resource_prefixes: vec!["/share/view".into()],
                    }],
                },
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /share/manage?share_token=share-secret HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Forbidden(message),
                request: Some(request),
                retries,
            }) => {
                assert!(message.contains("/share/manage"));
                assert_eq!(
                    request.request_id.as_deref(),
                    Some("edge:example.test:/share/manage")
                );
                assert_eq!(request.share_id, None);
                assert_eq!(retries, 0);
            }
            other => panic!("expected forbidden shared path rejection, got {:?}", other),
        }

        // 越权请求应该在网关入口处被拒绝，不应触发上游 accept。
        let backend_accept = timeout(Duration::from_millis(180), backend.accept()).await;
        assert!(backend_accept.is_err());
    }

    #[tokio::test]
    async fn rejects_request_when_route_rate_limit_is_exceeded() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let backend_accept_count = Arc::new(AtomicUsize::new(0));
        let backend_accept_count_clone = Arc::clone(&backend_accept_count);

        let backend_task = tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            backend_accept_count_clone.fetch_add(1, Ordering::SeqCst);
            let _request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "public".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/public".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: gateway_config::RouteRateLimitConfig {
                    requests: Some(1),
                    window_ms: Some(1_000),
                    key: gateway_config::RateLimitKeyConfig::ClientIp,
                },
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = Arc::new(ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        ));

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let mut outcomes = Vec::new();
            for _ in 0..2 {
                let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
                let service = Arc::clone(&service);
                outcomes.push(
                    service
                        .handle_connection("edge", &mut downstream, client_addr)
                        .await,
                );
            }
            outcomes
        });

        for _ in 0..2 {
            let mut client = TcpStream::connect(gateway_addr)
                .await
                .expect("connect gateway");
            client
                .write_all(
                    b"GET /public HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
                )
                .await
                .expect("write request");
        }

        let outcomes = server.await.expect("gateway task");
        assert!(outcomes[0].is_ok());
        match &outcomes[1] {
            Err(ProxyConnectionError {
                error: GatewayError::RateLimited(message),
                retries,
                ..
            }) => {
                assert!(message.contains("exceeded"));
                assert_eq!(*retries, 0);
            }
            other => panic!("expected rate limited rejection, got {:?}", other),
        }

        backend_task.await.expect("backend task");
        assert_eq!(backend_accept_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn allows_request_after_rate_limit_window_resets() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "public".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/public".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: gateway_config::RouteRateLimitConfig {
                    requests: Some(1),
                    window_ms: Some(30),
                    key: gateway_config::RateLimitKeyConfig::ClientIp,
                },
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let mut request = RequestContext::new("edge", "example.test", "/public", HttpMethod::Get);
        request.client_addr = Some("127.0.0.1:18080".parse().expect("socket addr"));

        let first = service
            .handle(request.clone())
            .await
            .expect("first request");
        assert_eq!(first.status_code, 200);

        let second = service
            .handle(request.clone())
            .await
            .expect_err("second request should be limited");
        match second {
            GatewayError::RateLimited(message) => assert!(message.contains("exceeded")),
            other => panic!("expected rate limited error, got {:?}", other),
        }

        sleep(Duration::from_millis(45)).await;

        let third = service.handle(request).await.expect("window should reset");
        assert_eq!(third.status_code, 200);
    }

    #[tokio::test]
    async fn serves_admin_ui_before_route_resolution() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![],
            upstreams: vec![UpstreamConfig {
                name: "unused".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::with_admin(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
            Some(fake_admin_service()),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, _) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection(
                    "edge",
                    &mut downstream,
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 40000)),
                )
                .await
                .expect("admin request should succeed")
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /__admin/ HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let completed = server.await.expect("gateway task");
        assert_eq!(completed.response.status_code, 200);
        assert_eq!(
            completed.response.upstream.as_deref(),
            Some("internal://admin-ui")
        );

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.contains("溪流网关管理面"));
    }

    #[tokio::test]
    async fn rejects_remote_admin_request() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![],
            upstreams: vec![UpstreamConfig {
                name: "unused".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::with_admin(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
            Some(fake_admin_service()),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, _) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection(
                    "edge",
                    &mut downstream,
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 9), 41000)),
                )
                .await
                .expect("admin request should return an HTTP response")
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /__admin/ HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let completed = server.await.expect("gateway task");
        assert_eq!(completed.response.status_code, 403);
        assert_eq!(
            completed.response.upstream.as_deref(),
            Some("internal://admin-ui")
        );

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(text.contains("loopback"));
    }

    #[tokio::test]
    async fn forwards_x_forwarded_for_and_drops_hop_by_hop_headers() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let mut raw = Vec::new();
            let mut buf = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buf).await.expect("read backend request");
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..read]);
                if raw.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let text = String::from_utf8(raw).expect("request utf-8");
            assert!(text.contains("X-Forwarded-For: 127.0.0.1"));
            assert!(text.contains("Host: example.test"));
            assert!(text.contains("Connection: keep-alive"));
            assert!(!text.contains("Proxy-Connection"));

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
                .expect("proxy request");
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /hello HTTP/1.1\r\nHost: example.test\r\nConnection: keep-alive\r\nProxy-Connection: keep-alive\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        server.await.expect("gateway task");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
    }

    #[tokio::test]
    async fn reuses_upstream_connection_when_response_has_content_length() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let accept_count = Arc::new(AtomicUsize::new(0));
        let backend_accept_count = Arc::clone(&accept_count);

        let backend_task = tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            backend_accept_count.fetch_add(1, Ordering::SeqCst);

            // 第一条请求读完后不关连接，验证网关会把它放回池里供下一次复用。
            let first_request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read first backend request");
            assert_eq!(first_request.path_for_route(), "/one");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: keep-alive\r\n\r\none",
                )
                .await
                .expect("write first backend response");

            // 第二条请求如果还能从同一条 TCP 流里读出来，就说明连接复用已经生效。
            let second_request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read second backend request");
            assert_eq!(second_request.path_for_route(), "/two");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: keep-alive\r\n\r\ntwo",
                )
                .await
                .expect("write second backend response");

            // 两次请求都处理完后，再短暂观察监听口；如果还有第二次 accept，就说明网关偷偷新建了连接。
            let unexpected_accept = timeout(Duration::from_millis(200), backend.accept()).await;
            assert!(unexpected_accept.is_err());
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
                service
                    .handle_connection("edge", &mut downstream, client_addr)
                    .await
                    .expect("proxy request");
            }
        });

        let mut first_client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect first gateway");
        first_client
            .write_all(b"GET /one HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write first request");
        let mut first_response = Vec::new();
        first_client
            .read_to_end(&mut first_response)
            .await
            .expect("read first response");

        let mut second_client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect second gateway");
        second_client
            .write_all(b"GET /two HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write second request");
        let mut second_response = Vec::new();
        second_client
            .read_to_end(&mut second_response)
            .await
            .expect("read second response");

        server.await.expect("gateway task");
        backend_task.await.expect("backend task");

        assert_eq!(accept_count.load(Ordering::SeqCst), 1);
        assert!(
            String::from_utf8(first_response)
                .expect("first response utf-8")
                .ends_with("one")
        );
        assert!(
            String::from_utf8(second_response)
                .expect("second response utf-8")
                .ends_with("two")
        );
    }

    #[tokio::test]
    async fn eof_delimited_upstream_response_is_not_marked_reusable() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        let backend_task = tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nhello")
                .await
                .expect("write backend response");
        });

        let mut upstream = TcpStream::connect(backend_addr)
            .await
            .expect("connect backend");
        let settings = RuntimeSettings::default();
        let response = read_upstream_response(
            &mut upstream,
            settings.proxy_policy(),
            &settings,
            HttpMethod::Get,
        )
        .await
        .expect("read upstream response");

        backend_task.await.expect("backend task");

        assert_eq!(response.status_code, 200);
        assert!(!response.reusable_connection);
        assert!(
            String::from_utf8(response.bytes)
                .expect("response utf-8")
                .ends_with("hello")
        );
    }

    #[tokio::test]
    async fn upstream_transfer_encoding_response_does_not_reuse_connection() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let accept_count = Arc::new(AtomicUsize::new(0));
        let backend_accept_count = Arc::clone(&accept_count);

        let backend_task = tokio::spawn(async move {
            for index in 0..2 {
                let (mut stream, _) = backend.accept().await.expect("accept backend");
                backend_accept_count.fetch_add(1, Ordering::SeqCst);

                let _request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                    .await
                    .expect("read backend request");

                if index == 0 {
                    // 第一条响应故意返回 Transfer-Encoding，验证网关拒绝后不会把连接放回池里。
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
                        )
                        .await
                        .expect("write first backend response");
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .await
                        .expect("write second backend response");
                }
            }
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = Arc::new(ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        ));

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let mut outcomes = Vec::new();
            for _ in 0..2 {
                let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
                let service = Arc::clone(&service);
                outcomes.push(
                    service
                        .handle_connection("edge", &mut downstream, client_addr)
                        .await,
                );
            }
            outcomes
        });

        let mut first_client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect first client");
        first_client
            .write_all(b"GET /one HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write first request");

        let mut second_client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect second client");
        second_client
            .write_all(b"GET /two HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write second request");

        let outcomes = server.await.expect("gateway task");
        backend_task.await.expect("backend task");

        match &outcomes[0] {
            Err(ProxyConnectionError {
                error: GatewayError::Unsupported(message),
                ..
            }) => {
                assert!(message.contains("transfer-encoding"));
            }
            other => panic!(
                "expected first request to fail on upstream transfer-encoding, got {:?}",
                other
            ),
        }
        assert!(outcomes[1].is_ok());
        assert_eq!(accept_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn returns_timeout_when_upstream_response_stalls() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (_stream, _) = backend.accept().await.expect("accept backend");
            sleep(Duration::from_millis(120)).await;
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                upstream_read_timeout_ms: 50,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /slow HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Io(message),
                ..
            }) => {
                assert!(message.contains("timed out"));
            }
            other => panic!("expected upstream timeout error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_upstream_response_with_invalid_status_line() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let _request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            stream
                .write_all(b"HTTP/1.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write invalid upstream response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /bad-upstream HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Protocol(message),
                ..
            }) => assert!(message.contains("status code")),
            other => panic!(
                "expected invalid upstream status line error, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn rejects_upstream_response_with_too_many_headers() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let _request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nX-One: 1\r\nX-Two: 2\r\nX-Three: 3\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("write upstream response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                max_upstream_headers: 3,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /bad-upstream HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Protocol(message),
                ..
            }) => assert!(message.contains("too many headers")),
            other => panic!("expected too many upstream headers error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_upstream_response_body_larger_than_limit() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let _request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                )
                .await
                .expect("write upstream response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                max_upstream_body_bytes: 4,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /bad-upstream HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Protocol(message),
                ..
            }) => assert!(message.contains("body exceeds")),
            other => panic!("expected oversized upstream body error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_content_length_headers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\na",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("duplicate content-length"));
            }
            other => panic!("expected duplicate content-length error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_missing_host_header() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(b"GET /hello HTTP/1.1\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("host header"));
            }
            other => panic!("expected host header error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn retries_on_connect_failure_and_uses_next_endpoint() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve dead port");
        let dead_addr = reserved.local_addr().expect("dead addr");
        drop(reserved);

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            assert_eq!(request.path_for_route(), "/retry");

            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nretried",
                )
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                upstream_retry_attempts: 2,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![
                    EndpointConfig {
                        address: dead_addr.to_string(),
                        weight: 1,
                    },
                    EndpointConfig {
                        address: backend_addr.to_string(),
                        weight: 1,
                    },
                ],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            let completed = service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
                .expect("proxy request");
            assert_eq!(completed.retries, 1);
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /retry HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        server.await.expect("gateway task");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.ends_with("retried"));
    }

    #[tokio::test]
    async fn upstream_policy_can_extend_runtime_retry_budget() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve dead port");
        let dead_addr = reserved.local_addr().expect("dead addr");
        drop(reserved);

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let request = HttpRequest::read_from(&mut stream, &RuntimeSettings::default())
                .await
                .expect("read backend request");
            assert_eq!(request.path_for_route(), "/policy");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\npolicy",
                )
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                upstream_retry_attempts: 1,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: ProxyPolicyConfig::default(),
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: ProxyPolicyConfig {
                    connect_timeout_ms: None,
                    read_timeout_ms: None,
                    retry_attempts: Some(2),
                },
                endpoints: vec![
                    EndpointConfig {
                        address: dead_addr.to_string(),
                        weight: 1,
                    },
                    EndpointConfig {
                        address: backend_addr.to_string(),
                        weight: 1,
                    },
                ],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /policy HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task").expect("proxy request");
        assert_eq!(outcome.retries, 1);
        assert_eq!(outcome.response.status_code, 200);
    }

    #[tokio::test]
    async fn route_policy_overrides_upstream_retry_budget() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve dead port");
        let dead_addr = reserved.local_addr().expect("dead addr");
        drop(reserved);

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                upstream_retry_attempts: 3,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: ProxyPolicyConfig {
                    connect_timeout_ms: None,
                    read_timeout_ms: None,
                    retry_attempts: Some(1),
                },
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: ProxyPolicyConfig {
                    connect_timeout_ms: None,
                    read_timeout_ms: None,
                    retry_attempts: Some(2),
                },
                endpoints: vec![
                    EndpointConfig {
                        address: dead_addr.to_string(),
                        weight: 1,
                    },
                    EndpointConfig {
                        address: backend_addr.to_string(),
                        weight: 1,
                    },
                ],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /policy HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Io(message),
                retries,
                ..
            }) => {
                assert!(message.contains("connect upstream"));
                assert_eq!(retries, 1);
            }
            other => panic!(
                "expected route retry override to stop retries, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn route_policy_overrides_upstream_read_timeout() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            sleep(Duration::from_millis(80)).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                upstream_read_timeout_ms: 500,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:0".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: ProxyPolicyConfig {
                    connect_timeout_ms: None,
                    read_timeout_ms: Some(40),
                    retry_attempts: None,
                },
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: ProxyPolicyConfig {
                    connect_timeout_ms: None,
                    read_timeout_ms: Some(200),
                    retry_attempts: None,
                },
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let service = ProxyService::new(
            Router::from_config(&config),
            FilterRegistry::with_defaults(),
            UpstreamRegistry::from_config(&config),
            config.runtime_settings(),
        );

        let gateway = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway");
        let gateway_addr = gateway.local_addr().expect("gateway addr");

        let server = tokio::spawn(async move {
            let (mut downstream, client_addr) = gateway.accept().await.expect("accept gateway");
            service
                .handle_connection("edge", &mut downstream, client_addr)
                .await
        });

        let mut client = TcpStream::connect(gateway_addr)
            .await
            .expect("connect gateway");
        client
            .write_all(
                b"GET /slow-policy HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("gateway task");
        match outcome {
            Err(ProxyConnectionError {
                error: GatewayError::Io(message),
                ..
            }) => {
                assert!(message.contains("timed out"));
            }
            other => panic!("expected route read timeout override, got {:?}", other),
        }
    }

    #[test]
    fn error_response_maps_route_not_found_to_404() {
        let bytes = error_response(&GatewayError::RouteNotMatched);
        let text = String::from_utf8(bytes).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn error_response_maps_unsupported_feature_to_501() {
        let bytes = error_response(&GatewayError::Unsupported("expect header".into()));
        let text = String::from_utf8(bytes).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 501 Not Implemented"));
    }

    #[test]
    fn error_response_maps_rate_limited_to_429() {
        let bytes = error_response(&GatewayError::RateLimited("public route".into()));
        let text = String::from_utf8(bytes).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 429 Too Many Requests"));
    }

    #[tokio::test]
    async fn rejects_duplicate_host_headers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET /hello HTTP/1.1\r\nHost: one.test\r\nHost: two.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("duplicate host"));
            }
            other => panic!("expected duplicate host error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_request_target_that_is_not_origin_form() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET http://example.test/hello HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Unsupported(message)) => {
                assert!(message.contains("origin-form"));
            }
            other => panic!("expected origin-form error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn request_parser_keeps_path_and_query_separate() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET /share/view?token=abc123&scope=read HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let request = server
            .await
            .expect("server task")
            .expect("request should parse");

        assert_eq!(request.path_for_route(), "/share/view");
        assert_eq!(
            request.query_for_policy().as_deref(),
            Some("token=abc123&scope=read")
        );
    }

    #[tokio::test]
    async fn rejects_request_with_too_many_headers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            let settings = RuntimeSettings {
                max_request_headers: 2,
                ..RuntimeSettings::default()
            };
            HttpRequest::read_from(&mut stream, &settings).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET /hello HTTP/1.1\r\nHost: example.test\r\nX-One: 1\r\nX-Two: 2\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("too many headers"));
            }
            other => panic!("expected header count error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_request_body_larger_than_limit() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            let settings = RuntimeSettings {
                max_request_body_bytes: 4,
                ..RuntimeSettings::default()
            };
            HttpRequest::read_from(&mut stream, &settings).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("body exceeds"));
            }
            other => panic!("expected body limit error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_expect_100_continue_request() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nExpect: 100-continue\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Unsupported(message)) => {
                assert!(message.contains("100-continue"));
            }
            other => panic!("expected expect header rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_transfer_encoding_request() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Unsupported(message)) => {
                assert!(message.contains("transfer-encoding"));
            }
            other => panic!("expected transfer-encoding rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_transfer_encoding_with_content_length_request() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Unsupported(message)) => {
                assert!(message.contains("with content-length"));
            }
            other => panic!(
                "expected transfer-encoding with content-length rejection, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn rejects_multiple_downstream_requests_in_one_connection() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET /one HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\nGET /two HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Unsupported(message)) => {
                assert!(message.contains("pipelining"));
            }
            other => panic!("expected downstream pipelining rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_header_name() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            HttpRequest::read_from(&mut stream, &RuntimeSettings::default()).await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect listener");
        client
            .write_all(
                b"GET /hello HTTP/1.1\r\nHost: example.test\r\nBad Header: 1\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let outcome = server.await.expect("server task");
        match outcome {
            Err(GatewayError::Protocol(message)) => {
                assert!(message.contains("invalid header name"));
            }
            other => panic!("expected invalid header name error, got {:?}", other),
        }
    }
}
