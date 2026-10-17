# Release Notes — veloren-agent v1.0.0

**Tag:** `v1.0.0`
**Commit:** `<commit-hash>` (filled in at release time)
**Released:** 2026-10-17

## About this release

First stable release (1.0.0) of veloren-agent: a headless Rust frontend for
Veloren that connects to a local `veloren-server-cli` via `veloren-client`,
runs the game tick loop without a display/GPU, and exposes a stdio/JSON command
interface for Hermes (or any agent harness). Built and operated under the Hermes
Kanban infrastructure (see KANBAN.md, AGENTS.md).

## Highlights

- **Semantic versioning**: `Cargo.toml` at `1.0.0`; `--version` reports `1.0.0`.
- **Auto-recovery**: connection loss triggers exponential-backoff reconnect,
  re-authentication, and game re-entry; the loop only exits on a clean `quit`.
- **Full-stack deployment**: `docker compose up --build` starts
  veloren-server + veloren-agent + veloren-dashboard; configuration via `.env`.
- **Observability**: Prometheus `GET /metrics`, dashboard `GET /api/metrics`
  JSON, and a Performance tab with tick-latency sparklines.
- **Verified**: integration suite passes (8 tests: 3 mock + 5 lib; e2e tests
  skip when no local server is running).

## Quick start

```bash
cargo build --release
VELOREN_PASSWORD=<password> ./target/release/veloren-agent --character hermes
```

Or with Docker Compose:

```bash
cp .env.example .env   # set VELOREN_* credentials
docker compose up --build
```

## Verification performed

- `cargo check --release` — clean, zero warnings
- `cargo test --release --test integration` — 8 tests pass
  (3 mock-server tests + 5 library tests; e2e tests skipped without a server)

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full, commit-referenced list of
changes since the initial scaffold.

## Upgrade path

See [UPGRADE.md](UPGRADE.md) for moving from v0.1.0 to v1.0.0.
