# Milestones / 里程碑路线图

## English

Project: `Rivulet Gateway / 溪流网关`  
Baseline date: April 8, 2026

This roadmap tracks production-readiness progress with conservative, auditable exits.

### Milestone 0: Kernel Baseline

Status: completed

Delivered:

- typed config, routing, proxy core
- upstream registry, retry, and keepalive baseline
- workspace tests and packaging skeleton
- CI and release automation baseline

### Milestone 1: Grayscale Readiness

Status: completed

Delivered:

- protocol rejection matrix for unsupported HTTP/1.1 paths
- Linux package install/smoke/systemd lifecycle baseline
- cross-platform package outputs (Windows x86_64, Linux x86_64, Linux arm64, rpm included)

### Milestone 2: Production Edge Foundation

Status: in progress, major core gates closed

Closed in Milestone 2:

- route auth entry, share isolation (with resource prefixes), and first rate-limit layer
- worker thread configuration wired into runtime with fail-fast validation
- dual-track TLS foundation (external TLS first + built-in Rustls path)
- local hot reload (`SIGHUP` + loopback admin reload) with validate-first atomic swap
- runtime and access-log observability extension
- G3 public API gate execution chain connected and blocking in CI/release for Linux `x86_64` and `arm64`

Remaining in Milestone 2:

- threshold tuning for capacity variance across runner classes
- larger disposable-environment lifecycle coverage (install/upgrade/rollback/uninstall)
- stronger detached signing policy per release asset

### Milestone 3: Public Internet Scale Hardening

Status: not started

Planned focus:

- higher-confidence capacity model on dedicated Linux benchmark hosts
- stronger fault-injection suites and longer soak windows
- broader operational controls for multi-team incident response

### Milestone 4: Apache-Grade Project Maturity

Status: not started

Planned focus:

- governance and maintainer model hardening
- release/process auditability depth
- long-term contributor and security response workflows

### Current Release Summary

Current repository release chain already includes:

- CI packaging and validation for Windows x86_64, Linux x86_64, Linux arm64
- RPM generation for Linux x86_64 and Linux arm64
- CI/release systemd lifecycle gates for Linux x86_64 and arm64
- executable public API reliability gate in CI/release (blocking `standard` profile)
- workflow artifacts for public API gate (`result.json` + `summary.md`)

### Current Bottleneck

The next bottleneck is no longer gate wiring.  
The next bottleneck is stable threshold calibration and long-window capacity confidence across Linux environments.

## 中文

项目：`Rivulet Gateway / 溪流网关`  
基线日期：2026 年 4 月 8 日

本路线图用于追踪生产可用推进，强调保守推进与可审计退出标准。

### 里程碑 0：内核基线

状态：已完成

已交付：

- 强类型配置、路由、代理内核
- 上游注册、重试与 keepalive 基线能力
- workspace 测试与打包骨架
- CI 与 release 自动化基线

### 里程碑 1：灰度就绪

状态：已完成

已交付：

- HTTP/1.1 非支持路径协议拒绝矩阵
- Linux 安装/smoke/systemd 生命周期验证基线
- 跨平台产物输出（Windows x86_64、Linux x86_64、Linux arm64，含 rpm）

### 里程碑 2：生产边缘基础

状态：进行中，核心门禁已大幅收口

里程碑 2 已收口内容：

- 路由鉴权入口、分享隔离（含资源前缀）、限流第一层
- `worker_threads` 真实接线并具备 fail-fast 校验
- TLS 双轨基础能力（外置 TLS 优先 + 内建 Rustls 路径）
- 本地热重载（`SIGHUP` + loopback 管理面），先校验后原子切换
- 运行时与访问日志可观测字段增强
- G3 公网 API 门禁执行链已接通，并在 CI/release 对 Linux `x86_64` 与 `arm64` 阻断生效

里程碑 2 剩余内容：

- 面向不同 runner 资源波动的阈值持续校准
- 一次性环境安装/升级/回滚/卸载验证规模扩展
- 发布逐产物 detached 签名策略增强

### 里程碑 3：公网规模化加固

状态：未开始

计划重点：

- 在专用 Linux 基准机上建立更高置信容量模型
- 增强故障注入与更长稳态窗口验证
- 面向多团队协作的运维与应急控制能力

### 里程碑 4：Apache 级项目成熟度

状态：未开始

计划重点：

- 治理与维护者模型加固
- 发布与流程审计深度提升
- 长期贡献者与安全响应机制完善

### 当前发布链路摘要

当前仓库发布链路已具备：

- Windows x86_64、Linux x86_64、Linux arm64 的 CI 打包与验证
- Linux x86_64 与 Linux arm64 的 RPM 产物生成
- CI/release 上 Linux x86_64 与 arm64 的 systemd 生命周期门禁
- CI/release 上可执行公网 API 可靠性门禁（`standard` 阻断档）
- 公网 API 门禁审计产物归档（`result.json` + `summary.md`）

### 当前主瓶颈

当前瓶颈已从“门禁是否接通”转移为“阈值是否稳定、容量模型是否在 Linux 环境下具备长期置信度”。
