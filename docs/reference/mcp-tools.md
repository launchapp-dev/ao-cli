# MCP Tools Reference

All MCP tools exposed by `animus mcp serve`. These tools allow AI agents to interact with the Animus orchestrator over the Model Context Protocol. Each tool wraps an `animus` CLI command, accepting JSON input and returning structured results.

Every tool accepts an optional `project_root` parameter to override the default project root.

**v0.4.0 note — unified subject surface.** Tasks and requirements are also reachable through the
unified `animus subject --kind <kind>` CLI verb (`list`, `get`, `create`, `update`, `next`,
`status`). The in-tree `SubjectBackend` adapters at `kind=task` and `kind=requirement` read and
write the same state the legacy `animus.task.*` / `animus.requirements.*` MCP tools touch. The
legacy MCP tools remain available for v0.4.x; agents are encouraged to migrate to the subject
surface for consistency with externally-installed subject_backend plugins (Linear, Jira, GitHub
Issues, etc.).

---

## Agent Control (10 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.agent.list` | List configured project agent profiles | `project_root` |
| `animus.agent.get` | Get a configured agent profile | `id`, `project_root` |
| `animus.agent.run` | Launch an AI agent to execute work | `tool`, `model`, `prompt`, `cwd`, `timeout_secs`, `context_json`, `runtime_contract_json`, `detach`, `run_id`, `runner_scope`, `project_root` |
| `animus.agent.control` | Control a running agent (pause/resume/terminate) | `run_id`, `action` (`pause`, `resume`, `terminate`), `runner_scope` |
| `animus.agent.status` | Get status of an agent run | `run_id`, `runner_scope` |
| `animus.agent.memory.get` | Read project-scoped agent memory | `agent`, `project_root` |
| `animus.agent.memory.append` | Append project-scoped agent memory | `agent`, `text`, `source`, `project_root` |
| `animus.agent.memory.clear` | Clear project-scoped agent memory | `agent`, `project_root` |
| `animus.agent.message.send` | Send a message on a configured agent channel | `channel`, `from`, `to`, `text`, `workflow_id`, `phase_id`, `project_root` |
| `animus.agent.message.list` | List project-scoped agent messages | `channel`, `agent`, `limit`, `project_root` |

---

## Daemon Management (11 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.daemon.start` | Start the Animus daemon for task scheduling and agent management | `pool_size` (alias: `max_agents`), `interval_secs`, `auto_run_ready`, `auto_merge`, `auto_pr`, `auto_commit_before_merge`, `auto_prune_worktrees_after_merge`, `startup_cleanup`, `resume_interrupted`, `reconcile_stale`, `stale_threshold_hours`, `max_tasks_per_tick`, `phase_timeout_secs`, `idle_timeout_secs`, `skip_runner`, `autonomous`, `runner_scope`, `project_root` |
| `animus.daemon.stop` | Stop the daemon gracefully | `project_root` |
| `animus.daemon.status` | Check if daemon is running and view basic state | `project_root` |
| `animus.daemon.health` | Get detailed health metrics (active agents, queue, capacity) | `project_root` |
| `animus.daemon.pause` | Pause the scheduler without stopping the daemon | `project_root` |
| `animus.daemon.resume` | Resume the scheduler after a pause | `project_root` |
| `animus.daemon.events` | List recent daemon events for debugging and monitoring | `limit`, `project_root` |
| `animus.daemon.agents` | List currently running agent tasks and their status | `project_root` |
| `animus.daemon.logs` | Read daemon process log file | `limit`, `search`, `project_root` |
| `animus.daemon.config` | Read current daemon automation settings | `project_root` |
| `animus.daemon.config-set` | Update daemon automation settings and notification config | `auto_merge`, `auto_pr`, `auto_commit_before_merge`, `auto_prune_worktrees_after_merge`, `auto_run_ready`, `pool_size` (alias: `max_agents`), `interval_secs`, `max_tasks_per_tick`, `stale_threshold_hours`, `phase_timeout_secs`, `idle_timeout_secs`, `notification_config_json`, `notification_config_file`, `clear_notification_config`, `project_root` |

---

