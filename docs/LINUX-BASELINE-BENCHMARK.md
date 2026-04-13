# Linux Baseline Benchmark / Linux 基线压测

## English

This document defines the public API gate baseline benchmark contract for Linux `x86_64` and `arm64`.

### Core command

```bash
bash ./scripts/linux-public-api-gate.sh \
  --mode evaluate \
  --profile standard \
  --arch linux-x86_64 \
  --artifact ./dist/<target>/rivulet-gateway-<version>.tar.gz \
  --format tar.gz \
  --output-dir ./target/public-api-gate/manual-evaluate
```

For arm64:

```bash
--arch linux-arm64
```

### Modes

- `schema-check`: threshold schema and profile-arch contract validation
- `baseline`: steady-state samples
- `soak`: longer steady-state samples
- `failure-drill`: upstream fault and recovery drills
- `evaluate`: baseline + soak + failure-drill with final gate decision

### Standard profile (blocking)

`standard` is now architecture-aware and blocking in both CI and release.

For each architecture, thresholds include:

- availability minimum
- gateway `5xx` ratio maximum
- p95 and p99 latency limits
- request floor: `MIN_TOTAL_REQUESTS_BASELINE` and `MIN_TOTAL_REQUESTS_SOAK`

### Anti-jitter scoring

- 3 samples per baseline/soak scenario
- median-based scoring
- request-floor checks must pass, otherwise gate fails even if latency/error thresholds pass

### Long-run window (default)

- `baseline`: 3 rounds × 20s
- `soak`: 3 rounds × 60s
- `failure-drill`: each drill path 12s

### Output contract

Every run writes:

- `result.json` (machine-readable)
- `summary.md` (human-readable)

`result.json` now includes:

- `arch`
- `duration_profile` (currently `long`)
- `request_floor_checks`
- explicit `failure_reasons`

### G3.4 calibration report pipeline

Nightly now includes architecture observe sampling and calibration aggregation.

Calibration script:

```bash
bash ./scripts/public-api-calibration-report.sh \
  --profile standard \
  --output-dir ./target/public-api-calibration/manual \
  --inputs ./result-x86.json ./result-arm.json
```

Outputs:

- `calibration-report.json`
- `calibration-report.md`

Report fields include:

- per-architecture distribution summary for `availability/gateway_5xx_ratio/p95/p99` (`min/median/p90/p95/max`)
- delta against current `standard` thresholds
- sample counts and latest trend rows
- manual review items (no automatic threshold rewrite)

### G3.4 streak report pipeline

Streak script (report-only, non-blocking):

```bash
bash ./scripts/public-api-gate-streak-report.sh \
  --repo <owner/name> \
  --workflow ci \
  --window 40 \
  --output-dir ./target/public-api-streak/ci
```

Release streak:

```bash
--workflow release
```

Outputs:

- `streak-report.json`
- `streak-report.md`

The closure indicator tracks whether consecutive dual-architecture success reaches 10.

### Shutdown boundary and anti-hang diagnostics

- gate runtime now enforces bounded background process shutdown:
  - `TERM` with bounded wait
  - fallback `KILL` on timeout
  - explicit failure when process still cannot exit
- `result.json` now carries `process_shutdown` diagnostics (gateway and fixture backend)
- `summary.md` now appends process shutdown result and elapsed seconds
- when used together with `linux-systemd-validate.sh`, key `systemctl` actions are guarded by command timeout
- CI/release also applies workflow-level timeout to avoid indefinite hangs

### Log channel and evaluate path safety

- stage progress logs are emitted on `stderr` and are not part of machine-returned stdout values
- evaluate mode summary files are passed by explicit paths between script functions
- expected guardrail:
  - no stage text can be concatenated into JSON file paths during evaluate scoring

### G3.1.3 reliability notes (standard profile unchanged)

- `standard` thresholds remain unchanged; this batch does not relax SLO criteria
- gate runtime config for evaluate now uses `upstream_retry_attempts = 2`
- evaluate logs now print a compact diagnostics block from `result.json`:
  - `failure_reasons`
  - `threshold_checks`
  - `observed`
- in proxy kernel, single-endpoint upstreams now allow bounded same-endpoint retry for transient upstream I/O only; protocol errors are excluded

### G3.1.4 failure-drill diagnostics and pass-through fix

- single-endpoint retryable upstream status (`500/502/503/504`) now keeps pass-through semantics and avoids gateway-5xx contamination from exclusion fallback
- multi-endpoint retryable-status retry behavior remains unchanged
- failure-drill result contract now exposes additional audit fields:
  - `business_total_requests`
  - `business_gateway_5xx_ratio`
  - `business_network_errors`
  - `gateway_fault_breakdown`
  - `gateway_fault_network_errors_total`
- evaluate diagnostics log now appends a focused `failure_drill` summary block for direct CI troubleshooting

### Nightly integration (G3.4)

`nightly-benchmark.yml` now runs:

- Linux x86_64 + arm64 package build (`tar.gz`)
- Linux x86_64 + arm64 `public-api-gate --mode evaluate --profile observe`
- calibration summary aggregation (`calibration-report.*`)
- CI/release streak reporting (`streak-report.*`)

## 中文

