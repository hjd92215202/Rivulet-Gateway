# Contributing / 贡献指南

## English

Thanks for helping improve `Rivulet Gateway / 溪流网关`.

We are building toward an Apache-grade engineering bar:

- small, reviewable changes
- reproducible tests
- clear release artifacts
- explicit production-risk discussion

### Ground Rules

- Prefer focused pull requests over large mixed refactors.
- Do not add dependencies casually; justify operational and maintenance cost.
- Keep platform portability in mind for Windows, Linux x86_64, and Linux arm64.
- If behavior changes, update tests and docs in the same change.
- If packaging or release behavior changes, update workflow and validation docs too.

### Development Flow

1. Start from a clean branch.
2. Run relevant tests before proposing changes.
3. Add or update tests for new behavior.
4. Update docs for any user-visible, operational, or packaging change.
5. Use clear commit messages with a prefix such as `feat:`, `fix:`, or `docs:`.

### What Maintainers Expect In A PR

- problem statement
- scope of change
- risk or compatibility notes
- validation performed
- follow-up work if the change is intentionally incomplete

Issue and PR hygiene:

- use the repository issue templates for bug, performance, and release work
- use the pull request template when opening reviewable changes
- ownership review routes are defined in `.github/CODEOWNERS`

### Testing Expectations

At minimum, contributors should run the narrowest relevant checks:

- `cargo test --workspace`
- packaging or smoke scripts if release behavior changed
- benchmark scripts only when the change affects performance-sensitive paths

Useful entry points:

- [scripts/package.ps1](C:\Users\brace\Documents\New%20project\scripts\package.ps1)
- [scripts/package.sh](C:\Users\brace\Documents\New%20project\scripts\package.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)
- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)

### Dependency Policy

We intentionally keep the dependency surface narrow.
When proposing a new crate, explain:

- why in-house implementation is not preferable
- what operational risk it adds
- what update burden it creates
- whether it affects cross-platform packaging or release behavior

### Performance Claims

Do not post benchmark claims without:

- environment details
- command used
- workload shape
- whether access logging was disabled
- whether results were local, CI, or server-hosted

### Security

Do not open public issues for unpatched vulnerabilities.
Follow [SECURITY.md](C:\Users\brace\Documents\New%20project\SECURITY.md).

## 中文

感谢你参与改进 `Rivulet Gateway / 溪流网关`。

我们正在按 Apache 级别的工程标准推进项目：

- 变更要小而可审阅
- 测试要可复现
- 发布产物要清晰
- 生产风险讨论要明确透明

### 基本规则

- 优先提交聚焦的小型 PR，不要把多种无关重构混在一起。
- 不要随意增加依赖，必须说明运维成本和维护成本。
- 始终考虑 Windows、Linux x86_64、Linux arm64 的平台可移植性。
- 只要行为发生变化，就要在同一个变更里同步更新测试和文档。
- 只要打包或发布行为发生变化，就要同步更新 workflow 和验证文档。

### 开发流程

1. 从干净分支开始开发。
2. 提交前先跑与改动相关的验证。
3. 对新增行为补测试，对既有行为变化更新测试。
4. 只要涉及用户可见、运维可见或打包可见的变化，就同步更新文档。
5. 使用清晰的提交信息前缀，例如 `feat:`、`fix:`、`docs:`。

### Maintainer 对 PR 的预期

- 问题背景
- 变更范围
- 风险或兼容性说明
- 已做验证
- 如果本次刻意不做完，需要写清后续补项

Issue 和 PR 规范：

- Bug、性能和发布相关工作请优先使用仓库内模板
- 提交可审阅改动时请使用 PR 模板
- 代码归属和审阅路径在 `.github/CODEOWNERS` 中定义

### 测试要求

至少应运行与本次改动最相关、最小化的一组检查：

- `cargo test --workspace`
- 如果影响发布行为，则运行打包或 smoke 脚本
- 只有在改动涉及性能敏感路径时才运行 benchmark 脚本

常用入口：

- [scripts/package.ps1](C:\Users\brace\Documents\New%20project\scripts\package.ps1)
- [scripts/package.sh](C:\Users\brace\Documents\New%20project\scripts\package.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)
- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)

### 依赖策略

我们有意保持依赖面收敛。
如果要引入新的 crate，请说明：

- 为什么不优先自研
- 它会引入什么运维风险
- 它会带来什么更新和维护负担
- 它是否影响跨平台打包或发布行为

### 性能声明

没有以下信息时，不应对外发布 benchmark 结论：

- 测试环境说明
- 使用命令
- 工作负载形态
- 是否关闭 access logging
- 结果来自本地、CI 还是服务器

### 安全

未修复的漏洞不要通过公开 issue 披露。
请遵循 [SECURITY.md](C:\Users\brace\Documents\New%20project\SECURITY.md)。
