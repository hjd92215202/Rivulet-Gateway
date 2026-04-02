#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_PATH="${1:-}"
PORT="${PORT:-18080}"

if [[ -z "$ARTIFACT_PATH" ]]; then
  echo "usage: $0 <artifact-path>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 先做静态包体验证，尽快暴露缺文件、缺 service 定义这类明显问题。
bash "$SCRIPT_DIR/validate-package.sh" "$ARTIFACT_PATH"

# 再做安装级 smoke，确认从工件展开后的文件系统布局足以支撑最小运行闭环。
PORT="$PORT" bash "$SCRIPT_DIR/install-package.sh" "$ARTIFACT_PATH"

echo "linux validation suite passed: $ARTIFACT_PATH"
