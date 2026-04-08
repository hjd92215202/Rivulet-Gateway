# Linux Baseline Benchmark / Linux 基线压测

## English

This document defines the baseline and public API gate benchmark flow for Linux `x86_64` and `arm64`.

### 1. Baseline Report Script

Run the conservative baseline script on a Linux server:

```bash
bash ./scripts/linux-baseline-report.sh \
  --url http://127.0.0.1:8080/ngx/ \
  --host llmtamer.com:8080 \
  --duration 15 \
  --label llmtamer-loopback
```

Report output:

```text
./target/server-bench/<timestamp>-<label>/report.md
```

### 2. Public API Gate Script

Use the gate script for production SLO scoring and release blocking:

```bash
bash ./scripts/linux-public-api-gate.sh \
  --mode evaluate \
  --profile standard \
  --artifact ./dist/<target>/rivulet-gateway-<version>.tar.gz \
  --format tar.gz \
  --output-dir ./target/public-api-gate/manual-evaluate
```

Supported modes:

- `schema-check`: validate threshold schema only
- `baseline`: steady-state scenario
- `soak`: longer steady-state scenario
- `failure-drill`: upstream fault and recovery scenario
- `evaluate`: run all scenarios and produce final gate decision

Supported profiles:

- `standard` (default, blocking)
- `strict` (blocking, tighter thresholds)
- `observe` (non-blocking, report-first)

### 3. Standard SLO Profile (Blocking)

Current `standard` threshold contract:

- availability `>= 99.9`
- gateway-originated `5xx` ratio `<= 0.1`
- `p95 <= 80 ms`
- `p99 <= 200 ms`

Notes:

- gateway `5xx` and upstream business `5xx` are counted separately
- only gateway-originated faults are used by the gateway-fault threshold
- failure drill must pass recovery checks

### 4. Output Contract

Every mode writes:

- machine-readable `result.json`
- human-readable `summary.md`

For `evaluate`, these files are the audit source for CI/release gate decisions.

### 5. Operational Rules

- scripts should auto-install dependencies (`apt-get`, `dnf`, `yum` when available)
- scripts should auto-clean background processes and exit automatically
- run progressively; avoid unsafe sudden load spikes

## 中文

本文档定义溪流网关在 Linux `x86_64` 和 `arm64` 上的基线压测与公网 API 门禁压测流程。

### 1. 基线报告脚本

在 Linux 服务器执行保守基线脚本：

```bash
bash ./scripts/linux-baseline-report.sh \
  --url http://127.0.0.1:8080/ngx/ \
  --host llmtamer.com:8080 \
  --duration 15 \
  --label llmtamer-loopback
```

报告输出位置：

```text
./target/server-bench/<timestamp>-<label>/report.md
```

### 2. 公网 API 门禁脚本

使用门禁脚本执行生产口径 SLO 判分与发版阻断：

```bash
bash ./scripts/linux-public-api-gate.sh \
  --mode evaluate \
  --profile standard \
  --artifact ./dist/<target>/rivulet-gateway-<version>.tar.gz \
  --format tar.gz \
  --output-dir ./target/public-api-gate/manual-evaluate
```

支持模式：

- `schema-check`：仅校验阈值配置结构
- `baseline`：稳态场景
- `soak`：长稳态场景
- `failure-drill`：上游故障与恢复演练场景
- `evaluate`：串行执行全部场景并给出最终门禁结论

支持档位：

- `standard`（默认，阻断）
- `strict`（阻断，更严格）
- `observe`（不阻断，报告优先）

### 3. 标准档 SLO（阻断口径）

当前 `standard` 阈值契约：

- 可用性 `>= 99.9`
- 网关侧 `5xx` 比例 `<= 0.1`
- `p95 <= 80 ms`
- `p99 <= 200 ms`

说明：

- 网关侧 `5xx` 与上游业务 `5xx` 分开统计
- 门禁阈值只使用网关侧故障比例
- 故障演练场景必须通过恢复检查

### 4. 产出契约

每个模式都会输出：

- 机器可读 `result.json`
- 人类可读 `summary.md`

其中 `evaluate` 产物会作为 CI/release 门禁与审计回溯依据。

### 5. 运行规则

- 脚本在可用时自动安装依赖（`apt-get`、`dnf`、`yum`）
- 脚本自动清理后台进程并自动退出
- 压测需循序渐进，避免突发高压导致主机不稳定
