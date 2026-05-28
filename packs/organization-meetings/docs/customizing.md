# Customizing the Organization Meetings Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/prep-and-followup.yaml` and
`workflows/weekly-status.yaml` declares `model` and `tool`. Change them
and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  meeting-prep:
    model: gpt-4o
    tool: oai
  action-extractor:
    model: gpt-4o-mini
    tool: oai
  followup-drafter:
    model: gpt-4o
    tool: oai
  weekly-synthesizer:
    model: gpt-4o-mini
    tool: oai
  review-gate:
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

### Use Haiku for cheap phases, Sonnet for prose

Mixed-Claude is the default rationale: the review-gate is already on
Haiku. If you want to push more phases to Haiku for cost, the
`action-extractor` and `weekly-synthesizer` are the next best
candidates — both produce short structured output.

```bash
export ANTHROPIC_API_KEY=...
```

## Swap the meeting source

The pack uses `animus-subject-markdown` as a cheap, file-backed
substitute for a real calendar. The workflow YAML itself is
source-agnostic — it references the `meeting` subject kind without
caring where meetings come from.

### Google Calendar / Outlook / iCal (planned)

A calendar-aware subject backend would expose each event as a `meeting`
subject and let the principal pre-populate `prior_context` from prior
meetings with the same attendees. Migration shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-google-calendar
```

The plugin would register `subject_kinds: ["meeting"]`. The workflow
YAML does not change — only the plugin behind the `meeting` kind
changes.

If `meeting` is already claimed by `animus-subject-markdown`, you have
two options:

1. **Replace** — uninstall the markdown plugin, install the calendar
   plugin. The kind `meeting` now resolves to the calendar source.
2. **Coexist** — keep markdown registered as `meeting`, install the
   calendar plugin as `calendar.meeting`. Run two workflows side by
   side, or change the workflow's subject kind. (Subject kind is
   resolved at dispatch time via `animus subject` CLI flags or
   `default_subject_kind` in `.animus/config.json`.)

### Notion / Linear meeting templates

If your team already keeps meeting notes in Notion or Linear, a
subject backend wrapping the Notion API or Linear documents API is the
right approach. The protocol is the same as any other subject backend
(see "Roll your own" below) — it just needs to flatten Notion page
properties into the meeting subject shape (`title`, `attendees`,
`scheduled_at`, `type`, `prior_context`, `notes`).

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol).
Useful methods to implement: `meeting/list`, `meeting/get`,
`meeting/status`, `meeting/update`. See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language
that can speak JSON-RPC over stdio).

## Wire a calendar trigger

To auto-dispatch the `prep-and-followup` workflow N minutes before each
meeting, install a trigger plugin that watches the calendar:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-trigger-calendar
```

The trigger plugin would emit a `meeting.upcoming` event N minutes
before each event; the daemon's trigger supervisor dispatches the
workflow with the meeting subject id. The kill-switch
`ANIMUS_DAEMON_DISABLE_TRIGGERS=1` lets you opt out without
uninstalling.

For now (no trigger plugin), the principal can wire a cron equivalent:

```bash
*/15 * * * * animus subject list --kind meeting --json \
  | jq -r '.data.subjects[] | select(.scheduled_at >= "...")' \
  | xargs -I {} animus workflow run animus.organization-meetings/prep-and-followup \
      --subject-id "{}" --subject-kind meeting --project-root "$PROJECT_ROOT"
```

(This depends on the `--subject-id` CLI primitive landing — see the
"What doesn't work end-to-end yet" section of the README.)

## Add a "send via email" sink

This pack stops at draft + flag for review. Going further requires
either:

1. A custom command phase the principal triggers manually after editing
   the draft (e.g. `animus workflow run send-approved-followup
   --task-id <id>`), wired to your mail provider's send API.
2. An action plugin per mail sink (Gmail, Outlook, SES) that the
   `draft_followup` phase can call once the principal approves.

A minimal v1 of the manual-send pattern:

```yaml
# in a new workflow file: workflows/send-followup.yaml
phases:
  send_email:
    mode: command
    command:
      program: bash
      args:
        - "-c"
        - "msmtp --read-envelope-from -t < ${MAIL_DRAFT_PATH}"
      env:
        MAIL_DRAFT_PATH: "${ANIMUS_PHASE_INPUT_PATH}"
```

The phase reads the draft from the previous phase's output and shells
out to `msmtp` (or `mail`, or `sendgrid`). The principal would dispatch
this workflow only after editing the draft.

## Change the weekly rollup cadence

The default `weekly-status` workflow is a manual dispatch. To run it
automatically at the end of every week, add a schedule block:

```yaml
schedules:
  - id: friday-rollup
    cron: "0 17 * * FRI"   # Fridays at 5pm local
    workflow_ref: animus.organization-meetings/weekly-status
    enabled: true
```

You'll also want to widen or narrow the window. The default phase
directive looks back 7 days; edit the directive in
`weekly-status.yaml` to look back 5 weekdays, or 14 days for a
bi-weekly cadence.

## Change the review-gate behaviour

Right now `flag_for_review` sets the subject status to the normalized
`blocked` value and adds an `awaiting-human-review` label (the subject
protocol only accepts the normalized statuses `ready`, `in-progress`,
`blocked`, `done`, `cancelled`). To plug into your existing
notification stack:

- **Slack:** add a `mode: command` phase after `flag_for_review` that
  posts the draft followup to a private channel via a Slack incoming
  webhook. The phase can read the previous phase's output via the
  dispatch envelope.
- **Email-to-self:** same pattern with `msmtp` / `mail` posting to the
  principal's own inbox.
- **Linear:** install the Linear subject backend and create a
  follow-up issue with the draft attached.
