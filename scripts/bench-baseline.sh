#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/target/bench}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
OUTPUT_FILE="$OUTPUT_DIR/baseline-$TIMESTAMP.csv"

mkdir -p "$OUTPUT_DIR"

export GATEWAY_DISABLE_ACCESS_LOG=1

cd "$REPO_ROOT"
cargo run -q --release -p gateway-bench -- \
  --measure-secs 1 \
  --warmup-secs 1 \
  --concurrency 1,4,8,16,32,64 \
  --response-sizes 64,4096 \
  --idle-pools 1 | tee "$OUTPUT_FILE"

echo "benchmark report: $OUTPUT_FILE"
