# Configuration Reference

AO resolves behavior from project YAML, installed pack layers, scoped runtime state, and environment overrides.

## Project-Local Sources

### `.animus/config.json`

Repository-local AO configuration created during setup.

### `.animus/workflows.yaml` and `.animus/workflows/*.yaml`

These YAML files are the editable workflow source of truth for a project.

Typical uses:

- define repo-specific workflow ids such as `standard-workflow`
- define the repository's default workflow explicitly
- declare project MCP servers, agents, variables, phases, and workflow definitions

### `.animus/plugins/<pack-id>/`

Project-local pack overrides. Use this when a repository needs to override installed pack content without changing AO globally.

## Repo-Scoped Runtime Config

AO stores mutable project runtime config under `~/.animus/<repo-scope>/`.

Key files:

- `config/state-machines.v1.json`
- `state/pack-selection.v1.json`
- `daemon/pm-config.json`
- `resume-config.json`

These files are AO-managed state. Treat them as runtime data, not hand-authored config.

## Global User Config

### `~/.animus/config.json`

The global AO config stores machine-local user settings such as:

- agent runner auth token
- user-defined MCP server entries
- Claude profile launch environments

Use `ANIMUS_CONFIG_DIR` to override the global config root in tests or custom environments.

Example:

```json
{
  "claude_profiles": {
    "main": {
      "env": {
        "CLAUDE_CONFIG_DIR": "/Users/alice/.claude-main"
      }
    }
  }
}
```

## Installed Sources

### Machine-installed packs

Installed packs live at:

```text
~/.animus/packs/<pack-id>/<version>/
```

Manage them with:

```bash
animus pack list
animus pack inspect --pack-id animus.task
animus pack install --path /tmp/vendor.pack --activate
animus pack pin --pack-id vendor.pack --version =1.2.3
```

## Configuration Precedence

Behavior resolves in this order:

1. CLI flags
2. supported environment variables
3. project pack overrides in `.animus/plugins/`
4. project YAML in `.animus/workflows.yaml` and `.animus/workflows/*.yaml`
5. installed packs in `~/.animus/packs/`

## Environment Variables

| Variable | Description |
|---|---|
| `ANIMUS_CONFIG_DIR` | Override the global AO config directory |
| `ANIMUS_RUNNER_CONFIG_DIR` | Override the runner config directory |
| `ANIMUS_MCP_SCHEMA_DRAFT` | Select Draft-07 MCP tool input schemas |
| `CLAUDECODE` | Signals an embedded Claude Code environment |

## Notes

- Project YAML is the authored workflow surface.
- AO no longer ships bundled workflows; a project must author workflows locally or install a pack.
- Mutable runtime state lives under `~/.animus/<repo-scope>/`.
- The daemon schedules and supervises work; workflow and pack content still define behavior.
