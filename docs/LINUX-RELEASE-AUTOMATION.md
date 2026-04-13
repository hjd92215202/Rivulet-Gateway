# Linux Release Automation / Linux 发布包自动化验证

## English

This document describes the one-command Linux validation path with a bundled fixture backend.

The automation script:

- supports Linux x86_64 and Linux arm64
- auto-installs missing runtime dependencies when the host uses `apt-get`, `dnf`, or `yum`
- downloads the selected GitHub Release tarball for the current server architecture
- verifies `SHA256SUMS.txt`
- extracts the package
- starts a bundled fixture backend from the repository
- generates a gateway config automatically
- starts the packaged gateway binary
- verifies:
  - valid route returns `200`
  - wrong host returns `404`
  - backend-down returns `502`
- runs the baseline benchmark reporter
- writes a summary report and a benchmark report
- exits automatically after cleanup

Notes:

- After the benchmark report path is printed, the script still performs one final backend-down validation that expects `502`, so a short delay before `summary.md` appears is normal.
- The script stops the bundled backend by itself and does not require manual intervention.

Usage:

```bash
bash ./scripts/linux-release-e2e.sh \
  --host llmtamer.com:8080 \
  --tag v0.1.7 \
  --label llmtamer-e2e
```

Generated outputs:

- `./target/linux-release-e2e/<timestamp>-<label>/summary.md`
- `./target/linux-release-e2e/<timestamp>-<label>/report/.../report.md`

Prerequisites:

- Root or `sudo` is recommended so the script can auto-install missing tools.
- If the host uses `apt-get`, `dnf`, or `yum`, the script can install `curl`, `tar`, `sha256sum`, `python3`, and `wrk` automatically.
- If `--tag` is omitted, the script prefers the current checked-out Git tag and falls back to `v0.1.6`.

### Disposable Lifecycle Gate (G3.3)

Release now includes a dedicated disposable lifecycle gate script:

- script: `scripts/linux-disposable-lifecycle-gate.sh`
- output: `result.json` (machine-readable) + `summary.md` (human-readable)
- architecture support: Linux x86_64 + Linux arm64
- release blocking behavior:
  - `tar.gz` path runs `install -> upgrade -> rollback -> uninstall` (`--lifecycle-mode full`)
  - `rpm` path runs `install -> uninstall` (`--lifecycle-mode install-uninstall`)
- any stage failure blocks release publish

Example (tar.gz full lifecycle):

```bash
bash ./scripts/linux-disposable-lifecycle-gate.sh \
  --mode execute \
  --lifecycle-mode full \
  --arch linux-x86_64 \
  --format tar.gz \
  --from-tag v0.1.5 \
  --to-artifact ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --host localhost
```

Example (rpm install/uninstall):

```bash
bash ./scripts/linux-disposable-lifecycle-gate.sh \
  --mode execute \
  --lifecycle-mode install-uninstall \
  --arch linux-x86_64 \
  --format rpm \
  --to-artifact ./dist/rpmbuild/x86_64-unknown-linux-gnu/RPMS/x86_64/rivulet-gateway-0.1.6-1.x86_64.rpm \
  --host localhost
```

## 中文

本文说明如何通过“一条命令”完成 Linux 发布包验证，并使用仓库自带的 fixture backend。

自动化脚本会完成：

- 支持 Linux x86_64 和 Linux arm64
- 当系统使用 `apt-get`、`dnf` 或 `yum` 时，自动安装缺失的运行依赖
- 根据当前服务器架构下载指定 GitHub Release 的 tar 包
- 校验 `SHA256SUMS.txt`
- 解压发布包
- 启动仓库内自带的 fixture backend
- 自动生成网关配置
- 启动打包后的网关二进制
- 自动验证：
  - 正常路由返回 `200`
  - 错误 host 返回 `404`
  - 后端下线后返回 `502`
- 执行基线压测脚本
- 输出摘要报告和 benchmark 报告
- 自动清理并退出，不需要人工参与

说明：

- 当终端已经打印出 `report.md` 路径后，脚本还会继续做最后一步“后端下线后返回 502”的校验，所以 `summary.md` 晚几秒出现是正常现象。
- 脚本会自己停掉自带 backend，不需要人工手动停服务。

使用方式：

```bash
bash ./scripts/linux-release-e2e.sh \
  --host llmtamer.com:8080 \
  --tag v0.1.7 \
  --label llmtamer-e2e
```

输出结果：

- `./target/linux-release-e2e/<timestamp>-<label>/summary.md`
- `./target/linux-release-e2e/<timestamp>-<label>/report/.../report.md`

前置要求：

- 建议用 root 或具备 `sudo` 的用户执行，这样脚本才能自动补齐缺失依赖。
- 如果系统使用 `apt-get`、`dnf` 或 `yum`，脚本会自动安装 `curl`、`tar`、`sha256sum`、`python3` 和 `wrk`。
- 如果省略 `--tag`，脚本会优先使用当前 checkout 对应的 Git tag；如果拿不到，再回落到 `v0.1.6`。

### 一次性环境全生命周期门禁（G3.3）

release 现已接入专门的一次性环境全生命周期门禁脚本：

- 脚本：`scripts/linux-disposable-lifecycle-gate.sh`
- 输出：`result.json`（机器可读）+ `summary.md`（人工可读）
- 架构支持：Linux x86_64 + Linux arm64
- release 阻断方式：
  - `tar.gz` 路径执行 `安装 -> 升级 -> 回滚 -> 卸载`（`--lifecycle-mode full`）
  - `rpm` 路径执行 `安装 -> 卸载`（`--lifecycle-mode install-uninstall`）
- 任一阶段失败都会阻断发布

示例（tar.gz 全生命周期）：

```bash
bash ./scripts/linux-disposable-lifecycle-gate.sh \
  --mode execute \
  --lifecycle-mode full \
  --arch linux-x86_64 \
  --format tar.gz \
  --from-tag v0.1.5 \
  --to-artifact ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --host localhost
```

示例（rpm 安装/卸载）：

```bash
bash ./scripts/linux-disposable-lifecycle-gate.sh \
  --mode execute \
  --lifecycle-mode install-uninstall \
  --arch linux-x86_64 \
  --format rpm \
  --to-artifact ./dist/rpmbuild/x86_64-unknown-linux-gnu/RPMS/x86_64/rivulet-gateway-0.1.6-1.x86_64.rpm \
  --host localhost
```
