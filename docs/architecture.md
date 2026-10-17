# Architecture

## Design principle

Veloren's engine is deliberately modular — graphical frontends are entirely decoupled from the
client library (`veloren-client`). This means we can write a headless agent frontend using the
same client library that powers `teloren` (terminal) and `veloren-voxygen` (3D), without any GPU,
display, or windowing system.

## Component diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Hermes (A2A agent)                        │
│  Sends JSON commands on stdin, reads JSON-lines on stdout       │
└──────────────────────────────────────┬──────────────────────────┘
                                       │ stdio/JSON (one JSON per line)
┌──────────────────────────────────────▼──────────────────────────┐
│                    veloren-agent (headless Rust)                │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ stdin parser│  │ tick loop    │  │ observation emitter  │  │
│  │ (JSON cmds) │  │ client.tick()│  │ (JSON-lines stdout)  │  │
│  └─────────────┘  └──────────────┘  └──────────────────────┘  │
│           │                                                        │
│           │ veloren-client API                                    │
│           ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  veloren-client (from Veloren gitlab `weekly` tag)      │   │
│  │  • Client::new() — connect to server                      │   │
│  │  • client.tick(inputs, dt) — advance one game tick        │   │
│  │  • client.send_chat() / send_command()                    │   │
│  │  • client.state().ecs() — read ECS components             │   │
│  │  • client.current::<Pos>() / current::<Health>()          │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────┬──────────────────────────┘
                                       │ TCP (localhost:14004)
┌──────────────────────────────────────▼──────────────────────────┐
│                 veloren-server-cli (dedicated server)           │
│  --non-interactive --no-auth                                     │
│  Runs headless, accepts client connections, simulates the world │
└─────────────────────────────────────────────────────────────────┘
```

## Data flow

### Commands (Hermes → agent → game)

1. Hermes writes a JSON command to the agent's stdin (one object per line).
2. The agent parses it into an `AgentCommand` enum (`serde` with `tag = "cmd"`).
3. On the next tick, the agent applies the command:
   - `move` → sets `ControllerInputs.move_dir`
   - `chat` → calls `client.send_chat(msg)`
   - `command` → calls `client.send_command(name, args)`
   - `quit` → calls `client.logout()` and exits
4. `client.tick(inputs, dt)` sends the inputs to the server.

### Observations (game → agent → Hermes)

1. After each tick, the agent reads ECS components from `client.state().ecs()`:
   - `comp::Pos` → position `[x, y, z]`
   - `comp::Health` → current/maximum/is_dead
2. Chat events from `client.tick()` are buffered.
3. The agent serializes an `Observation` struct to JSON and writes it to stdout.
4. Hermes reads the JSON-lines stream.

## Key API surfaces

| Rust API | Purpose |
|---|---|
| `veloren_client::Client::new()` | Connect to server |
| `client.tick(inputs, dt)` | Advance one tick, returns `Vec<Event>` |
| `client.send_chat(msg)` | Send world chat |
| `client.send_command(name, args)` | Send server command |
| `client.current::<Pos>()` / `current::<Health>()` | Read own components |
| `client.state().ecs()` | Full ECS read access |
| `client.entity()` | Own player entity |
| `client.logout()` | Disconnect |

## Configuration

| Env var | Default | Purpose |
|---|---|---|
| `VELOREN_SERVER_BIN` | `~/.local/share/airshipper/profiles/default/veloren-server-cli` | Path to server binary |
| `VELOREN_PORT` | `14004` | Server port |
| `VELOREN_AGENT_BIN` | `./target/release/veloren-agent` | Path to agent binary |
| `VELOREN_AGENT_CHARACTER` | `hermes` | Character name |
