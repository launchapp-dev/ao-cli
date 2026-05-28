# Onboarding: Drop Animus into Your Existing Repo

This doc assumes **zero prior Animus knowledge**. If you have an
existing codebase and want to try the autonomous engineer loop on it,
this is the entry point.

Time required: 15 minutes for the first task. Longer if you want to
tune models or wire it to Linear.

## What you're getting

A workflow that picks up one task description from a markdown file,
researches your codebase, drafts an implementation, runs your tests,
gets a second LLM to review the diff, and opens a PR for you. It stops
at human review — it never auto-merges.

The pack is structurally identical to what we use to drive ao-cli's
own development. You're not getting a toy demo — you're getting our
production workflow, packaged as a starter.

## Prerequisites

1. **A git repo.** Any language, any size. The default test phase runs
   `cargo test --workspace`, so a Rust workspace works out of the box;
   for other ecosystems you'll edit one YAML field — see step 5.
2. **`animus` CLI** on your PATH. Install:
   ```bash
   curl -fsSL https://animus.sh/install | bash
   ```
3. **`gh` CLI** authenticated against your GitHub remote. Animus shells
   out to `gh` to open the PR — there's no in-process GitHub client.
   ```bash
   gh auth login
   ```
4. **LLM API keys** for Claude and OpenAI:
   ```bash
   export ANTHROPIC_API_KEY=...   # for plan/implement/research/finalize
   export OPENAI_API_KEY=...      # for the codex reviewer
   ```
   Both keys are required by default. To run with only one provider,
   see `docs/customizing.md` → "Swap the LLM provider per phase".

## Step 1: Copy the pack into your repo

The pack lives under `packs/engineering-backlog/` in the ao-cli repo.
Copy that whole directory into your repo at the same path. You don't
need to track every file — `pack.toml`, `workflows/`, and
`scripts/setup.sh` are the minimum.

```bash
# From your repo root
mkdir -p packs
cp -r /path/to/ao-cli/packs/engineering-backlog packs/engineering-backlog
```

## Step 2: Run the setup script

```bash
bash packs/engineering-backlog/scripts/setup.sh
```

This does three things idempotently:

1. Installs the three plugins this pack depends on:
   - `animus-subject-default` — provides the `task` subject kind
   - `animus-provider-claude` — runs Claude for plan/implement/research
   - `animus-provider-codex` — runs Codex for cross-model review
