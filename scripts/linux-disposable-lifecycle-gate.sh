#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/lib/linux-bootstrap.sh"

MODE="execute"
ARCH=""
FORMAT="tar.gz"
LIFECYCLE_MODE="full"
FROM_ARTIFACT=""
FROM_TAG=""
TO_ARTIFACT=""
TO_TAG=""
REPO_SLUG="hjd92215202/Rivulet-Gateway"
HOST_HEADER="localhost"
GATEWAY_PORT="18580"
BACKEND_PORT="19580"
LABEL="disposable-lifecycle"
WORK_DIR=""

RESULT_PATH=""
SUMMARY_PATH=""
POSTINSTALL_WORK_DIR=""
UPGRADE_WORK_DIR=""
UNINSTALL_WORK_DIR=""

POSTINSTALL_STATUS="not-run"
POSTINSTALL_SECONDS="0"
POSTINSTALL_REASON=""
POSTINSTALL_SUMMARY=""
UPGRADE_ROLLBACK_STATUS="not-run"
UPGRADE_ROLLBACK_SECONDS="0"
UPGRADE_ROLLBACK_REASON=""
UPGRADE_ROLLBACK_SUMMARY=""
UNINSTALL_STATUS="not-run"
UNINSTALL_SECONDS="0"
UNINSTALL_REASON=""
UNINSTALL_SUMMARY=""
CLEANUP_STATUS="not-run"
CLEANUP_SECONDS="0"
CLEANUP_REASON=""
OVERALL_STATUS="failed"
FAILURE_STAGE=""

usage() {
  cat <<'EOF'
usage: bash ./scripts/linux-disposable-lifecycle-gate.sh [source options] [runtime options]

source options:
  --from-artifact <path>        local starting artifact path (.tar.gz or .rpm)
  --from-tag <tag>              github release tag for the starting version
  --to-artifact <path>          local target artifact path (.tar.gz or .rpm)
  --to-tag <tag>                github release tag for the target version
  --repo <owner/name>           github repository slug, default: hjd92215202/Rivulet-Gateway
  --format <tar.gz|rpm>         release asset format, default: tar.gz

runtime options:
  --mode <execute|contract-check>          default: execute
  --lifecycle-mode <full|install-uninstall> default: full
  --arch <linux-x86_64|linux-arm64>        optional, defaults to detected host architecture
  --host <host>                            host header matched by generated route, default: localhost
  --gateway-port <port>                    base gateway port, default: 18580
  --backend-port <port>                    base backend port, default: 19580
  --label <label>                          work label, default: disposable-lifecycle
  --work-dir <dir>                         working directory, default: ./target/linux-disposable-lifecycle/<timestamp>-<label>

notes:
  - supports Linux x86_64 and Linux arm64
  - auto-installs runtime dependencies when apt-get, dnf, or yum is available
  - writes result.json and summary.md with stage-level pass/fail diagnostics
  - exits automatically after stage execution and cleanup

examples:
  bash ./scripts/linux-disposable-lifecycle-gate.sh \
    --from-tag v0.1.7 \
    --to-artifact ./dist/x86_64-unknown-linux-gnu/rivulet-gateway-0.1.8-linux-x86_64.tar.gz \
    --format tar.gz \
    --arch linux-x86_64 \
    --host localhost

  bash ./scripts/linux-disposable-lifecycle-gate.sh \
    --mode execute \
    --lifecycle-mode install-uninstall \
    --to-artifact ./dist/rpmbuild/x86_64-unknown-linux-gnu/RPMS/x86_64/rivulet-gateway-0.1.8-1.x86_64.rpm \
    --format rpm
EOF
}

fail() {
  echo "linux disposable lifecycle gate failed: $1" >&2
  exit 1
}

normalize_arch() {
  case "$1" in
    linux-x86_64|x86_64|amd64)
      printf '%s\n' "linux-x86_64"
      ;;
    linux-arm64|aarch64|arm64)
      printf '%s\n' "linux-arm64"
      ;;
    *)
      fail "unsupported arch value: $1"
      ;;
  esac
}

resolve_arch() {
  local detected=""
  detected="$(detect_linux_asset_arch)"
  if [[ -z "$ARCH" ]]; then
    ARCH="$detected"
    return 0
  fi

  ARCH="$(normalize_arch "$ARCH")"
  if [[ "$ARCH" != "$detected" ]]; then
    fail "requested arch=$ARCH does not match host architecture=$detected"
  fi
}

