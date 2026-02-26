# TASK-026 Requirements: Event Notification Connector Framework

## Phase
- Workflow phase: `requirements`
- Workflow ID: `63e1b4b9-91ef-445a-b2f9-69fcd70a4d46`
- Task: `TASK-026`
- Linked requirement: `REQ-026`

## Objective
Define a production-ready daemon notification framework that adds:
- connector adapters for outbound notifications,
- event subscription/filtering controls,
- secure credential reference handling,
- deterministic retry and dead-letter behavior,
- implementation-ready contracts and acceptance criteria.

## Existing Baseline Audit

| Surface | Current path | Current behavior | Gap for REQ-026 |
| --- | --- | --- | --- |
| Daemon event emission | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_run.rs` + `daemon_events.rs` | Emits daemon events to JSONL and optional stdout. | No connector fan-out or notification dispatch path. |
| Phase execution event detail | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_phase_exec.rs` + `daemon_scheduler_project_tick.rs` | Emits structured phase signals (`workflow-phase-*`). | No subscription model to route selected events to external channels. |
| Retry/outbox pattern | `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_git_ops.rs` | Has durable outbox with retries for git integration operations. | No notification delivery outbox/dead-letter queue. |
| Daemon config persistence | `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs` (`.ao/pm-config.json`) | Stores daemon automation booleans. | No notification connector/subscription schema. |
| Daemon event transport contract | `ao.daemon.event.v1` in `daemon_events.rs` | Stable event envelope with id/seq/timestamp/type/data. | No notification delivery result events for observability. |

## Scope
In scope for implementation after this requirements phase:
- Add a connector adapter framework for daemon notifications.
- Add event subscription filtering using existing daemon event records.
- Add credential reference resolution with strict redaction behavior.
- Add durable retry queue and dead-letter queue for failed notification sends.
- Add docs and tests that validate safety, determinism, and failure handling.

Out of scope for this task:
- Desktop notification transports or GUI-only integrations.
- External secret manager dependencies (vault/kms SDK integration).
- Replacing existing daemon event schema `ao.daemon.event.v1`.
- Changing workflow/task execution semantics unrelated to notifications.

## Constraints
- Keep implementation Rust-only under `crates/`.
- Keep daemon tick resilient: notification failures must not crash scheduling.
- Never persist raw credential values in repo-local files or daemon event logs.
- Preserve existing `ao daemon events` behavior for current event types.
- Keep notification dispatch deterministic and idempotency-aware per
  `(event_id, connector_id)`.
- Keep retry behavior bounded and predictable (no unbounded hot loops).

## Functional Requirements

### FR-01: Connector Adapter Registry
- Introduce a typed adapter interface for notification connectors.
- Initial connector types for this task:
  - `webhook` (generic HTTP JSON POST),
  - `slack_webhook` (Slack-compatible webhook payload).
- Each connector requires:
  - stable `id`,
  - `enabled` toggle,
  - connector type-specific configuration,
  - connector type-specific credential references.

### FR-02: Subscription Model
- Add subscription entries binding daemon events to connector ids.
- Subscription filters must support:
  - `event_types` list with exact match and `*` wildcard support,
  - optional `project_root` filter,
  - optional `workflow_id` filter,
  - optional `task_id` filter.
- Unmatched events must not enqueue notification deliveries.

### FR-03: Notification Config Contract
- Define a versioned notification config payload:
  - schema id: `ao.daemon-notification-config.v1`,
  - connectors collection,
  - subscriptions collection,
  - retry policy defaults.
- Config representation must be deterministic (stable field names and ordering in
  serialized output).
- Config mutation for repo-local state must remain command-driven (no manual
  `.ao` state edits required by workflow).

### FR-04: Credential Reference Handling
- Connector credentials must be stored as references, not raw secrets.
- Minimum credential reference type for v1: environment variable name(s).
- Missing credentials must fail delivery safely with redacted diagnostics.
- Redaction policy:
  - never log resolved secret values,
  - redact authorization headers/tokens in outbox, dead-letter, and daemon
    event payloads.

### FR-05: Delivery Queue, Retry, and Dead-Letter
- Add a durable notification outbox for pending deliveries.
- Add dead-letter storage for permanently failed or exhausted deliveries.
- Delivery entry must include:
  - `delivery_id`,
  - `event_id`,
  - `connector_id`,
  - `subscription_id`,
  - `attempts`,
  - `next_attempt_unix_secs`,
  - `last_error` (redacted),
  - serialized notification payload.
- Retry policy:
  - exponential backoff with deterministic clamp bounds,
  - max attempts configurable with safe default,
  - transient failures retried (timeout/network/HTTP 5xx/429),
  - non-retriable failures (invalid config/HTTP 4xx excluding 429) dead-lettered
    immediately.

