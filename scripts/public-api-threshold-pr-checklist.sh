#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

REPORT_PATH=""
OUTPUT_DIR=""
PYTHON_BIN=""
CHECKLIST_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-threshold-pr-checklist.sh [options]

Build a human-review checklist for threshold PR discussion from calibration-report.json.

Options:
  --report <path>             calibration-report.json path (required)
  --output-dir <path>         default: ./target/public-api-threshold-pr-checklist/<timestamp>
  -h, --help

Outputs:
  threshold-pr-checklist.md
EOF
}

fail() {
  echo "public api threshold pr checklist failed: $1" >&2
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
    --report)
      REPORT_PATH="$2"
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

[[ -n "$REPORT_PATH" ]] || fail "--report is required"
[[ -f "$REPORT_PATH" ]] || fail "report file not found: $REPORT_PATH"
REPORT_PATH="$(normalize_path "$REPORT_PATH")"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-threshold-pr-checklist/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
CHECKLIST_PATH="$OUTPUT_DIR/threshold-pr-checklist.md"

resolve_python_bin
"$PYTHON_BIN" - "$REPORT_PATH" "$CHECKLIST_PATH" <<'PY'
import json
import math
import sys
from datetime import datetime, timezone
from pathlib import Path

report_path = Path(sys.argv[1])
checklist_path = Path(sys.argv[2])

try:
    report = json.loads(report_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid calibration report json: {report_path} ({exc})")

analysis = report.get("analysis")
if not isinstance(analysis, dict) or not analysis:
    raise SystemExit("calibration report missing analysis block")

metric_order = [
    "availability_min",
    "gateway_5xx_ratio_max",
    "p95_ms_max",
    "p99_ms_max",
]

arch_sections = {}
for arch, block in analysis.items():
    if not isinstance(block, dict):
        continue
    current = block.get("thresholds_current_profile") or block.get("thresholds_standard") or {}
    recommended = block.get("recommended_thresholds") or report.get("recommended_thresholds", {}).get(arch, {})
    budget = block.get("change_budget") or report.get("change_budget", {}).get(arch, {})
    distribution = block.get("distribution") or {}
    trend = block.get("trend_latest") or []
    sample_count = int(block.get("sample_count") or 0)

    rows = []
    changed_metrics = []
    for metric in metric_order:
        current_value = float(current.get(metric, 0.0))
        recommended_value = float(recommended.get(metric, current_value))
        delta = round(recommended_value - current_value, 6)
        if not math.isclose(delta, 0.0, abs_tol=1e-9):
            changed_metrics.append(metric)
        budget_item = budget.get(metric, {})
        rows.append(
            {
                "metric": metric,
                "current": round(current_value, 6),
                "recommended": round(recommended_value, 6),
                "delta": delta,
                "max_step_down": round(float(budget_item.get("max_step_down", 0.0)), 6),
                "max_step_up": round(float(budget_item.get("max_step_up", 0.0)), 6),
            }
        )

    arch_sections[arch] = {
        "sample_count": sample_count,
        "rows": rows,
        "changed_metrics": changed_metrics,
        "distribution": distribution,
        "trend_latest": trend,
    }

recommendation_count = sum(len(item["changed_metrics"]) for item in arch_sections.values())
manual_items = report.get("recommended_review_items") or []

lines = [
    "# Public API Threshold PR Checklist / Public API 阈值 PR 审核清单",
    "",
    "## Overview / 概览",
    "",
    f"- profile: `{report.get('profile', 'unknown')}`",
    f"- calibration_generated_at: `{report.get('generated_at', 'unknown')}`",
    f"- checklist_generated_at: `{datetime.now(timezone.utc).isoformat()}`",
    f"- input_count: `{report.get('input_count', 0)}`",
    f"- recommendation_count: `{recommendation_count}`",
    "",
    "Decision policy / 决策策略:",
    "- Keep CI/release `standard` blocking unchanged; this checklist is report-driven and human-reviewed.",
    "- 保持 CI/release `standard` 阻断不变；本清单仅用于“报告驱动 + 人工审阅”的阈值提案。",
    "",
]

for arch, section in sorted(arch_sections.items()):
    lines.append(f"## Architecture: {arch}")
    lines.append("")
    lines.append(f"- sample_count: `{section['sample_count']}`")
    lines.append(f"- changed_metrics: `{', '.join(section['changed_metrics']) if section['changed_metrics'] else 'none'}`")
    lines.append("")
    lines.append("| metric | current | recommended | delta | max_step_down | max_step_up |")
    lines.append("| --- | ---: | ---: | ---: | ---: | ---: |")
    for row in section["rows"]:
        lines.append(
            f"| {row['metric']} | {row['current']} | {row['recommended']} | {row['delta']} | {row['max_step_down']} | {row['max_step_up']} |"
        )
    lines.append("")
    lines.append("Evidence snapshot / 证据快照:")
    dist = section["distribution"]
    for metric in ["availability", "gateway_5xx_ratio", "p95_ms", "p99_ms"]:
        block = dist.get(metric, {})
        lines.append(
            f"- {metric}: min={block.get('min', 0.0)} median={block.get('median', 0.0)} p95={block.get('p95', 0.0)} max={block.get('max', 0.0)}"
        )
    lines.append("")
    lines.append("Latest sample references / 最近样本引用:")
    if section["trend_latest"]:
        for item in section["trend_latest"][:5]:
            lines.append(f"- `{item.get('timestamp', 'n/a')}` | pass={item.get('pass', False)} | `{item.get('path', 'n/a')}`")
    else:
        lines.append("- no trend samples")
    lines.append("")

lines.append("## Risks And Rollback / 风险与回滚")
lines.append("")
lines.append("Risks / 风险:")
lines.append("- Threshold changes can hide regressions if evidence window is too short.")
lines.append("- 如果样本窗口太短，阈值调整可能掩盖真实回归。")
lines.append("- Any recommendation should remain within one-step change budget.")
lines.append("- 每次调整必须保持在单次预算范围内。")
lines.append("")
lines.append("Rollback conditions / 回滚条件:")
lines.append("- Revert threshold PR if two consecutive CI/release dual-arch gate runs fail after merge.")
lines.append("- 合并后若连续两次 CI/release 双架构 gate 失败，应立即回滚阈值 PR。")
lines.append("- Revert when failure reasons newly include `availability_below_threshold` or `gateway_5xx_ratio_above_threshold`.")
lines.append("- 如新出现 `availability_below_threshold` 或 `gateway_5xx_ratio_above_threshold`，应回滚。")
lines.append("")
lines.append("## PR Checklist / PR 提交检查项")
lines.append("")
lines.append("- [ ] threshold change is isolated in a dedicated `feat:` commit")
lines.append("- [ ] docs are updated in both Chinese and English")
lines.append("- [ ] calibration evidence links are attached in PR description")
lines.append("- [ ] rollback plan is written with clear trigger conditions")
lines.append("")

if manual_items:
    lines.append("## Manual Review Items / 人工复核项")
    lines.append("")
    for item in manual_items:
        lines.append(f"- {item}")
    lines.append("")

checklist_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

echo "checklist: $CHECKLIST_PATH"
