# Release Checklist / 发布检查清单

## English

Release target: `v0.1.0`

### Version

- [ ] `Cargo.toml` workspace version is correct
- [ ] release tag matches workspace version
- [ ] no unintended local changes remain

### Local Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] Windows package build completed
- [ ] Windows smoke test passed

### GitHub Workflow Expectations

- [ ] CI workflow is green on Linux x86_64
- [ ] CI workflow is green on Windows x86_64
- [ ] CI workflow is green on Linux arm64
- [ ] package jobs upload expected artifacts
- [ ] release workflow creates `SHA256SUMS.txt`
- [ ] GitHub Release page is created

### Release Assets

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`

### Post-Release Notes

- [ ] document any known limits
- [ ] record failed or skipped validations
- [ ] update roadmap follow-up items

## 中文

目标版本：`v0.1.0`

### 版本检查

- [ ] `Cargo.toml` 的 workspace version 正确
- [ ] release tag 与 workspace version 一致
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
- [ ] 各 package job 上传了预期产物
- [ ] release workflow 生成了 `SHA256SUMS.txt`
- [ ] GitHub Release 页面已创建

### 发布产物

- [ ] Windows `.zip`
- [ ] Linux x86_64 `.tar.gz`
- [ ] Linux x86_64 `.rpm`
- [ ] Linux arm64 `.tar.gz`
- [ ] Linux arm64 `.rpm`
- [ ] `SHA256SUMS.txt`

### 发版后记录

- [ ] 记录已知边界
- [ ] 记录失败或跳过的验证
- [ ] 更新路线图后续事项
