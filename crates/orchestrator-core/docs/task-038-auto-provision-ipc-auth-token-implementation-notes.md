# TASK-038 Implementation Notes: Auto-Provision IPC Auth Token Startup Coverage

## Purpose
Translate TASK-038 requirements into concrete test work that validates the full
`ensure_agent_runner_running` startup path from a fresh config directory with no
token.

## Locked Decisions from Requirements Phase
- Add one integration-style regression test that runs real startup logic.
- Test must start from missing token state and prove token generation plus
  authenticated readiness/status success.
- Token rotation is explicitly out of scope for TASK-038; startup must preserve
  existing token once created.
- Prefer unix-scoped test execution to avoid fixed TCP port collisions on
  non-unix hosts.

## Non-Negotiable Constraints
- Keep changes scoped to `crates/orchestrator-core` unless compilation requires
  a minimal cross-crate adjustment.
- Avoid any manual edits to `/.ao/*.json`.
- Ensure environment changes are guarded and restored.
- Ensure spawned runner is always terminated during test cleanup.

## Proposed Change Surface

### Runner Startup Regression Test
- `crates/orchestrator-core/src/services/runner_helpers.rs` (`#[cfg(test)]`)
  - add a new `#[tokio::test]` (unix-gated) that:
    - creates isolated temp `project_root`,
    - sets isolated `AO_RUNNER_CONFIG_DIR`,
    - removes `AGENT_RUNNER_TOKEN` override,
    - ensures no token exists in scoped config,
    - calls `ensure_agent_runner_running(project_root)`,
    - verifies token generation + readiness/status success,
    - verifies second startup call preserves token,
    - stops runner and verifies fixture cleanup.

### Test Isolation Helpers (if needed)
- `crates/orchestrator-core/src/services/runner_helpers.rs` test module
  - add lightweight env var guard helper similar to existing pattern used in
    `protocol` tests.
  - add optional static mutex for env-sensitive tests to prevent cross-test
    races.

## Suggested Test Flow
1. Create temp project root and temp runner config dir.
2. Write/initialize config state with missing token.
3. Apply env guards:
   - `AO_RUNNER_CONFIG_DIR=<temp-runner-dir>`
   - unset `AGENT_RUNNER_TOKEN`
   - unset `AO_SKIP_RUNNER_START`
4. Assert precondition: `Config::load_from_dir(...).agent_runner_token` is
   `None` or blank.
5. Call `ensure_agent_runner_running(project_root)` and assert `Some(pid)`.
6. Reload config and assert generated token is non-empty UUID.
7. Call `query_runner_status(config_dir)` and assert `Some(...)`.
8. Call `ensure_agent_runner_running(project_root)` again and assert token is
   unchanged.
9. Call `stop_agent_runner_process_at_config_dir(config_dir)` and assert
   cleanup.

## Risks and Mitigations
- Risk: flaky behavior from shared env mutation.
  - Mitigation: env guard + mutex around env-sensitive test.
- Risk: orphan runner process when assertion fails.
  - Mitigation: structured cleanup path (best-effort stop + kill fallback where
    needed).
- Risk: non-unix port/socket collision in CI.
  - Mitigation: keep integration-style startup test unix-only.

## Validation Targets
- Focused:
  - `cargo test -p orchestrator-core runner_helpers::tests`
- Crate-level:
  - `cargo test -p orchestrator-core`

## Deferred Security Follow-Up
- Periodic/rolling token rotation is not implemented in TASK-038.
- Follow-up design must define:
  - safe rollover semantics,
  - compatibility window for in-flight runner clients,
  - restart/rehydration behavior for daemon and CLI.
