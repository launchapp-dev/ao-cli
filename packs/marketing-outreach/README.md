# Marketing Outreach Pack

A first-party Animus reference pack that turns the orchestrator into an
autonomous outbound and lifecycle drafting layer. Every prospect in your
pipeline — inbound demo request, webinar attendee, account-based target,
cold list, referral — gets enriched with public context, a 3-line
personalized message drafted for human review, and a follow-up cadence
suggested. The reviewer edits if needed, clicks send. Animus did the
reading, the angle-finding, and the writing.

**Animus drafts personalized outreach for every prospect in your
pipeline, ready for human review.**

This is the second non-coding reference pack (after `customer-support`)
and it proves the "self-hosted workflow engine for AI agents, BYO
models, BYO data sources" framing for outbound marketing and SDR work.
It exists so agencies, marketing leads, founders running their own
outbound, and revenue ops teams have a working starter they can fork.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash`
  or follow [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default
  workflow YAML pins every agent to a Claude model (Sonnet for the
  research-discipline-heavy enrich + draft phases, Haiku for the
  cheap cadence + handoff phases) and `setup.sh` only installs
  `animus-provider-claude`. To use OpenAI or Gemini instead, see
  [`docs/customizing.md`](docs/customizing.md) — you'll edit each
  agent's `model` + `tool` and install the matching provider plugin
  before running setup.
- A directory to drop prospect markdown files into. The pack provides
  5 sample prospects you can use to dry-run before wiring up a real
  source.

## Setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy the workflow into .animus/workflows/, and
#    split the bundled sample-prospects.md into one file per prospect
#    under prospects/inbox/. setup.sh is idempotent; re-running never
#    clobbers edits to existing prospect files.
bash packs/marketing-outreach/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind prospect --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a triage run against a single prospect. See "Dispatching
#    prospects — current limitation" below for why we pass --title +
#    --description instead of --subject-id.
PROSPECT_FILE="$PROJECT_ROOT/prospects/inbox/PRS-1001.md"
animus workflow run animus.marketing-outreach/triage-prospect \
  --title "$(head -n 1 "$PROSPECT_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$PROSPECT_FILE")" \
  --sync \
  --project-root "$PROJECT_ROOT"
```

Inspect the run. Animus generates a fresh UUID workflow id per dispatch
and writes outputs under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`,
so you can either:

1. **List recent runs to find the id**, then drill in:

   ```bash
   animus workflow list --project-root "$PROJECT_ROOT"
   animus output phase-outputs \
     --workflow-id <id-from-list> \
     --project-root "$PROJECT_ROOT"
   ```

2. **Capture the id at dispatch time via `--json`** (the CLI envelope is
   `animus.cli.v1` — payload sits under `data`):

   ```bash
   WF_ID=$(animus workflow run animus.marketing-outreach/triage-prospect \
     --title "..." --description "..." --sync --json \
     --project-root "$PROJECT_ROOT" \
     | jq -r '.data.workflow_id')
   animus output phase-outputs --workflow-id "$WF_ID" \
     --project-root "$PROJECT_ROOT"
   ```

3. **Browse on disk** if you prefer. Persisted per-phase outputs live
   under `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
   as JSON files, one per phase. Run logs live under
   `~/.animus/<repo-scope>/runs/<run-id>/`. List the most recent
   workflow output dirs for any active project:

   ```bash
   ls -dt ~/.animus/*/state/workflows/*/phase-outputs/ 2>/dev/null \
     | head -5
   ```

The exact field shapes of `--json` envelope payloads are evolving; if
any of the `jq` paths above return `null` on your install, run the
command without `--json` once to see the human output, then adapt.

## Dispatching prospects — current limitation

