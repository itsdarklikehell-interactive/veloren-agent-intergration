# Hermes Integration

Connecting Hermes (A2A agent) to the Veloren agent via stdio/JSON.

## Architecture

```
┌─────────────┐     stdio/JSON      ┌──────────────────┐
│   Hermes    │◄───────────────────►│  veloren-agent   │
│  (A2A)      │                     │  (headless Rust) │
└─────────────┘                     └──────────────────┘
```

Hermes writes JSON commands to the agent's stdin and reads JSON-lines observations from stdout.

## Bridge script

`scripts/hermes-bridge.sh` is a thin wrapper:

```bash
./scripts/hermes-bridge.sh '{"cmd":"status"}'
```

It sends one JSON command to the agent's stdin and prints the observation to stdout.

## Command reference

| Command | JSON |
|---|---|
| move | `{"cmd":"move","x":0.0,"y":1.0}` |
| chat | `{"cmd":"chat","msg":"text"}` |
| command | `{"cmd":"command","name":"say","args":[]}` |
| status | `{"cmd":"status"}` |
| quit | `{"cmd":"quit"}` |

## Observation format

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

## Example: move + read observation

```bash
# Terminal 1: start the agent
./target/release/veloren-agent --character HERMES

# Terminal 2: move forward and get status
echo '{"cmd":"move","x":0.0,"y":1.0}' | ./target/release/veloren-agent --character HERMES
# → observation with updated position

echo '{"cmd":"status"}' | ./target/release/veloren-agent --character HERMES
# → latest observation
```

## Notes

- Commands are applied on the next tick (1/60s latency max).
- Chat buffer is a ring buffer of the last 50 messages.
- Position and health are `null` until the player enters the game world.
- `presence` is `None` during character selection; `Some(Character(...))` once in-game.

## Agent profile alignment

The Hermes integration is operated by the **Hermes (GLaDOS)** Kanban profile — Coordinator & Dispatcher:

- Hermes writes JSON commands to the agent's stdin and reads JSON-lines observations from stdout as part of running Developer tasks.
- Reviews `kanban_complete` submissions from the R&D Worker and QA Engineer.
- Manages `.env` credentials (never in chat) and routes them securely to the agent.
- Escalates blocked tasks (`kanban_block` with `kind=needs_input`) to the human supervisor.

Commit all changes with `[Agent: Hermes]` prefixed commit messages.
