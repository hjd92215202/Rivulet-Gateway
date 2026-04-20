#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CALIBRATION_PATH=""
PROPOSAL_PATH=""
CHECKLIST_PATH=""
WEEKLY_PATH=""
CLOSURE_PATH=""
CI_STREAK_PATH=""
RELEASE_STREAK_PATH=""
OUTPUT_DIR=""
PYTHON_BIN=""

REPORT_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-nightly-review-package.sh [options]

Build a single M2 nightly review package from ordered evidence artifacts.

Options:
  --calibration-report <path>      calibration-report.json path (required)
  --threshold-proposal <path>      threshold-change-proposal.md path (required)
  --threshold-checklist <path>     threshold-pr-checklist.md path (required)
  --closure-weekly <path>          closure-weekly-report.md path (required)
  --closure-status <path>          milestone2-closure-status.json path (required)
  --ci-streak <path>               streak-report.json for ci (required)
  --release-streak <path>          streak-report.json for release (required)
  --output-dir <path>              default: ./target/public-api-nightly-review/<timestamp>
  -h, --help

Outputs:
  m2-nightly-review-package.md
EOF
}

fail() {
  echo "public api nightly review package failed: $1" >&2
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
    --threshold-proposal)
      PROPOSAL_PATH="$2"
      shift 2
      ;;
    --threshold-checklist)
      CHECKLIST_PATH="$2"
      shift 2
      ;;
    --closure-weekly)
      WEEKLY_PATH="$2"
      shift 2
      ;;
    --closure-status)
      CLOSURE_PATH="$2"
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
[[ -n "$PROPOSAL_PATH" ]] || fail "--threshold-proposal is required"
[[ -n "$CHECKLIST_PATH" ]] || fail "--threshold-checklist is required"
[[ -n "$WEEKLY_PATH" ]] || fail "--closure-weekly is required"
[[ -n "$CLOSURE_PATH" ]] || fail "--closure-status is required"
[[ -n "$CI_STREAK_PATH" ]] || fail "--ci-streak is required"
[[ -n "$RELEASE_STREAK_PATH" ]] || fail "--release-streak is required"

[[ -f "$CALIBRATION_PATH" ]] || fail "calibration report not found: $CALIBRATION_PATH"
[[ -f "$PROPOSAL_PATH" ]] || fail "threshold proposal not found: $PROPOSAL_PATH"
[[ -f "$CHECKLIST_PATH" ]] || fail "threshold checklist not found: $CHECKLIST_PATH"
[[ -f "$WEEKLY_PATH" ]] || fail "closure weekly report not found: $WEEKLY_PATH"
[[ -f "$CLOSURE_PATH" ]] || fail "closure status not found: $CLOSURE_PATH"
[[ -f "$CI_STREAK_PATH" ]] || fail "ci streak report not found: $CI_STREAK_PATH"
[[ -f "$RELEASE_STREAK_PATH" ]] || fail "release streak report not found: $RELEASE_STREAK_PATH"

CALIBRATION_PATH="$(normalize_path "$CALIBRATION_PATH")"
PROPOSAL_PATH="$(normalize_path "$PROPOSAL_PATH")"
CHECKLIST_PATH="$(normalize_path "$CHECKLIST_PATH")"
WEEKLY_PATH="$(normalize_path "$WEEKLY_PATH")"
CLOSURE_PATH="$(normalize_path "$CLOSURE_PATH")"
CI_STREAK_PATH="$(normalize_path "$CI_STREAK_PATH")"
RELEASE_STREAK_PATH="$(normalize_path "$RELEASE_STREAK_PATH")"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-nightly-review/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUTPUT_DIR"
REPORT_PATH="$OUTPUT_DIR/m2-nightly-review-package.md"

resolve_python_bin
"$PYTHON_BIN" - "$CALIBRATION_PATH" "$PROPOSAL_PATH" "$CHECKLIST_PATH" "$WEEKLY_PATH" "$CLOSURE_PATH" "$CI_STREAK_PATH" "$RELEASE_STREAK_PATH" "$REPORT_PATH" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

calibration_path = Path(sys.argv[1])
proposal_path = Path(sys.argv[2])
checklist_path = Path(sys.argv[3])
weekly_path = Path(sys.argv[4])
closure_path = Path(sys.argv[5])
ci_streak_path = Path(sys.argv[6])
release_streak_path = Path(sys.argv[7])
report_path = Path(sys.argv[8])

