#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TAG="v0.1.5"
REPO_SLUG="hjd92215202/Rivulet-Gateway"
HOST_HEADER="localhost"
GATEWAY_PORT="18080"
BACKEND_PORT="19000"
BENCH_PATH="/fixture/default"
BENCH_DURATION="15"
LABEL="linux-e2e"
WORK_DIR=""

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-release-e2e.sh --host <host> [options]

required:
  --host <host>            host header matched by the generated route

optional:
  --tag <tag>              release tag, default: v0.1.5
  --repo <owner/name>      github repository slug, default: hjd92215202/Rivulet-Gateway
  --gateway-port <port>    gateway listen port, default: 18080
  --backend-port <port>    fixture backend port, default: 19000
  --bench-path <path>      benchmark path, default: /fixture/default
  --duration <secs>        benchmark duration per case, default: 15
  --label <label>          report label, default: linux-e2e
  --work-dir <dir>         working directory, default: ./target/linux-release-e2e/<timestamp>-<label>

example:
  bash ./scripts/linux-release-e2e.sh \
    --host llmtamer.com:8080 \
    --tag v0.1.5 \
    --label llmtamer-e2e
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST_HEADER="$2"
      shift 2
      ;;
    --tag)
      TAG="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --gateway-port)
      GATEWAY_PORT="$2"
      shift 2
      ;;
    --backend-port)
      BACKEND_PORT="$2"
      shift 2
      ;;
    --bench-path)
      BENCH_PATH="$2"
      shift 2
      ;;
    --duration)
      BENCH_DURATION="$2"
      shift 2
      ;;
    --label)
      LABEL="$2"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$HOST_HEADER" ]]; then
  usage >&2
  exit 1
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required command not found: $1" >&2
    exit 1
  fi
}

require_cmd curl
require_cmd tar
require_cmd sha256sum
require_cmd python3
require_cmd wrk

detect_arch_suffix() {
  case "$(uname -m)" in
    x86_64)
      echo "linux-x86_64"
      ;;
    aarch64|arm64)
      echo "linux-arm64"
      ;;
    *)
      echo "unsupported architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac
}

ASSET_ARCH_SUFFIX="$(detect_arch_suffix)"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-release-e2e/$TIMESTAMP-$LABEL"
fi

DOWNLOAD_DIR="$WORK_DIR/downloads"
EXTRACT_DIR="$WORK_DIR/extracted"
RUNTIME_DIR="$WORK_DIR/runtime"
LOG_DIR="$WORK_DIR/logs"
REPORT_DIR="$WORK_DIR/report"
SUMMARY_PATH="$WORK_DIR/summary.md"

mkdir -p "$DOWNLOAD_DIR" "$EXTRACT_DIR" "$RUNTIME_DIR" "$LOG_DIR" "$REPORT_DIR"

BACKEND_PID=""
GATEWAY_PID=""

