# Working with AO via MCP Tools — Agent Guide

This guide explains how AI agents (and MCP clients) interact with the AO orchestrator through its MCP tool surface. Every tool maps 1:1 to an `ao` CLI command and accepts JSON input.

For the full tool table with parameters, see [MCP Tools Reference](../reference/mcp-tools.md).

---

## Overview

AO exposes 73 MCP tools organized into 8 groups:

| Group | Tools | Purpose |
|-------|-------|---------|
| `animus.task.*` | 20 | Task lifecycle management |
| `animus.workflow.*` | 16 | Workflow execution and control |
| `animus.daemon.*` | 11 | Background scheduler management |
| `animus.requirements.*` | 6 | Requirements tracking |
| `animus.queue.*` | 7 | Dispatch queue management |
| `animus.output.*` | 6 | Agent output and monitoring |
| `animus.agent.*` | 3 | Direct agent execution |
| `animus.runner.*` | 4 | Runner process health |

Every tool accepts an optional `project_root` parameter to specify which project to operate on. If omitted, the current working directory is used.

---

## Task Management (`animus.task.*`)

Tasks are the primary unit of work. Each task has an ID (e.g., `TASK-001`), title, description, status, priority, and optional metadata like checklists, dependencies, and deadlines.

### Creating Tasks

```json
// animus.task.create
{
  "title": "Add retry logic to HTTP client",
  "description": "Implement exponential backoff for rate-limited responses",
  "priority": "high",
  "task_type": "feature",
  "linked_requirement": ["REQ-001"]
}
```

### Querying Tasks

```json
// animus.task.list — filter by status, priority, type, tags, search text
{ "status": "in-progress", "priority": "high", "limit": 10 }

// animus.task.get — full details for a single task
{ "id": "TASK-001" }

// animus.task.prioritized — tasks sorted by priority, respecting dependencies
{ "limit": 10 }

// animus.task.next — the single highest-priority ready task
{}

// animus.task.stats — aggregate counts by status, priority, type
{}

// animus.task.history — workflow dispatch history for a task
{ "id": "TASK-001" }
```

### Updating Tasks

```json
// animus.task.status — change task status
// Valid: backlog, todo, ready, in_progress, blocked, on_hold, done, cancelled
{ "id": "TASK-001", "status": "in-progress" }

// animus.task.update — update any fields
{ "id": "TASK-001", "title": "New title", "priority": "critical" }

// animus.task.assign — assign to user or agent
{ "id": "TASK-001", "assignee": "agent:claude", "assignee_type": "agent", "model": "claude-sonnet-4-6" }

// animus.task.set-priority
{ "id": "TASK-001", "priority": "critical" }

// animus.task.set-deadline — set or clear (omit deadline to clear)
{ "id": "TASK-001", "deadline": "2026-03-15T09:30:00Z" }
```

### Checklists

```json
// animus.task.checklist-add
{ "id": "TASK-001", "description": "Write unit tests for backoff" }

// animus.task.checklist-update — use animus.task.get first to find item_id
{ "id": "TASK-001", "item_id": "chk-1", "completed": true }
```

### Pause, Resume, Cancel

```json
// animus.task.pause — prevents daemon from scheduling
{ "id": "TASK-001" }

// animus.task.resume — re-enables scheduling
{ "id": "TASK-001" }

// animus.task.cancel — permanently cancel
{ "id": "TASK-001", "confirm": "yes" }
```

### Bulk Operations

```json
// animus.task.bulk-status — batch status updates
{
  "updates": [
    { "id": "TASK-001", "status": "done" },
    { "id": "TASK-002", "status": "ready" }
  ],
  "on_error": "continue"
}

// animus.task.bulk-update — batch field updates
{
  "updates": [
    { "id": "TASK-001", "priority": "high" },
    { "id": "TASK-002", "assignee": "agent:claude" }
  ],
  "on_error": "stop"
}
```

---

## Workflow Engine (`animus.workflow.*`)

Workflows orchestrate multi-phase execution of tasks. A workflow runs phases (e.g., research → implementation → review) sequentially, with each phase executing an AI agent.

### Running Workflows

```json
// animus.workflow.run — async via daemon (returns immediately)
{ "task_id": "TASK-001" }

// animus.workflow.run — with specific workflow definition
{ "task_id": "TASK-001", "workflow_ref": "default" }

// animus.workflow.execute — synchronous (blocks until complete, no daemon needed)
{ "task_id": "TASK-001" }

// animus.workflow.execute — run a specific phase only
{ "task_id": "TASK-001", "phase": "implementation", "model": "claude-sonnet-4-6" }

// animus.workflow.run-multiple — batch workflow dispatch
{
  "runs": [
    { "task_id": "TASK-001" },
    { "task_id": "TASK-002", "workflow_ref": "quick" }
  ],
  "on_error": "continue"
}
```

