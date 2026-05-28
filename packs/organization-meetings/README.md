# Organization Meetings Pack

Animus preps you for every meeting, captures the actions afterwards, and rolls
up your week into a status doc — without you opening a doc editor.

This is a first-party Animus reference pack that turns the orchestrator into
a meeting-ops assistant. Before the meeting, it drafts a 1-page brief from
the attendee context. After the meeting, it extracts action items from your
raw notes, drafts the followup email, contributes a 3-bullet entry to the
weekly status, and flags the meeting for your review. At the end of the week,
a separate workflow aggregates the contributions into a one-page status doc.

This is **a non-coding reference pack** for Animus. It exists to show that the
"self-hosted workflow engine for AI agents, BYO models, BYO data sources"
framing works for ops cadences, not just code.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash` or
  follow [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default workflow
  YAML pins every prose phase to Claude Sonnet and the cheap review-gate to
  Claude Haiku; `setup.sh` only installs `animus-provider-claude`. To use
  OpenAI or Gemini instead, see [`docs/customizing.md`](docs/customizing.md).
- A directory to drop meeting markdown files into. The pack provides 5
  sample meetings you can use to dry-run before wiring up a real source.

## Setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy both workflows into .animus/workflows/, and
#    split the bundled sample-meetings.md into one file per meeting
#    under meetings/inbox/. setup.sh is idempotent; re-running never
#    clobbers edits to existing meeting files.
bash packs/organization-meetings/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind meeting --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a prep-and-followup run against a single meeting. See
#    "Dispatching meetings — current limitation" below for why we pass
#    --title + --description instead of --subject-id.
MEETING_FILE="$PROJECT_ROOT/meetings/inbox/MTG-2001.md"
animus workflow run animus.organization-meetings/prep-and-followup \
  --title "$(head -n 1 "$MEETING_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$MEETING_FILE")" \
  --sync \
  --project-root "$PROJECT_ROOT"

# 5. At the end of the week, collect this project's per-meeting weekly
#    entries and run the rollup. The aggregator reads the entries from
#    the --description payload because nothing persists a `weekly_entry`
#    field on the meeting subject yet (see the limitation note below).
#    Scope strictly to THIS project's scoped runtime state via
#    `animus workflow list --project-root` + `animus output phase-outputs
#    --project-root`, so meetings from other projects on the same
#    machine cannot leak in.
# Last 7 days, in ISO-8601 (UTC) for string comparison against
# PersistedPhaseOutput.completed_at:
SINCE=$(python3 -c "import datetime; print((datetime.datetime.utcnow()-datetime.timedelta(days=7)).strftime('%Y-%m-%dT%H:%M:%SZ'))")