## Task Operations (20 tools)

### Query Tools (6)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.task.list` | List tasks with filters | `status`, `priority`, `task_type`, `assignee_type`, `tag[]`, `risk`, `linked_requirement`, `linked_architecture_entity`, `search`, `limit`, `offset`, `max_tokens` |
| `animus.task.get` | Fetch full task details by ID | `id` |
| `animus.task.prioritized` | List tasks sorted by priority | `status`, `priority`, `assignee_type`, `search`, `limit`, `offset`, `max_tokens` |
| `animus.task.next` | Get the single highest priority ready task | `project_root` |
| `animus.task.stats` | Aggregate task metrics by status, priority, type | `project_root` |
| `animus.task.history` | View workflow dispatch history for a task | `id` |

### Mutation Tools (14)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.task.create` | Create a new task | `title`, `description`, `priority`, `task_type`, `linked_requirement[]`, `linked_architecture_entity[]`, `project_root` |
| `animus.task.update` | Update task fields | `id`, `title`, `description`, `priority`, `status`, `assignee`, `linked_architecture_entity[]`, `replace_linked_architecture_entities`, `input_json` |
| `animus.task.delete` | Delete a task (destructive) | `id`, `confirm`, `dry_run` |
| `animus.task.status` | Update task status | `id`, `status` |
| `animus.task.assign` | Assign task to user or agent | `id`, `assignee`, `assignee_type`, `agent_role`, `model` |
| `animus.task.pause` | Pause a running task | `id` |
| `animus.task.resume` | Resume a paused task | `id` |
| `animus.task.cancel` | Cancel a task | `id`, `confirm`, `dry_run` |
| `animus.task.set-priority` | Set task priority | `id`, `priority` |
| `animus.task.set-deadline` | Set or clear task deadline | `id`, `deadline` |
| `animus.task.checklist-add` | Add a checklist item to a task | `id`, `description` |
| `animus.task.checklist-update` | Toggle checklist item completion | `id`, `item_id`, `completed` |
| `animus.task.bulk-status` | Batch-update status for multiple tasks | `updates[]` (each: `id`, `status`), `on_error` |
| `animus.task.bulk-update` | Batch-update fields for multiple tasks | `updates[]` (each: `id` + fields), `on_error` |

---

## Workflow Operations (16 tools)

### Runtime Tools (9)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.workflow.run` | Start a workflow for a task (async, via daemon) | `task_id`, `requirement_id`, `title`, `description`, `workflow_ref`, `input_json` |
| `animus.workflow.run-multiple` | Batch-run workflows for multiple tasks | `runs[]` (each: `task_id`, `workflow_ref`, `input_json`), `on_error` |
| `animus.workflow.execute` | Execute a workflow synchronously (no daemon) | `task_id`, `workflow_ref`, `phase`, `model`, `tool`, `phase_timeout_secs`, `input_json` |
| `animus.workflow.get` | Get full workflow state by ID | `id` |
| `animus.workflow.list` | List workflow executions | `status`, `workflow_ref`, `task_id`, `phase_id`, `search`, `sort`, `limit`, `offset`, `max_tokens` |
| `animus.workflow.pause` | Pause a running workflow | `id`, `confirm`, `dry_run` |
| `animus.workflow.cancel` | Cancel a running workflow permanently | `id`, `confirm`, `dry_run` |
| `animus.workflow.resume` | Resume a paused workflow | `id` |
| `animus.workflow.phase.approve` | Approve a gated workflow phase | `workflow_id`, `phase_id`, `feedback` |

### Decision & Checkpoint Tools (2)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.workflow.decisions` | List decisions made during workflow execution | `id`, `limit`, `offset`, `max_tokens` |
| `animus.workflow.checkpoints.list` | List saved workflow state checkpoints | `id`, `limit`, `offset`, `max_tokens` |

### Definition Tools (5)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.workflow.phases.list` | List available phase definitions | `project_root` |
| `animus.workflow.phases.get` | Get a specific phase definition | `phase` |
| `animus.workflow.definitions.list` | List workflow definitions | `project_root` |
| `animus.workflow.config.get` | Read effective workflow configuration | `project_root` |
| `animus.workflow.config.validate` | Validate workflow config for errors | `project_root` |

