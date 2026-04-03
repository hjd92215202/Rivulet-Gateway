//! `gateway-types` 只放跨模块共享的稳定类型。
//! 这里的结构尽量保持简单，避免把实现细节泄漏到所有 crate。
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

pub type Shared<T> = Arc<T>;

/// 网关内部统一错误类型。
/// 第一阶段先聚焦“能定位问题”，所以错误分类比错误层级更重要。
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("route did not match request")]
    RouteNotMatched,
    #[error("upstream cluster has no healthy endpoints: {0}")]
    NoHealthyUpstream(String),
    #[error("filter rejected request: {0}")]
    FilterRejected(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("unsupported feature: {0}")]
    Unsupported(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
}

pub type Result<T> = std::result::Result<T, GatewayError>;

/// 当前协议能力还很窄，只保留已经规划好的枚举值，避免接口后续频繁抖动。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    /// HTTP/1.1 是当前唯一真正走通真实流量的协议。
    Http1,
    /// HTTP/2 先把类型位置占住，后续单独补完整实现。
    Http2,
}

/// 请求方法会进入路由匹配、转发和日志，所以单独抽成共享类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    /// 对应最常见的读请求。
    Get,
    /// 对应带请求体的创建类请求。
    Post,
    /// 对应整资源覆盖写入。
    Put,
    /// 对应部分更新。
    Patch,
    /// 对应删除语义。
    Delete,
    /// 对应仅取响应头的探测请求。
    Head,
    /// 对应协商或探测能力的请求。
    Options,
}

impl Display for HttpMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        };
        f.write_str(value)
    }
}

impl TryFrom<&str> for HttpMethod {
    type Error = GatewayError;

