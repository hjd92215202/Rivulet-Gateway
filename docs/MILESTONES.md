# Milestones / 里程碑路线图

## English

Project: `Rivulet Gateway / 溪流网关`

Baseline date: April 7, 2026

This roadmap turns the current repository state into staged milestones with exit criteria.
It is intentionally conservative: the target is production-safe progress, not maximum feature velocity.

### Milestone 0: Kernel Baseline

Status: in progress, substantially established

What exists:

- typed config
- HTTP/1.1 routing and proxy core
- upstream health and retry skeleton
- conservative upstream keepalive reuse
- runtime listener loop
- packaging skeleton
- CI, release automation, and nightly benchmark baseline

Exit criteria:

- all existing workflows stay green on Linux, Windows, and Linux arm64
- Linux x86_64 and Linux arm64 package validation pass in CI
- Windows package smoke remains stable
- roadmap and governance docs are in repo

### Milestone 1: Grayscale Readiness

Goal:

- make the gateway safe enough for narrow-scope internal grayscale traffic

Required work:

- Linux real-host validation for `tar.gz` and `rpm`
- downstream connection lifecycle hardening
- clearer protocol rejection for unsupported request and response paths
- service installation and startup validation on Linux hosts
- release artifact handling refined for repeatable operator use

Exit criteria:

- Linux x86_64 staged install validated on real machines
- no unexplained `502` or socket churn regressions under conservative benchmark baselines
- operator documentation covers start, stop, validate, and rollback basics

### Milestone 2: Production Edge Foundation

Goal:

- support controlled production ingress for simple HTTP/1.1 traffic classes

Status:

- in progress, TLS dual-track gate is closed and accepted

Required work:

- better downstream keepalive handling
- local hot-reload control path (`SIGHUP` + loopback admin endpoint) with atomic config swap semantics
  first cut keeps listener address/port changes as restart-required
- structured access logging and operational controls
- stronger passive and active health behavior
- public API reliability and capacity gates on Linux x86_64 + arm64
- upgrade and rollback validation in disposable environments
- signed release artifacts

Exit criteria:

- dual-track TLS path (external TLS first + built-in Rustls) stays green under workspace tests
- Linux server benchmark campaign completed
- package installation and service lifecycle validation automated in disposable Linux environments
- release signing and checksum verification documented and working
- clear support boundaries published

### Milestone 3: Platform And Protocol Expansion

Goal:

- reduce platform and protocol gaps without losing dependency discipline

Required work:

- Linux arm64 release verification on native runners or self-hosted infra
- Windows packaging refinement
- HTTP/2 decision and plan
- chunked transfer support or stronger documented rejection policy
- richer routing, filters, and policy controls

Exit criteria:

- x86_64 and arm64 Linux artifacts validated consistently
- platform-specific installation notes published
- protocol expansion backed by tests, benchmarks, and operator docs

### Milestone 4: Apache-Grade Project Maturity

Goal:

- operate like a durable open infrastructure project, not just a codebase

Required work:

- maintainer model and support policy
- deprecation policy
- release note discipline and changelog hygiene
- security contact and coordinated disclosure process
- documented benchmark methodology and trend reporting
- contributor onboarding and issue triage rhythm

Exit criteria:

- repository governance is explicit and practiced
- release process is reproducible and auditable
- production claims are backed by repeatable evidence
- community maintenance work is not dependent on one-off knowledge

### Current Benchmark Summary

Local loopback baseline on the current Windows development host suggests:

- stable low-latency path exists for conservative concurrency levels
- upstream keepalive materially improves stability
- local socket churn becomes a bottleneck when reconnecting every request
- final capacity conclusions must wait for Linux server benchmarking

These numbers are tracked for trend detection, not for customer-facing sizing guidance.

### Current Release Summary

Implemented today:

- Windows x86_64 zip
- Linux x86_64 tar.gz
- Linux x86_64 rpm
- Linux arm64 tar.gz
- Linux arm64 rpm
- CI packaging and validation
- CI/release Linux x86_64 + arm64 systemd lifecycle gates
- explicit protocol rejection matrix for unsupported transfer-encoding paths
- `worker_threads` wired into runtime bootstrap with fail-fast validation
- dual-track TLS listener support with fail-fast cert/key checks and passing HTTPS runtime test
- nightly benchmark collection
- checksum generation and verification
- GitHub Release automation by version tag

Still required before stronger production claims:

- local hot-reload control path with atomic config swap and failure rollback behavior
- public API reliability/capacity gates with stable SLO reporting
- stronger per-asset signing policy
- native Linux install automation in disposable environments
- upgrade and rollback verification in disposable environments

## 中文

项目：`Rivulet Gateway / 溪流网关`

基线日期：2026 年 4 月 7 日

这份路线图把当前仓库状态拆成分阶段里程碑，并为每个阶段定义退出标准。
整体策略刻意保守：我们的目标是可安全推进到生产，而不是追求最快功能速度。

