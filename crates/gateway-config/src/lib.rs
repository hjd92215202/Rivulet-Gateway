//! 配置层负责把外部文本配置收敛成强类型结构。
//! 第一阶段暂时只支持 TOML，并且在加载时尽量把明显错误提前暴露出来。

use std::fs;
use std::path::Path;
use std::time::Duration;

use gateway_types::{GatewayError, HttpMethod, Protocol, Result, RuntimeSettings};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct GatewayConfigFile {
    /// 运行时相关的全局配置。
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// 暴露给客户端的入口集合。
    #[serde(default)]
    pub listeners: Vec<ListenerConfig>,
    /// 请求匹配规则集合。
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    /// 后端节点集合定义。
    #[serde(default)]
    pub upstreams: Vec<UpstreamConfig>,
}

impl GatewayConfigFile {
    /// 从磁盘加载配置并立即做结构校验。
    /// 这样启动阶段失败得更早，避免把配置问题拖到请求路径里。
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // 这里直接按整个文件读取，第一阶段配置规模还不值得做流式解析。
        let raw = fs::read_to_string(path)
            .map_err(|err| GatewayError::Io(format!("{}: {}", path.display(), err)))?;
        // TOML 解析失败时直接包装成配置错误，统一上抛给启动阶段。
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
            // listener 名字不能为空，否则控制面和日志都无法稳定引用它。
            if listener.name.trim().is_empty() {
                return Err(GatewayError::InvalidConfig(
                    "listener name must not be empty".into(),
                ));
            }
            // 地址不能为空，否则运行时无法绑定 socket。
            if listener.address.trim().is_empty() {
                return Err(GatewayError::InvalidConfig(format!(
                    "listener {} address must not be empty",
                    listener.name
                )));
            }
        }
        for upstream in &self.upstreams {
            // upstream 至少要有一个节点，否则配置虽然能写出来但永远不可用。
            if upstream.endpoints.is_empty() {
                return Err(GatewayError::InvalidConfig(format!(
                    "upstream {} must contain at least one endpoint",
                    upstream.name
                )));
            }
        }
        for route in &self.routes {
            // 路由引用不存在的 listener 时，说明入口绑定关系写错了。
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
            // 路由引用不存在的 upstream 时，请求最终会没有后端可转发。
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
            // worker 数当前主要用于运行时摘要展示，后面再接更完整的线程模型。
            worker_threads: self.runtime.worker_threads,
            // 配置层用秒表达优雅关闭，更接近运维使用习惯。
            graceful_shutdown: Duration::from_secs(self.runtime.graceful_shutdown_secs),
            // 下游读取超时直接转成 Duration，避免业务层反复转换单位。
            downstream_read_timeout: Duration::from_millis(self.runtime.downstream_read_timeout_ms),
            // 上游连接超时影响失败判定和重试速度。
            upstream_connect_timeout: Duration::from_millis(
                self.runtime.upstream_connect_timeout_ms,
            ),
            // 上游读取超时直接影响代理是否认为节点“卡住了”。
            upstream_read_timeout: Duration::from_millis(self.runtime.upstream_read_timeout_ms),
            // 至少保证有 1 次尝试，避免 0 把主链路推成非法配置。
            upstream_retry_attempts: self.runtime.upstream_retry_attempts.max(1),
            // 请求行长度至少保留一个基础可用下限，避免配置把解析器直接锁死。
            max_request_line_bytes: self.runtime.max_request_line_bytes.max(256),
            // 请求头数量至少允许 1 个，才能容纳 Host 头。
            max_request_headers: self.runtime.max_request_headers.max(1),
            // 请求体大小允许显式配置成 0，表示完全不接受带 body 的请求。
            max_request_body_bytes: self.runtime.max_request_body_bytes,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeConfig {
    /// worker 线程数，先保留为静态配置字段。
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    /// 优雅关闭窗口，单位是秒。
    #[serde(default = "default_graceful_shutdown_secs")]
    pub graceful_shutdown_secs: u64,
    /// 下游读取超时，单位是毫秒。
    #[serde(default = "default_downstream_read_timeout_ms")]
    pub downstream_read_timeout_ms: u64,
    /// 上游建连超时，单位是毫秒。
    #[serde(default = "default_upstream_connect_timeout_ms")]
    pub upstream_connect_timeout_ms: u64,
    /// 上游读响应超时，单位是毫秒。
    #[serde(default = "default_upstream_read_timeout_ms")]
    pub upstream_read_timeout_ms: u64,
    /// 单次请求最多尝试几次 upstream。
    #[serde(default = "default_upstream_retry_attempts")]
    pub upstream_retry_attempts: usize,
    /// 请求行允许的最大字节数。
    #[serde(default = "default_max_request_line_bytes")]
    pub max_request_line_bytes: usize,
    /// 请求头允许的最大数量。
    #[serde(default = "default_max_request_headers")]
    pub max_request_headers: usize,
    /// 请求体允许的最大字节数。
    #[serde(default = "default_max_request_body_bytes")]
    pub max_request_body_bytes: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            // 第一阶段默认值先求稳，不追求极端低延迟。
            worker_threads: default_worker_threads(),
            graceful_shutdown_secs: default_graceful_shutdown_secs(),
            downstream_read_timeout_ms: default_downstream_read_timeout_ms(),
            upstream_connect_timeout_ms: default_upstream_connect_timeout_ms(),
            upstream_read_timeout_ms: default_upstream_read_timeout_ms(),
            upstream_retry_attempts: default_upstream_retry_attempts(),
            max_request_line_bytes: default_max_request_line_bytes(),
            max_request_headers: default_max_request_headers(),
            max_request_body_bytes: default_max_request_body_bytes(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListenerConfig {
    /// listener 的逻辑名称。
    pub name: String,
    /// 绑定地址，例如 `0.0.0.0:8080`。
    pub address: String,
    /// listener 使用的协议类型。
    pub protocol: ProtocolConfig,
}

/// 协议枚举和 `gateway-types::Protocol` 分开定义，
/// 让配置层可以独立处理反序列化细节。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolConfig {
    /// 明文 HTTP/1.1。
    Http1,
    /// 预留给未来的 HTTP/2。
    Http2,
}

impl From<ProtocolConfig> for Protocol {
    fn from(value: ProtocolConfig) -> Self {
        match value {
            // 配置层枚举转成共享类型时不引入额外语义。
            ProtocolConfig::Http1 => Protocol::Http1,
            ProtocolConfig::Http2 => Protocol::Http2,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RouteConfig {
    /// 路由名用于运维定位。
    pub name: String,
    /// 这条路由属于哪个 listener。
    pub listener: String,
    /// host 白名单，空列表表示不限制。
    #[serde(default)]
    pub hosts: Vec<String>,
    /// 路径前缀集合，空列表表示不限制。
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    /// 方法白名单，空列表表示不限制。
    #[serde(default)]
    pub methods: Vec<HttpMethodConfig>,
    /// 命中后转发到哪个 upstream。
    pub upstream: String,
    /// 命中后要执行哪些过滤器。
    #[serde(default)]
    pub filters: Vec<String>,
}

/// 路由上的方法限制默认是“空列表表示不限制”，
/// 这样配置写起来更接近常见网关产品的使用方式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum HttpMethodConfig {
    /// GET 方法。
    GET,
    /// POST 方法。
    POST,
    /// PUT 方法。
    PUT,
    /// PATCH 方法。
    PATCH,
    /// DELETE 方法。
    DELETE,
    /// HEAD 方法。
    HEAD,
    /// OPTIONS 方法。
    OPTIONS,
}

impl From<HttpMethodConfig> for HttpMethod {
    fn from(value: HttpMethodConfig) -> Self {
        match value {
            // 这里只做纯映射，不引入额外兼容逻辑。
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
    /// upstream 集群名。
    pub name: String,
    /// 当前只支持的负载均衡策略。
    #[serde(default)]
    pub load_balance: LoadBalanceConfig,
    /// 可选主动健康检查配置。
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
    /// 节点列表。
    pub endpoints: Vec<EndpointConfig>,
}

/// 主动健康检查先从 TCP 探活开始，先解决“坏节点摘除”的核心问题。
#[derive(Clone, Debug, Deserialize)]
pub struct HealthCheckConfig {
    /// 主动探测间隔。
    #[serde(default = "default_health_check_interval_ms")]
    pub interval_ms: u64,
    /// 单次探测超时。
    #[serde(default = "default_health_check_timeout_ms")]
    pub timeout_ms: u64,
    /// 连续成功多少次后恢复健康。
    #[serde(default = "default_health_check_healthy_threshold")]
    pub healthy_threshold: u32,
    /// 连续失败多少次后判定不健康。
    #[serde(default = "default_health_check_unhealthy_threshold")]
    pub unhealthy_threshold: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceConfig {
    /// 轮询是第一阶段最简单也最稳定的策略。
    #[default]
    RoundRobin,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EndpointConfig {
    /// 节点地址。
    pub address: String,
    /// 节点权重。
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

fn default_max_request_line_bytes() -> usize {
    8 * 1024
}

fn default_max_request_headers() -> usize {
    100
}

fn default_max_request_body_bytes() -> usize {
    1024 * 1024
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
        assert_eq!(loaded.runtime.max_request_line_bytes, 8 * 1024);
        assert_eq!(loaded.runtime.max_request_headers, 100);
        assert_eq!(loaded.runtime.max_request_body_bytes, 1024 * 1024);
        assert_eq!(loaded.upstreams[0].endpoints[0].weight, 1);
    }
}
