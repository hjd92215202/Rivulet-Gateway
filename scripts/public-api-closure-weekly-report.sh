#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CALIBRATION_PATH=""
CI_STREAK_PATH=""
RELEASE_STREAK_PATH=""
CLOSURE_STATUS_PATH=""
WINDOW="10"
OUTPUT_DIR=""
PYTHON_BIN=""

REPORT_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-closure-weekly-report.sh [options]

Build weekly milestone-2 closure report from calibration and streak artifacts.

Options:
  --calibration-report <path>     calibration-report.json path (required)
  --ci-streak <path>              ci streak-report.json path (required)
  --release-streak <path>         release streak-report.json path (required)
  --closure-status <path>         milestone2-closure-status.json path (required)
  --window <N>                    latest samples/runs to summarize, default: 10
  --output-dir <path>             default: ./target/public-api-closure-weekly/<timestamp>
  -h, --help

Outputs:
  closure-weekly-report.md
EOF
}

fail() {
  echo "public api closure weekly report failed: $1" >&2
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
    --calibration-report)
      CALIBRATION_PATH="$2"
      shift 2
      ;;
    --ci-streak)
      CI_STREAK_PATH="$2"
      shift 2
      ;;
    --release-streak)
      RELEASE_STREAK_PATH="$2"
      shift 2
      ;;
    --closure-status)
      CLOSURE_STATUS_PATH="$2"
      shift 2
      ;;
    --window)
      WINDOW="$2"
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

[[ -n "$CALIBRATION_PATH" ]] || fail "--calibration-report is required"
[[ -n "$CI_STREAK_PATH" ]] || fail "--ci-streak is required"
[[ -n "$RELEASE_STREAK_PATH" ]] || fail "--release-streak is required"
[[ -n "$CLOSURE_STATUS_PATH" ]] || fail "--closure-status is required"
[[ "$WINDOW" =~ ^[0-9]+$ ]] || fail "--window must be a positive integer"
[[ "$WINDOW" -gt 0 ]] || fail "--window must be greater than 0"

[[ -f "$CALIBRATION_PATH" ]] || fail "calibration report not found: $CALIBRATION_PATH"
[[ -f "$CI_STREAK_PATH" ]] || fail "ci streak report not found: $CI_STREAK_PATH"
[[ -f "$RELEASE_STREAK_PATH" ]] || fail "release streak report not found: $RELEASE_STREAK_PATH"
[[ -f "$CLOSURE_STATUS_PATH" ]] || fail "closure status report not found: $CLOSURE_STATUS_PATH"

CALIBRATION_PATH="$(normalize_path "$CALIBRATION_PATH")"
CI_STREAK_PATH="$(normalize_path "$CI_STREAK_PATH")"
RELEASE_STREAK_PATH="$(normalize_path "$RELEASE_STREAK_PATH")"
CLOSURE_STATUS_PATH="$(normalize_path "$CLOSURE_STATUS_PATH")"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-closure-weekly/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
REPORT_PATH="$OUTPUT_DIR/closure-weekly-report.md"

resolve_python_bin
"$PYTHON_BIN" - "$CALIBRATION_PATH" "$CI_STREAK_PATH" "$RELEASE_STREAK_PATH" "$CLOSURE_STATUS_PATH" "$WINDOW" "$REPORT_PATH" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

calibration_path = Path(sys.argv[1])
ci_streak_path = Path(sys.argv[2])
release_streak_path = Path(sys.argv[3])
closure_status_path = Path(sys.argv[4])
window = int(sys.argv[5])
report_path = Path(sys.argv[6])


def load_json(path: Path, label: str):
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except Exception as exc:
        raise SystemExit(f"invalid {label} json: {path} ({exc})")


def run_success_rate(runs, size):
    selected = runs[:size]
    if not selected:
        return 0.0, 0, 0
    success = sum(1 for item in selected if bool(item.get("dual_arch_success", False)))
    return (success / len(selected)) * 100.0, success, len(selected)


def evidence_maturity(analysis, size):
    reasons = []
    ready = True
    for arch in ("linux-x86_64", "linux-arm64"):
        block = analysis.get(arch, {})
        sample_count = int(block.get("sample_count", 0))
        trend = block.get("trend_latest", [])
        availability = (block.get("distribution", {}).get("availability", {}))
        gateway_ratio = (block.get("distribution", {}).get("gateway_5xx_ratio", {}))

        if sample_count < size:
            ready = False
            reasons.append(f"{arch}: sample_count({sample_count}) < window({size})")

        if len(trend) < min(size, sample_count):
            ready = False
            reasons.append(f"{arch}: trend_latest count insufficient for window")

        avail_spread = float(availability.get("max", 0.0)) - float(availability.get("min", 0.0))
        ratio_spread = float(gateway_ratio.get("max", 0.0)) - float(gateway_ratio.get("min", 0.0))
        if avail_spread > 0.2:
            ready = False
            reasons.append(f"{arch}: availability spread too wide ({avail_spread:.6f})")
        if ratio_spread > 0.2:
            ready = False
            reasons.append(f"{arch}: gateway_5xx_ratio spread too wide ({ratio_spread:.6f})")

    return ready, reasons


