# Linux Post-Install Validation / Linux 安装后验证

## English

This document describes the clean-host validation entrypoint that closes the loop between package delivery and real service startup:

- script: `scripts/linux-postinstall-validate.sh`

What it does:

- installs a packaged artifact onto a real Linux host through the repository-owned install script
- writes a deterministic validation config to `/etc/gateway/gateway.toml`
- starts the real `rivulet-gateway.service`
- validates `200`, wrong-host `404`, service restart back to `200`, and backend-down `502`
- writes a `summary.md` report plus backend, `systemctl`, and `journalctl` logs
- can automatically remove the installed validation service when finished

Important boundary:

- this script is intentionally conservative
- it expects a dedicated validation host without an existing `rivulet-gateway` installation
- if the host already has an installed service, use `scripts/linux-upgrade-rollback-validate.sh` instead

Supported inputs:

- local artifact path: `--artifact <path>`
- GitHub Release download: `--tag <tag>` and optional `--repo <owner/name>`
- format selection: `--format tar.gz` or `--format rpm`

Supported platforms:

- Linux x86_64
- Linux arm64

Supported package managers for dependency bootstrap:

- `apt-get`
- `dnf`
- `yum`

Example:

```bash
bash ./scripts/linux-postinstall-validate.sh \
  --tag v0.1.6 \
  --host llmtamer.com:8080 \
  --cleanup remove
```

Keep the installed service for manual inspection:

```bash
bash ./scripts/linux-postinstall-validate.sh \
  --artifact ./dist/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --format tar.gz \
  --host llmtamer.com:8080 \
  --cleanup keep
```

Outputs:

- install summary: `./target/linux-postinstall-validate/<timestamp>-<label>/install/summary.md`
- remove summary: `./target/linux-postinstall-validate/<timestamp>-<label>/remove/summary.md`
- final report: `./target/linux-postinstall-validate/<timestamp>-<label>/summary.md`

Recommended role in the release and operations loop:

1. package locally or publish a GitHub Release tag
2. run `linux-postinstall-validate.sh` on a clean Linux validation host
3. run `linux-upgrade-rollback-validate.sh` on a host that already carries an earlier version
4. record any skipped validations in the release checklist

## 中文

这份文档说明“从包产物到真实服务启动”的 clean-host 验证入口：

- 脚本入口：`scripts/linux-postinstall-validate.sh`

它会做什么：

- 通过仓库自带安装脚本，把打包产物安装到真实 Linux 主机
- 生成一份确定性的验证配置并写入 `/etc/gateway/gateway.toml`
- 启动真实的 `rivulet-gateway.service`
- 自动验证 `200`、错误 Host 的 `404`、重启后仍为 `200`、后端下线后的 `502`
- 产出 `summary.md` 报告，以及 backend、`systemctl`、`journalctl` 日志
- 在验证完成后可自动卸载这次验证安装

重要边界：

- 这个脚本故意走保守路径
- 它假设目标机器是一台没有现存 `rivulet-gateway` 安装的专用验证主机
- 如果主机上已经装过服务，应改用 `scripts/linux-upgrade-rollback-validate.sh`

支持的输入方式：

- 本地产物路径：`--artifact <path>`
- 按 GitHub Release 下载：`--tag <tag>`，可选 `--repo <owner/name>`
- 产物格式：`--format tar.gz` 或 `--format rpm`

支持的平台：

- Linux x86_64
- Linux arm64

支持自动补齐依赖的包管理器：

- `apt-get`
- `dnf`
- `yum`

示例：

```bash
bash ./scripts/linux-postinstall-validate.sh \
  --tag v0.1.6 \
  --host llmtamer.com:8080 \
  --cleanup remove
```

如果想保留安装结果继续人工观察：

```bash
bash ./scripts/linux-postinstall-validate.sh \
  --artifact ./dist/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --format tar.gz \
  --host llmtamer.com:8080 \
  --cleanup keep
```

输出内容：

- 安装摘要：`./target/linux-postinstall-validate/<timestamp>-<label>/install/summary.md`
- 卸载摘要：`./target/linux-postinstall-validate/<timestamp>-<label>/remove/summary.md`
- 最终报告：`./target/linux-postinstall-validate/<timestamp>-<label>/summary.md`

推荐接入发布与运维闭环的方式：

1. 本地打包或推送 GitHub Release tag
2. 在干净的 Linux 验证主机上执行 `linux-postinstall-validate.sh`
3. 在已安装旧版本的主机上执行 `linux-upgrade-rollback-validate.sh`
4. 若有跳过项，把原因写进 release checklist
