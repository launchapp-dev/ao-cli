# CLI Command Surface

Complete reference of every `animus` command, subcommand, and key flag. This tree is the authoritative map of the CLI surface area. For global flags that apply to all commands, see [Global Flags](global-flags.md). For exit code semantics, see [Exit Codes](exit-codes.md).

## Global Flags

| Flag | Description |
|---|---|
| `--json` | Machine-readable JSON output (`animus.cli.v1` envelope) |
| `--project-root <PATH>` | Override project root resolution for the current command |

---

## Top-Level Command Tree

```
animus
├── version                  Show installed animus version
├── daemon                   Manage daemon lifecycle and automation settings
│   ├── start                Start the daemon in detached/background mode
│   ├── run                  Run the daemon in the current foreground process
│   ├── stop                 Stop the running daemon
│   ├── status               Show daemon runtime status
│   ├── health               Show daemon health diagnostics
│   ├── pause                Pause daemon scheduling
│   ├── resume               Resume daemon scheduling
│   ├── events               Stream or tail daemon event history
│   ├── logs                 Read daemon logs
│   ├── stream               Stream structured log events in real-time across daemon, workflows, and runs
│   ├── clear-logs           Clear daemon logs
│   ├── agents               List daemon-managed agents
│   ├── config               Update daemon automation configuration
│   ├── preflight            Report plugin preflight status (which required plugins are installed, which are missing, and the fix commands)
│   └── metrics              Print daemon observability metrics (counters, gauges, histograms)
│
├── agent                    Run and inspect agent executions
│   ├── list                 List configured agent profiles
│   ├── get                  Get a configured agent profile
│   ├── run                  Start an agent run
│   ├── control              Control an existing agent run
│   ├── status               Read status for a run id
│   ├── memory
│   │   ├── get              Read memory for a configured agent
│   │   ├── append           Append a memory entry for a configured agent
│   │   └── clear            Clear memory for a configured agent
│   └── message
│       ├── send             Send a message on an agent channel
│       └── list             List agent messages
│
├── project                  Manage project registration and metadata
│   ├── list                 List registered projects
│   ├── active               Show the active project
│   ├── get                  Get a project by id
│   ├── create               Create a new project entry
│   ├── load                 Mark a project as active
│   ├── rename               Rename a project
│   ├── archive              Archive a project
│   └── remove               Remove a project
│
├── queue                    Inspect and mutate the daemon dispatch queue
│   ├── list                 List queued dispatches
│   ├── stats                Show queue statistics
│   ├── enqueue              Enqueue a subject dispatch for a task, requirement, or custom title
│   ├── hold                 Hold a queued subject
│   ├── release              Release a held queued subject
│   ├── drop                 Drop (remove) a queued subject dispatch regardless of status
│   └── reorder              Reorder queued subjects by subject id
│
├── workflow                 Run and control workflow execution
│   ├── list                 List workflows
│   ├── get                  Get workflow details
│   ├── decisions            Show workflow decisions
│   ├── checkpoints
│   │   ├── list             List checkpoints for a workflow
│   │   ├── get              Get a specific checkpoint for a workflow
│   │   └── prune            Prune checkpoints using count and/or age retention
│   ├── run                  Run a workflow. Enqueues to daemon by default; use --sync to run in terminal
│   ├── resume               Resume a paused workflow
│   ├── resume-status        Check whether a workflow can be resumed
│   ├── pause                Pause an active workflow (confirmation required)
│   ├── cancel               Cancel a workflow (confirmation required)
│   ├── phase
│   │   ├── approve          Approve a pending phase gate
│   │   └── reject           Reject a pending phase gate
│   ├── phases
│   │   ├── list             List configured workflow phases
│   │   ├── get              Get a workflow phase by id
│   │   ├── upsert           Create or replace a workflow phase definition
│   │   └── remove           Remove a workflow phase definition (confirmation required)
│   ├── definitions
│   │   ├── list             List configured workflow definitions
│   │   └── upsert           Create or replace a workflow definition
│   ├── config
│   │   ├── get              Read resolved workflow config
│   │   ├── validate         Validate workflow config shape and references
│   │   └── compile          Validate and resolve YAML workflow files
│   ├── state-machine
│   │   ├── get              Read workflow state-machine config
│   │   ├── validate         Validate workflow state-machine config
│   │   └── set              Replace workflow state-machine config JSON
│   ├── agent-runtime
│   │   ├── get              Read workflow agent-runtime config
│   │   ├── validate         Validate workflow agent-runtime config
│   │   └── set              Replace workflow agent-runtime config JSON
│   ├── prompt
│   │   └── render           Render workflow phase prompt text and prompt sections
│
├── history                  Inspect and search execution history
│   ├── task                 List history records for a task
│   ├── get                  Get a history record by id
│   ├── recent               List recent history records
│   ├── search               Search history records
│   └── cleanup              Remove old history records
│
├── git                      Manage Git repositories, worktrees, and confirmation requests
│   ├── repo
│   │   ├── list             List registered repositories
│   │   ├── get              Get details for one repository
│   │   ├── init             Initialize and register a local repository
│   │   └── clone            Clone and register a repository
│   ├── branches             List repository branches
│   ├── status               Show repository status
│   ├── commit               Commit staged/untracked changes
│   ├── push                 Push branch updates
│   ├── pull                 Pull branch updates
│   ├── worktree
│   │   ├── create           Create a repository worktree
│   │   ├── list             List repository worktrees
│   │   ├── get              Get one worktree by name
│   │   ├── remove           Remove a worktree (confirmation required)
│   │   ├── prune            Prune managed task worktrees for done/cancelled tasks
│   │   ├── pull             Pull updates in a worktree
│   │   ├── push             Push updates from a worktree
│   │   ├── sync             Pull then push a worktree
│   │   └── sync-status      Show synchronization status for a worktree
│   └── confirm
│       ├── request          Request a confirmation record for a destructive git operation
│       ├── respond          Approve or reject a confirmation request
│       └── outcome          Record operation outcome for a confirmation request
│
├── skill                    Search, install, update, and publish versioned skills
│   ├── search               Search skills across built-in, user, project, and registry sources
│   ├── install              Install a skill with deterministic resolution
│   ├── list                 List all available skills (built-in, user, project, and installed)
│   ├── show                 Show details of a resolved skill definition
│   ├── update               Re-resolve one or all installed skills
│   ├── publish              Publish a new skill version into the registry catalog
│   ├── migrate-from-ao      Move legacy .ao/skills/ into .animus/skills/ (v0.3 → v0.4)
│   └── registry
│       ├── add              Register a new registry source or update an existing one
│       ├── remove           Remove a registered registry source
│       └── list             List all registered registry sources
│
├── model                    Inspect model availability, validation, and evaluations
│   ├── availability         Check model availability for one or more model ids
│   ├── status               Show configured model and API-key status
│   ├── validate             Validate model selection for a task or explicit list
│   ├── roster
│   │   ├── refresh          Refresh model roster from providers
│   │   └── get              Get current model roster snapshot
│   └── eval
│       ├── run              Run model evaluation
│       └── report           Show latest model evaluation report
│
├── pack                     Install, inspect, and pin workflow packs
│   ├── install              Install a pack from a local path or marketplace registry
│   ├── list                 List discovered packs and indicate which ones are active for this project
│   ├── inspect              Inspect a discovered pack or a local pack manifest
│   ├── pin                  Pin a pack version/source or toggle enablement for this project
│   ├── search               Search packs across marketplace registries
│   └── registry
│       ├── add              Add a marketplace registry (git URL)
│       ├── remove           Remove a marketplace registry
│       ├── list             List all registered marketplace registries
│       └── sync             Sync (re-clone) a registry to get latest pack catalog
│
├── plugin                   Discover, inspect, install, and call Animus STDIO plugins
│   ├── list                 Discover plugins via plugins.yaml, .animus/plugins/, $ANIMUS_PLUGIN_DIR, and $ANIMUS_PLUGIN_PATH
│   ├── info                 Print a plugin's manifest plus initialize-time capabilities
│   ├── call                 Send a JSON-RPC request to a plugin and print its response
│   ├── ping                 Health-check a plugin by spawning it, completing the handshake, and pinging
│   ├── install              Install a plugin binary from a public GitHub release (OWNER/REPO[@TAG]), a local path, or a direct URL into ~/.animus/plugins/ (override with --plugin-dir or $ANIMUS_PLUGIN_DIR)
│   ├── uninstall            Remove a previously installed plugin from ~/.animus/plugins/ (override with --plugin-dir or $ANIMUS_PLUGIN_DIR) and ~/.animus/plugins.yaml
│   ├── new                  Scaffold a new plugin project from the launchapp-dev/animus-plugin-template scaffold
│   ├── search               Search the public Animus plugin registry by substring + filters
│   ├── browse               Browse the public Animus plugin registry, grouped by kind
│   ├── update               Update one or all installed release-source plugins to the latest tag
│   ├── install-defaults     Install the standard set of provider plugins from public GitHub releases (claude, codex, gemini, opencode, oai). Skips plugins that are already installed. Optional flags pull in additional default plugins
│   └── lock                 Inspect and verify the plugin lockfile (`.animus/plugins.lock`). The lockfile records sha256 + version for every installed plugin so an `install --force` or tampered-binary scenario is visible to operators
│       ├── list             List every entry currently recorded in the plugin lockfile
│       └── verify           Re-hash every installed plugin binary and report mismatches against the lockfile
│
├── runner                   Inspect runner health and orphaned runs
│   ├── health               Show runner process health
│   ├── orphans
│   │   ├── detect           Detect orphaned runner processes
│   │   └── cleanup          Clean orphaned runner processes
│   └── restart-stats        Show runner restart statistics
│
├── status                   Show a unified project status dashboard
├── output                   Inspect run output and artifacts
│   ├── run                  Read run event payloads
│   ├── phase-outputs        Read persisted workflow phase outputs
│   ├── artifacts            List artifacts for an execution id
│   ├── download             Download an artifact payload
│   ├── jsonl                Read aggregated JSONL output streams for a run
│   ├── monitor              Inspect run output with optional task/phase filtering
│   └── cli                  Infer CLI provider details from run output
│
├── mcp                      Run the Animus MCP service endpoint
│   ├── serve                Start the MCP server in the current process
│   └── memory               Start the memory context MCP server for workflow phases
│
├── web                      Serve and open the Animus web UI
│   ├── serve                Spawn installed transport_backend + web_ui plugins and report bound URLs. Requires plugins from `animus plugin install-defaults --include-transports`
│   └── open                 Open the Animus web UI URL in a browser. Resolves the URL from an installed web_ui or transport_backend plugin unless --url is supplied
│
├── init                     Initialize an Animus project from a template
│   (no subcommands)         Supports registry-backed or local copy templates, plan mode, and daemon defaults
│
├── doctor                   Run environment and configuration diagnostics
│
├── trigger                  Inspect and manage event triggers
│   ├── list                 List all configured event triggers for this project
│   └── fire                 Manually fire a webhook trigger (for testing and development)
│
├── logs                     Tail and inspect daemon log output (in-tree or via log_storage_backend plugin)
│   └── tail                 Tail recent log entries from the active log storage backend
│
├── subject                  List, get, create, and update subjects via installed subject_backend plugins
│   ├── list                 List subjects for a given kind via the active subject_backend plugin
│   ├── get                  Fetch a single subject by id from the active subject_backend plugin
│   ├── create               Create a subject through the active subject_backend plugin
│   ├── update               Update a subject through the active subject_backend plugin
│   ├── next                 Return the highest-priority Ready subject for the given kind
│   └── status               Set the status of a subject by id through the active subject_backend
│
├── flavor                   Inspect or install Animus flavor manifests (`flavors/<name>.toml`) — v0.5
│   ├── list                 List available flavor manifests on disk
│   ├── current              Show the active flavor and drift against the manifest
│   ├── describe             Print a parsed flavor manifest (TOML by default, JSON via `--json`)
│   └── install              Install the named flavor (`default` only in v0.5); equivalent to `animus plugin install-defaults --include-subjects --include-transports`
│
└── help                     Print this message or the help of the given subcommand(s)
```

