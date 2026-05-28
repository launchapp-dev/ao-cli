# Customizing the Engineering Backlog Pack

This pack is meant to be forked. Here's the map of the most common
changes teams make once they have the basic loop running.

## Swap the LLM provider per phase

Each agent block in `workflows/implement-task.yaml` declares `model` and
`tool`. Change them per agent and reinstall the matching provider plugin.

### Common swap: cheaper implementer

The default implementer is Claude Sonnet. Teams running high task volume
often swap to a cheaper open-weights model (Kimi K2.5 has been the
ao-cli team's go-to for refactors and bug fixes — see
`.animus/workflows/common.yaml` in the ao-cli repo for the routing
matrix). Edit `task-implementer`:

```yaml
agents:
  task-implementer:
    model: kimi-k2-5
    tool: oai-runner
```

Then install the OAI runner (it speaks to OpenAI-compatible endpoints,
which covers MoonshotAI's Kimi, Z.ai's GLM, and others):

```bash
animus plugin install launchapp-dev/animus-provider-oai --allow-shadow-builtin
export KIMI_API_KEY=...   # or whichever provider you point at
```

### Use OpenAI everywhere

```yaml
agents:
  task-researcher:
    model: gpt-4o-mini
    tool: oai
  task-planner:
    model: gpt-4o
    tool: oai
  task-implementer:
    model: gpt-4o
    tool: oai
  diff-reviewer:
    model: gpt-5-codex-high   # keep codex as the cross-model reviewer
    tool: codex
  task-finalizer:
    model: gpt-4o-mini
    tool: oai
```

Note: even when you swap everything else to OpenAI, **keep the reviewer
on a different family** (Codex/Claude). Same-family review defeats the
"two models catch different things" purpose of the phase.

### Mix providers per phase

Phases are independent agents — mix freely. A common pattern:

| Phase     | Why this model                                              |
|-----------|-------------------------------------------------------------|
| research  | Haiku — cheap, structured output, lots of grep calls        |
| plan      | Sonnet — needs to reason about edges + tradeoffs            |
| implement | Sonnet or Kimi — heavy code lifting                         |
| review    | Codex — independent of Claude family                        |
| finalize  | Haiku — short decision tree, no code                        |

Set the env vars for every provider you reference:

```bash
export ANTHROPIC_API_KEY=...
export OPENAI_API_KEY=...
```

## Skip the review phase

Some teams have heavy CI review and don't want the extra LLM round-trip.
Edit the workflow's `phases` list:

```yaml
workflows:
  - id: animus.engineering-backlog/implement-task
    phases:
      - research
      - plan
      - implement
      - test
      - finalize    # finalize will see no review verdict — see below
```

You'll also need to update the `task-finalizer` system prompt — without
a review verdict, finalize just decides based on test pass/fail.
Replace the decision tree with:

```
If tests = pass: push + open PR.
If tests = fail: mark task blocked.
```

## Require manual approval before finalize

To gate the PR opening on a human's explicit OK (e.g. for high-risk
codebases), insert a manual phase between `review` and `test`:

```yaml
phases:
  human-gate:
    mode: manual
    directive: |
      Review the diff and the reviewer's verdict. Approve to continue
      to the test phase and PR opening, or reject to mark the task
      blocked.
    manual:
      instructions: |
        Inspect `git diff`. If you want this to ship as a PR, approve.
        Otherwise reject and the task will be marked blocked for follow-up.
      approval_note_required: false

workflows:
  - id: animus.engineering-backlog/implement-task
    phases:
      - research
      - plan
      - implement
      - review
      - human-gate    # <-- new
      - test
      - finalize
```

This is what `.animus/workflows/common.yaml` does with its `testing`
phase in the `full` workflow. The daemon stops the run and waits for
`animus workflow phase approve --id <workflow-id> --phase <phase-id>`
(or the equivalent MCP call `animus.workflow.phase.approve`).

## Change branch / PR target / PR template

Edit the `task-finalizer` system prompt directly — it currently calls
`gh pr create --fill --base "$BASE"`, where `$BASE` is discovered from
the repo's default branch (typically `main` or `develop`). The dynamic
discovery is the safer default — leave it intact and add extras like
`--draft` / reviewers / labels around it:

```
Default decision tree:
  ...
  1. `git push -u origin HEAD`
  2. `gh pr create --fill --base "$BASE" --draft \
       --reviewer @your-team`
  3. Call `animus.subject.update --kind task --id <id> \
     --labels awaiting-human-review`.
  4. Emit verdict `advance` with the PR url.
```

