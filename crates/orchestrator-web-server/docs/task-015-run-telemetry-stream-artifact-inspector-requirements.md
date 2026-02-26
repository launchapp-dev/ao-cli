# TASK-015 Requirements: Run Telemetry Stream and Artifact Inspector

## Phase
- Workflow phase: `requirements`
- Workflow ID: `45cbf98a-2acd-4a11-8f17-a0c052c079d6`
- Task: `TASK-015`

## Objective
Define production-ready run output observability for the standalone daemon web
UI with three core capabilities:
- real-time run event viewing with deterministic polling fallback,
- searchable/filterable JSONL run output exploration,
- artifact browsing and safe file download UX.

Primary outcomes:
- run-scoped telemetry stream operators can watch without using terminal tools,
- deterministic JSONL filtering for fast incident/debug triage,
- artifact access that is auditable, safe, and accessible in the web UI.

## Existing Baseline
- Web UI includes an `Events` page in
  `crates/orchestrator-web-server/web-ui/src/app/screens.tsx`, but it currently
  reads daemon-wide events from `/api/v1/events` and renders raw JSON.
- Web API/web server currently expose daemon, project, planning, task, workflow,
  and review routes, but no run-output or artifact routes.
- CLI already supports run output operations in
  `crates/orchestrator-cli/src/services/operations/ops_output.rs`:
  - run events lookup (`output run`),
  - aggregated JSONL (`output jsonl`),
  - filtered monitoring (`output monitor`),
  - artifact listing/files/download (`output artifacts|files|download`).
- Web UI API contracts/client currently have no typed output/artifact models or
  request methods.

## Scope
In scope for implementation after this requirements phase:
- Add read-only `/api/v1/output/*` API routes for run telemetry, JSONL, and
  artifact metadata/download.
- Add run event stream UX that prefers live stream and falls back to polling.
- Add JSONL search/filter UX for run output inspection.
- Add artifact inspector UX with browse + download flow.
- Add typed web-ui client contracts and tests for new output routes.

Out of scope for this task:
- Changes to runner event persistence schema or `.ao/runs` file format.
- Mutating/deleting artifacts from the web UI.
- Multi-user auth/authorization redesign.
- External telemetry vendor export/integration.
- Replacing existing daemon-wide `/api/v1/events` behavior.

## Constraints
- Preserve `ao.cli.v1` envelope semantics for JSON metadata endpoints.
- Keep run/artifact lookup repository-scoped and deterministic; do not read
  outside project-root-resolved AO run/artifact directories.
- Reject unsafe identifiers (`run_id`, `execution_id`, `artifact_id`) that could
  cause path traversal.
- Keep `.ao` mutation policy intact (no manual `.ao/*.json` edits).
- UI must remain keyboard operable and usable at `320px` width without page-level
  horizontal scroll.
- Poll fallback must not duplicate already-rendered run events.

## Functional Requirements

### FR-01: Output API Surface
Add read-only output routes under `/api/v1/output`:
- `GET /api/v1/output/runs/:run_id/events`
  - Returns ordered run event records.
  - Supports incremental fetch with `after_seq` and bounded `limit`.
- `GET /api/v1/output/runs/:run_id/events/stream`
  - SSE stream for incremental run event updates.
  - Uses stable event ids derived from event sequence.
- `GET /api/v1/output/runs/:run_id/jsonl`
  - Returns aggregated JSONL entries with source metadata.
  - Supports deterministic filtering parameters:
    - `query` (substring search),
    - `source_file`,
    - `task_id`,
    - `phase_id`,
    - bounded `limit`.
- `GET /api/v1/output/executions/:execution_id/artifacts`
  - Returns artifact metadata list (`artifact_id`, type, size, file path).
- `GET /api/v1/output/executions/:execution_id/files`
  - Returns artifact file ids for quick selection.
- `GET /api/v1/output/executions/:execution_id/artifacts/:artifact_id/download`
  - Streams artifact bytes with deterministic content headers for browser
    download.

### FR-02: Run Event Stream With Poll Fallback
- Web UI must attempt live run event streaming first.
- On stream failure/unavailability, UI must automatically switch to polling
  `events` endpoint without requiring manual user recovery.
- Reconnect/fallback behavior must preserve monotonic event ordering by sequence
  and deduplicate previously seen events.
- UI must expose explicit connection state:
  - `connecting`,
  - `live`,
  - `polling`,
  - `reconnecting`,
  - `disconnected`.

### FR-03: Run Telemetry Viewer UX
- Add a dedicated run output surface with:
  - run id input/control,
  - connection state indicator,
  - bounded live event list with newest-first readability,
  - empty state and error state messaging.
- Event entries must show at minimum:
  - sequence or timestamp,
  - event kind/type,
  - run id,
  - task/phase ids when present,
  - compact event summary + optional raw JSON expansion.

### FR-04: JSONL Search and Filter UX
- Provide deterministic filter controls:
  - free-text search,
  - source file selector,
  - task id,
  - phase id.
- Render filtered results with:
  - source file and timestamp hint,
  - line preview,
  - raw line expansion/copy affordance.
- Include no-results state and explicit filter reset action.
- Filtering must produce the same ordered result set for the same input.

### FR-05: Artifact Browser and Download UX
- Provide execution id input/control and artifact metadata table/list.
- Artifact list must include:
  - artifact id,
  - type/extension,
  - size (when known).
- Each artifact entry must expose a deterministic download action.
- Download flow must:
  - preserve artifact filename in browser save dialog when possible,
  - surface actionable errors when the download fails,
  - avoid rendering binary payload content in diagnostics panes.

