# Customizing the Sales Pipeline Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/qualify-lead.yaml` declares `model` and `tool`.
Change them and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  lead-enricher:
    model: gpt-4o
    tool: oai
  lead-qualifier:
    model: gpt-4o
    tool: oai
  discovery-writer:
    model: gpt-4o
    tool: oai
  next-action-recommender:
    model: gpt-4o-mini
    tool: oai
  rep-handoff:
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

### Use Gemini for cheap phases, Claude for reasoning

Mixed-provider workflows are fine — agents are independent. Set the env vars
for every provider you reference.

```bash
export GEMINI_API_KEY=...
export ANTHROPIC_API_KEY=...
```

## Swap the qualification framework (BANT → MEDDIC / CHAMP / GPCT)

BANT is the default because it's the shortest framework to score from a
single inbound message. Swap by editing the `lead-qualifier` agent.

### MEDDIC

```yaml
lead-qualifier:
  system_prompt: |
    You score one lead at a time using MEDDIC:
      - Metrics            — what KPI does the buyer move?
      - Economic Buyer     — who signs the check?
      - Decision Criteria  — what shortlist criteria are in play?
      - Decision Process   — how does the company buy software?
      - Identify Pain      — what's the cost of the status quo?
      - Champion           — is there an internal advocate?

    For each dimension return { score: strong|moderate|weak|unknown,
    evidence: "..." }. Mark qualified=true only if Economic Buyer is
    moderate-or-stronger AND Identify Pain is moderate-or-stronger.
```

Keep the `output_contract` in sync. Rename `bant` to `meddic` and update
`fields:` so downstream phases (recommend_next) read the right key — or
keep the top-level field name as `bant` for forward-compat and just
change the dimension names inside.

### CHAMP / GPCT

Same pattern. Replace the dimension list and the qualification rule.
Re-read `draft_discovery` and `recommend_next` prompts to make sure they
don't hardcode "BANT" — current prompts reference the qualification
verdict generically, so they survive framework swaps.

## Adjust the recommended-next action set

Edit the `next-action-recommender` system prompt. The default set is
`book-demo | send-case-study | nurture | disqualify`. To add a `route-to-channel`
or `gated-by-procurement` branch, list it in the prompt AND in the field
description under `recommend_next.output_contract.fields.recommended_next`.

If you add new outputs (e.g. a `proposed_slot` for booking), declare them in
the contract so the `flag_for_review` phase or any downstream automation can
rely on them.

## Swap the lead source

The pack uses `animus-subject-markdown` as a cheap, file-backed substitute
for a real CRM. The workflow YAML itself is source-agnostic — it references
the `lead` subject kind without caring where leads come from.

### Salesforce / HubSpot / Pipedrive (planned)

These backends are not yet shipped as plugins. The intended migration shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-salesforce
```

The plugin would register `subject_kinds: ["lead"]` (or `salesforce.lead`
if you want to run multiple sources side by side). The workflow YAML does
not change — only the plugin behind the `lead` kind changes.

If `lead` is already claimed by `animus-subject-markdown`, you have two
options:

1. **Replace** — uninstall the markdown plugin, install the Salesforce plugin.
   The kind `lead` now resolves to Salesforce.
2. **Coexist** — keep markdown registered as `lead`, install Salesforce as
   `salesforce.lead`. Run two workflows side by side, or change the
   workflow's subject kind. (Subject kind is resolved at dispatch time
   via `animus subject` CLI flags or `default_subject_kind` in
   `.animus/config.json`.)

### Linear (using the existing Linear subject backend)

If your inbound leads land in Linear (some teams use Linear as a lightweight
intake board), install `launchapp-dev/animus-subject-linear` and register
the `lead` kind to a Linear team. The workflow YAML doesn't change.

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol). Useful
methods to implement: `lead/list`, `lead/get`, `lead/status`. See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language that
can speak JSON-RPC over stdio).

## Wire to email send via trigger plugin (future work)

This pack stops at draft + flag. Going further to "send the rep an email
when a hot lead lands" requires either:

1. A custom `mode: command` phase after `flag_for_review` that calls
   your transactional email API (SendGrid, Postmark, SES) with the
   qualification verdict + discovery plan in the body.
2. A trigger plugin (the same shape as `launchapp-dev/animus-trigger-slack`)
   that watches for `workflow_completed` events on the `qualify-lead`
   workflow and posts to Slack / email / a CRM webhook.

Both are out of scope for v1. The honest constraint: this pack does NOT
auto-send outreach to the lead. Every recommended action requires a human
to actually do it.

## Wire to a real CRM for status writeback

The `flag_for_review` phase currently sets the subject status to the
normalized `blocked` value (the subject protocol only accepts the
normalized statuses `ready`, `in-progress`, `blocked`, `done`,
`cancelled`) and adds an `awaiting-rep-review` label.

To plug into a real CRM's lead stages (Salesforce / HubSpot / Pipedrive),
either:

- **Use a CRM subject backend.** When the backend handles `lead/status`,
  it can translate `blocked` to whatever CRM stage matches your workflow
  (e.g. "MQL", "Awaiting Rep").
- **Add a command phase after `flag_for_review`** that calls the CRM's
  REST API with the BANT verdict + recommended action to push a lead
  status / activity record.

## Change the human-review gate

Right now the rep reads `animus output phase-outputs --workflow-id <id>`
and decides what to do. To plug into your existing notification stack:

- **Slack:** add a `mode: command` phase after `flag_for_review` that
  calls `curl` to a Slack incoming webhook. The phase can read previous
  phase outputs via the dispatch envelope.
- **Email digest:** schedule a separate `mode: command` workflow that
  runs every morning, lists leads with `awaiting-rep-review`, and emails
  the rep a summary.
- **Linear / Jira ticket per hot lead:** install the matching subject
  backend and create a follow-up subject with the qualification verdict
  attached.

## Wire up a schedule

To qualify leads automatically as they arrive, add a schedule block to
the workflow file:

```yaml
schedules:
  - id: qualify-new-leads
    cron: "*/5 * * * *"   # every 5 minutes
    workflow_ref: animus.sales-pipeline/qualify-lead
    enabled: true
```

You'll also want a dispatcher agent that scans for leads with stage
`inbound` and enqueues one qualification workflow per lead (the
`requirements`/`req-dispatch` pattern in
[`.animus/workflows/requirements.yaml`](../../../.animus/workflows/requirements.yaml)
is the reference implementation).
