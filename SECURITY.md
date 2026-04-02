# Security Policy / 安全策略

## English

`Rivulet Gateway / 溪流网关` is infrastructure software, so security handling must be conservative and explicit.

### Reporting

Please do not disclose unpatched vulnerabilities through public issues.

When reporting a security issue, include:

- affected version or commit
- vulnerability class
- reproduction steps or proof of concept
- impact scope
- whether the issue is configuration-dependent

If a private reporting channel is not yet published for the public project, maintainers should bootstrap one before broad release.
Until then, security handling should remain restricted to trusted maintainers and deployment operators.

### Response Goals

Current target service levels for maintainers:

- initial acknowledgement: within 3 business days
- reproduction decision: within 7 business days
- fix or mitigation plan: as soon as severity is understood

These are goals, not guarantees, but they set the expected operating standard.

### Severity Guidance

High-priority examples:

- request smuggling or framing bypass
- upstream response boundary confusion
- auth or policy bypass once such features exist
- unsafe packaging or release artifact tampering
- denial-of-service conditions that are easy to trigger remotely

### Disclosure

Preferred sequence:

1. private report
2. maintainer reproduction
3. patch and release preparation
4. coordinated disclosure with mitigation notes

### Current Security Notes

At the current kernel stage:

- HTTP/1.1 only
- no TLS termination yet
- no chunked transfer support
- conservative response-boundary enforcement is in place, but still early-stage
- packaging and checksum verification are present, but release signing is not yet implemented

## 中文

`Rivulet Gateway / 溪流网关` 是基础设施软件，因此安全处理必须保守且明确。

### 漏洞反馈

未修复的漏洞不要通过公开 issue 披露。

反馈安全问题时，请尽量提供：

- 受影响版本或提交
- 漏洞类型
- 复现步骤或 POC
- 影响范围
- 是否依赖特定配置

如果项目还没有公开私密反馈渠道，Maintainer 应在大范围发布前先建立一条。
在此之前，安全处理应只在可信的维护者和部署运维之间流转。

### 响应目标

当前给 Maintainer 设定的目标服务级别：

- 首次确认：3 个工作日内
- 是否复现的判断：7 个工作日内
- 修复或缓解方案：在确认严重级别后尽快给出

这些是目标，不是绝对承诺，但它们定义了项目的基本运作标准。

### 严重性参考

高优先级示例包括：

- request smuggling 或报文边界绕过
- 上游响应边界混淆
- 在未来引入认证或策略后出现绕过
- 打包链路不安全或发布产物被篡改
- 容易被远程触发的拒绝服务问题

### 披露顺序

推荐流程：

1. 私密反馈
2. Maintainer 复现
3. 补丁和发布准备
4. 带缓解说明的协调披露

### 当前阶段的安全说明

在现阶段内核版本中：

- 仅支持 HTTP/1.1
- 还没有 TLS termination
- 不支持 chunked transfer
- 响应边界的保守校验已经存在，但仍处于早期阶段
- 已有打包与 checksum 校验，但还没有 release signing
