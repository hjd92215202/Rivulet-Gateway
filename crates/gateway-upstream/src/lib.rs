//! upstream 层负责管理后端节点集合和它们的健康状态。
//! 这里的目标不是做复杂调度，而是先把“选谁”和“谁健康”这两件事稳住。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use gateway_config::{GatewayConfigFile, HealthCheckConfig, LoadBalanceConfig};
use gateway_types::{GatewayError, Result, UpstreamEndpoint};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone, Debug)]
pub struct UpstreamRegistry {
    /// 以集群名索引所有 upstream。
    clusters: Arc<HashMap<String, Arc<UpstreamCluster>>>,
}

impl UpstreamRegistry {
    pub fn from_config(config: &GatewayConfigFile) -> Self {
        let clusters = config
            .upstreams
            .iter()
            .map(|cluster| {
                // 每个 endpoint 都有独立状态，这样主动探活和被动失败感知可以共享同一个状态源。
                let endpoints = cluster
                    .endpoints
                    .iter()
                    .map(|endpoint| {
                        Arc::new(EndpointState::new(UpstreamEndpoint {
                            address: endpoint.address.clone(),
                            weight: endpoint.weight,
                        }))
                    })
                    .collect();

                (
                    // 注册表按名字挂集群，方便路由命中后 O(1) 查找。
                    cluster.name.clone(),
                    Arc::new(UpstreamCluster {
                        name: cluster.name.clone(),
                        strategy: cluster.load_balance,
                        health_check: cluster.health_check.clone(),
                        endpoints,
                        cursor: AtomicUsize::new(0),
                    }),
                )
            })
            .collect();

        Self {
            clusters: Arc::new(clusters),
        }
    }

    pub fn cluster(&self, name: &str) -> Result<&Arc<UpstreamCluster>> {
        // 找不到集群直接返回错误，避免静默回落到错误目标。
        self.clusters
            .get(name)
            .ok_or_else(|| GatewayError::NotFound(format!("upstream cluster {}", name)))
    }

    pub fn clusters(&self) -> impl Iterator<Item = &Arc<UpstreamCluster>> {
        // 健康检查循环会遍历这里的只读视图。
        self.clusters.values()
    }
}

/// 集群内部只维护节点选择与健康状态，不直接掺杂连接池等更重的职责。
#[derive(Debug)]
pub struct UpstreamCluster {
    /// 集群逻辑名。
    pub name: String,
    /// 当前采用的负载均衡策略。
    pub strategy: LoadBalanceConfig,
    /// 可选健康检查配置。
    pub health_check: Option<HealthCheckConfig>,
    /// 这个集群下的所有 endpoint 状态。
    endpoints: Vec<Arc<EndpointState>>,
    /// 轮询游标。
    cursor: AtomicUsize,
}

impl UpstreamCluster {
    pub fn next_endpoint(&self) -> Result<UpstreamEndpoint> {
        // 纯选择场景直接复用更通用的排除式选择逻辑。
        Ok(self.select_endpoint(&[])?.endpoint().clone())
    }

    /// 重试时会传入已经失败过的地址列表，尽量避免同一次请求反复撞同一个坏节点。
    pub fn select_endpoint(&self, excluded_addresses: &[String]) -> Result<Arc<EndpointState>> {
        let healthy: Vec<_> = self
            .endpoints
            .iter()
            // 只有健康节点才能参与调度。
            .filter(|endpoint| endpoint.is_healthy())
            // 已经在本次请求里失败过的地址不再重复尝试。
            .filter(|endpoint| {
                !excluded_addresses
                    .iter()
                    .any(|item| item == &endpoint.endpoint.address)
            })
            .collect();

        if healthy.is_empty() {
            // 没有健康节点时直接返回，避免继续把流量打到坏节点上。
            return Err(GatewayError::NoHealthyUpstream(self.name.clone()));
        }

        let index = match self.strategy {
            LoadBalanceConfig::RoundRobin => {
                // relaxed 足够，因为这里不依赖严格全局顺序，只需要近似轮询。
                self.cursor.fetch_add(1, Ordering::Relaxed) % healthy.len()
            }
        };

        Ok(Arc::clone(healthy[index]))
    }

    pub fn endpoints(&self) -> &[Arc<EndpointState>] {
        // 主要给健康检查循环只读遍历使用。
        &self.endpoints
    }

    pub fn passive_success_threshold(&self) -> u32 {
        // 没有健康检查配置时，成功一次就恢复健康，保持最小可用策略。
        self.health_check
            .as_ref()
            .map(|config| config.healthy_threshold)
            .unwrap_or(1)
    }

    pub fn passive_failure_threshold(&self) -> u32 {
        // 没有健康检查配置时，不主动把节点永久摘掉，避免误伤唯一节点。
        self.health_check
            .as_ref()
            .map(|config| config.unhealthy_threshold)
            .unwrap_or(u32::MAX)
    }
}

/// `EndpointState` 是主动健康检查和被动失败感知共享的状态容器。
/// 这里用原子计数即可满足当前读多写少的场景，不急着引入更复杂的并发结构。
#[derive(Debug)]
pub struct EndpointState {
    /// 节点的静态描述。
    endpoint: UpstreamEndpoint,
    /// 当前是否健康。
    healthy: AtomicBool,
    /// 连续成功计数。
    consecutive_successes: AtomicU32,
    /// 连续失败计数。
    consecutive_failures: AtomicU32,
}

