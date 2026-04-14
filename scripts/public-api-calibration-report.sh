#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

PROFILE="standard"
THRESHOLD_FILE="$REPO_ROOT/scripts/public-api-thresholds.env"
OUTPUT_DIR=""
TREND_SIZE="10"
INPUTS=()
PYTHON_BIN=""

RESULT_PATH=""
SUMMARY_PATH=""

usage() {
  cat <<'EOF'
Usage: bash ./scripts/public-api-calibration-report.sh [options]

Build architecture-aware calibration report from public-api-gate result.json files.

Options:
  --inputs <path...>                    one or more evaluate result.json paths
  --profile <standard|strict|observe>   default: standard
  --threshold-file <path>               default: ./scripts/public-api-thresholds.env
  --trend-size <N>                      default: 10
  --output-dir <path>                   default: ./target/public-api-calibration/<timestamp>-<profile>
  -h, --help

Outputs:
  calibration-report.json
  calibration-report.md
EOF
}

fail() {
  echo "public api calibration report failed: $1" >&2
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
    --inputs)
      shift
      while [[ $# -gt 0 && "$1" != --* ]]; do
        INPUTS+=("$1")
        shift
      done
      ;;
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    --threshold-file)
      THRESHOLD_FILE="$2"
      shift 2
      ;;
    --trend-size)
      TREND_SIZE="$2"
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

case "$PROFILE" in
  standard|strict|observe)
    ;;
  *)
    fail "unsupported profile: $PROFILE"
    ;;
esac

[[ "$TREND_SIZE" =~ ^[0-9]+$ ]] || fail "--trend-size must be a positive integer"
[[ "$TREND_SIZE" -gt 0 ]] || fail "--trend-size must be greater than 0"
[[ -f "$THRESHOLD_FILE" ]] || fail "threshold file not found: $THRESHOLD_FILE"
[[ "${#INPUTS[@]}" -gt 0 ]] || fail "--inputs requires at least one path"

for i in "${!INPUTS[@]}"; do
  [[ -f "${INPUTS[$i]}" ]] || fail "input file not found: ${INPUTS[$i]}"
  INPUTS[$i]="$(normalize_path "${INPUTS[$i]}")"
done

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$REPO_ROOT/target/public-api-calibration/$(date +%Y%m%d-%H%M%S)-$PROFILE"
fi
mkdir -p "$OUTPUT_DIR"
RESULT_PATH="$OUTPUT_DIR/calibration-report.json"
SUMMARY_PATH="$OUTPUT_DIR/calibration-report.md"

resolve_python_bin
"$PYTHON_BIN" - "$PROFILE" "$THRESHOLD_FILE" "$TREND_SIZE" "$RESULT_PATH" "$SUMMARY_PATH" "${INPUTS[@]}" <<'PY'
import json
import math
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path

profile = sys.argv[1]
threshold_file = Path(sys.argv[2])
trend_size = int(sys.argv[3])
result_path = Path(sys.argv[4])
summary_path = Path(sys.argv[5])
input_paths = [Path(item) for item in sys.argv[6:]]

metric_keys = ["availability", "gateway_5xx_ratio", "p95_ms", "p99_ms"]
arches = ["linux-x86_64", "linux-arm64"]
arch_to_key = {"linux-x86_64": "LINUX_X86_64", "linux-arm64": "LINUX_ARM64"}
profile_key = profile.upper()


def percentile(values, ratio):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil((len(ordered) - 1) * ratio)))
    return float(ordered[index])


def summarize(values):
    if not values:
        return {
            "count": 0,
            "min": 0.0,
            "median": 0.0,
            "p90": 0.0,
            "p95": 0.0,
            "max": 0.0,
        }
    return {
        "count": len(values),
        "min": round(float(min(values)), 6),
        "median": round(float(statistics.median(values)), 6),
        "p90": round(percentile(values, 0.90), 6),
        "p95": round(percentile(values, 0.95), 6),
        "max": round(float(max(values)), 6),
    }


def parse_thresholds(path):
    thresholds = {}
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = [part.strip() for part in line.split("=", 1)]
        thresholds[key] = float(value)
    return thresholds


def clamp(value, lower, upper):
    return max(lower, min(upper, value))


