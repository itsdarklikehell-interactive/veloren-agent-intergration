#!/usr/bin/env python3
"""Web dashboard for the Veloren Agent Integration project.

Single deployable Flask app serving a vanilla HTML/JS dashboard. All updates
are delivered by polling JSON endpoints (no WebSockets):

  GET /                dashboard page (public)
  GET /api/health      uptime, version, last observation tick (public)
  GET /metrics         Prometheus-format metrics (public scrape target)
  GET /api/metrics     same metrics as JSON (dashboard API key required)
  GET /api/status      server tmux session, port 14004, agent running, registered
  GET /api/server-logs tmux capture-pane output of the 'veloren-server' session
  GET /api/agent-logs  raw agent stdout/stderr tail
  GET /api/observations parsed JSON observations from the agent
  POST /api/server/start   start veloren-server-cli in tmux
  POST /api/server/stop    kill the tmux session
  POST /api/agent/start    start a long-lived veloren-agent process (stdin pipe)
  POST /api/agent/stop     stop the agent process
  POST /api/agent/command  send one JSON command to the agent's stdin

Authentication
--------------
Every endpoint except GET /, GET /api/health and GET /metrics requires the dashboard API key.
Send it as an `X-API-Key` header, or as `Authorization: Bearer <key>`, or as
`Authorization: Basic user:<key>`. The key is taken from DASHBOARD_API_KEY in
the environment/.env; if unset, a random key is generated at startup and printed
to stdout. Bind to 127.0.0.1 (default) and/or sit the dashboard behind a TLS
reverse proxy for production use.

Persistence
-----------
Observations and agent logs are appended to a SQLite database at
    .hermes/artifacts/dashboard.db
(gitignored runtime data), so history survives dashboard restarts. The in-memory
behaviour is unchanged; SQLite is the source of truth for API reads.

Graceful shutdown
-----------------
`python3 web/app.py` handles SIGTERM and SIGINT: it stops the Werkzeug server,
stops a running agent, and closes the database. Under gunicorn the process
manager handles signals (use --graceful-timeout).

Run:
    cd /home/rizzo/Work/veloren-agent-intergration
    python3 web/app.py            # http://127.0.0.1:5000
    # or: flask --app web/app run --host 0.0.0.0 --port 5000
    # or: gunicorn --chdir web --bind 0.0.0.0:5000 app:app
"""

from __future__ import annotations

import atexit
import base64
import json
import os
import secrets
import signal
import socket
import sqlite3
import subprocess
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

from flask import Flask, jsonify, render_template, request, Response

# --------------------------------------------------------------------------- paths
REPO_ROOT = Path(__file__).resolve().parent.parent
WEB_DIR = Path(__file__).resolve().parent
ARTIFACTS_DIR = REPO_ROOT / ".hermes" / "artifacts"
DB_PATH = ARTIFACTS_DIR / "dashboard.db"

# --------------------------------------------------------------------------- config
DASHBOARD_VERSION = os.environ.get("DASHBOARD_VERSION", "2.0.0-m2")
DASHBOARD_API_KEY = os.environ.get("DASHBOARD_API_KEY") or secrets.token_urlsafe(24)
START_TIME = time.monotonic()

SESSION = os.environ.get("VELOREN_TMUX_SESSION", "veloren-server")
PORT = int(os.environ.get("VELOREN_PORT", "14004"))
AGENT_BIN = os.environ.get(
    "VELOREN_AGENT_BIN", str(REPO_ROOT / "target" / "release" / "veloren-agent")
)
AGENT_CHARACTER = os.environ.get("VELOREN_AGENT_CHARACTER", "hermes")
ASSETS_DEFAULT = os.environ.get(
    "VELOREN_ASSETS",
    "/home/rizzo/.local/share/airshipper/profiles/default/assets",
)
DASHBOARD_HOST = os.environ.get("VELOREN_DASHBOARD_HOST", "127.0.0.1")
DASHBOARD_PORT = int(os.environ.get("VELOREN_DASHBOARD_PORT", "5000"))

