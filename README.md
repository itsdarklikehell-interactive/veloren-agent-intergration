# Veloren Agent Integration

A coding project integrating agents with the Veloren world.

**Status:** Early development — local server runs, agent frontend written, CI and docs configured.

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
