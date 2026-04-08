#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

MODE="schema-check"
PROFILE="standard"
TARGET_ARCH=""
RESOLVED_ARCH=""
ARCH_KEY=""
THRESHOLDS_FILE="$REPO_ROOT/scripts/public-api-thresholds.env"
OUTPUT_DIR=""
ARTIFACT_PATH=""
FORMAT=""
PYTHON_BIN=""
HOST_HEADER="localhost"
GATEWAY_PORT="18680"
BACKEND_PORT="19680"

WORK_DIR=""
EXTRACT_DIR=""
RUNTIME_DIR=""
LOG_DIR=""
RUNS_DIR=""
RESULT_PATH=""
SUMMARY_PATH=""
CONFIG_PATH=""
PACKAGE_ROOT=""
PACKAGE_BIN=""
GATEWAY_PID=""
BACKEND_PID=""
DURATION_PROFILE="long"
LEGACY_FLAT_KEYS_PRESENT="false"

# 公网门禁默认走长跑窗口，减少偶发抖动造成的误判。
BASELINE_DURATION_SECS=20
BASELINE_CONCURRENCY=12
SOAK_DURATION_SECS=60
SOAK_CONCURRENCY=16
SAMPLE_COUNT=3
FAILURE_DURATION_SECS=12

THRESHOLD_AVAILABILITY=""
THRESHOLD_GATEWAY_5XX_RATIO_MAX=""
THRESHOLD_P95_MS=""
THRESHOLD_P99_MS=""
THRESHOLD_MIN_TOTAL_REQUESTS_BASELINE=""
THRESHOLD_MIN_TOTAL_REQUESTS_SOAK=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/linux-public-api-gate.sh [options]

Execute or validate public API reliability/capacity gate scenarios.

Options:
  --mode <schema-check|baseline|soak|failure-drill|evaluate>
  --profile <standard|strict|observe>          default: standard
  --arch <linux-x86_64|linux-arm64>            optional, defaults to uname-derived architecture
  --threshold-file <path>                      default: ./scripts/public-api-thresholds.env
  --output-dir <path>                          default: ./target/public-api-gate/<timestamp>-<mode>-<profile>-<arch>
  --artifact <path>                            required for non-schema modes, accepts .tar.gz or .rpm
  --format <tar.gz|rpm>                        optional, auto-detected when omitted
  --host <host>                                default: localhost
  --gateway-port <port>                        default: 18680
  --backend-port <port>                        default: 19680
  -h, --help

Modes:
  schema-check   validate threshold schema and profile-arch contracts only
  baseline       run steady-state samples and output scenario result
  soak           run longer steady-state samples and output scenario result
  failure-drill  run upstream business error + timeout/reset/backend-down drills
  evaluate       run baseline + soak + failure-drill and produce final gate decision

This script supports Linux x86_64 and Linux arm64.
When dependencies are missing it can auto-install via apt-get, dnf, or yum.
The script exits automatically when checks finish.
EOF
}

fail() {
  echo "public api gate check failed: $1" >&2
  exit 1
}

cleanup() {
  if [[ -n "$GATEWAY_PID" ]]; then
    kill "$GATEWAY_PID" >/dev/null 2>&1 || true
    wait "$GATEWAY_PID" >/dev/null 2>&1 || true
    GATEWAY_PID=""
  fi
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
    BACKEND_PID=""
  fi
}
trap cleanup EXIT

normalize_path() {
  local input_path="$1"
  printf '%s/%s\n' "$(cd "$(dirname "$input_path")" && pwd)" "$(basename "$input_path")"
}

validate_numeric() {
  local key="$1"
  local value="$2"
  [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]] || fail "$key must be numeric, got '$value'"
}

validate_positive_integer() {
  local key="$1"
  local value="$2"
  [[ "$value" =~ ^[0-9]+$ ]] || fail "$key must be an integer, got '$value'"
  [[ "$value" -gt 0 ]] || fail "$key must be greater than 0"
}

validate_threshold_value() {
  local key="$1"
  local value="$2"
  case "$key" in
    *_MIN_TOTAL_REQUESTS_BASELINE|*_MIN_TOTAL_REQUESTS_SOAK)
      validate_positive_integer "$key" "$value"
      ;;
    *_SLO_AVAILABILITY)
      validate_numeric "$key" "$value"
      awk "BEGIN {exit !($value > 0 && $value <= 100)}" || fail "$key must be within (0, 100]"
      ;;
    *_GATEWAY_5XX_RATIO_MAX)
      validate_numeric "$key" "$value"
      awk "BEGIN {exit !($value >= 0 && $value <= 100)}" || fail "$key must be within [0, 100]"
      ;;
    *_P95_MS|*_P99_MS)
      validate_numeric "$key" "$value"
      awk "BEGIN {exit !($value > 0)}" || fail "$key must be greater than 0"
      ;;
    *)
      fail "unknown threshold key: $key"
      ;;
  esac
}

