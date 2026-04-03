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

- does not install the package into the real system root
- host-side automation exists, but disposable-VM install and package-manager-native upgrade validation are not complete yet
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

- 还不会把包真实安装到系统根目录
- 还不会注册或启动真实 systemd 单元
- 还没有验证升级路径、回滚路径或配置迁移语义

下一层验证建议：

- 在 disposable Linux VM 中执行真实 `rpm -i` / `rpm -U` 验证
- systemd enable/start/stop 验证
- 有负载时的重启与优雅关闭检查
- 发布 checksum 与签名校验
