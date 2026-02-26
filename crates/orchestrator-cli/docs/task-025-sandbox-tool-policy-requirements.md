# TASK-025 Requirements: Enforce Sandbox and Tool Policy Controls

## Phase
- Workflow phase: `requirements`
- Workflow ID: `0bf4ff68-1b69-4090-ba23-a327b193a0de`
- Requirement: `REQ-025`
- Task: `TASK-025`
- Updated: `2026-02-26`

## Objective
Define a deterministic, repository-safe policy contract that adds:
- per-agent and per-task sandbox mode controls,
- explicit tool allow/deny enforcement,
- auditable elevated execution gates,
- actionable doctor diagnostics for policy health.

The contract must work for direct `ao agent run` execution and daemon-managed
workflow phase runs.

## Existing Baseline Audit (as implemented)

| Surface | Current implementation | Confirmed behavior | Remaining gap vs REQ-025 |
| --- | --- | --- | --- |
| Execution policy model | `crates/orchestrator-core/src/execution_policy.rs`, `crates/orchestrator-core/src/agent_runtime_config.rs`, `crates/orchestrator-core/src/types.rs` | Typed sandbox/tool policy exists with precedence resolution, source tracing, and stable hash | Task policy override writes are not validated in task service update path |
| Daemon policy propagation | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs`, `daemon_scheduler_project_tick.rs` | Daemon resolves policy per phase/task and emits `policy_hash`, `sandbox_mode`, `allow_elevated`, `tool_policy`, `policy_sources` | Requires full integration coverage for regression safety |
| Direct `ao agent run` behavior | `crates/orchestrator-cli/src/shared/runner.rs`, `crates/orchestrator-core/src/runtime_contract.rs` | Direct runs support policy enforcement only when policy is present in provided runtime contract; otherwise defaults apply | No automatic policy resolution from task/phase/agent context for direct runs |
| Runner enforcement and elevation | `crates/agent-runner/src/runner/process.rs` | Deny-over-allow enforced, sandbox-mode launch gating enforced, elevation request/outcome audit persisted, approvals are single-use | No first-class CLI UX for approval/query flow; currently contract-driven |
| Workspace/env guardrails | `crates/agent-runner/src/sandbox/workspace_guard.rs`, `crates/agent-runner/src/sandbox/env_sanitizer.rs` | Repository/worktree boundary checks and env sanitization remain active and fail closed | Keep unchanged while extending policy controls |
| Doctor diagnostics | `crates/orchestrator-core/src/doctor.rs`, `crates/orchestrator-cli/src/services/operations/ops_planning/mod.rs` | Policy checks are present with `healthy/degraded/unhealthy` grading | Requires deterministic output assertions in regression tests |

## Scope
In scope for implementation after this phase:
- Validate task-level execution policy override payloads before persistence.
- Expose/clarify task policy update input contract for operators.
- Define and lock direct `ao agent run` policy semantics (default + explicit
  policy injection paths).
- Keep runner enforcement and elevation behavior deterministic and auditable.
- Ensure daemon phase metadata and events preserve policy context fields.
- Add/expand tests for policy parsing, precedence, enforcement, elevation, and
  doctor grading.

Out of scope for this task:
- OS kernel sandboxing or container/jail orchestration.
- Interactive TTY approval prompts.
- Web UI policy editing flows.
- Manual edits to `.ao` JSON state files.

## Constraints
- Preserve existing command names and existing safe defaults for users without
  explicit policy configuration.
- Preserve backward compatibility for runtime contracts that do not include
  execution policy payloads.
- Keep `ao.cli.v1` envelope behavior unchanged in JSON mode.
- Keep `workspace_guard` repository-bound checks intact and fail closed.
- Keep policy evaluation deterministic (same input context -> same decision).
- Keep enforcement semantics explicit and auditable in run/phase artifacts.
- Deny rules must take precedence over allow rules.
- Elevated approvals must be operation-bound and single-use to prevent replay.
- Keep all file/state writes for elevation audit deterministic and atomic.

## Policy Model Contract

### Sandbox Modes
Resolved sandbox mode enum:
- `read_only`
- `workspace_write`
- `danger_full_access`

Default behavior when not configured:
- `workspace_write`

### Tool Policy
Resolved tool policy must support:
- `allow_prefixes`
- `allow_exact`
- `deny_prefixes`
- `deny_exact`

Decision rules:
1. Normalize tool identifiers to lowercase before evaluation.
2. `deny_exact`/`deny_prefixes` always block even when also allowlisted.
3. If allow rules are empty, preserve current MCP-only prefix enforcement and
   preserve current behavior for non-MCP mode.
4. Unknown/empty tool names fail closed.

### Policy Resolution Precedence
Resolved policy for a run must follow:
1. task-level override (if present)
2. phase-level runtime override
3. agent profile default
4. global default

Resolution output must include:
- resolved sandbox mode
- resolved allow/deny tool sets
- policy source trace (which level supplied each field)
- stable policy hash for audit correlation

### Direct `ao agent run` Contract
- If no execution policy payload is present in runtime contract, defaults apply:
  - `sandbox_mode=workspace_write`
  - `allow_elevated=false`
  - empty allow/deny lists
- Runtime policy payload may be supplied via either path:
  - `runtime_contract.policy.execution`
  - `runtime_contract.execution_policy`
- Both paths must be parsed deterministically and produce equivalent policy
  decisions.

## Elevated Execution Contract
When policy would block an operation that is explicitly elevatable:
- runner must emit/return `ELEVATION_REQUIRED` with:
  - `elevation_request_id`
  - blocked action metadata (tool, sandbox target, phase/run identity)
  - remediation guidance
- no side effects are executed before approval

Elevation records must persist:
- request record (`requested_at`, requester/run context, requested action,
  policy hash, reason)
- approval record (`approved`, approver identity, comment, approved_at)
- outcome record (`success`, `message`, `recorded_at`)

Approval safety rules:
- approval must bind to exact run scope (workflow/task/phase/run id + policy
  hash + requested action)
- approval is single-use
- approval mismatch fails closed with deterministic error

Audit file contract:
- elevation records persist at:
  - `<project_root>/.ao/state/elevation-audit.v1.json`
- writes must use atomic replace semantics to avoid partial-file corruption.

## Observability and Audit Requirements
Phase/run artifacts must include policy context:
- resolved policy hash
- resolved sandbox mode
- policy decision events (`allowed`, `blocked`, `elevation_required`,
  `elevation_approved`, `elevation_denied`)

Daemon phase events must expose policy metadata without leaking secrets.

## Doctor Check Contract
`ao doctor` must include policy checks:
- `policy_config_loadable`
- `policy_schema_valid`
- `policy_phase_bindings_valid`
- `policy_elevation_store_writable`
- `policy_runtime_defaults_resolvable`

Result grading:
- `unhealthy` if any policy check fails hard (invalid schema, unresolved policy
  references, non-writable required store path)
- `degraded` for soft warnings
- `healthy` when all checks pass

## Acceptance Criteria
- `AC-01`: Policy model supports sandbox mode + tool allow/deny fields at
  agent and task levels.
- `AC-02`: Task policy override writes reject empty tool entries and keep
  persisted values normalized/valid.
- `AC-03`: Policy resolution precedence is deterministic and serialized in
  run/phase metadata.
- `AC-04`: Runner blocks disallowed tool calls with deterministic
  `POLICY_VIOLATION` errors.
- `AC-05`: Deny rules always override allow rules.
- `AC-06`: Resolved sandbox mode is enforced before launching side-effecting
  execution.
- `AC-07`: Elevatable policy violations return `ELEVATION_REQUIRED` and create
  auditable request records.
- `AC-08`: Approved elevation can be consumed once and only for its bound
  action scope.
- `AC-09`: Elevation request/approval/outcome artifacts are persisted and
  queryable for audit.
- `AC-10`: `ao doctor` reports policy checks and grading (`healthy`,
  `degraded`, `unhealthy`) deterministically.
- `AC-11`: Existing workflows with no explicit policy config remain functional
  with current safe defaults.
- `AC-12`: Existing command-phase `tools_allowlist` behavior remains intact for
  command mode phases.
- `AC-13`: Workspace boundary enforcement remains unchanged and continues to
  fail closed.
- `AC-14`: Direct `ao agent run` defaults remain backward-compatible when no
  policy payload is supplied, and explicit payloads are honored via documented
  runtime contract paths.

## Verification Matrix

| Requirement | Verification method |
| --- | --- |
| `AC-01`, `AC-03` | Unit tests for config parsing + policy resolution precedence and hash stability |
| `AC-02` | Task service tests asserting policy override validation failures/success paths |
| `AC-04`, `AC-05` | Runner tests for allow/deny combinations and deny-over-allow behavior |
| `AC-06` | Runner launch tests validating sandbox-mode gating decisions |
| `AC-07`, `AC-08`, `AC-09` | Integration tests for elevation request, approval binding, single-use consumption, and persisted outcomes |
| `AC-10` | Doctor tests asserting policy check entries and result grading logic |
| `AC-11`, `AC-12`, `AC-13` | Regression tests for existing daemon/task execution and workspace guard behavior |
| `AC-14` | Direct `agent run` tests for default-policy fallback and explicit payload parsing (`policy.execution` and `execution_policy`) |

## Deterministic Deliverables for Next Phase
- Task execution policy validation in task service update path.
- CLI/task input contract documentation for policy overrides.
- Direct-run policy contract hardening and regression tests.
- End-to-end tests for policy resolution, enforcement, elevation, doctor, and
  backward compatibility.