is_supported_threshold_key() {
  local key="$1"
  if [[ "$key" =~ ^PUBLIC_API_(STANDARD|STRICT|OBSERVE)_LINUX_(X86_64|ARM64)_(SLO_AVAILABILITY|GATEWAY_5XX_RATIO_MAX|P95_MS|P99_MS|MIN_TOTAL_REQUESTS_BASELINE|MIN_TOTAL_REQUESTS_SOAK)$ ]]; then
    return 0
  fi

  case "$key" in
    PUBLIC_API_SLO_AVAILABILITY|PUBLIC_API_GATEWAY_5XX_RATIO_MAX|PUBLIC_API_P95_MS|PUBLIC_API_P99_MS)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

get_threshold_value() {
  local key="$1"
  local file_path="$2"
  awk -F '=' -v target="$key" '
    {
      raw=$0
      sub(/^[[:space:]]+/, "", raw)
      if (raw == "" || substr(raw, 1, 1) == "#") next
      split(raw, parts, "=")
      name=parts[1]
      value=substr(raw, index(raw, "=") + 1)
      gsub(/[[:space:]]/, "", name)
      gsub(/[[:space:]]/, "", value)
      if (name == target) {
        print value
        exit 0
      }
    }
  ' "$file_path"
}

require_threshold_value() {
  local key="$1"
  local file_path="$2"
  local value=""
  value="$(get_threshold_value "$key" "$file_path")"
  [[ -n "$value" ]] || fail "missing required key: $key"
  printf '%s\n' "$value"
}

to_profile_upper() {
  case "$1" in
    standard) printf '%s\n' "STANDARD" ;;
    strict) printf '%s\n' "STRICT" ;;
    observe) printf '%s\n' "OBSERVE" ;;
    *) fail "unsupported profile: $1" ;;
  esac
}

normalize_arch() {
  local raw_arch="$1"
  case "$raw_arch" in
    linux-x86_64|x86_64|amd64) printf '%s\n' "linux-x86_64" ;;
    linux-arm64|aarch64|arm64) printf '%s\n' "linux-arm64" ;;
    *) fail "unsupported arch value: $raw_arch" ;;
  esac
}

resolve_target_arch() {
  local raw_arch=""
  if [[ -n "$TARGET_ARCH" ]]; then
    RESOLVED_ARCH="$(normalize_arch "$TARGET_ARCH")"
  else
    raw_arch="$(uname -m)"
    RESOLVED_ARCH="$(normalize_arch "$raw_arch")"
  fi
  case "$RESOLVED_ARCH" in
    linux-x86_64) ARCH_KEY="LINUX_X86_64" ;;
    linux-arm64) ARCH_KEY="LINUX_ARM64" ;;
    *) fail "failed to map resolved arch: $RESOLVED_ARCH" ;;
  esac
}

load_threshold_profile() {
  local file_path="$1"
  local profile_upper=""
  local prefix=""
  local key=""
  local value=""

  [[ -f "$file_path" ]] || fail "missing threshold file: $file_path"

  while IFS='=' read -r key value; do
    key="$(echo "$key" | sed 's/[[:space:]]//g')"
    value="$(echo "$value" | sed 's/[[:space:]]//g')"
    [[ -z "$key" || "${key:0:1}" == "#" ]] && continue
    [[ -z "$value" ]] && fail "$key has empty value"
    is_supported_threshold_key "$key" || fail "unknown key in threshold file: $key"
    validate_threshold_value "$key" "$value"
  done <"$file_path"

  # 保留旧平铺键，仅用于兼容性检测展示，不参与按架构阻断判分。
  if [[ -n "$(get_threshold_value "PUBLIC_API_SLO_AVAILABILITY" "$file_path")" ]]; then
    LEGACY_FLAT_KEYS_PRESENT="true"
  fi

  profile_upper="$(to_profile_upper "$PROFILE")"
  prefix="PUBLIC_API_${profile_upper}_${ARCH_KEY}_"
  THRESHOLD_AVAILABILITY="$(require_threshold_value "${prefix}SLO_AVAILABILITY" "$file_path")"
  THRESHOLD_GATEWAY_5XX_RATIO_MAX="$(require_threshold_value "${prefix}GATEWAY_5XX_RATIO_MAX" "$file_path")"
  THRESHOLD_P95_MS="$(require_threshold_value "${prefix}P95_MS" "$file_path")"
  THRESHOLD_P99_MS="$(require_threshold_value "${prefix}P99_MS" "$file_path")"
  THRESHOLD_MIN_TOTAL_REQUESTS_BASELINE="$(require_threshold_value "${prefix}MIN_TOTAL_REQUESTS_BASELINE" "$file_path")"
  THRESHOLD_MIN_TOTAL_REQUESTS_SOAK="$(require_threshold_value "${prefix}MIN_TOTAL_REQUESTS_SOAK" "$file_path")"
}

resolve_mode_dependencies() {
  ensure_linux_commands awk grep sed python3
  if [[ "$MODE" == "schema-check" ]]; then
    return 0
  fi
  ensure_linux_commands curl tar
}

resolve_python_bin() {
  local candidate=""
  for candidate in python3 python; do
    if ! command -v "$candidate" >/dev/null 2>&1; then
      continue
    fi
    if "$candidate" -c 'import sys; sys.exit(0)' >/dev/null 2>&1; then
      PYTHON_BIN="$candidate"
      return 0
    fi
  done
  fail "python runtime is unavailable; checked python3 and python"
}