calibration = load_json(calibration_path, "calibration report")
ci_streak = load_json(ci_streak_path, "ci streak report")
release_streak = load_json(release_streak_path, "release streak report")
closure_status = load_json(closure_status_path, "closure status report")

analysis = calibration.get("analysis", {})
manual_items = calibration.get("recommended_review_items", [])
ci_runs = ci_streak.get("runs", [])
release_runs = release_streak.get("runs", [])

ci_rate, ci_success, ci_total = run_success_rate(ci_runs, window)
release_rate, release_success, release_total = run_success_rate(release_runs, window)

ready_for_threshold_pr, maturity_reasons = evidence_maturity(analysis, window)
recent_failure_reasons = closure_status.get("recent_failure_reasons", [])

lines = [
    "# Milestone 2 Closure Weekly Report / Milestone 2 收口周报",
    "",
    "## Snapshot / 快照",
    "",
    f"- report_generated_at: `{datetime.now(timezone.utc).isoformat()}`",
    f"- calibration_generated_at: `{calibration.get('generated_at', 'unknown')}`",
    f"- closure_status_generated_at: `{closure_status.get('generated_at', 'unknown')}`",
    f"- window: `{window}`",
    "",
    f"- ci_closure_ready: `{closure_status.get('ci_closure_ready', False)}`",
    f"- release_closure_ready: `{closure_status.get('release_closure_ready', False)}`",
    f"- overall_closure_ready: `{closure_status.get('overall_closure_ready', False)}`",
    f"- remaining_to_target: `{closure_status.get('remaining_to_target', 0)}`",
    "",
    "## Recent Gate Trend / 最近 Gate 趋势",
    "",
    f"- CI dual-arch success (last {ci_total}): `{ci_success}/{ci_total}` (`{ci_rate:.2f}%`)",
    f"- Release dual-arch success (last {release_total}): `{release_success}/{release_total}` (`{release_rate:.2f}%`)",
    "",
]

for arch in ("linux-x86_64", "linux-arm64"):
    block = analysis.get(arch, {})
    distribution = block.get("distribution", {})
    lines.append(f"## Observe Signal: {arch}")
    lines.append("")
    lines.append(f"- sample_count: `{block.get('sample_count', 0)}`")
    for metric in ("availability", "gateway_5xx_ratio", "p95_ms", "p99_ms"):
        dist = distribution.get(metric, {})
        lines.append(
            f"- {metric}: min=`{dist.get('min', 0.0)}` median=`{dist.get('median', 0.0)}` "
            f"p95=`{dist.get('p95', 0.0)}` max=`{dist.get('max', 0.0)}`"
        )
    lines.append("")

lines.extend(
    [
        "## Threshold PR Evidence Maturity / 阈值 PR 证据成熟度",
        "",
        f"- ready_for_threshold_pr: `{ready_for_threshold_pr}`",
        f"- calibration_manual_review_items: `{len(manual_items)}`",
    ]
)
if maturity_reasons:
    lines.append("- maturity blockers:")
    for reason in maturity_reasons:
        lines.append(f"  - {reason}")
else:
    lines.append("- maturity blockers: none")
lines.append("")

lines.append("## Recent Failure Reason Aggregation / 最近失败原因聚合")
lines.append("")
if recent_failure_reasons:
    lines.append("| reason | count |")
    lines.append("| --- | ---: |")
    for item in recent_failure_reasons:
        lines.append(f"| `{item.get('reason', 'unknown')}` | {item.get('count', 0)} |")
else:
    lines.append("- no recent failure reasons in closure synthesis window")
lines.append("")

lines.append("## Next Actions / 下一步动作")
lines.append("")
if closure_status.get("overall_closure_ready", False):
    lines.append("- Milestone 2 closure target is met; switch mainline to Milestone 3 hardening batch.")
else:
    lines.append("- Continue nightly observe sampling and keep CI/release standard dual-blocking unchanged.")
if ready_for_threshold_pr:
    lines.append("- Evidence is mature enough to open a human-reviewed threshold adjustment PR (if needed).")
else:
    lines.append("- Keep collecting observe samples until maturity blockers are cleared.")
lines.append("")

report_path.write_text("\n".join(lines), encoding="utf-8")
PY

echo "report: $REPORT_PATH"
