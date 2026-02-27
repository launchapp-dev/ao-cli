# TASK-042 Research Notes: Auto-Rebuild Runtime Binaries and Refresh Runner on `main` Updates

## Scope
- Workflow: `18355d1e-3ce8-42c1-9f38-417ea4716cd6`
- Task: `TASK-042`
- Phase: `research`
- Objective:
  - when `main` advances after task merges, rebuild AO runtime binaries (`cargo ao-bin-build`)
  - restart/refresh `agent-runner` so daemon executions use the latest runner binary and IPC contract

## AO State Evidence (2026-02-27)
- `TASK-042` metadata is not present in this worktree's repo-local `.ao/tasks` snapshot.
  - Observed highest local task file is `TASK-036` from `.ao/tasks/` listing.
- Closest requirement linkage in current repo-local AO data is runner lifecycle reliability:
  - `.ao/requirements/generated/REQ-003.json:12`
  - `.ao/requirements/generated/REQ-003.json:34`

## Code Evidence

### 1. Canonical multi-binary build command already exists
- Cargo alias already defines the runtime build set required by this task:
  - `.cargo/config.toml:22`
  - `.cargo/config.toml:23`

### 2. Scheduler tick starts daemon only when not running/paused
- Current tick path checks daemon status and only calls `daemon.start()` if status is not running/paused:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs:1778`
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs:1784`
- Implication: once daemon is running, no periodic runner compatibility/startup reconciliation is forced by tick itself.

### 3. Merge completion path has no binary refresh hook today
- Completed workflow path triggers `post_success_merge_push_and_cleanup(...)`:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs:377`
- Direct merge flow sets `merged_successfully = true` after merge/push:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:1252`
- After merge success, code only performs worktree cleanup/pruning:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:1265`
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:1267`
- No `cargo ao-bin-build` or runner restart occurs in this path today.

### 4. Runner compatibility check exists, but compares against binary file metadata only
- `runner_binary_build_id(...)` is derived from binary modified time + file size:
  - `crates/orchestrator-core/src/services/runner_helpers.rs:355`
  - `crates/orchestrator-core/src/services/runner_helpers.rs:366`
- `ensure_agent_runner_running(...)` compares runner-reported `build_id` to expected build ID from current binary path metadata and restarts if mismatch:
  - `crates/orchestrator-core/src/services/runner_helpers.rs:489`
  - `crates/orchestrator-core/src/services/runner_helpers.rs:497`
  - `crates/orchestrator-core/src/services/runner_helpers.rs:500`
- This does not detect "git `HEAD` changed but binaries were not rebuilt".

### 5. Runner status protocol already exposes `build_id`
- `RunnerStatusResponse` includes `build_id`:
  - `crates/protocol/src/agent_runner.rs:125`
- Runner returns build ID from `AO_RUNNER_BUILD_ID` env:
  - `crates/agent-runner/src/runner/mod.rs:162`
  - `crates/agent-runner/src/runner/mod.rs:256`

### 6. Default merge target is constrained to `main`
- Post-success merge config forces auto-merge target branch to `main`:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:22`
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:61`

### 7. Existing daemon lifecycle APIs can perform restart without new core primitives
- Daemon `start()` executes runner ensure/start logic:
  - `crates/orchestrator-core/src/services/daemon_impl.rs:157`
- Daemon `stop()` terminates runner process:
  - `crates/orchestrator-core/src/services/daemon_impl.rs:192`

## Deterministic Design Choice
Implement a **scheduler tick check** (primary) with an optional post-merge fast-path trigger.

Why this is preferred:
- Catches both daemon-driven direct merges and external/remote `main` advancements.
- Reuses existing runner lifecycle logic (`daemon.start()` / `ensure_agent_runner_running(...)`) instead of duplicating restart behavior.
- Can be made deterministic with persisted per-repo stamp state.

## Proposed Implementation Plan (Build-Ready)
1. Add a small persisted state record for binary refresh progress (per repo scope).
- Store under existing daemon repo-scope root (same family used by git outbox), e.g. `~/.ao/<repo-scope>/sync/runtime-binary-refresh.json`.
- Fields:
  - `last_successful_main_head`
  - `last_attempt_main_head`
  - `last_attempt_at`
  - `last_error` (optional)

2. Add helper in daemon scheduler to resolve current `main` commit deterministically.
- Use local refs first (`refs/heads/main`, `refs/remotes/origin/main`, `HEAD`), aligned with existing default-target ref logic:
  - `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs:1677`
- Resolve commit with `git rev-parse --verify <ref>`.

3. On each `project_tick`, before workflow phase execution, compare `current_main_head` with `last_successful_main_head`.
- If equal: no-op.
- If different:
  - skip rebuild when active agents are non-zero (`daemon.active_agents()`), keep pending.
  - otherwise run `cargo ao-bin-build` in `project_root`.

4. After successful build, refresh runner via existing daemon lifecycle.
- Call `daemon.start().await` to force runner ensure path against newly built binary metadata.
- Persist `last_successful_main_head` only after both build + runner refresh succeed.

5. Failure behavior.
- Persist `last_attempt_main_head` and `last_error`.
- Return non-fatal tick diagnostics if possible; if bubbling error, ensure daemon run loop continues (already true) and emits operator-visible event.

6. Optional fast-path.
- After `PostMergeOutcome::Completed` in direct-merge flow, trigger same refresh helper immediately to reduce stale window.

## Assumptions
- `TASK-042` AO task JSON is absent in this worktree snapshot; implementation will proceed using task prompt + code evidence.
- Running `cargo ao-bin-build` from daemon host environment is acceptable in runtime policy.
- Restarting runner is safe only when no active agents; otherwise defer until next eligible tick.
- `main` is the authoritative default branch for this repository (already enforced in merge config).

## Risks and Mitigations
- Risk: repeated failed rebuild attempts every tick can spam errors.
- Mitigation: persist attempt metadata and apply backoff/debounce before retry.

- Risk: restarting runner during active agent runs could terminate in-flight work.
- Mitigation: gate rebuild/restart on `active_agents == 0`; defer otherwise.

- Risk: remote `origin/main` updates are not visible locally without fetch.
- Mitigation: use local refs for deterministic baseline; optionally add low-frequency `git fetch --no-tags origin main` before comparison.

- Risk: rebuild latency impacts scheduler throughput.
- Mitigation: only run when `main` commit changes and not already successfully processed.

## Validation Plan (Implementation Phase)
- Unit tests for refresh-state read/write + commit-change detection helpers.
- Unit/integration tests in daemon scheduler for:
  - no-op when `main` unchanged
  - deferred rebuild when active agents > 0
  - successful stamp advancement after build + `daemon.start()`
- Regression checks:
  - `cargo test -p orchestrator-cli daemon_scheduler`
  - `cargo test -p orchestrator-core daemon_impl`
  - `cargo ao-bin-build`

## External Blockers
- None.
