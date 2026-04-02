# Governance / 项目治理

## English

This project is being developed with an Apache-grade quality target, even before formal foundation processes exist.

### Principles

- repository-owned automation over tribal knowledge
- explicit risk discussion over hidden assumptions
- small, testable increments over speculative rewrites
- portability and packaging as first-class engineering concerns
- truthful status reporting over aspirational claims

### Maintainer Responsibilities

Maintainers are responsible for:

- reviewing code and release changes
- keeping CI/CD and packaging healthy
- preserving a conservative dependency policy
- preventing unsupported production claims from entering docs or release notes
- triaging security and operational incidents

### Decision Style

Default decision rule:

- prefer the simpler, more inspectable path when two designs are close
- do not expand protocol surface faster than the test and validation system can support
- production-readiness claims must be backed by reproducible validation

### Required Project Areas

The project should maintain standards in:

- code review
- testing and load testing
- packaging and release automation
- documentation and upgrade guidance
- security handling
- community conduct and contributor onboarding

### Near-Term Governance Gaps

These still need formalization in future iterations:

- named maintainers
- release manager rotation
- deprecation policy
- support window policy
- public security contact

## 中文

即使目前还没有进入正式基金会流程，项目也按 Apache 级别的质量目标推进。

### 基本原则

- 用仓库内自动化替代口口相传的隐性知识
- 明确讨论风险，不接受隐含假设
- 以小步、可测试的增量替代投机式大重写
- 把可移植性和打包当作一等工程问题
- 真实汇报状态，不做超前宣传

### Maintainer 职责

Maintainer 负责：

- 审阅代码和发布变更
- 保持 CI/CD 与打包链路健康
- 坚持保守的依赖策略
- 阻止未经验证的生产可用性声明进入文档或 release note
- 分流并处理安全与运维事件

### 决策方式

默认决策规则：

- 当两个方案接近时，优先选更简单、更容易检查的路径
- 协议面扩张速度不能快于测试和验证体系的承载能力
- 任何生产就绪度声明都必须有可复现验证支撑

### 必须长期维护的项目领域

项目应持续在以下方面保持标准：

- 代码评审
- 功能测试与压测
- 打包与发布自动化
- 文档与升级指南
- 安全处置
- 社区行为规范与贡献者 onboarding

### 近期仍待正式化的治理缺口

未来迭代仍需补齐：

- 明确的 Maintainer 名单
- 发布经理轮值机制
- 弃用策略
- 支持窗口策略
- 对外公开的安全联系渠道