### Monitoring Workflows

```json
// animus.workflow.get — full workflow state
{ "id": "wf-abc123" }

// animus.workflow.list — list all workflow executions
{ "limit": 10 }

// animus.workflow.decisions — decision log during execution
{ "id": "wf-abc123" }

// animus.workflow.checkpoints.list — saved state checkpoints
{ "id": "wf-abc123" }
```

### Controlling Workflows

```json
// animus.workflow.pause
{ "id": "wf-abc123" }

// animus.workflow.resume
{ "id": "wf-abc123" }

// animus.workflow.cancel — permanently stop
{ "id": "wf-abc123", "confirm": "yes" }
```

### Phase & Definition Inspection

```json
// animus.workflow.phases.list — all available phase definitions
{}

// animus.workflow.phases.get — details of a specific phase
{ "phase": "implementation" }

// animus.workflow.definitions.list — all workflow definitions
{}

// animus.workflow.config.get — effective workflow configuration
{}

// animus.workflow.config.validate — check config for errors
{}

// animus.workflow.phase.approve — approve a gated phase
{ "workflow_id": "wf-abc123", "phase_id": "review", "feedback": "Looks good" }
```

---

## Daemon Management (`animus.daemon.*`)

The daemon is the background scheduler that picks up ready tasks, dispatches workflows, manages agents, and handles auto-merge/auto-PR.

### Lifecycle

```json
// animus.daemon.start — start the daemon
{}

// animus.daemon.start — with options
{
  "autonomous": true,
  "interval_secs": 5,
  "pool_size": 3,
  "auto_run_ready": true,
  "phase_timeout_secs": 1800
}

// animus.daemon.stop — graceful shutdown
{}

// animus.daemon.pause — stop picking up new work (in-progress continues)
{}

// animus.daemon.resume — resume scheduling
{}
```

### Monitoring

```json
// animus.daemon.status — is it running?
{}

// animus.daemon.health — detailed metrics (uptime, agents, capacity)
{}

// animus.daemon.agents — list currently running agents
{}

// animus.daemon.events — recent event history
{}

// animus.daemon.logs — read log file
{ "limit": 100 }

// animus.daemon.logs — search for errors
{ "search": "error" }
```

### Configuration

```json
// animus.daemon.config — read current settings
{}

// animus.daemon.config-set — update settings
{
  "auto_merge": true,
  "auto_pr": true,
  "auto_commit_before_merge": true,
  "auto_prune_worktrees_after_merge": true,
  "auto_run_ready": true,
  "notification_config_file": ".animus/notification-config.json"
}
```

---

## Agent Execution (`animus.agent.*`)

Agents are AI CLI tool processes (claude, codex, gemini) managed by the runner. You can run agents directly without the daemon.

```json
// animus.agent.run — launch an agent
{
  "tool": "claude",
  "model": "claude-sonnet-4-6",
  "prompt": "Fix the failing test in src/lib.rs",
  "detach": true
}

// animus.agent.status — check if agent is running/completed/failed
{ "run_id": "abc123" }

// animus.agent.control — pause, resume, or terminate
{ "run_id": "abc123", "action": "terminate" }
```

---

## Output & Monitoring (`animus.output.*`)

View what agents have produced during execution.

```json
// animus.output.run — full stdout/stderr from a run
{ "run_id": "abc123" }

// animus.output.tail — recent events (quick check on progress)
{ "run_id": "abc123", "limit": 50 }

// animus.output.tail — filter by event type
{ "task_id": "TASK-001", "event_types": ["stdout", "stderr"] }

// animus.output.monitor — stream live output
{ "run_id": "abc123" }

// animus.output.monitor — scope to a task and phase
{ "run_id": "abc123", "task_id": "TASK-001", "phase_id": "implementation" }

// animus.output.jsonl — structured event log
{ "run_id": "abc123", "entries": true }

// animus.output.artifacts — files generated during execution
{ "execution_id": "exec-abc123" }

// animus.output.phase-outputs — persisted workflow phase outputs
{ "workflow_id": "wf-abc123" }

// animus.output.phase-outputs — with specific phase
{ "workflow_id": "wf-abc123", "phase_id": "implementation" }
```

---

## Requirements (`animus.requirements.*`)

Requirements are high-level objectives that tasks are derived from.

```json
// animus.requirements.list
{ "limit": 20 }

// animus.requirements.get
{ "id": "REQ-001" }

// animus.requirements.create
{
  "title": "HTTP client resilience",
  "description": "All HTTP calls should handle transient failures gracefully",
  "priority": "high"
}

// animus.requirements.update
{ "id": "REQ-001", "status": "accepted" }

// animus.requirements.delete
{ "id": "REQ-001" }

// animus.requirements.refine — improve requirements, optionally with AI
{ "id": ["REQ-001"], "focus": "tighten acceptance criteria", "use_ai": true }
```

