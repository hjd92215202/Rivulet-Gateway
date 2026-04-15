#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CI_STREAK_PATH=""
RELEASE_STREAK_PATH=""
CALIBRATION_PATH=""
OUTPUT_DIR=""
PYTHON_BIN=""

RESULT_PATH=""
SUMMARY_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-milestone2-closure-status.sh [options]

Synthesize milestone-2 closure status from CI/release streak reports and calibration report.

Options:
  --ci-streak <path>            ci streak-report.json path (required)
  --release-streak <path>       release streak-report.json path (required)
  --calibration-report <path>   calibration-report.json path (required)
  --output-dir <path>           default: ./target/public-api-milestone2-closure/<timestamp>
  -h, --help

Outputs:
  milestone2-closure-status.json
  milestone2-closure-status.md
EOF
}

fail() {
  echo "public api milestone2 closure status failed: $1" >&2
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
    --ci-streak)
      CI_STREAK_PATH="$2"
      shift 2
      ;;
    --release-streak)
      RELEASE_STREAK_PATH="$2"
      shift 2
      ;;
    --calibration-report)
      CALIBRATION_PATH="$2"
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

[[ -n "$CI_STREAK_PATH" ]] || fail "--ci-streak is required"
[[ -n "$RELEASE_STREAK_PATH" ]] || fail "--release-streak is required"
[[ -n "$CALIBRATION_PATH" ]] || fail "--calibration-report is required"
[[ -f "$CI_STREAK_PATH" ]] || fail "ci streak report not found: $CI_STREAK_PATH"
[[ -f "$RELEASE_STREAK_PATH" ]] || fail "release streak report not found: $RELEASE_STREAK_PATH"
[[ -f "$CALIBRATION_PATH" ]] || fail "calibration report not found: $CALIBRATION_PATH"

CI_STREAK_PATH="$(normalize_path "$CI_STREAK_PATH")"
RELEASE_STREAK_PATH="$(normalize_path "$RELEASE_STREAK_PATH")"
CALIBRATION_PATH="$(normalize_path "$CALIBRATION_PATH")"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-milestone2-closure/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
RESULT_PATH="$OUTPUT_DIR/milestone2-closure-status.json"
SUMMARY_PATH="$OUTPUT_DIR/milestone2-closure-status.md"

resolve_python_bin
"$PYTHON_BIN" - "$CI_STREAK_PATH" "$RELEASE_STREAK_PATH" "$CALIBRATION_PATH" "$RESULT_PATH" "$SUMMARY_PATH" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

ci_path = Path(sys.argv[1])
release_path = Path(sys.argv[2])
calibration_path = Path(sys.argv[3])
result_path = Path(sys.argv[4])
summary_path = Path(sys.argv[5])


def load_json(path: Path, label: str):
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except Exception as exc:
        raise SystemExit(f"invalid {label} json: {path} ({exc})")


def parse_streak(payload: dict, workflow_name: str):
    consecutive = int(payload.get("consecutive_dual_arch_success", 0))
    target = int(payload.get("closure_target", 10))
    remaining = int(payload.get("remaining_to_target", max(0, target - consecutive)))
    ready = bool(payload.get("closure_ready", consecutive >= target))
    return {
        "workflow": workflow_name,
        "closure_target": target,
        "consecutive_dual_arch_success": consecutive,
        "closure_ready": ready,
        "remaining_to_target": remaining,
        "inspected_runs": int(payload.get("inspected_runs", 0)),
        "generated_at": payload.get("generated_at"),
        "runs": payload.get("runs", []),
    }


def aggregate_failure_reasons(workflow_block: dict):
    counts = {}
    for run in workflow_block.get("runs", []):
        if run.get("dual_arch_success", False):
            continue

        run_reasons = []
        required_jobs = run.get("required_jobs", {})
        for job_name, job_state in required_jobs.items():
            present = bool(job_state.get("present", False))
            conclusion = str(job_state.get("conclusion") or "unknown")
            if not present:
                run_reasons.append(f"{workflow_block['workflow']}/job_missing/{job_name}")
            elif conclusion != "success":
                run_reasons.append(f"{workflow_block['workflow']}/job_failed/{job_name}/{conclusion}")

        run_conclusion = str(run.get("conclusion") or "unknown")
        if run_conclusion != "success":
            run_reasons.append(f"{workflow_block['workflow']}/run_conclusion/{run_conclusion}")

        if not run_reasons:
            run_reasons.append(f"{workflow_block['workflow']}/dual_arch_not_success")

        for reason in run_reasons:
            counts[reason] = counts.get(reason, 0) + 1

    return [
        {"reason": reason, "count": count}
        for reason, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    ]


