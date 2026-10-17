# KANBAN.md — Kanban Agent Workflow

This document explains how autonomous agents claim, execute, and complete tasks within the Hermes Kanban infrastructure for the Veloren Agent Integration repository.

The Kanban personas and their responsibilities are the source of truth in [`SOUL.md`](SOUL.md) and [`AGENTS.md`](AGENTS.md). This file is the operational guide for the task lifecycle itself.

## Kanban columns

Tasks flow through these columns:

| Column | Meaning |
|---|---|
| **triage** | New requirements landed but not yet decomposed |
| **todo** | Decomposed tasks waiting to be pulled |
| **ready** | Tasks pulled into the claimable pool; any matching agent may claim |
| **running** | Task actively being executed by a claimed agent |
| **done** | Task completed and verified (by QA or Hermes) |
| **blocked** | Task cannot proceed; a reason (`dependency` or `needs_input`) is recorded |

## Agent profiles

| Profile | Kanban role | Primary tools |
|---|---|---|
| **Hermes (GLaDOS)** — Coordinator & Dispatcher | Seeds the board, assigns tasks, reviews completions, manages credentials, escalates blockers | `kanban_show`, `kanban_complete` review, `.env` routing |
| **R&D Worker** — Autonomous Senior R&D Developer | Claims and executes Developer tasks | `kanban_show`, `kanban_heartbeat`, `kanban_complete`, `kanban_block`, `kanban_create`, `kanban_link` |
| **Hermes Kanban QA Tester** — Technical Product Manager (PM) | Pulls triage, decomposes epics, seeds Developer + QA tasks, links dependencies, promotes to ready | `kanban_show`, `kanban_create`, `kanban_link` |
| **QA Engineer** — Autonomous Senior QA Automation Engineer | Validates completed Developer work against Acceptance Criteria | `kanban_show`, `kanban_complete` (greenlight), `kanban_block` (`kind=failed_qa`) |

## How agents claim and complete tasks

### Step 1 — Pull task context

When spawned into a running task, immediately invoke `kanban_show` to pull the core task data:

- title
- requirements / Acceptance Criteria
- previous runs and parent handoffs
- comments

Read the summaries and metadata of completed parent tasks to extract API definitions, data schemas, or architectural requirements. **Do not guess upstream context.**

### Step 2 — Signal liveness

During intensive compilation or research phases, periodically fire `kanban_heartbeat` so the dispatcher knows the task is still alive.

### Step 3 — Execute in isolation

Execute all code generation and testing strictly within the environment designated by `HERMES_KANBAN_WORKSPACE` (Docker sandbox or git worktree). Do not run unverified code on the host outside this isolation.

### Step 4 — Verify

Implement features alongside comprehensive test suites. Verify using automated test frameworks inside the sandbox until all tests report a 100% success state.

### Step 5 — Complete the task

When all verification constraints are met, invoke `kanban_complete` with:

- **summary**: A concise markdown narrative detailing exactly what changed, how it was verified, what open risks remain, and what the next downstream agent profile needs to know.
- **metadata**: Key-value data containing structured indicators like target branch names, issue IDs, generated endpoints, or exact test counts.

Keep secrets, raw logs, tokens, or lengthy stack traces out of task metadata.

Once completed, the dispatcher automatically evaluates child task dependencies and promotes them from `todo` to `ready`.

### Step 6 — Block when stuck

If a task cannot proceed, invoke `kanban_block` with a `kind`:

| kind | When to use |
|---|---|
| `dependency` | An upstream dependency is broken or missing |
| `needs_input` | Technical ambiguity, missing API key/credential, or failure persists across retries and requires human supervisor intervention |
| `failed_qa` | QA validation failed (QA Engineer only) — include exact reproduction steps, input payloads, and expected vs. actual outputs |

## Fan-out for complex tasks

If a task is too complex or requires multiple specializations, use `kanban_create` and `kanban_link` to programmatically fan out subtasks to relevant specialist agent profiles. Link new Developer and QA tasks as children of the main epic, with the QA task dependent on the Developer task.

## Commit conventions

All artifacts must be committed with `[Agent: <Profile-Name>]` prefixed commit messages, e.g.:

- `[Agent: R&D-Worker] Implement move command`
- `[Agent: QA-Engineer] Add integration tests for agent status`
- `[Agent: Hermes-Kanban-QA-Tester] Seed weekly R&D tasks`

## Lifecycle example

1. PM pulls a triage task, decomposes it into a Developer task and a QA task, links them, promotes the Developer task to `ready`.
2. R&D Worker claims the Developer task via `kanban_show`, implements in `HERMES_KANBAN_WORKSPACE`, runs tests, heartbeats during long builds, then `kanban_complete` with summary + metadata (branch name, test count).
3. Dispatcher promotes the dependent QA task to `ready`.
4. QA Engineer checks out the Developer's branch, runs independent integration/E2E tests and security scans, then `kanban_complete` with greenlight or `kanban_block` with `kind=failed_qa`.
5. Hermes reviews the completion, merges approved work, or escalates any `needs_input` blocker to the human supervisor.
