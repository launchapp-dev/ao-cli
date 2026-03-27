# Data Layout

AO stores authored project config under `.ao/` and repo-scoped runtime state
under `~/.ao/<repo-scope>/`.

## Project Layout

```text
.ao/
├── config.json
├── pm-config.json
├── plugins/
├── workflows.yaml
├── workflows/
```

Key points:

- `.ao/workflows.yaml` and `.ao/workflows/*.yaml` are project-authored workflow
  sources
- `.ao/plugins/` contains project-local pack overrides
- `.ao/config.json` and `.ao/pm-config.json` hold project configuration

## Repo-Scoped Runtime Layout

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

Key points:

- `~/.ao/<repo-scope>/` holds AO-managed runtime state for a specific
  repository scope
- `~/.ao/<repo-scope>/state/pack-selection.v1.json` tracks active pack pins and
  enablement
- `~/.ao/<repo-scope>/runs/` and `~/.ao/<repo-scope>/artifacts/` contain
  execution records and artifacts
- `~/.ao/<repo-scope>/logs/` stores structured runtime logs
- `~/.ao/<repo-scope>/worktrees/` stores managed task worktrees

### Installed packs

```text
~/.ao/packs/<pack-id>/<version>/
```

### Global config

```text
~/.ao/config.json
```

These stores serve different purposes:

- `~/.ao/config.json` is the machine-local user config
- `~/.ao/packs/` is the machine pack registry
- `~/.ao/<repo-scope>/...` is repo-scoped runtime state

## Mutation Policy

Do not hand-edit AO state files. Use AO commands or AO MCP tools unless you are
explicitly working on AO persistence as part of a migration.

## Resolution-Related Paths

| Path | Purpose |
|---|---|
| `.ao/plugins/<pack-id>/` | Project-local pack override root |
| `.ao/workflows.yaml` | Single-file project workflow source |
| `.ao/workflows/*.yaml` | Multi-file project workflow sources |
| `~/.ao/<repo-scope>/state/pack-selection.v1.json` | Repo-scoped pack pin/enablement state |
| `~/.ao/<repo-scope>/state/*.json` | Repo-scoped review, history, error, and QA state |
| `~/.ao/<repo-scope>/worktrees/` | Managed task worktrees |
| `~/.ao/packs/<pack-id>/<version>/` | Machine-installed pack root |

See also: [Configuration](configuration.md), [Workflows](../concepts/workflows.md).
