# Workflow Architecture / 工作流架构

## English

Repository workflows:

- `ci`
- `release`
- `nightly-benchmark`

### Trigger model

- `ci`: branch `push`, `pull_request`, `workflow_dispatch`
- `release`: tag `v*` push, `workflow_dispatch`
- `nightly-benchmark`: schedule + manual dispatch

### G3.1 blocking relation

```mermaid
flowchart TD
    A["Code Push / PR"] --> CI["ci workflow"]
    B["Tag v*"] --> REL["release workflow"]
    C["Schedule"] --> NB["nightly-benchmark workflow"]

    CI --> CI_STD["script-standards + schema-check"]
    CI_STD --> CI_PX["package linux x86_64"]
    CI_STD --> CI_PA["package linux arm64"]
    CI_PX --> CI_GX["public-api-gate x86_64 evaluate standard arch-aware"]
    CI_PA --> CI_GA["public-api-gate arm64 evaluate standard arch-aware"]
    CI_GX --> CI_SC["supply-chain"]
    CI_GA --> CI_SC

    REL --> REL_STD["script-standards + schema-check"]
    REL_STD --> REL_PX["package linux x86_64 release"]
    REL_STD --> REL_PA["package linux arm64 release"]
    REL_PX --> REL_GX["public-api-gate x86_64 release evaluate standard arch-aware"]
    REL_PA --> REL_GA["public-api-gate arm64 release evaluate standard arch-aware"]
    REL_GX --> REL_PUB["publish github release"]
    REL_GA --> REL_PUB

    NB --> NB_RUN["benchmark collection"]
```

### Blocking policy

- CI and release both use `--profile standard`.
- Both Linux architectures are mandatory.
- Any public-api gate failure blocks downstream publish chain (`supply-chain` in CI, `publish` in release).
- Gate jobs now run with explicit timeout and long-run evaluate windows.
- `script-standards` now includes fixture backend SIGTERM termination regression check.
- `systemd-lifecycle-linux-x86_64` and `systemd-lifecycle-linux-arm64` now run with workflow-level timeout guards.

### Audit artifacts

Public API gate jobs upload:

- `result.json`
- `summary.md`

These artifacts include architecture, duration profile, request-floor checks, and explicit failure reasons.

### CI hang containment boundaries

- fixture process layer:
  - bounded stop contract (`TERM` -> bounded wait -> `KILL` -> explicit fail)
- script execution layer:
  - key `systemctl` calls wrapped with command timeout in Linux systemd validation
- workflow orchestration layer:
  - explicit `timeout-minutes` on long-running systemd lifecycle jobs
- diagnosis expectation:
  - failures should surface as actionable diagnostics, not silent 6h hangs

## 中文

仓库工作流包括：

- `ci`
- `release`
- `nightly-benchmark`

### 触发模型

- `ci`：分支 `push`、`pull_request`、`workflow_dispatch`
- `release`：`v*` tag 推送、`workflow_dispatch`
- `nightly-benchmark`：定时 + 手动触发

### G3.1 阻断关系

- CI 与 release 均采用 `--profile standard`。
- `x86_64` 与 `arm64` 两条公网 API 门禁必须同时通过。
- 任一门禁失败即阻断后续链路：
  - CI 阻断 `supply-chain`
  - release 阻断 `publish`
- 门禁 job 已接入明确超时与长跑 evaluate 窗口。
- `script-standards` 已加入 fixture backend 的 SIGTERM 退出回归校验。
- `systemd-lifecycle-linux-x86_64` 与 `systemd-lifecycle-linux-arm64` 已增加 workflow 级超时保护。

### 审计产物

公网 API 门禁 job 固定上传：

- `result.json`
- `summary.md`

产物中包含架构、时长档位、最小样本门槛检查与明确失败原因，便于回溯审计。

### CI 卡死治理边界

- fixture 进程层：
  - 有界停止契约（`TERM` -> 有限等待 -> `KILL` -> 显式失败）
- 脚本执行层：
  - Linux systemd 验证中的关键 `systemctl` 调用都带命令级超时
- workflow 编排层：
  - 长耗时 systemd lifecycle job 配置 `timeout-minutes`
- 诊断目标：
  - 失败必须可解释、可复盘，不再出现静默 6 小时挂死
