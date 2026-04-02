# Release Process / 发布流程

## English

This document defines the first formal release process for `Rivulet Gateway / 溪流网关`.
The immediate target is the first public tag release: `v0.1.0`.

### Source Of Truth

- workspace version source: `Cargo.toml` under `[workspace.package]`
- tag naming rule: `v<version>`
- current first-release target: `v0.1.0`

### Release Trigger Model

- regular `push` or `pull_request` runs `.github/workflows/ci.yml`
- nightly benchmark collection runs `.github/workflows/nightly-benchmark.yml`
- pushing a tag like `v0.1.0` runs `.github/workflows/release.yml`
- the release workflow uploads Linux x86_64, Linux arm64, and Windows x86_64 artifacts
- the publish job creates a GitHub Release and attaches built artifacts plus `SHA256SUMS.txt`

### First Release Steps

1. Confirm `Cargo.toml` workspace version matches the intended release version.
2. Confirm CI workflow changes are already merged or present on the release branch.
3. Run local pre-release validation.
4. Commit any final release-prep changes with a clear `feat:` or `docs:` message.
5. Create an annotated tag such as `git tag -a v0.1.0 -m "Rivulet Gateway v0.1.0"`.
6. Push the branch and the tag to GitHub.
7. Monitor `.github/workflows/release.yml` until all package jobs and the publish job succeed.
8. Check that the GitHub Release page contains `zip`, `tar.gz`, `rpm`, and `SHA256SUMS.txt`.
9. Record remaining risks and post-release follow-up items in the roadmap.

### Local Pre-Release Validation

Minimum local checks before pushing a release tag:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- Windows package build
- Windows packaged binary smoke test

Recommended release-local commands on the current Windows development host:

```powershell
cargo fmt --all -- --check
cargo test --workspace
.\scripts\package.ps1 -Target x86_64-pc-windows-msvc -Format zip -SkipBuild
.\packaging\tests\server-smoke.ps1 -Binary .\dist\x86_64-pc-windows-msvc\rivulet-gateway-0.1.0\gateway.exe -Port 18083
```

### Artifact Expectations

Expected release outputs:

- Windows x86_64: `.zip`
- Linux x86_64: `.tar.gz` and `.rpm`
- Linux arm64: `.tar.gz` and `.rpm`
- `SHA256SUMS.txt`

### Current Release Boundaries

The release process is ready for conservative public packaging, but still has important limits:

- no artifact signing yet
- no SBOM or provenance publication yet
- no disposable-VM upgrade validation yet
- no systemd lifecycle validation in CI yet

## 中文

本文档定义 `Rivulet Gateway / 溪流网关` 的首个正式发布流程。
当前目标是首个公开标签版本：`v0.1.0`。

### 版本真源

- 工作区版本来源：`Cargo.toml` 中的 `[workspace.package]`
- 标签命名规则：`v<version>`
- 当前首发目标版本：`v0.1.0`

### 触发模型

- 常规 `push` 或 `pull_request` 触发 `.github/workflows/ci.yml`
- 夜间 benchmark 收集触发 `.github/workflows/nightly-benchmark.yml`
- 推送 `v0.1.0` 这种 tag 会触发 `.github/workflows/release.yml`
- release workflow 会产出 Linux x86_64、Linux arm64、Windows x86_64 包
- publish job 会创建 GitHub Release，并附上所有产物和 `SHA256SUMS.txt`

### 首发执行步骤

1. 确认 `Cargo.toml` 中的 workspace version 与目标发版号一致。
2. 确认本次发版依赖的 CI 变更已在发版分支上。
3. 执行本地发版前验证。
4. 用清晰的 `feat:` 或 `docs:` 提交最后的发版准备改动。
5. 创建注解 tag，例如 `git tag -a v0.1.0 -m "Rivulet Gateway v0.1.0"`。
6. 把分支和 tag 一起推送到 GitHub。
7. 观察 `.github/workflows/release.yml`，直到所有 package job 和 publish job 成功。
8. 检查 GitHub Release 页面是否包含 `zip`、`tar.gz`、`rpm` 和 `SHA256SUMS.txt`。
9. 把剩余风险和发版后的补项回填到路线图。

### 本地发版前验证

推 release tag 前，最低要求的本地检查：

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- Windows 打包构建
- Windows 打包后二进制 smoke test

当前 Windows 开发机推荐命令：

```powershell
cargo fmt --all -- --check
cargo test --workspace
.\scripts\package.ps1 -Target x86_64-pc-windows-msvc -Format zip -SkipBuild
.\packaging\tests\server-smoke.ps1 -Binary .\dist\x86_64-pc-windows-msvc\rivulet-gateway-0.1.0\gateway.exe -Port 18083
```

### 预期产物

预期发布输出：

- Windows x86_64：`.zip`
- Linux x86_64：`.tar.gz` 和 `.rpm`
- Linux arm64：`.tar.gz` 和 `.rpm`
- `SHA256SUMS.txt`

### 当前发版边界

当前发布流程已经适合保守的公开分发，但仍有明显边界：

- 还没有 artifact signing
- 还没有 SBOM 或 provenance 发布
- 还没有基于 disposable VM 的升级验证
- CI 里还没有 systemd 生命周期验证
