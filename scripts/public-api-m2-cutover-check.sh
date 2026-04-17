#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CLOSURE_STATUS_PATH=""
OUTPUT_DIR=""
PYTHON_BIN=""

RESULT_PATH=""
SUMMARY_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-m2-cutover-check.sh [options]

Evaluate whether Milestone 2 closure is ready for Milestone 3 cutover docs update.

Options:
  --closure-status <path>      milestone2-closure-status.json path (required)
  --output-dir <path>          default: ./target/public-api-m2-cutover-check/<timestamp>
  -h, --help

Outputs:
  m2-cutover-check.json
  m2-cutover-check.md
EOF
}

fail() {
  echo "public api m2 cutover check failed: $1" >&2
  exit 1
}

normalize_path() {
  local input_path="$1"
  printf '%s/%s\n' "$(cd "$(dirname "$input_path")" && pwd)" "$(basename "$input_path")"
}

resolve_python_bin() {
  local candidate=""
  for candidate in python3 python; do
    if ! command -v "$candidate" >/dev/null 2>&1; then
      continue
    fi
    if "$candidate" -c 'import sys; sys.exit(0)' >/dev/null 2>&1; then
      PYTHON_BIN="$candidate"
      return 0
    fi
  done
  fail "python runtime is unavailable; checked python3 and python"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --closure-status)
      CLOSURE_STATUS_PATH="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
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

[[ -n "$CLOSURE_STATUS_PATH" ]] || fail "--closure-status is required"
[[ -f "$CLOSURE_STATUS_PATH" ]] || fail "closure status report not found: $CLOSURE_STATUS_PATH"
CLOSURE_STATUS_PATH="$(normalize_path "$CLOSURE_STATUS_PATH")"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-m2-cutover-check/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
RESULT_PATH="$OUTPUT_DIR/m2-cutover-check.json"
SUMMARY_PATH="$OUTPUT_DIR/m2-cutover-check.md"

resolve_python_bin
"$PYTHON_BIN" - "$CLOSURE_STATUS_PATH" "$RESULT_PATH" "$SUMMARY_PATH" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

closure_path = Path(sys.argv[1])
result_path = Path(sys.argv[2])
summary_path = Path(sys.argv[3])

try:
    closure = json.loads(closure_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid closure status json: {closure_path} ({exc})")

ci_ready = bool(closure.get("ci_closure_ready", False))
release_ready = bool(closure.get("release_closure_ready", False))
overall_ready = bool(closure.get("overall_closure_ready", False))
remaining = int(closure.get("remaining_to_target", 0))
ineligible_reasons = closure.get("recent_ineligible_reasons", [])
rollback_recommended = bool(closure.get("rollback_recommended", False))

cutover_ready = ci_ready and release_ready and overall_ready
next_action = (
    "proceed_m2_to_m3_docs_cutover"
    if cutover_ready
    else "hold_m2_collect_more_evidence"
)

payload = {
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "source_closure_status": str(closure_path),
    "ci_closure_ready": ci_ready,
    "release_closure_ready": release_ready,
    "overall_closure_ready": overall_ready,
    "remaining_to_target": remaining,
    "cutover_ready": cutover_ready,
    "ineligible_noise_count": len(ineligible_reasons),
    "rollback_recommended": rollback_recommended,
    "next_action": next_action,
}
result_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")

lines = [
    "# M2 Cutover Check / M2 切线检查",
    "",
    "## Summary / 摘要",
    "",
    f"- generated_at: `{payload['generated_at']}`",
    f"- ci_closure_ready: `{ci_ready}`",
    f"- release_closure_ready: `{release_ready}`",
    f"- overall_closure_ready: `{overall_ready}`",
    f"- remaining_to_target: `{remaining}`",
    f"- cutover_ready: `{cutover_ready}`",
    f"- ineligible_noise_count: `{payload['ineligible_noise_count']}`",
    f"- rollback_recommended: `{payload['rollback_recommended']}`",
    f"- next_action: `{next_action}`",
    "",
    "## Cutover Rule / 切线规则",
    "",
    "- Only when CI + release are both dual-arch 10-green ready can M2 be marked completed.",
    "- 仅当 CI 与 release 双链路均达到双架构 10 连绿，才允许将 M2 标记为 completed。",
    "",
]

if ineligible_reasons:
    lines.append("## Ineligible Noise Note / 无效样本噪声说明")
    lines.append("")
    lines.append("| reason | count |")
    lines.append("| --- | ---: |")
    for item in ineligible_reasons:
        lines.append(f"| `{item.get('reason', 'unknown')}` | {item.get('count', 0)} |")
    lines.append("")

summary_path.write_text("\n".join(lines), encoding="utf-8")
PY

echo "cutover_result: $RESULT_PATH"
echo "cutover_summary: $SUMMARY_PATH"
