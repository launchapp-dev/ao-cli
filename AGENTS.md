# AGENTS.md

Agent operating guide for this repository.

## Mission

Use the AO CLI to plan and execute work on AO itself. Requirements and tasks in
`.ao/` are the planning source of truth.

## Repository Guardrails

- Keep this workspace Rust-only and non-Tauri.
- Keep first-class crates under `crates/`.
- Main command surface is `ao` from `crates/orchestrator-cli`.
- Keep `agent-runner`, `llm-cli-wrapper`, and `llm-mcp-server` buildable.
- Worktrees created by CLI/daemon must live in `~/.ao/<repo-scope>/worktrees/`.
- Agents should treat only the current repo scope as managed; do not operate in
  other repo scopes.

## Required Workflow

1. Confirm active project and current queue.
2. Pick a task from `task next` or `task prioritized`.
3. Move task and linked requirement to `in-progress` before coding.
4. Implement, test, and update docs in same change set.
5. Mark task and requirement `done` when acceptance criteria are met.

## Command Cheat Sheet

```bash
# Status and queue
cargo run -p orchestrator-cli -- project active
cargo run -p orchestrator-cli -- requirements list
cargo run -p orchestrator-cli -- task prioritized
cargo run -p orchestrator-cli -- task next

# Requirement lifecycle
cargo run -p orchestrator-cli -- requirements create --title "..." --priority should
cargo run -p orchestrator-cli -- requirements update --id REQ-XYZ --status in-progress
cargo run -p orchestrator-cli -- requirements update --id REQ-XYZ --status done

# Task lifecycle
cargo run -p orchestrator-cli -- task create --title "..."
cargo run -p orchestrator-cli -- task status --id TASK-XYZ --status in-progress
cargo run -p orchestrator-cli -- task status --id TASK-XYZ --status done
```

## Definition of Done

- Linked task status is `done`.
- Linked requirement status is `done`.
- `cargo check` passes for affected crates.
- User-facing behavior or docs are updated when command surfaces change.

## Current Seed Backlog

- `REQ-001` / `TASK-001`: self-hosting docs and conventions
- `REQ-002` / `TASK-002`: CLI UX/help/error polish
- `REQ-003` / `TASK-003`: runner lifecycle hardening
- `REQ-004` / `TASK-004`: wrapper and MCP integration
- `REQ-005` / `TASK-005`: CI coverage
- `REQ-006` / `TASK-006`: release packaging
