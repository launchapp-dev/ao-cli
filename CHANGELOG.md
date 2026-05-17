# Changelog

All notable changes to this project will be documented in this file.

## [0.4.1] - 2026-05-17

Same-day patch for issues caught during v0.4.0 release verification.

### Fixes
- **Plugin discovery config path renamed**: `default_config_path()` in `crates/orchestrator-plugin-host/src/discovery.rs:174` previously read `~/.config/ao/plugins.yaml` — a stale path the v0.4.0 hard rename missed. v0.4.1 reads from `~/.config/animus/plugins.yaml`. Users with a config at the old path need to `mv ~/.config/ao ~/.config/animus` on upgrade.
- **Docker image build broken upstream**: the v0.4.0 Release Binaries workflow's Docker step failed because OpenCode upstream changed its release-asset format from `opencode_linux_${ARCH}` (single binary) to `opencode-linux-${ARCH}.tar.gz` (tarball). Animus's `Dockerfile` now pulls the tarball and extracts the `opencode` binary out of it.
- **Docker stack finished rename**: `Dockerfile` and `docker-compose.yaml` both had leftover `ao` references the rename missed — `mkdir -p /root/.ao` → `mkdir -p /root/.animus`, compose service name `ao` → `animus`, image `ghcr.io/launchapp-dev/ao:latest` → `ghcr.io/launchapp-dev/animus:latest`, volume `ao-data:/root/.ao` → `animus-data:/root/.animus`, entrypoint `ao` → `animus`.

### Docs
- **Reference docs cover the v0.4.0 MCP surface**: `docs/reference/mcp-tools.md` now documents the `animus.skill.{list,get,search}` and `animus.memory.{get,list,append,clear}` families, including the `capabilities.memory: true` gating model. `docs/reference/cli/index.md` gets a "Selected Command Flags" section covering `animus init --update-registry`, the SHA256-required-with-URL plugin install rule, and the `--include-system-path` opt-in. `docs/reference/configuration.md` env var table expanded from 4 to ~30 entries. `docs/reference/data-layout.md` covers the new `~/.animus/packs/`, agent-host skill probe paths, and the resolution-paths table grew from 7 to 14 entries.
- **Narrative-docs `AO` / `ao` prose sweep**: 49 docs across `docs/{design,concepts,guides,architecture,contributing,reference,internals}/` plus the VitePress site config had bare project-name mentions of `AO` / `ao` that survived sub-agent A's code-focused rename. All now read `Animus`. Also folded into this pass: stale crate counts (`17-crate workspace` → `Rust-only Cargo workspace (around 20 crates)`), stale literal command examples (`ao status`, `ao now`, `ao git`, etc.), branch prefix `ao/<task-id>` → `animus/<task-id>` in `docs/concepts/worktrees.md` to match `crates/orchestrator-git-ops/src/daemon_git_worktree.rs`, and stale `PROJECT_ROOT` env var fallback in `docs/reference/cli/global-flags.md` per CLAUDE.md guidance.

### Known follow-ups (deferred to v0.4.x)
- `[[bin]] name = "ao-workflow-runner"` in `crates/workflow-runner-v2/Cargo.toml` is still the legacy bin name. Rename pending.
- Release archive filenames still use the `ao-v0.4.X-` prefix (e.g. `ao-v0.4.0-aarch64-apple-darwin.tar.gz`). The release script's archive-naming convention needs to be updated to `animus-v0.4.X-`.

## [0.4.0] - 2026-05-14

This is a major release and the v0.3.x → v0.4.0 break is intentionally large. Three commitments land together:

1. **One name across every surface.** The partial v0.3.x rebrand from AO to Animus is finished. There is no longer an `ao.*` namespace. Every MCP tool, environment variable, configuration directory, pack id, JSON envelope, plugin crate, and standalone plugin repository uses the `animus` name. No deprecation aliases. See the [naming contract](docs/architecture/naming-contract.md) and [migration guide](docs/migration/v0.3-to-v0.4.md).
2. **The architecture commits to plugin-first.** Subjects (units of dispatchable work) are now pluggable so teams can keep using Jira / Linear / GitHub Issues / Notion as their source of truth. Provider plugins (Claude, Codex, Gemini, OpenAI-compatible, OpenCode) move from in-tree workspace crates to standalone repositories. The plugin protocol crates move to a dedicated `launchapp-dev/animus-protocol` repository and ship as published Rust SDK crates that any plugin author depends on.
3. **The skill system rearchitects around the ecosystem-standard SKILL.md format.** Bundled YAML skills become an installable `animus.core-skills` pack. Skills installed for other agent hosts (Claude Code, Codex, Cursor, OpenCode) become discoverable subject sources for Animus workflows.

Expect breaking changes throughout. The migration is small in absolute terms because the v0.3.x install base is small; the rename is a deliberate inflection point that commits to Animus as an ecosystem rather than a single tool.

### Breaking changes

- **Full rename to `animus` (no aliases).** Every `ao.*` MCP tool name becomes `animus.*` (58 tools). Every `AO_*` environment variable becomes `ANIMUS_*` (51 vars). The project-local config directory `.ao/` becomes `.animus/`. Scoped runtime state at `~/.ao/<repo-scope>/` moves to `~/.animus/<repo-scope>/`. Pack ids `ao.task` / `ao.review` / `ao.requirement` become `animus.task` / `animus.review` / `animus.requirement`. The JSON output envelope schema `ao.cli.v1` becomes `animus.cli.v1`. The plugin protocol host name `"ao"` becomes `"animus"`. The in-tree provider crates rename from `ao-provider-*` to `animus-provider-*`; `ao-plugin-smoke` becomes `animus-plugin-smoke`. The migration guide has the complete list with before/after examples.
- **Bundled YAML skills moved to an installable pack.** `BUILTIN_SKILL_YAMLS` is removed. The 19 skills that previously shipped baked into the binary are now distributed as the `animus.core-skills` pack, auto-installed during `animus init` / `animus setup`. Existing references resolve via the `Installed` source.
- **`animus plugin install --url <url>` now requires `--sha256`.** Previously optional; URL-sourced installs without an integrity check are rejected at the CLI layer.
- **Plugin discovery no longer scans `$PATH` by default.** Discovery is limited to `.animus/plugins/` and `$ANIMUS_PLUGIN_PATH`. Restoring the prior behavior requires explicit `--include-system-path` on the discovery surface.
- **SKILL.md frontmatter: Animus-specific fields move under a vendor namespace.** `tool_policy`, `extra_args`, `env`, `mcp_servers`, `adapters`, `codex_config_overrides` move from top-level frontmatter to `animus:` in the SKILL.md frontmatter. Keeps SKILL.md portable across the ecosystem (Claude Code, Codex, Cursor, gstack) by isolating Animus-specific fields.
- **Subjects are now pluggable.** Native `animus task` is one backend among many. Workflows can declare `subject_type:` to operate over Linear/Jira/GitHub Issues subjects via plugin backends. Existing workflows that omit `subject_type:` continue using the native task backend with no change.
- **Plugin protocol crates move to standalone repository.** `orchestrator-plugin-protocol` is replaced by `animus-plugin-protocol` published from `launchapp-dev/animus-protocol`. The new `animus-subject-protocol`, `animus-provider-protocol`, and `animus-plugin-runtime` crates also publish from that repository. Plugin authors depend on these crates from crates.io rather than path-depending on the core workspace.
- **Provider plugins move to standalone repositories.** `animus-provider-{claude, codex, gemini, oai, opencode}` each ship from their own repository under `launchapp-dev/`. The daemon image installs them at build time from crates.io.

### Architectural commitments (documented for the first time)

