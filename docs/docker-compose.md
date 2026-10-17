# Running the Veloren Agent Integration stack with Docker Compose

This directory contains everything needed for single-command, full-stack
deployment:

```
docker compose up --build
```

## Services

| Service | Source | Port | Purpose |
|---------|--------|------|---------|
| `veloren-server` | `Dockerfile.server` (builds `veloren-server` from Veloren source) | 14004/tcp | Dedicated game server |
| `veloren-agent` | `Dockerfile` (builds `src/main.rs`) | — | Headless Rust agent |
| `veloren-dashboard` | `Dockerfile.dashboard` (Flask) | 5000/tcp | Monitoring dashboard UI |

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

   If `DASHBOARD_API_KEY` is unset, the dashboard generates a random key at startup and prints it to its container logs.

   `.env` is gitignored — never commit it.

2. Make sure Docker and Docker Compose v2 are installed and the daemon is running:

   ```bash
   docker --version && docker compose version
   ```

## Start the stack

```bash
docker compose up --build
```

- `--build` rebuilds all three images from their Dockerfiles.
- The dashboard becomes available at `http://localhost:5000` (public UI; API endpoints need `X-API-Key: <DASHBOARD_API_KEY>`).
- The agent connects to the server at `veloren-server:14004` over the internal Compose network.

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

## Notes

- The `veloren-server` service builds the dedicated server from the Veloren
  gitlab source (`Dockerfile.server`). If you prefer the Airshipper-provided
  `veloren-server-cli`, replace the `build:` block of `veloren-server` with an
  image containing that binary (or mount the Airshipper profiles directory and
  override the entrypoint to `veloren-server-cli --non-interactive`).
- The dashboard service mounts `dashboard-data` at `/app/.hermes/artifacts` so
  observations and agent logs survive container restarts.
- All env vars (`VELOREN_USERNAME`, `VELOREN_PASSWORD`, `VELOREN_ASSETS`,
  `DASHBOARD_API_KEY`, …) are passed through `.env` via `env_file`; none are
  hardcoded in the Compose file or images.
