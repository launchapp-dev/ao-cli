# Customizing the Recruiting Pipeline Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/screen-candidate.yaml` and
`workflows/debrief-synthesis.yaml` declares `model` and `tool`. Change
them and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  candidate-enricher:
    model: gpt-4o
    tool: oai
  rubric-screener:
    model: gpt-4o
    tool: oai
  interview-brief-writer:
    model: gpt-4o
    tool: oai
  recruiter-handoff:
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

### Mix providers per phase

Recruiting copy benefits from a stronger model, but the handoff phase is
mostly routing — feel free to cost-tune.

```bash
export GEMINI_API_KEY=...
export ANTHROPIC_API_KEY=...
```

```yaml
agents:
  rubric-screener:
    model: claude-sonnet-4-6
    tool: claude
  recruiter-handoff:
    model: gemini-1.5-flash
    tool: gemini
```

## Change the rubric

Edit the `rubric-screener` system prompt in
`workflows/screen-candidate.yaml` and keep the `output_contract` in sync.

Example: replace the default 4-dimension rubric with a role-specific 6-dimension
rubric for senior engineering candidates.

```yaml
output_contract:
  kind: phase_result
  required_fields:
    - technical_depth
    - system_design
    - communication
    - leadership_signals
    - culture_fit_signals
    - risk_flags
  fields:
    technical_depth:
      type: integer
      description: 0-5 score for hands-on technical depth.
    system_design:
      type: integer
      description: 0-5 score for system-design / architecture signal.
    communication:
      type: integer
      description: 0-5 score for written / spoken communication quality.
    leadership_signals:
      type: integer
      description: 0-5 score for mentorship, tech leadership, scope.
    culture_fit_signals:
      type: integer
      description: 0-5 score for evidence of alignment with stated company values.
    risk_flags:
      type: array
      description: Short list of yellow/red flags worth surfacing.
      items:
        type: string
```

NOTE: `culture_fit_signals` is a loaded dimension. Several teams have
deliberately dropped "culture fit" from their rubric in favor of
behavioral / values-alignment dimensions specifically because "culture
fit" rationalizes pattern matching to the existing team. Pick what
matches your hiring philosophy.

## Adjust the recommended_next routing

The `interview-brief-writer` emits `recommended_next` with a fixed enum
(proceed-to-screen, proceed-to-tech, proceed-to-onsite,
hold-for-recruiter-review, pass). To match your loop, edit both the
agent's system prompt AND the `recommended_next` field description in
the `output_contract`.

Example: a 5-stage loop with a take-home phase.

```yaml
recommended_next:
  type: string
  description: One of proceed-to-recruiter-screen, proceed-to-take-home, proceed-to-tech-screen, proceed-to-onsite, hold-for-recruiter-review, pass.
```

## Adjust the debrief recommendation enum

The `debrief-synthesizer` emits `recommendation` with the standard
7-value Google-style enum plus `no-decision`. Some teams use a 3-value
enum (hire / no-decision / no-hire) for less false precision; some use a
5-value enum dropping `lean-*`.

Edit the `recommendation` field description in the
`debrief-synthesis.yaml` output_contract AND the matching enum list in
the agent's system prompt.

## Swap the candidate source

The pack uses `animus-subject-markdown` as a cheap, file-backed substitute
for a real ATS. The workflow YAML itself is source-agnostic — it
references the `candidate` subject kind without caring where candidates
come from.

### Greenhouse / Lever / Ashby (planned)

