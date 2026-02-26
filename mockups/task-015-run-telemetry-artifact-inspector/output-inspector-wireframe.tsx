/**
 * TASK-015 wireframe scaffold.
 * Intent: capture deterministic UI state and interaction contracts for `/output`
 * before implementation in the production web UI.
 */

import { useMemo, useState } from "react";

type RouteState = "idle" | "loading" | "ready" | "invalid_input" | "not_found";
type ConnectionMode =
  | "connecting"
  | "live"
  | "polling"
  | "reconnecting"
  | "disconnected"
  | "stream-error";
type JsonlState = "loading" | "results" | "empty-results" | "filter-error";
type ArtifactPanelState = "loading" | "empty" | "list-ready" | "download-pending" | "download-failed";
type ArtifactDownloadState = "idle" | "downloading" | "downloaded" | "failed";
type TraceabilityId =
  | "AC-01"
  | "AC-02"
  | "AC-03"
  | "AC-04"
  | "AC-05"
  | "AC-06"
  | "AC-07"
  | "AC-08"
  | "AC-09"
  | "AC-10"
  | "AC-11"
  | "AC-12";

type RunTelemetryEvent = {
  seq: number;
  timestamp: string;
  kind: string;
  runId: string;
  taskId?: string;
  phaseId?: string;
  summary: string;
  payload: string;
};

type JsonlEntry = {
  id: string;
  timestampHint: string;
  sourceFile: string;
  taskId?: string;
  phaseId?: string;
  preview: string;
  rawLine: string;
};

type JsonlFilters = {
  query: string;
  sourceFile: string;
  taskId: string;
  phaseId: string;
  limit: number;
};

type ArtifactItem = {
  artifactId: string;
  artifactType: string;
  fileName: string;
  filePathHint: string;
  sizeBytes: number;
  downloadState: ArtifactDownloadState;
};

const MAX_EVENT_ROWS = 100;
const DEDUP_CACHE_WINDOW = 500;
const DEFAULT_JSONL_LIMIT = 50;
const MAX_JSONL_LIMIT = 200;
const SAFE_ID_PATTERN = /^[A-Za-z0-9_-]+$/;

const initialRunEvents: RunTelemetryEvent[] = [
  {
    seq: 4821,
    timestamp: "2026-02-25T16:42:19Z",
    kind: "agent.run.completed",
    runId: "run_20260225_164103_9f3ca1",
    taskId: "TASK-015",
    phaseId: "wireframe",
    summary: "Wireframe assets generated for /output UX handoff.",
    payload: JSON.stringify(
      {
        seq: 4821,
        kind: "agent.run.completed",
        run_id: "run_20260225_164103_9f3ca1",
        task_id: "TASK-015",
        phase_id: "wireframe",
        message: "Created mockups/task-015-run-telemetry-artifact-inspector",
      },
      null,
      2,
    ),
  },
  {
    seq: 4820,
    timestamp: "2026-02-25T16:41:58Z",
    kind: "agent.phase.progress",
    runId: "run_20260225_164103_9f3ca1",
    taskId: "TASK-015",
    phaseId: "wireframe",
    summary: "Wireframe HTML and CSS staged.",
    payload: JSON.stringify(
      {
        seq: 4820,
        kind: "agent.phase.progress",
        run_id: "run_20260225_164103_9f3ca1",
        detail: "wireframes.html and wireframes.css staged",
      },
      null,
      2,
    ),
  },
  {
    seq: 4819,
    timestamp: "2026-02-25T16:41:22Z",
    kind: "agent.phase.started",
    runId: "run_20260225_164103_9f3ca1",
    taskId: "TASK-015",
    phaseId: "wireframe",
    summary: "Wireframe phase started.",
    payload: JSON.stringify(
      {
        seq: 4819,
        kind: "agent.phase.started",
        run_id: "run_20260225_164103_9f3ca1",
        task_id: "TASK-015",
        phase_id: "wireframe",
      },
      null,
      2,
    ),
  },
];

