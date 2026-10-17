# Dashboard API Reference

The Veloren dashboard (`veloren-dashboard`, Flask, `http://127.0.0.1:5000`)
exposes a JSON HTTP API for operating and observing the agent and server.
Every response is JSON.

## Base URL

Local: `http://127.0.0.1:5000` (default bind; see `VELOREN_DASHBOARD_HOST` /
`VELOREN_DASHBOARD_PORT`). In Docker Compose the dashboard is published on
`localhost:5000`; from inside the Compose network it is reachable at
`veloren-dashboard:5000`.

## Authentication

Every `GET /api/*` endpoint except `GET /api/health` requires the dashboard
API key. Send it with any of:

```bash
curl -H "X-API-Key: <key>" http://127.0.0.1:5000/api/status
curl -H "Authorization: Bearer <key>" http://127.0.0.1:5000/api/analytics
curl -u "user:<key>" http://127.0.0.1:5000/api/health   # Basic auth also accepted
```

The key is read from `DASHBOARD_API_KEY` (`.env` or environment). If unset, a
random key is generated at startup and printed to stdout — it is never logged
or embedded in HTML. The browser UI reads the key from
`localStorage["veloren_dashboard_api_key"]` (a prompt offers to store it on
first request).

Public (no key): `GET /`, `GET /api/health`, `GET /metrics`,
`GET /api/viewport/*`. See `docs/viewport-api.md` for the public viewport
endpoints.

## Endpoints

### GET /api/status

Server tmux session, port 14004 listener, agent process, registration status,
and the last observation. Polled by the viewport page for the connection chips.

**Requires API key.**

**Response (200)**

```json
{
  "session": "veloren-server",
  "port": 14004,
  "server_running": true,
  "port_listening": true,
  "agent_running": true,
  "registered": true,
  "last_observation": { ... }
}
```

| Field | Type | Description |
|---|---|---|
| `session` | string | tmux session name running `veloren-server-cli` |
| `port` | u16 | Game server port (`VELOREN_PORT`, default 14004) |
| `server_running` | boolean | Whether the tmux session exists |
| `port_listening` | boolean | Whether port 14004 accepts TCP connections |
| `agent_running` | boolean | Whether the headless agent process is running |
| `registered` | boolean | Whether the latest observation shows the client registered |
| `last_observation` | object or null | Most recent agent observation (see `docs/api-reference.md`) |

### POST /api/agent/command

Send one JSON command to the agent's stdin (`move`, `chat`, `command`,
`status`, `quit`).

**Requires API key.**

**Request**

```http
POST /api/agent/command HTTP/1.1
Content-Type: application/json
X-API-Key: <key>

{
  "cmd": "move",
  "x": 0.0,
  "y": 1.0
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `cmd` | string | yes | One of `move`, `chat`, `command`, `status`, `quit` |
| `x` | f32 | no | `move`: X axis (-1 = left, 1 = right) |
| `y` | f32 | no | `move`: Y axis (-1 = back, 1 = forward) |
| `msg` | string | no | `chat`: message text |
| `name` | string | no | `command`: server command name |
| `args` | string[] | no | `command`: command arguments |

**Response (200)**

```json
{ "ok": true }
```

**Response (400)**

```json
{ "ok": false, "error": "missing cmd" }
```

Returned when `cmd` is missing. Commands are applied on the next tick — allow
up to one tick (~1/60s) of latency. See `docs/api-reference.md` for the full
command/observation spec.

### GET /api/analytics

Observation statistics and time series computed entirely from the SQLite
persistence store (`.hermes/artifacts/dashboard.db`), so numbers survive
dashboard restarts.

**Requires API key.**

**Query params**

| Param | Type | Default | Description |
|---|---|---|---|
| `window` | string | `all` | `all` | `<N>m` | `<N>h` | `<N>d` lookback window |
| `limit` | int | 20000 | Rows scanned, clamped to 1–200000 |

**Response (200)**

```json
{
  "window": "all",
  "limit": 20000,
  "observations": {
    "total": 1234,
    "registered": 1100,
    "registered_pct": 89.14,
    "peak_per_minute": 57
  },
  "presence": { "Some(Character(...))": 1000, "None": 234 },
  "health": {
    "avg_current": 92.5,
    "avg_maximum": 100.0,
    "min_current": 0.0,
    "dead": 3,
    "with_health": 1200,
    "latest": { "current": 100.0, "maximum": 100.0, "is_dead": false }
  },
  "activity": {
    "chat_messages": 450,
    "events": 120,
    "event_types": { "Tick": 100, "Emote": 20 }
  },
  "ticks": {
    "min": 100,
    "max": 9000,
    "span": 8900
  },
  "latency": {
    "min": 0.001,
    "avg": 0.0034,
    "max": 0.012,
    "p50": 0.003,
    "p95": 0.008,
    "count": 5000
  },
  "series": {
    "per_minute": [{ "t": "2026-10-18T10:00", "n": 42 }],
    "health": [{ "t": "...", "current": 100.0, "maximum": 100.0 }],
    "latency": [{ "t": "...", "avg_ms": 3.4 }]
  },
  "generated_at": "2026-10-18T10:00:00Z"
}
```

**Response (503)**

```json
{ "error": "persistence unavailable" }
```

Returned when SQLite persistence is unavailable.

### GET /api/observations/export  (a.k.a. `/api/export`)

Export persisted observations as CSV or JSON. Mounted at
`/api/observations/export`; commonly shortened to `/api/export`.

**Requires API key.**

**Query params**

| Param | Type | Default | Description |
|---|---|---|---|
| `format` | string | `json` | `csv` or `json` |
| `window` | string | `all` | `all` | `<N>m` | `<N>h` | `<N>d` |
| `limit` | int | 10000 | Rows exported, clamped to 1–200000 |

**CSV response**

Content-Type: `text/csv; charset=utf-8` with header
`Content-Disposition: attachment; filename="observations-<timestamp>.csv"`.

Columns: `id`, `tick`, `received_at`, `registered`, `presence`, `x`, `y`,
`z`, `health_current`, `health_maximum`, `is_dead`, `chat_count`,
`events_count`.

**JSON response**

```json
{
  "format": "json",
  "count": 42,
  "window": "all",
  "exported_at": "2026-10-18T10:00:00Z",
  "observations": [ ... ]
}
```

**Response (503)**

```json
{ "error": "persistence unavailable" }
```

## Example: poll from the shell

```bash
export DASHBOARD_API_KEY=<your key>
BASE=http://127.0.0.1:5000

# Status
curl -s -H "X-API-Key: $DASHBOARD_API_KEY" $BASE/api/status | python3 -m json.tool

# Send a move command
curl -s -X POST $BASE/api/agent/command \
  -H "X-API-Key: $DASHBOARD_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"cmd":"move","x":0.0,"y":1.0}' | python3 -m json.tool

# Analytics for the last 24 hours
curl -s -H "X-API-Key: $DASHBOARD_API_KEY" "$BASE/api/analytics?window=24h" \
  | python3 -m json.tool

# Export the last 6 hours as CSV
curl -s -H "X-API-Key: $DASHBOARD_API_KEY" \
  "$BASE/api/observations/export?format=csv&window=6h" \
  -o observations-2026.csv
```

## Related docs

- `docs/viewport-api.md` — public endpoints `GET /api/viewport/latest` and
  `GET /api/viewport/trail` (no API key).
- `docs/live-viewport.md` — user guide for the live viewport page (`GET /viewport`).
- `docs/api-reference.md` — agent JSON-over-stdio commands and observations spec.
- `docs/docker-compose.md` — running the full stack, including the dashboard.
