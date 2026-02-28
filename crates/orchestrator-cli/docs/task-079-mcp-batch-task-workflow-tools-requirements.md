# TASK-079 Requirements: MCP Batch Task and Workflow Operation Tools

## Phase
- Workflow phase: `requirements`
- Workflow ID: `7887fc3c-e471-46d3-9d8b-56e41347ed6b`
- Task: `TASK-079`
- Snapshot date: `2026-02-27`
- Requirement: unlinked in current task metadata

## Objective
Add deterministic MCP batch tools that reduce call round-trips for multi-item
task/workflow operations:
- `ao.task.bulk-update`
- `ao.task.bulk-status`
- `ao.workflow.run-multiple`

The new tools must preserve repository safety, keep behavior explicit, and
provide per-item outcomes in one response.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| MCP task/workflow mutation tools | `crates/orchestrator-cli/src/services/operations/ops_mcp.rs` (`ao.task.update`, `ao.task.status`, `ao.workflow.run`) | one mutation per MCP call | no batch mutation surface |
| MCP execution wrapper | `ops_mcp.rs` (`AoMcpServer::run_tool`, `execute_ao`) | executes one AO command and returns one `result`/`error` | no per-item aggregate result contract |
| Task update arg mapping | `ops_mcp.rs` (`ao_task_update`) | maps one request into `task update` CLI args | no shared bulk validator/executor |
| Task status arg mapping | `ops_mcp.rs` (`ao_task_status`) | maps one request into `task status` CLI args | no multi-task status transition contract |
| Workflow run arg mapping | `ops_mcp.rs` (`ao_workflow_run`) | maps one request into `workflow run` CLI args | no multi-task run launch contract |
| Core/CLI command surfaces | `runtime_project_task/task.rs`, `ops_workflow.rs` | validates and executes single update/status/run operations | MCP currently requires N round-trips for N operations |

## Scope
In scope for implementation after this requirements phase:
- Add MCP tools:
  - `ao.task.bulk-update`
  - `ao.task.bulk-status`
  - `ao.workflow.run-multiple`
- Add a shared batch execution/result contract for these tools.
- Add deterministic input validation, bounded operation count, and ordered
  execution semantics.
- Add fail-fast default behavior with opt-in continue-on-error mode.
- Add focused tests for validation, ordering, fail-fast/continue behavior, and
  regression safety.

Out of scope:
- Adding new top-level AO CLI commands (`ao task bulk-*`, `ao workflow run-multiple`).
- Implementing cross-operation datastore transactions or rollback semantics.
- Changing single-operation MCP tool contracts for `ao.task.update`,
  `ao.task.status`, or `ao.workflow.run`.
- Manual edits to `.ao/*.json`.

## Constraints
- Determinism:
  - execute operations in request order only.
  - result ordering must match input ordering.
- Safety:
  - validate tool input before any mutation.
  - enforce bounded operation count per request.
- Compatibility:
  - existing single-operation MCP tools remain unchanged.
- Explicit atomicity semantics:
  - one MCP call batches multiple operations, but execution is
    non-transactional across items (no rollback).
  - default mode should minimize partial mutation spread by stopping on first
    failure.
- Repository policy:
  - no direct `.ao` state-file patching.

## Batch Tool Contracts

### Common Input Contract
All three tools accept:
- `operations`: required array, min `1`, max `100`
- `continue_on_error`: optional bool, default `false`
- `project_root`: optional string override

Validation failures return MCP structured error and perform no operations.

### Tool-Specific Input Contract
- `ao.task.bulk-update` operation item:
  - `id` (required)
  - optional update fields mirroring `ao.task.update`:
    - `title`, `description`, `priority`, `status`, `assignee`, `input_json`
  - at least one update field must be supplied per item.
- `ao.task.bulk-status` operation item:
  - `id` (required)
  - `status` (required)
- `ao.workflow.run-multiple` operation item:
  - `task_id` (required)
  - optional `pipeline_id`, `input_json`

### Output Contract
Each tool returns structured data with schema:
- `schema`: `ao.mcp.batch.result.v1`
- `tool`: invoked MCP tool name
- `execution` object:
  - `continue_on_error`
  - `requested`
  - `attempted`
  - `succeeded`
  - `failed`
  - `skipped`
  - `all_succeeded`
- `results`: one entry per input operation, in order, each with:
  - `index`
  - `status` (`success` | `error` | `skipped`)
  - `request` (normalized operation input)
  - `result` (when success)
  - `error` (when error)
  - optional `reason` (for skipped entries after fail-fast stop)

