# State Management

## The `.ao/` Directory

`.ao/` is AO-managed project state. Treat it as application state, not as a
hand-edited config folder. Use `ao` commands or AO MCP tools for mutations.

Typical layout:

```text
.ao/
├── config.json
├── pm-config.json
├── plugins/
├── workflows.yaml
├── workflows/
```

## Repo-Scoped Runtime State

```text
~/.ao/<repo-scope>/
├── core-state.json
├── resume-config.json
├── docs/
├── requirements/
├── tasks/
├── index/
├── state/
│   ├── pack-selection.v1.json
│   ├── state-machines.v1.json
│   ├── reviews.json
│   ├── handoffs.json
│   ├── history.json
│   ├── errors.json
│   ├── qa-results.json
│   └── qa-review-approvals.json
├── runs/
├── artifacts/
├── logs/
└── worktrees/
```

## What Lives Where

### Project YAML

Project-authored workflow configuration lives in:

- `.ao/workflows.yaml`
- `.ao/workflows/*.yaml`

These files are the editable source of truth for project-local workflows and
overrides.

### Project Pack Overrides

Per-project pack overrides live in:

- `.ao/plugins/<pack-id>/`

These directories can override installed or bundled pack workflows and runtime
assets without changing the daemon or core code.

### Repo-Scoped State

Repo-scoped runtime state lives in:

- `~/.ao/<repo-scope>/core-state.json`
- `~/.ao/<repo-scope>/resume-config.json`
- `~/.ao/<repo-scope>/state/`
- `~/.ao/<repo-scope>/requirements/`
- `~/.ao/<repo-scope>/tasks/`
- `~/.ao/<repo-scope>/docs/`

This is managed by `ao pack pin`, `ao pack install --activate`, and related AO
commands.

### Execution Data

Transient and historical execution data lives in:

- `~/.ao/<repo-scope>/runs/<run_id>/events.jsonl`
- `~/.ao/<repo-scope>/artifacts/<execution_id>/...`
- `~/.ao/<repo-scope>/state/history.json`
- `~/.ao/<repo-scope>/state/errors.json`

## Machine-Level Pack Storage

Machine-installed packs live outside the project:

```text
~/.ao/packs/<pack-id>/<version>/
```

AO also uses a repo-scoped machine directory for worktrees and related runtime
state:

```text
~/.ao/<repo-scope>/worktrees/
```

These are distinct concerns:

- `~/.ao/packs/` stores reusable installed packs
- `~/.ao/config.json` stores machine-local user config
- `~/.ao/<repo-scope>/...` stores repository-scoped runtime data

## Mutation Policy

Never hand-edit `.ao/*.json` files unless you are explicitly working on AO's
own persistence layer as part of a migration.

Approved mutation surfaces:

- CLI commands such as `ao task status`
- AO MCP tools such as `ao.task.update`
- projectors consuming execution facts
- pack commands such as `ao pack pin`

## Configuration Precedence

At a high level, AO resolves behavior in this order:

1. CLI flags and environment variables
2. Project pack overrides in `.ao/plugins/`
3. Project-local YAML in `.ao/workflows.yaml` and `.ao/workflows/*.yaml`
4. Installed packs in `~/.ao/packs/`
5. Bundled kernel workflows and bundled first-party packs

This keeps local control in the repository while preserving a stable bundled
baseline.
