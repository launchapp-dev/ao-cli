# Feature Status

This page clarifies which Animus features are **shipped and stable**, **in-flight (partially implemented or experimental)**, or **target architecture** (future aspirations).

## Feature Status Legend

- **Shipped** — Stable on main, documented, and ready for production use
- **In-Flight** — Partially implemented, experimental, or in active development; may change
- **Planned** — Target architecture aspirations; not yet implemented

## Core Features

### Work Management

| Feature | Status | Description |
|---------|--------|-------------|
| Task CRUD | **Shipped** | Create, read, update, delete, list, and filter tasks with full lifecycle support |
| Task Dependencies | **Shipped** | Define task precedence edges and enforce execution order |
| Task Status Lifecycle | **Shipped** | Progress tasks through backlog → todo → ready → in_progress → done/cancelled states |
| Task Prioritization | **Shipped** | Set and rebalance task priority with budget policies |
| Task Blockers | **Shipped** | Mark tasks as blocked with reasons and automatic unblock detection |
| Requirements as First-Class | **Shipped** | Define requirements as `kind=requirement` subjects and link workflow execution through the unified subject surface |

### Workflows and Execution

| Feature | Status | Description |
|---------|--------|-------------|
| Workflow Execution Engine | **Shipped** | Execute multi-phase workflows with phase rework and decision gates |
| Workflow YAML Overlays | **Shipped** | Project-local `.animus/workflows.yaml` for custom workflow definitions |
| Workflow Packs | **In-Flight** | Bundled workflow libraries and versioned pack resolution; pack discovery functional, some features experimental |
| Phase Execution | **Shipped** | Run phases sequentially with timeout and error recovery |
| Phase Gates (Manual Approval) | **Shipped** | Workflow phases can require manual approval before advancing |
| Agent Rework Loops | **Shipped** | Phases can rework (re-execute) with improved prompts after failures |
| Workflow Checkpoints | **Shipped** | Save and restore workflow state at phase boundaries |
| Subject Dispatch (Universal Work Envelope) | **Shipped** | All work flows through a unified dispatch model (task, requirement, or custom subject) |

### Daemon and Scheduling

| Feature | Status | Description |
|---------|--------|-------------|
| Daemon Lifecycle | **Shipped** | Start, stop, pause, resume daemon scheduling process |
| Queue Management | **Shipped** | Enqueue, hold, release, drop, and reorder work dispatches |
| Capacity Limits | **Shipped** | Control concurrent agent runners and workflow spawn rate |
| Queue Statistics | **Shipped** | Inspect queue depth and per-status counts |
| Autonomous Scheduling | **Shipped** | Daemon auto-selects ready work and spawns workflows |
| Stale Item Detection | **Shipped** | Identify tasks and workflows with no recent state updates |

### CLI Command Surface

| Feature | Status | Description |
|---------|--------|-------------|
| Status Dashboard (`animus status`) | **Shipped** | Unified work dashboard showing subject, workflow, daemon, and queue state |
| Subject Commands | **Shipped** | Unified `animus subject` family for task, requirement, and external subject backends |
| Workflow Commands | **Shipped** | Workflow execution, status, checkpoints, and phase management |
| Daemon Commands | **Shipped** | Daemon lifecycle, health, queue, and event inspection |
| Git Integration (`animus git`) | **Shipped** | Worktree creation, branch management, push/pull, confirmation requests |
| MCP Integration (`animus mcp serve`) | **Shipped** | Expose Animus state and operations as MCP tools for use by AI agents |
| Skill Management (`animus skill`) | **Shipped** | Search, install, update, and publish versioned skills |
| Model Management (`animus model`) | **Shipped** | Check model availability, validate model selection, view model roster |
| History Inspection | **Shipped** | Inspect execution history through `animus history ...` |
| Template-Driven Project Init | **Shipped** | `animus init` supports registry-backed and local copy templates plus daemon defaults. Bundled templates (`task-queue`, `conductor`, `direct-workflow`) and the `--template` / `--path` / `--non-interactive` flags are stable. Registry pinning, template authoring helpers, and richer multi-registry management are planned |

### Observability and Output