resolve_output_layout() {
  local timestamp=""
  if [[ -z "$OUTPUT_DIR" ]]; then
    timestamp="$(date +%Y%m%d-%H%M%S)"
    OUTPUT_DIR="$REPO_ROOT/target/public-api-gate/$timestamp-$MODE-$PROFILE-$RESOLVED_ARCH"
  fi
  WORK_DIR="$OUTPUT_DIR"
  EXTRACT_DIR="$WORK_DIR/extracted"
  RUNTIME_DIR="$WORK_DIR/runtime"
  LOG_DIR="$WORK_DIR/logs"
  RUNS_DIR="$WORK_DIR/runs"
  RESULT_PATH="$WORK_DIR/result.json"
  SUMMARY_PATH="$WORK_DIR/summary.md"
  CONFIG_PATH="$RUNTIME_DIR/gateway.toml"
  mkdir -p "$WORK_DIR" "$EXTRACT_DIR" "$RUNTIME_DIR" "$LOG_DIR" "$RUNS_DIR"
}

resolve_artifact_format() {
  if [[ -n "$FORMAT" ]]; then
    case "$FORMAT" in
      tar.gz|rpm) return 0 ;;
      *) fail "unsupported format: $FORMAT" ;;
    esac
  fi
  case "$ARTIFACT_PATH" in
    *.tar.gz) FORMAT="tar.gz" ;;
    *.rpm) FORMAT="rpm" ;;
    *) fail "cannot infer artifact format from path: $ARTIFACT_PATH" ;;
  esac
}

resolve_artifact() {
  if [[ "$MODE" == "schema-check" ]]; then
    return 0
  fi
  [[ -n "$ARTIFACT_PATH" ]] || fail "--artifact is required for mode=$MODE"
  [[ -f "$ARTIFACT_PATH" ]] || fail "artifact not found: $ARTIFACT_PATH"
  ARTIFACT_PATH="$(normalize_path "$ARTIFACT_PATH")"
  resolve_artifact_format
  if [[ "$FORMAT" == "rpm" ]]; then
    ensure_linux_commands rpm2cpio cpio
  fi
}

extract_tarball_root() {
  local artifact_path="$1"
  tar -xzf "$artifact_path" -C "$EXTRACT_DIR"
  find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1
}

extract_rpm_root() {
  local artifact_path="$1"
  local unpack_dir="$EXTRACT_DIR/rpm-root"
  mkdir -p "$unpack_dir"
  (
    cd "$unpack_dir"
    rpm2cpio "$artifact_path" | cpio -idmu --quiet
  )
  printf '%s\n' "$unpack_dir"
}

prepare_package_binary() {
  if [[ "$MODE" == "schema-check" ]]; then
    return 0
  fi
  print_stage "extracting package payload for mode=$MODE"
  case "$FORMAT" in
    tar.gz) PACKAGE_ROOT="$(extract_tarball_root "$ARTIFACT_PATH")" ;;
    rpm) PACKAGE_ROOT="$(extract_rpm_root "$ARTIFACT_PATH")" ;;
    *) fail "unsupported format while extracting: $FORMAT" ;;
  esac
  [[ -n "$PACKAGE_ROOT" && -d "$PACKAGE_ROOT" ]] || fail "failed to resolve extracted package root"
  PACKAGE_BIN="$PACKAGE_ROOT/usr/bin/gateway"
  [[ -f "$PACKAGE_BIN" ]] || fail "gateway binary not found in package root: $PACKAGE_BIN"
}

render_gateway_config() {
  cat >"$CONFIG_PATH" <<EOF
[runtime]
worker_threads = 4
graceful_shutdown_secs = 30
downstream_read_timeout_ms = 5000
upstream_connect_timeout_ms = 700
upstream_read_timeout_ms = 700
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
methods = ["GET", "HEAD"]
upstream = "fixture-backend"
filters = ["request-id"]
EOF
}

start_fixture_backend() {
  if [[ -n "$BACKEND_PID" ]]; then
    return 0
  fi
  print_stage "starting fixture backend"
  "$PYTHON_BIN" "$REPO_ROOT/scripts/fixture-backend.py" \
    --bind 127.0.0.1 \
    --port "$BACKEND_PORT" \
    --default-bytes 1024 \
    >"$LOG_DIR/backend.log" 2>&1 &
  BACKEND_PID="$!"
}

stop_fixture_backend() {
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
    BACKEND_PID=""
  fi
}

start_gateway() {
  if [[ -n "$GATEWAY_PID" ]]; then
    return 0
  fi
  print_stage "starting packaged gateway"
  "$PACKAGE_BIN" "$CONFIG_PATH" >"$LOG_DIR/gateway.log" 2>&1 &
  GATEWAY_PID="$!"
}

wait_for_status() {
  local expected="$1"
  local path="$2"
  local attempts="${3:-30}"
  local delay_secs="${4:-0.25}"
  local max_time_secs="${5:-3}"
  local url="http://127.0.0.1:${GATEWAY_PORT}${path}"
  local code=""
  for _ in $(seq 1 "$attempts"); do
    code="$(curl --max-time "$max_time_secs" -sS -o /dev/null -w "%{http_code}" -H "Host: $HOST_HEADER" "$url" || true)"
    if [[ "$code" == "$expected" ]]; then
      printf '%s\n' "$code"
      return 0
    fi
    sleep "$delay_secs"
  done
  printf '%s\n' "$code"
  return 1
}

