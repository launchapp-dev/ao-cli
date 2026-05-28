# Customizing the Marketing Outreach Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/triage-prospect.yaml` declares `model` and
`tool`. Change them and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  prospect-enricher:
    model: gpt-4o
    tool: oai
  outreach-drafter:
    model: gpt-4o
    tool: oai
  cadence-planner:
    model: gpt-4o-mini
    tool: oai
  human-handoff:
    model: gpt-4o-mini
    tool: oai
```

Then:

```bash
# Provider plugins claim reserved tool names (oai → oai-runner,
# gemini → gemini, etc.) so the installer requires
# --allow-shadow-builtin to acknowledge the shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-oai --allow-shadow-builtin
export OPENAI_API_KEY=sk-...
animus daemon start --auto-install   # daemon picks up the env var
```

### Mix providers: Sonnet for drafting, Gemini for cadence

Mixed-provider workflows are fine — agents are independent. Set the env
vars for every provider you reference.

```bash
export GEMINI_API_KEY=...
export ANTHROPIC_API_KEY=...
```

## Change the angles or source channels

Edit the system prompt of `prospect-enricher` to widen or tighten the
"best angle" list (e.g. add "regulatory-deadline-coming" if you sell
compliance tooling, drop "competitive-displacement" if your category
doesn't have established competitors).

Then update `outreach-drafter`'s tone-by-source-channel table to match.
The two agent prompts MUST agree on the set of source channels and
angles — if the enricher emits an angle the drafter doesn't have tone
guidance for, you get generic copy.

If you add a new field (e.g. `industry_vertical`), declare it in the
`enrich` phase's `output_contract.required_fields` so downstream phases
can rely on it being present:

```yaml
output_contract:
  kind: phase_result
  required_fields:
    - company_summary
    - best_angle
    - industry_vertical
  fields:
    ...
    industry_vertical:
      type: string
      description: One of saas, fintech, healthcare, devtools, ecommerce, other.
```

## Swap the prospect source

The pack uses `animus-subject-markdown` as a cheap, file-backed
substitute for a real CRM. The workflow YAML itself is source-agnostic
— it references the `prospect` subject kind without caring where
prospects come from.

### HubSpot / Salesforce / Apollo / Clay (planned)

These backends are not yet shipped as plugins. The intended migration
shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-hubspot
```

The plugin would register `subject_kinds: ["prospect"]` (or
`hubspot.contact` if you want to run multiple sources side by side).
The workflow YAML does not change — only the plugin behind the
`prospect` kind changes.

If `prospect` is already claimed by `animus-subject-markdown`, you have
two options:

1. **Replace** — uninstall the markdown plugin, install the HubSpot
   plugin. The kind `prospect` now resolves to HubSpot.
2. **Coexist** — keep markdown registered as `prospect`, install
   HubSpot as `hubspot.prospect`. Run two workflows side by side, or
   change the workflow's subject kind. (Subject kind is resolved at
   dispatch time via `animus subject` CLI flags or
   `default_subject_kind` in `.animus/config.json`.)

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol).
Useful methods to implement: `prospect/list`, `prospect/get`,
`prospect/status`, `prospect/update` (for labels). See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language
that can speak JSON-RPC over stdio).

## Wire to a real send channel

This pack stops at draft + cadence + flag. The reviewer manually copies
the draft into their email tool / LinkedIn / Apollo / Outreach sequence.
Wiring an actual send step requires a separate action plugin or command
phase.

### Email (Sendgrid / Postmark / SES)

Add a `mode: command` phase after `prospect_flag_for_review` that calls the
provider's CLI or curl-able API. Guard it on a confirmation field so the
draft is never auto-sent — the human still gates the send via a
`confirmed_for_send` flag the reviewer flips on the subject.

### LinkedIn

There is no honest "send a LinkedIn DM via API" path for most accounts.
The reviewer flow is the right answer here: Animus drafts, the human
sends in-app. A future trigger plugin could watch for an "approved"
label and surface the draft in a browser extension queue.

### CRM sequencers (Outreach.io / Salesloft / Apollo)

These platforms accept template imports via API. A future
`animus-trigger-outreach-export` plugin could push the
`draft_message` + cadence plan as a new sequence the SDR enrolls the
prospect in. Until that ships, the reviewer copies the draft + cadence
manually.

## Track replies via a webhook trigger

If you wire up an email tool that reports reply events via webhook, you
can flip the prospect's status back to `ready` (or `done`) by adding a
webhook trigger plugin:

```bash
animus plugin install launchapp-dev/animus-trigger-webhook
```

Then configure a trigger in the workflow YAML that watches for a
`prospect.replied` event and dispatches a follow-up workflow (e.g.
`animus.marketing-outreach/handle-reply`, not shipped in v1).

## Change the human-review gate

Right now `prospect_flag_for_review` sets the subject status to the normalized
`blocked` value and adds an `awaiting-human-review` label (the subject
protocol only accepts the normalized statuses `ready`, `in-progress`,
`blocked`, `done`, `cancelled`). To plug into your existing
notification stack:

- **Slack:** add a `mode: command` phase after `prospect_flag_for_review` that
  calls `curl` to a Slack incoming webhook. The phase can read the
  previous phase's output via the dispatch envelope.
- **Email digest:** same pattern with `msmtp` / `mail` / `sendgrid` CLI;
  batch multiple prospects into a daily digest.
- **CRM:** install the matching subject backend and write the draft +
  cadence back as a CRM note on the prospect record.

## Wire up a schedule

To triage automatically as prospects arrive, add a schedule block to
the workflow file:

```yaml
schedules:
  - id: triage-new-prospects
    cron: "*/15 * * * *"   # every 15 minutes
    workflow_ref: animus.marketing-outreach/triage-prospect
    enabled: true
```

You'll also want a dispatcher agent that scans for prospects with
status `open` and enqueues one triage workflow per prospect (the
`requirements`/`req-dispatch` pattern in
[`.animus/workflows/requirements.yaml`](../../../.animus/workflows/requirements.yaml)
is the reference implementation).

## Tune the cadence aggressiveness

The `cadence-planner` agent currently recommends 2-4 touches and stops
on any reply. If your team prefers tighter cadences (e.g. 5-7 touches
over 21 days) or looser ones (1-2 polite nudges), edit the system
prompt's "Constraints" block. Keep `stop_conditions` honest — adding
"out-of-office bounce" and "unsubscribe request" is good hygiene.
