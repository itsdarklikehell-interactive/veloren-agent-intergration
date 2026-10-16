# Agent Runbook

Running the headless Veloren agent, sending commands, and reading observations.

## Build the agent

```bash
cd ~/Work/veloren-agent-intergration
cargo build --release
```

## Run the agent

```bash
./target/release/veloren-agent --character HERMES
```

The agent connects to `127.0.0.1:14004`, ensures a character named `HERMES` exists, enters the game, and emits one JSON-lines observation per tick to stdout.

## Send commands

Each command is a single JSON object on stdin. Use the bridge script or pipe directly:

```bash
# Via bridge script
./scripts/hermes-bridge.sh '{"cmd":"status"}'

# Direct
echo '{"cmd":"move","x":0.0,"y":1.0}' | ./target/release/veloren-agent --character HERMES
echo '{"cmd":"chat","msg":"hello from hermes"}' | ./target/release/veloren-agent --character HERMES
echo '{"cmd":"quit"}' | ./target/release/veloren-agent --character HERMES
```

## Command reference

| Command | JSON | Effect |
|---|---|---|
| move | `{"cmd":"move","x":0.0,"y":1.0}` | Set movement direction |
| chat | `{"cmd":"chat","msg":"text"}` | Send world chat |
| command | `{"cmd":"command","name":"say","args":[]}` | Send raw server command |
| status | `{"cmd":"status"}` | Observation emitted every tick anyway |
| quit | `{"cmd":"quit"}` | Disconnect and exit |

## Observation reference

Each tick, the agent writes one JSON object to stdout:

```json
{
  "tick": 1234,
  "registered": true,
  "presence": "Some(Character(...))",
  "position": [150.5, 200.3, 10.0],
  "health": {"current":100.0,"maximum":100.0,"is_dead":false},
  "chat": ["<Player> hello"],
  "events": []
}
```

## Environment variables

| Env var | Default | Purpose |
|---|---|---|
| `VELUREN_AGENT_BIN` | `./target/release/veloren-agent` | Agent binary path |
| `VELUREN_AGENT_CHARACTER` | `hermes` | Character name |

## Troubleshooting

| Symptom | Fix |
|---|---|
| Build fails on stable Rust | Use `cargo +nightly build` or patch registry crates |
| Agent can't connect | Ensure server is running on port 14004 |
| No observations | Agent may still be in character selection; wait for `presence` to become `Some(Character(...))` |
| `presence` stays `None` | Character creation may have failed; check server logs in tmux |
