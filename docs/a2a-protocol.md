# Agent-to-Agent protocol for veloren-agent

## Goal
Place a WebSocket HTTP server on the headless veloren-agent so that other
agents (Hermes, OpenClaw, your own harness) can:

- send commands to the agent (`/api/agent/<name>/command`)
- receive JSON-lines observations (`/api/agent/<name>/observations`)
- check agent status (`/api/agent/<name>/status`)

The agent speaks **JSON over stdio** today (commands on stdin, observations on
stdout). The WS layer is a thin routing shim: it accepts JSON commands on the
wire, forwards them to the same stdin/stdout pipeline, and broadcasts
observations back to subscribers. No game logic changes.

## Wire format (JSON, one object per frame)

### Command
```json
{
  "cmd": "move",
  "x": 0.0,
  "y": 1.0
}
```
or
```json
{"cmd": "chat", "msg": "hello from teammate"}
```
or
```json
{"cmd": "command", "name": "say", "args": ["hi"]}
```

### Observation (echoed back to subscribers)
```json
{
  "schema_version": 1,
  "tick": 1234,
  "registered": true,
  "presence": "Character",
  "position": [150.5, 200.3, 10.0],
  "health": {"current": 100.0, "maximum": 100.0, "is_dead": false},
  "chat": ["<Player1> hello"],
  "events": []
}
```

## Endpoint design

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/api/agent/<name>/command` | `X-API-Key` | Send one command to agent `<name>` |
| `GET` | `/api/agent/<name>/status` | none | Status + last observation |
| `WS` | `/api/agent/<name>/observations` | none | JSON-lines observations stream |
| `GET` | `/api/health` | none | Liveliness probe |
| `GET` | `/metrics` | none | Prometheus text exposition |

## Auth
- `DASHBOARD_API_KEY` (same key as dashboard) for `/api/agent/*` and `/metrics`.
- `GET /` and `GET /api/health` are public.

## Behavior notes
- Commands are applied on the next tick: up to ~1/60s latency.
- Subscribers get every observation from the subscribed agent, including
  internal messages (e.g., `chat`, `events`).
- The WS layer never blocks the game tick loop. Commands are queued and
  processed by the existing `cmd_rx` stdin thread.
