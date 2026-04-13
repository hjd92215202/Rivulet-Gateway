#!/usr/bin/env bash
set -euo pipefail

MODE="verify"
ASSETS_DIR=""
CERTIFICATE_IDENTITY_REGEXP=""
CERTIFICATE_OIDC_ISSUER="https://token.actions.githubusercontent.com"

usage() {
  cat <<'EOF'
Usage: bash ./scripts/verify-release-signatures.sh [options]

Verify detached signatures for release assets.

Options:
  --assets-dir <path>                       required
  --mode <verify|contract-check>            default: verify
  --certificate-identity-regexp <pattern>   required in verify mode
  --certificate-oidc-issuer <issuer>        default: https://token.actions.githubusercontent.com
  -h, --help

Modes:
  verify         validate signature contract and run cosign verify-blob for each asset
  contract-check validate signature file presence and naming only
EOF
}

fail() {
  echo "release signature verification failed: $1" >&2
  exit 1
}

verify_contract_for_asset() {
  local asset_path="$1"
  [[ -f "${asset_path}.sig" ]] || fail "missing detached signature for $(basename "$asset_path")"
  [[ -f "${asset_path}.pem" ]] || fail "missing certificate for $(basename "$asset_path")"
}

verify_blob_signature() {
  local asset_path="$1"
  local signature_path="${asset_path}.sig"
  local certificate_path="${asset_path}.pem"

  cosign verify-blob \
    --certificate "$certificate_path" \
    --signature "$signature_path" \
    --certificate-identity-regexp "$CERTIFICATE_IDENTITY_REGEXP" \
    --certificate-oidc-issuer "$CERTIFICATE_OIDC_ISSUER" \
    "$asset_path" >/dev/null
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --assets-dir)
      ASSETS_DIR="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --certificate-identity-regexp)
      CERTIFICATE_IDENTITY_REGEXP="$2"
      shift 2
      ;;
    --certificate-oidc-issuer)
      CERTIFICATE_OIDC_ISSUER="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "$ASSETS_DIR" ]] || fail "--assets-dir is required"
[[ -d "$ASSETS_DIR" ]] || fail "assets directory not found: $ASSETS_DIR"
[[ "$MODE" == "verify" || "$MODE" == "contract-check" ]] || fail "unsupported mode: $MODE"

if [[ "$MODE" == "verify" ]]; then
  command -v cosign >/dev/null 2>&1 || fail "cosign is required in verify mode"
  [[ -n "$CERTIFICATE_IDENTITY_REGEXP" ]] || fail "--certificate-identity-regexp is required in verify mode"
fi

mapfile -t assets < <(find "$ASSETS_DIR" -maxdepth 1 -type f ! -name '*.sig' ! -name '*.pem' | sort)
[[ "${#assets[@]}" -gt 0 ]] || fail "no release assets found under $ASSETS_DIR"

for asset in "${assets[@]}"; do
  verify_contract_for_asset "$asset"
  if [[ "$MODE" == "verify" ]]; then
    verify_blob_signature "$asset"
  fi
done

manifest_path="$ASSETS_DIR/SHA256SUMS.txt"
if [[ -f "$manifest_path" ]]; then
  [[ -f "$ASSETS_DIR/SHA256SUMS.sig" ]] || fail "missing compatibility signature SHA256SUMS.sig"
  [[ -f "$ASSETS_DIR/SHA256SUMS.pem" ]] || fail "missing compatibility certificate SHA256SUMS.pem"
  if [[ "$MODE" == "verify" ]]; then
    cosign verify-blob \
      --certificate "$ASSETS_DIR/SHA256SUMS.pem" \
      --signature "$ASSETS_DIR/SHA256SUMS.sig" \
      --certificate-identity-regexp "$CERTIFICATE_IDENTITY_REGEXP" \
      --certificate-oidc-issuer "$CERTIFICATE_OIDC_ISSUER" \
      "$manifest_path" >/dev/null
  fi
fi

echo "release signature verification passed: mode=$MODE assets=${#assets[@]}"