> **v0.4.4 surfaces removed.** Use `animus subject --kind task` for the
> former `animus task ...` tree and `animus subject --kind requirement`
> for the former `animus requirements ...` tree. `animus setup` was
> folded into `animus init`, `animus now` into `animus status`, and
> `animus errors` into `animus history`. `animus cloud` was retired —
> cloud sync ships as a separate plugin. See the v0.4.4 entry in
> [CHANGELOG.md](../../../CHANGELOG.md) for the full surface map.

## Selected Command Flags

The full flag set lives in `crates/orchestrator-cli/src/cli_types/`. This section
documents flags that were added or hardened in v0.4.0 and that callers most often
need to script against.

### `animus daemon start` / `animus daemon run` (plugin preflight)

The daemon runs a plugin preflight on every startup. Default posture is
**default-deny**: if a required role is unsatisfied (no provider plugin, or no
subject backend claiming `task` / `requirement`) the daemon refuses to start and
prints the exact `animus plugin install ...` command to remediate.

| Flag | Description |
|---|---|
| `--auto-install` | When preflight finds a missing role, install the daemon's recommended default plugin (pinned `owner/repo@tag`) before continuing. Avoids surprise network fetches when omitted. |
| `--skip-preflight` | Bypass preflight entirely. Escape hatch for dev iteration or intentionally degraded runs when required provider or subject plugins are not installed. |

