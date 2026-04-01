//! 配置层负责把外部文本配置收敛成强类型结构。
//! 第一阶段暂时只支持 TOML，并且在加载时尽量把明显错误提前暴露出来。

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
    /// 从磁盘加载配置并立即做结构校验。
    /// 这样启动阶段失败得更早，避免把配置问题拖到请求路径里。
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
        // 第一阶段先做“引用完整性”和“最小字段存在性”校验。
        // 更细的语义校验会随着控制面增强逐步补上。
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
        // 这里把外部配置转成运行时快照，并顺手做最小兜底，
        // 避免 0 次重试这类配置把代理主链路直接推入非法状态。
        RuntimeSettings {
            worker_threads: self.runtime.worker_threads,
            graceful_shutdown: Duration::from_secs(self.runtime.graceful_shutdown_secs),
            downstream_read_timeout: Duration::from_millis(self.runtime.downstream_read_timeout_ms),
            upstream_connect_timeout: Duration::from_millis(
                self.runtime.upstream_connect_timeout_ms,
            ),
            upstream_read_timeout: Duration::from_millis(self.runtime.upstream_read_timeout_ms),
            upstream_retry_attempts: self.runtime.upstream_retry_attempts.max(1),
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
    #[serde(default = "default_upstream_retry_attempts")]
    pub upstream_retry_attempts: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: default_worker_threads(),
            graceful_shutdown_secs: default_graceful_shutdown_secs(),
            downstream_read_timeout_ms: default_downstream_read_timeout_ms(),
            upstream_connect_timeout_ms: default_upstream_connect_timeout_ms(),
            upstream_read_timeout_ms: default_upstream_read_timeout_ms(),
            upstream_retry_attempts: default_upstream_retry_attempts(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListenerConfig {
    pub name: String,
    pub address: String,
    pub protocol: ProtocolConfig,
}

/// 协议枚举和 `gateway-types::Protocol` 分开定义，
/// 让配置层可以独立处理反序列化细节。
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

/// 路由上的方法限制默认是“空列表表示不限制”，
/// 这样配置写起来更接近常见网关产品的使用方式。
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
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
    pub endpoints: Vec<EndpointConfig>,
}

/// 主动健康检查先从 TCP 探活开始，先解决“坏节点摘除”的核心问题。
#[derive(Clone, Debug, Deserialize)]
pub struct HealthCheckConfig {
    #[serde(default = "default_health_check_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_health_check_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_health_check_healthy_threshold")]
    pub healthy_threshold: u32,
    #[serde(default = "default_health_check_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
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

fn default_upstream_retry_attempts() -> usize {
    2
}

fn default_health_check_interval_ms() -> u64 {
    3_000
}

fn default_health_check_timeout_ms() -> u64 {
    1_000
}

fn default_health_check_healthy_threshold() -> u32 {
    2
}

fn default_health_check_unhealthy_threshold() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_config() -> GatewayConfigFile {
        GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "127.0.0.1:8080".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![HttpMethodConfig::GET],
                upstream: "api".into(),
                filters: vec!["request-id".into()],
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: Some(HealthCheckConfig {
                    interval_ms: 3_000,
                    timeout_ms: 1_000,
                    healthy_threshold: 2,
                    unhealthy_threshold: 2,
                }),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        }
    }

    #[test]
    fn validate_rejects_missing_listener() {
        let mut config = valid_config();
        config.listeners.clear();

        let error = config.validate().expect_err("config should be invalid");
        match error {
            GatewayError::InvalidConfig(message) => {
                assert!(message.contains("at least one listener"))
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn validate_rejects_unknown_route_upstream() {
        let mut config = valid_config();
        config.routes[0].upstream = "missing".into();

        let error = config.validate().expect_err("config should be invalid");
        match error {
            GatewayError::InvalidConfig(message) => assert!(message.contains("unknown upstream")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn runtime_settings_clamps_retry_attempts() {
        let mut config = valid_config();
        config.runtime.upstream_retry_attempts = 0;

        let settings = config.runtime_settings();
        assert_eq!(settings.upstream_retry_attempts, 1);
    }

    #[test]
    fn load_from_file_applies_defaults() {
        let file_name = format!(
            "gateway-config-test-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(file_name);

        fs::write(
            &path,
            r#"
[[listeners]]
name = "edge"
address = "127.0.0.1:8080"
protocol = "http1"

[[upstreams]]
name = "api"
load_balance = "round_robin"

[[upstreams.endpoints]]
address = "127.0.0.1:9000"

[[routes]]
name = "default"
listener = "edge"
upstream = "api"
"#,
        )
        .expect("write config file");

        let loaded = GatewayConfigFile::load_from_file(&path).expect("config should load");
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.runtime.worker_threads, 4);
        assert_eq!(loaded.runtime.upstream_retry_attempts, 2);
        assert_eq!(loaded.upstreams[0].endpoints[0].weight, 1);
    }
}
