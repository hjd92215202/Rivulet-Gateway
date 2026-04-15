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
- [ ] Bilingual docs updated in same PR (`LINUX-BASELINE-BENCHMARK`, `PRODUCTION-READINESS`, `MILESTONES` minimum)
- [ ] Rollback trigger and rollback command path are documented

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
- [ ] 同一 PR 完成中英文文档同步（至少 `LINUX-BASELINE-BENCHMARK`、`PRODUCTION-READINESS`、`MILESTONES`）
- [ ] 已写明回滚触发条件与回滚执行路径

### 推荐 Linux 主机验证

- [ ] `bash ./scripts/linux-postinstall-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-disposable-lifecycle-gate.sh --mode execute --lifecycle-mode full --from-tag <previous> --to-tag <version> --arch <linux-x86_64|linux-arm64> --host <host>`
