# Running the Veloren Agent Integration stack with Docker Compose

## Overview

`docker compose up --build` deploys the full stack in one command:

| Service | Builds from | Port | Purpose |
|---|---|---|---|
| `veloren-server` | `Dockerfile.server` | 14004/tcp | Dedicated Veloren game server |
| `veloren-agent` | `Dockerfile` | — | Headless Rust agent (`veloren-client` based) |
| `veloren-dashboard` | `Dockerfile.dashboard` | 5000/tcp | Flask monitoring dashboard + viewport API |

All services share the `veloren-net` bridge network, so the agent reaches the
server at `veloren-server:14004`, and the dashboard is published on
`localhost:5000`. See [docs/viewport-api.md](viewport-api.md) for the public
viewport endpoints (`/api/viewport/latest`, `/api/viewport/trail`).

This directory contains everything needed for single-command, full-stack
deployment:

```
docker compose up --build
```

## Before the first run

1. Copy the example env file and fill in your values:

   ```bash
   cp .env.example .env
   ```

   Set at least:
   - `VELOREN_USERNAME` / `VELOREN_PASSWORD` — Veloren account credentials used by the agent (read from `.env` via `env_file`).
   - `VELOREN_ASSETS` — path to the Airshipper assets directory on the host (mounted into the agent container).
   - `DASHBOARD_API_KEY` — API key required on every dashboard API endpoint except `GET /` and `GET /api/health`. Generate one with:

     ```bash
     python3 -c "import secrets; print(secrets.token_urlsafe(24))"
     ```

   If `DASHBOARD_API_KEY` is unset, the dashboard generates a random key at startup (it is never logged or embedded in HTML). Set `DASHBOARD_API_KEY` in `.env` for stable, operator-chosen access; clients supply it per request via the `X-API-Key` header (the dashboard UI reads it from `localStorage`, never from the page source).

   `.env` is gitignored — never commit it.

2. Make sure Docker and Docker Compose v2 are installed and the daemon is running:

   ```bash
   docker --version && docker compose version
   ```

## Start the stack

```bash
docker compose up --build
```

- `--build` builds all three images from their Dockerfiles.
- Drop `--build` on later runs to reuse cached images.
- Add `-d` to run detached: `docker compose up --build -d`.

The dashboard UI is at `http://localhost:5000`; the live viewport is at
`http://localhost:5000/viewport`.

## Stop the stack

```bash
docker compose down          # stop and remove containers
docker compose down -v       # also remove named volumes (world data + dashboard DB)
```

## Validate the Compose file

```bash
docker compose config
```

Prints the merged, resolved configuration and exits non-zero on any syntax or
reference error.

## Build notes / known fixes

### Duplicate `veloren-common` dependency (ControllerInputs type mismatch)

`src/lib.rs` and `src/main.rs` import `ControllerInputs` from
`veloren_common::comp` and pass it to `veloren_client::Client::tick`. If
`veloren-client` resolved `veloren-common` from a different git revision than
the direct `veloren-common` dependency, Cargo would compile two copies of
`veloren-common` and the `ControllerInputs` passed to `Client::tick` would be a
different type than the one `tick` expects — a hard compile error.

Fix (commit `c87a44c`): pin all three Veloren crates to the same git revision
so Cargo unifies them into a single `veloren-common`:

```toml
veloren-client      = { git = ".../veloren.git", rev = "f97be347c5a7b2b4674c47aa29ff9cc63a85e486" }
veloren-common      = { git = ".../veloren.git", rev = "f97be347c5a7b2b4674c47aa29ff9cc63a85e486", features = ["no-assets"] }
veloren-common-net  = { git = ".../veloren.git", rev = "f97be347c5a7b2b4674c47aa29ff9cc63a85e486" }
```

Do not use `tag = "weekly"` for `veloren-common`/`veloren-common-net` while
`veloren-client` is pinned to a `rev` — that is exactly what produced the
duplicate. `cargo check` (and `docker compose build`) pass once all three share
the same `rev`.

### Veloren toolchain patches

`veloren-client` pulls crates that use APIs removed from modern Rust
(`shred`'s `core::ptr::invalid_mut`, `specs::SendDispatcher`,
`env!("CARGO_WORKSPACE_DIR")` in git deps, …). The Docker sandbox applies
`patches/veloren-weekly-toolchain.patch` to the cargo-fetched Veloren checkout
and a small `sed` fix to `shred-0.15` after `cargo fetch`, then builds with
`RUSTFLAGS="-D warnings" cargo +nightly build --release`. See
`docs/troubleshooting.md` ("Rust build blocked on nightly features") for the
full patch inventory.

### Airshipper server option

By default `veloren-server` builds `veloren-server-cli` from the Veloren
gitlab source (`Dockerfile.server`). To use the Airshipper-provided
`veloren-server-cli` instead, replace the `build:` block of `veloren-server`
with an image containing that binary, or mount the Airshipper profiles
directory (`VELOREN_SERVER_BIN`) and override the entrypoint to run
`veloren-server-cli --non-interactive`.
