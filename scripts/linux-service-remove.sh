#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

LABEL="service-remove"
WORK_DIR=""
PURGE_CONFIG="false"
MODE="auto"

SUMMARY_PATH=""
INSTALL_BIN="/usr/bin/gateway"
INSTALL_CONFIG_DIR="/etc/gateway"
INSTALL_CONFIG="/etc/gateway/gateway.toml"
INSTALL_UNIT="/usr/lib/systemd/system/rivulet-gateway.service"

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-service-remove.sh [options]

options:
  --mode <auto|manual|rpm>  removal strategy, default: auto
  --purge-config            remove /etc/gateway/gateway.toml and /etc/gateway when empty
  --label <label>           work label, default: service-remove
  --work-dir <dir>          working directory, default: ./target/linux-service-remove/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - stops and disables the service, then removes the installed files or rpm package
  - exits automatically after writing the removal summary

examples:
  bash ./scripts/linux-service-remove.sh
  bash ./scripts/linux-service-remove.sh --mode rpm --purge-config
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="$2"
      shift 2
      ;;
    --purge-config)
      PURGE_CONFIG="true"
      shift
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

case "$MODE" in
  auto|manual|rpm)
    ;;
  *)
    echo "unsupported mode: $MODE" >&2
    exit 1
    ;;
esac

ensure_linux_commands systemctl
if [[ "$MODE" == "rpm" ]]; then
  ensure_linux_commands rpm
fi

TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-service-remove/$TIMESTAMP-$LABEL"
fi
mkdir -p "$WORK_DIR"
SUMMARY_PATH="$WORK_DIR/summary.md"

stop_and_disable_service() {
  if run_privileged systemctl cat rivulet-gateway.service >/dev/null 2>&1; then
    print_stage "stopping rivulet-gateway service"
    run_privileged systemctl stop rivulet-gateway.service || true
    print_stage "disabling rivulet-gateway service"
    run_privileged systemctl disable rivulet-gateway.service || true
  fi
}

remove_manual_install() {
  print_stage "removing manually installed binary and unit files"
  run_privileged rm -f "$INSTALL_BIN" || true
  run_privileged rm -f "$INSTALL_UNIT" || true
  run_privileged systemctl daemon-reload || true

  if [[ "$PURGE_CONFIG" == "true" ]]; then
    print_stage "purging gateway config"
    run_privileged rm -f "$INSTALL_CONFIG" || true
    run_privileged rmdir "$INSTALL_CONFIG_DIR" >/dev/null 2>&1 || true
  else
    print_stage "preserving gateway config at $INSTALL_CONFIG"
  fi
}

remove_rpm_install() {
  if ! run_privileged rpm -q rivulet-gateway >/dev/null 2>&1; then
    echo "rpm package rivulet-gateway is not installed" >&2
    exit 1
  fi

  print_stage "removing rpm package rivulet-gateway"
  run_privileged rpm -e rivulet-gateway

  if [[ "$PURGE_CONFIG" == "true" ]]; then
    print_stage "purging residual config after rpm removal when present"
    run_privileged rm -f "$INSTALL_CONFIG" || true
    run_privileged rmdir "$INSTALL_CONFIG_DIR" >/dev/null 2>&1 || true
  fi
}

write_summary() {
  cat >"$SUMMARY_PATH" <<EOF
# Linux Service Remove Summary

## Inputs

- Mode: \`$MODE\`
- Purge config: \`$PURGE_CONFIG\`
- Working directory: \`$WORK_DIR\`

## Paths

- Binary: \`$INSTALL_BIN\`
- Config: \`$INSTALL_CONFIG\`
- Unit: \`$INSTALL_UNIT\`
EOF
}

stop_and_disable_service

case "$MODE" in
  manual)
    remove_manual_install
    ;;
  rpm)
    remove_rpm_install
    ;;
  auto)
    if command -v rpm >/dev/null 2>&1 && run_privileged rpm -q rivulet-gateway >/dev/null 2>&1; then
      remove_rpm_install
    else
      remove_manual_install
    fi
    ;;
esac

write_summary
print_stage "service removal summary written to $SUMMARY_PATH"
