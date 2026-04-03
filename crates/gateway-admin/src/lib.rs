//! 只读管理面 member 负责两类内容：
//! 1. 原生 HTML/CSS/JS 资产
//! 2. 管理面只读接口和 HTTP 响应拼装
//! 第一版明确不做配置写回，先把观测、验收和排障体验打牢。

use std::sync::Arc;

use gateway_types::{HttpMethod, RequestContext};

/// runtime 通过这个 trait 提供“当前系统快照”。
/// 管理面不直接依赖 runtime 内部实现，只消费稳定的摘要结构。
pub trait AdminOverviewProvider: Send + Sync {
    fn overview(&self) -> AdminOverview;
}

/// 管理面服务只负责识别管理路径并生成响应。
/// 真正的数据来源由外部 provider 注入，避免 member 之间相互缠绕。
#[derive(Clone)]
pub struct AdminService {
    provider: Arc<dyn AdminOverviewProvider>,
}

/// 为了让 proxy 层能够直接短路回写响应，这里返回完整的状态码和字节缓冲。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminHttpResponse {
    pub status_code: u16,
    pub bytes: Vec<u8>,
}

/// 管理面的总览快照由几类稳定片段组成。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminOverview {
    pub summary: AdminSummary,
    pub runtime: AdminRuntime,
    pub stats: AdminStats,
    pub listeners: Vec<AdminListener>,
    pub routes: Vec<AdminRoute>,
    pub upstreams: Vec<AdminUpstream>,
}

/// 顶层规模摘要，主要给首页卡片和快速 sanity check 使用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminSummary {
    pub listeners: usize,
    pub routes: usize,
    pub upstreams: usize,
    pub worker_threads: usize,
}

/// 运行时关键参数快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRuntime {
    pub graceful_shutdown_secs: u64,
    pub downstream_read_timeout_ms: u128,
    pub upstream_connect_timeout_ms: u128,
    pub upstream_read_timeout_ms: u128,
    pub upstream_retry_attempts: usize,
    pub upstream_idle_pool_size: usize,
}

/// 运行时指标快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminStats {
    pub total_requests: u64,
    pub completed_requests: u64,
    pub active_connections: u64,
    pub successful_responses: u64,
    pub client_error_responses: u64,
    pub server_error_responses: u64,
    pub upstream_retries: u64,
}

/// listener 摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminListener {
    pub name: String,
    pub address: String,
    pub protocol: String,
}

/// route 摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRoute {
    pub name: String,
    pub listener: String,
    pub hosts: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub methods: Vec<String>,
    pub upstream: String,
}

/// upstream 摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminUpstream {
    pub name: String,
    pub load_balance: String,
    pub endpoints: Vec<String>,
}

impl AdminService {
    /// 构造一个只读管理面服务。
    pub fn new(provider: Arc<dyn AdminOverviewProvider>) -> Self {
        Self { provider }
    }

    /// 如果请求命中了管理命名空间，就直接在这里返回响应；
    /// 否则返回 `None`，让 proxy 继续走普通业务链路。
    pub fn maybe_handle(&self, request: &RequestContext) -> Option<AdminHttpResponse> {
        if !request.path.starts_with("/__admin") {
            return None;
        }

        // 第一版故意只开放给本机环回访问，避免把管理面直接暴露到公网。
        if !request
            .client_addr
            .map(|addr| addr.ip().is_loopback())
            .unwrap_or(false)
        {
            return Some(text_response(
                request.method,
                403,
                "Forbidden",
                "admin ui is only available from loopback in the first cut\n",
            ));
        }

        match request.method {
            HttpMethod::Get | HttpMethod::Head => Some(self.handle_read(request)),
            _ => Some(text_response(
                request.method,
                405,
                "Method Not Allowed",
                "admin ui only supports GET and HEAD in the first cut\n",
            )),
        }
    }

    /// 这里把可读资产和只读 API 都收口到同一个路径分发器中。
    fn handle_read(&self, request: &RequestContext) -> AdminHttpResponse {
        match request.path.as_str() {
            "/__admin" | "/__admin/" | "/__admin/index.html" => asset_response(
                request.method,
                200,
                "OK",
                "text/html; charset=utf-8",
                include_str!("../assets/index.html"),
            ),
            "/__admin/styles.css" => asset_response(
                request.method,
                200,
                "OK",
                "text/css; charset=utf-8",
                include_str!("../assets/styles.css"),
            ),
            "/__admin/app.js" => asset_response(
                request.method,
                200,
                "OK",
                "application/javascript; charset=utf-8",
                include_str!("../assets/app.js"),
            ),
            "/__admin/api/overview" => {
                let overview = self.provider.overview();
                asset_response(
                    request.method,
                    200,
                    "OK",
                    "application/json; charset=utf-8",
                    &overview.to_json(),
                )
            }
            _ => text_response(request.method, 404, "Not Found", "admin asset not found\n"),
        }
    }
}

