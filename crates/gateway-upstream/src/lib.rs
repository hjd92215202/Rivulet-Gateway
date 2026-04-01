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
    clusters: Arc<HashMap<String, Arc<UpstreamCluster>>>,
}

impl UpstreamRegistry {
    pub fn from_config(config: &GatewayConfigFile) -> Self {
        let clusters = config
            .upstreams
            .iter()
            .map(|cluster| {
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
        self.clusters
            .get(name)
            .ok_or_else(|| GatewayError::NotFound(format!("upstream cluster {}", name)))
    }

    pub fn clusters(&self) -> impl Iterator<Item = &Arc<UpstreamCluster>> {
        self.clusters.values()
    }
}

#[derive(Debug)]
pub struct UpstreamCluster {
    pub name: String,
    pub strategy: LoadBalanceConfig,
    pub health_check: Option<HealthCheckConfig>,
    endpoints: Vec<Arc<EndpointState>>,
    cursor: AtomicUsize,
}

impl UpstreamCluster {
    pub fn next_endpoint(&self) -> Result<UpstreamEndpoint> {
        Ok(self.select_endpoint(&[])?.endpoint().clone())
    }

    pub fn select_endpoint(&self, excluded_addresses: &[String]) -> Result<Arc<EndpointState>> {
        let healthy: Vec<_> = self
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.is_healthy())
            .filter(|endpoint| {
                !excluded_addresses
                    .iter()
                    .any(|item| item == &endpoint.endpoint.address)
            })
            .collect();

        if healthy.is_empty() {
            return Err(GatewayError::NoHealthyUpstream(self.name.clone()));
        }

        let index = match self.strategy {
            LoadBalanceConfig::RoundRobin => {
                self.cursor.fetch_add(1, Ordering::Relaxed) % healthy.len()
            }
        };

        Ok(Arc::clone(healthy[index]))
    }

    pub fn endpoints(&self) -> &[Arc<EndpointState>] {
        &self.endpoints
    }

    pub fn passive_success_threshold(&self) -> u32 {
        self.health_check
            .as_ref()
            .map(|config| config.healthy_threshold)
            .unwrap_or(1)
    }

    pub fn passive_failure_threshold(&self) -> u32 {
        self.health_check
            .as_ref()
            .map(|config| config.unhealthy_threshold)
            .unwrap_or(u32::MAX)
    }
}

#[derive(Debug)]
pub struct EndpointState {
    endpoint: UpstreamEndpoint,
    healthy: AtomicBool,
    consecutive_successes: AtomicU32,
    consecutive_failures: AtomicU32,
}

impl EndpointState {
    fn new(endpoint: UpstreamEndpoint) -> Self {
        Self {
            endpoint,
            healthy: AtomicBool::new(true),
            consecutive_successes: AtomicU32::new(0),
            consecutive_failures: AtomicU32::new(0),
        }
    }

    pub fn endpoint(&self) -> &UpstreamEndpoint {
        &self.endpoint
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn record_success(&self, healthy_threshold: u32) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
        if successes >= healthy_threshold.max(1) {
            self.healthy.store(true, Ordering::Relaxed);
        }
    }

    pub fn record_failure(&self, unhealthy_threshold: u32) {
        self.consecutive_successes.store(0, Ordering::Relaxed);
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= unhealthy_threshold.max(1) {
            self.healthy.store(false, Ordering::Relaxed);
        }
    }
}

pub async fn probe_endpoint(endpoint: &EndpointState, config: &HealthCheckConfig) -> bool {
    let result = timeout(
        Duration::from_millis(config.timeout_ms),
        TcpStream::connect(&endpoint.endpoint.address),
    )
    .await;

    match result {
        Ok(Ok(_)) => {
            endpoint.record_success(config.healthy_threshold);
            true
        }
        Ok(Err(_)) | Err(_) => {
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
