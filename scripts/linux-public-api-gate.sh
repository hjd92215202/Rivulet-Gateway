#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

MODE="schema-check"
PROFILE="standard"
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

# 默认负载配置保持保守，优先生产可用验证，不追求极限跑分。
BASELINE_DURATION_SECS=6
BASELINE_CONCURRENCY=12
SOAK_DURATION_SECS=12
SOAK_CONCURRENCY=16
SAMPLE_COUNT=3
FAILURE_DURATION_SECS=4

THRESHOLD_AVAILABILITY=""
THRESHOLD_GATEWAY_5XX_RATIO_MAX=""
THRESHOLD_P95_MS=""
THRESHOLD_P99_MS=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/linux-public-api-gate.sh [options]

Execute or validate public API reliability/capacity gate scenarios.

Options:
  --mode <schema-check|baseline|soak|failure-drill|evaluate>
  --profile <standard|strict|observe>          default: standard
  --threshold-file <path>                      default: ./scripts/public-api-thresholds.env
  --output-dir <path>                          default: ./target/public-api-gate/<timestamp>-<mode>-<profile>
  --artifact <path>                            required for non-schema modes, accepts .tar.gz or .rpm
  --format <tar.gz|rpm>                        optional, auto-detected when omitted
  --host <host>                                default: localhost
  --gateway-port <port>                        default: 18680
  --backend-port <port>                        default: 19680
  -h, --help

Modes:
  schema-check   validate threshold schema and profile contracts only
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

validate_threshold_value() {
  local key="$1"
  local value="$2"

  validate_numeric "$key" "$value"

  case "$key" in
    *_SLO_AVAILABILITY)
      awk "BEGIN {exit !($value > 0 && $value <= 100)}" || fail "$key must be within (0, 100]"
      ;;
    *_GATEWAY_5XX_RATIO_MAX)
      awk "BEGIN {exit !($value >= 0 && $value <= 100)}" || fail "$key must be within [0, 100]"
      ;;
    *_P95_MS|*_P99_MS)
      awk "BEGIN {exit !($value > 0)}" || fail "$key must be greater than 0"
      ;;
    *)
      fail "unknown threshold key: $key"
      ;;
  esac
}

is_supported_threshold_key() {
  local key="$1"
  case "$key" in
    PUBLIC_API_STANDARD_SLO_AVAILABILITY|PUBLIC_API_STANDARD_GATEWAY_5XX_RATIO_MAX|PUBLIC_API_STANDARD_P95_MS|PUBLIC_API_STANDARD_P99_MS|\
    PUBLIC_API_STRICT_SLO_AVAILABILITY|PUBLIC_API_STRICT_GATEWAY_5XX_RATIO_MAX|PUBLIC_API_STRICT_P95_MS|PUBLIC_API_STRICT_P99_MS|\
    PUBLIC_API_OBSERVE_SLO_AVAILABILITY|PUBLIC_API_OBSERVE_GATEWAY_5XX_RATIO_MAX|PUBLIC_API_OBSERVE_P95_MS|PUBLIC_API_OBSERVE_P99_MS|\
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
      if (raw == "" || substr(raw, 1, 1) == "#") {
        next
      }
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
  if [[ -z "$value" ]]; then
    fail "missing required key: $key"
  fi
  printf '%s\n' "$value"
}

