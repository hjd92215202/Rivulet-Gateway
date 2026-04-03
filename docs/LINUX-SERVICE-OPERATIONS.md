# Linux Service Operations / Linux 服务运维脚本

## English

This document describes the repository-owned Linux install and remove scripts for the packaged gateway service.

Scripts:

- install: `scripts/linux-service-install.sh`
- remove: `scripts/linux-service-remove.sh`

### Install script behavior

- supports Linux x86_64 and Linux arm64
- auto-installs runtime dependencies when `apt-get`, `dnf`, or `yum` is available
- accepts either a local package artifact or a GitHub Release tag
- supports `tar.gz` and `rpm`
- installs the packaged binary and systemd unit
- preserves `/etc/gateway/gateway.toml` by default for tarball installs
- can replace config explicitly with `--replace-config`
- can enable and start the service automatically unless skipped
- writes an install summary to `./target/linux-service-install/<timestamp>-<label>/summary.md`

Example:

```bash
bash ./scripts/linux-service-install.sh --tag v0.1.6 --format tar.gz
```

### Remove script behavior

- supports Linux x86_64 and Linux arm64
- auto-installs runtime dependencies when `apt-get`, `dnf`, or `yum` is available
- stops and disables `rivulet-gateway.service`
- can remove either a manual install, an rpm install, or auto-detect between them
- preserves config by default
- can purge config with `--purge-config`
- writes a removal summary to `./target/linux-service-remove/<timestamp>-<label>/summary.md`

Example:

```bash
bash ./scripts/linux-service-remove.sh --mode auto
```

### Operational notes

- Tarball install is the most portable path across Debian-family and RPM-family hosts.
- RPM install is intended for native rpm-based hosts and uses `rpm -Uvh`.
- These scripts are designed for cautious operator workflows and do not claim full cluster-wide orchestration semantics.

## 中文

这份文档说明仓库内置的 Linux 服务安装与卸载脚本。

脚本入口：

- 安装：`scripts/linux-service-install.sh`
- 卸载：`scripts/linux-service-remove.sh`

### 安装脚本行为

- 支持 Linux x86_64 和 Linux arm64
- 当系统提供 `apt-get`、`dnf` 或 `yum` 时，自动安装缺失的运行依赖
- 同时支持“本地产物路径”与“按 GitHub Release tag 下载”
- 支持 `tar.gz` 和 `rpm`
- 安装打包后的二进制与 systemd unit
- 对 tarball 安装默认保留 `/etc/gateway/gateway.toml`
- 如需覆盖配置，可显式传 `--replace-config`
- 默认可自动 enable/start 服务，也可以通过参数跳过
- 会把安装摘要写到 `./target/linux-service-install/<timestamp>-<label>/summary.md`

示例：

```bash
bash ./scripts/linux-service-install.sh --tag v0.1.6 --format tar.gz
```

### 卸载脚本行为

- 支持 Linux x86_64 和 Linux arm64
- 当系统提供 `apt-get`、`dnf` 或 `yum` 时，自动安装缺失的运行依赖
- 自动 stop/disable `rivulet-gateway.service`
- 可以删除 manual install、rpm install，或者自动判断
- 默认保留配置
- 如需连配置一起清理，可传 `--purge-config`
- 会把卸载摘要写到 `./target/linux-service-remove/<timestamp>-<label>/summary.md`

示例：

```bash
bash ./scripts/linux-service-remove.sh --mode auto
```

### 运维说明

- `tar.gz` 安装是目前跨 Debian 系和 RPM 系主机最通用的路径
- `rpm` 安装面向原生 rpm 主机，内部使用 `rpm -Uvh`
- 这些脚本面向保守运维流程，不声称已经覆盖完整的集群编排语义
