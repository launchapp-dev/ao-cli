# Data Layout

Animus splits repository-authored configuration from repo-scoped runtime state.

## Project-Local Layout

These files live in the repository:

```text
.animus/
├── config.json
├── workflows.yaml              # optional single-file workflow source
├── workflows/
│   ├── custom.yaml
│   ├── standard-workflow.yaml
│   ├── hotfix-workflow.yaml
│   └── research-workflow.yaml
├── skills/
│   └── <skill-name>/SKILL.md   # optional project-scoped Markdown skills
└── plugins/
    └── <pack-id>/              # optional project pack override root
```

Key points:

- `.animus/workflows.yaml` and `.animus/workflows/*.yaml` are the authored workflow sources
- `.animus/skills/<name>/SKILL.md` is the project-scoped skill source (highest priority in
  the skill resolution chain)
- `.animus/plugins/<pack-id>/` is the project override root for pack content
- `.animus/config.json` stores repository-local Animus config
- Daemon automation settings are persisted under the repo-scoped runtime root,
  not under project-local `.animus/` for new writes

## Repo-Scoped Runtime Layout

Mutable runtime state lives outside the repo:

```text
~/.animus/<repo-scope>/
├── core-state.json
├── resume-config.json
├── workflow.db
├── config/
│   └── state-machines.v1.json
├── daemon/
│   └── pm-config.json
├── docs/
│   ├── architecture.json
│   ├── vision.json
│   └── product-vision.md
├── logs/
│   ├── events.jsonl
│   └── runs/
├── state/
│   ├── pack-selection.v1.json
│   ├── schedule-state.json
│   ├── reviews.json
│   ├── handoffs.json
│   ├── history.json
│   ├── errors.json
│   ├── qa-results.json
│   └── qa-review-approvals.json
└── worktrees/
```

Key points:

- `workflow.db` stores persisted workflows, tasks, requirements, and checkpoints
- `core-state.json` stores the shared runtime snapshot Animus loads at startup
- `config/state-machines.v1.json` stores the effective state-machine document
- `daemon/pm-config.json` stores persisted daemon settings
- `logs/events.jsonl` stores redacted structured runtime events when the
  in-tree log backend is active
- `worktrees/` stores managed task worktrees for that repository scope

## Machine-Wide Layout

Animus also uses machine-wide directories that are not tied to one repository:

```text
~/.animus/
├── config.json
├── credentials.json
├── daemon-events.jsonl
├── cli-tracker.json
├── packs/
│   └── <pack-id>/<version>/         # installed packs, including animus.core-skills
├── plugins/
│   └── <plugin-name>                # installed STDIO plugin binaries (animus plugin install)
├── skills/
│   └── <skill-name>/                # user-scoped Markdown skills (SKILL.md)
└── template-registries/
    └── <registry-id>/               # cached project-template registries (animus init)
        ├── .commit                  # pinned upstream commit sha
        └── templates/
            └── <template-id>/template.toml
```

Notes:

- `~/.animus/packs/animus.core-skills/<version>/` ships the bundled skill catalog. When the
  pack is installed, those skills appear with `source: "installed"`; when it is not,
  the legacy built-in fallback surfaces them as `source: "builtin"`.
- `~/.animus/template-registries/<registry-id>/` is pinned to a specific commit by default
  (v0.4.0 supply-chain hardening). `animus init --update-registry` fetches HEAD and re-pins.
- `~/.animus/plugins/` is the install target for `animus plugin install --path` and
  `animus plugin install --url --sha256`.

### Agent-host skill probes

Animus also scans well-known agent-host skill directories at discovery time. These are
treated as a separate, lower-trust source: only prompt text is honored, and structural
fields (`tool_policy`, `mcp_servers`, `env`, `extra_args`, `capabilities`, `adapters`,
`codex_config_overrides`) are stripped at parse time.

```text
~/.claude/skills/<name>/SKILL.md     # AgentHost { host: "claude-code", scope: Global }
~/.codex/skills/<name>/SKILL.md      # AgentHost { host: "codex",       scope: Global }
.claude/skills/<name>/SKILL.md       # AgentHost { host: "claude-code", scope: Project }
.codex/skills/<name>/SKILL.md        # AgentHost { host: "codex",       scope: Project }
```

These probes appear in `animus.skill.list` / `animus.skill.search` results with
`source: "agent_host"` and `source_detail.trust_tier: "prompt_text_only"`.

## Repository Scope Format

`<repo-scope>` is derived from the canonical project path:

```text
<sanitized-repo-name>-<12-hex-sha256-prefix>
```

This keeps runtime data stable across linked worktrees while avoiding collisions between repositories with the same basename.

## Mutation Policy

Do not hand-edit Animus-managed JSON or SQLite state unless you are explicitly working on Animus persistence or migrations.

Use Animus commands or Animus MCP tools instead.

## Resolution-Related Paths

| Path | Purpose |
|---|---|
| `.animus/workflows.yaml` | Single-file project workflow source |
| `.animus/workflows/*.yaml` | Multi-file project workflow sources |
| `.animus/skills/<name>/SKILL.md` | Project-scoped Markdown skill (highest skill priority) |
| `.animus/plugins/<pack-id>/` | Project-local pack override root |
| `~/.animus/<repo-scope>/workflow.db` | Persisted workflows, tasks, requirements, checkpoints |
| `~/.animus/<repo-scope>/config/state-machines.v1.json` | Repo-scoped state-machine config |
| `~/.animus/<repo-scope>/logs/events.jsonl` | Redacted structured runtime event log |
| `~/.animus/<repo-scope>/state/pack-selection.v1.json` | Repo-scoped pack selection state |
| `~/.animus/packs/<pack-id>/<version>/` | Machine-installed pack root |
| `~/.animus/packs/animus.core-skills/<version>/` | Bundled core skill catalog (installed pack) |
| `~/.animus/skills/<name>/SKILL.md` | User-scoped Markdown skill |
| `~/.animus/plugins/<name>` | Installed STDIO plugin binary (`animus plugin install`) |
| `~/.animus/template-registries/<registry-id>/` | Cached project-template registry (pinned by `.commit`) |
| `~/.claude/skills/<name>/SKILL.md` | Agent-host (Claude Code) skill probe — prompt-text-only trust |
| `~/.codex/skills/<name>/SKILL.md` | Agent-host (Codex) skill probe — prompt-text-only trust |

See also: [Configuration](configuration.md), [State Management](../concepts/state-management.md), [Project Setup](../getting-started/project-setup.md).
