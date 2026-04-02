#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_MANIFEST="$REPO_ROOT/Cargo.toml"
VERSION="$(awk '
  /^\[workspace\.package\]/ { in_workspace_package=1; next }
  in_workspace_package && /^\[/ { exit }
  in_workspace_package && /^version[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' "$WORKSPACE_MANIFEST")"

TARGET="${TARGET:-}"
FORMAT="auto"
PROFILE="release"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --format)
      FORMAT="$2"
      shift 2
      ;;
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$TARGET" ]]; then
  TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"
fi

if [[ -z "$VERSION" ]]; then
  echo "workspace version not found" >&2
  exit 1
fi

if [[ "$FORMAT" == "auto" ]]; then
  FORMAT="tar.gz"
fi

BUILD_ARGS=(build -p gateway-main)
if [[ "$PROFILE" == "release" ]]; then
  BUILD_ARGS+=(--release)
fi
if [[ -n "$TARGET" ]]; then
  BUILD_ARGS+=(--target "$TARGET")
fi

echo "building Rivulet Gateway"
echo "target=$TARGET format=$FORMAT profile=$PROFILE"
cargo "${BUILD_ARGS[@]}"

BINARY_PATH="$REPO_ROOT/target/$TARGET/$PROFILE/gateway-main"
if [[ ! -f "$BINARY_PATH" ]]; then
  echo "built binary not found: $BINARY_PATH" >&2
  exit 1
fi

DIST_ROOT="$REPO_ROOT/dist/$TARGET"
PACKAGE_ROOT="$DIST_ROOT/rivulet-gateway-$VERSION"
rm -rf "$PACKAGE_ROOT"

BIN_DIR="$PACKAGE_ROOT/usr/bin"
CONFIG_DIR="$PACKAGE_ROOT/etc/gateway"
SERVICE_DIR="$PACKAGE_ROOT/usr/lib/systemd/system"
DOC_DIR="$PACKAGE_ROOT/usr/share/doc/rivulet-gateway"

mkdir -p "$BIN_DIR" "$CONFIG_DIR" "$SERVICE_DIR" "$DOC_DIR"
cp "$BINARY_PATH" "$BIN_DIR/gateway"
cp "$REPO_ROOT/packaging/examples/gateway.toml" "$CONFIG_DIR/gateway.toml"
cp "$REPO_ROOT/packaging/linux/gateway.service" "$SERVICE_DIR/rivulet-gateway.service"
cp "$REPO_ROOT/README.md" "$DOC_DIR/README.md"

case "$FORMAT" in
  dir)
    echo "package root: $PACKAGE_ROOT"
    ;;
  tar.gz)
    TAR_PATH="$PACKAGE_ROOT.tar.gz"
    rm -f "$TAR_PATH"
    tar -czf "$TAR_PATH" -C "$DIST_ROOT" "$(basename "$PACKAGE_ROOT")"
    echo "archive: $TAR_PATH"
    ;;
  rpm)
    if ! command -v rpmbuild >/dev/null 2>&1; then
      echo "rpmbuild is required for rpm packaging" >&2
      exit 1
    fi

    RPM_TOPDIR="$REPO_ROOT/dist/rpmbuild/$TARGET"
    RPM_SOURCE_DIR="$RPM_TOPDIR/SOURCES"
    RPM_SPEC_DIR="$RPM_TOPDIR/SPECS"
    mkdir -p "$RPM_TOPDIR/BUILD" "$RPM_TOPDIR/BUILDROOT" "$RPM_TOPDIR/RPMS" "$RPM_TOPDIR/SOURCES" "$RPM_TOPDIR/SPECS" "$RPM_TOPDIR/SRPMS"

    SOURCE_TARBALL="$RPM_SOURCE_DIR/gateway-$VERSION.tar.gz"
    SPEC_PATH="$RPM_SPEC_DIR/rivulet-gateway.spec"
    RPM_ARCH="$(map_rpm_arch "$TARGET")"

    rm -f "$SOURCE_TARBALL"
    tar -czf "$SOURCE_TARBALL" -C "$DIST_ROOT" "$(basename "$PACKAGE_ROOT")"
    cp "$REPO_ROOT/packaging/rpm/rivulet-gateway.spec" "$SPEC_PATH"

    rpmbuild \
      --define "_topdir $RPM_TOPDIR" \
      --define "version $VERSION" \
      --define "release 1" \
      --define "package_root $(basename "$PACKAGE_ROOT")" \
      --define "build_arch $RPM_ARCH" \
      --target "$RPM_ARCH" \
      -ba "$SPEC_PATH"

    echo "rpm output: $RPM_TOPDIR/RPMS"
    ;;
  *)
    echo "unsupported format: $FORMAT" >&2
    exit 1
    ;;
esac

map_rpm_arch() {
  case "$1" in
    x86_64-unknown-linux-gnu)
      echo "x86_64"
      ;;
    aarch64-unknown-linux-gnu)
      echo "aarch64"
      ;;
    *)
      echo "unsupported rpm target: $1" >&2
      exit 1
      ;;
  esac
}