run_load_probe() {
  local scenario="$1"
  local path="$2"
  local duration_secs="$3"
  local concurrency="$4"
  local output_path="$5"

  local url="http://127.0.0.1:${GATEWAY_PORT}${path}"
  print_stage "running load probe scenario=$scenario duration=${duration_secs}s concurrency=$concurrency path=$path"

  "$PYTHON_BIN" - "$url" "$HOST_HEADER" "$duration_secs" "$concurrency" "$output_path" "$scenario" <<'PY'
import asyncio
import json
import sys
import time
from collections import Counter
from urllib.parse import urlparse

url = sys.argv[1]
host_header = sys.argv[2]
duration_secs = float(sys.argv[3])
concurrency = int(sys.argv[4])
output_path = sys.argv[5]
scenario = sys.argv[6]

parsed = urlparse(url)
if parsed.scheme != "http":
    raise SystemExit(f"unsupported scheme for gate probe: {parsed.scheme!r}")

target_host = parsed.hostname or "127.0.0.1"
target_port = parsed.port or 80
target_path = parsed.path or "/"
if parsed.query:
    target_path = f"{target_path}?{parsed.query}"

request_bytes = (
    f"GET {target_path} HTTP/1.1\r\n"
    f"Host: {host_header}\r\n"
    "Content-Length: 0\r\n"
    "Connection: close\r\n"
    "\r\n"
).encode("ascii")


def percentile(values, ratio):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = round((len(ordered) - 1) * ratio)
    return float(ordered[index])


async def single_request():
    started = time.perf_counter()
    reader = None
    writer = None
    try:
        reader, writer = await asyncio.open_connection(target_host, target_port)
        writer.write(request_bytes)
        await writer.drain()

        response = b""
        while b"\r\n\r\n" not in response:
            chunk = await reader.read(4096)
            if not chunk:
                raise RuntimeError("connection closed before response headers")
            response += chunk

        header_end = response.find(b"\r\n\r\n")
        header_bytes = response[:header_end].decode("latin1")
        status_parts = header_bytes.split("\r\n", 1)[0].split(" ")
        if len(status_parts) < 2 or not status_parts[1].isdigit():
            raise RuntimeError(f"invalid status line: {header_bytes.splitlines()[0]}")
        status_code = int(status_parts[1])

        headers = {}
        for line in header_bytes.split("\r\n")[1:]:
            if ":" not in line:
                continue
            name, value = line.split(":", 1)
            headers[name.strip().lower()] = value.strip()

        content_length = int(headers.get("content-length", "0") or "0")
        body = response[header_end + 4 :]
        while len(body) < content_length:
            chunk = await reader.read(4096)
            if not chunk:
                raise RuntimeError("connection closed before response body")
            body += chunk

        latency_ms = (time.perf_counter() - started) * 1000.0
        return {
            "ok": True,
            "status_code": status_code,
            "headers": headers,
            "latency_ms": latency_ms,
        }
    except Exception as exc:
        latency_ms = (time.perf_counter() - started) * 1000.0
        return {
            "ok": False,
            "error": str(exc),
            "latency_ms": latency_ms,
        }
    finally:
        if writer is not None:
            writer.close()
            try:
                await writer.wait_closed()
            except Exception:
                pass


async def worker(deadline, metrics):
    while time.perf_counter() < deadline:
        result = await single_request()
        metrics["latencies_ms"].append(result["latency_ms"])
        if result["ok"]:
            status = result["status_code"]
            headers = result["headers"]
            if 200 <= status < 300:
                metrics["success_2xx"] += 1
            elif status >= 500:
                if headers.get("x-rivulet-fixture", "").lower() == "true":
                    metrics["upstream_5xx"] += 1
                else:
                    metrics["gateway_5xx"] += 1
            else:
                metrics["other_status"] += 1
        else:
            metrics["network_errors"] += 1
            metrics["errors"][result["error"]] += 1


async def main():
    deadline = time.perf_counter() + duration_secs
    metrics = {
        "success_2xx": 0,
        "gateway_5xx": 0,
        "upstream_5xx": 0,
        "other_status": 0,
        "network_errors": 0,
        "latencies_ms": [],
        "errors": Counter(),
    }
    await asyncio.gather(*[worker(deadline, metrics) for _ in range(concurrency)])

    total_requests = (
        metrics["success_2xx"]
        + metrics["gateway_5xx"]
        + metrics["upstream_5xx"]
        + metrics["other_status"]
        + metrics["network_errors"]
    )
    availability = (metrics["success_2xx"] / total_requests * 100.0) if total_requests else 0.0
    gateway_5xx_ratio = (metrics["gateway_5xx"] / total_requests * 100.0) if total_requests else 0.0

    result = {
        "scenario": scenario,
        "duration_secs": duration_secs,
        "concurrency": concurrency,
        "total_requests": total_requests,
        "success_2xx": metrics["success_2xx"],
        "gateway_5xx": metrics["gateway_5xx"],
        "upstream_5xx": metrics["upstream_5xx"],
        "other_status": metrics["other_status"],
        "network_errors": metrics["network_errors"],
        "availability": round(availability, 6),
        "gateway_5xx_ratio": round(gateway_5xx_ratio, 6),
        "p95_ms": round(percentile(metrics["latencies_ms"], 0.95), 3),
        "p99_ms": round(percentile(metrics["latencies_ms"], 0.99), 3),
        "top_errors": [
            {"message": message, "count": count}
            for message, count in metrics["errors"].most_common(3)
        ],
    }

    with open(output_path, "w", encoding="utf-8") as fp:
        json.dump(result, fp, ensure_ascii=False, indent=2)


asyncio.run(main())
PY
}