load_threshold_profile() {
  local file_path="$1"

  if [[ ! -f "$file_path" ]]; then
    fail "missing threshold file: $file_path"
  fi

  local key=""
  local value=""
  while IFS='=' read -r key value; do
    key="$(echo "$key" | sed 's/[[:space:]]//g')"
    value="$(echo "$value" | sed 's/[[:space:]]//g')"
    [[ -z "$key" || "${key:0:1}" == "#" ]] && continue
    [[ -z "$value" ]] && fail "$key has empty value"
    is_supported_threshold_key "$key" || fail "unknown key in threshold file: $key"
    validate_threshold_value "$key" "$value"
  done <"$file_path"

  case "$PROFILE" in
    standard)
      # 向后兼容：若标准档新键不存在，回退到旧版扁平键。
      THRESHOLD_AVAILABILITY="$(get_threshold_value "PUBLIC_API_STANDARD_SLO_AVAILABILITY" "$file_path")"
      THRESHOLD_GATEWAY_5XX_RATIO_MAX="$(get_threshold_value "PUBLIC_API_STANDARD_GATEWAY_5XX_RATIO_MAX" "$file_path")"
      THRESHOLD_P95_MS="$(get_threshold_value "PUBLIC_API_STANDARD_P95_MS" "$file_path")"
      THRESHOLD_P99_MS="$(get_threshold_value "PUBLIC_API_STANDARD_P99_MS" "$file_path")"

      [[ -n "$THRESHOLD_AVAILABILITY" ]] || THRESHOLD_AVAILABILITY="$(require_threshold_value "PUBLIC_API_SLO_AVAILABILITY" "$file_path")"
      [[ -n "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" ]] || THRESHOLD_GATEWAY_5XX_RATIO_MAX="$(require_threshold_value "PUBLIC_API_GATEWAY_5XX_RATIO_MAX" "$file_path")"
      [[ -n "$THRESHOLD_P95_MS" ]] || THRESHOLD_P95_MS="$(require_threshold_value "PUBLIC_API_P95_MS" "$file_path")"
      [[ -n "$THRESHOLD_P99_MS" ]] || THRESHOLD_P99_MS="$(require_threshold_value "PUBLIC_API_P99_MS" "$file_path")"
      ;;
    strict)
      THRESHOLD_AVAILABILITY="$(require_threshold_value "PUBLIC_API_STRICT_SLO_AVAILABILITY" "$file_path")"
      THRESHOLD_GATEWAY_5XX_RATIO_MAX="$(require_threshold_value "PUBLIC_API_STRICT_GATEWAY_5XX_RATIO_MAX" "$file_path")"
      THRESHOLD_P95_MS="$(require_threshold_value "PUBLIC_API_STRICT_P95_MS" "$file_path")"
      THRESHOLD_P99_MS="$(require_threshold_value "PUBLIC_API_STRICT_P99_MS" "$file_path")"
      ;;
    observe)
      THRESHOLD_AVAILABILITY="$(require_threshold_value "PUBLIC_API_OBSERVE_SLO_AVAILABILITY" "$file_path")"
      THRESHOLD_GATEWAY_5XX_RATIO_MAX="$(require_threshold_value "PUBLIC_API_OBSERVE_GATEWAY_5XX_RATIO_MAX" "$file_path")"
      THRESHOLD_P95_MS="$(require_threshold_value "PUBLIC_API_OBSERVE_P95_MS" "$file_path")"
      THRESHOLD_P99_MS="$(require_threshold_value "PUBLIC_API_OBSERVE_P99_MS" "$file_path")"
      ;;
    *)
      fail "unsupported profile: $PROFILE"
      ;;
  esac
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
    OUTPUT_DIR="$REPO_ROOT/target/public-api-gate/$timestamp-$MODE-$PROFILE"
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
      tar.gz|rpm)
        ;;
      *)
        fail "unsupported format: $FORMAT"
        ;;
    esac
    return 0
  fi

  case "$ARTIFACT_PATH" in
    *.tar.gz)
      FORMAT="tar.gz"
      ;;
    *.rpm)
      FORMAT="rpm"
      ;;
    *)
      fail "cannot infer artifact format from path: $ARTIFACT_PATH"
      ;;
  esac
}

resolve_artifact() {
  if [[ "$MODE" == "schema-check" ]]; then
    return 0
  fi

  if [[ -z "$ARTIFACT_PATH" ]]; then
    fail "--artifact is required for mode=$MODE"
  fi

  if [[ ! -f "$ARTIFACT_PATH" ]]; then
    fail "artifact not found: $ARTIFACT_PATH"
  fi

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
    tar.gz)
      PACKAGE_ROOT="$(extract_tarball_root "$ARTIFACT_PATH")"
      ;;
    rpm)
      PACKAGE_ROOT="$(extract_rpm_root "$ARTIFACT_PATH")"
      ;;
    *)
      fail "unsupported format while extracting: $FORMAT"
      ;;
  esac

  if [[ -z "$PACKAGE_ROOT" || ! -d "$PACKAGE_ROOT" ]]; then
    fail "failed to resolve extracted package root"
  fi

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

