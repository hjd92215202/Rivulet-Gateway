//! runtime 层负责把配置、代理和后台任务真正装配起来。
//! 它不关心某次请求具体怎么转发，只关心“系统如何活起来并稳定运行”。

use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gateway_admin::{
    AdminListener, AdminOverview, AdminOverviewProvider, AdminReloadHandler, AdminReloadResult,
    AdminRoute, AdminRuntime, AdminService, AdminStats, AdminSummary, AdminUpstream,
};
use gateway_config::{GatewayConfigFile, ListenerTlsConfig, ProtocolConfig, TlsMinVersionConfig};
use gateway_observability::{AccessLogRecord, RuntimeStats, RuntimeStatsSnapshot, emit_access_log};
use gateway_proxy::{ProxyService, is_graceful_downstream_close, status_code_for_error};
use gateway_router::Router;
use gateway_types::{
    GatewayError, RequestContext, ResponseContext, Result, RuntimeSettings, Shared,
};
use gateway_upstream::{UpstreamRegistry, probe_endpoint};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

#[derive(Clone)]
struct ListenerRuntimeInfo {
    tls_acceptor: Option<TlsAcceptor>,
}

impl ListenerRuntimeInfo {
    fn tls_enabled(&self) -> bool {
        self.tls_acceptor.is_some()
    }
}

struct GatewayEpoch {
    config: GatewayConfigFile,
    runtime_settings: RuntimeSettings,
    proxy: ProxyService,
    upstreams: UpstreamRegistry,
    listener_runtime: HashMap<String, ListenerRuntimeInfo>,
}

impl GatewayEpoch {
    fn listener_runtime(&self, listener_name: &str) -> ListenerRuntimeInfo {
        self.listener_runtime
            .get(listener_name)
            .cloned()
            .unwrap_or(ListenerRuntimeInfo { tls_acceptor: None })
    }
}

struct ReloadState {
    epoch: Arc<GatewayEpoch>,
    config_version: u64,
    last_reload_result: String,
    last_reload_at: Option<String>,
}

#[derive(Clone)]
struct ReloadRuntimeSnapshot {
    config_version: u64,
    last_reload_result: String,
    last_reload_at: Option<String>,
}

struct ReloadCoordinator {
    config_path: Option<PathBuf>,
    state: RwLock<ReloadState>,
    admin_service: RwLock<Option<AdminService>>,
}

pub struct GatewayApp {
    reload: Arc<ReloadCoordinator>,
    /// 全局运行时指标。
    stats: Shared<RuntimeStats>,
}

impl ReloadCoordinator {
    fn new(initial_epoch: GatewayEpoch, config_path: Option<PathBuf>) -> Self {
        Self {
            config_path,
            state: RwLock::new(ReloadState {
                epoch: Arc::new(initial_epoch),
                config_version: 1,
                last_reload_result: "bootstrap".into(),
                last_reload_at: Some(now_unix_timestamp_string()),
            }),
            admin_service: RwLock::new(None),
        }
    }

    fn bind_admin_service(&self, admin: AdminService) {
        *self
            .admin_service
            .write()
            .expect("reload admin service lock poisoned") = Some(admin);
    }

    fn install_epoch(&self, epoch: GatewayEpoch) {
        self.state
            .write()
            .expect("reload state lock poisoned")
            .epoch = Arc::new(epoch);
    }

    fn current_epoch(&self) -> Arc<GatewayEpoch> {
        self.state
            .read()
            .expect("reload state lock poisoned")
            .epoch
            .clone()
    }

    fn config_version(&self) -> u64 {
        self.state
            .read()
            .expect("reload state lock poisoned")
            .config_version
    }

    fn runtime_snapshot(&self) -> ReloadRuntimeSnapshot {
        let state = self.state.read().expect("reload state lock poisoned");
        ReloadRuntimeSnapshot {
            config_version: state.config_version,
            last_reload_result: state.last_reload_result.clone(),
            last_reload_at: state.last_reload_at.clone(),
        }
    }

