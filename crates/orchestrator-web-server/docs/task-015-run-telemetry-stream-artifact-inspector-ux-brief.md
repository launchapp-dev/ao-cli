# TASK-015 UX Brief: Run Telemetry Stream and Artifact Inspector

## Phase
- Workflow phase: `ux-research`
- Workflow ID: `45cbf98a-2acd-4a11-8f17-a0c052c079d6`
- Task: `TASK-015`

## UX Objective
Design a deterministic output-inspection experience that lets operators answer,
from one place, three run-triage questions:
1. Is this run currently producing events, and how is the client connected?
2. Which JSONL lines match the current incident context?
3. Which artifacts exist for this execution, and can I download them safely?

The UX must be readable without terminal tools, preserve stable ordering and
deduplication across stream/poll transitions, and remain keyboard-usable from
desktop down to `320px`.

## Primary Users and Jobs

| User | Primary jobs | UX success signal |
| --- | --- | --- |
| Operator | Monitor active run progress and detect stalled/failing phases | Can identify connection mode and newest run events in under 10 seconds |
| Incident responder | Filter run output quickly by task/phase/source context | Can narrow JSONL output with deterministic filters in <= 3 interactions |
| Reviewer or lead | Retrieve output artifacts for evidence and handoff | Can find and download target artifact with clear success/failure feedback |

## UX Principles for This Phase
1. Status clarity first: connection state and fetch health are always visible.
2. Deterministic evidence: ordering, dedup, and filter behavior are predictable and explicit.
3. Progressive disclosure: summaries are scannable; raw JSON expands on demand.
4. Fail soft: stream interruption degrades to polling automatically without losing context.
5. Safe-by-default data handling: binary payloads are downloadable but never inlined in diagnostics.

## Information Architecture

### Primary Entry Points
1. Existing `/events` route remains daemon-wide and unchanged.
2. New route: `/output` (primary run output inspector).
3. Optional future deep-link expansion (not required in this task): `/output/runs/:runId`.

### `/output` Screen Structure
1. Context and controls bar:
   - run ID input,
   - execution ID input,
   - refresh/reconnect actions,
   - global status summary.
2. Run telemetry stream panel:
   - connection mode chip,
   - bounded event list,
   - row expansion for payload detail.
3. JSONL explorer panel:
   - deterministic filters (`query`, `source_file`, `task_id`, `phase_id`, `limit`),
   - results list with source/timestamp hints,
   - reset controls.
4. Artifact inspector panel:
   - artifact metadata list,
   - file-list assistive selector (if exposed),
   - per-artifact download action and error states.

## Key Screens and Interaction Contracts

| Screen | Goal | Primary interactions | Required states |
| --- | --- | --- | --- |
| `/output` (landing) | Start and anchor run/execution inspection | Enter run ID, enter execution ID, submit context, refresh data | idle, loading, ready, invalid_input, not_found |
| Telemetry stream panel | Observe live run events with deterministic fallback | Connect stream, auto-fallback to polling, reconnect, expand event details | connecting, live, polling, reconnecting, disconnected, stream-error |
| JSONL explorer panel | Find relevant historical lines quickly | Set filters, apply search, clear filters, expand/copy line | loading, results, empty-results, filter-error |
| Artifact inspector panel | Browse and download run artifacts | Load artifact list, trigger download, retry failed download | loading, empty, list-ready, download-pending, download-failed |

## Critical User Flows

### Flow A: Live Run Monitoring (Stream Healthy)
1. User opens `/output`, enters `run_id`, and starts inspection.
2. UI enters `connecting`, then `live` once SSE is established.
3. New events append in deterministic sequence order and announce updates politely.
4. User expands selected event rows to inspect payload details without losing list context.

### Flow B: Stream Failure to Polling Recovery
1. SSE fails or disconnects unexpectedly.
2. UI transitions to `reconnecting`, then `polling` fallback automatically.
3. Poll requests include last rendered sequence to avoid duplicates.
4. Connection indicator and helper text explain fallback mode.
5. If stream becomes available again, UI may reconnect while preserving monotonic ordering.

