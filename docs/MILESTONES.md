# Milestones / 里程碑路线图

## English

Project: `Rivulet Gateway / 溪流网关`  
Baseline date: April 8, 2026

### Milestone 0: Kernel baseline

Status: completed

- typed config, routing, proxy kernel
- workspace test baseline
- repository-owned CI/release baseline

### Milestone 1: Grayscale readiness

Status: completed

- protocol boundary rejection matrix
- Linux package/systemd validation baseline
- cross-platform packaging outputs (Windows x86_64, Linux x86_64, Linux arm64, rpm included)

### Milestone 2: Production edge foundation

Status: in progress (major gates closed)

Closed:

- route auth entry, share isolation, first rate-limit layer
- worker thread runtime wiring with fail-fast validation
- dual-track TLS foundation
- hot reload first cut (`SIGHUP` + loopback admin reload)
- runtime and access-log observability extension
- public API gate execution path connected and blocking in CI/release

Newly closed in G3.1:

- architecture-aware threshold model for `standard|strict|observe`
- long-run evaluate defaults (`baseline/soak/failure-drill`)
- 3-sample median scoring with request-floor hard checks
- explicit failure reasons in gate outputs

Newly closed in G3.1.3:

- public-api gate diagnostics are emitted directly in evaluate logs (`failure_reasons` / `threshold_checks` / `observed`)
- gate fixture runtime for standard evaluate uses retry budget `2` (without threshold relaxation)
- proxy kernel supports bounded same-endpoint retry for transient upstream I/O in single-endpoint clusters

Newly closed in G3.1.4:

- single-endpoint retryable upstream status (`500/502/503/504`) now preserves pass-through semantics and no longer downgrades into gateway-side 5xx by exclusion fallback
- multi-endpoint retry behavior remains unchanged for retryable upstream status
- failure-drill outputs now include auditable diagnostics (`business_total_requests`, `business_gateway_5xx_ratio`, gateway fault breakdown and key error counters)

Newly closed in G3.2:

- release publish pipeline now signs every release asset with detached signature and certificate outputs (`<asset>.sig` + `<asset>.pem`)
- release publish job now hard-blocks on detached signature verification before upload
- CI/release supply-chain workflows now enforce signature-contract checks and use Node 24 compatible JavaScript action runtime setting

Remaining in Milestone 2:

- threshold tuning iteration on stable Linux runners
- disposable full lifecycle coverage expansion

### Milestone 3: Public Internet scale hardening

Status: not started

- larger capacity confidence on dedicated Linux benchmark hosts
- longer soak and stronger failure-injection suites
- operation controls for multi-team incident workflows

### Milestone 4: Apache-grade maturity

Status: not started

- governance and maintainer model hardening
- release/process audit depth
- contributor and security response sustainability

## 中文

项目：`Rivulet Gateway / 溪流网关`  
基线日期：2026 年 4 月 8 日

### 里程碑 0：内核基线

状态：已完成

- 强类型配置、路由与代理内核
- workspace 测试基线
- 仓库自持的 CI/release 基线

### 里程碑 1：灰度就绪

状态：已完成

- 协议边界拒绝矩阵
- Linux 打包与 systemd 验证基线
- 跨平台产物输出（Windows x86_64、Linux x86_64、Linux arm64，含 rpm）

### 里程碑 2：生产边缘基础

状态：进行中（核心门禁已收口）

已收口：

- 路由鉴权入口、分享隔离、限流第一层
- `worker_threads` 运行时接线与 fail-fast 校验
- TLS 双轨基础能力
- 热重载首版（`SIGHUP` + loopback 管理面 reload）
- 运行时与访问日志可观测增强
- 公网 API 门禁执行链在 CI/release 已阻断生效

G3.1 新增收口：

- `standard|strict|observe` 按架构阈值模型
- 长跑默认窗口（baseline/soak/failure-drill）
- 3 次采样中位数判分 + 最小样本硬门槛
- 门禁产物可区分具体失败原因

G3.1.3 新增收口：

- public-api gate evaluate 日志直接输出关键诊断字段（`failure_reasons` / `threshold_checks` / `observed`）
- standard evaluate 工况重试预算提升到 `2`（不放宽阈值）
- 代理内核在单节点 upstream 场景支持上游瞬态 I/O 的有界同节点重试

G3.1.4 新增收口：

- 单节点 retryable 上游状态（`500/502/503/504`）改为保持透传语义，不再因“排除唯一节点”的回退路径被误转成网关侧 5xx
- 多节点场景对 retryable 上游状态的重试/切换行为保持不变
- failure-drill 产物新增可审计诊断字段（`business_total_requests`、`business_gateway_5xx_ratio`、网关故障拆分与关键错误计数）

G3.2 新增收口：

- release 发布链路已对每个发布资产生成 detached 签名与证书产物（`<asset>.sig` + `<asset>.pem`）
- release publish 在上传前新增逐产物验签硬门禁，验签失败会直接阻断发布
- CI/release supply-chain 流程新增签名契约检查，并启用 Node 24 兼容 JavaScript action 运行时设置

里程碑 2 剩余项：

- 在稳定 Linux runner 上持续校准阈值
- 一次性环境全生命周期验证规模扩展

### 里程碑 3：公网规模化加固

状态：未开始

- 在专用 Linux 压测机上提升容量置信度
- 拉长 soak 与增强故障注入演练
- 面向多团队应急协同的运维控制能力

### 里程碑 4：Apache 级成熟度

状态：未开始

- 治理与维护者模型加固
- 发布与流程审计深度提升
- 贡献者与安全响应机制长期可持续化
