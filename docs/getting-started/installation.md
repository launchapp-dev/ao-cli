# Installation

Current release: **v0.4.15** (2026-05-28). See [CHANGELOG.md](../../CHANGELOG.md)
for the full v0.4.x shipped state and
[`docs/migration/v0.4.11-to-v0.4.12.md`](../migration/v0.4.11-to-v0.4.12.md)
if you are upgrading from an earlier v0.4.x.

> **v0.4.12 changed the first-run flow.** The daemon no longer ships with
> bundled providers or subject backends. After installing the `animus`
> binary you must run `animus plugin install-defaults --include-subjects --include-transports`
> once before `animus daemon start` will boot. The command is idempotent and
> the plugins live in `~/.animus/plugins/` (shared across projects).

## Fast Path: Upstream Installer

Use the installer published from `launchapp-dev/animus-cli`:

```bash
curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash
animus plugin install-defaults --include-subjects --include-transports
```

Options:

```bash
# Install a specific release
ANIMUS_VERSION=v0.4.15 curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash

# Install into a custom directory
ANIMUS_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash

# Run install-defaults automatically at the end of the installer
ANIMUS_INSTALL_PLUGINS=1 curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash

# Skip the post-install plugin step (CI / Docker builds that install plugins separately)
ANIMUS_SKIP_PLUGIN_INSTALL=1 curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash
```

The upstream installer currently targets macOS. On Linux and Windows, use a release archive or build from source.

## Release Archives

Prebuilt releases are published at:

- <https://github.com/launchapp-dev/animus-cli/releases>

Download the archive for your platform, extract it, and place these binaries on your `PATH`:

- `animus`
- `agent-runner`
- `animus-oai-runner`
- `animus-workflow-runner` (v0.4.16+ name; the v0.4.x `ao-workflow-runner` is created as a back-compat symlink by `scripts/install.sh`)

(The v0.4.11 `llm-cli-wrapper` binary was removed in v0.4.12 — the crate
was deleted and its functionality folded into `agent-runner` and the
upstream `animus-session-backend` crate.)

Supported release targets:

| Target | Platform |
|--------|----------|
| `aarch64-apple-darwin` | macOS (Apple Silicon) |
| `x86_64-apple-darwin` | macOS (Intel) |
| `aarch64-unknown-linux-gnu` | Linux (arm64) |
| `x86_64-unknown-linux-gnu` | Linux (x86_64) |
| `x86_64-pc-windows-msvc` | Windows (x86_64) |

## Build From Source

```bash
git clone https://github.com/launchapp-dev/animus-cli.git
cd animus-cli

# Verify the runtime binaries
cargo animus-bin-check

# Debug build
cargo animus-bin-build

# Release build
cargo animus-bin-build-release
```

To run the CLI directly during development:

```bash
cargo run -p orchestrator-cli -- --help
```

## Verify Installation

```bash
animus --version
animus doctor
animus daemon preflight     # v0.4.12: checks that required plugins are installed
```

Run `animus doctor` inside a git repository to verify the local environment
and Animus prerequisites. Run `animus daemon preflight` from anywhere — it
reports the installed-vs-required plugin matrix and exits non-zero with the
exact `animus plugin install ...` fix command for each gap.

## Install Plugins

Animus v0.4.12 ships a slim core; providers, subject backends, triggers, and
log sinks all install as out-of-tree plugins from public GitHub repos.

**Recommended (one command):**

```bash
animus plugin install-defaults --include-subjects --include-transports
```

This installs:

- 5 providers (`animus-provider-claude` v0.2.2, `animus-provider-{codex,gemini,opencode}` v0.2.3, `animus-provider-oai` v0.2.2) — daemon requires at least one
- 5 subject backends (`animus-subject-default` v0.1.1, `animus-subject-requirements` v0.1.6, `animus-subject-{linear,sqlite,markdown}` v0.1.4) — daemon requires `default` for `kind=task` and `requirements` for `kind=requirement`
- 3 transport + UI plugins (`animus-transport-http` v0.2.1, `animus-transport-graphql` v0.2.3, `animus-web-ui` v0.1.1) — required for `animus web serve`

Add `--include-oai-agent` to also install `animus-provider-oai-agent@v0.1.3`
(OpenAI Responses API agent loop, separate from the chat completions provider).

