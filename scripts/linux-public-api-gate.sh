#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

usage() {
  cat <<'EOF'
Usage: bash ./scripts/linux-public-api-gate.sh [threshold-file]

Validate the public API reliability/capacity threshold schema.

This script supports Linux x86_64 and Linux arm64.
When dependencies are missing it can auto-install via apt-get, dnf, or yum.
The script exits automatically when checks finish.
EOF
}

fail() {
  echo "public api gate check failed: $1" >&2
  exit 1
}

validate_numeric() {
  local key="$1"
  local value="$2"
  [[ "$value" =~ ^[0-9]+([.][0-9]+)?$ ]] || fail "$key must be numeric, got '$value'"
}

main() {
  local thresholds_file="${1:-$REPO_ROOT/scripts/public-api-thresholds.env}"
  local key=""
  local value=""

  if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
    exit 0
  fi

  ensure_linux_commands awk grep sed
  print_stage "validating public api gate schema file"

  [[ -f "$thresholds_file" ]] || fail "missing threshold file: $thresholds_file"

  while IFS='=' read -r key value; do
    key="$(echo "$key" | sed 's/[[:space:]]//g')"
    value="$(echo "$value" | sed 's/[[:space:]]//g')"
    [[ -z "$key" || "${key:0:1}" == "#" ]] && continue
    [[ -z "$value" ]] && fail "$key has empty value"

    case "$key" in
      PUBLIC_API_SLO_AVAILABILITY)
        validate_numeric "$key" "$value"
        awk "BEGIN {exit !($value > 0 && $value <= 100)}" || fail "$key must be within (0, 100]"
        ;;
      PUBLIC_API_GATEWAY_5XX_RATIO_MAX)
        validate_numeric "$key" "$value"
        awk "BEGIN {exit !($value >= 0 && $value <= 100)}" || fail "$key must be within [0, 100]"
        ;;
      PUBLIC_API_P95_MS|PUBLIC_API_P99_MS)
        validate_numeric "$key" "$value"
        awk "BEGIN {exit !($value > 0)}" || fail "$key must be greater than 0"
        ;;
      *)
        fail "unknown key in threshold file: $key"
        ;;
    esac
  done <"$thresholds_file"

  for key in \
    PUBLIC_API_SLO_AVAILABILITY \
    PUBLIC_API_GATEWAY_5XX_RATIO_MAX \
    PUBLIC_API_P95_MS \
    PUBLIC_API_P99_MS; do
    grep -Eq "^${key}[[:space:]]*=" "$thresholds_file" || fail "missing required key: $key"
  done

  print_stage "public api gate schema is valid"
  echo "public api gate schema check passed"
}

main "$@"
