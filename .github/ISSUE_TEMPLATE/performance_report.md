---
name: Performance report
about: Report a throughput, latency, or resource regression
title: "[perf] "
labels: performance
assignees: ""
---

## Summary

Describe the observed regression or bottleneck.

## Environment

- version or commit:
- platform:
- architecture:
- cpu and memory:
- benchmark host type:
  local / CI / server

## Workload

- concurrency:
- response size:
- upstream keepalive setting:
- duration:
- benchmark command:

## Results

Include the measured values:

- throughput:
- p50 latency:
- p95 latency:
- p99 latency:
- error rate:

## Comparison

What are you comparing against:

- previous commit
- previous release
- another platform
- another configuration

## Evidence

Attach CSV output, logs, profiler output, or screenshots if available.

## Notes

Mention whether access logging was disabled and whether the result is reproducible.