**Or install individually:**

```bash
# Providers (at least one required)
animus plugin install launchapp-dev/animus-provider-claude
animus plugin install launchapp-dev/animus-provider-codex
animus plugin install launchapp-dev/animus-provider-gemini
animus plugin install launchapp-dev/animus-provider-opencode
animus plugin install launchapp-dev/animus-provider-oai

# Subject backends (required: default for kind=task, requirements for kind=requirement)
animus plugin install launchapp-dev/animus-subject-default
animus plugin install launchapp-dev/animus-subject-requirements
animus plugin install launchapp-dev/animus-subject-linear
animus plugin install launchapp-dev/animus-subject-sqlite
animus plugin install launchapp-dev/animus-subject-markdown

# Transport + web UI (required for `animus web serve`)
animus plugin install launchapp-dev/animus-transport-http
animus plugin install launchapp-dev/animus-transport-graphql
animus plugin install launchapp-dev/animus-web-ui

# Triggers (optional)
animus plugin install launchapp-dev/animus-trigger-webhook
animus plugin install launchapp-dev/animus-trigger-slack

# Log storage (optional)
animus plugin install launchapp-dev/animus-log-storage-file

# List installed plugins (with cosign signature column)
animus plugin list
```

Plugin installs verify a sigstore cosign signature when one is published. Use
`--signature-policy strict` to fail closed, `--allow-unsigned` for the current
warn-and-proceed posture, or `--signature-policy disabled` for air-gapped and
local-build workflows. See [Security](../reference/security.md).

To scaffold a new plugin of your own:

```bash
animus plugin new --kind subject --name my-plugin   # or --kind provider
```

## Prerequisites

Animus itself is a Rust application, but autonomous workflows need at least one supported AI coding CLI on your `PATH` (and the matching provider plugin installed):

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- [OpenAI Codex CLI](https://github.com/openai/codex)
- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [OpenCode](https://github.com/opencode-ai/opencode)

Example installs:

```bash
npm install -g @anthropic-ai/claude-code
npm install -g @openai/codex
npm install -g @google/gemini-cli
```

## Environment Variables

Common runtime overrides:

| Variable | Purpose | Default |
|----------|---------|---------|
| `ANIMUS_CONFIG_DIR` | Override the global config root | `~/.animus/` (via `protocol::Config::global_config_dir()`) |
| `ANIMUS_PLUGIN_DIR` | Override plugin install location | `~/.animus/plugins/` |
| `ANIMUS_VERSION` | Pin version for the installer script | latest release |
| `ANIMUS_INSTALL_DIR` | Pin install dir for the installer script | `~/.local/bin` |
| `ANIMUS_DAEMON_DISABLE_CONTROL_SERVER` | Opt out of the daemon Unix-socket control server | unset (server enabled) |
| `ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN` | Force the in-tree `events.jsonl` logger even if a log-storage plugin is installed | unset |
| `ANIMUS_DAEMON_DISABLE_SUBJECT_PLUGINS` | Skip subject plugin discovery entirely. Daemon will refuse most subject operations | unset |
| `ANIMUS_INSTALL_PLUGINS` | When set to `1`, the installer script runs `animus plugin install-defaults --include-subjects --include-transports` at the end | unset |
| `ANIMUS_SKIP_PLUGIN_INSTALL` | When set to `1`, the installer script skips the post-install plugin step entirely | unset |

The v0.4.11 env vars `ANIMUS_DAEMON_DISABLE_BUILTIN_TASK_ADAPTER` and
`ANIMUS_DAEMON_DISABLE_BUILTIN_REQUIREMENTS_ADAPTER` are **no-ops** in v0.4.12
— the in-tree adapters are gone. Install or uninstall the corresponding
subject_backend plugin instead.

State paths:

- Project-local config: `.animus/config.json`
- Workflow YAML overlays: `.animus/workflows.yaml`, `.animus/workflows/*.yaml`
- Scoped runtime root: `~/.animus/<repo-scope>/`
- Scoped daemon settings: `~/.animus/<repo-scope>/daemon/pm-config.json`
- Control socket: `~/.animus/<repo-scope>/control.sock` (0700)

## Next Steps

Proceed to the [Quick Start](quick-start.md) to initialize a repository and run the first workflow.