---

## Requirements (6 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.requirements.list` | List requirements with pagination | `limit`, `offset`, `max_tokens`, `status` |
| `animus.requirements.get` | Get full requirement details by ID | `id` |
| `animus.requirements.create` | Create a new requirement | `title`, `description`, `priority`, `acceptance_criterion[]` |
| `animus.requirements.update` | Update requirement fields | `id`, `title`, `description`, `priority`, `status`, `acceptance_criterion[]` |
| `animus.requirements.delete` | Delete a requirement | `id` |
| `animus.requirements.refine` | Refine requirements with optional AI assistance | `id[]`, `focus`, `use_ai`, `tool`, `model`, `timeout_secs`, `start_runner`, `input_json` |

---

## Queue Operations (7 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.queue.list` | List queued subject dispatches | `project_root` |
| `animus.queue.stats` | Get aggregate queue depth and status counts | `project_root` |
| `animus.queue.enqueue` | Add a subject dispatch to the queue | `task_id`, `requirement_id`, `title`, `description`, `workflow_ref`, `input_json` |
| `animus.queue.reorder` | Set preferred dispatch order | `subject_ids[]` |
| `animus.queue.hold` | Hold a pending subject from dispatch | `subject_id` |
| `animus.queue.release` | Release a held subject for dispatch | `subject_id` |
| `animus.queue.drop` | Remove a queued subject dispatch permanently | `subject_id`, `project_root` |

---

## Output & Monitoring (6 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.output.run` | Get stdout/stderr from an agent execution | `run_id` |
| `animus.output.tail` | Get most recent output/error/thinking events | `run_id`, `task_id`, `event_types[]`, `limit` |
| `animus.output.monitor` | Stream real-time output from a run, optionally scoped by task or phase | `run_id`, `task_id`, `phase_id` |
| `animus.output.jsonl` | Get structured JSONL event log | `run_id`, `entries` |
| `animus.output.artifacts` | Get files generated during execution | `execution_id` |
| `animus.output.phase-outputs` | Get persisted workflow phase outputs | `workflow_id`, `phase_id`, `project_root` |

---

## Runner (4 tools)

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.runner.health` | Check runner process health and capacity | `project_root` |
| `animus.runner.orphans-detect` | Find orphaned runner processes | `project_root` |
| `animus.runner.orphans-cleanup` | Clean up orphaned runner processes by run ID | `run_id`, `project_root` |
| `animus.runner.restart-stats` | View runner uptime and restart history | `project_root` |

---

## Skills (3 tools)

Discover and inspect skill definitions across every source the project can see: bundled built-ins,
the `animus.core-skills` pack and other installed packs, registry-tracked installs, user-scoped
(`~/.animus/skills/`), project-scoped (`.animus/skills/`), and agent-host probes
(`~/.claude/skills/`, `~/.codex/skills/`, etc.).

Each result carries a `source` tag (`"builtin"`, `"installed"`, `"user"`, `"project"`,
`"agent_host"`) plus a `source_detail` object with provenance. For `installed` sources,
`source_detail` includes `registry`, `source`, `version`, `integrity`, and `artifact`. For
`agent_host` sources, `source_detail` includes `host` (e.g. `"claude-code"`), `scope`
(`"project"` | `"global"`), `structural_fields_stripped: true`, and `trust_tier: "prompt_text_only"`
— a reminder that structural fields (`tool_policy`, `mcp_servers`, `env`, `extra_args`,
`capabilities`, `adapters`, `codex_config_overrides`) are stripped at parse time for agent-host
skills, so only prompt text and prompt directives are trusted.

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.skill.list` | Enumerate skills across all sources with optional `source` filter | `project_root`, `source` (`builtin` \| `installed` \| `user` \| `project` \| `agent_host` \| host id like `claude-code`) |
| `animus.skill.get` | Resolve a skill by name and return its full `SkillDefinition` plus provenance. Resolution priority: project > user > installed/pack > builtin > agent-host. Agent-host responses include a `notice` field explaining the structural-field strip | `project_root`, `name` |
| `animus.skill.search` | Case-insensitive substring match over skill `name`, `description`, and `tags`. Returns the same row shape as `animus.skill.list` plus a `truncated` flag when matches exceed `limit` | `project_root`, `query`, `source`, `limit` (default 50) |