    fn try_from(value: &str) -> Result<Self> {
        // 这里只接受已经明确支持的方法，避免“先放过去再说”带来语义歧义。
        match value {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "PATCH" => Ok(Self::Patch),
            "DELETE" => Ok(Self::Delete),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            other => Err(GatewayError::Unsupported(format!("http method {}", other))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    /// 请求进入的是哪个 listener。
    pub listener: String,
    /// 用于路由匹配和转发的主机名。
    pub host: String,
    /// 用于路径匹配的请求路径。
    pub path: String,
    /// 原始 query string 去掉前导 `?` 后的部分，给鉴权、分享令牌和灰度参数使用。
    pub query: Option<String>,
    /// 归一化后的 HTTP 方法。
    pub method: HttpMethod,
    /// 下游请求头快照，供鉴权、限流和分享隔离类策略读取。
    pub headers: Vec<(String, String)>,
    /// 客户端地址主要用于补充 `X-Forwarded-For`。
    pub client_addr: Option<SocketAddr>,
    /// 请求唯一标识，便于日志和链路定位。
    pub request_id: Option<String>,
    /// 需要由网关注入到上游的附加请求头。
    pub upstream_headers: Vec<(String, String)>,
    /// 当前请求如果命中了分享访问策略，这里会记录对应的 share id。
    pub share_id: Option<String>,
    /// 当前请求如果命中了分享访问策略，这里会记录对应的 share scope。
    pub share_scope: Option<String>,
}

impl RequestContext {
    /// `RequestContext` 是内核在请求生命周期内共享的最小上下文。
    /// 这里刻意不直接塞入原始 socket 或大块报文，避免后面耦合到传输层。
    pub fn new(
        listener: impl Into<String>,
        host: impl Into<String>,
        path: impl Into<String>,
        method: HttpMethod,
    ) -> Self {
        Self {
            // listener 名称由运行时入口注入，用于把请求绑定到具体入口策略。
            listener: listener.into(),
            // host 默认来自 Host 头，后面也可能被更高层协议适配填充。
            host: host.into(),
            // path 只保留路由匹配所需的路径部分，不把 query string 混进来。
            path: path.into(),
            // query 先由真实网络入口补充；纯内存调用默认没有 query。
            query: None,
            // method 是进入路由和代理的主分类条件之一。
            method,
            // 纯内存调用默认不带请求头。
            headers: Vec::new(),
            // 第一阶段只有真实网络入口会填这个值；纯内存调用时可以为空。
            client_addr: None,
            // request id 由过滤器补齐，不在构造函数里强行生成。
            request_id: None,
            // 默认没有附加上游头。
            upstream_headers: Vec::new(),
            // 默认没有分享访问上下文。
            share_id: None,
            share_scope: None,
        }
    }

    /// 从快照里读取指定 header 的第一个值。
    /// 当前先保持“按出现顺序取首个”的保守语义，避免在不同策略里出现隐式不一致。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(item, _)| item.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// 解析最常见的 `Authorization: Bearer <token>` 形式。
    pub fn bearer_token(&self) -> Option<&str> {
        let authorization = self.header("authorization")?;
        let (scheme, token) = authorization.split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        Some(token)
    }

    /// 从 query string 中按 `key=value` 形式提取首个值。
    /// 第一版先不做 URL decode，只提供稳定、可预期的最小解析能力。
    pub fn query_value(&self, key: &str) -> Option<&str> {
        let query = self.query.as_deref()?;
        for pair in query.split('&') {
            let (item_key, item_value) = pair.split_once('=')?;
            if item_key == key {
                return Some(item_value);
            }
        }
        None
    }

    /// 记录分享访问上下文，并准备好要注入给上游的稳定请求头。
    pub fn set_share_access(&mut self, share_id: &str, share_scope: &str) {
        self.share_id = Some(share_id.to_string());
        self.share_scope = Some(share_scope.to_string());
        self.upstream_headers.retain(|(name, _)| {
            !name.eq_ignore_ascii_case("x-rivulet-share-id")
                && !name.eq_ignore_ascii_case("x-rivulet-share-scope")
        });
        self.upstream_headers
            .push(("X-Rivulet-Share-Id".into(), share_id.to_string()));
        self.upstream_headers
            .push(("X-Rivulet-Share-Scope".into(), share_scope.to_string()));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseContext {
    /// 最终返回给客户端的状态码。
    pub status_code: u16,
    /// 实际选中的 upstream 地址，方便日志和排障。
    pub upstream: Option<String>,
}

impl ResponseContext {
    /// 第一阶段响应上下文只保留排障和过滤器需要的关键信息。
    pub fn new(status_code: u16) -> Self {
        Self {
            // 状态码是响应阶段最关键的摘要字段。
            status_code,
            // 默认先不绑定 upstream，由代理成功转发后再回填。
            upstream: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteMatch {
    /// 命中的路由名，用于日志和调试。
    pub route_name: String,
    /// 命中路由后应该转发到哪个 upstream 集群。
    pub upstream_name: String,
    /// 这次请求应该经过哪些过滤器。
    pub filter_names: Vec<String>,
    /// 路由层对上游超时和重试的可选覆盖策略。
    pub proxy_policy: ProxyPolicyOverrides,
    /// 路由命中后的鉴权策略。
    pub auth_policy: RouteAuthPolicy,
    /// 路由命中后的限流策略。
    pub rate_limit_policy: RouteRateLimitPolicy,
    /// 路由命中后的分享访问隔离策略。
    pub share_policy: RouteSharePolicy,
}

/// 路由级鉴权策略先保持最小可用集：
/// Bearer token 负责“统一入口鉴权”，query token 负责“分享链接/公开页保护”这类受控放行。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteAuthPolicy {
    /// 允许通过 `Authorization: Bearer <token>` 访问的静态 token 列表。
    pub bearer_tokens: Vec<String>,
    /// 允许通过 query string 放行的静态 token 列表。
    pub query_tokens: Vec<String>,
    /// query token 使用的参数名，默认保守地使用 `access_token`。
    pub query_token_name: String,
}

impl Default for RouteAuthPolicy {
    fn default() -> Self {
        Self {
            bearer_tokens: Vec::new(),
            query_tokens: Vec::new(),
            query_token_name: "access_token".into(),
        }
    }
}

impl RouteAuthPolicy {
    /// 只要没有配置任何 token，这条路由就视为“未开启鉴权”。
    pub fn is_enabled(&self) -> bool {
        !self.bearer_tokens.is_empty() || !self.query_tokens.is_empty()
    }

    /// 第一版先做确定性最强的静态 token 匹配，不引入外部状态或复杂依赖。
    pub fn authorize(&self, request: &RequestContext) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        if let Some(token) = request.bearer_token() {
            if self.bearer_tokens.iter().any(|item| item == token) {
                return Ok(());
            }
        }

        if let Some(token) = request.query_value(&self.query_token_name) {
            if self.query_tokens.iter().any(|item| item == token) {
                return Ok(());
            }
        }

        Err(GatewayError::Unauthorized(format!(
            "route requires a valid bearer token or {} query token",
            self.query_token_name
        )))
    }
}

/// 第一版限流策略只做最保守的入口保护：
/// 按客户端 IP 在固定时间窗口内计数，先解决公开页面和共享入口的基本防刷需求。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRateLimitPolicy {
    /// 单个窗口内允许的最大请求数。
    pub requests: Option<usize>,
    /// 统计窗口大小。
    pub window: Option<Duration>,
    /// 限流 key 的提取方式。
    pub key: RouteRateLimitKey,
}

impl Default for RouteRateLimitPolicy {
    fn default() -> Self {
        Self {
            requests: None,
            window: None,
            key: RouteRateLimitKey::ClientIp,
        }
    }
}

impl RouteRateLimitPolicy {
    /// 只有请求数和时间窗口都明确配置时，限流才真正开启。
    pub fn is_enabled(&self) -> bool {
        self.requests.is_some() && self.window.is_some()
    }

    /// 为请求提取稳定的限流 key。
    /// 第一版先以客户端 IP 为主；如果调用方没有提供远端地址，就回落到固定占位值。
    pub fn key_for(&self, request: &RequestContext) -> String {
        match self.key {
            RouteRateLimitKey::ClientIp => request
                .client_addr
                .map(|addr| addr.ip().to_string())
                .unwrap_or_else(|| "unknown-client".into()),
        }
    }
}

/// 当前只开放一类 key，先把行为做稳，再决定是否扩展到 header、token 或其他来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteRateLimitKey {
    ClientIp,
}

/// 第一版分享访问隔离策略：
/// 先用 query token 命中分享授权，再把明确的 share 元数据传给上游。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteSharePolicy {
    /// 分享 token 的 query 参数名。
    pub query_token_name: String,
    /// 当前路由允许的分享授权集合。
    pub grants: Vec<RouteShareGrant>,
}

impl Default for RouteSharePolicy {
    fn default() -> Self {
        Self {
            query_token_name: "share_token".into(),
            grants: Vec::new(),
        }
    }
}

impl RouteSharePolicy {
    /// 只有显式配置了 grants，这条路由才真正启用分享访问隔离。
    pub fn is_enabled(&self) -> bool {
        !self.grants.is_empty()
    }

    /// 根据 query token 匹配分享授权。
    /// 第一版故意只做显式 token 命中，不做模糊兜底。
    pub fn authorize(&self, request: &mut RequestContext) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let token = request.query_value(&self.query_token_name).ok_or_else(|| {
            GatewayError::Unauthorized(format!(
                "route requires {} query token for shared access",
                self.query_token_name
            ))
        })?;

        let grant = self
            .grants
            .iter()
            .find(|item| item.token == token)
            .ok_or_else(|| {
                GatewayError::Unauthorized(format!(
                    "route requires a valid {} query token for shared access",
                    self.query_token_name
                ))
            })?;

        request.set_share_access(&grant.share_id, &grant.scope);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteShareGrant {
    /// 分享 token 本身。
    pub token: String,
    /// 这次分享访问在上游侧看到的稳定 share id。
    pub share_id: String,
    /// 分享访问的语义作用域，例如 read / preview。
    pub scope: String,
}

/// 上游节点描述只保留“如何连过去”所需的最小字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpstreamEndpoint {
    /// 实际建立 TCP 连接用的地址。
    pub address: String,
    /// 权重字段先保留，当前 round robin 还未消费它。
    pub weight: u16,
}

/// 路由和 upstream 层都只做“可选覆盖”，
/// 没有明确写出来的字段会自然回落到更低层的默认值。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProxyPolicyOverrides {
    /// 如果有值，就覆盖上游 TCP 建连超时。
    pub upstream_connect_timeout: Option<Duration>,
    /// 如果有值，就覆盖上游响应读取超时。
    pub upstream_read_timeout: Option<Duration>,
    /// 如果有值，就覆盖单次请求可用的最大尝试次数。
    pub upstream_retry_attempts: Option<usize>,
}

impl ProxyPolicyOverrides {
    /// 当前这层的显式配置优先级最高，只有没写的字段才回落到 fallback。
    pub fn or_else(&self, fallback: &Self) -> Self {
        Self {
            // connect timeout 按最近一层有效值生效。
            upstream_connect_timeout: self
                .upstream_connect_timeout
                .or(fallback.upstream_connect_timeout),
            // read timeout 与其他字段保持同一优先级语义。
            upstream_read_timeout: self
                .upstream_read_timeout
                .or(fallback.upstream_read_timeout),
            // retry attempts 也按同一规则合成，避免局部特例。
            upstream_retry_attempts: self
                .upstream_retry_attempts
                .or(fallback.upstream_retry_attempts),
        }
    }

    /// 把可选覆盖解析成代理真正执行时用的确定策略。
    pub fn resolve(&self, runtime: &RuntimeSettings) -> ResolvedProxyPolicy {
        let runtime_policy = runtime.proxy_policy();
        ResolvedProxyPolicy {
            // 如果这层没写 connect timeout，就回落到全局默认。
            upstream_connect_timeout: self
                .upstream_connect_timeout
                .unwrap_or(runtime_policy.upstream_connect_timeout),
            // 如果这层没写 read timeout，就回落到全局默认。
            upstream_read_timeout: self
                .upstream_read_timeout
                .unwrap_or(runtime_policy.upstream_read_timeout),
            // 如果这层没写 retry attempts，就回落到全局默认。
            upstream_retry_attempts: self
                .upstream_retry_attempts
                .unwrap_or(runtime_policy.upstream_retry_attempts),
        }
    }
}

/// 代理执行路径只认这个“已经解析好的最终策略”。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedProxyPolicy {
    /// 最终生效的 connect timeout。
    pub upstream_connect_timeout: Duration,
    /// 最终生效的 read timeout。
    pub upstream_read_timeout: Duration,
    /// 最终生效的 retry attempts。
    pub upstream_retry_attempts: usize,
}

/// 运行时设置在启动后视为只读快照。
/// 这样后续做热更新时可以明确区分“配置输入”和“运行时生效值”。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSettings {
    /// 运行时 worker 线程数。
    pub worker_threads: usize,
    /// 优雅关闭时允许等待在途连接自然完成的窗口。
    pub graceful_shutdown: Duration,
    /// 读取下游请求头和请求体的超时。
    pub downstream_read_timeout: Duration,
    /// 与上游建立 TCP 连接的超时。
    pub upstream_connect_timeout: Duration,
    /// 读取上游响应的超时。
    pub upstream_read_timeout: Duration,
    /// 单次请求允许的最大尝试次数，至少为 1。
    pub upstream_retry_attempts: usize,
    /// 每个上游 endpoint 允许缓存的空闲连接数量。
    pub upstream_idle_pool_size: usize,
    /// 允许的最大上游状态行字节数。
    pub max_upstream_status_line_bytes: usize,
    /// 允许的最大上游响应头数量。
    pub max_upstream_headers: usize,
    /// 允许的最大上游响应头字节数。
    pub max_upstream_header_bytes: usize,
    /// 允许的最大上游响应体字节数。
    pub max_upstream_body_bytes: usize,
    /// 允许的最大请求行字节数。
    pub max_request_line_bytes: usize,
    /// 允许的最大请求头数量。
    pub max_request_headers: usize,
    /// 允许的最大请求体大小。
    pub max_request_body_bytes: usize,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            worker_threads: 4,
            graceful_shutdown: Duration::from_secs(30),
            downstream_read_timeout: Duration::from_secs(5),
            upstream_connect_timeout: Duration::from_secs(3),
            upstream_read_timeout: Duration::from_secs(5),
            upstream_retry_attempts: 2,
            upstream_idle_pool_size: 1,
            max_upstream_status_line_bytes: 8 * 1024,
            max_upstream_headers: 100,
            max_upstream_header_bytes: 64 * 1024,
            max_upstream_body_bytes: 8 * 1024 * 1024,
            max_request_line_bytes: 8 * 1024,
            max_request_headers: 100,
            max_request_body_bytes: 1024 * 1024,
        }
    }
}