### `animus daemon preflight`

Standalone preflight report. Runs the same checks as daemon startup but never
starts the daemon. Useful for CI and onboarding to confirm a project's plugin
prerequisites are in place.

| Flag | Description |
|---|---|
| `--auto-install` | Install missing required plugins from the daemon's recommended defaults instead of just reporting them. |

JSON envelope: `animus.daemon.preflight.v1` with fields `satisfied`, `missing`,
`auto_installed`, `ok`, `fix_message`.

Exit code matrix:

| Code | Meaning |
|---|---|
| 0 | All required roles satisfied. |
| 2 | At least one required role is missing. The error envelope's `message` carries the `animus plugin install ...` fix. CI scripts and `&&` chains can rely on this. |
| 1 | Transient plugin discovery failure (broken install index, IO error, etc.). Distinct from "ran successfully and found gaps". |

### `animus logs tail`

Tail recent persisted log entries from the active log storage backend. This is
the bounded, pull-style log reader; for live structured events use
`animus daemon stream`.

| Flag | Description |
|---|---|
| `--plugin <NAME>` | Filter entries to a named source plugin. With the in-tree fallback this matches the structured entry's `provider` field |
| `--level <LEVEL>` | Minimum severity to include. One of `debug`, `info`, `warn`, `error`. Default `info` |
| `--since <DURATION>` | Only return entries newer than the supplied duration (for example `1h`, `30m`, `15s`). Default `1h` |
| `--limit <COUNT>` | Maximum number of entries to return. Default `100` |
| `--follow` | Reserved for future streaming support. Today the in-tree fallback still returns the requested batch and exits, so use `animus daemon stream` for live follow behavior |

