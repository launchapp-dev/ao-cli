# TASK-041 Requirements: Split `cli_types.rs` and `web_api_service.rs` into Focused Modules

## Phase
- Workflow phase: `requirements`
- Workflow ID: `10a726b0-a373-4eee-9514-5486c43c6fed`
- Task: `TASK-041`
- Requirement: unlinked in current task metadata

## Objective
Replace two large monolith files with command/resource-focused module trees while
preserving all current CLI and web API behavior:
- `crates/orchestrator-cli/src/cli_types.rs` (currently 2882 LOC)
- `crates/orchestrator-web-api/src/services/web_api_service.rs` (currently 1813 LOC)

The implementation must keep command/route contracts stable and improve
maintainability by reducing single-file churn.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| CLI type definitions | `crates/orchestrator-cli/src/cli_types.rs` | all top-level command enums/args live in one file | high merge conflict risk, slow reviewability, difficult ownership boundaries |
| CLI command-group typing | `cli_types.rs` | runtime, planning, review, git, model, runner, web, and setup/doctor/tui args are interleaved | command-group boundaries are implicit and hard to navigate |
| CLI consumer imports | `crates/orchestrator-cli/src/services/**` | many modules import directly from `crate::cli_types::{...}` | refactor must preserve import compatibility or update all call sites deterministically |
| Web API service handlers | `crates/orchestrator-web-api/src/services/web_api_service.rs` | all resource handlers (`daemon`, `projects`, `requirements`, `tasks`, `workflows`, `reviews`) are in one impl block | no per-resource isolation; harder to test/maintain by domain |
| Web API request/parse helpers | `web_api_service.rs` | request DTOs, normalization helpers, parse helpers, event-log readers in same file | high cognitive load and mixed concerns |
| Web server dependency surface | `crates/orchestrator-web-server/src/services/web_server.rs` | calls `WebApiService` methods directly by name | split must keep method signatures and response shape stable |

## Scope
In scope for implementation after this requirements phase:
- split `cli_types.rs` into focused modules by command group (for example:
  `daemon_types`, `agent_types`, `task_types`, `workflow_types`, and peers).
- split `web_api_service.rs` into focused per-resource handler modules plus
  shared helper modules.
- preserve external behavior:
  - command names, flags, defaults, and clap help semantics,
  - `WebApiService` method signatures used by web-server routes,
  - response payload shapes and error classification behavior.
- add focused regression checks that detect behavior drift during structural
  refactor.

Out of scope:
- adding new CLI commands, removing commands, or changing flag contracts.
- adding new web API routes or changing route wiring.
- functional rewrites of planning/task/workflow domain logic.
- direct manual edits to `.ao/*.json`.

## Constraints
- Keep changes Rust-only under `crates/`.
- Keep `mod cli_types;` and `pub use services::WebApiService;` entrypoints valid.
- Preserve existing import ergonomics for `crate::cli_types::{...}` via stable
  re-exports from `cli_types` root module.
- Preserve all existing request validation and error envelopes.
- Keep refactor deterministic:
  - move code by domain with minimal semantic edits,
  - avoid behavior changes mixed with structural moves.
- No direct `.ao` state-file mutation.

## Functional Requirements

### FR-01: CLI Types Module Tree
- Replace monolithic `crates/orchestrator-cli/src/cli_types.rs` with a module
  tree rooted at `crates/orchestrator-cli/src/cli_types/mod.rs`.
- Module tree must include dedicated command-group modules for at least:
  - `daemon_types`
  - `agent_types`
  - `project_types`
  - `task_types`
  - `workflow_types`
  - `vision_types`
  - `requirements_types`
  - `architecture_types`
  - `execute_types`
  - `planning_types`
  - `review_types`
  - `qa_types`
  - `history_types`
  - `errors_types`
  - `task_control_types`
  - `git_types`
  - `skill_types`
  - `model_types`
  - `runner_types`
  - `output_types`
  - `mcp_types`
  - `web_types`
  - `setup_types`
  - `doctor_types`
  - `tui_types`

### FR-02: CLI Shared Type/Utility Segregation
- Shared constants and parsing helpers currently in `cli_types.rs`
  (for example positive-number parsers and shared help text constants) must be
  moved into explicit shared modules and imported where used.
- Top-level `Cli` and root `Command` definitions remain easy to discover from
  the root `cli_types` module.

### FR-03: CLI Compatibility Contract
- Existing service/runtime modules that import from `crate::cli_types::{...}`
  must continue to compile without semantic behavior changes.
- If re-export strategy is used, names and visibility remain unchanged from the
  current consumer perspective.

### FR-04: CLI Behavior Invariance
- `ao --help` and representative subgroup help for split command groups must
  preserve command/flag names, defaults, required/optional behavior, and value
  parsing semantics.
- Existing aliases and accepted value spellings remain unchanged.

