//! Library facade for the Veloren agent.
//!
//! Extracts the agent's connection lifecycle and tick loop from `src/main.rs`
//! so it can be exercised by integration tests (which spawn a mock TCP server)
//! while the production binary keeps using the real `veloren_client::Client`.

use std::{
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use specs::WorldExt;
use tokio::runtime::Runtime;
use veloren_client::{
    Client, ClientType, CredentialProvider, Credentials, Event,
    addr::ConnectionArgs,
};
use veloren_common::{
    comp::{self, ControllerInputs},
    clock::Clock,
    ViewDistances,
};
use serde::{Deserialize, Serialize};
use vek::Vec2;

/// Observation schema version, emitted as `schema_version` in every JSON observation.
pub const SCHEMA_VERSION: u32 = 1;

/// Default ticks between automatic keepalive pings sent by the underlying client.
pub const TICKS_PER_SEC: f64 = 60.0;

/// How long the agent may go without a successful tick before the connection is
/// considered dead and a reconnect is triggered (keepalive watchdog).
pub const DEFAULT_HEARTBEAT_TIMEOUT_SEC: u64 = 10;

/// Ticks between automatic chat replies (prevents reply loops/spam).
pub const AUTO_RESPONSE_COOLDOWN_TICKS: u64 = 300;

/// World-unit distance at which a patrol waypoint counts as reached.
pub const PATROL_REACH_THRESHOLD: f32 = 0.5;

/// Health observation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthObs {
    pub current: f32,
    pub maximum: f32,
    pub is_dead: bool,
}

/// JSON observation emitted to Hermes (stdout).
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

/// Outcome of running the agent tick loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOutcome {
    /// The agent received a `{"cmd":"quit"}` command and shut down cleanly.
    Quit,
    /// The connection was lost (tick error or keepalive watchdog timeout) and
    /// the caller should reconnect.
    ConnectionLost,
}

/// Keepalive watchdog: tracks the last time the connection made progress and
/// reports whether it is considered dead.
#[derive(Debug, Clone)]
pub struct Heartbeat {
    timeout: Duration,
    last_ok: Instant,
}

/// Optional A2A (Hermes Kanban) heartbeat configuration, read from env by the
/// production binary and passed explicitly (or `None`) by tests so the test
/// harness never spawns the `hermes` CLI.
#[derive(Clone, Debug)]
pub struct A2AConfig {
    pub enabled: bool,
    pub interval_ticks: u64,
    pub task_id: String,
}

impl Heartbeat {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_ok: Instant::now(),
        }
    }

    /// Record that the connection just made a successful tick.
    pub fn ok(&mut self) {
        self.last_ok = Instant::now();
    }

    /// True if no successful tick has been recorded within `timeout`.
    pub fn is_dead(&self) -> bool {
        self.last_ok.elapsed() > self.timeout
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// JSON command from Hermes (stdin).
#[derive(Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum AgentCommand {
    #[serde(rename = "move")]
    Move { x: f32, y: f32 },

    #[serde(rename = "chat")]
    Chat { msg: String },

    #[serde(rename = "command")]
    ServerCommand { name: String, args: Vec<String> },

    #[serde(rename = "status")]
    Status,

    #[serde(rename = "patrol")]
    Patrol { waypoints: Vec<[f32; 2]> },

    #[serde(rename = "quit")]
    Quit,
}

/// A live game connection: the abstraction the tick loop works against so the
/// same logic can run against the real Veloren client or a test mock.
pub trait GameConnection: Send + Sync + 'static {
    /// Advance the connection one tick. Returns world events, or an error if the
    /// connection has been lost.
    fn tick(
        &mut self,
        move_dir: Vec2<f32>,
        dt: f64,
    ) -> Result<Vec<Event>, String>;

    /// Whether the agent is registered/authenticated with the server.
    fn registered(&self) -> bool;

    /// Current presence (e.g. `Some("Character")` when in-game).
    fn presence(&self) -> Option<String>;

    /// Current server tick.
    fn get_tick(&self) -> u64;

    /// Last known world position `[x, y, z]`.
    fn position(&self) -> Option<[f32; 3]>;

    /// Current health, if in-game.
    fn health(&self) -> Option<HealthObs>;

    /// Send a chat message to the server.
    fn send_chat(&mut self, msg: String);

    /// Send a server command (`name`, `args`).
    fn send_command(&mut self, name: String, args: Vec<String>);

    /// Per-tick cleanup.
    fn cleanup(&mut self);

    /// Log out / disconnect.
    fn logout(&mut self);
}