When a `log_storage_backend` plugin is installed, `animus logs tail` reads
through that backend. Set
`ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN=1` to force the in-tree
`~/.animus/<repo-scope>/logs/events.jsonl` fallback.

### `animus init`

Initialize an Animus project from a template registry or a local template directory.

| Flag | Description |
|---|---|
| `--template <TEMPLATE_ID>` | Project template id to fetch from the default template registry. Conflicts with `--path` |
| `--path <PATH>` | Local template directory containing `template.toml`. Conflicts with `--template` |
| `--non-interactive` | Run without prompts. Requires `--template`, `--path`, or `--walkthrough` |
| `--plan` | Preview init changes without writing project files |
| `--force` | Overwrite existing project files targeted by the template |
| `--update-registry` | Fetch the latest commit from the template registry and re-pin the local cache before loading the template (v0.4.0 supply-chain hardening — by default the registry uses the pinned cache) |
| `--walkthrough` | Run the onboarding walkthrough: detect CLIs, install default plugins, and copy the bundled hello-world workflow |
| `--no-install` | Walkthrough only: skip `animus plugin install-defaults` |
| `--no-template` | Walkthrough only: skip copying the hello-world workflow template into `.animus/workflows/` |
| `--auto-start` | Walkthrough only: start the autonomous daemon after init completes |
| `--walkthrough-template <NAME>` | Walkthrough only: choose the bundled workflow template. Current default is `hello-world` |
| `--auto-merge <bool>` | Override the template default for automatic merge |
| `--auto-pr <bool>` | Override the template default for automatic pull request creation |
| `--auto-commit-before-merge <bool>` | Override the template default for automatic commit before merge |

