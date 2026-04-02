# AGENTS.md

## Purpose / 目的

This file is the canonical AI-facing working agreement for `Rivulet Gateway / 溪流网关`.
Every AI agent that edits, reviews, tests, documents, packages, or releases this repository should read this file first.

本文件是 `Rivulet Gateway / 溪流网关` 面向 AI 的统一工作规约。
任何进入仓库进行开发、测试、文档、打包、发布、评审的 AI，都应先阅读本文件。

## Project Identity / 项目标识

- English name: `Rivulet Gateway`
- Chinese name: `溪流网关`
- Product direction: build a commercial-grade Rust gateway with core capabilities comparable to the essential kernel surface of Nginx, then iterate version by version.
- Engineering bar: aim for Apache-grade standards across development, testing, benchmarking, CI/CD, packaging, release management, documentation, governance, and community hygiene.

- 英文名：`Rivulet Gateway`
- 中文名：`溪流网关`
- 产品方向：用 Rust 构建一套具备 Nginx 核心能力的商用级网关，先把内核做稳，再逐版迭代。
- 工程标准：以 Apache 基金会级别的开发、测试、压测、CI/CD、打包、发布、文档、治理和社区维护标准推进。

## Non-Negotiable Principles / 不可妥协原则

- Start from zero without historical baggage.
- Do not introduce dependencies unless the risk, maintenance cost, and long-term necessity are clearly justified.
- Prefer self-implemented core abstractions and protocol boundaries over outsourcing critical behavior to opaque libraries.
- Prioritize correctness, stability, observability, and operability over feature breadth.
- Production usability matters more than extreme benchmark chasing at the current stage.

- 从零开始，不背历史包袱。
- 能不引入依赖就不引入；只有风险、维护成本、长期必要性都清楚时才允许加依赖。
- 核心抽象、协议边界、关键链路优先自研，不把关键行为交给黑盒库。
- 正确性、稳定性、可观测性、可运维性优先于功能面扩张。
- 当前阶段优先追求生产可用，不追求极限跑分。

## Collaboration Expectations For AI / AI 协作要求

- Default to execution, not endless planning.
- Do not repeatedly ask the human for permission for obvious next engineering steps.
- When a path has hidden risk or non-obvious tradeoffs, surface the tradeoff clearly and then proceed carefully.
- Preserve user intent across turns; avoid losing project context.
- When rules or decisions become durable, write them down in-repo instead of relying on chat memory.

- 默认直接推进，不要停留在无休止的计划阶段。
- 对明显的下一步工程动作，不要反复向人确认。
- 遇到隐藏风险或存在明显取舍的路径，要先把取舍说清楚，再谨慎推进。
- 跨轮次保留用户意图，避免上下文丢失。
- 凡是已经沉淀下来的规则或决策，都要写入仓库文档，而不是只留在对话里。

## Code Standards / 代码标准

- Use UTF-8 throughout the whole project to avoid mojibake and cross-platform encoding issues.
- Code comments should be in Chinese when they explain business meaning, protocol handling, safety boundaries, or non-obvious design.
- Comment density should be high for meaningful business logic; avoid empty or decorative comments.
- Tests should be comprehensive for the area being changed.
- If a bug is found during adjacent work, fix it along the way when safe and scoped.

- 全项目统一使用 UTF-8，避免乱码和跨平台编码问题。
- 只要是在解释业务意义、协议处理、安全边界或不明显设计，代码注释优先用中文。
- 对有业务意义的代码应保持较高注释密度，但不要写空洞注释。
- 改到哪里，就把对应测试补到足够覆盖。
- 中途发现顺手可修的问题，在风险可控且范围明确时应一并修掉。

## Testing And Benchmarking / 测试与压测

- Benchmarking must be progressive and conservative; do not destabilize the developer machine or server.
- Baseline measurements are for production readiness and bottleneck discovery, not vanity numbers.
- Benchmark workflows should produce machine-readable or report-style outputs that can be reviewed later.
- Prefer bundled deterministic fixtures over ad hoc external dependencies.
- Smoke, baseline, failure-path, and degraded-path validation should be automated whenever possible.

- 压测要循序渐进、保守推进，不能把开发机或服务器打崩。
- 基线测试的目的，是判断生产可用性和识别瓶颈，不是刷漂亮数字。
- 压测流程应尽量产出可复查的报告或结构化结果。
- 优先使用仓库自带、行为可控的 fixture，不依赖临时外部环境。
- 能自动化的 smoke、baseline、故障路径、降级路径验证，都尽量自动化。

