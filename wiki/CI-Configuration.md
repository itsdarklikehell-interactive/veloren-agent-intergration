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
