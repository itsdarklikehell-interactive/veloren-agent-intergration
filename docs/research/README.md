# Research Reports

This directory contains aggregated research reports from the autonomous R&D agent pipeline.

## Structure

- `github-issues-<date>.json` — GitHub issues fetched during weekly sync
- `sync-context-<date>.json` — Context passed to subagents for research
- `research-<timestamp>.md` — Aggregated research findings from subagents

## Weekly Sync

Every Monday at 8 AM, the cron job (`pipeline/cron-weekly-sync.sh`) checks GitHub and Linear for unassigned or prioritized R&D tasks, then Hermes dispatches parallel subagents to research competing architectural solutions. Outputs are aggregated here.
