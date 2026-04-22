# Release Checklist / 发布检查清单

## English

Release target: `v<version>`

### Version

- [ ] `Cargo.toml` workspace version is correct for ongoing development.
- [ ] The release tag is the intended public version.
- [ ] Release asset filenames match the tag version.
- [ ] No unintended local changes remain.

### Local Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] Windows package build completed
- [ ] Windows smoke test passed

### GitHub Workflow Expectations

- [ ] CI workflow is green on Linux x86_64
- [ ] CI workflow is green on Windows x86_64
- [ ] CI workflow is green on Linux arm64
- [ ] CI systemd lifecycle gate is green on Linux x86_64 and Linux arm64
- [ ] Package jobs upload expected artifacts
- [ ] Release systemd lifecycle gate is green on Linux x86_64 and Linux arm64
- [ ] Release disposable lifecycle gate is green on Linux x86_64 and Linux arm64
- [ ] Release disposable lifecycle tar.gz path passes full lifecycle (`install -> upgrade -> rollback -> uninstall`)
- [ ] Release disposable lifecycle rpm path passes install/uninstall lifecycle
- [ ] Release workflow creates `SHA256SUMS.txt`
- [ ] Release workflow creates `SHA256SUMS.sig` and `SHA256SUMS.pem`
- [ ] Release workflow creates detached signature pairs for every release asset (`<asset>.sig` + `<asset>.pem`)
- [ ] Release workflow blocks publish when detached signature verification fails
- [ ] Release workflow uploads `SBOM.spdx.json`
- [ ] Supply-chain provenance attestation job succeeds
- [ ] GitHub Release page is created

### M2 Sampling Mode Checks (Release)

- [ ] `release.yml` supports `run_mode=full|m2-sampling`
- [ ] Scheduled release sampling trigger is enabled (`30 2,12 * * *` UTC)
- [ ] `m2-sampling` runs only script-standards + Linux package + release public-api gate x86_64/arm64
- [ ] `m2-sampling` does not execute windows/systemd/disposable/publish jobs
- [ ] Release public-api gate artifacts include `sampling-metadata.json`
- [ ] `sampling-metadata.json` contains run_mode/event_name/sampling_label/run_id/arch

### M2 Sampling Mode Checks (CI)

- [ ] `ci.yml` supports `run_mode=full|m2-sampling`
- [ ] Scheduled CI sampling trigger is enabled (`30 2,12 * * *` UTC)
- [ ] `m2-sampling` runs only script-standards + Linux package + public-api gate x86_64/arm64
- [ ] `m2-sampling` does not execute windows/systemd/disposable-contract/supply-chain jobs
- [ ] CI public-api gate artifacts include `sampling-metadata.json`
- [ ] `sampling-metadata.json` contains run_mode/event_name/sampling_label/run_id/arch

