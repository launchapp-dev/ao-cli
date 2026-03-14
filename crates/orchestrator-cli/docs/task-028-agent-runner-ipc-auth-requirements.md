# TASK-028 Requirements: Agent-Runner IPC Socket Authentication

## Phase
- Workflow phase: `requirements`
- Workflow ID: `48b88e48-0d96-4034-a1f1-ab2c3d36b53d`
- Task: `TASK-028`
- Requirement linkage: pending AO requirement record in this worktree snapshot

## Objective
Define a production-safe authentication contract for the local `agent-runner`
IPC control channel so only authorized clients can submit
run/control/status/model commands over the unix socket.

## Current Baseline (Verified)

| Surface | Current location | Current status |
| --- | --- | --- |
| IPC transport | `crates/agent-runner/src/ipc/server.rs`, `crates/agent-runner/src/ipc/router.rs` | Unix socket accepts connections and dispatches first recognized operational payload immediately |
| Auth helper | `crates/agent-runner/src/ipc/auth.rs`, `crates/agent-runner/src/ipc/mod.rs` | Stale websocket-header helper exists but is not wired into `ipc/mod.rs` and is unused by current JSONL socket flow |
| Token resolution | `crates/protocol/src/config.rs` | `Config::get_token()` falls back to hard-coded `dev-token` |
| CLI shared runner connect | `crates/orchestrator-cli/src/shared/runner.rs`, `crates/orchestrator-cli/src/services/runtime/runtime_agent/connection.rs` | Socket connection helper returns connected stream with no auth bootstrap step |
| CLI direct runner call paths | `crates/orchestrator-cli/src/shared/task_generation.rs`, `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`, `crates/orchestrator-cli/src/services/operations/ops_planning/{draft_runtime.rs,refinement_runtime.rs,requirements_runtime.rs}`, `crates/orchestrator-cli/src/services/operations/ops_runner.rs`, `crates/orchestrator-cli/src/services/runtime/runtime_agent/{run.rs,status.rs}` | Callers write first operational payload directly after connect with no auth handshake |
| Core runner readiness/compatibility probe | `crates/orchestrator-core/src/services/runner_helpers.rs` | Direct status probe sends `RunnerStatusRequest` with no auth step |
| Logging surface | `crates/agent-runner/src/ipc/router.rs` | Received payload preview is logged in debug; auth payload design must prevent token leakage |
| Auth test baseline | `crates/agent-runner/src/ipc/*`, `crates/protocol/tests/agent_runner.rs` | No current IPC auth gate tests; protocol tests currently cover runner status request shape but no auth payload contract |

## Gap to Close
- Any local process with socket access can issue runner commands because no
  token validation is enforced on accepted connections.
- `dev-token` fallback prevents fail-closed behavior and makes missing-token
  misconfiguration silently pass.
- CLI and core call paths currently have no deterministic auth step prior to
  command dispatch.
- Current router debug payload logging would leak credential material if auth is
  naively implemented as a plain first-line token payload.

## Scope

In scope for implementation after this phase:
- Add connection-level token authentication for unix socket JSONL IPC.
- Add an explicit auth request/response wire contract in `protocol` so clients
  and server share deterministic payload types.
- Require successful auth before any run/control/status/model/runner-status
  request is handled.
- Remove `dev-token` fallback from `protocol::Config::get_token()`.
- Make runner client auth bootstrap deterministic and centralized so shared
  runner connect paths enforce auth by default instead of duplicating
  handshake logic at each callsite.
- Ensure all runner clients send auth data before operational payloads:
  - orchestrator-cli runtime and operations paths,
  - orchestrator-core runner status compatibility probes.
- Preserve runner lifecycle behavior: runner start/health/compat checks must
  continue to function with auth enabled.
- Replace stale websocket-style auth helper logic in
  `crates/agent-runner/src/ipc/auth.rs` with JSONL socket auth used by router.
- Add tests and docs that prove unauthorized connections are rejected and token
  data is never exposed.

Out of scope for this task:
- Replacing unix socket transport with a different IPC mechanism.
- Adding external secret manager dependencies.
- Reworking broader AO daemon/workflow state behavior unrelated to runner IPC.

## Constraints
- Authentication must fail closed: missing/invalid token must reject requests.
- Do not log or serialize raw token values in runner/CLI logs or error payloads.
- Preserve deterministic newline-delimited JSON IPC behavior after auth.
- Authentication is a one-time bootstrap for each connection: the first non-empty
  JSONL payload must be auth; operational payloads are valid only after success.
- Keep existing request/response payload shapes for run/control/status/model and
  runner-status operations (auth should be an additive pre-step, not a breaking
  rewrite of existing operational message formats).
- `protocol::Config::get_token()` must fail with explicit error semantics when
  token material is missing/blank (no implicit defaults).
- Keep current runner socket location/config directory behavior unchanged.
- Keep non-unix build compatibility; unix socket auth enforcement is the primary
  requirement surface.