AGENT_LOG = WEB_DIR / "agent.log"

# --------------------------------------------------------------------------- SQLite schema
SCHEMA = """
CREATE TABLE IF NOT EXISTS observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tick INTEGER,
    registered INTEGER,
    presence TEXT,
    position TEXT,
    health TEXT,
    chat TEXT,
    events TEXT,
    received_at TEXT NOT NULL,
    raw TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_observations_tick ON observations(tick);
CREATE INDEX IF NOT EXISTS idx_observations_received_at ON observations(received_at);

CREATE TABLE IF NOT EXISTS agent_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    line TEXT NOT NULL,
    logged_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_logs_logged_at ON agent_logs(logged_at);

CREATE TABLE IF NOT EXISTS tick_latency (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tick INTEGER NOT NULL,
    duration_seconds REAL NOT NULL,
    recorded_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tick_latency_tick ON tick_latency(tick);

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
"""

# --------------------------------------------------------------------------- state
_state_lock = threading.RLock()
_agent_proc: subprocess.Popen | None = None
_db_conn: sqlite3.Connection | None = None
_agent_start_time: float | None = None
_requests_total = 0


def db_connect() -> sqlite3.Connection:
    """Open (creating if needed) the SQLite persistence store and init schema."""
    ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH, check_same_thread=False)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.executescript(SCHEMA)
    now = datetime.now(timezone.utc).isoformat()
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?, ?)", ("version", DASHBOARD_VERSION)
    )
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?, ?)", ("started_at", now)
    )
    conn.commit()
    return conn