aggregate_sample_metrics() {
  local output_path="$1"
  shift
  "$PYTHON_BIN" - "$output_path" "$@" <<'PY'
import json
import statistics
import sys

output_path = sys.argv[1]
sample_paths = sys.argv[2:]
if not sample_paths:
    raise SystemExit("no sample paths provided")

samples = []
for path in sample_paths:
    with open(path, "r", encoding="utf-8") as fp:
        samples.append(json.load(fp))

availability = [item["availability"] for item in samples]
gateway_ratio = [item["gateway_5xx_ratio"] for item in samples]
p95 = [item["p95_ms"] for item in samples]
p99 = [item["p99_ms"] for item in samples]
requests = [item["total_requests"] for item in samples]

summary = {
    "scenario": samples[0]["scenario"].split("-sample-")[0],
    "sample_count": len(samples),
    "samples": sample_paths,
    "availability_median": round(statistics.median(availability), 6),
    "gateway_5xx_ratio_median": round(statistics.median(gateway_ratio), 6),
    "p95_ms_median": round(statistics.median(p95), 3),
    "p99_ms_median": round(statistics.median(p99), 3),
    "total_requests_sum": int(sum(requests)),
}

with open(output_path, "w", encoding="utf-8") as fp:
    json.dump(summary, fp, ensure_ascii=False, indent=2)
PY
}

prepare_runtime() {
  if [[ "$MODE" == "schema-check" ]]; then
    return 0
  fi
  render_gateway_config
  start_fixture_backend
  start_gateway

  print_stage "waiting for gateway healthy response"
  local status=""
  status="$(wait_for_status "200" "/fixture/default" 40 0.25 3)" || true
  if [[ "$status" != "200" ]]; then
    fail "gateway did not become healthy on /fixture/default, status=$status"
  fi
}

run_baseline_samples() {
  local summary_path="$RUNS_DIR/baseline-summary.json"
  local sample_paths=()
  local sample_path=""
  local index=0
  for index in $(seq 1 "$SAMPLE_COUNT"); do
    sample_path="$RUNS_DIR/baseline-sample-$index.json"
    run_load_probe "baseline-sample-$index" "/fixture/default" "$BASELINE_DURATION_SECS" "$BASELINE_CONCURRENCY" "$sample_path"
    sample_paths+=("$sample_path")
    sleep 1
  done
  aggregate_sample_metrics "$summary_path" "${sample_paths[@]}"
  printf '%s\n' "$summary_path"
}

run_soak_samples() {
  local summary_path="$RUNS_DIR/soak-summary.json"
  local sample_paths=()
  local sample_path=""
  local index=0
  for index in $(seq 1 "$SAMPLE_COUNT"); do
    sample_path="$RUNS_DIR/soak-sample-$index.json"
    run_load_probe "soak-sample-$index" "/fixture/default" "$SOAK_DURATION_SECS" "$SOAK_CONCURRENCY" "$sample_path"
    sample_paths+=("$sample_path")
    sleep 1
  done
  aggregate_sample_metrics "$summary_path" "${sample_paths[@]}"
  printf '%s\n' "$summary_path"
}

