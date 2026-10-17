#!/usr/bin/env python3
"""Web dashboard for the Veloren Agent Integration project.

Single deployable Flask app serving a vanilla HTML/JS dashboard. All updates
are delivered by polling JSON endpoints (no WebSockets):

  GET /                dashboard page (public)
  GET /api/health      uptime, version, last observation tick (public)
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
Every endpoint except GET / and GET /api/health requires the dashboard API key.
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

from flask import Flask, jsonify, render_template, request

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

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
"""

# --------------------------------------------------------------------------- state
_state_lock = threading.RLock()
_agent_proc: subprocess.Popen | None = None
_db_conn: sqlite3.Connection | None = None


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
        try:
            line = raw.decode("utf-8", errors="replace").rstrip("\n")
        except Exception:
            continue
        if not line:
            continue
        _append_agent(line)
        try:
            obs = json.loads(line)
            if isinstance(obs, dict):
                append_observation(obs, line)
        except json.JSONDecodeError:
            pass  # non-JSON noise on stdout/stderr


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
    global _agent_proc
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
        thread = threading.Thread(target=_reader, args=(proc.stdout,), daemon=True)
        thread.start()
        return {"ok": True, "pid": proc.pid}


def stop_agent() -> dict:
    global _agent_proc
    with _state_lock:
        proc = _agent_proc
        _agent_proc = None
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
        f"(set DASHBOARD_API_KEY to override; required on all API endpoints except GET / and GET /api/health)"
    )
    print(f"  Persistence: {DB_PATH}")
    _serve(DASHBOARD_HOST, DASHBOARD_PORT)
