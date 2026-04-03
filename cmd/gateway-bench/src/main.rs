//! 本地压测入口负责搭出一个“后端服务 + 网关 + 压测客户端”的闭环。
//! 目标不是追求花哨功能，而是先给内核阶段建立一套可重复执行的能力边界基线。

use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gateway_config::{
    EndpointConfig, GatewayConfigFile, ListenerConfig, LoadBalanceConfig, ProtocolConfig,
    RouteConfig, RuntimeConfig, UpstreamConfig,
};
use gateway_runtime::GatewayApp;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::sleep;

#[derive(Clone, Debug)]
struct BenchOptions {
    /// 每轮正式测量持续秒数。
    measure_secs: u64,
    /// 预热阶段持续秒数。
    warmup_secs: u64,
    /// 并发阶梯。
    concurrency_levels: Vec<usize>,
    /// 响应体大小集合，单位字节。
    response_sizes: Vec<usize>,
    /// 上游空闲连接池大小集合。
    idle_pool_sizes: Vec<usize>,
}

impl Default for BenchOptions {
    fn default() -> Self {
        Self {
            // 默认把单轮时长压在几秒，兼顾稳定性和总执行时间。
            measure_secs: 2,
            warmup_secs: 1,
            concurrency_levels: vec![1, 8, 32, 128],
            response_sizes: vec![64, 4096],
            idle_pool_sizes: vec![0, 1],
        }
    }
}

impl BenchOptions {
    fn from_args() -> Result<Self, Box<dyn Error>> {
        let mut options = Self::default();
        let mut args = env::args().skip(1);

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--measure-secs" => {
                    options.measure_secs = parse_u64_arg(args.next(), "--measure-secs")?;
                }
                "--warmup-secs" => {
                    options.warmup_secs = parse_u64_arg(args.next(), "--warmup-secs")?;
                }
                "--concurrency" => {
                    options.concurrency_levels =
                        parse_usize_list_arg(args.next(), "--concurrency")?;
                }
                "--response-sizes" => {
                    options.response_sizes = parse_usize_list_arg(args.next(), "--response-sizes")?;
                }
                "--idle-pools" => {
                    options.idle_pool_sizes = parse_usize_list_arg(args.next(), "--idle-pools")?;
                }
                other => {
                    return Err(format!("unknown argument {}", other).into());
                }
            }
        }

        if options.concurrency_levels.is_empty() {
            return Err("concurrency list must not be empty".into());
        }
        if options.response_sizes.is_empty() {
            return Err("response size list must not be empty".into());
        }
        if options.idle_pool_sizes.is_empty() {
            return Err("idle pool list must not be empty".into());
        }

        Ok(options)
    }
}

#[derive(Debug)]
struct ScenarioSpec {
    /// 当前场景的上游空闲连接池大小。
    idle_pool_size: usize,
    /// 当前场景后端返回的响应体大小。
    response_size: usize,
    /// 当前场景的客户端并发。
    concurrency: usize,
}

#[derive(Debug)]
struct ScenarioResult {
    /// 场景本身的规格。
    spec: ScenarioSpec,
    /// 正式测量窗口长度。
    duration: Duration,
    /// 成功请求数。
    successes: u64,
    /// 失败请求数。
    errors: u64,
    /// 下载字节数，用于观察带宽型瓶颈。
    response_bytes: u64,
    /// p50 延迟，单位毫秒。
    p50_ms: f64,
    /// p95 延迟，单位毫秒。
    p95_ms: f64,
    /// p99 延迟，单位毫秒。
    p99_ms: f64,
    /// backend 实际 accept 的 TCP 连接数。
    backend_accepts: u64,
    /// backend 实际处理的请求数。
    backend_requests: u64,
    /// 当前场景中最主要的错误类型，用于辅助判断瓶颈是在网络、协议还是实现路径。
    top_errors: Vec<(String, u64)>,
}

#[derive(Default)]
struct BackendStats {
    /// 用来观察上游 keepalive 是否真的减少了建连次数。
    accept_count: AtomicU64,
    /// 用来校验后端实际收到的请求量。
    request_count: AtomicU64,
}

