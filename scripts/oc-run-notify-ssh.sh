#!/usr/bin/env bash
set -euo pipefail

# Event-driven remote wrapper.
# Runs a remote command via SSH, then sends OpenClaw message on completion/failure.
#
# Example:
#   oc-run-notify-ssh.sh \
#     --host grunt@192.168.1.95 \
#     --label "snake 2000g" \
#     --channel discord \
#     --target 366115325797990400 \
#     -- ssh-cmd 'cd ~/neural-mutator && source .venv/bin/activate && python -m src.train ...'

HOST=""
LABEL="remote-job"
CHANNEL=""
TARGET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:-}"; shift 2 ;;
    --label)
      LABEL="${2:-}"; shift 2 ;;
    --channel)
      CHANNEL="${2:-}"; shift 2 ;;
    --target)
      TARGET="${2:-}"; shift 2 ;;
    --)
      shift; break ;;
    *)
      echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$HOST" || -z "$CHANNEL" || -z "$TARGET" ]]; then
  echo "--host, --channel, --target are required" >&2
  exit 2
fi
if [[ $# -eq 0 ]]; then
  echo "Remote command required after --" >&2
  exit 2
fi

remote_cmd="$*"
start_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

set +e
ssh "$HOST" "bash -lc $(printf '%q' "$remote_cmd")"
rc=$?
set -e

end_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
status="completed"
if [[ "$rc" -ne 0 ]]; then
  status="failed"
fi

msg="⚒️ ${LABEL} on ${HOST} ${status} (exit=${rc})\nStart: ${start_iso} UTC\nEnd: ${end_iso} UTC"

set +e
openclaw message send --channel "$CHANNEL" --target "$TARGET" --message "$msg" >/dev/null 2>&1
set -e

exit "$rc"
