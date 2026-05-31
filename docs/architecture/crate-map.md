# Crate Map

The Animus workspace is a Cargo workspace of 18 crates organized by runtime
responsibility. `Cargo.toml` is the source of truth for membership.

## Foundation

| Crate | Responsibility |
|---|---|
| `protocol` | Shared protocol, config, repository-scope, and CLI JSON envelope types |
| `orchestrator-store` | Atomic persistence helpers and repo-scoped state directory support |
| `orchestrator-logging` | Shared tracing, log path, and runtime log plumbing |

## Runtime

| Crate | Responsibility |
|---|---|
| `orchestrator-daemon-runtime` | Daemon queue, scheduling, subject dispatch, trigger handling, and runtime supervision |
| `workflow-runner-v2` | Workflow phase execution, checkpoint persistence, and runtime contracts |
| `agent-runner` | Runner process that launches and supervises provider sessions |
| `oai-runner` | OpenAI-compatible runner binary |

## CLI and Services

| Crate | Responsibility |
|---|---|
| `orchestrator-cli` | Main `animus` binary, clap surface, MCP server, output formatting, and operations |
| `orchestrator-core` | Domain services, bootstrap, state mutation APIs, plugin registry, and preflight |
| `orchestrator-config` | Workflow YAML loading, pack loading, scaffolding, and phase plan resolution |
| `orchestrator-git-ops` | Branch, worktree, merge, and PR helper operations |
| `orchestrator-notifications` | Notification/runtime integration support |
| `orchestrator-providers` | Provider-facing adapter glue and compatibility helpers |

## Plugin Runtime

| Crate | Responsibility |
|---|---|
| `orchestrator-plugin-host` | Plugin discovery, install lockfiles, manifest probes, stdio host, router, and signature verification |
| `orchestrator-session-host` | Provider plugin session bridge used by `agent-runner` |
| `animus-plugin-protocol` | In-tree copy of the stdio plugin protocol types |
| `animus-subject-protocol` | Normalized subject schema, subject backend trait, and subject errors |
| `animus-plugin-runtime` | Runtime helper crate for plugin implementations |

The workspace also depends on external `launchapp-dev/animus-protocol` crates
for provider/session contracts, currently through `animus-provider-protocol` and
`animus-session-backend` in `Cargo.toml`.

## Repo-Local Fixtures

| Crate | Responsibility |
|---|---|
| `animus-provider-mock` | Mock provider plugin for contract and e2e tests |
| `animus-plugin-smoke` | Smoke-test plugin for host and protocol behavior |
| `orchestrator-web-server` | Legacy in-repo web server directory retained outside the current Cargo workspace |

## Web

The active web stack is not part of the current Cargo workspace. `animus web`
discovers and spawns external plugins, normally installed through:

```bash
animus plugin install-defaults --include-transports
```

The curated transport set is currently `animus-transport-http`,
`animus-transport-graphql`, and `animus-web-ui`. The exact tags live in
`orchestrator-core::plugin_registry`.
