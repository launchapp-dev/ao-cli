# Skill System

Animus skills are reusable prompt + runtime config bundles that compose into
phases at workflow execution time. This document specifies the source-chain
priority, the two-tier trust model that protects unvetted SKILL.md files, and
the `animus:` vendor namespace used to extend portable SKILL.md frontmatter
with Animus-specific runtime fields.

## Source chain

`load_skill_sources(project_root, user_config_dir)` returns sources in
**lowest-to-highest priority** order. `resolve_skill` iterates that slice in
reverse, so the first match wins. The chain is:

| Priority | Origin | Trust tier | Source |
| --- | --- | --- | --- |
| 1 (highest) | `Project` | High | `<project>/.animus/skills/`, `<project>/.animus/config/skill_definitions/` (v0.3 `.ao/skills/` is no longer scanned — see "Migrating from v0.3" below) |
| 2 | `User` | High | `~/.animus/skills/`, `~/.animus/config/skill_definitions/` |
| 3a | `Installed { registry, source, version, integrity, artifact }` | High | Pack `[skills]` contributions and registry-tracked installs from `animus skill install` |
| 3b | `Installed` (registry snapshots) | High | `~/.animus/<scope>/state/skills-registry.v1.json` |
| 4 | `Builtin` | High (legacy fallback) | `BUILTIN_SKILL_YAMLS` compiled into the binary |
| 5a | `AgentHost { host, scope: Project }` | **Prompt-text-only** | `<project>/.claude/skills/`, `<project>/.codex/skills/`, ... |
| 5b (lowest) | `AgentHost { host, scope: Global }` | **Prompt-text-only** | `~/.claude/skills/`, `~/.codex/skills/`, ... |

Trust-tier semantics are enforced at load time. See "Two-tier trust model" below.

### Migrating from v0.3 paths

v0.3 stored project skills under `<project>/.ao/skills/` and user skills under
`~/.ao/skills/`. v0.4's naming contract renamed these to `.animus/skills/`. The
resolver no longer scans the legacy `.ao/skills/` paths, but it does probe them
once per process and emits a single `warning:` line per legacy path when one
exists without its `.animus/skills/` replacement. To migrate cleanly:

```bash
animus skill migrate-from-ao         # both project + user scopes
animus skill migrate-from-ao --dry-run --project-only  # preview only
```

The migration moves every entry under `.ao/skills/` into `.animus/skills/`,
drops a `.migrated-from-ao` marker so the warning stops firing, and removes
the now-empty legacy directory. It refuses to clobber a non-empty
`.animus/skills/` that lacks the marker — operators must merge manually.

### Builtin fallback (v0.3 → v0.4)

The 19 bundled skill YAMLs (27 names counting alias re-exports) used to ship as
`include_str!`-compiled constants in `BUILTIN_SKILL_YAMLS`. v0.4 repackages
them as an installable pack called `animus.core-skills` that `animus init` and
`animus setup` auto-install. `BUILTIN_SKILL_YAMLS` stays in the binary as a
fallback so existing v0.3 projects continue to resolve every catalog skill
even before they re-run setup. A follow-up release will retire the fallback
once `animus.core-skills` is universal.

## Two-tier trust model

`SkillSourceOrigin` splits skills into two trust tiers:

### High trust (`Project`, `User`, `Installed`, `Builtin`)

These sources may contribute every field of `SkillDefinition`:

- `prompt.system`, `prompt.prefix`, `prompt.suffix`, `prompt.directives`
- `tool_policy` (`allow`/`deny` lists)
- `extra_args`, `env`, `mcp_servers`
- `adapters` (per-tool prompt and runtime overrides)
- `codex_config_overrides`, `capabilities`, `timeout_secs`, `model`, `tags`,
  `activation`

