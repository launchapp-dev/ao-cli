# TASK-015 Mockup Review: Run Telemetry Stream and Artifact Inspector

## Phase
- Workflow phase: `mockup-review`
- Workflow ID: `45cbf98a-2acd-4a11-8f17-a0c052c079d6`
- Task: `TASK-015`

## Scope of Review
Reviewed `TASK-015` mockup artifacts against:
- `task-015-run-telemetry-stream-artifact-inspector-requirements.md`
- `task-015-run-telemetry-stream-artifact-inspector-ux-brief.md`

Reviewed artifacts:
- `mockups/task-015-run-telemetry-artifact-inspector/wireframes.html`
- `mockups/task-015-run-telemetry-artifact-inspector/wireframes.css`
- `mockups/task-015-run-telemetry-artifact-inspector/output-inspector-wireframe.tsx`
- `mockups/task-015-run-telemetry-artifact-inspector/README.md`

## Mismatch Resolution Log

| Mismatch | Requirement/UX reference | Resolution |
| --- | --- | --- |
| Artifact panel only modeled metadata rows and per-row download, but did not explicitly represent `files` endpoint assist flow | FR-01 (`/executions/:execution_id/files`), UX IA artifact panel controls | Added artifact file-id selector board/state sourced from `/api/v1/output/executions/:execution_id/files` in HTML and React wireframe scaffold |
| Download UX implied filename preservation but did not visibly document deterministic response-header contract | FR-05 download flow + AC-02 stable headers | Added explicit download header callout (`Content-Disposition`) and live status feedback for successful binary-safe download handling |
| Path-safety validation covered run/execution IDs but artifact-id traversal handling was only implicit | FR-08 security hygiene + AC-11 unsafe id rejection | Added explicit unsafe `artifact_id` blocked state in wireframes and artifact-id validation branch in the React scaffold |
| Mobile board lacked explicit polite live-region update for connection/event progression | NFR-03 accessibility + UX accessibility rule 3 (`aria-live="polite"`) | Added mobile `aria-live` update sample and aligned it with connection/event state messaging |
| React scaffold heading hierarchy and artifact interaction semantics under-modeled route-level accessibility and retry labeling | UX accessibility rules 1/5/7 + AC-09 | Promoted route heading to `h1`, added file-id input error association, and made download action labels state-aware (`Download` vs `Retry download`) |

## Acceptance Criteria Traceability (Mockup Phase)

| AC | Evidence |
| --- | --- |
| `AC-01` | Output route structure and endpoint-oriented sections remain explicit across desktop/fallback/mobile boards and state scaffold (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-02` | Artifact download flow now includes deterministic filename/header semantics and binary-safe handling callouts (`wireframes.html`, `output-inspector-wireframe.tsx`, `README.md`) |
| `AC-03` | Stream-first telemetry path with automatic fallback states remains explicit (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-04` | Event dedup/order behavior remains represented in fallback diagnostics and `mergeRunEvents` helper (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-05` | JSONL filter controls include required query/source/task/phase/limit + reset and bounded error states (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-06` | Artifact inspector now covers metadata table plus file-id selector assist and per-artifact download actions (`wireframes.html`, `output-inspector-wireframe.tsx`, `README.md`) |
| `AC-07` | Invalid and not-found errors are rendered with actionable copy and `role="alert"` semantics (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-08` | `/output` remains modeled as a dedicated route with run and execution context controls (`wireframes.html`, `README.md`) |
| `AC-09` | Focus-visible controls, route-level `h1`, semantic list/table structures, and mobile live-region behavior are explicit (`wireframes.html`, `wireframes.css`, `output-inspector-wireframe.tsx`) |
| `AC-10` | Mockup scope remains additive to `/output`, with no modeled mutation of existing `/events` behavior (`README.md`, `output-inspector-wireframe.tsx`) |
| `AC-11` | Safe-identifier guardrails are explicit for run/execution/artifact identifiers, including traversal-style artifact-id rejection (`wireframes.html`, `output-inspector-wireframe.tsx`) |
| `AC-12` | Wireframe state transitions and failure hooks remain explicit for downstream automated coverage of fallback/filter/download paths (`output-inspector-wireframe.tsx`, `README.md`) |

## Outcome
`TASK-015` mockups now provide explicit acceptance-criteria traceability for output streaming, JSONL filtering, artifact browse/download flows, and identifier safety/accessibility behavior before implementation handoff.
