//! Headless Veloren agent frontend.
//!
//! Uses `veloren-client` (the same modular client library teloren uses) to
//! connect to a local `veloren-server-cli` instance and exposes a stdio/JSON
//! command interface for Hermes (or any agent harness).
//!
//! Commands (one JSON object per line on stdin):
//!   {"cmd":"move","x":1.0,"y":0.0}     set movement direction
//!   {"cmd":"chat","msg":"hello"}        send world chat
//!   {"cmd":"command","name":"say","args":["hi"]}  send a server command
//!   {"cmd":"status"}                    request a status observation
//!   {"cmd":"patrol","waypoints":[[x,y],[x,y]]}  patrol waypoints (loops back to start)
//!   {"cmd":"quit"}                      disconnect and exit
//!
//! Observations (one JSON object per line on stdout):
//!   {"schema_version":1,"tick":0,"registered":true,"presence":"Character","position":[x,y,z],
//!    "health":{"current":100.0,"maximum":100.0,"is_dead":false},
//!    "chat":["..."],"events":["..."]}
//!
//!   schema_version — observation schema version; see SCHEMA_VERSION.

#![deny(unsafe_code)]

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use specs::WorldExt;
use tokio::runtime::Runtime;
use veloren_client::{
    Client, ClientType, CredentialProvider, Credentials,
    addr::ConnectionArgs,
};
use veloren_common::{
    comp::{self, ControllerInputs},
    clock::Clock,
    ViewDistances,
};
use vek::Vec2;

const DEFAULT_SERVER: &str = "127.0.0.1:14004";
const DEFAULT_CHARACTER: &str = "hermes";
const TICKS_PER_SEC: f64 = 60.0;

/// Observation schema version, emitted as `schema_version` in every JSON observation.
/// Bump this on any breaking change to the observation shape.
const SCHEMA_VERSION: u32 = 1;

/// Kanban task the A2A bridge posts tick/status comments to.
/// Override with the HERMES_KANBAN_TASK_ID environment variable.
const DEFAULT_KANBAN_TASK_ID: &str = "1cd858da-57c2-40af-9351-ede69e912324";

/// Ticks between kanban heartbeat comments (60 ticks/sec; default 300 = every 5s).
/// Override with HERMES_KANBAN_INTERVAL_TICKS.
const DEFAULT_A2A_INTERVAL_TICKS: u64 = 300;

/// World-unit distance at which a patrol waypoint counts as reached.
const PATROL_REACH_THRESHOLD: f32 = 0.5;

/// Ticks between automatic chat replies (prevents reply loops/spam).
const AUTO_RESPONSE_COOLDOWN_TICKS: u64 = 300;

/// CLI arguments.
#[derive(Parser, Debug)]
#[command(author, version, about = "Headless Veloren agent frontend")]
struct Args {
    /// Server address (host:port), default 127.0.0.1:14004
    #[arg(long, default_value = DEFAULT_SERVER)]
    server: String,

    /// Character name to play
    #[arg(long, default_value = DEFAULT_CHARACTER)]
    character: String,

    /// Run without auth (for --no-auth local servers)
    #[arg(long)]
    no_auth: bool,
}

/// JSON command from Hermes (stdin).
#[derive(Deserialize, Debug)]
#[serde(tag = "cmd")]
enum AgentCommand {
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

/// Health observation.
#[derive(Serialize, Debug)]
struct HealthObs {
    current: f32,
    maximum: f32,
    is_dead: bool,
}

/// JSON observation to Hermes (stdout).
#[derive(Serialize, Debug)]
struct Observation {
    schema_version: u32,
    tick: u64,
    registered: bool,
    presence: Option<String>,
    position: Option<[f32; 3]>,
    health: Option<HealthObs>,
    chat: Vec<String>,
    events: Vec<String>,
}

/// Credential provider for authenticated Veloren servers.
/// Reads username/password from VELOREN_USERNAME / VELOREN_PASSWORD env vars.
/// Only trusts the official auth server (https://auth.veloren.net).
struct SimpleCredentialProvider {
    username: String,
    password: String,
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

fn main() {
    let args = Args::parse();

    let runtime = Arc::new(Runtime::new().expect("failed to create tokio runtime"));

    // --- Connect to the Veloren server ------------------------------------
    let addr = ConnectionArgs::Tcp {
        hostname: args.server.clone(),
        prefer_ipv6: false,
    };

    let provider = SimpleCredentialProvider {
        username: env::var("VELOREN_USERNAME").unwrap_or_else(|_| args.character.clone()),
        password: env::var("VELOREN_PASSWORD")
            .expect("VELOREN_PASSWORD must be set for authenticated servers"),
    };

    let mut client = runtime
        .block_on(Client::new(
            addr,
            Arc::clone(&runtime),
            None, // locale
            &provider,
            &|_stage| {}, // version mismatch callback (unused)
            |_| {}, // add_foreign_systems
            PathBuf::from("/tmp/veloren-agent-config"),
            ClientType::Game,
        ))
        .expect("failed to create Veloren client");

    eprintln!(
        "Connected to {} (registered: {})",
        args.server,
        client.registered()
    );

    // --- Ensure a character exists and enter the game ---------------------
    ensure_character(&mut client, &args.character);

    // --- Main tick loop ---------------------------------------------------
    let mut clock = Clock::new(Duration::from_secs_f64(1.0 / TICKS_PER_SEC));
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Pending movement direction (set by commands or patrol AI, applied each tick)
    let mut pending_chat: Option<String> = None;
    let mut last_sent_chat: Option<String> = None;
    let mut pending_command: Option<(String, Vec<String>)> = None;

    let mut chat_buffer: Vec<String> = Vec::new();

    // Last known world position [x, y, z], refreshed after each observation.
    let mut last_position: Option<[f32; 3]> = None;

    // Patrol state: {"cmd":"patrol","waypoints":[[x,y],...]} steers the agent
    // toward each waypoint in order, looping back to the start.
    let mut patrol_waypoints: Vec<[f32; 2]> = Vec::new();
    let mut patrol_index: usize = 0;
    let mut patrol_active: bool = false;

    // Cooldown (ticks) between automatic chat replies; prevents reply loops.
    let mut auto_response_cooldown: u64 = 0;

    // A2A bridge: periodically posts tick/status to Hermes Kanban via
    // `hermes kanban comment <task_id> <report>`. Non-blocking (spawned thread).
    let kanban_task_id =
        env::var("HERMES_KANBAN_TASK_ID").unwrap_or_else(|_| DEFAULT_KANBAN_TASK_ID.to_string());
    let a2a_interval: u64 = env
        ::var("HERMES_KANBAN_INTERVAL_TICKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_A2A_INTERVAL_TICKS)
        .max(1);
    let a2a_enabled = env::var("HERMES_KANBAN_ENABLE")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "no" | "off"))
        .unwrap_or(true);
    let mut ticks_since_heartbeat: u64 = 0;

