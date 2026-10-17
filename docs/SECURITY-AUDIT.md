# Security Audit — Veloren Agent Dashboard & Credentials

**Repo:** `veloren-agent-intergration` (`/home/rizzo/Work/veloren-agent-intergration`)
**Branch:** `main` (remote: `origin` = `https://github.com/itsdarklikehell/veloren-agent-intergration.git`)
**Audit profile:** security-auditor
**Kanban task:** `61a6f427-1264-4ec6-b678-cb5c337761c9`
**Audit date:** 2026-10-17
**Scope:** dashboard auth (`web/app.py`, `web/templates/*`, `web/static/app.js`), credentials (`.env`, `.env.example`, `VELOREN_PASSWORD` in `src/main.rs`), `.gitignore`, `.dockerignore`, `Dockerfile*`, `docker-compose.yml`, `scripts/*`, git history, container image layers.
**Method:** static review + git-history sweep (`git log --all`, `git log -S`, `git grep` over HEAD) + runtime banner test (`DASHBOARD_API_KEY=<canary>` → canary absent from output) + container image build (`docker build -f Dockerfile.dashboard`) + container filesystem/logs inspection.

---

## Summary

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| F1 | `DASHBOARD_API_KEY` embedded in HTML page responses (`window.DASHBOARD_API_KEY` + on-page display) | HIGH | Fixed (re-verified at HEAD) |
| F2 | `DASHBOARD_API_KEY` value printed to stdout / container logs at startup | HIGH | Fixed (re-verified at HEAD, runtime test) |
| F3 | `.env` (with `VELOREN_PASSWORD`) gitignored and never committed | — | PASS (verified) |
| F4 | API key transmitted header-only; constant-time comparison | — | PASS (verified) |
| F5 | Dockerfiles bake no secrets (`.dockerignore` excludes `.env`; compose uses `env_file` at runtime) | — | PASS (verified) |
| F6 | Container image layers contain no credentials (build + layer inspection) | — | PASS (verified) |
| F7 | Runtime artifacts (`web/agent.log`, `target/`, `__pycache__/`) untracked in git | MEDIUM | Fixed (re-verified at HEAD) |
| R1 | `docker-compose.yml` comment falsely claimed the key is printed to container logs | LOW | Fixed (this cycle) |

---

## Findings

### F1 — FIXED: Dashboard API key no longer embedded in page source

Historically `web/app.py` passed `api_key=DASHBOARD_API_KEY` into `dashboard.html` and `viewport.html`, which rendered `<script>window.DASHBOARD_API_KEY = {{ api_key|tojson }};</script>` (plus an on-page key display in `viewport.html`).

**Verification at HEAD:** `web/templates/dashboard.html` and `web/templates/viewport.html` contain no `DASHBOARD_API_KEY` reference; `web/app.py` no longer passes `api_key=` to any `render_template` call; `web/static/app.js` reads the key from `localStorage` (`veloren_dashboard_api_key`) and sends it as the `X-API-Key` header. `window.DASHBOARD_API_KEY` is not set anywhere (only a harmless `||` fallback remains in `app.js`). The key never touches the page source.

**Remediation (prior cycles):** Removed `api_key=` from both `render_template` calls; deleted the inline script and on-page key display from both templates; JS helpers now read the key from `localStorage`. Operators set it once per browser via `localStorage.setItem("veloren_dashboard_api_key", "<key>")`.

### F2 — FIXED: Dashboard API key no longer printed to stdout / container logs

Historically `web/app.py` printed the actual key value at startup (`  API key: <actual key>`), persisting it in `docker compose logs`.

**Verification at HEAD + runtime test:** Startup banner now reports only `DASHBOARD_API_KEY set in environment` or `auto-generated at startup (never logged)`. Empirical check: launched `web/app.py` with `DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a`; canary string occurred **0 times** in output. Same result from the built container image (`docker run -e DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a veloren-dashboard-audit`): **0 occurrences**. `docs/docker-compose.md` states the key is never logged.

### F3 — PASS: `.env` with `VELOREN_PASSWORD` is gitignored and never committed

- `.gitignore` excludes `.env` ("Environment variables (never commit real credentials)").
- `git log --all -- .env` → no history in any branch; `.env` was never added in any commit.
- `git ls-files` returns only `.env.example` for that pattern; `web/agent.log`, `target/`, `__pycache__/` are untracked (see F7).
- `.dockerignore` excludes `.env` from build context; `docker-compose.yml` loads `.env` via `env_file` at runtime (never baked into an image layer).
- `src/main.rs` reads `VELOREN_PASSWORD` via `env::var(...)` and panics on absence without printing it; `scripts/verify-readiness.sh` and `scripts/test-integration.sh` require it via `: "${VELOREN_PASSWORD:?...}"` (no echo).
- `.env.example` is tracked but contains only placeholders (`VELOREN_PASSWORD=your-password-here`, empty `DASHBOARD_API_KEY=`, `LINEAR_API_KEY=`), introduced in commit `1cc6b25` with no real values.

**Hygiene note:** a local `.env` exists on disk (untracked, gitignored) with live credentials — it must never be staged or committed.

### F4 — PASS: API key transmission is header-only