    fn reload(&self) -> AdminReloadResult {
        let config_path = match &self.config_path {
            Some(path) => path.clone(),
            None => {
                return self.record_reload_failure(
                    "reload requires a concrete config file path at bootstrap".into(),
                );
            }
        };

        let loaded = match GatewayConfigFile::load_from_file(&config_path) {
            Ok(config) => config,
            Err(error) => return self.record_reload_failure(error.to_string()),
        };
        if let Err(error) = loaded.validate() {
            return self.record_reload_failure(error.to_string());
        }

        let current = self.current_epoch();
        if let Err(error) = ensure_static_listener_shape(&current.config, &loaded) {
            return self.record_reload_failure(error.to_string());
        }

        let admin = self
            .admin_service
            .read()
            .expect("reload admin service lock poisoned")
            .clone();
        let new_epoch = match build_gateway_epoch(loaded, admin) {
            Ok(epoch) => epoch,
            Err(error) => return self.record_reload_failure(error.to_string()),
        };

        let mut state = self.state.write().expect("reload state lock poisoned");
        let from_version = state.config_version;
        state.config_version += 1;
        let to_version = state.config_version;
        state.epoch = Arc::new(new_epoch);
        state.last_reload_result = "success".into();
        state.last_reload_at = Some(now_unix_timestamp_string());
        let timestamp = state
            .last_reload_at
            .clone()
            .unwrap_or_else(|| "unknown".into());
        drop(state);

        emit_reload_event(
            "success",
            from_version,
            to_version,
            "reload applied",
            &timestamp,
        );
        AdminReloadResult {
            accepted: true,
            reload_result: "success".into(),
            config_version: to_version,
            message: "reload applied".into(),
        }
    }

    fn record_reload_failure(&self, message: String) -> AdminReloadResult {
        let mut state = self.state.write().expect("reload state lock poisoned");
        let version = state.config_version;
        state.last_reload_result = "failed".into();
        state.last_reload_at = Some(now_unix_timestamp_string());
        let timestamp = state
            .last_reload_at
            .clone()
            .unwrap_or_else(|| "unknown".into());
        drop(state);

        emit_reload_event("failed", version, version, &message, &timestamp);
        AdminReloadResult {
            accepted: false,
            reload_result: "failed".into(),
            config_version: version,
            message,
        }
    }
}

impl GatewayApp {
    pub fn from_config(config: GatewayConfigFile) -> Self {
        Self::try_from_config(config, None)
            .expect("from_config should succeed for in-memory test bootstrap")
    }

    pub fn try_from_config(
        config: GatewayConfigFile,
        config_path: Option<PathBuf>,
    ) -> Result<Self> {
        let stats = Arc::new(RuntimeStats::default());
        let initial_epoch = build_gateway_epoch(config, None)?;
        let reload = Arc::new(ReloadCoordinator::new(initial_epoch, config_path));

        let provider = Arc::new(RuntimeAdminOverviewProvider {
            reload: Arc::clone(&reload),
            stats: Arc::clone(&stats),
        });
        let reloader = Arc::new(RuntimeAdminReloadHandler {
            reload: Arc::clone(&reload),
        });
        let admin = AdminService::new(provider, Some(reloader));
        reload.bind_admin_service(admin.clone());

        let boot_config = reload.current_epoch().config.clone();
        let epoch_with_admin = build_gateway_epoch(boot_config, Some(admin))?;
        reload.install_epoch(epoch_with_admin);

        Ok(Self { reload, stats })
    }

    pub async fn handle(&self, request: RequestContext) -> Result<ResponseContext> {
        self.stats.record_request_started();
        self.reload.current_epoch().proxy.handle(request).await
    }