## Locked Requirement Decisions (Phase Output)
- Token API decision: `protocol::Config::get_token()` must return
  `Result<String>` and error on:
  - missing env/config token material,
  - empty/whitespace-only token values.
- Client bootstrap decision: shared runner connection helpers are the mandatory
  integration point for auth bootstrap:
  - `crates/orchestrator-cli/src/shared/runner.rs::connect_runner(...)` and
    related shared helpers,
  - `crates/orchestrator-core/src/services/runner_helpers.rs::query_runner_status(...)`.
- Server state-machine decision:
  - first non-empty JSONL line is parsed only as auth request,
  - server sends exactly one auth result payload,
  - server closes connection immediately after any failed auth result,
  - operational payload parsing starts only after successful auth.
- Logging decision: auth request lines are never emitted in debug payload
  previews; auth failures log only connection id and failure code.

## Deterministic Handshake Shape (Target)
First line on a new connection is reserved for auth:

```json
{"kind":"ipc_auth","token":"<opaque-token>"}
```

Server auth response must be deterministic JSONL:

```json
{"kind":"ipc_auth_result","ok":true}
```

```json
{"kind":"ipc_auth_result","ok":false,"code":"invalid_token","message":"unauthorized"}
```

Contract details:
- `kind` values are fixed and machine-parseable.
- `code` is present when `ok=false` and must be one of:
  `malformed_auth_payload`, `invalid_token`, `server_token_unavailable`.
- `message` is short and redacted (no token material).
- First non-empty line on the connection is the only valid auth request slot.
- Any non-auth first payload is rejected with `malformed_auth_payload`.
- Connection is closed immediately after any `ok=false` auth result.

## Functional Requirements

### FR-01: IPC Auth Wire Contract
- Define explicit auth payload types in `crates/protocol/src/agent_runner.rs`
  for connection bootstrap (request + response).
- Auth payloads must be deterministic JSONL with stable field names and
  serialize/deserialize coverage.
- Auth request/response payloads must use `#[serde(deny_unknown_fields)]` to
  reject malformed/ambiguous payload shapes deterministically.
- Auth failure responses must be structured and automation-safe (machine
  parseable reason code category, no secret material).

### FR-02: Token Source Contract
- Token resolution precedence remains:
  1. `AGENT_RUNNER_TOKEN` environment variable
  2. `agent_runner_token` from resolved config
- `protocol::Config::get_token()` must not return `dev-token` fallback and must
  return `Result<String>`.
- Missing/blank token material must produce explicit error behavior rather than
  silent fallback.
- Callers must propagate token-resolution errors with deterministic,
  non-secret-bearing messages.

### FR-03: Connection Authentication Gate
- Each newly accepted unix socket connection must complete auth successfully
  before any operational request is dispatched.
- The first non-empty payload on each new connection must decode as auth
  request shape; empty lines may be ignored before auth.
- Any first payload that is not a valid auth request must receive deterministic
  rejection (`code=malformed_auth_payload`) and connection closure.
- Invalid token must receive deterministic rejection (`code=invalid_token`) and
  connection closure.
- Missing/invalid server token material must reject with
  `code=server_token_unavailable` and close.
- Authentication outcome is scoped to the connection lifetime.

### FR-04: Server-Side Enforcement
- `agent-runner` request routing must enforce auth for:
  - run requests
  - control requests
  - agent status requests
  - model status requests
  - runner status requests
- Requests received before auth completion must not trigger handler side effects.

### FR-05: Client-Side Auth Coverage
- All orchestrator-cli paths that connect to `agent-runner.sock` must send auth
  data first and proceed with existing request payloads only after auth success.
- Shared connection/auth logic must be centralized via
  `crates/orchestrator-cli/src/shared/runner.rs` (connection bootstrap) and
  `crates/orchestrator-cli/src/services/runtime/runtime_agent/connection.rs` so
  runtime agent run/control/status/model/runner-status commands inherit auth by
  default.
- Direct runner call paths (`task_generation`, planning runtimes, daemon phase
  execution, runner ops health/status) must also execute the same auth
  bootstrap before writing operational payloads.
- Required coverage includes (current function-level call sites):
  `connect_runner_for_agent_command`, `run_prompt_against_runner`,
  `run_workflow_phase_attempt`, `query_runner_status_direct`, and all planning
  runtime runner connect/write paths.
- `orchestrator-core` runner status/compatibility probes must authenticate
  before sending `RunnerStatusRequest`.
- Failure reporting must be actionable and deterministic for automation.
- Coverage audit for this requirement must be validated with:
  - `rg -n "connect_runner\\(" crates/orchestrator-cli crates/orchestrator-core`
  - `rg -n "RunnerStatusRequest::default\\(" crates/orchestrator-cli crates/orchestrator-core`

### FR-06: Security and Observability
- Unauthorized attempts should be observable (connection id + failure reason
  category) without exposing secret material.
- Router/auth logging must avoid payload-preview token leakage.
- Existing logging and JSON-output behavior must remain stable for successful
  authenticated requests.