impl AdminOverview {
    /// 先手写最小 JSON 输出，避免引入额外序列化风险。
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"summary\":{},",
                "\"runtime\":{},",
                "\"stats\":{},",
                "\"listeners\":{},",
                "\"routes\":{},",
                "\"upstreams\":{}",
                "}}"
            ),
            self.summary.to_json(),
            self.runtime.to_json(),
            self.stats.to_json(),
            json_array(self.listeners.iter().map(AdminListener::to_json)),
            json_array(self.routes.iter().map(AdminRoute::to_json)),
            json_array(self.upstreams.iter().map(AdminUpstream::to_json)),
        )
    }
}

impl AdminSummary {
    fn to_json(&self) -> String {
        format!(
            "{{\"listeners\":{},\"routes\":{},\"upstreams\":{},\"worker_threads\":{}}}",
            self.listeners, self.routes, self.upstreams, self.worker_threads
        )
    }
}

impl AdminRuntime {
    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"graceful_shutdown_secs\":{},",
                "\"downstream_read_timeout_ms\":{},",
                "\"upstream_connect_timeout_ms\":{},",
                "\"upstream_read_timeout_ms\":{},",
                "\"upstream_retry_attempts\":{},",
                "\"upstream_idle_pool_size\":{}",
                "}}"
            ),
            self.graceful_shutdown_secs,
            self.downstream_read_timeout_ms,
            self.upstream_connect_timeout_ms,
            self.upstream_read_timeout_ms,
            self.upstream_retry_attempts,
            self.upstream_idle_pool_size,
        )
    }
}

impl AdminStats {
    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"total_requests\":{},",
                "\"completed_requests\":{},",
                "\"active_connections\":{},",
                "\"successful_responses\":{},",
                "\"client_error_responses\":{},",
                "\"server_error_responses\":{},",
                "\"upstream_retries\":{}",
                "}}"
            ),
            self.total_requests,
            self.completed_requests,
            self.active_connections,
            self.successful_responses,
            self.client_error_responses,
            self.server_error_responses,
            self.upstream_retries,
        )
    }
}

impl AdminListener {
    fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"address\":\"{}\",\"protocol\":\"{}\"}}",
            escape_json(&self.name),
            escape_json(&self.address),
            escape_json(&self.protocol),
        )
    }
}

impl AdminRoute {
    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"name\":\"{}\",",
                "\"listener\":\"{}\",",
                "\"hosts\":{},",
                "\"path_prefixes\":{},",
                "\"methods\":{},",
                "\"upstream\":\"{}\"",
                "}}"
            ),
            escape_json(&self.name),
            escape_json(&self.listener),
            json_string_array(&self.hosts),
            json_string_array(&self.path_prefixes),
            json_string_array(&self.methods),
            escape_json(&self.upstream),
        )
    }
}

impl AdminUpstream {
    fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"load_balance\":\"{}\",\"endpoints\":{}}}",
            escape_json(&self.name),
            escape_json(&self.load_balance),
            json_string_array(&self.endpoints),
        )
    }
}

/// 所有管理面资源都显式关闭缓存，避免灰度期看到旧数据。
fn asset_response(
    method: HttpMethod,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) -> AdminHttpResponse {
    let body_bytes = body.as_bytes();
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: {}\r\nCache-Control: no-store\r\n\r\n",
        status_code,
        reason,
        body_bytes.len(),
        content_type,
    )
    .into_bytes();

    // `HEAD` 仍然返回完整 header，但不回写 body。
    if method == HttpMethod::Get {
        response.extend_from_slice(body_bytes);
    }

    AdminHttpResponse {
        status_code,
        bytes: response,
    }
}

/// 纯文本响应统一走同一条拼装路径，避免管理面自己再分叉响应格式。
fn text_response(
    method: HttpMethod,
    status_code: u16,
    reason: &str,
    body: &str,
) -> AdminHttpResponse {
    asset_response(
        method,
        status_code,
        reason,
        "text/plain; charset=utf-8",
        body,
    )
}

/// 字符串数组要逐项转义，避免把配置值直接拼坏 JSON。
fn json_string_array(items: &[String]) -> String {
    json_array(
        items
            .iter()
            .map(|item| format!("\"{}\"", escape_json(item.as_str()))),
    )
}