/// Factory for creating a `GameConnection`. Abstracts `Client::new` so the
/// retry/wait-for-server logic can be tested with a mock connector.
pub trait Connector: Send + Sync + 'static {
    type Connection: GameConnection;

    /// Establish a new connection. Blocks until connected or hard-failed.
    fn connect(&self) -> Result<Self::Connection, String>;
}

/// Connect with retries: waits for the server to come up before returning.
pub fn connect_with_retry<C: Connector>(
    connector: &C,
    max_retries: u32,
    retry_delay: Duration,
) -> Result<C::Connection, String> {
    let mut last_err: Option<String> = None;
    for attempt in 1..=max_retries {
        match connector.connect() {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                last_err = Some(e.clone());
                eprintln!(
                    "connect attempt {attempt}/{max_retries} failed: {e}, retrying in {retry_delay:?}"
                );
                std::thread::sleep(retry_delay);
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| "connection failed: no attempts made".to_string()))
}

/// Production connector: wraps `veloren_client::Client::new`.
pub struct ClientConnector {
    addr: ConnectionArgs,
    provider: SimpleCredentialProvider,
    runtime: Arc<Runtime>,
    config_dir: PathBuf,
}

impl ClientConnector {
    pub fn new(
        addr: ConnectionArgs,
        provider: SimpleCredentialProvider,
        runtime: Arc<Runtime>,
        config_dir: PathBuf,
    ) -> Self {
        Self {
            addr,
            provider,
            runtime,
            config_dir,
        }
    }
}

impl Connector for ClientConnector {
    type Connection = ClientGameConnection;

    fn connect(&self) -> Result<ClientGameConnection, String> {
        let client = self
            .runtime
            .block_on(Client::new(
                self.addr.clone(),
                Arc::clone(&self.runtime),
                None, // locale
                &self.provider,
                &|_stage| {}, // version mismatch callback (unused)
                |_| {}, // add_foreign_systems
                self.config_dir.clone(),
                ClientType::Game,
            ))
            .map_err(|e| format!("{e:?}"))?;
        Ok(ClientGameConnection::new(client))
    }
}

/// Credential provider for authenticated Veloren servers.
/// Reads username/password from VELOREN_USERNAME / VELOREN_PASSWORD env vars.
/// Only trusts the official auth server (https://auth.veloren.net).
#[derive(Clone, Debug)]
pub struct SimpleCredentialProvider {
    pub username: String,
    pub password: String,
}

impl CredentialProvider for SimpleCredentialProvider {
    fn auth_trusted(&self, auth_provider: &Option<String>) -> bool {
        auth_provider == &Some("https://auth.veloren.net".to_string())
    }

    fn get_credentials(
        &self,
        _auth_provider: &Option<String>,
    ) -> impl std::future::Future<Output = Result<Credentials, ()>> + Send {
        let username = self.username.clone();
        let password = self.password.clone();
        async move { Ok(Credentials::UsernamePassword(username, password)) }
    }
}

/// Production `GameConnection` backed by a real `veloren_client::Client`.
pub struct ClientGameConnection {
    client: Client,
}

impl ClientGameConnection {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn into_inner(self) -> Client {
        self.client
    }
}

impl GameConnection for ClientGameConnection {
    fn tick(
        &mut self,
        move_dir: Vec2<f32>,
        dt: f64,
    ) -> Result<Vec<Event>, String> {
        let mut inputs = ControllerInputs::default();
        inputs.move_dir = move_dir;
        self.client
            .tick(inputs, Duration::from_secs_f64(dt))
            .map_err(|e| format!("{e:?}"))
    }

    fn registered(&self) -> bool {
        self.client.registered()
    }

    fn presence(&self) -> Option<String> {
        self.client.presence().map(|p| format!("{p:?}"))
    }

    fn get_tick(&self) -> u64 {
        self.client.get_tick()
    }

    fn position(&self) -> Option<[f32; 3]> {
        self.client
            .state()
            .ecs()
            .read_storage::<comp::Pos>()
            .get(self.client.entity())
            .map(|p| [p.0.x, p.0.y, p.0.z])
    }

    fn health(&self) -> Option<HealthObs> {
        self.client.current::<comp::Health>().map(|h| HealthObs {
            current: h.current(),
            maximum: h.maximum(),
            is_dead: h.is_dead,
        })
    }

