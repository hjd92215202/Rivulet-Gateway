# Rivulet Gateway

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

Current kernel cut:

- Cargo workspace with focused crates
- Strongly typed config model
- Route matching core
- Filter chain abstraction
- Upstream registry with round robin selection
- Minimal HTTP/1.1 request parser
- Minimal reverse proxy path over raw TCP
- Listener runtime with graceful stop signal
- Conservative upstream keepalive reuse
- Local benchmark harness
- Initial multi-platform packaging skeleton

Current limits:

- HTTP/1.1 only
- One request per connection
- `Content-Length` request bodies only
- No chunked request support yet
- No TLS yet
- `worker_threads` config is not yet wired into a custom Tokio runtime
- Linux package and RPM flow are scaffolded, but native target build hosts are still preferred

Run:

```powershell
cargo run -p gateway-main -- config/gateway.toml
```

CI/CD:

- CI workflow: `.github/workflows/ci.yml`
- Release workflow: `.github/workflows/release.yml`
- Nightly benchmark workflow: `.github/workflows/nightly-benchmark.yml`
- Packaging guide: `packaging/README.md`
