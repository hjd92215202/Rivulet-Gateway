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
VALIDATE_PATH="/fixture/default"
CLEANUP_MODE="remove"
LABEL="postinstall-validate"
WORK_DIR=""

INSTALL_WORK_DIR=""
REMOVE_WORK_DIR=""
LOG_DIR=""
SUMMARY_PATH=""
CONFIG_PATH=""
BACKEND_PID=""
REMOVE_SUMMARY_PATH=""
INSTALL_SUMMARY_PATH=""
HEALTHY_STATUS=""
WRONG_HOST_STATUS=""
RESTART_STATUS=""
BACKEND_DOWN_STATUS=""
INSTALL_COMPLETED="false"

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-postinstall-validate.sh [input options] [runtime options]

input options:
  --artifact <path>         local package artifact path (.tar.gz or .rpm)
  --tag <tag>               github release tag to download when --artifact is omitted
  --repo <owner/name>       github repository slug, default: hjd92215202/Rivulet-Gateway
  --format <tar.gz|rpm>     release asset format when downloading, default: tar.gz

runtime options:
  --host <host>             host header matched by the generated route, default: localhost
  --gateway-port <port>     gateway listen port, default: 18080
  --backend-port <port>     fixture backend port, default: 19000
  --path <path>             validation path prefix, default: /fixture/default
  --cleanup <remove|keep>   remove the installed service after validation, default: remove
  --label <label>           work label, default: postinstall-validate
  --work-dir <dir>          working directory, default: ./target/linux-postinstall-validate/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - validates real installed service startup, 200 / 404 / restart -> 200 / backend-down -> 502
  - exits automatically after report generation and optional cleanup
  - this script is intentionally conservative and expects a dedicated validation host without an existing rivulet-gateway install

examples:
  bash ./scripts/linux-postinstall-validate.sh --tag v0.1.6 --host llmtamer.com:8080
  bash ./scripts/linux-postinstall-validate.sh --artifact ./dist/rivulet-gateway.rpm --format rpm --cleanup keep
EOF
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
    --path)
      VALIDATE_PATH="$2"
      shift 2
      ;;
    --cleanup)
      CLEANUP_MODE="$2"
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

case "$CLEANUP_MODE" in
  remove|keep)
    ;;
  *)
    echo "unsupported cleanup mode: $CLEANUP_MODE" >&2
    exit 1
    ;;
esac

if [[ -z "$ARTIFACT_PATH" && -z "$TAG" ]]; then
  TAG="$(resolve_default_tag)"
fi

ensure_linux_commands bash curl python3 systemctl journalctl

TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-postinstall-validate/$TIMESTAMP-$LABEL"
fi

INSTALL_WORK_DIR="$WORK_DIR/install"
REMOVE_WORK_DIR="$WORK_DIR/remove"
LOG_DIR="$WORK_DIR/logs"
SUMMARY_PATH="$WORK_DIR/summary.md"
CONFIG_PATH="$WORK_DIR/gateway.toml"

mkdir -p "$INSTALL_WORK_DIR" "$REMOVE_WORK_DIR" "$LOG_DIR"

assert_no_existing_installation() {
  if run_privileged systemctl cat rivulet-gateway.service >/dev/null 2>&1; then
    echo "existing rivulet-gateway service detected; use linux-upgrade-rollback-validate.sh for hosts that already carry an install" >&2
    exit 1
  fi

  if run_privileged test -e /usr/bin/gateway || run_privileged test -e /etc/gateway/gateway.toml; then
    echo "existing rivulet-gateway files detected; postinstall validation requires a clean validation host" >&2
    exit 1
  fi
}

assert_systemd_available() {
  if ! run_privileged systemctl list-unit-files >/dev/null 2>&1; then
    echo "systemctl is present but cannot talk to a running systemd manager" >&2
    exit 1
  fi
}

