# Server Validation

This document defines the first server-side validation path for `Rivulet Gateway`.

Goal:

- Verify that a packaged artifact is not only structurally correct, but also runnable after extraction.
- Keep the suite dependency-light so it can run on ordinary Linux servers or CI agents.

Current validation layers:

1. Artifact layout validation
   File: `packaging/tests/validate-package.sh`
   Checks expected binary, config, service file, and docs entries.

2. Installed-layout smoke validation
   File: `packaging/tests/install-package.sh`
   Extracts `tar.gz` or `rpm` into a temporary root and runs the packaged gateway binary.

3. Combined Linux validation entrypoint
   File: `packaging/tests/run-linux-validation.sh`
   Runs both steps in order.

4. Release checksum verification
   File: `packaging/tests/verify-checksums.sh`
   Verifies published artifacts against `SHA256SUMS`.

Validation expectations:

- `tar.gz` contains:
  `/usr/bin/gateway`
  `/etc/gateway/gateway.toml`
  `/usr/lib/systemd/system/rivulet-gateway.service`
  `/usr/share/doc/rivulet-gateway/README.md`
- `rpm` exposes the same payload entries
- service file keeps:
  `ExecStart=/usr/bin/gateway /etc/gateway/gateway.toml`
- packaged binary starts successfully
- route miss returns `404`
- upstream miss returns `502`

Example commands:

```bash
bash ./packaging/tests/run-linux-validation.sh ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.0.tar.gz
bash ./packaging/tests/run-linux-validation.sh ./dist/rpmbuild/x86_64-unknown-linux-gnu/RPMS/x86_64/rivulet-gateway-0.1.0-1.x86_64.rpm
bash ./packaging/tests/verify-checksums.sh ./SHA256SUMS.txt .
```

Server prerequisites:

- `bash`
- `curl`
- for `rpm` validation: `rpm`, `rpm2cpio`, `cpio`

Current scope limits:

- Does not install the package into the real system root.
- Does not register or start a real systemd unit.
- Does not yet verify upgrade paths, rollback paths, or config migration semantics.

Next validation layers to add:

- real `rpm -i` / `rpm -U` validation inside disposable Linux VMs
- systemd enable/start/stop verification
- restart and graceful shutdown checks under load
- release checksum and signature verification
