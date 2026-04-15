#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
THRESHOLD_FILE="scripts/public-api-thresholds.env"
MAX_STEP_RATIO="0.10"

REQUIRED_DOCS=(
  "docs/LINUX-BASELINE-BENCHMARK.md"
  "docs/PRODUCTION-READINESS.md"
  "docs/MILESTONES.md"
  "docs/RELEASE-PROCESS.md"
  "docs/RELEASE-CHECKLIST.md"
)

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-threshold-policy-check.sh

Enforce threshold tuning policy for pull requests and pushes.

Policy:
  - only PUBLIC_API_STANDARD_<ARCH>_* keys can be changed
  - each key step change must stay within 10% budget
  - when effective threshold values change, required docs must be updated
  - on pull_request, PR body must include nightly evidence links
  - on pull_request, PR body must explicitly declare ready_for_threshold_pr=true
EOF
}

fail() {
  echo "public api threshold policy check failed: $1" >&2
  exit 1
}

resolve_base_rev() {
  local base_ref=""
  local resolved=""

  if [[ "${GITHUB_EVENT_NAME:-}" == "pull_request" && -n "${GITHUB_BASE_REF:-}" ]]; then
    base_ref="origin/${GITHUB_BASE_REF}"
    git fetch --no-tags --depth=200 origin "${GITHUB_BASE_REF}" >/dev/null 2>&1 || true
    if git rev-parse --verify -q "$base_ref" >/dev/null 2>&1; then
      resolved="$(git merge-base HEAD "$base_ref")"
    fi
  fi

  if [[ -z "$resolved" && -n "${GITHUB_EVENT_BEFORE:-}" ]]; then
    if git rev-parse --verify -q "${GITHUB_EVENT_BEFORE}" >/dev/null 2>&1; then
      resolved="${GITHUB_EVENT_BEFORE}"
    fi
  fi

  if [[ -z "$resolved" ]] && git rev-parse --verify -q HEAD~1 >/dev/null 2>&1; then
    resolved="$(git rev-parse HEAD~1)"
  fi

  printf '%s\n' "$resolved"
}

require_docs_changed() {
  local changed_files="$1"
  local doc_path=""
  local missing=()

  for doc_path in "${REQUIRED_DOCS[@]}"; do
    if ! grep -Fxq "$doc_path" <<<"$changed_files"; then
      missing+=("$doc_path")
    fi
  done

  if [[ "${#missing[@]}" -gt 0 ]]; then
    fail "threshold values changed but required docs were not updated: ${missing[*]}"
  fi
}

check_pr_body_evidence() {
  if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" || -z "${GITHUB_EVENT_PATH:-}" ]]; then
    return 0
  fi

  python3 - "$GITHUB_EVENT_PATH" <<'PY'
import json
import re
import sys
from pathlib import Path

event_path = Path(sys.argv[1])
try:
    payload = json.loads(event_path.read_text(encoding="utf-8"))
except Exception as exc:
    raise SystemExit(f"unable to parse pull_request event payload: {exc}")

body = str(payload.get("pull_request", {}).get("body") or "").lower()
required_tokens = [
    "calibration-report",
    "threshold-change-proposal",
    "threshold-pr-checklist",
    "closure-weekly-report",
]
missing = [token for token in required_tokens if token not in body]
if missing:
    raise SystemExit(
        "pull request body is missing nightly evidence links/keywords: "
        + ", ".join(missing)
    )

ready_true_patterns = (
    r"ready_for_threshold_pr\s*[:=]\s*true",
    r"\"ready_for_threshold_pr\"\s*:\s*true",
)

if not any(re.search(pattern, body) for pattern in ready_true_patterns):
    raise SystemExit(
        "pull request body must declare evidence maturity with ready_for_threshold_pr=true"
    )
PY
}

main() {
  local base_rev=""
  local changed_files=""
  local threshold_changed="false"
  local tmp_dir=""
  local base_file=""
  local current_file=""
  local python_output=""

  base_rev="$(resolve_base_rev)"
  if [[ -z "$base_rev" ]]; then
    echo "public-api-threshold-policy: unable to resolve base revision, skipping"
    exit 0
  fi

  changed_files="$(git diff --name-only "$base_rev"...HEAD)"
  if grep -Fxq "$THRESHOLD_FILE" <<<"$changed_files"; then
    threshold_changed="true"
  fi

  if [[ "$threshold_changed" != "true" ]]; then
    echo "public-api-threshold-policy: no threshold file change detected"
    exit 0
  fi

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT
  base_file="$tmp_dir/base.env"
  current_file="$tmp_dir/current.env"

  git show "$base_rev:$THRESHOLD_FILE" >"$base_file" \
    || fail "failed to read $THRESHOLD_FILE from base revision $base_rev"
  cp "$REPO_ROOT/$THRESHOLD_FILE" "$current_file"

  python_output="$(python3 - "$base_file" "$current_file" "$MAX_STEP_RATIO" <<'PY'
import math
import sys
from pathlib import Path

base_file = Path(sys.argv[1])
current_file = Path(sys.argv[2])
max_ratio = float(sys.argv[3])

allowed_prefixes = (
    "PUBLIC_API_STANDARD_LINUX_X86_64_",
    "PUBLIC_API_STANDARD_LINUX_ARM64_",
)

def parse_env(path: Path):
    data = {}
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        data[key.strip()] = value.strip()
    return data

base = parse_env(base_file)
current = parse_env(current_file)

changed = []
for key in sorted(set(base.keys()) | set(current.keys())):
    if base.get(key) != current.get(key):
        changed.append(key)

if not changed:
    print("NO_EFFECTIVE_THRESHOLD_CHANGE")
    raise SystemExit(0)

for key in changed:
    if not key.startswith(allowed_prefixes):
        raise SystemExit(
            "non-standard threshold key change detected: "
            + key
            + " (only PUBLIC_API_STANDARD_<ARCH>_* is allowed)"
        )

for key in changed:
    old = base.get(key)
    new = current.get(key)
    if old is None or new is None:
        raise SystemExit("threshold key add/remove is not allowed in this policy: " + key)
    try:
        old_val = float(old)
        new_val = float(new)
    except ValueError as exc:
        raise SystemExit(f"threshold value must be numeric for {key}: {exc}")

    delta = abs(new_val - old_val)
    if math.isclose(old_val, 0.0, abs_tol=1e-12):
        if not math.isclose(new_val, 0.0, abs_tol=1e-12):
            raise SystemExit(f"threshold step budget exceeded for {key}: base is 0, new={new_val}")
        continue

    allowed = abs(old_val) * max_ratio
    if delta > allowed + 1e-12:
        raise SystemExit(
            f"threshold step budget exceeded for {key}: old={old_val}, new={new_val}, "
            f"delta={delta}, allowed={allowed}"
        )

print("EFFECTIVE_THRESHOLD_CHANGE")
for key in changed:
    print(key)
PY
)"

  if grep -Fxq "NO_EFFECTIVE_THRESHOLD_CHANGE" <<<"$python_output"; then
    echo "public-api-threshold-policy: threshold file changed but no effective key/value delta"
    exit 0
  fi

  require_docs_changed "$changed_files"
  check_pr_body_evidence

  echo "public-api-threshold-policy: threshold changes are policy-compliant"
}

main "$@"
