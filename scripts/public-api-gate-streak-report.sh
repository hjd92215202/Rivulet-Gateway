#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

REPO_SLUG="hjd92215202/Rivulet-Gateway"
WORKFLOW_NAME=""
WINDOW="40"
OUTPUT_DIR=""
TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
PYTHON_BIN=""

RESULT_PATH=""
SUMMARY_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-gate-streak-report.sh [options]

Compute consecutive dual-architecture public-api gate pass streak from GitHub Actions runs.

Options:
  --repo <owner/name>                 default: hjd92215202/Rivulet-Gateway
  --workflow <ci|release>             required
  --window <N>                        eligible sample window, default: 40
  --output-dir <path>                 default: ./target/public-api-streak/<timestamp>-<workflow>
  --token <github-token>              optional, defaults to GH_TOKEN or GITHUB_TOKEN
  -h, --help

Outputs:
  streak-report.json
  streak-report.md
EOF
}

fail() {
  echo "public api gate streak report failed: $1" >&2
  exit 1
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
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --workflow)
      WORKFLOW_NAME="$2"
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
    --token)
      TOKEN="$2"
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

case "$WORKFLOW_NAME" in
  ci|release)
    ;;
  *)
    fail "--workflow must be ci or release"
    ;;
esac

[[ "$WINDOW" =~ ^[0-9]+$ ]] || fail "--window must be a positive integer"
[[ "$WINDOW" -gt 0 ]] || fail "--window must be greater than 0"
[[ -n "$TOKEN" ]] || fail "missing GitHub token; provide --token or set GH_TOKEN/GITHUB_TOKEN"

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-streak/$(date +%Y%m%d-%H%M%S)-$WORKFLOW_NAME"
fi
mkdir -p "$OUTPUT_DIR"
RESULT_PATH="$OUTPUT_DIR/streak-report.json"
SUMMARY_PATH="$OUTPUT_DIR/streak-report.md"

resolve_python_bin
"$PYTHON_BIN" - "$REPO_SLUG" "$WORKFLOW_NAME" "$WINDOW" "$TOKEN" "$RESULT_PATH" "$SUMMARY_PATH" <<'PY'
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone

repo = sys.argv[1]
workflow = sys.argv[2]
window = int(sys.argv[3])
token = sys.argv[4]
result_path = sys.argv[5]
summary_path = sys.argv[6]

workflow_file = {
    "ci": "ci.yml",
    "release": "release.yml",
}[workflow]

required_jobs = {
    "ci": ["Public API Gate Linux x86_64", "Public API Gate Linux arm64"],
    "release": ["Release Public API Gate Linux x86_64", "Release Public API Gate Linux arm64"],
}[workflow]

terminal_failures = {"failure", "cancelled", "timed_out", "action_required"}
max_fetch_runs = max(window * 12, 120)


def api_get(url: str) -> dict:
    request = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "rivulet-public-api-streak-report",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = response.read().decode("utf-8")
            return json.loads(payload)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        if exc.code == 403 and "rate limit" in body.lower():
            raise SystemExit("github api rate limit exceeded while building streak report")
        raise SystemExit(f"github api request failed: status={exc.code} body={body}")
    except urllib.error.URLError as exc:
        raise SystemExit(f"github api request failed: {exc}")


def fetch_runs(max_items: int):
    runs = []
    page = 1
    per_page = 100
    while len(runs) < max_items:
        query = urllib.parse.urlencode(
            {
                "status": "completed",
                "per_page": per_page,
                "page": page,
            }
        )
        url = f"https://api.github.com/repos/{repo}/actions/workflows/{workflow_file}/runs?{query}"
        payload = api_get(url)
        page_runs = payload.get("workflow_runs", [])
        if not page_runs:
            break
        runs.extend(page_runs)
        if len(page_runs) < per_page:
            break
        page += 1
    return runs[:max_items]


def fetch_jobs(run_id: int):
    query = urllib.parse.urlencode({"per_page": 100})
    url = f"https://api.github.com/repos/{repo}/actions/runs/{run_id}/jobs?{query}"
    payload = api_get(url)
    return payload.get("jobs", [])


