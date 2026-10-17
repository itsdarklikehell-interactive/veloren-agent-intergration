#!/usr/bin/env bash
# R&D Agent Pipeline — Recursive Loop Executor
# Follows: Research & Analyze -> Design & Architecture -> Code Sandbox Generation
#          -> Automated Testing -> Debugging -> Pull Request / Deployment
#
# Usage: ./pipeline/run-pipeline.sh <task-description>
#
# Each step produces verifiable artifacts. The loop repeats until the task
// is complete or escalation is required.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TASK_DESC="${1:-}"
LOG_DIR="$REPO_ROOT/.hermes/logs"
ARTIFACT_DIR="$REPO_ROOT/.hermes/artifacts"

mkdir -p "$LOG_DIR" "$ARTIFACT_DIR" "$REPO_ROOT/docs/research"

log() {
  echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] $*" | tee -a "$LOG_DIR/pipeline.log"
}

fail() {
  log "FAIL: $*"
  exit 1
}

# --- Step 1: Research & Analyze ---
research_analyze() {
  log "Step 1: Research & Analyze — $TASK_DESC"
  
  # Read existing repo context
  if [ -f "$REPO_ROOT/AGENTS.md" ]; then
    log "  Read AGENTS.md for interaction guidelines"
  fi
  if [ -f "$REPO_ROOT/SOUL.md" ]; then
    log "  Read SOUL.md for persona and pipeline"
  fi
  
  # Check for existing similar work
  if [ -d "$REPO_ROOT/docs/research" ]; then
    log "  Scanning docs/research/ for prior art"
    ls "$REPO_ROOT/docs/research/" 2>/dev/null || log "  (no prior research found)"
  fi
  
  # Record research findings
  echo "# Research: $TASK_DESC" > "$ARTIFACT_DIR/research-$(date +%s).md"
  echo "Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$ARTIFACT_DIR/research-$(date +%s).md"
  echo "" >> "$ARTIFACT_DIR/research-$(date +%s).md"
  echo "## Problem Statement" >> "$ARTIFACT_DIR/research-$(date +%s).md"
  echo "$TASK_DESC" >> "$ARTIFACT_DIR/research-$(date +%s).md"
  
  log "  Research complete — artifact saved"
}

# --- Step 2: Design & Architecture ---
design_architecture() {
  log "Step 2: Design & Architecture"
  
  cat > "$ARTIFACT_DIR/design-$(date +%s).md" <<EOF
# Design: $TASK_DESC
Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

## Interfaces
[TBD — specify during implementation]

## Data Models
[TBD — specify during implementation]

## Error Handling
[TBD — specify during implementation]

## Integration Points
[TBD — specify during implementation]

## Conventions
Follow project conventions in AGENTS.md and SOUL.md.
EOF
  
  log "  Design artifact saved"
}

# --- Step 3: Code Sandbox Generation ---
sandbox_generate() {
  log "Step 3: Code Sandbox Generation"
  
  # Check if Docker is available
  if ! command -v docker &>/dev/null; then
    log "  WARNING: Docker not available — running on host (less isolated)"
    return 0
  fi
  
  log "  Docker sandbox available — code generation will run in container"
  # The actual code generation is delegated to subagents via Hermes delegate_task
  log "  Sandbox step complete"
}

# --- Step 4: Automated Testing ---
automated_test() {
  log "Step 4: Automated Testing & Verification"
  
  # Run cargo test for Rust projects
  if [ -f "$REPO_ROOT/Cargo.toml" ]; then
    log "  Running cargo test..."
    if cargo test 2>&1 | tee "$LOG_DIR/test-output.log"; then
      log "  Tests passed"
    else
      log "  FAIL: Tests did not pass"
      return 1
    fi
  fi
  
  # Run cargo build
  if [ -f "$REPO_ROOT/Cargo.toml" ]; then
    log "  Running cargo build..."
    if cargo build 2>&1 | tee "$LOG_DIR/build-output.log"; then
      log "  Build succeeded"
    else
      log "  FAIL: Build did not succeed"
      return 1
    fi
  fi
  
  log "  Testing step complete"
}

# --- Step 5: Debugging ---
debugging() {
  log "Step 5: Debugging (if needed)"
  
  # Check if previous step failed
  if [ -f "$LOG_DIR/test-output.log" ] && grep -qi "fail\|error" "$LOG_DIR/test-output.log"; then
    log "  Test failures detected — debugging required"
    log "  Analyze traceback, isolate root cause, fix, add regression test"
    return 1  # Signal that debugging loop is needed
  fi
  
  log "  No debugging needed"
}

# --- Step 6: Pull Request / Deployment ---
pr_deploy() {
  log "Step 6: Pull Request / Deployment"
  
  log "  Changes ready for PR — commit with [Agent: R&D-Agent] prefix"
  log "  Hermes will review and approve deployment"
  
  # Check git status
  if git -C "$REPO_ROOT" status --porcelain | grep -q .; then
    log "  Uncommitted changes detected — ready for commit"
  else
    log "  No changes to deploy"
  fi
}

# --- Main Loop ---
main() {
  log "=== R&D Pipeline Started: $TASK_DESC ==="
  
  research_analyze
  design_architecture
  sandbox_generate
  
  if automated_test; then
    log "  Testing passed — proceeding to deployment"
    pr_deploy
    log "=== Pipeline Complete ==="
  else
    log "  Testing failed — entering debugging loop"
    if debugging; then
      log "  Debugging resolved — re-running tests"
      automated_test && pr_deploy
    else
      log "  Debugging could not resolve — escalating to Hermes"
      fail "Pipeline blocked — human review required"
    fi
  fi
}

main "$@"
