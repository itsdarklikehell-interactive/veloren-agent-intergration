# Troubleshooting

Known issues and their workarounds for the Veloren Agent Integration project.

## Rust build blocked on nightly features

**Symptom:** `cargo build` fails with errors like:

```
error[E0554]: `#![feature]` may not be used on the stable release channel
  --> foldhash-0.2.0/src/lib.rs: ... feature(hasher_prefixfree_extras)
error[E0658]: use of unstable library feature `allocator_api`
  --> hashbrown-0.17.1/src/table.rs: ...
```

**Cause:** `veloren-client` (from Veloren's gitlab `weekly` tag) pulls in `hashbrown`,
`shred`, and `foldhash` with `nightly` features enabled via `specs` feature unification.
These use unstable Rust features unavailable on the stable toolchain.

**Workaround 1 — Nightly Rust:**

```bash
rustup toolchain install nightly
cargo +nightly build --release
```

**Workaround 2 — Patch transitive dependencies for modern toolchains:**

`veloren-client` (gitlab `weekly`) pulls crates that use APIs removed from
modern Rust, so the build fails on both stable and nightly until patched.
Apply these after `cargo fetch`. The repo's `patches/veloren-weekly-toolchain.patch`
contains the git-checkout half and is applied automatically by CI
(`.github/workflows/ci.yml`, nightly job) and the Docker sandbox (`Dockerfile`):

Registry crates (`~/.cargo/registry/src/*/<crate>-<version>/`):
- `shred-0.15.0/src/meta.rs`: replace
  `core::ptr::invalid_mut::<R>((self as *mut Self).addr())` with
  `(self as *mut Self).wrapping_add(1).cast::<R>()` (`ptr::invalid_mut` no
  longer exists).

Git checkout (`~/.cargo/git/checkouts/veloren-*/<rev>/`):
- `common/src/event.rs`: `use specs::shred;` →
  `use specs::shred::{ResourceId, SystemData, World};` inside the
  `event_emitters!` macro.
- `common/state/src/state.rs`: `shred::{Fetch, FetchMut, SendDispatcher}` →
  `shred::{Fetch, FetchMut, AsyncDispatcher, Dispatcher}`; add
  `use std::ops::{Deref, DerefMut};`; add the `SendDispatcher(Dispatcher)`
  wrapper with `unsafe impl Send/Sync`; `DispatcherBuilder::new()` →
  `DispatcherBuilder::<'static, 'static>::new()`; drop
  `.try_into_sendable()`; field `dispatcher: SendDispatcher(dispatcher)`;
  add `unsafe impl Send/Sync for State`.
- `common/base/src/userdata_dir.rs`: `env!("CARGO_WORKSPACE_DIR")` →
  `std::env::var("CARGO_WORKSPACE_DIR")` with a `"."` fallback (compile-time
  `env!()` is unavailable for git dependencies).
- `common/src/slowjob.rs`: `HashMap<&String, u64>` → `HashMap<String, u64>`.
- `common/systems/src/{arcing,aura,beam,buff,character_behavior,controller,interpolation,melee,phys/mod,pool,projectile,shockwave,stats}.rs`:
  add `use specs::shred::ResourceId;` and `use specs::World;`.

## teloren cannot resolve `127.0.0.1`

**Symptom:** teloren panics with `HostnameLookupFailed(Custom { kind: Uncategorized, error: "failed to lookup address information: Name or service not known" })` even though raw TCP connections to `127.0.0.1:14004` succeed.

**Cause:** hickory-resolver (used by `veloren-client`) fails to resolve the loopback address in this environment.

**Workaround:** Try `no_proxy=*`:

```bash
no_proxy=* /tmp/teloren-probe/target/debug/teloren \
  --server 127.0.0.1:14004 \
  --username HERMES \
  --character HERMES \
  --password ''
```

If that fails, a custom client using raw `std::net::TcpStream::connect("127.0.0.1:14004")` works fine — the issue is isolated to hickory-resolver's `lookup_host`.

## Server not accepting connections

**Symptom:** `ss -tlnp \| grep 14004` shows nothing, or connection is refused.

**Cause:** The `veloren-server-cli` process is not running, or still generating the world.

**Fix:**

```bash
# Start the server
./scripts/start-server.sh

# Wait for world generation (check tmux logs)
tmux attach -t veloren-server
# Look for "world generation" completion messages in the logs

