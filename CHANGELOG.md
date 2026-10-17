# Changelog

All notable changes to the veloren-agent project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] — 2026-10-18

### Added

- Autonomous goal-oriented behavior system: `explore`/`gather`/`avoid-danger` agent behaviors with patrol integration — [bbd7c80][], [e24aa79][], [5eb9694][]
- Viewport API documentation: camera controls and viewport endpoints — [103a35e][]
- Deploy runbook: Docker Compose deployment and operations — [87ab548][], [61a5197][]

### Changed

- `docs/docker-compose.md`: consolidated deploy runbook and service docs
- `README.md`: behavior system, viewport, and deploy documentation

### Fixed

- Security audit: dashboard auth controls re-verified at HEAD; 401 responses now prompt for key — [0b3f343][], [0f00f74][]
- `src/behavior.rs`: `obstacle_avoidance` normalization and `dead_code` allowances — [5eb9694][]
- `agent-behavior` compile fixes: `specs::Join` import, entity iteration, Vec2 normalization, f32 division — [e24aa79][]

## [1.0.0] — 2026-10-17

First stable release. A headless Rust agent frontend that drives a local
`veloren-server-cli` through `veloren-client`: tick loop without rendering,
stdio/JSON command interface, JSON-lines observations (`schema_version=1`),
server lifecycle auto-recovery, Prometheus metrics, Flask dashboard, and CI.

### Added

- Headless `veloren-agent` binary: `move`/`chat`/`command`/`status`/`patrol`/`quit`
  stdio commands and JSON-lines observations (tick, position, health, chat, events)
  — [2875204][], [65bbdc7][]
- Authenticated server access via `VELOREN_USERNAME` / `VELOREN_PASSWORD` env vars
  — [1cc6b25][]
- Server lifecycle auto-recovery: exponential-backoff reconnect, re-authenticate,
  re-enter the game, resume ticking; clean exit (code 0) on `quit`
  — [234293c][], [57bbeb1][], [1b3fa74][]
- `docker-compose.yml`: three-service stack (veloren-server, veloren-agent,
  veloren-dashboard), configuration via `.env` (gitignored, no committed secrets)
  — [b061c3d][]
- Prometheus `GET /metrics` with SQLite tick-latency baselines; dashboard
  `GET /api/metrics` JSON endpoint (API-key protected) — [72d126f][], [ed5fd4e][]
- Dashboard Performance tab: metric cards and rolling tick-latency sparkline
  — [ed5fd4e][]
- Integration test suite: mock TCP server plus server-gated end-to-end tests
  that skip when no local server runs; 8 tests pass (3 mock + 5 lib)
  — [76538c9][], [1b3fa74][]
- Kanban agent personas (Hermes/GLaDOS coordinator, R&D Worker, QA), AGENTS.md,
  SOUL.md, KANBAN.md, and the autonomous R&D pipeline (pipeline/, .hermes/cron)
  — [8fe3bb5][], [dcf9164][]
- Full documentation: quickstart, architecture, API reference, troubleshooting,
  contributing, and project wiki — [2f10fac][], [80b4681][]
- CI: shell-based workflow, GitHub Pages deployment, nightly
  `cargo build/test --release -D warnings` job, Docker sandbox job, and the
  veloren-compat patch set (patches/) — [5ddb753][], [2fbe8dc][], [ddc9134][]

### Changed

- Removed `--no-auth` from server startup (public listing) — [03dc7c8][]
- Normalized `VELUREN_*` env-var typos to `VELOREN_*` across `.env.example`,
  docs, wiki, and scripts — [c77d7cf][], [80b4681][]
- Toolchain pinned to nightly via `rust-toolchain.toml`; CI builds/tests with
  `-D warnings` on nightly — [872e8ea][], [2fbe8dc][]
- CI shell template pinned to `ci-shell.yml@main` — [10c46d7][]

### Fixed

- Duplicate `veloren-common` dependency causing `ControllerInputs` type mismatch
  — [c87a44c][]
- Blocking stdin drain that froze the tick loop; stdin is now drained on a
  reader thread into an mpsc channel, polled non-blockingly each tick
  — [57bbeb1][]
- `src/main.rs` compile errors and warnings (Clock API, `Error` Display impl,
  unused imports) — [872e8ea][]
- CI nightly build: veloren checkout path resolution, `shred` patch path and
  format — [68e763b][], [ddc9134][]
- `verify-readiness.sh` timing; Dockerfile base image changed to `rust:bookworm`
  with nightly installed via rustup — [c77d7cf][], [9863847][]

[2875204]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/2875204
[65bbdc7]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/65bbdc7
[1cc6b25]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/1cc6b25
[234293c]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/234293c
[57bbeb1]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/57bbeb1
[1b3fa74]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/1b3fa74
[b061c3d]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/b061c3d
[72d126f]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/72d126f
[ed5fd4e]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/ed5fd4e
[76538c9]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/76538c9
[8fe3bb5]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/8fe3bb5
[dcf9164]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/dcf9164
[2f10fac]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/2f10fac
[80b4681]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/80b4681
[5ddb753]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/5ddb753
[2fbe8dc]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/2fbe8dc
[ddc9134]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/ddc9134
[03dc7c8]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/03dc7c8
[c77d7cf]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/c77d7cf
[872e8ea]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/872e8ea
[10c46d7]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/10c46d7
[c87a44c]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/c87a44c
[68e763b]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/68e763b
[9863847]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/9863847
[bbd7c80]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/bbd7c80
[e24aa79]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/e24aa79
[5eb9694]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/5eb9694
[103a35e]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/103a35e
[87ab548]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/87ab548
[61a5197]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/61a5197
[0b3f343]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/0b3f343
[0f00f74]: https://github.com/itsdarklikehell/veloren-agent-intergration/commit/0f00f74
