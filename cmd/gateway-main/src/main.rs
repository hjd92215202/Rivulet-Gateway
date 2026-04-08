//! 二进制入口只负责装配和生命周期管理。
//! 业务能力尽量收敛到各个 crate 中，避免 main 变成难维护的“大杂烩”。

use std::env;
use std::path::PathBuf;

use gateway_config::GatewayConfigFile;
use gateway_runtime::GatewayApp;

fn main() {
    let path = config_path();
    let config = match GatewayConfigFile::load_from_file(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config {}: {}", path.display(), error);
            std::process::exit(1);
        }
    };
    let worker_threads = config.runtime.worker_threads;

    let app = match GatewayApp::try_from_config(config, Some(path.clone())) {
        Ok(app) => app,
        Err(error) => {
            eprintln!(
                "failed to bootstrap gateway app {}: {}",
                path.display(),
                error
            );
            std::process::exit(1);
        }
    };
    let summary = app.summary();

    println!("gateway bootstrap complete");
    println!("listeners: {}", summary.listeners);
    println!("routes: {}", summary.routes);
    println!("upstreams: {}", summary.upstreams);
    println!("worker_threads: {}", summary.worker_threads);

    let runtime = match build_tokio_runtime(worker_threads) {
        Ok(runtime) => runtime,
        Err(message) => {
            eprintln!("failed to build tokio runtime: {}", message);
            std::process::exit(1);
        }
    };

    let run_result = runtime.block_on(async move { app.run_until(wait_for_shutdown()).await });
    if let Err(error) = run_result {
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

fn build_tokio_runtime(worker_threads: usize) -> Result<tokio::runtime::Runtime, String> {
    if worker_threads == 0 {
        return Err("runtime.worker_threads must be greater than 0".into());
    }

    // 这里显式构建多线程 runtime，让配置项真正影响调度模型。
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .map_err(|err| format!("tokio runtime build failed: {}", err))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tokio_runtime_rejects_zero_worker_threads() {
        let error = build_tokio_runtime(0).expect_err("zero worker threads should fail");
        assert!(error.contains("worker_threads"));
    }

    #[test]
    fn build_tokio_runtime_accepts_positive_worker_threads() {
        let runtime = build_tokio_runtime(2).expect("runtime should build");
        let value = runtime.block_on(async {
            tokio::spawn(async { 1 + 1 })
                .await
                .expect("spawn should join")
        });
        assert_eq!(value, 2);
    }
}
