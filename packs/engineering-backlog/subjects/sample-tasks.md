# Sample Engineering Tasks

Five realistic dev tasks for dry-running the `implement-task` workflow.
Drop this file into your tasks directory (or split it into one task per
file) and the `animus-subject-default` plugin will surface each `##`
block as a separate subject of kind `task`.

The shape each entry follows:

```
## <task-id>: <short title>
status: ready
priority: <low | med | high | critical>
labels: <comma-separated>
task_type: <bug | feature | refactor | dep-upgrade | docs>

<body — description, acceptance criteria, links to relevant code>
```

These are intentionally small (1-2 hours of work each) so the workflow
can complete end-to-end on a real codebase. Replace with your own once
the pipeline is wired up.

---

## TASK-1001: Wrong exit code when config file is missing
status: blocked
priority: high
labels: bug,cli,illustrative
task_type: bug

NOTE: This task is illustrative only — the exit-code behavior described
below may already be fixed against your checkout of ao-cli. Before
running it as a smoke test, verify the repro still produces exit 0.
For a durably-runnable first smoke test on the ao-cli tree, use
TASK-1005 (a docs task that always has surface area). The README
points at TASK-1001 because it's the most representative engineering
shape; replace the body below with a real failing bug in your own
codebase before dispatching.

Repro:
```
$ animus daemon start --project-root /tmp/no-such-dir
Error: failed to read config
$ echo $?
0
```

The CLI prints an error message but exits 0, which breaks shell scripts
that test `$?` to detect failure. Expected: exit non-zero (typically 1
or 2) whenever the daemon fails to start.

Acceptance criteria:
- `animus daemon start` exits non-zero on missing project root
- Existing successful start path still exits 0
- Add a regression test that asserts the exit code for the failing case

Likely files: `crates/orchestrator-cli/src/services/operations/ops_daemon.rs`,
`crates/orchestrator-cli/src/main.rs`.

---

## TASK-1002: Add `--dry-run` flag to `animus queue enqueue`
status: ready
priority: med
labels: feature,queue
task_type: feature

Operators want to validate an enqueue call without actually mutating the
queue — useful for CI scripts and for understanding what would happen.

Acceptance criteria:
- `animus queue enqueue --dry-run --task-id TASK-X --workflow-ref standard`
  prints the resolved enqueue plan (task id, workflow ref, priority,
  estimated queue position) and exits 0 without modifying the queue.
- Help text describes the flag.
- Add a CLI integration test that runs `--dry-run` and asserts the
  queue is unchanged.

Likely files: `crates/orchestrator-cli/src/cli_types/queue.rs`,
`crates/orchestrator-cli/src/services/operations/ops_queue.rs`.

---

## TASK-1003: Refactor `daemon_scheduler_phase_exec.rs` — extract retry policy
status: ready
priority: med
labels: refactor,scheduler
task_type: refactor

The phase exec module is 800+ lines and the retry/backoff logic is
inlined into the main exec function. Extract it into a `RetryPolicy`
struct in a new sibling module so it can be unit-tested in isolation.

Acceptance criteria:
- New file `crates/orchestrator-cli/src/services/runtime/runtime_daemon/daemon_scheduler_retry_policy.rs`
  defines `RetryPolicy` with `next_delay(attempts: u32) -> Duration` and
  `should_retry(error: &SomeErr, attempts: u32) -> bool`.
- `daemon_scheduler_phase_exec.rs` uses the new struct instead of
  inlined logic.
- At least 3 unit tests covering: first retry uses base delay,
  exponential backoff caps at max, non-retriable error returns false.
- No behavior change — the existing integration tests still pass.

Likely files: `crates/orchestrator-cli/src/services/runtime/runtime_daemon/`.

---

## TASK-1004: Bump `tokio` to 1.42
status: ready
priority: low
labels: dep-upgrade,deps
task_type: dep-upgrade

We're currently pinned to tokio 1.38. 1.42 ships several runtime perf
improvements and a fix for a slow-leak bug in `JoinSet`. Bump and
verify nothing breaks.

Acceptance criteria:
- `Cargo.toml` `tokio = "1.42"` (or workspace dep entry updated).
- `cargo update -p tokio` reflected in `Cargo.lock`.
- `cargo test --workspace` green.
- If any deprecation warnings appear, file follow-up tasks rather than
  fixing inline (out of scope).

Likely files: `Cargo.toml`, `Cargo.lock`, possibly `crates/*/Cargo.toml`
if a workspace member pins tokio directly.

---

## TASK-1005: Document `ANIMUS_DAEMON_DISABLE_TRIGGERS` env var
status: ready
priority: low
labels: docs
task_type: docs

The kill-switch env var landed in v0.4.12 but isn't documented in
`docs/reference/configuration.md`. Add an entry under "Plugin
kill-switches" with: what it does, when to use it (debugging trigger
restart loops), and that it requires a daemon restart to take effect.

Acceptance criteria:
- New subsection in `docs/reference/configuration.md` covers the
  env var.
- Mention the sibling `ANIMUS_DAEMON_DISABLE_SUBJECT_PLUGINS` for
  completeness.
- No source code changes.

Likely files: `docs/reference/configuration.md`.
