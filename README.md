# Herbs veloren-agent

Veloren agent + A2A protocol tools for Hermes OpenClaw integration.

## Setup

1. Build: `cargo +nightly build --release`
2. Run: `./target/release/veloren-agent --server 127.0.0.1:14004 --character hermes`
3. Connect OpenClaw via A2A HTTP/WSS on port 18080

## Tools

- `scripts/openclaw-to-a2a-bridge.sh` — one-shot A2A command/status/observ
- `scripts/a2a-daily-note.sh` — generate Obsidian daily notes for A2A activity
- `scripts/verify-readiness.sh` — smoke-test the agent

## A2A Protocol

See `docs/a2a-protocol.md` for the full wire format and endpoints.

## Pre-commit

- `cargo +nightly build --release` — check the build
- `scripts/verify-readiness.sh` — run the smoke test
- Verify + push: `git add -A && git commit -m "..." && git push`