try:
    calibration = json.loads(calibration_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid calibration-report.json: {calibration_path} ({exc})")

try:
    closure = json.loads(closure_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid milestone2-closure-status.json: {closure_path} ({exc})")

try:
    ci_streak = json.loads(ci_streak_path.read_text(encoding="utf-8-sig"))
    release_streak = json.loads(release_streak_path.read_text(encoding="utf-8-sig"))
except Exception as exc:
    raise SystemExit(f"invalid streak-report.json: {exc}")

proposal_text = proposal_path.read_text(encoding="utf-8-sig", errors="replace")
checklist_text = checklist_path.read_text(encoding="utf-8-sig", errors="replace")
weekly_text = weekly_path.read_text(encoding="utf-8-sig", errors="replace")

ready_for_threshold_pr = "ready_for_threshold_pr: `true`" in weekly_text.lower()

ci_eligible = int(ci_streak.get("eligible_runs", 0))
ci_ineligible = int(ci_streak.get("ineligible_runs", 0))
release_eligible = int(release_streak.get("eligible_runs", 0))
release_ineligible = int(release_streak.get("ineligible_runs", 0))

latest_release_eligible = None
for item in release_streak.get("runs", []):
    if bool(item.get("eligible_for_streak", False)):
        latest_release_eligible = item
        break

ineligible_reasons = closure.get("recent_ineligible_reasons", [])
rollback_recommended = bool(closure.get("rollback_recommended", False))

lines = [
    "# M2 Nightly Review Package / M2 夜间审阅包",
    "",
    "## Snapshot / 快照",
    "",
    f"- generated_at: `{datetime.now(timezone.utc).isoformat()}`",
    f"- calibration_generated_at: `{calibration.get('generated_at', 'unknown')}`",
    f"- closure_generated_at: `{closure.get('generated_at', 'unknown')}`",
    f"- ci_closure_ready: `{closure.get('ci_closure_ready', False)}`",
    f"- release_closure_ready: `{closure.get('release_closure_ready', False)}`",
    f"- overall_closure_ready: `{closure.get('overall_closure_ready', False)}`",
    f"- remaining_to_target: `{closure.get('remaining_to_target', 0)}`",
    f"- rollback_recommended: `{rollback_recommended}`",
    f"- ready_for_threshold_pr: `{str(ready_for_threshold_pr).lower()}`",
    "",
    "## Streak Sample Quality / 连绿样本质量",
    "",
    f"- ci_eligible_runs: `{ci_eligible}`",
    f"- ci_ignored_ineligible_runs: `{ci_ineligible}`",
    f"- release_eligible_runs: `{release_eligible}`",
    f"- release_ignored_ineligible_runs: `{release_ineligible}`",
    f"- latest_release_eligible_run_id: `{latest_release_eligible.get('run_id') if latest_release_eligible else 'none'}`",
    f"- latest_release_eligible_created_at: `{latest_release_eligible.get('created_at') if latest_release_eligible else 'none'}`",
    f"- ineligible_reason_categories: `{len(ineligible_reasons)}`",
    "",
]

if ineligible_reasons:
    lines.extend(
        [
            "| ineligible reason | count |",
            "| --- | ---: |",
        ]
    )
    for item in ineligible_reasons:
        lines.append(f"| `{item.get('reason', 'unknown')}` | {item.get('count', 0)} |")
    lines.append("")

lines.extend(
    [
        "## Review Order (Mandatory) / 固定审阅顺序（必须按序）",
        "",
        "1. `calibration-report.json`",
        "2. `threshold-change-proposal.md`",
        "3. `threshold-pr-checklist.md`",
        "4. `closure-weekly-report.md`",
        "5. `milestone2-closure-status.json`",
        "",
        "## Decision Rules / 决策规则",
        "",
        "- If `ready_for_threshold_pr` is false, do not open threshold PR; continue observe-only sampling.",
        "- 若 `ready_for_threshold_pr` 为 false，禁止发起阈值 PR，仅继续 observe 采样。",
        "- If threshold PR is opened, scope must be only `PUBLIC_API_STANDARD_<ARCH>_*` and single-step <=10%.",
        "- 若发起阈值 PR，范围必须仅限 `PUBLIC_API_STANDARD_<ARCH>_*` 且单步变更 <=10%。",
        "- If `rollback_recommended` is true, rollback latest threshold PR first and pause new threshold changes.",
        "- 若 `rollback_recommended` 为 true，先回滚最近阈值 PR，再暂停新的阈值调整。",
        "",
        "## Evidence Pointers / 证据入口",
        "",
        f"- calibration: `{calibration_path}`",
        f"- proposal: `{proposal_path}`",
        f"- checklist: `{checklist_path}`",
        f"- weekly: `{weekly_path}`",
        f"- closure: `{closure_path}`",
        f"- ci_streak: `{ci_streak_path}`",
        f"- release_streak: `{release_streak_path}`",
        "",
        "## Fast Notes / 快速结论",
        "",
        f"- proposal_has_recommendation_text: `{'recommended' in proposal_text.lower()}`",
        f"- checklist_has_rollback_section: `{'rollback' in checklist_text.lower()}`",
        f"- weekly_has_maturity_section: `{'maturity' in weekly_text.lower()}`",
        "",
    ]
)

report_path.write_text("\n".join(lines), encoding="utf-8")
PY

echo "review_package: $REPORT_PATH"
