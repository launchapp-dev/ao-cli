# Architecture: Sales Pipeline Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + a subject backend
that knows how to read leads.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads leads as markdown files from a directory. Routes the `lead` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the enricher / qualifier / discovery-writer / recommender / handoff agents. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets the rep browse qualification verdicts in a web UI instead of `animus output tail`. |

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

## How a lead flows

```
+-------------------+          +-----------------------+
| leads/inbox/      |          | animus-subject-       |
|   *.md            |  ---->   | markdown (plugin)     |
+-------------------+          | kind=lead             |
                               +-----------+-----------+
                                           |
                                           |  list / get / status
                                           v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.sales-pipeline/qualify-lead             |
|     (per lead subject)                                        |
|                                                              |
|   phase: lead_enrich          ---> JSON { firmographics, fit,     |
|                                       signals }              |
|         agent: Sonnet (research + structured output)         |
|                                                              |
|   phase: qualify         ---> JSON { bant, qualified, ... }  |
|         agent: Sonnet (judgment-heavy, calibrated)           |
|                                                              |
|   phase: draft_discovery ---> JSON { questions[] }           |
|         agent: Sonnet (tailored to enrichment)               |
|                                                              |
|   phase: recommend_next  ---> JSON { recommended_next,       |
|                                       owner_hint, urgency }  |
|         agent: Haiku (cheap, deterministic policy)           |
|                                                              |
|   phase: lead_flag_for_review ---> subject status update          |
|         agent: Haiku, mutates_state=true                     |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Sales rep reads      |
            | qualification +      |
            | discovery plan, then |
            | actions in their CRM |
            | / email / phone tool |
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

Sales pipelines are the second canonical "BYO data source" workflow after
customer support:

1. **Subject backend is non-engineering.** A lead is not code, not a task,
   not a requirement — it's a record about an account and a person. Animus
   treats it identically to every other subject kind.
2. **Phases are LLM-only.** No `mode: command`, no shell calls, no git ops.
   The runner just sequences LLM calls and captures structured outputs.
3. **The rep is the loop closer.** Animus stops at `lead_flag_for_review`. It
   does not auto-send outreach. The honest constraint: every recommended
   action requires a human to actually do it.

This pattern (subject backend + LLM-only workflow + human handoff)
generalizes to recruiting screens, contract review, expense approvals,
content moderation, and dozens of other agency-style use cases.
