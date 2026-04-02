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

- make sure `wrk` is installed
- make sure the gateway and backend are already running
- make sure the route returns the expected precheck status

## 中文

在 Linux 服务器上可以直接用下面这个脚本生成可复现的基线压测报告：

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

同目录还会保留原始 `wrk` 输出和环境快照。

执行前请确认：

- 已安装 `wrk`
- 网关和后端已经启动
- 预检查返回的是你期望的状态码
