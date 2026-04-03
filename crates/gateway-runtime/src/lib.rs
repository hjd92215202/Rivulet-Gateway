//! runtime 层负责把配置、代理和后台任务真正装配起来。
//! 它不关心某次请求具体怎么转发，只关心“系统如何活起来并稳定运行”。

use std::sync::Arc;
use std::time::{Duration, Instant};

use gateway_admin::{
    AdminListener, AdminOverview, AdminOverviewProvider, AdminRoute, AdminRuntime, AdminService,
    AdminStats, AdminSummary, AdminUpstream,
};
use gateway_config::GatewayConfigFile;
use gateway_observability::{AccessLogRecord, RuntimeStats, RuntimeStatsSnapshot, emit_access_log};
use gateway_proxy::{ProxyService, status_code_for_error};
use gateway_router::Router;
use gateway_types::{GatewayError, RequestContext, ResponseContext, Result, Shared};
use gateway_upstream::{UpstreamRegistry, probe_endpoint};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::watch;

pub struct GatewayApp {
    /// 原始配置快照，供摘要输出和后台任务读取。
    config: GatewayConfigFile,
    /// 负责真正处理请求转发的代理服务。
    proxy: ProxyService,
    /// 所有 upstream 集群及其运行时状态。
    upstreams: UpstreamRegistry,
    /// 全局运行时指标。
    stats: Shared<RuntimeStats>,
}

impl GatewayApp {
    pub fn from_config(config: GatewayConfigFile) -> Self {
        // 配置在这里一次性装配成运行时对象，避免主逻辑里反复解析和构造。
        let router = Router::from_config(&config);
        let filters = gateway_filters::FilterRegistry::with_defaults();
        let upstreams = UpstreamRegistry::from_config(&config);
        let runtime_settings = config.runtime_settings();
        let stats = Arc::new(RuntimeStats::default());
        let admin = AdminService::new(Arc::new(RuntimeAdminOverviewProvider {
            config: config.clone(),
            stats: Arc::clone(&stats),
        }));

        Self {
            config,
            proxy: ProxyService::with_admin(
                router,
                filters,
                upstreams.clone(),
                runtime_settings,
                Some(admin),
            ),
            upstreams,
            stats,
        }
    }

