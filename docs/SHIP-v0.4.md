# Animus v0.4.0 — Plugin Ecosystem Ship

**Released:** 2026-05-18
**Animus CLI:** v0.4.2
**Protocol:** animus-protocol v0.1.0

Animus is a Rust-based autonomous agent orchestrator. v0.4.0 makes
the entire stack pluggable: anyone can ship a subject backend, an
LLM provider, or a trigger source as a stdio plugin under their own
GitHub repo and have the Animus daemon dispatch through it.

## Quickstart (60 seconds)

```bash
# Install the CLI
brew install launchapp-dev/animus/animus     # macOS
# or download: https://github.com/launchapp-dev/animus-cli/releases/latest

# Initialize a project
mkdir my-animus && cd my-animus && git init
animus init --template task-queue

# Install a provider plugin from a public repo
animus plugin install launchapp-dev/animus-provider-claude

# Start the daemon
animus daemon start

# Create + run a workflow phase
animus task create --title "test"
animus task next
```

## What's pluggable

Three plugin kinds:

| Kind | Wraps | Examples shipped |
|---|---|---|
| Subject backend | A system-of-record (Linear, Jira, GitHub Issues, Notion) | `animus-subject-linear` |
| LLM provider | A CLI tool (Claude, Codex, Gemini, OpenCode) or HTTP API (OpenAI-compatible) | `animus-provider-{claude,codex,gemini,opencode,oai}` |
| Trigger backend | Event sources (Slack, webhooks, cron, file watchers) | (v0.4.x — protocol designed, plugins TBD) |

## Plugins shipped today

| Repo | Kind | One-liner | Install |
|---|---|---|---|
| `launchapp-dev/animus-provider-claude` | provider | Claude Code CLI wrapper (Sonnet/Opus/Haiku) | `animus plugin install launchapp-dev/animus-provider-claude` |
| `launchapp-dev/animus-provider-codex` | provider | OpenAI Codex CLI wrapper | `animus plugin install launchapp-dev/animus-provider-codex` |
| `launchapp-dev/animus-provider-gemini` | provider | Google Gemini CLI wrapper | `animus plugin install launchapp-dev/animus-provider-gemini` |
| `launchapp-dev/animus-provider-opencode` | provider | OpenCode CLI wrapper | `animus plugin install launchapp-dev/animus-provider-opencode` |
| `launchapp-dev/animus-provider-oai` | provider | Generic OpenAI-compatible HTTP provider | `animus plugin install launchapp-dev/animus-provider-oai` |
| `launchapp-dev/animus-provider-mock` | provider | Deterministic mock provider for tests | `animus plugin install launchapp-dev/animus-provider-mock` |
| `launchapp-dev/animus-subject-linear` | subject | Linear issues as the system-of-record | `animus plugin install launchapp-dev/animus-subject-linear` |

## Building your own plugin

```bash
animus plugin new --kind subject --name jira
cd animus-subject-jira
# implement src/backend.rs against the SubjectBackend trait
cargo build --release
animus plugin install --path target/release/animus-subject-jira
```

The protocol crates ship from `launchapp-dev/animus-protocol`:

- `animus-plugin-protocol` (wire types)
- `animus-subject-protocol` (`SubjectBackend` trait)
- `animus-provider-protocol` (`ProviderBackend` trait)
- `animus-trigger-protocol` (`TriggerBackend` trait — v0.4.x)
- `animus-plugin-runtime` (the stdio JSON-RPC loop)
- `animus-session-backend` (CLI subprocess management)

Plugin authors write `subject_backend_main(info, backend).await`
from `main()` and the runtime handles the wire protocol.

## What changed under the hood

- 8 standalone GitHub repos under `launchapp-dev/`
- The animus daemon discovers installed plugins and dispatches phase
  execution through them (proven via integration tests)
- Fallback to bundled in-tree implementations when plugin missing
- MCP exposes `animus.plugin.{list,info,install,uninstall,ping,call}`
  so AI agents can manage their own plugins programmatically

## What's coming in v0.5

- Sigstore + cosign keyless signing for plugin releases
- A plugin marketplace (browse + search via `animus-plugin-registry`)
- Trigger backends: Slack, generic webhooks, GitHub Actions
- Cross-language plugins (Python + TypeScript SDKs against the protocol)

## Links

- Animus CLI: https://github.com/launchapp-dev/animus-cli
- Plugin protocol: https://github.com/launchapp-dev/animus-protocol
- Plugin template: https://github.com/launchapp-dev/animus-plugin-template
- Plugin registry: https://github.com/launchapp-dev/animus-plugin-registry (v0.4.x)

## For agents helping a user set up Animus

If a user asks you to set up Animus for them, follow this exact sequence:

1. Verify the user is in a git repo (`git rev-parse --show-toplevel`). If not, ask whether to `git init` here or elsewhere.
2. Install the animus CLI if missing. Check `which animus` and download the latest release for the user's platform if it's not on PATH.
3. Run `animus init --template task-queue` to scaffold the project.
4. Ask the user which LLM provider they want. Common choices:
   - Claude → `animus plugin install launchapp-dev/animus-provider-claude`
   - Codex → `animus plugin install launchapp-dev/animus-provider-codex`
   - OpenAI HTTP → `animus plugin install launchapp-dev/animus-provider-oai` (set `OPENAI_API_KEY`)
   - Gemini → `animus plugin install launchapp-dev/animus-provider-gemini`
5. Verify the install: `animus plugin list` — confirm the plugin appears.
6. Set up secrets the provider needs (env vars or `.env` file — DON'T commit secrets).
7. Show the user how to run their first workflow: `animus task create --title "my-first-task" && animus task next`.

Do not skip steps. Do not silently install multiple plugins — ask the user first.
