# Admin UI / 管理面设计

## English

### Goal

The first admin UI cut gives Rivulet Gateway a built-in, read-only operational surface without adding a Node-based frontend toolchain or exposing risky write paths.

### Workspace boundary

- `crates/gateway-admin`
  - owns native `HTML + CSS + JS` assets
  - owns read-only admin HTTP response assembly
  - owns minimal JSON rendering for the admin overview payload
- `crates/gateway-runtime`
  - owns runtime snapshot assembly from config and metrics
  - implements `AdminOverviewProvider`
  - injects the admin service into the running gateway
- `crates/gateway-proxy`
  - owns the `/__admin` short-circuit
  - serves admin responses before route resolution and upstream forwarding
  - keeps admin traffic isolated from normal business routing

### Access model

- Entry path: `/__admin/`
- JSON overview API: `/__admin/api/overview`
- Static assets:
  - `/__admin/index.html`
  - `/__admin/styles.css`
  - `/__admin/app.js`

### Security posture in the first cut

- Read-only only
- Loopback-only access
- No config writeback
- No admin auth workflow yet because the surface is intentionally not exposed publicly
- No dependency on external frontend package managers or build steps

### Current page scope

- runtime summary cards
- runtime stats cards
- runtime settings cards
- listener table
- route table
- upstream table
- explicit workspace-boundary explanation on the page itself

### Why this shape

- Keeps the management plane observable without expanding protocol risk too early
- Preserves the project rule of minimizing third-party dependency surface
- Makes local first-test, gray rollout, and issue triage easier
- Leaves space for later iterations such as auth, config diff, health views, and controlled mutations

### Verification

- `cargo test --workspace`
- loopback requests to `/__admin/` return `200`
- loopback requests to `/__admin/api/overview` return JSON
- non-loopback admin access is rejected with `403`

## 中文

### 目标

管理面第一版的目标，是在不引入 Node 前端工具链、不开放高风险写操作的前提下，为溪流网关提供一个内置、只读、可直接用于排障和验收的运维观察入口。

### Workspace 边界

- `crates/gateway-admin`
  - 负责原生 `HTML + CSS + JS` 静态资产
  - 负责只读管理接口的 HTTP 响应拼装
  - 负责管理面总览 JSON 的最小序列化输出
- `crates/gateway-runtime`
  - 负责把配置与运行时指标汇总成快照
  - 实现 `AdminOverviewProvider`
  - 在应用装配阶段把管理面服务注入运行中的网关
- `crates/gateway-proxy`
  - 负责 `/__admin` 命名空间的短路
  - 在路由匹配和上游转发之前直接处理管理请求
  - 保证管理流量与正常业务路由链分离

### 访问模型

- 页面入口：`/__admin/`
- JSON 总览接口：`/__admin/api/overview`
- 静态资源：
  - `/__admin/index.html`
  - `/__admin/styles.css`
  - `/__admin/app.js`

### 第一版安全边界

- 仅只读
- 仅允许 loopback 访问
- 不支持配置写回
- 暂不引入管理鉴权流程，因为第一版刻意不对公网暴露
- 不依赖外部前端包管理器和构建步骤

### 当前页面范围

- 运行摘要卡片
- 运行指标卡片
- 运行时参数卡片
- listener 表格
- route 表格
- upstream 表格
- 页面内直接展示 workspace 边界说明

### 为什么这样设计

- 先把管理面可观测性建立起来，但不提前扩大协议和权限风险
- 延续项目“尽量减少第三方依赖面”的原则
- 让本地首测、灰度观察、线上排障更直接
- 为后续鉴权、配置 diff、健康视图、受控写操作预留清晰扩展点

### 验证方式

- `cargo test --workspace`
- loopback 访问 `/__admin/` 返回 `200`
- loopback 访问 `/__admin/api/overview` 返回 JSON
- 非 loopback 管理访问返回 `403`
