# TASK-055 Requirements: Expose OrchestratorTask Estimation Fields

## Phase
- Workflow phase: `requirements`
- Workflow ID: `b8b7b420-2cba-4f6f-8805-f940d4fa84d3`
- Task: `TASK-055`
- Requirement: unlinked in current task metadata

## Objective
Resolve the current mismatch where multiple `OrchestratorTask` fields exist in
the persisted schema but are not operator-settable from first-class task
create/update flows. The implementation must make these fields usable for
agent-assisted estimation without breaking task JSON compatibility.

## Baseline Audit

| Field | Current write path | Current read path | Gap |
| --- | --- | --- | --- |
| `risk` | defaulted to `medium` in task create paths (`task_impl.rs`, `planning_shared.rs`) | filter-only read via `TaskFilter.risk` in core; CLI does not expose a risk filter | effectively constant in CLI-driven workflows |
| `scope` | defaulted to `medium`; AI task generation may set from complexity | serialized only; no current scheduling behavior depends on it | not operator-settable; low practical value today |
| `complexity` | defaulted to `medium`; AI task generation may set from candidate complexity | daemon routing/model selection paths consume task complexity | manual task flows cannot set complexity despite runtime use |
| `impact_area` | defaulted to `[]`; requirement-derived task creation may set frontend | `OrchestratorTask::is_frontend_related` uses it for frontend gating logic | manual task flows cannot set impact area |
| `estimated_effort` | always `null` in current create flows | serialized only | no first-class way to carry effort estimate |
| `resource_requirements` | always `ResourceRequirements::default()` | serialized only | no first-class way to carry resource limits/requirements |

## Decision
For `TASK-055`, **expose these fields in create/update surfaces** rather than
removing them from `OrchestratorTask`.

Why this decision:
- It preserves compatibility of existing task JSON shape and call sites.
- `complexity` and `impact_area` already participate in runtime behavior.
- It unlocks estimation metadata for manual/CLI/API task management immediately.

## Scope
In scope for implementation after this requirements phase:
- Extend task create/update inputs so all six fields are settable.
- Add CLI flags for `ao task create` and `ao task update` to populate these
  fields.
- Add CLI risk filtering support in `ao task list` (`--risk`).
- Extend Web API task create/patch request parsing for these fields.
- Keep serialization shape stable and persisted per-task JSON deterministic.
- Add targeted tests for parsing, updates, persistence, and filter behavior.

Out of scope:
- Removing `risk`, `scope`, `complexity`, `impact_area`,
  `estimated_effort`, or `resource_requirements` from `OrchestratorTask`.
- Changing scheduler routing semantics beyond existing complexity consumption.
- Backfilling existing tasks in `/.ao/tasks` with non-default estimates.
- Manual edits to `/.ao/*.json`.

## Constraints
- Backward compatibility: existing task JSON files must remain readable without
  migration.
- Omitted fields keep current defaults.
- Validation errors must remain explicit and actionable.
- Output remains deterministic for both JSON and non-JSON CLI modes.
- Changes stay scoped to Rust crates in this repository.

## Field Input Contract

| Field | Accepted values / shape | Default when omitted |
| --- | --- | --- |
| `risk` | `high|medium|low` | `medium` |
| `scope` | `large|medium|small` | `medium` |
| `complexity` | `high|medium|low` | `medium` |
| `impact_area` | repeated values: `frontend|backend|database|api|infrastructure|docs|tests|cicd` | `[]` |
| `estimated_effort` | trimmed string; empty string treated as unset/`null` | `null` |
| `resource_requirements.max_cpu_percent` | number in `(0, 100]` | `null` |
| `resource_requirements.max_memory_mb` | positive integer | `null` |
| `resource_requirements.requires_network` | `true|false` | `true` |

## Functional Requirements

### FR-01: Core Input Surface
- `TaskCreateInput` and `TaskUpdateInput` must include fields needed to set all
  six target task properties.
- Core update application logic must mutate these fields when provided.

### FR-02: CLI Create Surface
- `ao task create` must accept flags for `risk`, `scope`, `complexity`,
  `impact_area`, `estimated_effort`, and resource requirements.

### FR-03: CLI Update Surface
- `ao task update` must accept equivalent flags and apply only provided values.
- Update path must support clearing `estimated_effort` and impact-area values.

### FR-04: CLI List Risk Filter
- `ao task list` must expose `--risk` and route it to `TaskFilter.risk`.

### FR-05: Web API Parity
- Task create/patch request models and handlers in `orchestrator-web-api` must
  support the same field set (including parsing/validation).

### FR-06: Persistence and Readback
- `ao task get`, list endpoints, and task JSON persistence must reflect updated
  values without schema drift.

### FR-07: Validation
- Invalid enum keys or out-of-range resource values must fail with deterministic
  `invalid_input`-style errors.

### FR-08: Compatibility
- Existing task operations unrelated to these fields must remain behaviorally
  unchanged.

## Acceptance Criteria
- `AC-01`: Operators can set all six fields via `ao task create`.
- `AC-02`: Operators can update all six fields via `ao task update`.
- `AC-03`: `ao task list --risk <value>` filters tasks by persisted risk.
- `AC-04`: Web API task create/patch accepts and persists the same field set.
- `AC-05`: Omitted fields still default to legacy values.
- `AC-06`: Task JSON payload shape remains backward compatible and includes the
  updated values.
- `AC-07`: Invalid enum/resource inputs fail with actionable validation errors.
- `AC-08`: Targeted CLI/core/web-api tests pass.

## Testable Acceptance Checklist
- `T-01`: CLI parse tests for new `task create/update/list` flags.
- `T-02`: Core service tests proving create/update mutation for all six fields.
- `T-03`: Web API parsing/handler tests for create/patch field parity.
- `T-04`: Risk filter tests for CLI/Web API list paths.
- `T-05`: Validation tests for enum parsing and resource bounds.
- `T-06`: Regression tests confirming unchanged behavior when new flags are not
  provided.

## Acceptance Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01 | core unit/service tests for create/update mutation paths |
| FR-02, FR-03 | CLI argument parse + runtime handler tests |
| FR-04 | task list filter tests with mixed-risk fixtures |
| FR-05 | web-api request/handler tests for create/patch payloads |
| FR-06 | persistence/readback assertions from task service tests |
| FR-07 | parse/validation negative tests |
| FR-08 | existing task command regression checks |

## Implementation Notes (Input to Next Phase)
Primary implementation targets:
- `crates/orchestrator-core/src/types.rs`
- `crates/orchestrator-core/src/services/task_impl.rs`
- `crates/orchestrator-core/src/services/task_shared.rs`
- `crates/orchestrator-cli/src/cli_types/task_types.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
- `crates/orchestrator-cli/src/shared/parsing.rs`
- `crates/orchestrator-web-api/src/services/web_api_service/{requests.rs,parsing.rs,tasks_handlers.rs}`

## Deterministic Deliverables for Implementation Phase
- Task create/update CLI and API surfaces can set estimation fields.
- Risk filter is available in CLI list.
- Persisted task JSON remains compatible and deterministic.
- Tests prove field mutation, validation, and regression safety.