def evaluate_run(run: dict):
    run_id = int(run["id"])
    run_conclusion = str(run.get("conclusion") or "")
    jobs = fetch_jobs(run_id)
    job_map = {job.get("name", ""): job for job in jobs}
    required = {}
    missing_jobs = []
    local_skip_reasons = []
    dual_pass = True
    eligible_for_streak = True
    streak_impact = "counted_success"

    for job_name in required_jobs:
        job = job_map.get(job_name)
        if job is None:
            required[job_name] = {"present": False, "conclusion": "missing"}
            missing_jobs.append(job_name)
            dual_pass = False
            eligible_for_streak = False
            local_skip_reasons.append(f"job_missing/{job_name}")
            continue

        conclusion = str(job.get("conclusion") or "")
        required[job_name] = {"present": True, "conclusion": conclusion}
        if conclusion == "success":
            continue
        dual_pass = False
        if conclusion == "skipped":
            eligible_for_streak = False
            local_skip_reasons.append(f"job_skipped/{job_name}")
        else:
            streak_impact = "counted_failure"
            local_skip_reasons.append(f"job_failed/{job_name}/{conclusion or 'unknown'}")

    if eligible_for_streak and not dual_pass:
        if run_conclusion not in terminal_failures and streak_impact != "counted_failure":
            eligible_for_streak = False
            streak_impact = "ignored_not_eligible"
            local_skip_reasons.append(f"run_not_terminal/{run_conclusion or 'unknown'}")
        else:
            streak_impact = "counted_failure"
    elif not eligible_for_streak:
        streak_impact = "ignored_not_eligible"

    return {
        "run_id": run_id,
        "run_number": run.get("run_number"),
        "status": run.get("status"),
        "conclusion": run_conclusion,
        "created_at": run.get("created_at"),
        "html_url": run.get("html_url"),
        "required_jobs": required,
        "missing_jobs": missing_jobs,
        "dual_arch_success": dual_pass,
        "eligible_for_streak": eligible_for_streak,
        "streak_impact": streak_impact,
        "skip_reasons": local_skip_reasons,
    }


runs = fetch_runs(max_fetch_runs)
if not runs:
    closure_target = 10
    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "repo": repo,
        "workflow": workflow,
        "workflow_file": workflow_file,
        "window": window,
        "closure_target": closure_target,
        "inspected_runs": 0,
        "eligible_runs": 0,
        "ineligible_runs": 0,
        "skip_reasons": [],
        "consecutive_dual_arch_success": 0,
        "closure_ready": False,
        "remaining_to_target": closure_target,
        "target_reached": False,
        "message": "no historical workflow runs found",
        "runs": [],
    }
    with open(result_path, "w", encoding="utf-8") as fp:
        json.dump(report, fp, ensure_ascii=False, indent=2)
    with open(summary_path, "w", encoding="utf-8") as fp:
        fp.write("# Public API Gate Streak Report\n\n")
        fp.write(f"- repo: `{repo}`\n")
        fp.write(f"- workflow: `{workflow}`\n")
        fp.write(f"- closure_target: `{closure_target}`\n")
        fp.write(f"- closure_ready: `False`\n")
        fp.write(f"- remaining_to_target: `{closure_target}`\n")
        fp.write("- no historical workflow runs found\n")
    raise SystemExit(0)

inspected = []
streak = 0
consecutive_eligible_failures = 0
eligible_runs = 0
ineligible_runs = 0
skip_reason_counts = {}
failure_signal_runs = []

success_streak_done = False
failure_streak_done = False

