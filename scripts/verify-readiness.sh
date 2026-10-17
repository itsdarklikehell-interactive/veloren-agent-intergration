#!/usr/bin/env bash
# Readiness check for the headless veloren-agent (stdio/JSON architecture; no HTTP server).
#
# Starts the built agent against the local Veloren server (tmux session 'veloren-server',
# port 14004), sends one {"cmd":"status"} command on stdin, waits for JSON observations
# containing "tick" to flow on stdout, then asks the agent to quit.
#
# This replaces an HTTP readiness poll (127.0.0.1:8000), which does not apply to a
# headless stdio/JSON agent.
#
# Required env:  VELOREN_USERNAME, VELOREN_PASSWORD  (authenticated server)
# Optional env:  VELOREN_PORT (default 14004), VELOREN_AGENT_BIN,
#                VELOREN_AGENT_CHARACTER (default "hermes"), VELOREN_ASSETS
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

AGENT_BIN="${VELOREN_AGENT_BIN:-./target/release/veloren-agent}"
PORT="${VELOREN_PORT:-14004}"
CHARACTER="${VELOREN_AGENT_CHARACTER:-${VELUREN_AGENT_CHARACTER:-hermes}}"

: "${VELOREN_USERNAME:?VELOREN_USERNAME must be set for authenticated servers}"
: "${VELOREN_PASSWORD:?VELOREN_PASSWORD must be set for authenticated servers}"

# --- 1. Build the agent if the release binary is missing ---------------------
if [[ ! -x "$AGENT_BIN" ]]; then
  echo "Agent binary not found at $AGENT_BIN — building with cargo build --release"
  cargo build --release
fi

# --- 2. The Veloren server must be reachable ---------------------------------
if ! ss -tlnp | grep -q ":${PORT}"; then
  echo "ERROR: nothing listening on port $PORT — start the Veloren server first:" >&2
  echo "  ./scripts/start-server.sh    # tmux session: veloren-server" >&2
  exit 1
fi

# --- 3. stdio/JSON readiness: status command -> observations with "tick" -----
OBS="$(mktemp)"
trap 'rm -f "$OBS"' EXIT

# Start agent in background; stdout+stderr go to the obs file.
"$AGENT_BIN" --character "$CHARACTER" > "$OBS" 2>&1 &
AGENT_PID=$!

# Give the agent a moment to start and connect, then send the status command.
sleep 2
printf '%s\n' '{"cmd":"status"}' > /proc/$AGENT_PID/fd/0 2>/dev/null || true

# Wait up to 30 seconds for tick observations to appear on stdout.
FOUND=0
for i in $(seq 1 30); do
  if grep -q '"tick"' "$OBS" 2>/dev/null; then
    FOUND=1
    break
  fi
  sleep 1
done

# Ask the agent to quit cleanly.
printf '%s\n' '{"cmd":"quit"}' > /proc/$AGENT_PID/fd/0 2>/dev/null || true

# Wait for the agent process to exit (up to 15 seconds).
for i in $(seq 1 15); do
  if ! kill -0 "$AGENT_PID" 2>/dev/null; then
    break
  fi
  sleep 1
done

# Force-kill if still running.
if kill -0 "$AGENT_PID" 2>/dev/null; then
  kill -9 "$AGENT_PID" 2>/dev/null || true
  wait "$AGENT_PID" 2>/dev/null || true
fi

if [[ $FOUND -eq 0 ]]; then
  echo "ERROR: readiness check failed — no JSON observation with 'tick' on stdout" >&2
  echo "--- agent output ($OBS) ---" >&2
  cat "$OBS" >&2 || true
  exit 1
fi

echo "READY: headless agent emitted a stdio/JSON observation (tick observed) against the server on port $PORT"
