#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

ARTIFACT_PATH=""
TAG=""
REPO_SLUG="hjd92215202/Rivulet-Gateway"
FORMAT="tar.gz"
HOST_HEADER="localhost"
GATEWAY_PORT="18080"
BACKEND_PORT="19000"
LABEL="systemd-validate"
WORK_DIR=""
UNIT_NAME="rivulet-gateway-validate.service"

DOWNLOAD_DIR=""
EXTRACT_DIR=""
LOG_DIR=""
SUMMARY_PATH=""
PACKAGE_ROOT=""
PACKAGE_BIN=""
PACKAGE_UNIT=""
DEPLOY_ROOT=""
DEPLOY_BIN=""
DEPLOY_CONFIG_DIR=""
DEPLOY_CONFIG=""
DEPLOY_UNIT_PATH=""
BACKEND_PID=""
HEALTHY_STATUS=""
WRONG_HOST_STATUS=""
RESTART_STATUS=""
BACKEND_DOWN_STATUS=""
BACKEND_STOP_RESULT="not-attempted"
BACKEND_STOP_SECONDS="0"
SYSTEMD_START_RESULT="not-attempted"
SYSTEMD_RESTART_RESULT="not-attempted"
SYSTEMD_STOP_RESULT="not-attempted"
SYSTEMCTL_TIMEOUT_SECS="45"

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-systemd-validate.sh [input options] [runtime options]

input options:
  --artifact <path>         local package artifact path (.tar.gz or .rpm)
  --tag <tag>               github release tag to download when --artifact is omitted
  --repo <owner/name>       github repository slug, default: hjd92215202/Rivulet-Gateway
  --format <tar.gz|rpm>     release asset format when downloading, default: tar.gz

runtime options:
  --host <host>             host header matched by the generated route, default: localhost
  --gateway-port <port>     gateway listen port, default: 18080
  --backend-port <port>     fixture backend port, default: 19000
  --label <label>           work label, default: systemd-validate
  --work-dir <dir>          working directory, default: ./target/linux-systemd-validate/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - validates systemd start, restart, stop, healthy-path, wrong-host, and backend-down behavior
  - exits automatically after service teardown and report generation

examples:
  bash ./scripts/linux-systemd-validate.sh --artifact ./dist/linux-x86_64.tar.gz --host llmtamer.com:8080
  bash ./scripts/linux-systemd-validate.sh --tag v0.1.6 --format rpm --host llmtamer.com:8080
EOF
}

fail() {
  echo "linux systemd validation failed: $1" >&2
  exit 1
}

resolve_default_tag() {
  local exact_tag=""

  if command -v git >/dev/null 2>&1; then
    exact_tag="$(git -C "$REPO_ROOT" describe --tags --exact-match 2>/dev/null || true)"
    if [[ -n "$exact_tag" ]]; then
      printf '%s\n' "$exact_tag"
      return 0
    fi
  fi

  printf '%s\n' "v0.1.6"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact)
      ARTIFACT_PATH="$2"
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

case "$FORMAT" in
  tar.gz|rpm)
    ;;
  *)
    echo "unsupported format: $FORMAT" >&2
    exit 1
    ;;
esac

if [[ -z "$ARTIFACT_PATH" && -z "$TAG" ]]; then
  TAG="$(resolve_default_tag)"
fi

ensure_linux_commands curl tar sha256sum python3 systemctl journalctl timeout
if [[ "$FORMAT" == "rpm" ]]; then
  ensure_linux_commands rpm2cpio cpio
fi

ASSET_ARCH_SUFFIX="$(detect_linux_asset_arch)"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-systemd-validate/$TIMESTAMP-$LABEL"
fi

DOWNLOAD_DIR="$WORK_DIR/downloads"
EXTRACT_DIR="$WORK_DIR/extracted"
LOG_DIR="$WORK_DIR/logs"
SUMMARY_PATH="$WORK_DIR/summary.md"

mkdir -p "$DOWNLOAD_DIR" "$EXTRACT_DIR" "$LOG_DIR"

run_privileged_with_timeout() {
  local timeout_secs="$1"
  shift

  if [[ "$(id -u)" -eq 0 ]]; then
    timeout "${timeout_secs}s" "$@"
  elif command -v sudo >/dev/null 2>&1; then
    timeout "${timeout_secs}s" sudo "$@"
  else
    echo "need root or sudo to run privileged command: $*" >&2
    exit 1
  fi
}

