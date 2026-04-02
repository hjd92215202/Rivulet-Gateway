---
name: Performance report / 性能反馈
about: Report a throughput, latency, or resource regression / 报告吞吐、延迟或资源回归
title: "[perf] "
labels: performance
assignees: ""
---

## English Summary

Describe the observed regression or bottleneck.

## 中文摘要

请描述观察到的性能回归或瓶颈。

## Environment / 环境

- version or commit / 版本或提交：
- platform / 平台：
- architecture / 架构：
- cpu and memory / CPU 和内存：
- benchmark host type / 测试主机类型：
  local / CI / server

## Workload / 负载模型

- concurrency / 并发：
- response size / 响应大小：
- upstream keepalive setting / 上游 keepalive 设置：
- duration / 持续时间：
- benchmark command / benchmark 命令：

## Results / 结果

Include the measured values / 请填写测得的数据：

- throughput：
- p50 latency：
- p95 latency：
- p99 latency：
- error rate：

## Comparison / 对比基线

What are you comparing against / 你在和什么比较：

- previous commit
- previous release
- another platform
- another configuration

## Evidence / 证据

Attach CSV output, logs, profiler output, or screenshots if available.

## Notes / 备注

Mention whether access logging was disabled and whether the result is reproducible.
