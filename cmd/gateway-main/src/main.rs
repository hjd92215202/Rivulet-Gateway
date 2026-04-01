//! 二进制入口只负责装配和生命周期管理。
//! 业务能力尽量收敛到各个 crate 中，避免 main 变成难维护的“大杂烩”。

use std::env;
use std::path::PathBuf;

use gateway_config::GatewayConfigFile;
use gateway_runtime::GatewayApp;

#[tokio::main]
async fn main() {
    let path = config_path();
    let config = match GatewayConfigFile::load_from_file(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config {}: {}", path.display(), error);
            std::process::exit(1);
        }
    };

    let app = GatewayApp::from_config(config);
    let summary = app.summary();

    println!("gateway bootstrap complete");
    println!("listeners: {}", summary.listeners);
    println!("routes: {}", summary.routes);
    println!("upstreams: {}", summary.upstreams);
    println!("worker_threads: {}", summary.worker_threads);

    if let Err(error) = app.run_until(wait_for_shutdown()).await {
        eprintln!("gateway stopped with error: {}", error);
        std::process::exit(1);
    }
}

fn config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config/gateway.toml"))
}

async fn wait_for_shutdown() {
    // 第一版先用 ctrl-c 作为统一停止信号，后面再扩展成更完整的优雅下线流程。
    match tokio::signal::ctrl_c().await {
        Ok(()) => {}
        Err(error) => {
            eprintln!("failed to listen for shutdown signal: {}", error);
        }
    }
}
