# Production Readiness Report / 生产就绪度报告

## English

Report date: April 2, 2026

Project: `Rivulet Gateway / 溪流网关`

This report describes the current boundary of the gateway based on repository state, automated tests, packaging work, and local benchmark exploration.

### Executive Summary

Current status:

- suitable for controlled lab, staging, and low-risk grayscale validation
- not yet ready for broad Internet-facing production traffic

Why:

- core reverse proxy path exists and is tested
- packaging and CI/CD are now repository-owned
- conservative keepalive reuse exists for safe response boundaries
- but the protocol surface and runtime model are still narrow

### What Exists Today

Implemented layers:

- typed config model
- routing
- filter chain skeleton
- upstream registry and health-state tracking
- HTTP/1.1 request parsing
- reverse proxy over raw TCP
- conservative upstream keepalive reuse
- runtime listener loop and graceful drain
- Windows and Linux packaging skeleton
- GitHub Actions for CI, packaging, release, and nightly benchmark collection

### Current Hard Limits

- HTTP/1.1 only
- no TLS termination
- no HTTP/2
- no chunked request support
- no chunked upstream response support
- no downstream request pipelining; current model supports only sequential keepalive requests on one connection
- no real auth, rate limit, WAF, or policy engine yet
- no hot reload or dynamic config plane yet
- no per-asset detached signatures yet (current trust anchor is a keyless-signed checksum manifest)

### Known Engineering Gaps

1. `worker_threads` is still a config field, but it is not yet wired into a custom Tokio runtime builder.
2. Packaging validation now includes CI/release systemd lifecycle gates for Linux x86_64 and arm64, but disposable-VM installation and upgrade coverage is still missing.
3. The benchmark harness is intentionally conservative and local; it is not a substitute for server-grade load testing on Linux.
4. Release checksums, keyless manifest signing, SBOM export, and provenance attestations now exist, but stronger per-asset signing policy and trust publication still need refinement.

### Validation Completed

Repository validation:

- workspace tests
- protocol boundary tests
- keepalive reuse tests
- Windows package build and smoke validation
- Linux package structure validation designed into CI
- Linux installed-layout smoke validation designed into CI and release workflows
- Linux systemd lifecycle validation gated in CI and release workflows for x86_64 and arm64 artifacts

Key validation entry points:

- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)
- [packaging/tests/run-linux-validation.sh](C:\Users\brace\Documents\New%20project\packaging\tests\run-linux-validation.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)

### Local Benchmark Snapshot

Environment used:

- Windows 10 Pro build 19045
- Intel i7-6500U
- 2 physical cores / 4 logical processors
- 16 GB RAM
- Rust 1.92.0

Important caution:

- these numbers are local loopback baselines, not release-quality server benchmarks
- they are useful for trend tracking and bottleneck discovery, not final capacity planning

Observed stable path with `upstream_idle_pool_size = 1`:

- 64-byte response, concurrency 8: about 1561 req/s, p95 about 9.9 ms
- 64-byte response, concurrency 32: about 1266 req/s, p95 about 58.8 ms
- 4096-byte response, concurrency 8: about 1590 req/s, p95 about 11.0 ms
- 4096-byte response, concurrency 64: about 1734 req/s, p95 about 57.1 ms

Observed warning sign:

- when upstream keepalive is disabled and every request reconnects, this Windows host can hit `502` responses and socket churn behavior much earlier
- this strongly suggests connection churn and local socket lifecycle become a bottleneck before the gateway core itself is fully exercised

Engineering conclusion:

- conservative upstream connection reuse is already materially important for stability
- future production exploration should prioritize Linux hosts and real NIC traffic before drawing capacity conclusions

### Packaging And Release Readiness

Current state:

- Windows x86_64 zip: implemented and locally validated
- Linux x86_64 tar.gz: implemented in scripts and workflows
- Linux x86_64 rpm: implemented in scripts and workflows
- Linux arm64 tar.gz/rpm: implemented in scripts and workflows

Release automation status:

- CI builds and validates package artifacts
- release workflow produces artifacts and checksum manifest
- checksum verification script exists
- release workflow signs `SHA256SUMS.txt` with keyless Sigstore (`SHA256SUMS.sig` + `SHA256SUMS.pem`)
- CI/release workflows emit SBOM and provenance attestations
- CI/release workflows block publish when Linux x86_64 or arm64 systemd lifecycle gates fail

Remaining release-grade work:

- stronger per-asset detached signature policy
- native Linux install verification in disposable test systems
- upgrade and rollback verification in disposable test systems

### Production Use Guidance Right Now

Reasonable near-term use:

- development environments
- CI integration tests
- internal staging
- low-risk grayscale routes with tight scope and rollback control

Not recommended yet:

- public edge gateway for mixed client traffic
- TLS termination at scale
- multi-tenant policy enforcement
- high-throughput production ingress without Linux server benchmarking and installation validation

### Next Bottlenecks To Address

1. Linux real-host benchmark and disposable-environment install/upgrade validation
2. downstream keepalive lifecycle improvements beyond conservative sequential mode
3. chunked transfer support or explicit non-support enforcement across all edges
4. TLS and certificate lifecycle design
5. runtime configurability and operations plane
6. signed release process and public support policy

## 中文

报告日期：2026 年 4 月 2 日

项目：`Rivulet Gateway / 溪流网关`

本报告基于仓库现状、自动化测试、打包工作和本地 benchmark 探索，描述当前网关的能力边界。

### 执行摘要

当前状态：

