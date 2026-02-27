# TASK-047 Implementation Notes: Task Status Transition Validation

## Phase Context
- Workflow phase: `requirements`
- Workflow ID: `bf2d29fd-c70f-4c1d-9484-8000864047b7`
- Task: `TASK-047`

## Purpose
Translate TASK-047 into a minimal, deterministic implementation slice that
introduces explicit task status transition guardrails, adds a dedicated reopen
action for terminal tasks, and updates status-mutating call sites to use the
same transition policy.

## Non-Negotiable Constraints
- Keep transition policy centralized in `orchestrator-core` task status logic.
- Do not allow terminal-state reopen through generic `set_status`/`update`
  paths.
- Preserve existing `ao.cli.v1` output envelope behavior.
- Do not manually edit `/.ao/*.json`.
- Keep daemon reconciliation behavior operational after enforcement.

## Proposed Change Surface

### 1) Core Transition Evaluator (State-Machine Style)
- `crates/orchestrator-core/src/services/task_shared.rs`
  - add transition evaluation helper(s) similar in style to requirement
    lifecycle pattern (explicit event + from/to validation outcome).
  - encode rules:
    - `InProgress` only from `Backlog|Ready`,
    - `Done` only from `InProgress`,
    - terminal (`Done|Cancelled`) requires explicit reopen event to leave.
  - return structured error context for invalid transitions.

Suggested shape (internal):
- `TaskStatusTransitionEvent` enum (for example `SetStatus(target)` and
  `Reopen(target)`).
- `TaskStatusTransitionOutcome` containing `from`, `to`, `matched`, and
  optional guard detail.
- shared validator function consumed by all status-mutating task APIs.

### 2) Task Service API Wiring
- `crates/orchestrator-core/src/services.rs`
  - add a dedicated task reopen API entrypoint (for example
    `reopen(id, target_status)`), or equivalent explicit-action method.
- `crates/orchestrator-core/src/services/task_impl.rs`
  - enforce validation in both `InMemoryServiceHub` and `FileServiceHub` for:
    - `set_status`,
    - `update` when `status` is present,
    - `replace` when persisted status differs from prior state.
  - ensure metadata/version updates remain consistent on success only.

### 3) CLI Command Surface for Explicit Reopen
- `crates/orchestrator-cli/src/cli_types.rs`
  - add task reopen command/args in `TaskCommand`.
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
  - route reopen command to new core API.
  - keep `task status` behavior strict (no terminal reopen via status update).
- update help strings to clearly document reopen semantics.

### 4) Bypass Call-Site Migration
Update existing status mutation paths that bypass centralized validation:
- `crates/orchestrator-cli/src/services/operations/ops_task_control.rs`
  - replace direct `task.status = ...` + `replace` flows with guarded
    status/reopen API calls as appropriate.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
  - refactor direct/detoured done/block transitions to compliant sequences.
  - remove status bypasses implemented through raw struct mutation + `replace`
    where transition validation should apply.

### 5) Error Handling and Operator Clarity
- Make rejected transitions deterministic and actionable:
  - include task id + from/to status + required action hint (for example,
    explicit reopen requirement).
- Ensure CLI error classification remains stable for user-facing invalid
  transition attempts.

## Suggested Implementation Sequence
1. Implement transition evaluator and unit tests in `orchestrator-core`.
2. Integrate evaluator into task service methods (`set_status`, `update`,
   status-changing `replace`) for both hub implementations.
3. Add explicit reopen API method in core services and wire CLI command surface.
4. Migrate `task-control` and daemon status mutation paths to compliant APIs.
5. Add/adjust tests in core + CLI/daemon modules for guarded behavior.
6. Run targeted test set and resolve regressions.

## Testing Strategy
Core tests:
- transition matrix tests for allowed/denied edges.
- service mutation tests proving no state mutation on invalid transitions.
- reopen tests for terminal-only availability and target validation.

CLI/runtime tests:
- parse/dispatch coverage for new `task reopen` command.
- e2e task lifecycle tests covering:
  - `backlog -> ready -> in_progress -> done`,
  - terminal reopen path,
  - rejected terminal direct status updates.

Daemon/task-control tests:
- scheduler flows that previously set `done` directly now follow compliant
  sequence and still end in expected terminal state.
- task-control cancel/reopen behavior remains deterministic and guarded.

## Validation Targets
- `cargo test -p orchestrator-core`
- `cargo test -p orchestrator-cli --test cli_e2e`
- `cargo test -p orchestrator-cli --test cli_smoke`
- targeted daemon runtime module tests under
  `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`

## Risks and Mitigations
- Risk: hidden status bypass through `replace` persists.
  - Mitigation: compare previous vs new status in replace path and validate.
- Risk: daemon reconciliation paths break due stricter `Done` prerequisite.
  - Mitigation: migrate to compliant transition sequencing and add explicit
    scheduler regression coverage.
- Risk: unclear operator UX for terminal reopen failures.
  - Mitigation: explicit CLI reopen command and deterministic error hints.
- Risk: metadata regressions (`started_at`, `completed_at`, blocked fields).
  - Mitigation: add focused assertions on metadata side effects in tests.

## Deliverables for Next Phase
- Central transition validator integrated across task status mutation APIs.
- Explicit CLI/core reopen flow for terminal tasks.
- Daemon/task-control status paths aligned with transition rules.
- Regression tests proving guardrails and compatibility.