run_systemctl_command() {
  local action="$1"
  local allow_fail="${2:-false}"
  shift 2

  if run_privileged_with_timeout "$SYSTEMCTL_TIMEOUT_SECS" systemctl "$action" "$@"; then
    return 0
  fi

  if [[ "$allow_fail" == "true" ]]; then
    return 0
  fi
  fail "systemctl $action exceeded timeout ${SYSTEMCTL_TIMEOUT_SECS}s"
}

stop_process_bounded() {
  local pid="$1"
  local term_wait_secs="${2:-8}"
  local kill_wait_secs="${3:-4}"
  local allow_fail="${4:-false}"
  local started_at=""
  local elapsed=""
  local loops=""

  if [[ -z "$pid" ]]; then
    printf '%s|%s\n' "not-running" "0"
    return 0
  fi

  started_at="$(date +%s)"
  if ! kill -0 "$pid" >/dev/null 2>&1; then
    wait "$pid" >/dev/null 2>&1 || true
    printf '%s|%s\n' "already-exited" "0"
    return 0
  fi

  kill "$pid" >/dev/null 2>&1 || true
  loops=$((term_wait_secs * 4))
  for _ in $(seq 1 "$loops"); do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      wait "$pid" >/dev/null 2>&1 || true
      elapsed=$(( $(date +%s) - started_at ))
      printf '%s|%s\n' "terminated" "$elapsed"
      return 0
    fi
    sleep 0.25
  done

  kill -KILL "$pid" >/dev/null 2>&1 || true
  loops=$((kill_wait_secs * 4))
  for _ in $(seq 1 "$loops"); do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      wait "$pid" >/dev/null 2>&1 || true
      elapsed=$(( $(date +%s) - started_at ))
      printf '%s|%s\n' "killed-after-timeout" "$elapsed"
      return 0
    fi
    sleep 0.25
  done

  elapsed=$(( $(date +%s) - started_at ))
  if [[ "$allow_fail" == "true" ]]; then
    printf '%s|%s\n' "failed" "$elapsed"
    return 0
  fi
  printf '%s|%s\n' "failed" "$elapsed"
  return 1
}

shutdown_backend_process() {
  local allow_fail="${1:-false}"
  local info=""
  info="$(stop_process_bounded "$BACKEND_PID" 8 4 "$allow_fail")" || return 1
  IFS='|' read -r BACKEND_STOP_RESULT BACKEND_STOP_SECONDS <<<"$info"
  BACKEND_PID=""
  return 0
}

cleanup() {
  shutdown_backend_process "true" >/dev/null 2>&1 || true

  if [[ -n "$DEPLOY_UNIT_PATH" ]]; then
    run_systemctl_command stop "true" "$UNIT_NAME" >/dev/null 2>&1 || true
    run_systemctl_command disable "true" "$UNIT_NAME" >/dev/null 2>&1 || true
  fi

  if [[ -n "$UNIT_NAME" ]] && command -v journalctl >/dev/null 2>&1; then
    run_privileged journalctl -u "$UNIT_NAME" --no-pager >"$LOG_DIR/journal.log" 2>&1 || true
  fi

  if [[ -n "$DEPLOY_UNIT_PATH" ]]; then
    run_privileged rm -f "$DEPLOY_UNIT_PATH" || true
    run_systemctl_command daemon-reload "true" >/dev/null 2>&1 || true
  fi

  if [[ -n "$DEPLOY_ROOT" ]]; then
    run_privileged rm -rf "$DEPLOY_ROOT" || true
  fi
  if [[ -n "$DEPLOY_CONFIG_DIR" ]]; then
    run_privileged rm -rf "$DEPLOY_CONFIG_DIR" || true
  fi
}
trap cleanup EXIT

standardize_artifact_path() {
  local artifact_path="$1"
  printf '%s/%s\n' "$(cd "$(dirname "$artifact_path")" && pwd)" "$(basename "$artifact_path")"
}

