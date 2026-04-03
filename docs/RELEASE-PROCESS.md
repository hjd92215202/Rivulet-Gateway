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
- The publish job creates or updates the GitHub Release and uploads all packaged assets plus `SHA256SUMS.txt`.

### Release Steps

1. Confirm the intended public version, for example `v0.1.5`.
2. Confirm the branch already contains the required CI/CD, packaging, and documentation changes.
3. Run local pre-release validation.
4. Commit any final release-prep changes with a clear `feat:` or `docs:` message.
5. Create an annotated tag such as `git tag -a v0.1.5 -m "Rivulet Gateway v0.1.5"`.
6. Push the branch and the tag to GitHub.
7. Monitor `.github/workflows/release.yml` until all packaging jobs and the publish job succeed.
8. Check that the GitHub Release page contains `zip`, `tar.gz`, `rpm`, and `SHA256SUMS.txt`, and that every asset filename matches the tag version.
9. Record remaining risks and post-release follow-up items in the roadmap.

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

### Current Release Boundaries

The release process is ready for conservative public packaging, but still has important limits:

- No artifact signing yet.
- No SBOM or provenance publication yet.
- No disposable-VM upgrade validation yet.
- No systemd lifecycle validation in CI yet.

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
- publish job 会创建或更新 GitHub Release，并上传所有打包产物和 `SHA256SUMS.txt`。

### 发布步骤

1. 确认本次公开版本号，例如 `v0.1.5`。
2. 确认当前分支已经包含所需的 CI/CD、打包和文档变更。
3. 执行本地发布前验证。
4. 用明确的 `feat:` 或 `docs:` 提交最后的发布准备改动。
5. 创建注解 tag，例如 `git tag -a v0.1.5 -m "Rivulet Gateway v0.1.5"`。
6. 将分支和 tag 一起推送到 GitHub。
7. 观察 `.github/workflows/release.yml`，直到所有打包 job 和 publish job 成功。
8. 检查 GitHub Release 页面是否包含 `zip`、`tar.gz`、`rpm` 和 `SHA256SUMS.txt`，并确认所有产物文件名与 tag 版本一致。
9. 把剩余风险和后续事项回填到路线图。

### 本地发布前验证

推送 release tag 前，最低要求的本地检查：

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- Windows 打包构建
- Windows 打包后二进制 smoke test

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

### 当前发布边界

当前发布流程已经适合保守公开分发，但仍有明显边界：

- 还没有 artifact signing。
- 还没有 SBOM 或 provenance 发布。
- 还没有基于 disposable VM 的升级验证。
- CI 里还没有 systemd 生命周期验证。
