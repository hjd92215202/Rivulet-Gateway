#![cfg(unix)]

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gateway_config::GatewayConfigFile;
use gateway_runtime::GatewayApp;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

#[tokio::test]
async fn sighup_reload_applies_new_config_and_bumps_version() {
    let listener_port = reserve_port();
    let backend_port = reserve_port();
    let backend_addr = format!("127.0.0.1:{backend_port}");
    let config_path = std::env::temp_dir().join(format!(
        "rivulet-sighup-reload-{}.toml",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));

    write_reload_test_config(
        &config_path,
        &format!("127.0.0.1:{listener_port}"),
        &backend_addr,
        4200,
    );
    let config = GatewayConfigFile::load_from_file(&config_path).expect("load config");
    let app =
        GatewayApp::try_from_config(config, Some(config_path.clone())).expect("bootstrap gateway");
    let server = tokio::spawn(async move {
        app.run_until(async {
            sleep(Duration::from_millis(800)).await;
        })
        .await
    });

    sleep(Duration::from_millis(50)).await;

    let overview_before = send_admin_request(
        listener_port,
        b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    assert!(overview_before.contains("\"config_version\":1"));
    assert!(overview_before.contains("\"upstream_read_timeout_ms\":4200"));

    write_reload_test_config(
        &config_path,
        &format!("127.0.0.1:{listener_port}"),
        &backend_addr,
        7300,
    );
    send_sighup_to_current_process();

    let mut overview_after = String::new();
    for _ in 0..15 {
        sleep(Duration::from_millis(40)).await;
        overview_after = send_admin_request(
            listener_port,
            b"GET /__admin/api/overview HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        if overview_after.contains("\"config_version\":2") {
            break;
        }
    }

    assert!(overview_after.contains("\"config_version\":2"));
    assert!(overview_after.contains("\"last_reload_result\":\"success\""));
    assert!(overview_after.contains("\"upstream_read_timeout_ms\":7300"));

    server.await.expect("server task").expect("gateway run");
    let _ = fs::remove_file(config_path);
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
    fs::write(path, toml).expect("write config");
}

fn reserve_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("reserve port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn send_sighup_to_current_process() {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    const SIGHUP: i32 = 1;
    let pid = std::process::id() as i32;
    // 用真实信号触发 runtime 的 SIGHUP 监听路径，验证和管理面重载使用的是同一生效逻辑。
    let code = unsafe { kill(pid, SIGHUP) };
    assert_eq!(code, 0, "send sighup should succeed");
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
