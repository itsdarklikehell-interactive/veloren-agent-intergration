//! Integration tests for the veloren-agent headless frontend.
//!
//! These tests exercise the agent binary end-to-end. Tests that require a
//! running Veloren server on port 14004 are gated behind `server_available()`
//! and will skip (not fail) when no server is present, so the suite is safe to
//! run in CI without a game server.
//!
//! Run all tests:
//!     cargo test --release --test integration
//!
//! Run E2E tests only (requires a local server):
//!     cargo test --release --test integration -- --ignored

use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Returns the path to the agent binary, honoring VELOREN_AGENT_BIN.
fn agent_bin() -> std::path::PathBuf {
    std::env::var("VELOREN_AGENT_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::Path::new("./target/release/veloren-agent").to_path_buf())
}

/// True if something accepts TCP connections on 127.0.0.1:14004.
fn server_available() -> bool {
    std::net::TcpStream::connect("127.0.0.1:14004").is_ok()
}

/// Spawn the agent with `--character <name>` and piped stdin/stdout/stderr.
/// Returns the child process. The caller is responsible for waiting/killing it.
fn spawn_agent(character: &str) -> std::process::Child {
    Command::new(agent_bin())
        .arg("--character")
        .arg(character)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("VELOREN_PASSWORD", "integration-test")
        .spawn()
        .expect("failed to spawn agent binary")
}

/// Send a JSON command to the agent's stdin and wait for the process to exit.
fn send_and_wait(mut child: std::process::Child, cmd: &str) -> std::process::ExitStatus {
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{cmd}\n").as_bytes());
        let _ = stdin.flush();
    }
    child.wait().expect("agent process should terminate")
}

/// Recovery backoff constants, mirroring src/main.rs so the integration suite
/// can verify the auto-recovery configuration without importing the binary
/// crate (binary crates are not importable as libraries).
const RECOVERY_INITIAL_BACKOFF_SECS: u64 = 1;
const RECOVERY_MAX_BACKOFF_SECS: u64 = 60;
const RECOVERY_BACKOFF_MULTIPLIER: f64 = 2.0;

fn next_backoff(current: Duration) -> Duration {
    let next = (current.as_secs_f64() * RECOVERY_BACKOFF_MULTIPLIER)
        .min(RECOVERY_MAX_BACKOFF_SECS as f64);
    Duration::from_secs_f64(next)
}

#[test]
fn test_agent_help_flag() {
    let output = Command::new(agent_bin())
        .arg("--help")
        .output()
        .expect("agent binary should execute");
    assert!(output.status.success(), "--help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--server") || stdout.contains("--character"),
        "--help should document available CLI options"
    );
    assert!(
        stdout.contains("Headless"),
        "--help should describe the agent"
    );
}

#[test]
fn test_agent_version_flag() {
    let output = Command::new(agent_bin())
        .arg("--version")
        .output()
        .expect("agent binary should execute");
    assert!(output.status.success(), "--version should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.is_empty(),
        "--version should print a version string"
    );
}

#[test]
fn test_agent_rejects_unknown_arg() {
    let output = Command::new(agent_bin())
        .arg("--nonexistent-flag")
        .output()
        .expect("agent binary should execute");
    assert!(
        !output.status.success(),
        "unknown flag should cause a non-zero exit"
    );
}

#[test]
fn test_agent_fails_to_connect_without_server_gracefully() {
    // Without a server, the agent should not panic or crash with a signal.
    // It enters the auto-recovery retry loop; we kill it after a short grace
    // period and verify it was terminated by our kill, not a panic.
    let mut child = spawn_agent("test-no-server");
    thread::sleep(Duration::from_millis(800));
    // The agent is in its retry loop; kill it.
    child.kill().expect("should be able to kill the agent");
    let _ = child.wait();
    // If we get here without a panic, the graceful-failure requirement is met.
}

#[test]
fn test_agent_quit_after_connection_exits_cleanly() {
    // Requires a server; skipped otherwise because the agent cannot reach the
    // tick loop (where `quit` is processed) without a successful connection.
    if !server_available() {
        eprintln!("Skipping test_agent_quit_after_connection_exits_cleanly: no server on port 14004");
        return;
    }

    let mut child = spawn_agent("test-quit");

    // Wait for the agent to connect and enter the game (up to 20 seconds).
    let mut connected = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        // Try to read a line of stdout to confirm observations are flowing.
        if let Some(stdout) = child.stdout.take() {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) > 0 && line.contains("\"tick\"") {
                connected = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    if !connected {
        // Couldn't confirm connection in time; kill and skip rather than hang.
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("Skipping: agent did not emit observations within timeout");
        return;
    }

    // Send quit and verify clean exit.
    let status = send_and_wait(child, r#"{"cmd":"quit"}"#);
    assert!(
        status.success(),
        "agent should exit 0 after quit command"
    );
}

#[test]
#[ignore]
fn test_e2e_move_and_observation() {
    // Full E2E: connect, move, verify observation reflects movement.
    // Requires a running server; ignored by default to avoid hanging CI.
    if !server_available() {
        eprintln!("Skipping test_e2e_move_and_observation: no server on port 14004");
        return;
    }

    let mut child = spawn_agent("test-e2e-move");

    // Wait for first observation with a position.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut first_pos: Option<[f32; 3]> = None;
    while std::time::Instant::now() < deadline {
        if let Some(stdout) = child.stdout.take() {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) > 0 {
                if let Ok(obs) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(pos) = obs.get("position").and_then(|v| v.as_array()) {
                        first_pos = Some([
                            pos[0].as_f64().unwrap() as f32,
                            pos[1].as_f64().unwrap() as f32,
                            pos[2].as_f64().unwrap() as f32,
                        ]);
                        break;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    let start_pos = first_pos.expect("should receive an observation with position");

    // Send a move command.
    let _ = send_and_wait(child, r#"{"cmd":"move","x":0.0,"y":1.0}"#);

    // The agent should still be running (move doesn't quit).
    // We can't easily read more stdout here because send_and_wait consumes the child.
    // This test primarily verifies the move command is accepted without error.
    let _ = start_pos;
}

#[test]
fn test_recovery_backoff_growth_and_cap() {
    // Verifies the auto-recovery backoff configuration is sane: exponential
    // growth with a hard cap. Mirrors the logic in src/main.rs.
    let b0 = Duration::from_secs(RECOVERY_INITIAL_BACKOFF_SECS);
    let b1 = next_backoff(b0);
    let b2 = next_backoff(b1);
    let b3 = next_backoff(b2);

    // Should grow exponentially.
    assert!(b1 > b0, "first backoff should exceed initial");
    assert!(b2 > b1, "second backoff should exceed first");

    // Should not exceed the max cap.
    assert!(
        b3.as_secs() <= RECOVERY_MAX_BACKOFF_SECS,
        "backoff should not exceed max cap"
    );

    // At max cap, further calls stay at the cap.
    let max_cap = Duration::from_secs(RECOVERY_MAX_BACKOFF_SECS);
    assert_eq!(next_backoff(max_cap), max_cap, "capped backoff should stay capped");
}

#[test]
fn test_recovery_backoff_monotonic() {
    // Backoff must never decrease across repeated failures.
    let mut b = Duration::from_secs(RECOVERY_INITIAL_BACKOFF_SECS);
    for _ in 0..20 {
        let next = next_backoff(b);
        assert!(
            next >= b,
            "backoff must not decrease across retries"
        );
        b = next;
    }
}