The template registry URL can be overridden globally via `ANIMUS_TEMPLATE_REGISTRY_URL`.
In `--json` mode, `animus init` also returns `recommended_install`, sourced from
`crates/orchestrator-cli/config/default-install.json`, so automations can read the
recommended pack and plugin set without scraping prose.

### `animus plugin install`

Install a plugin binary into `~/.animus/plugins/` after verifying its integrity.

Three install sources, mutually exclusive:

```bash
# 1. Public GitHub repo (latest release, or pinned with @tag / --tag)
animus plugin install launchapp-dev/animus-provider-claude
animus plugin install launchapp-dev/animus-provider-claude@v0.2.2
animus plugin install launchapp-dev/animus-provider-claude --tag v0.2.2

# 2. Local binary
animus plugin install --path ./target/release/animus-provider-claude

# 3. HTTPS URL with mandatory checksum
animus plugin install --url https://example.com/plugin --sha256 a1b2c3d4...
```

| Argument / Flag | Description |
|---|---|
| `<OWNER/REPO[@TAG]>` | Public GitHub repo slug (positional). Resolves the latest release (or supplied tag), downloads the matching architecture asset, verifies the published checksum, installs the binary, and registers it in `~/.animus/plugins.yaml`. Mutually exclusive with `--path` and `--url` |
| `--path <PATH>` | Local path to the plugin binary. SHA256 verification is optional for local installs |
| `--url <URL>` | HTTPS URL to download the plugin binary from. `--sha256` is **required** when installing from a URL (v0.4.0 supply-chain hardening) |
| `--tag <TAG>` | Release tag to install when using the `owner/repo` positional. Defaults to the latest release. Conflicts with the `@tag` syntax on the positional |
| `--latest` | Explicit opt-in to resolving the latest release when no tag is given (this is the default; the flag exists for self-documenting commands). Conflicts with `--tag` and `owner/repo@tag` syntax |
| `--name <NAME>` | Optional logical plugin name. Defaults to the binary file name |
| `--sha256 <HEX>` | Expected SHA256 hex digest. Required with `--url`; optional with `--path` or a public-repo install. The install fails if the downloaded/copied binary's checksum does not match |
| `--force` | Overwrite an existing installed plugin with the same name |
| `--skip-manifest-check` | Skip running `--manifest` against the installed binary to verify it (use sparingly) |
| `--plugin-dir <PATH>` | Override the plugin install directory. Takes precedence over `$ANIMUS_PLUGIN_DIR`. Defaults to `~/.animus/plugins/` |
| `--signature-policy <strict\|warn\|disabled>` | Signature enforcement mode. `strict` fails closed, `warn` logs and proceeds, and `disabled` skips verification |
| `--allow-unsigned` | Convenience alias for `--signature-policy warn`; mutually exclusive with `--signature-policy` and `--require-signature` |
| `--require-signature` | Legacy alias for `--signature-policy strict` |
| `--skip-signature` | Legacy alias for `--signature-policy disabled` |
| `--trusted-signers <PATH>` | Path to a trusted-signers YAML allowlist. Defaults to `~/.animus/trusted-signers.yaml`. When the file is absent, the CLI verifies signatures against the cert's stated repo identity but does not enforce a publisher allowlist |
| `--allow-shadow-builtin` | Permit installing a provider plugin whose `provider_tool` collides with an in-tree backend (`claude` / `codex` / `gemini` / `opencode` / `oai-runner`). Without this flag the install pipeline refuses such plugins because they silently hijack all dispatch for the matching tool |
| `--allow-org <OWNER>` | Mark an additional GitHub owner as trusted (repeatable). Skips the trust-on-first-use prompt for that owner and writes the entry to `~/.animus/trusted-orgs.yaml` after the install succeeds |
| `--yes` | Auto-confirm the trust-on-first-use prompt for unknown orgs |
| `--force-rewrite-lockfile` | Discard an unparseable / schema-incompatible `.animus/plugins.lock` (or `~/.animus/plugins.lock`) and rebuild a fresh lockfile starting from this install. Without this flag, an unreadable lockfile fails the install **closed** with an actionable error pointing at the corrupt path. **Security warning**: rewriting drops the recorded sha256 integrity history, so subsequent `--force` installs cannot detect pre-existing tamper. See [Security › Lockfile fail-closed policy](../security.md#lockfile-fail-closed-policy) |

#### Signature verification (v0.4.x+)

When installing from a public repo, the CLI looks for a cosign keyless bundle next to the release asset and verifies it via `cosign verify-blob`. The outcome (one of `verified`, `unsigned`, `invalid`, `untrusted_signer`, `skipped`) is persisted in `~/.animus/plugins.yaml` and surfaced in the `SIG` column of `animus plugin list`. See [Security](../security.md) and [Plugin Signing](../../architecture/plugin-signing.md).

- **`--signature-policy strict`**: refuse install when the bundle is missing, invalid, or signed by an untrusted identity. Requires `cosign` on `$PATH`.
- **`--signature-policy warn`**: log signature failures and install with `signature_status=unsigned` or `untrusted_signer`. This is the v0.4.12 transition default.
- **`--signature-policy disabled`**: bypass verification entirely; install records `signature_status=skipped`.
- **Legacy flags**: `--require-signature` maps to `strict`, `--skip-signature` maps to `disabled`, and `--allow-unsigned` maps to `warn`.

The trusted-signers file format:

```yaml
trusted_signers:
  - identity: "launchapp-dev/animus-*"
    issuer: "https://token.actions.githubusercontent.com"
```

`identity` is a glob (`*` / `?`) matched against `<owner>/<repo>`. When the file is absent, the default is "any signer is acceptable, but the cosign cert must claim an identity rooted at the repo we downloaded from."

### `animus flavor`

v0.5 introduces a single curated flavor manifest at
`flavors/default.toml`. The `animus flavor` subcommand inspects and installs
it.

```bash
# Show every flavor manifest the loader discovered.
animus flavor list

# Print the parsed manifest (TOML by default, JSON via --json).
animus flavor describe --name default
animus flavor describe --name default --json

# Show drift: which required plugins are installed vs missing.
animus flavor current
animus flavor current --json

# Install the flavor. v0.5 only supports `default`; refuses other names
# per the "One flavor at launch" discipline rule.
animus flavor install            # uses `default`
animus flavor install default
```

`animus flavor install` is equivalent to
`animus plugin install-defaults --include-subjects --include-transports` and
installs every required plugin slug the manifest declares whose curated tag
is pinned in
[`crates/orchestrator-core/src/plugin_registry.rs`](../../../crates/orchestrator-core/src/plugin_registry.rs).
Slugs the manifest declares but the constants table hasn't pinned yet (e.g.
`animus-provider-ollama`, `animus-trigger-cron`) emit a warning and are
skipped — the manifest is forward-looking; the constants table is the
authoritative tag pin.

