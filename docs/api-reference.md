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

## Server lifecycle auto-recovery
If the connection to the Veloren server is lost (`client.tick()` returns an error),
the agent does not exit. Instead it enters an auto-recovery loop:

1. **Detect** — a tick error signals the connection is lost.
2. **Back off** — wait for an initial 1 second, then double the backoff after each
   failed attempt, capped at 60 seconds.
3. **Reconnect** — create a new `Client`, re-authenticate with the same
   credentials, and run `ensure_character()` to find or re-create the character.
4. **Resume** — once the agent is back in-game (`presence` becomes `Some`),
   the tick loop resumes and observations flow again.

The outer recovery loop only exits on a clean `{"cmd":"quit"}` command. This
means transient server restarts, network blips, or brief port unavailability are
handled automatically without operator intervention.

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

### `GET /api/metrics`

JSON rendering of the same metrics exposed by `GET /metrics`, for dashboard clients (and anything that prefers JSON over Prometheus text format). Requires the dashboard API key (`X-API-Key` header or `Authorization: Bearer`), like the other `GET /api/*` endpoints.

Response:

```json
{
  "agent_observations_total": 1234,
  "agent_tick_latency_seconds": { "min": 0.0012, "avg": 0.0034, "max": 0.0120, "count": 5000 },
  "server_connected": 1,
  "agent_uptime_seconds": 3661.5,
  "last_observation_tick": 9000,
  "dashboard_requests_total": 842
}
```

When no tick-latency samples have been persisted yet, `agent_tick_latency_seconds` is `{}` and the dashboard renders "—" for latency values.

## Dashboard tabs

The dashboard (`GET /`) has five tabs:

- **Status** — server tmux session, port 14004, agent process, registration, last observation.
- **Control Panel** — start/stop server and agent, send JSON commands to the agent's stdin, live observations.
- **Server Logs** — tmux `capture-pane` tail of the `veloren-server` session.
- **Agent Logs** — stdout/stderr tail of the `veloren-agent` process.
- **Performance** — Prometheus metrics from `GET /metrics` (observation throughput, tick-latency min/avg/max baseline with a rolling sparkline, server connectivity, agent uptime, dashboard request count), polled every 5s. No game server required.
