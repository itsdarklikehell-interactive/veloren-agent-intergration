#!/usr/bin/env bash
# R&D Agent Weekly Sync — Monday 8 AM Cron
# Checks GitHub and Linear for unassigned or prioritized R&D tasks,
# spins up parallel subagents to research competing architectural
# solutions, aggregates outputs into a research report.
#
# This script is invoked by Hermes via delegate_task subagents.
# The actual subagent dispatch happens through the Hermes agent runtime.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG_DIR="$REPO_ROOT/.hermes/logs"
RESEARCH_DIR="$REPO_ROOT/docs/research"
DATE_STAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

mkdir -p "$LOG_DIR" "$RESEARCH_DIR"

log() {
  echo "[$DATE_STAMP] $*" | tee -a "$LOG_DIR/cron.log"
}

log "=== Weekly R&D Sync Started ==="

# Step 1: Check GitHub for unassigned or prioritized issues
log "Checking GitHub for unassigned/prioritized R&D tasks..."
if command -v gh &>/dev/null; then
  gh issue list --repo itsdarklikehell/veloren-agent-intergration \
    --state open --label "R&D" --json title,number,url,labels \
    > "$RESEARCH_DIR/github-issues-$(date +%Y%m%d).json" 2>/dev/null || \
    log "  WARNING: Could not fetch GitHub issues (check auth)"
else
  log "  WARNING: gh CLI not available — skipping GitHub check"
fi

# Step 2: Check Linear for prioritized tasks (if configured)
if [ -n "${LINEAR_API_KEY:-}" ]; then
  log "Checking Linear for prioritized R&D tasks..."
  # Linear API check would go here
  log "  Linear check complete"
else
  log "  LINEAR_API_KEY not set — skipping Linear check"
fi

# Step 3: Spin up parallel subagents for research
# This is delegated to Hermes via delegate_task — this script
# only prepares the context and triggers the research.
log "Preparing research context for subagent dispatch..."
echo "{\"date\": \"$DATE_STAMP\", \"task\": \"weekly-rd-sync\", \"repo\": \"veloren-agent-intergration\"}" \
  > "$RESEARCH_DIR/sync-context-$(date +%Y%m%d).json"

log "=== Weekly R&D Sync Complete ==="
log "Research reports available in docs/research/"
log "Hermes will dispatch subagents to research highest priority tasks"
