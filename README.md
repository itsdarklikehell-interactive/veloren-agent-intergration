# Veloren Agent Integration

A coding project integrating agents with the Veloren world.

**Status:** Early development — local server runs, agent frontend written, CI and docs configured. Agent workflow is Kanban-driven (see [KANBAN.md](KANBAN.md)).

## What this project does

Veloren's engine is deliberately modular: the graphical frontend is entirely decoupled from the client library (`veloren-client`). This project builds a **headless agent frontend** that:

1. Uses `veloren-client` to connect to a local `veloren-server-cli` instance.
2. Runs the game tick loop without rendering (no GPU, no display, no Wayland/X11 needed).
3. Exposes a **stdio/JSON command interface** that Hermes (or any agent harness) can drive: move, chat, command, status, quit.
4. Emits **JSON-lines observations**: tick, position, health, chat, events.

```
┌─────────────┐     stdio/JSON      ┌──────────────────┐
│   Hermes    │◄───────────────────►│  veloren-agent   │
│  (A2A)      │                     │  (headless Rust) │
└─────────────┘                     └────────┬─────────┘
                                             │ veloren-client API
                                             ▼
                                    ┌──────────────────┐
                                    │ veloren-server   │
                                    │  (local, port    │
                                    │   14004)         │
                                    └──────────────────┘
```

## Documentation

| Document | Description |
|---|---|
| [Quick Start](docs/quickstart.md) | Get a local server + agent running in 5 minutes |
| [Architecture](docs/architecture.md) | System design and data flow |
| [API Reference](docs/api-reference.md) | JSON commands and observations spec |
| [Troubleshooting](docs/troubleshooting.md) | Known issues and workarounds |
| [Contributing](docs/contributing.md) | How to contribute to this project |
| [Wiki Home](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki) | Project wiki (operations, runbooks, recipes) |

## Agent Infrastructure — Kanban-based workflow

This repository is operated by autonomous agents under the Hermes Kanban infrastructure. Agent personas and the Kanban lifecycle are defined in [`SOUL.md`](SOUL.md) and [`AGENTS.md`](AGENTS.md); task claiming and completion are explained in [`KANBAN.md`](KANBAN.md).

### Agent personas

| Profile | Role | Focus |
|---|---|---|
| **Hermes (GLaDOS)** — Coordinator & Dispatcher | Kanban dispatcher, task assignment, PR review, credential management, escalation | Assigning tasks, reviewing `kanban_complete` submissions, managing `.env` credentials, escalating blocked tasks to the human supervisor |
| **R&D Worker** — Autonomous Senior R&D Developer | Execution engine within Kanban tasks | Claiming tasks via `kanban_show`, signaling liveness via `kanban_heartbeat`, completing via `kanban_complete`, blocking via `kanban_block` |
| **Hermes Kanban QA Tester** — Technical Product Manager | Requirements ingestion, decomposition, task seeding | Pulling triage tasks, creating Developer + QA tasks via `kanban_create`, linking via `kanban_link`, promoting to ready |
| **QA Engineer** — Autonomous Senior QA Automation Engineer | Independent validation against Acceptance Criteria | Black-box/integration testing, security scans, performance baselines, `kanban_complete` (greenlight) or `kanban_block` (`kind=failed_qa`) |

### Kanban lifecycle

Tasks flow: **triage → todo → ready → running → done**. Blocked tasks move to a **blocked** state with a reason (`dependency` or `needs_input`).

1. **PM ingests** a requirement from triage, decomposes it into vertical slices, creates Developer + QA tasks, links dependencies, promotes to ready.
2. **R&D Worker claims** a ready task, runs `kanban_show` for context, executes in an isolated workspace, verifies with tests, completes via `kanban_complete`.
3. **QA Engineer validates** the completed work against Acceptance Criteria, runs independent tests, completes or blocks via `kanban_block`.
4. **Hermes reviews** completions, merges approved work, escalates blockers to the human supervisor.

Agents communicate through commit messages prefixed with `[Agent: <Profile-Name>]`, the Kanban toolset (`kanban_show`, `kanban_create`, `kanban_link`, `kanban_complete`, `kanban_block`, `kanban_heartbeat`), issue comments, and pull request descriptions.


### Wiki pages by primary operator

