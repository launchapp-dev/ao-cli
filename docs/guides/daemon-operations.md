# Daemon Operations Guide

The AO daemon is the autonomous scheduler that picks up tasks, dispatches workflows, and manages agent execution. It runs in the background and continuously processes work according to your workflow configuration.

## Starting the Daemon

### Background Mode (Autonomous)

Start the daemon as a detached background process:

```bash
animus daemon start --autonomous
```

This forks a child process and redirects stderr to `.ao/daemon.log`. The daemon will continuously poll for ready tasks and dispatch workflows.

### Foreground Mode

Run the daemon in the foreground for debugging:

```bash
animus daemon run
```

Output streams directly to your terminal. Use Ctrl+C to stop.

## Stopping the Daemon

Graceful shutdown with drain (waits for in-progress phases to complete):

```bash
animus daemon stop
```

## Pausing and Resuming

Pause the scheduler without stopping the daemon process. In-progress work continues but no new work is picked up:

```bash
animus daemon pause
```

Resume scheduling:

```bash
animus daemon resume
```

## Configuration

View and update daemon automation settings:

```bash
animus daemon config
```

Key configuration options:

| Setting | Description |
|---------|-------------|
| `auto_merge` | Automatically merge PRs after successful workflow completion |
| `auto_pr` | Automatically create PRs for completed work |
| `pool_size` | Maximum number of concurrent agents the daemon will run |
| `active_hours` | Time window during which schedule-driven workflow dispatch is allowed |
| `auto_run_ready` | Whether ready tasks are promoted during daemon ticks |

`active_hours` only gates schedule-driven dispatch. Ready-task pickup is controlled separately by `auto_run_ready`.

Update a specific setting:

```bash
animus daemon config --pool-size 3
animus daemon config --auto-merge true
animus daemon config --auto-run-ready false
```

## Monitoring

### Daemon Status

Check whether the daemon is running and its current state:

```bash
animus daemon status
```

### Health Check

Detailed health information including uptime and resource usage:

```bash
animus daemon health
```

### Logs

Read daemon logs:

```bash
animus daemon logs
```

The daemon writes structured JSON log lines to `.ao/daemon.log`. Log rotation occurs at 10MB (rotated file: `.ao/daemon.log.1`).

Clear logs when they grow too large:

```bash
animus daemon clear-logs
```

### Events

Stream the event history to see what the daemon has been doing:

```bash
animus daemon events
```

### Agent Visibility

List agents currently managed by the daemon:

```bash
animus daemon agents
```

## Diagnostics

### Reading Daemon Logs Directly

For real-time debugging, tail the log file:

```bash
tail -f .ao/daemon.log
```

The log contains structured JSON lines with event types like `daemon_startup`, `daemon_shutdown`, workflow dispatches, and phase completions.

### Runner Health

The runner is a separate process from the daemon. It spawns CLI tools (claude, codex, gemini). Check its health:

```bash
animus runner health
```

### Orphan Detection

Detect orphaned runner processes that lost their parent:

```bash
animus runner orphans detect
```

Clean them up:

```bash
animus runner orphans cleanup
```

### Restart Statistics

View how often the runner has restarted:

```bash
animus runner restart-stats
```

## Common Patterns

### Start Daemon and Monitor

```bash
animus daemon start --autonomous
animus daemon status
animus daemon events
```

### Pause While Making Manual Changes

```bash
animus daemon pause
# Make your changes...
animus daemon resume
```

### Debug a Stuck Workflow

```bash
animus daemon status           # Check daemon state
animus daemon logs             # Look for errors
animus runner health           # Check runner process
animus workflow list            # Find the stuck workflow
animus workflow get --id WF-001 # Inspect workflow state
```