### FR-06: Dispatch Semantics
- Notification dispatch must be at-least-once per eligible
  `(event_id, connector_id, subscription_id)`.
- Deduplicate outbox enqueue operations for identical delivery keys.
- Dispatch should be non-blocking for primary daemon health/status emission;
  failures are recorded and retried asynchronously.

### FR-07: Observability
- Emit daemon events for notification lifecycle:
  - `notification-delivery-enqueued`,
  - `notification-delivery-sent`,
  - `notification-delivery-failed`,
  - `notification-delivery-dead-lettered`.
- Lifecycle events must include delivery ids, connector ids, and redacted error
  metadata.

### FR-08: Documentation and Operator Guidance
- Add operator docs for:
  - connector configuration contract,
  - subscription examples,
  - credential setup using env vars,
  - retry/dead-letter troubleshooting.
- Include safe examples that do not embed secrets.

## Non-Functional Requirements

### NFR-01: Determinism
- Identical event/config/input conditions must produce stable enqueue keys and
  retry schedule progression.

### NFR-02: Reliability
- Notification subsystem must not prevent daemon from continuing project ticks.
- Outbox/dead-letter writes must be atomic (temp file + rename pattern).

### NFR-03: Security
- Secret material must never be written to repo-local state, logs, or events.
- Error messages must preserve operator actionability while redacting secrets.

### NFR-04: Performance
- Notification processing per tick must be bounded to avoid starving workflow
  execution.

## Acceptance Criteria
- `AC-01`: Connector adapter registry supports `webhook` and `slack_webhook`
  connector types.
- `AC-02`: Subscription filters route only matching daemon events to connectors.
- `AC-03`: Credentials are resolved from references; raw secrets are never
  persisted or logged.
- `AC-04`: Notification deliveries are queued in a durable outbox with stable
  keys and attempt counters.
- `AC-05`: Transient delivery failures retry with exponential backoff and
  bounded attempt counts.
- `AC-06`: Non-retriable or exhausted deliveries move to dead-letter storage
  with redacted failure context.
- `AC-07`: Notification failures do not crash or pause daemon scheduling loops.
- `AC-08`: Notification lifecycle emits observable daemon events with redacted
  metadata.
- `AC-09`: Existing daemon event output remains backward compatible for current
  consumers.
- `AC-10`: Documentation includes setup/troubleshooting and safe credential
  guidance.

## Testable Acceptance Checklist
- `T-01`: Unit tests for subscription event matching, wildcard behavior, and
  contextual filters.
- `T-02`: Unit tests for credential resolution and redaction behavior.
- `T-03`: Unit tests for retry classifier (transient vs permanent failures).
- `T-04`: Unit tests for backoff/attempt clamp behavior.
- `T-05`: Integration test for outbox enqueue -> retry -> success flow.
- `T-06`: Integration test for permanent failure -> dead-letter flow.
- `T-07`: Regression test proving daemon tick proceeds when notification send
  fails.
- `T-08`: Snapshot/assertion tests for notification lifecycle daemon event
  payload shape.

## Acceptance Verification Matrix
| Requirement area | Verification method |
| --- | --- |
| Connector dispatch + subscriptions | Unit tests for route/match + integration enqueue assertions |
| Credential safety | Redaction tests + grep/assert no raw secret in persisted payloads |
| Retry/dead-letter behavior | Outbox lifecycle tests with deterministic clock inputs |
| Daemon resilience | Daemon run/tick tests with forced connector failures |
| Event observability | `daemon-events` payload assertions for notification lifecycle events |
| Documentation completeness | Docs review against FR-08 checklist |

## Implementation Notes (Input to Next Phase)
Recommended implementation targets:
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`
  - new module(s) for notification config, connectors, queueing, and dispatch,
  - integrate enqueue + flush points with `daemon_run.rs`,
  - reuse atomic JSONL write and retry patterns from `daemon_scheduler_git_ops.rs`.
- `crates/orchestrator-cli/src/cli_types.rs` and runtime daemon handlers
  - expose command-level config management needed for notification setup.
- `crates/orchestrator-cli/tests/` and daemon runtime test modules
  - add deterministic coverage for matching, credential redaction, retry, and
    dead-letter flows.

## Deterministic Deliverables for Implementation Phase
- Add notification config model and validation.
- Add connector adapter implementations (`webhook`, `slack_webhook`).
- Add durable notification outbox + dead-letter persistence.
- Add dispatch integration in daemon run loop with lifecycle event emission.
- Add tests for acceptance checklist and docs for operator setup/troubleshooting.
