# TASK-079 Requirements: MCP Bulk Task and Workflow Operations

## Phase
- Workflow phase: `requirements`
- Workflow ID: `7887fc3c-e471-46d3-9d8b-56e41347ed6b`
- Task: `TASK-079`
- Snapshot date: `2026-02-27`

## Objective
Add deterministic MCP batch tools that reduce round-trips for common task and
workflow operations:
- `ao.task.bulk-update`
- `ao.task.bulk-status`
- `ao.workflow.run-multiple`

Each tool must execute a bounded list of operations in a single MCP request and
return structured per-item outcomes.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Single task mutation MCP tools | `crates/orchestrator-cli/src/services/operations/ops_mcp.rs` | `ao.task.update` and `ao.task.status` only accept one task per call | 10 task updates require 10 MCP round-trips |
| Single workflow run MCP tool | `ops_mcp.rs` | `ao.workflow.run` only supports one `task_id` per call | cannot launch a deterministic task batch from one request |
| CLI task command surface | `crates/orchestrator-cli/src/cli_types/task_types.rs` | only singular `Update` and `Status` commands | no first-class batch command to proxy directly |
| CLI workflow command surface | `crates/orchestrator-cli/src/cli_types/workflow_types.rs` | only singular `Run` command | no first-class run-many command |
| MCP command execution wrapper | `ops_mcp.rs` (`execute_ao`, `run_tool`) | executes one AO command and returns one result payload | no shared batch execution model, no per-item aggregation |

## Scope
In scope for implementation after this requirements phase:
- Add MCP tools:
  - `ao.task.bulk-update`
  - `ao.task.bulk-status`
  - `ao.workflow.run-multiple`
- Add deterministic batch execution helper(s) in `ops_mcp.rs` that:
  - preserve input order,
  - execute one AO command per item in sequence,
  - aggregate per-item success/failure outcomes.
- Add `on_error` execution policy with values:
  - `stop` (default): stop on first failed item and mark remaining items as skipped,
  - `continue`: attempt all items and report all failures.
- Add shared batch response schema with summary metadata and item-level results.
- Add focused tests for validation, policy behavior, and result shaping.

Out of scope:
- Adding new top-level AO CLI commands (`task bulk-update`, `workflow run-multiple`)
  in this task slice.
- Cross-command rollback/true transactional all-or-nothing semantics.
- Parallel mutation execution inside a single batch request.
- Direct edits to `/.ao/*.json`.

## Constraints
- Determinism:
  - items execute strictly in request order,
  - result list preserves request order and includes stable zero-based indices.
- Boundedness:
  - each batch must enforce size limits (`1..=100` items).
- Safety:
  - no manual state-file edits; only AO command pathways are used.
- Compatibility:
  - existing single-item MCP tools remain unchanged.
- Atomicity model:
  - MCP call is atomic at request boundary only; per-item AO mutations are not
    rollback-capable after success.
  - `on_error=stop` provides fail-fast behavior, not rollback.

## Input Contracts

### `ao.task.bulk-status`
- `updates: [{ id: string, status: string }]` (required, `1..=100`)
- `on_error?: "stop" | "continue"` (default: `"stop"`)
- `project_root?: string`

Validation:
- `id` and `status` must be non-empty after trim.
- duplicate `id` values in one request are rejected.

### `ao.task.bulk-update`
- `updates: [{
    id: string,
    title?: string,
    description?: string,
    priority?: string,
    status?: string,
    assignee?: string,
    input_json?: string
  }]` (required, `1..=100`)
- `on_error?: "stop" | "continue"` (default: `"stop"`)
- `project_root?: string`

Validation:
- `id` must be non-empty after trim.
- each item must include at least one mutable field besides `id`.
- duplicate `id` values in one request are rejected.

### `ao.workflow.run-multiple`
- `runs: [{
    task_id: string,
    pipeline_id?: string,
    input_json?: string
  }]` (required, `1..=100`)
- `on_error?: "stop" | "continue"` (default: `"stop"`)
- `project_root?: string`

Validation:
- `task_id` must be non-empty after trim.

## Output Contract

Successful response payload for all three tools:
- `schema`: `ao.mcp.batch.result.v1`
- `tool`
- `on_error`
- `summary`:
  - `requested`
  - `executed`
  - `succeeded`
  - `failed`
  - `skipped`
  - `completed` (`true` when `failed == 0`)
