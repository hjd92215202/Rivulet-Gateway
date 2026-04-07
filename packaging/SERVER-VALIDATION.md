# Server Validation / 服务器验证

## English

This document defines the first server-side validation path for `Rivulet Gateway`.

Goal:

- verify that a packaged artifact is not only structurally correct, but also runnable after extraction
- keep the suite dependency-light so it can run on ordinary Linux servers or CI agents

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

5. Real-host install-path validation
   File: `scripts/linux-postinstall-validate.sh`
   Installs the packaged service onto a clean Linux validation host, starts the real service, verifies healthy-path and degraded-path behavior, and can auto-clean the install afterward.

6. Temporary-unit systemd lifecycle validation
   File: `scripts/linux-systemd-validate.sh`
   Uses a temporary validation unit on a Linux host to verify start, restart, stop, and degraded-path behavior.

7. Host-side upgrade and rollback validation
   File: `scripts/linux-upgrade-rollback-validate.sh`
   Verifies that a temporary validation deployment can start, upgrade, roll back, and recover service health.

8. CI/release lifecycle gates
   Files: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
   Run Linux x86_64 and Linux arm64 systemd lifecycle validation on packaged `tar.gz` and `rpm` artifacts before publish/provenance continuation.

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
- clean-host install path can start the real `rivulet-gateway.service`

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

- host-side lifecycle checks exist, but package-manager-native install/upgrade validation in disposable VMs is not complete yet
- CI/release gates currently validate service lifecycle on hosted runners, not disposable VM snapshots
- does not yet verify config migration semantics

Next validation layers to add:

- real `rpm -i` / `rpm -U` validation inside disposable Linux VMs
- systemd validation inside disposable Linux VMs or dedicated validation hosts
- restart and graceful shutdown checks under load
- release checksum and signature verification

## 中文

本文档定义 `Rivulet Gateway / 溪流网关` 的第一套服务器侧验证路径。

目标：

- 不仅验证打包产物结构正确，还要验证解压后可运行
- 保持验证套件依赖轻量，确保普通 Linux 服务器或 CI agent 都能运行

当前验证层次：

1. 产物结构验证
   文件：`packaging/tests/validate-package.sh`
   检查预期的二进制、配置、服务文件和文档条目。

2. 安装后布局 smoke 验证
   文件：`packaging/tests/install-package.sh`
   将 `tar.gz` 或 `rpm` 解到临时根目录，并执行打包后的 gateway 二进制。

3. Linux 组合验证入口
   文件：`packaging/tests/run-linux-validation.sh`
   按顺序执行前两步。

4. 发布 checksum 校验
   文件：`packaging/tests/verify-checksums.sh`
   用 `SHA256SUMS` 校验发布产物。

5. 真实主机安装路径验证
   文件：`scripts/linux-postinstall-validate.sh`
   在干净 Linux 验证主机安装打包服务，启动真实服务并验证健康路径、降级路径，支持验证后自动清理。

6. 临时单元 systemd 生命周期验证
   文件：`scripts/linux-systemd-validate.sh`
   在 Linux 主机使用临时验证单元校验 start/restart/stop 与降级路径行为。

7. 主机侧升级与回滚验证
   文件：`scripts/linux-upgrade-rollback-validate.sh`
   验证临时部署可启动、可升级、可回滚，并在回滚后恢复服务健康。

8. CI/release 生命周期门禁
   文件：`.github/workflows/ci.yml`、`.github/workflows/release.yml`
   在发布前对 Linux x86_64 与 Linux arm64 的 `tar.gz` 和 `rpm` 产物执行 systemd 生命周期门禁验证。

验证预期：

- `tar.gz` 必须包含：
  `/usr/bin/gateway`
  `/etc/gateway/gateway.toml`
  `/usr/lib/systemd/system/rivulet-gateway.service`
  `/usr/share/doc/rivulet-gateway/README.md`
- `rpm` 需要暴露相同 payload 条目
- service 文件必须保持：
  `ExecStart=/usr/bin/gateway /etc/gateway/gateway.toml`
- 打包二进制能够成功启动
- 路由未命中返回 `404`
- 上游未命中返回 `502`

示例命令：

```bash
bash ./packaging/tests/run-linux-validation.sh ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.0.tar.gz
bash ./packaging/tests/run-linux-validation.sh ./dist/rpmbuild/x86_64-unknown-linux-gnu/RPMS/x86_64/rivulet-gateway-0.1.0-1.x86_64.rpm
bash ./packaging/tests/verify-checksums.sh ./SHA256SUMS.txt .
```

服务器前置要求：

- `bash`
- `curl`
- 若要验证 `rpm`：需要 `rpm`、`rpm2cpio`、`cpio`

当前范围限制：

- 已有主机侧生命周期验证，但一次性 VM 中的包管理器原生安装/升级验证仍未完成
- CI/release 门禁当前在 hosted runner 上执行服务生命周期验证，不等同于 disposable VM 快照验证
- 还没有验证配置迁移语义

下一层验证建议：

- 在 disposable Linux VM 中执行真实 `rpm -i` / `rpm -U` 验证
- systemd enable/start/stop 验证
- 有负载时的重启与优雅关闭检查
- 发布 checksum 与签名校验