### Flow C: Incident Triage With JSONL Filters
1. User applies `task_id` and `phase_id` filters, then adds free-text query.
2. Result set updates with deterministic ordering and bounded size.
3. User inspects source metadata and expands a candidate line for raw content.
4. User resets filters to return to baseline result set.

### Flow D: Artifact Evidence Retrieval
1. User enters `execution_id` and loads artifact metadata.
2. User selects an artifact row and triggers download.
3. Browser save flow uses server-provided filename when present.
4. On failure, UI shows actionable error text and preserves list state for retry.

### Flow E: Missing or Invalid Resource Handling
1. User enters malformed or unknown IDs.
2. API returns deterministic `invalid_input` or `not_found` envelope errors.
3. UI maps errors to actionable messages near the relevant input control.
4. User corrects ID and retries without page reload.

## Layout, Hierarchy, and Spacing Guidance

### Desktop (`>= 960px`)
- Keep one page header with title, short description, and connection summary.
- Render context controls in a single row that wraps cleanly when needed.
- Use two stacked content rows:
  - row 1: telemetry stream panel (primary width),
  - row 2: JSONL explorer and artifact inspector (equal or weighted split).
- Keep raw JSON/details inside expandable regions to protect scanability.

### Mobile (`< 960px`, including `320px`)
- Stack sections in this strict order: controls -> telemetry -> JSONL -> artifacts.
- Keep filter controls single-column with full-width inputs and buttons.
- Ensure event/jsonl/artifact rows wrap long IDs and filenames with no horizontal page scroll.
- Keep primary controls and download actions at minimum `44x44px` target size.

### Spacing and Visual Rhythm
- Use shared spacing scale (`4/8/12/16/24/32px`) already used in the app shell.
- Reserve strongest visual contrast for connection mode, errors, and active filters.
- Keep empty-state messaging concise and non-alarming.

## Accessibility Constraints (Non-Negotiable)
1. Route exposes one `h1` and ordered section headings for telemetry, JSONL, and artifacts.
2. Connection mode text is always present and not color-only (`live`, `polling`, etc.).
3. Connection changes and new-event availability are announced via `aria-live="polite"` without moving focus.
4. Event, JSONL, and artifact collections use semantic list/table markup with clear headers.
5. Expand/collapse controls use `button` semantics with `aria-expanded` and `aria-controls`.
6. Filter inputs have programmatic labels and error text association.
7. Download controls are keyboard reachable and include descriptive labels with artifact identity.
8. Invalid/missing-resource feedback uses `role="alert"` and points to corrective action.
9. Focus ring remains visible on all controls with WCAG AA contrast.
10. At `320px`, all core interactions remain usable without horizontal page-level scrolling.
11. Reduced-motion preferences are honored for connection/reconnect feedback transitions.

## Interaction Details

| Interaction | Expected behavior | Error prevention/recovery |
| --- | --- | --- |
| Enter run context | Updates telemetry and JSONL queries to the selected run | Validate empty/unsafe IDs before request; show inline correction hints |
| Stream mode transition | Mode chip updates immediately on connect/fallback/reconnect | Preserve last sequence and render deduped merges only |
| Apply JSONL filters | Deterministic server query + stable ordered results | Keep active-filter summary visible and offer one-click reset |
| Expand event or JSONL row | Reveals raw payload in-place | Do not collapse sibling rows unexpectedly; keep keyboard focus stable |
| Download artifact | Triggers binary download with safe filename handling | On failure, keep artifact list intact and expose retry action |

## UX Acceptance Checklist for Implementation Phase
- New `/output` route exists and is discoverable from primary navigation.
- Run stream view shows explicit connection states and automatic stream-to-poll fallback.
- Event rendering preserves deterministic ordering and deduplicates across fallback/reconnect.
- JSONL explorer includes required filters, reset action, and explicit empty-results state.
- Artifact inspector shows metadata and supports per-artifact download with actionable failures.
- Invalid and missing IDs map to clear `invalid_input` and `not_found` user feedback.
- Output inspector remains keyboard-operable and readable at `320px` without horizontal scroll.
