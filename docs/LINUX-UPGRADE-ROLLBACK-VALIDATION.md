# Linux Upgrade Rollback Validation / Linux 升级回滚验证

## English

This document describes the conservative real-host Linux upgrade and rollback validation path.

The script:

- supports Linux x86_64 and Linux arm64
- auto-installs runtime dependencies when `apt-get`, `dnf`, or `yum` is available
- accepts either local package artifacts or GitHub Release tags
- supports `tar.gz` and `rpm`
- deploys a temporary validation binary, config, and `systemd` unit
- starts the initial package
- upgrades to the target package with `systemctl restart`
- rolls back to the original package with `systemctl restart`
- validates:
  - initial package returns `200`
  - wrong host returns `404`
  - upgraded package returns `200`
  - rolled-back package returns `200`
  - backend-down path returns `502` after rollback
- captures `journalctl` output
- tears the validation unit down automatically and exits automatically after writing the report

Script entrypoint:

- `scripts/linux-upgrade-rollback-validate.sh`

Examples:

```bash
bash ./scripts/linux-upgrade-rollback-validate.sh \
  --from-tag v0.1.5 \
  --to-tag v0.1.6 \
  --host llmtamer.com:8080
```

```bash
bash ./scripts/linux-upgrade-rollback-validate.sh \
  --from-artifact ./dist/old.tar.gz \
  --to-artifact ./dist/new.tar.gz \
  --host llmtamer.com:8080
```

Outputs:

- `./target/linux-upgrade-rollback/<timestamp>-<label>/summary.md`
- `./target/linux-upgrade-rollback/<timestamp>-<label>/logs/backend.log`
- `./target/linux-upgrade-rollback/<timestamp>-<label>/logs/journal.log`

Scope notes:

- This is host-side validation, not yet disposable-VM validation.
- It intentionally uses a temporary validation unit instead of a production service name.
- It validates the operator basics for upgrade and rollback without claiming full package-manager-native migration coverage yet.

## 中文

这份文档说明如何在真实 Linux 主机上，执行一条保守的升级与回滚自动化验证链。

这条脚本会完成：

- 支持 Linux x86_64 和 Linux arm64
- 当系统提供 `apt-get`、`dnf` 或 `yum` 时，自动安装缺失的运行依赖
- 同时支持“本地产物路径”与“按 GitHub Release tag 下载”
- 支持 `tar.gz` 和 `rpm`
- 部署临时验证二进制、配置和 `systemd` 单元
- 先启动初始版本
- 通过 `systemctl restart` 切换到目标版本
- 再通过 `systemctl restart` 回滚到原版本
- 自动验证：
  - 初始版本返回 `200`
  - 错误 host 返回 `404`
  - 升级后的版本返回 `200`
  - 回滚后的版本返回 `200`
  - 回滚后 backend 下线时返回 `502`
- 自动采集 `journalctl` 日志
- 自动停止验证服务、清理临时部署，并在写出报告后自动退出

脚本入口：

- `scripts/linux-upgrade-rollback-validate.sh`

示例：

```bash
bash ./scripts/linux-upgrade-rollback-validate.sh \
  --from-tag v0.1.5 \
  --to-tag v0.1.6 \
  --host llmtamer.com:8080
```

```bash
bash ./scripts/linux-upgrade-rollback-validate.sh \
  --from-artifact ./dist/old.tar.gz \
  --to-artifact ./dist/new.tar.gz \
  --host llmtamer.com:8080
```

输出结果：

- `./target/linux-upgrade-rollback/<timestamp>-<label>/summary.md`
- `./target/linux-upgrade-rollback/<timestamp>-<label>/logs/backend.log`
- `./target/linux-upgrade-rollback/<timestamp>-<label>/logs/journal.log`

范围说明：

- 这是一条主机侧验证链，还不是 disposable VM 级别的验证
- 它刻意使用临时验证单元，不直接碰正式生产服务名
- 它先把运维视角下最基础的升级/回滚动作自动化跑通，但暂不声称已经覆盖完整的包管理器原生迁移语义