    pub fn summary(&self) -> GatewaySummary {
        let epoch = self.reload.current_epoch();
        GatewaySummary {
            listeners: epoch.config.listeners.len(),
            routes: epoch.config.routes.len(),
            upstreams: epoch.config.upstreams.len(),
            worker_threads: epoch.config.runtime.worker_threads,
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

        let health_shutdown = shutdown_rx.clone();
        let health_app = Arc::clone(&shared);
        handles.push(tokio::spawn(async move {
            health_check_supervisor(health_app, health_shutdown).await
        }));

        #[cfg(unix)]
        {
            let signal_shutdown = shutdown_rx.clone();
            let signal_app = Arc::clone(&shared);
            handles.push(tokio::spawn(async move {
                sighup_reload_loop(signal_app, signal_shutdown).await
            }));
        }

        let boot_epoch = shared.reload.current_epoch();
        for listener in boot_epoch.config.listeners.clone() {
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
        let graceful = shared
            .reload
            .current_epoch()
            .runtime_settings
            .graceful_shutdown;
        wait_for_connection_drain(Arc::clone(&shared), graceful).await;

        Ok(())
    }
}

struct RuntimeAdminOverviewProvider {
    reload: Arc<ReloadCoordinator>,
    /// 指标通过原子快照读取，保证管理面只读且不会反向影响主链路。
    stats: Shared<RuntimeStats>,
}

impl AdminOverviewProvider for RuntimeAdminOverviewProvider {
    fn overview(&self) -> AdminOverview {
        let epoch = self.reload.current_epoch();
        let reload = self.reload.runtime_snapshot();
        let stats = self.stats.snapshot();
        let runtime = epoch.config.runtime_settings();

        AdminOverview {
            summary: AdminSummary {
                listeners: epoch.config.listeners.len(),
                routes: epoch.config.routes.len(),
                upstreams: epoch.config.upstreams.len(),
                worker_threads: epoch.config.runtime.worker_threads,
            },
            runtime: AdminRuntime {
                graceful_shutdown_secs: runtime.graceful_shutdown.as_secs(),
                downstream_read_timeout_ms: runtime.downstream_read_timeout.as_millis(),
                downstream_keepalive_idle_timeout_ms: runtime
                    .downstream_keepalive_idle_timeout
                    .as_millis(),
                downstream_keepalive_max_requests: runtime.downstream_keepalive_max_requests,
                upstream_connect_timeout_ms: runtime.upstream_connect_timeout.as_millis(),
                upstream_read_timeout_ms: runtime.upstream_read_timeout.as_millis(),
                upstream_retry_attempts: runtime.upstream_retry_attempts,
                upstream_idle_pool_size: runtime.upstream_idle_pool_size,
                config_version: reload.config_version,
                last_reload_result: reload.last_reload_result,
                last_reload_at: reload.last_reload_at,
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
            listeners: epoch
                .config
                .listeners
                .iter()
                .map(|listener| AdminListener {
                    name: listener.name.clone(),
                    address: listener.address.clone(),
                    protocol: listener.protocol.as_str().to_string(),
                })
                .collect(),
            routes: epoch
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
            upstreams: epoch
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

struct RuntimeAdminReloadHandler {
    reload: Arc<ReloadCoordinator>,
}

impl AdminReloadHandler for RuntimeAdminReloadHandler {
    fn reload(&self) -> AdminReloadResult {
        self.reload.reload()
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

fn build_gateway_epoch(
    config: GatewayConfigFile,
    admin: Option<AdminService>,
) -> Result<GatewayEpoch> {
    let router = Router::from_config(&config);
    let filters = gateway_filters::FilterRegistry::with_defaults();
    let upstreams = UpstreamRegistry::from_config(&config);
    let runtime_settings = config.runtime_settings();
    let proxy = ProxyService::with_admin(
        router,
        filters,
        upstreams.clone(),
        runtime_settings.clone(),
        admin,
    );

    let mut listener_runtime = HashMap::new();
    for listener in &config.listeners {
        let info = ListenerRuntimeInfo {
            tls_acceptor: listener
                .tls
                .as_ref()
                .map(|tls| build_tls_acceptor(&listener.name, tls))
                .transpose()?,
        };
        if listener_runtime
            .insert(listener.name.clone(), info)
            .is_some()
        {
            return Err(GatewayError::InvalidConfig(format!(
                "duplicate listener name {}",
                listener.name
            )));
        }
    }

    Ok(GatewayEpoch {
        config,
        runtime_settings,
        proxy,
        upstreams,
        listener_runtime,
    })
}

fn ensure_static_listener_shape(
    current: &GatewayConfigFile,
    candidate: &GatewayConfigFile,
) -> Result<()> {
    let current_shape: HashMap<&str, (&str, ProtocolConfig)> = current
        .listeners
        .iter()
        .map(|listener| {
            (
                listener.name.as_str(),
                (listener.address.as_str(), listener.protocol),
            )
        })
        .collect();
    let candidate_shape: HashMap<&str, (&str, ProtocolConfig)> = candidate
        .listeners
        .iter()
        .map(|listener| {
            (
                listener.name.as_str(),
                (listener.address.as_str(), listener.protocol),
            )
        })
        .collect();

    if current_shape.len() != candidate_shape.len() {
        return Err(GatewayError::InvalidConfig(
            "listener shape changed and requires restart".into(),
        ));
    }

    for (name, (address, protocol)) in current_shape {
        let Some((candidate_address, candidate_protocol)) = candidate_shape.get(name) else {
            return Err(GatewayError::InvalidConfig(format!(
                "listener {} shape changed and requires restart",
                name
            )));
        };

        if address != *candidate_address || protocol != *candidate_protocol {
            return Err(GatewayError::InvalidConfig(format!(
                "listener {} address/protocol changed and requires restart",
                name
            )));
        }
    }

    Ok(())
}

fn now_unix_timestamp_string() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".into(),
    }
}

fn emit_reload_event(
    reload_result: &str,
    from_version: u64,
    to_version: u64,
    reason: &str,
    timestamp: &str,
) {
    println!(
        "{{\"event\":\"reload\",\"reload_result\":\"{}\",\"from_version\":{},\"to_version\":{},\"reason\":\"{}\",\"at\":\"{}\"}}",
        escape_json(reload_result),
        from_version,
        to_version,
        escape_json(reason),
        escape_json(timestamp),
    );
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn build_tls_acceptor(listener_name: &str, tls: &ListenerTlsConfig) -> Result<TlsAcceptor> {
    // TLS 证书读取放在启动阶段，任何加载失败都必须 fail-fast，避免进程带病上线。
    let cert_file = fs::File::open(&tls.cert_file).map_err(|err| {
        GatewayError::InvalidConfig(format!(
            "listener {} tls cert_file {} open failed: {}",
            listener_name, tls.cert_file, err
        ))
    })?;
    let key_file = fs::File::open(&tls.key_file).map_err(|err| {
        GatewayError::InvalidConfig(format!(
            "listener {} tls key_file {} open failed: {}",
            listener_name, tls.key_file, err
        ))
    })?;

    let mut cert_reader = BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|err| {
            GatewayError::InvalidConfig(format!(
                "listener {} tls cert_file {} parse failed: {}",
                listener_name, tls.cert_file, err
            ))
        })?;
    if certs.is_empty() {
        return Err(GatewayError::InvalidConfig(format!(
            "listener {} tls cert_file {} does not contain certificates",
            listener_name, tls.cert_file
        )));
    }

    let mut key_reader = BufReader::new(key_file);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|err| {
            GatewayError::InvalidConfig(format!(
                "listener {} tls key_file {} parse failed: {}",
                listener_name, tls.key_file, err
            ))
        })?
        .ok_or_else(|| {
            GatewayError::InvalidConfig(format!(
                "listener {} tls key_file {} does not contain a supported private key",
                listener_name, tls.key_file
            ))
        })?;

    let versions = match tls.min_version {
        TlsMinVersionConfig::Tls1_2 => vec![&rustls::version::TLS13, &rustls::version::TLS12],
        TlsMinVersionConfig::Tls1_3 => vec![&rustls::version::TLS13],
    };
    let server_config = rustls::ServerConfig::builder_with_protocol_versions(&versions)
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| {
            GatewayError::InvalidConfig(format!(
                "listener {} tls cert/key configuration is invalid: {}",
                listener_name, err
            ))
        })?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
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
                let (stream, client_addr) = accepted
                    .map_err(|err| GatewayError::Io(format!("accept on {}: {}", listener_address, err)))?;
                let app = Arc::clone(&app);
                let listener_name = listener_name.clone();
                tokio::spawn(async move {
                    app.stats.record_connection_opened();
                    let listener_runtime = app.reload.current_epoch().listener_runtime(&listener_name);
                    let tls_enabled = listener_runtime.tls_enabled();

                    if let Some(tls_acceptor) = listener_runtime.tls_acceptor {
                        // TLS 终止只负责把连接解包为安全的明文流，后续请求处理语义与明文入口完全复用。
                        match tls_acceptor.accept(stream).await {
                            Ok(mut tls_stream) => {
                                handle_client_stream(
                                    app.clone(),
                                    listener_name.clone(),
                                    client_addr,
                                    tls_enabled,
                                    &mut tls_stream,
                                )
                                .await;
                                // 最佳努力发送 TLS close_notify，尽量让对端拿到完整的优雅关闭信号。
                                // 如果对端已经提前断开，这里只做可观测提示，不影响主请求结果。
                                if let Err(err) = tls_stream.shutdown().await {
                                    eprintln!(
                                        "{{\"listener\":\"{}\",\"client\":\"{}\",\"event\":\"tls_close_notify_failed\",\"error\":\"{}\"}}",
                                        listener_name, client_addr, err
                                    );
                                }
                            }
                            Err(err) => {
                                let record = AccessLogRecord::failure(
                                    listener_name.clone(),
                                    None,
                                    400,
                                    0,
                                    0,
                                    format!("tls handshake failed: {}", err),
                                )
                                .with_runtime(
                                    app.reload.config_version(),
                                    true,
                                    Some(listener_name.as_str()),
                                );
                                emit_access_log(&record);
                            }
                        }
                    } else {
                        let mut stream = stream;
                        handle_client_stream(
                            app.clone(),
                            listener_name.clone(),
                            client_addr,
                            false,
                            &mut stream,
                        )
                        .await;
                    }

                    app.stats.record_connection_closed();
                });
            }
        }
    }

    Ok(())
}

async fn handle_client_stream<S>(
    app: Arc<GatewayApp>,
    listener_name: String,
    client_addr: std::net::SocketAddr,
    tls_enabled: bool,
    stream: &mut S,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut handled_requests = 0_usize;

    loop {
        let epoch = app.reload.current_epoch();
        let runtime = epoch.runtime_settings.clone();
        // 限制单连接最大请求数，避免极端长连接占用资源过久。
        if handled_requests >= runtime.downstream_keepalive_max_requests {
            break;
        }
        let started_at = Instant::now();
        let config_version = app.reload.config_version();
        let tls_listener = if tls_enabled {
            Some(listener_name.as_str())
        } else {
            None
        };

        // 单个请求失败不应该把整个 listener 打穿，
        // 所以这里把错误就地转换成 HTTP 响应返回给客户端。
        match epoch
            .proxy
            .handle_connection(&listener_name, stream, client_addr)
            .await
        {
            Ok(completed) => {
                handled_requests += 1;
                app.stats.record_request_started();
                app.stats
                    .record_request_completed(completed.response.status_code);
                app.stats.record_retries(completed.retries);
                let record = AccessLogRecord::success(
                    &completed.request,
                    &completed.response,
                    started_at.elapsed().as_millis(),
                    completed.retries,
                )
                .with_runtime(config_version, tls_enabled, tls_listener);
                emit_access_log(&record);

                if completed.close_downstream {
                    break;
                }
            }
            Err(error) => {
                // keepalive 场景下，空闲超时和对端主动关闭属于正常连接生命周期。
                if is_graceful_downstream_close(&error.error) {
                    break;
                }

                app.stats.record_request_started();
                let status_code = status_code_for_error(&error.error);
                app.stats.record_request_completed(status_code);
                app.stats.record_retries(error.retries);
                let record = AccessLogRecord::failure(
                    listener_name.clone(),
                    error.request.as_ref(),
                    status_code,
                    started_at.elapsed().as_millis(),
                    error.retries,
                    error.error.to_string(),
                )
                .with_runtime(config_version, tls_enabled, tls_listener);
                emit_access_log(&record);

                let response = gateway_proxy::error_response(&error.error);
                let _ = stream.write_all(&response).await;
                let _ = stream.flush().await;
                break;
            }
        }
    }
}

async fn health_check_supervisor(
    app: Arc<GatewayApp>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut next_probe_at = HashMap::<String, Instant>::new();

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                match changed {
                    Ok(_) | Err(_) => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                let now = Instant::now();
                let epoch = app.reload.current_epoch();

                for cluster in epoch.upstreams.clusters() {
                    let Some(config) = cluster.health_check.as_ref() else {
                        continue;
                    };
                    let entry = next_probe_at.entry(cluster.name.clone()).or_insert(now);
                    if *entry > now {
                        continue;
                    }

                    for endpoint in cluster.endpoints() {
                        let _ = probe_endpoint(endpoint.as_ref(), config).await;
                    }
                    *entry = now + Duration::from_millis(config.interval_ms);
                }

                next_probe_at.retain(|cluster_name, _| epoch.upstreams.cluster(cluster_name).is_ok());
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
async fn sighup_reload_loop(
    app: Arc<GatewayApp>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sighup = signal(SignalKind::hangup())
        .map_err(|err| GatewayError::Io(format!("watch sighup: {}", err)))?;

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                match changed {
                    Ok(_) | Err(_) => break,
                }
            }
            received = sighup.recv() => {
                if received.is_some() {
                    let _ = app.reload.reload();
                } else {
                    break;
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
        EndpointConfig, ListenerConfig, ListenerTlsConfig, LoadBalanceConfig, ProtocolConfig,
        RouteConfig, RuntimeConfig, TlsMinVersionConfig, UpstreamConfig,
    };
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, RootCertStore};
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{Duration, sleep, timeout};
    use tokio_rustls::TlsConnector;

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
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
    async fn run_until_returns_501_for_transfer_encoding_request() {
        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 501 Not Implemented"));

        server.await.expect("server task").expect("gateway ok");
    }

    #[tokio::test]
    async fn run_until_returns_501_for_transfer_encoding_with_content_length_request() {
        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                b"POST /upload HTTP/1.1\r\nHost: example.test\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .expect("write request");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");

        let text = String::from_utf8(response).expect("utf-8 response");
        assert!(text.starts_with("HTTP/1.1 501 Not Implemented"));

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
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                sleep(Duration::from_millis(140)).await;
            })
            .await
        });

        // 这里短等片刻，让监听任务进入 accept 循环，但还不要晚到错过关闭前窗口。
        sleep(Duration::from_millis(40)).await;

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
        assert!(started_at.elapsed() >= Duration::from_millis(120));
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
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
        assert!(text.contains("\"worker_threads\":4"));

        server.await.expect("server task").expect("gateway ok");
    }

    #[tokio::test]
    async fn run_until_supports_sequential_requests_on_same_connection() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            for (index, (path, body)) in [("/one", "one"), ("/two", "two")].into_iter().enumerate()
            {
                let mut request = Vec::new();
                let mut temp = [0_u8; 1024];
                loop {
                    let read = stream.read(&mut temp).await.expect("read backend request");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&temp[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8(request).expect("request utf-8");
                assert!(text.starts_with(&format!("GET {} HTTP/1.1", path)));
                let connection_header = if index == 0 { "keep-alive" } else { "close" };

                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n{}",
                            body.len(),
                            connection_header,
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("write backend response");
            }
        });

        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                downstream_keepalive_idle_timeout_ms: 800,
                downstream_keepalive_max_requests: 4,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                sleep(Duration::from_millis(350)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        let mut client = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /one HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write first request");
        let first = read_http_response(&mut client).await;
        let first_text = String::from_utf8(first).expect("first response utf-8");
        assert!(first_text.starts_with("HTTP/1.1 200 OK"));
        assert!(first_text.ends_with("one"));

        client
            .write_all(b"GET /two HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write second request");
        let second = read_http_response(&mut client).await;
        let second_text = String::from_utf8(second).expect("second response utf-8");
        assert!(second_text.starts_with("HTTP/1.1 200 OK"));
        assert!(second_text.ends_with("two"));

        server.await.expect("server task").expect("gateway ok");
    }

    #[tokio::test]
    async fn run_until_closes_connection_after_downstream_keepalive_request_cap() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");

        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let mut request = Vec::new();
            let mut temp = [0_u8; 1024];
            loop {
                let read = stream.read(&mut temp).await.expect("read backend request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&temp[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nok",
                )
                .await
                .expect("write backend response");
        });

        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig {
                downstream_keepalive_idle_timeout_ms: 2_000,
                downstream_keepalive_max_requests: 1,
                ..RuntimeConfig::default()
            },
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: None,
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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
                sleep(Duration::from_millis(300)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        let mut client = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect gateway");
        client
            .write_all(b"GET /one HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write first request");
        let first = read_http_response(&mut client).await;
        let first_text = String::from_utf8(first).expect("first response utf-8");
        assert!(first_text.starts_with("HTTP/1.1 200 OK"));
        assert!(first_text.ends_with("ok"));

        // 命中单连接请求上限后，网关应主动回收连接；后续读取应该看到 EOF。
        let mut tail = [0_u8; 16];
        let closed = timeout(Duration::from_millis(250), client.read(&mut tail))
            .await
            .expect("read should complete")
            .expect("read should not fail");
        assert_eq!(closed, 0);

        server.await.expect("server task").expect("gateway ok");
    }

    #[tokio::test]
    async fn run_until_fails_fast_when_tls_cert_file_missing() {
        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: Some(ListenerTlsConfig {
                    cert_file: "/tmp/rivulet-missing-cert.pem".into(),
                    key_file: "/tmp/rivulet-missing-key.pem".into(),
                    min_version: TlsMinVersionConfig::Tls1_2,
                }),
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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

        let error = match GatewayApp::try_from_config(config, None) {
            Ok(_) => panic!("tls missing file should fail"),
            Err(error) => error,
        };
        match error {
            GatewayError::InvalidConfig(message) => assert!(message.contains("cert_file")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[tokio::test]
    async fn run_until_fails_fast_when_tls_cert_key_mismatch() {
        let (cert_path, _, _) = write_test_tls_materials("tls-mismatch-cert");
        let (_, key_path, _) = write_test_tls_materials("tls-mismatch-key");

        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: Some(ListenerTlsConfig {
                    cert_file: cert_path.to_string_lossy().to_string(),
                    key_file: key_path.to_string_lossy().to_string(),
                    min_version: TlsMinVersionConfig::Tls1_2,
                }),
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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

        let error = match GatewayApp::try_from_config(config, None) {
            Ok(_) => panic!("tls cert/key mismatch should fail"),
            Err(error) => error,
        };
        match error {
            GatewayError::InvalidConfig(message) => assert!(message.contains("cert/key")),
            other => panic!("unexpected error: {:?}", other),
        }

        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[tokio::test]
    async fn run_until_serves_https_traffic_with_built_in_tls() {
        let backend = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind backend");
        let backend_addr = backend.local_addr().expect("backend addr");
        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.expect("accept backend");
            let mut request = Vec::new();
            let mut temp = [0_u8; 1024];
            loop {
                let read = stream.read(&mut temp).await.expect("read backend request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&temp[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write backend response");
        });

        let (cert_path, key_path, cert_der) = write_test_tls_materials("tls-success");
        let listener_port = reserve_port();
        let config = GatewayConfigFile {
            runtime: RuntimeConfig::default(),
            listeners: vec![ListenerConfig {
                name: "edge".into(),
                address: format!("127.0.0.1:{listener_port}"),
                protocol: ProtocolConfig::Http1,
                tls: Some(ListenerTlsConfig {
                    cert_file: cert_path.to_string_lossy().to_string(),
                    key_file: key_path.to_string_lossy().to_string(),
                    min_version: TlsMinVersionConfig::Tls1_2,
                }),
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
                auth: Default::default(),
                rate_limit: Default::default(),
                share: Default::default(),
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

        let mut roots = RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(cert_der))
            .expect("add root cert");
        let client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(client_config));

        let tcp = TcpStream::connect(("127.0.0.1", listener_port))
            .await
            .expect("connect tls listener");
        let server_name = ServerName::try_from("localhost").expect("server name");
        let mut tls_stream = connector
            .connect(server_name, tcp)
            .await
            .expect("tls handshake");

        tls_stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: example.test\r\nContent-Length: 0\r\n\r\n")
            .await
            .expect("write request");
        let response = read_http_response(&mut tls_stream).await;
        let text = String::from_utf8(response).expect("response utf-8");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.ends_with("ok"));

        server.await.expect("server task").expect("gateway ok");
        let _ = fs::remove_file(cert_path);
        let _ = fs::remove_file(key_path);
    }

    #[tokio::test]
    async fn run_until_admin_reload_endpoint_applies_config_and_bumps_version() {
        let listener_port = reserve_port();
        let backend_port = reserve_port();
        let backend_addr = format!("127.0.0.1:{backend_port}");
        let config_path = std::env::temp_dir().join(format!(
            "rivulet-reload-success-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));

        write_reload_test_config(
            &config_path,
            &format!("127.0.0.1:{listener_port}"),
            &backend_addr,
            4000,
        );
        let config = GatewayConfigFile::load_from_file(&config_path).expect("load config");
        let app = GatewayApp::try_from_config(config, Some(config_path.clone()))
            .expect("bootstrap app with reload path");
        let server = tokio::spawn(async move {
            app.run_until(async {
                sleep(Duration::from_millis(450)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        let overview_before = send_admin_request(
            listener_port,
            b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        assert!(overview_before.contains("\"config_version\":1"));
        assert!(overview_before.contains("\"upstream_read_timeout_ms\":4000"));

        write_reload_test_config(
            &config_path,
            &format!("127.0.0.1:{listener_port}"),
            &backend_addr,
            7001,
        );
        let reload_response = send_admin_request(
            listener_port,
            b"POST /__admin/api/reload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        assert!(reload_response.contains("\"accepted\":true"));
        assert!(reload_response.contains("\"reload_result\":\"success\""));
        assert!(reload_response.contains("\"config_version\":2"));

        let overview_after = send_admin_request(
            listener_port,
            b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        assert!(overview_after.contains("\"config_version\":2"));
        assert!(overview_after.contains("\"last_reload_result\":\"success\""));
        assert!(overview_after.contains("\"upstream_read_timeout_ms\":7001"));

        server.await.expect("server task").expect("gateway ok");
        let _ = fs::remove_file(config_path);
    }

    #[tokio::test]
    async fn run_until_admin_reload_rejects_listener_shape_change_and_keeps_old_epoch() {
        let listener_port = reserve_port();
        let backend_port = reserve_port();
        let backend_addr = format!("127.0.0.1:{backend_port}");
        let config_path = std::env::temp_dir().join(format!(
            "rivulet-reload-fail-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));

        write_reload_test_config(
            &config_path,
            &format!("127.0.0.1:{listener_port}"),
            &backend_addr,
            4100,
        );
        let config = GatewayConfigFile::load_from_file(&config_path).expect("load config");
        let app = GatewayApp::try_from_config(config, Some(config_path.clone()))
            .expect("bootstrap app with reload path");
        let server = tokio::spawn(async move {
            app.run_until(async {
                sleep(Duration::from_millis(450)).await;
            })
            .await
        });

        sleep(Duration::from_millis(40)).await;

        write_reload_test_config(
            &config_path,
            &format!("127.0.0.1:{}", listener_port + 1),
            &backend_addr,
            9200,
        );
        let reload_response = send_admin_request(
            listener_port,
            b"POST /__admin/api/reload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        assert!(reload_response.contains("\"accepted\":false"));
        assert!(reload_response.contains("\"reload_result\":\"failed\""));
        assert!(reload_response.contains("\"config_version\":1"));

        let overview_after = send_admin_request(
            listener_port,
            b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        assert!(overview_after.contains("\"config_version\":1"));
        assert!(overview_after.contains("\"last_reload_result\":\"failed\""));
        assert!(overview_after.contains("\"upstream_read_timeout_ms\":4100"));

        server.await.expect("server task").expect("gateway ok");
        let _ = fs::remove_file(config_path);
    }

    fn write_test_tls_materials(prefix: &str) -> (std::path::PathBuf, std::path::PathBuf, Vec<u8>) {
        let rcgen::CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
                .expect("generate test cert");
        let cert_pem = cert.pem();
        let cert_der = cert.der().as_ref().to_vec();
        let key_pem = key_pair.serialize_pem();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let cert_path = std::env::temp_dir().join(format!("{}-{}.crt.pem", prefix, unique));
        let key_path = std::env::temp_dir().join(format!("{}-{}.key.pem", prefix, unique));
        fs::write(&cert_path, cert_pem).expect("write cert");
        fs::write(&key_path, key_pem).expect("write key");
        (cert_path, key_path, cert_der)
    }

    fn write_reload_test_config(
        path: &std::path::Path,
        listener_address: &str,
        backend_address: &str,
        upstream_read_timeout_ms: u64,
    ) {
        let toml = format!(
            concat!(
                "[runtime]\n",
                "worker_threads = 2\n",
                "upstream_read_timeout_ms = {upstream_read_timeout_ms}\n\n",
                "[[listeners]]\n",
                "name = \"edge\"\n",
                "address = \"{listener_address}\"\n",
                "protocol = \"http1\"\n\n",
                "[[upstreams]]\n",
                "name = \"api\"\n",
                "load_balance = \"round_robin\"\n\n",
                "[[upstreams.endpoints]]\n",
                "address = \"{backend_address}\"\n",
                "weight = 1\n\n",
                "[[routes]]\n",
                "name = \"default\"\n",
                "listener = \"edge\"\n",
                "hosts = [\"example.test\"]\n",
                "path_prefixes = [\"/\"]\n",
                "methods = [\"GET\"]\n",
                "upstream = \"api\"\n",
            ),
            upstream_read_timeout_ms = upstream_read_timeout_ms,
            listener_address = listener_address,
            backend_address = backend_address,
        );
        fs::write(path, toml).expect("write reload config");
    }

    async fn send_admin_request(port: u16, request_bytes: &[u8]) -> String {
        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect admin listener");
        client
            .write_all(request_bytes)
            .await
            .expect("write admin request");
        String::from_utf8(read_http_response(&mut client).await).expect("admin response utf-8")
    }

    async fn read_http_response<S>(stream: &mut S) -> Vec<u8>
    where
        S: AsyncRead + Unpin,
    {
        let mut response = Vec::new();
        let mut temp = [0_u8; 1024];
        let header_end = loop {
            let read = stream.read(&mut temp).await.expect("read response");
            assert!(read > 0, "connection closed before headers completed");
            response.extend_from_slice(&temp[..read]);
            if let Some(position) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                break position;
            }
        };

        let header_text = String::from_utf8(response[..header_end].to_vec()).expect("header utf-8");
        let content_length = header_text
            .split("\r\n")
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    if name.eq_ignore_ascii_case("content-length") {
                        Some(
                            value
                                .trim()
                                .parse::<usize>()
                                .expect("content-length should parse"),
                        )
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(0);

        let expected_total = header_end + 4 + content_length;
        while response.len() < expected_total {
            let read = stream.read(&mut temp).await.expect("read response body");
            assert!(read > 0, "connection closed before response body completed");
            response.extend_from_slice(&temp[..read]);
        }

        response.truncate(expected_total);
        response
    }

    fn reserve_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("reserve port")
            .local_addr()
            .expect("local addr")
            .port()
    }
}
