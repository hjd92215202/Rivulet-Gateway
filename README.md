# Gateway

A Rust gateway built from zero with a strict dependency policy.

Current design rules:

- Keep core abstractions in-house.
- Only add dependencies when the risk and maintenance cost are justified.
- Stabilize config, routing, filters, upstream, and runtime boundaries first.
- Ship a narrow but correct HTTP gateway before adding more protocol surface.

Current kernel cut:

- Cargo workspace with focused crates
- Strongly typed config model
- Route matching core
- Filter chain abstraction
- Upstream registry with round robin selection
- Minimal HTTP/1.1 request parser
- Minimal reverse proxy path over raw TCP
- Listener runtime with graceful stop signal

Current limits:

- HTTP/1.1 only
- One request per connection
- `Content-Length` request bodies only
- No chunked request support yet
- No keep-alive reuse yet
- No TLS yet

Run:

```powershell
cargo run -p gateway-main -- config/gateway.toml
```
