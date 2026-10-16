# API Reference

The agent speaks **JSON over stdio**. One JSON object per line on stdin (commands), one JSON object per line on stdout (observations).

## Commands (stdin → agent)

### `move`

Set the player's movement direction for subsequent ticks.

```json
{"cmd":"move","x":0.0,"y":1.0}
```

| Field | Type | Description |
|---|---|---|
| `x` | f32 | X axis movement (-1 = left, 1 = right) |
| `y` | f32 | Y axis movement (-1 = back, 1 = forward) |

### `chat`

Send a message to world chat.

```json
{"cmd":"chat","msg":"hello from hermes"}
```

| Field | Type | Description |
|---|---|---|
| `msg` | string | Chat message text |

### `command`

Send a raw server command.

```json
{"cmd":"command","name":"say","args":["hi"]}
```

| Field | Type | Description |
|---|---|---|
| `name` | string | Command name |
| `args` | string[] | Command arguments |

### `status`

No-op. The agent emits a full observation every tick regardless; this command is a placeholder for future explicit status requests.

```json
{"cmd":"status"}
```

### `quit`

Disconnect from the server and exit the agent cleanly.

```json
{"cmd":"quit"}
```

## Observations (stdout → Hermes)

One JSON object per tick:

```json
{
  "tick": 1234,
  "registered": true,
  "presence": "Some(Character(CharacterId(42)))",
  "position": [150.5, 200.3, 10.0],
  "health": {
    "current": 100.0,
    "maximum": 100.0,
    "is_dead": false
  },
  "chat": ["<Player1> hello", "Server: welcome"],
  "events": []
}
```

| Field | Type | Description |
|---|---|---|
| `tick` | u64 | Current game tick |
| `registered` | bool | Whether the client is registered with the server |
| `presence` | string | Debug format of `PresenceKind` (e.g. `Some(Character(...))` or `None`) |
| `position` | [f32, f32, f32] or null | Player position in world coordinates |
| `health` | object or null | Current health stats |
| `health.current` | f32 | Current health |
| `health.maximum` | f32 | Maximum health |
| `health.is_dead` | bool | Whether the player is dead |
| `chat` | string[] | Recent chat messages (ring buffer, last 50) |
| `events` | string[] | Non-chat events from this tick (debug format) |

## Example session

```bash
# Terminal 1: run the agent
./target/release/veloren-agent --character HERMES

# Terminal 2: send commands and read observations
echo '{"cmd":"status"}' | ./target/release/veloren-agent --character HERMES
# → {"tick":0,"registered":true,"presence":"...","position":[...],...}

echo '{"cmd":"move","x":0.0,"y":1.0}' | ./target/release/veloren-agent --character HERMES
# → agent moves forward, observations updated

echo '{"cmd":"chat","msg":"testing"}' | ./target/release/veloren-agent --character HERMES
# → chat message sent, appears in subsequent observations' chat field

echo '{"cmd":"quit"}' | ./target/release/veloren-agent --character HERMES
# → clean disconnect
```

## Hermès bridge script

`scripts/hermes-bridge.sh` is a thin wrapper that sends one command and prints the observation:

```bash
./scripts/hermes-bridge.sh '{"cmd":"status"}'
```

## Notes

- Commands are applied on the next tick; there may be up to one tick (1/60s) of latency.
- The observation chat buffer is a ring buffer of the last 50 messages.
- Position and health are `null` until the player has entered the game world.
- `presence` is `None` during character selection/loading; becomes `Some(Character(...))` once in-game.