- `results`: ordered per-item records:
  - `index`
  - `status` (`success` | `failed` | `skipped`)
  - `target_id` (`task id` or `task_id` for workflow runs)
  - `command` (AO subcommand string, for auditability)
  - `result` (CLI `data` on success, nullable otherwise)
  - `error` (CLI error payload/message on failure, nullable otherwise)
  - `exit_code` (nullable for skipped items)

Failed MCP invocation (input validation failure) returns structured error
without mutating state.

## Functional Requirements

### FR-01: Tool Registration
Register `ao.task.bulk-update`, `ao.task.bulk-status`, and
`ao.workflow.run-multiple` as discoverable MCP tools.

### FR-02: Batch Input Validation
Enforce non-empty arrays, item count bounds, per-item required fields, and
duplicate-id rules for task batch tools.

### FR-03: Deterministic Sequential Execution
Execute each item in input order and collect results in the same order.

### FR-04: Error Policy Support
Support `on_error=stop|continue` with deterministic behavior for skipped items
when stopping early.

### FR-05: Existing CLI Command Reuse
Implement batch tools by composing existing AO commands:
- `task update`
- `task status`
- `workflow run`

### FR-06: Structured Batch Result
Return `ao.mcp.batch.result.v1` payload with summary counters and ordered item
results.

### FR-07: Error Fidelity
Preserve CLI error details and exit codes per failed item where available.

### FR-08: Backward Compatibility
Keep behavior unchanged for existing non-batch MCP tools.

### FR-09: Test Coverage
Add tests for input validation, duplicate detection, policy behavior,
summary/result shaping, and regression safety.

## Acceptance Criteria
- `AC-01`: All three new MCP tools are discoverable and callable.
- `AC-02`: Empty or oversized batch inputs are rejected deterministically.
- `AC-03`: `ao.task.bulk-status` maps each item to `task status` command args.
- `AC-04`: `ao.task.bulk-update` maps each item to `task update` command args.
- `AC-05`: `ao.workflow.run-multiple` maps each item to `workflow run` args.
- `AC-06`: `on_error=stop` halts execution on first failure and marks the rest
  as skipped.
- `AC-07`: `on_error=continue` executes all items and reports all failures.
- `AC-08`: Batch response includes schema, summary counters, and ordered
  per-item records.
- `AC-09`: Existing single-item MCP tools remain behaviorally unchanged.
- `AC-10`: Focused tests cover validation, policy behavior, and result
  shaping.

## Testable Acceptance Checklist
- `T-01`: batch input normalization/validation tests (`len`, empties, duplicates).
- `T-02`: task bulk-status arg-builder tests for deterministic ordering.
- `T-03`: task bulk-update arg-builder tests with optional-field combinations.
- `T-04`: workflow run-multiple arg-builder tests with optional pipeline/input.
- `T-05`: `on_error=stop` execution model test produces failed + skipped tail.
- `T-06`: `on_error=continue` execution model test produces full result length.
- `T-07`: summary counters (`requested/executed/succeeded/failed/skipped`) are
  correct for mixed outcomes.
- `T-08`: non-batch MCP regression test confirms existing tools unchanged.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-05 | `ops_mcp.rs` tool registration and arg-builder tests |
| FR-02, FR-04 | validation + policy tests in `ops_mcp.rs` |
| FR-03, FR-06, FR-07 | deterministic batch-result shaping tests |
| FR-08 | targeted regression assertions for existing single-item tools |
| FR-09 | focused `cargo test -p orchestrator-cli services::operations::ops_mcp` |

## Implementation Notes Input (Next Phase)
Primary target:
- `crates/orchestrator-cli/src/services/operations/ops_mcp.rs`

Likely touched areas:
- new batch input structs and policy enum,
- per-item command arg builders for task/workflow batch tools,
- shared batch execution and result-shaping helper,
- MCP tool handler registrations,
- `#[cfg(test)]` coverage for validation and policy semantics.

## Deterministic Deliverables for Implementation Phase
- Three new MCP batch tools with bounded, deterministic request/response
  behavior.
- Shared batch result schema with explicit summary and per-item outcomes.
- Focused tests proving validation, ordering, and error-policy semantics.

