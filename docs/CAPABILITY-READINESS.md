# Capability Readiness / 能力就绪度

## English

This document maps the requested gateway capabilities to the current repository state.

### Available now

- Auth entry: first conservative cut is available now through route-level static bearer token and query token policies
- Route aggregation: basic capability is already available now through listener + host + path prefix + method matching into upstream routing
- Rate limiting and public-page protection: first conservative cut is available now through route-level in-memory fixed-window limiting keyed by client IP
- Shared-access isolation: conservative capability is available now through route-level share grants that resolve query tokens into stable `share_id` and `scope`, enforce optional `resource_prefixes` boundaries, and inject `X-Rivulet-Share-Id` / `X-Rivulet-Share-Scope` upstream

### Not complete yet

- Shared-access isolation: resource-prefix boundary control is available, but finer-grained resource binding, revocation, dynamic policy sources, and stronger audit controls are not complete yet
- Rate limiting and public-page protection: the first cut is available, but richer key extraction, distributed counters, and stronger abuse heuristics are not complete yet

### Recommended milestone mapping

- Milestone 1.x: keep strengthening auth entry, install validation, and grayscale operability
- Milestone 2: keep hardening scoped share isolation and conservative in-memory rate limiting for protected or public routes
- Later Milestone 2+: move from static policies to more production-shaped policy composition after observability and runtime controls are stronger

### Current implementation boundary

- no external identity provider integration yet
- no dynamic credential storage yet
- no dynamic share grant storage or revocation source yet
- no distributed rate-limit counter store yet
- no advanced public-route abuse heuristics yet

The current priority remains: keep the kernel narrow, correct, observable, and operable before widening the policy surface.

## 中文

这份文档把当前关心的几类网关能力，映射到仓库现状。

### 现在已经可用

- 鉴权入口：现在已经具备第一版保守能力，可在路由级使用静态 Bearer Token 和 Query Token 做入口保护
- 路由聚合：基础能力现在就可用，已经支持 listener + host + path prefix + method 到 upstream 的路由聚合
- 限流与公开页面保护：现在已经具备第一版保守能力，可在路由级按客户端 IP 启用内存固定窗口限流
- 分享访问隔离：现在已经具备保守可用能力，可在路由级用分享授权把 query token 解析成稳定的 `share_id` 与 `scope`，按可选 `resource_prefixes` 约束访问路径，并以上游请求头 `X-Rivulet-Share-Id`、`X-Rivulet-Share-Scope` 传递给后端

### 还没完整具备

- 分享访问隔离：资源前缀边界已可用，但更细粒度的资源绑定、撤销机制、动态策略来源和审计控制还没完成
- 限流与公开页面保护：第一版已经可用，但更丰富的 key 提取、分布式计数和更强的公开页滥用防护还没完成

### 建议按里程碑理解

- 里程碑 1.x：继续加固鉴权入口、安装后验证和灰度可运维性
- 里程碑 2：继续加固带作用域的分享访问隔离，以及面向受保护或公开页面的保守型内存限流
- 里程碑 2 后段：在可观测性和运行时控制更强之后，再把静态策略演进成更接近生产形态的策略组合

### 当前实现边界

- 还没有接入外部身份提供方
- 还没有动态凭据存储
- 还没有动态分享授权存储或撤销来源
- 还没有分布式限流计数存储
- 还没有更高级的公开页面滥用防护启发式

当前优先级仍然是：在扩大策略面之前，先把内核做窄、做稳、做可观测、做可运维。