choose_asset_url() {
  local release_json_path="$1"
  local suffix="$2"

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

resolve_artifact_path() {
  if [[ -n "$ARTIFACT_PATH" ]]; then
    if [[ ! -f "$ARTIFACT_PATH" ]]; then
      echo "artifact not found: $ARTIFACT_PATH" >&2
      exit 1
    fi
    ARTIFACT_PATH="$(standardize_artifact_path "$ARTIFACT_PATH")"
    printf '%s\n' "$ARTIFACT_PATH"
    return 0
  fi

  local release_json_path="$DOWNLOAD_DIR/release.json"
  local asset_suffix=""
  local artifact_url=""
  local artifact_name=""
  local sha_url=""

  case "$FORMAT" in
    tar.gz)
      asset_suffix="-${ASSET_ARCH_SUFFIX}.tar.gz"
      ;;
    rpm)
      asset_suffix="-${ASSET_ARCH_SUFFIX}.rpm"
      ;;
  esac

  print_stage "resolving release metadata for tag=$TAG repo=$REPO_SLUG"
  curl -fsSL "https://api.github.com/repos/$REPO_SLUG/releases/tags/$TAG" -o "$release_json_path"

  artifact_url="$(choose_asset_url "$release_json_path" "$asset_suffix")"
  sha_url="$(choose_asset_url "$release_json_path" "SHA256SUMS.txt")"
  artifact_name="$(basename "$artifact_url")"

  print_stage "downloading artifact=$artifact_name"
  curl -fL "$artifact_url" -o "$DOWNLOAD_DIR/$artifact_name"
  print_stage "downloading checksum manifest"
  curl -fL "$sha_url" -o "$DOWNLOAD_DIR/SHA256SUMS.txt"

  print_stage "verifying release checksum"
  (
    cd "$DOWNLOAD_DIR"
    grep " ${artifact_name}$" SHA256SUMS.txt | sha256sum -c -
  )

  printf '%s\n' "$DOWNLOAD_DIR/$artifact_name"
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

assert_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected file not found: $path" >&2
    exit 1
  fi
}

assert_systemd_available() {
  if ! run_privileged_with_timeout "$SYSTEMCTL_TIMEOUT_SECS" systemctl list-unit-files >/dev/null 2>&1; then
    echo "systemctl is present but cannot talk to a running systemd manager" >&2
    exit 1
  fi
}

render_config() {
  local config_path="$1"
  cat >"$config_path" <<EOF
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
}

prepare_validation_layout() {
  local generated_config="$WORK_DIR/gateway.toml"
  local generated_unit="$WORK_DIR/$UNIT_NAME"

  PACKAGE_BIN="$PACKAGE_ROOT/usr/bin/gateway"
  PACKAGE_UNIT="$PACKAGE_ROOT/usr/lib/systemd/system/rivulet-gateway.service"
  assert_file "$PACKAGE_BIN"
  assert_file "$PACKAGE_UNIT"

  DEPLOY_ROOT="/opt/rivulet-gateway-validate/$TIMESTAMP-$LABEL"
  DEPLOY_BIN="$DEPLOY_ROOT/bin/gateway"
  DEPLOY_CONFIG_DIR="/etc/rivulet-gateway-validate/$TIMESTAMP-$LABEL"
  DEPLOY_CONFIG="$DEPLOY_CONFIG_DIR/gateway.toml"
  DEPLOY_UNIT_PATH="/etc/systemd/system/$UNIT_NAME"

  print_stage "rendering validation config"
  render_config "$generated_config"

  print_stage "deploying packaged binary and config into validation paths"
  run_privileged install -d -m 0755 "$DEPLOY_ROOT/bin" "$DEPLOY_CONFIG_DIR"
  run_privileged install -m 0755 "$PACKAGE_BIN" "$DEPLOY_BIN"
  run_privileged install -m 0644 "$generated_config" "$DEPLOY_CONFIG"

  print_stage "materializing validation systemd unit"
  sed \
    -e 's#^Description=Rivulet Gateway$#Description=Rivulet Gateway Validation#' \
    -e "s#^ExecStart=.*#ExecStart=$DEPLOY_BIN $DEPLOY_CONFIG#" \
    "$PACKAGE_UNIT" >"$generated_unit"
  run_privileged install -m 0644 "$generated_unit" "$DEPLOY_UNIT_PATH"
  run_systemctl_command daemon-reload "false"
}

wait_for_status() {
  local expected="$1"
  local host_header="$2"
  local path="$3"
  local attempts="${4:-30}"
  local delay_secs="${5:-0.25}"
  local max_time_secs="${6:-3}"
  local url="http://127.0.0.1:${GATEWAY_PORT}${path}"
  local code=""

  for _ in $(seq 1 "$attempts"); do
    code="$(curl --max-time "$max_time_secs" -sS -o /dev/null -w "%{http_code}" -H "Host: $host_header" "$url" || true)"
    if [[ "$code" == "$expected" ]]; then
      printf '%s\n' "$code"
      return 0
    fi
    sleep "$delay_secs"
  done

  printf '%s\n' "$code"
  return 1
}