| Wiki page | Primary operator | Purpose |
|---|---|---|
| [Server Operations](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/Server-Operations) | R&D Worker | Start/stop/monitor the local Veloren dedicated server |
| [Agent Runbook](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/Agent-Runbook) | R&D Worker | Build, run, and command the headless agent |
| [Teloren Setup](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/Teloren-Setup) | R&D Worker | Connect the terminal frontend to the local server |
| [CI Configuration](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/CI-Configuration) | Hermes (GLaDOS) | GitHub Actions shellcheck/shfmt, nightly and Docker-sandbox CI |
| [Build Troubleshooting](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/Build-Troubleshooting) | R&D Worker (QA validates fixes) | Fixing Rust build issues on stable toolchain |
| [Hermes Integration](https://github.com/itsdarklikehell/veloren-agent-intergration/wiki/Hermes-Integration) | Hermes (GLaDOS) | Connecting Hermes (A2A) to the agent via stdio/JSON |

## Quick start (tl;dr)

```bash
# 1. Clone the repo
git clone https://github.com/itsdarklikehell/veloren-agent-intergration.git
cd veloren-agent-intergration

# 2. Start the local dedicated server
./scripts/start-server.sh

# 3. Build the agent (requires Rust; see build notes below)
cargo build --release

# 4. Run the agent
./target/release/veloren-agent --character HERMES

# 5. Send a command (in another terminal)
echo '{"cmd":"status"}' | ./target/release/veloren-agent --character HERMES
```


## Web Dashboard

A self-contained Flask dashboard for operating and observing the agent and
server lives under `web/`. It polls JSON endpoints (no WebSockets) and renders
a vanilla HTML/JS interface:

- **Status** — tmux session `veloren-server`, port 14004 listener, agent process, registration status
- **Server logs** — `tmux capture-pane` output of the `veloren-server` session
- **Agent logs** — raw stdout/stderr of the headless `veloren-agent` process
- **Control panel** — start/stop the server and agent, send JSON commands
  (move, chat, command, status, quit) to the agent's stdin, watch live JSON-lines observations

### Run

```bash
cd /home/rizzo/Work/veloren-agent-intergration/web
python3 -m pip install -r requirements.txt   # flask, python-dotenv
python3 app.py                               # http://127.0.0.1:5000
```

### Endpoints

| Endpoint | Method | Description |
|---|---|---|
| `/` | GET | Dashboard page (public, no auth) |
| `/api/health` | GET | Uptime, version, last observation tick (public, no auth) |
| `/api/status` | GET | tmux session, port 14004, agent running, registered status |
| `/api/server-logs` | GET | Tmux capture-pane tail of `veloren-server` |
| `/api/agent-logs` | GET | Raw agent stdout/stderr tail |
| `/api/observations` | GET | Parsed JSON-lines observations from the agent |
| `/api/server/start` | POST | Start `veloren-server-cli` in tmux (`scripts/start-server.sh`) |
| `/api/server/stop` | POST | Kill the tmux session |
| `/api/agent/start` | POST | Start `veloren-agent` with a stdin pipe |
| `/api/agent/stop` | POST | Send `quit` and stop the agent process |
| `/api/agent/command` | POST | Send one JSON command to the agent's stdin |

### Authentication

Every endpoint except `GET /` and `GET /api/health` requires the dashboard API
key. The key is read from `DASHBOARD_API_KEY` (`.env` or environment); if unset,
a random key is generated at startup and printed to stdout. Send it with any of:

```bash
curl -H "X-API-Key: <key>" http://127.0.0.1:5000/api/status
curl -H "Authorization: Bearer <key>" http://127.0.0.1:5000/api/observations
curl -u "user:<key>" http://127.0.0.1:5000/api/health   # Basic auth also accepted
```

`GET /api/health` is intentionally unauthenticated so load balancers and
monitoring can poll it. The dashboard page embeds the key so the browser UI can
poll the API; bind to `127.0.0.1` (the default) or put a TLS reverse proxy in
front for network-facing deployments.

Credentials (`VELOREN_USERNAME`/`VELOREN_PASSWORD`) are read from `.env` and
forwarded to the agent process; they are never exposed by the dashboard.

### Production (gunicorn)

For anything beyond local testing, run the dashboard under [gunicorn](https://gunicorn.org)
instead of Flask's dev server:

```bash
# 1. Install gunicorn
python3 -m pip install gunicorn

# 2. Configure the dashboard (copy .env.example to .env and fill in values)
cp .env.example .env
# set at least: DASHBOARD_API_KEY (required on all API endpoints except / and /api/health)

# 3. Run (from the repo root)
cd /home/rizzo/Work/veloren-agent-intergration
set -a; . .env; set +a
gunicorn --chdir web --bind 0.0.0.0:5000 --workers 2 --threads 4 --timeout 30 \
  --access-logfile - --error-logfile - app:app
```

Notes:

- **Bind address** — the dashboard defaults to `127.0.0.1:5000`; the gunicorn example above binds `0.0.0.0:5000` for container/proxy deployments. Override with `VELOREN_DASHBOARD_HOST` / `VELOREN_DASHBOARD_PORT`.
- **Workers** — agent start/stop state (`POST /api/agent/*`) lives in memory, so it is per-worker. Use `--workers 1` if you rely on the dashboard to start and stop the agent; use more workers with a single-writer agent lifecycle handled externally. SQLite (`WAL` mode) is shared safely across workers for observations and logs.
- **Persistence** — observations and agent logs are written to `.hermes/artifacts/dashboard.db` (gitignored); the directory must be writable by the gunicorn process.
- **Graceful shutdown** — gunicorn handles SIGTERM/SIGINT with its `--graceful-timeout`; the dev server (`python3 web/app.py`) handles signals itself.
- **TLS** — terminate TLS in a reverse proxy (nginx, Caddy, or similar) in front of gunicorn; the dashboard itself serves HTTP only.

Example systemd unit (`/etc/systemd/system/veloren-dashboard.service`):

```ini
[Unit]
Description=Veloren Agent Dashboard
After=network.target

[Service]
Type=simple
WorkingDirectory=/home/rizzo/Work/veloren-agent-intergration
EnvironmentFile=/home/rizzo/Work/veloren-agent-intergration/.env
ExecStart=gunicorn --chdir web --bind 0.0.0.0:5000 --workers 2 --threads 4 --timeout 30 app:app
Restart=always
User=rizzo

[Install]
WantedBy=multi-user.target
```

## Prerequisites

| Tool | Purpose | Install check |
|---|---|---|
| Rust (`cargo`, `rustc`) | Build the agent | `cargo --version` |
| Veloren (via Airshipper) | Provides `veloren-server-cli` | `ls ~/.local/share/airshipper/profiles/default/veloren-server-cli` |
| tmux | Run server in background | `tmux -V` |

## Build status

The Rust agent (`src/main.rs`) compiles against `veloren-client` from the Veloren gitlab `weekly` tag. On the current stable Rust toolchain, the build is blocked on `nightly` feature usage in transitive dependencies (`hashbrown`, `shred`, `foldhash`). Workarounds:

- Use nightly Rust: `rustup toolchain install nightly && cargo +nightly build`
- Patch the registry crates (see [Troubleshooting](docs/troubleshooting.md))
- Write the agent in Python using the raw TCP connection (TCP to `127.0.0.1:14004` works fine)

The shell scripts (`scripts/`) are CI-validated with shellcheck and shfmt.

## CI

GitHub Actions CI runs on every push and pull request to `main`, using the shared
[itsdarklikehell/ci-templates](https://github.com/itsdarklikehell/ci-templates)
`ci-shell.yml` reusable workflow. It validates all `*.sh` files with shellcheck and shfmt.

See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Repository structure

```
veloren-agent-intergration/
├── .github/workflows/ci.yml        # GitHub Actions CI (reusable template)
├── Cargo.toml                      # Rust project manifest
├── Cargo.lock                      # Locked dependencies
├── README.md                       # This file
├── KANBAN.md                       # Kanban agent workflow: claim/complete/block tasks
├── SOUL.md                         # Agent profiles & Kanban personas
├── AGENTS.md                       # Agent interaction guidelines
├── docs/
│   ├── quickstart.md               # Step-by-step getting started
│   ├── architecture.md             # System design + data flow
│   ├── api-reference.md            # JSON command/observation spec
│   ├── troubleshooting.md          # Known issues + fixes
│   └── contributing.md             # Contributor guide
├── web/
│   ├── app.py                      # Flask dashboard: API-key auth, SQLite persistence, /api/health, graceful shutdown
│   ├── requirements.txt            # flask, python-dotenv
│   ├── templates/dashboard.html
│   └── static/                     # app.js, style.css
├── scripts/
│   ├── start-server.sh             # Start veloren-server-cli in tmux
│   ├── stop-server.sh              # Stop the local server
│   ├── connect-teloren.sh          # Connect teloren terminal frontend
│   └── hermes-bridge.sh            # stdio JSON bridge for Hermes
├── pipeline/
│   ├── run-pipeline.sh             # R&D agent recursive loop executor
│   └── cron-weekly-sync.sh         # Weekly R&D sync (GitHub + Linear)
├── .hermes/cron/rd-agent-cron      # Cron schedule for weekly R&D sync
└── src/
    └── main.rs                     # Headless agent frontend
```

## teloren controls (when connected)

| Key | Action |
|---|---|
| wasd | Move |
| SPACE | Jump |
| x | Primary attack |
| z | Secondary attack |
| g | Toggle glide |
| r | Respawn |
| return | Open chat |
| q | Quit |

## Repository

- Local: `~/Work/veloren-agent-intergration`
- Remote: `https://github.com/itsdarklikehell/veloren-agent-intergration`
- Branch: `main`