### FR-05: Web API Service Module Tree
- Replace monolithic
  `crates/orchestrator-web-api/src/services/web_api_service.rs` with a module
  tree rooted at `crates/orchestrator-web-api/src/services/web_api_service/mod.rs`.
- The tree must separate handlers by resource domain, including at least:
  - `system_handlers`
  - `daemon_handlers`
  - `projects_handlers`
  - `requirements_handlers`
  - `vision_handlers`
  - `tasks_handlers`
  - `workflows_handlers`
  - `reviews_handlers`
- Request DTOs and parse/normalization utilities must be separated from handler
  implementations into dedicated helper modules.

### FR-06: Web API Compatibility Contract
- Public `WebApiService` constructor and method signatures used by
  `orchestrator-web-server` route handlers remain unchanged.
- Returned JSON structure and error code semantics remain backward compatible.

### FR-07: Event/Log Helper Stability
- Event publishing/subscription (`subscribe_events`, `read_events_since`,
  sequence generation, daemon log-file parsing) remains behaviorally equivalent
  after modularization.
- Filtering by `project_root` and deterministic sequence ordering is preserved.

### FR-08: Deterministic Refactor Discipline
- Structural moves and any required visibility/import fixes must be isolated
  from unrelated logic changes.
- Any unavoidable behavior change must be explicitly documented in
  implementation notes before merge.

### FR-09: Regression Coverage
- Add/adjust targeted tests and compile checks that validate:
  - CLI parsing/help invariance for representative split groups.
  - web-server to web-api integration calls still compile and execute expected
    request/response flows.
  - event read/publish helpers preserve sequence ordering and filtering.

### FR-10: Documentation Traceability
- Track this modularization work in task docs and cross-reference known-issue
  guidance so future contributors can find the split rationale and scope.

## Acceptance Criteria
- `AC-01`: `cli_types` no longer exists as a single monolith file and is
  replaced by a command-group module tree.
- `AC-02`: root `cli_types` exports remain compatible with existing call sites
  (or are migrated deterministically with no behavior drift).
- `AC-03`: `web_api_service` no longer exists as a single monolith file and is
  replaced by per-resource handler modules plus shared helpers.
- `AC-04`: `WebApiService` external method surface used by web-server routes is
  preserved.
- `AC-05`: representative CLI help/parsing behavior remains unchanged.
- `AC-06`: representative web API response/error behavior remains unchanged.
- `AC-07`: event subscription/read semantics remain deterministic and unchanged.
- `AC-08`: targeted validation (`cargo check`/tests for touched crates) passes.
- `AC-09`: no direct edits are made to `.ao/*.json`.

## Testable Acceptance Checklist
- `T-01`: `cargo check -p orchestrator-cli` passes after `cli_types` split.
- `T-02`: `cargo check -p orchestrator-web-api -p orchestrator-web-server`
  passes after `web_api_service` split.
- `T-03`: representative CLI smoke/help coverage remains green (top-level plus
  split command groups such as daemon/agent/task/workflow/git).
- `T-04`: web-server route calls to `WebApiService` compile and keep expected
  JSON/error behavior.
- `T-05`: event helper behavior tests (or equivalent assertions) confirm
  deterministic sequence/project filtering.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-02, FR-03 | code inspection + `cargo check -p orchestrator-cli` |
| FR-04 | CLI smoke/help tests for representative command groups |
| FR-05, FR-06 | `cargo check` across web-api/web-server + route-level checks |
| FR-07 | focused event helper tests/assertions |
| FR-08 | PR diff review for structural-only movement discipline |
| FR-09 | targeted test runs for touched crates |
| FR-10 | docs update in TASK-041 requirement/implementation notes and `CLAUDE.md` |

## Implementation Notes Input (Next Phase)
Primary source targets:
- `crates/orchestrator-cli/src/cli_types.rs` -> `crates/orchestrator-cli/src/cli_types/`
- `crates/orchestrator-cli/src/main.rs` (module path continuity only)
- `crates/orchestrator-cli/src/services/**` (import path compatibility verification)
- `crates/orchestrator-web-api/src/services/web_api_service.rs` ->
  `crates/orchestrator-web-api/src/services/web_api_service/`
- `crates/orchestrator-web-api/src/services/mod.rs`
- `crates/orchestrator-web-server/src/services/web_server.rs` (compile-time API compatibility)

Likely test targets:
- `crates/orchestrator-cli/tests/cli_smoke.rs`
- `crates/orchestrator-cli/tests/cli_e2e.rs` (if command coverage touches split groups)
- `crates/orchestrator-web-server` route/service tests (existing or newly added focused tests)

## Deterministic Deliverables for Implementation Phase
- `cli_types` command-group module decomposition with stable re-export surface.
- `web_api_service` per-resource handler decomposition with stable service API.
- Focused regression checks demonstrating no CLI/web behavior drift.
