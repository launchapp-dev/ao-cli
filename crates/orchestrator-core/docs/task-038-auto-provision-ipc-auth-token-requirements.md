# TASK-038 Requirements: Auto-Provision IPC Auth Token in Runner Startup Path

## Phase
- Workflow phase: `requirements`
- Workflow ID: `afdcda44-c924-4707-9d76-66a252e11507`
- Task: `TASK-038`
- Task title: `Auto-provision IPC auth token in ensure_agent_runner_running`

## Objective
Define deterministic regression coverage for the daemon startup path so a fresh
runner config directory with no token still starts `agent-runner` successfully
because `ensure_agent_runner_running` auto-provisions IPC auth token material.

## Current Baseline (Implemented)

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Token generation helper | `crates/protocol/src/config.rs` (`Config::ensure_token_exists`) | creates a UUID token when `agent_runner_token` is missing/blank | helper behavior is covered in `protocol` tests only |
| Startup provisioning hook | `crates/orchestrator-core/src/services/runner_helpers.rs` (`ensure_agent_runner_running`) | calls `Config::ensure_token_exists(&config_dir)` before readiness/spawn logic | no end-to-end test proves startup succeeds from empty token state |
| Auth readiness gate | `crates/orchestrator-core/src/services/runner_helpers.rs` (`authenticate_runner_stream`, `is_agent_runner_ready`) | authentication requires token resolved from runner config/env | no test validates generated token is immediately usable for startup readiness |
| Existing runner lifecycle tests | `crates/orchestrator-core/src/services/runner_helpers.rs` (`#[cfg(test)]` module) | stale cleanup and stop/lifecycle branches covered | missing integration-style startup test for token auto-provision path |

## Problem Statement
Token auto-provisioning is implemented, but there is no regression test that
starts from a truly empty runner config and exercises the real startup path.
Without this, future refactors can break daemon startup/auth bootstrapping
without detection.

## Scope
In scope for implementation after this requirements phase:
- Add an integration-style test in `orchestrator-core` that:
  - starts from a fresh runner config dir with no token set,
  - calls `ensure_agent_runner_running`,
  - verifies runner startup succeeds,
  - verifies token is generated and persisted.
- Assert generated token is usable by existing auth-dependent readiness/status
  calls (`is_agent_runner_ready`/`query_runner_status`) in the same test flow.
- Ensure test cleanup terminates the spawned runner and removes fixture
  artifacts so repeated runs remain deterministic.
- Clarify token-rotation decision for this task and document follow-up boundary.

Out of scope for TASK-038:
- Implementing periodic token rotation or dual-token rollover logic.
- Changing IPC wire format, auth handshake schema, or token source precedence.
- Manual edits to `/.ao/*.json`.
- Unrelated daemon/workflow/CLI behavior changes.

## Constraints
- Keep tests deterministic and host-safe:
  - isolate runner config via `AO_RUNNER_CONFIG_DIR` temp directory,
  - isolate project root via temp directory,
  - avoid dependence on user/global `~/.ao` runner state.
- Environment variables used by startup/auth (`AO_RUNNER_CONFIG_DIR`,
  `AO_SKIP_RUNNER_START`, `AGENT_RUNNER_TOKEN`) must be scoped/restored per
  test to avoid cross-test contamination.
- Test must clean up spawned runner process on success and failure paths.
- Use repository-local binaries/path resolution only; do not require external
  services.
- Prefer `#[cfg(unix)]` for this integration-style startup test to avoid
  non-unix fixed-port (`127.0.0.1:9001`) collision risk in shared CI hosts.

## Functional Requirements

### FR-01: End-to-End Startup Coverage for Missing Token
- Add a regression test that invokes
  `ensure_agent_runner_running(project_root)` starting from a config directory
  where `config.json` has no `agent_runner_token`.
- The test must exercise real startup logic, not a mocked token helper path.

### FR-02: Fresh-State Preconditions Are Explicit
- Test setup must prove precondition before startup call:
  - token missing/blank in runner config,
  - no `AGENT_RUNNER_TOKEN` override in environment.

### FR-03: Token Auto-Provision Is Asserted
- After successful startup, runner config must contain non-empty
  `agent_runner_token`.
- Token value should be format-validated as UUID to match current generation
  behavior.

### FR-04: Generated Token Enables Authenticated Readiness
- Test must verify readiness/status path succeeds after startup using generated
  token (for example `query_runner_status(config_dir).is_some()`).

### FR-05: Startup Success and Cleanup Are Verified
- `ensure_agent_runner_running` returns a runner PID (`Some(pid)`) when startup
  is not skipped.
- Test must stop the spawned runner and verify no fixture-owned runner process
  remains alive.

### FR-06: Token Stability Across Immediate Reuse
- A second startup call in the same isolated config should preserve the existing
  token value (no rotation on each startup call).

### FR-07: Token Rotation Decision for TASK-038
- Security review for this task must explicitly decide periodic rotation scope.
- Decision for TASK-038: no periodic rotation implementation in this task.
- Rationale: safe rotation requires coordinated rollover semantics (daemon,
  runner, and client reconnect windows) that are beyond this regression test
  scope.

## Acceptance Criteria
- `AC-01`: A new `orchestrator-core` test fails if token auto-provision is
  removed from `ensure_agent_runner_running`.
- `AC-02`: Test setup proves token is absent before startup call.
- `AC-03`: Calling `ensure_agent_runner_running` on fresh config succeeds and
  returns `Some(pid)`.
- `AC-04`: `config.json` contains generated non-empty UUID token after startup.
- `AC-05`: Authenticated status/readiness query succeeds after startup using
  generated token.
- `AC-06`: Test cleanup stops runner process and leaves no fixture-owned runner
  alive.
- `AC-07`: Repeated startup in same config preserves token value.
- `AC-08`: Requirements/implementation notes explicitly document that periodic
  token rotation is deferred to follow-up work.

## Testable Acceptance Checklist
- `T-01`: Add integration-style startup test under
  `crates/orchestrator-core/src/services/runner_helpers.rs` test module.
- `T-02`: Assert token is missing before startup and generated after startup.
- `T-03`: Assert `query_runner_status(config_dir)` returns `Some(...)` after
  startup.
- `T-04`: Assert second startup call keeps token unchanged.
- `T-05`: Ensure runner stop/cleanup executes in test teardown path.
- `T-06`: Run focused tests:
  - `cargo test -p orchestrator-core runner_helpers`

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-02, FR-03 | new startup regression test with explicit pre/post token assertions |
| FR-04, FR-05 | status/readiness + PID assertions in same isolated flow |
| FR-06 | second startup invocation token-preservation assertion |
| FR-07 | requirement + implementation notes decision record |

## Rotation Follow-Up (Documented, Not in Scope)
- Candidate follow-up task: define secure rotation policy with:
  - rotation trigger (`age`, manual invalidate, compromise recovery),
  - grace/rollover model to avoid breaking in-flight clients,
  - daemon/runner/client coordination and restart contract,
  - explicit observability without secret leakage.

## Deterministic Deliverables for Implementation Phase
- One deterministic startup regression test proving token auto-provision and
  successful runner startup/auth from fresh config state.
- Scoped test helpers for env isolation and cleanup as needed.
- Updated task docs reflecting deferred token-rotation policy.
