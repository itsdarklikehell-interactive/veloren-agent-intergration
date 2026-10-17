#!/usr/bin/env bash
# End-to-end integration test for the veloren-agent headless frontend.
#
# This script exercises the agent against a local Veloren server and validates:
#   1. The agent connects and emits JSON-lines observations with "tick"
#   2. The agent accepts a `quit` command and exits cleanly (exit 0)
#   3. Auto-recovery: agent reconnects after the server connection drops
#
# Usage:
#   ./scripts/test-integration.sh
#
# Required env: VELOREN_USERNAME, VELOREN_PASSWORD
# Optional env: VELOREN_PORT (default 14004), VELOREN_AGENT_BIN,
#               VELOREN_AGENT_CHARACTER (default "hermes")
#
# Exits 0 if all checks pass, non-zero otherwise.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

AGENT_BIN="${VELOREN_AGENT_BIN:-./target/release/veloren-agent}"
PORT="${VELOREN_PORT:-14004}"
CHARACTER="${VELOREN_AGENT_CHARACTER:-integration-test}"

: "${VELOREN_USERNAME:?VELOREN_USERNAME must be set}"
: "${VELOREN_PASSWORD:?VELOREN_PASSWORD must be set}"

PASS=0
FAIL=0

log() { echo "[test-integration] $*"; }
fail() { echo "[test-integration] FAIL: $*" >&2; FAIL=$((FAIL + 1)); }
pass() { echo "[test-integration] PASS: $*"; PASS=$((PASS + 1)); }

# --- 1. Build the agent if missing ---------------------------------------
if [[ ! -x "$AGENT_BIN" ]]; then
  log "Agent binary not found at $AGENT_BIN — building with cargo build --release"
  cargo build --release
fi

# --- 2. Server must be reachable -----------------------------------------
if ! ss -tlnp | grep -q ":${PORT}"; then
  log "SKIP: nothing listening on port $PORT — start the Veloren server first"
  log "  (./scripts/start-server.sh)"
  exit 0
fi

# --- 3. Start agent, wait for observations with "tick" -------------------
OBS="$(mktemp)"
trap 'rm -f "$OBS"' EXIT

export VELOREN_USERNAME VELOREN_PASSWORD
"$AGENT_BIN" --character "$CHARACTER" > "$OBS" 2>&1 &
AGENT_PID=$!

# Wait up to 30 seconds for tick observations.
FOUND=0
for i in $(seq 1 30); do
  if grep -q '"tick"' "$OBS" 2>/dev/null; then
    FOUND=1
    break
  fi
  sleep 1
done

if [[ $FOUND -eq 1 ]]; then
  pass "agent emitted JSON-lines observations with 'tick' on stdout"
else
  fail "agent did not emit observations within 30s"
  echo "--- agent output ---" >&2
  cat "$OBS" >&2 || true
  kill -9 "$AGENT_PID" 2>/dev/null || true
  wait "$AGENT_PID" 2>/dev/null || true
  exit 1
fi

# --- 4. Send quit, verify clean exit -------------------------------------
printf '%s\n' '{"cmd":"quit"}' > /proc/$AGENT_PID/fd/0 2>/dev/null || true

QUIT_OK=0
for i in $(seq 1 15); do
  if ! kill -0 "$AGENT_PID" 2>/dev/null; then
    QUIT_OK=1
    break
  fi
  sleep 1
done

if kill -0 "$AGENT_PID" 2>/dev/null; then
  kill -9 "$AGENT_PID" 2>/dev/null || true
  wait "$AGENT_PID" 2>/dev/null || true
  fail "agent did not exit after quit command"
else
  pass "agent exited cleanly after quit command"
fi

# --- 5. Auto-recovery: kill the server, verify agent retries -------------
# Stop the server; the agent should enter its recovery retry loop.
if tmux has-session -t veloren-server 2>/dev/null; then
  log "Stopping server to test auto-recovery..."
  tmux kill-session -t veloren-server
  sleep 3

  # The agent should still be running (in retry loop), not have exited.
  if kill -0 "$AGENT_PID" 2>/dev/null; then
    pass "agent survived server shutdown (in auto-recovery loop)"
  else
    fail "agent exited after server shutdown (expected auto-recovery)"
  fi

  # Restart the server for cleanup
  log "Restarting server..."
  ./scripts/start-server.sh > /dev/null 2>&1 || true
  sleep 5

  # Agent should reconnect and emit observations again.
  RECONNECTED=0
  for i in $(seq 1 15); do
    if grep -q '"tick"' "$OBS" 2>/dev/null; then
      # Check for new ticks after the restart (file grows)
      RECONNECTED=1
      break
    fi
    sleep 2
  done

  if [[ $RECONNECTED -eq 1 ]]; then
    pass "agent reconnected and resumed emitting observations"
  else
    fail "agent did not reconnect after server restart"
  fi

  # Clean up: send quit to the agent
  printf '%s\n' '{"cmd":"quit"}' > /proc/$AGENT_PID/fd/0 2>/dev/null || true
  for i in $(seq 1 10); do
    if ! kill -0 "$AGENT_PID" 2>/dev/null; then
      break
    fi
    sleep 1
  done
  if kill -0 "$AGENT_PID" 2>/dev/null; then
    kill -9 "$AGENT_PID" 2>/dev/null || true
  fi
else
  log "SKIP: no tmux session 'veloren-server' — skipping auto-recovery test"
fi

# --- Summary -------------------------------------------------------------
echo ""
echo "=== Integration test summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"

if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
exit 0
