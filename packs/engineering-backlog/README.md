# Engineering Backlog Pack

> Animus picks up your backlog, drafts code changes, opens PRs while you
> sleep. The autonomous tier-1 for engineering work.

This is **the canonical reference pack** for adopting Animus on a
codebase. If you're an engineering team looking at Animus for the first
time and asking "where do I start?", the answer is here.

The workflow shape — research → plan → implement → cross-model review →
test → PR — is the same one we use to drive ao-cli's own development
(`.animus/workflows/common.yaml`, `.animus/workflows/review.yaml`). You
are not getting a toy demo. You are getting our production engineering
loop, packaged so it works on your codebase too.

## 60-second value prop

You have a backlog of dev work: bug fixes, small features, refactors,
dependency bumps, doc updates. Most of it is small, well-scoped, and
boring to do but expensive to defer.

This pack runs the autonomous tier-1: Animus picks one task, reads your
codebase, drafts a plan, writes the code, has a different LLM review
the diff, runs your tests, opens a PR. You wake up to a PR queue with
the implementer's plan, the reviewer's verdict, and green tests.

You review the PR (humans still gate the merge — by design). The boring
80% is done. You spend your time on the architecture work that's
actually hard.

This is the original Animus use case. Everything else in the platform
generalizes from this pattern.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash`
  or [the install guide](../../docs/getting-started/installation.md))
- `gh` CLI authenticated against your GitHub remote (used to open the
  PR)
- Two API keys: `ANTHROPIC_API_KEY` (for plan/implement/research/finalize)
  and `OPENAI_API_KEY` (for the Codex reviewer). The cross-model review
  is the point — Claude implements, Codex reviews independently. To
  run with only one provider see
  [`docs/customizing.md`](docs/customizing.md).
- A directory to drop task markdown files into. The pack provides 5
  sample tasks you can use to dry-run before wiring up a real source.

## 15-minute setup

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins (subject backend + claude + codex), copy the
#    workflow into .animus/workflows/, split the bundled sample-tasks.md
#    into one file per task under tasks/backlog/. setup.sh is
#    idempotent; rerunning never clobbers your edits.
bash packs/engineering-backlog/scripts/setup.sh

# 2. Smoke-test the subject backend
animus subject list --kind task --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Switch to a feature branch BEFORE dispatching. Title-dispatch
#    runs in your checkout (no managed worktree), so the implement
#    phase commits land on the current branch and the finalize phase
#    pushes from HEAD. Doing this from main would pollute main.
git switch -c ao/smoke-engineering-backlog-001

# 5. Dispatch the workflow against the first sample task synchronously.
#    See "Dispatching tasks — current limitation" below for why we pass
#    --title + --description instead of --task-id.
TASK_FILE="$PROJECT_ROOT/tasks/backlog/TASK-1005.md"
animus workflow run animus.engineering-backlog/implement-task \
  --title "$(head -n 1 "$TASK_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$TASK_FILE")" \
  --sync \
  --project-root "$PROJECT_ROOT"
```

## Dispatching tasks — current limitation

`animus workflow run` accepts `--task-id`, `--requirement-id`, or
`--title`. `--task-id` resolves through the built-in task adapter,
which does not see plugin-backed markdown tasks in a fresh project,
so dispatching by `--task-id TASK-1001` against a freshly seeded
backlog will return `task not found`. The verified-working dispatch
path is `--title + --description` (same approach the customer-support
pack uses for the same reason — see
`packs/customer-support/README.md`). In that path:

- `research`, `plan`, `implement`, `review`, and `test` all work
  end-to-end — they see the task content inline.
- `finalize` will open the PR if review approved + tests green, but
  it will SKIP the subject status mutation because the subject id is
  a title string, not a backend-qualified id. The workflow YAML
  already guards against this and the agent emits a clear "skipped
  subject update" note in its verdict.
- **Title-dispatch runs in the user's checkout**, not a managed
  worktree. The implement phase commits on whichever branch you're
  currently on. Switch to a feature branch BEFORE dispatching by
  title (e.g. `git switch -c ao/smoke-test-001`) so the finalize
  phase's push doesn't land on your default branch. The finalize
  agent has a safety gate that refuses to push when HEAD is on
  `main` / `master` / `develop` — switch branches first.

**The headline missing primitive is `animus workflow run --subject-id
<id> --subject-kind task`** — which would let any subject backend
(default markdown, Linear, etc.) resolve the subject context up-front
and thread the backend-qualified id through the run envelope. This
is the same gap the customer-support pack documents.

End-to-end `--task-id` dispatch (so finalize can mutate the task
status via `animus.subject.status`) is NOT available with this
plugin-backed setup today. `animus subject create --kind task` routes
through the `animus-subject-default` plugin, but
`animus workflow run --task-id` resolves through the built-in task
adapter, so plugin-created ids return "task not found". Until the
missing primitive lands, stick with the `--title` path above. The
finalize phase will skip the subject status mutation (and emit
"skipped subject update" in its verdict) but the PR will still be
opened on the success path.