---

## Queue Management (`animus.queue.*`)

The dispatch queue controls the order in which the daemon picks up work.

```json
// animus.queue.list — view queued dispatches
{}

// animus.queue.stats — aggregate depth and status counts
{}

// animus.queue.enqueue — manually add to queue
{ "task_id": "TASK-001" }

// animus.queue.hold — prevent dispatch without removing
{ "subject_id": "TASK-001" }

// animus.queue.release — resume dispatch eligibility
{ "subject_id": "TASK-001" }

// animus.queue.drop — remove a queued subject entirely
{ "subject_id": "TASK-004" }

// animus.queue.reorder — set preferred dispatch order
{ "subject_ids": ["TASK-003", "TASK-001", "TASK-002"] }
```

---

## Runner Health (`animus.runner.*`)

The runner is a separate process that spawns CLI tools. It's managed by the daemon but can be checked independently.

```json
// animus.runner.health — is the runner up and has capacity?
{}

// animus.runner.orphans-detect — find leaked processes
{}

// animus.runner.orphans-cleanup — clean up leaked processes by run id
{ "run_id": ["run-123", "run-456"] }

// animus.runner.restart-stats — uptime and restart history
{}
```

---

## Common Workflows

### 1. Start fresh: create a task and run it

```
animus.task.create        → { "title": "...", "priority": "high" }
animus.task.status        → { "id": "TASK-XXX", "status": "ready" }
animus.workflow.execute   → { "task_id": "TASK-XXX" }
```

### 2. Let the daemon handle everything

```
animus.daemon.start       → { "autonomous": true, "auto_run_ready": true }
animus.task.create        → { "title": "...", "priority": "high" }
animus.task.status        → { "id": "TASK-XXX", "status": "ready" }
                        (daemon picks it up automatically)
animus.daemon.agents      → {} (check what's running)
animus.output.tail        → { "task_id": "TASK-XXX" }
```

### 3. Monitor and debug

```
animus.daemon.status      → {} (is it running?)
animus.daemon.health      → {} (capacity and metrics)
animus.daemon.logs        → { "search": "error" }
animus.runner.health      → {} (runner process ok?)
animus.runner.orphans-detect → {} (leaked processes?)
```

### 4. Batch process multiple tasks

```
animus.task.bulk-status   → { "updates": [{"id":"TASK-001","status":"ready"}, ...] }
animus.workflow.run-multiple → { "runs": [{"task_id":"TASK-001"}, {"task_id":"TASK-002"}] }
```

### 5. Requirements-driven planning

```
animus.requirements.create → { "title": "...", "description": "..." }
animus.requirements.refine → { "id": ["REQ-001"], "use_ai": true }
animus.task.create         → { "title": "...", "linked_requirement": ["REQ-001"] }
```

### 6. Queue management

```
animus.queue.list          → {} (see what's queued)
animus.queue.hold          → { "subject_id": "TASK-003" } (hold back a task)
animus.queue.reorder       → { "subject_ids": ["TASK-001", "TASK-002"] } (prioritize)
animus.queue.release       → { "subject_id": "TASK-003" } (let it dispatch)
```

---

## Pagination

All list tools support:

| Parameter | Type | Default | Max | Description |
|-----------|------|---------|-----|-------------|
| `limit` | integer | 25 | 200 | Items per page |
| `offset` | integer | 0 | — | Items to skip |
| `max_tokens` | integer | 3000 | 12000 | Token budget for response compaction |

Responses use the `animus.mcp.list.result.v1` envelope with pagination metadata.

## Batch Operations

Batch tools (`animus.task.bulk-status`, `animus.task.bulk-update`, `animus.workflow.run-multiple`) share:

| Parameter | Values | Description |
|-----------|--------|-------------|
| `on_error` | `"stop"` (default), `"continue"` | Whether to halt or proceed on failure |

Max 100 items per batch call. Responses use `animus.mcp.batch.result.v1` with per-item results.

---

## Tool Sequencing Tips

Many tools are designed to be used in sequence. The tool descriptions include `Sequencing:` hints:

- **Before creating**: `animus.task.list` or `animus.requirements.list` to check for duplicates
- **Before updating**: `animus.task.get` to verify current state
- **Before running workflows**: `animus.task.get` to verify the task exists
- **Before starting agents**: `animus.runner.health` to verify capacity
- **After starting daemon**: `animus.daemon.status` or `animus.daemon.health` to verify
- **After running workflows**: `animus.output.tail` or `animus.workflow.get` to monitor

See also: [MCP Tools Reference](../reference/mcp-tools.md), [Task Management](task-management.md), [Daemon Operations](daemon-operations.md), [Writing Workflows](writing-workflows.md).
