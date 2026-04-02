#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_MANIFEST="$REPO_ROOT/Cargo.toml"

awk '
  /^\[workspace\.package\]/ { in_workspace_package=1; next }
  in_workspace_package && /^\[/ { exit }
  in_workspace_package && /^version[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' "$WORKSPACE_MANIFEST"
