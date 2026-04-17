# Release Process / 发布流程

## English

This document defines the formal release process for `Rivulet Gateway / 溪流网关`.

### Source Of Truth

- Development builds fall back to `Cargo.toml` under `[workspace.package].version`.
- Release tags use the format `v<version>`.
- Tag-triggered release packaging uses the Git tag as the asset version source and strips the leading `v`.
- Package names, RPM versions, and uploaded GitHub Release assets must match the release tag version.

### Release Trigger Model

- Regular `push` and `pull_request` events run `.github/workflows/ci.yml`.
- Nightly benchmark collection runs `.github/workflows/nightly-benchmark.yml`.
- Pushing a tag such as `v0.1.5` runs `.github/workflows/release.yml`.
- The release workflow builds Linux x86_64, Linux arm64, and Windows x86_64 artifacts.
- CI and release workflows both run Linux x86_64 and Linux arm64 systemd lifecycle gates on packaged artifacts.
- Release workflow now also runs disposable lifecycle gates on Linux x86_64 and Linux arm64:
  - `tar.gz`: `install -> upgrade -> rollback -> uninstall`
  - `rpm`: `install -> uninstall`
- The publish job creates or updates the GitHub Release and uploads packaged assets, `SHA256SUMS.txt`, detached per-asset signatures/certificates, compatibility checksum signature files, and SBOM.
- CI and release workflows also generate supply-chain provenance attestations for built artifacts.

### Release Steps

1. Confirm the intended public version, for example `v0.1.5`.
2. Confirm the branch already contains the required CI/CD, packaging, and documentation changes.
3. Run local pre-release validation.
4. Commit any final release-prep changes with a clear `feat:` or `docs:` message.
5. Create an annotated tag such as `git tag -a v0.1.5 -m "Rivulet Gateway v0.1.5"`.
6. Push the branch and the tag to GitHub.
7. Monitor `.github/workflows/release.yml` until all packaging jobs, systemd lifecycle gates, disposable lifecycle gates, and the publish job succeed.
8. Check that every published release asset has detached signature and certificate pairs (`<asset>.sig` and `<asset>.pem`), and confirm compatibility files `SHA256SUMS.sig` and `SHA256SUMS.pem` are present.
9. Check that the GitHub Release page contains `zip`, `tar.gz`, `rpm`, `SHA256SUMS.txt`, detached signature/certificate files, and `SBOM.spdx.json`, and that every asset filename matches the tag version.
10. Record remaining risks and post-release follow-up items in the roadmap.

### M2 Threshold Tuning Review Flow

For M2 closure period, threshold tuning must follow this strict sequence and remain report-driven:

1. Review nightly artifacts in order:
   - `calibration-report.json`
   - `threshold-change-proposal.md`
   - `threshold-pr-checklist.md`
   - `closure-weekly-report.md`
2. Open a threshold PR only when evidence maturity is ready and changes stay within one-step budget (`<=10%`).
3. Threshold PR must change only `PUBLIC_API_STANDARD_<ARCH>_*` keys in `scripts/public-api-thresholds.env`.
4. Threshold PR description must include links or references to nightly evidence artifacts.
5. Threshold PR description must explicitly declare `ready_for_threshold_pr=true` (evidence maturity satisfied).
6. If evidence is not mature, do not open threshold PR; continue nightly observe sampling only.
7. Update bilingual docs in the same threshold PR (`LINUX-BASELINE-BENCHMARK`, `PRODUCTION-READINESS`, `MILESTONES` at minimum).
8. If post-merge CI/release dual-arch gate shows consecutive regressions, revert threshold PR immediately.

### M2 Nightly Operating Loop (Closure Sprint)

During M2 closure sprint, operate nightly outputs with one fixed routine:

1. Read `m2-nightly-review-package.md` first as the single entry.
2. Follow the mandatory evidence order from the package:
   - `calibration-report.json`
   - `threshold-change-proposal.md`
   - `threshold-pr-checklist.md`
   - `closure-weekly-report.md`
   - `milestone2-closure-status.json`
3. If `ready_for_threshold_pr` is false, keep observe-only sampling and do not open threshold PR.
4. If threshold PR is opened, keep changes limited to `PUBLIC_API_STANDARD_<ARCH>_*` with one-step budget (`<=10%`).
5. Use `m2-cutover-check.json` as final cutover gate:
   - `cutover_ready=true` -> execute M2->M3 docs cutover
   - otherwise continue M2 sampling and review loop
6. Treat streak eligibility strictly:
   - only runs with both required Public API Gate jobs present and successful are counted as valid streak samples
   - runs where gate jobs are `skipped` or `missing` are `not_eligible` noise, not gate regression
