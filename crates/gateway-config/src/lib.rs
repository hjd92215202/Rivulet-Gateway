use std::fs;
use std::path::Path;
use std::time::Duration;

use gateway_types::{GatewayError, HttpMethod, Protocol, Result, RuntimeSettings};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct GatewayConfigFile {
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub listeners: Vec<ListenerConfig>,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
}

impl GatewayConfigFile {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .map_err(|err| GatewayError::Io(format!("{}: {}", path.display(), err)))?;
        let config: Self =
            toml::from_str(&raw).map_err(|err| GatewayError::InvalidConfig(err.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.listeners.is_empty() {
            return Err(GatewayError::InvalidConfig(
                "at least one listener is required".into(),
            ));
        }
        if self.upstreams.is_empty() {
            return Err(GatewayError::InvalidConfig(
                "at least one upstream is required".into(),
            ));
        }
        for listener in &self.listeners {
            if listener.name.trim().is_empty() {
                return Err(GatewayError::InvalidConfig(
                    "listener name must not be empty".into(),
                ));
            }
            if listener.address.trim().is_empty() {
                return Err(GatewayError::InvalidConfig(format!(
                    "listener {} address must not be empty",
                    listener.name
                )));
            }
        }
        for upstream in &self.upstreams {
            if upstream.endpoints.is_empty() {
                return Err(GatewayError::InvalidConfig(format!(
                    "upstream {} must contain at least one endpoint",
                    upstream.name
                )));
            }
        }
        for route in &self.routes {
            if !self
                .listeners
                .iter()
                .any(|item| item.name == route.listener)
            {
                return Err(GatewayError::InvalidConfig(format!(
                    "route {} references unknown listener {}",
                    route.name, route.listener
                )));
            }
            if !self
                .upstreams
                .iter()
                .any(|item| item.name == route.upstream)
            {
                return Err(GatewayError::InvalidConfig(format!(
                    "route {} references unknown upstream {}",
                    route.name, route.upstream
                )));
            }
        }
        Ok(())
    }

    pub fn runtime_settings(&self) -> RuntimeSettings {
        RuntimeSettings {
            worker_threads: self.runtime.worker_threads,
            graceful_shutdown: Duration::from_secs(self.runtime.graceful_shutdown_secs),
            downstream_read_timeout: Duration::from_millis(self.runtime.downstream_read_timeout_ms),
            upstream_connect_timeout: Duration::from_millis(
                self.runtime.upstream_connect_timeout_ms,
            ),
            upstream_read_timeout: Duration::from_millis(self.runtime.upstream_read_timeout_ms),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    #[serde(default = "default_graceful_shutdown_secs")]
    pub graceful_shutdown_secs: u64,
    #[serde(default = "default_downstream_read_timeout_ms")]
    pub downstream_read_timeout_ms: u64,
    #[serde(default = "default_upstream_connect_timeout_ms")]
    pub upstream_connect_timeout_ms: u64,
    #[serde(default = "default_upstream_read_timeout_ms")]
    pub upstream_read_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: default_worker_threads(),
            graceful_shutdown_secs: default_graceful_shutdown_secs(),
            downstream_read_timeout_ms: default_downstream_read_timeout_ms(),
            upstream_connect_timeout_ms: default_upstream_connect_timeout_ms(),
            upstream_read_timeout_ms: default_upstream_read_timeout_ms(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListenerConfig {
    pub name: String,
    pub address: String,
    pub protocol: ProtocolConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolConfig {
    Http1,
    Http2,
}

impl From<ProtocolConfig> for Protocol {
    fn from(value: ProtocolConfig) -> Self {
        match value {
            ProtocolConfig::Http1 => Protocol::Http1,
            ProtocolConfig::Http2 => Protocol::Http2,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RouteConfig {
    pub name: String,
    pub listener: String,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub methods: Vec<HttpMethodConfig>,
    pub upstream: String,
    #[serde(default)]
    pub filters: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum HttpMethodConfig {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

impl From<HttpMethodConfig> for HttpMethod {
    fn from(value: HttpMethodConfig) -> Self {
        match value {
            HttpMethodConfig::GET => HttpMethod::Get,
            HttpMethodConfig::POST => HttpMethod::Post,
            HttpMethodConfig::PUT => HttpMethod::Put,
            HttpMethodConfig::PATCH => HttpMethod::Patch,
            HttpMethodConfig::DELETE => HttpMethod::Delete,
            HttpMethodConfig::HEAD => HttpMethod::Head,
            HttpMethodConfig::OPTIONS => HttpMethod::Options,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpstreamConfig {
    pub name: String,
    #[serde(default)]
    pub load_balance: LoadBalanceConfig,
    pub endpoints: Vec<EndpointConfig>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceConfig {
    #[default]
    RoundRobin,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EndpointConfig {
    pub address: String,
    #[serde(default = "default_weight")]
    pub weight: u16,
}

fn default_weight() -> u16 {
    1
}

fn default_worker_threads() -> usize {
    4
}

fn default_graceful_shutdown_secs() -> u64 {
    30
}

fn default_downstream_read_timeout_ms() -> u64 {
    5_000
}

fn default_upstream_connect_timeout_ms() -> u64 {
    3_000
}

fn default_upstream_read_timeout_ms() -> u64 {
    5_000
}
