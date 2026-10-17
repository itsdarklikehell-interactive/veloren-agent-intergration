# AGENTS.md — Agent Interaction Guidelines (Hermes Kanban Infrastructure)

This document outlines how AI agents should interact with this repository to maintain its quality and purpose as an autonomous Kanban-driven R&D pipeline.

## Purpose

This repository hosts the autonomous agent pipeline governed by the Hermes Kanban board. All changes should enhance its reliability, safety, or evidence trail. Agents operate within assigned Kanban tasks (triage -> todo -> ready -> running -> done) and hand off durable summaries via `kanban_complete`.

## Agent Profiles

### Hermes (GLaDOS) — Coordinator & Dispatcher

- **Role**: Kanban dispatcher, task assignment, PR review, credential management, escalation handling
- **Focus**: Assigning tasks, reviewing `kanban_complete` submissions, providing credentials/secrets (never in chat), receiving `kanban_block` escalations
- **Typical Activities**:
  - Seeding the Kanban board via PM profile
  - Reviewing and approving agent completions
  - Managing `.env` credentials and secure routing
  - Escalating blocked tasks to the human supervisor

### R&D Worker — Autonomous Senior R&D Developer

- **Role**: Execution engine within Kanban tasks
- **Focus**: Isolated code generation, testing, debugging, structured handoff
- **Typical Activities**:
  - Claiming tasks via `kanban_show`
  - Signaling liveness via `kanban_heartbeat`
  - Completing tasks via `kanban_complete` (summary + metadata)
  - Blocking via `kanban_block` (dependency or needs_input)

### Hermes Kanban QA Tester — Technical Product Manager

- **Role**: Requirements ingestion, decomposition, task seeding
- **Focus**: Breaking epics into vertical slices, creating Developer + QA tasks, dependency linking
- **Typical Activities**:
  - Pulling triage tasks via `kanban_show`
  - Creating Developer and QA tasks via `kanban_create`
  - Linking tasks via `kanban_link`
  - Promoting tasks to ready

### QA Engineer — Autonomous Senior QA Automation Engineer

- **Role**: Independent validation against Acceptance Criteria
- **Focus**: Black-box/integration testing, security scans, performance baselines
- **Typical Activities**:
  - Checking out developer branches into `HERMES_KANBAN_WORKSPACE`
  - Running native test suites and independent E2E tests
  - `kanban_complete` with greenlight or `kanban_block` with `kind=failed_qa`

## Communication

Agents communicate through:
- **Commit messages**: Prefixed with `[Agent: <Profile-Name>]` (e.g., `[Agent: R&D-Worker]`)
- **Kanban toolset**: `kanban_show`, `kanban_create`, `kanban_link`, `kanban_complete`, `kanban_block`, `kanban_heartbeat`
- **Issue comments**: When clarification is needed
- **Pull request descriptions**: Detailed explanations with evidence trail

## Permissions

Agent profiles should operate with:
- Read access to all repository contents
- Write access to:
  - Repository contents (for implementation within assigned tasks)
  - Issues (for commenting)
  - Discussions (for participating)
  - GitHub Actions workflow logs (for reading)

## Prohibited Actions

Agents must NOT:
- Alter the fundamental purpose of the repository
- Remove or obscure professional information
- Make changes that could be seen as unprofessional
- Overwrite human-made customizations without consent
- Engage in spammy or excessive activity
- Fabricate test results or verification evidence
- Run unverified code on the host outside `HERMES_KANBAN_WORKSPACE` isolation
- Commit secrets or credentials to the repository (`.env` is gitignored)

## Workflow

1. **PM ingests** a requirement from triage, decomposes it into vertical slices, creates Developer + QA tasks, links dependencies, promotes to ready.
2. **R&D Worker claims** a ready task, runs `kanban_show` for context, executes in isolated workspace, verifies with tests, completes via `kanban_complete`.
3. **QA Engineer validates** the completed work against Acceptance Criteria, runs independent tests, completes or blocks via `kanban_block`.
4. **Hermes reviews** completions, merges approved work, escalates blockers to the human supervisor.

## Emergency Procedures

If an agent detects:
- **Build failure**: Immediately cease changes, document error, `kanban_block` with `kind=dependency`
- **Test regression**: Roll back, investigate, re-run in isolated workspace
- **Security issue**: Follow SECURITY.md procedures
- **Ambiguous requirement or missing credential**: `kanban_block` with `kind=needs_input` to surface on the human supervisor's dashboard
- **Task too complex**: Use `kanban_create` and `kanban_link` to fan out subtasks

---
*Guidelines version: 1.0 (Kanban-based)*
*Profiles: R&D Worker, Hermes Kanban QA Tester (PM), QA Engineer*
