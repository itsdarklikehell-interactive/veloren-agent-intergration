# Veloren Agent Integration — Delivery Plan

**Repo:** /home/rizzo/Work/veloren-agent-intergration (remote: itsdarklikehell/veloren-agent-intergration, branch: main)
**Date:** 2026-10-17
**Active profile:** project-lead
**Kanban task:** 55f72c3f-61d4-4406-b695-18ee578ee554
**Local Kanban:** 9 tasks (6 done, 3 todo) | GitHub Projects #3 synced
**Profiles:** project-lead, research-development (R&D Worker), qa-testing (QA Engineer)

---

## 1. Current status summary

**Done (6/6 original Kanban tasks)**
- Headless Rust agent frontend (`src/main.rs`) built against `veloren-client` (Veloren `weekly` tag); binary present at `target/release/veloren-agent`.
- Web dashboard built and tested under `web/`: Flask app (`app.py`) serving `dashboard.html` + static assets; endpoints `/api/status`, `/api/server-logs`, `/api/agent-logs`, `/api/observations`, and `POST /api/{server,agent}/{start,stop}` plus `/api/agent/command`. Credentials from `.env` are never exposed by the dashboard.
- Shell CI green: `.github/workflows/ci.yml` runs the shared `ci-templates ci-shell.yml` with shellcheck + shfmt on all `*.sh`.
- Dockerfile + `pipeline/run-pipeline.sh` + `pipeline/cron-weekly-sync.sh` in place; `.hermes/cron/rd-agent-cron` schedule configured.

**Open**
- **CI nightly build fix (R&D / research-development):** `ci.yml` nightly job ("Zero-warning nightly build") patches transitive deps via a hardcoded path (`~/.cargo/git/checkouts/veloren-*/f97be34`) plus a `sed` on `shred-0.15.0/src/meta.rs`. Environment-specific and fragile — nightly job is not green. Stable toolchain build is blocked on nightly features in transitive deps (`hashbrown`, `shred`, `foldhash`).
- **QA validation of dashboard (QA / qa-testing):** dashboard exists but has not been independently validated against acceptance criteria.

**Known risks (from docs/troubleshooting.md)**
- hickory-resolver cannot resolve `127.0.0.1` for teloren (workaround: `no_proxy=*` or raw `TcpStream` client).
- Server world generation must complete before port 14004 accepts connections.

---

## 2. Milestone plan

| Milestone | Priority | Owner (profile) | Target | Done criterion |
|-----------|----------|-----------------|--------|----------------|
| **M1 CI pipeline full green status** | P0 | R&D Worker — research-development | 2026-10-23 | shell-ci green AND nightly job green on `main` for 2 consecutive runs; `cargo +nightly build --release` + `cargo +nightly test --release` with `RUSTFLAGS="-D warnings"` pass; patch step no longer depends on `~/.cargo/git/checkout` path; merged via PR with `[Agent: R&D-Worker]` commits and QA greenlight. |
| **M2 Dashboard production readiness** | P1 | QA Engineer — qa-testing | 2026-10-30 | QA validates all 9 dashboard endpoints (black-box + input validation on `/api/agent/command`, credential non-exposure, error handling, concurrency, log tails); runbook documented; prod run documented (gunicorn, `0.0.0.0:5000`, env config); `kanban_complete` greenlight or `kanban_block kind=failed_qa`. |
| **M3 Agent feature roadmap** | P2 | Hermes — project-lead | 2026-11-13 | Roadmap doc with prioritized vertical slices (agent commands move/chat/command/status/quit, observation schema stability, A2A stdio/JSON bridge to Hermes, server lifecycle integration, performance baselines); Developer + QA tasks created, linked, and promoted to ready in Kanban. |

Target dates assume work begins 2026-10-17; M1 and M2 can run in parallel (different owners). M3 starts after M1/M2 complete or once triage capacity allows.

---

## 3. Recommended next steps (this week)

1. **R&D (research-development) — claim CI nightly task:** run `kanban_show`, rewrite the nightly job's patch step to apply `patches/veloren-weekly-toolchain.patch` inside the repository workspace after `cargo fetch` (instead of the `~/.cargo/git/checkout/...` path), verify `cargo +nightly build --release` and `cargo +nightly test --release` with `-D warnings` locally, then open a PR and merge with QA greenlight.
2. **QA (qa-testing) — claim dashboard validation task:** run `kanban_show`, execute the endpoint test matrix against a local `python3 web/app.py` instance, record results, then `kanban_complete` with greenlight or `kanban_block kind=failed_qa` with exact reproduction steps.
3. **Hermes (project-lead):** review both completions, merge approved work, escalate any `needs_input` blockers to the human supervisor, then seed M3 roadmap tasks into Kanban triage and decompose into Developer + QA slices.

---

## 4. Evidence basis

Verified by reading repository files: `README.md`, `AGENTS.md`, `SOUL.md`, `KANBAN.md`, `.github/workflows/ci.yml`, `web/app.py`, `pipeline/run-pipeline.sh`, `patches/veloren-weekly-toolchain.patch`, `docs/troubleshooting.md`. CI shell job status is per project context (green); nightly job and dashboard QA are unverified open items — fresh verification evidence required before marking either complete.

---
*Commit convention: `[Agent: project-lead] Add delivery plan for M1–M3 milestones`*
