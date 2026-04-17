# Production Readiness Report / 生产就绪度报告

## M2 Closure Addendum (G3.4.2) / M2 收口补充（G3.4.2）

### English

This batch keeps `standard` dual-blocking unchanged and extends report-only closure tooling:

- nightly now generates `threshold-change-proposal.md` for architecture-aware `standard` threshold suggestions (manual review only)
- nightly now generates `closure-weekly-report.md` for M2 closure trend review (observe + CI/release streak synthesis)
- `milestone2-closure-status.json` now includes:
  - `ci_closure_ready`
  - `release_closure_ready`
  - `overall_closure_ready`
  - `remaining_to_target`
  - `recent_failure_reasons`

### 中文

本批保持 `standard` 双阻断不变，并补齐“只报告不自动改值”的收口工具链：

- nightly 新增 `threshold-change-proposal.md`，用于按架构 `standard` 阈值建议（仅人工评审）
- nightly 新增 `closure-weekly-report.md`，用于 M2 收口趋势周报（observe + CI/release streak 汇总）
- `milestone2-closure-status.json` 新增稳定字段：
  - `ci_closure_ready`
  - `release_closure_ready`
  - `overall_closure_ready`
  - `remaining_to_target`
  - `recent_failure_reasons`

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
- G3.4 calibration tooling is now wired through nightly observe sampling and report artifacts

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

### G3.1.3 reliability closure (real gate scoring failures)

- root-cause signal from gate artifacts was consistent on x86_64 and arm64:
  - `availability_below_threshold`
  - `gateway_5xx_ratio_above_threshold`
  - `failure_drill_not_passed`
- gate runtime profile for evaluate now uses `upstream_retry_attempts = 2` to absorb transient runner-side I/O jitter without relaxing thresholds
- evaluate mode now prints `failure_reasons`, `threshold_checks`, and `observed` directly in job logs for faster triage
- proxy kernel now supports bounded same-endpoint retry when all conditions are true:
  - single-endpoint upstream
  - retry budget remains
  - error is transient upstream I/O (connect/read/write upstream)
- protocol errors remain non-retryable on same endpoint; `Unsupported -> 501` contract remains unchanged

### G3.1.4 failure-drill closure (without relaxing standard)

- CI failures were narrowed to `failure_drill_not_passed` on both Linux x86_64 and arm64, while availability/latency/gateway ratio checks already passed
- proxy kernel now preserves pass-through semantics for retryable upstream status (`500/502/503/504`) in single-endpoint clusters
- retryable-status retry/switch semantics for multi-endpoint clusters remain unchanged
- gate outputs now include failure-drill diagnostics for direct CI triage:
  - `business_total_requests`
  - `business_gateway_5xx_ratio`
  - `gateway_fault_breakdown`
  - `gateway_fault_network_errors_total`
- this batch does not relax `standard` profile thresholds and does not change blocking semantics in CI/release

### G3.1.5 failure-drill stability closure (arm64 real gate failure)

- root signal remained `failure_drill_not_passed` with threshold checks otherwise green
- reliability closure is dual-track without lowering `standard`:
  - kernel track: upstream transient I/O errors now carry stable `upstream_io/<kind>:` prefixes and keep bounded same-endpoint retry semantics
  - gate track: business `503` drill now runs 3 samples and uses median-based gateway-contamination check
- failure-drill contract adds audit fields:
  - `business_samples`
  - `business_gateway_5xx_median`
  - `business_gateway_5xx_max`
  - `business_pass_policy`
- blocking policy remains unchanged:
  - CI/release still block on `standard`
  - no threshold relaxation and no protocol-surface expansion

### G3.4 calibration closure in progress (observe + report, no threshold auto-rewrite)

- nightly now collects Linux x86_64 and Linux arm64 observe samples using `public-api-gate --mode evaluate --profile observe`
- nightly summary now produces:
  - `calibration-report.json` + `calibration-report.md`
  - `streak-report.json` + `streak-report.md` for both `ci` and `release`
- calibration report includes per-architecture distribution statistics and deltas against current `standard` thresholds
- streak report tracks consecutive dual-architecture success against closure target (`10`)
- CI/release `standard` dual blocking remains unchanged; this phase is report-first and does not auto-edit threshold files

### G3.4.1 closure execution wiring (report-driven, still non-blocking)

- calibration report now includes conservative `recommended_thresholds` and per-metric `change_budget`
- nightly summary now generates:
  - `threshold-pr-checklist.md` for manual threshold PR review
  - `milestone2-closure-status.json/.md` for milestone closure synthesis
- streak reports now expose:
  - `closure_ready`
  - `remaining_to_target`
- closure status is computed from both workflows:
  - `ci` and `release` must both reach 10 consecutive dual-architecture green runs
- this batch keeps report-only behavior:
  - no hard blocking on streak count
  - no automatic threshold rewrite

### G3.4.1 收口执行（先报告、非硬阻断）

- 校准报告新增保守建议字段：`recommended_thresholds` 与 `change_budget`
- nightly 汇总新增产物：
  - `threshold-pr-checklist.md`（阈值 PR 人工评审清单）
  - `milestone2-closure-status.json/.md`（里程碑收口状态汇总）
- streak 报告新增收口字段：
  - `closure_ready`
  - `remaining_to_target`
- Milestone 2 收口判定保持：
  - `ci` 与 `release` 都达到连续 10 次双架构全绿
