# TASK-042 Requirements: Auto-Rebuild Runtime Binaries and Refresh Runner on `main` Updates

## Phase
- Workflow phase: `requirements`
- Workflow ID: `18355d1e-3ce8-42c1-9f38-417ea4716cd6`
- Task: `TASK-042`

## Objective
Define a deterministic daemon contract that:
- detects when repository `main` has advanced,
- rebuilds AO runtime binaries via `cargo ao-bin-build`,
- refreshes the runner process so daemon health checks and agent execution use
  binaries compatible with the latest merged code.

## Current Baseline Audit

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Runtime build command alias | `.cargo/config.toml` | `cargo ao-bin-build` exists and builds `orchestrator-cli`, `agent-runner`, `llm-cli-wrapper`, `llm-mcp-server` | daemon does not invoke it automatically |
| Workflow completion merge path | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs` + `daemon_scheduler_git_ops.rs` | merge/push/cleanup can run after workflow completion | no runtime binary refresh after successful merge |
| Scheduler tick lifecycle | `daemon_scheduler_project_tick.rs` | daemon is started when not running/paused, then normal scheduling proceeds | no `main`-change detection that triggers rebuild/restart |
| Runner compatibility gate | `crates/orchestrator-core/src/services/runner_helpers.rs` | runner compatibility checks protocol + build ID derived from runner binary metadata | does not detect "`main` moved but binaries were never rebuilt" |
| Repo-scoped runtime state pathing | `daemon_scheduler_git_ops.rs` (`repo_ao_root`) | repo-scoped state under `~/.ao/<repo-scope>/...` already used for sync/outbox data | no persisted watermark for binary-refresh progress |
| Daemon config/override surface | `cli_types.rs`, `runtime_daemon.rs`, `.ao/pm-config.json` | supports auto-merge/pr/prune toggles and env overrides | no toggle for auto-rebuild/restart on `main` updates |

## Scope
In scope for implementation after this phase:
- Add a scheduler-driven `main` update detector that compares current `main`
  commit to last successfully refreshed commit.
- Add deterministic persisted refresh state per repo scope.
- Add build-and-refresh action:
  - run `cargo ao-bin-build` in project root,
  - invoke runner refresh logic through existing daemon lifecycle APIs.
- Add safe deferral when active agents are running.
- Add daemon config and run-time override support to enable/disable this
  automation.
- Add structured daemon events and targeted regression tests.
- Optionally trigger the same refresh helper immediately after successful
  merge completion (fast path), while keeping scheduler tick as the canonical
  safety net.

Out of scope:
- Changing supported default branch away from `main`.
- Rebuilding binaries for every tick when `main` is unchanged.
- Rebuilding/restarting while active agents are executing work.
- Manual edits to `/.ao/*.json` repository state files.
- New desktop wrapper/runtime dependencies.

## Constraints
- Refresh decision must be deterministic and idempotent per `main` commit.
- `main` resolution must use local git refs only (`refs/heads/main`,
  `refs/remotes/origin/main`, fallback `HEAD`) unless explicitly fetched by
  existing flows.
- Refresh state writes must be atomic and confined to repo-scoped daemon
  runtime state (under `~/.ao/<repo-scope>/sync/`).
- Runner refresh must reuse existing daemon/runner lifecycle APIs; no parallel
  unmanaged runner process spawning.
- If daemon is paused, behavior must preserve pause semantics (no unintended
  transition to running).
- Failures must be non-fatal for scheduler continuity and must emit
  operator-visible diagnostics.
- Output must remain deterministic and machine-readable in daemon event
  streams.

## Functional Requirements

### FR-01: Detect `main` Commit Drift
- Add helper logic to resolve current `main` head SHA deterministically.
- If current head equals persisted `last_successful_main_head`, no rebuild
  action is taken.
- If current head differs, scheduler marks commit as pending refresh.

### FR-02: Persist Refresh Watermark State
- Add per-repo state file for refresh bookkeeping (for example
  `~/.ao/<repo-scope>/sync/runtime-binary-refresh.json`).
- Persist at minimum:
  - `last_successful_main_head`,
  - `last_attempt_main_head`,
  - `last_attempt_at`,
  - `last_error` (optional/nullable).
- State updates must be atomic and tolerant of missing file on first run.

### FR-03: Scheduler Tick Integration
- Invoke refresh check during daemon project tick in a stable point before
  phase execution fan-out.
- Scheduler tick remains the canonical trigger so external merges to `main`
  (outside daemon merge path) are eventually reconciled.

### FR-04: Safe Deferral While Agents Are Active
- If `daemon.active_agents() > 0`, refresh must be deferred.
- Deferred state must not incorrectly advance `last_successful_main_head`.
- Defer reason and pending head must be observable in daemon events/logs.

### FR-05: Rebuild Contract
- On eligible refresh, run `cargo ao-bin-build` from project root.
- Build success/failure must be captured deterministically:
  - success advances flow to runner refresh,
  - failure records `last_error` and keeps commit pending.

### FR-06: Runner Refresh Contract
- After successful rebuild, invoke existing daemon lifecycle path to ensure the
  runner process uses the rebuilt binary contract.
- Refresh must remain compatible with existing runner scope/config resolution.
- Successful completion is only recorded after both build and runner refresh
  succeed.

### FR-07: Failure Handling and Retry Behavior
- Build or runner-refresh failures must not crash the daemon run loop.
- Failed commit remains pending for retry on later ticks.
- Retry behavior must avoid tight error spam loops (backoff/debounce required).

### FR-08: Optional Post-Merge Fast Path
- After `PostMergeOutcome::Completed`, optionally invoke the same refresh
  helper immediately.
- Fast path must reuse the same state machine as tick path and remain
  idempotent.

### FR-09: Config and Override Surfaces
- Add persisted daemon config toggle for this automation in `.ao/pm-config.json`
  surfaced via `ao daemon config`.
- Add daemon run/start override flag(s) and corresponding env override
  semantics, consistent with existing daemon automation toggles.
- Default behavior must be explicit and documented (enable/disable default).

### FR-10: Observability Contract
- Emit structured daemon event(s) for refresh decisions and outcomes with
  deterministic payload fields:
  - trigger source (`tick` or `post_merge`),
  - target `main` head,
  - action (`noop`, `deferred`, `build_started`, `build_failed`,
    `runner_refreshed`),
  - error summary when relevant.

### FR-11: Regression Coverage
- Add targeted tests for:
  - head-drift detection and idempotent no-op behavior,
  - state persistence read/write and atomic update behavior,
  - active-agent deferral,
  - successful build + runner refresh watermark advancement,
  - failure paths retaining pending state and reporting diagnostics.

## Acceptance Criteria
- `AC-01`: Daemon can determine current `main` head deterministically from local
  refs.
- `AC-02`: Refresh state persists last successful processed `main` commit across
  daemon restarts.
- `AC-03`: No rebuild occurs when current `main` head is unchanged from last
  successful head.
- `AC-04`: On head change and no active agents, daemon runs
  `cargo ao-bin-build`.
- `AC-05`: After successful rebuild, daemon refreshes runner through existing
  lifecycle path and records success.
- `AC-06`: If build fails, runner refresh is not treated as successful and head
  remains pending.
- `AC-07`: If active agents are present, refresh is deferred safely with no
  task/workflow interruption.
- `AC-08`: Failures are non-fatal to scheduler loop and are emitted as
  structured diagnostics.
- `AC-09`: Optional post-merge fast path can trigger same refresh logic without
  duplicate side effects.
- `AC-10`: `ao daemon config` and daemon run/start surfaces expose this
  automation toggle with deterministic behavior.
- `AC-11`: Added tests cover success, no-op, deferral, and failure contracts.

## Testable Acceptance Checklist
- `T-01`: unit tests for `main` head resolution and drift detection helper.
- `T-02`: unit tests for refresh-state load/save and first-run defaults.
- `T-03`: scheduler test verifies unchanged head returns no-op.
- `T-04`: scheduler test verifies active-agent deferral.
- `T-05`: scheduler/integration test verifies successful build + runner refresh
  advances `last_successful_main_head`.
- `T-06`: scheduler/integration test verifies failed build records error and
  does not advance success watermark.
- `T-07`: daemon config/CLI tests verify new toggle parsing and persistence.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-02 | helper unit tests and temp-dir state fixtures |
| FR-03, FR-04 | scheduler project-tick tests with mocked daemon/task load |
| FR-05, FR-06 | integration-style tests around build command and runner refresh invocation boundaries |
| FR-07, FR-10 | daemon event/log assertions for failure and retry/defer paths |
| FR-08 | post-merge flow test calling shared refresh helper |
| FR-09 | CLI argument/config persistence tests (`cli_smoke` + daemon config path) |
| FR-11 | targeted `cargo test -p orchestrator-cli` for touched daemon modules |

## Implementation Notes Input (Next Phase)
Primary code surfaces:
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_run.rs`
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs`
- `crates/orchestrator-cli/src/cli_types.rs`
- `crates/orchestrator-core/src/daemon_config.rs` (if persisted toggle is added
  to typed config API)

Likely test surfaces:
- daemon scheduler tests under
  `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`
- `crates/orchestrator-cli/tests/cli_smoke.rs`
- `crates/orchestrator-cli/tests/cli_e2e.rs` (if config/runtime wiring needs
  e2e validation)

Reference research artifact:
- `crates/orchestrator-cli/docs/task-042-auto-rebuild-runner-on-main-update-research-notes.md`

## Deterministic Deliverables for Implementation Phase
- Repo-scoped `main`-head refresh watermark state.
- Scheduler-based auto rebuild/runner refresh flow with active-agent safety
  gating.
- Optional post-merge fast-path trigger reusing the same refresh helper.
- Configurable enable/disable control via daemon config and run/start overrides.
- Focused regression tests for no-op, success, defer, and failure outcomes.
