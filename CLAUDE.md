# Animus - Coding Agent Guide

This file is the current working brief for AI coding agents operating in this repo.
If a statement here conflicts with source, update the docs and follow source.

For the v0.4.0 naming + plugin extraction, see [docs/architecture/naming-contract.md](docs/architecture/naming-contract.md), [docs/architecture/subject-backend-plugins.md](docs/architecture/subject-backend-plugins.md), and the migration guide at [docs/migration/v0.3-to-v0.4.md](docs/migration/v0.3-to-v0.4.md).

## Verify Before Repeating

This repo has had stale prose before. Verify against these files before restating
architecture, command counts, routes, or state paths:

- `Cargo.toml`
- `docs/reference/cli/index.md`
- `docs/reference/mcp-tools.md`
- `crates/orchestrator-cli/src/cli_types/root_types.rs`
- `crates/orchestrator-core/src/config.rs`
- `crates/orchestrator-core/src/services.rs`
- `crates/protocol/src/config.rs`
- `crates/protocol/src/repository_scope.rs`
- `crates/orchestrator-web-server/web-ui/package.json`
- `crates/orchestrator-web-server/web-ui/src/app/router.tsx`

## Current Baseline

Animus is a Rust-only agent orchestrator with:

- a Cargo workspace of around 22 first-class crates (`crates/` currently lists ~30 directory entries; the extra entries are nested test fixtures and helper crates such as `animus-plugin-smoke` and `animus-provider-mock`)
- the CLI binary named `animus`
- a visible CLI surface that includes `project` and `queue`
- hidden `review` and `qa` command trees
- scoped runtime state under `~/.animus/<repo-scope>/`
- project-local workflow YAML overlays under `.animus/workflows.yaml` or `.animus/workflows/*.yaml`
- a React 18 web UI in `crates/orchestrator-web-server/web-ui`
- a stdio plugin host (`orchestrator-plugin-host`) for subject and provider plugins
- the v0.4.0 plugin extraction **complete**: 8 standalone repositories live at <https://github.com/launchapp-dev> (`animus-protocol`, `animus-plugin-template`, `animus-subject-linear`, plus `animus-provider-{claude,codex,gemini,opencode,oai}`), each tagged `v0.1.0` with green CI. Only `animus-provider-mock` and `animus-plugin-smoke` remain in-tree, as test fixtures.

Do not reintroduce stale claims such as:

- 9-crate or 10-crate workspace summaries
- "plugin extraction in flux" or "in progress" framing — v0.4.0 plugin extraction is shipped
- `PROJECT_ROOT` or "last-project-root registry" resolution rules
- removed crates like `llm-mcp-server`
- outdated CLI groups such as a top-level `planning` facade
- React 19, `urql`, or other old web UI stack descriptions

## Workspace Map

Core orchestration:

- `crates/orchestrator-cli`
- `crates/orchestrator-core`
- `crates/orchestrator-config`
- `crates/orchestrator-store`
- `crates/protocol`

Runtime and provider layer:

- `crates/agent-runner`
- `crates/llm-cli-wrapper`
- `crates/oai-runner`
- `crates/workflow-runner-v2`
- `crates/orchestrator-daemon-runtime`
- `crates/orchestrator-providers`
- `crates/orchestrator-git-ops`
- `crates/orchestrator-notifications`

Web surface:

- `crates/orchestrator-web-contracts`
- `crates/orchestrator-web-api`
- `crates/orchestrator-web-server`

## Root Resolution And State

Project root resolution is currently:

1. `--project-root`
2. git common root for the current cwd or linked worktree
3. current working directory

Do not document environment-variable fallbacks unless you add them in code.

State layout is split:

- Project-local `.animus/` stores repo config and workflow YAML overlays.
- Scoped runtime state lives in `~/.animus/<repo-scope>/`.
- Global config lives in `protocol::Config::global_config_dir()` and can be overridden with `ANIMUS_CONFIG_DIR`.

Important current paths:

- Project-local config: `.animus/config.json`
- Project-local daemon settings: `.animus/pm-config.json`
- Workflow YAML overlays: `.animus/workflows.yaml`, `.animus/workflows/*.yaml`
- Scoped runtime root: `~/.animus/<repo-scope>/`
- Compiled workflow config: `~/.animus/<repo-scope>/config/workflow-config.v2.json`
- Agent runtime config: `~/.animus/<repo-scope>/config/agent-runtime-config.v2.json`
- State machines: `~/.animus/<repo-scope>/config/state-machines.v1.json`
- Runs: `~/.animus/<repo-scope>/runs/`
- Artifacts: `~/.animus/<repo-scope>/artifacts/`

Legacy readers still probe older repo-local run/artifact paths. Preserve compatibility when needed,
but write new features against the scoped runtime root.

## Working Rules

- Keep the repo Rust-only. Do not add `tauri`, `wry`, `tao`, `gtk`, `webkit*`, `webview*`, or similar desktop-shell dependencies.
- Treat Animus JSON state as tool-managed. Use CLI commands instead of hand-editing `.animus/*.json` or scoped state JSON.
- Supported hand-edit exception: workflow YAML overlays in `.animus/workflows.yaml` and `.animus/workflows/*.yaml`.
- In scripts, CI snippets, and automation, pass `--project-root "$(pwd)"`.
- If you change CLI behavior, update `docs/reference/cli/index.md`.
- If you change MCP tools, update `docs/reference/mcp-tools.md` and `docs/guides/agents.md`.
- Everything is `animus`. New MCP tools are named `animus.<group>.<verb>`, env vars are `ANIMUS_*`, state paths are `.animus/` or `~/.animus/<repo-scope>/`, pack ids are `animus.*`. The CLI is invoked via `animus`. The legacy `ao.*` surfaces were dropped in v0.4.0 (no aliases). See [docs/architecture/naming-contract.md](docs/architecture/naming-contract.md).
- Workflow YAML supports `${VAR}` env-var interpolation for non-secret config (URLs, team IDs, feature flags), with `${VAR:-default}` and `${VAR:?error}` fallback shapes; substitution happens before YAML parsing, and unset required vars fail with file path + line number. API keys and other credentials do NOT belong in workflow YAML — each plugin reads its own secrets from the daemon's process environment (e.g. `LINEAR_API_TOKEN`, `OPENAI_API_KEY`). See `docs/reference/configuration.md#workflow-yaml-interpolation-non-secret-config`.
- Plugin kill-switches: `ANIMUS_DAEMON_DISABLE_TRIGGERS=1` skips the trigger plugin supervisor on daemon start (and interrupts in-progress restart backoff); `ANIMUS_PROVIDER_DISABLE_PLUGIN=1` forces `SessionBackendResolver` to bypass installed provider plugins and dispatch through in-tree backends only. Both require a daemon restart to take effect and to re-enable. Documented in `docs/reference/configuration.md#plugin-kill-switches`.
- Prefer narrow verification over full-workspace rebuilds while iterating.

## Implementation Landmarks

CLI and dispatch:

- `crates/orchestrator-cli/src/main.rs`
- `crates/orchestrator-cli/src/cli_types/root_types.rs`
- `crates/orchestrator-cli/src/cli_types/`
- `crates/orchestrator-cli/src/shared/output.rs`

Core services and state:

- `crates/orchestrator-core/src/config.rs`
- `crates/orchestrator-core/src/services.rs`
- `crates/orchestrator-core/src/services/`
- `crates/orchestrator-core/src/workflow/`

Workflow and runtime config:

- `crates/orchestrator-config/src/workflow_config/`
- `crates/orchestrator-config/src/agent_runtime_config.rs`
- `crates/workflow-runner-v2/src/`

Web UI:

- `crates/orchestrator-web-server/web-ui/src/app/router.tsx`
- `crates/orchestrator-web-server/web-ui/src/app/`
- `crates/orchestrator-web-server/web-ui/src/lib/graphql/`

