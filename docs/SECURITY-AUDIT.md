# Security Audit — Veloren Agent Dashboard & Credentials

**Repo:** `veloren-agent-intergration` (`/home/rizzo/Work/veloren-agent-intergration`)
**Branch:** `main` (remote: `origin` = `https://github.com/itsdarklikehell/veloren-agent-intergration.git`)
**Audit profile:** security-auditor
**Kanban task:** `61a6f427-1264-4ec6-b678-cb5c337761c9`
**Scope:** dashboard auth (`web/app.py`, `web/templates/*`, `web/static/app.js`), credentials (`.env`, `.env.example`, `VELOREN_PASSWORD` in `src/main.rs`), `.gitignore`, `.dockerignore`, `Dockerfile*`, `docker-compose.yml`, `scripts/*`, git history, container image layers.

---

## Summary

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| F1 | `DASHBOARD_API_KEY` embedded in HTML page responses (`window.DASHBOARD_API_KEY` + on-page display) | HIGH | Fixed |
| F2 | `DASHBOARD_API_KEY` value printed to stdout / container logs at startup | HIGH | Fixed |
| F3 | `.env` (with `VELOREN_PASSWORD`) gitignored and never committed | — | PASS (verified) |
| F4 | API key transmitted header-only; constant-time comparison | — | PASS (verified) |
| F5 | Dockerfiles bake no secrets (`.dockerignore` excludes `.env`; compose uses `env_file` at runtime) | — | PASS (verified) |
| F6 | Container image layers contain no credentials (build + layer inspection) | — | PASS (verified) |
| F7 | Runtime artifacts (`web/agent.log`, `target/`, `__pycache__/`) were tracked in git | MEDIUM | Fixed |

---

## Findings

### F1 — HIGH: Dashboard API key leaked into page source

`web/app.py` passed `api_key=DASHBOARD_API_KEY` into both `dashboard.html` and `viewport.html`, and the templates rendered:

```html
<script>window.DASHBOARD_API_KEY = {{ api_key|tojson }};</script>
```

`viewport.html` additionally rendered the key into the page (`<code id="vp-api-key">`). Any party able to view the page source (browser extensions, screenshots, XSS, shoulder-surfing on a shared workstation) obtains the full dashboard API key. The frontend then sent it as the `X-API-Key` header — header-based transport itself is sound, but the key never needed to reach the client embedded in the HTML.

**Remediation:** Removed `api_key=` from both `render_template` calls; deleted the inline `window.DASHBOARD_API_KEY` script and the on-page key display from both templates; the JS `request()` helpers now read the key from `localStorage` (`veloren_dashboard_api_key`) and send it as `X-API-Key`. Operators set it once per browser via `localStorage.setItem("veloren_dashboard_api_key", "<key>")`. The key never touches the page source.

### F2 — HIGH: Dashboard API key printed to container logs

`web/app.py` printed the actual key value at startup:

```text
  API key: <actual key>  (set DASHBOARD_API_KEY to override; ...)
```

With the dashboard bound to `0.0.0.0:5000` in Compose, stdout/stderr is captured by the container runtime, so a randomly auto-generated key (or the configured one) was persisted in `docker compose logs veloren-dashboard`. `docs/docker-compose.md` documented this behavior.

**Remediation:** Startup banner now reports only whether `DASHBOARD_API_KEY` is set in the environment or auto-generated — the value is never printed (`KEY_LEAKED_IN_BANNER: False`, verified by running the image with `DASHBOARD_API_KEY=supersecretkey123` and confirming the string does not appear in output). `docs/docker-compose.md` updated accordingly ("never logged or embedded in HTML").

### F3 — PASS: `.env` with `VELOREN_PASSWORD` is gitignored and never committed

- `.gitignore` excludes `.env` ("Environment variables (never commit real credentials)").
- `git ls-files .env` → not tracked; `git log --all -- .env` → no history; `.env` was never added in any commit.
- `.dockerignore` excludes `.env` from build context.
- `docker-compose.yml` loads `.env` via `env_file` (runtime injection — never baked into an image layer).
- `src/main.rs` reads `VELOREN_PASSWORD` from `env::var(...)` and panics on absence without ever printing it; `scripts/verify-readiness.sh` and `scripts/test-integration.sh` require it via `: "${VELOREN_PASSWORD:?...}"` (no echo).

