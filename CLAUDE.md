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

- a Cargo workspace of around 20 crates (in flux during the v0.4.0 plugin extraction)
- the CLI binary named `animus`
- a visible CLI surface that includes `project` and `queue`
- hidden `review` and `qa` command trees
- scoped runtime state under `~/.animus/<repo-scope>/`
- project-local workflow YAML overlays under `.animus/workflows.yaml` or `.animus/workflows/*.yaml`
- a React 18 web UI in `crates/orchestrator-web-server/web-ui`
- a stdio plugin host (`orchestrator-plugin-host`) for subject and provider plugins

Do not reintroduce stale claims such as:

- 9-crate or 10-crate workspace summaries
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

- `daemon`, `agent`, `project`, `queue`, `task`, `workflow`
- `vision`, `requirements`, `architecture`
- `history`, `errors`, `git`, `skill`, `model`, `runner`
- `status`, `output`, `mcp`, `web`, `setup`, `tui`, `doctor`

Hidden but implemented:

- `review`
- `qa`

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

Common flow:

```bash
animus task next
animus task status --id TASK-XXX --status in-progress
animus workflow run --task-id TASK-XXX
animus queue list
animus daemon health
```

If a task is specifically about persistence or migrations, it can justify direct state-file work.
Otherwise, treat Animus state as a command surface, not a manual editing target.
