# Recruiting Pipeline Pack

Animus runs your recruiting top-of-funnel: enriches candidates, screens against
your rubric, drafts interviewer briefs and debriefs — your team focuses on the
human conversations.

This is a first-party Animus reference pack that turns the orchestrator into an
autonomous research-and-prep layer for the recruiting workflow. New candidates
get enriched from public context, scored against the role rubric, and handed off
with a 1-page interviewer brief. After the loop, raw interview notes get
synthesized into a weighted hire / no-hire recommendation. The recruiter and
hiring committee make the actual decisions.

## What this pack does NOT do

The constraint is explicit on purpose:

- **No hiring decisions.** Animus produces scores, briefs, and recommendations.
  Humans decide who advances, who gets an offer, and who's rejected.
- **No auto-send candidate comm.** This pack does not draft outbound recruiter
  outreach, does not send rejections, and does not schedule interviews. Drafts
  for *interviewers* (briefs) are produced; drafts for *candidates* are not.
- **No reference checks or background screens.** Out of scope.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash` or
  follow [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default workflow
  YAML pins every agent to Claude Sonnet (recruiting copy is high-stakes and
  benefits from a stronger model). `setup.sh` only installs
  `animus-provider-claude`. To use OpenAI or Gemini instead, see
  [`docs/customizing.md`](docs/customizing.md) — you'll edit each agent's
  `model` + `tool` and install the matching provider plugin before running
  setup.
- A directory to drop candidate markdown files into. The pack provides 6
  realistic samples (5 for screening, 1 with raw interview notes for debrief)
  you can use to dry-run before wiring up a real source.

## Setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy both workflows into .animus/workflows/, and
#    split the bundled sample-candidates.md into one file per candidate
#    under candidates/inbox/. setup.sh is idempotent; re-running never
#    clobbers edits to existing candidate files.
bash packs/recruiting-pipeline/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind candidate --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a screen run against a single candidate. See "Dispatching
#    candidates — current limitation" below for why we pass --title +
#    --description instead of --subject-id.
CAND_FILE="$PROJECT_ROOT/candidates/inbox/CAND-1001.md"
animus workflow run animus.recruiting-pipeline/screen-candidate \
  --title "$(head -n 1 "$CAND_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$CAND_FILE")" \
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
   WF_ID=$(animus workflow run animus.recruiting-pipeline/screen-candidate \
     --title "..." --description "..." --sync --json \
     --project-root "$PROJECT_ROOT" \
     | jq -r '.data.workflow_id')
   animus output phase-outputs --workflow-id "$WF_ID" \
     --project-root "$PROJECT_ROOT"
   ```

3. **Browse on disk** if you prefer. Persisted per-phase outputs live
   under `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
   as JSON files, one per phase.

The exact field shapes of `--json` envelope payloads are evolving; if any
of the `jq` paths above return `null` on your install, run the command
without `--json` once to see the human output, then adapt.

## Dispatching candidates — current limitation

`animus workflow run` currently accepts `--task-id`, `--requirement-id`, or
`--title` to identify the subject. There is **no first-class
`--subject-id` flag for arbitrary subject kinds yet**. That means today's
dispatch path for this pack creates a `custom` subject and passes the
candidate title + notes inline (the `--description` flag) — the workflow's
LLM phases see the candidate content and produce enrichment, scores, and
the brief, but the final `flag_for_review` phase cannot write the status
back to the real `animus-subject-markdown` candidate record (the run is
not associated to the candidate's backend-qualified id).

What works today:

- `enrich`, `screen_against_rubric`, `draft_interview_brief`, `collect`,
  and `synthesize` produce real output you can read via
  `animus output phase-outputs`.
- The recruiter takes the brief / debrief and acts on it in their ATS.

What doesn't work end-to-end yet:

- `flag_for_review` / `flag` updating the candidate subject's status.
  The phase prompt instructs the agent to call
  `animus subject status --kind candidate --id <id>` and that will work
  if the agent passes the correct backend-qualified id — but the run
  envelope doesn't carry it automatically because the dispatch went
  through `--title`.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind candidate`** — which would let the markdown backend
resolve the candidate context up-front and thread the backend-qualified
id through the run envelope. That's the headline missing primitive
this pack surfaces.

## What you get

### Workflow: `screen-candidate` (4 phases)

