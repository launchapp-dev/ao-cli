# Architecture: Marketing Outreach Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + a subject backend
that knows how to read prospects.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads prospects as markdown files from a directory. Routes the `prospect` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the enrichment / drafter / cadence / handoff agents. |
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

## How a prospect flows

```
+-------------------+          +-----------------------+
| prospects/inbox/  |          | animus-subject-       |
|   *.md            |  ---->   | markdown (plugin)     |
+-------------------+          | kind=prospect         |
                               +-----------+-----------+
                                           |
                                           |  list / get / status
                                           v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.marketing-outreach/triage-prospect      |
|     (per prospect subject)                                    |
|                                                              |
|   phase: enrich          ---> JSON { company_summary,        |
|                                       recent_signals[],      |
|                                       best_angle }           |
|         agent: Sonnet (research discipline, no fabrication)  |
|                                                              |
|   phase: draft_outreach  ---> JSON { subject_line,           |
|                                       draft_message,         |
|                                       assumptions[] }        |
|         agent: Sonnet (tight 3-line copy, channel-aware tone)|
|                                                              |
|   phase: suggest_cadence ---> JSON { touches[],              |
|                                       stop_conditions[] }    |
|         agent: Haiku (cheap planner, 2-4 touches max)        |
|                                                              |
|   phase: flag_for_review ---> subject status update          |
|         agent: Haiku, mutates_state=true                     |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Human reviewer reads |
            | draft, edits if      |
            | needed, sends from   |
            | email / LinkedIn /   |
            | CRM of choice        |
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

This is the second non-coding reference pack (after `customer-support`),
and it generalizes the "BYO data source + LLM-only workflow + human gate"
pattern to outbound marketing:

1. **Subject backend is non-engineering.** Markdown files (or, later,
   HubSpot / Salesforce / Apollo) are not code, tasks, or requirements —
   they're just records. Animus treats them identically to every other
   subject kind.
2. **Phases are LLM-only.** No `mode: command`, no shell calls, no git
   ops. The runner is just sequencing LLM calls and capturing
   structured outputs. The `flag_for_review` phase touches subject
   state, nothing else.
3. **The reviewer is the loop closer.** Animus stops at
   `flag_for_review`. It does NOT send the email or the LinkedIn DM —
   it's the autonomous drafting layer in front of the human.

This pattern (subject backend + LLM-only workflow + human handoff) keeps
the model honest: every outbound message a customer sees was approved
by a person, but the SDR or marketer didn't have to write it from
scratch.