### Release Assets

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`
- [ ] `SHA256SUMS.sig`
- [ ] `SHA256SUMS.pem`
- [ ] Each published asset has matching `.sig` and `.pem` files
- [ ] `SBOM.spdx.json`

### Post-Release Notes

- [ ] Document any known limits
- [ ] Record failed or skipped validations
- [ ] Update roadmap follow-up items

### M2 Threshold Tuning PR Checklist

- [ ] Nightly evidence reviewed in order: calibration -> proposal -> checklist -> weekly
- [ ] Change scope only touches `PUBLIC_API_STANDARD_<ARCH>_*` keys
- [ ] One-step change budget per key stays within `<=10%`
- [ ] PR description includes nightly artifact links/references
- [ ] PR description explicitly declares `ready_for_threshold_pr=true`
- [ ] Bilingual docs updated in same PR (`LINUX-BASELINE-BENCHMARK`, `PRODUCTION-READINESS`, `MILESTONES` minimum)
- [ ] Rollback trigger and rollback command path are documented

### M2 Closure Sprint Nightly Checklist

- [ ] `m2-nightly-review-package.md` is present and reviewed first
- [ ] Package indicates `ready_for_threshold_pr=true` before any threshold PR is opened
- [ ] `m2-cutover-check.json` is reviewed as final closure gate
- [ ] If `cutover_ready=false`, no M2->M3 docs cutover is attempted
- [ ] If `cutover_ready=true`, M2 completion + M3 kickoff docs update is prepared as isolated commit
- [ ] Streak evidence confirms `eligible_runs` excludes `skipped/missing` gate jobs (treated as `not_eligible`)
- [ ] `recent_failure_reasons` and `recent_ineligible_reasons` are reviewed separately before closure decisions
- [ ] Manual nightly sampling runs are executed and tagged with slot/label metadata (`am-1030` / `pm-2030`)
- [ ] If `rollback_recommended=true`, latest threshold PR is rolled back before any new threshold tuning PR
- [ ] Confirm this batch is sampling acceleration and reporting hardening, not threshold relaxation

### Recommended Linux Host Validation

- [ ] `bash ./scripts/linux-postinstall-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-disposable-lifecycle-gate.sh --mode execute --lifecycle-mode full --from-tag <previous> --to-tag <version> --arch <linux-x86_64|linux-arm64> --host <host>`

## 中文

目标版本：`v<version>`

### 版本检查

- [ ] `Cargo.toml` 中的 workspace version 对开发主线仍然正确
- [ ] release tag 就是本次要公开的版本号
- [ ] release assets 文件名与 tag 版本一致
- [ ] 没有意外的本地未提交改动

### 本地验证

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] Windows 打包完成
- [ ] Windows smoke test 通过

### GitHub 工作流预期

- [ ] Linux x86_64 CI 通过
- [ ] Windows x86_64 CI 通过
- [ ] Linux arm64 CI 通过
- [ ] CI 中 Linux x86_64 和 Linux arm64 的 systemd 生命周期门禁通过
- [ ] package job 上传了预期产物
- [ ] release workflow 中 Linux x86_64 和 Linux arm64 的 systemd 生命周期门禁通过
- [ ] release workflow 中 Linux x86_64 和 Linux arm64 的一次性环境全生命周期门禁通过
- [ ] 一次性环境全生命周期门禁的 tar.gz 路径通过全流程（`安装 -> 升级 -> 回滚 -> 卸载`）
- [ ] 一次性环境全生命周期门禁的 rpm 路径通过安装/卸载流程
- [ ] release workflow 生成了 `SHA256SUMS.txt`
- [ ] release workflow 生成了 `SHA256SUMS.sig` 和 `SHA256SUMS.pem`
- [ ] release workflow 为每个发布资产生成 detached 签名对（`<asset>.sig` + `<asset>.pem`）
- [ ] detached 验签失败时 release workflow 会阻断 publish
- [ ] release workflow 上传了 `SBOM.spdx.json`
- [ ] supply-chain provenance 证明步骤通过
- [ ] GitHub Release 页面已创建

### M2 采样模式检查（Release）

- [ ] `release.yml` 支持 `run_mode=full|m2-sampling`
- [ ] 已启用 release 定时采样触发（`30 2,12 * * *` UTC）
- [ ] `m2-sampling` 仅执行 script-standards + Linux 打包 + release public-api gate x86_64/arm64
- [ ] `m2-sampling` 不执行 windows/systemd/disposable/publish
- [ ] release public-api gate 产物包含 `sampling-metadata.json`
- [ ] `sampling-metadata.json` 包含 run_mode/event_name/sampling_label/run_id/arch 字段

### M2 采样模式检查（CI）

- [ ] `ci.yml` 支持 `run_mode=full|m2-sampling`
- [ ] 已启用 CI 定时采样触发（`30 2,12 * * *` UTC）
- [ ] `m2-sampling` 仅执行 script-standards + Linux 打包 + public-api gate x86_64/arm64
- [ ] `m2-sampling` 不执行 windows/systemd/disposable-contract/supply-chain
- [ ] CI public-api gate 产物包含 `sampling-metadata.json`
- [ ] `sampling-metadata.json` 包含 run_mode/event_name/sampling_label/run_id/arch 字段

### 发布产物

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`
- [ ] `SHA256SUMS.sig`
- [ ] `SHA256SUMS.pem`
- [ ] 每个发布资产都带有对应的 `.sig` 和 `.pem`
- [ ] `SBOM.spdx.json`

### 发布后记录

- [ ] 记录已知边界
- [ ] 记录失败或跳过的验证
- [ ] 更新路线图后续事项

### M2 阈值调优 PR 清单

- [ ] 已按顺序审阅 nightly 证据：`calibration -> proposal -> checklist -> weekly`
- [ ] 变更范围仅包含 `PUBLIC_API_STANDARD_<ARCH>_*` 键
- [ ] 每个键的单次调整幅度不超过 `<=10%`
- [ ] PR 描述已附带 nightly 产物链接/引用
- [ ] PR 描述已显式声明 `ready_for_threshold_pr=true`
- [ ] 同一 PR 完成中英文文档同步（至少 `LINUX-BASELINE-BENCHMARK`、`PRODUCTION-READINESS`、`MILESTONES`）
- [ ] 已写明回滚触发条件与回滚执行路径

### M2 收口冲刺 Nightly 清单

- [ ] `m2-nightly-review-package.md` 已生成并作为首个审阅入口
- [ ] 在 `ready_for_threshold_pr=true` 之前不发起阈值 PR
- [ ] `m2-cutover-check.json` 已作为收口最终门槛审阅
- [ ] 若 `cutover_ready=false`，不执行 M2->M3 文档切线
- [ ] 若 `cutover_ready=true`，以独立提交完成 M2 完成标记与 M3 启动文档更新
- [ ] 已确认 streak 的 `eligible_runs` 不包含 `skipped/missing` gate job（按 `not_eligible` 忽略）
- [ ] 已分别审阅 `recent_failure_reasons` 与 `recent_ineligible_reasons`，避免噪声误判为 gate 回归
- [ ] 已执行手动 nightly 补采样并记录 slot/label 元数据（`am-1030` / `pm-2030`）
- [ ] 若 `rollback_recommended=true`，已先回滚最近阈值 PR，再评估新的阈值调整
- [ ] 已确认本批是采样加速与证据链加固，而非阈值放宽

### 推荐 Linux 主机验证

- [ ] `bash ./scripts/linux-postinstall-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-disposable-lifecycle-gate.sh --mode execute --lifecycle-mode full --from-tag <previous> --to-tag <version> --arch <linux-x86_64|linux-arm64> --host <host>`