### FR-07: Documentation and Test Coverage
- Requirements and implementation notes must match final code behavior.
- Tests must cover authorized and unauthorized paths plus missing-token
  behavior.

### FR-08: Dead Code and Legacy Auth Cleanup
- `crates/agent-runner/src/ipc/mod.rs` must expose the active auth module used
  by router/server flow.
- Legacy websocket-header-specific auth logic must be removed or replaced so no
  stale auth path remains in `agent-runner` IPC.

## Acceptance Criteria
- `AC-01`: Unauthenticated unix socket connections cannot execute runner
  operations.
- `AC-02`: Connections with missing or incorrect token are rejected
  deterministically before request handlers run.
- `AC-03`: Connections with valid token can run existing run/control/status
  workflows without regression.
- `AC-04`: `protocol::Config::get_token()` no longer falls back to
  `dev-token`.
- `AC-05`: `orchestrator-core` runner compatibility checks still function
  against authenticated runner sockets.
- `AC-06`: No raw token values appear in logs, emitted errors, or persisted
  runner artifacts.
- `AC-07`: Updated docs and tests demonstrate the enforced auth contract.
- `AC-08`: Any non-auth first payload is rejected with
  `code=malformed_auth_payload`, and connection closes before handlers run.
- `AC-09`: Non-unix targets continue to build with existing TCP behavior while
  unix auth enforcement remains the required security surface.
- `AC-10`: No stale websocket-header auth path remains in `agent-runner` IPC
  module tree after implementation.
- `AC-11`: `protocol::Config::get_token()` returns an error (no fallback) when
  token material is missing or blank.
- `AC-12`: Shared runner connection helpers enforce auth bootstrap for all
  audited runner socket call paths.

## Testable Acceptance Checklist
- `T-01`: `protocol` tests for auth wire payload serialization and token
  resolution precedence/no-dev-fallback rules.
- `T-02`: `agent-runner` router/auth tests proving pre-auth operational payloads
  are rejected.
- `T-03`: `agent-runner` router/auth tests proving valid-token requests are
  accepted and routed.
- `T-04`: `orchestrator-cli` runtime/operations path tests for
  run/status/control/model/runner-status after auth.
- `T-05`: `orchestrator-core` runner helper tests covering authenticated
  `RunnerStatusRequest` compatibility probes.
- `T-06`: Negative tests for missing token configuration and mismatched token.
- `T-07`: Log/error assertions confirming token values are never emitted.
- `T-08`: Auth payload decode tests reject unknown fields and malformed
  first-line shapes.
- `T-09`: Coverage audit asserts all runner socket request paths authenticate
  before first operational payload write.
- `T-10`: `protocol::Config::get_token()` tests cover missing token, blank env
  token, blank config token, and env-over-config precedence.
- `T-11`: Client bootstrap tests prove auth handshake runs before first
  operational payload on shared connect paths.

## Acceptance Verification Matrix
| Requirement area | Verification method |
| --- | --- |
| Auth wire contract | `protocol` unit tests + roundtrip assertions |
| Auth gate enforcement | `agent-runner` IPC router/auth tests |
| Token source + fallback removal | `protocol` config tests |
| Client handshake coverage | `orchestrator-cli` runner connection/runtime tests |
| Core runner helper compatibility | `orchestrator-core` runner helper tests |
| Regression on operational requests | existing + updated runtime agent/daemon tests |
| Secret redaction/no leakage | targeted assertions on router/auth logs + error outputs |
| Malformed auth payload handling | `protocol` + `agent-runner` auth decode/rejection tests |
| Dead-code cleanup and active auth wiring | compile + module wiring checks in `agent-runner` IPC |

## Implementation Notes Input
Primary files expected in implementation phase:
- `crates/agent-runner/src/ipc/router.rs`
- `crates/agent-runner/src/ipc/auth.rs`
- `crates/agent-runner/src/ipc/mod.rs`
- `crates/agent-runner/src/ipc/server.rs`
- `crates/protocol/src/config.rs`
- `crates/protocol/src/agent_runner.rs`
- `crates/protocol/tests/agent_runner.rs`
- `crates/orchestrator-cli/src/shared/runner.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_agent/connection.rs`
- `crates/orchestrator-core/src/services/runner_helpers.rs`
- `crates/orchestrator-cli/src/shared/task_generation.rs`
- `crates/orchestrator-cli/src/services/operations/ops_planning/{draft_runtime.rs,refinement_runtime.rs,requirements_runtime.rs}`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`
- `crates/orchestrator-cli/src/services/operations/ops_runner.rs`

## Deterministic Deliverables for Implementation Phase
- Auth request/response payload contract added to `protocol`.
- Connection-level auth gate enforced by `agent-runner` before request routing.
- Client-side auth handshake coverage for CLI and core runner helper paths.
- `dev-token` fallback removed from token resolution.
- Tests and docs updated to prove fail-closed auth behavior and token redaction.
