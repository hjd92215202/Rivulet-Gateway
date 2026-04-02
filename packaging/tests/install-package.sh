#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_PATH="${1:-}"
WORK_DIR="${2:-}"
ROOT_DIR=""

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

extract_tarball_root() {
  local artifact_path="$1"
  local work_dir="$2"
  local unpack_dir="$work_dir/unpack"
  mkdir -p "$unpack_dir"

  tar -xzf "$artifact_path" -C "$unpack_dir"
  find "$unpack_dir" -mindepth 2 -maxdepth 2 -type d -path '*/usr' -prune | sed 's#/usr$##' | head -n 1
}

extract_rpm_root() {
  local artifact_path="$1"
  local work_dir="$2"
  local unpack_dir="$work_dir/rpm-root"
  mkdir -p "$unpack_dir"

  if ! command -v rpm2cpio >/dev/null 2>&1; then
    echo "rpm2cpio is required to inspect rpm payloads" >&2
    exit 1
  fi
  if ! command -v cpio >/dev/null 2>&1; then
    echo "cpio is required to inspect rpm payloads" >&2
    exit 1
  fi

  (
    cd "$unpack_dir"
    rpm2cpio "$artifact_path" | cpio -idmu --quiet
  )

  echo "$unpack_dir"
}

assert_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected installed file missing: $path" >&2
    exit 1
  fi
}

validate_installed_root() {
  local root_dir="$1"

  if [[ -z "$root_dir" || ! -d "$root_dir" ]]; then
    echo "installed root directory not found" >&2
    exit 1
  fi

  assert_file "$root_dir/usr/bin/gateway"
  assert_file "$root_dir/etc/gateway/gateway.toml"
  assert_file "$root_dir/usr/lib/systemd/system/rivulet-gateway.service"
  assert_file "$root_dir/usr/share/doc/rivulet-gateway/README.md"

  # 额外校验 service 中的关键启动命令，避免包内容在但服务定义漂移。
  if ! grep -q '^ExecStart=/usr/bin/gateway /etc/gateway/gateway.toml$' "$root_dir/usr/lib/systemd/system/rivulet-gateway.service"; then
    echo "service file ExecStart does not match expected gateway command" >&2
    exit 1
  fi
}

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

case "$ARTIFACT_PATH" in
  *.tar.gz)
    ROOT_DIR="$(extract_tarball_root "$ARTIFACT_PATH" "$WORK_DIR")"
    ;;
  *.rpm)
    ROOT_DIR="$(extract_rpm_root "$ARTIFACT_PATH" "$WORK_DIR")"
    ;;
  *)
    echo "unsupported artifact: $ARTIFACT_PATH" >&2
    exit 1
    ;;
esac

validate_installed_root "$ROOT_DIR"

# 这里不尝试在 CI 里真的安装 systemd 服务，
# 而是直接用“安装后的文件系统布局”执行二进制，验证包体足以支撑最小上线闭环。
PORT="${PORT:-18080}" bash "$(dirname "$0")/server-smoke.sh" "$ROOT_DIR/usr/bin/gateway"

echo "installed package smoke passed: $ARTIFACT_PATH"
