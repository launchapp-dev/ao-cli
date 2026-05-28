# Architecture: Organization Meetings Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + a subject backend
that knows how to read meetings.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads meetings as markdown files from a directory. Routes the `meeting` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the prep / extract / followup / synthesis agents. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets the principal browse drafts in a web UI instead of `animus output tail`. |
| Trigger (optional) | A calendar trigger plugin (Google Calendar / Outlook / iCal) | Auto-dispatch the `prep-and-followup` workflow N minutes before each meeting. Out-of-scope for v1 — see `docs/customizing.md`. |

Install the defaults in one go:

```bash
animus plugin install-defaults --include-subjects --include-transports
animus plugin install launchapp-dev/animus-subject-markdown
# Provider plugins claim reserved tool names (claude, oai, gemini),
# so installation needs --allow-shadow-builtin to acknowledge the
# shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-claude --allow-shadow-builtin
```

(`scripts/setup.sh` runs the install commands for you.)

## How a meeting flows

```
+-------------------+          +-----------------------+
| meetings/inbox/   |          | animus-subject-       |
|   MTG-*.md        |  ---->   | markdown (plugin)     |
+-------------------+          | kind=meeting          |
                               +-----------+-----------+
                                           |
                                           |  list / get / status / update
                                           v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.organization-meetings/prep-and-followup |
|     (per meeting subject)                                     |
|                                                              |
|   phase: pre_brief         ---> JSON { objectives, ... }     |
|         agent: Sonnet (1-page prep)                          |
|                                                              |
|   phase: extract_actions   ---> JSON { actions: [...] }      |
|         agent: Sonnet (structured extraction)                |
|                                                              |
|   phase: draft_followup    ---> JSON { subject, body, ... }  |
|         agent: Sonnet (email draft)                          |
|                                                              |
|   phase: synthesize_weekly ---> JSON { decisions, ... }      |
|         agent: Sonnet (3-bullet weekly entry)                |
|                                                              |
|   phase: meeting_flag_for_review   ---> subject status=blocked       |
|         agent: Haiku, mutates_state=true                     |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Principal reads brief|
            | before meeting,      |
            | reviews followup     |
            | after, sends from    |
            | their own inbox      |
            +----------------------+
```

The `weekly-status` workflow is a separate, single-phase rollup that
aggregates the per-meeting `synthesize_weekly` contributions from the
past 7 days:

```
+--------------------------------------------------+
| Animus daemon                                    |
|                                                  |
| workflow run animus.organization-meetings/       |
|              weekly-status                       |
|                                                  |
|   phase: aggregate_weekly_status                 |
|     reads meetings where scheduled_at >= now-7d  |
|     reads each meeting's prior weekly_entry      |
|     emits { decisions, blockers, themes,         |
|             quiet_areas, skipped_meetings }      |
|                                                  |
+--------------------------------------------------+
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

This is the meeting-ops counterpart to the customer-support pack — same
shape, different subject kind:

1. **Subject backend is non-engineering.** Markdown files (or, later,
   Google Calendar) are not code, tasks, or requirements — they're just
   meeting records. Animus treats them identically to every other
   subject kind.
2. **Phases are LLM-only.** No `mode: command`, no shell calls, no git
   ops. The runner is just sequencing LLM calls and capturing structured
   outputs.
3. **The principal is the loop closer.** Animus stops at
   `meeting_flag_for_review`. It does not send email, post to Slack, or write to
   any external system — the principal reviews and acts in their own
   tool.
4. **Per-meeting work feeds a weekly rollup.** The per-meeting
   `synthesize_weekly` phase produces a 3-bullet entry that the standalone
   `weekly-status` workflow aggregates. This is the "small contributions
   add up over the week" pattern — the principal never has to write the
   weekly status from scratch.

This pattern (subject backend + LLM-only workflow + human review gate +
periodic rollup) generalizes to standups, board updates, customer health
reviews, and any other ops cadence where small recurring inputs need to
land in a periodic synthesis.