### FR-06: Contracts and Routing Integration
- Add typed web-ui models/guards for all new output endpoint payloads.
- Extend web-ui API client with output/artifact request methods.
- Add route(s) and primary navigation affordance for output inspector.
- Preserve existing route behavior and shell navigation semantics.

### FR-07: Error and Empty-State Behavior
- Missing run/execution/artifact resources return deterministic `not_found`
  behavior and are shown as actionable UI errors.
- Invalid ids return deterministic `invalid_input` errors.
- Empty run/artifact datasets render explicit empty states, not blank panels.

### FR-08: Security and Data Hygiene
- No endpoint may expose file paths outside the allowed project AO directories.
- Download endpoint must validate and sanitize requested artifact identifier.
- Telemetry/diagnostics must not log binary artifact payload bodies.

## Non-Functional Requirements

### NFR-01: Determinism
- Event ordering is stable by sequence/timestamp for identical input.
- Poll fallback produces deterministic merge behavior with no duplicates.
- Bounded limits are explicit and enforced.

### NFR-02: Performance
- Event viewer and JSONL result sets must be bounded to prevent unbounded
  browser memory growth.
- Polling cadence must be bounded and backoff-aware under repeated failures.
- Large artifact downloads must stream without requiring full payload expansion in
  JSON.

### NFR-03: Accessibility
- All new controls are keyboard operable.
- Status changes (stream mode/fallback/errors) are announced politely.
- Data lists/tables have clear headings and readable semantics.

### NFR-04: Responsive Layout
- Desktop and mobile layouts preserve clear section hierarchy:
  - run controls,
  - telemetry stream,
  - JSONL explorer,
  - artifacts.
- At mobile widths, controls wrap cleanly and content remains readable without
  horizontal page scroll.

## UX and Information Hierarchy Requirements
- Prioritize operator triage flow:
  1. identify run/execution,
  2. watch current event stream status,
  3. filter historical JSONL context,
  4. inspect/download artifacts.
- Keep event and JSONL content scannable with progressive disclosure for raw
  payload detail.
- Surface actionable context first (state, errors, ids, timestamps), raw JSON
  second.

## Acceptance Criteria
- `AC-01`: `/api/v1/output` metadata routes exist for run events, JSONL, and
  artifact listing/files.
- `AC-02`: `/api/v1/output/.../download` route returns artifact bytes with stable
  download headers.
- `AC-03`: UI run event viewer prefers stream mode and auto-falls back to polling
  on failure.
- `AC-04`: Event list deduplicates and preserves deterministic order across
  reconnect/fallback transitions.
- `AC-05`: JSONL explorer supports required filters (`query`, `source_file`,
  `task_id`, `phase_id`) and reset behavior.
- `AC-06`: Artifact inspector renders metadata and supports per-artifact download.
- `AC-07`: Invalid and missing resource errors are shown with actionable messages.
- `AC-08`: New route/navigation entry for run output inspector is present and
  usable.
- `AC-09`: New UI remains keyboard-accessible and mobile-readable at `320px`.
- `AC-10`: Existing daemon events page and existing `/api/v1/events` behavior are
  unchanged.
- `AC-11`: Identifier/path validation blocks traversal and unsafe ids.
- `AC-12`: Automated tests cover stream fallback, JSONL filtering, artifact
  listing/download contracts, and route wiring.

## Testable Acceptance Checklist
- `T-01`: Web API tests validate id sanitization and deterministic lookup behavior
  for run/output endpoints.
- `T-02`: Web server handler tests validate route responses and download headers.
- `T-03`: Hook tests validate stream-first connection and automatic poll fallback.
- `T-04`: Hook/component tests validate event deduplication and ordering.
- `T-05`: JSONL filter tests validate deterministic filtering and empty-state
  behavior.
- `T-06`: Artifact UI tests validate list rendering and download initiation/error
  handling.
- `T-07`: Router/shell tests validate output route + navigation inclusion.
- `T-08`: Regression tests validate existing routes/events behavior remains stable.

## Acceptance Verification Matrix
| Requirement | Verification method |
| --- | --- |
| Output API route coverage | Web API + web server tests for `/api/v1/output/*` |
| Stream + poll fallback | `use-run-output-events` tests with SSE failure simulation |
| JSONL filtering | Unit tests on query/filter logic + component state assertions |
| Artifact browse/download | Endpoint tests + UI interaction tests |
| Accessibility/responsive baseline | Component tests + existing accessibility baseline checks |
| No regression to existing routes | Existing router/events/client test suite + targeted regressions |

## Implementation Notes (Next Phase Input)
Primary implementation targets:
- `crates/orchestrator-web-api/src/services/web_api_service.rs`
  - output/artifact read methods and validation helpers
- `crates/orchestrator-web-server/src/services/web_server.rs`
  - `/api/v1/output/*` route/handler wiring + download response
- `crates/orchestrator-web-server/openapi.json`
  - route contract additions
- `crates/orchestrator-web-server/web-ui/src/lib/api/contracts/{models,guards}.ts`
  - typed payload contracts for output/artifact APIs
- `crates/orchestrator-web-server/web-ui/src/lib/api/client.ts`
  - output/artifact request methods
- `crates/orchestrator-web-server/web-ui/src/lib/output/`
  - stream+poll fallback hook and filter helpers
- `crates/orchestrator-web-server/web-ui/src/app/`
  - run output page integration and route wiring
- `crates/orchestrator-web-server/web-ui/src/styles.css`
  - layout and states for output inspector
