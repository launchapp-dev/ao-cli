# TASK-018 UX Brief: Web GUI CI, Smoke E2E, and Release Gates

## Phase
- Workflow phase: `ux-research`
- Workflow ID: `7106cb2f-5f67-42eb-8897-06ad9dac43d4`
- Task: `TASK-018`
- Project root: `/Users/samishukri/ao-cli`

## Inputs and Scope Basis
- Source requirements:
  - `crates/orchestrator-web-server/docs/task-018-web-gui-ci-e2e-release-gates-requirements.md`
- Source user flows and wireframes:
  - `mockups/task-018-web-gui-ci-e2e-release-gates/README.md`
  - `mockups/task-018-web-gui-ci-e2e-release-gates/wireframes.html`

This brief covers operator experience for:
- frontend CI matrix visibility,
- smoke E2E failure triage,
- release go/no-go checklist decisions,
- rollback validation confidence.

## UX Objective
Design a deterministic release-gating experience that lets operators answer three
questions quickly:
1. Did web GUI checks pass?
2. If not, where is the failure evidence?
3. Is rollback confidence validated before publish decisions?

The workflow experience must keep pass/fail status explicit, preserve audit
evidence, and minimize ambiguity across pull request, release, and rollback
validation paths.

## Primary Users and Jobs

| User | Primary jobs | UX success signal |
| --- | --- | --- |
| PR author | Confirm web GUI checks pass before merge | Can identify matrix + smoke status from checks list in <= 2 clicks |
| Release operator | Make go/no-go decision on release runs | Can verify all required gate evidence from one checklist and one release run |
| On-call responder | Validate rollback candidate quickly during incidents | Can run rollback validation and compare candidate vs rollback outcomes in one summary view |

## UX Principles for This Phase
1. Clarity first: pass/fail state and blocking dependencies are visible at top of each screen.
2. Deterministic naming: workflows, jobs, and artifacts use stable labels for quick lookup.
3. Evidence over intuition: every gate decision links to specific run output or checklist evidence URL.
4. Progressive disclosure: summary first, diagnostics/details on demand.
5. Accessible outcomes: status must be understandable by text alone, not color-only signaling.

## Information Architecture

### Primary Operator Entry Points
1. Pull request Checks tab.
2. `web-ui-ci.yml` workflow run summary.
3. `release.yml` run summary (`web-ui-gates` prerequisite).
4. `release-rollback-validation.yml` manual dispatch form and run summary.
5. `.github/release-checklists/web-gui-release.md` checklist artifact.

### Required Evidence Artifacts
1. Web UI matrix results (Node `20.x`, `22.x`).
2. Smoke E2E pass/fail output and assertion report (`smoke-assertions.txt`).
3. Smoke failure logs (stdout/stderr + assertion report).
4. Release gate completion status before build/publish jobs.
5. Rollback validation summary for `candidate_ref` and `rollback_ref`.

## Key Screens, States, and Interactions

| Screen | Goal | Primary interactions | Required states |
| --- | --- | --- | --- |
| PR Checks list | Decide merge readiness for web GUI changes | Open required checks, inspect failed check, jump to run details | pending, success, failure, cancelled |
| `web-ui-ci.yml` run summary | Verify matrix and smoke coverage | Expand matrix jobs, open smoke step logs, download failure artifacts | queued, running, passed, failed |
| `release.yml` run summary | Confirm release is blocked unless web gates pass | Inspect `web-ui-gates` status, confirm build jobs gated by dependency graph | blocked-by-gates, running, passed, failed |
| Web GUI release checklist (`.md`) | Record auditable go/no-go decision | Fill checklist fields, attach evidence URLs, record decision notes | draft, ready-for-go, blocked, signed-off |
| Rollback validation dispatch form | Launch deterministic candidate vs rollback smoke validation | Enter `candidate_ref`, enter `rollback_ref`, trigger run | idle, validation-error, submitted |
| Rollback validation run summary | Compare candidate and fallback confidence | Read per-ref outcome, inspect logs/artifacts, copy summary into incident/release notes | running, candidate-failed, rollback-failed, both-passed |

## Critical User Flows (From Requirements + Wireframes)

### Flow A: Pull Request Gate Triage
1. PR author opens Checks tab after pushing web GUI changes.
2. Author confirms `web-ui-ci` required checks have completed.
3. On failure, author opens failing job and checks smoke assertion output and uploaded logs.
4. Author applies fix and re-runs until required checks are green.

### Flow B: Release Go/No-Go Decision
1. Release operator opens `release.yml` run for `v*` tag or `version/**` branch.
2. Operator verifies `web-ui-gates` succeeded before any publish path proceeds.
3. Operator updates release checklist with CI run URLs and explicit decision notes.
4. Publish proceeds only when checklist evidence and required jobs are both green.

