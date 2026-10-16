#!/usr/bin/env bash
# Stop the local Veloren dedicated server.

set -euo pipefail

SESSION="veloren-server"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  tmux kill-session -t "$SESSION"
  echo "Server session '$SESSION' stopped."
else
  echo "No server session '$SESSION' running."
fi
