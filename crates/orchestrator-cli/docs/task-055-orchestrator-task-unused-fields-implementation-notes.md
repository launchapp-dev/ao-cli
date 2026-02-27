# TASK-055 Implementation Notes: Expose OrchestratorTask Estimation Fields

## Purpose
Translate the requirements decision into a deterministic implementation plan
that exposes currently hard-defaulted task fields in CLI/API create-update
flows, without changing the persisted `OrchestratorTask` schema.

## Chosen Strategy
- Keep `OrchestratorTask` fields in place.
- Expand task mutation inputs and command surfaces to make those fields
  operator-settable.
- Preserve defaults and compatibility when fields are omitted.

This avoids schema churn while enabling agent-assisted estimation metadata.

## Non-Negotiable Constraints
- No manual edits to `/.ao/*.json`.
- No removal of existing `OrchestratorTask` fields in this task.
- Keep deterministic parsing and explicit validation errors.
- Preserve behavior when new flags/payload fields are not provided.

## Proposed Change Surface

### 1) Core Types and Update Plumbing
- `crates/orchestrator-core/src/types.rs`
  - extend `TaskCreateInput` with optional:
    - `risk`, `scope`, `complexity`, `impact_area`, `estimated_effort`,
      `resource_requirements`.
  - extend `TaskUpdateInput` with optional counterparts to support mutation.
- `crates/orchestrator-core/src/services/task_impl.rs`
  - wire new create-input fields into constructed `OrchestratorTask` with
    legacy defaults when omitted.
- `crates/orchestrator-core/src/services/task_shared.rs`
  - extend `apply_task_update` to mutate the new update fields.
  - keep trimming/empty handling deterministic for `estimated_effort`.

### 2) CLI Task Command Surface
- `crates/orchestrator-cli/src/cli_types/task_types.rs`
  - add new `task create` and `task update` args for:
    - `risk`, `scope`, `complexity`, repeated `impact-area`,
      `estimated-effort`, and resource requirement inputs.
  - add `--risk` filter to `task list`.
- `crates/orchestrator-cli/src/shared/parsing.rs`
  - add parsing helpers for risk/scope/complexity/impact-area values.
  - keep normalization and error messaging aligned with existing parse helpers.
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
  - map new args into `TaskCreateInput`/`TaskUpdateInput`.
  - route risk filter into `TaskFilter`.

### 3) Web API Task Parity
- `crates/orchestrator-web-api/src/services/web_api_service/requests.rs`
  - add fields to `TaskCreateRequest` and `TaskPatchRequest`.
- `crates/orchestrator-web-api/src/services/web_api_service/parsing.rs`
  - add parse helpers for scope/complexity/impact-area and resource limits.
  - enforce numeric bounds validation for resource fields.
- `crates/orchestrator-web-api/src/services/web_api_service/tasks_handlers.rs`
  - map parsed request fields into core create/update inputs.

## Implementation Sequence
1. Extend core input structs and apply/create logic.
2. Add CLI arg definitions and parsing helpers.
3. Wire CLI runtime handlers for create/update/list.
4. Add web-api request/parsing/handler parity.
5. Add or update tests across core/cli/web-api.

## Testing Plan
- `cargo test -p orchestrator-core`
- `cargo test -p orchestrator-cli`
- `cargo test -p orchestrator-web-api`

Targeted checks:
- create/update persistence for all six fields,
- CLI parse failures for invalid enums/resource values,
- risk filter behavior,
- unchanged defaults when fields are omitted.

## Risks and Mitigations
- Risk: update semantics become ambiguous for partial resource updates.
  - Mitigation: define explicit merge semantics in `TaskUpdateInput` and test
    each field independently.
- Risk: broadened CLI surface introduces inconsistent parsing behavior.
  - Mitigation: centralize enum parsing in `shared/parsing.rs` and add negative
    tests.
- Risk: behavior drift in existing task workflows.
  - Mitigation: keep defaults unchanged and add regression assertions for
    legacy create/update paths.

## Expected Outcome
- Operators and automation can set and evolve task estimation metadata through
  first-class CLI/API paths.
- `OrchestratorTask` schema stays stable.
- Existing behavior remains unchanged when new inputs are not used.
