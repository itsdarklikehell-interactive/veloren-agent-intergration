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