- Accepted via `X-API-Key` header (documented preferred), `Authorization: Bearer`, or `Authorization: Basic`.
- Sweep for query-parameter key transmission (`?api_key=`, `request.args.get` for key, etc.) across the repo: **0 hits**.
- Comparison uses `secrets.compare_digest` (constant-time) in `_require_api_key()`; `401` responses return `{"error": "unauthorized"}` and never echo the key.
- Public endpoints (`GET /`, `GET /api/health`, `GET /metrics`, `/viewport`, `/api/viewport/*`) require no key; everything else is guarded by `@app.before_request`.
- Frontend sends the key as `X-API-Key` read from `localStorage`; never embedded in HTML (see F1).

### F5 — PASS: Dockerfiles contain no embedded secrets

- `Dockerfile.dashboard` / `Dockerfile` / `Dockerfile.server`: no `ENV`/`ARG`/`RUN` carries credentials. Only non-secret config is baked (`VELOREN_DASHBOARD_HOST`, `VELOREN_DASHBOARD_PORT`, `PYTHON*` vars).
- `COPY . .` is filtered by `.dockerignore` (excludes `.env`, `.git/`, `/target/`, `.hermes/`, `__pycache__/`).

### F6 — PASS: Container image layers contain no credentials

- Built image `veloren-dashboard-audit:latest` from `Dockerfile.dashboard`.
- Container filesystem inspection: `/app/.env` absent; no `VELOREN_PASSWORD=` real values or canary key anywhere under `/app` (matches only `.env.example` placeholders and documentation mentioning variable names).
- Container run with `DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a`: key string absent from stdout/stderr.
- `docker history` layers: base image, pip install, `COPY . .` (filtered by `.dockerignore`), CMD. No secret layer.

### F7 — FIXED: Runtime artifacts no longer tracked in git

Historically `web/agent.log` (7.9 MB), `target/`, and `web/__pycache__/` were tracked (`agent.log` introduced in commit `3a242bb`).

**Verification at HEAD:** `git ls-files` returns no matches for `.env`, `agent.log`, `__pycache__`, or `target/`. `.gitignore` lists `web/*.log`, `web/__pycache__/`, `__pycache__/`, `/target/` (`.env` already listed).

### R1 — FIXED (this cycle): stale `docker-compose.yml` comment

The Compose file header claimed the dashboard auto-generates `DASHBOARD_API_KEY` at startup "(printed to container logs)". That behavior was removed under F2; the comment was inaccurate. Corrected to state the key is never logged and is supplied per request via the `X-API-Key` header; added a runtime-injection note under the `veloren-dashboard` service.

---

## Remediation applied this cycle

1. `docker-compose.yml` — corrected the startup-banner claim (key is never logged); documented runtime injection via `.env` / `env_file` under `veloren-dashboard`.

All other items (F1, F2, F7 remediation in code/templates/JS; F3–F6 controls) were verified at HEAD and required no code change.

---

## Verification

| Check | Command | Result |
|---|---|---|
| Python syntax | `python3 -m py_compile web/app.py` | clean |
| Key never in page source | grep `DASHBOARD_API_KEY` in `web/templates/*`, `web/app.py` | 0 template embeds; key passed nowhere |
| Key never logged (local) | `DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a timeout 5 python3 web/app.py` | **0 occurrences** of canary |
| Key never logged (container) | `docker run -e DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a veloren-dashboard-audit` | **0 occurrences** of canary |
| `.env` not baked | `docker run --rm veloren-dashboard-audit test -f /app/.env` | absent |
| Secret sweep of image | `grep -rl 'VELOREN_PASSWORD=' /app` inside container | only docs/`.env.example` placeholders |
| `.env` never committed | `git log --all -- .env` | no history |
| Sensitive files untracked | `git ls-files | grep -E '(\.env|agent\.log|__pycache__|target/)'` | only `.env.example` |
| Query-param key usage | repo-wide sweep for `?api_key=` / `args.get` key patterns | 0 hits |

---

## Commit

- Audit re-verification + R1 fix: `docker-compose.yml`, `docs/SECURITY-AUDIT.md` (this audit cycle, task `61a6f427-1264-4ec6-b678-cb5c337761c9`).
- Prior audit fixes: `8970a8c` (`.gitignore`, `web/app.py`, `web/static/app.js`, `web/templates/dashboard.html`, `web/templates/viewport.html`, `docs/docker-compose.md`, `docs/SECURITY-AUDIT.md`, untracking of `target/`, `web/agent.log`, `web/__pycache__/`) and `0f00f74` (401 key prompt, `web/*.log` ignore, viewport cleanup).
- Note: unrelated pre-existing feature work (`Dockerfile.server` mold package, README viewport section, `docs/troubleshooting.md`, `src/lib.rs` patrol/wander constants) lives in `8970a8c`/`0f00f74`'s tree but is not part of this audit; `src/lib.rs` has further unstaged churn left out of these commits.

**Verdict:** No active findings. All dashboard auth and credential-storage controls verified at HEAD. Local `.env` must remain out of git; dashboard should sit behind TLS + access control if exposed beyond localhost (see `docs/docker-compose.md`, "Production notes").