run_failure_drill() {
  local summary_path="$RUNS_DIR/failure-drill-summary.json"
  local business_path="$RUNS_DIR/failure-business-503.json"
  local timeout_path="$RUNS_DIR/failure-timeout.json"
  local reset_path="$RUNS_DIR/failure-reset.json"
  local backend_down_path="$RUNS_DIR/failure-backend-down.json"
  local recovery_status=""

  # 先验证上游业务 5xx 不应计入网关 5xx，再验证 timeout/reset/backend-down 的网关故障分类。
  run_load_probe "failure-business-503" "/fixture/status/503" "$FAILURE_DURATION_SECS" 8 "$business_path"
  run_load_probe "failure-timeout" "/fixture/delay/2000" "$FAILURE_DURATION_SECS" 8 "$timeout_path"
  run_load_probe "failure-reset" "/fixture/reset" "$FAILURE_DURATION_SECS" 6 "$reset_path"

  print_stage "stopping fixture backend for backend-down drill"
  stop_fixture_backend
  run_load_probe "failure-backend-down" "/fixture/default" "$FAILURE_DURATION_SECS" 4 "$backend_down_path"

  print_stage "restarting fixture backend for recovery check"
  start_fixture_backend
  recovery_status="$(wait_for_status "200" "/fixture/default" 20 0.25 3 || true)"

  "$PYTHON_BIN" - "$summary_path" "$business_path" "$timeout_path" "$reset_path" "$backend_down_path" "$recovery_status" <<'PY'
import json
import sys

summary_path = sys.argv[1]
business_path = sys.argv[2]
timeout_path = sys.argv[3]
reset_path = sys.argv[4]
backend_down_path = sys.argv[5]
recovery_status = sys.argv[6]

with open(business_path, "r", encoding="utf-8") as fp:
    business = json.load(fp)
with open(timeout_path, "r", encoding="utf-8") as fp:
    timeout_case = json.load(fp)
with open(reset_path, "r", encoding="utf-8") as fp:
    reset_case = json.load(fp)
with open(backend_down_path, "r", encoding="utf-8") as fp:
    backend_down = json.load(fp)

gateway_fault_5xx_total = timeout_case["gateway_5xx"] + reset_case["gateway_5xx"] + backend_down["gateway_5xx"]
summary = {
    "scenario": "failure-drill",
    "business_upstream_5xx": business["upstream_5xx"],
    "business_gateway_5xx": business["gateway_5xx"],
    "gateway_fault_5xx_total": gateway_fault_5xx_total,
    "recovery_status": recovery_status,
    "business_errors_are_upstream_only": business["upstream_5xx"] > 0 and business["gateway_5xx"] == 0,
    "gateway_faults_observed": gateway_fault_5xx_total > 0,
    "recovery_pass": recovery_status == "200",
}
summary["pass"] = summary["business_errors_are_upstream_only"] and summary["gateway_faults_observed"] and summary["recovery_pass"]

with open(summary_path, "w", encoding="utf-8") as fp:
    json.dump(summary, fp, ensure_ascii=False, indent=2)
PY

  printf '%s\n' "$summary_path"
}

