use std::sync::Arc;

use gateway_config::GatewayConfigFile;
use gateway_observability::RuntimeStats;
use gateway_proxy::ProxyService;
use gateway_router::Router;
use gateway_types::{GatewayError, RequestContext, ResponseContext, Result, Shared};
use gateway_upstream::{UpstreamRegistry, probe_endpoint};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::watch;

pub struct GatewayApp {
    config: GatewayConfigFile,
    proxy: ProxyService,
    upstreams: UpstreamRegistry,
    stats: Shared<RuntimeStats>,
}

impl GatewayApp {
    pub fn from_config(config: GatewayConfigFile) -> Self {
        let router = Router::from_config(&config);
        let filters = gateway_filters::FilterRegistry::with_defaults();
        let upstreams = UpstreamRegistry::from_config(&config);
        let runtime_settings = config.runtime_settings();

        Self {
            config,
            proxy: ProxyService::new(router, filters, upstreams.clone(), runtime_settings),
            upstreams,
            stats: Arc::new(RuntimeStats::default()),
        }
    }

    pub async fn handle(&self, request: RequestContext) -> Result<ResponseContext> {
        self.stats.record_request();
        self.proxy.handle(request).await
    }

    pub fn summary(&self) -> GatewaySummary {
        GatewaySummary {
            listeners: self.config.listeners.len(),
            routes: self.config.routes.len(),
            upstreams: self.config.upstreams.len(),
            worker_threads: self.config.runtime.worker_threads,
        }
    }

    pub async fn run_until<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        let shared = Arc::new(self);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut handles = Vec::new();

        for cluster in shared.upstreams.clusters() {
            if cluster.health_check.is_some() {
                let app = Arc::clone(&shared);
                let cluster_name = cluster.name.clone();
                let cluster_shutdown = shutdown_rx.clone();
                handles.push(tokio::spawn(async move {
                    health_check_loop(app, cluster_name, cluster_shutdown).await
                }));
            }
        }

        for listener in shared.config.listeners.clone() {
            let tcp_listener = TcpListener::bind(&listener.address)
                .await
                .map_err(|err| GatewayError::Io(format!("bind {}: {}", listener.address, err)))?;
            let app = Arc::clone(&shared);
            let listener_name = listener.name.clone();
            let listener_address = listener.address.clone();
            let listener_shutdown = shutdown_rx.clone();
            handles.push(tokio::spawn(async move {
                listener_loop(
                    app,
                    tcp_listener,
                    listener_name,
                    listener_address,
                    listener_shutdown,
                )
                .await
            }));
        }

        shutdown.await;
        let _ = shutdown_tx.send(true);

        for handle in handles {
            handle
                .await
                .map_err(|err| GatewayError::Io(format!("listener task join error: {}", err)))??;
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct GatewaySummary {
    pub listeners: usize,
    pub routes: usize,
    pub upstreams: usize,
    pub worker_threads: usize,
}

async fn listener_loop(
    app: Arc<GatewayApp>,
    listener: TcpListener,
    listener_name: String,
    listener_address: String,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                match changed {
                    Ok(_) | Err(_) => break,
                }
            }
            accepted = listener.accept() => {
                let (mut stream, client_addr) = accepted
                    .map_err(|err| GatewayError::Io(format!("accept on {}: {}", listener_address, err)))?;
                let app = Arc::clone(&app);
                let listener_name = listener_name.clone();
                tokio::spawn(async move {
                    if let Err(error) = app.proxy.handle_connection(&listener_name, &mut stream, client_addr).await {
                        let response = gateway_proxy::error_response(&error);
                        let _ = stream.write_all(&response).await;
                        let _ = stream.flush().await;
                    }
                });
            }
        }
    }

    Ok(())
}

async fn health_check_loop(
    app: Arc<GatewayApp>,
    cluster_name: String,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let cluster = app.upstreams.cluster(&cluster_name)?.clone();
    let config = cluster.health_check.clone().ok_or_else(|| {
        GatewayError::InvalidConfig(format!("cluster {} missing health config", cluster_name))
    })?;
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(config.interval_ms));

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                match changed {
                    Ok(_) | Err(_) => break,
                }
            }
            _ = ticker.tick() => {
                for endpoint in cluster.endpoints() {
                    let _ = probe_endpoint(endpoint.as_ref(), &config).await;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_config::{
        EndpointConfig, ListenerConfig, LoadBalanceConfig, ProtocolConfig, RouteConfig,
        RuntimeConfig, UpstreamConfig,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn run_until_serves_proxy_traffic() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .expect("read backend request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&request);
            assert!(text.starts_with("GET /health HTTP/1.1"));

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let app = GatewayApp::from_config(config);
        let server = tokio::spawn(async move {
            app.run_until(async {
                sleep(Duration::from_millis(250)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        let mut client = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.ends_with("ok"));

        server.await.expect("server task").expect("gateway ok");
    }

    #[tokio::test]
    async fn run_until_returns_404_for_unmatched_route() {
        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
            }],
            routes: vec![RouteConfig {
                name: "default".into(),
                listener: "edge".into(),
                hosts: vec!["example.test".into()],
                path_prefixes: vec!["/ok".into()],
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let app = GatewayApp::from_config(config);
        let server = tokio::spawn(async move {
            app.run_until(async {
                sleep(Duration::from_millis(250)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        let mut client = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /missing HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 404 Not Found"));

        server.await.expect("server task").expect("gateway ok");
    }

    fn reserve_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("reserve port")
            .local_addr()
            .expect("local addr")
            .port()
    }
}
