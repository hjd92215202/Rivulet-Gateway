#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

ARTIFACT_PATH=""
TAG=""
REPO_SLUG="hjd92215202/Rivulet-Gateway"
FORMAT="tar.gz"
LABEL="service-install"
WORK_DIR=""
REPLACE_CONFIG="false"
SKIP_ENABLE="false"
SKIP_START="false"

DOWNLOAD_DIR=""
EXTRACT_DIR=""
BACKUP_DIR=""
SUMMARY_PATH=""
ASSET_ARCH_SUFFIX=""
RESOLVED_ARTIFACT=""
PACKAGE_ROOT=""
PACKAGE_BIN=""
PACKAGE_CONFIG=""
PACKAGE_UNIT=""
INSTALL_BIN="/usr/bin/gateway"
INSTALL_CONFIG_DIR="/etc/gateway"
INSTALL_CONFIG="/etc/gateway/gateway.toml"
INSTALL_UNIT_DIR="/usr/lib/systemd/system"
INSTALL_UNIT="/usr/lib/systemd/system/rivulet-gateway.service"

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-service-install.sh [input options] [install options]

input options:
  --artifact <path>         local package artifact path (.tar.gz or .rpm)
  --tag <tag>               github release tag to download when --artifact is omitted
  --repo <owner/name>       github repository slug, default: hjd92215202/Rivulet-Gateway
  --format <tar.gz|rpm>     release asset format when downloading, default: tar.gz

install options:
  --replace-config          replace /etc/gateway/gateway.toml instead of preserving it
  --skip-enable             install without enabling the service
  --skip-start              install without starting the service
  --label <label>           work label, default: service-install
  --work-dir <dir>          working directory, default: ./target/linux-service-install/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - installs binary, config, and systemd unit, then optionally enables and starts the service
  - exits automatically after writing the install summary

examples:
  bash ./scripts/linux-service-install.sh --tag v0.1.6 --format tar.gz
  bash ./scripts/linux-service-install.sh --artifact ./dist/rivulet-gateway.rpm --format rpm --replace-config
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
    --replace-config)
      REPLACE_CONFIG="true"
      shift
      ;;
    --skip-enable)
      SKIP_ENABLE="true"
      shift
      ;;
    --skip-start)
      SKIP_START="true"
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

ensure_linux_commands curl tar sha256sum python3 systemctl
if [[ "$FORMAT" == "rpm" ]]; then
  ensure_linux_commands rpm
fi

ASSET_ARCH_SUFFIX="$(detect_linux_asset_arch)"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$REPO_ROOT/target/linux-service-install/$TIMESTAMP-$LABEL"
fi

DOWNLOAD_DIR="$WORK_DIR/downloads"
EXTRACT_DIR="$WORK_DIR/extracted"
BACKUP_DIR="$WORK_DIR/backups"
SUMMARY_PATH="$WORK_DIR/summary.md"

mkdir -p "$DOWNLOAD_DIR" "$EXTRACT_DIR" "$BACKUP_DIR"

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
    standardize_artifact_path "$ARTIFACT_PATH"
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

assert_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "expected file not found: $path" >&2
    exit 1
  fi
}

backup_if_present() {
  local source_path="$1"
  local backup_name="$2"
  if [[ -f "$source_path" ]]; then
    run_privileged install -D -m 0644 "$source_path" "$BACKUP_DIR/$backup_name"
  fi
}

install_from_tarball() {
  print_stage "extracting package archive"
  PACKAGE_ROOT="$(extract_tarball_root "$RESOLVED_ARTIFACT")"
  if [[ -z "$PACKAGE_ROOT" || ! -d "$PACKAGE_ROOT" ]]; then
    echo "failed to determine extracted package root" >&2
    exit 1
  fi

  PACKAGE_BIN="$PACKAGE_ROOT/usr/bin/gateway"
  PACKAGE_CONFIG="$PACKAGE_ROOT/etc/gateway/gateway.toml"
  PACKAGE_UNIT="$PACKAGE_ROOT/usr/lib/systemd/system/rivulet-gateway.service"

  assert_file "$PACKAGE_BIN"
  assert_file "$PACKAGE_CONFIG"
  assert_file "$PACKAGE_UNIT"

  print_stage "backing up existing installation files when present"
  backup_if_present "$INSTALL_BIN" "usr-bin-gateway.bak"
  backup_if_present "$INSTALL_CONFIG" "gateway.toml.bak"
  backup_if_present "$INSTALL_UNIT" "rivulet-gateway.service.bak"

  print_stage "installing gateway binary and service unit"
  run_privileged install -d -m 0755 "$(dirname "$INSTALL_BIN")" "$INSTALL_CONFIG_DIR" "$INSTALL_UNIT_DIR"
  run_privileged install -m 0755 "$PACKAGE_BIN" "$INSTALL_BIN"
  run_privileged install -m 0644 "$PACKAGE_UNIT" "$INSTALL_UNIT"

  if [[ "$REPLACE_CONFIG" == "true" || ! -f "$INSTALL_CONFIG" ]]; then
    print_stage "installing gateway config"
    run_privileged install -m 0644 "$PACKAGE_CONFIG" "$INSTALL_CONFIG"
  else
    print_stage "preserving existing gateway config at $INSTALL_CONFIG"
  fi
}

install_from_rpm() {
  print_stage "installing rpm package with rpm -Uvh"
  run_privileged rpm -Uvh --replacepkgs "$RESOLVED_ARTIFACT"
}

enable_and_start_service() {
  print_stage "reloading systemd daemon"
  run_privileged systemctl daemon-reload

  if [[ "$SKIP_ENABLE" == "false" ]]; then
    print_stage "enabling rivulet-gateway service"
    run_privileged systemctl enable rivulet-gateway.service
  fi

  if [[ "$SKIP_START" == "false" ]]; then
    print_stage "starting rivulet-gateway service"
    run_privileged systemctl restart rivulet-gateway.service
    run_privileged systemctl --no-pager --full status rivulet-gateway.service >"$WORK_DIR/systemctl-status.txt"
  fi
}

write_summary() {
  cat >"$SUMMARY_PATH" <<EOF
# Linux Service Install Summary

## Inputs

- Artifact: \`$RESOLVED_ARTIFACT\`
- Tag: \`${TAG:-local-artifact}\`
- Format: \`$FORMAT\`
- Architecture suffix: \`$ASSET_ARCH_SUFFIX\`
- Replace config: \`$REPLACE_CONFIG\`
- Skip enable: \`$SKIP_ENABLE\`
- Skip start: \`$SKIP_START\`
- Working directory: \`$WORK_DIR\`

## Installed Paths

- Binary: \`$INSTALL_BIN\`
- Config: \`$INSTALL_CONFIG\`
- Unit: \`$INSTALL_UNIT\`

## Backups

- Backup directory: \`$BACKUP_DIR\`

## Notes

- For tar.gz installs, existing config is preserved unless \`--replace-config\` is used.
- For rpm installs, package-manager-native replacement is delegated to \`rpm -Uvh\`.
EOF
}

RESOLVED_ARTIFACT="$(resolve_artifact_path)"
RESOLVED_ARTIFACT="$(standardize_artifact_path "$RESOLVED_ARTIFACT")"

case "$RESOLVED_ARTIFACT" in
  *.tar.gz)
    install_from_tarball
    ;;
  *.rpm)
    install_from_rpm
    ;;
  *)
    echo "unsupported artifact type: $RESOLVED_ARTIFACT" >&2
    exit 1
    ;;
esac

enable_and_start_service
write_summary
print_stage "service install summary written to $SUMMARY_PATH"