impl BackendStats {
    fn reset(&self) {
        self.accept_count.store(0, Ordering::Relaxed);
        self.request_count.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> (u64, u64) {
        (
            self.accept_count.load(Ordering::Relaxed),
            self.request_count.load(Ordering::Relaxed),
        )
    }
}

#[derive(Default)]
struct WorkerMetrics {
    /// 当前 worker 成功完成的请求数。
    successes: u64,
    /// 当前 worker 失败的请求数。
    errors: u64,
    /// 当前 worker 接收的响应总字节数。
    response_bytes: u64,
    /// 当前 worker 的逐请求延迟样本。
    latencies_micros: Vec<u64>,
    /// 当前 worker 汇总的错误分布。
    error_kinds: BTreeMap<String, u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = BenchOptions::from_args()?;

    println!("gateway local benchmark");
    println!(
        "measure_secs={} warmup_secs={} concurrency={:?} response_sizes={:?} idle_pools={:?}",
        options.measure_secs,
        options.warmup_secs,
        options.concurrency_levels,
        options.response_sizes,
        options.idle_pool_sizes
    );
    println!(
        "pool,response_bytes,concurrency,successes,errors,rps,p50_ms,p95_ms,p99_ms,backend_accepts,backend_requests,accepts_per_request,mbps"
    );

    for &idle_pool_size in &options.idle_pool_sizes {
        for &response_size in &options.response_sizes {
            for &concurrency in &options.concurrency_levels {
                let spec = ScenarioSpec {
                    idle_pool_size,
                    response_size,
                    concurrency,
                };
                let result = run_scenario(&options, spec).await?;
                print_result(&result);
            }
        }
    }

