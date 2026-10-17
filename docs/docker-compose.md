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

1. Copy the example env file:

   ```bash
   cp .env.example .env
   ```

2. Fill in `.env` (it is gitignored — never commit it):

   | Variable | Required | Example |
   |---|---|---|
   | `VELOREN_USERNAME` | Yes | `hermes` |
   | `VELOREN_PASSWORD` | Yes (authenticated servers) | `...` |
   | `VELOREN_ASSETS` | Recommended | `/home/rizzo/.local/share/airshipper/profiles/default/assets` |
   | `DASHBOARD_API_KEY` | Recommended | `python3 -c "import secrets; print(secrets.token_urlsafe(24))"` |
   | `VELOREN_PORT` | No | `14004` |
   | `VELOREN_AGENT_CHARACTER` | No | `hermes` |
   | `VELOREN_DASHBOARD_HOST` | No | `0.0.0.0` |
   | `VELOREN_DASHBOARD_PORT` | No | `5000` |
   | `DASHBOARD_VERSION` | No | `2.0.0-m2` |

   If `DASHBOARD_API_KEY` is unset, the dashboard generates a random key at
   startup (it is never logged or embedded in HTML). Set `DASHBOARD_API_KEY`
   in `.env` for stable, operator-chosen access; clients supply it per request
   via the `X-API-Key` header. Every API endpoint except `GET /`,
   `GET /api/health`, `GET /metrics`, and the two `/api/viewport/*` endpoints
   requires it.

3. Make sure Docker and Docker Compose v2 are installed and the daemon is running:

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

## Health checks

| Service | Check |
|---|---|
| `veloren-server` | TCP port 14004 reachable from inside the container ||
| `veloren-agent` | `/usr/local/bin/veloren-agent --help` exits 0 (release binary starts) ||
| `veloren-dashboard` | `GET /api/health` (public, no API key) returns HTTP 200 once the app serves ||

The agent's real work — joining the game world — is ordered after the server
via `depends_on: veloren-server` with `condition: service_healthy`, so the
agent container never starts before the world is ready. Mark the agent
healthy in `docker compose ps` before expecting observations.

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

## Verify the deployment

```bash
# 1. All containers created and healthy
docker compose ps

# 2. Game server port accepting connections (from the host)
ss -tlnp | grep 14004

# 3. Public dashboard endpoints
curl -s http://localhost:5000/api/health                                # uptime, version, last tick
curl -s http://localhost:5000/api/viewport/latest                       # latest observation
curl -s "http://localhost:5000/api/viewport/trail?limit=50"           # position trail

# 4. Authenticated API (requires DASHBOARD_API_KEY)
curl -s -H "X-API-Key: $DASHBOARD_API_KEY" http://localhost:5000/api/status
```

## Manage the stack

| Goal | Command |
|---|---|
| Stream logs (all services) | `docker compose logs -f` |
| Logs of one service | `docker compose logs -f veloren-agent` |
| Restart one service | `docker compose restart veloren-dashboard` |
| Rebuild and restart | `docker compose up --build` |
| Stop and remove containers | `docker compose down` |
| Stop and remove named volumes (world data + dashboard DB) | `docker compose down -v` |

## Environment variable reference

`veloren-agent` receives `VELOREN_USERNAME`, `VELOREN_PASSWORD`,
`VELOREN_ASSETS`, `VELOREN_PORT`, and `VELOREN_AGENT_CHARACTER` from `.env`
(via `env_file` / `environment`). `veloren-dashboard` receives
`DASHBOARD_API_KEY`, `VELOREN_DASHBOARD_HOST`, `VELOREN_DASHBOARD_PORT`, and
`DASHBOARD_VERSION` the same way. None of these are hardcoded in the Compose
file or the images.

## Volumes and persistence

| Volume | Mounted at | Contents |
|---|---|---|
| `veloren-server-data` | `/var/lib/veloren/server` | Veloren world data |
| `dashboard-data` | `/app/.hermes/artifacts` | SQLite dashboard DB, observations, agent logs |

`docker compose down` preserves both volumes. `docker compose down -v`
removes them (world resets, observation and trail history lost).

## Production notes

- Image size: all three Dockerfiles are multi-stage. The builder stages carry
  the full Rust/Python toolchain, while the runner images ship only the
  release binary (`veloren-agent`, `veloren-server`) or the app plus runtime
  deps (`veloren-dashboard` on `python:3.12-slim`). Build once, promote the
  same image through staging to production — do not rebuild per environment.
- Resource limits: every service declares `deploy.resources.limits` (cpus +
  memory) in docker-compose.yml so server world generation cannot starve the
  agent or dashboard. Tune them to the host; the defaults assume a host with
  at least 4 CPUs and 8 GB of RAM dedicated to the stack.
- Health checks: all three services declare one. The agent's real work is
  ordered after the server via `depends_on: veloren-server` with
  `condition: service_healthy`, so the agent container never starts before the
  world is ready.
- The dashboard image runs Flask's development server (`python3 web/app.py`).
  For anything beyond local testing, run behind a TLS reverse proxy (nginx,
  Caddy, …). Bind to `0.0.0.0` only when a proxy or firewall sits in front;
  the local default is `127.0.0.1`. Agent start/stop state lives in dashboard
  memory, so it is per-worker under gunicorn — use `--workers 1` if the
  dashboard drives the agent lifecycle.
- Both viewport endpoints (`/api/viewport/latest`, `/api/viewport/trail`) are
  public by design so overlays can consume them; do not expose the dashboard
  to the open internet without TLS and access control.

## Troubleshooting (quick reference)

| Symptom | First step |
|---|---|
| `veloren-server` unhealthy / not listening | `docker compose logs veloren-server` — world generation can take several minutes |
| Agent cannot reach the server | Confirm both services are on `veloren-net`; the agent must connect to `veloren-server:14004`, not `127.0.0.1` |
| Dashboard API returns 401 | Set/send `DASHBOARD_API_KEY` as `X-API-Key` |
| `GET /api/viewport/latest` → `{"ok":false,"error":"no observations yet"}` | The agent has not emitted an observation yet — start the agent and wait for it to enter the world |
| Port 14004 or 5000 already in use | Stop the conflicting process or change the host port mapping in `docker-compose.yml` |

For local (non-Docker) issues and the full agent-connection troubleshooting
section, see [docs/troubleshooting.md](troubleshooting.md).