# CI Configuration

GitHub Actions CI for the Veloren Agent Integration project.

## Workflow

File: `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  ci:
    uses: itsdarklikehell/ci-templates/.github/workflows/ci-shell.yml@v1
    with:
      shellcheck: true
      shfmt: true
    secrets: inherit
```

This uses the shared [itsdarklikehell/ci-templates](https://github.com/itsdarklikehell/ci-templates)
`ci-shell.yml` reusable workflow. On every push and PR to `main`, it:

1. Checks out the repository
2. Installs shellcheck and runs it on all `*.sh` files (advisory, `continue-on-error`)
3. Installs shfmt and checks formatting on all `*.sh` files (advisory, `continue-on-error`)

## Local checks

Before pushing, run the same checks locally:

```bash
shellcheck scripts/*.sh
shfmt -d -i 2 -ci scripts/*.sh
```

Auto-format:

```bash
shfmt -w -i 2 scripts/*.sh
```

## Why ci-shell.yml?

The project's user-facing interface is shell scripts (`scripts/`). The Rust agent build currently requires nightly Rust or registry patches due to `nightly` feature usage in `hashbrown`/`shred`/`foldhash` (transitive deps of `veloren-client`), so Rust compilation is not yet part of CI. Once the build is stable on stable Rust, a `ci-rust.yml` or similar can be added.

## ci-templates reference

Source: https://github.com/itsdarklikehell/ci-templates

Available workflows: `ci-node.yml`, `ci-python.yml`, `ci-go.yml`, `ci-generic.yml`, `ci-shell.yml`.

## Additional jobs

### `nightly` — zero-warning nightly build

Runs on every push/PR to `main`. Installs the nightly Rust toolchain and verifies
`RUSTFLAGS="-D warnings" cargo build --release` (zero warnings) followed by
`cargo test --release`. The agent depends on `veloren-client` from Veloren's
gitlab `weekly` tag, which exercises unstable features; nightly is the
canonical toolchain for this project (see `docs/troubleshooting.md`).

### `docker-sandbox` — Docker build & test

Builds `Dockerfile` (Rust sandbox image) and runs `cargo build --release` and
`cargo test --release` inside the container, so the agent builds and tests in
an isolated environment without a host Rust toolchain.

## Headless readiness (`hermes verify`)

`hermes verify --json` no longer polls HTTP `127.0.0.1:8000`. The saved recipe
at `.hermes/environment.json` marks the agent as headless (`start: null`, no
port), so `hermes verify` runs `cargo build` + `cargo test` and skips the
inapplicable HTTP readiness phase. Headless readiness is verified over stdio/JSON
instead — see `scripts/verify-readiness.sh` (pipe `{"cmd":"status"}` to the
agent, assert a JSON observation with `"tick"` is emitted, then `quit`).

Local readiness check (requires the local server in tmux `veloren-server`):

```bash
export VELOREN_USERNAME=hermes VELOREN_PASSWORD=<password>
./scripts/verify-readiness.sh
```

## Agent profile alignment

CI is operated and reviewed by the **Hermes (GLaDOS)** Kanban profile — Coordinator & Dispatcher:

- Hermes reviews shellcheck/shfmt results and nightly/Docker-sandbox build outcomes on every push and PR to `main`.
- Escalates persistent CI failures (build blocked, test regression) via `kanban_block` with `kind=dependency`.
- Treats green CI as evidence that changes are safe to merge before QA Engineer validation.

Commit all changes with `[Agent: Hermes]` prefixed commit messages.