The loader probes for `flavors/<name>.toml` in this order:

1. `$ANIMUS_FLAVORS_DIR` if set
2. `<cwd>/flavors/`
3. parent directories walking up to `/`

JSON output uses the `animus.flavor.cli.v1` envelope.

### `animus plugin install-defaults`

Bulk-install the standard provider plugin set in one shot. Each repo runs through
the same install pipeline as `animus plugin install`, so signature checks,
manifest probes, and the `launchapp-dev` org allowlist are preserved.

When `flavors/default.toml` is present, the install plan is sourced from the
manifest's `[providers]`, `[subjects]`, `[transports]`, `[workflow_runner]`,
and `[queue]` sections (plus optional `[ui]` and recommended add-ons gated by
`--include-*` flags). When the manifest is absent, the hardcoded
`DEFAULT_PROVIDER_PLUGINS / DEFAULT_SUBJECT_PLUGINS / DEFAULT_TRANSPORT_PLUGINS`
tables remain the fallback.

```bash
# Install all 5 default providers (claude, codex, gemini, opencode, oai)
animus plugin install-defaults

# Add the OAI-agent plugin
animus plugin install-defaults --include-oai-agent

# Add the default subject_backend plugins (default, requirements, linear, sqlite, markdown)
animus plugin install-defaults --include-subjects
```

