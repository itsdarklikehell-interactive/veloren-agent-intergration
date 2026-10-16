# Veloren Agent Integration

A coding project integrating agents with the Veloren world.

## Current State

### What's working

| Component | Status | Details |
|---|---|---|
| Airshipper launcher | Installed | `airshipper` 0.17.0 |
| Veloren build | Installed | weekly `f97be347` (2026-10-14) |
| Game client | Present | `veloren-voxygen` (133 MB) |
| Dedicated server CLI | Present | `veloren-server-cli` (83 MB) |
| Local server | Running | `veloren-server-cli --non-interactive --no-auth` on port 14004 |
| teloren (terminal frontend) | Built | `/tmp/teloren-probe/target/debug/teloren` |
| Raw TCP to server | Working | Python and Rust std can connect to 127.0.0.1:14004 |

### What's blocked

| Issue | Detail | Workaround |
|---|---|---|
| teloren connection | hickory-resolver `lookup_host` fails on `127.0.0.1` even though raw TCP works | Try `no_proxy=*`, or patch teloren to use raw TcpStream |
| veloren-agent Rust build | `hashbrown`/`shred` use unstable `nightly` features; `veloren-common-base` needs `CARGO_WORKSPACE_DIR` | Requires nightly Rust or extensive crate patching |

## Architecture

Veloren's engine is deliberately modular — the graphical frontend is entirely decoupled from the client library (`veloren-client`). teloren proves this: it's an ANSI terminal frontend using the same `veloren-client` library as the 3D client (`veloren-voxygen`).

**Recommended agent architecture:**

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

The agent would:
1. Use `veloren-client` to connect to the local `veloren-server-cli`
2. Run the game tick loop headless (no GPU/display needed)
3. Expose a stdio/JSON command interface for Hermes
4. Emit JSON-lines observations (position, health, chat, entities)

## Quick start

### 1. Start the local dedicated server

```bash
cd ~/Work/veloren-agent-intergration
./scripts/start-server.sh
```

This starts `veloren-server-cli --non-interactive --no-auth` in a tmux session named `veloren-server`.

Attach: `tmux attach -t veloren-server`
Detach: `Ctrl-b d`
Stop: `tmux kill-session -t veloren-server`

### 2. Verify the server is listening

```bash
ss -tlnp | grep 14004
```

### 3. Connect teloren (terminal frontend)

teloren is built at `/tmp/teloren-probe/target/debug/teloren`. It may fail to resolve `127.0.0.1` due to a hickory-resolver issue. Try:

```bash
# Try with no_proxy
no_proxy=* /tmp/teloren-probe/target/debug/teloren \
  --server 127.0.0.1:14004 \
  --username hermes \
  --character hermes \
  --password ''
```

If DNS resolution fails, raw TCP still works — a Python or custom client can connect directly.

### 4. Build the veloren-agent (Rust headless frontend)

```bash
cd ~/Work/veloren-agent-intergration
cargo build --release
```

**Note:** The Rust build currently fails on stable Rust due to `nightly` feature usage in `hashbrown` and `shred` (transitive dependencies of `veloren-client`). Options:
- Use nightly Rust (`rustup toolchain install nightly`)
- Patch `hashbrown`, `shred`, and `foldhash` in the cargo registry
- Write the agent in Python using the raw TCP connection

## Files

| Path | Purpose |
|---|---|
| `Cargo.toml` | Rust project manifest (depends on `veloren-client` from gitlab) |
| `src/main.rs` | Headless agent frontend (stdin JSON commands, stdout JSON-lines observations) |
| `README.md` | This file |
| `scripts/start-server.sh` | Starts `veloren-server-cli` in tmux |
| `scripts/hermes-bridge.sh` | Thin shell bridge for Hermes to send commands to the agent |

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

## Next steps

1. Fix teloren's hickory-resolver connection issue (try `no_proxy=*` or patch to use raw `TcpStream::connect`)
2. Resolve the Rust nightly feature build issues (or write the agent in Python)
3. Create the Hermes↔agent bridge (stdio JSON or TCP)
4. Add automated tests for server start/stop and agent connect/disconnect

## Repository

- Local: `~/Work/veloren-agent-intergration`
- Remote: `https://github.com/itsdarklikehell/veloren-agent-intergration`
- Branch: `main`
