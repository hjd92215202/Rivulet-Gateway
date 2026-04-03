# Rivulet Gateway Packaging / 溪流网关打包说明

## English

This directory defines the first delivery path for `Rivulet Gateway` with a low-dependency packaging policy.

Targets in scope:

- Windows x86_64: `zip`
- Linux x86_64: `tar.gz` and `rpm`
- Linux arm64: `tar.gz` and `rpm`

Packaging rules:

- Prefer native Rust builds and native platform tooling.
- Do not add heavy release-time dependencies unless the maintenance cost is justified.
- Keep package contents small: binary, config, service unit, and docs only.
- Reuse the same staged filesystem layout for `tar.gz` and `rpm` to avoid drift.

Entry points:

- PowerShell: `scripts/package.ps1`
- Bash: `scripts/package.sh`
- Version lookup: `scripts/get-version.sh`, `scripts/get-version.ps1`

GitHub Actions:

- CI: `.github/workflows/ci.yml`
- Release packaging: `.github/workflows/release.yml`
- Nightly benchmark: `.github/workflows/nightly-benchmark.yml`

Smoke suites:

- Linux: `packaging/tests/server-smoke.sh`
- Windows: `packaging/tests/server-smoke.ps1`
- Artifact validation: `packaging/tests/validate-package.sh`
- Installed-package smoke: `packaging/tests/install-package.sh`
- Combined Linux validation: `packaging/tests/run-linux-validation.sh`
- Checksum verification: `packaging/tests/verify-checksums.sh`

Example commands:

```powershell
./scripts/package.ps1 -Target x86_64-pc-windows-msvc
./scripts/package.ps1 -Target x86_64-unknown-linux-gnu -Format tar.gz
```

```bash
./scripts/package.sh --target x86_64-unknown-linux-gnu --format tar.gz
./scripts/package.sh --target aarch64-unknown-linux-gnu --format rpm
```

GitHub CI/CD policy:

- Every push and pull request runs workspace tests on Linux, Windows, and Linux arm64.
- CI also builds and smoke-tests Windows x86_64, Linux x86_64, and Linux arm64 packages.
- CI validates Linux `tar.gz` and `rpm` package contents before publishing artifacts.
- CI extracts staged Linux artifacts and runs smoke tests against the installed filesystem layout.
- Tag pushes like `v0.1.0` publish release assets automatically.
- If repository visibility or runner policy does not allow hosted arm64 runners, replace that job with a self-hosted arm64 runner label.
- A separate nightly workflow runs a conservative benchmark baseline and uploads CSV output for trend tracking.

Server rollout references:

- validation flow: `packaging/SERVER-VALIDATION.md`
- real-host install-path validation: `docs/LINUX-POSTINSTALL-VALIDATION.md`
- real-host systemd validation: `docs/LINUX-SYSTEMD-VALIDATION.md`
- host-side upgrade rollback validation: `docs/LINUX-UPGRADE-ROLLBACK-VALIDATION.md`
- install and remove operations: `docs/LINUX-SERVICE-OPERATIONS.md`

## 中文

本目录定义 `Rivulet Gateway / 溪流网关` 的首条正式分发路径，并坚持低依赖打包策略。

当前目标平台：

- Windows x86_64：`zip`
- Linux x86_64：`tar.gz` 和 `rpm`
- Linux arm64：`tar.gz` 和 `rpm`

打包规则：

- 优先使用原生 Rust 构建和原生平台工具链。
- 除非维护成本有充分理由，否则不引入重量级发布期依赖。
- 尽量保持包内容精简，只包含二进制、配置、服务单元和文档。
- `tar.gz` 与 `rpm` 复用同一套 staged 文件系统布局，避免内容漂移。

入口脚本：

- PowerShell：`scripts/package.ps1`
- Bash：`scripts/package.sh`
- 版本读取：`scripts/get-version.sh`、`scripts/get-version.ps1`

GitHub Actions：

- CI：`.github/workflows/ci.yml`
- 发布打包：`.github/workflows/release.yml`
- 夜间 benchmark：`.github/workflows/nightly-benchmark.yml`

Smoke 套件：

- Linux：`packaging/tests/server-smoke.sh`
- Windows：`packaging/tests/server-smoke.ps1`
- 产物结构校验：`packaging/tests/validate-package.sh`
- 安装后 smoke：`packaging/tests/install-package.sh`
- Linux 组合验证入口：`packaging/tests/run-linux-validation.sh`
- checksum 校验：`packaging/tests/verify-checksums.sh`

示例命令：

```powershell
./scripts/package.ps1 -Target x86_64-pc-windows-msvc
./scripts/package.ps1 -Target x86_64-unknown-linux-gnu -Format tar.gz
```

```bash
./scripts/package.sh --target x86_64-unknown-linux-gnu --format tar.gz
./scripts/package.sh --target aarch64-unknown-linux-gnu --format rpm
```

GitHub CI/CD 策略：

- 每次 push 和 pull request 都会在 Linux、Windows、Linux arm64 上运行 workspace tests。
- CI 同时会构建并 smoke 测试 Windows x86_64、Linux x86_64、Linux arm64 包。
- CI 会在上传产物前校验 Linux `tar.gz` 和 `rpm` 的内容结构。
- CI 会解压 Linux staged 产物，并对安装后文件系统布局执行 smoke 测试。
- 推送 `v0.1.0` 这类标签时会自动发布 release assets。
- 如果仓库可见性或 runner 策略不允许托管 arm64 runner，需要把对应 job 替换为自托管 arm64 runner 标签。
- 另有独立夜间工作流执行保守 benchmark 基线，并上传 CSV 结果用于趋势跟踪。

服务器上线参考：

- 验证流程：`packaging/SERVER-VALIDATION.md`
