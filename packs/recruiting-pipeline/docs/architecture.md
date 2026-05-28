# Architecture: Recruiting Pipeline Pack

A 1-pager on how this pack composes existing Animus primitives. No new Rust
code is required — only workflow YAML + agent prompts + a subject backend
that knows how to read candidate records.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads candidates as markdown files from a directory. Routes the `candidate` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the enricher / screener / brief-writer / synthesizer agents. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets recruiters and interviewers browse briefs and debriefs in a web UI instead of `animus output tail`. |

Install all of the above in one go:

```bash
animus plugin install-defaults --include-subjects --include-transports
animus plugin install launchapp-dev/animus-subject-markdown
# Provider plugins claim reserved tool names (claude, oai, gemini),
# so installation needs --allow-shadow-builtin to acknowledge the
# shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-claude --allow-shadow-builtin
```

(`scripts/setup.sh` runs the install commands for you.)

## How a candidate flows

### Screen path

```
+-----------------------+         +-----------------------+
| candidates/inbox/     |         | animus-subject-       |
|   CAND-*.md           |  --->   | markdown (plugin)     |
+-----------------------+         | kind=candidate        |
                                  +-----------+-----------+
                                              |
                                              |  list / get / status
                                              v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.recruiting-pipeline/screen-candidate    |
|     (per candidate subject)                                   |
|                                                              |
|   phase: screen_enrich         ---> JSON { open_source,      |
|         agent: Claude Sonnet         talks_writing,          |
|                                       github_trail, ... }    |
|                                                              |
|   phase: screen_against_rubric ---> JSON { skill_match,      |
|         agent: Claude Sonnet         level_fit,              |
|                                       motivation_signals,    |
|                                       risk_flags, ... }      |
|                                                              |
|   phase: draft_interview_brief ---> JSON { candidate_context |
|         agent: Claude Sonnet         areas_to_probe,         |
|                                       suggested_questions,   |
|                                       items_to_verify,       |
|                                       recommended_next }     |
|                                                              |
|   phase: screen_flag_for_review --> subject status update    |
|         agent: Claude Sonnet, mutates_state=true             |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Recruiter reads      |
            | brief, decides       |
            | whether to advance.  |
            | Hiring manager preps |
            | from the brief.      |
            +----------------------+
```

### Debrief path

```
+----------------------+         +-----------------------+
| candidate file with  |         | animus-subject-       |
| raw interview notes  |  --->   | markdown (plugin)     |
+----------------------+         +-----------+-----------+
                                             |
                                             v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.recruiting-pipeline/debrief-synthesis   |
|                                                              |
|   phase: debrief_collect    ---> JSON { interviewers: [...] }|
|         agent: Claude Sonnet                                 |
|                                                              |
|   phase: debrief_synthesize ---> JSON { weighted_scores,     |
|         agent: Claude Sonnet     agreement_level,            |
|                                   recommendation,            |
|                                   confidence, rationale,     |
|                                   committee_questions }      |
|                                                              |
|   phase: debrief_flag       ---> subject status update       |
|         agent: Claude Sonnet, mutates_state=true             |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Hiring committee     |
            | reads synthesis,     |
            | makes the call.      |
            +----------------------+
```

## Where outputs land

Standard Animus paths — nothing pack-specific:

- Run events / artifacts: `~/.animus/<repo-scope>/runs/<run-id>/`
- Per-phase JSON output: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/<phase-id>.json`
- Workflow snapshots: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/`

Stream in real time with `animus output monitor --run-id <run-id>`. Pull a
structured per-phase snapshot with `animus output phase-outputs
--workflow-id <workflow-id>`.

## The constraint, encoded in agent prompts

The "no hiring decisions, no candidate comm" constraint is enforced at the
agent-prompt layer, not at the runtime layer:

- The `rubric-screener` prompt explicitly says "You DO NOT make a hire /
  no-hire call. You produce evidence..."
- The `interview-brief-writer` `recommended_next` field is a routing
  recommendation for the *recruiter* — not a candidate-facing action.
- The `debrief-synthesizer` produces a `recommendation` field, but the
  prompt says "You produce a RECOMMENDATION. The hiring committee makes
  the DECISION. If interviewers disagreed strongly or evidence was thin,
  recommend no-decision rather than forcing a call."
- The `recruiter-handoff` and `debrief-handoff` prompts both say "You DO
  NOT decide... You DO NOT send messages to the candidate."

The phase ids in both workflows are also namespaced (`screen_*` and
`debrief_*` prefixes). Animus merges all `.animus/workflows/*.yaml`
files into one global `phase_definitions` map keyed by phase id, so an
unnamespaced `flag` or `enrich` phase in this pack would silently
overwrite (or be overwritten by) a colliding phase in another
project workflow. Keep the prefixes when forking.

This is intentional. Hiring is high-stakes and asymmetric — a missed
strong candidate costs a recruiter cycle; an auto-sent rejection that
should have been a screen costs a reputation. Animus stops at the human
review gate.

## Why this pack matters architecturally

This is the second non-engineering Animus reference pack (after
customer-support) and the first that demonstrates:

1. **Multi-workflow pipelines per subject kind.** `candidate` flows
   through `screen-candidate` early in the loop and
   `debrief-synthesis` after the interviews. Same subject, different
   workflows, same backend.
2. **Structured-output-heavy LLM phases.** Every phase produces a
   typed JSON contract that the next phase consumes. No mode: command,
   no shell, no git ops.
3. **Honest "human owns the decision" framing.** The constraint is
   encoded in prompts AND surfaced in the recommendation enum (the
   `no-decision` value in debrief synthesis is a first-class output).

This pattern (subject backend + LLM-only multi-workflow pipeline +
honest human handoff) generalizes to grant review, due-diligence,
investment screening, and any other high-stakes evaluation flow.
