# Task Management Guide

Tasks are the primary unit of work in Animus. Each task tracks a discrete piece of work from creation through completion, with support for priorities, dependencies, checklists, and agent assignment.

## Creating Tasks

```bash
animus task create --title "Add retry logic to HTTP client" --task-type feature --priority high
```

Available task types:

| Type | Use case |
|------|----------|
| `feature` | New functionality |
| `bugfix` | Fix for a known defect |
| `hotfix` | Urgent production fix |
| `refactor` | Code restructuring without behavior change |
| `docs` | Documentation updates |
| `test` | Test coverage additions |
| `chore` | Maintenance, dependency bumps, CI tweaks |
| `experiment` | Exploratory or spike work |

You can also supply a description inline:

```bash
animus task create \
  --title "Retry HTTP 429 responses" \
  --task-type feature \
  --priority high \
  --description "Implement exponential backoff for rate-limited responses in the HTTP client module."
```

## Task Status Flow

Tasks move through a defined set of statuses:

```
Backlog --> Ready --> In-Progress --> Done
                  \              \
                   \--> Blocked   \--> Cancelled
                   \--> On-Hold
```

Change status with `animus task status`:

```bash
animus task status --id TASK-001 --status ready
animus task status --id TASK-001 --status in-progress
animus task status --id TASK-001 --status done
```

To unblock a task, set it back to `ready`:

```bash
animus task status --id TASK-001 --status ready
```

## Assigning Tasks

Assign a task to an agent with a specific model:

```bash
animus task assign --id TASK-001 --assignee agent:claude --type agent --model claude-sonnet-4-6
```

Or assign to a human:

```bash
animus task assign --id TASK-001 --type human --assignee "alice"
```

## Priority Management

Set priority directly:

```bash
animus task set-priority --id TASK-001 --priority critical
```

Priority levels: `critical`, `high`, `medium`, `low`.

Rebalance priorities across multiple tasks by budget:

```bash
animus task rebalance-priority
```

## Dependencies

Add a dependency so one task blocks another:

```bash
animus task dependency-add --id TASK-002 --dependency-id TASK-001 --type blocks
```

When TASK-001 is not yet done, TASK-002 cannot move to `in-progress`. The daemon respects dependency ordering when picking the next task to execute.

Remove a dependency:

```bash
animus task dependency-remove --id TASK-002 --dependency-id TASK-001
```

## Checklists

Add checklist items to a task for granular tracking:

```bash
animus task checklist-add --id TASK-001 --description "Implement retry logic"
animus task checklist-add --id TASK-001 --description "Add unit tests for backoff"
animus task checklist-add --id TASK-001 --description "Update API docs"
```

Toggle a checklist item as complete:

```bash
animus task checklist-update --id TASK-001 --item-id chk-1 --completed true
```

Agents use checklists during PO review and rework phases to verify acceptance criteria.

## Querying Tasks

List tasks with filters:

```bash
animus task list                             # All tasks
animus task list --status in-progress        # Only in-progress tasks
animus task list --task-type feature         # Only features
animus task list --priority high             # Only high-priority
```

View tasks sorted by priority:

```bash
animus task list --sort priority
```

Get the next task the daemon would pick:

```bash
animus task next
```

View task statistics:

```bash
animus task stats
```

Get a single task by ID:

```bash
animus task get --id TASK-001
```

All commands support `--json` for machine-readable output:

```bash
animus task list --status ready --json
```

## Task History

View workflow dispatch history for a task:

```bash
animus task history --id TASK-001
```

## Pausing and Cancelling

Pause a task (prevents daemon from scheduling it):

```bash
animus task pause --id TASK-001
```

Resume a paused task:

```bash
animus task resume --id TASK-001
```

Cancel a task (requires confirmation):

```bash
animus task cancel --id TASK-001 --confirm TASK-001
```

## Deadlines

Set a deadline:

```bash
animus task set-deadline --id TASK-001 --deadline "2026-03-15T09:30:00Z"
```

Clear a deadline:

```bash
animus task set-deadline --id TASK-001
```
