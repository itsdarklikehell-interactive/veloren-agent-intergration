#!/usr/bin/env bash
# Start the public Veloren dedicated server.
# Uses --non-interactive (no stdin) with official auth server (no --no-auth).
# Server name and auth are configured in settings.ron.

set -euo pipefail

SERVER_BIN="${VELOREN_SERVER_BIN:-/home/rizzo/.local/share/airshipper/profiles/default/veloren-server-cli}"
SESSION="veloren-server"
PORT="${VELOREN_PORT:-14004}"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "Server session '$SESSION' already exists. Attach with: tmux attach -t $SESSION"
  exit 0
fi

echo "Starting Veloren dedicated server on port $PORT (session: $SESSION)..."
tmux new-session -d -s "$SESSION" "$SERVER_BIN --non-interactive"

echo "Server started. Attach with: tmux attach -t $SESSION"
echo "Detach with: Ctrl-b d"
echo "Stop with: tmux kill-session -t $SESSION"
echo "Verify: ss -tlnp | grep $PORT"
