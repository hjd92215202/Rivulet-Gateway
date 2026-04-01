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
    Http1,
    Http2,
}

/// 请求方法会进入路由匹配、转发和日志，所以单独抽成共享类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
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
    pub listener: String,
    pub host: String,
    pub path: String,
    pub method: HttpMethod,
    pub client_addr: Option<SocketAddr>,
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
            listener: listener.into(),
            host: host.into(),
            path: path.into(),
            method,
            client_addr: None,
            request_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseContext {
    pub status_code: u16,
    pub upstream: Option<String>,
}

impl ResponseContext {
    /// 第一阶段响应上下文只保留排障和过滤器需要的关键信息。
    pub fn new(status_code: u16) -> Self {
        Self {
            status_code,
            upstream: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteMatch {
    pub route_name: String,
    pub upstream_name: String,
    pub filter_names: Vec<String>,
}

/// 上游节点描述只保留“如何连过去”所需的最小字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpstreamEndpoint {
    pub address: String,
    pub weight: u16,
}

/// 运行时设置在启动后视为只读快照。
/// 这样后续做热更新时可以明确区分“配置输入”和“运行时生效值”。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSettings {
    pub worker_threads: usize,
    pub graceful_shutdown: Duration,
    pub downstream_read_timeout: Duration,
    pub upstream_connect_timeout: Duration,
    pub upstream_read_timeout: Duration,
    pub upstream_retry_attempts: usize,
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
    }
}
