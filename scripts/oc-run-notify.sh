#!/usr/bin/env bash
set -euo pipefail

# Event-driven run wrapper for OpenClaw.
# Runs a command, then sends a completion/failure message exactly once on exit.
#
# Example:
#   oc-run-notify.sh \
#     --label "snake repro" \
#     --channel discord \
#     --target 366115325797990400 \
#     --log /tmp/snake.log \
#     -- bash -lc 'source .venv/bin/activate && python -m src.train ...'

LABEL="job"
CHANNEL=""
TARGET=""
LOG_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --label)
      LABEL="${2:-}"; shift 2 ;;
    --channel)
      CHANNEL="${2:-}"; shift 2 ;;
    --target)
      TARGET="${2:-}"; shift 2 ;;
    --log)
      LOG_PATH="${2:-}"; shift 2 ;;
    --)
      shift; break ;;
    *)
      echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$CHANNEL" || -z "$TARGET" ]]; then
  echo "--channel and --target are required" >&2
  exit 2
fi
if [[ $# -eq 0 ]]; then
  echo "Command required after --" >&2
  exit 2
fi

start_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if [[ -n "$LOG_PATH" ]]; then
  mkdir -p "$(dirname "$LOG_PATH")"
fi

set +e
if [[ -n "$LOG_PATH" ]]; then
  "$@" >>"$LOG_PATH" 2>&1
else
  "$@"
fi
rc=$?
set -e

end_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
status="completed"
if [[ "$rc" -ne 0 ]]; then
  status="failed"
fi

msg="⚒️ ${LABEL} ${status} (exit=${rc})\nStart: ${start_iso} UTC\nEnd: ${end_iso} UTC"
if [[ -n "$LOG_PATH" ]]; then
  msg+="\nLog: ${LOG_PATH}"
fi

# Best-effort notify; do not mask underlying exit code.
set +e
openclaw message send --channel "$CHANNEL" --target "$TARGET" --message "$msg" >/dev/null 2>&1
set -e

exit "$rc"
