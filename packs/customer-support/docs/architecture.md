# Architecture: Customer Support Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + a subject backend
that knows how to read tickets.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads tickets as markdown files from a directory. Routes the `ticket` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the classifier / drafter / handoff agents. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets the human reviewer browse drafts in a web UI instead of `animus output tail`. |

Install all of the above in one go:

```bash
animus plugin install-defaults --include-subjects --include-transports
animus plugin install launchapp-dev/animus-subject-markdown
# Provider plugins claim reserved tool names (claude, oai, gemini),
# so installation needs --allow-shadow-builtin to acknowledge the
# shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-claude --allow-shadow-builtin
```

(`scripts/setup.sh` runs the install commands for you.)

## How a ticket flows

```
+-------------------+          +-----------------------+
| tickets/inbox/    |          | animus-subject-       |
|   *.md            |  ---->   | markdown (plugin)     |
+-------------------+          | kind=ticket           |
                               +-----------+-----------+
                                           |
                                           |  list / get / status
                                           v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.customer-support/triage-ticket          |
|     (per ticket subject)                                      |
|                                                              |
|   phase: classify     ---> JSON { category, urgency }        |
|         agent: Haiku (cheap, structured output)              |
|                                                              |
|   phase: draft_response ---> JSON { draft_reply, ... }       |
|         agent: Sonnet (richer prose, tone-aware)             |
|                                                              |
|   phase: summarize_for_human ---> JSON { tldr, edits, ... }  |
|         agent: Haiku (short brief)                           |
|                                                              |
|   phase: flag_for_review ---> subject status update          |
|         agent: Haiku, mutates_state=true                     |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Human reviewer reads |
            | output, edits draft, |
            | sends from their     |
            | support tool         |
            +----------------------+
```

## Where outputs land

Standard Animus paths — nothing pack-specific:

- Run events / artifacts: `~/.animus/<repo-scope>/runs/<run-id>/`
- Per-phase JSON output: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/<phase-id>.json`
- Workflow snapshots: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/`

Stream in real time with `animus output monitor --run-id <run-id>`. Pull a
structured per-phase snapshot with `animus output phase-outputs --workflow-id
<workflow-id>`.

## Why this pack matters architecturally

This is the first Animus workflow that proves the "BYO data source" framing
end-to-end:

1. **Subject backend is non-engineering.** Markdown files (or, later, Zendesk)
   are not code, tasks, or requirements — they're just records. Animus treats
   them identically to every other subject kind.
2. **Phases are LLM-only.** No `mode: command`, no shell calls, no git ops.
   The runner is just sequencing LLM calls and capturing structured outputs.
3. **The reviewer is the loop closer.** Animus stops at `flag_for_review`.
   It does not pretend to be a full support helpdesk — it's the autonomous
   tier-1 layer in front of the human.

This pattern (subject backend + LLM-only workflow + human handoff) generalizes
to expense approvals, contract review, content moderation, hiring screen, and
dozens of other agency-style use cases.
