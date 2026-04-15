#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

REPORT_PATH=""
OUTPUT_DIR=""
PYTHON_BIN=""
PROPOSAL_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-threshold-change-proposal.sh [options]

Build a threshold-change proposal package from calibration-report.json (report-only, no auto write).

Options:
  --report <path>             calibration-report.json path (required)
  --output-dir <path>         default: ./target/public-api-threshold-proposal/<timestamp>
  -h, --help

Outputs:
  threshold-change-proposal.md
EOF
}

fail() {
  echo "public api threshold change proposal failed: $1" >&2
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
  OUTPUT_DIR="$REPO_ROOT/target/public-api-threshold-proposal/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
PROPOSAL_PATH="$OUTPUT_DIR/threshold-change-proposal.md"

resolve_python_bin
"$PYTHON_BIN" - "$REPORT_PATH" "$PROPOSAL_PATH" <<'PY'
import json
import math
import sys
from datetime import datetime, timezone
from pathlib import Path

report_path = Path(sys.argv[1])
proposal_path = Path(sys.argv[2])

try:
    report = json.loads(report_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid calibration report json: {report_path} ({exc})")

profile = str(report.get("profile") or "").strip().lower()
if profile != "standard":
    raise SystemExit(
        f"proposal generation currently only supports profile=standard, got: {profile or 'unknown'}"
    )

analysis = report.get("analysis")
if not isinstance(analysis, dict) or not analysis:
    raise SystemExit("calibration report missing analysis block")

metric_env_key = {
    "availability_min": "SLO_AVAILABILITY",
    "gateway_5xx_ratio_max": "GATEWAY_5XX_RATIO_MAX",
    "p95_ms_max": "P95_MS",
    "p99_ms_max": "P99_MS",
}
metric_order = list(metric_env_key.keys())

arch_prefix = {
    "linux-x86_64": "PUBLIC_API_STANDARD_LINUX_X86_64_",
    "linux-arm64": "PUBLIC_API_STANDARD_LINUX_ARM64_",
}

arch_blocks = {}
total_changes = 0

for arch, prefix in arch_prefix.items():
    block = analysis.get(arch, {})
    current = block.get("thresholds_current_profile") or block.get("thresholds_standard") or {}
    recommended = block.get("recommended_thresholds") or report.get("recommended_thresholds", {}).get(arch, {})
    budget = block.get("change_budget") or report.get("change_budget", {}).get(arch, {})
    trend = block.get("trend_latest") or []
    sample_count = int(block.get("sample_count") or 0)
    distribution = block.get("distribution") or {}

    rows = []
    changed_rows = []
    for metric in metric_order:
        current_value = float(current.get(metric, 0.0))
        recommended_value = float(recommended.get(metric, current_value))
        delta = round(recommended_value - current_value, 6)
        delta_ratio = round((delta / current_value) * 100.0, 6) if not math.isclose(current_value, 0.0) else 0.0
        budget_item = budget.get(metric, {})
        max_step_down = float(budget_item.get("max_step_down", 0.0))
        max_step_up = float(budget_item.get("max_step_up", 0.0))
        within_budget = (-max_step_down - 1e-9) <= delta <= (max_step_up + 1e-9)

        row = {
            "env_key": f"{prefix}{metric_env_key[metric]}",
            "metric": metric,
            "current": round(current_value, 6),
            "recommended": round(recommended_value, 6),
            "delta": delta,
            "delta_ratio_percent": delta_ratio,
            "max_step_down": round(max_step_down, 6),
            "max_step_up": round(max_step_up, 6),
            "within_budget": within_budget,
        }
        rows.append(row)
        if not math.isclose(delta, 0.0, abs_tol=1e-9):
            changed_rows.append(row)
            total_changes += 1

    arch_blocks[arch] = {
        "sample_count": sample_count,
        "rows": rows,
        "changed_rows": changed_rows,
        "distribution": distribution,
        "trend_latest": trend,
    }

manual_items = report.get("recommended_review_items") or []

lines = [
    "# Threshold Change Proposal (Report-Only)",
    "",
    "## Overview",
    "",
    f"- profile: `{profile}`",
    f"- calibration_generated_at: `{report.get('generated_at', 'unknown')}`",
    f"- proposal_generated_at: `{datetime.now(timezone.utc).isoformat()}`",
    f"- input_count: `{report.get('input_count', 0)}`",
    f"- proposed_metric_changes: `{total_changes}`",
    "",
    "## Guardrails",
    "",
    "- This proposal is report-only. It does not modify `scripts/public-api-thresholds.env` automatically.",
    "- Only architecture-aware `standard` keys are in scope for updates.",
    "- Single-change budget must remain within 10% (`|delta| <= 10%`).",
    "- Threshold changes must land in an isolated `feat:` commit with bilingual doc updates.",
    "",
]

for arch, block in arch_blocks.items():
    lines.append(f"## Architecture: {arch}")
    lines.append("")
    lines.append(f"- sample_count: `{block['sample_count']}`")
    lines.append(f"- changed_metrics: `{len(block['changed_rows'])}`")
    lines.append("")
    lines.append("| env_key | current | recommended | delta | delta_ratio(%) | budget_down | budget_up | within_budget |")
    lines.append("| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
    for row in block["rows"]:
        lines.append(
            f"| `{row['env_key']}` | {row['current']} | {row['recommended']} | {row['delta']} | {row['delta_ratio_percent']} | "
            f"{row['max_step_down']} | {row['max_step_up']} | {row['within_budget']} |"
        )
    lines.append("")
    lines.append("Evidence sample (latest observe points):")
    if block["trend_latest"]:
        for item in block["trend_latest"][:5]:
            lines.append(
                f"- `{item.get('timestamp', 'n/a')}` pass=`{item.get('pass', False)}` "
                f"availability=`{item.get('availability', 0.0)}` "
                f"gateway_5xx_ratio=`{item.get('gateway_5xx_ratio', 0.0)}` "
                f"p95=`{item.get('p95_ms', 0.0)}` p99=`{item.get('p99_ms', 0.0)}` "
                f"source=`{item.get('path', 'n/a')}`"
            )
    else:
        lines.append("- no trend evidence found")
    lines.append("")
    lines.append("Distribution snapshot:")
    for metric in ["availability", "gateway_5xx_ratio", "p95_ms", "p99_ms"]:
        dist = block["distribution"].get(metric, {})
        lines.append(
            f"- {metric}: min={dist.get('min', 0.0)} median={dist.get('median', 0.0)} "
            f"p95={dist.get('p95', 0.0)} max={dist.get('max', 0.0)}"
        )
    lines.append("")

lines.extend(["## Manual Review Items", ""])
if manual_items:
    for item in manual_items:
        lines.append(f"- {item}")
else:
    lines.append("- no immediate manual review items")
lines.append("")

lines.extend(
    [
        "## PR Description Seed",
        "",
        "Use this block when creating threshold update PRs:",
        "",
        "```markdown",
        "### Threshold update scope",
        "- profile: standard",
        "- architectures: linux-x86_64, linux-arm64",
        "- budget rule: single-step <=10%",
        "",
        "### Evidence",
        "- calibration report: <artifact-link>",
        "- threshold change proposal: <artifact-link>",
        "- threshold PR checklist: <artifact-link>",
        "",
        "### Risk & rollback",
        "- rollback if two consecutive dual-arch CI/release gate failures appear after merge",
        "- rollback immediately when availability/gateway_5xx threshold failures newly appear",
        "```",
        "",
    ]
)

proposal_path.write_text("\n".join(lines), encoding="utf-8")
PY

echo "proposal: $PROPOSAL_PATH"
