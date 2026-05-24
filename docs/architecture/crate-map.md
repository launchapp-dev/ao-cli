# Crate Map

The Animus workspace is a Cargo workspace of around 20 crates organized by responsibility.

## Foundation

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [protocol](../../crates/protocol/README.md) | Shared wire and config types | IPC messages, config schemas, scoped runtime paths, CLI JSON envelopes |

## Core

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [orchestrator-core](../../crates/orchestrator-core/README.md) | Domain logic and services | `ServiceHub`, service APIs, workflow lifecycle, task and requirement state mutation |
| [orchestrator-config](../../crates/orchestrator-config/README.md) | Workflow and runtime configuration | Workflow YAML parsing, pack loading, scaffolding, phase plan resolution |
| [orchestrator-store](../../crates/orchestrator-store/README.md) | Persistence primitives | Atomic JSON writes, repo-scoped state directory helpers |
| `orchestrator-logging` | Shared logging support | Structured logging helpers and runtime log plumbing |

## Runtime

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [orchestrator-daemon-runtime](../../crates/orchestrator-daemon-runtime/README.md) | Daemon scheduling runtime | Queue execution, reactive dispatch, subprocess supervision |
| `workflow-runner-v2` | Workflow execution runtime | Phase execution, state-machine transitions, checkpoint persistence |
| [agent-runner](../../crates/agent-runner/README.md) | Agent process runner | IPC server, AI CLI execution, output parsing |

## CLI

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [orchestrator-cli](../../crates/orchestrator-cli/README.md) | Main `animus` binary | Clap command surface, JSON output, MCP server, operational commands |

## Web

The web stack is no longer in-tree. `animus web` discovers and spawns three
standalone plugins from
[`launchapp-dev`](https://github.com/launchapp-dev): `animus-transport-http`
(REST + SSE), `animus-transport-graphql` (GraphQL), and `animus-web-ui` (React
dashboard wrapped as a `web_ui` plugin). Install them with
`animus plugin install-defaults --include-transports`. See
[docs/guides/web-dashboard.md](../guides/web-dashboard.md).

## Integration

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [orchestrator-providers](../../crates/orchestrator-providers/README.md) | Provider integrations | Built-in task, requirement, subject, and git adapters |
| [orchestrator-notifications](../../crates/orchestrator-notifications/README.md) | Notification delivery | Webhook and runtime notification support |
| [orchestrator-git-ops](../../crates/orchestrator-git-ops/README.md) | Git automation | Branching, worktree, merge, and PR helper operations |

## Model and Runner Adapters

| Crate | Description | Key Responsibility |
|-------|-------------|-------------------|
| [llm-cli-wrapper](../../crates/llm-cli-wrapper/README.md) | AI CLI abstraction layer | Claude, Codex, Gemini, and related CLI integration |
| [oai-runner](../../crates/oai-runner/README.md) | OpenAI-compatible runner | Streaming API execution for OpenAI-compatible endpoints |
