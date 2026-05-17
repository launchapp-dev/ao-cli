# Configuration Reference

Animus resolves behavior from project YAML, installed pack layers, scoped runtime state, and environment overrides.

## Project-Local Sources

### `.animus/config.json`

Repository-local Animus configuration created during setup.

### `.animus/workflows.yaml` and `.animus/workflows/*.yaml`

These YAML files are the editable workflow source of truth for a project.

Typical uses:

- define repo-specific workflow ids such as `standard-workflow`
- define the repository's default workflow explicitly
- declare project MCP servers, agents, variables, phases, and workflow definitions

### `.animus/plugins/<pack-id>/`

Project-local pack overrides. Use this when a repository needs to override installed pack content without changing Animus globally.

## Repo-Scoped Runtime Config

Animus stores mutable project runtime config under `~/.animus/<repo-scope>/`.

Key files:

- `config/state-machines.v1.json`
- `state/pack-selection.v1.json`
- `daemon/pm-config.json`
- `resume-config.json`

These files are Animus-managed state. Treat them as runtime data, not hand-authored config.

## Global User Config

### `~/.animus/config.json`

The global Animus config stores machine-local user settings such as:

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

The complete v0.4.0 env var surface was renamed from `AO_*` to `ANIMUS_*`. There are no
legacy aliases; the old names will not be read.

### Core

| Variable | Description |
|---|---|
| `ANIMUS_CONFIG_DIR` | Override the global Animus config directory (default `~/.animus`) |
| `ANIMUS_RUNNER_CONFIG_DIR` | Override the runner config directory |
| `ANIMUS_RUNNER_SCOPE` | Runner scope identifier. Defaults to the repo-scope derived from the project root |
| `ANIMUS_MCP_SCHEMA_DRAFT` | Select Draft-07 MCP tool input schemas |
| `ANIMUS_MCP_ENDPOINT` | Override the MCP server endpoint for the CLI's embedded client |
| `ANIMUS_USER_ID` | Override the recorded user id for authored actions |
| `ANIMUS_ASSIGNEE_USER_ID` | Override the assignee user id when creating tasks |
| `ANIMUS_DEBUG` | Enable verbose debug logging across the CLI and daemon |
| `ANIMUS_LOG_JSON` | Emit log lines as JSON for log shippers |
| `ANIMUS_DEBUG_MCP_STDIO` | Log raw MCP stdio frames for plugin/server debugging |

### Plugins and templates

| Variable | Description |
|---|---|
| `ANIMUS_PLUGIN_DIR` | Override the plugin install directory used by `animus plugin install` and scanned at discovery time. Defaults to `~/.animus/plugins/`. Takes precedence over the default but is overridden by `--plugin-dir <PATH>` on the install/uninstall commands |
| `ANIMUS_PLUGIN_PATH` | Colon-separated list of additional directories to scan for `animus-provider-*` and `animus-plugin-*` binaries during plugin discovery (in addition to the install dir) |
| `ANIMUS_TEMPLATE_REGISTRY_URL` | Override the default template registry URL used by `animus init`. Defaults to the LaunchApp project-templates registry |

### Runner and workflow

| Variable | Description |
|---|---|
| `ANIMUS_SKIP_RUNNER_START` | Skip auto-starting the runner alongside the daemon |
| `ANIMUS_AUTO_REBUILD_RUNNER_ON_MAIN_UPDATE` | Rebuild the runner binary when main is updated |
| `ANIMUS_RUNNER_BUILD_ID` | Pin a specific runner build id |
| `ANIMUS_WORKFLOW_RUNNER_BIN` | Override the workflow-runner binary path |
| `ANIMUS_PHASE_RUN_ATTEMPTS` | Maximum attempts for a single phase run before giving up |
| `ANIMUS_PHASE_MAX_CONTINUATIONS` | Cap on phase continuation rounds |

### Notifications

| Variable | Description |
|---|---|
| `ANIMUS_NOTIFY_WEBHOOK_URL` | Default webhook URL referenced by notification configs that use `webhook_url_env: "ANIMUS_NOTIFY_WEBHOOK_URL"` |
| `ANIMUS_NOTIFY_BEARER_TOKEN` | Default bearer token referenced by notification configs that use `Authorization: "ANIMUS_NOTIFY_BEARER_TOKEN"` |
| `ANIMUS_NOTIFY_MISSING_URL` | Behavior tuning for notification dispatch when the webhook URL is unset |

Any other env var name can be referenced by a notification config's `url_env` or per-header
env lookups, so projects can define their own `ANIMUS_NOTIFY_*` variables and reference
them from the persisted notification config.

### Cloud sync

| Variable | Description |
|---|---|
| `ANIMUS_CLOUD_URL` | Override the cloud-sync server URL for `animus cloud` commands |

### Provider plugin tuning

These passthrough variables are read by the bundled provider plugins (claude, codex,
gemini, opencode) when spawning their underlying CLIs.

| Variable | Description |
|---|---|
| `ANIMUS_CLAUDE_BYPASS_PERMISSIONS` | Pass `--dangerously-skip-permissions` (or equivalent) to the Claude provider plugin. Intended for unattended test environments only — never set in production |
| `ANIMUS_CLAUDE_EXTRA_ARGS` / `_JSON` | Extra args to forward to the Claude CLI |
| `ANIMUS_CODEX_EXTRA_ARGS` / `_JSON` | Extra args to forward to the Codex CLI |
| `ANIMUS_CODEX_EXTRA_CONFIG_OVERRIDES` / `_JSON` | Extra `--config` overrides for Codex |
| `ANIMUS_CODEX_NETWORK_ACCESS` | Toggle Codex sandbox network access |
| `ANIMUS_CODEX_WEB_SEARCH` | Toggle Codex web search capability |
| `ANIMUS_GEMINI_EXTRA_ARGS` / `_JSON` | Extra args to forward to the Gemini CLI |
| `ANIMUS_OPENCODE_EXTRA_ARGS` / `_JSON` | Extra args to forward to the OpenCode CLI |
| `ANIMUS_AI_CLI_EXTRA_ARGS` / `_JSON` | Generic CLI passthrough used by the wrapper crate |

### External / inherited

| Variable | Description |
|---|---|
| `CLAUDECODE` | Set by the Claude Code harness when an embedded session is active. Animus detects it and unsets it before spawning a nested `claude` CLI to avoid the "cannot launch inside another Claude Code session" guard |

## Notes

- Project YAML is the authored workflow surface.
- Animus no longer ships bundled workflows; a project must author workflows locally or install a pack.
- Mutable runtime state lives under `~/.animus/<repo-scope>/`.
- The daemon schedules and supervises work; workflow and pack content still define behavior.
