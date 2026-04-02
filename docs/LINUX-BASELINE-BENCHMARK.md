# Linux Baseline Benchmark / Linux 基线压测

## English

Use the script below on a Linux server to generate a repeatable baseline report:

```bash
bash ./scripts/linux-baseline-report.sh \
  --url http://127.0.0.1:8080/ngx/ \
  --host llmtamer.com:8080 \
  --duration 15 \
  --label llmtamer-loopback
```

The script writes a Markdown report under:

```text
./target/server-bench/<timestamp>-<label>/report.md
```

It also stores raw `wrk` output and environment snapshots in the same directory.

Before running:

- Linux x86_64 and Linux arm64 are both supported.
- Root or `sudo` is recommended so missing tools can be installed automatically.
- If the host uses `apt-get`, `dnf`, or `yum`, the script can auto-install `curl` and `wrk`.
- Make sure the gateway and backend are already running.
- Make sure the route returns the expected precheck status.

## 中文

在 Linux 服务器上，可以直接用下面这条脚本生成可复现的基线压测报告：

```bash
bash ./scripts/linux-baseline-report.sh \
  --url http://127.0.0.1:8080/ngx/ \
  --host llmtamer.com:8080 \
  --duration 15 \
  --label llmtamer-loopback
```

脚本会把 Markdown 报告写到：

```text
./target/server-bench/<timestamp>-<label>/report.md
```

同目录下还会保留原始 `wrk` 输出和环境快照。

执行前请确认：

- 同时支持 Linux x86_64 和 Linux arm64。
- 建议用 root 或具备 `sudo` 的用户执行，这样脚本可以自动补齐缺失工具。
- 如果系统使用 `apt-get`、`dnf` 或 `yum`，脚本会自动安装 `curl` 和 `wrk`。
- 网关和后端已经启动。
- 路由预检查返回的是预期状态码。
