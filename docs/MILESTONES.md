# Milestones

Project: `Rivulet Gateway / 溪流网关`

Baseline date: April 2, 2026

This roadmap turns the current repository state into staged milestones with exit criteria.
It is intentionally conservative: the target is production-safe progress, not maximum feature velocity.

## Milestone 0: Kernel Baseline

Status: in progress, substantially established

What exists:

- typed config
- HTTP/1.1 routing and proxy core
- upstream health and retry skeleton
- conservative upstream keepalive reuse
- runtime listener loop
- packaging skeleton
- CI, release automation, and nightly benchmark baseline

Exit criteria:

- all existing workflows stay green on Linux and Windows
- Linux x86_64 package validation passes in CI
- Windows package smoke remains stable
- roadmap and governance docs are in repo

## Milestone 1: Grayscale Readiness

Goal:

- make the gateway safe enough for narrow-scope internal grayscale traffic

Required work:

- Linux real-host validation for `tar.gz` and `rpm`
- downstream connection lifecycle hardening
- clearer protocol rejection for unsupported request and response paths
- service installation and startup validation on Linux hosts
- release artifact handling refined for repeatable operator use

Exit criteria:

- Linux x86_64 staged install validated on real machines
- no unexplained `502` or socket churn regressions under conservative benchmark baselines
- operator documentation covers start, stop, validate, and rollback basics

## Milestone 2: Production Edge Foundation

Goal:

- support controlled production ingress for simple HTTP/1.1 traffic classes

Required work:

- TLS termination design and implementation
- better downstream keepalive handling
- structured access logging and operational controls
- stronger passive and active health behavior
- upgrade and rollback validation in disposable environments
- signed release artifacts

Exit criteria:

- Linux server benchmark campaign completed
- package installation and service lifecycle validation automated in disposable Linux environments
- release signing and checksum verification documented and working
- clear support boundaries published

## Milestone 3: Platform And Protocol Expansion

Goal:

- reduce platform and protocol gaps without losing dependency discipline

Required work:

- Linux arm64 release verification on native runners or self-hosted infra
- Windows packaging refinement
- HTTP/2 decision and plan
- chunked transfer support or stronger documented rejection policy
- richer routing, filters, and policy controls

Exit criteria:

- x86_64 and arm64 Linux artifacts validated consistently
- platform-specific installation notes published
- protocol expansion backed by tests, benchmarks, and operator docs

## Milestone 4: Apache-Grade Project Maturity

Goal:

- operate like a durable open infrastructure project, not just a codebase

Required work:

- maintainer model and support policy
- deprecation policy
- release note discipline and changelog hygiene
- security contact and coordinated disclosure process
- documented benchmark methodology and trend reporting
- contributor onboarding and issue triage rhythm

Exit criteria:

- repository governance is explicit and practiced
- release process is reproducible and auditable
- production claims are backed by repeatable evidence
- community maintenance work is not dependent on one-off knowledge

## Current Benchmark Summary

Local loopback baseline on the current Windows development host suggests:

- stable low-latency path exists for conservative concurrency levels
- upstream keepalive materially improves stability
- local socket churn becomes a bottleneck when reconnecting every request
- final capacity conclusions must wait for Linux server benchmarking

These numbers are tracked for trend detection, not for customer-facing sizing guidance.

## Current Release Summary

Implemented today:

- Windows x86_64 zip
- Linux x86_64 tar.gz
- Linux x86_64 rpm
- Linux arm64 workflow path
- CI packaging and validation
- nightly benchmark collection
- checksum generation and verification

Still required before stronger production claims:

- artifact signing
- real Linux install automation
- systemd lifecycle validation
- upgrade and rollback verification