# Per-project workflow list (project-scoped via --project-root), then
# pull each run's synthesize_weekly phase output, filter by
# completed_at >= SINCE, and unwrap the persisted envelope so the
# aggregator sees the JSON object the agent emitted (decisions,
# blockers, theme) augmented with the workflow id for attribution.
# `animus workflow list --json` returns an envelope under .data whose
# shape depends on the transport: a bare array on the local path, or
# `{ runs: [...] }` on the daemon-control path. The control path also
# IGNORES the --workflow-ref CLI flag (it only carries status/cursor/
# limit on the wire), so we post-filter on workflow_ref in jq to
# guarantee we only collect prep-and-followup runs even when the
# daemon is up.
WF_REF="animus.organization-meetings/prep-and-followup"
WF_IDS=$(animus workflow list \
  --workflow-ref "$WF_REF" \
  --project-root "$PROJECT_ROOT" --json \
  | jq -r --arg ref "$WF_REF" '
      (.data | if type == "array" then . else (.runs // []) end)[]
      | select(.workflow_ref == $ref)
      | .id
    ')

WEEKLY_ENTRIES=""
for WF_ID in $WF_IDS; do
  ENTRY=$(animus output phase-outputs \
    --workflow-id "$WF_ID" --phase-id synthesize_weekly \
    --project-root "$PROJECT_ROOT" --json 2>/dev/null \
    | jq -c --arg since "$SINCE" --arg wf "$WF_ID" '
        .data.outputs[]?
        | select(.completed_at >= $since)
        | (.payload // {}) + { workflow_id: $wf, completed_at: .completed_at }
      ')
  [ -n "$ENTRY" ] && WEEKLY_ENTRIES="${WEEKLY_ENTRIES}${ENTRY}"$'\n'
done

animus workflow run animus.organization-meetings/weekly-status \
  --title "Weekly status $(date +%Y-%m-%d)" \
  --description "$WEEKLY_ENTRIES" \
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
   WF_ID=$(animus workflow run animus.organization-meetings/prep-and-followup \
     --title "..." --description "..." --sync --json \
     --project-root "$PROJECT_ROOT" \
     | jq -r '.data.workflow_id')
   animus output phase-outputs --workflow-id "$WF_ID" \
     --project-root "$PROJECT_ROOT"
   ```

3. **Browse on disk.** Persisted per-phase outputs live under
   `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
   as JSON files, one per phase. Run logs live under
   `~/.animus/<repo-scope>/runs/<run-id>/`.

## Dispatching meetings — current limitation

`animus workflow run` currently accepts `--task-id`, `--requirement-id`,
or `--title` to identify the subject. There is **no first-class
`--subject-id` flag for arbitrary subject kinds yet**. That means today's
dispatch path for this pack creates a `custom` subject and passes the
meeting title + body inline (the `--description` flag) — the workflow's
LLM phases see the meeting content and produce real output, but the final
`meeting_flag_for_review` phase cannot write the status back to the real
`animus-subject-markdown` meeting record (the run is not associated to
the meeting's backend-qualified id).

What works today:

- `pre_brief`, `extract_actions`, `draft_followup`, and
  `synthesize_weekly` produce real output you can read via
  `animus output phase-outputs`.
- The principal takes the brief / draft / weekly entry and acts on
  them in their own tools.

What doesn't work end-to-end yet:

- `meeting_flag_for_review` updating the meeting subject's status. The phase
  prompt instructs the agent to call `animus subject status --kind
  meeting --id <id>` and that will work if the agent passes the
  correct backend-qualified id — but the run envelope doesn't carry
  it automatically because the dispatch went through `--title`.
- The weekly rollup reading per-meeting `weekly_entry` contributions.
  The same missing `--subject-id` primitive is what would let the
  per-meeting `synthesize_weekly` phase persist its 3-bullet output
  back onto the meeting record.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind meeting`** — which would let the markdown backend
resolve the subject context (title + body + attendees + prior_context)
up-front and thread the backend-qualified id through the run envelope.
That's the headline missing primitive this pack surfaces, same as the
customer-support pack.

## What you get

The `prep-and-followup` workflow has five phases:

| Phase | What it does |
|---|---|
| `pre_brief` | Reads meeting metadata + attendee prior_context. Produces objectives, per-attendee context, talking points, and risks — a 1-pager the principal can read in 2 minutes. |
| `extract_actions` | Parses raw notes into structured action items `{ owner, action, due, priority, source_quote }`. Conservative: only emits actions where owner and verb are clearly identifiable. |
| `draft_followup` | Drafts the post-meeting followup email summarizing decisions, next steps, and open questions. Output ready for review and send. |
| `synthesize_weekly` | Condenses the meeting into a 3-bullet entry (decisions, blockers, theme) for the weekly status doc to aggregate. |
| `meeting_flag_for_review` | Sets the meeting subject status to `blocked` (the protocol-normalized "needs human action" state) and adds an `awaiting-human-review` label. |

The `weekly-status` workflow has one phase:

| Phase | What it does |
|---|---|
| `aggregate_weekly_status` | Reads meeting subjects from the past 7 days, reads each meeting's per-meeting weekly_entry, produces the one-page status doc covering decisions, blockers, recurring themes, quiet areas, and skipped meetings. |

Per-phase structured outputs land under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
and the streaming run log lives under `~/.animus/<repo-scope>/runs/<run-id>/`
— same layout as every other Animus workflow.

## Customize it

This pack is meant to be forked. The four files you'll edit most are:

- **`workflows/prep-and-followup.yaml`** — change the brief format, the
  action-item shape, the followup email tone, the weekly entry format,
  or the models per phase.
- **`workflows/weekly-status.yaml`** — change the rollup window, the
  section grouping, or the model for the aggregator.
- **`subjects/sample-meetings.md`** — replace with your own meetings
  once you've validated the pipeline.
- **`scripts/setup.sh`** — adjust the install steps if you're swapping
  the provider or subject backend.

For deeper changes — swapping the LLM, wiring to Google Calendar /
Notion / Linear, sending the followup via email — see
[`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No live audio transcription.** The pack assumes you (or another
  tool) provide the raw meeting notes as text. We do not connect to
  Zoom, Google Meet, or any STT service.
- **No auto-send.** The followup email is drafted, never sent. The
  principal reviews and sends from their own inbox. A "send from
  Animus" experience is a follow-up pack (it would need an action
  plugin per mail provider).
- **No real calendar integration.** The pack uses
  `animus-subject-markdown` reading markdown files. To pull from
  Google Calendar / Outlook / iCal, you need a calendar-aware subject
  backend plugin. See `docs/customizing.md` for the shape.
- **No long-running cross-meeting memory.** The prep phase reads
  `prior_context` from the meeting subject itself; it does not
  automatically pull "last 3 1:1s with this person". You stage the
  context manually (or a smarter subject backend does it for you).
- **No reminder cadence.** The pack doesn't nag you about overdue
  reviews — once a meeting is flagged blocked, it stays blocked until
  you act on it.

If you build any of the above on top of this pack, please open a PR —
the intent is that this pack grows into a library of ops patterns over
time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page diagram of how
meetings flow through the phases and which plugins are involved.
