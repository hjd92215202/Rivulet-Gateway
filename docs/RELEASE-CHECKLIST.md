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
- [ ] Package jobs upload expected artifacts
- [ ] Release workflow creates `SHA256SUMS.txt`
- [ ] GitHub Release page is created

### Release Assets

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`

### Post-Release Notes

- [ ] Document any known limits
- [ ] Record failed or skipped validations
- [ ] Update roadmap follow-up items

### Recommended Linux Host Validation

- [ ] `bash ./scripts/linux-systemd-validate.sh --tag <version> --host <host:port>`
- [ ] `bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag <previous> --to-tag <version> --host <host:port>`

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
- [ ] package job 上传了预期产物
- [ ] release workflow 生成了 `SHA256SUMS.txt`
- [ ] GitHub Release 页面已创建

### 发布产物

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`

### 发布后记录

- [ ] 记录已知边界
- [ ] 记录失败或跳过的验证
- [ ] 更新路线图后续事项
