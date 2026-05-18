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
│   └── config               Update daemon automation configuration
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
├── task                     Manage tasks, dependencies, status, and operational controls
│   ├── list                 List tasks with optional filters
│   ├── next                 Get the next ready task
│   ├── stats                Show task statistics
│   ├── get                  Get a task by id
│   ├── create               Create a task
│   ├── update               Update a task
│   ├── delete               Delete a task (confirmation required)
│   ├── assign               Assign an assignee to a task
│   ├── checklist-add        Add a checklist item
│   ├── checklist-update     Mark a checklist item complete/incomplete
│   ├── dependency-add       Add a task dependency edge
│   ├── dependency-remove    Remove a task dependency edge
│   ├── status               Set task status
│   ├── history              Show workflow dispatch history for a task
│   ├── pause                Pause a task
│   ├── resume               Resume a paused task
│   ├── cancel               Cancel a task (confirmation required)
│   ├── reopen               Reopen a task from terminal state (Done/Cancelled) back to Backlog
│   ├── set-priority         Set task priority
│   ├── set-deadline         Set or clear task deadline
│   └── rebalance-priority   Rebalance task priorities using a high-priority budget policy
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
├── requirements            Draft and manage project requirements
│   ├── execute              Execute a requirement into implementation tasks and optional workflows
│   ├── list                 List requirements
│   ├── get                  Get a requirement by id
│   ├── create               Create a requirement
│   ├── update               Update a requirement
│   ├── delete               Delete a requirement
│   ├── graph
│   │   ├── get              Read the requirement graph
│   │   └── save             Replace the requirement graph with provided JSON
│   ├── mockups
│   │   ├── list             List requirement mockups
│   │   ├── create           Create a mockup record
│   │   ├── link             Link a mockup to requirements or flows
│   │   └── get-file         Get a mockup file by relative path
│   └── recommendations
│       ├── scan             Run recommendation scan over current project context
│       ├── list             List saved recommendation reports
│       ├── apply            Apply a recommendation report
│       ├── config-get       Read recommendation config
│       └── config-update    Update recommendation config
│
├── history                  Inspect and search execution history
│   ├── task                 List history records for a task
│   ├── get                  Get a history record by id
│   ├── recent               List recent history records
│   ├── search               Search history records
│   └── cleanup              Remove old history records
│
├── errors                   Inspect and retry recorded operational errors
│   ├── list                 List recorded errors
│   ├── get                  Get an error by id
│   ├── stats                Show error summary stats
│   ├── retry                Retry an error by id
│   └── cleanup              Remove old error records
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
│   ├── install              Install a skill from registry resolution or a local Markdown skill path
│   ├── list                 List all available skills (built-in, user, project, and installed)
│   ├── show                 Show details of a resolved skill definition
│   ├── update               Re-resolve one or all installed skills
│   ├── publish              Publish a new skill version into the registry catalog
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
│   ├── ping                 Health-check a plugin by spawning, handshaking, and pinging
│   ├── install              Install a plugin from a public GitHub repo (owner/repo[@tag]), a local --path, or an https --url --sha256
│   ├── uninstall            Remove a previously installed plugin from ~/.animus/plugins/
│   └── new                  Scaffold a new plugin project from launchapp-dev/animus-plugin-template
│
├── runner                   Inspect runner health and orphaned runs
│   ├── health               Show runner process health
│   ├── orphans
│   │   ├── detect           Detect orphaned runner processes
│   │   └── cleanup          Clean orphaned runner processes
│   └── restart-stats        Show runner restart statistics
│
├── status                   Show a unified project status dashboard
├── now                      Show unified work inbox and current focus
│   (no subcommands)         Displays: next task, active workflows, blocked items, stale items
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
│   └── serve                Start the MCP server in the current process
│
├── web                      Serve and open the Animus web UI
│   ├── serve                 Start the Animus web server
│   └── open                  Open the Animus web UI URL in a browser
│
├── init                     Initialize an Animus project from a template
│   (no subcommands)         Supports registry-backed or local copy templates, plan mode, and daemon defaults
│
├── setup                    Guided onboarding and configuration wizard
├── cloud                    Sync tasks and requirements with a remote animus-sync server
│   ├── login                Authenticate with animus cloud using device auth flow
│   ├── setup                Configure the sync server connection for this project
│   ├── push                 Push local tasks, requirements, and workflow config to the sync server
│   ├── pull                 Pull tasks and requirements from the sync server into local state
│   ├── status               Show sync configuration, cloud projects, daemon states, and active workflows
│   ├── link                 Link this project (auto-detects from git remote or uses explicit project ID)
│   └── deploy               Manage deployments on animus-cloud
│
└── doctor                   Run environment and configuration diagnostics
```

## Selected Command Flags

The full flag set lives in `crates/orchestrator-cli/src/cli_types/`. This section
documents flags that were added or hardened in v0.4.0 and that callers most often
need to script against.

### `animus init`

Initialize an Animus project from a template registry or a local template directory.

| Flag | Description |
|---|---|
| `--template <TEMPLATE_ID>` | Project template id to fetch from the default template registry. Conflicts with `--path` |
| `--path <PATH>` | Local template directory containing `template.toml`. Conflicts with `--template` |
| `--non-interactive` | Run without prompts. Requires `--template` or `--path` |
| `--plan` | Preview init changes without writing project files |
| `--force` | Overwrite existing project files targeted by the template |
| `--update-registry` | Fetch the latest commit from the template registry and re-pin the local cache before loading the template (v0.4.0 supply-chain hardening — by default the registry uses the pinned cache) |
| `--auto-merge <bool>` | Override the template default for automatic merge |
| `--auto-pr <bool>` | Override the template default for automatic pull request creation |
| `--auto-commit-before-merge <bool>` | Override the template default for automatic commit before merge |

The template registry URL can be overridden globally via `ANIMUS_TEMPLATE_REGISTRY_URL`.

### `animus plugin install`

Install a plugin binary into `~/.animus/plugins/` after verifying its integrity.

Three install sources, mutually exclusive:

```bash
# 1. Public GitHub repo (latest release, or pinned with @tag / --tag)
animus plugin install launchapp-dev/animus-provider-claude
animus plugin install launchapp-dev/animus-provider-claude@v0.1.0
animus plugin install launchapp-dev/animus-provider-claude --tag v0.1.0

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
| `--require-signature` | Refuse to install when no cosign signature bundle is published, or when verification fails. Mutually exclusive with `--skip-signature`. Requires `cosign` on `$PATH` |
| `--skip-signature` | Bypass cosign signature verification entirely. Use this for plugins that haven't adopted signing yet, air-gapped installs, or local-build workflows |
| `--trusted-signers <PATH>` | Path to a trusted-signers YAML allowlist. Defaults to `~/.animus/trusted-signers.yaml`. When the file is absent, the CLI verifies signatures against the cert's stated repo identity but does not enforce a publisher allowlist |
| `--allow-shadow-builtin` | Permit installing a provider plugin whose `provider_tool` collides with an in-tree backend (`claude` / `codex` / `gemini` / `opencode` / `oai-runner`). Without this flag the install pipeline refuses such plugins because they silently hijack all dispatch for the matching tool |
| `--allow-org <OWNER>` | Mark an additional GitHub owner as trusted (repeatable). Skips the trust-on-first-use prompt for that owner and writes the entry to `~/.animus/trusted-orgs.yaml` after the install succeeds |
| `--yes` | Auto-confirm the trust-on-first-use prompt for unknown orgs |

