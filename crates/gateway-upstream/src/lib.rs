use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use gateway_config::{GatewayConfigFile, LoadBalanceConfig};
use gateway_types::{GatewayError, Result, UpstreamEndpoint};

#[derive(Debug)]
pub struct UpstreamCluster {
    pub name: String,
    pub strategy: LoadBalanceConfig,
    endpoints: Vec<UpstreamEndpoint>,
    cursor: AtomicUsize,
}

impl UpstreamCluster {
    pub fn next_endpoint(&self) -> Result<UpstreamEndpoint> {
        if self.endpoints.is_empty() {
            return Err(GatewayError::NoHealthyUpstream(self.name.clone()));
        }

        let index = match self.strategy {
            LoadBalanceConfig::RoundRobin => {
                self.cursor.fetch_add(1, Ordering::Relaxed) % self.endpoints.len()
            }
        };

        Ok(self.endpoints[index].clone())
    }
}

#[derive(Debug)]
pub struct UpstreamRegistry {
    clusters: HashMap<String, UpstreamCluster>,
}

impl UpstreamRegistry {
    pub fn from_config(config: &GatewayConfigFile) -> Self {
        let clusters = config
            .upstreams
            .iter()
            .map(|cluster| {
                (
                    cluster.name.clone(),
                    UpstreamCluster {
                        name: cluster.name.clone(),
                        strategy: cluster.load_balance,
                        endpoints: cluster
                            .endpoints
                            .iter()
                            .map(|endpoint| UpstreamEndpoint {
                                address: endpoint.address.clone(),
                                weight: endpoint.weight,
                            })
                            .collect(),
                        cursor: AtomicUsize::new(0),
                    },
                )
            })
            .collect();

        Self { clusters }
    }

    pub fn cluster(&self, name: &str) -> Result<&UpstreamCluster> {
        self.clusters
            .get(name)
            .ok_or_else(|| GatewayError::NotFound(format!("upstream cluster {}", name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_config::{
        EndpointConfig, GatewayConfigFile, ListenerConfig, ProtocolConfig, RuntimeConfig,
        UpstreamConfig,
    };

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
}