## Functional Requirements

### FR-01: New MCP Batch Tool Surfaces
Expose and register:
- `ao.task.bulk-update`
- `ao.task.bulk-status`
- `ao.workflow.run-multiple`

### FR-02: Deterministic Input Validation
Validate batch-level and operation-level shape before any mutation:
- operations array exists and is non-empty,
- operations count does not exceed max bound,
- required fields per operation are present,
- bulk-update items include at least one mutable field.

### FR-03: Arg Parity With Existing Single Tools
Per-operation CLI args must match existing single-tool mappings for:
- `ao.task.update`
- `ao.task.status`
- `ao.workflow.run`

### FR-04: Ordered Batch Execution
Execute operations strictly by input index, sequentially and deterministically.

### FR-05: Failure Strategy
- Default (`continue_on_error=false`): stop at first error; remaining items are
  returned as `skipped` with deterministic reason.
- Continue mode (`continue_on_error=true`): execute all items and collect
  per-item success/error outcomes.

### FR-06: Structured Aggregate Result
Return per-item results and aggregate counters in one deterministic payload
using `ao.mcp.batch.result.v1`.

### FR-07: Explicit Non-Transactional Semantics
Document and expose in result metadata that batch execution is non-transactional
across items (single MCP call, no rollback guarantee).

### FR-08: Regression Safety
Existing MCP tools (`ao.task.update`, `ao.task.status`, `ao.workflow.run`) keep
their current behavior and payload shape.

### FR-09: Focused Test Coverage
Add `ops_mcp` tests covering:
- input validation,
- operation bound checks,
- ordered result mapping,
- fail-fast stop behavior,
- continue-on-error behavior,
- non-batch regression safety.

## Acceptance Criteria
- `AC-01`: MCP server exposes `ao.task.bulk-update`, `ao.task.bulk-status`, and
  `ao.workflow.run-multiple`.
- `AC-02`: Invalid batch input is rejected before any operation executes.
- `AC-03`: Batch execution order matches request order deterministically.
- `AC-04`: Default mode stops on first failure and marks remaining items as
  skipped.
- `AC-05`: Continue mode executes all operations and reports mixed outcomes.
- `AC-06`: Batch response includes `ao.mcp.batch.result.v1` schema and complete
  aggregate counters.
- `AC-07`: Per-item result entries include index, status, request, and
  success/error payloads.
- `AC-08`: Existing single mutation MCP tools remain behaviorally unchanged.
- `AC-09`: Tests cover validation, execution semantics, and regression safety.

## Testable Acceptance Checklist
- `T-01`: validation test rejects empty `operations`.
- `T-02`: validation test rejects batches larger than max bound.
- `T-03`: bulk-update validation test rejects items with no mutable fields.
- `T-04`: fail-fast test verifies first error stops execution and marks
  remaining as skipped.
- `T-05`: continue-on-error test verifies all items execute and failures are
  captured per index.
- `T-06`: ordered result test verifies output index and result order align with
  input order.
- `T-07`: regression tests verify `ao.task.update`, `ao.task.status`, and
  `ao.workflow.run` paths are unchanged.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01 | MCP tool registration and schema tests in `ops_mcp.rs` |
| FR-02, FR-03 | helper-level validation and arg-building tests |
| FR-04, FR-05, FR-06 | batch executor tests with synthetic success/failure command results |
| FR-07 | contract assertions on batch metadata fields |
| FR-08, FR-09 | targeted regression tests for existing single tools and focused batch tests |

## Implementation Notes Input (Next Phase)
Primary source target:
- `crates/orchestrator-cli/src/services/operations/ops_mcp.rs`

Likely touched areas:
- new MCP input structs for batch tools,
- new batch response schema structs/constants/helpers,
- batch execution helper(s) that reuse existing AO command execution flow,
- MCP tool handlers for new batch tools,
- `#[cfg(test)]` unit coverage for validation/execution semantics.

Secondary targets only if required by implementation:
- MCP schema/registration support helpers in the same file.

## Deterministic Deliverables for Implementation Phase
- Three new MCP batch tools for task update, task status, and workflow run.
- Single-call aggregate response contract with per-item outcomes and summary.
- Deterministic fail-fast/continue-on-error behavior.
- Focused tests proving validation, ordering, failure handling, and regression
  safety.
