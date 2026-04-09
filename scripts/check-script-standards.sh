#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() {
  echo "script standards check failed: $1" >&2
  exit 1
}

check_file_header() {
  local script_path="$1"
  local line1=""
  local line2=""

  line1="$(sed -n '1p' "$script_path")"
  line2="$(sed -n '2p' "$script_path")"

  [[ "$line1" == "#!/usr/bin/env bash" ]] || fail "$script_path missing bash shebang"
  [[ "$line2" == "set -euo pipefail" ]] || fail "$script_path missing strict bash options"
}

check_linux_script_contract() {
  local script_path="$1"

  grep -Fq 'source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"' "$script_path" \
    || fail "$script_path must source scripts/lib/linux-bootstrap.sh"
  grep -Fq 'ensure_linux_commands' "$script_path" \
    || fail "$script_path must use ensure_linux_commands"
  grep -Fq 'print_stage' "$script_path" \
    || fail "$script_path must print stage progress with print_stage"
  grep -Fq 'supports Linux x86_64 and Linux arm64' "$script_path" \
    || fail "$script_path help text must declare Linux x86_64 and Linux arm64 support"
  grep -Fq 'apt-get, dnf, or yum' "$script_path" \
    || fail "$script_path help text must declare apt-get, dnf, or yum support"
  grep -Fq 'exits automatically' "$script_path" \
    || fail "$script_path help text must declare automatic exit behavior"
}

check_print_stage_contract() {
  local helper_path="$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

  grep -Fq 'print_stage() {' "$helper_path" \
    || fail "$helper_path missing print_stage helper"
  grep -Fq "printf '[stage] %s\\n' \"\$message\" >&2" "$helper_path" \
    || fail "$helper_path print_stage must write stage logs to stderr"
}

main() {
  local script_path=""

  [[ -f "$REPO_ROOT/scripts/lib/linux-bootstrap.sh" ]] \
    || fail "shared helper scripts/lib/linux-bootstrap.sh is missing"
  check_print_stage_contract

  while IFS= read -r script_path; do
    bash -n "$script_path"
    check_file_header "$script_path"
  done < <(find "$REPO_ROOT/scripts" "$REPO_ROOT/packaging/tests" -type f -name '*.sh' | sort)

  while IFS= read -r script_path; do
    check_linux_script_contract "$script_path"
  done < <(find "$REPO_ROOT/scripts" -maxdepth 1 -type f -name 'linux-*.sh' | sort)

  echo "script standards check passed"
}

main "$@"
