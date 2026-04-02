# Script Standards / 脚本标准

## English

The repository script standard is:

- Happy-path execution should not require manual intervention.
- Scripts must print explicit stage progress for long-running work.
- Scripts must clean up background processes and exit automatically.
- Linux validation and benchmark scripts should support x86_64 and arm64.
- Linux validation and benchmark scripts should auto-install missing dependencies when `apt-get`, `dnf`, or `yum` is available.
- Shared bootstrap logic should live in reusable helpers instead of being duplicated.
- CI enforces these rules before the main test matrix starts.

Shared helper:

- `scripts/lib/linux-bootstrap.sh`

CI enforcement entry:

- `scripts/check-script-standards.sh`

Current scripts aligned to this standard:

- `scripts/linux-release-e2e.sh`
- `scripts/linux-baseline-report.sh`

## 中文

仓库里的脚本统一标准如下：

- 正常路径执行时不依赖人工介入。
- 对耗时步骤必须打印明确的阶段进度。
- 脚本结束时必须自动清理后台进程并自动退出。
- Linux 验证和压测脚本要同时支持 x86_64 和 arm64。
- Linux 验证和压测脚本在系统存在 `apt-get`、`dnf` 或 `yum` 时，要自动安装缺失依赖。
- 公共启动与依赖补齐逻辑要沉淀到可复用 helper，而不是在各脚本里重复实现。
- CI 会在主测试矩阵开始前强制校验这些规则。

共享 helper：

- `scripts/lib/linux-bootstrap.sh`

CI 校验入口：

- `scripts/check-script-standards.sh`

当前已对齐到该标准的脚本：

- `scripts/linux-release-e2e.sh`
- `scripts/linux-baseline-report.sh`
