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

## Project conventions

- **Shell scripts:** POSIX-compatible where possible, `bash` shebang, `set -euo pipefail`
- **Rust:** 2021 edition, `#![deny(unsafe_code)]`, clippy clean
- **Commits:** Clear, descriptive messages. Prefix with `[Agent: GLaDOS]` if made by the GLaDOS agent
- **Docs:** Update `docs/` and `README.md` together when changing behavior

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
