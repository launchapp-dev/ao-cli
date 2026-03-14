# TASK-041 Implementation Notes: `cli_types` and `web_api_service` Modularization

## Purpose
Translate TASK-041 requirements into a low-risk, deterministic implementation
plan that separates command/resource concerns without changing CLI or web API
behavior.

## Non-Negotiable Constraints
- Structural refactor only for this task scope: no intentional command or API
  behavior changes.
- Preserve `crate::cli_types::{...}` consumer ergonomics via root re-exports.
- Preserve `WebApiService` public method signatures used by web-server routes.
- Keep changes Rust-only under `crates/`.
- No direct edits to `.ao/*.json`.

## Proposed Change Surface

### CLI type definitions (`orchestrator-cli`)
- Replace `crates/orchestrator-cli/src/cli_types.rs` with
  `crates/orchestrator-cli/src/cli_types/mod.rs`.
- Split definitions into command-group files and re-export from root module.
- Keep root module as the stable import surface for existing services/runtime.

### Web API service handlers (`orchestrator-web-api`)
- Replace `crates/orchestrator-web-api/src/services/web_api_service.rs` with
  `crates/orchestrator-web-api/src/services/web_api_service/mod.rs`.
- Move handlers into per-resource `impl WebApiService` modules.
- Move request DTOs and parse/normalization helpers into focused helper modules.

## Proposed File Layout

### `orchestrator-cli`
```text
crates/orchestrator-cli/src/cli_types/
  mod.rs
  root_types.rs
  shared_types.rs
  daemon_types.rs
  agent_types.rs
  project_types.rs
  task_types.rs
  workflow_types.rs
  vision_types.rs
  requirements_types.rs
  architecture_types.rs
  execute_types.rs
  planning_types.rs
  review_types.rs
  qa_types.rs
  history_types.rs
  errors_types.rs
  task_control_types.rs
  git_types.rs
  skill_types.rs
  model_types.rs
  runner_types.rs
  output_types.rs
  mcp_types.rs
  web_types.rs
  setup_types.rs
  doctor_types.rs
  tui_types.rs
```

### `orchestrator-web-api`
```text
crates/orchestrator-web-api/src/services/web_api_service/
  mod.rs
  system_handlers.rs
  daemon_handlers.rs
  projects_handlers.rs
  requirements_handlers.rs
  vision_handlers.rs
  tasks_handlers.rs
  workflows_handlers.rs
  reviews_handlers.rs
  requests.rs
  parsing.rs
  event_stream.rs
```

## Migration Strategy

### Stage 1: `cli_types` mechanical split
1. Create `cli_types/` directory module with root `mod.rs`.
2. Move top-level `Cli` and `Command` to `root_types.rs`.
3. Move each command-group enum/args to matching `*_types.rs`.
4. Move shared constants/parsers (`parse_positive_u64`, help-text constants, and
   shared value enums) to `shared_types.rs`.
5. Re-export all externally consumed types from `cli_types/mod.rs`.
6. Run `cargo check -p orchestrator-cli`; fix only import/module errors.

### Stage 2: `web_api_service` mechanical split
1. Create `web_api_service/` directory module with root `mod.rs` containing:
   - `WebApiService` struct,
   - constructor,
   - shared event publisher/subscriber glue.
2. Move handler methods into per-resource files by endpoint domain.
3. Move request DTO structs to `requests.rs`.
4. Move parse/normalize helpers and enum/string parsers to `parsing.rs`.
5. Move event-log file parsing (`read_events_for_project`, sequence helpers) to
   `event_stream.rs`.
6. Run `cargo check -p orchestrator-web-api -p orchestrator-web-server`; fix
   only module/import visibility breakages.

### Stage 3: Regression confirmation
1. Run representative CLI help/smoke checks for split command groups.
2. Run targeted tests in touched crates.
3. Verify no intentional command flag or route response differences.

## Import and Visibility Guidance
- Keep `pub(crate)` visibility unchanged unless a moved item now requires
  module-level visibility adjustment.
- Prefer `pub(crate) use` re-exports from root modules over broad `pub use`.
- Avoid introducing wildcard imports in split files; keep imports explicit.

## Determinism and Safety Guidance
- Keep each commit/slice mechanical and reviewable.
- Do not combine refactor moves with style-only rewrites across unrelated code.
- Preserve method order in each resource module where practical to simplify diff
  review against existing route handlers.
- Keep serialization/validation logic byte-for-byte equivalent unless a compile
  fix requires minimal adjustment.

## Testing Plan
- Compile gates:
  - `cargo check -p orchestrator-cli`
  - `cargo check -p orchestrator-web-api -p orchestrator-web-server`
- Targeted regression checks:
  - `cargo test -p orchestrator-cli --test cli_smoke` (or equivalent help/smoke coverage)
  - additional touched-module tests for web API/service behavior as available

## Risks and Mitigations
- Risk: import breakages across many CLI service modules.
  - Mitigation: stable re-export surface in `cli_types/mod.rs` and incremental
    compile checks after each move.
- Risk: accidental behavior drift during handler movement.
  - Mitigation: mechanical extraction first, no logic edits, then focused tests.
- Risk: hidden coupling of helper functions across resources.
  - Mitigation: centralize shared helpers (`parsing.rs`, `event_stream.rs`) and
    keep resource modules thin.
- Risk: large diff becomes hard to review.
  - Mitigation: stage by file tree, preserve names/signatures/order where
    possible, and validate at each stage.
