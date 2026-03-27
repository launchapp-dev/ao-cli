# Project Setup

## What `ao setup` Does

`ao setup` initializes the repository-scoped AO workspace. It:

1. resolves the project root
2. creates the `.ao/` directory tree
3. scaffolds project-local workflow YAML
4. prepares AO-managed state in the machine-scoped runtime directory
5. leaves bundled workflows and bundled first-party packs available by default

`ao setup` does not copy bundled task or requirement logic into your repo. That
behavior is resolved from bundled sources and pack overlays unless you override
it locally.

## What Gets Created

Typical project-local files after setup:

```text
.ao/
├── config.json
├── pm-config.json
├── workflows/
│   ├── custom.yaml
│   ├── standard-workflow.yaml
│   ├── hotfix-workflow.yaml
│   └── research-workflow.yaml
└── state/
    └── state-machines.v1.json
```

### `workflows/`

These are project-local YAML entry points. The default scaffold wraps bundled
pack workflows such as `ao.task/standard` rather than duplicating task logic in
the repository.

If you prefer a single-file layout, AO also understands `.ao/workflows.yaml`.

### `state/`

Project-local state currently includes workflow state-machine configuration and
other AO-managed metadata that can be resolved from the project. Manage it with
`ao` commands rather than editing it by hand.

## Machine-Scoped Runtime State

AO stores runtime state outside the repository under the repo scope for the
current checkout:

```text
~/.ao/<repo-scope>/
├── core-state.json
├── resume-config.json
├── state/
├── docs/
├── tasks/
├── requirements/
├── runs/
├── artifacts/
└── worktrees/
```

That split keeps project-authored workflow YAML in the repository while the
mutable execution history stays machine-scoped.

## Bundled vs Installed Packs

AO resolves workflows from multiple layers:

1. project YAML in `.ao/workflows.yaml` and `.ao/workflows/*.yaml`
2. installed packs in `~/.ao/packs/<pack-id>/<version>/`
3. bundled kernel workflows and bundled first-party packs

Bundled first-party packs currently own task, requirement, review, and QA
behavior. Canonical refs include:

- `ao.task/standard`
- `ao.task/quick-fix`
- `ao.task/triage`

Legacy `builtin/*` refs still resolve, but they are compatibility aliases.

## Mutation Policy

Do not hand-edit `.ao` state files. Use:

- `ao task ...`
- `ao requirements ...`
- `ao workflow ...`
- `ao pack ...`
- AO MCP tools

## Next Steps

- [Quick Start](quick-start.md)
- [A Typical Day](typical-day.md)
- [Workflows](../concepts/workflows.md)