| Flag | Description |
|---|---|
| `--plugin-dir <PATH>` | Override the plugin install directory. Same semantics as `animus plugin install --plugin-dir` |
| `--force` | Reinstall plugins that are already present (default: skip with a warning) |
| `--yes` | Auto-confirm the trust-on-first-use prompt for the `launchapp-dev` org |
| `--include-oai-agent` | Also install `animus-provider-oai-agent` (curated tag in `orchestrator-core::plugin_registry::DEFAULT_OAI_AGENT_PLUGINS`) |
| `--include-subjects` | Also install the default subject_backend plugins (`subject-default`, `subject-requirements`, `subject-linear`, `subject-sqlite`, `subject-markdown`) |
| `--include-transports` | Also install the default transport_backend + web_ui plugins (`transport-http`, `transport-graphql`, `web-ui`) that back `animus web` |
| `--json` | Emit per-plugin results + summary as JSON |
| `--force-rewrite-lockfile` | Discard an unparseable / schema-incompatible `plugins.lock` and rebuild a fresh lockfile for the batch. Without this flag the batch fails closed up front, *before* the per-target skip loop runs, so an all-skipped run cannot mask a corrupt lockfile. Same security caveat as `animus plugin install --force-rewrite-lockfile` |

The command pins each install to the curated release tags declared in
[`crates/orchestrator-core/src/plugin_registry.rs`](../../../crates/orchestrator-core/src/plugin_registry.rs)
and are shared with the daemon preflight, so bumping the registry rolls both
surfaces at once. Plugins that fail to install are recorded in the summary's
`failed` count, the per-repo failure is emitted in the JSON envelope, and the
process exits non-zero so installer scripts can detect partial failure
(codex round-6 P2).

### `animus plugin list` / `info` / `call` / `ping`

The discovery scan deliberately omits `$PATH` by default in v0.4.0 to prevent stray
binaries from being picked up. Pass `--include-system-path` to opt in to scanning
`$PATH` for `animus-provider-*` and `animus-plugin-*` binaries.

`animus plugin info`, `animus plugin ping`, and `animus plugin call` spawn the
target binary with manifest-derived env checks enabled. If the plugin declares
required vars in `env_required` and they are unset, these commands now fail
before handshake instead of proceeding with a partially initialized process.

| Command | Flags |
|---|---|
| `animus plugin list` | `--include-system-path` |
| `animus plugin info` | `--name <NAME>`, `--include-system-path` |
| `animus plugin call` | `--name <NAME>`, `--method <METHOD>`, `--params <JSON>`, `--include-system-path` |
| `animus plugin ping` | `--name <NAME>`, `--include-system-path` |
| `animus plugin uninstall` | `--name <NAME>`, `--plugin-dir <PATH>` |

Default discovery order (no `--include-system-path`):
`~/.animus/plugins.yaml` (or the legacy `~/.config/animus/plugins.yaml` only
when the new registry is absent) → `.animus/plugins/` → global install dir
(`$ANIMUS_PLUGIN_DIR` when set, otherwise `~/.animus/plugins/`) →
`$ANIMUS_PLUGIN_PATH`. With `--include-system-path`, `$PATH` is appended.