impl EndpointState {
    fn new(endpoint: UpstreamEndpoint) -> Self {
        Self {
            // 默认先认为节点健康，让初始流量能打进去。
            endpoint,
            healthy: AtomicBool::new(true),
            consecutive_successes: AtomicU32::new(0),
            consecutive_failures: AtomicU32::new(0),
        }
    }

    pub fn endpoint(&self) -> &UpstreamEndpoint {
        // 暴露只读 endpoint，避免状态被外部误改。
        &self.endpoint
    }

    pub fn is_healthy(&self) -> bool {
        // 健康位频繁读取，直接用原子布尔保持成本最低。
        self.healthy.load(Ordering::Relaxed)
    }

    /// 成功会清空失败计数，并在达到阈值后把节点恢复成健康状态。
    pub fn record_success(&self, healthy_threshold: u32) {
        // 一次成功会清空失败计数，代表节点开始恢复。
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
        if successes >= healthy_threshold.max(1) {
            // 达到阈值后再恢复健康，避免刚恢复时的抖动误判。
            self.healthy.store(true, Ordering::Relaxed);
        }
    }

    /// 失败会清空成功计数，并在达到阈值后摘除节点。
    pub fn record_failure(&self, unhealthy_threshold: u32) {
        // 一次失败会清空成功计数，代表恢复过程被打断。
        self.consecutive_successes.store(0, Ordering::Relaxed);
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= unhealthy_threshold.max(1) {
            // 连续失败达到阈值后才摘除，避免偶发瞬时网络毛刺。
            self.healthy.store(false, Ordering::Relaxed);
        }
    }
}

/// 第一阶段的主动探活先做 TCP connect，
/// 它不够精细，但能以很低复杂度覆盖“端口是否可连”这个核心问题。
pub async fn probe_endpoint(endpoint: &EndpointState, config: &HealthCheckConfig) -> bool {
    // 第一阶段只以 TCP connect 成功与否作为健康判定。
    let result = timeout(
        Duration::from_millis(config.timeout_ms),
        TcpStream::connect(&endpoint.endpoint.address),
    )
    .await;

    match result {
        Ok(Ok(_)) => {
            // 连上就算成功，让状态机自己决定是否恢复健康。
            endpoint.record_success(config.healthy_threshold);
            true
        }
        Ok(Err(_)) | Err(_) => {
            // 连接失败或超时都按失败处理。
            endpoint.record_failure(config.unhealthy_threshold);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_config::{
        EndpointConfig, GatewayConfigFile, ListenerConfig, ProtocolConfig, RuntimeConfig,
        UpstreamConfig,
    };
    use tokio::net::TcpListener;

    #[test]
    fn round_robin_moves_between_endpoints() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "0.0.0.0:8080".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                endpoints: vec![
                    EndpointConfig {
                        address: "127.0.0.1:9000".into(),
                        weight: 1,
                    },
                    EndpointConfig {
                        address: "127.0.0.1:9001".into(),
                        weight: 1,
                    },
                ],
            }],
        };

        let registry = UpstreamRegistry::from_config(&config);
        let cluster = registry.cluster("api").expect("cluster exists");

        let first = cluster.next_endpoint().expect("first endpoint");
        let second = cluster.next_endpoint().expect("second endpoint");

        assert_ne!(first.address, second.address);
    }

    #[test]
    fn unhealthy_endpoint_is_skipped_by_selection() {
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: "0.0.0.0:8080".into(),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: Some(HealthCheckConfig {
                    interval_ms: 1000,
                    timeout_ms: 100,
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                endpoints: vec![
                    EndpointConfig {
                        address: "127.0.0.1:9000".into(),
                        weight: 1,
                    },
                    EndpointConfig {
                        address: "127.0.0.1:9001".into(),
                        weight: 1,
                    },
                ],
            }],
        };

        let registry = UpstreamRegistry::from_config(&config);
        let cluster = registry.cluster("api").expect("cluster exists");

        cluster.endpoints()[0].record_failure(1);
        let selected = cluster.next_endpoint().expect("healthy endpoint");

        assert_eq!(selected.address, "127.0.0.1:9001");
    }

    #[tokio::test]
    async fn tcp_probe_marks_endpoint_unhealthy_then_healthy() {
        let reserved = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let addr = reserved.local_addr().expect("listener addr");
        drop(reserved);

        let endpoint = EndpointState::new(UpstreamEndpoint {
            address: addr.to_string(),
            weight: 1,
        });
        let config = HealthCheckConfig {
            interval_ms: 100,
            timeout_ms: 100,
            healthy_threshold: 1,
            unhealthy_threshold: 1,
        };

        let first = probe_endpoint(&endpoint, &config).await;
        assert!(!first);
        assert!(!endpoint.is_healthy());

        let listener = TcpListener::bind(addr).await.expect("rebind health");
        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let second = probe_endpoint(&endpoint, &config).await;
        assert!(second);
        assert!(endpoint.is_healthy());

        accept_task.abort();
    }

    #[test]
    fn passive_thresholds_delay_state_changes() {
        let endpoint = EndpointState::new(UpstreamEndpoint {
            address: "127.0.0.1:9000".into(),
            weight: 1,
        });

        endpoint.record_failure(2);
        assert!(endpoint.is_healthy());

        endpoint.record_failure(2);
        assert!(!endpoint.is_healthy());

        endpoint.record_success(2);
        assert!(!endpoint.is_healthy());

        endpoint.record_success(2);
        assert!(endpoint.is_healthy());
    }
}
