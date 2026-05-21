# Installation

Current release: **v0.4.4** (2026-05-21). See [CHANGELOG.md](../../CHANGELOG.md)
for the full v0.4.x shipped state.

## Fast Path: Upstream Installer

Use the installer published from `launchapp-dev/animus-cli`:

```bash
curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash
```

Options:

```bash
# Install a specific release
ANIMUS_VERSION=v0.4.4 curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash

# Install into a custom directory
ANIMUS_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/launchapp-dev/animus-cli/main/scripts/install.sh | bash
```

The upstream installer currently targets macOS. On Linux and Windows, use a release archive or build from source.

## Release Archives

Prebuilt releases are published at:

- <https://github.com/launchapp-dev/animus-cli/releases>

Download the archive for your platform, extract it, and place these binaries on your `PATH`:

- `animus`
- `agent-runner`
- `llm-cli-wrapper`
- `animus-oai-runner`
- `ao-workflow-runner`

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
```

Run `animus doctor` inside a git repository to verify the local environment and Animus prerequisites.

## Install Plugins

Animus v0.4.x ships a slim core; providers, subject backends, triggers, and log
sinks all install as out-of-tree plugins from public GitHub repos:

```bash
# Provider (one is required for autonomous runs)
animus plugin install launchapp-dev/animus-provider-claude
animus plugin install launchapp-dev/animus-provider-codex
animus plugin install launchapp-dev/animus-provider-gemini
animus plugin install launchapp-dev/animus-provider-opencode
animus plugin install launchapp-dev/animus-provider-oai

# Subject backends (optional — in-tree task and requirement adapters work out of the box)
animus plugin install launchapp-dev/animus-subject-linear
animus plugin install launchapp-dev/animus-subject-sqlite
animus plugin install launchapp-dev/animus-subject-markdown
animus plugin install launchapp-dev/animus-subject-requirements

# Triggers
animus plugin install launchapp-dev/animus-trigger-webhook
animus plugin install launchapp-dev/animus-trigger-slack

# Log storage
animus plugin install launchapp-dev/animus-log-storage-file

# List installed plugins (with cosign signature column)
animus plugin list
```

Plugin installs verify a sigstore cosign signature when one is published. Use
`--require-signature` to refuse unsigned installs or `--skip-signature` to
bypass verification entirely. See [docs/architecture/plugin-signing.md](../architecture/plugin-signing.md).

To scaffold a new plugin of your own:

```bash
animus plugin new my-plugin --kind subject   # or --kind provider
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

State paths:

- Project-local config: `.animus/config.json`
- Project-local daemon settings: `.animus/pm-config.json`
- Workflow YAML overlays: `.animus/workflows.yaml`, `.animus/workflows/*.yaml`
- Scoped runtime root: `~/.animus/<repo-scope>/`
- Control socket: `~/.animus/<repo-scope>/control.sock` (0700)

## Next Steps

Proceed to the [Quick Start](quick-start.md) to initialize a repository and run the first workflow.