    pub async fn handle(&self, request: RequestContext) -> Result<ResponseContext> {
        self.stats.record_request_started();
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

    pub fn stats_snapshot(&self) -> RuntimeStatsSnapshot {
        // 暴露快照而不是原始原子字段，避免外部误改内部状态。
        self.stats.snapshot()
    }

    pub async fn run_until<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        // runtime 会同时托管 listener 和后台健康检查任务，
        // 统一由一个 shutdown 信号驱动退出。
        let shared = Arc::new(self);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut handles = Vec::new();

        for cluster in shared.upstreams.clusters() {
            if cluster.health_check.is_some() {
                // 每个启用主动探活的集群独立起一个后台任务。
                let app = Arc::clone(&shared);
                let cluster_name = cluster.name.clone();
                let cluster_shutdown = shutdown_rx.clone();
                handles.push(tokio::spawn(async move {
                    health_check_loop(app, cluster_name, cluster_shutdown).await
                }));
            }
        }

        for listener in shared.config.listeners.clone() {
            // listener 在启动阶段先完成 bind，尽早发现端口冲突等问题。
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
        // 所有后台任务统一消费这个停止信号。
        let _ = shutdown_tx.send(true);

        for handle in handles {
            // 等待 listener 和健康检查循环停止，确保不会再产生新工作。
            handle
                .await
                .map_err(|err| GatewayError::Io(format!("listener task join error: {}", err)))??;
        }

        // 后台任务停掉后，再等待在途连接自然排空。
        wait_for_connection_drain(
            Arc::clone(&shared),
            shared.config.runtime_settings().graceful_shutdown,
        )
        .await;

        Ok(())
    }
}

struct RuntimeAdminOverviewProvider {
    /// 管理面读取的是启动时生效的配置快照，而不是外部可变状态。
    config: GatewayConfigFile,
    /// 指标通过原子快照读取，保证管理面只读且不会反向影响主链路。
    stats: Shared<RuntimeStats>,
}

impl AdminOverviewProvider for RuntimeAdminOverviewProvider {
    fn overview(&self) -> AdminOverview {
        let stats = self.stats.snapshot();
        let runtime = self.config.runtime_settings();

        AdminOverview {
            summary: AdminSummary {
                listeners: self.config.listeners.len(),
                routes: self.config.routes.len(),
                upstreams: self.config.upstreams.len(),
                worker_threads: self.config.runtime.worker_threads,
            },
            runtime: AdminRuntime {
                graceful_shutdown_secs: runtime.graceful_shutdown.as_secs(),
                downstream_read_timeout_ms: runtime.downstream_read_timeout.as_millis(),
                upstream_connect_timeout_ms: runtime.upstream_connect_timeout.as_millis(),
                upstream_read_timeout_ms: runtime.upstream_read_timeout.as_millis(),
                upstream_retry_attempts: runtime.upstream_retry_attempts,
                upstream_idle_pool_size: runtime.upstream_idle_pool_size,
            },
            stats: AdminStats {
                total_requests: stats.total_requests,
                completed_requests: stats.completed_requests,
                active_connections: stats.active_connections,
                successful_responses: stats.successful_responses,
                client_error_responses: stats.client_error_responses,
                server_error_responses: stats.server_error_responses,
                upstream_retries: stats.upstream_retries,
            },
            listeners: self
                .config
                .listeners
                .iter()
                .map(|listener| AdminListener {
                    name: listener.name.clone(),
                    address: listener.address.clone(),
                    protocol: listener.protocol.as_str().to_string(),
                })
                .collect(),
            routes: self
                .config
                .routes
                .iter()
                .map(|route| AdminRoute {
                    name: route.name.clone(),
                    listener: route.listener.clone(),
                    hosts: route.hosts.clone(),
                    path_prefixes: route.path_prefixes.clone(),
                    methods: route.methods.iter().map(ToString::to_string).collect(),
                    upstream: route.upstream.clone(),
                })
                .collect(),
            upstreams: self
                .config
                .upstreams
                .iter()
                .map(|upstream| AdminUpstream {
                    name: upstream.name.clone(),
                    load_balance: upstream.load_balance.as_str().to_string(),
                    endpoints: upstream
                        .endpoints
                        .iter()
                        .map(|endpoint| format!("{} (w={})", endpoint.address, endpoint.weight))
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct GatewaySummary {
    /// listener 数量。
    pub listeners: usize,
    /// route 数量。
    pub routes: usize,
    /// upstream 数量。
    pub upstreams: usize,
    /// worker 线程数量。
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
                    // 收到停止信号后不再 accept 新连接。
                    Ok(_) | Err(_) => break,
                }
            }
            accepted = listener.accept() => {
                // 每个连接都放到独立任务里，避免相互阻塞。
                let (mut stream, client_addr) = accepted
                    .map_err(|err| GatewayError::Io(format!("accept on {}: {}", listener_address, err)))?;
                let app = Arc::clone(&app);
                let listener_name = listener_name.clone();
                tokio::spawn(async move {
                    // 连接一进来就记录活动连接数和请求开始时间。
                    app.stats.record_connection_opened();
                    app.stats.record_request_started();
                    let started_at = Instant::now();

                    // 单个连接失败不应该把整个 listener 打崩，
                    // 所以这里把错误就地转换成 HTTP 响应返回给客户端。
                    match app.proxy.handle_connection(&listener_name, &mut stream, client_addr).await {
                        Ok(completed) => {
                            // 成功路径记录状态码、重试次数和 access log。
                            app.stats.record_request_completed(completed.response.status_code);
                            app.stats.record_retries(completed.retries);
                            emit_access_log(&AccessLogRecord::success(
                                &completed.request,
                                &completed.response,
                                started_at.elapsed().as_millis(),
                                completed.retries,
                            ));
                        }
                        Err(error) => {
                            // 失败路径同样要打点和输出 access log，避免观测黑洞。
                            let status_code = status_code_for_error(&error.error);
                            app.stats.record_request_completed(status_code);
                            app.stats.record_retries(error.retries);
                            emit_access_log(&AccessLogRecord::failure(
                                listener_name.clone(),
                                error.request.as_ref(),
                                status_code,
                                started_at.elapsed().as_millis(),
                                error.retries,
                                error.error.to_string(),
                            ));

                            let response = gateway_proxy::error_response(&error.error);
                            let _ = stream.write_all(&response).await;
                            let _ = stream.flush().await;
                        }
                    }

                    // 连接任务结束时统一归还活动连接计数。
                    app.stats.record_connection_closed();
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
    // 先拿到集群和健康检查配置，后续循环只读使用。
    let cluster = app.upstreams.cluster(&cluster_name)?.clone();
    let config = cluster.health_check.clone().ok_or_else(|| {
        GatewayError::InvalidConfig(format!("cluster {} missing health config", cluster_name))
    })?;
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(config.interval_ms));

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                match changed {
                    // 收到停止信号后退出探测循环。
                    Ok(_) | Err(_) => break,
                }
            }
            _ = ticker.tick() => {
                // 健康检查目前串行执行，先用更简单、可预测的行为换取可维护性。
                for endpoint in cluster.endpoints() {
                    // 单个 endpoint 的探测失败不会影响其他节点继续探测。
                    let _ = probe_endpoint(endpoint.as_ref(), &config).await;
                }
            }
        }
    }

    Ok(())
}

async fn wait_for_connection_drain(app: Arc<GatewayApp>, timeout: Duration) {
    // 这一段逻辑只负责等待已有连接自然结束，不会再接收新连接。
    let started_at = Instant::now();

    while started_at.elapsed() < timeout {
        if app.stats.snapshot().active_connections == 0 {
            // 没有活动连接时可以提前结束等待。
            break;
        }
        // 先用短周期轮询，保持实现和行为都足够直观。
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
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
                policy: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
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
                policy: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
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

    #[tokio::test]
    async fn handle_updates_runtime_stats_snapshot() {
        let config = GatewayConfigFile {
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
                methods: vec![],
                upstream: "api".into(),
                filters: vec![],
                policy: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: "127.0.0.1:9000".into(),
                    weight: 1,
                }],
            }],
        };