If your team always wants PRs against a specific branch (not the repo
default), you CAN replace `"$BASE"` with a literal — just be aware that
this overrides the safer dynamic-discovery behavior for every repo this
pack runs against.

For a PR template, drop a `.github/pull_request_template.md` in the
target repo. `gh pr create --fill` picks it up automatically.

## Swap the task source (markdown → Linear)

The pack uses `animus-subject-default` for file-backed tasks. The
workflow YAML is source-agnostic — it references the `task` subject
kind without caring where tasks come from.

### Linear

```bash
animus plugin install launchapp-dev/animus-subject-linear
export LINEAR_API_TOKEN=...
```

Once installed, Linear claims the `task` kind. `animus subject list
--kind task` returns Linear issues. The workflow YAML does not change.

If `task` is already claimed by `animus-subject-default`, you have two
options:

1. **Replace** — uninstall the default plugin, install Linear. The kind
   `task` now resolves to Linear.
2. **Coexist** — keep the default registered as `task`, install Linear
   as `linear.task`. Run two workflows side by side, or change the
   workflow's subject kind. (Subject kind is resolved at dispatch time
   via `animus subject` CLI flags or `default_subject_kind` in
   `.animus/config.json`.)

### GitHub Issues

Not yet shipped as a first-party plugin. The shape is straightforward —
a subject backend that maps `task/list` → `gh issue list`, `task/get` →
`gh issue view`, `task/status` → label / state mutations. If you build
this, please open a PR — it's a frequent ask.

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol).
Useful methods to implement: `task/list`, `task/get`, `task/status`,
`task/update`. See
[`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language
that can speak JSON-RPC over stdio).

## Add/remove phases

Phases are independent units of work. To add a phase (e.g. a lint
gate between implement and review), declare it in `phases:` then add
it to the workflow's `phases:` list:

```yaml
phases:
  lint:
    mode: command
    directive: Run clippy as a deterministic gate.
    command:
      cwd_mode: task_root
      program: cargo
      args:
        - clippy
        - --workspace
        - --all-targets
        - --
        - -D
        - warnings
      timeout_secs: 600
      success_exit_codes:
        - 0

workflows:
  - id: animus.engineering-backlog/implement-task
    phases:
      - research
      - plan
      - implement
      - lint        # <-- new
      - review
      - test
      - finalize
```

The ao-cli repo's `.animus/workflows/common.yaml` runs lint + unit-test
both with `on_verdict.rework.target: implementation` so failures route
back to a fixer loop. You can do the same here — see "Wire a rework
loop" below.

## Wire a rework loop

By default this pack stops at the first failure (review request_changes
or test fail → task `blocked`). For a self-healing loop, route failures
back to the implement phase:

```yaml
workflows:
  - id: animus.engineering-backlog/implement-task
    phases:
      - research
      - plan
      - implement
      - review:
          on_verdict:
            rework:
              target: implement
      - test:
          on_verdict:
            rework:
              target: implement
      - finalize
```

Combined with `phase_rich_config.max_rework_attempts`, this caps the
loop so a stuck task can't burn the budget. The canonical
`.animus/workflows/common.yaml` `review-cycle` workflow is the
reference.

## Run different model tiers per task type

The ao-cli repo routes tasks to different implementation workflows
based on `task_type` (bug / feature / refactor / docs). Pattern:

1. Add a `triage` phase before `research` that reads `task_type` and
   sets a `workflow_ref` for the rest of the run.
2. Define multiple workflow ids — `implement-task-bug`,
   `implement-task-refactor` — that differ only in the implementer's
   model.

See `.animus/workflows/common.yaml` `triager` agent for the canonical
routing matrix.

## Wire up a schedule

To pick up the next ready task automatically:

```yaml
schedules:
  - id: implement-next-task
    cron: "*/15 * * * *"
    workflow_ref: animus.engineering-backlog/implement-task
    enabled: true
```

This needs a dispatcher agent that runs `animus subject next --kind
task` and feeds the result into a workflow run. The `work-planner`
agent in `.animus/workflows/common.yaml` is the reference
implementation — it scans the backlog, dedupes against the queue, and
enqueues ready tasks.

## Constrain the implementer to specific paths

For codebases with a sensitive subtree (auth, billing, infra):

```yaml
agents:
  task-implementer:
    skills:
      - freeze       # restricts Edit/Write to allowed paths
    system_prompt: |
      ...standard prompt...

      You are scoped to crates/{public,api,handlers}/ only. Refuse
      tasks that require edits outside this tree and emit a verdict
      `rework` with reason `out_of_scope`.
```

The `freeze` skill is a gstack pattern but the principle applies to
any scoping mechanism — the agent just needs to know what it can and
can't touch.