def db_close() -> None:
    global _db_conn
    with _state_lock:
        if _db_conn is not None:
            try:
                _db_conn.commit()
            except sqlite3.Error:
                pass
            try:
                _db_conn.close()
            except sqlite3.Error:
                pass
            _db_conn = None


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def append_observation(obs: dict, raw: str) -> None:
    """Persist one parsed observation to SQLite."""
    if _db_conn is None:
        return
    with _state_lock:
        try:
            _db_conn.execute(
                """INSERT INTO observations
                   (tick, registered, presence, position, health, chat, events, received_at, raw)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    obs.get("tick"),
                    int(registered) if isinstance((registered := obs.get("registered")), bool) else None,
                    obs.get("presence"),
                    json.dumps(obs.get("position")) if obs.get("position") is not None else None,
                    json.dumps(obs.get("health")) if obs.get("health") is not None else None,
                    json.dumps(obs.get("chat")) if isinstance(obs.get("chat"), list) else None,
                    json.dumps(obs.get("events")) if isinstance(obs.get("events"), list) else None,
                    now_iso(),
                    raw,
                ),
            )
            _db_conn.commit()
        except sqlite3.Error:  # noqa: BLE001 - persistence must never break the reader
            pass


def record_tick_latency(tick: int, duration_seconds: float) -> None:
    """Persist one tick's processing duration for baseline min/avg/max metrics."""
    if _db_conn is None:
        return
    with _state_lock:
        try:
            _db_conn.execute(
                "INSERT INTO tick_latency (tick, duration_seconds, recorded_at) VALUES (?, ?, ?)",
                (tick, duration_seconds, now_iso()),
            )
            _db_conn.commit()
        except sqlite3.Error:  # noqa: BLE001 - metrics must never break the reader
            pass


# --------------------------------------------------------------------------- init persistence
try:
    _db_conn = db_connect()
except Exception as exc:  # noqa: BLE001 - dashboard must start even if persistence is unavailable
    import sys as _sys

    print(f"[dashboard] SQLite persistence unavailable: {exc}", file=_sys.stderr)
    _db_conn = None

# --------------------------------------------------------------------------- legacy in-memory state (kept for the agent reader thread)
_agent_log_lines: list[str] = []  # mirror of recent agent output (DB is source of truth)

# --------------------------------------------------------------------------- helpers
def tmux_running() -> bool:
    """True if the Veloren server tmux session exists."""
    try:
        result = subprocess.run(
            ["tmux", "has-session", "-t", SESSION],
            capture_output=True,
            timeout=5,
        )
        return result.returncode == 0
    except Exception:
        return False


def port_listening() -> bool:
    """True if something accepts TCP connections on 127.0.0.1:PORT."""
    try:
        with socket.create_connection(("127.0.0.1", PORT), timeout=1.5):
            return True
    except OSError:
        return False


def server_logs(lines: int = 300) -> list[str]:
    """Tail of the tmux 'veloren-server' pane capture (capture-pane)."""
    try:
        result = subprocess.run(
            ["tmux", "capture-pane", "-t", SESSION, "-p", f"-S-{lines}"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            return []
        return result.stdout.splitlines()[-lines:]
    except Exception:
        return []


def _append_agent(line: str) -> None:
    """Store one agent output line: log file + SQLite."""
    with _state_lock:
        try:
            with open(AGENT_LOG, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        except OSError:
            pass
        _agent_log_lines.append(line)
        if _db_conn is not None:
            try:
                _db_conn.execute(
                    "INSERT INTO agent_logs(line, logged_at) VALUES (?, ?)",
                    (line, now_iso()),
                )
                _db_conn.commit()
            except sqlite3.Error:  # noqa: BLE001
                pass


def _reader(pipe) -> None:
    """Read agent stdout line by line; stash raw lines + parsed observations."""
    for raw in iter(pipe.readline, b""):
        start = time.perf_counter()
        try:
            line = raw.decode("utf-8", errors="replace").rstrip("\n")
        except Exception:
            continue
        if not line:
            continue
        _append_agent(line)
        tick = None
        try:
            obs = json.loads(line)
            if isinstance(obs, dict):
                append_observation(obs, line)
                tick = obs.get("tick")
        except json.JSONDecodeError:
            pass  # non-JSON noise on stdout/stderr
        duration = time.perf_counter() - start
        if isinstance(tick, int):
            record_tick_latency(tick, duration)


def agent_running() -> bool:
    with _state_lock:
        return _agent_proc is not None and _agent_proc.poll() is None


def latest_observation():
    with _state_lock:
        if _db_conn is None:
            return None
        row = _db_conn.execute(
            "SELECT raw FROM observations ORDER BY id DESC LIMIT 1"
        ).fetchone()
    return json.loads(row["raw"]) if row else None


def start_agent() -> dict:
    global _agent_proc, _agent_start_time
    with _state_lock:
        if agent_running():
            return {"ok": True, "already": True}
        if not os.path.exists(AGENT_BIN):
            return {
                "ok": False,
                "error": f"agent binary not found: {AGENT_BIN} (build with cargo build --release)",
            }
        try:
            agent_env = os.environ.copy()
            agent_env.setdefault("VELOREN_ASSETS", ASSETS_DEFAULT)
            proc = subprocess.Popen(
                [AGENT_BIN, "--character", AGENT_CHARACTER],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                env=agent_env,
            )
        except OSError as exc:
            return {"ok": False, "error": str(exc)}
        _agent_proc = proc
        _agent_start_time = time.monotonic()
        thread = threading.Thread(target=_reader, args=(proc.stdout,), daemon=True)
        thread.start()
        return {"ok": True, "pid": proc.pid}


def stop_agent() -> dict:
    global _agent_proc, _agent_start_time
    with _state_lock:
        proc = _agent_proc
        _agent_proc = None
        _agent_start_time = None
    if proc is None:
        return {"ok": True, "already": True}
    try:
        if proc.stdin and not proc.stdin.closed:
            proc.stdin.write(b'{"cmd":"quit"}\n')
            proc.stdin.flush()
    except OSError:
        pass
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
    return {"ok": True}


def send_command(command: dict) -> dict:
    with _state_lock:
        proc = _agent_proc
    if proc is None or proc.stdin is None or proc.stdin.closed:
        return {"ok": False, "error": "agent not running"}
    try:
        proc.stdin.write((json.dumps(command) + "\n").encode())
        proc.stdin.flush()
        return {"ok": True}
    except OSError as exc:
        return {"ok": False, "error": str(exc)}


# --------------------------------------------------------------------------- auth
def _extract_api_key() -> str | None:
    """Pull the API key from X-API-Key, Authorization: Bearer, or Authorization: Basic."""
    auth = request.headers.get("Authorization", "")
    if auth.startswith("Bearer "):
        return auth[len("Bearer "):].strip()
    if auth.startswith("Basic "):
        try:
            decoded = base64.b64decode(auth[len("Basic "):]).decode("utf-8", "replace")
            if ":" in decoded:
                return decoded.split(":", 1)[1].strip()
        except Exception:
            return None
    return request.headers.get("X-API-Key")


# --------------------------------------------------------------------------- app
app = Flask(
    __name__,
    template_folder=str(WEB_DIR / "templates"),
    static_folder=str(WEB_DIR / "static"),
)


@app.before_request
def _require_api_key():
    """API-key guard for every endpoint except the public ones (/, /api/health, /static/*)."""
    if (
        request.path in ("/", "/favicon.ico")
        or request.path == "/api/health"
        or request.path == "/metrics"
        or request.path.startswith("/static/")
    ):
        return None
    key = _extract_api_key()
    if not key or not secrets.compare_digest(key, DASHBOARD_API_KEY):
        return (
            jsonify({"error": "unauthorized"}),
            401,
            {"WWW-Authenticate": 'Bearer realm="veloren-dashboard"'},
        )


@app.after_request
def _count_requests(response):
    """Count every served request for dashboard_requests_total."""
    global _requests_total
    with _state_lock:
        _requests_total += 1
    return response


@app.get("/")
def index():
    return render_template("dashboard.html", api_key=DASHBOARD_API_KEY)


@app.get("/api/health")
def health():
    last_tick = None
    with _state_lock:
        if _db_conn is not None:
            row = _db_conn.execute(
                "SELECT tick FROM observations ORDER BY id DESC LIMIT 1"
            ).fetchone()
            last_tick = row["tick"] if row else None
    return jsonify(
        {
            "status": "ok",
            "uptime_seconds": round(time.monotonic() - START_TIME, 3),
            "version": DASHBOARD_VERSION,
            "last_observation_tick": last_tick,
        }
    )


def metrics_payload() -> dict:
    """Compute the Prometheus metric values as a plain dict.

    Shared by GET /metrics (text exposition format) and GET /api/metrics (JSON).
    All values are computed from live state + the SQLite persistence store so the
    tick-latency baseline survives dashboard restarts.
    """
    with _state_lock:
        requests_total = _requests_total
        agent_start = _agent_start_time

    connected = 1 if port_listening() else 0

    obs_total = 0
    last_tick = None
    durations: list[float] = []
    if _db_conn is not None:
        with _state_lock:
            row = _db_conn.execute("SELECT COUNT(*) AS n FROM observations").fetchone()
            obs_total = row["n"] if row else 0
            row = _db_conn.execute("SELECT MAX(tick) AS m FROM observations").fetchone()
            last_tick = row["m"] if row else None
            rows = _db_conn.execute(
                "SELECT duration_seconds FROM tick_latency ORDER BY id DESC LIMIT 10000"
            ).fetchall()
            durations = [row["duration_seconds"] for row in rows]

    uptime = 0.0
    if agent_start is not None:
        uptime = max(0.0, time.monotonic() - agent_start)

    latency: dict = {}
    if durations:
        latency = {
            "min": min(durations),
            "avg": sum(durations) / len(durations),
            "max": max(durations),
            "count": len(durations),
        }

    return {
        "agent_observations_total": obs_total,
        "agent_tick_latency_seconds": latency,
        "server_connected": connected,
        "agent_uptime_seconds": uptime,
        "last_observation_tick": last_tick,
        "dashboard_requests_total": requests_total,
    }


def _format_metric(name: str, documentation: str, typ: str, value_lines: list[str]) -> list[str]:
    lines: list[str] = []
    lines.append(f"# HELP {name} {documentation}")
    lines.append(f"# TYPE {name} {typ}")
    lines.extend(value_lines)
    return lines


@app.get("/metrics")
def metrics():
    """Prometheus text-format metrics; scrape target for Prometheus."""
    payload = metrics_payload()
    lat = payload["agent_tick_latency_seconds"] or {}
    last_tick = payload["last_observation_tick"]

    lines: list[str] = []
    lines.extend(
        _format_metric(
            "agent_observations_total",
            "Total number of agent observations received and persisted",
            "counter",
            [f"agent_observations_total {payload['agent_observations_total']}"],
        )
    )
    if lat:
        lines.extend(
            _format_metric(
                "agent_tick_latency_seconds",
                "Tick processing latency baseline in seconds (min/avg/max over persisted samples)",
                "gauge",
                [
                    f'agent_tick_latency_seconds{{stat="min"}} {lat["min"]:.6f}',
                    f'agent_tick_latency_seconds{{stat="avg"}} {lat["avg"]:.6f}',
                    f'agent_tick_latency_seconds{{stat="max"}} {lat["max"]:.6f}',
                ],
            )
        )
    else:
        lines.extend(
            _format_metric(
                "agent_tick_latency_seconds",
                "Tick processing latency baseline in seconds (min/avg/max over persisted samples)",
                "gauge",
                [
                    'agent_tick_latency_seconds{stat="min"} 0',
                    'agent_tick_latency_seconds{stat="avg"} 0',
                    'agent_tick_latency_seconds{stat="max"} 0',
                ],
            )
        )
    lines.extend(
        _format_metric(
            "server_connected",
            "Whether the game server port is accepting connections (1 = yes, 0 = no)",
            "gauge",
            [f"server_connected {payload['server_connected']}"],
        )
    )
    lines.extend(
        _format_metric(
            "agent_uptime_seconds",
            "Seconds the agent process has been running in this dashboard session (0 when stopped)",
            "gauge",
            [f"agent_uptime_seconds {payload['agent_uptime_seconds']:.3f}"],
        )
    )
    lines.extend(
        _format_metric(
            "last_observation_tick",
            "Tick number of the most recently received agent observation",
            "gauge",
            [f"last_observation_tick {last_tick if last_tick is not None else 0}"],
        )
    )
    lines.extend(
        _format_metric(
            "dashboard_requests_total",
            "Total number of HTTP requests served by the dashboard",
            "counter",
            [f"dashboard_requests_total {payload['dashboard_requests_total']}"],
        )
    )
    body = "\n".join(lines) + "\n"
    return Response(body, content_type="text/plain; version=0.0.4; charset=utf-8")


@app.get("/api/metrics")
def api_metrics():
    """JSON rendering of the Prometheus metrics (dashboard API key required).

    Same values as GET /metrics, returned as JSON for dashboard clients:
    observation throughput, tick-latency baseline (min/avg/max), server
    connectivity, agent uptime, last observation tick and request count.
    """
    return jsonify(metrics_payload())


@app.get("/api/status")
def status():
    with _state_lock:
        last = latest_observation()
    return jsonify(
        {
            "session": SESSION,
            "port": PORT,
            "server_running": tmux_running(),
            "port_listening": port_listening(),
            "agent_running": agent_running(),
            "registered": bool(last and last.get("registered")),
            "last_observation": last,
        }
    )


@app.get("/api/server-logs")
def srv_logs():
    return jsonify({"lines": server_logs(300)})


@app.get("/api/agent-logs")
def agt_logs():
    with _state_lock:
        lines: list[str] = []
        if _db_conn is not None:
            rows = _db_conn.execute(
                "SELECT line FROM agent_logs ORDER BY id DESC LIMIT 300"
            ).fetchall()
            lines = [row["line"] for row in reversed(rows)]
    return jsonify({"lines": lines})


@app.get("/api/observations")
def agt_obs():
    with _state_lock:
        data: list[dict] = []
        if _db_conn is not None:
            rows = _db_conn.execute(
                "SELECT raw FROM observations ORDER BY id DESC LIMIT 100"
            ).fetchall()
            data = [json.loads(row["raw"]) for row in reversed(rows)]
    return jsonify({"observations": data})


@app.post("/api/server/start")
def server_start():
    script = REPO_ROOT / "scripts" / "start-server.sh"
    try:
        result = subprocess.run(
            ["bash", str(script)], capture_output=True, text=True, timeout=20
        )
        output = (result.stdout or "") + (result.stderr or "")
        return jsonify({"ok": result.returncode == 0, "output": output.strip()})
    except Exception as exc:  # noqa: BLE001 - report to dashboard UI
        return jsonify({"ok": False, "error": str(exc)}), 500


@app.post("/api/server/stop")
def server_stop():
    try:
        result = subprocess.run(
            ["tmux", "kill-session", "-t", SESSION], capture_output=True, text=True, timeout=10
        )
        output = (result.stdout or "") + (result.stderr or "")
        return jsonify({"ok": result.returncode == 0, "output": output.strip()})
    except Exception as exc:  # noqa: BLE001
        return jsonify({"ok": False, "error": str(exc)}), 500


@app.post("/api/agent/start")
def agent_start():
    return jsonify(start_agent())


@app.post("/api/agent/stop")
def agent_stop():
    return jsonify(stop_agent())


@app.post("/api/agent/command")
def agent_command():
    data = request.get_json(silent=True) or {}
    cmd = data.get("cmd")
    if not cmd:
        return jsonify({"ok": False, "error": "missing cmd"}), 400
    message: dict = {"cmd": cmd}
    for key in ("x", "y", "msg", "name", "args"):
        if key in data:
            message[key] = data[key]
    return jsonify(send_command(message))


# --------------------------------------------------------------------------- graceful shutdown
_shutdown = threading.Event()
_httpd = None


def _handle_signal(signum, frame):
    _shutdown.set()


signal.signal(signal.SIGTERM, _handle_signal)
signal.signal(signal.SIGINT, _handle_signal)


def _serve(host: str, port: int) -> None:
    """Run the Werkzeug dev server until SIGTERM/SIGINT, then shut down gracefully."""
    global _httpd
    from werkzeug.serving import make_server

    _httpd = make_server(host, port, app, threaded=True)
    server_thread = threading.Thread(target=_httpd.serve_forever, daemon=True)
    server_thread.start()
    _shutdown.wait()  # blocks until a signal arrives
    _httpd.shutdown()
    server_thread.join(timeout=5)
    stop_agent()
    db_close()


def _cleanup() -> None:
    stop_agent()
    db_close()


atexit.register(_cleanup)


if __name__ == "__main__":
    print(f"Veloren Agent Dashboard v{DASHBOARD_VERSION}")
    print(f"  Listening on http://{DASHBOARD_HOST}:{DASHBOARD_PORT}")
    print(
        f"  API key: {DASHBOARD_API_KEY}  "
        f"(set DASHBOARD_API_KEY to override; required on all API endpoints except GET /, GET /api/health and GET /metrics)"
    )
    print(f"  Persistence: {DB_PATH}")
    _serve(DASHBOARD_HOST, DASHBOARD_PORT)
