# Quick Start

Get a local Veloren server running and connect the headless agent in five minutes.

## Prerequisites

| Tool | Purpose | Install check |
|---|---|---|
| Rust (`cargo`, `rustc`) | Build the agent | `cargo --version` |
| Veloren (Airshipper) | Provides `veloren-server-cli` | `ls ~/.local/share/airshipper/profiles/default/veloren-server-cli` |
| tmux | Run server in background | `tmux -V` |

## Step 1 — Start the local dedicated server

```bash
cd ~/Work/veloren-agent-intergration
./scripts/start-server.sh
```

This launches `veloren-server-cli --non-interactive --no-auth` in a tmux session named `veloren-server`.

Attach to watch logs: `tmux attach -t veloren-server`  
Detach: `Ctrl-b d`  
Stop: `./scripts/stop-server.sh`

Verify it's listening:

```bash
ss -tlnp | grep 14004
```

You should see `veloren-server-` listening on `0.0.0.0:14004`.

## Step 2 — Build the agent

```bash
cargo build --release
```

**Build notes:** The agent depends on `veloren-client` (from Veloren's gitlab `weekly` tag). If compilation fails on stable Rust due to `nightly` feature gates in `hashbrown`/`shred`/`foldhash`, use nightly:

```bash
rustup toolchain install nightly
cargo +nightly build --release
```

## Step 3 — Run the agent

```bash
./target/release/veloren-agent --character HERMES
```

The agent will:
1. Connect to `127.0.0.1:14004`
2. Ensure a character named `HERMES` exists (creates one if missing)
3. Enter the game world
4. Emit one JSON-lines observation per tick to stdout

## Step 4 — Send commands

In another terminal:

```bash
# Get current status
echo '{"cmd":"status"}' | ./target/release/veloren-agent --character HERMES

# Move forward
echo '{"cmd":"move","x":0.0,"y":1.0}' | ./target/release/veloren-agent --character HERMES

# Send world chat
echo '{"cmd":"chat","msg":"hello from hermes"}' | ./target/release/veloren-agent --character HERMES

# Quit cleanly
echo '{"cmd":"quit"}' | ./target/release/veloren-agent --character HERMES
```

## Step 5 — Connect teloren (optional terminal frontend)

teloren is an ANSI terminal frontend for Veloren (built separately at `/tmp/teloren-probe/target/debug/teloren`):

```bash
./scripts/connect-teloren.sh HERMES
```

Attach: `tmux attach -t veloren-agent`  
Controls: `wasd` = move, `SPACE` = jump, `x` = primary attack, `z` = secondary, `g` = glide, `r` = respawn, `return` = chat, `q` = quit.

## Next steps

- Read the [Architecture](architecture.md) doc for system design
- Read the [API Reference](api-reference.md) for the full JSON command/observation spec
- Read [Troubleshooting](troubleshooting.md) for known issues