const initialJsonlRows: JsonlEntry[] = [
  {
    id: "jsonl-1041",
    timestampHint: "2026-02-25T16:41:46Z",
    sourceFile: "events.jsonl",
    taskId: "TASK-015",
    phaseId: "wireframe",
    preview: "sse disconnected; switching to polling fallback",
    rawLine:
      '{"timestamp_hint":"2026-02-25T16:41:46Z","message":"sse disconnected","mode":"reconnecting","fallback":"polling"}',
  },
  {
    id: "jsonl-1034",
    timestampHint: "2026-02-25T16:40:13Z",
    sourceFile: "events.jsonl",
    taskId: "TASK-015",
    phaseId: "wireframe",
    preview: "stream connected with Last-Event-ID=4818",
    rawLine:
      '{"timestamp_hint":"2026-02-25T16:40:13Z","message":"stream connected","last_event_id":4818}',
  },
  {
    id: "jsonl-1022",
    timestampHint: "2026-02-25T16:33:08Z",
    sourceFile: "json-output.jsonl",
    taskId: "TASK-015",
    phaseId: "requirements",
    preview: "requirements summary persisted",
    rawLine:
      '{"timestamp_hint":"2026-02-25T16:33:08Z","phase_id":"requirements","message":"task requirements persisted"}',
  },
  {
    id: "jsonl-1014",
    timestampHint: "2026-02-25T16:31:54Z",
    sourceFile: "stderr.jsonl",
    preview: "transient stream handshake warning",
    rawLine:
      '{"timestamp_hint":"2026-02-25T16:31:54Z","warning":"temporary stream handshake timeout"}',
  },
];

const initialArtifacts: ArtifactItem[] = [
  {
    artifactId: "artifact_321",
    artifactType: "jsonl-bundle",
    fileName: "run-20260225-events.zip",
    filePathHint: ".ao/artifacts/exec_20260225_164100_d6b21f/run-20260225-events.zip",
    sizeBytes: 196_608,
    downloadState: "idle",
  },
  {
    artifactId: "artifact_322",
    artifactType: "log",
    fileName: "wireframe-phase.log",
    filePathHint: ".ao/artifacts/exec_20260225_164100_d6b21f/wireframe-phase.log",
    sizeBytes: 41_984,
    downloadState: "idle",
  },
  {
    artifactId: "artifact_323",
    artifactType: "png",
    fileName: "mockup-preview.png",
    filePathHint: ".ao/artifacts/exec_20260225_164100_d6b21f/mockup-preview.png",
    sizeBytes: 1_884_160,
    downloadState: "idle",
  },
];

const defaultJsonlFilters: JsonlFilters = {
  query: "",
  sourceFile: "",
  taskId: "",
  phaseId: "",
  limit: DEFAULT_JSONL_LIMIT,
};

function isSafeIdentifier(value: string): boolean {
  return SAFE_ID_PATTERN.test(value);
}

function normalizeFilterValue(value: string): string {
  return value.trim();
}

function clampJsonlLimit(limit: number): number {
  if (!Number.isFinite(limit)) {
    return DEFAULT_JSONL_LIMIT;
  }
  return Math.min(Math.max(Math.trunc(limit), 1), MAX_JSONL_LIMIT);
}

function formatTimestamp(value: string): string {
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) {
    return value;
  }
  return new Date(parsed).toISOString().slice(0, 19) + " UTC";
}