    fn send_chat(&mut self, msg: String) {
        self.client.send_chat(msg);
    }

    fn send_command(&mut self, name: String, args: Vec<String>) {
        self.client.send_command(name, args);
    }

    fn cleanup(&mut self) {
        self.client.cleanup();
    }

    fn logout(&mut self) {
        self.client.logout();
    }
}

/// Ensure the named character exists on the server and enter the game.
/// (Production only: operates on the real `Client`.)
pub fn ensure_character(client: &mut Client, character_name: &str) {
    client.load_character_list();

    let clock = Clock::new(Duration::from_secs_f64(1.0 / TICKS_PER_SEC));
    let mut waited = 0.0;
    while client.character_list().characters.is_empty() && client.character_list().loading {
        let _ = client.tick(ControllerInputs::default(), clock.real_dt());
        client.cleanup();
        waited += clock.real_dt().as_secs_f64();
        if waited > 10.0 {
            eprintln!("Timed out waiting for character list");
            break;
        }
    }

    let character_id = client
        .character_list()
        .characters
        .iter()
        .find(|c| c.character.alias == character_name)
        .map(|c| c.character.id.unwrap());

    match character_id {
        Some(id) => {
            eprintln!("Found character '{character_name}', entering game...");
            client.request_character(id, ViewDistances { terrain: 12, entity: 12 });
        }
        None => {
            eprintln!("No character named '{character_name}' found. Creating...");
            let body = comp::Body::Humanoid(comp::body::humanoid::Body {
                species: comp::body::humanoid::Species::Human,
                body_type: comp::body::humanoid::BodyType::Male,
                hair_style: 0,
                beard: 0,
                eyes: 0,
                accessory: 0,
                hair_color: 0,
                skin: 0,
                eye_color: 0,
                height_scale: 128,
            });
            client.create_character(
                character_name.to_string(),
                None,
                None,
                body,
                false, // hardcore
                None,  // start site
            );
            client.load_character_list();

            let mut waited = 0.0;
            while client.character_list().loading {
                let _ = client.tick(ControllerInputs::default(), clock.real_dt());
                client.cleanup();
                waited += clock.real_dt().as_secs_f64();
                if waited > 10.0 {
                    eprintln!("Timed out waiting for character creation");
                    return;
                }
            }

            let character_id = client
                .character_list()
                .characters
                .iter()
                .find(|c| c.character.alias == character_name)
                .map(|c| c.character.id.unwrap());

            if let Some(id) = character_id {
                eprintln!("Character created, entering game...");
                client.request_character(id, ViewDistances { terrain: 12, entity: 12 });
            } else {
                eprintln!("Failed to create character '{character_name}'");
            }
        }
    }

    let mut waited = 0.0;
    while client.presence().is_none() && waited < 15.0 {
        let _ = client.tick(ControllerInputs::default(), clock.real_dt());
        client.cleanup();
        waited += clock.real_dt().as_secs_f64();
    }

    match client.presence() {
        Some(_) => eprintln!("In-game. Agent ready."),
        None => eprintln!("Warning: not yet in-game after startup."),
    }
}