| Phase | What it does |
|---|---|
| `screen_enrich` | Summarizes the candidate's public footprint: open-source contributions, talks, writing, github trail, prior employers. Never invents facts — says "not found in available context" when evidence is missing. |
| `screen_against_rubric` | Scores the candidate against the role rubric: `skill_match`, `level_fit`, `motivation_signals` (each 0-5) and a `risk_flags` array. Does NOT make a hire / no-hire call. |
| `draft_interview_brief` | Produces a 1-page brief for the next interviewer: context, areas to probe, suggested questions, items to verify, and a `recommended_next` enum (proceed-to-screen / proceed-to-tech / proceed-to-onsite / hold-for-recruiter-review / pass). |
| `screen_flag_for_review` | Sets the candidate subject status to `blocked` (the protocol-normalized "needs human action" state) and adds an `awaiting-recruiter-review` label so it surfaces in the recruiter's inbox. |

### Workflow: `debrief-synthesis` (3 phases)

| Phase | What it does |
|---|---|
| `debrief_collect` | Normalizes raw interview notes from multiple interviewers into a structured per-interviewer record (interviewer, focus_area, signals, quotes, per-dimension rubric). Quotes verbatim — never invents quotes. |
| `debrief_synthesize` | Produces a weighted recommendation across the loop: weighted scores, agreement level, recommendation enum (`strong-hire` through `strong-no-hire`, or `no-decision`), confidence (0.0-1.0), rationale, and committee questions. Recommends `no-decision` rather than forcing a call when evidence is thin or interviewers disagreed strongly. |
| `debrief_flag` | Sets the candidate subject status to `blocked` and adds an `awaiting-debrief-decision` label so the hiring committee picks it up. |

Outputs land under standard Animus paths: per-phase JSON snapshots under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`,
and run artifacts under `~/.animus/<repo-scope>/runs/<run-id>/` (note:
workflow id and run id are distinct — list runs via `animus workflow
list` to find the matching ids).

## Sample candidates

The bundled `subjects/sample-candidates.md` has 6 realistic profiles:

| ID | Persona |
|---|---|
| `CAND-1001` | Senior eng referral — strong public signal, passive interest |
| `CAND-1002` | Inbound junior dev — new grad with one personal project |
| `CAND-1003` | Executive search candidate for VP Engineering |
| `CAND-1004` | Recent grad with a strong frontend portfolio |
| `CAND-1005` | Passive-sourced staff eng — has declined outreach twice before |
| `CAND-2001` | DEBRIEF READY — interview loop complete, raw notes from 3 interviewers (for `debrief-synthesis`) |

## Customize it

This pack is meant to be forked. The files you'll edit most:

- **`workflows/screen-candidate.yaml`** — change the rubric dimensions, the
  scoring scale, the models per phase, or the directives the agents see.
- **`workflows/debrief-synthesis.yaml`** — change the weighting rule across
  interviewers, the recommendation enum, or the committee questions schema.
- **`subjects/sample-candidates.md`** — replace with your own candidates
  once you've validated the pipeline.

For deeper changes — swapping the LLM, wiring to Greenhouse/Lever/Ashby,
integrating a video-interview platform, plugging into Slack for recruiter
review — see [`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No real ATS integration.** The pack uses `animus-subject-markdown`
  reading markdown files from a directory. To pull candidates from
  Greenhouse, Lever, Ashby, or Workday, you need a subject backend plugin
  for that source. See `docs/customizing.md` for the shape.
- **No live web enrichment.** The `enrich` phase works from context
  provided in the dispatch envelope (the candidate notes / links you
  pass in). It does NOT have web-search tools wired up. To add live
  enrichment, attach a research-capable provider or wire a custom MCP
  search server. See `docs/customizing.md`.
- **No outbound candidate comm.** Drafts for interviewers (briefs)
  are produced; drafts for candidates (sourcing messages, rejections,
  offer letters) are not. A "draft candidate comm" pack is a follow-up;
  even then, Animus should never auto-send.
- **No scheduling.** Interview scheduling, calendar holds, and
  availability matching are out of scope.
- **No long-running candidate memory.** Each screen / debrief is
  independent. There's no cross-candidate calibration ("we've already
  rejected 3 candidates this strong this week" is not modelled).
- **No hire / no-hire decision.** This is the headline constraint.
  Animus produces a recommendation; humans decide.

If you build any of the above on top of this pack, please open a PR — the
intent is that this pack grows into a library of recruiting patterns over
time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page diagram
of how candidates flow through the phases and which plugins are involved.
