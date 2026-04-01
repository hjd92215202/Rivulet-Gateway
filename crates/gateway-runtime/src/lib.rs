use std::sync::Arc;

use gateway_config::GatewayConfigFile;
use gateway_observability::RuntimeStats;
use gateway_proxy::ProxyService;
use gateway_router::Router;
use gateway_types::{GatewayError, RequestContext, ResponseContext, Result, Shared};
use gateway_upstream::UpstreamRegistry;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::watch;

pub struct GatewayApp {
    config: GatewayConfigFile,
    proxy: ProxyService,
    stats: Shared<RuntimeStats>,
}

impl GatewayApp {
    pub fn from_config(config: GatewayConfigFile) -> Self {
        let router = Router::from_config(&config);
        let filters = gateway_filters::FilterRegistry::with_defaults();
        let upstreams = UpstreamRegistry::from_config(&config);

        Self {
            config,
            proxy: ProxyService::new(router, filters, upstreams),
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