- 适合受控实验环境、staging 环境和低风险灰度验证
- 还不适合承接大范围公网生产流量

原因：

- 核心反向代理链路已经存在并且有测试覆盖
- 打包和 CI/CD 已经内建到仓库
- 对安全响应边界已有保守的 keepalive 复用
- 但协议面和运行时模型仍然偏窄

### 当前已具备能力

已实现层次：

- 强类型配置模型
- 路由
- 过滤器链骨架
- 上游注册中心与健康状态跟踪
- HTTP/1.1 请求解析
- 基于原始 TCP 的反向代理
- 保守的上游 keepalive 复用
- 监听运行时与优雅 drain
- Windows 与 Linux 打包骨架
- 用于 CI、打包、发布和夜间 benchmark 收集的 GitHub Actions

### 当前硬边界

- 仅支持 HTTP/1.1
- 没有 TLS termination
- 没有 HTTP/2
- 不支持 chunked request
- 不支持 chunked upstream response
- 没有下游请求 pipelining；当前模型只支持同连接顺序 keepalive 请求
- 还没有真正的认证、限流、WAF 或策略引擎
- 还没有热更新或动态配置面
- 还没有“每个产物单独签名”的完整策略（当前信任锚是 keyless 签名的 checksum 清单）

### 已知工程缺口

1. `worker_threads` 仍只是配置字段，还没有接入自定义 Tokio runtime builder。
2. 打包验证已覆盖 CI/release 中 Linux x86_64 与 arm64 的 systemd 生命周期门禁，但一次性 VM 环境中的安装与升级覆盖仍然缺失。
3. benchmark 工具当前故意保持保守且只跑本地，不可替代 Linux 服务器级负载测试。
4. 发布 checksum、keyless 清单签名、SBOM 与 provenance 已接入，但按单个产物逐一签名和更强信任链发布仍需完善。

### 已完成验证

仓库级验证：

- workspace tests
- 协议边界测试
- keepalive 复用测试
- Windows 打包构建与 smoke 验证
- Linux 包结构验证已接入 CI
- Linux 安装后布局 smoke 验证已接入 CI 和 release workflow
- Linux x86_64 与 arm64 产物的 systemd 生命周期验证已接入 CI 与 release 门禁

关键验证入口：

- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)
- [packaging/tests/run-linux-validation.sh](C:\Users\brace\Documents\New%20project\packaging\tests\run-linux-validation.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)

### 本地 Benchmark 快照

测试环境：

- Windows 10 Pro build 19045
- Intel i7-6500U
- 2 个物理核心 / 4 个逻辑处理器
- 16 GB RAM
- Rust 1.92.0

重要提示：

- 这些数字只是本地 loopback 基线，不是可直接对外宣称的服务器 benchmark
- 它们适合用来做趋势跟踪和瓶颈发现，不适合直接用于容量规划

当 `upstream_idle_pool_size = 1` 时，观察到的稳定路径：

- 64 字节响应、并发 8：约 1561 req/s，p95 约 9.9 ms
- 64 字节响应、并发 32：约 1266 req/s，p95 约 58.8 ms
- 4096 字节响应、并发 8：约 1590 req/s，p95 约 11.0 ms
- 4096 字节响应、并发 64：约 1734 req/s，p95 约 57.1 ms

观察到的警示信号：

- 当关闭上游 keepalive、每个请求都重连时，这台 Windows 主机会更早出现 `502` 和 socket churn 行为
- 这说明在网关核心还没有被完全打满之前，连接 churn 和本地 socket 生命周期就已经先成为瓶颈

工程结论：

- 保守的上游连接复用已经对稳定性产生了实质帮助
- 后续生产探索应优先转向 Linux 主机和真实网卡流量，再做容量判断

### 打包与发布就绪度

当前状态：

- Windows x86_64 zip：已实现并完成本地验证
- Linux x86_64 tar.gz：已在脚本和工作流中实现
- Linux x86_64 rpm：已在脚本和工作流中实现
- Linux arm64 tar.gz/rpm：已在脚本和工作流中实现

发布自动化状态：

- CI 会构建并验证打包产物
- release workflow 会产出发行包和 checksum 清单
- 已有 checksum 校验脚本
- release workflow 会对 `SHA256SUMS.txt` 做 keyless Sigstore 签名（`SHA256SUMS.sig` + `SHA256SUMS.pem`）
- CI/release workflow 会产出 SBOM 与 provenance
- CI/release workflow 在 Linux x86_64 或 arm64 的 systemd 生命周期门禁失败时会阻断后续发布

距离更高等级发布还需补齐：

- 更强的按单个产物逐一 detached 签名策略
- 在一次性测试系统中完成原生 Linux 安装验证
- 在一次性测试系统中完成升级与回滚验证

### 当前生产使用建议

当前合理的近端使用场景：

- 开发环境
- CI 集成测试
- 内部 staging
- 范围受控、可快速回滚的低风险灰度路由

当前不建议：

- 面向公网混合客户端的边缘网关
- 大规模 TLS termination
- 多租户策略执行
- 在缺少 Linux 服务器 benchmark 与安装验证前承接高吞吐生产入口

### 下一批瓶颈

1. Linux 真实主机 benchmark 与一次性环境安装/升级验证
2. 下游 keepalive 生命周期继续增强（超出当前保守顺序模式）
3. 对 chunked transfer 的支持，或更彻底的显式非支持策略
4. TLS 与证书生命周期设计
5. 运行时可配置性与运维平面
6. 签名发布流程与公开支持策略