function formatBytes(value: number): string {
  if (value >= 1024 * 1024) {
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (value >= 1024) {
    return `${Math.round(value / 1024)} KB`;
  }
  return `${value} B`;
}

function mergeRunEvents(current: RunTelemetryEvent[], incoming: RunTelemetryEvent[]): RunTelemetryEvent[] {
  const dedupBySeq = new Map<number, RunTelemetryEvent>();

  for (const entry of [...current, ...incoming]) {
    dedupBySeq.set(entry.seq, entry);
  }

  const ordered = [...dedupBySeq.values()].sort((left, right) => right.seq - left.seq);
  return ordered.slice(0, MAX_EVENT_ROWS);
}

function filterJsonlEntries(entries: JsonlEntry[], filters: JsonlFilters): JsonlEntry[] {
  const query = normalizeFilterValue(filters.query).toLowerCase();
  const sourceFile = normalizeFilterValue(filters.sourceFile);
  const taskId = normalizeFilterValue(filters.taskId).toLowerCase();
  const phaseId = normalizeFilterValue(filters.phaseId).toLowerCase();

  const filtered = entries.filter((entry) => {
    if (sourceFile.length > 0 && entry.sourceFile !== sourceFile) {
      return false;
    }

    if (taskId.length > 0 && (entry.taskId ?? "").toLowerCase() !== taskId) {
      return false;
    }

    if (phaseId.length > 0 && (entry.phaseId ?? "").toLowerCase() !== phaseId) {
      return false;
    }

    if (query.length > 0) {
      const corpus = `${entry.preview} ${entry.rawLine} ${entry.sourceFile}`.toLowerCase();
      return corpus.includes(query);
    }

    return true;
  });

  const limit = clampJsonlLimit(filters.limit);
  return filtered
    .slice()
    .sort((left, right) => right.timestampHint.localeCompare(left.timestampHint))
    .slice(0, limit);
}

function deriveArtifactPanelState(items: ArtifactItem[]): ArtifactPanelState {
  if (items.length === 0) {
    return "empty";
  }
  if (items.some((entry) => entry.downloadState === "downloading")) {
    return "download-pending";
  }
  if (items.some((entry) => entry.downloadState === "failed")) {
    return "download-failed";
  }
  return "list-ready";
}

function nextPollSample(lastSeq: number, runId: string): RunTelemetryEvent[] {
  return [
    {
      seq: lastSeq,
      timestamp: "2026-02-25T16:43:00Z",
      kind: "agent.phase.progress",
      runId,
      taskId: "TASK-015",
      phaseId: "wireframe",
      summary: "Duplicate row returned by poll and deduped by sequence.",
      payload: JSON.stringify(
        {
          seq: lastSeq,
          kind: "agent.phase.progress",
          detail: "duplicate sample for dedup check",
        },
        null,
        2,
      ),
    },
    {
      seq: lastSeq + 1,
      timestamp: "2026-02-25T16:43:05Z",
      kind: "agent.phase.progress",
      runId,
      taskId: "TASK-015",
      phaseId: "wireframe",
      summary: "Polling fallback appended one new deterministic event.",
      payload: JSON.stringify(
        {
          seq: lastSeq + 1,
          kind: "agent.phase.progress",
          fallback_mode: "polling",
          dedup_window: DEDUP_CACHE_WINDOW,
        },
        null,
        2,
      ),
    },
  ];
}

export const acceptanceTraceability: Record<TraceabilityId, string[]> = {
  "AC-01": ["State model includes output run events, JSONL entries, artifacts, and validation-ready ids."],
  "AC-02": [
    "Artifact controls model stable download header semantics and preserve deterministic file names for save dialogs.",
  ],
  "AC-03": ["Connection mode model starts stream-first and supports poll fallback."],
  "AC-04": ["`mergeRunEvents` deduplicates by sequence and preserves monotonic ordering."],
  "AC-05": ["JSONL filters include query/source_file/task_id/phase_id/limit and reset behavior."],
  "AC-06": ["Artifact panel states cover loading, empty, list-ready, pending, and failed paths."],
  "AC-07": ["Invalid and not-found errors map to inline route-level error strings."],
  "AC-08": ["Component represents dedicated `/output` route hierarchy and controls."],
  "AC-09": ["Markup includes aria-live status and expandable rows with aria-expanded/aria-controls."],
  "AC-10": ["Scope is isolated to output inspector behavior with no daemon `/events` override."],
  "AC-11": [
    "Safe-id validation blocks path-traversal-like run/execution/artifact ids before request actions.",
  ],
  "AC-12": ["Deterministic state transitions expose explicit hooks for stream fallback and artifact error test paths."],
};

export function OutputInspectorWireframe() {
  const [routeState, setRouteState] = useState<RouteState>("idle");
  const [connectionMode, setConnectionMode] = useState<ConnectionMode>("connecting");
  const [jsonlState, setJsonlState] = useState<JsonlState>("results");

  const [runId, setRunId] = useState("run_20260225_164103_9f3ca1");
  const [executionId, setExecutionId] = useState("exec_20260225_164100_d6b21f");
  const [runIdError, setRunIdError] = useState<string | null>(null);
  const [executionIdError, setExecutionIdError] = useState<string | null>(null);

  const [events, setEvents] = useState<RunTelemetryEvent[]>(initialRunEvents);
  const [expandedEvents, setExpandedEvents] = useState<Record<number, boolean>>({ 4821: true });
  const [streamNotice, setStreamNotice] = useState("Connection mode changed to live.");

  const [jsonlFilters, setJsonlFilters] = useState<JsonlFilters>({
    ...defaultJsonlFilters,
    query: "fallback polling",
    sourceFile: "events.jsonl",
    taskId: "TASK-015",
    phaseId: "wireframe",
  });
  const [expandedJsonlRows, setExpandedJsonlRows] = useState<Record<string, boolean>>({ "jsonl-1041": true });
  const [jsonlError, setJsonlError] = useState<string | null>(null);

  const [artifacts, setArtifacts] = useState<ArtifactItem[]>(initialArtifacts);
  const [artifactFileId, setArtifactFileId] = useState(initialArtifacts[0]?.artifactId ?? "");
  const [artifactFileIdError, setArtifactFileIdError] = useState<string | null>(null);
  const [artifactMessage, setArtifactMessage] = useState<string | null>(null);
  const [artifactStatusNotice, setArtifactStatusNotice] = useState(
    "No artifact download started. Select an artifact id and trigger download.",
  );

  const sourceFileOptions = useMemo(
    () => [...new Set(initialJsonlRows.map((entry) => entry.sourceFile))].sort(),
    [],
  );

  const filteredJsonlRows = useMemo(
    () => filterJsonlEntries(initialJsonlRows, jsonlFilters),
    [jsonlFilters],
  );

  const artifactPanelState = useMemo(() => deriveArtifactPanelState(artifacts), [artifacts]);
  const lastSeq = events.length > 0 ? events[0].seq : 0;

  const handleStartInspection = () => {
    setRouteState("loading");
    setRunIdError(null);
    setExecutionIdError(null);

    if (!isSafeIdentifier(runId)) {
      setRouteState("invalid_input");
      setConnectionMode("disconnected");
      setRunIdError("invalid_input: run_id must only use letters, numbers, underscore, or dash.");
      return;
    }

    if (!isSafeIdentifier(executionId)) {
      setRouteState("invalid_input");
      setConnectionMode("disconnected");
      setExecutionIdError(
        "invalid_input: execution_id must only use letters, numbers, underscore, or dash.",
      );
      return;
    }

    if (executionId.includes("missing")) {
      setRouteState("not_found");
      setConnectionMode("disconnected");
      setExecutionIdError(`not_found: execution ${executionId} has no artifact rows.`);
      return;
    }

    setRouteState("ready");
    setConnectionMode("live");
    setStreamNotice(`Connected to stream for ${runId}.`);
  };

  const handleFallbackToPolling = () => {
    setConnectionMode("reconnecting");
    setStreamNotice(`Stream disconnected. Switching to polling after seq ${lastSeq}.`);

    const incoming = nextPollSample(lastSeq, runId);
    setEvents((current) => mergeRunEvents(current, incoming));
    setConnectionMode("polling");
  };

  const handleReconnectStream = () => {
    setConnectionMode("connecting");
    setConnectionMode("live");
    setStreamNotice(`Stream reconnected at seq ${events[0]?.seq ?? 0}.`);
  };

  const handleApplyFilters = () => {
    setJsonlState("loading");
    setJsonlError(null);

    if (jsonlFilters.limit > MAX_JSONL_LIMIT || jsonlFilters.limit < 1) {
      const normalizedLimit = clampJsonlLimit(jsonlFilters.limit);
      setJsonlFilters((current) => ({ ...current, limit: normalizedLimit }));
      setJsonlState("filter-error");
      setJsonlError(`filter_error: limit must be between 1 and ${MAX_JSONL_LIMIT}; reset to ${normalizedLimit}.`);
      return;
    }

    setJsonlState(filteredJsonlRows.length > 0 ? "results" : "empty-results");
  };

  const handleResetFilters = () => {
    setJsonlFilters(defaultJsonlFilters);
    setJsonlError(null);
    setJsonlState("results");
  };

  const toggleEventExpanded = (seq: number) => {
    setExpandedEvents((current) => ({ ...current, [seq]: !current[seq] }));
  };

  const toggleJsonlExpanded = (id: string) => {
    setExpandedJsonlRows((current) => ({ ...current, [id]: !current[id] }));
  };

  const handleDownloadArtifact = (artifactId: string) => {
    setArtifactMessage(null);
    setArtifactFileIdError(null);

    if (!isSafeIdentifier(artifactId)) {
      setArtifactFileIdError(
        `invalid_input: artifact_id ${artifactId} must only use letters, numbers, underscore, or dash.`,
      );
      setArtifactStatusNotice(`Blocked unsafe artifact id ${artifactId} before request dispatch.`);
      return;
    }

    const targetArtifact = artifacts.find((entry) => entry.artifactId === artifactId);
    if (!targetArtifact) {
      setArtifactMessage(`not_found: artifact ${artifactId} is not available for execution ${executionId}.`);
      setArtifactStatusNotice(`Artifact lookup failed for ${artifactId}.`);
      return;
    }

    setArtifacts((current) =>
      current.map((artifact) =>
        artifact.artifactId === artifactId
          ? { ...artifact, downloadState: "downloading" }
          : artifact,
      ),
    );

    if (artifactId === "artifact_323") {
      setArtifacts((current) =>
        current.map((artifact) =>
          artifact.artifactId === artifactId ? { ...artifact, downloadState: "failed" } : artifact,
        ),
      );
      setArtifactMessage(`download_failed: ${artifactId} not_found. Check execution context and retry.`);
      setArtifactStatusNotice(`Download failed for ${artifactId}.`);
      return;
    }

    setArtifacts((current) =>
      current.map((artifact) =>
        artifact.artifactId === artifactId ? { ...artifact, downloadState: "downloaded" } : artifact,
      ),
    );
    setArtifactStatusNotice(
      `Download ready for ${artifactId}. Content-Disposition: attachment; filename="${targetArtifact.fileName}".`,
    );
  };

  const handleDownloadSelectedFileId = () => {
    handleDownloadArtifact(artifactFileId.trim());
  };

  return (
    <section aria-label="Output inspector wireframe">
      <header>
        <h1>/output - Run Telemetry Stream and Artifact Inspector</h1>
        <p>
          Route state: <strong>{routeState}</strong>. Connection mode: <strong>{connectionMode}</strong>.
        </p>
      </header>

      <form
        onSubmit={(event) => {
          event.preventDefault();
          handleStartInspection();
        }}
        aria-label="Output context controls"
      >
        <label htmlFor="run-id">Run ID</label>
        <input
          id="run-id"
          value={runId}
          aria-invalid={runIdError ? true : undefined}
          aria-describedby={runIdError ? "run-id-error" : undefined}
          onChange={(event) => setRunId(event.target.value)}
        />
        {runIdError ? (
          <p id="run-id-error" role="alert">
            {runIdError}
          </p>
        ) : null}

        <label htmlFor="execution-id">Execution ID</label>
        <input
          id="execution-id"
          value={executionId}
          aria-invalid={executionIdError ? true : undefined}
          aria-describedby={executionIdError ? "execution-id-error" : undefined}
          onChange={(event) => setExecutionId(event.target.value)}
        />
        {executionIdError ? (
          <p id="execution-id-error" role="alert">
            {executionIdError}
          </p>
        ) : null}

        <div>
          <button type="submit">Start inspection</button>
          <button type="button" onClick={handleFallbackToPolling}>
            Simulate stream failure
          </button>
          <button type="button" onClick={handleReconnectStream}>
            Reconnect stream
          </button>
        </div>
      </form>

      <p aria-live="polite">{streamNotice}</p>

      <article aria-labelledby="telemetry-title">
        <h3 id="telemetry-title">Run Telemetry</h3>
        <p>
          Showing {events.length} rows (max {MAX_EVENT_ROWS}). Dedup cache window {DEDUP_CACHE_WINDOW}.
        </p>

        <ol aria-label="Telemetry events list">
          {events.map((entry) => {
            const expanded = Boolean(expandedEvents[entry.seq]);
            return (
              <li key={entry.seq}>
                <button
                  type="button"
                  aria-expanded={expanded}
                  aria-controls={`event-${entry.seq}`}
                  onClick={() => toggleEventExpanded(entry.seq)}
                >
                  <span>{entry.kind}</span>
                  <span>{`seq ${entry.seq}`}</span>
                  <span>{entry.taskId ? `${entry.taskId}/${entry.phaseId ?? "no-phase"}` : "no task metadata"}</span>
                  <span>{formatTimestamp(entry.timestamp)}</span>
                </button>
                {expanded ? (
                  <div id={`event-${entry.seq}`}>
                    <p>{entry.summary}</p>
                    <pre>{entry.payload}</pre>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ol>
      </article>

      <article aria-labelledby="jsonl-title">
        <h3 id="jsonl-title">JSONL Explorer</h3>
        <p>
          Filter state: <strong>{jsonlState}</strong>.
        </p>

        <form
          onSubmit={(event) => {
            event.preventDefault();
            handleApplyFilters();
          }}
        >
          <label htmlFor="query">Query</label>
          <input
            id="query"
            value={jsonlFilters.query}
            onChange={(event) =>
              setJsonlFilters((current) => ({ ...current, query: event.target.value }))
            }
          />

          <label htmlFor="source-file">Source file</label>
          <select
            id="source-file"
            value={jsonlFilters.sourceFile}
            onChange={(event) =>
              setJsonlFilters((current) => ({ ...current, sourceFile: event.target.value }))
            }
          >
            <option value="">all</option>
            {sourceFileOptions.map((source) => (
              <option key={source} value={source}>
                {source}
              </option>
            ))}
          </select>

          <label htmlFor="task-id">Task ID</label>
          <input
            id="task-id"
            value={jsonlFilters.taskId}
            onChange={(event) =>
              setJsonlFilters((current) => ({ ...current, taskId: event.target.value }))
            }
          />

          <label htmlFor="phase-id">Phase ID</label>
          <input
            id="phase-id"
            value={jsonlFilters.phaseId}
            onChange={(event) =>
              setJsonlFilters((current) => ({ ...current, phaseId: event.target.value }))
            }
          />

          <label htmlFor="limit">Limit</label>
          <input
            id="limit"
            type="number"
            value={jsonlFilters.limit}
            onChange={(event) =>
              setJsonlFilters((current) => ({ ...current, limit: Number(event.target.value) }))
            }
          />

          <div>
            <button type="submit">Apply filters</button>
            <button type="button" onClick={handleResetFilters}>
              Reset filters
            </button>
          </div>
        </form>

        {jsonlError ? (
          <p role="alert">
            <strong>{jsonlError}</strong>
          </p>
        ) : null}

        <p>
          Active filters: query=<code>{jsonlFilters.query || "(none)"}</code> source_file=
          <code>{jsonlFilters.sourceFile || "(all)"}</code> task_id=
          <code>{jsonlFilters.taskId || "(all)"}</code> phase_id=
          <code>{jsonlFilters.phaseId || "(all)"}</code> limit=<code>{jsonlFilters.limit}</code>
        </p>

        {filteredJsonlRows.length === 0 ? (
          <p>No rows match current filters.</p>
        ) : (
          <ol aria-label="JSONL filtered rows">
            {filteredJsonlRows.map((row) => {
              const expanded = Boolean(expandedJsonlRows[row.id]);
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    aria-expanded={expanded}
                    aria-controls={`jsonl-${row.id}`}
                    onClick={() => toggleJsonlExpanded(row.id)}
                  >
                    <span>{row.sourceFile}</span>
                    <span>{formatTimestamp(row.timestampHint)}</span>
                    <span>{row.preview}</span>
                  </button>
                  {expanded ? (
                    <div id={`jsonl-${row.id}`}>
                      <pre>{row.rawLine}</pre>
                      <button type="button">Copy line</button>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ol>
        )}
      </article>

      <aside aria-labelledby="artifacts-title">
        <h3 id="artifacts-title">Artifact Inspector</h3>
        <p>
          Panel state: <strong>{artifactPanelState}</strong>.
        </p>

        <form
          onSubmit={(event) => {
            event.preventDefault();
            handleDownloadSelectedFileId();
          }}
          aria-label="Artifact file id selector"
        >
          <label htmlFor="artifact-file-id">Artifact file ID</label>
          <input
            id="artifact-file-id"
            value={artifactFileId}
            aria-invalid={artifactFileIdError ? true : undefined}
            aria-describedby={artifactFileIdError ? "artifact-file-id-error" : "artifact-file-id-help"}
            onChange={(event) => setArtifactFileId(event.target.value)}
          />
          <p id="artifact-file-id-help">
            Source endpoint: <code>/api/v1/output/executions/:execution_id/files</code>.
          </p>
          {artifactFileIdError ? (
            <p id="artifact-file-id-error" role="alert">
              {artifactFileIdError}
            </p>
          ) : null}

          <div>
            <button type="submit">Download selected file id</button>
          </div>
        </form>

        <p aria-live="polite">{artifactStatusNotice}</p>

        {artifactMessage ? (
          <p role="alert">
            <strong>{artifactMessage}</strong>
          </p>
        ) : null}

        {artifacts.length === 0 ? (
          <p>No artifacts available for this execution id.</p>
        ) : (
          <table aria-label="Execution artifacts">
            <caption>
              Execution artifacts for <code>{executionId}</code> with deterministic per-row downloads.
            </caption>
            <thead>
              <tr>
                <th scope="col">artifact_id</th>
                <th scope="col">type</th>
                <th scope="col">size</th>
                <th scope="col">file</th>
                <th scope="col">download</th>
              </tr>
            </thead>
            <tbody>
              {artifacts.map((artifact) => {
                const disabled = artifact.downloadState === "downloading";
                const buttonLabel =
                  artifact.downloadState === "downloading"
                    ? "Downloading..."
                    : artifact.downloadState === "failed"
                      ? "Retry download"
                      : "Download";
                const actionAriaLabel =
                  buttonLabel === "Retry download"
                    ? `Retry download for artifact ${artifact.artifactId} ${artifact.fileName}`
                    : `Download artifact ${artifact.artifactId} ${artifact.fileName}`;

                return (
                  <tr key={artifact.artifactId}>
                    <td>{artifact.artifactId}</td>
                    <td>{artifact.artifactType}</td>
                    <td>{formatBytes(artifact.sizeBytes)}</td>
                    <td>
                      <span>{artifact.fileName}</span>
                      <br />
                      <small>{artifact.filePathHint}</small>
                    </td>
                    <td>
                      <button
                        type="button"
                        disabled={disabled}
                        aria-label={actionAriaLabel}
                        onClick={() => handleDownloadArtifact(artifact.artifactId)}
                      >
                        {buttonLabel}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </aside>
    </section>
  );
}