2. Copies `workflows/implement-task.yaml` into `.animus/workflows/`
   (your repo's workflow overlay).
3. Seeds `tasks/backlog/TASK-100[1-5].md` from the bundled sample
   tasks. The sample tasks are about ao-cli — feel free to delete them
   or leave them as filler.

If the install fails on a "trust-on-first-use" prompt for the
`launchapp-dev` publisher, follow the on-screen instructions to add
the publisher to your trusted list.

## Step 3: Write your first task

Drop a markdown file in `tasks/backlog/`:

```bash
cat > tasks/backlog/TASK-0001.md <<'EOF'
## TASK-0001: <short title>
status: ready
priority: med
task_type: feature

<description — what to build, acceptance criteria, anything the
implementer should know>
EOF
```

The file stem (`TASK-0001`) is the task id. The `## TASK-0001: <title>`
heading is what the workflow phases see as the task title.

Sanity-check the subject backend sees it:

```bash
animus subject list --kind task
# should show TASK-0001 and any sample tasks left from setup
```

## Step 4: Start the daemon

```bash
animus daemon start --auto-install --project-root "$(pwd)"
```

`--auto-install` lets the daemon install any missing plugins on the
fly. After this, in another terminal you can watch live activity:

```bash
animus daemon events --follow
```

## Step 5: Adjust the test command (non-Rust projects only)

The pack defaults to `cargo test --workspace`. If you're not on Rust,
edit `.animus/workflows/implement-task.yaml`, find the `test:` phase,
and change the `command:` block:

```yaml
test:
  mode: command
  command:
    cwd_mode: task_root
    program: npm           # or pytest, go, make, etc.
    args:
      - test
    timeout_secs: 600
    success_exit_codes:
      - 0
```

## Step 6: Run the workflow

`animus workflow run --task-id` resolves through the built-in task
adapter, which doesn't see plugin-backed markdown tasks in a fresh
project. The verified-working dispatch path is `--title +
--description` (the workflow phases see the task content inline):

```bash
# Switch to a feature branch FIRST. Title-dispatch runs in your
# checkout, so without this the implement phase commits land on
# whichever branch you're currently on (often main).
git switch -c ao/smoke-onboarding-001

TASK_FILE="tasks/backlog/TASK-0001.md"
animus workflow run animus.engineering-backlog/implement-task \
  --title "$(head -n 1 "$TASK_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$TASK_FILE")" \
  --sync \
  --project-root "$(pwd)"
```

End-to-end `--task-id` dispatch (so finalize can mutate the task
status) is NOT available with the plugin-backed default setup today
— `animus subject create` writes a plugin task, but
`animus workflow run --task-id` resolves through the built-in task
adapter and can't see it. Stick with the `--title` path until the
missing `animus workflow run --subject-id` primitive ships. See the
README's "Dispatching tasks — current limitation" section for the
full rationale.

`--sync` makes the CLI wait for completion. Drop it to dispatch and
return immediately; the daemon will run the workflow in the
background.

Expected timeline for a small task:

| Phase     | Wall-clock (typical) |
|-----------|----------------------|
| research  | 10-30s               |
| plan      | 30-60s               |
| implement | 1-5 min              |
| review    | 30-90s               |
| test      | depends on your suite |
| finalize  | 5-15s                |

## Step 7: Read the output

Find the workflow id from the dispatch output, then:

```bash
animus output phase-outputs --workflow-id <id>
```

You'll see one JSON file per phase. The interesting ones:

- `plan.json` — the Markdown implementation plan
- `review.json` — the Codex reviewer's verdict and findings
- `finalize.json` — the PR url if review approved + tests green,
  otherwise the reason the task was marked blocked

Watch it live with:

```bash
animus output monitor --run-id <run-id>
```

## Step 8: Review the PR

Finalize is the terminal phase, so its `phase_decision.verdict` is
ALWAYS `advance` on a successful workflow run — even when the run
ended in a blocked task. The machine-readable terminal discriminator
is the `outcome` field in the finalize phase output:

| `outcome` value          | Meaning                                  |
|--------------------------|------------------------------------------|
| `pr_opened`              | PR exists. Look at `pr_url`.             |
| `blocked_by_review`      | Codex requested changes. See `blocking_findings`. |
| `blocked_by_tests`       | `cargo test` failed. See `test_failure_summary`. |
| `skipped_branch_safety`  | Title-dispatch ran on `main`/`master`/`develop` and the safety gate refused to push. Switch to a feature branch and rerun. |

Read it from disk:

```bash
animus output phase-outputs --workflow-id <id> --phase-id finalize
# look for "outcome": "..." in the result JSON
```

If `outcome: pr_opened`, you'll have a PR open against `main`
(configurable — see `docs/customizing.md`). Review it like any other
PR. The branch is `ao/task-TASK-0001` by convention.

If `outcome` is one of the `blocked_*` values, the task subject is
set to `blocked` (when dispatched via `--task-id`; the title-dispatch
path notes "skipped subject update" in the verdict). Read the
findings/summary in the finalize phase output to see what went wrong.
Common moves:

- Edit the task description with more constraints, set status back to
  `ready`, re-run.
- Pin the implementer to a different model (see `docs/customizing.md`
  → "Common swap: cheaper implementer").
- Edit the plan manually (read `plan.json`, write a follow-up task
  with the corrected approach as the description).

## What to do next

Once you have one task running end-to-end:

1. **Wire it to your real backlog.** Replace
   `animus-subject-default` with `animus-subject-linear` (or roll
   your own — see `docs/customizing.md`). The workflow YAML doesn't
   change.
2. **Add a schedule.** Run the workflow on a cron so tasks get picked
   up automatically (`docs/customizing.md` → "Wire up a schedule").
3. **Tune the models.** Default is Claude for everything except
   review. Swap based on cost / quality preferences per phase.
4. **Read `docs/architecture.md`.** Understand why each phase is
   structured the way it is so you can extend it without breaking the
   loop's correctness story.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `animus subject list --kind task` returns empty | Setup didn't seed files, or `animus-subject-default` isn't pointing at `tasks/backlog/`. Check the plugin's config or re-run setup. |
| Daemon won't start, complains about missing plugins | Re-run `animus daemon start --auto-install` or `animus plugin install-defaults`. |
| `gh pr create` in finalize fails with "not authenticated" | The daemon inherits the env it was started in. Restart the daemon from a shell where `gh auth status` succeeds. |
| Codex review phase says "model not available" | Check `OPENAI_API_KEY` is set in the daemon's env. Reset by stopping the daemon, exporting the key, restarting. |
| Implement phase produces no commits | The plan was too vague or the implementer hit an unrelated tool error. Read `implement.json` for the residual_risks + validation_commands the agent recorded. |

For deeper Animus issues, see
[`docs/guides/troubleshooting.md`](../../../docs/guides/troubleshooting.md).
