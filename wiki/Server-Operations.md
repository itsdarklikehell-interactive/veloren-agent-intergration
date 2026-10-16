# Server Operations

Starting, stopping, and monitoring the local Veloren dedicated server.

## Start the server

```bash
cd ~/Work/veloren-agent-intergration
./scripts/start-server.sh
```

This launches `veloren-server-cli --non-interactive --no-auth` in a tmux session named `veloren-server`.

### Attach / detach / stop

| Action | Command |
|---|---|
| Attach (watch logs) | `tmux attach -t veloren-server` |
| Detach | `Ctrl-b d` |
| Stop | `./scripts/stop-server.sh` or `tmux kill-session -t veloren-server` |

## Verify the server is listening

```bash
ss -tlnp | grep 14004
```

Expected output: `veloren-server-` listening on `0.0.0.0:14004` and `[::]:14004`.

## World generation

On first start, the server generates the world. This can take 30–90 seconds. Watch the tmux pane for completion messages like `starting civilisation creation` followed by readiness indicators.

## Configuration

| Env var | Default | Purpose |
|---|---|---|
| `VELUREN_SERVER_BIN` | `~/.local/share/airshipper/profiles/default/veloren-server-cli` | Server binary path |
| `VELUREN_PORT` | `14004` | TCP port |

## Logs

Server logs are visible in the tmux session. For a text dump:

```bash
tmux capture-pane -t veloren-server -p
```

## Troubleshooting

| Symptom | Fix |
|---|---|
| Port 14004 not listening | Wait for world generation to complete; check tmux for errors |
| "Address already in use" | Stop existing server: `./scripts/stop-server.sh` |
| Server crashes immediately | Check Airshipper installation; verify `veloren-server-cli` binary exists |
