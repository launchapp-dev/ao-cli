# Self-Hosting Workflow

AO is built using AO. The project's own requirements and tasks are tracked through the same `ao` commands that users run on their projects. This guide documents that workflow.

## The "AO Builds AO" Loop

The development cycle follows this pattern:

1. Requirements are drafted and refined using `ao requirements`
2. Tasks are created from requirements using `animus requirements execute --id REQ-001`
3. The daemon picks up tasks and dispatches workflows
4. Agents implement, test, and review changes
5. Completed work is merged back into the codebase

## Viewing the Backlog

List all requirements:

```bash
animus requirements list
```

View prioritized tasks:

```bash
animus task list --sort priority
```

Check task statistics for overall progress:

```bash
animus task stats
```

## Working on a Task

Get the next highest-priority ready task:

```bash
animus task next
```

Start work on it:

```bash
animus task status --id TASK-XXX --status in-progress
```

Complete the task:

```bash
animus task status --id TASK-XXX --status done
```

## Autonomous Execution

For fully autonomous operation, start the daemon:

```bash
animus daemon start --autonomous
```

The daemon will:

1. Poll for ready tasks
2. Respect dependency ordering
3. Create git worktrees for each task
4. Dispatch the configured workflow pipeline
5. Run through phases: requirements, implementation, testing, review
6. Create PRs and optionally auto-merge on success
7. Clean up worktrees after merge

Monitor progress:

```bash
animus daemon status
animus daemon events
animus task stats
```

## Task State Management

When the daemon encounters issues, tasks may end up in a blocked state. Always use `animus task status` to reset:

```bash
animus task status --id TASK-XXX --status ready
```

This clears all blocking metadata (`paused`, `blocked_at`, `blocked_reason`, `blocked_by`). Never edit task JSON files in `.ao/` directly.

## Environment Setup

When running the daemon from inside a Claude Code session, the `CLAUDECODE` environment variable is inherited. This prevents the `claude` CLI from starting. Unset it before launching:

```bash
env -u CLAUDECODE animus daemon start --autonomous
```

Or:

```bash
unset CLAUDECODE
animus daemon start --autonomous
```