#### Signature verification (v0.4.x+)

When installing from a public repo, the CLI looks for `<asset>.tar.gz.bundle` next to the binary asset and verifies it via `cosign verify-blob` (sigstore keyless). The outcome (one of `verified`, `unsigned`, `invalid`, `untrusted_signer`, `skipped`) is persisted in `~/.animus/plugins.yaml` and surfaced in the `SIG` column of `animus plugin list`. See [docs/architecture/plugin-signing.md](../../architecture/plugin-signing.md).

- **Default mode** (no flags): verify if a bundle is present, install with `signature_status=verified`; if no bundle, warn and install with `signature_status=unsigned`. A FAILING signature refuses install.
- **`--require-signature`**: refuse install when no bundle is published or verification fails.
- **`--skip-signature`**: bypass verification entirely; install records `signature_status=skipped`.
- **Missing `cosign` binary**: install proceeds and records `signature_status=unsigned`. Install [cosign](https://github.com/sigstore/cosign) to enable verification.

The trusted-signers file format:

```yaml
trusted_signers:
  - identity: "launchapp-dev/animus-*"
    issuer: "https://token.actions.githubusercontent.com"
```

`identity` is a glob (`*` / `?`) matched against `<owner>/<repo>`. When the file is absent, the default is "any signer is acceptable, but the cosign cert must claim an identity rooted at the repo we downloaded from."

### `animus plugin list` / `info` / `call` / `ping`

The discovery scan deliberately omits `$PATH` by default in v0.4.0 to prevent stray
binaries from being picked up. Pass `--include-system-path` to opt in to scanning
`$PATH` for `animus-provider-*` and `animus-plugin-*` binaries.

| Command | Flags |
|---|---|
| `animus plugin list` | `--include-system-path` |
| `animus plugin info` | `--name <NAME>`, `--include-system-path` |
| `animus plugin call` | `--name <NAME>`, `--method <METHOD>`, `--params <JSON>`, `--include-system-path` |
| `animus plugin ping` | `--name <NAME>`, `--include-system-path` |
| `animus plugin uninstall` | `--name <NAME>`, `--plugin-dir <PATH>` |

Default discovery order (no `--include-system-path`):
`~/.animus/plugins.yaml` (or the legacy `~/.config/animus/plugins.yaml` on first
read) → `.animus/plugins/` → `$ANIMUS_PLUGIN_DIR` (or `~/.animus/plugins/`) →
`$ANIMUS_PLUGIN_PATH`. With `--include-system-path`, `$PATH` is appended.

`animus plugin list --json` returns a top-level `warnings` array when a configured
plugin failed its `--manifest` probe (binary missing, exited non-zero, returned
non-JSON, etc.). Human output emits each warning to stderr. The
`animus.plugin.list` MCP tool carries the same `warnings` field.

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
| Top-level commands | 25 |
| Total subcommands (all levels) | 198 |

Counts exclude autogenerated `help` entries.
