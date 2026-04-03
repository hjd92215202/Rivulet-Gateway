#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

FROM_ARTIFACT=""
FROM_TAG=""
TO_ARTIFACT=""
TO_TAG=""
REPO_SLUG="hjd92215202/Rivulet-Gateway"
FORMAT="tar.gz"
HOST_HEADER="localhost"
GATEWAY_PORT="18080"
BACKEND_PORT="19000"
LABEL="upgrade-rollback"
WORK_DIR=""
UNIT_NAME="rivulet-gateway-upgrade-validate.service"

DOWNLOAD_DIR=""
EXTRACT_DIR=""
LOG_DIR=""
SUMMARY_PATH=""
DEPLOY_ROOT=""
DEPLOY_BIN=""
DEPLOY_CONFIG_DIR=""
DEPLOY_CONFIG=""
DEPLOY_UNIT_PATH=""
BACKEND_PID=""
INITIAL_STATUS=""
UPGRADE_STATUS=""
ROLLBACK_STATUS=""
WRONG_HOST_STATUS=""
BACKEND_DOWN_STATUS=""
FROM_RESOLVED_ARTIFACT=""
TO_RESOLVED_ARTIFACT=""
FROM_PACKAGE_ROOT=""
TO_PACKAGE_ROOT=""

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-upgrade-rollback-validate.sh [source options] [runtime options]

source options:
  --from-artifact <path>    local starting artifact path (.tar.gz or .rpm)
  --from-tag <tag>          github release tag for the starting version
  --to-artifact <path>      local target artifact path (.tar.gz or .rpm)
  --to-tag <tag>            github release tag for the target version
  --repo <owner/name>       github repository slug, default: hjd92215202/Rivulet-Gateway
  --format <tar.gz|rpm>     release asset format when downloading, default: tar.gz

runtime options:
  --host <host>             host header matched by the generated route, default: localhost
  --gateway-port <port>     gateway listen port, default: 18080
  --backend-port <port>     fixture backend port, default: 19000
  --label <label>           work label, default: upgrade-rollback
  --work-dir <dir>          working directory, default: ./target/linux-upgrade-rollback/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - validates initial start, upgrade restart, rollback restart, wrong-host, and backend-down behavior
  - exits automatically after service teardown and report generation

examples:
  bash ./scripts/linux-upgrade-rollback-validate.sh --from-tag v0.1.5 --to-tag v0.1.6 --host llmtamer.com:8080
  bash ./scripts/linux-upgrade-rollback-validate.sh --from-artifact ./old.tar.gz --to-artifact ./new.tar.gz --host llmtamer.com:8080
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-artifact)
      FROM_ARTIFACT="$2"
      shift 2
      ;;
    --from-tag)
      FROM_TAG="$2"
      shift 2
      ;;
    --to-artifact)
      TO_ARTIFACT="$2"
      shift 2
      ;;
    --to-tag)
      TO_TAG="$2"
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

if [[ -z "$FROM_ARTIFACT" && -z "$FROM_TAG" ]]; then
  echo "either --from-artifact or --from-tag is required" >&2
  exit 1
fi

if [[ -z "$TO_ARTIFACT" && -z "$TO_TAG" ]]; then
  echo "either --to-artifact or --to-tag is required" >&2
  exit 1
fi

ensure_linux_commands curl tar sha256sum python3 systemctl journalctl
if [[ "$FORMAT" == "rpm" ]]; then
  ensure_linux_commands rpm2cpio cpio
fi

ASSET_ARCH_SUFFIX="$(detect_linux_asset_arch)"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-upgrade-rollback/$TIMESTAMP-$LABEL"
fi

DOWNLOAD_DIR="$WORK_DIR/downloads"
EXTRACT_DIR="$WORK_DIR/extracted"
LOG_DIR="$WORK_DIR/logs"
SUMMARY_PATH="$WORK_DIR/summary.md"

mkdir -p "$DOWNLOAD_DIR" "$EXTRACT_DIR" "$LOG_DIR"

