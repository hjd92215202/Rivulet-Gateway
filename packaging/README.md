# Rivulet Gateway Packaging

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

GitHub Actions:

- CI: `.github/workflows/ci.yml`
- Release packaging: `.github/workflows/release.yml`

Smoke suites:

- Linux: `packaging/tests/server-smoke.sh`
- Windows: `packaging/tests/server-smoke.ps1`

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

- Every push and pull request runs workspace tests on Linux and Windows.
- CI also builds and smoke-tests Windows x86_64 and Linux x86_64 packages.
- Tag pushes like `v0.1.0` publish release assets automatically.
- Linux arm64 packaging is wired for GitHub Actions and runs on `ubuntu-24.04-arm` during manual workflow dispatch.
- If the repository visibility or runner policy does not allow hosted arm64 runners, replace that job with a self-hosted arm64 runner label.