    // Non-blocking command channel: a reader thread blocks on stdin so the
    // main tick loop can drain available commands without ever stalling.
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();
    {
        let tx = cmd_tx.clone();
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines().flatten() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    'running: loop {
        clock.tick();
        auto_response_cooldown = auto_response_cooldown.saturating_sub(1);
        ticks_since_heartbeat = ticks_since_heartbeat.saturating_add(1);

        // Movement resets every tick. Patrol sets a base steering direction
        // below; an explicit {"cmd":"move"} command overrides it when commands drain.
        let mut move_dir = Vec2::zero();

        // --- Patrol steering: walk toward the current waypoint ----------
        if patrol_active && patrol_index < patrol_waypoints.len() {
            if let Some(pos) = last_position {
                let target = Vec2::new(patrol_waypoints[patrol_index][0], patrol_waypoints[patrol_index][1]);
                let cur = Vec2::new(pos[0], pos[1]);
                let to_target = target - cur;
                let dist = (to_target.x * to_target.x + to_target.y * to_target.y).sqrt();
                if dist < PATROL_REACH_THRESHOLD {
                    patrol_index = (patrol_index + 1) % patrol_waypoints.len();
                    eprintln!(
                        "Patrol reached waypoint {}, heading to {:?}",
                        patrol_index, patrol_waypoints[patrol_index]
                    );
                } else if dist > 0.001 {
                    let inv_len = 1.0 / dist;
                    move_dir = Vec2::new(to_target.x * inv_len, to_target.y * inv_len);
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
                                client.logout();
                                break 'running;
                            }
                            AgentCommand::Patrol { waypoints } => {
                                if waypoints.is_empty() {
                                    patrol_active = false;
                                    patrol_index = 0;
                                    eprintln!("Patrol stopped");
                                } else {
                                    patrol_waypoints = waypoints;
                                    patrol_index = 0;
                                    patrol_active = true;
                                    eprintln!(
                                        "Patrol started with {} waypoints",
                                        patrol_waypoints.len()
                                    );
                                }
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to parse command: {e}");
                        }
                    }
                }

        // --- Apply inputs -------------------------------------------------
        let mut inputs = ControllerInputs::default();
        inputs.move_dir = move_dir;

        if let Some(msg) = pending_chat.take() {
            last_sent_chat = Some(msg.clone());
            client.send_chat(msg);
        }
        if let Some((name, args)) = pending_command.take() {
            client.send_command(name, args);
        }

        // --- Tick the client ---------------------------------------------
        let events = match client.tick(inputs, clock.real_dt()) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("Client tick error: {e:?}");
                break;
            }
        };

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
                    // --- Event-driven auto-response --------------------
                    // Auto-reply once per cooldown when a player addresses the
                    // agent (by name or keyword), never echoing our own text.
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

        // Position
        let position = client
            .state()
            .ecs()
            .read_storage::<comp::Pos>()
            .get(client.entity())
            .map(|p| [p.0.x, p.0.y, p.0.z]);
        last_position = position;

        // Health
        let health = client.current::<comp::Health>().map(|h| HealthObs {
            current: h.current(),
            maximum: h.maximum(),
            is_dead: h.is_dead,
        });

        let obs = Observation {
            schema_version: SCHEMA_VERSION,
            tick: client.get_tick(),
            registered: client.registered(),
            presence: format!("{:?}", client.presence()).into(),
            position,
            health,
            chat: chat_buffer.clone(),
            events: obs_events,
        };

        let line = serde_json::to_string(&obs).unwrap();
        writeln!(out, "{line}").unwrap();
        out.flush().unwrap();

        // --- A2A bridge: periodic tick/status kanban heartbeat ----------
        if a2a_enabled && ticks_since_heartbeat >= a2a_interval {
            ticks_since_heartbeat = 0;
            let task_id = kanban_task_id.clone();
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
        }

        client.cleanup();
    }

    eprintln!("Agent shut down.");
}

/// Ensure the named character exists on the server and enter the game.
/// If no characters exist yet, creates a default humanoid character.
fn ensure_character(client: &mut Client, character_name: &str) {
    client.load_character_list();

    // Tick until the character list arrives or we time out.
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

    // Look for our character.
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

            // Wait for character list to update
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

    // Tick until we're in-game (presence becomes Some(Character(_)))
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
