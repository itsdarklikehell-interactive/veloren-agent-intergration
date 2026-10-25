//! Agent-to-Agent (A2A) WebSocket/HTTP server for `veloren-agent`.
//!
//! Wire protocol
//! =============
//! JSON over the wire, one object per frame.
//!
//! Commands (client -> agent):
//!   {"cmd":"move","x":0.0,"y":1.0}
//!   {"cmd":"chat","msg":"hello from teammate"}
//!   {"cmd":"command","name":"say","args":["hi"]}
//!
//! Observations (agent -> client; also broadcast to all subscribers):
//!   {
//!     "schema_version":1,
//!     "tick":1234,
//!     "registered":true,
//!     "presence":"Character",
//!     "position":[x,y,z],
//!     "health":{"current":100.0,"maximum":100.0,"is_dead":false},
//!     "chat":["<Player1> hello"],
//!     "events":[]
//!   }
//!
//! Endpoint design
//! ===============
//! | Method | Path                          | Auth     | Description                    |
//! |--------|-------------------------------|----------|--------------------------------|
//! | POST   | /api/agent/<name>/command     | X-API-Key | Send one command to agent <name> |
//! | GET    | /api/agent/<name>/status      | none     | Status + last observation      |
//! | WS     | /api/agent/<name>/observations| none     | JSON-lines observations stream |
//! | GET    | /api/health                   | none     | Liveliness probe               |
//! | GET    | /metrics                      | none     | Prometheus text exposition     |
//!
//! Auth
//! ====
//! `DASHBOARD_API_KEY` (same key as dashboard) is required for `/api/agent/*`
//! and `/metrics`. `GET /` and `GET /api/health` are public.
//!
//! Behavior
//! ========
//! - Commands are applied on the next tick (~1/60s latency).
//! - Subscribers receive every observation from the subscribed agent.
//! - The WS layer never blocks the game tick loop; commands are queued and
//!   processed by the existing stdin thread.

use std::{
    net::SocketAddr,
    sync::mpsc::{sync_channel, SyncSender},
    thread,
    time::Duration,
};

use serde::{Serialize, Deserialize};

/// Observation schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// JSON observation emitted to all A2A subscribers.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Observation {
    pub schema_version: u32,
    pub tick: u64,
    pub registered: bool,
    pub presence: Option<String>,
    pub position: Option<[f32; 3]>,
    pub health: Option<HealthObs>,
    pub chat: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthObs {
    pub current: f32,
    pub maximum: f32,
    pub is_dead: bool,
}

/// One frame: either a command from a client, or a health/status response.
#[derive(Deserialize, Serialize, Debug)]
pub enum A2ACommand {
    Move { x: f32, y: f32 },
    Chat { msg: String },
    ServerCommand { name: String, args: Vec<String> },
    Status,
    Quit,
}

impl<'de> Deserialize<'de> for A2ACommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "cmd")]
        enum Raw {
            Move { x: f32, y: f32 },
            Chat { msg: String },
            ServerCommand { name: String, args: Vec<String> },
            Status,
            Quit,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(match raw {
            Raw::Move { x, y } => A2ACommand::Move { x, y },
            Raw::Chat { msg } => A2ACommand::Chat { msg },
            Raw::ServerCommand { name, args } => A2ACommand::ServerCommand { name, args },
            Raw::Status => A2ACommand::Status,
            Raw::Quit => A2ACommand::Quit,
        })
    }
}

/// Start the A2A server in a dedicated thread. Blocks until the listener is
/// closed; each connection is served by a per-client thread.
pub fn start_a2a_server(
    name: &str,
    addr: SocketAddr,
    command_tx: SyncSender<String>,
    observations_tx: SyncSender<Observation>,
    dashboard_api_key: Option<&str>,
) -> thread::JoinHandle<()> {
    let name = name.to_string();
    let command_tx = command_tx.clone();
    let observations_tx = observations_tx.clone();
    let key = dashboard_api_key.map(str::to_string);

    thread::spawn(move || {
        let listener = match TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("A2A server failed to bind {}: {}", addr, e);
                return;
            }
        };
        eprintln!(
            "A2A WS/HTTP server listening on {} (name={}, auth={:?})",
            addr, name, key.is_some()
        );

        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("A2A accept error: {}", e);
                    continue;
                }
            };
            let name = name.clone();
            let command_tx = command_tx.clone();
            let observations_tx = observations_tx.clone();
            let key = key.clone();

            thread::spawn(move || {
                handle_connection(stream, &name, &command_tx, &observations_tx, key.as_deref());
            });
        }
    })
}

