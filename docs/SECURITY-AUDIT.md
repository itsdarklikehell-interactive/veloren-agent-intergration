# Security Audit — Veloren Agent Integration

**Repo:** `veloren-agent-intergration` (`/home/rizzo/Work/veloren-agent-intergration`)
**Branch:** `main` (remote: `origin` = `https://github.com/itsdarklikehell/veloren-agent-intergration.git`)
**Audit profile:** security-auditor
**Audit date:** 2026-10-18
**Scope:** dashboard auth (`web/app.py`, `web/templates/*`, `web/static/app.js`), credentials (`.env`, `.env.example`, `VELOREN_PASSWORD` in `src/main.rs`), `.gitignore`, `.dockerignore`, `Dockerfile*`, `docker-compose.yml`, `scripts/*`, git history.
**Method:** static review + git-history sweep (`git log --all -- .env`, `git ls-files`) + runtime banner test (`DASHBOARD_API_KEY=<canary>` → canary absent from output) + Python syntax check.

---

## Summary

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| F1 | Hardcoded secrets in tracked code | — | PASS (none found) |
| F2 | `.env` gitignored and never committed | — | PASS (verified) |
| F3 | API key transmitted header-only; constant-time comparison | — | PASS (verified) |
| F4 | Dockerfiles bake no secrets | — | PASS (verified) |

**Verdict:** No active findings. All credential-handling controls verified at HEAD. Local `.env` must remain out of git; dashboard should sit behind TLS + access control if exposed beyond localhost (see `docs/docker-compose.md`, "Production notes").

---

## F1 — PASS: No hardcoded secrets in tracked code

Repo-wide sweeps for hardcoded credentials (password/secret/api_key/token assignments, `sk_*`, `ghp_*`, `glpat-*`, `xoxb-*`, 32+ hex strings) across all tracked files (excluding `target/`, `.git/`, `Cargo.lock` checksums):

- **0 hits** for real secret material.
- `Cargo.toml` / `Cargo.lock` / `docs/docker-compose.md` contain only Veloren git revision hashes (dependency pins), not secrets.
- This document references only the test canary `AUDIT_DO_NOT_LEAK_9f3a` (used for runtime banner tests) and historical finding descriptions — no live credentials.
- `.env.example` (tracked) contains placeholders only: `VELOREN_PASSWORD=your-password-here`, empty `DASHBOARD_API_KEY=`, `LINEAR_API_KEY=`.
- `src/main.rs` reads `VELOREN_PASSWORD` via `env::var(...)` and panics on absence without printing it (lines 117-119).
- `scripts/*`: 0 hits for hardcoded passwords/secrets/tokens.
- `.env` on disk is local-only, untracked, and gitignored (see F2) — it must never be staged or committed.

## F2 — PASS: `.env` with `VELOREN_PASSWORD` is gitignored and never committed

- `.gitignore` excludes `.env` ("Environment variables (never commit real credentials)").
- `git log --all -- .env` → no history in any branch; `.env` was never added in any commit.
- `git ls-files` returns nothing for `.env`; only `.env.example` is tracked for that pattern.
- `.dockerignore` excludes `.env` from build context (line 9); `docker-compose.yml` loads `.env` via `env_file` at runtime (never baked into an image layer — see F4).

## F3 — PASS: API key transmission is header-only, constant-time comparison

- Accepted via `X-API-Key` header (documented preferred), `Authorization: Bearer <key>`, or `Authorization: Basic user:<key>` (`web/app.py`, `_extract_api_key()`, lines 400-412).
- Sweep for query-parameter key transmission (`?api_key=`, `request.args.get` for key): **0 hits** in code.
- Comparison uses `secrets.compare_digest(key, DASHBOARD_API_KEY)` (constant-time) in `_require_api_key()` (line 436).
- `401` responses return `{"error": "unauthorized"}` and never echo the key (lines 437-441).
- Public endpoints (`GET /`, `GET /api/health`, `GET /metrics`, `/viewport`, `/api/viewport/*`) require no key; everything else is guarded by `@app.before_request` (lines 423-441).
- Frontend (`web/static/app.js`) reads the key from `localStorage` (`veloren_dashboard_api_key`) and sends it as the `X-API-Key` header. `web/static/app.js` line 14 retains a `window.DASHBOARD_API_KEY ||` fallback, but `window.DASHBOARD_API_KEY` is never set in any template (0 references in `web/templates/*`), so the fallback is inert and the key never touches page source.
- Startup banner reports only `DASHBOARD_API_KEY set in environment` or `auto-generated at startup (never logged)` — the actual key value is never printed (lines 1065-1073).

**Runtime test:** `DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a timeout 4 python3 web/app.py` → canary string occurred **0 times** in output; banner prints only "set in environment".

## F4 — PASS: Dockerfiles contain no embedded secrets; `.env` never baked into images

- `Dockerfile`, `Dockerfile.dashboard`, `Dockerfile.server`: no `ENV`/`ARG`/`RUN` carries credentials. Only non-secret config is baked (`PYTHONDONTWRITEBYTECODE`, `PYTHONUNBUFFERED`, `VELOREN_DASHBOARD_HOST`, `VELOREN_DASHBOARD_PORT`).
- `.dockerignore` excludes `.env`, `.git/`, `/target/`, `.hermes/`, `__pycache__/` from build context.
- `docker-compose.yml`: all three services use `env_file: - .env` / `environment` at runtime; no credentials hardcoded in the Compose file (see `docs/docker-compose.md`, "Environment variable reference").

---

## Verification

| Check | Command | Result |
|---|---|---|
| Python syntax | `python3 -m py_compile web/app.py` | clean |
| Key never logged (banner) | `DASHBOARD_API_KEY=AUDIT_DO_NOT_LEAK_9f3a timeout 4 python3 web/app.py` | **0 occurrences** of canary |
| Query-param key usage | repo-wide sweep for `?api_key=` / `args.get` key patterns | 0 hits in code |
| Constant-time compare | `grep secrets.compare_digest web/app.py` | 1 hit (`_require_api_key`, line 436) |
| `.env` not baked | `.dockerignore` line 9 excludes `.env` | present |
| `.env` never committed | `git log --all -- .env` | no history |
| Sensitive files untracked | `git ls-files | grep -E '(\.env$)'` | only `.env.example` |
| Header-based auth | `grep -n "X-API-Key\|Authorization" web/app.py` | header-only extraction, lines 400-412 |

---

## Commit

- Security audit re-verification at HEAD: `docs/SECURITY-AUDIT.md` (this audit cycle).
- No code changes required; all controls verified at HEAD.
