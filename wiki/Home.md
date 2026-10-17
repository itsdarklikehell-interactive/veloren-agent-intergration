# Home

Welcome to the Veloren Agent Integration wiki.

This is the operations and knowledge base for the project. For design docs, API specs, and getting started guides, see the [docs folder in the repo](https://github.com/itsdarklikehell/veloren-agent-intergration/tree/main/docs).

## Pages

| Page | Description |
|---|---|
| [Server Operations](Server-Operations) | Starting, stopping, and monitoring the local Veloren dedicated server |
| [Agent Runbook](Agent-Runbook) | Running the headless agent, sending commands, reading observations |
| [Teloren Setup](Teloren-Setup) | Connecting the terminal frontend to the local server |
| [CI Configuration](CI-Configuration) | GitHub Actions setup using ci-templates |
| [Build Troubleshooting](Build-Troubleshooting) | Fixing Rust build issues on stable toolchain |
| [Hermes Integration](Hermes-Integration) | Connecting Hermes (A2A) to the agent via stdio/JSON |

## Quick links

- [Repository](https://github.com/itsdarklikehell/veloren-agent-intergration)
- [Issues](https://github.com/itsdarklikehell/veloren-agent-intergration/issues)
- [Pull Requests](https://github.com/itsdarklikehell/veloren-agent-intergration/pulls)
- [Actions / CI](https://github.com/itsdarklikehell/veloren-agent-intergration/actions)

## Agent infrastructure (Kanban)

This project is operated by autonomous agents under the Hermes Kanban infrastructure. Agent personas and the Kanban lifecycle are defined in the repository:

- [SOUL.md](https://github.com/itsdarklikehell/veloren-agent-intergration/blob/main/SOUL.md) — Agent profiles & Kanban personas (R&D Worker, Hermes Kanban QA Tester/PM, QA Engineer)
- [AGENTS.md](https://github.com/itsdarklikehell/veloren-agent-intergration/blob/main/AGENTS.md) — Agent interaction guidelines
- [KANBAN.md](https://github.com/itsdarklikehell/veloren-agent-intergration/blob/main/KANBAN.md) — How agents claim, complete, and block Kanban tasks

The four Kanban profiles are:

| Profile | Role |
|---|---|
| Hermes (GLaDOS) — Coordinator & Dispatcher | Task assignment, PR review, credentials, escalation |
| R&D Worker — Autonomous Senior R&D Developer | Claims and executes Developer tasks |
| Hermes Kanban QA Tester — Technical Product Manager | Requirements ingestion, decomposition, task seeding |
| QA Engineer — Autonomous Senior QA Automation Engineer | Validation against Acceptance Criteria |
