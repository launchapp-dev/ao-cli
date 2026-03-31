# oai-runner Hardening Roadmap

This roadmap captures the AO-managed follow-up work created from the Claude Code
comparison performed on 2026-03-31.

## Requirements

### REQ-668

Title: Harden oai-runner shell execution safety and process lifecycle

Acceptance criteria:

- `execute_command` terminates timed-out or cancelled subprocesses reliably,
  including child processes where supported
- shell execution uses a sanitized environment by default
- command execution returns deterministic output for timeout, signal, and
  non-zero exit cases
- read-only mode continues to exclude shell execution
- targeted tests cover timeout/cancellation and environment sanitization

### REQ-669

Title: Add concurrency-safe tool orchestration to oai-runner

Acceptance criteria:

- tool calls can be classified as concurrency-safe versus serial-only
- concurrency-safe tool batches execute in parallel with a bounded limit
- tool results remain persisted in deterministic assistant order
- cancellation synthesizes matching tool results instead of leaving transcript
  gaps
- targeted tests cover parallel read/search/list batches and cancellation

### REQ-670

Title: Expand oai-runner MCP transport and result handling robustness

Acceptance criteria:

- `McpServerConfig` supports at least `stdio` and one remote transport mode
- transport-specific errors are surfaced clearly
- large MCP outputs are bounded before entering conversation state
- non-text MCP content is handled predictably
- targeted tests cover config parsing and bounded MCP result handling

### REQ-671

Title: Strengthen oai-runner structured event stream protocol

Acceptance criteria:

- NDJSON serialization escapes newline-sensitive Unicode separators
- structured event shapes stay compatible where practical
- tests cover NDJSON-safe serialization
- text mode remains unchanged for human-readable runs

### REQ-672

Title: Improve oai-runner context pressure recovery

Acceptance criteria:

- preflight compaction remains available
- runner gains at least one additional safe recovery path for context or output
  pressure failures
- session persistence stays coherent across recovery
- targeted tests cover the added recovery path

## Tasks

- `TASK-1979`: Implement oai-runner shell execution hardening
- `TASK-1980`: Implement concurrency-safe tool orchestration in oai-runner
- `TASK-1981`: Implement MCP transport and result-handling hardening in
  oai-runner
- `TASK-1982`: Implement NDJSON-safe structured output for oai-runner
- `TASK-1983`: Implement context-pressure recovery improvements in oai-runner

## Execution notes

- The implementation is intentionally split into disjoint file ownership so
  parallel workers can make progress without frequent merge conflicts.
- Validation should stay scoped to `oai-runner` because the workspace currently
  contains unrelated compile failures outside this crate.
