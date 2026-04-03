# Rivulet Gateway / 溪流网关

## English

A Rust gateway built from zero with a strict dependency policy.

Working name:

- English: `Rivulet Gateway`
- Chinese: `溪流网关`

Current design rules:

- Keep core abstractions in-house.
- Only add dependencies when the risk and maintenance cost are justified.
- Stabilize config, routing, filters, upstream, and runtime boundaries first.
- Ship a narrow but correct HTTP gateway before adding more protocol surface.

Engineering standard from today onward:

- Build toward Apache-grade engineering discipline across development, testing, load testing, CI/CD, packaging, release notes, documentation, and community hygiene.
- Prefer explicit design docs, reproducible scripts, and minimal release-time dependencies.
- Treat portability as a first-class requirement for Windows, Linux x86_64, and Linux arm64.
- Keep project governance and distribution assets close to the source tree instead of hiding them in external tooling.
- Make CI/CD and packaging reproducible through repository-owned GitHub Actions workflows.
- Keep governance, security, and contributor policy explicit in-repo.

Current kernel cut:

- Cargo workspace with focused crates
- read-only admin UI workspace member with native HTML/CSS/JS assets
- strongly typed config model
- route matching core
- filter chain abstraction
- upstream registry with round-robin selection
- minimal HTTP/1.1 request parser
- minimal reverse proxy path over raw TCP
- listener runtime with graceful stop signal
- conservative upstream keepalive reuse
- local benchmark harness
- initial multi-platform packaging skeleton

Current limits:

- HTTP/1.1 only
- admin UI is read-only and loopback-only in the first cut (`/__admin/`)
- one request per connection
- `Content-Length` request bodies only
- no chunked request support yet
- no `Expect: 100-continue` support yet
- no TLS yet
- `worker_threads` config is not yet wired into a custom Tokio runtime
- Linux package and RPM flow are scaffolded, but native target build hosts are still preferred

Run:

```powershell
cargo run -p gateway-main -- config/gateway.toml
```

CI/CD:

- CI workflow: `.github/workflows/ci.yml`
- release workflow: `.github/workflows/release.yml`
- nightly benchmark workflow: `.github/workflows/nightly-benchmark.yml`
- workflow architecture: `docs/WORKFLOW-ARCHITECTURE.md`
- admin UI design: `docs/ADMIN-UI.md`
- Linux first-test checklist: `docs/LINUX-SERVER-FIRST-TEST.md`
- Linux baseline benchmark: `docs/LINUX-BASELINE-BENCHMARK.md`
- Linux release automation: `docs/LINUX-RELEASE-AUTOMATION.md`
- Linux systemd validation: `docs/LINUX-SYSTEMD-VALIDATION.md`
- Linux upgrade rollback validation: `docs/LINUX-UPGRADE-ROLLBACK-VALIDATION.md`
- Linux service operations: `docs/LINUX-SERVICE-OPERATIONS.md`
- packaging guide: `packaging/README.md`

Project policy docs:

- contribution guide: `CONTRIBUTING.md`
- security policy: `SECURITY.md`
- governance: `GOVERNANCE.md`
- code of conduct: `CODE_OF_CONDUCT.md`
- production readiness report: `docs/PRODUCTION-READINESS.md`
- milestone roadmap: `docs/MILESTONES.md`
- release process: `docs/RELEASE-PROCESS.md`
- release checklist: `docs/RELEASE-CHECKLIST.md`

## 中文

这是一个从零开始构建、严格控制依赖面的 Rust 网关项目。

当前名称：

- 英文名：`Rivulet Gateway`
- 中文名：`溪流网关`

当前架构原则：

- 核心抽象优先自研，避免把关键边界交给外部黑盒。
- 只有当风险、维护成本和收益都清晰可控时，才引入新依赖。
- 先把配置、路由、过滤器、上游、运行时这些核心边界做稳。
- 先交付一个能力收敛但行为正确的 HTTP 网关，再逐步扩协议面。

从今天起的工程标准：

- 以 Apache 级别的工程纪律为目标推进开发、测试、压测、CI/CD、打包、发布、文档和社区治理。
- 优先选择显式设计文档、可复现实验脚本和轻量级发版依赖。
- 把 Windows、Linux x86_64、Linux arm64 的可移植性当作一等要求。
- 把治理、分发和发布资产放在仓库内，避免依赖外部隐式流程。
- 通过仓库自带的 GitHub Actions 保证 CI/CD 与打包流程可复现。
- 让治理、安全和贡献规则都在仓库内清晰可见。

当前内核能力：

- 基于 Cargo workspace 的模块化 crate 结构
- 强类型配置模型
- 路由匹配核心
- 过滤器链抽象
- 带轮询选择的上游注册中心
- 最小化 HTTP/1.1 请求解析器
- 基于原始 TCP 的最小反向代理链路
- 支持优雅停止的监听运行时
- 保守的上游 keepalive 复用
- 本地 benchmark 基线工具
- 初版多平台打包骨架

当前限制：

- 仅支持 HTTP/1.1
- 每个下游连接当前只处理一个请求
- 请求体仅支持 `Content-Length`
- 暂不支持 chunked request
- 暂无 TLS
- `worker_threads` 配置项尚未接入自定义 Tokio runtime
- Linux 安装包和 RPM 流程已具备骨架，但仍更推荐在原生目标平台构建

运行方式：

```powershell
cargo run -p gateway-main -- config/gateway.toml
```

CI/CD 入口：

- CI 工作流：`.github/workflows/ci.yml`
- 发布工作流：`.github/workflows/release.yml`
- 夜间 benchmark 工作流：`.github/workflows/nightly-benchmark.yml`
- 工作流架构图：`docs/WORKFLOW-ARCHITECTURE.md`
- Linux 首测清单：`docs/LINUX-SERVER-FIRST-TEST.md`
- Linux 基线压测：`docs/LINUX-BASELINE-BENCHMARK.md`
- Linux 自动化验证：`docs/LINUX-RELEASE-AUTOMATION.md`
- 打包说明：`packaging/README.md`

项目治理文档：

- 贡献指南：`CONTRIBUTING.md`
- 安全策略：`SECURITY.md`
- 治理文档：`GOVERNANCE.md`
- 行为准则：`CODE_OF_CONDUCT.md`
- 生产就绪度报告：`docs/PRODUCTION-READINESS.md`
- 里程碑路线图：`docs/MILESTONES.md`
- 发布流程：`docs/RELEASE-PROCESS.md`
- 发布检查清单：`docs/RELEASE-CHECKLIST.md`

管理面补充：

- 已新增独立 workspace member：`crates/gateway-admin`
- 前端仅使用原生 `HTML + CSS + JS`，不引入 Node 构建链
- 当前仅提供只读管理面，默认访问入口为 `http://127.0.0.1:<port>/__admin/`
- 第一版仅允许 loopback 访问，避免误暴露到公网
- 设计说明文档：`docs/ADMIN-UI.md`
