//! Integration tests for the Veloren agent connection lifecycle.
//!
//! These tests spawn a small mock TCP server speaking a line-based JSON
//! protocol (hello -> registered, tick -> observation) and drive the agent's
//! tick loop (`run_agent`) against a `GameConnection` backed by that socket.
//! This verifies connect/auth, observation parsing, auto-reconnect on
//! connection loss, and the connect-retry ("wait for server") logic without
//! needing a real Veloren server or the real auth service.

use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    sync::mpsc,
    thread,
    time::Duration,
};
use serde_json::Value;
use veloren_agent::{AgentOutcome, Connector, GameConnection, HealthObs, connect_with_retry, run_agent};
use vek::Vec2;

// -------- mock TCP server ------------------------------------------------

/// A mock Veloren server. Each accepted connection:
/* hello -> {"type":"registered","tick":0}
   tick   -> {"type":"observation",...}
   close/EOF -> close connection
   `close_after` makes the server close the connection after N ticks, which
   simulates connection loss for the auto-reconnect test.
*/
pub struct MockServer {
    addr: SocketAddr,
    _accept_thread: Option<std::thread::JoinHandle<()>>,
}

impl MockServer {
    pub fn bind(close_after: u64) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let limit = Arc::new(AtomicU64::new(close_after));
        let accept_thread = thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let limit = Arc::clone(&limit);
                let _ = thread::spawn(move || handle_connection(stream, limit));
            }
        });
        Ok(Self { addr, _accept_thread: Some(accept_thread) })
    }

    pub fn addr(&self) -> SocketAddr { self.addr }
}

fn handle_connection(stream: TcpStream, limit: Arc<AtomicU64>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    let mut ticks = 0u64;
    while let Some(line) = read_line(&mut reader) {
        if line.is_empty() { continue; }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "hello" => {
                write_line(&mut writer, r#"{"type":"registered","tick":0}"#);
            }
            "tick" => {
                ticks += 1;
                let obs = format!(
                    "{{\"type\":\"observation\",\"tick\":{},\"position\":[10.0,20.0,5.0],\
                     \"health\":{{\"current\":100.0,\"maximum\":100.0,\"is_dead\":false}},\
                     \"chat\":[],\"events\":[]}}",
                    ticks
                );
                write_line(&mut writer, &obs);
                if ticks >= limit.load(Ordering::Relaxed) {
                    break; // simulate connection loss
                }
            }
            "close" => break,
            _ => {}
        }
    }
}

fn read_line(reader: &mut impl BufRead) -> Option<String> {
    let mut buf = String::new();
    match reader.read_line(&mut buf) {
        Ok(0) => None,
        Ok(_) => Some(buf.trim_end().to_string()),
        Err(_) => None,
    }
}

fn write_line(writer: &mut impl Write, line: &str) {
    let _ = writeln!(writer, "{line}");
    let _ = writer.flush();
}

// -------- mock connection + connector (implement the lib traits) ----------

pub struct MockConnection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    last_tick: u64,
    last_position: Option<[f32; 3]>,
    last_health: Option<HealthObs>,
}

