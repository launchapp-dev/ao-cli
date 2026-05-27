# Customizing the Customer Support Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/triage-ticket.yaml` declares `model` and `tool`.
Change them and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  ticket-classifier:
    model: gpt-4o-mini
    tool: oai
  response-drafter:
    model: gpt-4o
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

### Use Gemini for cheap phases, Claude for drafting

Mixed-provider workflows are fine — agents are independent. Set the env vars
for every provider you reference.

```bash
export GEMINI_API_KEY=...
export ANTHROPIC_API_KEY=...
```

## Change the categories or urgency rubric

Edit the system prompt of `ticket-classifier` in
`workflows/triage-ticket.yaml`. Keep the `output_contract.required_fields`
list in sync — if you add a `severity` field, declare it in the contract so
downstream phases can rely on it being present.

Example: adding a "sentiment" field.

```yaml
output_contract:
  kind: phase_result
  required_fields:
    - category
    - urgency
    - sentiment
  fields:
    ...
    sentiment:
      type: string
      description: One of frustrated, neutral, appreciative.
```

## Adjust the tone guide

Update the `response-drafter` system prompt. The category list in the
classifier's prompt MUST match the categories the drafter knows tone for,
otherwise you'll get generic "supportive" replies for unknown categories.

## Swap the ticket source

The pack uses `animus-subject-markdown` as a cheap, file-backed substitute
for a real helpdesk. The workflow YAML itself is source-agnostic — it
references the `ticket` subject kind without caring where tickets come from.

### Zendesk / Intercom / Front / Help Scout (planned)

These backends are not yet shipped as plugins. The intended migration shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-zendesk
```

The plugin would register `subject_kinds: ["ticket"]` (or `zendesk.ticket`
if you want to run multiple sources side by side). The workflow YAML does
not change — only the plugin behind the `ticket` kind changes.

If `ticket` is already claimed by `animus-subject-markdown`, you have two
options:

1. **Replace** — uninstall the markdown plugin, install the Zendesk plugin.
   The kind `ticket` now resolves to Zendesk.
2. **Coexist** — keep markdown registered as `ticket`, install Zendesk as
   `zendesk.ticket`. Run two workflows side by side, or change the
   workflow's subject kind. (Subject kind is resolved at dispatch time
   via `animus subject` CLI flags or `default_subject_kind` in
   `.animus/config.json`.)

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol). Useful
methods to implement: `ticket/list`, `ticket/get`, `ticket/status`. See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language that
can speak JSON-RPC over stdio).

## Change the human-review gate

Right now `flag_for_review` sets the subject status to the normalized
`blocked` value and adds an `awaiting-human-review` label (the subject
protocol only accepts the normalized statuses `ready`, `in-progress`,
`blocked`, `done`, `cancelled`). To plug into your existing notification
stack:

- **Slack:** add a `mode: command` phase after `flag_for_review` that calls
  `curl` to a Slack incoming webhook. The phase can read the previous
  phase's output via the dispatch envelope.
- **Email:** same pattern with `msmtp` / `mail` / `sendgrid` CLI.
- **Linear/Jira:** install the matching subject backend and create a
  follow-up subject with the draft attached.

## Add a "send-on-approve" step (future work)

This pack stops at draft + flag. Going further requires either:

1. A custom command phase the reviewer triggers manually after editing the
   draft (e.g. `animus workflow run send-approved-reply --task-id <id>`),
   wired to your support tool's send-message API.
2. A trigger plugin that watches for human approval (Slack reaction, web UI
   button click) and dispatches the send.

Both are out of scope for v1 of this pack. If you build either, please
contribute back.

## Wire up a schedule

To triage automatically as tickets arrive, add a schedule block to the
workflow file:

```yaml
schedules:
  - id: triage-new-tickets
    cron: "*/5 * * * *"   # every 5 minutes
    workflow_ref: animus.customer-support/triage-ticket
    enabled: true
```

You'll also want a dispatcher agent that scans for tickets with status
`open` and enqueues one triage workflow per ticket (the
`requirements`/`req-dispatch` pattern in
[`.animus/workflows/requirements.yaml`](../../../.animus/workflows/requirements.yaml)
is the reference implementation).