def bounded_step_recommendation(current, raw_target, down_ratio=0.10, up_ratio=0.10, floor_value=0.0):
    max_step_down = current * down_ratio
    max_step_up = current * up_ratio
    lower_bound = max(floor_value, current - max_step_down)
    upper_bound = current + max_step_up
    recommended = clamp(raw_target, lower_bound, upper_bound)
    return (
        round(recommended, 6),
        {
            "max_step_down": round(max_step_down, 6),
            "max_step_up": round(max_step_up, 6),
            "lower_bound": round(lower_bound, 6),
            "upper_bound": round(upper_bound, 6),
            "raw_target": round(raw_target, 6),
        },
    )


thresholds_raw = parse_thresholds(threshold_file)
thresholds = {}
for arch in arches:
    suffix = arch_to_key[arch]
    prefix = f"PUBLIC_API_{profile_key}_{suffix}_"
    thresholds[arch] = {
        "availability_min": thresholds_raw[f"{prefix}SLO_AVAILABILITY"],
        "gateway_5xx_ratio_max": thresholds_raw[f"{prefix}GATEWAY_5XX_RATIO_MAX"],
        "p95_ms_max": thresholds_raw[f"{prefix}P95_MS"],
        "p99_ms_max": thresholds_raw[f"{prefix}P99_MS"],
    }

records = {arch: [] for arch in arches}
for path in input_paths:
    try:
        payload = json.loads(path.read_text(encoding="utf-8-sig"))
    except Exception as exc:
        raise SystemExit(f"invalid json input: {path} ({exc})")

    arch = payload.get("arch")
    if arch not in records:
        raise SystemExit(f"unsupported arch in input: {path} ({arch})")

    observed = payload.get("observed", {})
    missing = [key for key in metric_keys if key not in observed]
    if missing:
        raise SystemExit(f"input missing observed metrics {missing}: {path}")

    mtime = datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc)
    records[arch].append(
        {
            "path": str(path),
            "timestamp": mtime.isoformat(),
            "availability": float(observed["availability"]),
            "gateway_5xx_ratio": float(observed["gateway_5xx_ratio"]),
            "p95_ms": float(observed["p95_ms"]),
            "p99_ms": float(observed["p99_ms"]),
            "pass": bool(payload.get("pass", False)),
        }
    )

analysis = {}
review_items = []
recommended_thresholds = {}
change_budget = {}

