# TASK-040 Requirements: Align Structured-Output Test with Runtime Semantics

## Phase
- Workflow phase: `requirements`
- Workflow ID: `e36089f0-be43-4548-a937-0dc3b0bd90f6`
- Task: `TASK-040`

## Objective
Resolve the pre-existing failing test in
`orchestrator-core/src/agent_runtime_config.rs` by aligning assertions with the
current structured-output semantics used by runtime phase execution.

## Current Baseline Audit

| Surface | Current location | Current state | Gap |
| --- | --- | --- | --- |
| Structured-output classifier | `crates/orchestrator-core/src/agent_runtime_config.rs` (`is_structured_output_phase`) | returns `true` when a phase has `output_contract` or `output_json_schema` | behavior is correct for current config |
| Built-in `implementation` phase | `crates/orchestrator-core/src/agent_runtime_config.rs` and `crates/orchestrator-core/config/agent-runtime-config.v2.json` | defines both `output_contract` and `output_json_schema` | phase is structured-output by definition |
| Runtime enforcement path | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs` | consumes contract/schema for policy validation | changing semantics would affect runtime behavior |
| Failing test | `crates/orchestrator-core/src/agent_runtime_config.rs` (`builtin_defaults_mark_review_as_structured_output`) | asserts `implementation` is *not* structured-output | assertion contradicts current semantics and fails |

## Problem Statement
The failing test is stale. It expects `implementation` to be non-structured,
while current built-in runtime configuration and classifier logic both treat
`implementation` as structured-output.

## Decision for Implementation
Use test-alignment, not runtime-semantic change:
- Keep structured-output semantics contract-first (`output_contract` /
  `output_json_schema` imply structured-output).
- Update the failing test to assert `implementation` is structured-output.
- Preserve one non-structured negative control assertion (for example,
  `testing`) to keep coverage for the false path.

## Scope
In scope for implementation after this requirements phase:
- Update `builtin_defaults_mark_review_as_structured_output` assertions in
  `agent_runtime_config.rs`.
- Keep/introduce a negative control assertion for a phase without contract/schema.
- Run targeted `orchestrator-core` tests for this behavior.

Out of scope:
- Changing `is_structured_output_phase` semantics.
- Removing `implementation` output contract/schema from built-in config.
- Runtime daemon scheduler behavior changes.
- Manual edits to `/.ao/*.json`.

## Constraints
- Preserve current built-in runtime behavior for all workflow phases.
- Keep changes minimal and deterministic; avoid unrelated refactors.
- Ensure assertions reflect both hardcoded fallback and checked-in built-in JSON semantics.
- Do not introduce schema/version changes in agent runtime config.

## Functional Requirements

### FR-01: Implementation Structured-Output Parity
- Tests must assert that `implementation` is structured-output under built-in
  defaults.

### FR-02: Review/Audit Heuristic Coverage Retention
- Tests must continue asserting a known review phase (for example,
  `code-review`) is structured-output.

### FR-03: Negative Control Coverage
- Tests must include at least one phase without output contract/schema (for
  example, `testing`) and assert it is not structured-output.

### FR-04: Runtime Behavior Stability
- No production logic changes are required to satisfy this task; only stale
  test expectations should be corrected.

### FR-05: Regression Validation
- Targeted `orchestrator-core` tests for `agent_runtime_config` must pass after
  the update.

## Acceptance Criteria
- `AC-01`: `builtin_defaults_mark_review_as_structured_output` passes and
  asserts `code-review == true`, `implementation == true`, and `testing == false`.
- `AC-02`: `is_structured_output_phase` implementation remains unchanged in
  behavior for contract/schema-based phases.
- `AC-03`: No changes are made to built-in `implementation` contract/schema
  definitions.
- `AC-04`: Targeted `orchestrator-core` test execution shows no regressions in
  `agent_runtime_config` tests.

## Testable Acceptance Checklist
- `T-01`: `cargo test -p orchestrator-core builtin_defaults_mark_review_as_structured_output -- --nocapture`
- `T-02`: `cargo test -p orchestrator-core agent_runtime_config::tests -- --nocapture`

## Verification Matrix
| Requirement area | Verification method |
| --- | --- |
| Test/runtime semantic parity | updated unit assertions in `agent_runtime_config` tests |
| Structured-output true path | `implementation` + `code-review` assertions |
| Structured-output false path | `testing` assertion |
| Regression safety | targeted `agent_runtime_config::tests` run |

## Implementation Notes (Input to Next Phase)
Primary expected change target:
- `crates/orchestrator-core/src/agent_runtime_config.rs`
  - update test `builtin_defaults_mark_review_as_structured_output` to assert
    `implementation` is structured-output
  - include/retain one non-structured phase assertion (`testing`)

No runtime production-code changes are expected unless implementation uncovers
new contradictory evidence.

## Deterministic Deliverables for Implementation Phase
- Updated `agent_runtime_config` unit test expectations aligned with current
  built-in semantics.
- Negative-control assertion preserving false-path coverage.
- Passing targeted tests for the corrected test scope.
