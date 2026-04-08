# Production Readiness Report / 生产就绪度报告

## English

Report date: April 8, 2026  
Project: `Rivulet Gateway / 溪流网关`

### Executive Summary

Current status:

- suitable for controlled production grayscale (API-first traffic class)
- not yet recommended for broad Internet mixed-traffic edge

Key reason:

- G3 public API gate execution chain is now connected and blocking in CI/release
- reliability and capacity checks are now automated and auditable
- remaining gaps are scale hardening and release trust-depth improvement

### Current Gate Status (April 8, 2026)

- `cargo fmt --all -- --check`: pass
- `cargo test --workspace`: pass
- `scripts/check-script-standards.sh` (Git Bash): pass
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard`: pass
- CI/release now run executable public API gate jobs on Linux `x86_64` and `arm64`, and block on failure

### What Is Production-Usable Now

- HTTP/1.1 reverse proxy kernel
- explicit unsupported-path rejection (`Unsupported -> 501`)
- sequential downstream keepalive model (non-pipelined)
- route auth entry, share isolation, and first-rate-limit layer
- built-in Rustls TLS option plus external-TLS-first deployment path
- local hot reload (`SIGHUP` + loopback `POST /__admin/api/reload`) with validate-first atomic swap and rollback-on-failure
- structured runtime/reload observability (`config_version`, `last_reload_result`, `last_reload_at`, `tls_enabled`, `tls_listener`)
- Linux package + systemd lifecycle gates on `x86_64` and `arm64`
- public API reliability gate outputs (`result.json`, `summary.md`) archived in workflows

### Hard Limits

- protocol scope remains HTTP/1.1 only
- no HTTP/2, no mTLS, no WebSocket in current production claim
- no chunked request/response support (explicitly rejected)
- listener address/port and `worker_threads` changes still require restart

### G3 Scope Closure

Closed in this batch:

- executable gate modes: `schema-check`, `baseline`, `soak`, `failure-drill`, `evaluate`
- profile-aware thresholds: `standard`, `strict`, `observe` (default is blocking `standard`)
- scenario-level error classification with gateway-vs-upstream split
- CI/release blocking integration for Linux `x86_64` and `arm64`

### Remaining Work Before Stronger Public-Edge Claim

1. threshold tuning and capacity model calibration on stable Linux benchmark hosts
2. disposable-environment full lifecycle validation at scale (install, upgrade, rollback, uninstall)
3. stronger per-asset detached signature policy and public trust publication
4. operation maturity for larger multi-team rollout

## 中文

报告日期：2026 年 4 月 8 日  
项目：`Rivulet Gateway / 溪流网关`

### 执行摘要

当前状态：

- 已适合受控生产灰度（以 API 流量为主）
- 暂不建议直接承接大规模公网混合流量入口

核心原因：

- G3 公网 API 门禁执行链已打通，并在 CI/release 中进入阻断模式
- 可靠性与容量检查已自动化且可审计
- 剩余短板集中在规模化加固与发布信任链深度

### 当前门禁状态（2026 年 4 月 8 日）

- `cargo fmt --all -- --check`：通过
- `cargo test --workspace`：通过
- `scripts/check-script-standards.sh`（Git Bash）：通过
- `scripts/linux-public-api-gate.sh --mode schema-check --profile standard`：通过
- CI/release 已在 Linux `x86_64` 与 `arm64` 执行可执行公网 API 门禁，任一失败即阻断后续发布

### 当前可用于生产灰度的能力

- HTTP/1.1 反向代理内核
- 非支持路径显式拒绝（`Unsupported -> 501`）
- 下游顺序 keepalive（非 pipelining）模型
- 路由级鉴权入口、分享隔离、限流第一层能力
- 内建 Rustls TLS 选项与外置 TLS 优先生产路径
- 本地热重载（`SIGHUP` + loopback `POST /__admin/api/reload`），采用先校验后原子切换，失败回滚旧配置
- 结构化运行时/重载可观测字段（`config_version`、`last_reload_result`、`last_reload_at`、`tls_enabled`、`tls_listener`）
- Linux `x86_64`/`arm64` 打包与 systemd 生命周期门禁
- 公网 API 门禁产物（`result.json`、`summary.md`）可在工作流中归档审计

### 当前硬边界

- 协议面仍限定 HTTP/1.1
- 现阶段生产声明不包含 HTTP/2、mTLS、WebSocket
- chunked request/response 仍不支持，采用显式拒绝策略
- listener 地址/端口与 `worker_threads` 变更仍需重启生效

### G3 本批收口

本批已完成：

- 可执行门禁模式：`schema-check`、`baseline`、`soak`、`failure-drill`、`evaluate`
- 多档阈值：`standard`、`strict`、`observe`（默认 `standard` 阻断）
- 场景级网关故障与上游业务故障分离统计
- Linux `x86_64`/`arm64` 的 CI/release 阻断接线

### 在更强公网声明前仍需补齐

1. 在稳定 Linux 基准机上继续校准阈值与容量模型
2. 在一次性环境中扩大安装、升级、回滚、卸载全链路验证规模
3. 升级逐产物 detached 签名策略并公开信任链材料
4. 面向多团队协作场景的运维成熟度提升
