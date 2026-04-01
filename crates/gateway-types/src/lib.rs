use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

pub type Shared<T> = Arc<T>;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Protocol {
    Http1,
    Http2,
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpstreamEndpoint {
    pub address: String,
    pub weight: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSettings {
    pub worker_threads: usize,
    pub graceful_shutdown: Duration,
    pub downstream_read_timeout: Duration,
    pub upstream_connect_timeout: Duration,
    pub upstream_read_timeout: Duration,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            worker_threads: 4,
            graceful_shutdown: Duration::from_secs(30),
            downstream_read_timeout: Duration::from_secs(5),
            upstream_connect_timeout: Duration::from_secs(3),
            upstream_read_timeout: Duration::from_secs(5),
        }
    }
}