/// The agent tick loop. Parameterized over a `GameConnection` and an output
/// sink so production (`src/main.rs`) and integration tests share one code path.
///
/// Returns `AgentOutcome::Quit` on a `{"cmd":"quit"}` command, or
/// `AgentOutcome::ConnectionLost` when the connection dies (tick error or
/// keepalive watchdog timeout), at which point the caller should reconnect.
pub fn run_agent<C: GameConnection>(
    conn: &mut C,
    out: &mut impl Write,
    cmd_rx: &std::sync::mpsc::Receiver<String>,
    heartbeat_timeout: Duration,
    a2a: Option<A2AConfig>,
) -> AgentOutcome {
    let mut clock = Clock::new(Duration::from_secs_f64(1.0 / TICKS_PER_SEC));
    let mut heartbeat = Heartbeat::new(heartbeat_timeout);

    // Pending movement direction (set by commands or patrol AI, applied each tick)
    let mut pending_chat: Option<String> = None;
    let mut last_sent_chat: Option<String> = None;
    let mut pending_command: Option<(String, Vec<String>)> = None;

    let mut chat_buffer: Vec<String> = Vec::new();

    // Last known world position [x, y, z], refreshed after each observation.
    let mut last_position: Option<[f32; 3]> = None;

    // Patrol state.
    let mut patrol_waypoints: Vec<[f32; 2]> = Vec::new();
    let mut patrol_index: usize = 0;
    let mut patrol_active: bool = false;

    // Cooldown (ticks) between automatic chat replies.
    let mut auto_response_cooldown: u64 = 0;

    // A2A kanban heartbeat state.
    let mut ticks_since_heartbeat: u64 = 0;

    loop {
        clock.tick();
        auto_response_cooldown = auto_response_cooldown.saturating_sub(1);

        // Keepalive watchdog: if no successful tick within the timeout, the
        // connection is dead — signal the caller to reconnect.
        if heartbeat.is_dead() {
            eprintln!("keepalive watchdog timed out; connection considered dead");
            return AgentOutcome::ConnectionLost;
        }

        // Movement resets every tick. Patrol sets a base steering direction
        // below; an explicit `{"cmd":"move"}` command overrides it when commands drain.
        let mut move_dir = Vec2::zero();

        // --- Patrol steering: walk toward the current waypoint ----------
        if patrol_active && patrol_index < patrol_waypoints.len() {
            if let Some(pos) = last_position {
                let target = Vec2::new(patrol_waypoints[patrol_index][0], patrol_waypoints[patrol_index][1]);
                let cur = Vec2::new(pos[0], pos[1]);
                let to_target = target - cur;
                let dist = to_target.magnitude();
                if dist < PATROL_REACH_THRESHOLD {
                    patrol_index = (patrol_index + 1) % patrol_waypoints.len();
                } else if dist > 0.001 {
                    let mut dir = to_target;
                    dir.normalize();
                    move_dir = dir;
                }
            }
        }

        // --- Drain pending stdin commands (non-blocking) ------------------
        for line in cmd_rx.try_iter() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<AgentCommand>(trimmed) {
                Ok(cmd) => match cmd {
                    AgentCommand::Move { x, y } => {
                        move_dir = Vec2::new(x, y);
                    }
                    AgentCommand::Chat { msg } => {
                        pending_chat = Some(msg);
                    }
                    AgentCommand::ServerCommand { name, args } => {
                        pending_command = Some((name, args));
                    }
                    AgentCommand::Status => {
                        // Status is reported every tick via observation.
                    }
                    AgentCommand::Quit => {
                        conn.logout();
                        return AgentOutcome::Quit;
                    }
                    AgentCommand::Patrol { waypoints } => {
                        if waypoints.is_empty() {
                            patrol_active = false;
                            patrol_index = 0;
                        } else {
                            patrol_waypoints = waypoints;
                            patrol_index = 0;
                            patrol_active = true;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to parse command: {e}");
                }
            }
        }

        // --- Apply inputs -------------------------------------------------
        if let Some(msg) = pending_chat.take() {
            last_sent_chat = Some(msg.clone());
            conn.send_chat(msg);
        }
        if let Some((name, args)) = pending_command.take() {
            conn.send_command(name, args);
        }

        // --- Tick the connection ------------------------------------------
        let events = match conn.tick(move_dir, clock.real_dt().as_secs_f64()) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("Connection tick error: {e}");
                return AgentOutcome::ConnectionLost;
            }
        };
        heartbeat.ok();

        // --- Build observation ------------------------------------------
        let mut obs_events: Vec<String> = Vec::new();
        for event in &events {
            match event {
                veloren_client::Event::Chat(msg) => {
                    let content = msg
                        .content()
                        .as_plain()
                        .unwrap_or_default()
                        .to_string();
                    chat_buffer.push(content.clone());
                    if chat_buffer.len() > 50 {
                        chat_buffer.remove(0);
                    }
                    // --- Event-driven auto-response ---------------------
                    let lowered = content.to_lowercase();
                    let is_ours = last_sent_chat.as_ref() == Some(&content);
                    let triggered = !is_ours
                        && !content.starts_with("Auto-response:")
                        && auto_response_cooldown == 0
                        && (lowered.contains("hermes")
                            || lowered.contains("ping")
                            || lowered.contains("hello")
                            || lowered.contains("hi"));
                    if triggered {
                        let reply = format!("Auto-response: I heard \"{content}\"");
                        last_sent_chat = Some(reply.clone());
                        pending_chat = Some(reply);
                        auto_response_cooldown = AUTO_RESPONSE_COOLDOWN_TICKS;
                    }
                }
                other => {
                    obs_events.push(format!("{other:?}"));
                }
            }
        }

        let position = conn.position();
        last_position = position;
        let health = conn.health();

        let obs = Observation {
            schema_version: SCHEMA_VERSION,
            tick: conn.get_tick(),
            registered: conn.registered(),
            presence: conn.presence(),
            position,
            health,
            chat: chat_buffer.clone(),
            events: obs_events,
        };

        let line = serde_json::to_string(&obs).unwrap();
        writeln!(out, "{line}").unwrap();
        out.flush().unwrap();

        // --- A2A bridge: periodic tick/status kanban heartbeat ----------
        if let Some(ref cfg) = a2a {
            if cfg.enabled && ticks_since_heartbeat >= cfg.interval_ticks {
                ticks_since_heartbeat = 0;
                let task_id = cfg.task_id.clone();
                let report = format!(
                    "tick={} registered={} presence={:?} position={:?} health={:?}",
                    obs.tick, obs.registered, obs.presence, obs.position, obs.health
                );
                std::thread::spawn(move || {
                    match std::process::Command::new("hermes")
                        .args(["kanban", "comment", &task_id, &report])
                        .output()
                    {
                        Ok(out) if out.status.success() => {}
                        Ok(out) => eprintln!(
                            "kanban heartbeat comment failed (exit {:?}): {}",
                            out.status.code(),
                            String::from_utf8_lossy(&out.stderr)
                        ),
                        Err(e) => eprintln!("kanban heartbeat comment spawn failed: {e}"),
                    }
                });
            } else {
                ticks_since_heartbeat = ticks_since_heartbeat.saturating_add(1);
            }
        }

        conn.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy in-memory connection implementing `GameConnection` for the
    /// `connect_with_retry` unit tests (no real client / network needed).
    struct DummyConnection;
    impl GameConnection for DummyConnection {
        fn tick(&mut self, _move_dir: Vec2<f32>, _dt: f64) -> Result<Vec<Event>, String> {
            Ok(vec![])
        }
        fn registered(&self) -> bool { true }
        fn presence(&self) -> Option<String> { Some("Character".into()) }
        fn get_tick(&self) -> u64 { 0 }
        fn position(&self) -> Option<[f32; 3]> { None }
        fn health(&self) -> Option<HealthObs> { None }
        fn send_chat(&mut self, _msg: String) {}
        fn send_command(&mut self, _name: String, _args: Vec<String>) {}
        fn cleanup(&mut self) {}
        fn logout(&mut self) {}
    }

    #[test]
    fn heartbeat_not_dead_immediately() {
        let h = Heartbeat::new(Duration::from_millis(100));
        assert!(!h.is_dead());
    }

    #[test]
    fn heartbeat_dead_after_timeout() {
        let mut h = Heartbeat::new(Duration::from_millis(100));
        assert!(!h.is_dead());
        std::thread::sleep(Duration::from_millis(150));
        assert!(h.is_dead());
        h.ok();
        assert!(!h.is_dead());
    }

    #[test]
    fn connect_with_retry_succeeds_on_first_try() {
        struct OkConnector;
        impl Connector for OkConnector {
            type Connection = DummyConnection;
            fn connect(&self) -> Result<DummyConnection, String> { Ok(DummyConnection) }
        }
        let c = OkConnector;
        let res = connect_with_retry(&c, 3, Duration::from_millis(1));
        assert!(res.is_ok());
    }

    #[test]
    fn connect_with_retry_retries_then_succeeds() {
        struct FlakyConnector {
            calls: std::sync::atomic::AtomicUsize,
        }
        impl Connector for FlakyConnector {
            type Connection = DummyConnection;
            fn connect(&self) -> Result<DummyConnection, String> {
                let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 { Err("server not up".to_string()) } else { Ok(DummyConnection) }
            }
        }
        let c = FlakyConnector { calls: std::sync::atomic::AtomicUsize::new(0) };
        let res = connect_with_retry(&c, 5, Duration::from_millis(1));
        assert!(res.is_ok());
        assert_eq!(c.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn connect_with_retry_fails_after_max_retries() {
        struct AlwaysFailConnector;
        impl Connector for AlwaysFailConnector {
            type Connection = DummyConnection;
            fn connect(&self) -> Result<DummyConnection, String> { Err("down".to_string()) }
        }
        let c = AlwaysFailConnector;
        let res = connect_with_retry(&c, 2, Duration::from_millis(1));
        assert!(res.is_err());
    }
}
