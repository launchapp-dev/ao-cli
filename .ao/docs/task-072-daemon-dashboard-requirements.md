# TASK-072: Daemon Status Dashboard Pane - Requirements

## Task Overview

Add sidebar pane to the TUI showing daemon health, active agents, task queue breakdown, and recent errors. Include keyboard shortcuts for daemon control.

## Implementation Scope

### Integration Approach

**Decision**: Create a new `handle_daemon_monitor` TUI (similar to `handle_workflow_monitor`) rather than modifying the existing Agent Console TUI. This provides:

- Cleaner separation of concerns
- Dedicated full-screen dashboard for daemon monitoring
- No disruption to existing Agent Console functionality
- Easier maintenance and testing

### Pane Layout (Daemon Monitor TUI)

```
┌─────────────────────────────────────────────────────┐
│ DAEMON STATUS DASHBOARD                             │
├─────────────────────────────────────────────────────┤
│ Daemon Health          │ Agent Activity             │
│ ─────────────────     │ ──────────────              │
│ Status: Running        │ Active: 2/5               │
│ Runner: Connected      │ Agents:                     │
│ PID: 12345            │   - TASK-072 (impl)        │
│                        │   - TASK-075 (qa)          │
├────────────────────────┼────────────────────────────┤
│ Task Queue            │ Recent Errors              │
│ ──────────            │ ─────────────              │
│ Ready: 12             │ [timestamp] ERROR msg      │
│ In Progress: 3         │ [timestamp] ERROR msg      │
│ Blocked: 2            │ [timestamp] WARNING msg    │
│ On Hold: 1            │                            │
└────────────────────────┴────────────────────────────┘
Controls: d=toggle daemon  p=pause/resume  r=refresh  q=quit
```

### Data Sources (via hub.daemon() and hub.tasks())

| Display Item | Source | Query Method |
|--------------|--------|--------------|
| Daemon Status | DaemonHealth.status | hub.daemon().health() |
| Runner Connected | DaemonHealth.runner_connected | hub.daemon().health() |
| Runner PID | DaemonHealth.runner_pid | hub.daemon().health() |
| Daemon PID | DaemonHealth.daemon_pid | hub.daemon().health() |
| Active Agents | DaemonHealth.active_agents | hub.daemon().health() |
| Max Agents | DaemonHealth.max_agents | hub.daemon().health() |
| Task Queue Stats | TaskStatistics | hub.tasks().statistics() |
| Recent Errors | errors.json | hub.errors().list() |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| d | Toggle daemon start/stop |
| p | Toggle daemon pause/resume |
| r | Force refresh |
| q | Quit |

### Acceptance Criteria

1. **Daemon Health Display**
   - Shows current daemon status: Running/Paused/Stopped/Starting/Stopping/Crashed
   - Shows runner connection status
   - Shows daemon PID and runner PID when available

2. **Active Agents Display**
   - Shows count (e.g., "2/5")
   - Lists active task IDs with current phase

3. **Task Queue Breakdown**
   - Ready count
   - In Progress count  
   - Blocked count
   - On Hold count

4. **Recent Errors**
   - Shows last 3-5 errors from error log
   - Displays timestamp and error message

5. **Interactive Controls**
   - 'd' key toggles daemon start/stop (async)
   - 'p' key toggles pause/resume scheduler
   - Visual feedback on control actions

6. **Auto-Refresh**
   - Refresh interval: 2-5 seconds (configurable)
   - Manual refresh via 'r' key

### Constraints

- Must use existing `hub.daemon()` and `hub.tasks()` service APIs
- Must handle daemon unavailability gracefully (show "unavailable" state)
- Must not block event loop during refresh (async queries)
- Must follow existing TUI patterns in `services/tui/`

### Implementation Files (Anticipated)

1. `crates/orchestrator-cli/src/services/tui/daemon_monitor/mod.rs` - Entry point
2. `crates/orchestrator-cli/src/services/tui/daemon_monitor/render.rs` - Rendering
3. `crates/orchestrator-cli/src/services/tui/daemon_monitor/state.rs` - State management

### CLI Integration

Add new subcommand:
```bash
ao tui daemon-monitor [--refresh-interval SECS]
```

### Risk Assessment

- **Low Risk**: Uses existing service APIs, follows established TUI patterns
- **Medium**: Daemon control actions require proper error handling for edge cases
