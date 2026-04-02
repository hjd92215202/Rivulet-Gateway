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