| Feature | Status | Description |
|---------|--------|-------------|
| JSON Output Envelopes (`--json` flag) | **Shipped** | Machine-readable `animus.cli.v1` JSON for all commands |
| Run Output Inspection | **Shipped** | Read agent run logs, artifacts, and JSONL event streams |
| Daemon Health and Status | **Shipped** | Real-time daemon process health and scheduling diagnostics |
| Workflow Decisions | **Shipped** | View automated and manual decisions made during workflow execution |
| Event Streaming | **Shipped** | Stream structured log events in real-time from daemon and runners |

## Web and TUI Surfaces

| Feature | Status | Description |
|---------|--------|-------------|
| Web UI (`animus web serve`) | **Shipped** | React-based web dashboard for task, workflow, and requirement management |
| Web UI — Task Dashboard | **Shipped** | View, filter, search, and manage tasks from the web UI |
| Web UI — Workflow Monitoring | **Shipped** | Monitor active and completed workflows with phase details |
| Web UI — Dark Mode | **Shipped** | Built-in dark mode theme support |
| TUI Dashboard (`animus tui`) | **In-Flight** | Terminal UI for Animus; under active development |
| Mobile-Friendly Responsiveness | **In-Flight** | Web UI responsive behavior is experimental; primary experience is desktop |

## Data Persistence and Configuration

| Feature | Status | Description |
|---------|--------|-------------|
| Project-Local `.animus/` Config | **Shipped** | Store repo-scoped workflow YAML and daemon settings under `.animus/` |
| Scoped Runtime State (`~/.animus/<repo-scope>/`) | **Shipped** | Per-repo runtime state isolation with automatic cleanup |
| JSON State Files | **Shipped** | All Animus state is tool-managed JSON (tasks, workflows, requirements, runs) |
| Git Worktree Isolation | **Shipped** | Every task gets its own git worktree for safe parallel execution |
| Worktree Lifecycle | **Shipped** | Automatic creation, pull/push synchronization, and cleanup |

## Agent Integration and Automation

| Feature | Status | Description |
|---------|--------|-------------|
| MCP Tool Exposure | **Shipped** | All Animus operations available as typed MCP tools for agent use |
| Agent Runtime Configuration | **Shipped** | Configure model, temperature, tool selection per workflow |
| Built-in Workflows | **Shipped** | Standard phases (requirements, implementation, testing, QA, deployment) |
| Workflow Prompt Rendering | **Shipped** | Render workflow phase prompts with context and section injection |
| Multi-Model Support | **Shipped** | Configure different models for different agents and phases |

## Target Architecture (Not Yet Shipped)

The following represent aspirational architectural goals and roadmap items:

| Feature | Status | Notes |
|---------|--------|-------|
| Full Autonomous Planning Loop | **Planned** | Long-term vision of agents autonomously defining requirements, planning tasks, and executing workflows without human intervention |
| Advanced Multi-Agent Orchestration | **Planned** | Coordinate work across multiple specialized agents with dynamic role assignment |
| Agent Persistence and Continuity | **Planned** | Agents maintaining long-lived state across multiple workflows |
| Production Deployment Integrations | **Planned** | Deep integration with Kubernetes, cloud providers, and production infrastructure |
| Real-Time Collaboration | **Planned** | Multi-user simultaneous access to Animus projects |
| Advanced Performance Optimization | **Planned** | Further optimization of queue scheduling, workflow parallelization, and resource utilization |

## Known Limitations

- **Web UI on Mobile**: The web UI is optimized for desktop; mobile experience is functional but not fully responsive
- **TUI**: Terminal UI is in-flight; some features may be missing or unstable
- **Pack Resolution**: Workflow pack discovery and installation is functional but may evolve before stabilizing
- **Performance at Scale**: Very large task backlogs (10k+) may show performance degradation; optimization is on the roadmap

## Stable JSON Contracts

The following JSON schemas are considered stable and will be maintained for backward compatibility:

- `animus.cli.v1` — CLI output envelope
- `animus.status.v1` — Status dashboard schema
- Workflow and subject-related state schemas

When contracts change, version numbers will increment and old/new versions will
briefly overlap to allow client migration.
