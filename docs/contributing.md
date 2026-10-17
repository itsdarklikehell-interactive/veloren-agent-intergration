# Contributing

Welcome to Veloren Agent Integration! Here's how to contribute.

## Getting started

1. Fork the repo: https://github.com/itsdarklikehell/veloren-agent-intergration/fork
2. Clone your fork: `git clone https://github.com/<your-username>/veloren-agent-intergration.git`
3. Create a branch: `git checkout -b <feature-name>`
4. Make your changes
5. Run checks locally (see below)
6. Commit and push: `git push origin <feature-name>`
7. Open a Pull Request

## Repository tour

| Path | Purpose |
|---|---|
| `src/main.rs` | Headless Rust agent frontend (`veloren-client`-based, no rendering) |
| `web/app.py` | Flask dashboard: API-key auth, SQLite persistence, HTTP API |
| `web/templates/`, `web/static/` | Dashboard HTML + JS (dashboard, viewport, analytics, export) |
| `scripts/` | `start-server.sh`, `stop-server.sh`, `connect-teloren.sh`, `hermes-bridge.sh` |
| `docs/` | User-facing documentation |
| `patches/` | Veloren toolchain patches applied during the Docker build |
| `tests/` | Integration tests |

## Environment setup

### Rust agent

The agent uses `veloren-client` from Veloren's gitlab, which requires a
nightly Rust toolchain plus patches (`patches/veloren-weekly-toolchain.patch`)
to build on modern toolchains:

```bash
rustup toolchain install nightly
cargo +nightly build --release
```

The Docker build (`Dockerfile`) applies the patch automatically; see
`docs/troubleshooting.md` ("Rust build blocked on nightly features") for the
manual patch inventory.

### Dashboard (Python)

```bash
cd web
python3 -m pip install -r requirements.txt   # flask, python-dotenv
python3 app.py                               # http://127.0.0.1:5000
```

Copy `.env.example` to `.env` and fill in `VELOREN_USERNAME`,
`VELOREN_PASSWORD`, and `DASHBOARD_API_KEY`. `.env` is gitignored — never
commit it.

## Local checks

### Shell scripts

```bash
# Lint
shellcheck scripts/*.sh

# Format check
shfmt -d -i 2 -ci scripts/*.sh

# Auto-format
shfmt -w -i 2 scripts/*.sh
```

### Rust

```bash
# Build
cargo build

# Check for warnings
cargo build 2>&1 | grep warning

# Format
cargo fmt --check
cargo fmt
```

### Python dashboard

```bash
cd web
python3 -m py_compile app.py
```

### Docs

- Keep `docs/` and `README.md` in sync when adding or changing behavior.
- New dashboard endpoint? Document it in `docs/dashboard-api.md` and add it to
  the README endpoint table.
- New user-facing page? Add a guide to `docs/` and link it from README.

## Project conventions

- **Shell scripts:** POSIX-compatible where possible, `bash` shebang,
  `set -euo pipefail`
- **Rust:** 2021 edition, `#![deny(unsafe_code)]`, clippy clean
- **Commits:** Clear, descriptive messages. Prefix with
  `[Agent: <Profile-Name>]` (e.g. `[Agent: Hermes]`, `[Agent: R&D-Worker]`,
  `[Agent: QA-Engineer]`)
- **Docs:** Update `docs/` and `README.md` together when changing behavior

## Pull request checklist

- [ ] `cargo build --release` (or `cargo +nightly build --release`) passes with no new warnings
- [ ] `cargo fmt --check` and clippy are clean
- [ ] `shellcheck scripts/*.sh` and `shfmt -d -i 2 -ci scripts/*.sh` pass
- [ ] Dashboard imports and `GET /api/health` returns 200 (`python3 web/app.py`)
- [ ] Docs updated: `docs/`, README tables, and this file if conventions changed
- [ ] Commit message prefixed with `[Agent: <Profile-Name>]`

## Areas that need help

| Area | Description |
|---|---|
| Rust build on stable | Fix `hashbrown`/`shred`/`foldhash` nightly feature issues, or patch `veloren-client` |
| teloren DNS fix | Resolve hickory-resolver `lookup_host` failure on `127.0.0.1` |
| Python agent | A Python implementation using raw TCP (avoids Rust build issues entirely) |
| More commands | `look`, `interact`, `inventory`, `craft`, etc. |
| Tests | Unit tests for the agent, integration tests with a local server |

## Releasing

No formal release process yet. Tag releases as `v0.1.0`, `v0.1.1`, etc.

## Code of conduct

Be respectful. This is a learning/experimentation project — questions are welcome.

## License

MIT License.