        let app = GatewayApp::from_config(config);
        let before = app.stats_snapshot();
        assert_eq!(before.total_requests, 0);

        let response = app
            .handle(RequestContext::new(
                "edge",
                "example.test",
                "/health",
                gateway_types::HttpMethod::Get,
            ))
            .await
            .expect("handle should succeed");

        assert_eq!(response.status_code, 200);

        let after = app.stats_snapshot();
        assert_eq!(after.total_requests, 1);
    }

    #[tokio::test]
    async fn run_until_waits_for_inflight_connection_to_finish() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            sleep(Duration::from_millis(120)).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                graceful_shutdown_secs: 1,
                ..RuntimeConfig::default()
            },
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
                policy: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
                endpoints: vec![EndpointConfig {
                    address: backend_addr.to_string(),
                    weight: 1,
                }],
            }],
        };

        let app = GatewayApp::from_config(config);
        let started_at = Instant::now();
        let server = tokio::spawn(async move {
            app.run_until(async {
                // 先给客户端建立在途连接的时间，再触发关闭流程。
                sleep(Duration::from_millis(80)).await;
            })
            .await
        });

        // 这里短等片刻，让监听任务进入 accept 循环，但还不要晚到错过关闭前窗口。
        sleep(Duration::from_millis(20)).await;

        let mut client = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /wait HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        server.await.expect("server task").expect("gateway ok");
        assert!(started_at.elapsed() >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn run_until_serves_admin_overview_on_loopback() {
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
                policy: Default::default(),
            }],
            upstreams: vec![UpstreamConfig {
                name: "api".into(),
                load_balance: LoadBalanceConfig::RoundRobin,
                health_check: None,
                policy: Default::default(),
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
            .write_all(
                b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.contains("application/json; charset=utf-8"));
        assert!(text.contains("\"listeners\":1"));
        assert!(text.contains("\"routes\":["));

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
