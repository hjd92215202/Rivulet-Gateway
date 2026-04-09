#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$REPO_ROOT/target/fixture-backend-test"
mkdir -p "$LOG_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "skip fixture backend termination test on non-Linux host"
  exit 0
fi

if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN="python"
else
  echo "python runtime not found" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl command not found" >&2
  exit 1
fi

PORT="$("$PYTHON_BIN" - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

BACKEND_LOG="$LOG_DIR/backend-${PORT}.log"
"$PYTHON_BIN" "$REPO_ROOT/scripts/fixture-backend.py" \
  --bind 127.0.0.1 \
  --port "$PORT" \
  --default-bytes 64 \
  >"$BACKEND_LOG" 2>&1 &
BACKEND_PID="$!"

cleanup() {
  if kill -0 "$BACKEND_PID" >/dev/null 2>&1; then
    kill -KILL "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

for _ in $(seq 1 30); do
  code="$(curl --max-time 2 -sS -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/healthz" || true)"
  if [[ "$code" == "200" ]]; then
    break
  fi
  sleep 0.2
done

if [[ "${code:-}" != "200" ]]; then
  echo "fixture backend failed to become healthy before SIGTERM test" >&2
  cat "$BACKEND_LOG" >&2 || true
  exit 1
fi

kill -TERM "$BACKEND_PID" >/dev/null 2>&1 || true

terminated="false"
for _ in $(seq 1 32); do
  if ! kill -0 "$BACKEND_PID" >/dev/null 2>&1; then
    terminated="true"
    break
  fi
  sleep 0.25
done

if [[ "$terminated" != "true" ]]; then
  echo "fixture backend did not exit within 8s after SIGTERM" >&2
  cat "$BACKEND_LOG" >&2 || true
  exit 1
fi

wait "$BACKEND_PID" >/dev/null 2>&1 || true
trap - EXIT
echo "fixture backend SIGTERM termination test passed"