7. Distinguish diagnostics in closure artifacts:
   - `recent_failure_reasons` is gate-failure-only (eligible runs)
   - `recent_ineligible_reasons` is skip/missing noise aggregation

### Local Pre-Release Validation

Minimum local checks before pushing a release tag:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- Windows package build
- Windows packaged binary smoke test

Recommended Linux host checks before stronger production claims:

- `bash ./scripts/linux-postinstall-validate.sh --tag <version> --host <host:port>`
- `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`
- `bash ./scripts/linux-disposable-lifecycle-gate.sh --mode execute --lifecycle-mode full --from-tag <previous> --to-tag <version> --arch <linux-x86_64|linux-arm64> --host <host>`

Recommended release-local commands on the current Windows development host:

```powershell
cargo fmt --all -- --check
cargo test --workspace
.\scripts\package.ps1 -Target x86_64-pc-windows-msvc -Format zip -SkipBuild
.\packaging\tests\server-smoke.ps1 -Binary .\dist\x86_64-pc-windows-msvc\rivulet-gateway-<version>\gateway.exe -Port 18083
```

### Artifact Expectations

Expected release outputs:

- Windows x86_64: `.zip`
- Linux x86_64: `.tar.gz` and `.rpm`
- Linux arm64: `.tar.gz` and `.rpm`
- `SHA256SUMS.txt`
- detached per-asset signatures and certificates (`<asset>.sig`, `<asset>.pem`)
- compatibility checksum signature files (`SHA256SUMS.sig`, `SHA256SUMS.pem`)
- `SBOM.spdx.json`

### Current Release Boundaries

The release process now includes keyless detached per-asset signing, checksum compatibility signatures, SBOM export, provenance attestation, and disposable lifecycle blocking gates. Remaining limits are:

- lifecycle gates still run on shared GitHub-hosted runners, and threshold/throughput tuning still needs ongoing calibration.
- broader public Internet readiness still excludes HTTP/2, mTLS, and WebSocket.

## 中文

本文定义 `Rivulet Gateway / 溪流网关` 的正式发布流程。

### 版本真源

- 日常开发构建默认回落到 `Cargo.toml` 中 `[workspace.package].version`。
- 发布 tag 统一采用 `v<version>` 格式。
- tag 触发的发布打包会把 Git tag 作为产物版本来源，并自动去掉前缀 `v`。
- 包名、RPM 版本号以及上传到 GitHub Release 的 assets 文件名必须与 tag 版本一致。

### 触发模型

- 常规 `push` 和 `pull_request` 触发 `.github/workflows/ci.yml`。
- 夜间基线压测触发 `.github/workflows/nightly-benchmark.yml`。
- 推送 `v0.1.5` 这类 tag 会触发 `.github/workflows/release.yml`。
- release workflow 会构建 Linux x86_64、Linux arm64、Windows x86_64 三类产物。
- CI 与 release workflow 都会在打包产物上执行 Linux x86_64 与 Linux arm64 的 systemd 生命周期门禁验证。
- release workflow 现已新增 Linux x86_64 与 Linux arm64 的一次性环境全生命周期门禁：
  - `tar.gz`：`安装 -> 升级 -> 回滚 -> 卸载`
  - `rpm`：`安装 -> 卸载`
- publish job 会创建或更新 GitHub Release，并上传打包产物、`SHA256SUMS.txt`、逐产物 detached 签名/证书、兼容清单签名文件和 SBOM。
- CI 与 release workflow 还会为构建产物生成 supply-chain provenance 证明。

### 发布步骤

1. 确认本次公开版本号，例如 `v0.1.5`。
2. 确认当前分支已经包含所需的 CI/CD、打包和文档变更。
3. 执行本地发布前验证。
4. 用明确的 `feat:` 或 `docs:` 提交最后的发布准备改动。
5. 创建注解 tag，例如 `git tag -a v0.1.5 -m "Rivulet Gateway v0.1.5"`。
6. 将分支和 tag 一起推送到 GitHub。
7. 观察 `.github/workflows/release.yml`，直到所有打包 job、systemd 生命周期门禁 job、一次性环境全生命周期门禁 job 和 publish job 成功。
8. 检查每个发布资产是否都包含 detached 签名与证书对（`<asset>.sig` 与 `<asset>.pem`），并确认兼容文件 `SHA256SUMS.sig` 与 `SHA256SUMS.pem` 存在。
9. 检查 GitHub Release 页面是否包含 `zip`、`tar.gz`、`rpm`、`SHA256SUMS.txt`、逐产物签名文件、`SBOM.spdx.json`，并确认所有产物文件名与 tag 版本一致。
10. 把剩余风险和后续事项回填到路线图。

