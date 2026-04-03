# Linux Systemd Validation / Linux systemd 验证

## English

This document describes the real-host Linux `systemd` validation path for packaged gateway artifacts.

The script:

- supports Linux x86_64 and Linux arm64
- auto-installs missing runtime dependencies when `apt-get`, `dnf`, or `yum` is available
- accepts either a local package artifact or a GitHub Release tag download path
- validates packaged payload extraction for `tar.gz` and `rpm`
- deploys the packaged binary and a generated config into temporary validation paths
- materializes a temporary `systemd` unit based on the packaged service file
- starts a bundled fixture backend from the repository
- validates:
  - healthy route returns `200`
  - wrong host returns `404`
  - `systemctl restart` returns the service to healthy `200`
  - backend-down path returns `502`
- captures `journalctl` output
- tears the validation service down automatically and exits automatically after writing the report

Script entrypoint:

- `scripts/linux-systemd-validate.sh`

Examples:

```bash
bash ./scripts/linux-systemd-validate.sh \
  --artifact ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --host llmtamer.com:8080
```

```bash
bash ./scripts/linux-systemd-validate.sh \
  --tag v0.1.6 \
  --format rpm \
  --host llmtamer.com:8080
```

Outputs:

- `./target/linux-systemd-validate/<timestamp>-<label>/summary.md`
- `./target/linux-systemd-validate/<timestamp>-<label>/logs/backend.log`
- `./target/linux-systemd-validate/<timestamp>-<label>/logs/journal.log`

Scope notes:

- This validation intentionally uses a temporary validation unit instead of touching an existing production service name.
- It validates real `systemd` lifecycle behavior on the host, but it does not yet claim full production install automation.
- It is the conservative bridge between unpack-only validation and later disposable-VM package installation validation.

## 中文

这份文档说明如何在真实 Linux 主机上，对打包后的网关产物执行一条保守的 `systemd` 生命周期验证链。

这条脚本会完成：

- 支持 Linux x86_64 和 Linux arm64
- 当系统提供 `apt-get`、`dnf` 或 `yum` 时，自动安装缺失的运行依赖
- 同时支持“本地产物路径”与“按 GitHub Release tag 下载”
- 支持对 `tar.gz` 和 `rpm` 产物做真实主机侧验证
- 将打包后的二进制和自动生成的配置部署到临时验证目录
- 基于包内自带的 service 文件生成临时 `systemd` 验证单元
- 启动仓库内置的 fixture backend
- 自动验证：
  - 正常路由返回 `200`
  - 错误 host 返回 `404`
  - `systemctl restart` 后仍然恢复到 `200`
  - backend 下线后返回 `502`
- 自动采集 `journalctl` 日志
- 自动停止验证服务、清理临时部署，并在写出报告后自动退出

脚本入口：

- `scripts/linux-systemd-validate.sh`

示例：

```bash
bash ./scripts/linux-systemd-validate.sh \
  --artifact ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.6-linux-x86_64.tar.gz \
  --host llmtamer.com:8080
```

```bash
bash ./scripts/linux-systemd-validate.sh \
  --tag v0.1.6 \
  --format rpm \
  --host llmtamer.com:8080
```

输出结果：

- `./target/linux-systemd-validate/<timestamp>-<label>/summary.md`
- `./target/linux-systemd-validate/<timestamp>-<label>/logs/backend.log`
- `./target/linux-systemd-validate/<timestamp>-<label>/logs/journal.log`

范围说明：

- 这条验证链刻意使用临时验证单元，不直接碰现有生产服务名
- 它已经验证了真实主机上的 `systemd` 启停行为，但还不等同于“完整生产安装自动化”
- 它是“仅解压验证”和“未来一次性 VM 真安装验证”之间的一层保守过渡
