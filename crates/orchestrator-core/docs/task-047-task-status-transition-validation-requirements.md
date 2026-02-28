# TASK-047 Requirements: Task Status Transition Validation

## Phase
- Workflow phase: `requirements`
- Workflow ID: `1d7526ca-484b-4c90-afc7-ae1b16fe2b3c`
- Task: `TASK-047`

## Objective
Add guardrails to task status transitions so that:
- Terminal states (Done, Cancelled) require explicit reopen action
- InProgress requires Ready or Backlog as prior state
- Done requires InProgress as prior state

This follows the existing requirement lifecycle state machine pattern.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Status transition validation | `crates/orchestrator-core/src/services/task_shared.rs` (`validate_task_status_transition`) | Blocks terminal state transitions only | Missing: InProgress→Done guard, InProgress prior-state requirement |
| Task status enum | `crates/orchestrator-core/src/types.rs` (`TaskStatus`) | Defines Backlog, Ready, InProgress, Blocked, OnHold, Done, Cancelled | Already has `is_terminal()` helper |
| Status mutation | `crates/orchestrator-core/src/services/task_shared.rs` (`apply_task_status`) | Applies status, sets paused/cancelled flags, timestamps | Works correctly |
| CLI status command | `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs` | Calls `tasks.set_status()` | Bypasses validation in some paths |
| Reopen command | N/A | Not implemented | Needs new CLI command |
| Daemon scheduler | Various `set_status` calls | Uses set_status directly | Will need validation updates |

## Problem Statement
Currently any task status can transition to any other (Done→Backlog, Cancelled→InProgress). This violates basic workflow invariants:
- A task cannot go directly to Done without being InProgress first
- A task cannot go to InProgress without being Ready or Backlog first
- Terminal states (Done, Cancelled) require explicit reopen action to exit

## Scope
In scope for implementation:
- Enhance `validate_task_status_transition` to enforce:
  - Done requires InProgress as prior state
  - InProgress requires Ready or Backlog as prior state
- Add `task reopen` CLI command to exit terminal states
- Add regression tests for transition validation
- Update all `set_status` call paths to use validation

Out of scope:
- Task lifecycle state machine ( state machine enginefull for tasks)
- Changes to daemon scheduling logic beyond status validation
- Direct/manual edits to `/.ao/*.json`

## Constraints
- Keep validation deterministic for both in-memory and file-backed hubs
- Keep mutation centralized in existing shared task helper
- Preserve current timestamp behavior
- Avoid broad refactors unrelated to transition validation

## Functional Requirements

### FR-01: Done Requires InProgress Prior State
- When status transitions to `Done`, current status must be `InProgress`.
- Any other prior state (Backlog, Ready, Blocked, OnHold, Cancelled) must be rejected.

### FR-02: InProgress Requires Ready or Backlog Prior State
- When status transitions to `InProgress`, current status must be `Ready` or `Backlog`.
- Any other prior state (InProgress, Blocked, OnHold, Done, Cancelled) must be rejected.
- Exception: daemon scheduling logic may auto-transition from Ready→InProgress.

### FR-03: Terminal States Require Explicit Reopen
- When current status is `Done` or `Cancelled`, only `task reopen` command can exit to a non-terminal state.
- Direct `task status` command to non-terminal states from terminal states must be rejected.

### FR-04: Same-Status Transitions Are No-Ops
- Transitioning from a status to itself should succeed as a no-op (existing behavior).

### FR-05: Daemon Auto-Transitions Are Exempt from FR-02
- Daemon scheduler's automatic Ready→InProgress transitions must be allowed.
- This applies to daemon-internal status updates, not CLI `task status` command.
- Implementation: Add optional bypass flag to `set_status` trait method.

## Implementation Detail: set_status Bypasses Validation

| Call Path | Current behavior | Required change |
| --- | --- | --- |
| `task update --status` | Uses `apply_task_update` which validates | No change needed |
| `task status` | Uses `set_status` directly, bypasses validation | Will fail after FR-01/FR-02 added |
| Daemon scheduler | Uses `set_status` directly | Needs bypass flag |

The `set_status` implementations in both `InMemoryServiceHub` and `FileServiceHub` call `apply_task_status` directly without calling `validate_task_status_transition`. This is why `task status` command allows invalid transitions.

## Acceptance Criteria
- `AC-01`: `backlog → done` fails with "cannot transition directly to Done without InProgress"
- `AC-02`: `ready → in-progress` succeeds
- `AC-03`: `in-progress → done` succeeds
- `AC-04`: `done → backlog` fails with "terminal state requires reopen"
- `AC-05`: `cancelled → ready` fails with "terminal state requires reopen"
- `AC-06`: `task reopen` command successfully transitions Done/Cancelled→Backlog
- `AC-07`: `backlog → in-progress` fails (requires Ready first)
- `AC-08`: All regression tests pass

## Testable Acceptance Checklist
- `T-01`: Unit tests in `services/tests.rs` validating each transition rule
- `T-02`: CLI e2e tests for `task status` command validation
- `T-03`: CLI e2e test for `task reopen` command
- `T-04`: Run: `cargo test -p orchestrator-core services::tests -- --nocapture`
- `T-05`: Run: `cargo test -p orchestrator-cli --test cli_e2e task -- --nocapture`

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01 | Unit tests asserting Done transition requires InProgress |
| FR-02 | Unit tests asserting InProgress requires Ready/Backlog |
| FR-03 | Unit tests + e2e tests for terminal state rejection |
| FR-04 | Existing no-op behavior preserved |
| FR-05 | Daemon scheduler continues to function |

## Implementation Notes Input (Next Phase)
Primary implementation surfaces:

### 1) Core Validation Logic
- `crates/orchestrator-core/src/services/task_shared.rs`
  - Enhance `validate_task_status_transition` with FR-01, FR-02 logic

### 2) Task Service API Changes
- `crates/orchestrator-core/src/services.rs` (`TaskServiceApi` trait)
  - Add optional `bypass_validation: bool` parameter to `set_status` method
- `crates/orchestrator-core/src/services/task_impl.rs`
  - Both implementations: add validation call when bypass is false

### 3) CLI Commands
- `crates/orchestrator-cli/src/cli_types/task_types.rs`
  - Add `Reopen` variant to `TaskCommand`
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
  - Implement `reopen` handler that transitions terminal→Backlog

### 4) Daemon Scheduler Updates
- All `set_status` calls in daemon code need `bypass_validation: true`

## Deterministic Deliverables for Implementation Phase
- Status transitions are validated according to FR-01 through FR-04
- `task reopen` command exists and works
- Regression tests cover all transition rules
- All existing tests continue to pass