resolve_work_layout() {
  local timestamp=""
  if [[ -z "$WORK_DIR" ]]; then
    timestamp="$(date +%Y%m%d-%H%M%S)"
    WORK_DIR="$REPO_ROOT/target/linux-disposable-lifecycle/$timestamp-$LABEL"
  fi

  POSTINSTALL_WORK_DIR="$WORK_DIR/postinstall"
  UPGRADE_WORK_DIR="$WORK_DIR/upgrade-rollback"
  UNINSTALL_WORK_DIR="$WORK_DIR/uninstall"
  RESULT_PATH="$WORK_DIR/result.json"
  SUMMARY_PATH="$WORK_DIR/summary.md"

  mkdir -p "$WORK_DIR" "$POSTINSTALL_WORK_DIR" "$UPGRADE_WORK_DIR" "$UNINSTALL_WORK_DIR"
}

write_reports() {
  python3 - "$RESULT_PATH" "$SUMMARY_PATH" \
    "$MODE" "$LIFECYCLE_MODE" "$ARCH" "$FORMAT" "$HOST_HEADER" \
    "$FROM_TAG" "$FROM_ARTIFACT" "$TO_TAG" "$TO_ARTIFACT" \
    "$OVERALL_STATUS" "$FAILURE_STAGE" \
    "$POSTINSTALL_STATUS" "$POSTINSTALL_SECONDS" "$POSTINSTALL_REASON" "$POSTINSTALL_SUMMARY" \
    "$UPGRADE_ROLLBACK_STATUS" "$UPGRADE_ROLLBACK_SECONDS" "$UPGRADE_ROLLBACK_REASON" "$UPGRADE_ROLLBACK_SUMMARY" \
    "$UNINSTALL_STATUS" "$UNINSTALL_SECONDS" "$UNINSTALL_REASON" "$UNINSTALL_SUMMARY" \
    "$CLEANUP_STATUS" "$CLEANUP_SECONDS" "$CLEANUP_REASON" <<'PY'
import json
import sys
from datetime import datetime, timezone

(
    result_path,
    summary_path,
    mode,
    lifecycle_mode,
    arch,
    fmt,
    host_header,
    from_tag,
    from_artifact,
    to_tag,
    to_artifact,
    overall_status,
    failure_stage,
    postinstall_status,
    postinstall_seconds,
    postinstall_reason,
    postinstall_summary,
    upgrade_status,
    upgrade_seconds,
    upgrade_reason,
    upgrade_summary,
    uninstall_status,
    uninstall_seconds,
    uninstall_reason,
    uninstall_summary,
    cleanup_status,
    cleanup_seconds,
    cleanup_reason,
) = sys.argv[1:]

result = {
    "mode": mode,
    "lifecycle_mode": lifecycle_mode,
    "arch": arch,
    "format": fmt,
    "host": host_header,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "sources": {
        "from_tag": from_tag or None,
        "from_artifact": from_artifact or None,
        "to_tag": to_tag or None,
        "to_artifact": to_artifact or None,
    },
    "stages": {
        "postinstall": {
            "status": postinstall_status,
            "seconds": int(postinstall_seconds),
            "reason": postinstall_reason or "",
            "summary_path": postinstall_summary or "",
        },
        "upgrade_rollback": {
            "status": upgrade_status,
            "seconds": int(upgrade_seconds),
            "reason": upgrade_reason or "",
            "summary_path": upgrade_summary or "",
        },
        "uninstall": {
            "status": uninstall_status,
            "seconds": int(uninstall_seconds),
            "reason": uninstall_reason or "",
            "summary_path": uninstall_summary or "",
        },
        "cleanup": {
            "status": cleanup_status,
            "seconds": int(cleanup_seconds),
            "reason": cleanup_reason or "",
        },
    },
    "failure_stage": failure_stage or "",
    "pass": overall_status == "passed",
}

with open(result_path, "w", encoding="utf-8") as fp:
    json.dump(result, fp, ensure_ascii=False, indent=2)

with open(summary_path, "w", encoding="utf-8") as fp:
    fp.write("# Linux Disposable Lifecycle Gate Summary\n\n")
    fp.write("## Inputs\n\n")
    fp.write(f"- mode: `{mode}`\n")
    fp.write(f"- lifecycle_mode: `{lifecycle_mode}`\n")
    fp.write(f"- arch: `{arch}`\n")
    fp.write(f"- format: `{fmt}`\n")
    fp.write(f"- host: `{host_header}`\n")
    fp.write(f"- from_tag: `{from_tag or 'n/a'}`\n")
    fp.write(f"- from_artifact: `{from_artifact or 'n/a'}`\n")
    fp.write(f"- to_tag: `{to_tag or 'n/a'}`\n")
    fp.write(f"- to_artifact: `{to_artifact or 'n/a'}`\n\n")

    fp.write("## Stage Results\n\n")
    fp.write(f"- postinstall: `{postinstall_status}` ({postinstall_seconds}s)\n")
    fp.write(f"- postinstall summary: `{postinstall_summary or 'n/a'}`\n")
    if postinstall_reason:
        fp.write(f"- postinstall reason: `{postinstall_reason}`\n")

    fp.write(f"- upgrade_rollback: `{upgrade_status}` ({upgrade_seconds}s)\n")
    fp.write(f"- upgrade_rollback summary: `{upgrade_summary or 'n/a'}`\n")
    if upgrade_reason:
        fp.write(f"- upgrade_rollback reason: `{upgrade_reason}`\n")

    fp.write(f"- uninstall: `{uninstall_status}` ({uninstall_seconds}s)\n")
    fp.write(f"- uninstall summary: `{uninstall_summary or 'n/a'}`\n")
    if uninstall_reason:
        fp.write(f"- uninstall reason: `{uninstall_reason}`\n")
    fp.write(f"- cleanup: `{cleanup_status}` ({cleanup_seconds}s)\n")
    if cleanup_reason:
        fp.write(f"- cleanup reason: `{cleanup_reason}`\n")

    fp.write("\n## Final Result\n\n")
    fp.write(f"- status: `{overall_status}`\n")
    fp.write(f"- failure_stage: `{failure_stage or 'none'}`\n")
    fp.write(f"- result_json: `{result_path}`\n")
PY
}