## Script Standards / 脚本标准

- All operational and benchmark scripts should follow one standard.
- Happy-path execution should not require manual intervention.
- Scripts should print explicit progress for long-running phases.
- Scripts should automatically clean up background processes and exit on their own when done.
- Linux operational scripts should support x86_64 and arm64.
- Linux operational scripts should support automatic dependency installation when `apt-get`, `dnf`, or `yum` is available.
- If a script cannot complete automatically, it should fail loudly and explain why.

- 所有运维类、压测类脚本都应遵循统一标准。
- 正常路径执行时不依赖人工介入。
- 对耗时阶段，脚本要明确打印当前进度。
- 执行完成后，脚本要自动清理后台进程并自动退出。
- Linux 运维类脚本应同时支持 x86_64 和 arm64。
- Linux 运维类脚本在存在 `apt-get`、`dnf` 或 `yum` 时，应支持自动安装缺失依赖。
- 如果脚本无法自动完成，应明确失败并给出原因，不能静默卡住。

## Packaging And Platform Support / 打包与平台支持

- Target platforms currently in scope:
- Windows x86_64
- Linux x86_64
- Linux arm64
- Linux distribution format support should include `rpm`.
- Keep release-time dependency burden minimal.
- Prefer native target builds and native tooling when possible.

- 当前重点支持的平台：
- Windows x86_64
- Linux x86_64
- Linux arm64
- Linux 分发格式应支持 `rpm`。
- 尽量降低发布期依赖负担。
- 能用目标平台原生构建和原生工具链时，优先使用原生方式。

## Documentation Standards / 文档标准

- All user-facing and engineering-facing docs should be bilingual in Chinese and English.
- When documentation is updated, both languages should stay aligned in meaning.
- Important workflows should be captured as reproducible docs, not tribal knowledge.

- 所有面向用户和工程的文档都应同时提供中英文版本。
- 更新文档时，中英文语义要保持一致。
- 重要流程必须沉淀成可复现文档，不能依赖口口相传。

## CI/CD And Release Rules / CI/CD 与发布规则

- GitHub Actions is the primary CI/CD system.
- CI, release, benchmark, packaging, and distribution should be repository-owned and reproducible.
- Release tags follow `v<version>`.
- Release asset filenames must align with the release tag version.
- Do not reuse old tags for new code.
- When release behavior changes, update release docs and checklists in the same line of work.

- GitHub Actions 是主要的 CI/CD 体系。
- CI、发布、压测、打包、分发流程都要由仓库自持并可复现。
- 发布 tag 统一采用 `v<version>`。
- Release 产物文件名必须与 tag 版本对齐。
- 旧 tag 不复用来承载新代码。
- 只要发布行为变了，发布文档和检查清单就要同步更新。

## Git And Change Management / Git 与变更管理

- Make meaningful, scoped commits as work progresses.
- For feature or substantial engineering work, prefer `feat:` commit messages.
- Commit history should make architectural and operational milestones easy to trace.
- Do not hide important behavior changes inside vague commit messages.

- 随着工作推进，保持提交粒度清晰、语义明确。
- 对功能性或重要工程改动，优先使用 `feat:` 提交信息。
- 提交历史应能清楚反映架构、运维、发布等关键里程碑。
- 重要行为变更不能藏在模糊提交信息里。

## Current Strategic Focus / 当前阶段重点

- Strengthen protocol boundaries and production readiness.
- Improve automated testing and benchmark coverage.
- Improve cross-platform packaging and server validation flows.
- Reduce manual steps in deployment, verification, and release workflows.
- Keep the gateway kernel narrow, correct, and operable before expanding protocol surface.

- 持续加固协议边界和生产可用性。
- 持续提升自动化测试和压测覆盖。
- 持续完善跨平台打包和服务器验证流程。
- 持续减少部署、验证、发布过程中的人工步骤。
- 在扩展协议面之前，先把网关内核做窄、做稳、做可运维。

## How AI Should Use This File / AI 如何使用本文件

- Treat this file as the top-level repository working contract.
- If a new durable rule appears in chat, update this file or a linked in-repo policy file.
- If a local implementation does not yet match the desired standard, distinguish clearly between current state and required state.
- Do not silently discard these constraints when solving a narrow task.

- 把本文件视为仓库级最高工作约定。
- 如果对话中出现新的长期规则，要把规则补进本文件或其关联规范文档。
- 如果当前实现还没达到目标标准，要明确区分“现状”和“要求”，不能混淆。
- 即便是在解决局部问题时，也不能静默忽略这些约束。