cleanup() {
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
  fi

  if [[ -n "$DEPLOY_UNIT_PATH" ]]; then
    run_privileged systemctl stop "$UNIT_NAME" >/dev/null 2>&1 || true
    run_privileged systemctl disable "$UNIT_NAME" >/dev/null 2>&1 || true
  fi

  if [[ -n "$UNIT_NAME" ]] && command -v journalctl >/dev/null 2>&1; then
    run_privileged journalctl -u "$UNIT_NAME" --no-pager >"$LOG_DIR/journal.log" 2>&1 || true
  fi

  if [[ -n "$DEPLOY_UNIT_PATH" ]]; then
    run_privileged rm -f "$DEPLOY_UNIT_PATH" || true
    run_privileged systemctl daemon-reload || true
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

resolve_downloaded_artifact() {
  local role="$1"
  local tag="$2"
  local download_dir="$3"
  local release_json_path="$download_dir/release.json"
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

  mkdir -p "$download_dir"

  print_stage "resolving ${role} release metadata for tag=$tag repo=$REPO_SLUG"
  curl -fsSL "https://api.github.com/repos/$REPO_SLUG/releases/tags/$tag" -o "$release_json_path"

  artifact_url="$(choose_asset_url "$release_json_path" "$asset_suffix")"
  sha_url="$(choose_asset_url "$release_json_path" "SHA256SUMS.txt")"
  artifact_name="$(basename "$artifact_url")"

  print_stage "downloading ${role} artifact=$artifact_name"
  curl -fL "$artifact_url" -o "$download_dir/$artifact_name"
  print_stage "downloading ${role} checksum manifest"
  curl -fL "$sha_url" -o "$download_dir/SHA256SUMS.txt"

  print_stage "verifying ${role} release checksum"
  (
    cd "$download_dir"
    grep " ${artifact_name}$" SHA256SUMS.txt | sha256sum -c -
  )

  printf '%s\n' "$download_dir/$artifact_name"
}

resolve_artifact_input() {
  local role="$1"
  local artifact_path="$2"
  local tag="$3"
  local download_dir="$4"

  if [[ -n "$artifact_path" ]]; then
    if [[ ! -f "$artifact_path" ]]; then
      echo "${role} artifact not found: $artifact_path" >&2
      exit 1
    fi
    standardize_artifact_path "$artifact_path"
    return 0
  fi

  resolve_downloaded_artifact "$role" "$tag" "$download_dir"
}

extract_tarball_root() {
  local artifact_path="$1"
  local destination_dir="$2"
  mkdir -p "$destination_dir"
  tar -xzf "$artifact_path" -C "$destination_dir"
  find "$destination_dir" -mindepth 1 -maxdepth 1 -type d | head -n 1
}

extract_rpm_root() {
  local artifact_path="$1"
  local destination_dir="$2"
  mkdir -p "$destination_dir"
  (
    cd "$destination_dir"
    rpm2cpio "$artifact_path" | cpio -idmu --quiet
  )
  printf '%s\n' "$destination_dir"
}

extract_package_root() {
  local artifact_path="$1"
  local destination_dir="$2"

  case "$artifact_path" in
    *.tar.gz)
      extract_tarball_root "$artifact_path" "$destination_dir"
      ;;
    *.rpm)
      extract_rpm_root "$artifact_path" "$destination_dir"
      ;;
    *)
      echo "unsupported artifact type: $artifact_path" >&2
      exit 1
      ;;
  esac
}

assert_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected file not found: $path" >&2
    exit 1
  fi
}

