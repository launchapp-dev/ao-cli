# TASK-025 Implementation Notes: Sandbox and Tool Policy Enforcement

## Phase
- Workflow phase: `requirements`
- Workflow ID: `0bf4ff68-1b69-4090-ba23-a327b193a0de`
- Requirement: `REQ-025`
- Task: `TASK-025`
- Updated: `2026-02-26`

## Purpose
Translate `TASK-025` requirements into implementation slices from the current
code baseline, without broad behavioral drift outside runner/daemon execution
safety.

## Baseline Status Snapshot
Implemented baseline in this repo:
- Core execution policy model, precedence resolution, and stable hashing:
  - `crates/orchestrator-core/src/execution_policy.rs`
- Agent/phase/task policy fields wired in config/types:
  - `crates/orchestrator-core/src/agent_runtime_config.rs`
  - `crates/orchestrator-core/src/types.rs`
- Daemon phase resolution and policy metadata propagation:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
- Runner enforcement for tool allow/deny, sandbox gating, and elevation audit:
  - `crates/agent-runner/src/runner/process.rs`
- Doctor policy health checks and grading:
  - `crates/orchestrator-core/src/doctor.rs`

Remaining gaps to close in TASK-025:
- Task execution policy override writes are not validated before persistence.
- Operator-facing task policy update path is effectively `--input-json` only
  and needs explicit contract clarity.
- Direct `ao agent run` policy behavior is contract-driven but not explicitly
  documented as the supported path.
- Regression coverage is incomplete across task validation, direct-run policy
  parsing, and daemon event metadata.

## Non-Negotiable Constraints
- Keep all implementation in Rust crates under `crates/`.
- Preserve `ao.cli.v1` output envelope and current exit-code semantics.
- Keep `.ao` state mutations command-driven; do not rely on manual JSON edits.
- Preserve workspace-bound execution checks in runner sandbox guard.
- Preserve current behavior for projects with no explicit policy overrides.
- Do not break runtime contracts that omit execution policy payloads.

## Remaining Change Surface

### Task policy validation
- `crates/orchestrator-core/src/services/task_impl.rs`
- `crates/orchestrator-core/src/services/task_shared.rs`
  - validate `TaskUpdateInput.execution_policy` with
    `validate_execution_policy_overrides(...)` before persistence.
  - fail closed on invalid/empty policy entries.

### Task CLI contract clarity
- `crates/orchestrator-cli/src/services/runtime/runtime_project_task/task.rs`
  - keep `--input-json` update path as canonical for task policy override in
    this task.
  - ensure help text and docs clearly show supported `execution_policy` shape.

### Direct-run policy contract hardening
- `crates/orchestrator-cli/src/shared/runner.rs`
  - keep default behavior unchanged when policy is absent.
  - explicitly support both runtime payload paths for explicit policy:
    - `runtime_contract.policy.execution`
    - `runtime_contract.execution_policy`

### Runner/daemon integration hardening
- `crates/agent-runner/src/runner/process.rs`
  - retain deterministic deny-over-allow and single-use elevation semantics.
  - ensure audit file writes stay atomic.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
  - preserve policy metadata in phase events and elevation-required signaling.

### Doctor diagnostics
- `crates/orchestrator-core/src/doctor.rs`
  - preserve policy check names and hard-fail classification.

## Execution Sequence
1. Add task update validation for execution policy overrides.
2. Add/adjust unit tests for task policy validation behavior.
3. Add direct-run tests for explicit policy payload path compatibility.
4. Add daemon integration assertions for policy metadata/elevation-required
   phase events.
5. Add doctor regression assertions for policy checks and grading.
6. Update docs/help examples for task override and direct-run policy usage.

## Enforcement Guidance
- Normalize identifiers before matching.
- Evaluate deny rules before allow rules.
- Fail closed on parse/shape errors.
- Keep policy violation errors machine-parsable:
  - `POLICY_VIOLATION`
  - `ELEVATION_REQUIRED`

## Elevation Guidance
- Elevation approvals must bind to:
  - run identity
  - workflow/task/phase identity (when present)
  - requested action (tool/sandbox change)
  - resolved policy hash
- Approved elevation must be consumed once and produce an outcome record.

## Suggested Data Contracts

### Task update payload (`task update --input-json`)
```json
{
  "execution_policy": {
    "sandbox_mode": "read_only",
    "allow_prefixes": ["ao."],
    "allow_exact": ["phase_transition"],
    "deny_prefixes": ["bash"],
    "deny_exact": ["ao.git.push"],
    "allow_elevated": true
  }
}
```

### Resolved policy payload (runtime contract)
- `sandbox_mode`
- `tool_policy.allow_prefixes`
- `tool_policy.allow_exact`
- `tool_policy.deny_prefixes`
- `tool_policy.deny_exact`
- `policy_hash`
- `policy_sources`

### Elevation record
- `id`
- `run_id`
- `workflow_id`
- `task_id`
- `phase_id`
- `agent_id`
- `policy_hash`
- `requested_action`
- `requested_sandbox_mode`
- `reason`
- `requested_at`
- `approved`
- `approved_by`
- `approved_at`
- `outcome`

## Testing Plan

### Core/config tests
- execution policy hash and precedence resolution remain deterministic.
- invalid override entries fail validation.

### Runner tests
- deny-over-allow semantics are enforced.
- unknown tool names fail closed.
- sandbox mode violations block launch.
- elevation required path emits deterministic error payload.
- approved elevation allows one matching operation only.

### CLI/daemon integration tests
- daemon phase events include policy metadata/signals.
- phase execution metadata artifacts include resolved policy hash.
- elevation records persist request/approval/outcome fields.
- direct-run policy parsing supports both payload paths.
- doctor output includes new policy checks and correct health grading.

### Regression tests
- existing MCP-only allow-prefix behavior still works when no denylist is set.
- command mode `tools_allowlist` behavior remains unchanged.
- workspace guard behavior remains unchanged.

## Risks and Mitigations
- Risk: policy schema churn breaks existing config files.
  - Mitigation: keep new fields optional with stable defaults and clear errors.
- Risk: inconsistent enforcement between direct run and daemon phases.
  - Mitigation: share one policy resolution utility and one runner enforcement
    path.
- Risk: hidden task policy write failures due invalid payload shape.
  - Mitigation: validate overrides before persistence and test error paths.
- Risk: elevation token replay/mismatch.
  - Mitigation: operation-bound, hash-bound, single-use approvals with strict
    verification.
- Risk: doctor noise from optional paths.
  - Mitigation: classify optional-path issues as degraded warnings, not hard
    failures.