ci_payload = load_json(ci_path, "ci streak report")
release_payload = load_json(release_path, "release streak report")
calibration_payload = load_json(calibration_path, "calibration report")

ci_block = parse_streak(ci_payload, "ci")
release_block = parse_streak(release_payload, "release")

analysis = calibration_payload.get("analysis", {})
recommended_thresholds = calibration_payload.get("recommended_thresholds", {})
manual_items = calibration_payload.get("recommended_review_items", [])

recommendation_count = 0
for arch, current in recommended_thresholds.items():
    block = analysis.get(arch, {})
    baseline = block.get("thresholds_current_profile") or block.get("thresholds_standard") or {}
    for metric, value in current.items():
        if float(value) != float(baseline.get(metric, value)):
            recommendation_count += 1

ci_closure_ready = ci_block["closure_ready"]
release_closure_ready = release_block["closure_ready"]
overall_ready = ci_closure_ready and release_closure_ready
remaining = max(ci_block["remaining_to_target"], release_block["remaining_to_target"])
recent_failure_reasons = aggregate_failure_reasons(ci_block) + aggregate_failure_reasons(release_block)

status = {
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "milestone": "Milestone 2",
    "ci": ci_block,
    "release": release_block,
    "ci_closure_ready": ci_closure_ready,
    "release_closure_ready": release_closure_ready,
    "calibration": {
        "profile": calibration_payload.get("profile", "unknown"),
        "generated_at": calibration_payload.get("generated_at"),
        "input_count": int(calibration_payload.get("input_count", 0)),
        "manual_review_item_count": len(manual_items),
        "recommended_threshold_change_count": recommendation_count,
    },
    "overall_closure_ready": overall_ready,
    "remaining_to_target": remaining,
    "recent_failure_reasons": recent_failure_reasons,
    "next_action": (
        "promote Milestone 2 to completed and move roadmap focus to Milestone 3"
        if overall_ready
        else "continue nightly observe collection and manual threshold review"
    ),
}

result_path.write_text(json.dumps(status, ensure_ascii=False, indent=2), encoding="utf-8")

lines = [
    "# Milestone 2 Closure Status / Milestone 2 收口状态",
    "",
    "## Overview / 概览",
    "",
    f"- generated_at: `{status['generated_at']}`",
    f"- ci_closure_ready: `{status['ci_closure_ready']}`",
    f"- release_closure_ready: `{status['release_closure_ready']}`",
    f"- overall_closure_ready: `{status['overall_closure_ready']}`",
    f"- remaining_to_target: `{status['remaining_to_target']}`",
    f"- next_action: `{status['next_action']}`",
    "",
    "## Streak Details / 连绿详情",
    "",
    "| workflow | closure_ready | consecutive_dual_arch_success | closure_target | remaining_to_target | inspected_runs |",
    "| --- | --- | ---: | ---: | ---: | ---: |",
    f"| ci | {ci_block['closure_ready']} | {ci_block['consecutive_dual_arch_success']} | {ci_block['closure_target']} | {ci_block['remaining_to_target']} | {ci_block['inspected_runs']} |",
    f"| release | {release_block['closure_ready']} | {release_block['consecutive_dual_arch_success']} | {release_block['closure_target']} | {release_block['remaining_to_target']} | {release_block['inspected_runs']} |",
    "",
    "## Calibration Snapshot / 校准快照",
    "",
    f"- profile: `{status['calibration']['profile']}`",
    f"- input_count: `{status['calibration']['input_count']}`",
    f"- recommended_threshold_change_count: `{status['calibration']['recommended_threshold_change_count']}`",
    f"- manual_review_item_count: `{status['calibration']['manual_review_item_count']}`",
    "",
]

if manual_items:
    lines.append("Manual review highlights / 人工复核重点:")
    for item in manual_items[:12]:
        lines.append(f"- {item}")
else:
    lines.append("Manual review highlights / 人工复核重点:")
    lines.append("- no immediate manual review items")

lines.append("")
lines.append("## Recent Failure Reasons / 最近失败原因聚合")
lines.append("")
if recent_failure_reasons:
    lines.append("| reason | count |")
    lines.append("| --- | ---: |")
    for item in recent_failure_reasons:
        lines.append(f"| `{item['reason']}` | {item['count']} |")
else:
    lines.append("- no recent failure reasons were observed in inspected streak runs")

lines.append("")
summary_path.write_text("\n".join(lines), encoding="utf-8")
PY

echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"