assert_systemd_available() {
  if ! run_privileged systemctl list-unit-files >/dev/null 2>&1; then
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

deploy_package_phase() {
  local phase_name="$1"
  local package_root="$2"
  local package_bin="$package_root/usr/bin/gateway"
  local package_unit="$package_root/usr/lib/systemd/system/rivulet-gateway.service"
  local generated_unit="$WORK_DIR/${phase_name}-${UNIT_NAME}"

  assert_file "$package_bin"
  assert_file "$package_unit"

  print_stage "deploying ${phase_name} packaged binary"
  run_privileged install -m 0755 "$package_bin" "$DEPLOY_BIN"

  print_stage "rendering ${phase_name} validation systemd unit"
  sed \
    -e 's#^Description=Rivulet Gateway$#Description=Rivulet Gateway Upgrade Validation#' \
    -e "s#^ExecStart=.*#ExecStart=$DEPLOY_BIN $DEPLOY_CONFIG#" \
    "$package_unit" >"$generated_unit"
  run_privileged install -m 0644 "$generated_unit" "$DEPLOY_UNIT_PATH"
  run_privileged systemctl daemon-reload
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

print_stage "checking systemd availability"
assert_systemd_available

print_stage "resolving source artifacts"
FROM_RESOLVED_ARTIFACT="$(resolve_artifact_input "from" "$FROM_ARTIFACT" "$FROM_TAG" "$DOWNLOAD_DIR/from")"
TO_RESOLVED_ARTIFACT="$(resolve_artifact_input "to" "$TO_ARTIFACT" "$TO_TAG" "$DOWNLOAD_DIR/to")"

print_stage "extracting source artifacts"
FROM_PACKAGE_ROOT="$(extract_package_root "$FROM_RESOLVED_ARTIFACT" "$EXTRACT_DIR/from")"
TO_PACKAGE_ROOT="$(extract_package_root "$TO_RESOLVED_ARTIFACT" "$EXTRACT_DIR/to")"

DEPLOY_ROOT="/opt/rivulet-gateway-upgrade-validate/$TIMESTAMP-$LABEL"
DEPLOY_BIN="$DEPLOY_ROOT/bin/gateway"
DEPLOY_CONFIG_DIR="/etc/rivulet-gateway-upgrade-validate/$TIMESTAMP-$LABEL"
DEPLOY_CONFIG="$DEPLOY_CONFIG_DIR/gateway.toml"
DEPLOY_UNIT_PATH="/etc/systemd/system/$UNIT_NAME"

print_stage "preparing validation directories and config"
render_config "$WORK_DIR/gateway.toml"
run_privileged install -d -m 0755 "$DEPLOY_ROOT/bin" "$DEPLOY_CONFIG_DIR"
run_privileged install -m 0644 "$WORK_DIR/gateway.toml" "$DEPLOY_CONFIG"

print_stage "starting bundled fixture backend"
python3 "$REPO_ROOT/scripts/fixture-backend.py" \
  --bind 127.0.0.1 \
  --port "$BACKEND_PORT" \
  --default-bytes 512 \
  >"$LOG_DIR/backend.log" 2>&1 &
BACKEND_PID="$!"

deploy_package_phase "initial" "$FROM_PACKAGE_ROOT"

print_stage "starting validation service from initial package"
run_privileged systemctl start "$UNIT_NAME"
INITIAL_STATUS="$(wait_for_status "200" "$HOST_HEADER" "/fixture/default")"

print_stage "checking wrong host returns 404 on initial package"
WRONG_HOST_STATUS="$(wait_for_status "404" "invalid.example.test" "/fixture/default" 8 0.25 2 || true)"

deploy_package_phase "upgrade" "$TO_PACKAGE_ROOT"

print_stage "restarting validation service on upgraded package"
run_privileged systemctl restart "$UNIT_NAME"
UPGRADE_STATUS="$(wait_for_status "200" "$HOST_HEADER" "/fixture/default")"

deploy_package_phase "rollback" "$FROM_PACKAGE_ROOT"

print_stage "restarting validation service on rolled-back package"
run_privileged systemctl restart "$UNIT_NAME"
ROLLBACK_STATUS="$(wait_for_status "200" "$HOST_HEADER" "/fixture/default")"

print_stage "stopping bundled backend to verify degraded 502 path after rollback"
kill "$BACKEND_PID" >/dev/null 2>&1 || true
wait "$BACKEND_PID" >/dev/null 2>&1 || true
BACKEND_PID=""
BACKEND_DOWN_STATUS="$(wait_for_status "502" "$HOST_HEADER" "/fixture/default" 5 0.3 8 || true)"

print_stage "stopping validation service"
run_privileged systemctl stop "$UNIT_NAME"

print_stage "writing summary report"
cat >"$SUMMARY_PATH" <<EOF
# Linux Upgrade Rollback Validation Summary

## Inputs

- From artifact: \`$FROM_RESOLVED_ARTIFACT\`
- To artifact: \`$TO_RESOLVED_ARTIFACT\`
- Format: \`$FORMAT\`
- Host header: \`$HOST_HEADER\`
- Gateway port: \`$GATEWAY_PORT\`
- Backend port: \`$BACKEND_PORT\`
- Architecture suffix: \`$ASSET_ARCH_SUFFIX\`
- Working directory: \`$WORK_DIR\`
- Unit name: \`$UNIT_NAME\`

## Extracted Payloads

- From package root: \`$FROM_PACKAGE_ROOT\`
- To package root: \`$TO_PACKAGE_ROOT\`
- Deployed binary path: \`$DEPLOY_BIN\`
- Deployed config path: \`$DEPLOY_CONFIG\`
- Deployed unit path: \`$DEPLOY_UNIT_PATH\`

## Checks

- Initial package healthy status: \`$INITIAL_STATUS\`
- Wrong-host status: \`$WRONG_HOST_STATUS\`
- Upgraded package healthy status: \`$UPGRADE_STATUS\`
- Rolled-back package healthy status: \`$ROLLBACK_STATUS\`
- Backend-down status after rollback: \`$BACKEND_DOWN_STATUS\`

## Logs

- Backend log: \`$LOG_DIR/backend.log\`
- Journal log: \`$LOG_DIR/journal.log\`
EOF

print_stage "upgrade rollback validation summary written to $SUMMARY_PATH"
