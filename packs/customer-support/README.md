# Customer Support Pack

A first-party Animus reference pack that turns the orchestrator into an autonomous
tier-1 support agent. New tickets get triaged into categories, urgency-scored, and
a draft response is written for a human to review-and-send. The reviewer just edits
the draft and clicks send — Animus did the reading, the routing, and the writing.

This is **the first non-coding reference pack** for Animus. It exists to prove the
"self-hosted workflow engine for AI agents, BYO models, BYO data sources" framing
in practice and to give agencies, support leads, and ops teams a working starter
they can fork.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash` or follow
  [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default
  workflow YAML pins every agent to a Claude model (Haiku for the
  cheap classify/handoff phases, Sonnet for drafting) and `setup.sh`
  only installs `animus-provider-claude`. To use OpenAI or Gemini
  instead, see [`docs/customizing.md`](docs/customizing.md) — you'll
  edit each agent's `model` + `tool` and install the matching provider
  plugin before running setup.
- A directory to drop ticket markdown files into. The pack provides 5 sample
  tickets you can use to dry-run before wiring up a real source.

## Setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy the workflow into .animus/workflows/, and
#    split the bundled sample-tickets.md into one file per ticket
#    under tickets/inbox/. setup.sh is idempotent; re-running never
#    clobbers edits to existing ticket files.
bash packs/customer-support/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind ticket --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a triage run against a single ticket. See "Dispatching
#    tickets — current limitation" below for why we pass --title +
#    --description instead of --subject-id.
TICKET_FILE="$PROJECT_ROOT/tickets/inbox/TKT-1001.md"
animus workflow run animus.customer-support/triage-ticket \
  --title "$(head -n 1 "$TICKET_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$TICKET_FILE")" \
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
   WF_ID=$(animus workflow run animus.customer-support/triage-ticket \
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

The exact field shapes of `--json` envelope payloads are evolving; if any
of the `jq` paths above return `null` on your install, run the command
without `--json` once to see the human output, then adapt.

## Dispatching tickets — current limitation

`animus workflow run` currently accepts `--task-id`, `--requirement-id`, or
`--title` to identify the subject. There is **no first-class
`--subject-id` flag for arbitrary subject kinds yet**. That means today's
dispatch path for this pack creates a `custom` subject and passes the
ticket title + body inline (the `--description` flag) — the workflow's
LLM phases see the ticket content and produce a draft, but the final
`ticket_flag_for_review` phase cannot write the status back to the real
`animus-subject-markdown` ticket record (the run is not associated to
the ticket's backend-qualified id).

What works today:

- `classify`, `draft_response`, and `summarize_for_human` produce real
  output you can read via `animus output phase-outputs`.
- The reviewer takes the draft and acts on it in their support tool.

What doesn't work end-to-end yet:

- `ticket_flag_for_review` updating the ticket subject's status. The phase
  prompt instructs the agent to call `animus subject status --kind
  ticket --id <id>` and that will work if the agent passes the correct
  backend-qualified id — but the run envelope doesn't carry it
  automatically because the dispatch went through `--title`.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind ticket`** — which would let the markdown backend resolve
the subject context (title + body) up-front and thread the
backend-qualified id through the run envelope. That's the headline
missing primitive this pack surfaces.

## What you get

The `triage-ticket` workflow has four phases:

| Phase | What it does |
|---|---|
| `classify` | Reads ticket subject + body. Returns a JSON verdict with `category` (billing / bug / feature / security / integration / other) and `urgency` (low / med / high / critical). |
| `draft_response` | Picks tone from the category (apologetic for billing, investigatory for bugs, curious for feature asks, urgent for security) and drafts a customer-facing reply. |
| `summarize_for_human` | Produces a TL;DR + the top 3 suggested edits for the reviewer so they can approve fast. |
| `ticket_flag_for_review` | Sets the ticket subject status to `blocked` (the protocol-normalized "needs human action" state) and adds an `awaiting-human-review` label so it surfaces in the reviewer's inbox. |

Outputs land under `~/.animus/<repo-scope>/runs/<workflow-id>/` like any other
Animus workflow. Each phase's structured output is captured separately for audit.

## Customize it

This pack is meant to be forked. The two files you'll edit most are:

- **`workflows/triage-ticket.yaml`** — change the categories, urgency rubric, tone
  guides, models per phase (cheap Haiku for `classify`, stronger model for
  `draft_response`), or the directives the agents see.
- **`subjects/sample-tickets.md`** — replace with your own tickets once you've
  validated the pipeline.

For deeper changes — swapping the LLM, wiring to Zendesk/Intercom, adjusting the
human-review gate — see [`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No real ticket source integration.** The pack uses
  `animus-subject-markdown` reading markdown files from a directory. To pull
  from Zendesk, Intercom, Front, Help Scout, or email, you need a subject
  backend plugin for that source. See `docs/customizing.md` for the shape.
- **No send-on-approve UI.** The reviewer reads the draft from `animus output`
  output and copies it into their real support tool. A "send from Animus"
  experience is a follow-up pack (it would need an action plugin per support
  tool).
- **No long-running context.** Each ticket is triaged independently. There's no
  cross-ticket memory ("we've already replied to this user 3 times this week"
  is not modelled).
- **No SLA timers.** Urgency is scored, but the pack doesn't escalate when a
  ticket goes stale. Wire up a schedule + a dispatcher agent (the
  `requirements` pattern in `.animus/workflows/requirements.yaml` shows how)
  to add that.

If you build any of the above on top of this pack, please open a PR — the
intent is that this pack grows into a library of support patterns over time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page diagram of how
tickets flow through the phases and which plugins are involved.