cleanup_best_effort() {
  # 在 execute 模式下，无论前面阶段是否失败，都尝试做一次无害卸载收尾，
  # 避免把半安装状态遗留在验证机，影响下一轮门禁复现。
  if [[ "$MODE" != "execute" ]]; then
    CLEANUP_STATUS="skipped"
    CLEANUP_REASON="contract_check_mode"
  elif [[ "$UNINSTALL_STATUS" == "passed" ]]; then
    CLEANUP_STATUS="not-needed"
    CLEANUP_REASON=""
  else
    local start_ts=""
    local end_ts=""
    local rc=""
    start_ts="$(date +%s)"
    set +e
    bash "$REPO_ROOT/scripts/linux-service-remove.sh" \
      --purge-config \
      --label "$LABEL-cleanup" \
      --work-dir "$WORK_DIR/cleanup" >/dev/null 2>&1
    rc="$?"
    set -e
    end_ts="$(date +%s)"

    CLEANUP_SECONDS="$((end_ts - start_ts))"
    if [[ "$rc" -eq 0 ]]; then
      CLEANUP_STATUS="passed"
      CLEANUP_REASON=""
    else
      CLEANUP_STATUS="failed"
      CLEANUP_REASON="cleanup_exit_${rc}"
    fi
  fi

  if [[ -n "$RESULT_PATH" && -n "$SUMMARY_PATH" ]]; then
    write_reports || true
  fi
}
trap cleanup_best_effort EXIT

run_stage_command() {
  local stage_name="$1"
  shift

  local start_ts=""
  local end_ts=""
  local rc=""
  start_ts="$(date +%s)"
  set +e
  "$@"
  rc="$?"
  set -e
  end_ts="$(date +%s)"

  case "$stage_name" in
    postinstall)
      POSTINSTALL_SECONDS="$((end_ts - start_ts))"
      if [[ "$rc" -eq 0 ]]; then
        POSTINSTALL_STATUS="passed"
        POSTINSTALL_REASON=""
      else
        POSTINSTALL_STATUS="failed"
        POSTINSTALL_REASON="command_exit_${rc}"
      fi
      ;;
    upgrade_rollback)
      UPGRADE_ROLLBACK_SECONDS="$((end_ts - start_ts))"
      if [[ "$rc" -eq 0 ]]; then
        UPGRADE_ROLLBACK_STATUS="passed"
        UPGRADE_ROLLBACK_REASON=""
      else
        UPGRADE_ROLLBACK_STATUS="failed"
        UPGRADE_ROLLBACK_REASON="command_exit_${rc}"
      fi
      ;;
    uninstall)
      UNINSTALL_SECONDS="$((end_ts - start_ts))"
      if [[ "$rc" -eq 0 ]]; then
        UNINSTALL_STATUS="passed"
        UNINSTALL_REASON=""
      else
        UNINSTALL_STATUS="failed"
        UNINSTALL_REASON="command_exit_${rc}"
      fi
      ;;
    *)
      fail "unknown stage name: $stage_name"
      ;;
  esac

  return "$rc"
}

