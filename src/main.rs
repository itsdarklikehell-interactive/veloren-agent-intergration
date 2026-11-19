//! Headless Veloren agent frontend.
//!
//! Uses `veloren-client` (the same modular client library teloren uses) to
//! connect to a local `veloren-server-cli` instance and exposes a stdio/JSON
//! command interface for Hermes (or any agent harness). The connection
//! lifecycle (connect/retry, auto-reconnect on loss, keepalive watchdog) and
//! the tick loop live in `src/lib.rs` so they can be exercised by integration
//! tests; this file is the production entry point.
//!
//! Commands (one JSON object per line on stdin):
//!   {"cmd":"move","x":1.0,"y":0.0}     set movement direction
//!   {"cmd":"chat","msg":"hello"}        send world chat
//!   {"cmd":"command","name":"say","args":["hi"]}  send a server command
//!   {"cmd":"status"}                    request a status observation
//!   {"cmd":"patrol","waypoints":[[x,y],[x,y]]}  patrol waypoints (loops back to start)
//!   {"cmd":"wander"}                              wander mode (random nearby targets, toggles)
//!   {"cmd":"quit"}                      disconnect and exit
//!
//! Observations (one JSON object per line on stdout):
//!   {"schema_version":1,"tick":0,"registered":true,"presence":"Character","position":[x,y,z],
//!    "health":{"current":100.0,"maximum":100.0,"is_dead":false},"chat":["..."],"events":["..."]}
//!
//! Server lifecycle auto-recovery:
//!   If the connection to the server is lost (client.tick() error or the
//!   keepalive watchdog timing out), the agent reconnects with exponential
//!   backoff, re-authenticates, re-enters the game, and resumes ticking. The
//!   outer loop only exits on a clean `quit` command.

#![deny(unsafe_code)]

use clap::Parser;
use std::{
    env,
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::runtime::Runtime;
use veloren_agent::{
    A2AConfig, AgentOutcome, ClientConnector, ClientGameConnection, Connector, GameConnection,
    DEFAULT_HEARTBEAT_TIMEOUT_SEC, SimpleCredentialProvider, ensure_character, run_agent,
};
use veloren_client::addr::ConnectionArgs;

const DEFAULT_SERVER: &str = "127.0.0.1:14004";
const DEFAULT_CHARACTER: &str = "hermes";

/// Kanban task the A2A bridge posts tick/status comments to.
const DEFAULT_KANBAN_TASK_ID: &str = "1cd858da-57c2-40af-9351-ede69e912324";

/// Ticks between kanban heartbeat comments (60 ticks/sec; default 300 = every 5s).
const DEFAULT_A2A_INTERVAL_TICKS: u64 = 300;

/// --- Server lifecycle auto-recovery configuration ---------------------
/// Initial backoff (seconds) before the first reconnection attempt.
const RECOVERY_INITIAL_BACKOFF_SECS: u64 = 1;

/// Maximum backoff (seconds) between reconnection attempts.
const RECOVERY_MAX_BACKOFF_SECS: u64 = 60;

/// Backoff multiplier applied after each failed reconnection attempt.
const RECOVERY_BACKOFF_MULTIPLIER: f64 = 2.0;

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

/// Compute the next backoff duration, applying exponential growth capped at
/// `RECOVERY_MAX_BACKOFF_SECS`.
fn next_backoff(current: Duration) -> Duration {
    let next = (current.as_secs_f64() * RECOVERY_BACKOFF_MULTIPLIER)
        .min(RECOVERY_MAX_BACKOFF_SECS as f64);
    Duration::from_secs_f64(next)
}

/// Read A2A (Hermes Kanban) heartbeat configuration from the environment,
/// mirroring the previous inline behavior.
fn build_a2a_config() -> Option<A2AConfig> {
    let enabled = env::var("HERMES_KANBAN_ENABLE")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "no" | "off"))
        .unwrap_or(true);
    let interval: u64 = env::var("HERMES_KANBAN_INTERVAL_TICKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_A2A_INTERVAL_TICKS)
        .max(1);
    let task_id = env::var("HERMES_KANBAN_TASK_ID")
        .unwrap_or_else(|_| DEFAULT_KANBAN_TASK_ID.to_string());
    Some(A2AConfig {
        enabled,
        interval_ticks: interval,
        task_id,
    })
}

fn main() {
    let args = Args::parse();

    let runtime = Arc::new(Runtime::new().expect("failed to create tokio runtime"));

    let provider = SimpleCredentialProvider {
        username: env::var("VELOREN_USERNAME").unwrap_or_else(|_| args.character.clone()),
        password: env::var("VELOREN_PASSWORD")
            .expect("VELOREN_PASSWORD must be set for authenticated servers"),
    };

    let addr = ConnectionArgs::Tcp {
        hostname: args.server.clone(),
        prefer_ipv6: false,
    };
    let connector = ClientConnector::new(
        addr,
        provider,
        Arc::clone(&runtime),
        PathBuf::from("/tmp/veloren-agent-config"),
    );

    let a2a = build_a2a_config();
    let heartbeat_timeout = Duration::from_secs(DEFAULT_HEARTBEAT_TIMEOUT_SEC);

    // --- Outer server lifecycle auto-recovery loop ----------------------
    // If the connection to the server is lost (tick error or keepalive
    // watchdog timeout), we reconnect with exponential backoff,
    // re-authenticate, re-enter the game, and resume the tick loop.
    // This loop only exits on a clean quit command.
    let mut backoff = Duration::from_secs(RECOVERY_INITIAL_BACKOFF_SECS);

    'agent: loop {
        // --- Connect to the Veloren server and enter the game -----------
        let conn = match connector.connect() {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!(
                    "Failed to connect to server: {e}. Retrying in {}s...",
                    backoff.as_secs()
                );
                std::thread::sleep(backoff);
                backoff = next_backoff(backoff);
                continue 'agent;
            }
        };
        // Reset backoff on successful connection.
        backoff = Duration::from_secs(RECOVERY_INITIAL_BACKOFF_SECS);

        let mut client = conn.into_inner();
        ensure_character(&mut client, &args.character);

        eprintln!(
            "Connected to {} (registered: {})",
            args.server,
            client.registered()
        );

        let mut conn = ClientGameConnection::new(client);

        // --- Run the agent tick loop ------------------------------------
        // Returns AgentOutcome::Quit on clean quit, or
        // AgentOutcome::ConnectionLost on connection loss (tick error or
        // keepalive watchdog timeout), at which point we reconnect.
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();
        {
            let tx = cmd_tx.clone();
            std::thread::spawn(move || {
                for line in io::stdin().lock().lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }

        match run_agent(&mut conn, &mut out, &cmd_rx, heartbeat_timeout, a2a.clone()) {
            AgentOutcome::Quit => {
                eprintln!("Agent quit cleanly.");
                break 'agent;
            }
            AgentOutcome::ConnectionLost => {
                eprintln!(
                    "Connection lost. Attempting to reconnect in {}s...",
                    backoff.as_secs()
                );
                conn.logout();
                std::thread::sleep(backoff);
                backoff = next_backoff(backoff);
                continue 'agent;
            }
        }
    }

    eprintln!("Agent shut down.");
}