    Ok(())
}

async fn run_scenario(
    options: &BenchOptions,
    spec: ScenarioSpec,
) -> Result<ScenarioResult, Box<dyn Error>> {
    let backend_listener = TcpListener::bind("127.0.0.1:0").await?;
    let backend_addr = backend_listener.local_addr()?;
    let backend_stats = Arc::new(BackendStats::default());
    let response_bytes = Arc::new(build_backend_response(spec.response_size));
    let backend_shutdown = spawn_backend(
        backend_listener,
        Arc::clone(&backend_stats),
        Arc::clone(&response_bytes),
    );

    let gateway_port = reserve_port()?;
    let gateway_addr = format!("127.0.0.1:{gateway_port}");
    let gateway_shutdown =
        spawn_gateway(gateway_port, backend_addr.to_string(), spec.idle_pool_size);

    // 给 listener 和后台任务一个很短的启动窗口，避免把冷启动抖动混进错误率。
    sleep(Duration::from_millis(120)).await;

    let request_bytes = Arc::new(build_downstream_request());
    if options.warmup_secs > 0 {
        let warmup_deadline = Instant::now() + Duration::from_secs(options.warmup_secs);
        run_workers(
            spec.concurrency,
            &gateway_addr,
            Arc::clone(&request_bytes),
            warmup_deadline,
        )
        .await?;
        // 预热只为了让连接和调度进入稳定态，所以统计要在正式测量前清零。
        backend_stats.reset();
    }

    let started_at = Instant::now();
    let deadline = started_at + Duration::from_secs(options.measure_secs);
    let metrics = run_workers(spec.concurrency, &gateway_addr, request_bytes, deadline).await?;
    let duration = started_at.elapsed();

    let _ = gateway_shutdown.send(());
    let _ = backend_shutdown.send(());

    // 这里明确等待场景清理完成，避免下一个场景复用到脏端口或残留任务。
    sleep(Duration::from_millis(80)).await;

    let (backend_accepts, backend_requests) = backend_stats.snapshot();

    Ok(ScenarioResult {
        p50_ms: percentile_ms(&metrics.latencies_micros, 0.50),
        p95_ms: percentile_ms(&metrics.latencies_micros, 0.95),
        p99_ms: percentile_ms(&metrics.latencies_micros, 0.99),
        spec,
        duration,
        successes: metrics.successes,
        errors: metrics.errors,
        response_bytes: metrics.response_bytes,
        backend_accepts,
        backend_requests,
        top_errors: summarize_error_kinds(&metrics.error_kinds),
    })
}

fn spawn_gateway(
    gateway_port: u16,
    backend_address: String,
    idle_pool_size: usize,
) -> oneshot::Sender<()> {
    let config = GatewayConfigFile {
        runtime: RuntimeConfig {
            // 当前 benchmark 主要观察 I/O 语义和连接边界，超时用保守默认值即可。
            upstream_idle_pool_size: idle_pool_size,
            ..RuntimeConfig::default()
        },
        listeners: vec![ListenerConfig {
            name: "bench".into(),
            address: format!("127.0.0.1:{gateway_port}"),
            protocol: ProtocolConfig::Http1,
        }],
        routes: vec![RouteConfig {
            name: "bench-route".into(),
            listener: "bench".into(),
            hosts: vec!["bench.test".into()],
            path_prefixes: vec!["/".into()],
            methods: vec![],
            upstream: "bench-upstream".into(),
            filters: vec![],
            policy: Default::default(),
            auth: Default::default(),
            rate_limit: Default::default(),
            share: Default::default(),
        }],
        upstreams: vec![UpstreamConfig {
            name: "bench-upstream".into(),
            load_balance: LoadBalanceConfig::RoundRobin,
            health_check: None,
            policy: Default::default(),
            endpoints: vec![EndpointConfig {
                address: backend_address,
                weight: 1,
            }],
        }],
    };

    let app = GatewayApp::from_config(config);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = app
            .run_until(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    shutdown_tx
}

fn spawn_backend(
    listener: TcpListener,
    stats: Arc<BackendStats>,
    response_bytes: Arc<Vec<u8>>,
) -> oneshot::Sender<()> {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut connections = JoinSet::new();

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, _)) => {
                            stats.accept_count.fetch_add(1, Ordering::Relaxed);
                            let connection_stats = Arc::clone(&stats);
                            let connection_response = Arc::clone(&response_bytes);
                            connections.spawn(async move {
                                run_backend_connection(stream, connection_stats, connection_response).await
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        while let Some(joined) = connections.join_next().await {
            let _ = joined;
        }
    });
    shutdown_tx
}

async fn run_backend_connection(
    mut stream: TcpStream,
    stats: Arc<BackendStats>,
    response_bytes: Arc<Vec<u8>>,
) -> std::io::Result<()> {
    loop {
        // 这里故意只做最小请求读取：当前压测流量都是 `Content-Length: 0` 的 GET，
        // 所以把边界稳定地读到 `\r\n\r\n` 即可。
        if !read_request_head(&mut stream).await? {
            break;
        }
        stats.request_count.fetch_add(1, Ordering::Relaxed);
        stream.write_all(response_bytes.as_ref()).await?;
        stream.flush().await?;
    }

    Ok(())
}

async fn read_request_head(stream: &mut TcpStream) -> std::io::Result<bool> {
    let mut buffer = Vec::with_capacity(512);
    let mut temp = [0_u8; 512];

    loop {
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(true);
        }

        let read = stream.read(&mut temp).await?;
        if read == 0 {
            return Ok(!buffer.is_empty());
        }
        buffer.extend_from_slice(&temp[..read]);
    }
}

async fn run_workers(
    concurrency: usize,
    gateway_addr: &str,
    request_bytes: Arc<Vec<u8>>,
    deadline: Instant,
) -> Result<WorkerMetrics, Box<dyn Error>> {
    let mut workers = JoinSet::new();

    for _ in 0..concurrency {
        let address = gateway_addr.to_string();
        let request = Arc::clone(&request_bytes);
        workers.spawn(async move { run_worker(&address, request, deadline).await });
    }

    let mut merged = WorkerMetrics::default();
    while let Some(joined) = workers.join_next().await {
        let metrics = joined??;
        merged.successes += metrics.successes;
        merged.errors += metrics.errors;
        merged.response_bytes += metrics.response_bytes;
        merged.latencies_micros.extend(metrics.latencies_micros);
        merge_error_kinds(&mut merged.error_kinds, metrics.error_kinds);
    }

    Ok(merged)
}

async fn run_worker(
    gateway_addr: &str,
    request_bytes: Arc<Vec<u8>>,
    deadline: Instant,
) -> Result<WorkerMetrics, std::io::Error> {
    let mut metrics = WorkerMetrics::default();

    while Instant::now() < deadline {
        let started_at = Instant::now();
        match single_request(gateway_addr, request_bytes.as_ref()).await {
            Ok(response_len) => {
                metrics.successes += 1;
                metrics.response_bytes += response_len as u64;
                metrics
                    .latencies_micros
                    .push(started_at.elapsed().as_micros() as u64);
            }
            Err(error) => {
                metrics.errors += 1;
                *metrics.error_kinds.entry(error).or_insert(0) += 1;
            }
        }
    }

    Ok(metrics)
}