write_schema_outputs() {
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$MODE" "$PROFILE" "$RESOLVED_ARCH" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" \
    "$THRESHOLD_MIN_TOTAL_REQUESTS_BASELINE" "$THRESHOLD_MIN_TOTAL_REQUESTS_SOAK" "$DURATION_PROFILE" "$LEGACY_FLAT_KEYS_PRESENT" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
mode = sys.argv[3]
profile = sys.argv[4]
arch = sys.argv[5]
threshold_file = sys.argv[6]
availability = float(sys.argv[7])
gateway_ratio = float(sys.argv[8])
p95 = float(sys.argv[9])
p99 = float(sys.argv[10])
min_baseline = int(sys.argv[11])
min_soak = int(sys.argv[12])
duration_profile = sys.argv[13]
legacy_flat_keys_present = sys.argv[14].lower() == "true"

result = {
    "mode": mode,
    "profile": profile,
    "arch": arch,
    "duration_profile": duration_profile,
    "threshold_file": threshold_file,
    "thresholds": {
        "availability_min": availability,
        "gateway_5xx_ratio_max": gateway_ratio,
        "p95_ms_max": p95,
        "p99_ms_max": p99,
        "min_total_requests_baseline": min_baseline,
        "min_total_requests_soak": min_soak,
    },
    "legacy_flat_keys_present": legacy_flat_keys_present,
    "pass": True,
    "message": "threshold schema and profile-arch contract are valid",
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Summary\n\n")
    fp.write("## Mode\n\n")
    fp.write(f"- mode: `{mode}`\n")
    fp.write(f"- profile: `{profile}`\n")
    fp.write(f"- arch: `{arch}`\n")
    fp.write(f"- duration_profile: `{duration_profile}`\n")
    fp.write(f"- threshold file: `{threshold_file}`\n\n")
    fp.write("## Result\n\n")
    fp.write("- schema-check passed\n")
    fp.write(f"- availability >= `{availability}`\n")
    fp.write(f"- gateway_5xx_ratio <= `{gateway_ratio}`\n")
    fp.write(f"- p95 <= `{p95} ms`\n")
    fp.write(f"- p99 <= `{p99} ms`\n")
    fp.write(f"- min_total_requests_baseline >= `{min_baseline}`\n")
    fp.write(f"- min_total_requests_soak >= `{min_soak}`\n")
PY
}

compose_single_mode_result() {
  local scenario="$1"
  local scenario_summary_json="$2"
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$scenario" "$PROFILE" "$RESOLVED_ARCH" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" \
    "$THRESHOLD_MIN_TOTAL_REQUESTS_BASELINE" "$THRESHOLD_MIN_TOTAL_REQUESTS_SOAK" "$DURATION_PROFILE" "$scenario_summary_json" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
scenario = sys.argv[3]
profile = sys.argv[4]
arch = sys.argv[5]
threshold_file = sys.argv[6]
availability = float(sys.argv[7])
gateway_ratio = float(sys.argv[8])
p95 = float(sys.argv[9])
p99 = float(sys.argv[10])
min_baseline = int(sys.argv[11])
min_soak = int(sys.argv[12])
duration_profile = sys.argv[13]
scenario_summary_path = sys.argv[14]

with open(scenario_summary_path, "r", encoding="utf-8") as fp:
    scenario_summary = json.load(fp)

pass_value = True
message = "single scenario report generated"
if scenario == "baseline":
    pass_value = scenario_summary.get("total_requests_sum", 0) >= min_baseline
    message = "baseline request-floor check"
elif scenario == "soak":
    pass_value = scenario_summary.get("total_requests_sum", 0) >= min_soak
    message = "soak request-floor check"
elif scenario == "failure-drill":
    pass_value = bool(scenario_summary.get("pass", False))
    message = "failure drill classification and recovery checks"

result = {
    "mode": scenario,
    "profile": profile,
    "arch": arch,
    "duration_profile": duration_profile,
    "threshold_file": threshold_file,
    "thresholds": {
        "availability_min": availability,
        "gateway_5xx_ratio_max": gateway_ratio,
        "p95_ms_max": p95,
        "p99_ms_max": p99,
        "min_total_requests_baseline": min_baseline,
        "min_total_requests_soak": min_soak,
    },
    "scenario_summary": scenario_summary,
    "pass": pass_value,
    "message": message,
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Summary\n\n")
    fp.write(f"- mode: `{scenario}`\n")
    fp.write(f"- profile: `{profile}`\n")
    fp.write(f"- arch: `{arch}`\n")
    fp.write(f"- duration_profile: `{duration_profile}`\n")
    fp.write(f"- threshold file: `{threshold_file}`\n")
    fp.write(f"- result json: `{result_path}`\n\n")
    fp.write("## Scenario Summary\n\n")
    fp.write("```json\n")
    fp.write(json.dumps(scenario_summary, ensure_ascii=False, indent=2))
    fp.write("\n```\n")
PY
}

compose_evaluate_result() {
  local baseline_summary="$1"
  local soak_summary="$2"
  local failure_summary="$3"
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$PROFILE" "$RESOLVED_ARCH" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" \
    "$THRESHOLD_MIN_TOTAL_REQUESTS_BASELINE" "$THRESHOLD_MIN_TOTAL_REQUESTS_SOAK" "$baseline_summary" "$soak_summary" \
    "$failure_summary" "$MODE" "$ARTIFACT_PATH" "$FORMAT" "$DURATION_PROFILE" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
profile = sys.argv[3]
arch = sys.argv[4]
threshold_file = sys.argv[5]
threshold_availability = float(sys.argv[6])
threshold_gateway_ratio = float(sys.argv[7])
threshold_p95 = float(sys.argv[8])
threshold_p99 = float(sys.argv[9])
threshold_min_requests_baseline = int(sys.argv[10])
threshold_min_requests_soak = int(sys.argv[11])
baseline_path = sys.argv[12]
soak_path = sys.argv[13]
failure_path = sys.argv[14]
mode = sys.argv[15]
artifact_path = sys.argv[16]
artifact_format = sys.argv[17]
duration_profile = sys.argv[18]

with open(baseline_path, "r", encoding="utf-8") as fp:
    baseline = json.load(fp)
with open(soak_path, "r", encoding="utf-8") as fp:
    soak = json.load(fp)
with open(failure_path, "r", encoding="utf-8") as fp:
    failure = json.load(fp)

observed_availability = min(baseline["availability_median"], soak["availability_median"])
observed_gateway_ratio = max(baseline["gateway_5xx_ratio_median"], soak["gateway_5xx_ratio_median"])
observed_p95 = max(baseline["p95_ms_median"], soak["p95_ms_median"])
observed_p99 = max(baseline["p99_ms_median"], soak["p99_ms_median"])

request_floor_checks = {
    "baseline": {
        "minimum": threshold_min_requests_baseline,
        "observed": int(baseline["total_requests_sum"]),
        "pass": int(baseline["total_requests_sum"]) >= threshold_min_requests_baseline,
    },
    "soak": {
        "minimum": threshold_min_requests_soak,
        "observed": int(soak["total_requests_sum"]),
        "pass": int(soak["total_requests_sum"]) >= threshold_min_requests_soak,
    },
}
threshold_checks = {
    "availability": observed_availability >= threshold_availability,
    "gateway_5xx_ratio": observed_gateway_ratio <= threshold_gateway_ratio,
    "p95_ms": observed_p95 <= threshold_p95,
    "p99_ms": observed_p99 <= threshold_p99,
    "failure_drill": bool(failure.get("pass", False)),
}
failure_reasons = []
if not request_floor_checks["baseline"]["pass"]:
    failure_reasons.append("insufficient_baseline_requests")
if not request_floor_checks["soak"]["pass"]:
    failure_reasons.append("insufficient_soak_requests")
if not threshold_checks["availability"]:
    failure_reasons.append("availability_below_threshold")
if not threshold_checks["gateway_5xx_ratio"]:
    failure_reasons.append("gateway_5xx_ratio_above_threshold")
if not threshold_checks["p95_ms"]:
    failure_reasons.append("p95_above_threshold")
if not threshold_checks["p99_ms"]:
    failure_reasons.append("p99_above_threshold")
if not threshold_checks["failure_drill"]:
    failure_reasons.append("failure_drill_not_passed")

would_pass = len(failure_reasons) == 0
blocking = profile != "observe"
gate_pass = would_pass if blocking else True

result = {
    "mode": mode,
    "profile": profile,
    "arch": arch,
    "duration_profile": duration_profile,
    "artifact": {
        "path": artifact_path,
        "format": artifact_format,
    },
    "threshold_file": threshold_file,
    "thresholds": {
        "availability_min": threshold_availability,
        "gateway_5xx_ratio_max": threshold_gateway_ratio,
        "p95_ms_max": threshold_p95,
        "p99_ms_max": threshold_p99,
        "min_total_requests_baseline": threshold_min_requests_baseline,
        "min_total_requests_soak": threshold_min_requests_soak,
    },
    "scenarios": {
        "baseline": baseline,
        "soak": soak,
        "failure_drill": failure,
    },
    "observed": {
        "availability": round(observed_availability, 6),
        "gateway_5xx_ratio": round(observed_gateway_ratio, 6),
        "p95_ms": round(observed_p95, 3),
        "p99_ms": round(observed_p99, 3),
        "failure_drill_pass": bool(failure.get("pass", False)),
    },
    "request_floor_checks": request_floor_checks,
    "threshold_checks": threshold_checks,
    "failure_reasons": failure_reasons,
    "blocking_profile": blocking,
    "would_pass": would_pass,
    "pass": gate_pass,
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Summary\n\n")
    fp.write("## Inputs\n\n")
    fp.write(f"- mode: `{mode}`\n")
    fp.write(f"- profile: `{profile}`\n")
    fp.write(f"- arch: `{arch}`\n")
    fp.write(f"- duration_profile: `{duration_profile}`\n")
    fp.write(f"- artifact: `{artifact_path}`\n")
    fp.write(f"- format: `{artifact_format}`\n")
    fp.write(f"- threshold file: `{threshold_file}`\n\n")
    fp.write("## Observed\n\n")
    fp.write(f"- availability: `{round(observed_availability, 6)}`\n")
    fp.write(f"- gateway_5xx_ratio: `{round(observed_gateway_ratio, 6)}`\n")
    fp.write(f"- p95: `{round(observed_p95, 3)} ms`\n")
    fp.write(f"- p99: `{round(observed_p99, 3)} ms`\n")
    fp.write(f"- failure_drill_pass: `{bool(failure.get('pass', False))}`\n")
    fp.write(f"- baseline request floor: `{request_floor_checks['baseline']['observed']}/{request_floor_checks['baseline']['minimum']}`\n")
    fp.write(f"- soak request floor: `{request_floor_checks['soak']['observed']}/{request_floor_checks['soak']['minimum']}`\n")
    fp.write(f"- would_pass: `{would_pass}`\n")
    fp.write(f"- pass: `{gate_pass}`\n")
    if failure_reasons:
        fp.write(f"- failure_reasons: `{', '.join(failure_reasons)}`\n")
PY
}

run_mode() {
  local baseline_summary=""
  local soak_summary=""
  local failure_summary=""
  case "$MODE" in
    schema-check)
      write_schema_outputs
      ;;
    baseline)
      baseline_summary="$(run_baseline_samples)"
      compose_single_mode_result "baseline" "$baseline_summary"
      ;;
    soak)
      soak_summary="$(run_soak_samples)"
      compose_single_mode_result "soak" "$soak_summary"
      ;;
    failure-drill)
      failure_summary="$(run_failure_drill)"
      compose_single_mode_result "failure-drill" "$failure_summary"
      ;;
    evaluate)
      baseline_summary="$(run_baseline_samples)"
      soak_summary="$(run_soak_samples)"
      failure_summary="$(run_failure_drill)"
      compose_evaluate_result "$baseline_summary" "$soak_summary" "$failure_summary"
      ;;
    *)
      fail "unsupported mode: $MODE"
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="$2"
      shift 2
      ;;
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    --arch)
      TARGET_ARCH="$2"
      shift 2
      ;;
    --threshold-file)
      THRESHOLDS_FILE="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --artifact)
      ARTIFACT_PATH="$2"
      shift 2
      ;;
    --format)
      FORMAT="$2"
      shift 2
      ;;
    --host)
      HOST_HEADER="$2"
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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

case "$PROFILE" in
  standard|strict|observe)
    ;;
  *)
    fail "unsupported profile: $PROFILE"
    ;;
esac

case "$MODE" in
  schema-check|baseline|soak|failure-drill|evaluate)
    ;;
  *)
    fail "unsupported mode: $MODE"
    ;;
esac

resolve_mode_dependencies
resolve_python_bin
resolve_target_arch
resolve_output_layout
load_threshold_profile "$THRESHOLDS_FILE"
resolve_artifact
prepare_package_binary
prepare_runtime

run_mode

print_stage "public api gate mode=$MODE profile=$PROFILE arch=$RESOLVED_ARCH completed"
echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"

if [[ "$MODE" == "evaluate" ]]; then
  gate_pass="$("$PYTHON_BIN" - "$RESULT_PATH" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fp:
    result = json.load(fp)
print("true" if result.get("pass") else "false")
PY
)"
  if [[ "$gate_pass" != "true" ]]; then
    fail "public-api gate did not pass for profile=$PROFILE arch=$RESOLVED_ARCH"
  fi
fi

exit 0