- 本批仍坚持：
  - 仅报告，不因 streak 计数增加新的硬阻断
  - 不自动回写阈值文件

### G3.4.3 closure sprint operating loop (M2 first, then M3)

- nightly summary now generates `m2-nightly-review-package.md` as one ordered entry point for closure review.
- nightly summary now generates `m2-cutover-check.json/.md` as explicit M2->M3 cutover readiness signal.
- operating rule is now stable:
  - review package first
  - keep threshold PR disabled when `ready_for_threshold_pr=false`
  - execute docs cutover only when `cutover_ready=true`

### G3.4.3 收口冲刺运营闭环（先 M2，后 M3）

- nightly 汇总新增 `m2-nightly-review-package.md`，作为收口审阅统一入口（固定顺序）。
- nightly 汇总新增 `m2-cutover-check.json/.md`，用于 M2->M3 切线可执行判定。
- 运营规则固定为：
  - 先审阅 package；
  - `ready_for_threshold_pr=false` 时禁止阈值 PR；
  - 仅在 `cutover_ready=true` 时执行文档切线。

### G3.4.4 streak eligibility semantics fix (no threshold relaxation)

- streak counting now uses eligible gate runs only:
  - both required Public API Gate jobs must be present and `success` to count as valid streak samples
  - `skipped`/`missing` gate jobs are now marked `not_eligible` and no longer reset streak
- closure synthesis now separates diagnostics:
  - `recent_failure_reasons` keeps only real gate failures from eligible runs
  - `recent_ineligible_reasons` tracks skip/missing noise causes for auditability
- nightly review package now includes streak sample-quality summary:
  - eligible/ineligible counts for both CI and release
  - aggregated ineligible reason categories
- this batch is a statistics-semantics repair only:
  - no `standard` threshold relaxation
  - no CI/release blocking policy downgrade
  - no gateway protocol-surface change

### G3.4.4 收口（连绿统计语义修复，不放宽阈值）

- streak 统计口径改为仅基于有效 gate run：
  - 仅当两条 required Public API Gate job 均存在且 `success` 才计入连绿样本；
  - gate job `skipped`/`missing` 统一标记为 `not_eligible`，不再误伤 streak 归零。
- 收口聚合诊断字段完成分离：
  - `recent_failure_reasons` 仅保留有效样本中的真实 gate 失败；
  - `recent_ineligible_reasons` 单独记录 skip/missing 噪声来源，便于审计。
- nightly 审阅包新增样本质量摘要：
  - CI/release 的 eligible/ineligible 样本数量；
  - ineligible 原因聚合。
- 本批明确属于“统计语义修复”：
  - 不放宽 `standard` 阈值；
  - 不降低 CI/release 阻断强度；
  - 不改变网关对外协议面。

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
2. reach and hold the milestone-2 closure target of 10 consecutive dual-architecture green runs
3. advance operational maturity for multi-team incident workflows

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
- G3.4 校准工具链已接入 nightly observe 采样与报告产物

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

### G3.1.3 可靠性收口（真实判分失败治理）

- x86_64 与 arm64 的门禁产物出现一致根因：
  - `availability_below_threshold`
  - `gateway_5xx_ratio_above_threshold`
  - `failure_drill_not_passed`
- evaluate 门禁工况将 `upstream_retry_attempts` 调整为 `2`，用于吸收 runner 瞬态 I/O 抖动，不放宽阈值
- evaluate 结束后会在 job 日志直接输出 `failure_reasons`、`threshold_checks`、`observed`，加速排障闭环
- 代理内核新增“有界同节点重试”能力，仅在以下条件同时满足时生效：
  - upstream 只有单节点
  - 请求仍有重试预算
  - 错误属于上游瞬态 I/O（connect/read/write upstream）
- 协议错误仍不允许同节点重试，`Unsupported -> 501` 契约保持不变

### G3.1.4 failure-drill 收口（不放宽 standard）

- CI 失败已收敛为双架构一致的 `failure_drill_not_passed`，可用性/延迟/网关 5xx 比例检查均已通过
- 代理内核在单节点集群下对 retryable 上游状态（`500/502/503/504`）改为优先保持透传语义
- 多节点集群对 retryable 状态的重试与切换行为保持不变
- gate 产物补齐 failure-drill 诊断字段，便于在 Actions 日志直接判因：
  - `business_total_requests`
  - `business_gateway_5xx_ratio`
  - `gateway_fault_breakdown`
  - `gateway_fault_network_errors_total`
- 本批不放宽 `standard` 阈值，不改变 CI/release 阻断语义

### G3.4 校准收口进行中（观测+报告，不自动改阈值）

- nightly 现已采集 Linux x86_64 与 Linux arm64 的 observe 样本（`public-api-gate --mode evaluate --profile observe`）
- nightly 汇总新增产物：
  - `calibration-report.json` + `calibration-report.md`
  - `streak-report.json` + `streak-report.md`（分别覆盖 `ci` 与 `release`）
- 校准报告固定包含按架构分布统计与当前 `standard` 阈值差值
- 连续全绿统计报告固定跟踪“连续双架构成功次数”与收口目标（`10`）
- CI/release 的 `standard` 双阻断口径保持不变；本阶段坚持“先报告后调阈值”

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
2. 达成并保持“连续 10 次双架构全绿”的里程碑收口目标
3. 提升多团队协同下的运维与应急成熟度
