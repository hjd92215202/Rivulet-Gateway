# Production Readiness Report / 生产就绪度报告

## English

Report date: April 8, 2026  
Project: `Rivulet Gateway / 溪流网关`

### Executive summary

Current status:

- production-gray usable for API-first traffic
- not yet declared as broad public-edge mixed-traffic gateway

The main progress of this batch:

- G3.1 capacity gate moved from "connected" to "stable blocking contract"
- CI and release use the same `standard` profile and both block on failures
- threshold model is now profile + architecture aware (`linux-x86_64`, `linux-arm64`)

### Current blocking gates

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `scripts/check-script-standards.sh` (Git Bash)
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard --arch linux-x86_64`
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard --arch linux-arm64`
- CI/release `public-api-gate-*` evaluate jobs (long-run, architecture-aware, blocking)

### G3.1 readiness closure

- evaluate scoring uses 3-sample median for baseline and soak
- request-floor checks are mandatory:
  - `MIN_TOTAL_REQUESTS_BASELINE`
  - `MIN_TOTAL_REQUESTS_SOAK`
- failures now include explicit reasons (threshold miss vs sample-floor miss)
- result contract now includes `arch`, `duration_profile`, `request_floor_checks`

### G3.1.1 stability closure (CI hang containment)

- fixed fixture backend SIGTERM deadlock class:
  - signal handler no longer calls `server.shutdown()` in the same thread
  - shutdown is triggered and executed from a separate thread
- Linux validation scripts now enforce bounded background shutdown:
  - `TERM` with bounded polling
  - fallback `KILL` when timeout is exceeded
  - explicit failure when process still cannot exit
- systemd lifecycle script now wraps key `systemctl` actions with command timeout guards
- CI/release systemd lifecycle jobs now have explicit workflow-level `timeout-minutes`

### G3.1.2 hotfix closure (public-api gate evaluate path pollution)

- stage progress logs are now emitted to `stderr` via shared `print_stage`
- `linux-public-api-gate.sh` no longer relies on command substitution to capture summary paths in evaluate flow
- script standards check now blocks regressions where `print_stage` writes to `stdout`
- expected failure class removed:
  - stage text accidentally concatenated into JSON file path and causing `FileNotFoundError`

### CI hang troubleshooting checklist

1. check fixture backend shutdown signal path and confirm SIGTERM exits within bounded time
2. check script summary/result for process-shutdown diagnostics (`result`, `seconds`)
3. check whether `systemctl` timeout boundary was hit in lifecycle summary
4. check workflow timeout vs script timeout to locate where blocking occurred

### Current limits

- protocol remains HTTP/1.1 only
- `Unsupported -> 501` contract unchanged
- no HTTP/2, no mTLS, no WebSocket in production claim
- listener address/port and `worker_threads` changes still require restart

### Remaining production-hardening gaps

1. continue per-architecture threshold tuning on stable Linux runners
2. expand disposable install/upgrade/rollback/uninstall coverage
3. strengthen detached signature policy for each release asset
4. advance operational maturity for multi-team incident workflows

## 中文

报告日期：2026 年 4 月 8 日  
项目：`Rivulet Gateway / 溪流网关`

### 执行摘要

当前状态：

- 已可用于 API 优先场景的生产灰度
- 仍未声明可直接承接大规模公网混合流量入口

本批次核心进展：

- G3.1 容量门禁从“已接通”推进到“稳定阻断契约”
- CI 与 release 统一使用 `standard` 阻断档，口径一致
- 阈值模型升级为 profile + 架构维度（`linux-x86_64`、`linux-arm64`）

### 当前阻断门禁

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `scripts/check-script-standards.sh`（Git Bash）
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard --arch linux-x86_64`
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard --arch linux-arm64`
- CI/release 中 `public-api-gate-*` 的 evaluate 长跑门禁（按架构阻断）

### G3.1 收口内容

- baseline 与 soak 均采用 3 次采样中位数判分
- 请求量门槛改为强制校验：
  - `MIN_TOTAL_REQUESTS_BASELINE`
  - `MIN_TOTAL_REQUESTS_SOAK`
- 失败原因可区分“阈值不达标”与“样本不足”
- 输出契约新增 `arch`、`duration_profile`、`request_floor_checks`

### G3.1.1 稳定性收口（CI 卡死治理）

- 已修复 fixture backend 的 SIGTERM 死锁类风险：
  - 信号处理器不再同线程直接调用 `server.shutdown()`
  - 由独立线程执行 shutdown，避免 `serve_forever` 同线程互锁
- Linux 验证脚本新增有界后台停止机制：
  - 先 `TERM` + 有限轮询
  - 超时后自动 `KILL`
  - 仍无法退出时显式失败，不再无限等待
- systemd 生命周期脚本对关键 `systemctl` 操作增加命令级超时保护
- CI/release 的 systemd lifecycle job 增加 workflow 级 `timeout-minutes` 兜底

### G3.1.2 热修复收口（public-api gate evaluate 路径污染）

- 共享 `print_stage` 的进度日志统一改为写入 `stderr`
- `linux-public-api-gate.sh` 的 evaluate 流程不再通过命令替换捕获 summary 路径
- 脚本标准检查新增约束：`print_stage` 若写回 `stdout` 会被门禁拦截
- 已移除这类错误形态：
  - stage 文本拼入 JSON 路径并触发 `FileNotFoundError`

### CI 卡死排障检查清单

1. 先确认 fixture backend 的 SIGTERM 退出链路是否在阈值内完成
2. 查看脚本 `summary/result` 中的进程停止诊断字段（`result`、`seconds`）
3. 检查 lifecycle summary 是否命中 `systemctl` 超时边界
4. 对照 workflow 超时与脚本超时，定位阻塞发生层级

### 当前硬边界

- 协议面仍限定 HTTP/1.1
- `Unsupported -> 501` 契约保持不变
- 生产声明仍不包含 HTTP/2、mTLS、WebSocket
- listener 地址/端口与 `worker_threads` 变更仍需重启

### 仍需继续推进的生产加固项

1. 在稳定 Linux runner 上继续做按架构阈值校准
2. 扩展一次性环境安装/升级/回滚/卸载验证覆盖
3. 加强逐产物 detached 签名策略
4. 提升多团队协同下的运维与应急成熟度