for arch in arches:
    arch_records = sorted(records[arch], key=lambda item: item["timestamp"], reverse=True)
    metric_summary = {metric: summarize([item[metric] for item in arch_records]) for metric in metric_keys}
    threshold = thresholds[arch]
    delta = {
        "availability_vs_min_median": round(metric_summary["availability"]["median"] - threshold["availability_min"], 6),
        "gateway_5xx_ratio_vs_max_median": round(threshold["gateway_5xx_ratio_max"] - metric_summary["gateway_5xx_ratio"]["median"], 6),
        "p95_ms_vs_max_median": round(threshold["p95_ms_max"] - metric_summary["p95_ms"]["median"], 6),
        "p99_ms_vs_max_median": round(threshold["p99_ms_max"] - metric_summary["p99_ms"]["median"], 6),
    }

    arch_review = []
    arch_recommended = dict(threshold)
    arch_budget = {}
    if metric_summary["availability"]["count"] == 0:
        arch_review.append("no samples available for this architecture")
        arch_budget["availability_min"] = {
            "direction": "up-only",
            "max_step_down": 0.0,
            "max_step_up": 0.0,
            "lower_bound": round(threshold["availability_min"], 6),
            "upper_bound": round(threshold["availability_min"], 6),
            "raw_target": round(threshold["availability_min"], 6),
            "applied_delta": 0.0,
        }
        for key in ("gateway_5xx_ratio_max", "p95_ms_max", "p99_ms_max"):
            arch_budget[key] = {
                "direction": "two-way",
                "max_step_down": 0.0,
                "max_step_up": 0.0,
                "lower_bound": round(threshold[key], 6),
                "upper_bound": round(threshold[key], 6),
                "raw_target": round(threshold[key], 6),
                "applied_delta": 0.0,
            }
    else:
        if metric_summary["availability"]["min"] < threshold["availability_min"]:
            arch_review.append("availability minimum fell below threshold in at least one sample")
        if metric_summary["gateway_5xx_ratio"]["max"] > threshold["gateway_5xx_ratio_max"]:
            arch_review.append("gateway 5xx ratio exceeded threshold in at least one sample")
        if metric_summary["p95_ms"]["max"] > threshold["p95_ms_max"]:
            arch_review.append("p95 latency exceeded threshold in at least one sample")
        if metric_summary["p99_ms"]["max"] > threshold["p99_ms_max"]:
            arch_review.append("p99 latency exceeded threshold in at least one sample")

        # 中文注释：我们用“阈值 10% 以内余量”作为人工评审触发线，避免还没越线就失控。
        if delta["availability_vs_min_median"] < 0.05:
            arch_review.append("availability median headroom is narrow (<0.05)")
        if delta["gateway_5xx_ratio_vs_max_median"] < max(0.01, threshold["gateway_5xx_ratio_max"] * 0.1):
            arch_review.append("gateway 5xx median headroom is narrow (<10% threshold)")
        if delta["p95_ms_vs_max_median"] < threshold["p95_ms_max"] * 0.1:
            arch_review.append("p95 median headroom is narrow (<10% threshold)")
        if delta["p99_ms_vs_max_median"] < threshold["p99_ms_max"] * 0.1:
            arch_review.append("p99 median headroom is narrow (<10% threshold)")

        availability_values = [item["availability"] for item in arch_records]
        availability_margin = max(0.05, (100.0 - threshold["availability_min"]) * 0.1)
        consecutive_above = 0
        for item in arch_records:
            if item["availability"] >= threshold["availability_min"] + availability_margin:
                consecutive_above += 1
            else:
                break

        availability_budget_up = max(0.001, (100.0 - threshold["availability_min"]) * 0.1)
        availability_raw_target = threshold["availability_min"]
        if consecutive_above >= min(5, len(availability_values)):
            p10_candidate = percentile(availability_values, 0.10) - 0.01
            availability_raw_target = max(threshold["availability_min"], p10_candidate)
        availability_recommended = min(100.0, threshold["availability_min"] + availability_budget_up)
        availability_recommended = min(availability_recommended, availability_raw_target)
        availability_recommended = round(max(threshold["availability_min"], availability_recommended), 6)
        arch_recommended["availability_min"] = availability_recommended
        arch_budget["availability_min"] = {
            "direction": "up-only",
            "max_step_down": 0.0,
            "max_step_up": round(availability_budget_up, 6),
            "lower_bound": round(threshold["availability_min"], 6),
            "upper_bound": round(min(100.0, threshold["availability_min"] + availability_budget_up), 6),
            "raw_target": round(availability_raw_target, 6),
            "applied_delta": round(availability_recommended - threshold["availability_min"], 6),
            "consecutive_above_margin": consecutive_above,
            "significant_margin": round(availability_margin, 6),
        }

        ratio_values = [item["gateway_5xx_ratio"] for item in arch_records]
        ratio_raw = percentile(ratio_values, 0.95) + max(0.01, percentile(ratio_values, 0.95) * 0.2)
        ratio_recommended, ratio_budget = bounded_step_recommendation(
            threshold["gateway_5xx_ratio_max"],
            ratio_raw,
            down_ratio=0.10,
            up_ratio=0.10,
            floor_value=0.0,
        )
        arch_recommended["gateway_5xx_ratio_max"] = ratio_recommended
        arch_budget["gateway_5xx_ratio_max"] = {
            "direction": "two-way",
            **ratio_budget,
            "applied_delta": round(ratio_recommended - threshold["gateway_5xx_ratio_max"], 6),
        }

        p95_values = [item["p95_ms"] for item in arch_records]
        p95_raw = percentile(p95_values, 0.95) * 1.1 + 2.0
        p95_recommended, p95_budget = bounded_step_recommendation(
            threshold["p95_ms_max"],
            p95_raw,
            down_ratio=0.10,
            up_ratio=0.10,
            floor_value=1.0,
        )
        arch_recommended["p95_ms_max"] = p95_recommended
        arch_budget["p95_ms_max"] = {
            "direction": "two-way",
            **p95_budget,
            "applied_delta": round(p95_recommended - threshold["p95_ms_max"], 6),
        }

        p99_values = [item["p99_ms"] for item in arch_records]
        p99_raw = percentile(p99_values, 0.95) * 1.1 + 5.0
        p99_recommended, p99_budget = bounded_step_recommendation(
            threshold["p99_ms_max"],
            p99_raw,
            down_ratio=0.10,
            up_ratio=0.10,
            floor_value=1.0,
        )
        arch_recommended["p99_ms_max"] = p99_recommended
        arch_budget["p99_ms_max"] = {
            "direction": "two-way",
            **p99_budget,
            "applied_delta": round(p99_recommended - threshold["p99_ms_max"], 6),
        }

    if len(arch_records) < 10:
        arch_review.append("sample count is below 10; keep collecting nightly observe evidence")

    changed_metrics = [
        key for key in arch_recommended.keys()
        if round(arch_recommended[key] - threshold[key], 6) != 0
    ]
    if changed_metrics:
        arch_review.append(
            "conservative recommendations differ from current thresholds: "
            + ", ".join(sorted(changed_metrics))
        )

    for item in arch_review:
        review_items.append(f"[{arch}] {item}")

    recommended_thresholds[arch] = arch_recommended
    change_budget[arch] = arch_budget
    analysis[arch] = {
        "sample_count": len(arch_records),
        # 向后兼容：保留旧字段，避免已有消费方直接读取失败。
        "thresholds_standard": threshold,
        "thresholds_current_profile": threshold,
        "recommended_thresholds": arch_recommended,
        "change_budget": arch_budget,
        "distribution": metric_summary,
        "delta_vs_standard": delta,
        "trend_latest": arch_records[:trend_size],
        "manual_review_items": arch_review,
    }

