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
  --window <N>                        number of latest runs to inspect, default: 40
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
    # 中文注释：只看 completed run，避免把 in_progress 噪声当成 streak 中断。
    query = urllib.parse.urlencode({"status": "completed", "per_page": min(100, max_items)})
    url = f"https://api.github.com/repos/{repo}/actions/workflows/{workflow_file}/runs?{query}"
    payload = api_get(url)
    return payload.get("workflow_runs", [])[:max_items]


def fetch_jobs(run_id: int):
    query = urllib.parse.urlencode({"per_page": 100})
    url = f"https://api.github.com/repos/{repo}/actions/runs/{run_id}/jobs?{query}"
    payload = api_get(url)
    return payload.get("jobs", [])


runs = fetch_runs(window)
if not runs:
    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "repo": repo,
        "workflow": workflow,
        "workflow_file": workflow_file,
        "window": window,
        "closure_target": 10,
        "inspected_runs": 0,
        "consecutive_dual_arch_success": 0,
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
        fp.write("- no historical workflow runs found\n")
    raise SystemExit(0)

inspected = []
streak = 0
for run in runs:
    run_id = int(run["id"])
    jobs = fetch_jobs(run_id)
    job_map = {job.get("name", ""): job for job in jobs}
    required = {}
    dual_pass = True
    missing_jobs = []
    for job_name in required_jobs:
        job = job_map.get(job_name)
        if job is None:
            missing_jobs.append(job_name)
            required[job_name] = {"present": False, "conclusion": "missing"}
            dual_pass = False
            continue
        conclusion = str(job.get("conclusion") or "")
        required[job_name] = {"present": True, "conclusion": conclusion}
        if conclusion != "success":
            dual_pass = False

    inspected.append(
        {
            "run_id": run_id,
            "run_number": run.get("run_number"),
            "status": run.get("status"),
            "conclusion": run.get("conclusion"),
            "created_at": run.get("created_at"),
            "html_url": run.get("html_url"),
            "required_jobs": required,
            "missing_jobs": missing_jobs,
            "dual_arch_success": dual_pass,
        }
    )

    if dual_pass:
        streak += 1
    else:
        break

report = {
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "repo": repo,
    "workflow": workflow,
    "workflow_file": workflow_file,
    "window": window,
    "closure_target": 10,
    "inspected_runs": len(inspected),
    "consecutive_dual_arch_success": streak,
    "target_reached": streak >= 10,
    "required_jobs": required_jobs,
    "runs": inspected,
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(report, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Public API Gate Streak Report\n\n")
    fp.write("## Overview\n\n")
    fp.write(f"- repo: `{repo}`\n")
    fp.write(f"- workflow: `{workflow}`\n")
    fp.write(f"- window: `{window}`\n")
    fp.write(f"- consecutive_dual_arch_success: `{streak}`\n")
    fp.write("- closure_target: `10`\n")
    fp.write(f"- target_reached: `{streak >= 10}`\n\n")
    fp.write("## Required Jobs\n\n")
    for name in required_jobs:
        fp.write(f"- `{name}`\n")
    fp.write("\n## Inspected Runs\n\n")
    fp.write("| run_number | run_id | created_at | dual_arch_success | ci/release conclusion | url |\n")
    fp.write("| ---: | ---: | --- | --- | --- | --- |\n")
    for item in inspected:
        fp.write(
            f"| {item.get('run_number')} | {item['run_id']} | {item.get('created_at')} | {item['dual_arch_success']} | {item.get('conclusion')} | {item.get('html_url')} |\n"
        )
    if not inspected:
        fp.write("| n/a | n/a | n/a | n/a | n/a | n/a |\n")

PY

echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"
