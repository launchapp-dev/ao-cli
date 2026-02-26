# TASK-022 Requirements: Guided Onboarding and Configuration Wizard

## Phase
- Workflow phase: `requirements`
- Workflow ID: `d8efdd60-29a4-4c22-b04b-ffbc09c47f09`
- Task: `TASK-022`

## Objective
Deliver a production-ready AO onboarding flow that supports:
- guided setup for first-run operators,
- deterministic non-interactive setup for CI/automation,
- doctor-driven diagnostics and remediation,
- API-only configuration writes (no direct ad-hoc file mutation from command handlers).

## Existing Baseline Audit

| Surface | Current path | Current behavior | Gap for TASK-022 |
| --- | --- | --- | --- |
| Doctor command wiring | `crates/orchestrator-cli/src/main.rs`, `crates/orchestrator-cli/src/services/operations/ops_planning/mod.rs` | `ao doctor` returns a basic report plus daemon health | no remediation contract, no fix execution path |
| Core doctor checks | `crates/orchestrator-core/src/doctor.rs` | only `cwd_resolvable` and `project_root_env` checks | insufficient setup diagnostics for onboarding |
| Daemon config writes | `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs` | direct `std::fs` write to `.ao/pm-config.json` | violates API-only config-write goal |
| Daemon config reads | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs` | direct parse of `.ao/pm-config.json` | duplicated parsing logic, no typed config boundary |
| Config API precedent | `crates/orchestrator-cli/src/services/operations/ops_workflow.rs` + `orchestrator_core::write_*` APIs | workflow/runtime config writes go through core APIs | onboarding/daemon config should align with this pattern |

## Scope
In scope for implementation after this phase:
- Add a dedicated onboarding command flow (recommended surface: `ao setup`) with:
  - guided mode for interactive terminals,
  - non-interactive mode for automation.
- Define and implement doctor remediation semantics:
  - read-only diagnostics mode,
  - explicit remediation apply mode for safe, deterministic fixes.
- Standardize config writes touched by onboarding through typed API boundaries in `orchestrator-core`.
- Remove direct config file writes from CLI command handlers in onboarding-related paths.
- Add acceptance tests for interactive guardrails, non-interactive determinism, remediation behavior, and config persistence semantics.

Out of scope for TASK-022:
- Web UI onboarding flows.
- Desktop wrapper integrations.
- Manual editing of `.ao/*.json` state/config files.
- Auto-installing external CLIs/package managers as part of remediation.

## Constraints
- Preserve `ao.cli.v1` output envelope behavior for `--json`.
- Preserve current exit code semantics (`2/3/4/5/1`) and classify validation failures as invalid input.
- Preserve project root resolution precedence and repository safety rules.
- Keep remediation deterministic and explicit; no hidden side effects.
- Keep writes atomic and project-root scoped.
- Do not expose secrets (API keys/tokens) in output payloads or logs.

## Functional Requirements

### FR-01: Onboarding Command Modes
- Add onboarding entrypoint with two modes:
  - guided interactive mode (TTY-driven),
  - non-interactive mode (`--non-interactive`) with explicit inputs.
- Non-interactive mode must never prompt.
- Guided mode must have deterministic step ordering and explicit completion summary.

### FR-02: Setup Plan + Apply Contract
- Onboarding must support a read-only plan stage and an apply stage.
- Plan output must include:
  - detected environment/config status,
  - required changes,
  - blocked items requiring operator action.
- Apply stage must return deterministic mutation metadata: which config domains changed and which were unchanged.

### FR-03: Doctor Diagnostics and Remediation
- Extend doctor checks beyond current baseline to cover onboarding-critical readiness:
  - project root/config bootstrap presence,
  - runner reachability,
  - model CLI/API-key availability signal,
  - key AO config validity.
- Each check must include:
  - stable check ID,
  - status (`ok|warn|fail`),
  - human-readable details,
  - remediation availability metadata.
- Add explicit remediation execution mode (for example `ao doctor --fix`) that applies only safe, local fixes.

### FR-04: API-Only Config Writes
- Onboarding and daemon-related config mutations must flow through typed API helpers in `orchestrator-core`.
- CLI handlers must not directly call `std::fs::write` for onboarding-owned config files.
- Affected config surfaces include at minimum:
  - `.ao/config.json`,
  - `.ao/pm-config.json`,
  - any onboarding-updated state/config artifacts in `.ao/state/*`.

### FR-05: Idempotence and Determinism
- Running onboarding with identical effective inputs must be idempotent.
- Re-run apply output must mark unchanged domains deterministically.
- Failed writes must not leave partially written config files.

### FR-06: Error and Output Contract
- Missing required non-interactive input must fail with invalid-input semantics and actionable next steps.
- Doctor/setup JSON payloads must be stable and testable.
- Human-readable output must remain concise and actionable.

### FR-07: Backward Compatibility
- Existing command behavior outside onboarding/doctor-remediation/config-write alignment must remain unchanged.
- Existing daemon automation behavior must remain compatible with pre-existing config values.

## Non-Functional Requirements

### NFR-01: Repository Safety
- All mutations must stay within resolved project root and AO-approved config locations.
- No destructive side effects outside explicit apply/fix operations.

### NFR-02: Performance
- Onboarding and doctor checks should complete quickly for local use and automation pipelines.
- Diagnostics should avoid unnecessary process spawning or repeated expensive probes.

### NFR-03: Testability
- All acceptance behaviors must be assertable through CLI integration tests without manual intervention.

## Acceptance Criteria
- `AC-01`: Onboarding command supports guided and non-interactive modes.
- `AC-02`: Non-interactive mode performs zero prompts and fails deterministically when required input is missing.
- `AC-03`: Onboarding exposes a read-only plan and explicit apply behavior.
- `AC-04`: Doctor output includes stable check IDs, statuses, and remediation metadata.
- `AC-05`: Doctor remediation mode applies safe fixes and reports applied/failed/skipped actions.
- `AC-06`: Onboarding-owned config writes are routed through `orchestrator-core` API helpers.
- `AC-07`: Direct ad-hoc config writes in onboarding/daemon-config paths are removed or replaced.
- `AC-08`: Writes are atomic; failed mutations do not corrupt config files.
- `AC-09`: JSON output follows `ao.cli.v1` envelope conventions.
- `AC-10`: Existing unrelated command behavior remains unchanged.
- `AC-11`: Sensitive values are redacted/withheld from onboarding/doctor output.
- `AC-12`: Regression tests cover idempotent re-run behavior.

## Testable Acceptance Checklist
- `T-01`: CLI test for guided onboarding mode selection behavior.
- `T-02`: CLI test for non-interactive missing-input failure contract.
- `T-03`: CLI test for onboarding plan output shape and stability.
- `T-04`: CLI test for onboarding apply output with changed + unchanged domains.
- `T-05`: CLI test for doctor diagnostics payload fields.
- `T-06`: CLI test for doctor remediation apply path and result reporting.
- `T-07`: Regression test that daemon automation config still drives scheduler behavior.
- `T-08`: Regression test proving no partial writes on forced write failure path.
- `T-09`: Regression test for unaffected command groups.

## Acceptance Verification Matrix
| Requirement area | Verification method |
| --- | --- |
| Guided + non-interactive onboarding | CLI integration tests for mode routing and no-prompt behavior |
| Doctor diagnostics/remediation contract | Unit + CLI tests for check payload and fix execution results |
| API-only config writes | Static/behavioral tests around helper usage and persisted files |
| Atomic persistence + idempotence | File-level regression tests and repeated-run assertions |
| Envelope/error compatibility | JSON snapshot/assertion tests with exit-code checks |

## Implementation Notes Input to Next Phase
Primary implementation targets:
- `crates/orchestrator-cli/src/cli_types.rs`
- `crates/orchestrator-cli/src/main.rs`
- `crates/orchestrator-cli/src/services/operations.rs`
- `crates/orchestrator-cli/src/services/operations/ops_planning/mod.rs` (doctor extraction/cleanup)
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs`
- `crates/orchestrator-core/src/doctor.rs`
- `crates/orchestrator-core/src/lib.rs` (new config API exports as needed)

