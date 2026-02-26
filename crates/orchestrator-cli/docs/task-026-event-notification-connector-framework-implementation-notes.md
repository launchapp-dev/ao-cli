# TASK-026 Implementation Notes: Event Notification Connector Framework

## Purpose
Translate TASK-026 requirements into implementation slices for daemon
notifications while preserving existing daemon scheduling reliability.

## Non-Negotiable Constraints
- Keep all implementation in Rust crates under `crates/`.
- Preserve existing daemon event emission contract (`ao.daemon.event.v1`).
- Prevent secret leakage: never persist raw credentials in `.ao`, JSONL logs, or
  emitted event payloads.
- Keep notification failures isolated from workflow/task execution progress.
- Avoid manual `.ao` JSON edits; rely on command-driven persistence flows.

## Proposed Change Surface

### Runtime daemon modules
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_run.rs`
  - wire notification enqueue + flush into event emission lifecycle.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_events.rs`
  - reuse `DaemonEventRecord` envelope for connector dispatch inputs.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`
  - add notification-specific modules (suggested split):
    - `daemon_notifications.rs` (orchestration API),
    - `daemon_notifications_config.rs` (schema + validation),
    - `daemon_notifications_connectors.rs` (adapter registry + connector impls),
    - `daemon_notifications_queue.rs` (outbox/dead-letter persistence + retry).

### CLI and config plumbing
- `crates/orchestrator-cli/src/cli_types.rs`
  - add daemon config arguments required to manage notification configuration.
- `crates/orchestrator-cli/src/services/runtime/runtime_daemon.rs`
  - load/save notification configuration through daemon command handlers.

### Tests
- daemon runtime unit tests in notification modules.
- integration coverage in existing daemon runtime test areas for:
  - enqueue/flush behavior,
  - retry/dead-letter transitions,
  - resilience under connector failure.

## Data Contract Draft

Suggested notification config shape:

```json
{
  "schema": "ao.daemon-notification-config.v1",
  "version": 1,
  "connectors": [
    {
      "id": "ops-webhook",
      "type": "webhook",
      "enabled": true,
      "config": {
        "url_env": "AO_NOTIFY_WEBHOOK_URL",
        "headers_env": {
          "Authorization": "AO_NOTIFY_WEBHOOK_TOKEN"
        },
        "timeout_secs": 10
      }
    }
  ],
  "subscriptions": [
    {
      "id": "critical-phase-failures",
      "enabled": true,
      "connector_id": "ops-webhook",
      "event_types": [
        "workflow-phase-contract-violation",
        "notification-delivery-dead-lettered"
      ],
      "project_root": null,
      "workflow_id": null,
      "task_id": null
    }
  ],
  "retry_policy": {
    "max_attempts": 5,
    "base_delay_secs": 2,
    "max_delay_secs": 300
  }
}
```

Queue entry shape should include:
- `delivery_id`, `delivery_key`, `event_id`,
- `connector_id`, `subscription_id`,
- `attempts`, `next_attempt_unix_secs`,
- redacted `last_error`,
- connector payload.

## Adapter Contract Draft
- `NotificationConnector` behavior:
  - `connector_type()` for routing,
  - `validate_config()` for startup-time guardrails,
  - `send(delivery)` returning typed success/failure classification.
- Failure classification categories:
  - `transient` (retry),
  - `permanent` (dead-letter),
  - `misconfigured` (dead-letter + config guidance).

## Retry and Dead-Letter Strategy
- Reuse deterministic outbox patterns from `daemon_scheduler_git_ops.rs`:
  - JSONL persistence,
  - temp-file + rename writes,
  - bounded exponential backoff.
- Suggested paths under repo-scoped AO runtime root:
  - `~/.ao/<repo-scope>/notifications/outbox.jsonl`,
  - `~/.ao/<repo-scope>/notifications/dead-letter.jsonl`.
- Deduplicate queued deliveries by deterministic delivery key.

## Execution Sequence
1. Add config model + validation and command plumbing.
2. Add adapter registry and minimal connector implementations.
3. Add queue persistence and retry/dead-letter processor.
4. Integrate enqueue on matched daemon events.
5. Flush queue during daemon ticks with bounded processing budget.
6. Emit notification lifecycle daemon events.
7. Add deterministic tests and update operator docs.

## Testing Plan
- Unit tests:
  - subscription matching and wildcard semantics,
  - credential reference resolution + redaction,
  - retry classification and backoff math.
- Integration tests:
  - successful delivery path removes queue entry,
  - transient failure retries then succeeds,
  - permanent failure routes to dead-letter.
- Resilience tests:
  - connector failures do not abort daemon tick execution.

## Risks and Mitigations
- Risk: connector errors slow daemon tick throughput.
  - Mitigation: cap delivery flush operations per tick and keep retry async.
- Risk: accidental secret exposure in events/errors.
  - Mitigation: central redaction utility applied before all persistence/emits.
- Risk: duplicate delivery from repeated enqueue.
  - Mitigation: deterministic delivery key and outbox dedupe check.
