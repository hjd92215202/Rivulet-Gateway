# Production Readiness Report

Report date: April 2, 2026

Project: `Rivulet Gateway / 溪流网关`

This report describes the current boundary of the gateway based on repository state, automated tests, packaging work, and local benchmark exploration.

## Executive Summary

Current status:

- suitable for controlled lab, staging, and low-risk grayscale validation
- not yet ready for broad Internet-facing production traffic

Why:

- core reverse proxy path exists and is tested
- packaging and CI/CD are now repository-owned
- conservative keepalive reuse exists for safe response boundaries
- but the protocol surface and runtime model are still narrow

## What Exists Today

Implemented layers:

- typed config model
- routing
- filter chain skeleton
- upstream registry and health-state tracking
- HTTP/1.1 request parsing
- reverse proxy over raw TCP
- conservative upstream keepalive reuse
- runtime listener loop and graceful drain
- Windows and Linux packaging skeleton
- GitHub Actions for CI, packaging, release, and nightly benchmark collection

## Current Hard Limits

- HTTP/1.1 only
- no TLS termination
- no HTTP/2
- no chunked request support
- no chunked upstream response support
- no downstream keepalive request multiplexing; current model is effectively one request per accepted downstream connection
- no real auth, rate limit, WAF, or policy engine yet
- no hot reload or dynamic config plane yet
- no signed release artifacts yet

## Known Engineering Gaps

1. `worker_threads` is still a config field, but it is not yet wired into a custom Tokio runtime builder.
2. Packaging validation is strong at the staged-layout level, but real Linux installation and service lifecycle validation still needs disposable VM coverage.
3. The benchmark harness is intentionally conservative and local; it is not a substitute for server-grade load testing on Linux.
4. Release checksums exist, but artifact signing and trust-chain publication are not implemented.

## Validation Completed

Repository validation:

- workspace tests
- protocol boundary tests
- keepalive reuse tests
- Windows package build and smoke validation
- Linux package structure validation designed into CI
- Linux installed-layout smoke validation designed into CI and release workflows

Key validation entry points:

- [packaging/SERVER-VALIDATION.md](C:\Users\brace\Documents\New%20project\packaging\SERVER-VALIDATION.md)
- [packaging/tests/run-linux-validation.sh](C:\Users\brace\Documents\New%20project\packaging\tests\run-linux-validation.sh)
- [scripts/bench-baseline.sh](C:\Users\brace\Documents\New%20project\scripts\bench-baseline.sh)

## Local Benchmark Snapshot

Environment used:

- Windows 10 Pro build 19045
- Intel i7-6500U
- 2 physical cores / 4 logical processors
- 16 GB RAM
- Rust 1.92.0

Important caution:

- these numbers are local loopback baselines, not release-quality server benchmarks
- they are useful for trend tracking and bottleneck discovery, not final capacity planning

Observed stable path with `upstream_idle_pool_size = 1`:

- 64-byte response, concurrency 8: about 1561 req/s, p95 about 9.9 ms
- 64-byte response, concurrency 32: about 1266 req/s, p95 about 58.8 ms
- 4096-byte response, concurrency 8: about 1590 req/s, p95 about 11.0 ms
- 4096-byte response, concurrency 64: about 1734 req/s, p95 about 57.1 ms

Observed warning sign:

- when upstream keepalive is disabled and every request reconnects, this Windows host can hit `502` responses and socket churn behavior much earlier
- this strongly suggests connection churn and local socket lifecycle become a bottleneck before the gateway core itself is fully exercised

Engineering conclusion:

- conservative upstream connection reuse is already materially important for stability
- future production exploration should prioritize Linux hosts and real NIC traffic before drawing capacity conclusions

## Packaging And Release Readiness

Current state:

- Windows x86_64 zip: implemented and locally validated
- Linux x86_64 tar.gz: implemented in scripts and workflows
- Linux x86_64 rpm: implemented in scripts and workflows
- Linux arm64 tar.gz/rpm: implemented in workflows, intended for native arm64 runners

Release automation status:

- CI builds and validates package artifacts
- release workflow produces artifacts and checksum manifest
- checksum verification script exists

Remaining release-grade work:

- artifact signing
- provenance / SBOM strategy
- upgrade and rollback validation
- native Linux install verification in disposable test systems

## Production Use Guidance Right Now

Reasonable near-term use:

- development environments
- CI integration tests
- internal staging
- low-risk grayscale routes with tight scope and rollback control

Not recommended yet:

- public edge gateway for mixed client traffic
- TLS termination at scale
- multi-tenant policy enforcement
- high-throughput production ingress without Linux server benchmarking and installation validation

## Next Bottlenecks To Address

1. Linux real-host benchmark and installation validation
2. downstream keepalive lifecycle improvements
3. chunked transfer support or explicit non-support enforcement across all edges
4. TLS and certificate lifecycle design
5. runtime configurability and operations plane
6. signed release process and public support policy