- **Naming**: one name everywhere. See [naming contract](docs/architecture/naming-contract.md).
- **Plugin-first architecture**: subjects, providers, triggers, and skills are all plugins. The core is the orchestration runtime; plugins are independent release artifacts. See [subject backend plugins](docs/architecture/subject-backend-plugins.md).
- **Standalone plugin repositories**: every plugin lives in its own GitHub repository under `launchapp-dev/animus-{kind}-{name}`. The repository name, crate name, and binary name all match. Plugin authors generate new plugins from the `animus-plugin-template` scaffold via `animus plugin new --kind <kind> --name <name>`.

### Features

#### Plugin host
- **Stdio plugin host foundation**: New `orchestrator-plugin-host` (in core) plus `animus-plugin-protocol` (extracted to `launchapp-dev/animus-protocol`) implement newline-delimited JSON-RPC 2.0 plugin protocol with handshake, request/response, notifications, and streaming over stdio.
- **Plugin host wired through the runtime**: CLI, MCP server, subject dispatch, and daemon all route through the new plugin host. New `animus plugin install/list/get` commands and `animus.plugin.*` MCP tools.
- **Claude and Codex shipped as stdio plugins**: First-class `animus-provider-claude` and `animus-provider-codex` provider plugins running over the new protocol, distributed from their own repositories.
- **Three additional providers plus mock + runtime**: `animus-provider-gemini`, `animus-provider-opencode`, `animus-provider-oai`, plus the in-tree `animus-provider-mock` (testing) and the shared `animus-plugin-runtime` crate (used by every provider for the RPC loop, streaming, and logging).
- **Plugin smoke test**: `animus-plugin-smoke` crate exercises the end-to-end plugin contract for CI.
- **Subject backend plugins**: New `animus-subject-protocol` defines the `SubjectBackend` trait + normalized Subject schema. First reference implementation: `animus-subject-linear` ships from its own repository. The `animus plugin new` scaffold + `animus-plugin-template` make new subject backends a single-command bootstrap.
- **Template-driven `animus init`**: External template registry (default `launchapp-dev/animus-project-templates`) cloned and cached under `~/.animus/template-registries/<registry-id>/`. Three bundled templates: `task-queue`, `conductor`, `direct-workflow`. Flags: `--template <id>`, `--path <local-dir>`, `--non-interactive`, `--force`, `--update-registry`.
- **Template registry commit pinning**: `animus init` captures the registry HEAD on first clone into `.commit` metadata and refuses to silently fast-forward on subsequent runs. Use `--update-registry` to fetch latest and re-pin.
- **Bootstrap packs externalized**: Templates carry `[[packs]]` declarations that activate during init (e.g. `animus.task`, `animus.requirement`).
- **Markdown skill installs**: `animus skill install` supports SKILL.md format (frontmatter + body) alongside YAML. Body content lands in `prompt.system`. Animus-specific runtime fields live under the `animus:` frontmatter namespace.
- **Agent memory and communication config**: New per-agent memory state plus inter-agent messaging plumbed into phase prompt rendering. Memory MCP tools (`animus.memory.{get, list, append, clear}`) capability-gated on `agent.capabilities.memory == true`.
- **Plugin streaming + structured logging**: Provider RPC loop streams chunks back over notifications; structured logs flow to the host stderr sink.
- **`animus plugin install --url <https-url>`**: Download a plugin binary from an HTTPS URL with mandatory SHA256 integrity verification.

#### Subject backend protocol (v0.4.0 plugin extraction)
- **`animus-subject-protocol` crate**: New workspace crate (also published from `launchapp-dev/animus-protocol`) defining the `SubjectBackend` trait, normalized `Subject` schema (`SubjectId`, `SubjectStatus`, `SubjectFilter`, `SubjectList`, `SubjectPatch`, `SubjectSchema`, `ChangeKind`, `SubjectChangedEvent`), and JSON-RPC method constants (`subject/list`, `subject/get`, `subject/update`, `subject/watch`, `subject/schema`).
- **`animus-provider-protocol` crate**: New workspace crate defining the `ProviderBackend` trait, `AgentRunRequest`/`AgentRunResponse`/`AgentResumeRequest`/`AgentCancelRequest`/`ProviderManifest`/`ProviderCapabilities`/`TokenUsage` types, and `BackendError` with structured RPC error mapping.
- **`animus-plugin-runtime`**: Generalized from `ao-provider-runtime`. Provides `subject_backend_main()` and `provider_main()` entry points, full stdin/stdout JSON-RPC loop, lifecycle method handling (initialize/initialized/$/ping/shutdown/exit/health/check), and the `animus:` shape future plugin authors target.
- **Subject backend design doc**: `docs/architecture/subject-backend-plugins.md` defines the v0.4.0 plugin contract — `SubjectBackend` trait, normalized schema, JSON-RPC method set, workflow YAML binding, native `animus task` migration plan, and 8 open questions for red-team.

