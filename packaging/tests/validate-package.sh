#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_PATH="${1:-}"
WORK_DIR="${2:-}"

if [[ -z "$ARTIFACT_PATH" ]]; then
  echo "usage: $0 <artifact-path> [work-dir]" >&2
  exit 1
fi

if [[ ! -f "$ARTIFACT_PATH" ]]; then
  echo "artifact not found: $ARTIFACT_PATH" >&2
  exit 1
fi

if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$(mktemp -d)"
fi

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

case "$ARTIFACT_PATH" in
  *.tar.gz)
    validate_tarball "$ARTIFACT_PATH" "$WORK_DIR"
    ;;
  *.rpm)
    validate_rpm "$ARTIFACT_PATH"
    ;;
  *)
    echo "unsupported artifact: $ARTIFACT_PATH" >&2
    exit 1
    ;;
esac

validate_tarball() {
  local artifact_path="$1"
  local work_dir="$2"
  local unpack_dir="$work_dir/unpack"
  mkdir -p "$unpack_dir"

  tar -xzf "$artifact_path" -C "$unpack_dir"
  local root_dir
  root_dir="$(find "$unpack_dir" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  if [[ -z "$root_dir" ]]; then
    echo "tarball did not contain a package root directory" >&2
    exit 1
  fi

  assert_file "$root_dir/usr/bin/gateway"
  assert_file "$root_dir/etc/gateway/gateway.toml"
  assert_file "$root_dir/usr/lib/systemd/system/rivulet-gateway.service"
  assert_file "$root_dir/usr/share/doc/rivulet-gateway/README.md"

  echo "tarball validation passed: $artifact_path"
}

validate_rpm() {
  local artifact_path="$1"
  if ! command -v rpm >/dev/null 2>&1; then
    echo "rpm command is required to validate rpm artifacts" >&2
    exit 1
  fi

  local file_list
  file_list="$(rpm -qlp "$artifact_path")"

  grep -qx '/usr/bin/gateway' <<<"$file_list" || fail_missing '/usr/bin/gateway'
  grep -qx '/etc/gateway/gateway.toml' <<<"$file_list" || fail_missing '/etc/gateway/gateway.toml'
  grep -qx '/usr/lib/systemd/system/rivulet-gateway.service' <<<"$file_list" || fail_missing '/usr/lib/systemd/system/rivulet-gateway.service'
  grep -qx '/usr/share/doc/rivulet-gateway/README.md' <<<"$file_list" || fail_missing '/usr/share/doc/rivulet-gateway/README.md'

  echo "rpm validation passed: $artifact_path"
}

assert_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected file missing: $path" >&2
    exit 1
  fi
}

fail_missing() {
  local path="$1"
  echo "expected rpm entry missing: $path" >&2
  exit 1
}
