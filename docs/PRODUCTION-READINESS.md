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