本文档定义溪流网关在 Linux `x86_64` 与 `arm64` 上的公网 API 门禁基线压测契约。

### 核心命令

```bash
bash ./scripts/linux-public-api-gate.sh \
  --mode evaluate \
  --profile standard \
  --arch linux-x86_64 \
  --artifact ./dist/<target>/rivulet-gateway-<version>.tar.gz \
  --format tar.gz \
  --output-dir ./target/public-api-gate/manual-evaluate
```

arm64 只需改为：

```bash
--arch linux-arm64
```

### 支持模式

- `schema-check`：阈值结构与 profile-arch 契约校验
- `baseline`：稳态样本
- `soak`：长稳态样本
- `failure-drill`：上游故障与恢复演练
- `evaluate`：串行执行 baseline + soak + failure-drill 并给出最终门禁结论

### standard 阻断档

`standard` 现已升级为按架构阈值，并且在 CI 与 release 同口径阻断。

每个架构都包含以下阈值：

- 可用性下限
- 网关侧 `5xx` 比例上限
- p95 / p99 延迟上限
- 最小样本门槛：`MIN_TOTAL_REQUESTS_BASELINE` 与 `MIN_TOTAL_REQUESTS_SOAK`

### 抗抖动判分

- baseline/soak 均执行 3 次采样
- 使用中位数判分
- 请求量门槛必须通过，否则即使延迟与错误率达标也判定失败

### 长跑窗口（默认）

- `baseline`：3 轮 × 20 秒
- `soak`：3 轮 × 60 秒
- `failure-drill`：每个故障子场景 12 秒

### 产出契约

每次运行固定输出：

- `result.json`（机器可读）
- `summary.md`（人工可读）

`result.json` 新增字段：

- `arch`
- `duration_profile`（当前为 `long`）
- `request_floor_checks`
- 明确的 `failure_reasons`

### G3.4 校准报告链路

nightly 现已接入按架构 observe 样本采集与校准汇总。

校准脚本：

```bash
bash ./scripts/public-api-calibration-report.sh \
  --profile standard \
  --output-dir ./target/public-api-calibration/manual \
  --inputs ./result-x86.json ./result-arm.json
```

输出：

- `calibration-report.json`
- `calibration-report.md`

报告固定包含：

- `availability/gateway_5xx_ratio/p95/p99` 的按架构分布摘要（`min/median/p90/p95/max`）
- 与当前 `standard` 阈值的差值
- 样本量与最近趋势记录
- 人工评审建议项（不自动回写阈值）

### G3.4 连续全绿统计链路

连续全绿统计脚本（仅报告，不阻断）：

```bash
bash ./scripts/public-api-gate-streak-report.sh \
  --repo <owner/name> \
  --workflow ci \
  --window 40 \
  --output-dir ./target/public-api-streak/ci
```

release 口径只需改为：

```bash
--workflow release
```

输出：

- `streak-report.json`
- `streak-report.md`

收口指标用于跟踪“连续 10 次双架构全绿”是否达成。

### 停止边界与防挂死诊断

- 门禁运行时已强制后台进程有界停止：
  - 先 `TERM` + 有限等待
  - 超时后自动 `KILL`
  - 仍无法退出则显式失败
- `result.json` 新增 `process_shutdown` 诊断字段（gateway 与 fixture backend）
- `summary.md` 追加进程停止结果与耗时秒数
- 与 `linux-systemd-validate.sh` 配合时，关键 `systemctl` 操作带命令级超时保护
- CI/release 侧同时有 workflow 级超时兜底，避免无限挂死

### 日志通道与 evaluate 路径安全

- stage 进度日志统一写入 `stderr`，不再混入机器读取的 stdout 返回值
- evaluate 模式的 summary 文件在脚本函数间改为显式路径传递
- 预期防线：
  - 评分阶段不会再把 stage 文本拼接进 JSON 文件路径

### G3.1.3 可靠性说明（standard 档不降级）

- `standard` 阈值保持不变，本批不放宽 SLO 判分标准
- evaluate 门禁工况配置 `upstream_retry_attempts = 2`
- evaluate 结束后会从 `result.json` 输出精简诊断块：
  - `failure_reasons`
  - `threshold_checks`
  - `observed`
- 代理内核新增单节点有界同节点重试，仅适用于上游瞬态 I/O；协议错误不进入该重试路径

### G3.1.4 failure-drill 诊断增强与透传修复

- 单节点 retryable 上游状态（`500/502/503/504`）恢复“业务状态优先透传”语义，避免因排除唯一节点导致的网关 5xx 污染
- 多节点场景对 retryable 状态的重试行为保持不变
- failure-drill 产物新增审计字段：
  - `business_total_requests`
  - `business_gateway_5xx_ratio`
  - `business_network_errors`
  - `gateway_fault_breakdown`
  - `gateway_fault_network_errors_total`
- evaluate 诊断日志追加 `failure_drill` 摘要块，便于在 CI 直接定位失败原因

### G3.4 nightly 接线

`nightly-benchmark.yml` 现已执行：

- Linux x86_64 + arm64 打包（`tar.gz`）
- Linux x86_64 + arm64 `public-api-gate --mode evaluate --profile observe`
- 校准汇总报告（`calibration-report.*`）
- CI/release 连续全绿统计报告（`streak-report.*`）