### 里程碑 0：内核基线

状态：进行中，主体已建立

当前已有：

- 强类型配置
- HTTP/1.1 路由与代理核心
- 上游健康检查与重试骨架
- 保守的上游 keepalive 复用
- 监听运行时循环
- 打包骨架
- CI、发布自动化和夜间 benchmark 基线

退出标准：

- Linux、Windows、Linux arm64 的现有工作流持续保持绿色
- Linux x86_64 和 Linux arm64 的打包验证在 CI 中通过
- Windows 打包 smoke 保持稳定
- 路线图和治理文档已进入仓库

### 里程碑 1：灰度就绪

目标：

- 让网关足够安全，可承接范围受控的内部灰度流量

必要工作：

- 在真实 Linux 主机上验证 `tar.gz` 和 `rpm`
- 加固下游连接生命周期
- 对暂不支持的请求和响应路径给出更清晰的协议拒绝
- 在 Linux 主机上验证服务安装与启动
- 优化发布产物处理方式，便于运维重复执行

退出标准：

- Linux x86_64 的 staged install 已在真实机器验证
- 保守 benchmark 基线下不再出现无法解释的 `502` 或 socket churn 回归
- 运维文档覆盖启动、停止、验证和回滚基础动作

### 里程碑 2：生产边缘基础

目标：

- 支持受控生产环境中的简单 HTTP/1.1 流量入口

状态：

- 进行中，TLS 双轨门禁已收口并通过验收

必要工作：

- 更完整的下游 keepalive 生命周期处理
- 本地热重载控制路径（`SIGHUP` + loopback 管理端点）与原子配置切换语义
  首版保持“监听地址/端口变更需重启”边界
- 结构化 access log 与运维控制
- 更强的被动与主动健康行为
- Linux x86_64 + arm64 的公网 API 可靠性与容量门禁
- 在一次性环境里完成升级和回滚验证
- 发布产物签名

退出标准：

- 双轨 TLS 路径（外置 TLS 优先 + 内建 Rustls）在 workspace 测试中持续稳定通过
- Linux 服务器 benchmark 活动完成
- 在一次性 Linux 环境中自动化完成安装与服务生命周期验证
- 发布签名和 checksum 校验已文档化并实际跑通
- 对外发布明确的支持边界

### 里程碑 3：平台与协议扩展

目标：

- 在不破坏依赖纪律的前提下缩小平台和协议差距

必要工作：

- 在原生 runner 或自托管基础设施上完成 Linux arm64 发布验证
- 优化 Windows 打包
- 给出 HTTP/2 决策与计划
- 支持 chunked transfer，或给出更强的非支持策略文档
- 更丰富的路由、过滤器与策略控制

退出标准：

- x86_64 与 arm64 Linux 产物都能稳定验证
- 平台专属安装说明已发布
- 协议扩展有测试、benchmark 和运维文档支撑

### 里程碑 4：Apache 级项目成熟度

目标：

- 让项目像长期运营的开源基础设施一样工作，而不只是一个代码仓库

必要工作：

- Maintainer 模型与支持策略
- 弃用策略
- release note 纪律与 changelog 规范
- 安全联络与协调披露机制
- benchmark 方法学与趋势报告文档
- 贡献者 onboarding 与 issue 分流节奏

退出标准：

- 仓库治理清晰且实际执行
- 发布流程可复现、可审计
- 生产声明有可重复证据支撑
- 社区维护工作不依赖一次性口头知识

### 当前 Benchmark 摘要

当前 Windows 开发机上的本地 loopback 基线说明：

- 在保守并发下，低延迟链路已经存在
- 上游 keepalive 对稳定性有明显帮助
- 每请求重连时，本地 socket churn 会更早成为瓶颈
- 最终容量结论必须等 Linux 服务器 benchmark 完成后才能下

这些数字目前只用于趋势观察和瓶颈发现，不用于对客户给出容量承诺。

### 当前发布摘要

今天已经具备：

- Windows x86_64 zip
- Linux x86_64 tar.gz
- Linux x86_64 rpm
- Linux arm64 tar.gz
- Linux arm64 rpm
- CI 打包与验证
- CI/release 中 Linux x86_64 + arm64 的 systemd 生命周期门禁
- 对不支持 transfer-encoding 路径的显式协议拒绝矩阵
- `worker_threads` 已接入 runtime 启动流程并具备快速失败校验
- TLS 双轨监听能力已接入，证书/私钥 fail-fast 校验与 HTTPS 运行时测试通过
- 夜间 benchmark 收集
- checksum 生成与校验
- 基于版本标签的 GitHub Release 自动发布

在做更强生产声明前仍需补齐：

- 本地热重载控制路径（原子切换与失败回滚语义）
- 公网 API 可靠性/容量门禁与稳定 SLO 报告
- 更强的逐产物签名策略
- 一次性环境中的原生 Linux 安装自动化
- 一次性环境中的升级与回滚验证