impl MockConnection {
    pub fn connect(addr: SocketAddr) -> Result<Self, String> {
        let stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let reader = BufReader::new(stream);
        let mut conn = MockConnection { writer, reader, last_tick: 0, last_position: None, last_health: None };
        write_line(&mut conn.writer, r#"{"type":"hello","username":"test-agent"}"#);
        let Some(line) = read_line(&mut conn.reader) else {
            return Err("server did not respond to hello".to_string());
        };
        let value: Value = serde_json::from_str(&line).unwrap_or_default();
        if value.get("type").and_then(Value::as_str) != Some("registered") {
            return Err(format!("unexpected response to hello: {line}"));
        }
        Ok(conn)
    }
}

impl GameConnection for MockConnection {
    fn tick(&mut self, _move_dir: Vec2<f32>, _dt: f64) -> Result<Vec<veloren_client::Event>, String> {
        write_line(&mut self.writer, r#"{"type":"tick"}"#);
        let Some(line) = read_line(&mut self.reader) else {
            return Err("connection lost".to_string());
        };
        let value: Value = serde_json::from_str(&line).unwrap_or_default();
        if value.get("type").and_then(Value::as_str) != Some("observation") {
            return Err("connection lost".to_string());
        }
        self.last_tick = value.get("tick").and_then(Value::as_u64).unwrap_or(0);
        self.last_position = value.get("position").and_then(|p| p.as_array()).map(|arr| {
            [
                arr[0].as_f64().unwrap_or(0.0) as f32,
                arr[1].as_f64().unwrap_or(0.0) as f32,
                arr[2].as_f64().unwrap_or(0.0) as f32,
            ]
        });
        self.last_health = value.get("health").map(|h| HealthObs {
            current: h.get("current").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            maximum: h.get("maximum").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            is_dead: h.get("is_dead").and_then(Value::as_bool).unwrap_or(false),
        });
        Ok(vec![])
    }
    fn registered(&self) -> bool { true }
    fn presence(&self) -> Option<String> { Some("Character".into()) }
    fn get_tick(&self) -> u64 { self.last_tick }
    fn position(&self) -> Option<[f32; 3]> { self.last_position }
    fn health(&self) -> Option<HealthObs> { self.last_health.clone() }
    fn send_chat(&mut self, msg: String) {
        let _ = writeln!(self.writer, "{}", serde_json::to_string(&msg).unwrap());
        let _ = self.writer.flush();
    }
    fn send_command(&mut self, _name: String, _args: Vec<String>) {}
    fn cleanup(&mut self) {}
    fn logout(&mut self) {
        let _ = write_line(&mut self.writer, r#"{"type":"close"}"#);
    }
}

pub struct MockConnector {
    addr: SocketAddr,
    fail_limit: AtomicU64,
    calls: AtomicU64,
}

impl MockConnector {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr, fail_limit: AtomicU64::new(0), calls: AtomicU64::new(0) }
    }
    pub fn set_fail_limit(&self, n: u64) { self.fail_limit.store(n, Ordering::SeqCst); }
    pub fn attempts(&self) -> u64 { self.calls.load(Ordering::SeqCst) }
}

impl Connector for MockConnector {
    type Connection = MockConnection;
    fn connect(&self) -> Result<MockConnection, String> {
        let calls = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if calls <= self.fail_limit.load(Ordering::SeqCst) {
            return Err("server not up yet".to_string());
        }
        MockConnection::connect(self.addr)
    }
}

// -------- test helpers ---------------------------------------------------

fn quit_after(delay: Duration, tx: &mpsc::Sender<String>) {
    let tx = tx.clone();
    thread::spawn(move || {
        thread::sleep(delay);
        let _ = tx.send(r#"{"cmd":"quit"}"#.to_string());
    });
}

fn parse_observations(out: &str) -> Vec<Value> {
    out.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn is_well_formed_observation(v: &Value) -> bool {
    v.get("schema_version") == Some(&Value::from(1u32))
        && v.get("registered") == Some(&Value::from(true))
        && v.get("tick").and_then(Value::as_u64).is_some()
        && v.get("position").and_then(Value::as_array).is_some()
}

// -------- tests ----------------------------------------------------------

#[test]
fn test_connect_auth_and_observations() {
    // Connect to a mock server (auth handshake) and verify the agent emits
    // well-formed observations containing schema_version/registered/tick.
    let server = MockServer::bind(u64::MAX).unwrap();
    let connector = MockConnector::new(server.addr());
    let mut conn = connect_with_retry(&connector, 5, Duration::from_millis(10)).unwrap();
    assert!(conn.registered(), "agent should be registered after auth handshake");

    let (tx, rx) = mpsc::channel();
    quit_after(Duration::from_millis(100), &tx);

    let mut out: Vec<u8> = Vec::new();
    let outcome = run_agent(&mut conn, &mut out, &rx, Duration::from_secs(10), None);
    assert_eq!(outcome, AgentOutcome::Quit, "agent should quit cleanly on command");

    let observations = parse_observations(&String::from_utf8_lossy(&out));
    assert!(!observations.is_empty(), "expected at least one observation on stdout");
    assert!(
        observations.iter().any(is_well_formed_observation),
        "expected a well-formed observation (schema_version=1, registered=true, tick set);\n got {observations:?}"
    );
}

#[test]
fn test_auto_reconnect_on_connection_loss() {
    // Server closes after 3 ticks -> agent must detect connection loss and
    // report ConnectionLost so the caller can reconnect.
    let loss_server = MockServer::bind(3).unwrap();
    let connector = MockConnector::new(loss_server.addr());
    let mut conn = connect_with_retry(&connector, 3, Duration::from_millis(10)).unwrap();

    let (_tx, rx) = mpsc::channel(); // no quit command; wait for the server to close
    let mut out: Vec<u8> = Vec::new();
    let outcome = run_agent(&mut conn, &mut out, &rx, Duration::from_secs(10), None);
    assert_eq!(
        outcome, AgentOutcome::ConnectionLost,
        "agent should report ConnectionLost when the server closes the connection"
    );

    // Reconnect to a fresh (still-up) mock server and resume cleanly.
    let resume_server = MockServer::bind(u64::MAX).unwrap();
    let connector2 = MockConnector::new(resume_server.addr());
    let mut conn2 = connect_with_retry(&connector2, 3, Duration::from_millis(10)).unwrap();
    assert!(conn2.registered(), "reconnected agent should be registered");

    let (tx2, rx2) = mpsc::channel();
    quit_after(Duration::from_millis(100), &tx2);
    let mut out2: Vec<u8> = Vec::new();
    let outcome2 = run_agent(&mut conn2, &mut out2, &rx2, Duration::from_secs(10), None);
    assert_eq!(outcome2, AgentOutcome::Quit, "reconnected agent should quit cleanly");

    let observations = parse_observations(&String::from_utf8_lossy(&out2));
    assert!(!observations.is_empty(), "reconnected agent should emit observations");
    assert!(
        observations.iter().any(is_well_formed_observation),
        "expected a well-formed observation after reconnect;\n got {observations:?}"
    );
}

#[test]
fn test_connect_retry_waits_for_server() {
    // connect_with_retry must keep retrying until the server accepts, then
    // return a connected session.
    let server = MockServer::bind(u64::MAX).unwrap();
    let connector = MockConnector::new(server.addr());
    connector.set_fail_limit(2); // fail first two connect() attempts

    let conn = connect_with_retry(&connector, 10, Duration::from_millis(5)).unwrap();
    assert!(conn.registered(), "connected session should be registered");
    assert_eq!(connector.attempts(), 3, "should have retried until the server accepted");
}