---

## Memory (4 tools)

Project-scoped agent memory store. Each entry is `{ id, text, created_at, source }` and lives
keyed by `agent_id` under the repo-scoped runtime root.

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.memory.get` | Fetch the full memory document for an agent profile, optionally narrowing to a single entry by id | `agent_id`, `entry_id`, `project_root` |
| `animus.memory.list` | List memory entries for an agent with optional case-sensitive `prefix` filter on entry text | `agent_id`, `prefix`, `limit`, `project_root` |
| `animus.memory.append` | Add a new memory entry. The entry receives a fresh uuid and timestamp. Returns the appended entry | `agent_id`, `text`, `source`, `project_root` |
| `animus.memory.clear` | Delete a single entry by `entry_id`, or wipe all entries for the agent when `delete_all: true`. One of `entry_id` or `delete_all=true` is required | `agent_id`, `entry_id`, `delete_all`, `project_root` |

### Memory tool exposure model

The `animus.memory.*` tools are exposed in two places, with different gating:

- **Top-level `animus mcp serve`**: all four tools are always present. External MCP clients
  (Claude Desktop, Cursor, etc.) can read/write memory for any agent id.
- **Spawned workflow agents**: the memory MCP server is injected into a phase's runtime
  contract only when the active agent profile has `capabilities.memory: true`. Profiles with
  the capability absent or set to `false` do not see the memory tools in their tool list.
  See `crates/workflow-runner-v2/src/runtime_contract.rs::inject_memory_mcp_for_capable_agent`.

---

## Plugins (2 tools)

Discovered Animus STDIO plugins are reachable from MCP clients via these meta-tools.
Plugins themselves can declare additional `mcp_tools` in their `initialize` response;
those are aggregated automatically.

| Tool | Description | Key Parameters |
|---|---|---|
| `animus.plugin.list` | List discovered plugins (providers, subject backends, custom) with name, version, kind, source, and path. The response gains a parallel top-level `warnings` array for plugins that failed their `--manifest` probe (binary missing, exit non-zero, malformed JSON). | `project_root` (optional) |
| `animus.plugin.call` | Send a JSON-RPC request to a discovered plugin. The plugin is spawned (or reused) and handshaked before the call. | `name`, `method`, `params` (optional), `project_root` (optional) |

Discovery order: `~/.animus/plugins.yaml` (or the legacy `~/.config/animus/plugins.yaml` on first read) → `.animus/plugins/` → `$ANIMUS_PLUGIN_DIR` (defaults to `~/.animus/plugins/`) → `$ANIMUS_PLUGIN_PATH` → `$PATH` (`animus-provider-*` / `animus-plugin-*` prefixes; `$PATH` opt-in via `--include-system-path`).

---

## List Tool Pagination

All list tools support pagination via these common parameters:

| Parameter | Type | Default | Max | Description |
|---|---|---|---|---|
| `limit` | integer | 25 | 200 | Maximum items to return |
| `offset` | integer | 0 | -- | Items to skip |
| `max_tokens` | integer | 3000 | 12000 | Token budget for response compaction (min: 256) |

List responses are wrapped in a guard envelope (`animus.mcp.list.result.v1`) that includes pagination metadata.

## Batch Tool Behavior

Batch tools (`animus.task.bulk-status`, `animus.task.bulk-update`, `animus.workflow.run-multiple`) accept an `on_error` parameter:

| Value | Behavior |
|---|---|
| `"continue"` | Process all items regardless of failures |
| `"stop"` | Stop processing after the first failure; remaining items are marked `"skipped"` |

Batch responses use the `animus.mcp.batch.result.v1` schema with a summary of succeeded/failed/skipped counts and per-item results.

Maximum batch size is 100 items per call.

See also: [JSON Envelope Contract](json-envelope.md), [CLI Command Surface](cli/index.md).