write_schema_outputs() {
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$MODE" "$PROFILE" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
mode = sys.argv[3]
profile = sys.argv[4]
threshold_file = sys.argv[5]
availability = float(sys.argv[6])
gateway_ratio = float(sys.argv[7])
p95 = float(sys.argv[8])
p99 = float(sys.argv[9])

result = {
    "mode": mode,
    "profile": profile,
    "threshold_file": threshold_file,
    "thresholds": {
        "availability_min": availability,
        "gateway_5xx_ratio_max": gateway_ratio,
        "p95_ms_max": p95,
        "p99_ms_max": p99,
    },
    "pass": True,
    "message": "threshold schema and profile contract are valid",
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Summary\n\n")
    fp.write("## Mode\n\n")
    fp.write(f"- mode: `{mode}`\n")
    fp.write(f"- profile: `{profile}`\n")
    fp.write(f"- threshold file: `{threshold_file}`\n\n")
    fp.write("## Result\n\n")
    fp.write("- schema-check passed\n")
    fp.write(f"- availability >= `{availability}`\n")
    fp.write(f"- gateway_5xx_ratio <= `{gateway_ratio}`\n")
    fp.write(f"- p95 <= `{p95} ms`\n")
    fp.write(f"- p99 <= `{p99} ms`\n")
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

  # 先压一段上游业务 5xx，验证分类是否准确，不应被算进网关 5xx。
  run_load_probe "failure-business-503" "/fixture/status/503" "$FAILURE_DURATION_SECS" 8 "$business_path"
  # 再压上游慢响应和连接重置，验证网关侧 5xx 的故障路径能被观测到。
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

gateway_fault_5xx_total = (
    timeout_case["gateway_5xx"] + reset_case["gateway_5xx"] + backend_down["gateway_5xx"]
)

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
summary["pass"] = (
    summary["business_errors_are_upstream_only"]
    and summary["gateway_faults_observed"]
    and summary["recovery_pass"]
)

with open(summary_path, "w", encoding="utf-8") as fp:
    json.dump(summary, fp, ensure_ascii=False, indent=2)
PY

  printf '%s\n' "$summary_path"
}

compose_single_mode_result() {
  local scenario="$1"
  local scenario_summary_json="$2"
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$scenario" "$PROFILE" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" "$scenario_summary_json" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
scenario = sys.argv[3]
profile = sys.argv[4]
threshold_file = sys.argv[5]
availability = float(sys.argv[6])
gateway_ratio = float(sys.argv[7])
p95 = float(sys.argv[8])
p99 = float(sys.argv[9])
scenario_summary_path = sys.argv[10]

with open(scenario_summary_path, "r", encoding="utf-8") as fp:
    scenario_summary = json.load(fp)

result = {
    "mode": scenario,
    "profile": profile,
    "threshold_file": threshold_file,
    "thresholds": {
        "availability_min": availability,
        "gateway_5xx_ratio_max": gateway_ratio,
        "p95_ms_max": p95,
        "p99_ms_max": p99,
    },
    "scenario_summary": scenario_summary,
    "pass": True,
}

if scenario == "failure-drill":
    result["pass"] = bool(scenario_summary.get("pass", False))
    result["message"] = "failure drill classification and recovery checks"

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Summary\n\n")
    fp.write(f"- mode: `{scenario}`\n")
    fp.write(f"- profile: `{profile}`\n")
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
  "$PYTHON_BIN" - "$RESULT_PATH" "$SUMMARY_PATH" "$PROFILE" "$THRESHOLDS_FILE" \
    "$THRESHOLD_AVAILABILITY" "$THRESHOLD_GATEWAY_5XX_RATIO_MAX" "$THRESHOLD_P95_MS" "$THRESHOLD_P99_MS" \
    "$baseline_summary" "$soak_summary" "$failure_summary" "$MODE" "$ARTIFACT_PATH" "$FORMAT" <<'PY'
import json
import sys

result_path = sys.argv[1]
summary_path = sys.argv[2]
profile = sys.argv[3]
threshold_file = sys.argv[4]
threshold_availability = float(sys.argv[5])
threshold_gateway_ratio = float(sys.argv[6])
threshold_p95 = float(sys.argv[7])
threshold_p99 = float(sys.argv[8])
baseline_path = sys.argv[9]
soak_path = sys.argv[10]
failure_path = sys.argv[11]
mode = sys.argv[12]
artifact_path = sys.argv[13]
artifact_format = sys.argv[14]

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

would_pass = (
    observed_availability >= threshold_availability
    and observed_gateway_ratio <= threshold_gateway_ratio
    and observed_p95 <= threshold_p95
    and observed_p99 <= threshold_p99
    and failure.get("pass", False)
)

# observe 档用于“产出报告不阻断”，因此保留 would_pass，同时把 gate pass 固定为 true。
blocking = profile != "observe"
gate_pass = would_pass if blocking else True

result = {
    "mode": mode,
    "profile": profile,
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
    fp.write(f"- artifact: `{artifact_path}`\n")
    fp.write(f"- format: `{artifact_format}`\n")
    fp.write(f"- threshold file: `{threshold_file}`\n\n")
    fp.write("## Thresholds\n\n")
    fp.write(f"- availability >= `{threshold_availability}`\n")
    fp.write(f"- gateway_5xx_ratio <= `{threshold_gateway_ratio}`\n")
    fp.write(f"- p95 <= `{threshold_p95} ms`\n")
    fp.write(f"- p99 <= `{threshold_p99} ms`\n\n")
    fp.write("## Observed\n\n")
    fp.write(f"- availability: `{round(observed_availability, 6)}`\n")
    fp.write(f"- gateway_5xx_ratio: `{round(observed_gateway_ratio, 6)}`\n")
    fp.write(f"- p95: `{round(observed_p95, 3)} ms`\n")
    fp.write(f"- p99: `{round(observed_p99, 3)} ms`\n")
    fp.write(f"- failure_drill_pass: `{bool(failure.get('pass', False))}`\n")
    fp.write(f"- blocking profile: `{blocking}`\n")
    fp.write(f"- would_pass: `{would_pass}`\n")
    fp.write(f"- pass: `{gate_pass}`\n\n")
    fp.write("## Scenario Files\n\n")
    fp.write(f"- baseline summary: `{baseline_path}`\n")
    fp.write(f"- soak summary: `{soak_path}`\n")
    fp.write(f"- failure summary: `{failure_path}`\n")
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
resolve_output_layout
load_threshold_profile "$THRESHOLDS_FILE"
resolve_artifact
prepare_package_binary
prepare_runtime

run_mode

print_stage "public api gate mode=$MODE profile=$PROFILE completed"
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
    fail "public-api gate did not pass for profile=$PROFILE"
  fi
fi

exit 0
