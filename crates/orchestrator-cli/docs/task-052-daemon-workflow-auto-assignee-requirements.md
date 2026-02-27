# TASK-052 Requirements: Auto-Assign Task Assignees for Daemon Workflow Starts and Human In-Progress Updates

## Phase
- Workflow phase: `requirements`
- Workflow ID: `2448fe71-93f7-4cae-8661-7c90e3dc9d1f`
- Task: `TASK-052`
- Requirement: unlinked in current task metadata

## Objective
Use the existing `Assignee` model (`agent|human|unassigned`) so task ownership is populated automatically instead of remaining `unassigned`.

Target behavior from task brief:
- When daemon scheduling starts a workflow for a task, assign `Assignee::Agent { role, model }`.
- When a human runs `ao task status --status in-progress`, infer and assign `Assignee::Human { user_id }` from environment/config context.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Assignee model | `crates/orchestrator-core/src/types.rs` (`Assignee`) | supports `Agent { role, model }`, `Human { user_id }`, `Unassigned` | model exists but is underused in runtime flows |
| Assignee mutation APIs | `crates/orchestrator-core/src/services/task_impl.rs` (`assign_agent`, `assign_human`) | direct APIs already persist typed assignee + metadata updates | daemon and `task status` paths do not call these APIs |
| Daemon ready-task startup | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs` (`run_ready_task_workflows_for_project`) | starts workflows and syncs task status to `in-progress` | no agent assignee update on workflow start |
| Phase execution target resolution | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs` | computes selected tool/model for phase execution | selected model is not propagated to task assignee at workflow start |
| Human status command | `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs` (`TaskCommand::Status`) | calls `tasks.set_status` only | no human-assignee inference when moving to `in-progress` |
| Legacy state compatibility | `crates/orchestrator-core/src/services/state_store.rs` | normalizes legacy string assignee to typed `human` object | new behavior must preserve backward compatibility with existing state |

## Problem Statement
Task ownership semantics are already modeled but not integrated into the main operational paths:
- daemon auto-starting workflows leaves tasks effectively anonymous,
- human operators marking tasks `in-progress` do not become assignees unless they run a separate assign command.

This weakens accountability, review routing, and observability for active work.

## Scope
In scope for implementation after this requirements phase:
- Auto-assign agent assignee when daemon starts a workflow from ready queue.
- Derive deterministic `role` and `model` for that assignment from workflow/agent runtime context.
- Auto-assign human assignee on `ao task status --status in-progress` when identity can be inferred.
- Keep status updates successful even when assignee inference is unavailable.
- Add focused tests for daemon assignment and human inference behavior.

Out of scope:
- Bulk backfill/rewrite of existing task assignees.
- Changing assignee behavior for every status transition (only the trigger above is required).
- Redesign of workflow phase execution model selection/failover logic.
- Manual edits to `/.ao/*.json`.

## Constraints
- Preserve existing `ao.cli.v1` JSON envelope and exit-code semantics.
- Keep changes additive and deterministic.
- Do not block `task status` on identity inference failure.
- Preserve typed assignee persistence and legacy-state normalization compatibility.
- Keep daemon behavior scoped to daemon-started workflow launches (no unrelated workflow command behavior changes).

## Assignee Resolution Contract

### Daemon Workflow-Start Assignment
Trigger:
- In daemon ready-queue startup path, immediately after `workflows.run(...)` succeeds for a task.

Assignment payload:
- `assignee.type = agent`
- `assignee.role` resolution order:
  1. phase `agent_id` from agent runtime config for workflow current phase
  2. fallback to current phase id
- `assignee.model`:
  - primary model selected for the current phase using the same target-selection logic used by phase execution,
  - `null` only when no model can be resolved for that phase.

Audit metadata:
- `updated_by = "ao-daemon"`

Overwrite rule:
- daemon workflow-start assignment replaces existing assignee (human or agent), because the daemon has asserted active agent ownership for execution.