cleanup() {
  if [[ -n "$GATEWAY_PID" ]]; then
    kill "$GATEWAY_PID" >/dev/null 2>&1 || true
    wait "$GATEWAY_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

release_json_path="$DOWNLOAD_DIR/release.json"
release_api_url="https://api.github.com/repos/$REPO_SLUG/releases/tags/$TAG"
curl -fsSL "$release_api_url" -o "$release_json_path"

choose_asset_url() {
  local suffix="$1"
  python3 - "$release_json_path" "$suffix" <<'PY'
import json
import sys
from pathlib import Path

release = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
suffix = sys.argv[2]

for asset in release.get("assets", []):
    name = asset.get("name", "")
    if name.endswith(suffix):
        print(asset["browser_download_url"])
        raise SystemExit(0)

raise SystemExit(f"asset with suffix {suffix!r} not found")
PY
}

TARBALL_URL="$(choose_asset_url "-${ASSET_ARCH_SUFFIX}.tar.gz")"
SHA_URL="$(choose_asset_url "SHA256SUMS.txt")"
TARBALL_NAME="$(basename "$TARBALL_URL")"

curl -fL "$TARBALL_URL" -o "$DOWNLOAD_DIR/$TARBALL_NAME"
curl -fL "$SHA_URL" -o "$DOWNLOAD_DIR/SHA256SUMS.txt"

(
  cd "$DOWNLOAD_DIR"
  grep " ${TARBALL_NAME}$" SHA256SUMS.txt | sha256sum -c -
)

tar -xzf "$DOWNLOAD_DIR/$TARBALL_NAME" -C "$EXTRACT_DIR"
PACKAGE_ROOT="$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
if [[ -z "$PACKAGE_ROOT" ]]; then
  echo "failed to find extracted package root" >&2
  exit 1
fi

GATEWAY_CONFIG_PATH="$RUNTIME_DIR/gateway.toml"
cat >"$GATEWAY_CONFIG_PATH" <<EOF
[runtime]
worker_threads = 4
graceful_shutdown_secs = 30
downstream_read_timeout_ms = 5000
upstream_connect_timeout_ms = 1500
upstream_read_timeout_ms = 4000
upstream_retry_attempts = 2
upstream_idle_pool_size = 1
max_upstream_status_line_bytes = 8192
max_upstream_headers = 100
max_upstream_header_bytes = 65536
max_upstream_body_bytes = 8388608
max_request_line_bytes = 8192
max_request_headers = 100
max_request_body_bytes = 1048576

[[listeners]]
name = "public-http"
address = "127.0.0.1:${GATEWAY_PORT}"
protocol = "http1"

[[upstreams]]
name = "fixture-backend"
load_balance = "round_robin"

[[upstreams.endpoints]]
address = "127.0.0.1:${BACKEND_PORT}"
weight = 1

[[routes]]
name = "fixture-route"
listener = "public-http"
hosts = ["${HOST_HEADER}"]
path_prefixes = ["/fixture/"]
methods = []
upstream = "fixture-backend"
filters = ["request-id"]
EOF

python3 "$REPO_ROOT/scripts/fixture-backend.py" \
  --bind 127.0.0.1 \
  --port "$BACKEND_PORT" \
  --default-bytes 1024 \
  >"$LOG_DIR/backend.log" 2>&1 &
BACKEND_PID="$!"

"$PACKAGE_ROOT/usr/bin/gateway" "$GATEWAY_CONFIG_PATH" >"$LOG_DIR/gateway.log" 2>&1 &
GATEWAY_PID="$!"

wait_for_status() {
  local expected="$1"
  local host_header="$2"
  local path="$3"
  local attempts="${4:-40}"
  local delay_secs="${5:-0.25}"
  local url="http://127.0.0.1:${GATEWAY_PORT}${path}"
  local code=""

  for _ in $(seq 1 "$attempts"); do
    code="$(curl -sS -o /dev/null -w "%{http_code}" -H "Host: $host_header" "$url" || true)"
    if [[ "$code" == "$expected" ]]; then
      echo "$code"
      return 0
    fi
    sleep "$delay_secs"
  done

  echo "$code"
  return 1
}

VALID_PATH="$BENCH_PATH"
WRONG_HOST="invalid.example.test"

PRECHECK_STATUS="$(wait_for_status "200" "$HOST_HEADER" "$VALID_PATH")"
WRONG_HOST_STATUS="$(wait_for_status "404" "$WRONG_HOST" "$VALID_PATH" 5 0.2 || true)"

bash "$REPO_ROOT/scripts/linux-baseline-report.sh" \
  --url "http://127.0.0.1:${GATEWAY_PORT}${BENCH_PATH}" \
  --host "$HOST_HEADER" \
  --duration "$BENCH_DURATION" \
  --label "$LABEL" \
  --out-dir "$REPORT_DIR"

BENCH_REPORT_PATH="$(find "$REPORT_DIR" -name report.md | sort | tail -n 1)"

kill "$BACKEND_PID" >/dev/null 2>&1 || true
wait "$BACKEND_PID" >/dev/null 2>&1 || true
BACKEND_PID=""

BACKEND_DOWN_STATUS="$(wait_for_status "502" "$HOST_HEADER" "$VALID_PATH" 5 0.2 || true)"

cat >"$SUMMARY_PATH" <<EOF
# Linux Release E2E Summary

## Inputs

- Release tag: \`$TAG\`
- Repository: \`$REPO_SLUG\`
- Architecture suffix: \`$ASSET_ARCH_SUFFIX\`
- Host header: \`$HOST_HEADER\`
- Gateway port: \`$GATEWAY_PORT\`
- Backend port: \`$BACKEND_PORT\`
- Benchmark path: \`$BENCH_PATH\`
- Working directory: \`$WORK_DIR\`

## Downloaded Assets

- Tarball: \`$DOWNLOAD_DIR/$TARBALL_NAME\`
- Checksums: \`$DOWNLOAD_DIR/SHA256SUMS.txt\`

## Smoke Results

- Valid route status: \`$PRECHECK_STATUS\`
- Wrong host status: \`$WRONG_HOST_STATUS\`
- Backend-down status: \`$BACKEND_DOWN_STATUS\`

## Generated Paths

- Extracted package root: \`$PACKAGE_ROOT\`
- Runtime config: \`$GATEWAY_CONFIG_PATH\`
- Gateway log: \`$LOG_DIR/gateway.log\`
- Backend log: \`$LOG_DIR/backend.log\`
- Benchmark report: \`$BENCH_REPORT_PATH\`

## Exit Criteria

- valid route should be \`200\`
- wrong host should be \`404\`
- backend-down should be \`502\`
EOF

echo "summary: $SUMMARY_PATH"
echo "benchmark report: $BENCH_REPORT_PATH"
