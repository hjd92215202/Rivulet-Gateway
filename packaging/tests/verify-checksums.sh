#!/usr/bin/env bash
set -euo pipefail

MANIFEST_PATH="${1:-SHA256SUMS}"
BASE_DIR="${2:-.}"

if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "checksum manifest not found: $MANIFEST_PATH" >&2
  exit 1
fi

MANIFEST_PATH="$(cd "$(dirname "$MANIFEST_PATH")" && pwd)/$(basename "$MANIFEST_PATH")"
BASE_DIR="$(cd "$BASE_DIR" && pwd)"

(
  cd "$BASE_DIR"
  sha256sum --check "$MANIFEST_PATH"
)
