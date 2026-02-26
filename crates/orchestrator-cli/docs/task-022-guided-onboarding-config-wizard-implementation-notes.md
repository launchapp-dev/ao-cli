# TASK-022 Implementation Notes: Guided Onboarding and Config Wizard

## Purpose
Translate TASK-022 requirements into an implementation plan that preserves AO's
repository safety and output contracts while introducing onboarding and
doctor-based remediation.

## Non-Negotiable Constraints
- Keep changes Rust-only in workspace crates.
- Keep `.ao` mutations behind AO APIs (no ad-hoc JSON edits in handlers).
- Preserve `ao.cli.v1` envelope and exit-code classification behavior.
- Keep unrelated command behavior unchanged.

## Proposed Change Surface

### CLI command and dispatch wiring
- `crates/orchestrator-cli/src/cli_types.rs`
  - add onboarding command args and non-interactive flags.
  - extend `Doctor` command args to support remediation mode.
- `crates/orchestrator-cli/src/main.rs`
  - route onboarding and doctor commands through dedicated operation handlers.
- `crates/orchestrator-cli/src/services/operations.rs`
  - register a new onboarding operations module (and optionally split doctor out
    of planning module).

### Onboarding/doctor operations layer
- `crates/orchestrator-cli/src/services/operations/ops_onboarding.rs` (new)
  - implement setup plan/apply flow.
  - implement non-interactive validation contract.
  - normalize onboarding output payloads for JSON/non-JSON modes.
- `crates/orchestrator-cli/src/services/operations/ops_doctor.rs` (new or
  extracted)
  - keep `ao doctor` read-only mode.
  - implement remediation mode with deterministic result reporting.
- `crates/orchestrator-cli/src/services/operations/ops_planning/mod.rs`
  - remove migrated doctor handling to avoid cross-domain coupling.

### Core diagnostics and config APIs
- `crates/orchestrator-core/src/doctor.rs`
  - extend check catalog and remediation metadata.
  - add execution path for safe, local remediation actions.
- `crates/orchestrator-core/src/lib.rs`
  - export onboarding/doctor/config helper APIs used by CLI handlers.
- `crates/orchestrator-core/src/config/` or `services/` additions (new files)
  - typed load/update/save helpers for onboarding-owned config surfaces.
  - guarantee atomic writes and path safety.

### Daemon config alignment
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs`
  - replace direct `.ao/pm-config.json` `std::fs` writes with typed config API.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs`
  - replace raw JSON parse of `pm-config` with shared typed loader.

## Implementation Sequence
1. Add CLI types and command wiring for onboarding + doctor remediation.
2. Add core typed config APIs for onboarding-owned config domains.
3. Migrate daemon config write/read paths to typed APIs.
4. Implement onboarding plan/apply logic with non-interactive validation.
5. Expand doctor checks and remediation execution.
6. Normalize output payloads and failure messages.
7. Add tests for acceptance criteria and regressions.
8. Run `cargo check` + targeted CLI tests.

## Onboarding Payload Guidance
Keep payload keys stable for deterministic automation:
- `mode`: `guided|non_interactive`
- `dry_run`: boolean
- `doctor`: check summary
- `planned_changes`: ordered list of config actions
- `applied_changes`: ordered list for apply mode
- `unchanged`: ordered list of no-op domains
- `blocked`: actionable blockers requiring operator intervention
- `next_step`: concrete command guidance

## Doctor Remediation Payload Guidance
Recommended remediation result shape:
- `check_id`
- `status_before`
- `status_after`
- `action`: `applied|skipped|failed|manual_required`
- `details`
- `error` (optional)

## Testing Plan
- Add/extend tests in:
  - `crates/orchestrator-cli/tests/cli_smoke.rs`
  - `crates/orchestrator-cli/tests/cli_e2e.rs`
  - targeted unit tests under `crates/orchestrator-core/src/*` for doctor/config helpers
- Minimum test matrix:
  - onboarding guided/non-interactive mode routing,
  - non-interactive missing-input validation failure,
  - deterministic plan/apply payload keys,
  - doctor remediation execution/reporting,
  - daemon config persistence compatibility,
  - idempotent re-run semantics.

## Risks and Mitigations
- Risk: onboarding introduces command-surface ambiguity.
  - Mitigation: explicit mode flags and stable output schema.
- Risk: remediation mutates configs unexpectedly.
  - Mitigation: plan-first model + explicit apply + per-action reporting.
- Risk: duplicated config parsers drift over time.
  - Mitigation: single typed API for read/write paths used by CLI + scheduler.
- Risk: regressions in existing daemon automation behavior.
  - Mitigation: regression tests around current `pm-config` behavior and defaults.