### Human `task status` In-Progress Assignment
Trigger:
- `ao task status --status in-progress` path only.

Identity inference precedence:
1. `AO_ASSIGNEE_USER_ID`
2. `AO_USER_ID`
3. repo-local git config `user.email`
4. repo-local git config `user.name`
5. `USER` / `USERNAME`

Assignment behavior:
- when a non-empty identity is resolved, set `assignee.type = human` with that `user_id` and update metadata using same identity.
- when identity cannot be resolved, keep status change behavior and leave assignee unchanged.

## Functional Requirements

### FR-01: Daemon Auto-Assign on Workflow Start
Daemon ready-queue workflow starts must assign the task to `Assignee::Agent`.

### FR-02: Deterministic Agent Role/Model Derivation
Daemon assignment must derive `role` and `model` from runtime phase configuration/selection using a deterministic fallback contract.

### FR-03: Assignee and Status Coherence
After daemon workflow start, task status remains `in-progress` and assignee reflects the active agent.

### FR-04: Human In-Progress Auto-Assignment
`ao task status --status in-progress` must infer and assign a human assignee when identity is available.

### FR-05: Non-Blocking Inference Failure
If human identity cannot be inferred, status update still succeeds and no hard error is introduced.

### FR-06: Backward Compatibility
Legacy task state loading and current CLI/daemon output contracts remain compatible.

### FR-07: Regression Coverage
Add tests covering daemon agent-assignment, model/role derivation, human inference precedence, and inference-failure fallback.

## Acceptance Criteria
- `AC-01`: daemon ready-queue workflow start sets task assignee to `agent`.
- `AC-02`: daemon-assigned `role` follows configured phase agent id fallback contract.
- `AC-03`: daemon-assigned `model` is populated from phase target resolution when resolvable.
- `AC-04`: daemon assignment writes `updated_by = "ao-daemon"`.
- `AC-05`: `ao task status --status in-progress` sets human assignee when inference succeeds.
- `AC-06`: human inference precedence follows the documented order.
- `AC-07`: if inference fails, status transition still succeeds and assignee is unchanged.
- `AC-08`: no regression in existing task/workflow JSON envelopes or exit codes.

## Testable Acceptance Checklist
- `T-01`: daemon scheduler test where ready task starts workflow and assignee becomes `agent`.
- `T-02`: daemon scheduler test asserts assigned `role` + `model` values for known runtime config.
- `T-03`: task status command/runtime test where `in-progress` infers `AO_ASSIGNEE_USER_ID`.
- `T-04`: task status test for inference fallback order (env over git, git over shell user vars).
- `T-05`: task status test where no identity is found still returns success and preserves assignee.
- `T-06`: regression test ensuring non-`in-progress` status updates do not auto-assign human assignee.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-02, FR-03 | daemon scheduler tests (`run_ready_task_workflows_for_project`) |
| FR-04, FR-05 | task runtime tests for `TaskCommand::Status` in-progress path |
| FR-06 | existing JSON contract/smoke tests plus targeted assertions |
| FR-07 | targeted `cargo test -p orchestrator-cli` on touched modules |

## Implementation Notes (Input to Next Phase)
Primary expected change targets:
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs` (shared phase target resolution reuse)
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
- `crates/orchestrator-cli/src/services/runtime/` (new identity inference helper, if extracted)

Likely supporting surfaces:
- `crates/orchestrator-core/src/services/task_impl.rs` (no API changes expected; reuse existing assign methods)
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler.rs` tests
- `crates/orchestrator-cli/src/cli_types/mod.rs` and/or runtime tests for command behavior

## Deterministic Deliverables for Implementation Phase
- Daemon workflow-start path assigns `Assignee::Agent { role, model }` deterministically.
- `ao task status --status in-progress` infers and applies `Assignee::Human` when identity is available.
- Identity inference failures remain non-fatal and deterministic.
- Focused tests validate assignment semantics and compatibility constraints.
