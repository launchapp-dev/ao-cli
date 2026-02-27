# TASK-109 Requirements: Implement Bounded Agent Pool with Tokio Semaphore

## Phase
- Workflow phase: `requirements`
- Workflow ID: `e6ebf911-74f1-4a89-803a-898d5c120132`
- Task: `TASK-109`
- Requirement: unlinked in current task metadata

## Objective
Introduce a standalone daemon concurrency pool that enforces bounded agent
execution with `tokio::sync::Semaphore`, while keeping scheduling/selection
decisions outside the pool.

Target behavior from task brief:
- add `AgentPool` in a new `daemon_agent_pool.rs` module,
- own semaphore permits based on configured pool size (`--max-agents` /
  `AO_MAX_AGENTS` derived value),
- support non-blocking spawn admission (`try_spawn`) and graceful shutdown
  (`drain`),
- forward agent completion results to a coordinator-facing `mpsc` channel,
- track active/spawned/completed/failed counters deterministically.

## Current Baseline Audit
Snapshot date: `2026-02-27`.

| Surface | Current location | Current behavior | Gap |
| --- | --- | --- | --- |
| Running-phase execution batching | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs` (`execute_running_workflow_phases_for_project`) | builds scheduled runs inline, spawns all with `tokio::task::JoinSet`, then awaits completions in a batch loop | no reusable bounded pool abstraction and no non-blocking admission API |
| Tick-level phase execution budget | `daemon_scheduler_project_tick.rs` (`project_tick`) | calls `execute_running_workflow_phases_for_project(..., args.max_tasks_per_tick)` once per tick | concurrency and lifecycle management are coupled to tick loop |
| Agent-cap configuration source | `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs` + `crates/orchestrator-core/src/services/runner_helpers.rs` | daemon start/run sets `AO_MAX_AGENTS`; core reads env override and exposes `max_agents` in daemon health | no dedicated runtime pool type consuming this bound directly |
| Completion handling | `daemon_scheduler_project_tick.rs` `JoinSet` join loop | completion processing and workflow/task updates are inlined in scheduler function | no channel handoff abstraction to support reactive coordinator fan-in |

## Problem Statement
Daemon phase execution currently relies on inline `JoinSet` management in the
tick function. This makes concurrency limits, spawn admission, completion
handoff, and graceful drain behavior tightly coupled to one call site, which
blocks the next reactive coordinator slice and makes bounded execution behavior
harder to reuse and validate in isolation.

## Scope
In scope for implementation after this phase:
- Add `daemon_agent_pool.rs` with `AgentPool` as a standalone runtime utility.
- Introduce semaphore-backed capacity control for agent execution slots.
- Add non-blocking spawn admission (`try_spawn`) that returns `false` when pool
  is full or no longer accepting work.
- Ensure each spawned agent releases its permit on completion (including failure
  paths).
- Send agent completion payloads to a coordinator-facing `mpsc` receiver.
- Track pool metrics:
  - `active_count`
  - `total_spawned`
  - `total_completed`
  - `total_failed`
- Add `drain` behavior that stops new admissions and waits for running agents to
  finish.
- Add focused tests for capacity bounds, non-blocking behavior, counter
  integrity, and drain semantics.

Out of scope:
- Deciding which work item to run next (covered by `WorkQueue` task).
- Replacing the daemon loop with a reactive `select!` coordinator (next batch).
- Changing workflow decision semantics (`advance/rework/fail`) in result
  processing.
- Manual edits to `/.ao/*.json`.

## Constraints
- Pool behavior must be deterministic for identical admission/completion order.
- `try_spawn` must remain non-blocking (no await on permit acquisition).
- Pool must remain scheduler-agnostic: it accepts slots and reports completions
  only.
- Permit lifecycle must be leak-safe under success/error/panic paths.
- `drain` must terminate only after all running agents are complete and no new
  spawns are accepted.
- Counter reads must be thread-safe and race-tolerant without panics.
- Change set remains Rust-only and repository-safe.

## Agent Pool Contract

### Core API
- `AgentPool::new(pool_size: usize) -> Self`
- `try_spawn(slot: AgentSlot) -> bool`
- `drain(...)` (async wait for in-flight tasks after admissions are closed)
- `active_count() -> usize`
- `is_full() -> bool`

### Admission Semantics
- `try_spawn` returns `true` only when:
  - pool is accepting work, and
  - a semaphore permit is immediately available.
- `try_spawn` returns `false` when:
  - pool is full, or
  - pool has entered drain/closed state.

### Completion Semantics
- Every admitted slot results in exactly one completion send attempt to the
  pool’s completion channel.
- Completion payload includes enough context for coordinator-side result
  processing (task/workflow/phase context carried by `AgentSlotResult`).
- Completion send failures (for example receiver dropped) must not leak permits
  or block shutdown.

### Counter Semantics
- `active_count`: in-flight agents currently holding permits.
- `total_spawned`: count of slots admitted into execution.
- `total_completed`: count of admitted slots that reached completion handling.
- `total_failed`: count of admitted slots that end in runtime/agent failure
  classification.

### Drain Semantics
- Drain transitions pool to no-admit mode.
- After drain starts, all future `try_spawn` calls return `false`.
- Drain waits until all in-flight agents finish and `active_count == 0`.
- Drain is idempotent and safe to call multiple times.

## Functional Requirements

### FR-01: New Pool Module
Create `daemon_agent_pool.rs` with `AgentPool` and related pool-internal types.

### FR-02: Bounded Concurrency via Semaphore
Pool must enforce a hard upper bound of concurrent active agents equal to
`pool_size`.

### FR-03: Non-Blocking Spawn Admission
`try_spawn` must not block; it must return immediate success/failure based on
pool state and permit availability.

### FR-04: Completion Channel Handoff
Agent completion must be published through an `mpsc` channel consumed by the
coordinator layer.

### FR-05: Permit Release on All Paths
Permits must be released on normal completion and failure paths.

### FR-06: Counter Tracking
Pool must maintain accurate active/spawned/completed/failed counters under
concurrent execution.

### FR-07: Drain Behavior
Pool must support explicit drain that closes admissions and waits for currently
running agents.

### FR-08: Scheduler-Agnostic Boundary
Pool must not perform queue prioritization, dependency gating, or workflow
state transitions.

### FR-09: Integration Readiness
Pool API and completion stream must be suitable for subsequent reactive
coordinator wiring.

### FR-10: Regression Coverage
Add tests for bounds, admission, completion signaling, counters, and drain.

## Acceptance Criteria
- `AC-01`: New module `daemon_agent_pool.rs` exists and compiles with
  `AgentPool::new(pool_size: usize) -> Self`.
- `AC-02`: `try_spawn` returns `false` immediately when pool is at capacity.
- `AC-03`: concurrent active agents never exceed configured pool size.
- `AC-04`: admitted agents release permits on completion, enabling subsequent
  admissions.
- `AC-05`: completion events are sent via channel for admitted agents.
- `AC-06`: `active_count`, `total_spawned`, `total_completed`, and
  `total_failed` are observable and consistent with execution outcomes.
- `AC-07`: `drain` stops new admissions and waits for in-flight agents to
  finish.
- `AC-08`: pool implementation does not embed task selection/prioritization
  logic.

## Testable Acceptance Checklist
- `T-01`: pool initialized with size `N` admits at most `N` concurrent slots.
- `T-02`: when all permits are consumed, `try_spawn` returns `false` without
  waiting.
- `T-03`: after one running slot completes, `try_spawn` can admit another slot.
- `T-04`: completion channel receives one event per admitted slot.
- `T-05`: counters move monotonically with expected values across success/fail
  slot outcomes.
- `T-06`: `drain` blocks until all in-flight slots finish and then returns with
  zero active agents.
- `T-07`: after drain starts, additional `try_spawn` calls return `false`.

## Verification Matrix

| Requirement area | Verification method |
| --- | --- |
| FR-01, FR-02, FR-03 | unit tests in `daemon_agent_pool.rs` for permit/admission behavior |
| FR-04, FR-05 | pool tests asserting completion send + permit release |
| FR-06 | pool tests for counter invariants under mixed outcomes |
| FR-07 | async drain tests with delayed slot completion |
| FR-08, FR-09 | code review checks ensuring no scheduling policy logic in pool |
| FR-10 | targeted `cargo test -p orchestrator-cli` for daemon runtime modules |

## Implementation Notes Input (Next Phase)
Primary expected change targets:
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_agent_pool.rs` (new)
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler.rs`
  (module wiring/import surface)
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_project_tick.rs`
  (temporary call-site adaptation, if needed for compilation)

Likely supporting surfaces:
- `AgentSlot` and `AgentSlotResult` abstractions produced by dependency task
  `TASK-108`.
- daemon runtime tests under
  `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`.

## Deterministic Deliverables for Implementation Phase
- Semaphore-bounded `AgentPool` module with non-blocking admission.
- Completion channel handoff for coordinator consumption.
- Accurate active/spawned/completed/failed pool metrics.
- Drain semantics that safely quiesce in-flight agent execution.
- Focused tests proving bound correctness and lifecycle safety.
