# TASK-028 Implementation Notes: Agent-Runner IPC Socket Authentication

## Purpose
Define the concrete implementation plan to enforce token authentication on the
`agent-runner` unix socket while keeping existing runner workflows and
observability behavior stable.

## Verified Starting Point
- `agent-runner` currently accepts socket connections and routes the first
  recognized operational payload with no auth gate.
- `ipc/auth.rs` is a stale helper (websocket `Authorization` header) and is not
  wired in `ipc/mod.rs`.
- `protocol::Config::get_token()` currently returns `dev-token` on missing
  config/env token material.
- Multiple clients connect directly and send operational payloads first:
  `runtime_agent`, daemon scheduler phase execution, planning runtimes, task
  generation, runner ops health checks, and `orchestrator-core` compatibility
  probes.

## Decisions Locked In Requirements Phase
- Token API behavior is explicit: `protocol::Config::get_token()` returns
  `Result<String>` and errors on missing/blank token material.
- Auth bootstrap is mandatory on shared runner connect surfaces so all existing
  callers inherit auth without per-callsite token-write duplication.
- Router/auth flow is a strict connection gate:
  - first non-empty JSONL line must be auth request,
  - server emits one auth result payload,
  - failed auth closes the stream before handler dispatch.
- Auth payload text is excluded from payload preview logging.

## Function-Level Callsite Inventory (Auth Required)
- `crates/orchestrator-cli/src/services/runtime/runtime_agent/connection.rs`
  - `connect_runner_for_agent_command(...)`
- `crates/orchestrator-cli/src/shared/task_generation.rs`
  - `run_prompt_against_runner(...)`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`
  - `run_workflow_phase_attempt(...)`
- `crates/orchestrator-cli/src/services/operations/ops_runner.rs`
  - `query_runner_status_direct(...)`
- `crates/orchestrator-core/src/services/runner_helpers.rs`
  - `query_runner_status(...)`
- `crates/orchestrator-cli/src/services/operations/ops_planning/{draft_runtime.rs,refinement_runtime.rs,requirements_runtime.rs}`
  - planning runtime runner connect/send paths

## Non-Negotiable Constraints
- Fail closed: no token, invalid token, or malformed auth payload must block all
  operational handlers.
- Never log raw token data (including debug payload previews).
- Keep existing run/control/status/model/runner-status payload shapes unchanged
  after auth succeeds.
- Preserve current socket/config directory behavior and AO runner lifecycle
  semantics.

## Proposed Auth Handshake Contract
Add explicit auth payload types in `protocol`:
- Client first JSONL message:
  `{"kind":"ipc_auth","token":"<opaque-token>"}`.
- Server response:
  `{"kind":"ipc_auth_result","ok":true}` on success, otherwise
  `{"kind":"ipc_auth_result","ok":false,"code":"<reason>","message":"<redacted>"}`.
- On auth failure: server writes failure response and closes connection.
- On auth success: connection transitions to authenticated mode and existing
  operational requests proceed unchanged.

Implementation intent:
- Keep auth as a deterministic pre-step instead of wrapping all operational
  messages in a new envelope enum.
- Parse and handle the first non-empty message through a dedicated auth path
  before any payload preview logging.
- Reject unknown fields in auth payload decoding to avoid ambiguous request
  shapes.
- On auth failure, return structured `ipc_auth_result` and close connection in
  the same request cycle (no additional request parsing).

## Token Resolution Contract
- `Config::get_token()` must stop returning `dev-token` and return
  `Result<String>`.
- Token precedence remains:
  1. `AGENT_RUNNER_TOKEN`
  2. `agent_runner_token` from resolved config
- Missing or blank token returns explicit error behavior.
- All call sites that need a token must handle and propagate this error
  deterministically.

## File-by-File Change Plan

### Protocol
- `crates/protocol/src/agent_runner.rs`
  - add auth request/response structs for IPC bootstrap.
- `crates/protocol/src/config.rs`
  - remove `dev-token` fallback from `get_token()`.
  - change `get_token()` to return `Result<String>`.
  - return explicit error on missing/blank token.
- `crates/protocol/tests/agent_runner.rs`
  - add auth payload serialization/roundtrip tests.
  - add tests for token precedence and no-fallback behavior.

### Agent Runner
- `crates/agent-runner/src/ipc/mod.rs`
  - wire `auth` module for actual runtime use in router connection handling.
- `crates/agent-runner/src/ipc/auth.rs`
  - replace websocket-header helper with JSONL auth decode/validate helpers.
  - keep error messages redacted.
- `crates/agent-runner/src/ipc/router.rs`
  - enforce auth as first-step gate per connection.
  - reject pre-auth operational payloads deterministically.
  - avoid logging auth payload contents.
- `crates/agent-runner/src/ipc/server.rs`
  - keep connection lifecycle logging stable while integrating auth outcomes.

### CLI/Core Clients
- `crates/orchestrator-cli/src/shared/runner.rs`
  - add shared runner-auth helper used immediately after socket connect.
  - wire `connect_runner(...)` to run the auth bootstrap before returning stream.
- `crates/orchestrator-cli/src/services/runtime/runtime_agent/connection.rs`
  - ensure runtime agent run/control/status/model/runner-status commands
    authenticate immediately after connect.
- `crates/orchestrator-cli/src/shared/task_generation.rs`
  - apply shared auth helper.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`
  - apply shared auth helper.