render_validation_config() {
  cat >"$CONFIG_PATH" <<EOF
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

cleanup() {
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
  fi

  if command -v journalctl >/dev/null 2>&1; then
    run_privileged journalctl -u rivulet-gateway.service --no-pager >"$LOG_DIR/journal.log" 2>&1 || true
  fi

  if [[ "$INSTALL_COMPLETED" == "true" && "$CLEANUP_MODE" == "remove" ]]; then
    print_stage "removing installed validation service"
    bash "$REPO_ROOT/scripts/linux-service-remove.sh" \
      --mode auto \
      --purge-config \
      --label "$LABEL" \
      --work-dir "$REMOVE_WORK_DIR" \
      >/dev/null
  fi
}
trap cleanup EXIT

print_stage "checking clean-host requirement"
assert_no_existing_installation
print_stage "checking systemd availability"
assert_systemd_available

print_stage "installing packaged gateway onto the validation host"
install_args=(--format "$FORMAT" --label "$LABEL" --work-dir "$INSTALL_WORK_DIR" --replace-config --skip-start)
if [[ -n "$ARTIFACT_PATH" ]]; then
  install_args+=(--artifact "$ARTIFACT_PATH")
else
  install_args+=(--tag "$TAG" --repo "$REPO_SLUG")
fi
bash "$REPO_ROOT/scripts/linux-service-install.sh" "${install_args[@]}"
INSTALL_COMPLETED="true"
INSTALL_SUMMARY_PATH="$INSTALL_WORK_DIR/summary.md"

print_stage "rendering validation config into /etc/gateway/gateway.toml"
render_validation_config
run_privileged install -d -m 0755 /etc/gateway
run_privileged install -m 0644 "$CONFIG_PATH" /etc/gateway/gateway.toml

print_stage "starting bundled fixture backend"
python3 "$REPO_ROOT/scripts/fixture-backend.py" \
  --bind 127.0.0.1 \
  --port "$BACKEND_PORT" \
  --default-bytes 1024 \
  >"$LOG_DIR/backend.log" 2>&1 &
BACKEND_PID="$!"

print_stage "starting installed rivulet-gateway service"
run_privileged systemctl daemon-reload
run_privileged systemctl restart rivulet-gateway.service
run_privileged systemctl --no-pager --full status rivulet-gateway.service >"$LOG_DIR/systemctl-status.txt"

print_stage "checking healthy route returns 200"
HEALTHY_STATUS="$(wait_for_status "200" "$HOST_HEADER" "$VALIDATE_PATH")"
print_stage "checking wrong host returns 404"
WRONG_HOST_STATUS="$(wait_for_status "404" "invalid.example.test" "$VALIDATE_PATH" 8 0.25 2 || true)"

print_stage "restarting installed service"
run_privileged systemctl restart rivulet-gateway.service
RESTART_STATUS="$(wait_for_status "200" "$HOST_HEADER" "$VALIDATE_PATH")"

print_stage "stopping bundled backend to verify degraded 502 path"
kill "$BACKEND_PID" >/dev/null 2>&1 || true
wait "$BACKEND_PID" >/dev/null 2>&1 || true
BACKEND_PID=""
BACKEND_DOWN_STATUS="$(wait_for_status "502" "$HOST_HEADER" "$VALIDATE_PATH" 5 0.3 8 || true)"

if [[ "$CLEANUP_MODE" == "remove" ]]; then
  REMOVE_SUMMARY_PATH="$REMOVE_WORK_DIR/summary.md"
else
  REMOVE_SUMMARY_PATH="cleanup disabled"
fi

print_stage "writing summary report"
cat >"$SUMMARY_PATH" <<EOF
# Linux Post-Install Validation Summary

## Inputs

- Artifact: \`${ARTIFACT_PATH:-download-via-tag}\`
- Tag: \`${TAG:-local-artifact}\`
- Repository: \`$REPO_SLUG\`
- Format: \`$FORMAT\`
- Host header: \`$HOST_HEADER\`
- Gateway port: \`$GATEWAY_PORT\`
- Backend port: \`$BACKEND_PORT\`
- Validation path: \`$VALIDATE_PATH\`
- Cleanup mode: \`$CLEANUP_MODE\`
- Working directory: \`$WORK_DIR\`

## Checks

- Healthy route status: \`$HEALTHY_STATUS\`
- Wrong-host status: \`$WRONG_HOST_STATUS\`
- Restart healthy status: \`$RESTART_STATUS\`
- Backend-down status: \`$BACKEND_DOWN_STATUS\`

## Outputs

- Install summary: \`$INSTALL_SUMMARY_PATH\`
- Remove summary: \`$REMOVE_SUMMARY_PATH\`
- Validation config: \`$CONFIG_PATH\`
- Backend log: \`$LOG_DIR/backend.log\`
- Systemctl status: \`$LOG_DIR/systemctl-status.txt\`
- Journal log: \`$LOG_DIR/journal.log\`

## Exit Criteria

- healthy route should be \`200\`
- wrong host should be \`404\`
- post-restart route should still be \`200\`
- backend-down should be \`502\`
EOF

echo "summary: $SUMMARY_PATH"
print_stage "linux postinstall validation completed"
