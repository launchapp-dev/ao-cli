# Sales Pipeline Pack

> **Animus qualifies + drafts discovery for every lead overnight.** Your reps
> walk in to ready-to-call accounts each morning — enriched, BANT-scored,
> with 5 targeted discovery questions and a recommended next action already
> drafted. They review, approve, and dial.

A first-party Animus reference pack that turns the orchestrator into an
autonomous SDR/BDR layer. New leads get enriched, scored against BANT,
discovery-prepped, and routed with a recommended next action — all before
a rep touches them.

**Honest constraint:** this pack stops at human review. Animus does NOT
auto-send outreach. Every recommended action (book demo / send case study
/ nurture / disqualify) requires a rep to actually do it. That's by design
— sending cold messages from an LLM without a human approval gate is how
you burn a domain and a brand.

**Setup time: ~15 minutes** from `git clone` to first qualified lead.

This is the second non-coding reference pack for Animus (after
[`customer-support`](../customer-support/README.md)). It exists to prove the
"self-hosted workflow engine for AI agents, BYO models, BYO data sources"
framing on a second canonical vertical and to give sales ops, founders,
and agencies a working starter they can fork.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash` or
  follow [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default
  workflow YAML pins reasoning agents to `claude-sonnet-4-6` (enrich,
  qualify, draft discovery) and cheaper agents to `claude-haiku-4-5`
  (recommend next, rep handoff). `setup.sh` only installs
  `animus-provider-claude`. To use OpenAI or Gemini instead, see
  [`docs/customizing.md`](docs/customizing.md) — you'll edit each
  agent's `model` + `tool` and install the matching provider plugin
  before running setup.
- A directory to drop lead markdown files into. The pack provides 5
  sample leads you can use to dry-run before wiring up a real CRM.

## Setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy the workflow into .animus/workflows/, and
#    split the bundled sample-leads.md into one file per lead under
#    leads/inbox/. setup.sh is idempotent; re-running never clobbers
#    edits to existing lead files.
bash packs/sales-pipeline/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind lead --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a qualify run against a single lead. See "Dispatching
#    leads — current limitation" below for why we pass --title +
#    --description instead of --subject-id.
LEAD_FILE="$PROJECT_ROOT/leads/inbox/LEAD-2001.md"
animus workflow run animus.sales-pipeline/qualify-lead \
  --title "$(head -n 1 "$LEAD_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$LEAD_FILE")" \
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
   WF_ID=$(animus workflow run animus.sales-pipeline/qualify-lead \
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

## Dispatching leads — current limitation

`animus workflow run` currently accepts `--task-id`, `--requirement-id`,
or `--title` to identify the subject. There is **no first-class
`--subject-id` flag for arbitrary subject kinds yet**. That means today's
dispatch path for this pack creates a `custom` subject and passes the
lead title + body inline (the `--description` flag) — the workflow's
LLM phases see the lead content and produce all the outputs, but the
final `lead_flag_for_review` phase cannot write the status back to the real
`animus-subject-markdown` lead record (the run is not associated to the
lead's backend-qualified id).

What works today:

- `enrich`, `qualify`, `draft_discovery`, and `recommend_next` produce
  real output you can read via `animus output phase-outputs`.
- The rep takes the BANT verdict + discovery plan + recommended action
  and acts on it in their CRM, email, or phone tool.

What doesn't work end-to-end yet:

- `lead_flag_for_review` updating the lead subject's status. The phase
  prompt instructs the agent to call `animus subject status --kind lead
  --id <id>` and that will work if the agent passes the correct
  backend-qualified id — but the run envelope doesn't carry it
  automatically because the dispatch went through `--title`.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind lead`** — which would let the markdown backend resolve
the subject context (title + body) up-front and thread the
backend-qualified id through the run envelope. That's the headline
missing primitive this pack surfaces (shared with the
`customer-support` pack).

## What you get

The `qualify-lead` workflow has five phases:

| Phase | What it does |
|---|---|
| `enrich` | Reads lead subject + body. Returns firmographics (industry, employee band, region, tech stack, recent events), a fit score (0-100), and up to 5 verbatim intent signals. Sonnet. |
| `qualify` | Scores the lead against BANT: per-dimension { score, evidence } for budget, authority, need, timing. Sets `qualified: true` only if 3 of 4 dimensions are moderate-or-stronger and need/timing aren't weak. Sonnet. |
| `draft_discovery` | Writes 5 targeted discovery questions specific to this account's industry / role / stated pain, each with a rationale. Ordered: lowest-stakes opener → most decisive closer. Sonnet. |
| `recommend_next` | Picks one of `book-demo` / `send-case-study` / `nurture` / `disqualify`. Adds an `owner_hint` (inbound-rep / enterprise-ae / smb-ae / nurture-bot) and `urgency` (same-day / this-week / this-month / none). Haiku. |
| `lead_flag_for_review` | Sets the lead subject status to `blocked` (the protocol-normalized "needs human action" state) and adds an `awaiting-rep-review` label so it surfaces in the rep's inbox. Haiku. |

Outputs land under `~/.animus/<repo-scope>/runs/<workflow-id>/` like any
other Animus workflow. Each phase's structured output is captured
separately for audit.

## A morning-of walkthrough

The product promise is "your reps walk in to ready-to-call accounts each
morning." Here's what that actually looks like for a rep:

1. Overnight, 12 new inbound leads land in `leads/inbox/` (via your
   webhook → markdown writer of choice, or a real subject backend).
2. A scheduled `qualify-new-leads` workflow runs the 5-phase qualify
   pipeline against each one (see
   [`docs/customizing.md`](docs/customizing.md) for the schedule shape).
3. By 8am, each lead has a phase-outputs JSON directory with the BANT
   verdict, discovery plan, and recommended action.
4. The rep opens `animus output phase-outputs --workflow-id <id>` for
   each `awaiting-rep-review` lead, scans the recommendation, and
   either:
   - **`book-demo`** — copies the discovery questions into their
     calendar invite and sends.
   - **`send-case-study`** — picks the right case study based on the
     enrichment industry tag and sends.
   - **`nurture`** — adds to drip.
   - **`disqualify`** — sends a polite close-the-loop reply.

The honest constraint again: every send is a rep action. Animus does
not auto-email leads in this pack.

## Customize it

This pack is meant to be forked. The two files you'll edit most are:

- **`workflows/qualify-lead.yaml`** — change the qualification framework
  (BANT → MEDDIC / CHAMP / GPCT), the recommended-action set, tone
  guides, or models per phase. The full migration map is in
  [`docs/customizing.md`](docs/customizing.md).
- **`subjects/sample-leads.md`** — replace with your own leads once
  you've validated the pipeline.

For deeper changes — swapping the LLM, wiring to Salesforce / HubSpot /
Linear, adjusting the human-review gate, sending notifications — see
[`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No real CRM integration.** The pack uses `animus-subject-markdown`
  reading markdown files from a directory. To pull from Salesforce,
  HubSpot, Pipedrive, or your inbound webform, you need a subject
  backend plugin for that source. See `docs/customizing.md` for the
  shape.
- **No auto-outreach.** The rep reads the BANT verdict + discovery plan
  from `animus output` and acts in their CRM / email / phone tool. A
  "send from Animus" experience would be a follow-up pack (it would
  need a trigger plugin per CRM + an explicit human-approval gate; LLM
  outbound without that is reputation arson).
- **No long-running context.** Each lead is qualified independently.
  There's no cross-lead memory ("we've spoken to this company twice
  this quarter under a different contact" is not modelled).
- **No SLA timers.** Urgency is scored, but the pack doesn't escalate
  when a hot lead goes stale. Wire up a schedule + a dispatcher agent
  (the `requirements` pattern in `.animus/workflows/requirements.yaml`
  shows how) to add that.
- **No multi-touch attribution.** Each phase output is a per-lead
  snapshot; building a pipeline funnel report on top of this requires
  a downstream consumer.

If you build any of the above on top of this pack, please open a PR —
the intent is that this pack grows into a library of sales patterns
over time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page diagram
of how leads flow through the phases and which plugins are involved.