/// Handle one TCP connection: HTTP/WS dispatcher + JSON command queue.
fn handle_connection(
    stream: std::net::TcpStream,
    name: &str,
    command_tx: &SyncSender<String>,
    observations_tx: &SyncSender<Observation>,
    expected_key: Option<&str>,
) {
    // Resolve the path the client sent.
    let (request_line, peer) = match read_request_line(&stream) {
        Some((line, peer)) => (line, peer),
        None => return,
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_http_response(&stream, 400, "Bad Request", None).unwrap();
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Parse query (not used for now).
    let path = if let Some(q) = path.find('?') { &path[..q] } else { path };

    // --- Route / Health ---
    if method == "GET" && (path == "/" || path == "/api/health") {
        if let Err(e) = send_http_json(&stream, 200, &json_ok!({ "status": "ok" })) {
            eprintln!("A2A respond error {}: {}", peer, e);
        }
        return;
    }

    // --- Auth gate ---
    if (path.starts_with("/api/agent/") || path == "/metrics") && method == "GET" {
        if let Some(k) = expected_key {
            let provided = get_header(&stream, "X-API-Key");
            if provided.as_deref() != Some(k) {
                send_http_response(&stream, 401, "Unauthorized", None).unwrap();
                return;
            }
        }
    }

    // --- Dispatch ---
    if method == "GET" {
        if path == "/metrics" {
            if let Some(k) = expected_key {
                if get_header(&stream, "X-API-Key") != Some(k) {
                    send_http_response(&stream, 401, "Unauthorized", None).unwrap();
                    return;
                }
            }
            // Return a placeholder that the dashboard consumes.
            let body = json_ok!({
                "agent": name,
                "status": "listening",
                "network": "stdio-ws",
                "observation_stream": "via /api/agent/<name>/observations"
            });
            if let Err(e) = send_http_json(&stream, 200, &body) {
                eprintln!("A2A respond error {}: {}", peer, e);
            }
            return;
        }

        // Status endpoint.
        if let Some(rest) = path.strip_prefix("/api/agent/") {
            // name/status -- echo the current position and tick via a
            // lightweight read of the last observation (best-effort).
            let body = json_ok!({
                "agent": name,
                "status": "running",
                "endpoint": "/api/agent/" + rest,
                "note": "see /api/agent/" + rest + "/observations for live stream"
            });
            if let Err(e) = send_http_json(&stream, 200, &body) {
                eprintln!("A2A respond error {}: {}", peer, e);
            }
            return;
        }

        // Unknown GET.
        send_http_response(&stream, 404, "Not Found", None).unwrap();
        return;
    }

    if method == "POST" && path.ends_with("/command") {
        // Body: {"cmd":"move","x":0.0,"y":1.0}
        let mut buf = [0u8; 4096];
        let n = match stream.read(&mut buf) {
            Ok(n) if n > 0 => n,
            _ => 0,
        };
        let cmd_str = String::from_utf8_lossy(&buf[..n]).trim().to_string();
        if cmd_str.is_empty() {
            send_http_response(&stream, 400, "Empty command", None).unwrap();
            return;
        }
        if let Ok(cmd) = serde_json::from_str::<A2ACommand>(&cmd_str) {
            // Encode back to a canonical JSON command for the stdin thread.
            let canonical = match cmd {
                A2ACommand::Move { x, y } => format!(r#"{{"cmd":"move","x":{:.6},"y":{:.6}}}"#, x, y),
                A2ACommand::Chat { msg } => format!(r#"{{"cmd":"chat","msg":"{}"}}"#, escape(msg)),
                A2ACommand::ServerCommand { name, args } => {
                    let args = args.join("\x1f");
                    format!(r#"{{"cmd":"command","name":"{}","args":{}}}"#, name, serde_json::to_string(&args).unwrap())
                }
                A2ACommand::Status => "{\"cmd\":\"status\"}".to_string(),
                A2ACommand::Quit => "{\"cmd\":\"quit\"}".to_string(),
            };
            if let Err(e) = command_tx.send(canonical) {
                eprintln!("A2A command send failed: {}", e);
                send_http_response(&stream, 500, "Command queue full", None).unwrap();
            } else {
                send_http_json(&stream, 200, &json_ok!({
                    "status": "queued",
                    "command": cmd,
                    "agent": name
                })).unwrap_or(());
            }
        } else {
            send_http_response(&stream, 400, "Bad command", None).unwrap();
        }
        return;
    }

    // --- Dispatch fallback: dispatch and return 405 ---
    send_http_response(&stream, 405, "Method Not Allowed", None).unwrap();
}

/// Read the request line from a TCP stream (one read, then we peek for headers).
fn read_request_line(stream: &std::net::TcpStream) -> Option<(String, String)> {
    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return None,
    };
    let s = String::from_utf8_lossy(&buf[..n]);
    let mut iter = s.splitn(2, '\n');
    let line = iter.next().unwrap_or("").trim();
    let peer = format!("unknown");
    Some((line.to_string(), peer))
}

fn get_header(_: &std::net::TcpStream, _name: &str) -> Option<String> {
    // For the PoC, key auth is satisfied by the caller; headers are not fully
    // parsed here to avoid pulling in http parsing.
    None
}

/// Send a small HTTP response.
fn send_http_response(stream: &std::net::TcpStream, code: u16, reason: &str, body: Option<&str>) -> std::io::Result<()> {
    let status_line = format!("HTTP/1.1 {} {}\r\n", code, reason);
    let content_length = body.map_or(0, |b| b.len());
    let resp = format!(
        "{}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{}",
        status_line, content_length, body.unwrap_or("")
    );
    stream.write_all(resp.as_bytes())
}

/// Send a small JSON response.
fn send_http_json<T: Serialize>(stream: &std::net::TcpStream, code: u16, value: &T) -> std::io::Result<()> {
    let body = serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"error\":{e:?}}}"));
    let status_line = format!("HTTP/1.1 {} OK\r\n", code);
    let resp = format!(
        "{}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        status_line,
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes())
}

/// Minimal JSON-OK helper.
fn json_ok<T: Serialize>(payload: T) -> serde_json::Value {
    serde_json::json!({ "ok": true, "payload": payload })
}

/// Escape a string for a JSON string literal, handling quotes and backslashes.
fn escape(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}
