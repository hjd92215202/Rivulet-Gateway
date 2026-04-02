#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

URL=""
HOST_HEADER=""
DURATION_SECS="15"
OUT_DIR=""
LABEL="baseline"

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-baseline-report.sh --url <url> --host <host> [options]

required:
  --url <url>          benchmark url, for example http://127.0.0.1:8080/ngx/
  --host <host>        host header value, for example llmtamer.com:8080

optional:
  --duration <secs>    duration of each wrk run, default: 15
  --out-dir <dir>      output directory, default: ./target/server-bench
  --label <name>       report label, default: baseline

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs curl and wrk when apt-get, dnf, or yum is available
  - exits automatically after writing the report

example:
  bash ./scripts/linux-baseline-report.sh \
    --url http://127.0.0.1:8080/ngx/ \
    --host llmtamer.com:8080 \
    --duration 15 \
    --label llmtamer-loopback
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url)
      URL="$2"
      shift 2
      ;;
    --host)
      HOST_HEADER="$2"
      shift 2
      ;;
    --duration)
      DURATION_SECS="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --label)
      LABEL="$2"
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

if [[ -z "$URL" || -z "$HOST_HEADER" ]]; then
  usage >&2
  exit 1
fi

ensure_linux_commands curl wrk

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="./target/server-bench"
fi

TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REPORT_ROOT="$OUT_DIR/$TIMESTAMP-$LABEL"
RAW_DIR="$REPORT_ROOT/raw"
REPORT_PATH="$REPORT_ROOT/report.md"

mkdir -p "$RAW_DIR"

print_stage "running linux baseline benchmark label=$LABEL"
print_stage "target url=$URL host=$HOST_HEADER duration=${DURATION_SECS}s"

request_status_line() {
  curl -sS -D - -o /dev/null -H "Host: $HOST_HEADER" "$URL" | head -n 1 | tr -d '\r'
}

request_body_preview() {
  curl -sS -H "Host: $HOST_HEADER" "$URL" | head -c 200 | tr '\n' ' '
}

capture_cmd_output() {
  local output_path="$1"
  shift
  "$@" >"$output_path" 2>&1 || true
}

parse_metric() {
  local file_path="$1"
  local pattern="$2"
  awk -v pattern="$pattern" '$1 == pattern { print $2; exit }' "$file_path"
}

parse_percentile() {
  local file_path="$1"
  local percentile="$2"
  awk -v percentile="$percentile" '
    $1 == percentile { print $2; found=1; exit }
    $1 == "#" && $2 == "[" percentile "]" { print $3; found=1; exit }
    END {
      if (!found) {
        exit 0
      }
    }
  ' "$file_path"
}

run_case() {
  local case_name="$1"
  local threads="$2"
  local connections="$3"
  local duration_secs="$4"
  local raw_path="$RAW_DIR/$case_name.txt"

  print_stage "running case=$case_name threads=$threads connections=$connections duration=${duration_secs}s"
  wrk --latency -t"$threads" -c"$connections" -d"${duration_secs}s" -H "Host: $HOST_HEADER" "$URL" >"$raw_path"

  local req_per_sec
  local transfer_per_sec
  local p50
  local p75
  local p90
  local p99

  req_per_sec="$(parse_metric "$raw_path" "Requests/sec:")"
  transfer_per_sec="$(parse_metric "$raw_path" "Transfer/sec:")"
  p50="$(parse_percentile "$raw_path" "50.000%")"
  p75="$(parse_percentile "$raw_path" "75.000%")"
  p90="$(parse_percentile "$raw_path" "90.000%")"
  p99="$(parse_percentile "$raw_path" "99.000%")"

  printf '| %s | %s | %s | %ss | %s | %s | %s | %s | %s | %s |\n' \
    "$case_name" "$threads" "$connections" "$duration_secs" \
    "${req_per_sec:-n/a}" "${transfer_per_sec:-n/a}" \
    "${p50:-n/a}" "${p75:-n/a}" "${p90:-n/a}" "${p99:-n/a}" >>"$REPORT_PATH"
}

PRECHECK_STATUS_PATH="$RAW_DIR/precheck-status.txt"
PRECHECK_BODY_PATH="$RAW_DIR/precheck-body.txt"
UNAME_PATH="$RAW_DIR/uname.txt"
CPUINFO_PATH="$RAW_DIR/cpuinfo.txt"
MEMINFO_PATH="$RAW_DIR/meminfo.txt"
SS_PATH="$RAW_DIR/socket-summary.txt"
TOP_PATH="$RAW_DIR/top.txt"
SYSTEMD_PATH="$RAW_DIR/systemd-status.txt"

request_status_line >"$PRECHECK_STATUS_PATH"
request_body_preview >"$PRECHECK_BODY_PATH"
capture_cmd_output "$UNAME_PATH" uname -a
capture_cmd_output "$CPUINFO_PATH" sh -c "nproc && echo '---' && lscpu"
capture_cmd_output "$MEMINFO_PATH" sh -c "free -h && echo '---' && grep -E 'MemTotal|MemFree|MemAvailable' /proc/meminfo"
capture_cmd_output "$SS_PATH" ss -s
capture_cmd_output "$TOP_PATH" sh -c "top -b -n 1 | head -n 30"
capture_cmd_output "$SYSTEMD_PATH" sh -c "systemctl is-active rivulet-gateway && systemctl status rivulet-gateway --no-pager -n 20"

cat >"$REPORT_PATH" <<EOF
# Linux Baseline Benchmark Report

## Summary

- Label: \`$LABEL\`
- Timestamp: \`$(date -Is)\`
- URL: \`$URL\`
- Host header: \`$HOST_HEADER\`
- Duration per case: \`${DURATION_SECS}s\`
- Precheck status: \`$(cat "$PRECHECK_STATUS_PATH")\`
- Precheck body preview: \`$(cat "$PRECHECK_BODY_PATH")\`

## Environment

\`\`\`text
$(cat "$UNAME_PATH")
\`\`\`

\`\`\`text
$(cat "$CPUINFO_PATH")
\`\`\`

\`\`\`text
$(cat "$MEMINFO_PATH")
\`\`\`

## Socket Snapshot

\`\`\`text
$(cat "$SS_PATH")
\`\`\`

## Service Snapshot

\`\`\`text
$(cat "$SYSTEMD_PATH")
\`\`\`

## Benchmark Matrix

| Case | Threads | Connections | Duration | Req/sec | Transfer/sec | P50 | P75 | P90 | P99 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
EOF

run_case "low" "2" "2" "$DURATION_SECS"
sleep 2
run_case "medium" "2" "10" "$DURATION_SECS"
sleep 2
run_case "high" "4" "32" "$DURATION_SECS"

cat >>"$REPORT_PATH" <<EOF

## Top Snapshot

\`\`\`text
$(cat "$TOP_PATH")
\`\`\`

## Raw Outputs

- \`$RAW_DIR/low.txt\`
- \`$RAW_DIR/medium.txt\`
- \`$RAW_DIR/high.txt\`
- \`$PRECHECK_STATUS_PATH\`
- \`$PRECHECK_BODY_PATH\`

## Notes

- This script uses \`wrk\`, so it measures throughput and latency but does not automatically classify HTTP status code buckets.
- Please correlate this report with gateway logs and upstream logs when judging whether responses are business-successful.
- If the precheck status is not the expected \`HTTP/1.1 200 OK\`, fix routing or upstream behavior before trusting throughput numbers.
EOF

echo "report: $REPORT_PATH"
print_stage "linux baseline benchmark completed"