report = {
    "profile": profile,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "input_count": len(input_paths),
    "trend_size": trend_size,
    "analysis": analysis,
    "recommended_thresholds": recommended_thresholds,
    "change_budget": change_budget,
    "recommended_review_items": review_items,
}

result_path.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

lines = [
    "# Public API Calibration Report",
    "",
    "## Inputs",
    "",
    f"- profile: `{profile}`",
    f"- generated_at: `{report['generated_at']}`",
    f"- input_count: `{len(input_paths)}`",
    f"- trend_size: `{trend_size}`",
    "",
    "## Summary",
    "",
]

for arch in arches:
    block = analysis[arch]
    lines.append(f"### {arch}")
    lines.append("")
    lines.append(f"- sample_count: `{block['sample_count']}`")
    lines.append(f"- availability median vs min: `{block['delta_vs_standard']['availability_vs_min_median']}`")
    lines.append(f"- gateway_5xx median headroom: `{block['delta_vs_standard']['gateway_5xx_ratio_vs_max_median']}`")
    lines.append(f"- p95 median headroom(ms): `{block['delta_vs_standard']['p95_ms_vs_max_median']}`")
    lines.append(f"- p99 median headroom(ms): `{block['delta_vs_standard']['p99_ms_vs_max_median']}`")
    lines.append("")
    lines.append("Recommended thresholds (conservative, budget-limited):")
    lines.append("")
    lines.append("| metric | current | recommended | delta | max_step_down | max_step_up |")
    lines.append("| --- | ---: | ---: | ---: | ---: | ---: |")
    for metric, current_key in [
        ("availability_min", "availability_min"),
        ("gateway_5xx_ratio_max", "gateway_5xx_ratio_max"),
        ("p95_ms_max", "p95_ms_max"),
        ("p99_ms_max", "p99_ms_max"),
    ]:
        budget = block["change_budget"][current_key]
        recommended = block["recommended_thresholds"][current_key]
        current = block["thresholds_current_profile"][current_key]
        delta_value = round(recommended - current, 6)
        lines.append(
            f"| {metric} | {current} | {recommended} | {delta_value} | {budget['max_step_down']} | {budget['max_step_up']} |"
        )
    lines.append("")
    lines.append("| metric | min | median | p90 | p95 | max |")
    lines.append("| --- | ---: | ---: | ---: | ---: | ---: |")
    for metric in metric_keys:
        dist = block["distribution"][metric]
        lines.append(
            f"| {metric} | {dist['min']} | {dist['median']} | {dist['p90']} | {dist['p95']} | {dist['max']} |"
        )
    lines.append("")
    lines.append("Latest trend samples:")
    lines.append("")
    lines.append("| timestamp(UTC) | availability | gateway_5xx_ratio | p95_ms | p99_ms | pass | file |")
    lines.append("| --- | ---: | ---: | ---: | ---: | --- | --- |")
    for item in block["trend_latest"]:
        lines.append(
            f"| {item['timestamp']} | {item['availability']} | {item['gateway_5xx_ratio']} | {item['p95_ms']} | {item['p99_ms']} | {item['pass']} | `{item['path']}` |"
        )
    if not block["trend_latest"]:
        lines.append("| n/a | n/a | n/a | n/a | n/a | n/a | n/a |")
    lines.append("")

lines.extend(["## Manual Review Items", ""])
if review_items:
    for item in review_items:
        lines.append(f"- {item}")
else:
    lines.append("- no immediate manual review items")

summary_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"
