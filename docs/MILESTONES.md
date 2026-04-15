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

Newly closed in G3.1.5:

- arm64 `failure_drill_not_passed` stability issue is closed without lowering `standard` thresholds
- business `503` drill now uses 3-sample median policy for gateway contamination checks (`business_gateway_5xx_median == 0`)
- upstream transient I/O errors now include stable `upstream_io/<kind>:` diagnostic prefixes for CI triage
- failure-drill outputs now include `business_samples`, `business_gateway_5xx_median`, `business_gateway_5xx_max`, and `business_pass_policy`
- 中文同步：本批属于“语义加固 + 抗抖收口”，不属于阈值放宽；arm64 失败路径通过三采样中位数判定与上游 I/O 前缀诊断收敛。

Newly closed in G3.2:

- release publish pipeline now signs every release asset with detached signature and certificate outputs (`<asset>.sig` + `<asset>.pem`)
- release publish job now hard-blocks on detached signature verification before upload
- CI/release supply-chain workflows now enforce signature-contract checks and use Node 24 compatible JavaScript action runtime setting

Newly closed in G3.3:

- release now blocks on disposable lifecycle validation across Linux x86_64 and Linux arm64
- `tar.gz` release path is gated by full lifecycle orchestration (`install -> upgrade -> rollback -> uninstall`)
- `rpm` release path is gated by install/uninstall lifecycle validation
- CI now includes lightweight disposable lifecycle contract checks to catch script-interface drift before release

Newly closed in G3.4 (tooling baseline):

- nightly now collects dual-architecture observe samples (`linux-x86_64` + `linux-arm64`) for threshold calibration evidence
- nightly summary now generates calibration report artifacts (`calibration-report.json` / `calibration-report.md`)
- nightly summary now generates CI/release streak artifacts (`streak-report.json` / `streak-report.md`) for milestone-2 closure tracking

Newly closed in G3.4.1 (closure execution tooling):

- calibration report now includes conservative recommendation fields (`recommended_thresholds` + `change_budget`)
- nightly summary now generates threshold PR review artifact (`threshold-pr-checklist.md`)
- nightly summary now generates milestone-2 closure synthesis artifacts (`milestone2-closure-status.json` / `milestone2-closure-status.md`)
- streak reports now expose `closure_ready` and `remaining_to_target` for explicit closure tracking

Newly closed in G3.4.2 (closure execution reinforcement):

- nightly summary now generates `threshold-change-proposal.md` for architecture-aware `standard` threshold proposal packaging
- milestone closure synthesis now exposes `ci_closure_ready` and `release_closure_ready` as top-level fields
- milestone closure synthesis now aggregates `recent_failure_reasons` from streak run details
- nightly summary now generates `closure-weekly-report.md` for observe sample trend and closure evidence maturity

G3.4.2 新增收口（执行加固）：

- nightly 汇总新增 `threshold-change-proposal.md`，用于按架构 `standard` 阈值建议包
- 里程碑收口状态新增顶层字段：`ci_closure_ready`、`release_closure_ready`
- 里程碑收口状态新增 `recent_failure_reasons`（基于 streak run 明细聚合）
- nightly 汇总新增 `closure-weekly-report.md`，用于 observe 趋势与收口证据成熟度周报

G3.4.3 新增收口（冲刺运营闭环）：

- nightly 汇总新增统一审阅入口 `m2-nightly-review-package.md`（固定证据顺序与决策规则）
- nightly 汇总新增切线判定产物 `m2-cutover-check.json/.md`（用于 M2->M3 切线门槛）
- 收口执行口径固化为“先审阅包、后判定、再动作”，避免跨轮次执行漂移

G3.4.1 新增收口（执行工具链）:

- 校准报告新增保守建议字段：`recommended_thresholds` 与 `change_budget`
- nightly 汇总新增 `threshold-pr-checklist.md`（阈值 PR 人工评审清单）
- nightly 汇总新增 `milestone2-closure-status.json/.md`（里程碑 2 收口状态汇总）
- streak 报告新增 `closure_ready` 与 `remaining_to_target`（连续全绿收口追踪）

Remaining in Milestone 2:

- threshold tuning iteration on stable Linux runners
- reach closure target: 10 consecutive dual-architecture green runs under current `standard` blocking profile

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

G3.3 新增收口：

- release 已在 Linux x86_64 与 Linux arm64 接入一次性环境全生命周期门禁阻断
- `tar.gz` 路径已由全生命周期编排脚本执行 `安装 -> 升级 -> 回滚 -> 卸载`
- `rpm` 路径已接入 `安装 -> 卸载` 生命周期验证
- CI 已新增一次性环境全生命周期脚本的轻量契约检查，用于提前发现脚本接口漂移

G3.4 新增收口（工具链基线）：

- nightly 已新增双架构 observe 样本采集（`linux-x86_64` + `linux-arm64`），用于阈值校准证据沉淀
- nightly 汇总已新增校准报告产物（`calibration-report.json` / `calibration-report.md`）
- nightly 汇总已新增 CI/release 连续全绿统计产物（`streak-report.json` / `streak-report.md`）

里程碑 2 剩余项：

- 在稳定 Linux runner 上持续校准阈值
- 在当前 `standard` 阻断口径下达成“连续 10 次双架构全绿”收口目标

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
