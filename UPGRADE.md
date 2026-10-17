# Upgrade Path — v0.1.0 → v1.0.0

This document covers upgrading an existing v0.1.0 checkout of veloren-agent to
v1.0.0. There are **no breaking changes** to the agent protocol or observation
schema (`schema_version=1` JSON-lines is stable).

## 1. Apply the release

```bash
git fetch origin
git checkout v1.0.0   # or: git pull origin main
```

## 2. Rust toolchain

v1.0.0 pins the nightly toolchain (`rust-toolchain.toml`). Install it if you
have not already:

```bash
rustup install nightly
cargo build --release   # clean, zero warnings
```

CI runs `cargo build/test --release -D warnings` on nightly; match that
locally before deploying.

## 3. Environment variables

- Rename any `VELUREN_*` references to `VELOREN_*` (typo fix applied across
  `.env.example`, docs, wiki, and scripts).
- `VELOREN_USERNAME` / `VELOREN_PASSWORD` are required for authenticated
  servers (new in v1.0.0).
- Copy `.env.example` to `.env` and fill in credentials; `.env` is gitignored
  and no secrets are committed.

## 4. Deployment

- Preferred: `docker compose up --build` starts the full three-service stack
  (veloren-server, veloren-agent, veloren-dashboard) on `veloren-net`.
- The manual `scripts/start-server.sh` flow still works, but server startup no
  longer uses `--no-auth` — run an authenticated local `veloren-server-cli`.

## 5. CLI

`--version` now prints `1.0.0` (derived from `Cargo.toml`). All other flags
(`--server`, `--character`, `--no_auth`) are unchanged.

## 6. Verify

```bash
cargo test --release --test integration
# 8 tests pass (3 mock + 5 lib); e2e tests skip without a local server
```