For non-Rust projects, edit one field — the `test:` phase's `program:`
— in `.animus/workflows/implement-task.yaml`. See
[`docs/onboarding.md`](docs/onboarding.md) for the walkthrough.

Inspect the run. Animus mints a fresh UUID per dispatch and writes
outputs under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`,
so you can either:

1. **List recent runs to find the id**, then drill in:

   ```bash
   animus workflow list --project-root "$PROJECT_ROOT"
   animus output phase-outputs \
     --workflow-id <id-from-list> \
     --project-root "$PROJECT_ROOT"
   ```

2. **Capture the id at dispatch time via `--json`** (the CLI envelope
   is `animus.cli.v1` — payload sits under `data`). Stay on the
   verified `--title + --description` path:

   ```bash
   TASK_FILE="$PROJECT_ROOT/tasks/backlog/TASK-1005.md"
   WF_ID=$(animus workflow run animus.engineering-backlog/implement-task \
     --title "$(head -n 1 "$TASK_FILE" | sed 's/^## //')" \
     --description "$(tail -n +2 "$TASK_FILE")" \
     --sync --json \
     --project-root "$PROJECT_ROOT" \
     | jq -r '.data.workflow_id')
   animus output phase-outputs --workflow-id "$WF_ID" \
     --project-root "$PROJECT_ROOT"
   ```

3. **Watch it live** in another terminal:

   ```bash
   animus output monitor --run-id <run-id>
   ```

## Real example: TASK-1001 end-to-end (illustrative)

> Note: this walkthrough is illustrative — TASK-1001 is marked
> `status: illustrative` in the sample bundle because the underlying
> bug may already be fixed against your tree. The output shapes shown
> below are real but the specific findings may differ. Use it to
> understand what each phase produces, then dispatch your own task.

The illustrative task is a small bug: `animus daemon start` returns
exit 0 even when the daemon fails to start (it prints an error but the
shell sees success).

What you'll see when you dispatch it:

### `research` phase (Haiku, ~15s)

```json
{
  "relevant_files": [
    "crates/orchestrator-cli/src/services/operations/ops_daemon.rs",
    "crates/orchestrator-cli/src/main.rs"
  ],
  "approach_sketch": "The daemon start command currently catches the error from start_daemon() and prints it via tracing, then falls through to a normal Ok(()) return. The fix is to propagate the error and let the CLI's top-level error handler set the exit code. Need to verify the top-level handler does set a non-zero exit on Err.",
  "risks": [
    "Existing callers might rely on exit 0 + stderr — check tests.",
    "If start_daemon partially succeeds (e.g. spawns child then fails), we need to clean up."
  ]
}
```

### `plan` phase (Sonnet, ~45s)

A Markdown plan with file paths, line ranges, ordered steps, and a
list of tests to add. Looks like:

```markdown
## Goal
`animus daemon start --project-root /nonexistent` exits non-zero.

## Files to change
- `crates/orchestrator-cli/src/services/operations/ops_daemon.rs:142-178`
  — change `match start_daemon(...)` arm to return `Err(...)` instead
  of `Ok(())` after logging.
- `tests/cli/daemon_start.rs` (new) — regression test.

## Steps
1. Convert the catch-and-log path to bubble the error up.
2. Add a focused integration test under `tests/cli/`.
3. Run `cargo test -p orchestrator-cli`.

## Tests
- `daemon_start_missing_root_exits_nonzero` — spawn the CLI with a
  bogus project-root, assert exit code != 0.