async fn single_request(gateway_addr: &str, request_bytes: &[u8]) -> Result<usize, String> {
    let mut stream = TcpStream::connect(gateway_addr)
        .await
        .map_err(|error| format!("connect: {}", error))?;
    stream
        .write_all(request_bytes)
        .await
        .map_err(|error| format!("write: {}", error))?;
    stream
        .flush()
        .await
        .map_err(|error| format!("flush: {}", error))?;

    let mut response = Vec::with_capacity(1024);
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|error| format!("read: {}", error))?;

    // benchmark 里也做最小正确性校验，避免把错误响应误算成成功吞吐。
    if !response.starts_with(b"HTTP/1.1 200 OK\r\n") {
        let status_line = response
            .split(|byte| *byte == b'\n')
            .next()
            .map(|line| String::from_utf8_lossy(line).trim().to_string())
            .filter(|line| !line.is_empty())
            .unwrap_or_else(|| "unknown response".into());
        return Err(format!("unexpected status code: {}", status_line));
    }

    Ok(response.len())
}

fn build_downstream_request() -> Vec<u8> {
    b"GET /bench HTTP/1.1\r\nHost: bench.test\r\nContent-Length: 0\r\n\r\n".to_vec()
}

fn build_backend_response(response_size: usize) -> Vec<u8> {
    let body = vec![b'x'; response_size];
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

fn percentile_ms(samples_micros: &[u64], percentile: f64) -> f64 {
    if samples_micros.is_empty() {
        return 0.0;
    }

    let mut sorted = samples_micros.to_vec();
    sorted.sort_unstable();

    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index] as f64 / 1_000.0
}

fn print_result(result: &ScenarioResult) {
    let seconds = result.duration.as_secs_f64().max(0.001);
    let rps = result.successes as f64 / seconds;
    let accepts_per_request = if result.backend_requests == 0 {
        0.0
    } else {
        result.backend_accepts as f64 / result.backend_requests as f64
    };
    let mbps = result.response_bytes as f64 / seconds / 1024.0 / 1024.0;

    println!(
        "{},{},{},{},{},{:.2},{:.3},{:.3},{:.3},{},{},{:.4},{:.2}",
        result.spec.idle_pool_size,
        result.spec.response_size,
        result.spec.concurrency,
        result.successes,
        result.errors,
        rps,
        result.p50_ms,
        result.p95_ms,
        result.p99_ms,
        result.backend_accepts,
        result.backend_requests,
        accepts_per_request,
        mbps
    );

    if !result.top_errors.is_empty() {
        let details = result
            .top_errors
            .iter()
            .map(|(message, count)| format!("{} x{}", message, count))
            .collect::<Vec<_>>()
            .join(" | ");
        println!(
            "# errors pool={} body={} concurrency={} -> {}",
            result.spec.idle_pool_size, result.spec.response_size, result.spec.concurrency, details
        );
    }
}

fn reserve_port() -> Result<u16, Box<dyn Error>> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

fn parse_u64_arg(value: Option<String>, flag: &str) -> Result<u64, Box<dyn Error>> {
    let value = value.ok_or_else(|| format!("missing value for {}", flag))?;
    Ok(value.parse::<u64>()?)
}

fn parse_usize_list_arg(value: Option<String>, flag: &str) -> Result<Vec<usize>, Box<dyn Error>> {
    let value = value.ok_or_else(|| format!("missing value for {}", flag))?;
    let mut parsed = Vec::new();

    for item in value.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        parsed.push(item.parse::<usize>()?);
    }

    if parsed.is_empty() {
        return Err(format!("{} must contain at least one numeric item", flag).into());
    }

    Ok(parsed)
}

fn merge_error_kinds(target: &mut BTreeMap<String, u64>, source: BTreeMap<String, u64>) {
    for (message, count) in source {
        *target.entry(message).or_insert(0) += count;
    }
}

fn summarize_error_kinds(error_kinds: &BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut items = error_kinds
        .iter()
        .map(|(message, count)| (message.clone(), *count))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    items.truncate(3);
    items
}