resolve_artifact_path() {
  local artifact="$1"
  printf '%s/%s\n' "$(cd "$(dirname "$artifact")" && pwd)" "$(basename "$artifact")"
}

validate_required_inputs() {
  case "$MODE" in
    execute|contract-check)
      ;;
    *)
      fail "unsupported mode: $MODE"
      ;;
  esac

  case "$FORMAT" in
    tar.gz|rpm)
      ;;
    *)
      fail "unsupported format: $FORMAT"
      ;;
  esac

  case "$LIFECYCLE_MODE" in
    full|install-uninstall)
      ;;
    *)
      fail "unsupported lifecycle mode: $LIFECYCLE_MODE"
      ;;
  esac

  if [[ "$MODE" == "contract-check" ]]; then
    return 0
  fi

  if [[ -z "$TO_ARTIFACT" && -z "$TO_TAG" ]]; then
    fail "either --to-artifact or --to-tag is required in execute mode"
  fi

  if [[ "$LIFECYCLE_MODE" == "full" && -z "$FROM_ARTIFACT" && -z "$FROM_TAG" ]]; then
    fail "full lifecycle mode requires --from-artifact or --from-tag"
  fi

  if [[ -n "$TO_ARTIFACT" ]]; then
    [[ -f "$TO_ARTIFACT" ]] || fail "to-artifact not found: $TO_ARTIFACT"
    TO_ARTIFACT="$(resolve_artifact_path "$TO_ARTIFACT")"
  fi
  if [[ -n "$FROM_ARTIFACT" ]]; then
    [[ -f "$FROM_ARTIFACT" ]] || fail "from-artifact not found: $FROM_ARTIFACT"
    FROM_ARTIFACT="$(resolve_artifact_path "$FROM_ARTIFACT")"
  fi
}

run_contract_check_mode() {
  print_stage "running disposable lifecycle contract-check mode"
  POSTINSTALL_STATUS="passed"
  POSTINSTALL_REASON="contract-check"
  POSTINSTALL_SUMMARY="not-generated-in-contract-check"
  POSTINSTALL_SECONDS="0"

  if [[ "$LIFECYCLE_MODE" == "full" ]]; then
    UPGRADE_ROLLBACK_STATUS="passed"
    UPGRADE_ROLLBACK_REASON="contract-check"
    UPGRADE_ROLLBACK_SUMMARY="not-generated-in-contract-check"
    UPGRADE_ROLLBACK_SECONDS="0"
  else
    UPGRADE_ROLLBACK_STATUS="skipped"
    UPGRADE_ROLLBACK_REASON="lifecycle_mode_install-uninstall"
    UPGRADE_ROLLBACK_SUMMARY="not-applicable"
    UPGRADE_ROLLBACK_SECONDS="0"
  fi

  UNINSTALL_STATUS="passed"
  UNINSTALL_REASON="contract-check"
  UNINSTALL_SUMMARY="not-generated-in-contract-check"
  UNINSTALL_SECONDS="0"
  CLEANUP_STATUS="skipped"
  CLEANUP_REASON="contract_check_mode"
  CLEANUP_SECONDS="0"
  OVERALL_STATUS="passed"
}