`animus plugin list --json` returns a top-level `warnings` array when a configured
plugin failed its `--manifest` probe (binary missing, exited non-zero, returned
non-JSON, etc.). Human output emits each warning to stderr. The
`animus.plugin.list` MCP tool carries the same `warnings` field.

### `animus web serve` / `open`

`animus web` uses the same manifest-derived env checks as the one-shot plugin
commands above. Required vars declared by the selected `transport_backend` or
`web_ui` plugin must be present before the CLI will spawn them.

| Command | Flags |
|---|---|
| `animus web serve` | `--open`, `--json` |
| `animus web open` | `--url <URL>`, `--path <PATH>`, `--json` |

`animus web serve --open` starts the transport plugins and launches the
resolved browser URL in one step. `animus web open --url <URL>` skips plugin
discovery entirely and opens the supplied URL directly; `--path <PATH>`
appends a sub-path such as `/runs` when the URL is resolved from installed
plugins.

If `animus web serve` or `animus web open` fails even though the transport
plugins are installed, inspect the target plugin with
`animus plugin info --name <plugin-name>` and set any missing `env_required`
entries first.

### `animus plugin search` / `browse` / `update`

Marketplace commands read the public plugin registry and optionally compare it
with installed release-source plugins.

| Command | Flags |
|---|---|
| `animus plugin search [QUERY]` | `--kind <KIND>`, `--tag <TAG>` (repeatable), `--org <ORG>`, `--stability <STABILITY>`, `--registry-url <URL>`, `--no-cache`, `--json` |
| `animus plugin browse` | `--kind <KIND>`, `--installed`, `--available`, `--registry-url <URL>`, `--no-cache`, `--json` |
| `animus plugin update [NAME]` | `--tag <TAG>`, `--dry-run`, `--force`, `--json` |

Use `animus plugin update --dry-run` as the update check; there is no
`plugin list --check-updates` flag.

### `animus plugin lock`

The plugin lockfile records installed plugin version and SHA256 metadata.
Project-local installs use `.animus/plugins.lock`; otherwise commands fall back
to `~/.animus/plugins.lock`.

| Command | Flags |
|---|---|
| `animus plugin lock list` | `--lockfile <PATH>`, `--json` |
| `animus plugin lock verify` | `--lockfile <PATH>`, `--plugin-dir <PATH>`, `--json` |

### `animus plugin new`

Scaffold a new plugin project from the
[`launchapp-dev/animus-plugin-template`](https://github.com/launchapp-dev/animus-plugin-template)
repository. Clones the template at the requested ref, copies the
`<kind>/` subdirectory into the output directory, substitutes
`{{var}}` markers, and strips the `.tmpl` suffix from rendered files.

| Flag | Description |
|---|---|
| `--kind <KIND>` | Plugin kind: `subject`, `provider`, or `trigger` |
| `--name <NAME>` | Plugin short name in kebab-case (e.g. `jira`, `linear`, `openai-compat`) |
| `--org <ORG>` | GitHub org used in the generated project's repository field. Default `launchapp-dev` |
| `--description <TEXT>` | Short description. Defaults to `An Animus <kind> backend plugin` |
| `--out-dir <PATH>` | Output directory. Defaults to `./animus-<kind>-<name>` |
| `--template-version <REF>` | Git branch or tag to clone. Default `main` |
| `--template-repo <URL>` | Template git URL. Defaults to `launchapp-dev/animus-plugin-template` |
| `--template-path <PATH>` | Use a local checkout of the template repo (skips `git clone`) |
| `--force` | Overwrite an existing output directory |

Substitution variables (hardcoded today; see `template-manifest.toml`
in the template repo for the source of truth): `name`, `NAME_UPPER`,
`NAME_PASCAL`, `name_snake`, `kind`, `full_name`, `description`,
`org`, `year`, `author` (from `git config user.name`),
`author_email` (from `git config user.email`).

## Summary

| Metric | Count |
|---|---|
| Top-level commands | 22 |
| Nested command entries (all levels) | 175 |

Counts exclude autogenerated `help` entries.
