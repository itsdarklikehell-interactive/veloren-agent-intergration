# Live Viewport — User Guide

The live viewport (`http://127.0.0.1:5000/viewport`) is the dashboard's
real-time 2D top-down view of the agent in the Veloren world. It shows the
agent's position, movement trail, health, chat, and recent events. No dashboard
API key is required to open it.

## Opening the viewport

- Local: `http://127.0.0.1:5000/viewport`
- Docker Compose: `http://localhost:5000/viewport`
- From the dashboard: open `http://127.0.0.1:5000/` and follow the viewport link.

## What you see

| Panel | What it shows |
|---|---|
| Top-down map | 2D view of the world around the agent: the agent's position marker plus its movement trail (position history). |
| Telemetry | Current tick, position (x, y, z), health, registration and presence status. |
| Chat | Recent chat messages from the latest observation. |
| Events | Recent non-chat events from the latest observation. |
| Connection chips | Server tmux session, port 14004 listener, agent running, registration — from `GET /api/status`. Chips show "unknown" without a valid `DASHBOARD_API_KEY`. |

## How it works

The page is plain HTML/JS — no WebSockets. It polls:

- `GET /api/viewport/latest` every ~1 second: latest observation (position, health, chat, events).
- `GET /api/viewport/trail` every ~2 seconds: position history for drawing the movement trail.
- `GET /api/status` for the connection chips.

Both viewport endpoints are **public by design**, so an overlay or OBS dashboard
can consume them without `DASHBOARD_API_KEY`. All other `GET /api/*` endpoints
require the key.

## Reading the map

- The agent marker shows the latest position from `GET /api/viewport/latest`.
- The trail is a polyline through historic observations that carry a position,
  drawn oldest → newest. Trail data is read from SQLite persistence
  (`.hermes/artifacts/dashboard.db`), so history survives dashboard restarts.
- Position and health report zero/null until the player has entered the game
  world; `presence` is `None` during character selection/loading and becomes
  `Some(Character(...))` once in-game.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Map shows "Waiting for position data…" | No observation persisted yet | Start the agent (`POST /api/agent/start`) and wait for it to enter the world — `registered` must become `true`. |
| Position marker frozen | Agent not emitting observations | Check `docker compose logs veloren-agent` or `web/agent.log`; confirm `agent_running` via `GET /api/status`. |
| Trail is empty | No position history yet | Move the agent (`POST /api/agent/command` with `{"cmd":"move",...}`) so position-bearing observations accumulate. |
| Connection chips all red/unknown | Server or agent down | Start the server (`POST /api/server/start`) and agent; confirm port 14004 accepts connections (`ss -tlnp \| grep 14004`). |
| Viewport page won't load | Dashboard not running | `python3 web/app.py` (local) or `docker compose up -d veloren-dashboard`. |

## Tips

- Polling intervals live in `web/templates/viewport.html` (`POLL_VIEWPORT` /
  `POLL_TRAIL`) if you host your own copy.
- Only the two viewport endpoints are key-free — do not expose the dashboard
  to the open internet without TLS and access control.
- Export observations for offline analysis:
  `GET /api/observations/export?format=csv&window=all` (API key required).

## Related docs

- `docs/viewport-api.md` — reference for `GET /api/viewport/latest` and
  `GET /api/viewport/trail`, including request/response schemas.
- `docs/dashboard-api.md` — reference for all authenticated dashboard endpoints
  (`/api/status`, `/api/agent/command`, `/api/analytics`, `/api/observations/export`).