run_execute_mode() {
  local postinstall_gateway_port="$GATEWAY_PORT"
  local postinstall_backend_port="$BACKEND_PORT"
  local upgrade_gateway_port="$((GATEWAY_PORT + 100))"
  local upgrade_backend_port="$((BACKEND_PORT + 100))"

  local postinstall_cmd=(
    bash "$REPO_ROOT/scripts/linux-postinstall-validate.sh"
    --format "$FORMAT"
    --host "$HOST_HEADER"
    --gateway-port "$postinstall_gateway_port"
    --backend-port "$postinstall_backend_port"
    --cleanup keep
    --label "$LABEL-postinstall"
    --work-dir "$POSTINSTALL_WORK_DIR"
  )
  if [[ -n "$TO_ARTIFACT" ]]; then
    postinstall_cmd+=(--artifact "$TO_ARTIFACT")
  else
    postinstall_cmd+=(--tag "$TO_TAG" --repo "$REPO_SLUG")
  fi

  print_stage "running postinstall validation stage"
  if ! run_stage_command postinstall "${postinstall_cmd[@]}"; then
    FAILURE_STAGE="postinstall"
    return 1
  fi
  POSTINSTALL_SUMMARY="$POSTINSTALL_WORK_DIR/summary.md"

  if [[ "$LIFECYCLE_MODE" == "full" ]]; then
    local upgrade_cmd=(
      bash "$REPO_ROOT/scripts/linux-upgrade-rollback-validate.sh"
      --format "$FORMAT"
      --host "$HOST_HEADER"
      --gateway-port "$upgrade_gateway_port"
      --backend-port "$upgrade_backend_port"
      --label "$LABEL-upgrade-rollback"
      --work-dir "$UPGRADE_WORK_DIR"
    )
    if [[ -n "$FROM_ARTIFACT" ]]; then
      upgrade_cmd+=(--from-artifact "$FROM_ARTIFACT")
    else
      upgrade_cmd+=(--from-tag "$FROM_TAG" --repo "$REPO_SLUG")
    fi
    if [[ -n "$TO_ARTIFACT" ]]; then
      upgrade_cmd+=(--to-artifact "$TO_ARTIFACT")
    else
      upgrade_cmd+=(--to-tag "$TO_TAG" --repo "$REPO_SLUG")
    fi

    print_stage "running upgrade-rollback validation stage"
    if ! run_stage_command upgrade_rollback "${upgrade_cmd[@]}"; then
      FAILURE_STAGE="upgrade_rollback"
      return 1
    fi
    UPGRADE_ROLLBACK_SUMMARY="$UPGRADE_WORK_DIR/summary.md"
  else
    UPGRADE_ROLLBACK_STATUS="skipped"
    UPGRADE_ROLLBACK_SECONDS="0"
    UPGRADE_ROLLBACK_REASON="lifecycle_mode_install-uninstall"
    UPGRADE_ROLLBACK_SUMMARY="not-applicable"
  fi

  local uninstall_cmd=(
    bash "$REPO_ROOT/scripts/linux-service-remove.sh"
    --purge-config
    --label "$LABEL-uninstall"
    --work-dir "$UNINSTALL_WORK_DIR"
  )

  print_stage "running uninstall validation stage"
  if ! run_stage_command uninstall "${uninstall_cmd[@]}"; then
    FAILURE_STAGE="uninstall"
    return 1
  fi
  UNINSTALL_SUMMARY="$UNINSTALL_WORK_DIR/summary.md"
  CLEANUP_STATUS="not-needed"
  CLEANUP_REASON=""
  CLEANUP_SECONDS="0"

  OVERALL_STATUS="passed"
  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="$2"
      shift 2
      ;;
    --lifecycle-mode)
      LIFECYCLE_MODE="$2"
      shift 2
      ;;
    --arch)
      ARCH="$2"
      shift 2
      ;;
    --format)
      FORMAT="$2"
      shift 2
      ;;
    --from-artifact)
      FROM_ARTIFACT="$2"
      shift 2
      ;;
    --from-tag)
      FROM_TAG="$2"
      shift 2
      ;;
    --to-artifact)
      TO_ARTIFACT="$2"
      shift 2
      ;;
    --to-tag)
      TO_TAG="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --host)
      HOST_HEADER="$2"
      shift 2
      ;;
    --gateway-port)
      GATEWAY_PORT="$2"
      shift 2
      ;;
    --backend-port)
      BACKEND_PORT="$2"
      shift 2
      ;;
    --label)
      LABEL="$2"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="$2"
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

resolve_arch
resolve_work_layout
validate_required_inputs

if [[ "$MODE" == "contract-check" ]]; then
  run_contract_check_mode
else
  ensure_linux_commands bash python3
  if ! run_execute_mode; then
    OVERALL_STATUS="failed"
    fail "stage failure detected at $FAILURE_STAGE"
  fi
fi

print_stage "linux disposable lifecycle gate completed"
echo "result: $RESULT_PATH"
echo "summary: $SUMMARY_PATH"