## Out of scope
- Cleanup logic for partially-spawned daemon. Filed as TASK-1006.
```

### `implement` phase (Sonnet, ~3min)

Edits the files, adds the test, makes 2 focused commits, runs
`cargo check -p orchestrator-cli` to verify the crate compiles.
Outputs:

```json
{
  "summary": "Propagated daemon start errors to the CLI exit code and added a regression test.",
  "changed_files": [
    "crates/orchestrator-cli/src/services/operations/ops_daemon.rs",
    "tests/cli/daemon_start.rs"
  ],
  "validation_commands": ["cargo check -p orchestrator-cli"],
  "commit_message": "fix(daemon): exit non-zero when start_daemon fails"
}
```

### `review` phase (Codex, ~60s)

Runs `git diff main...HEAD` and produces:

```json
{
  "verdict": "approve",
  "summary": "The fix correctly propagates the error. The regression test asserts both stderr content and exit code. One p2 nit on test naming consistency — left for follow-up.",
  "findings": [
    {
      "severity": "p2",
      "file": "tests/cli/daemon_start.rs",
      "line": 12,
      "summary": "Test name `daemon_start_missing_root_exits_nonzero` is verbose. Existing tests in this file use shorter names. Not blocking."
    }
  ]
}
```

### `test` phase (deterministic, depends on suite)

Runs `cargo test --workspace`. Exit code 0 means the gate passes.

### `finalize` phase (Haiku, ~10s)

Since review = approve and tests = pass:

```json
{
  "outcome": "pr_opened",
  "pr_url": "https://github.com/you/your-repo/pull/247"
}
```

The PR is open. You review and merge.

If review had been `request_changes` the finalize output would be:

```json
{
  "outcome": "blocked_by_review",
  "blocking_findings": [
    "Missing test for the empty-project-root case",
    "Error wrapping loses the original io::Error context"
  ]
}
```

…and the task subject would be set to `blocked` (when dispatched via
`--task-id`). The PR would NOT be opened. Read the `outcome` field to
discriminate — finalize is the terminal phase, so its
`phase_decision.verdict` is always `advance` on a successful run
regardless of whether the run produced a PR or hit a block.

## What you get

The `implement-task` workflow has six phases:

| Phase | What it does |
|---|---|
| `research` | Scans the codebase for relevant files using grep/rg/find. Returns JSON with `relevant_files`, `approach_sketch`, `risks`. Read-only. Cheap model (Haiku). |
| `plan` | Writes a detailed implementation plan with file paths, line ranges, ordered steps, and tests to add. Returns Markdown. Read-only. Strong model (Sonnet). |
| `implement` | Edits files, adds tests, makes focused commits. The only phase that mutates the working tree. Strong model (Sonnet). |
| `review` | Independent diff review on a different model family (Codex). Returns `approve` or `request_changes` with structured findings. Read-only. |
| `test` | Deterministic gate. Runs `cargo test --workspace` (or your test command). No LLM. |
| `finalize` | If review approved + tests green: `git push` + `gh pr create`. Otherwise: mark task `blocked`. Never auto-merges. |

Outputs land under `~/.animus/<repo-scope>/runs/<workflow-id>/` like any
other Animus workflow. Each phase's structured output is captured
separately for audit.

## Honest constraints

This pack is **the official engineering reference**, not a turnkey
product. Things it doesn't do today:

- **Does NOT auto-merge.** Finalize stops at "PR opened". A human
  reviews and merges. This is a deliberate choice — auto-merge from
  an autonomous agent is the kind of thing that ships
  `rm -rf /` to prod. If you want auto-merge for low-risk paths,
  wire it yourself in `task-finalizer`.
- **Does NOT handle merge conflicts.** If the branch conflicts at PR
  time, you rebase manually. The ao-cli repo has a
  `rebase-and-retry` workflow in `.animus/workflows/common.yaml` — fork
  the pattern if you need it.
- **Does NOT preserve long-running context.** Each task is independent.
  There's no cross-task memory ("we made the same mistake 3 tasks ago"
  is not modelled — yet).
- **Does NOT understand your codebase deeply on the first run.** The
  first few tasks will produce average plans because the researcher
  starts from grep. Plans improve substantially once you wire any
  project memory primitive (a tools/MCP server the implementer can
  query, like a gbrain index or a code-search MCP). Configure it via
  the agent's `mcp_servers` list — see [`docs/customizing.md`](docs/customizing.md).
- **Cost is real.** Sonnet for plan + implement + Codex for review
  runs about $0.50-$2.00 per task depending on size. For high-volume
  backlogs, swap the implementer to Kimi or GLM (see
  [`docs/customizing.md`](docs/customizing.md)).

If you build any of the above on top of this pack, please open a PR.
The intent is that this pack stays as the canonical engineering loop
and grows in clarity over time.

## Customize it

This pack is meant to be forked. The three files you'll edit most:

- **`workflows/implement-task.yaml`** — swap models per phase, add/remove
  phases, change branch / PR target, wire a rework loop. See
  [`docs/customizing.md`](docs/customizing.md) for the full map.
- **`subjects/sample-tasks.md`** — replace with your own backlog once
  you've validated the pipeline. Or, swap the whole subject backend to
  Linear (`launchapp-dev/animus-subject-linear`) so tasks come from
  your real tracker.
- **`scripts/setup.sh`** — if you have repo-specific install steps
  (custom test runner, custom branch convention), put them here so the
  rest of the team gets them on first run.

For deeper changes — swapping the LLM per phase, wiring to Linear,
adjusting the human-review gate, running a self-healing rework loop —
see [`docs/customizing.md`](docs/customizing.md).

## Onboarding

Brand new to Animus? Start with
[`docs/onboarding.md`](docs/onboarding.md). It assumes zero prior
knowledge and walks through 15 minutes of clicks to a working PR.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page
diagram of how a task flows through the phases and which plugins are
involved.