These backends are not yet shipped as plugins. The intended migration
shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-greenhouse
animus plugin install launchapp-dev/animus-subject-lever
animus plugin install launchapp-dev/animus-subject-ashby
```

The plugin would register `subject_kinds: ["candidate"]` (or
`greenhouse.candidate` if you want to run multiple sources side by
side). The workflow YAML does not change — only the plugin behind the
`candidate` kind changes.

If `candidate` is already claimed by `animus-subject-markdown`, you
have two options:

1. **Replace** — uninstall the markdown plugin, install the ATS plugin.
   The kind `candidate` now resolves to the ATS.
2. **Coexist** — keep markdown registered as `candidate`, install the
   ATS plugin as `greenhouse.candidate`. Run two workflows side by
   side, or change the workflow's subject kind. (Subject kind is
   resolved at dispatch time via `animus subject` CLI flags or
   `default_subject_kind` in `.animus/config.json`.)

### Wire to an ATS via trigger plugin

For event-driven dispatch (new candidate created in Greenhouse → screen
workflow runs automatically), install or build a trigger plugin that
listens on the ATS's webhook and dispatches the workflow:

```bash
# Hypothetical
animus plugin install launchapp-dev/animus-trigger-greenhouse-webhook
```

The trigger plugin would receive the Greenhouse webhook, extract the
candidate id, and call `animus workflow run --subject-kind candidate
--subject-id <id> animus.recruiting-pipeline/screen-candidate`. Note:
this depends on the `--subject-id` CLI primitive landing (see the
README's "current limitation" section).

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol).
Useful methods to implement: `candidate/list`, `candidate/get`,
`candidate/status`. See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language
that can speak JSON-RPC over stdio).

## Integrate a video-interview platform

For zoom / google-meet / hireflix integration, the integration point is
the candidate notes field. Two patterns:

### Pattern A: pre-interview brief generation

Wire a trigger plugin that listens for "interview scheduled" webhooks
from your scheduling tool (Greenhouse Scheduler, Calendly, Gem). When
the webhook fires, dispatch `screen-candidate` so the brief is fresh.
The interviewer reads the brief 15 minutes before the call.

### Pattern B: post-interview note ingestion

Wire a trigger plugin (or a scheduled job) that pulls transcripts from
your video platform (Zoom transcripts, Read.ai, Otter) into the
candidate's notes field, then dispatches `debrief-synthesis`. The
synthesis runs on the actual transcript instead of free-form notes.

NOTE: transcript-based debriefs are higher-risk for hallucinations of
candidate statements. Keep the `collect` phase's "Quote verbatim — never
invent quotes" instruction strict, and have the synthesizer flag any
quote that doesn't appear verbatim in the source notes.

## Add live web enrichment

The default `enrich` phase works ONLY from context in the dispatch
envelope. To add live web search, two options:

1. **Provider-side web search.** Use a provider plugin that has web
   search built in (e.g. an OpenAI provider configured to use the
   `web_search` tool). The agent's prompt then changes to "use web
   search to research the candidate's public footprint" instead of
   "work only from context provided".
2. **MCP search server.** Wire a custom MCP server providing web search
   tools and include it in the agent's allowed tools.

Either way: update the `candidate-enricher` system prompt to remove
the "you do not have live web access" caveat. And remember the
limitation — public-context research on candidates is governed by your
jurisdiction's employment law and your company's hiring policies.
Verify before shipping.

## Change the human-review gate

Right now `flag_for_review` and `flag` set the subject status to the
normalized `blocked` value and add labels (`awaiting-recruiter-review`
and `awaiting-debrief-decision` respectively). To plug into your
existing notification stack:

- **Slack:** add a `mode: command` phase after the flag phase that
  calls `curl` to a Slack incoming webhook. The phase can read the
  previous phase's output via the dispatch envelope.
- **Email:** same pattern with `msmtp` / `mail` / `sendgrid` CLI.
- **Greenhouse / Lever in-app notifications:** install the matching
  subject backend and create a follow-up subject (a note on the
  candidate record) with the brief attached.

## What's intentionally NOT customizable here

A few constraints are encoded in the agent prompts on purpose. Editing
them at all is a load-bearing change you should think hard about:

- **The screener never makes a hire / no-hire call.** It produces
  rubric scores. The pack's framing is "Animus is research, humans
  decide". A custom prompt that asks the screener to recommend
  hire / no-hire is a different pack.
- **The debrief synthesizer recommends, but doesn't decide.** The
  `no-decision` value in the recommendation enum exists specifically
  so the model has an honest out when evidence is thin or
  interviewers disagreed. Removing it forces false-precision.
- **No agent in this pack drafts candidate-facing communication.**
  Sourcing messages, screen-call invites, take-home assignments,
  offer letters, and rejection notes are intentionally out of scope.
  Draft those yourself or build a separate pack — but please make
  the send-step a human action.

## Wire up a schedule

To screen automatically as candidates arrive, add a schedule block to
the workflow file:

```yaml
schedules:
  - id: screen-new-candidates
    cron: "*/10 * * * *"   # every 10 minutes
    workflow_ref: animus.recruiting-pipeline/screen-candidate
    enabled: true
```

You'll also want a dispatcher agent that scans for candidates with
`stage: sourced` and enqueues one screen workflow per candidate (the
`requirements`/`req-dispatch` pattern in
[`.animus/workflows/requirements.yaml`](../../../.animus/workflows/requirements.yaml)
is the reference implementation).
