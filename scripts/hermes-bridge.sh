#!/usr/bin/env bash
# Thin bridge between Hermes and the veloren-agent headless frontend.
#
# Usage: ./scripts/hermes-bridge.sh '<command json>'
#
# Sends a JSON command to the agent's stdin, prints JSON-lines observations
# from stdout. This is a stateless per-command bridge; for production use,
# keep a long-lived agent process and pipe commands/observations through it.

set -euo pipefail

AGENT_BIN="${VELUREN_AGENT_BIN:-./target/release/veloren-agent}"
CHARACTER="${VELUREN_AGENT_CHARACTER:-hermes}"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 '<command json>'" >&2
  exit 1
fi

CMD="$1"

echo "$CMD" | "$AGENT_BIN" --character "$CHARACTER"
