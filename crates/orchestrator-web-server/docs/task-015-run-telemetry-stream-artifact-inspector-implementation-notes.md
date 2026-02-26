# TASK-015 Implementation Notes: Run Telemetry Stream and Artifact Inspector

## Purpose
Translate `TASK-015` requirements into deterministic implementation slices for
the build phase, spanning:
- output read APIs,
- run stream + poll fallback behavior,
- JSONL filtering UX,
- artifact browse/download UX.

## Non-Negotiable Constraints
- Keep `.ao` state file mutation policy intact; no manual `.ao/*.json` edits.
- Preserve existing `/api/v1/events` daemon stream behavior.
- Keep output/artifact access read-only and project-scoped.
- Reject unsafe id/path input (`run_id`, `execution_id`, `artifact_id`).
- Keep JSON metadata endpoints compatible with `ao.cli.v1` envelope handling.
- Keep UI accessible and responsive at narrow widths.

## Baseline Integration Points
- Web API surface:
  `crates/orchestrator-web-api/src/services/web_api_service.rs`
- HTTP route wiring:
  `crates/orchestrator-web-server/src/services/web_server.rs`
- Static API contract docs:
  `crates/orchestrator-web-server/openapi.json`
- Web UI route shell and screens:
  `crates/orchestrator-web-server/web-ui/src/app/{router.tsx,shell.tsx,screens.tsx}`
- API client and contract guards:
  `crates/orchestrator-web-server/web-ui/src/lib/api/`
- Existing SSE pattern:
  `crates/orchestrator-web-server/web-ui/src/lib/events/use-daemon-events.ts`

## Proposed Source Layout (Build Phase)
- `crates/orchestrator-web-api/src/services/web_api_service.rs`
  - add output read handlers:
    - run events snapshot
    - run events stream feed helper
    - JSONL query/filter
    - artifact list/files
    - artifact download path resolution
  - add shared safe-id/path utilities for output routes
- `crates/orchestrator-web-server/src/services/web_server.rs`
  - add `/api/v1/output/*` routes
  - add SSE handler for run events stream
  - add binary download handler response (`Content-Disposition`,
    `application/octet-stream`)
- `crates/orchestrator-web-server/openapi.json`
  - document output endpoints and response shapes
- `crates/orchestrator-web-server/web-ui/src/lib/api/contracts/models.ts`
  - output event/jsonl/artifact model types
- `crates/orchestrator-web-server/web-ui/src/lib/api/contracts/guards.ts`
  - payload decoders for output routes
- `crates/orchestrator-web-server/web-ui/src/lib/api/client.ts`
  - output request methods and typed response handling
- `crates/orchestrator-web-server/web-ui/src/lib/output/`
  - `use-run-output-events.ts` (stream + poll fallback)
  - `jsonl-filters.ts` (query/filter normalization helpers)
  - tests for fallback, dedup, filtering
- `crates/orchestrator-web-server/web-ui/src/app/`
  - add output inspector page component and route wiring
- `crates/orchestrator-web-server/web-ui/src/styles.css`
  - output page layout, table/list styles, responsive stacking

## API Design Notes
Recommended route contract:
- `GET /api/v1/output/runs/:run_id/events`
  - query: `after_seq`, `limit`
  - returns ordered event payload rows
- `GET /api/v1/output/runs/:run_id/events/stream`
  - SSE for incremental event updates
- `GET /api/v1/output/runs/:run_id/jsonl`
  - query: `query`, `source_file`, `task_id`, `phase_id`, `limit`
  - returns source-aware JSONL entries
- `GET /api/v1/output/executions/:execution_id/artifacts`
- `GET /api/v1/output/executions/:execution_id/files`
- `GET /api/v1/output/executions/:execution_id/artifacts/:artifact_id/download`

Validation notes:
- enforce safe identifier policy before filesystem access,
- reject `..`, root/prefix, and empty ids,
- keep run lookup behavior consistent with existing CLI output lookup precedence.

## Run Stream + Poll Fallback Notes
- Reuse existing SSE parsing approach from `use-daemon-events` but scope by
  `run_id`.
- Track `lastSeq` and merged set to guarantee deterministic deduplication.
- Fallback policy:
  1. try SSE stream,
  2. if stream fails, switch to polling with bounded interval/backoff,
  3. resume from `lastSeq` for incremental consistency.
- Expose connection mode and last sync timestamp for operator feedback.

## JSONL Filter Notes
- Keep server-side filtering available to reduce payload volume.
- Normalize query/filter inputs (trim, case handling) for deterministic behavior.
- Preserve source metadata (`source_file`, `timestamp_hint`) in result rows.
- Keep result window bounded and predictable.

## Artifact Download Notes
- Use direct fetch/blob flow for download endpoint in UI (not JSON byte-array
  rendering).
- Preserve server-provided filename when available.
- Keep binary payloads out of diagnostics telemetry body capture.
- Surface concise user-facing download failure states.

## UI Composition Notes
- Route recommendation: add dedicated output inspector route in shell nav.
- Suggested section hierarchy:
  1. run/execution selectors,
  2. live stream panel,
  3. JSONL filter/results panel,
  4. artifact inspector panel.
- Keep data-dense panels readable with:
  - fixed headings,
  - clear empty/error states,
  - optional disclosure for raw JSON details.

## Suggested Build Sequence
1. Add output API route handlers and safe-id validation.
2. Add run events snapshot + stream endpoints.
3. Add JSONL filtering endpoint and tests.
4. Add artifact metadata/files/download endpoints and tests.
5. Add UI contract models/guards/client methods.
6. Implement `use-run-output-events` stream/poll hook + tests.
7. Add output inspector route/page and style integration.
8. Add component tests for filtering, fallback states, and download controls.
9. Run web-ui tests/build and relevant Rust tests for server/API crates.

## Testing Targets
- Rust:
  - web API/service tests for lookup and validation behavior
  - web server route tests for `/api/v1/output/*` and download headers
- Web UI:
  - stream fallback and dedup tests
  - JSONL filter behavior tests
  - output inspector component interaction tests
  - router/nav regression tests

## Regression Guardrails
- Do not alter existing daemon events hook contracts.
- Do not break existing `APP_ROUTE_PATHS` baseline checks.
- Preserve existing telemetry diagnostics behavior from `TASK-019`.
- Keep generated embedded assets out of scope until implementation/build phase
  explicitly requires regeneration.

## Deferred Follow-Ups (Not in TASK-015)
- Cross-run aggregated analytics dashboard.
- Persisted client-side output filter presets.
- Artifact inline previewers for specialized binary formats.
- Multi-project/global output search surface.
