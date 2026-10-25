#!/usr/bin/env bash
# ============================================================================
# openclaw-to-a2a-bridge.sh
#
# Lets OpenClaw talk to the veloren-agent's A2A HTTP/WSS server.
#
# Usage:
#   openclaw-to-a2a-bridge.sh --send 'move 2 0'        # POST a move command
#   openclaw-to-a2a-bridge.sh --send-chat 'hello'      # POST a chat message
#   openclaw-to-a2a-bridge.sh --status <name>          # GET agent status
#   openclaw-to-a2a-bridge.sh --observ <name>          # stream observations
#   openclaw-to-a2a-bridge.sh --help
# ============================================================================
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_DIR"

A2A_ADDR="127.0.0.1:18080"
AGENT_NAME="hermes"
A2A_KEY="${A2A_API_KEY:-}"
METHOD="GET"
PATH="/api/agent/${AGENT_NAME}/status"
BODY=""

usage() {
  echo "Usage: $0 <command> [args]"
  echo ""
  echo "Commands:"
  echo "  --send  <x> <y>            POST a move command (x, y)"
  echo "  --send-chat  <msg>         POST a chat message"
  echo "  --status  <name>           GET agent status"
  echo "  --observ  <name>           Stream observations (WS not supported, fallback to status)"
  echo "  --help                     Show this help"
  echo ""
  echo "Env:"
  echo "  A2A_API_KEY   Optional API key for auth"
  echo "  A2A_ADDR      Server address (default: 127.0.0.1:18080)"
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --send)
      shift
      METHOD="POST"
      PATH="/api/agent/${AGENT_NAME}/command"
      BODY=$(printf '{"cmd":"move","x":%s,"y":%s}' "$1" "$2")
      ;;
    --send-chat)
      shift
      METHOD="POST"
      PATH="/api/agent/${AGENT_NAME}/command"
      BODY=$(printf '{"cmd":"chat","msg":"%s"}' "$1")
      ;;
    --status)
      shift
      AGENT_NAME="$1"
      METHOD="GET"
      PATH="/api/agent/${AGENT_NAME}/status"
      BODY=""
      ;;
    --observ)
      shift
      AGENT_NAME="$1"
      METHOD="GET"
      PATH="/api/agent/${AGENT_NAME}/observations"
      BODY=""
      ;;
    --help|-h)
      usage
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

echo "== openclaw-to-a2a-bridge =="
echo "  target:      ${A2A_ADDR}"
echo "  agent:       ${AGENT_NAME}"
echo "  method:      ${METHOD}"
echo "  path:        ${PATH}"
echo "  auth:        ${A2A_KEY:+set} (key length: ${#A2A_KEY})"
echo "  body:        ${BODY:-<empty>}"

# Prefer curl, fall back to python3 (from Hermes venv if needed)
if command -v curl >/dev/null 2>&1; then
  ARGS=(-s)
  if [[ -n "$A2A_KEY" ]]; then
    ARGS+=(--header "X-API-Key: ${A2A_KEY}")
  fi
  if [[ -n "$BODY" ]]; then
    ARGS+=(--header "Content-Type: application/json" --data "$BODY")
  fi
  curl "${ARGS[@]}" "http://${A2A_ADDR}${PATH}"
else
  # Use python3 from Hermes venv if available
  PYTHON_BIN="$(command -v python3)"
  if [[ -z "$PYTHON_BIN" ]]; then
    PYTHON_BIN="/home/hans/.hermes/hermes-agent/venv/bin/python3"
  fi
  "$PYTHON_BIN" - "$A2A_ADDR" "$METHOD" "$PATH" "$BODY" "$A2A_KEY" << 'PY'
import json, sys, urllib.request, urllib.error

addr, method, path, body, key = sys.argv[1:6]
url = f"http://{addr}{path}"

req = urllib.request.Request(url, method=method)
req.add_header("Content-Type", "application/json")
if key:
    req.add_header("X-API-Key", key)

try:
    if body:
        req.data = body.encode()
    resp = urllib.request.urlopen(req, timeout=10)
    print(resp.read().decode())
except urllib.error.HTTPError as e:
    print(f"HTTP {e.code}: {e.read().decode()}", file=sys.stderr)
    sys.exit(e.code)
PY
fi

echo ""
echo "== done =="