`Installed` skills carry an integrity snapshot (sha256 + artifact filename)
recorded at install time, so the high-trust grant is auditable and
reproducible. `Project`/`User` skills are checked into the project (or written
to the user's own `~/.animus/`), so they inherit the user's filesystem trust.

### Prompt-text-only (`AgentHost`)

SKILL.md files discovered under another agent host's skill directory
(`~/.claude/skills/`, `~/.codex/skills/`, `~/.cursor/skills/`,
`~/.config/opencode/skills/`, `~/.kiro/skills/`, `~/.slate/skills/`, plus
project-scoped variants where the host supports them) are PROMPT-TEXT-ONLY.

After parsing the markdown frontmatter, `strip_structural_fields_for_agent_host`
clears every runtime field on the resulting `SkillDefinition`:

```rust
definition.tool_policy = None;
definition.extra_args.clear();
definition.env.clear();
definition.mcp_servers.clear();
definition.adapters.clear();
definition.codex_config_overrides.clear();
definition.capabilities.clear();
definition.model = SkillModelPreference::default();
definition.timeout_secs = None;
```

This guarantees a malicious SKILL.md dropped under `~/.claude/skills/` cannot
silently widen Animus's tool surface, attach an MCP server, leak environment
variables, escalate codex permissions, force the runner onto a cheaper or
restricted model (`model`), or monopolize the runner with an arbitrarily
long timeout (`timeout_secs`). Only the prompt body, description, metadata,
and activation filters flow through.

A field-coverage test in `crates/orchestrator-config/src/skill_scoping.rs`
(`agent_host_strip_covers_every_runtime_field`) enumerates every serializable
field on `SkillDefinition` and fails the build if a new field is added
without an explicit allowlist / strip-list classification.

### Promoting an AgentHost skill to high trust

To use the structural fields of a SKILL.md discovered from another agent host,
the user must explicitly install it:

```bash
animus skill install --path ~/.claude/skills/my-favorite/
```

`animus skill install` converts it into a high-trust `Installed` entry with an
integrity snapshot. The user reads the SKILL.md once, decides whether the
declared `tool_policy` and MCP servers are acceptable, and pins the version.

### Agent host discovery table

| Host | Project-scoped | Global |
| --- | --- | --- |
| `claude-code` | `<project>/.claude/skills/` | `~/.claude/skills/` |
| `codex` | `<project>/.codex/skills/` | `~/.codex/skills/` |
| `opencode` | `<project>/.config/opencode/skills/` | `~/.config/opencode/skills/` |
| `cursor` | `<project>/.cursor/skills/` | `~/.cursor/skills/` |
| `kiro` | n/a | `~/.kiro/skills/` |
| `slate` | n/a | `~/.slate/skills/` |

Each child directory of a host root that contains a `SKILL.md` file is treated
as one skill. Direct `*.md` files at the root are also accepted (the filename
stem becomes the skill name when frontmatter omits `name`).

## Vendor namespace `animus:`

`MarkdownSkillFrontmatter` parses these fields by default:

- `name`, `description`, `version`, `metadata.version`

To configure Animus runtime behavior from a SKILL.md without breaking
portability to other agent hosts, place the structural fields under an
`animus:` namespace:

```yaml
---
name: my-skill
description: Custom skill for X
animus:
  tool_policy:
    allow: ["task.*"]
    deny: ["task.delete"]
  mcp_servers: ["context7"]
  model:
    preferred: claude-sonnet-4-6
  adapters:
    codex:
      prompt:
        system: "Codex-specific override"
  extra_args: ["--verbose"]
  env:
    REVIEW_MODE: strict
  capabilities:
    is_review: true
  codex_config_overrides: ["max_tokens=4096"]
  timeout_secs: 300
  tags: ["quality"]
---

# Skill body
The body becomes `prompt.system`.
```

Top-level placement of these fields is intentionally NOT parsed; other agent
hosts (Claude Code, Codex, Cursor) would complain about unknown keys at the
root of the frontmatter. Nesting under `animus:` keeps the file portable.

The `animus:` namespace is honored by every source that earns high trust. On
an `AgentHost` source the structural fields under `animus:` are still parsed
into the in-memory `SkillDefinition`, but `strip_structural_fields_for_agent_host`
clears them before the skill leaves `load_skill_sources`. The trust boundary
trumps the namespace shape.

## `animus.core-skills` pack

The bundled skill catalog ships as a pack at
`crates/orchestrator-config/config/bundled-packs/animus.core-skills/`:

```
animus.core-skills/
├── pack.toml         # [skills] section, no [workflows]
└── skills/
    ├── api-documentation.yaml
    ├── architecture-review.yaml
    ├── changelog-generation.yaml
    ├── ... (19 unique YAMLs)
    └── unit-testing.yaml
```

The pack manifest declares 8 aliases under `[skills.aliases]` so `testing`,
`queue-management`, `scheduling`, `risk-management`, `vision-alignment`,
`requirements-management`, `acceptance-criteria`, and `deliverable-validation`
all resolve to existing YAML files. The pack registry emits the 27-name set
to `load_skill_sources` as `SkillSourceOrigin::Installed { registry: "bundled", source: "animus.core-skills", version: "0.1.0", ... }`.

`animus init` and `animus setup` auto-install the pack via
`ensure_bundled_pack_installed("animus.core-skills")` and pin it in
`.animus/state/pack-selection.v1.json` so the resolver picks it up on the
next workflow run.

## Implementation map

- `crates/orchestrator-config/src/skill_definition.rs` — `SkillDefinition`,
  `SkillApplicationResult`, validation, application and merge logic.
- `crates/orchestrator-config/src/skill_scoping.rs` — source chain, agent-host
  discovery, trust enforcement, `animus:` namespace parsing.
- `crates/orchestrator-config/src/skill_resolution.rs` — `resolve_skill`,
  `resolve_skills_for_project`, `list_available_skills`.
- `crates/orchestrator-config/src/pack_config/types.rs` — `PackSkills` manifest
  shape and the optional `workflows`/`skills` invariant.
- `crates/orchestrator-config/config/bundled-packs/animus.core-skills/` — the
  bundled core-skills pack content.
