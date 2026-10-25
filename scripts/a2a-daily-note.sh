#!/usr/bin/env bash
# ============================================================================
# a2a-daily-note.sh
#
# Generates a daily Obsidian note capturing A2A agent activity
# (commands sent, observations received, status checks).
#
# Usage:
#   a2a-daily-note.sh <YYYY-MM-DD>   # Generate note for a specific date
#   a2a-daily-note.sh --today        # Generate note for today
#   a2a-daily-note.sh --open         # Generate + open in Obsidian
#
# Output:
#   /home/hans/Documents/Obsidian Vault/Dagelijks/YYYY-MM-DD.md
# ============================================================================
set -euo pipefail

VAULT="${OBSIDIAN_VAULT_PATH:-/home/hans/Documents/Obsidian Vault}"
NOTES_DIR="$VAULT/Dagelijks"
A2A_BRIDGE="./scripts/openclaw-to-a2a-bridge.sh"
TODAY=$(date +%Y-%m-%d)
DATE="${1:-$TODAY}"

if [[ "$DATE" == "--today" ]]; then
  DATE=$(date +%Y-%m-%d)
fi

if [[ ! -d "$NOTES_DIR" ]]; then
  echo "ERROR: Notes dir not found: $NOTES_DIR" >&2
  exit 1
fi

NOTE_FILE="$NOTES_DIR/$DATE.md"

# Check if note already exists
if [[ -f "$NOTE_FILE" ]]; then
  echo "Note already exists: $NOTE_FILE"
  exit 0
fi

# Collect A2A activity for the day
# (In a real scenario, you'd query the A2A server's metrics/logs here)
# For now, we create an empty note and log how to fill it.

cat > "$NOTE_FILE" << EOF
# A2A Dagelijkse Notitie — $DATE

## A2A Activiteit

### Commando's Verzonden (OpenClaw -> veloren-agent)
- [ ]

### Observaties Ontvangen (veloren-agent -> OpenClaw)
- [ ]

### Status Controles
- [ ]

### Buggen / Probleemoplossing
- [ ]

## Memo

- Beslissingen over agent-gebruik
- Dingen die blijven staan
- Dingen die geleerd zijn

## Verbindingen

- [[A2A Protocol]] - het agent-to-agent communicatieprotocol
- [[veloren-agent]] - de headless veloren agent

## Overige notities

- [ ]
EOF

echo "Created: $NOTE_FILE"
echo "Tip: Run $A2A_BRIDGE --observ hermes to stream live A2A observations"
echo "      and paste results into the note."
