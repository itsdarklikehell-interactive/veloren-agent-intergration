# Viewport API

The Veloren Agent dashboard exposes two public HTTP endpoints for the live
viewport (the 2D top-down map at `GET /viewport`):

- `GET /api/viewport/latest` — the latest agent observation: tick, position,
  health, registration, presence, recent chat and events
- `GET /api/viewport/trail` — position history for drawing the agent's
  movement trail on the viewport map

Both endpoints are **public** — no dashboard API key is required. They are
intended to be polled by the viewport page or any external client (overlay,
stream dashboard, monitoring).

## Base URL

Local: `http://127.0.0.1:5000` (default bind; see `VELOREN_DASHBOARD_HOST` /
`VELOREN_DASHBOARD_PORT`). In Docker Compose the dashboard is published on
`localhost:5000`; from inside the Compose network it is reachable at
`veloren-dashboard:5000`.

## GET /api/viewport/latest

Returns the most recent agent observation in viewport-friendly shape. The
viewport page polls this every ~1 second.

**Request**

```http
GET /api/viewport/latest HTTP/1.1
Host: 127.0.0.1:5000
```

No headers or query parameters are required.

**Response (200)**

```json
{
  "ok": true,
  "tick": 1234,
  "x": 150.5,
  "y": 200.3,
  "z": 10.0,
  "health_current": 100.0,
  "health_maximum": 100.0,
  "is_dead": false,
  "registered": true,
  "presence": "Some(Character(CharacterId(42)))",
  "chat": ["<Player1> hello", "Server: welcome"],
  "events": []
}
```

| Field | Type | Description |
|---|---|---|
| `ok` | boolean | `true` when an observation is available |
| `tick` | u64 or null | Current game tick |
| `x, y, z` | f32 | Player position in world coordinates |
| `health_current` | f32 or null | Current health |
| `health_maximum` | f32 or null | Maximum health |
| `is_dead` | boolean | Whether the player is dead |
| `registered` | boolean | Whether the client is registered with the server |
| `presence` | string or null | Debug format of `PresenceKind`; `None` during character selection/loading |
| `chat` | string[] | Last 8 chat messages from the latest observation |
| `events` | string[] | Last 8 non-chat events from the latest observation |

Position and health report `0` / `null` until the player has entered the game
world; `presence` is `None` until the agent has a character in-game.

**Response (no observations yet)**

```json
{
  "ok": false,
  "error": "no observations yet"
}
```

Returned until the agent has emitted at least one observation. Clients should
treat this as "waiting for data" and retry on the next poll.

## GET /api/viewport/trail

Returns the position trail — historic observations that carry a position —
for drawing the agent's movement path on the viewport map.

**Request**

```http
GET /api/viewport/trail?limit=300 HTTP/1.1
Host: 127.0.0.1:5000
```

| Query param | Type | Default | Description |
|---|---|---|---|
| `limit` | int | 300 | Maximum number of trail points to return |

**Response (200)**

```json
{
  "ok": true,
  "trail": [
    { "tick": 1230, "x": 148.2, "y": 201.0, "z": 10.0 },
    { "tick": 1231, "x": 149.1, "y": 200.6, "z": 10.0 },
    { "tick": 1232, "x": 150.0, "y": 200.4, "z": 10.0 }
  ]
}
```

| Field | Type | Description |
|---|---|---|
| `ok` | boolean | `true` when trail data is available |
| `trail[].tick` | u64 | Game tick of the observation |
| `trail[].x, .y, .z` | f32 | Position at that tick |

Points are returned oldest → newest so a client can draw a continuous
polyline. Only observations with a non-null position are included. Trail data
is read from the SQLite persistence store (`.hermes/artifacts/dashboard.db`),
so history survives dashboard restarts.

If persistence is unavailable:

```json
{
  "ok": false,
  "error": "persistence unavailable"
}
```

## Related endpoints

| Endpoint | Auth | Purpose |
|---|---|---|
| `GET /viewport` | None | Live viewport page (polls `/api/viewport/latest` and `/api/viewport/trail`) |
| `GET /api/status` | Dashboard API key | Server tmux session, port 14004, agent process, registration — polled by the viewport page for the connection chips |
| `GET /api/health` | None | Dashboard uptime, version, last observation tick |

## Example: poll from the shell

```bash
# Latest observation
curl -s http://127.0.0.1:5000/api/viewport/latest | python3 -m json.tool

# Position trail (last 100 points)
curl -s "http://127.0.0.1:5000/api/viewport/trail?limit=100" | python3 -m json.tool
```

## Notes

- Both viewport endpoints are intentionally unauthenticated so a public
  overlay or OBS dashboard can consume them without `DASHBOARD_API_KEY`. All
  other `GET /api/*` endpoints require the key (`X-API-Key` header or
  `Authorization: Bearer`).
- The viewport page polls `latest` every ~1s and `trail` every ~2s; tune
  `POLL_VIEWPORT` / `POLL_TRAIL` in `web/templates/viewport.html` if you host
  your own copy.
- Observations are persisted to SQLite (WAL mode); `docker compose down -v`
  removes the `dashboard-data` volume and wipes the trail history.