ARTIFACT_PATH="$(resolve_artifact_path)"
ARTIFACT_PATH="$(standardize_artifact_path "$ARTIFACT_PATH")"

print_stage "checking systemd availability"
assert_systemd_available

print_stage "extracting package payload"
case "$ARTIFACT_PATH" in
  *.tar.gz)
    PACKAGE_ROOT="$(extract_tarball_root "$ARTIFACT_PATH")"
    ;;
  *.rpm)
    PACKAGE_ROOT="$(extract_rpm_root "$ARTIFACT_PATH")"
    ;;
  *)
    echo "unsupported artifact type: $ARTIFACT_PATH" >&2
    exit 1
    ;;
esac

if [[ -z "$PACKAGE_ROOT" || ! -d "$PACKAGE_ROOT" ]]; then
  echo "failed to determine extracted package root" >&2
  exit 1
fi

prepare_validation_layout

print_stage "starting bundled fixture backend"
python3 "$REPO_ROOT/scripts/fixture-backend.py" \
  --bind 127.0.0.1 \
  --port "$BACKEND_PORT" \
  --default-bytes 512 \
  >"$LOG_DIR/backend.log" 2>&1 &
BACKEND_PID="$!"

print_stage "starting validation systemd unit"
run_systemctl_command start "false" "$UNIT_NAME"
SYSTEMD_START_RESULT="ok"

print_stage "checking healthy route returns 200"
HEALTHY_STATUS="$(wait_for_status "200" "$HOST_HEADER" "/fixture/default")"

print_stage "checking wrong host returns 404"
WRONG_HOST_STATUS="$(wait_for_status "404" "invalid.example.test" "/fixture/default" 8 0.25 2 || true)"

print_stage "restarting validation systemd unit"
run_systemctl_command restart "false" "$UNIT_NAME"
SYSTEMD_RESTART_RESULT="ok"

print_stage "checking route still returns 200 after restart"
RESTART_STATUS="$(wait_for_status "200" "$HOST_HEADER" "/fixture/default")"

print_stage "stopping bundled backend to verify degraded 502 path"
shutdown_backend_process "false" >/dev/null || fail "fixture backend did not stop within bounded timeout"
BACKEND_DOWN_STATUS="$(wait_for_status "502" "$HOST_HEADER" "/fixture/default" 5 0.3 8 || true)"

print_stage "stopping validation systemd unit"
run_systemctl_command stop "false" "$UNIT_NAME"
SYSTEMD_STOP_RESULT="ok"

print_stage "writing summary report"
cat >"$SUMMARY_PATH" <<EOF
# Linux Systemd Validation Summary

## Inputs

- Artifact: \`$ARTIFACT_PATH\`
- Format: \`$FORMAT\`
- Host header: \`$HOST_HEADER\`
- Gateway port: \`$GATEWAY_PORT\`
- Backend port: \`$BACKEND_PORT\`
- Architecture suffix: \`$ASSET_ARCH_SUFFIX\`
- Working directory: \`$WORK_DIR\`
- Unit name: \`$UNIT_NAME\`

## Validation Paths

- Extracted package root: \`$PACKAGE_ROOT\`
- Deployed binary: \`$DEPLOY_BIN\`
- Deployed config: \`$DEPLOY_CONFIG\`
- Deployed unit: \`$DEPLOY_UNIT_PATH\`

## Checks

- Healthy route status: \`$HEALTHY_STATUS\`
- Wrong-host status: \`$WRONG_HOST_STATUS\`
- Restart healthy status: \`$RESTART_STATUS\`
- Backend-down status: \`$BACKEND_DOWN_STATUS\`

## Systemd Control

- systemctl timeout budget: \`${SYSTEMCTL_TIMEOUT_SECS}s\`
- systemd start result: \`$SYSTEMD_START_RESULT\`
- systemd restart result: \`$SYSTEMD_RESTART_RESULT\`
- systemd stop result: \`$SYSTEMD_STOP_RESULT\`

## Process Shutdown

- fixture backend stop result: \`$BACKEND_STOP_RESULT\`
- fixture backend stop seconds: \`$BACKEND_STOP_SECONDS\`

## Logs

- Backend log: \`$LOG_DIR/backend.log\`
- Journal log: \`$LOG_DIR/journal.log\`
EOF

print_stage "systemd validation summary written to $SUMMARY_PATH"