### 本地发布前验证

推送 release tag 前，最低要求的本地检查：

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- Windows 打包构建
- Windows 打包后二进制 smoke test

对外声明更强生产可用前，建议补充 Linux 主机验证：

- `bash ./scripts/linux-postinstall-validate.sh --tag <version> --host <host:port>`
- `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`
- `bash ./scripts/linux-disposable-lifecycle-gate.sh --mode execute --lifecycle-mode full --from-tag <previous> --to-tag <version> --arch <linux-x86_64|linux-arm64> --host <host>`

当前 Windows 开发机推荐命令：

```powershell
cargo fmt --all -- --check
cargo test --workspace
.\scripts\package.ps1 -Target x86_64-pc-windows-msvc -Format zip -SkipBuild
.\packaging\tests\server-smoke.ps1 -Binary .\dist\x86_64-pc-windows-msvc\rivulet-gateway-<version>\gateway.exe -Port 18083
```

### 预期产物

预期发布输出：

- Windows x86_64：`.zip`
- Linux x86_64：`.tar.gz` 和 `.rpm`
- Linux arm64：`.tar.gz` 和 `.rpm`
- `SHA256SUMS.txt`
- 逐产物 detached 签名与证书（`<asset>.sig`、`<asset>.pem`）
- 兼容清单签名文件（`SHA256SUMS.sig`、`SHA256SUMS.pem`）
- `SBOM.spdx.json`

### 当前发布边界

当前发布流程已接入 keyless 逐产物 detached 签名、兼容清单签名、SBOM 和 provenance，但仍有明显边界：

- 一次性环境全生命周期门禁已接入 release 阻断，但当前仍依赖 GitHub 共享 runner，阈值与吞吐口径需要持续校准。
- 更大规模公网能力声明仍不包含 HTTP/2、mTLS、WebSocket。

### M2 阈值评审流程（收口阶段）

在 Milestone 2 收口阶段，阈值调整必须按“先报告、后评审、再小步提交”执行：

1. 按顺序审阅 nightly 产物：
   - `calibration-report.json`
   - `threshold-change-proposal.md`
   - `threshold-pr-checklist.md`
   - `closure-weekly-report.md`
2. 仅在证据成熟度达标时发起阈值 PR，且单次变更预算不超过 `10%`。
3. 阈值 PR 只允许修改 `scripts/public-api-thresholds.env` 中 `PUBLIC_API_STANDARD_<ARCH>_*` 键。
4. PR 描述必须附 nightly 证据链接或可追溯引用。
5. PR 描述必须显式声明 `ready_for_threshold_pr=true`（证据成熟）。
6. 若证据未成熟，禁止发起阈值 PR，仅继续 nightly observe 采样。
7. 同一 PR 必须同步更新中英文文档（至少包含 `LINUX-BASELINE-BENCHMARK`、`PRODUCTION-READINESS`、`MILESTONES`）。
8. 合并后若出现连续回归，按清单要求立即回滚阈值 PR。

### M2 夜间运营闭环（收口冲刺）

在 M2 收口冲刺阶段，nightly 产物按固定闭环执行：

1. 先读 `m2-nightly-review-package.md`，作为唯一入口。
2. 严格按包内固定顺序审阅证据：
   - `calibration-report.json`
   - `threshold-change-proposal.md`
   - `threshold-pr-checklist.md`
   - `closure-weekly-report.md`
   - `milestone2-closure-status.json`
3. 若 `ready_for_threshold_pr=false`，只允许继续 observe 采样，禁止发起阈值 PR。
4. 若发起阈值 PR，变更必须仅限 `PUBLIC_API_STANDARD_<ARCH>_*` 且单步预算不超过 `10%`。
5. 以 `m2-cutover-check.json` 作为切线最终门槛：
   - `cutover_ready=true`：执行 M2->M3 文档切线；
   - 否则继续 M2 采样与评审闭环。
6. 连绿统计口径必须严格按“有效样本”执行：
   - 仅当两条 required Public API Gate job 均 `present=true` 且 `success` 才计入 streak；
   - gate job `skipped` 或 `missing` 归类为 `not_eligible` 噪声，不计成功也不计失败。
7. 收口诊断字段必须分离解读：
   - `recent_failure_reasons` 仅统计有效样本中的真实 gate 失败；
   - `recent_ineligible_reasons` 单独统计 skip/missing 噪声原因。