for run in runs:
    item = evaluate_run(run)
    inspected.append(item)

    if not item["eligible_for_streak"]:
        ineligible_runs += 1
        for reason in item["skip_reasons"]:
            skip_reason_counts[reason] = skip_reason_counts.get(reason, 0) + 1
        continue

    eligible_runs += 1
    is_success = bool(item["dual_arch_success"])

    if not success_streak_done:
        if is_success:
            streak += 1
            if streak >= window:
                success_streak_done = True
        else:
            success_streak_done = True

    if not failure_streak_done:
        if is_success:
            failure_streak_done = True
        else:
            consecutive_eligible_failures += 1
            failure_signal_runs.append(
                {
                    "run_id": item["run_id"],
                    "run_number": item.get("run_number"),
                    "conclusion": item.get("conclusion"),
                    "html_url": item.get("html_url"),
                }
            )

    if eligible_runs >= window and success_streak_done and (failure_streak_done or consecutive_eligible_failures >= 2):
        break

rollback_recommended = consecutive_eligible_failures >= 2

report = {
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "repo": repo,
    "workflow": workflow,
    "workflow_file": workflow_file,
    "window": window,
    "closure_target": 10,
    "inspected_runs": len(inspected),
    "eligible_runs": eligible_runs,
    "ineligible_runs": ineligible_runs,
    "skip_reasons": [
        {"reason": reason, "count": count}
        for reason, count in sorted(skip_reason_counts.items(), key=lambda item: (-item[1], item[0]))
    ],
    "consecutive_dual_arch_success": streak,
    "consecutive_eligible_failures": consecutive_eligible_failures,
    "rollback_recommended": rollback_recommended,
    "closure_ready": streak >= 10,
    "remaining_to_target": max(0, 10 - streak),
    "target_reached": streak >= 10,
    "required_jobs": required_jobs,
    "failure_signal_runs": failure_signal_runs,
    "runs": inspected,
}

if eligible_runs < window:
    report["eligible_window_satisfied"] = False
    report["eligible_runs_missing"] = window - eligible_runs
else:
    report["eligible_window_satisfied"] = True
    report["eligible_runs_missing"] = 0

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(report, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Streak Report\n\n")
    fp.write("## Overview\n\n")
    fp.write(f"- repo: `{repo}`\n")
    fp.write(f"- workflow: `{workflow}`\n")
    fp.write(f"- eligible_window_target: `{window}`\n")
    fp.write(f"- consecutive_dual_arch_success: `{streak}`\n")
    fp.write("- closure_target: `10`\n")
    fp.write(f"- eligible_runs: `{eligible_runs}`\n")
    fp.write(f"- ineligible_runs: `{ineligible_runs}`\n")
    fp.write(f"- eligible_window_satisfied: `{report['eligible_window_satisfied']}`\n")
    fp.write(f"- eligible_runs_missing: `{report['eligible_runs_missing']}`\n")
    fp.write(f"- consecutive_eligible_failures: `{consecutive_eligible_failures}`\n")
    fp.write(f"- rollback_recommended: `{rollback_recommended}`\n")
    fp.write(f"- closure_ready: `{streak >= 10}`\n")
    fp.write(f"- remaining_to_target: `{max(0, 10 - streak)}`\n")
    fp.write(f"- target_reached: `{streak >= 10}`\n\n")
    if skip_reason_counts:
        fp.write("## Ineligible Reasons\n\n")
        fp.write("| reason | count |\n")
        fp.write("| --- | ---: |\n")
        for reason, count in sorted(skip_reason_counts.items(), key=lambda item: (-item[1], item[0])):
            fp.write(f"| `{reason}` | {count} |\n")
        fp.write("\n")
    fp.write("## Required Jobs\n\n")
    for name in required_jobs:
        fp.write(f"- `{name}`\n")
    fp.write("\n## Inspected Runs\n\n")
    fp.write("| run_number | run_id | created_at | eligible_for_streak | streak_impact | dual_arch_success | ci/release conclusion | url |\n")
    fp.write("| ---: | ---: | --- | --- | --- | --- | --- | --- |\n")
    for item in inspected:
        fp.write(
            f"| {item.get('run_number')} | {item['run_id']} | {item.get('created_at')} | {item['eligible_for_streak']} | {item['streak_impact']} | {item['dual_arch_success']} | {item.get('conclusion')} | {item.get('html_url')} |\n"
        )
    if not inspected:
        fp.write("| n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |\n")

PY

echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"
