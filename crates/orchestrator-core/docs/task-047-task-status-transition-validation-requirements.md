# TASK-047 Requirements: Task Status Transition Validation

## Phase
- Workflow phase: `requirements`
- Workflow ID: `bf2d29fd-c70f-4c1d-9484-8000864047b7`
- Task: `TASK-047`
- Requirement: unlinked in current task metadata

## Objective
Add deterministic guardrails for task status transitions so invalid lifecycle
jumps are rejected, terminal states require explicit reopen behavior, and task
status updates follow a state-machine style pattern similar to requirement
lifecycle handling.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Core status mutation helper | `crates/orchestrator-core/src/services/task_shared.rs` (`apply_task_status`) | mutates status and side effects directly with no transition validation | any status can jump to any other |
| Service status API path | `crates/orchestrator-core/src/services/task_impl.rs` (`set_status`, `update`) | calls `apply_task_status` for both in-memory and file-backed hubs | user-facing status changes are unguarded |
| Replace path | `TaskServiceApi::replace` implementations | can persist arbitrary `task.status` changes | transition guards can be bypassed by callers |
| Task-control cancel flow | `crates/orchestrator-cli/src/services/operations/ops_task_control.rs` | sets `task.status = Cancelled` then `replace` | bypasses centralized transition logic |
| Daemon task sync flows | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs` | several paths set `Done` or `Blocked` directly/indirectly from mixed prior states | strict transition rules would regress daemon reconciliation unless updated |
| Reopen command surface | `crates/orchestrator-cli/src/cli_types.rs` + task handlers | no explicit reopen command/action exists | terminal states cannot be safely reopened under explicit rule |

## Scope
In scope for implementation after this requirements phase:
- Introduce centralized task status transition validation in
  `orchestrator-core` with deterministic allowed/disallowed outcomes.
- Enforce the transition policy across task status mutation entrypoints:
  - `tasks.set_status(...)`,
  - `tasks.update(...status...)`,
  - status-changing `tasks.replace(...)` call paths.
- Add explicit reopen action for terminal tasks (`done`/`cancelled`) instead of
  allowing direct terminal-to-nonterminal `status` updates.
- Update daemon/task-control call sites that currently bypass transition
  validation.
- Add focused tests proving transition rules, reopen behavior, and no-regression
  on existing non-terminal flows.

Out of scope for this task:
- Reworking workflow state machine behavior.
- Reworking requirement lifecycle definitions.
- Manual edits to `/.ao/*.json` state files.
- Redesigning task priority/type/dependency semantics.

## Transition Policy (Required)

### Rule Set
- `InProgress` requires prior status `Ready` or `Backlog`.
- `Done` requires prior status `InProgress`.
- `Done` and `Cancelled` are terminal for normal `set_status`/`update` flows.
- Transitioning from terminal state to non-terminal state requires explicit
  reopen action.

### Explicit Reopen Contract
- Reopen is a distinct action/event, not a normal status update.
- Reopen source states: `Done`, `Cancelled`.
- Reopen destination states: `Backlog` or `Ready` (implementation-default target
  should be deterministic and documented).
- Reopen must clear `cancelled` when leaving `Cancelled`.

### Compatibility Rule
- All non-terminal transitions not restricted by the rule set above remain
  allowed unless a stricter policy is explicitly added in this task.

## Constraints
- Validation must be centralized so callers cannot accidentally bypass rules.
- Error paths must be deterministic and include clear from/to status context.
- No silent mutation on invalid transitions.
- Existing status side effects (`paused`, block metadata, started/completed
  timestamps) must remain consistent unless explicitly documented and tested.
- Keep behavior repository-safe and Rust-only (no desktop wrapper changes).

## Functional Requirements

### FR-01: Central Transition Evaluator
- Implement a task status transition evaluator that uses explicit state + event
  mapping (pattern aligned with requirement lifecycle state-machine style).

### FR-02: Terminal Guardrails
- Standard status update flows must reject transitions out of `Done` or
  `Cancelled`.

### FR-03: In-Progress Entry Rule
- Attempting to set `InProgress` from any state other than `Backlog` or `Ready`
  must fail.

### FR-04: Done Entry Rule
- Attempting to set `Done` from any state other than `InProgress` must fail.

### FR-05: Explicit Reopen Action
- Provide explicit reopen action for terminal tasks.
- Reopen must be exposed through CLI/runtime command surface used by operators.

### FR-06: Entry-Point Coverage
- Transition rules must be enforced for:
  - `task status`,
  - `task update --status`,
  - task-control/daemon paths that mutate status via replace/direct assignment.

### FR-07: Daemon Compatibility
- Daemon reconciliation flows must remain functional under the new rules by
  using compliant transition sequences.

### FR-08: Deterministic Failure Behavior
- Invalid transitions must return stable, actionable errors and leave task state
  unchanged.

### FR-09: Regression Tests
- Add deterministic tests for valid/invalid transitions, reopen behavior, and
  daemon/task-control integrations affected by stricter validation.

### FR-10: Backward Compatibility
- Existing supported non-terminal lifecycle operations continue to work when
  they comply with the new guardrails.

## Acceptance Criteria
- `AC-01`: `InProgress` from `Ready` and `Backlog` succeeds.
- `AC-02`: `InProgress` from `Blocked|OnHold|Done|Cancelled` is rejected.
- `AC-03`: `Done` from `InProgress` succeeds.
- `AC-04`: `Done` from any state other than `InProgress` is rejected.
- `AC-05`: Direct terminal-to-nonterminal status updates are rejected.
- `AC-06`: Explicit reopen action transitions terminal task to allowed reopen
  state.
- `AC-07`: Reopen is unavailable for non-terminal tasks.
- `AC-08`: Invalid transitions do not mutate task status/metadata.
- `AC-09`: Daemon/task-control flows remain operational and compliant.
- `AC-10`: Test coverage exists for transition matrix and affected integrations.

## Testable Acceptance Checklist
- `T-01`: Unit tests for transition evaluator matrix (allowed vs denied).
- `T-02`: Task service tests for `set_status` and `update(status)` guard
  enforcement.
- `T-03`: Replace-path tests confirm status-changing replace honors validation.
- `T-04`: CLI tests for explicit reopen command/action and error messaging.
- `T-05`: Daemon scheduler tests cover previously direct-to-done flows under new
  compliant sequencing.
- `T-06`: Task-control cancel/reopen tests verify terminal semantics and no
  bypass.

## Acceptance Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01..FR-04 | `orchestrator-core` unit tests for transition evaluator + service mutations |
| FR-05 | CLI parse/dispatch + runtime handler tests for reopen action |
| FR-06 | regression tests around `task status`, `task update`, and replace call paths |
| FR-07 | daemon scheduler/runtime tests in `orchestrator-cli` |
| FR-08 | negative tests asserting unchanged state after rejected transition |
| FR-09, FR-10 | targeted crate tests proving guarded behavior without unrelated regressions |

## Implementation Notes Input (Next Phase)
Primary source targets:
- `crates/orchestrator-core/src/services/task_shared.rs`
- `crates/orchestrator-core/src/services/task_impl.rs`
- `crates/orchestrator-core/src/services.rs` (task API surface)
- `crates/orchestrator-cli/src/cli_types.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
- `crates/orchestrator-cli/src/services/operations/ops_task_control.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`

Likely test targets:
- `crates/orchestrator-core` task service/unit tests (new + existing modules)
- `crates/orchestrator-cli/tests/cli_e2e.rs`
- daemon runtime/scheduler tests under
  `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`

## Deterministic Deliverables for Implementation Phase
- Centralized task-status transition validator with explicit reopen semantics.
- CLI/runtime reopen surface for terminal tasks.
- Updated daemon/task-control status paths that comply with guardrails.
- Focused regression tests covering transition rules and integration paths.