### Flow C: Smoke Failure Diagnosis
1. Smoke step fails in CI or release gates.
2. Operator downloads deterministic failure artifacts.
3. Operator reviews route/API assertion output and server stdout/stderr logs.
4. Operator records blocker status and links evidence in checklist or incident notes.

### Flow D: Rollback Validation Confidence
1. Operator triggers `release-rollback-validation.yml` manually.
2. Inputs `candidate_ref` and `rollback_ref`.
3. Workflow runs smoke checks for both refs and emits side-by-side summary.
4. Operator uses summary to confirm rollback readiness without mutating tags/releases.

## Interaction Contracts

| Interaction | Expected behavior | Recovery affordance |
| --- | --- | --- |
| Open failed smoke step | Show failing assertion label, route/API context, and artifact pointers within one scroll view | Keep deterministic assertion names and link to uploaded smoke artifacts |
| Download smoke artifacts | Provide stable artifact names for logs and assertions | Retain for finite window (`retention-days: 7`) and document names in checklist |
| Complete release checklist | Require evidence URL fields before go/no-go sign-off | Block "ready-for-go" state until mandatory evidence slots are filled |
| Run rollback validation | Keep candidate and rollback outputs separated with explicit headings | Fail run if either ref fails and include both outcomes in summary |

## Layout, Hierarchy, and Spacing Guidance

### Checklist Authoring Structure
- Keep sections in this order: Metadata -> Preflight -> CI Gate Evidence ->
  Decision -> Rollback Readiness -> Post-release Verification.
- Use short labels and one evidence URL slot per required gate item.
- Keep line lengths moderate so checklist remains readable in narrow viewports.

### Responsive Readability
- Avoid wide multi-column tables in step summaries; prefer bullet lists and short key-value lines.
- Keep artifact names concise and predictable to reduce truncation on mobile GitHub views.
- Ensure important statuses appear near the top of each summary to avoid long-scroll hunting.

## Accessibility Constraints (Non-Negotiable)
1. All statuses include explicit text (`passed`, `failed`, `blocked`, `cancelled`) and never rely on color only.
2. Checklist and summary headings use ordered levels with no skipped structure.
3. Checkbox and control labels are descriptive out of context for screen reader users.
4. Evidence links include meaning in adjacent label text (what the evidence proves).
5. Run summaries remain plain-text legible without screenshots or custom styling.
6. `workflow_dispatch` field help text must clearly distinguish `candidate_ref` vs `rollback_ref`.
7. Failure guidance includes concrete next action language (`inspect logs`, `rerun smoke`, `block release`).
8. Keyboard navigation must reach all actionable elements with visible focus treatment.
9. Mobile layout at `320px` must avoid horizontal scrolling for primary triage and checklist flows.
10. Status announcements and updates should be compatible with polite live-region behavior.

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Gate status is hard to locate | Incorrect merge/release decisions | Stable required-check names and ordered summary sections |
| Smoke failures lack context | Slow recovery and reruns | Upload deterministic logs plus assertion report on failure |
| Checklist becomes stale or incomplete | Lost release auditability | Mandatory evidence URL slots and explicit decision section |
| Rollback refs are entered incorrectly | False confidence in rollback readiness | Clear input labels/descriptions and per-ref summary headings |

## Requirements Traceability (UX to Acceptance Criteria)

| Acceptance Criterion | UX coverage in this brief |
| --- | --- |
| `AC-01`, `AC-02`, `AC-03` | PR checks and `web-ui-ci.yml` run summary screens with matrix + smoke interactions |
| `AC-04`, `AC-05` | Smoke failure triage flow and interaction contracts with assertion-level diagnostics |
| `AC-06`, `AC-07`, `AC-08` | Release workflow gate topology screen and go/no-go decision flow |
| `AC-09` | Release checklist screen, hierarchy, and mandatory evidence fields |
| `AC-10`, `AC-11` | Rollback dispatch + summary screens with per-ref outcomes and fail-closed behavior |
| `AC-12` | Failure artifact interaction contract and deterministic evidence artifacts section |

## UX Acceptance Checklist for Implementation Phase
- PR checks clearly expose web UI matrix and smoke outcome as required gates.
- Release workflow communicates that `web-ui-gates` is blocking build/publish.
- Smoke failure artifacts are easy to locate and identify from run logs.
- Release checklist supports explicit, auditable go/no-go decisions with evidence URLs.
- Rollback validation workflow input form and summary make candidate vs rollback outcomes unambiguous.
- Summary and checklist content remain readable and actionable on narrow/mobile GitHub layouts.
