#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:-./gateway}"
PORT="${PORT:-18080}"
TMP_DIR="$(mktemp -d)"
CONFIG_PATH="$TMP_DIR/gateway.toml"
PID=""

cleanup() {
  if [[ -n "$PID" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for smoke tests" >&2
  exit 1
fi

cat >"$CONFIG_PATH" <<EOF
[runtime]
worker_threads = 4
graceful_shutdown_secs = 5
downstream_read_timeout_ms = 5000
upstream_connect_timeout_ms = 1000
upstream_read_timeout_ms = 1000
upstream_retry_attempts = 1
upstream_idle_pool_size = 1
max_upstream_status_line_bytes = 8192
max_upstream_headers = 100
max_upstream_header_bytes = 65536
max_upstream_body_bytes = 8388608
max_request_line_bytes = 8192
max_request_headers = 100
max_request_body_bytes = 1048576

[[listeners]]
name = "smoke"
address = "127.0.0.1:${PORT}"
protocol = "http1"

[[upstreams]]
name = "missing-upstream"
load_balance = "round_robin"

[[upstreams.endpoints]]
address = "127.0.0.1:19090"
weight = 1

[[routes]]
name = "default"
listener = "smoke"
hosts = ["smoke.test"]
path_prefixes = ["/"]
methods = []
upstream = "missing-upstream"
filters = ["request-id"]
EOF

"$BINARY" "$CONFIG_PATH" >"$TMP_DIR/stdout.log" 2>"$TMP_DIR/stderr.log" &
PID="$!"
sleep 1

status_ok="$(curl -s -o /dev/null -w "%{http_code}" -H "Host: smoke.test" "http://127.0.0.1:${PORT}/ok")"
status_missing="$(curl -s -o /dev/null -w "%{http_code}" -H "Host: other.test" "http://127.0.0.1:${PORT}/missing")"

if [[ "$status_ok" != "502" ]]; then
  echo "expected 502 from missing upstream, got $status_ok" >&2
  exit 1
fi

if [[ "$status_missing" != "404" ]]; then
  echo "expected 404 from unmatched route, got $status_missing" >&2
  exit 1
fi

echo "server smoke passed on port ${PORT}"