`animus workflow run` currently accepts `--task-id`, `--requirement-id`,
or `--title` to identify the subject. There is **no first-class
`--subject-id` flag for arbitrary subject kinds yet**. That means
today's dispatch path for this pack creates a `custom` subject and
passes the prospect title + body inline (the `--description` flag) —
the workflow's LLM phases see the prospect context and produce a
draft + cadence, but the final `flag_for_review` phase cannot write
the status back to the real `animus-subject-markdown` prospect record
(the run is not associated to the prospect's backend-qualified id).

What works today:

- `enrich`, `draft_outreach`, and `suggest_cadence` produce real
  output you can read via `animus output phase-outputs`.
- The reviewer takes the draft + cadence and runs them through their
  email tool / LinkedIn / CRM sequencer manually.

What doesn't work end-to-end yet:

- `flag_for_review` updating the prospect subject's status. The phase
  prompt instructs the agent to call `animus subject status --kind
  prospect --id <id>` and that will work if the agent passes the
  correct backend-qualified id — but the run envelope doesn't carry
  it automatically because the dispatch went through `--title`.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind prospect`** — which would let the markdown backend
resolve the subject context (title + body) up-front and thread the
backend-qualified id through the run envelope. That's the headline
missing primitive this pack surfaces (same as `customer-support`).

## What you get

The `triage-prospect` workflow has four phases:

| Phase | What it does |
|---|---|
| `enrich` | Reads prospect record (company, contact, role, source, context notes). Returns a JSON research blob: `company_summary`, `recent_signals[]`, `best_angle`. Discipline rule: never invents funding rounds, hiring signals, or quotes. |
| `draft_outreach` | Picks tone from the source channel (warm for `demo_request` / `webinar_attendee`, signal-led for `cold_outbound`, referrer-named for `referral`) and writes a 3-line message + subject line. Strict 3-line limit. |
| `suggest_cadence` | Recommends 2-4 follow-up touches with `wait_days`, channel (email / linkedin / call), and one-line content per touch. Stops on any reply. Stays honest: cold prospects get fewer touches. |
| `flag_for_review` | When dispatched against a real backend-qualified prospect id (e.g. `PRS-1001` or `prospect:...`), sets the prospect subject status to `blocked` (the protocol-normalized "needs human action" state) and adds an `awaiting-human-review` label so it surfaces in the reviewer's inbox. In the documented dry-run path (`--title`/`--description`), the phase intentionally SKIPS the mutating subject calls — see "Dispatching prospects — current limitation" above. |

Per-phase structured outputs land under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
(one JSON file per phase, useful for later analysis like "which angles
drive the highest reply rate?"). Run logs and artifacts live separately
under `~/.animus/<repo-scope>/runs/<run-id>/` (keyed by run id, not
workflow id).

## Customize it

This pack is meant to be forked. The two files you'll edit most are:

- **`workflows/triage-prospect.yaml`** — change the angle list, the
  tone-by-source-channel rules, the cadence constraints, models per
  phase (cheap Haiku for cadence / handoff, stronger model for enrich
  and draft), or the directives the agents see.
- **`subjects/sample-prospects.md`** — replace with your own prospects
  once you've validated the pipeline.

For deeper changes — swapping the LLM, wiring to HubSpot/Salesforce,
adding a real send step, adjusting the human-review gate — see
[`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No actual send.** The pack stops at draft + cadence plan. It does
  not send the email, the LinkedIn DM, or push to Outreach.io / Apollo
  / Salesloft. The reviewer copies the draft into their send tool of
  choice. A "send from Animus" experience is a follow-up pack — it
  would need an action plugin per send channel.
- **No real CRM integration.** The pack uses `animus-subject-markdown`
  reading markdown files from a directory. To pull from HubSpot,
  Salesforce, Apollo, Clay, or your data warehouse, you need a subject
  backend plugin for that source. See `docs/customizing.md` for the
  shape.
- **No reply tracking.** Each prospect is triaged independently. If a
  prospect replies, Animus does not know — there's no built-in webhook
  ingestion for inbound replies. Wire up `animus-trigger-webhook` (or
  equivalent) and flip the subject status back to `done` on reply.
- **No deliverability / spam scoring.** The pack drafts copy; it does
  not check it against spam triggers, send-volume limits, or warmup
  state. That's a separate concern handled at the send-tool layer.
- **No A/B testing.** Each prospect gets one draft. No variant
  generation or experiment tracking.
- **No team-level analytics.** Outputs are per-prospect. Roll-ups
  ("reply rate by angle this month") need to be built on top of the
  per-phase JSON output files.

If you build any of the above on top of this pack, please open a PR —
the intent is that this pack grows into a library of outbound and
lifecycle patterns over time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page diagram
of how prospects flow through the phases and which plugins are
involved.