#### Skill system rearchitect
- **`animus.core-skills` pack**: The 19 bundled YAML skills now ship as an installable pack at `crates/orchestrator-config/config/bundled-packs/animus.core-skills/`. Auto-installs during `animus init` and `animus setup` and pins itself in `.animus/state/pack-selection.v1.json`. Users can `cd ~/.animus/packs/animus.core-skills/0.1.0/skills/` to read, fork, or override any of the 27 catalog skill names. Pack updates ship via `animus skill install` — no binary upgrade required.
- **`PackManifest::workflows` is now optional**: Skills-only packs are first-class. Validation requires at least one of `[workflows]` or `[skills]`. `PackSkills` is a sibling to `PackWorkflows`.
- **`SkillSourceOrigin::AgentHost { host, scope }`**: New variant scans skills installed for other agent hosts. Six hosts wired via `AGENT_HOST_SPECS`: Claude Code (`~/.claude/skills/`, `<project>/.claude/skills/`), Codex (`~/.codex/skills/`), OpenCode (`~/.config/opencode/skills/`), Cursor (`~/.cursor/skills/`), Kiro (`~/.kiro/skills/`), Slate (`~/.slate/skills/`). Project-scoped variants take precedence over global. Each scope is `Project | Global`.
- **Two-tier trust model**: `strip_structural_fields_for_agent_host` removes `tool_policy`, `extra_args`, `env`, `mcp_servers`, `adapters`, `codex_config_overrides`, and `capabilities` from any AgentHost-discovered skill at parse time. AgentHost skills can only contribute prompt text + directives. To use a host-discovered skill's full structural fields, the user must explicitly run `animus skill install --path` to promote it to the high-trust `Installed` source.
- **`animus:` vendor namespace in SKILL.md frontmatter**: Animus-specific runtime fields (`tool_policy`, `mcp_servers`, `model.preferred`, `adapters`, etc.) now live under `animus:` in the SKILL.md frontmatter, keeping SKILL.md portable across other agent hosts (Claude Code, Codex, Cursor, gstack) that don't recognize Animus-specific keys. Top-level placement of these fields is no longer parsed (hard cut, no deprecation warning). The trust strip still applies even when `animus:` namespace is present in an AgentHost source.
- **Source chain priority**: `AgentHost::Global < AgentHost::Project < Builtin < Installed (pack + registry) < User < Project`. `load_skill_sources` returns sources in lowest-to-highest order; resolution iterates in reverse for highest-priority match.
- **`docs/architecture/skill-system.md`**: New design doc covering the source chain, trust tiers, vendor namespace, and `animus.core-skills` pack layout.

#### `animus.skill.*` MCP tools
- **`animus.skill.list`**: Enumerate all skills across every source (Builtin, Installed, AgentHost::Project, AgentHost::Global, User, Project). Each entry carries `source` (variant tag) + `source_detail` with provenance: registry/source/version for Installed; host/scope plus `structural_fields_stripped: true` and `trust_tier: "prompt_text_only"` for AgentHost. Optional `source` filter (`builtin`, `installed`, `agent_host`, or a host id like `claude-code`).
- **`animus.skill.get`**: Fetch the full `SkillDefinition` for a skill by name. AgentHost-source responses include a `notice` field explaining that structural fields were stripped at parse time.
- **`animus.skill.search`**: Case-insensitive substring match over name, description, and tags. Optional source filter and result limit.

