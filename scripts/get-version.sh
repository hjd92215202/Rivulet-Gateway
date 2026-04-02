#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_MANIFEST="$REPO_ROOT/Cargo.toml"

normalize_version() {
  local raw_version="$1"

  # Strip a leading v so tag-based release assets and package versions stay aligned.
  if [[ "$raw_version" == v* ]]; then
    printf '%s\n' "${raw_version#v}"
  else
    printf '%s\n' "$raw_version"
  fi
}

resolve_release_version_override() {
  # An explicit env override comes first for CI/CD and controlled release replays.
  if [[ -n "${RIVULET_RELEASE_VERSION:-}" ]]; then
    normalize_version "$RIVULET_RELEASE_VERSION"
    return 0
  fi

  # Tag-triggered GitHub releases use the tag value directly to keep assets aligned.
  if [[ "${GITHUB_REF_TYPE:-}" == "tag" && -n "${GITHUB_REF_NAME:-}" ]]; then
    normalize_version "$GITHUB_REF_NAME"
    return 0
  fi

  return 1
}

if resolve_release_version_override; then
  exit 0
fi

awk '
  /^\[workspace\.package\]/ { in_workspace_package=1; next }
  in_workspace_package && /^\[/ { exit }
  in_workspace_package && /^version[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' "$WORKSPACE_MANIFEST"