- `crates/orchestrator-cli/src/services/operations/ops_planning/{draft_runtime.rs,refinement_runtime.rs,requirements_runtime.rs}`
  - apply shared auth helper.
- `crates/orchestrator-cli/src/services/operations/ops_runner.rs`
  - authenticate before runner health/status probes.
- `crates/orchestrator-core/src/services/runner_helpers.rs`
  - authenticate compatibility `RunnerStatusRequest` probes used during runner
    startup/health checks.
  - keep readiness probe semantics unchanged (`is_agent_runner_ready` remains
    transport-only connect check).

## Implementation Sequence
1. Add protocol auth payload types and token contract changes.
2. Change `Config::get_token()` to `Result<String>` and migrate callsites.
3. Implement server-side auth gate + token-safe logging behavior.
4. Add shared client-side auth helper in `shared/runner.rs`.
5. Wire runtime agent connection helper to perform auth bootstrap once per
   connection.
6. Migrate direct runner call paths (task generation, daemon phase execution,
   planning runtimes, runner ops status) to the shared auth bootstrap.
7. Update orchestrator-core runner helper compatibility probes for
   authenticated status checks.
8. Add/adjust tests across protocol, agent-runner, orchestrator-core, and
   orchestrator-cli.
9. Run callsite coverage audit (`rg "connect_runner\\(|RunnerStatusRequest::default\\("`) to
   confirm no unauthenticated request path remains.
10. Reconcile requirements/implementation docs with final code behavior.

## Test Plan
- `cargo test -p protocol`
- `cargo test -p agent-runner`
- `cargo test -p orchestrator-core runner_helpers`
- `cargo test -p orchestrator-cli runtime_agent`
- targeted orchestrator-cli tests for `ops_runner`, planning runtimes, and
  daemon phase execution runner paths
- targeted orchestrator-cli tests for task generation/planning/runtime-daemon
  runner call paths if test modules are available
- targeted tests for planning/runtime/runner operations touching authenticated
  connection paths
- targeted tests for token resolution error cases:
  - missing token
  - blank env token
  - blank config token
- targeted tests for shared connect bootstrap ordering:
  - auth request written before first operational payload
- post-change callsite audit with:
  - `rg -n "connect_runner\\(" crates/orchestrator-cli crates/orchestrator-core`
  - `rg -n "RunnerStatusRequest::default\\(" crates/orchestrator-cli crates/orchestrator-core`

## Risks and Mitigations
- Risk: token leakage via existing payload preview logs.
  - Mitigation: auth handled before payload preview logging; redact all auth
    failure output.
- Risk: missed client call path leads to partial auth rollout.
  - Mitigation: centralize auth bootstrap in shared helper and verify with `rg`
    audit for all `connect_runner` usages.
- Risk: runner compatibility probe regressions break auto-start logic.
  - Mitigation: update `orchestrator-core::runner_helpers` alongside CLI paths
    and cover with tests.
- Risk: stale websocket-era auth code remains and confuses future maintenance.
  - Mitigation: replace/remove stale helper behavior and keep only JSONL socket
    auth flow in IPC module tree.
