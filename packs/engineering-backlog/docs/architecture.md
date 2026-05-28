# Architecture: Engineering Backlog Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + plugins Animus
already ships.

This is **the** canonical reference for the original Animus use case: an
autonomous engineer working off a task backlog. Everything else in this
pack family is a generalization of this pattern.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-default`](https://github.com/launchapp-dev/animus-subject-default) | Provides the `task` subject kind via file-backed markdown/JSON storage. Same backend Animus uses to manage its own backlog. |
| Provider (impl/plan) | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) | Runs Claude Haiku (research, finalize) and Claude Sonnet (plan, implement). |
| Provider (review) | [`launchapp-dev/animus-provider-codex`](https://github.com/launchapp-dev/animus-provider-codex) | Runs Codex for cross-model diff review. Independent of the implementer catches blind spots a single model misses. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets you watch runs in a web UI instead of `animus output tail`. |

Install everything in one go via `scripts/setup.sh`. Manual install:

```bash
animus plugin install launchapp-dev/animus-subject-default
animus plugin install launchapp-dev/animus-provider-claude --allow-shadow-builtin
animus plugin install launchapp-dev/animus-provider-codex --allow-shadow-builtin
```

## How a task flows

```
+-------------------+          +-----------------------+
| tasks/backlog/    |          | animus-subject-       |
|   TASK-*.md       |  ---->   | default (plugin)      |
+-------------------+          | kind=task             |
                               +-----------+-----------+
                                           |
                                           |  list / get / status / labels
                                           v
+----------------------------------------------------------------------+
| Animus daemon                                                        |
|                                                                      |
|   workflow run animus.engineering-backlog/implement-task             |
|     --task-id TASK-XXXX                                              |
|                                                                      |
|   phase: research      ---> JSON { relevant_files, sketch, risks }   |
|         agent: Haiku, read-only (grep/rg/find)                       |
|                                                                      |
|   phase: plan          ---> Markdown plan (files + line refs + steps)|
|         agent: Sonnet, read-only                                     |
|                                                                      |
|   phase: implement     ---> commits in a git worktree                |
|         agent: Sonnet, mutates_state=true                            |
|                                                                      |
|   phase: review        ---> JSON { verdict, findings[] }             |
|         agent: Codex (cross-model), read-only                        |
|                                                                      |
|   phase: test          ---> exit_code from `cargo test --workspace`  |
|         mode: command, deterministic gate                            |
|                                                                      |
|   phase: finalize      ---> push branch + open PR, OR                |
|         agent: Haiku        ---> mark task blocked + label           |
|                                                                      |
+----------------------+-----------------------------------------------+
                       |
                       v
            +----------------------+
            | Human reviews PR.    |
            | Animus does NOT      |
            | auto-merge.          |
            +----------------------+
```

## Where outputs land

Standard Animus paths — nothing pack-specific:

- Run events / artifacts: `~/.animus/<repo-scope>/runs/<run-id>/`
- Per-phase JSON output: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/<phase-id>.json`
- Workflow snapshots: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/`

Stream in real time with `animus output monitor --run-id <run-id>`. Pull a
structured per-phase snapshot with `animus output phase-outputs
--workflow-id <workflow-id>`.

## Why this pack matters architecturally

This pack is the canonical "Animus eating its own dog food" — the workflow
shape is the same one we use internally to drive the ao-cli backlog (see
`.animus/workflows/common.yaml`, `.animus/workflows/review.yaml`). The
pack format makes it portable to any codebase.

1. **Cross-model review is a real gate.** Claude implements, Codex
   reviews. The two models catch different things — the same model
   reviewing its own work is largely theater. This is the same procedure
   the ao-cli repo's CLAUDE.md requires of sub-agents before commit.
2. **The implement phase is the only one that mutates code.** Every
   other phase is read-only — research, plan, and review never touch
   the working tree. Test is a deterministic gate (no LLM). Finalize
   only mutates remote state (push, gh pr create, subject label).
3. **Stop at human PR review.** Animus does not auto-merge by default.
   The PR is the contract surface between the autonomous worker and the
   human reviewer.

This pattern (research → plan → implement → cross-model review → test → PR)
is the canonical engineering-agent loop. If you're adopting Animus for
software work, this is the workflow shape you'll iterate on.

## What's different from the customer-support pack

The customer-support pack proves "subject backend + LLM-only workflow +
human handoff" generalizes to non-engineering domains. This pack is the
**reverse direction**: it uses the full Animus primitive stack — shell
commands, git ops, deterministic test gates, cross-model review,
mutating subject ops — as the canonical "what Animus is for" reference.

If you only read one pack to understand the platform: read this one.