`.env.example` is tracked but contains only placeholders (`VELOREN_PASSWORD=your-password-here`, empty `DASHBOARD_API_KEY=`), introduced in commit `1cc6b25` with no real values.

### F4 — PASS: API key transmission is header-based

- Accepted via `X-API-Key` header (documented as preferred), `Authorization: Bearer <key>`, and `Authorization: Basic user:<key>`.
- Comparison uses `secrets.compare_digest` (constant-time) in `_require_api_key()`; `401` responses return `{"error": "unauthorized"}` and never echo the key.
- Public endpoints (`GET /`, `GET /api/health`, `GET /metrics`, `/viewport`, `/api/viewport/*`) require no key; everything else is guarded by `@app.before_request`.

### F5 — PASS: Dockerfiles contain no embedded secrets

- `Dockerfile.dashboard` / `Dockerfile` / `Dockerfile.server` use `COPY . .`, but `.dockerignore` excludes `.env`, `.git/`, `/target/`, `.hermes/`, `__pycache__/`.
- No `ENV`/`ARG`/`RUN echo` carries credentials. Only non-secret config is baked (`VELOREN_DASHBOARD_HOST`, `VELOREN_DASHBOARD_PORT`, `PYTHON*` vars).

### F6 — PASS: Container image layers contain no credentials

- Built `Dockerfile.dashboard` image and inspected it: `/app/.env` absent (`RESULT:ENV_CLEAN`); no `VELOREN_PASSWORD=` / real `DASHBOARD_API_KEY` values anywhere in the filesystem.
- `docker history` shows layers: base image, pip install, `COPY . .` (8.47 MB, filtered by `.dockerignore`), CMD. No secret layer.

### F7 — MEDIUM: Runtime artifacts tracked in git

`web/agent.log` (7.9 MB of agent stdout), `target/` build artifacts, and `web/__pycache__/app.cpython-314.pyc` were tracked (agent.log introduced in commit `3a242bb`). Logs can contain session/game data and expand the leak surface if the repo is cloned or mirrored.

**Remediation:** `git rm -r --cached target web/agent.log web/__pycache__` (files kept on disk, removed from git) and added `web/agent.log`, `web/__pycache__/`, `__pycache__/` to `.gitignore` (`.env` and `/target/` were already listed).

---

## Remediations applied

1. `web/app.py` — key removed from template context; startup banner no longer prints the key value; auth docstring updated (key never logged, clients supply it themselves).
2. `web/templates/dashboard.html` — deleted `<script>window.DASHBOARD_API_KEY = ...</script>`.
3. `web/templates/viewport.html` — deleted inline key script and on-page key display; JS reads key from `localStorage`.
4. `web/static/app.js` — `request()` reads `localStorage.getItem("veloren_dashboard_api_key")`; comment documents operator setup.
5. `.gitignore` — added `web/agent.log`, `web/__pycache__/`, `__pycache__/`.
6. `docs/docker-compose.md` — corrected the statement about the key being printed to container links; explains `localStorage`/header delivery.
7. Untracked `target/`, `web/agent.log`, `web/__pycache__/` from git.

## Verification

- Static secret scan (added/changed lines across `git diff HEAD`): 0 hits for hardcoded `api_key`/`secret`/`password`/`token` assignments, `os.system`, `shell=True`, `eval`/`exec`, `pickle.load`.
- `python3 -m py_compile web/app.py` → clean.
- `docker build -f Dockerfile.dashboard` → image contains no `.env`, no secret values.
- Runtime banner check with `DASHBOARD_API_KEY=supersecretkey123` → key string absent from output.

---

## Commit

- Commit: `<commit-hash-filled-after-commit>`
- Files: `.gitignore`, `web/app.py`, `web/static/app.js`, `web/templates/dashboard.html`, `web/templates/viewport.html`, `docs/docker-compose.md`, `docs/SECURITY-AUDIT.md`, plus removal of `target/`, `web/agent.log`, `web/__pycache__/` from tracking.
- Note: unrelated pre-existing working-tree changes (`Dockerfile.server` mold package, README viewport section, `docs/troubleshooting.md`, `src/lib.rs` patrol/wander constants) were left unstaged and are not part of this audit commit.
