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

**Workaround 2 — Patch registry crates:**

Remove the `#![cfg_attr(feature = "nightly", feature(...))]` lines from:
- `~/.cargo/registry/src/.../foldhash-0.2.0/src/lib.rs`, `fast.rs`, `quality.rs`
- `~/.cargo/registry/src/.../hashbrown-0.17.1/src/lib.rs`
- `~/.cargo/registry/src/.../shred-0.15.0/src/lib.rs`

Remove the `#[cfg(feature = "nightly")] fn write_str(...)` blocks from `foldhash` source files.

**Note:** These patches are local to your machine and not committed to the repo.

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

## Still stuck?

Open an issue at https://github.com/itsdarklikehell/veloren-agent-intergration/issues with:
- The full error output
- Your Rust version (`rustc --version`)
- Whether you're using Airshipper-installed Veloren
