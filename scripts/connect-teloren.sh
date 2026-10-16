#!/usr/bin/env bash
# Connect teloren (terminal Veloren frontend) to the local server.
# Runs teloren in a tmux session so Hermes can send keystrokes and read screen output.
#
# Usage: ./scripts/connect-teloren.sh [character_name]
#
# Prerequisites:
#   - Local server running (./scripts/start-server.sh)
#   - teloren binary at /tmp/teloren-probe/target/debug/teloren
#
# Known issue: teloren's hickory-resolver may fail to resolve 127.0.0.1.
# Workaround: try no_proxy=*

set -euo pipefail

TELOREN_BIN="${TELOREN_BIN:-/tmp/teloren-probe/target/debug/teloren}"
SESSION="veloren-agent"
CHARACTER="${1:-hermes}"
SERVER="127.0.0.1:14004"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "Teloren session '$SESSION' already exists. Attach with: tmux attach -t $SESSION"
  exit 0
fi

if [[ ! -x "$TELOREN_BIN" ]]; then
  echo "Error: teloren binary not found at $TELOREN_BIN"
  echo "Build it first: cd /tmp/teloren-probe && cargo build"
  exit 1
fi

echo "Starting teloren connected to $SERVER (session: $SESSION, character: $CHARACTER)..."
tmux new-session -d -s "$SESSION" "no_proxy=* '$TELOREN_BIN' --server '$SERVER' --username '$CHARACTER' --character '$CHARACTER' --password '' 2>&1"

echo "Teloren started. Attach with: tmux attach -t $SESSION"
echo "Detach with: Ctrl-b d"
echo "Stop with: tmux kill-session -t $SESSION"
echo ""
echo "Controls: wasd=move, SPACE=jump, x=primary, z=secondary, g=glide, r=respawn, return=chat, q=quit"
