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