/// 最小 JSON array 生成器。
fn json_array<I>(items: I) -> String
where
    I: IntoIterator<Item = String>,
{
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

/// 只处理当前管理面会遇到的必要转义字符。
fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    struct FakeProvider;

    impl AdminOverviewProvider for FakeProvider {
        fn overview(&self) -> AdminOverview {
            AdminOverview {
                summary: AdminSummary {
                    listeners: 1,
                    routes: 2,
                    upstreams: 1,
                    worker_threads: 4,
                },
                runtime: AdminRuntime {
                    graceful_shutdown_secs: 30,
                    downstream_read_timeout_ms: 5000,
                    upstream_connect_timeout_ms: 3000,
                    upstream_read_timeout_ms: 4000,
                    upstream_retry_attempts: 2,
                    upstream_idle_pool_size: 1,
                },
                stats: AdminStats {
                    total_requests: 10,
                    completed_requests: 9,
                    active_connections: 1,
                    successful_responses: 8,
                    client_error_responses: 1,
                    server_error_responses: 0,
                    upstream_retries: 2,
                },
                listeners: vec![AdminListener {
                    name: "edge".into(),
                    address: "127.0.0.1:8080".into(),
                    protocol: "http1".into(),
                }],
                routes: vec![AdminRoute {
                    name: "api".into(),
                    listener: "edge".into(),
                    hosts: vec!["example.test".into()],
                    path_prefixes: vec!["/api".into()],
                    methods: vec!["GET".into()],
                    upstream: "api-cluster".into(),
                }],
                upstreams: vec![AdminUpstream {
                    name: "api-cluster".into(),
                    load_balance: "round_robin".into(),
                    endpoints: vec!["127.0.0.1:9000".into()],
                }],
            }
        }
    }

    fn loopback_request(path: &str, method: HttpMethod) -> RequestContext {
        let mut request = RequestContext::new("edge", "localhost:8080", path, method);
        request.client_addr = Some(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            40000,
        )));
        request
    }

    #[test]
    fn ignores_non_admin_path() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let request = loopback_request("/api/orders", HttpMethod::Get);
        assert_eq!(service.maybe_handle(&request), None);
    }

    #[test]
    fn rejects_remote_admin_access() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let mut request = RequestContext::new("edge", "example.test", "/__admin/", HttpMethod::Get);
        request.client_addr = Some(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(10, 0, 0, 2),
            41000,
        )));

        let response = service
            .maybe_handle(&request)
            .expect("admin response should exist");

        assert_eq!(response.status_code, 403);
        assert!(
            String::from_utf8(response.bytes)
                .expect("utf-8")
                .contains("loopback")
        );
    }

    #[test]
    fn serves_admin_html() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let response = service
            .maybe_handle(&loopback_request("/__admin/", HttpMethod::Get))
            .expect("admin response should exist");

        let text = String::from_utf8(response.bytes).expect("utf-8");
        assert_eq!(response.status_code, 200);
        assert!(text.contains("溪流网关管理面"));
        assert!(text.contains("Content-Type: text/html; charset=utf-8"));
    }

    #[test]
    fn serves_admin_overview_json() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let response = service
            .maybe_handle(&loopback_request("/__admin/api/overview", HttpMethod::Get))
            .expect("admin response should exist");

        let text = String::from_utf8(response.bytes).expect("utf-8");
        assert_eq!(response.status_code, 200);
        assert!(text.contains("\"worker_threads\":4"));
        assert!(text.contains("\"listeners\":["));
        assert!(text.contains("\"upstreams\":["));
    }

    #[test]
    fn head_request_returns_headers_without_body() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let response = service
            .maybe_handle(&loopback_request("/__admin/api/overview", HttpMethod::Head))
            .expect("admin response should exist");

        let text = String::from_utf8(response.bytes).expect("utf-8");
        assert_eq!(response.status_code, 200);
        assert!(text.contains("Content-Type: application/json; charset=utf-8"));
        assert!(!text.contains("\"summary\""));
    }

    #[test]
    fn rejects_non_read_admin_method() {
        let service = AdminService::new(Arc::new(FakeProvider));
        let response = service
            .maybe_handle(&loopback_request("/__admin/api/overview", HttpMethod::Post))
            .expect("admin response should exist");

        let text = String::from_utf8(response.bytes).expect("utf-8");
        assert_eq!(response.status_code, 405);
        assert!(text.starts_with("HTTP/1.1 405 Method Not Allowed"));
        assert!(text.contains("Content-Type: text/plain; charset=utf-8"));
    }

    #[test]
    fn escapes_json_content() {
        let escaped = escape_json("line1\n\"quoted\"\\tail");
        assert_eq!(escaped, "line1\\n\\\"quoted\\\"\\\\tail");
    }
}
