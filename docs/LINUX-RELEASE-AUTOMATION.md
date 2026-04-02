# Linux Release Automation / Linux 发版包自动化验证

## English

This document describes the one-command Linux validation path with a bundled fixture backend.

The automation script:

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

Note:

- After the benchmark report path is printed, the script still performs one final backend-down validation that expects `502`, so a short delay before `summary.md` appears is normal.

Usage:

```bash
bash ./scripts/linux-release-e2e.sh \
  --host llmtamer.com:8080 \
  --tag v0.1.6 \
  --label llmtamer-e2e
```

Generated outputs:

- `./target/linux-release-e2e/<timestamp>-<label>/summary.md`
- `./target/linux-release-e2e/<timestamp>-<label>/report/.../report.md`

Prerequisites:

- `curl`
- `tar`
- `sha256sum`
- `python3`
- `wrk`

## 中文

本文档说明如何通过“一条命令”完成 Linux 发版包验证，并使用仓库自带的 fixture backend。

自动化脚本会完成：

- 根据当前服务器架构下载指定 GitHub Release 的 tar 包
- 校验 `SHA256SUMS.txt`
- 解压发版包
- 启动仓库内自带的 fixture backend
- 自动生成网关配置
- 启动打包后的网关二进制
- 验证：
  - 正常路由返回 `200`
  - 错误 host 返回 `404`
  - 停掉后端后返回 `502`
- 执行基线压测脚本
- 输出摘要报告和 benchmark 报告

使用方式：

```bash
bash ./scripts/linux-release-e2e.sh \
  --host llmtamer.com:8080 \
  --tag v0.1.6 \
  --label llmtamer-e2e
```

输出结果：

- `./target/linux-release-e2e/<timestamp>-<label>/summary.md`
- `./target/linux-release-e2e/<timestamp>-<label>/report/.../report.md`

前置要求：

- `curl`
- `tar`
- `sha256sum`
- `python3`
- `wrk`

说明：

- 当终端已经打印出 `report.md` 路径后，脚本还会继续做最后一步“后端下线后返回 502”的校验，所以 `summary.md` 晚几秒出现是正常现象。