External agents (Claude Code session driving Animus, workflow-spawned subprocesses connected over MCP) can now discover the skill catalog at runtime — including the user's existing `~/.claude/skills/` library that v0.3.x couldn't reach — without shelling out to `animus skill list --json` or hardcoding skill names.

### Fixes
- **Honor configured rework routing** (#024f0161): `rework` verdicts now respect the configured routing target instead of defaulting to the immediately preceding phase.
- **Plugin handshake hangs blocked**: `PluginHost::handshake` now wraps the initialize round trip in a 30-second timeout so a non-responsive plugin can't deadlock daemon startup.
- **`expect()` panics removed from plugin code paths**: `animus.plugin.call` MCP tool and the provider runtime's `initialize` / `health/check` paths now return structured RPC errors instead of panicking when serialization or registry initialization fails.
- **`git clone` / `git pull` errors surfaced**: Template registry sync now captures stdout/stderr from git subprocess invocations and includes them in error messages (was previously swallowed).

### Docs
- **Naming contract**: `docs/architecture/naming-contract.md` commits to one name across every surface (Animus). Cross-linked from architecture index, VitePress nav, `CONTRIBUTING.md`, and `CLAUDE.md`.
- **Subject backend plugin protocol**: `docs/architecture/subject-backend-plugins.md` defines the v0.4.0 subject backend plugin design.
- **Skill system v0.4.0**: `docs/architecture/skill-system.md` — source chain, trust tiers, `animus:` namespace, `animus.core-skills` pack layout.
- **Migration guide**: `docs/migration/v0.3-to-v0.4.md` walks through every breaking change with before/after examples covering all 14 surfaces (rename, plugin discovery, template registry pinning, skill system, subject backend pluggability).
- **`feature-status.md`**: Template-driven project init flipped from In-Flight to Shipped.
- **`docs/architecture/index.md`**: Updated to reflect the all-`animus` naming, the ~20-crate workspace size, and the v0.4.0 subject backend plugin link.
- **`CLAUDE.md`** (project guide): Updated state paths from `.ao/` → `.animus/`, env var references from `AO_*` → `ANIMUS_*`, and the working-rules naming guidance to the all-`animus` model.

### Internal
- **Cargo aliases renamed**: `cargo ao-fmt`/`ao-lint`/`ao-bin-build`/`ao-bin-check` → `cargo animus-fmt`/`animus-lint`/`animus-bin-build`/`animus-bin-check`. CI release workflow updated.

### Known issues / fix-before-tag
- Plugin discovery executes `--manifest` on every candidate found in `.animus/plugins/` and `$ANIMUS_PLUGIN_PATH` with no signature, allowlist, or sandboxing. `$PATH` is opt-in (off by default). Treat the install / discovery surface as trusted-only until a richer trust model lands.
- Subject backend reference plugin (`animus-subject-linear`) and `animus plugin new` scaffold command are still on the v0.4.0 roadmap; the protocol crate stack is in place to support them. Native `animus task` migration to satisfy the `SubjectBackend` trait also remains pending.
- The optional `animus.skill.registry.list` MCP tool was deferred — `SkillRegistrySourceConfig` is `pub(super)` to `ops_skill` and exposing it cleanly needs a thin `pub fn list_skill_registries()` accessor first. Follow-up.

## [0.3.2] - 2026-04-10

### Fixes
- **Default decision contract evidence types** (TASK-222): Implementation phase decision contract now accepts the common evidence kinds agents actually return (`bug_confirmed`, `fix_identified`, etc.), not only `files_modified`. First workflow runs no longer fail immediately when an agent does an analysis-only pass.
- **`phase_decision.evidence` is now optional** when no required evidence types are configured.
- **Strip secrets from tracked state**: Removed `.ao/sync.json` from tracking; it can contain auth tokens.
- **Rustfmt + lint cleanups**: `runtime_contract.rs` method chain and test formatting.

### Features
- **`animus task status --force`**: Allow forced status transitions for skip/cancel paths.
- **Surface agent file-edit decisions**: When an agent runs an analysis phase and modifies no files, the phase output now explains why (permissions, working directory, prompt constraints, or "no change needed") instead of leaving the user guessing.

### Docs
- Restructured getting-started to lead with autonomous mode.
- README updated with v0.3.0 feature surface.

## [0.3.1] - 2026-04-07

### Features
- **Animus daemon Docker image**: Multi-stage `Dockerfile` produces a `linux/amd64` image with minimal dependencies, sized for Fly.io cloud deployment.
- **Claude Code, Codex, and OpenCode bundled in the daemon image**: The cloud daemon ships with all three coding CLIs pre-installed so spawned workflow agents work without per-tenant install steps.
- **Docker image publish in release workflow**: CI now pushes `registry.fly.io/ao-daemon` on tagged releases.

### Fixes
- **Auto-start daemon and agent-runner on sync workflow execution** (TASK-221): Fixes "first-run socket error" when invoking a sync workflow without a daemon already running.
- **Dockerfile Rust bump to 1.89**: `async-graphql` requires rustc 1.89+.

## [0.3.0] - 2026-04-06

### Features
- Cloud status integration: ao cloud status calls /api/cli/status for cloud projects and daemons — show user's cloud projects, daemon states, and active workflows from the cloud API
- Cloud API routing: ao-cli route deploy commands through cloud API instead of direct Fly.io — animus cloud deploy create/destroy/start/stop/status should call /api/cli/daemons/* endpoints instead of requiring Fly.io credentials locally
- Auto-detect project linking from git remote + GitHub App installation
- Config alignment: animus cloud push now sends .ao/ config files
- Device auth flow: implement animus cloud login device auth flow
- ACP evaluation: research ACP spec and design AO as ACP server for IDE integration
- Deploy subcommands: add start/stop/status/create/destroy deployment subcommands
- Feature branch workflow: enable feature branch workflow in standard task pipelines
- Cloud CLI: evolve ao sync into ao cloud CLI subcommands
- Memory MCP: wire ao-memory-mcp into default workflow phases
- Marketplace MVP: Skill marketplace MVP — GitHub registry fetch + pack download
- Event triggers Phase 2: Generic Webhook Support
- Event triggers Phase 1: file watcher support
- Remote MCP servers: HTTP/remote MCP server support

### Fixes
- Cloud linking: rustfmt fix for cloud.rs auto-detect linking
- Config test isolation: improve orchestrator-config test isolation and lock handling
- Model references: replace broken glm-5 model refs with claude-haiku, disable pr-sweep schedule
- Sparkcube removal: remove sparkcube tool_profile refs blocking all daemon workflows
- CLI E2E tests: resolve all 6 storage-mismatch and warning failures
- Lint warnings: resolve all clippy warnings workspace-wide
- Daemon task state: fix last failing unit test — daemon_run task-state-change assertion
- Config test assertion: align test assertion with standard-workflow ID rename
- Env lock handling: recover from poisoned env lock in tests
- Path references: replace hardcoded fixture paths and absolute paths with CARGO_MANIFEST_DIR

### Refactors
- Rebrand to animus: rename primary binary from 'ao' to 'animus', maintain 'ao' as alias
- Help text updates: update all help text and error messages from 'ao' to 'animus'
- Test updates: update test assertions for rebrand to 'animus'
- Unused dependencies: strip unused MCP servers — remove filesystem, sequential-thinking, memory, rust-docs

### Documentation
- Public repo preparation: clean README.md with Animus branding, verify ELv2 LICENSE, remove hardcoded local paths, add CONTRIBUTING.md, ensure .gitignore covers sensitive files
- README updates: fix typo and update GitHub release links

### Style
- Formatting fixes: rustfmt fixes for line length, import ordering, and cloud status command

## [0.2.35] - Previous Release

See git history for earlier releases.