impl RuntimeSettings {
    /// 把全局运行时设置转成代理可直接使用的最终策略。
    pub fn proxy_policy(&self) -> ResolvedProxyPolicy {
        ResolvedProxyPolicy {
            // connect timeout 直接来自运行时的全局配置。
            upstream_connect_timeout: self.upstream_connect_timeout,
            // read timeout 直接来自运行时的全局配置。
            upstream_read_timeout: self.upstream_read_timeout,
            // retry attempts 在进入这里前已经做过最小保底。
            upstream_retry_attempts: self.upstream_retry_attempts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_try_from_known_value() {
        let method = HttpMethod::try_from("POST").expect("method should parse");
        assert_eq!(method, HttpMethod::Post);
    }

    #[test]
    fn http_method_try_from_unknown_value_returns_error() {
        let error = HttpMethod::try_from("TRACE").expect_err("trace should be unsupported");
        match error {
            GatewayError::Unsupported(message) => assert!(message.contains("TRACE")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn runtime_settings_default_retry_attempts_are_positive() {
        let settings = RuntimeSettings::default();
        assert!(settings.upstream_retry_attempts >= 1);
        assert!(settings.upstream_idle_pool_size >= 1);
        assert!(settings.max_upstream_status_line_bytes >= 256);
        assert!(settings.max_upstream_headers >= 1);
        assert!(settings.max_upstream_header_bytes >= 1024);
        assert!(settings.max_request_line_bytes >= 256);
        assert!(settings.max_request_headers >= 1);
    }

    #[test]
    fn proxy_policy_overrides_prefer_nearest_scope() {
        let runtime = RuntimeSettings::default();
        let upstream = ProxyPolicyOverrides {
            upstream_connect_timeout: Some(Duration::from_secs(2)),
            upstream_read_timeout: Some(Duration::from_secs(4)),
            upstream_retry_attempts: Some(3),
        };
        let route = ProxyPolicyOverrides {
            upstream_connect_timeout: Some(Duration::from_secs(1)),
            upstream_read_timeout: None,
            upstream_retry_attempts: Some(1),
        };

        let resolved = route.or_else(&upstream).resolve(&runtime);

        assert_eq!(resolved.upstream_connect_timeout, Duration::from_secs(1));
        assert_eq!(resolved.upstream_read_timeout, Duration::from_secs(4));
        assert_eq!(resolved.upstream_retry_attempts, 1);
    }

    #[test]
    fn request_context_can_read_headers_bearer_and_query() {
        let mut request =
            RequestContext::new("edge", "example.test", "/share/view", HttpMethod::Get);
        request.query = Some("token=abc123&scope=read".into());
        request.headers = vec![
            ("Authorization".into(), "Bearer secret-token".into()),
            ("X-Share-Id".into(), "share-001".into()),
        ];

        assert_eq!(request.header("x-share-id"), Some("share-001"));
        assert_eq!(request.bearer_token(), Some("secret-token"));
        assert_eq!(request.query_value("token"), Some("abc123"));
        assert_eq!(request.query_value("missing"), None);
        assert!(request.upstream_headers.is_empty());
    }

    #[test]
    fn route_auth_policy_accepts_matching_bearer_or_query_token() {
        let mut request =
            RequestContext::new("edge", "example.test", "/share/view", HttpMethod::Get);
        request.headers = vec![("Authorization".into(), "Bearer gateway-secret".into())];
        request.query = Some("access_token=share-secret".into());

        let policy = RouteAuthPolicy {
            bearer_tokens: vec!["gateway-secret".into()],
            query_tokens: vec!["share-secret".into()],
            query_token_name: "access_token".into(),
        };

        assert!(policy.authorize(&request).is_ok());

        request.headers.clear();
        assert!(policy.authorize(&request).is_ok());
    }

    #[test]
    fn route_auth_policy_rejects_request_without_valid_token() {
        let request = RequestContext::new("edge", "example.test", "/private", HttpMethod::Get);
        let policy = RouteAuthPolicy {
            bearer_tokens: vec!["gateway-secret".into()],
            query_tokens: Vec::new(),
            query_token_name: "access_token".into(),
        };

        let error = policy
            .authorize(&request)
            .expect_err("request should be rejected");
        match error {
            GatewayError::Unauthorized(message) => {
                assert!(message.contains("route requires"));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn route_rate_limit_policy_uses_client_ip_key() {
        let mut request = RequestContext::new("edge", "example.test", "/public", HttpMethod::Get);
        request.client_addr = Some("127.0.0.1:8080".parse().expect("socket addr"));

        let policy = RouteRateLimitPolicy {
            requests: Some(10),
            window: Some(Duration::from_secs(1)),
            key: RouteRateLimitKey::ClientIp,
        };

        assert!(policy.is_enabled());
        assert_eq!(policy.key_for(&request), "127.0.0.1");
    }

    #[test]
    fn route_share_policy_sets_share_headers_on_match() {
        let mut request =
            RequestContext::new("edge", "example.test", "/share/view", HttpMethod::Get);
        request.query = Some("share_token=share-secret".into());

        let policy = RouteSharePolicy {
            query_token_name: "share_token".into(),
            grants: vec![RouteShareGrant {
                token: "share-secret".into(),
                share_id: "share-001".into(),
                scope: "read".into(),
            }],
        };

        policy
            .authorize(&mut request)
            .expect("share should authorize");

        assert_eq!(request.share_id.as_deref(), Some("share-001"));
        assert_eq!(request.share_scope.as_deref(), Some("read"));
        assert_eq!(
            request.upstream_headers,
            vec![
                ("X-Rivulet-Share-Id".into(), "share-001".into()),
                ("X-Rivulet-Share-Scope".into(), "read".into()),
            ]
        );
    }
}