## CLI Reality Check

Visible top-level command groups currently include:

- `daemon`, `agent`, `project`, `queue`, `workflow`, `subject`
- `history`, `git`, `skill`, `model`, `runner`
- `status`, `output`, `mcp`, `web`, `init`, `doctor`
- `pack`, `plugin`, `trigger`, `logs`

Hidden but implemented: none currently.

Tasks and requirements live exclusively under the unified
`animus subject --kind <kind>` surface as of v0.4.4. The legacy
`animus task` and `animus requirements` command trees were removed;
the in-tree `InTreeTaskSubjectBackend` and
`InTreeRequirementsSubjectBackend` adapters keep the underlying
`orchestrator-core` task and requirements services backing
`~/.animus/<repo-scope>/` state. Set
`ANIMUS_DAEMON_DISABLE_BUILTIN_TASK_ADAPTER=1` or
`ANIMUS_DAEMON_DISABLE_BUILTIN_REQUIREMENTS_ADAPTER=1` to opt out of
either in-tree adapter. External subject_backend plugins claiming
`kind=task` or `kind=requirement` automatically displace the in-tree
adapter (the `SubjectRouter` rejects duplicate kinds at startup).

Also removed in v0.4.4: `animus cloud`, `animus setup`, `animus now`,
and `animus errors`. Use `animus init` (onboarding), `animus status`
(unified inbox + status), and `animus history` (operational error
history) instead. The matching MCP tool families (`ao_task_*`,
`ao_requirements_*`, `ao_cloud_*`, `ao_errors_*`) were dropped at the
same time.

Subject CLI verbs available against any registered backend:

- `animus subject list --kind <kind>`
- `animus subject get --kind <kind> --id <id>`
- `animus subject create --kind <kind> --title <title> [...]`
- `animus subject update --kind <kind> --id <id> [...]`
- `animus subject next --kind <kind>`              (highest-priority Ready)
- `animus subject status --kind <kind> --id <id> --status <s>`

Use `cargo run -p orchestrator-cli -- --help` or `docs/reference/cli/index.md`
when changing or documenting the command tree.

## Service Model

The main production hub is `FileServiceHub`. Tests commonly use `InMemoryServiceHub`.
If you touch orchestration behavior, look for both implementations and update tests accordingly.

Keep these patterns intact:

- CLI output uses the `animus.cli.v1` envelope for `--json`
- state mutations flow through service APIs, not ad hoc file writes
- workflow YAML overlays compile into generated runtime config under scoped state
- git/worktree behavior is repo-scope aware

## Build And Test Commands

Rust:

```bash
cargo animus-fmt
cargo animus-lint
cargo animus-bin-check
cargo test -p orchestrator-cli
cargo test --workspace
```

Web UI:

```bash
cd crates/orchestrator-web-server/web-ui
npm test
npm run typecheck
npm run build
```

Prefer targeted crate or package tests while iterating. Use workspace-wide checks when the change
crosses crate boundaries or touches shared contracts.

## Web UI Notes

The embedded UI currently uses:

- React 18
- React Router 7
- `@tanstack/react-query`
- `graphql-request`
- Tailwind CSS 4
- `next-themes`
- Base UI and local UI components under `src/components/ui/`

If GraphQL contracts change, verify the Rust schema export path and regenerate client types.

## Animus-Managed Workflow

Animus is meant to self-host its planning and execution state.

Common flow (legacy task surface; both `animus task` and `animus subject
--kind task` operate on the same in-tree state):

```bash
animus task next
animus task status --id TASK-XXX --status in-progress
animus workflow run --task-id TASK-XXX
animus queue list
animus daemon health
```

Equivalent via the unified subject surface:

```bash
animus subject next --kind task
animus subject status --kind task --id task:TASK-XXX --status in-progress
animus workflow run --task-id TASK-XXX
```

If a task is specifically about persistence or migrations, it can justify direct state-file work.
Otherwise, treat Animus state as a command surface, not a manual editing target.
