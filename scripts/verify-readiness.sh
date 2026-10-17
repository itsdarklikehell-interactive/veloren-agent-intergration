#!/usr/bin/env bash
# Readiness check for the headless veloren-agent (stdio/JSON architecture; no HTTP server).
#
# Starts the built agent against the local Veloren server (tmux session 'veloren-server',
# port 14004), sends one {"cmd":"status"} command on stdin, and asserts that a JSON
# observation containing "tick" is emitted on stdout. Then asks the agent to quit.
#
# This replaces an HTTP readiness poll (127.0.0.1:8000), which does not apply to a
# headless stdio/JSON agent.
#
# Required env:  VELOREN_USERNAME, VELOREN_PASSWORD  (authenticated server)
# Optional env:  VELOREN_PORT (default 14004), VELOREN_AGENT_BIN,
#                VELOREN_AGENT_CHARACTER (default "hermes")
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

AGENT_BIN="${VELOREN_AGENT_BIN:-./target/release/veloren-agent}"
PORT="${VELOREN_PORT:-14004}"
CHARACTER="${VELUREN_AGENT_CHARACTER:-hermes}"

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

# --- 3. stdio/JSON readiness: status command -> observation with "tick" ------
OBS="$(mktemp)"
trap 'rm -f "$OBS"' EXIT

# Send "status", wait for ticks to flow, then "quit" so the agent exits cleanly.
# (Both commands in one pipe would race: the drain reads all of stdin at once and
#  "quit" would break the loop before any observation is written.)
set +e
( printf '%s\n' '{"cmd":"status"}'; sleep 3; printf '%s\n' '{"cmd":"quit"}'
) | timeout 90 bash -c "\"$AGENT_BIN\" --character \"$CHARACTER\" > \"$OBS\" 2>&1"
rc=$?
set -e

if [[ $rc -eq 124 ]]; then
  echo "WARNING: agent did not exit on its own within 90s (long-lived process); timeout ended it."
fi

if ! grep -q '"tick"' "$OBS"; then
  echo "ERROR: readiness check failed — no JSON observation with 'tick' on stdout" >&2
  echo "--- agent output ($OBS) ---" >&2
  cat "$OBS" >&2 || true
  exit 1
fi

echo "READY: headless agent emitted a stdio/JSON observation (tick observed) against the server on port $PORT"
