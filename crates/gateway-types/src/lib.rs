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
        // 这里故意只接受已经明确支持的方法，避免“先放过去再说”带来语义歧义。
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
    /// 归一化后的 HTTP 方法。
    pub method: HttpMethod,
    /// 客户端地址主要用于补充 `X-Forwarded-For`。
    pub client_addr: Option<SocketAddr>,
    /// 请求唯一标识，便于日志和链路定位。
    pub request_id: Option<String>,
}

impl RequestContext {
    /// `RequestContext` 是内核在请求生命周期内共享的最小上下文。
    /// 这里故意不直接塞入原始 socket 或大块报文，避免后面耦合到传输层。
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
            // method 是进入路由和代理的主分类条件之一。
            method,
            // 第一阶段只有真实网络入口会填这个值；纯内存调用时可以为空。
            client_addr: None,
            // request id 由过滤器补齐，不在构造函数里强行生成。
            request_id: None,
        }
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
}

/// 上游节点描述只保留“如何连过去”所需的最小字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpstreamEndpoint {
    /// 实际建立 TCP 连接用的地址。
    pub address: String,
    /// 权重字段先保留，当前 round robin 还未消费它。
    pub weight: u16,
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
            max_request_line_bytes: 8 * 1024,
            max_request_headers: 100,
            max_request_body_bytes: 1024 * 1024,
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
        assert!(settings.max_request_line_bytes >= 256);
        assert!(settings.max_request_headers >= 1);
    }
}