# Verify
ss -tlnp | grep 14004
```

## Character creation fails

**Symptom:** Agent logs "Failed to create character" or times out waiting for the character list.

**Cause:** The server may not have finished initializing, or character list loading is slow.

**Fix:** Wait longer after starting the server before running the agent. The agent waits up to 10 seconds for the character list and 15 seconds to enter the game — increase these timeouts in `src/main.rs` if needed.

## Scripts fail CI (shellcheck/shfmt)

**Symptom:** GitHub Actions CI reports shellcheck or shfmt violations.

**Fix:** Run locally before pushing:

```bash
shellcheck scripts/*.sh
shfmt -d -i 2 -ci scripts/*.sh
```

Auto-format with:

```bash
shfmt -w -i 2 scripts/*.sh
```

## No tmux session

**Symptom:** `tmux has-session -t veloren-server` fails.

**Fix:** Start the server:

```bash
./scripts/start-server.sh
```

## Port already in use

**Symptom:** Server fails to bind to port 14004.

**Fix:** Stop any existing server:

```bash
./scripts/stop-server.sh
# or
tmux kill-session -t veloren-server
```

Then restart with `./scripts/start-server.sh`.

## Agent connection issues

### Agent cannot connect to the Veloren server

**Symptom:** Agent logs "Connection refused" or "timed out connecting to
127.0.0.1:14004"; `ss -tlnp | grep 14004` shows nothing.

**Cause:** The `veloren-server-cli` process is not running, or still
generating the world.

**Fix:**

```bash
# Start the server
./scripts/start-server.sh

# Wait for world generation, then check the tmux logs
tmux attach -t veloren-server   # detach: Ctrl-b d

# Verify the port is listening
ss -tlnp | grep 14004
```

### Agent runs but never registers (presence stays None)

**Symptom:** Observations flow but `"presence": null` / `None` and
`"registered": false` indefinitely.

**Cause:** The server has not finished initializing or the character list is
slow; the agent waits up to 10 seconds for the character list and 15 seconds
to enter the game world.

**Fix:** Wait longer after starting the server before launching the agent, or
raise the timeouts in `src/main.rs`. In Docker Compose the agent starts only
after `veloren-server` passes its healthcheck, so slow world generation simply
delays the agent — check `docker compose logs veloren-agent`.

### Agent process exits immediately

**Symptom:** `POST /api/agent/start` returns `ok`, but `agent_running` is
soon `false`; `web/agent.log` shows a crash or panic.

**Cause:** Missing assets (`VELOREN_ASSETS` path wrong or not mounted), bad
credentials, or the server not yet ready.

**Fix:**

- Confirm `VELOREN_ASSETS` points to an existing Airshipper assets directory.
- Confirm `VELOREN_USERNAME` / `VELOREN_PASSWORD` are correct.
- Check `docker compose logs veloren-agent` (Compose) or `web/agent.log`
  (local) for the panic / backtrace.

### Dashboard cannot start the agent

**Symptom:** `POST /api/agent/start` returns
`{"ok":false,"error":"agent binary not found: ..."}`.

**Cause:** `VELOREN_AGENT_BIN` points to a binary that was never built.

**Fix:** Build the agent first: `cargo build --release` (or `cargo
+nightly build --release` on the stable toolchain). In Compose, the image is
built from `Dockerfile`, which compiles `src/main.rs`.

### In Docker Compose: agent fails to reach `veloren-server:14004`

**Symptom:** `veloren-agent` logs connection errors to
`veloren-server:14004` even though the server container is up.

**Cause:** Inside the agent container, `127.0.0.1` / `localhost` refers to
the container itself, not the host. The agent must address the server by its
Compose service name, `veloren-server`.

**Fix:** The Compose file already starts the agent only after the server's
healthcheck passes, and both services share `veloren-net`. If you customized
`docker-compose.yml`, confirm the agent connects to `veloren-server:14004`
and both services are on `veloren-net`.

### `GET /api/viewport/latest` returns `{"ok": false, "error": "no observations yet"}`

**Symptom:** The viewport shows "Waiting for position data…" forever.

**Cause:** No observation has been persisted yet — the agent is not running,
not connected, or still entering the world.

**Fix:**

1. Confirm the agent is running:

   ```bash
   curl -s -H "X-API-Key: $DASHBOARD_API_KEY" http://localhost:5000/api/status
   ```

   → `agent_running` should be `true`.

2. Confirm the server port accepts connections: `server_connected` should be
   `1`, or `ss -tlnp | grep 14004` should show a listener.

3. Wait for the agent to enter the world — observations appear once
   `registered` is `true`.

## Still stuck?

Open an issue at https://github.com/itsdarklikehell/veloren-agent-intergration/issues with:
- The full error output (`docker compose logs <service>` or `web/agent.log`)
- Your Docker / Compose versions (`docker --version && docker compose version`)
- Whether you are using the local scripts or Docker Compose
