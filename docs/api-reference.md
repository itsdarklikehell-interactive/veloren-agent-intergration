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

## Prometheus Metrics

### `GET /metrics`

Prometheus text exposition format (v0.0.4). Public endpoint — no dashboard API key required; intended as a Prometheus scrape target.

| Metric | Type | Labels | Meaning |
|--------|------|--------|---------|
| `agent_observations_total` | counter | — | Total agent observations received and persisted to SQLite |
| `agent_tick_latency_seconds` | gauge | `stat="min"\|"avg"\|"max"` | Tick processing latency baseline in seconds, computed over the last 10,000 persisted tick latencies |
| `server_connected` | gauge | — | `1` if the game server port (`VELOREN_PORT`, default 14004) accepts TCP connections, else `0` |
| `agent_uptime_seconds` | gauge | — | Seconds the agent process has been running in this dashboard session; `0` when stopped |
| `last_observation_tick` | gauge | — | Tick number of the most recent agent observation |
| `dashboard_requests_total` | counter | — | Total HTTP requests served by the dashboard |

Tick processing latency is measured per observation in the agent reader thread (decode → parse → persist) and stored in the `tick_latency` table of `.hermes/artifacts/dashboard.db`, so the min/avg/max baseline survives dashboard restarts.

### Prometheus scrape config

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: veloren-dashboard
    scrape_interval: 15s
    # /metrics is public; no authorization header needed
    static_configs:
      - targets: ["127.0.0.1:5000"]
```

### Example queries

```promql
# Latest observed game tick
last_observation_tick

# Is the game server port reachable right now?
server_connected

# Average tick-processing latency over the last hour
avg_over_time(agent_tick_latency_seconds{stat="avg"}[1h])

# Dashboard request rate
rate(dashboard_requests_total[5m])
```
