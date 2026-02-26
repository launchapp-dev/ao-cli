# TASK-015 Wireframes: Run Telemetry Stream and Artifact Inspector

Concrete wireframes for the standalone daemon web UI route
`/output`, focused on stream-first run telemetry, deterministic JSONL filtering,
and safe artifact browsing/download behavior.

## Files
- `wireframes.html`: desktop and mobile wireframe boards for `/output`.
- `wireframes.css`: shared visual system for hierarchy, spacing, focus, and responsive behavior.
- `output-inspector-wireframe.tsx`: React-oriented state and component scaffold for implementation handoff.

## Route Coverage

| Route | Covered in |
| --- | --- |
| `/output` | `wireframes.html` (`Live Stream`, `Poll Fallback`, `Mobile 320px`) + `output-inspector-wireframe.tsx` (`OutputInspectorWireframe`) |

## State Coverage
- Page state: `idle`, `loading`, `ready`, `invalid_input`, `not_found`
- Telemetry connection state: `connecting`, `live`, `polling`, `reconnecting`, `disconnected`, `stream-error`
- JSONL state: `loading`, `results`, `empty-results`, `filter-error`
- Artifact state: `loading`, `empty`, `list-ready`, `download-pending`, `download-failed`

## Data Contract Coverage in Mockups
- Run telemetry rows show `seq`, timestamp, event kind, run id, optional task/phase, and expandable raw payload.
- JSONL explorer models required filters (`query`, `source_file`, `task_id`, `phase_id`, `limit`) plus reset and active filter summary.
- Artifact inspector models both `artifacts` and `files` endpoint intent through:
  - file-id selector sourced from `/api/v1/output/executions/:execution_id/files`,
  - artifact rows with `artifact_id`, type, size, filename/path hint,
  - per-row and file-id-driven deterministic download states.

## Accessibility and Responsive Intent
- One page `h1` and ordered route sections with semantic headings.
- Connection mode is always present as text + status chip and announced via `aria-live="polite"`.
- Expand/collapse actions are modeled with `button`, `aria-expanded`, and `aria-controls`.
- Filter and context controls include explicit labels and inline error associations.
- Download actions include descriptive labels that identify the artifact.
- Mobile board is fixed to `320px` and keeps controls/content readable without page-level horizontal scroll.
- Interactive controls model a minimum `44px` target height.

## Acceptance Criteria Traceability

| AC | Wireframe trace |
| --- | --- |
| `AC-01` | Output route structure and endpoint-oriented sections represented in `wireframes.html` and `.tsx` state model |
| `AC-02` | Artifact panel shows deterministic download actions plus explicit `Content-Disposition` filename semantics |
| `AC-03` | Telemetry boards show stream-first connect path with fallback behavior |
| `AC-04` | Event merge/dedup intent represented in fallback board and `.tsx` merge helper |
| `AC-05` | JSONL filter bar includes required controls and explicit reset action |
| `AC-06` | Artifact inspector table/list + file-id selector shows metadata and deterministic per-artifact download actions |
| `AC-07` | Invalid and missing run/execution/artifact ids are shown as actionable `role="alert"` feedback |
| `AC-08` | `/output` as dedicated route with run/execution context controls |
| `AC-09` | Focus styles, semantic labels, route-level `h1`, and explicit 320px board/live-region announcements included |
| `AC-10` | Existing `/events` behavior remains untouched (scope limited to `/output` mockups) |
| `AC-11` | Unsafe identifier handling represented via inline run/execution/artifact validation callouts |
| `AC-12` | Wireframe state transitions and failure hooks are explicit for downstream automated tests (stream fallback, filter bounds, artifact download failure) |
