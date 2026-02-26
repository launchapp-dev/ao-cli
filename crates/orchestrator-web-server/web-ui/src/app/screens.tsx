import { FormEvent, ReactNode, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";

import { useProjectContext } from "./project-context";
import { DiagnosticsPanel } from "./diagnostics-panel";
import { api, firstApiError, RequestJsonValue } from "../lib/api/client";
import { ApiError } from "../lib/api/envelope";
import { ResourceState, useApiResource } from "../lib/api/use-api-resource";
import { useDaemonEvents } from "../lib/events/use-daemon-events";
import { RunTelemetryEntry, useRunTelemetry } from "../lib/output/use-run-telemetry";

type DaemonAction = "start" | "pause" | "resume" | "stop" | "clear";

type DaemonActionConfig = {
  label: string;
  method: "POST" | "DELETE";
  path: string;
  impact: string;
  confirmationPhrase?: string;
};

type ActionRecord = {
  id: string;
  action: DaemonAction;
  mode: "execute" | "dry-run";
  outcome: "ok" | "error" | "preview";
  timestamp: string;
  detail: string;
};

type OutputJsonlState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ok"; data: OutputRunJsonlPayload }
  | { kind: "error"; error: ApiError };

type OutputArtifactsState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ok"; data: OutputArtifactInfo[] }
  | { kind: "error"; error: ApiError };

type DownloadArtifactState =
  | { kind: "idle" }
  | { kind: "downloading"; artifactId: string }
  | { kind: "ok"; message: string }
  | { kind: "error"; error: ApiError };

type OutputRunJsonlPayload = {
  run_id: string;
  total_matches: number;
  truncated: boolean;
  entries: RunTelemetryEntry[];
};

type OutputArtifactInfo = {
  artifact_id: string;
  artifact_type: string;
  file_path?: string;
  size_bytes?: number;
};

const DAEMON_ACTION_CONFIG: Record<DaemonAction, DaemonActionConfig> = {
  start: {
    label: "Start",
    method: "POST",
    path: "/api/v1/daemon/start",
    impact: "Starts daemon processing.",
  },
  pause: {
    label: "Pause",
    method: "POST",
    path: "/api/v1/daemon/pause",
    impact: "Pauses new daemon scheduling work.",
  },
  resume: {
    label: "Resume",
    method: "POST",
    path: "/api/v1/daemon/resume",
    impact: "Resumes daemon scheduling work.",
  },
  stop: {
    label: "Stop",
    method: "POST",
    path: "/api/v1/daemon/stop",
    impact: "Stops the daemon and can interrupt active scheduling.",
    confirmationPhrase: "STOP",
  },
  clear: {
    label: "Clear Logs",
    method: "DELETE",
    path: "/api/v1/daemon/logs",
    impact: "Permanently clears daemon log history shown in the UI.",
    confirmationPhrase: "CLEAR LOGS",
  },
};

export function matchesConfirmationPhrase(input: string, expected: string): boolean {
  return input.trim().replace(/\s+/g, " ").toUpperCase() === expected.toUpperCase();
}

export function DashboardPage() {
  const state = useApiResource(
    async () => {
      const [systemInfo, daemonStatus, activeProject, taskStats] = await Promise.all([
        api.systemInfo(),
        api.daemonStatus(),
        api.projectsActive(),
        api.tasksStats(),
      ]);

      const error = firstApiError(systemInfo, daemonStatus, activeProject, taskStats);
      if (error) {
        return error;
      }

      return {
        kind: "ok" as const,
        data: {
          systemInfo: systemInfo.data,
          daemonStatus: daemonStatus.data,
          activeProject: activeProject.data,
          taskStats: taskStats.data,
        },
      };
    },
    [],
  );

  return (
    <RouteSection title="Dashboard" description="Global status overview for AO daemon and workspace.">
      <ResourceStateView
        state={state}
        emptyMessage="No dashboard data returned."
        render={(data) => (
          <div className="grid two">
            <JsonPanel title="System Info" data={data.systemInfo} />
            <JsonPanel title="Daemon Status" data={data.daemonStatus} />
            <JsonPanel title="Active Project" data={data.activeProject} />
            <JsonPanel title="Task Stats" data={data.taskStats} />
          </div>
        )}
      />
    </RouteSection>
  );
}

export function DaemonPage() {
  const [refreshNonce, setRefreshNonce] = useState(0);
  const [pendingAction, setPendingAction] = useState<DaemonAction | null>(null);
  const [confirmationInput, setConfirmationInput] = useState("");
  const [dryRunMode, setDryRunMode] = useState(true);
  const [actionRecords, setActionRecords] = useState<ActionRecord[]>([]);
  const [actionState, setActionState] = useState<
    { kind: "idle" } | { kind: "ok"; message: string } | { kind: "error"; message: string }
  >({ kind: "idle" });

  const healthState = useApiResource(
    async () => api.daemonHealth(),
    [refreshNonce],
  );

  const logsState = useApiResource(
    async () => api.daemonLogs(100),
    [refreshNonce],
    {
      isEmpty: (data) => Array.isArray(data) && data.length === 0,
    },
  );

  const runAction = (action: DaemonAction, runAsDryRun = false) => {
    const config = DAEMON_ACTION_CONFIG[action];
    if (runAsDryRun) {
      const previewMessage = `Dry-run preview completed for ${config.label}.`;
      setActionState({ kind: "ok", message: previewMessage });
      setActionRecords((current) => [
        {
          id: `${Date.now()}-${action}-preview`,
          action,
          mode: "dry-run",
          outcome: "preview",
          timestamp: new Date().toISOString(),
          detail: `${config.method} ${config.path} (${config.impact})`,
        },
        ...current,
      ]);
      setPendingAction(null);
      setConfirmationInput("");
      return;
    }

    const request =
      action === "start"
        ? api.daemonStart()
        : action === "pause"
          ? api.daemonPause()
          : action === "resume"
            ? api.daemonResume()
            : action === "stop"
              ? api.daemonStop()
              : api.daemonClearLogs();

    void request.then((result) => {
      if (result.kind === "error") {
        setActionState({ kind: "error", message: `${result.code}: ${result.message}` });
        setActionRecords((current) => [
          {
            id: `${Date.now()}-${action}-error`,
            action,
            mode: "execute",
            outcome: "error",
            timestamp: new Date().toISOString(),
            detail: `${result.code}: ${result.message}`,
          },
          ...current,
        ]);
        return;
      }

      setActionState({ kind: "ok", message: `${config.label} completed.` });
      setActionRecords((current) => [
        {
          id: `${Date.now()}-${action}-ok`,
          action,
          mode: "execute",
          outcome: "ok",
          timestamp: new Date().toISOString(),
          detail: `${config.method} ${config.path}`,
        },
        ...current,
      ]);
      setRefreshNonce((current) => current + 1);
      setPendingAction(null);
      setConfirmationInput("");
    });
  };

  const requestAction = (action: DaemonAction) => {
    const config = DAEMON_ACTION_CONFIG[action];
    if (!config.confirmationPhrase) {
      runAction(action, false);
      return;
    }

    setPendingAction(action);
    setConfirmationInput("");
    setDryRunMode(true);
  };

  const pendingConfig = pendingAction ? DAEMON_ACTION_CONFIG[pendingAction] : null;
  const canConfirm = pendingConfig?.confirmationPhrase
    ? matchesConfirmationPhrase(confirmationInput, pendingConfig.confirmationPhrase)
    : false;

  return (
    <RouteSection title="Daemon" description="Control daemon state, health, and log stream.">
      <div className="panel-actions">
        <button type="button" onClick={() => requestAction("start")}>
          Start
        </button>
        <button type="button" onClick={() => requestAction("pause")}>
          Pause
        </button>
        <button type="button" onClick={() => requestAction("resume")}>
          Resume
        </button>
        <button type="button" className="danger-action" onClick={() => requestAction("stop")}>
          Stop
        </button>
        <button type="button" className="danger-action" onClick={() => requestAction("clear")}>
          Clear Logs
        </button>
      </div>

      {pendingConfig ? (
        <section className="safeguard-panel" aria-label="Action safeguards">
          <h2>Review High-Risk Action</h2>
          <p>
            <strong>Action:</strong> {pendingConfig.label}
          </p>
          <p>
            <strong>Preview:</strong> <code>{pendingConfig.method}</code> <code>{pendingConfig.path}</code>
          </p>
          <p>{pendingConfig.impact}</p>
          <label>
            Confirmation phrase
            <input
              value={confirmationInput}
              onChange={(event) => setConfirmationInput(event.target.value)}
              placeholder={pendingConfig.confirmationPhrase}
              aria-describedby="confirmation-help"
            />
          </label>
          <p id="confirmation-help">
            Type <code>{pendingConfig.confirmationPhrase}</code> to enable confirmation.
          </p>
          <label className="inline-checkbox">
            <input
              type="checkbox"
              checked={dryRunMode}
              onChange={(event) => setDryRunMode(event.target.checked)}
            />
            Preview only (dry-run, no API call)
          </label>
          <div className="panel-actions">
            <button type="button" disabled={!canConfirm} onClick={() => runAction(pendingAction!, dryRunMode)}>
              {dryRunMode ? "Run Dry-Run Preview" : "Confirm and Execute"}
            </button>
            <button
              type="button"
              onClick={() => {
                setPendingAction(null);
                setConfirmationInput("");
              }}
            >
              Cancel
            </button>
          </div>
        </section>
      ) : null}

      {actionState.kind === "ok" ? (
        <p role="status" aria-live="polite">
          {actionState.message}
        </p>
      ) : null}
      {actionState.kind === "error" ? (
        <ErrorState
          error={{
            kind: "error",
            code: "daemon_action_failed",
            message: actionState.message,
            exitCode: 1,
          }}
        />
      ) : null}

      <DiagnosticsPanel title="Daemon Diagnostics" actionPrefixes={["daemon."]} />

      <div className="grid two">
        <div className="panel">
          <h2>Action Audit Trail</h2>
          {actionRecords.length === 0 ? (
            <EmptyState message="No daemon actions recorded in this session." />
          ) : (
            <pre>{JSON.stringify(actionRecords.slice(0, 20), null, 2)}</pre>
          )}
        </div>

        <ResourceStateView
          state={healthState}
          emptyMessage="No daemon health response available."
          render={(data) => <JsonPanel title="Health" data={data} />}
        />

        <ResourceStateView
          state={logsState}
          emptyMessage="No daemon logs returned yet."
          render={(data) => <JsonPanel title="Logs" data={data} />}
        />
      </div>
    </RouteSection>
  );
}

export function ProjectsPage() {
  const { activeProjectId, projects, source } = useProjectContext();

  const requirementsState = useApiResource(
    async () => api.projectsRequirementsSummary(),
    [],
    {
      isEmpty: (data) => Array.isArray(data) && data.length === 0,
    },
  );

  return (
    <RouteSection
      title="Projects"
      description="Workspace projects with context selection and requirement summaries."
    >
      <p>
        Active context: <code>{activeProjectId ?? "none"}</code> (<code>{source}</code>)
      </p>
      <div className="grid two">
        <div className="panel">
          <h2>Projects</h2>
          {projects.length === 0 ? (
            <EmptyState message="No projects available in context." />
          ) : (
            <ul>
              {projects.map((project) => (
                <li key={project.id}>
                  <Link to={`/projects/${project.id}`}>{project.name}</Link>
                </li>
              ))}
            </ul>
          )}
        </div>

        <ResourceStateView
          state={requirementsState}
          emptyMessage="No requirement summary data found."
          render={(data) => <JsonPanel title="Requirement Summary" data={data} />}
        />
      </div>
    </RouteSection>
  );
}

export function ProjectDetailPage() {
  const params = useParams();
  const projectId = params.projectId ?? "";

  const state = useApiResource(
    async () => {
      const [project, tasks, workflows, requirements] = await Promise.all([
        api.projectsById(projectId),
        api.projectTasks(projectId),
        api.projectWorkflows(projectId),
        api.projectsRequirementsById(projectId),
      ]);

      const error = firstApiError(project, tasks, workflows, requirements);
      if (error) {
        return error;
      }

      return {
        kind: "ok" as const,
        data: {
          project: project.data,
          tasks: tasks.data,
          workflows: workflows.data,
          requirements: requirements.data,
        },
      };
    },
    [projectId],
  );

  return (
    <RouteSection title="Project Detail" description={`Project scope for ${projectId}.`}>
      <ResourceStateView
        state={state}
        emptyMessage="Project details are empty."
        render={(data) => (
          <div className="grid two">
            <JsonPanel title="Project" data={data.project} />
            <JsonPanel title="Tasks" data={data.tasks} />
            <JsonPanel title="Workflows" data={data.workflows} />
            <JsonPanel title="Requirements" data={data.requirements} />
          </div>
        )}
      />
    </RouteSection>
  );
}

export function RequirementDetailPage() {
  const params = useParams();
  const projectId = params.projectId ?? "";
  const requirementId = params.requirementId ?? "";

  const state = useApiResource(
    async () => api.projectRequirementDetail(projectId, requirementId),
    [projectId, requirementId],
  );

  return (
    <RouteSection
      title="Requirement Detail"
      description={`Requirement ${requirementId} within project ${projectId}.`}
    >
      <ResourceStateView
        state={state}
        emptyMessage="Requirement detail payload is empty."
        render={(data) => (
          <div className="grid">
            <JsonPanel title="Requirement" data={data} />
            <p>
              <Link to={`/planning/requirements/${encodeURIComponent(requirementId)}`}>
                Edit in Planning Workspace
              </Link>
            </p>
          </div>
        )}
      />
    </RouteSection>
  );
}

export function TasksPage() {
  const state = useApiResource(
    async () => {
      const [tasks, stats] = await Promise.all([api.tasksList(), api.tasksStats()]);
      const error = firstApiError(tasks, stats);
      if (error) {
        return error;
      }

      return {
        kind: "ok" as const,
        data: {
          tasks: tasks.data,
          stats: stats.data,
        },
      };
    },
    [],
  );

  return (
    <RouteSection title="Tasks" description="Task explorer with stats and detail entry points.">
      <ResourceStateView
        state={state}
        emptyMessage="No tasks returned."
        render={(data) => (
          <div className="grid two">
            <JsonPanel title="Tasks" data={data.tasks} />
            <JsonPanel title="Task Stats" data={data.stats} />
          </div>
        )}
      />
    </RouteSection>
  );
}

export function TaskDetailPage() {
  const params = useParams();
  const taskId = params.taskId ?? "";

  const state = useApiResource(
    async () => api.tasksById(taskId),
    [taskId],
  );

  return (
    <RouteSection title="Task Detail" description={`Task ${taskId}.`}>
      <ResourceStateView
        state={state}
        emptyMessage="Task detail payload is empty."
        render={(data) => <JsonPanel title="Task" data={data} />}
      />
    </RouteSection>
  );
}

export function WorkflowsPage() {
  const state = useApiResource(
    async () => api.workflowsList(),
    [],
    {
      isEmpty: (data) => Array.isArray(data) && data.length === 0,
    },
  );

  return (
    <RouteSection title="Workflows" description="Workflow list and execution entrypoint.">
      <ResourceStateView
        state={state}
        emptyMessage="No workflow records returned."
        render={(data) => <JsonPanel title="Workflows" data={data} />}
      />
    </RouteSection>
  );
}

export function WorkflowDetailPage() {
  const params = useParams();
  const workflowId = params.workflowId ?? "";

  const state = useApiResource(
    async () => {
      const [workflow, decisions, checkpoints] = await Promise.all([
        api.workflowsById(workflowId),
        api.workflowDecisions(workflowId),
        api.workflowCheckpoints(workflowId),
      ]);
      const error = firstApiError(workflow, decisions, checkpoints);
      if (error) {
        return error;
      }

      return {
        kind: "ok" as const,
        data: {
          workflow: workflow.data,
          decisions: decisions.data,
          checkpoints: checkpoints.data,
        },
      };
    },
    [workflowId],
  );

  return (
    <RouteSection title="Workflow Detail" description={`Workflow ${workflowId}.`}>
      <ResourceStateView
        state={state}
        emptyMessage="Workflow detail payload is empty."
        render={(data) => (
          <div className="grid two">
            <JsonPanel title="Workflow" data={data.workflow} />
            <JsonPanel title="Decisions" data={data.decisions} />
            <JsonPanel title="Checkpoints" data={data.checkpoints} />
          </div>
        )}
      />
    </RouteSection>
  );
}

export function WorkflowCheckpointPage() {
  const params = useParams();
  const workflowId = params.workflowId ?? "";
  const checkpoint = params.checkpoint ?? "";

  const state = useApiResource(
    async () => api.workflowCheckpointById(workflowId, checkpoint),
    [workflowId, checkpoint],
  );

  return (
    <RouteSection
      title="Checkpoint Detail"
      description={`Checkpoint ${checkpoint} for workflow ${workflowId}.`}
    >
      <ResourceStateView
        state={state}
        emptyMessage="Checkpoint payload is empty."
        render={(data) => <JsonPanel title="Checkpoint" data={data} />}
      />
    </RouteSection>
  );
}

export function EventsPage() {
  const { connectionState, events } = useDaemonEvents();

  const mostRecentEvents = useMemo(() => {
    return [...events].reverse().slice(0, 25);
  }, [events]);

  return (
    <RouteSection
      title="Events"
      description="Live daemon event stream with reconnect and Last-Event-ID resume."
    >
      <p>
        Stream state: <code>{connectionState}</code>
      </p>
      <div aria-live="polite" aria-atomic="false" className="panel">
        <h2>Latest Events</h2>
        {mostRecentEvents.length === 0 ? (
          <EmptyState message="No daemon events have been received yet." />
        ) : (
          <pre>{JSON.stringify(mostRecentEvents, null, 2)}</pre>
        )}
      </div>
    </RouteSection>
  );
}

export function OutputPage() {
  const [runId, setRunId] = useState("");
  const [taskIdFilter, setTaskIdFilter] = useState("");
  const [phaseIdFilter, setPhaseIdFilter] = useState("");
  const [containsFilter, setContainsFilter] = useState("");
  const [sourceFileFilter, setSourceFileFilter] = useState("");
  const [jsonlState, setJsonlState] = useState<OutputJsonlState>({ kind: "idle" });
  const [executionId, setExecutionId] = useState("");
  const [artifactsState, setArtifactsState] = useState<OutputArtifactsState>({ kind: "idle" });
  const [downloadState, setDownloadState] = useState<DownloadArtifactState>({ kind: "idle" });

  const telemetry = useRunTelemetry({
    runId,
    taskId: taskIdFilter,
    phaseId: phaseIdFilter,
    enabled: runId.trim().length > 0,
  });

  const recentTelemetryEntries = useMemo(() => {
    return telemetry.entries.slice(Math.max(telemetry.entries.length - 50, 0));
  }, [telemetry.entries]);

  const searchJsonl = () => {
    const normalizedRunId = runId.trim();
    if (!normalizedRunId) {
      setJsonlState({
        kind: "error",
        error: {
          kind: "error",
          code: "invalid_input",
          message: "Run ID is required to search JSONL output.",
          exitCode: 2,
        },
      });
      return;
    }

    setJsonlState({ kind: "loading" });
    void api
      .outputRunJsonl(normalizedRunId, {
        contains: normalizeOptionalText(containsFilter),
        sourceFile: normalizeOptionalText(sourceFileFilter),
        taskId: normalizeOptionalText(taskIdFilter),
        phaseId: normalizeOptionalText(phaseIdFilter),
        limit: 500,
      })
      .then((result) => {
        if (result.kind === "error") {
          setJsonlState({ kind: "error", error: result });
          return;
        }

        const decoded = decodeOutputRunJsonlPayload(result.data);
        if (!decoded) {
          setJsonlState({
            kind: "error",
            error: {
              kind: "error",
              code: "invalid_payload",
              message: "JSONL response payload did not match expected shape.",
              exitCode: 1,
            },
          });
          return;
        }

        setJsonlState({ kind: "ok", data: decoded });
      });
  };

  const loadArtifacts = () => {
    const normalizedExecutionId = executionId.trim();
    if (!normalizedExecutionId) {
      setArtifactsState({
        kind: "error",
        error: {
          kind: "error",
          code: "invalid_input",
          message: "Execution ID is required to inspect artifacts.",
          exitCode: 2,
        },
      });
      return;
    }

    setArtifactsState({ kind: "loading" });
    void api.outputArtifacts(normalizedExecutionId).then((result) => {
      if (result.kind === "error") {
        setArtifactsState({ kind: "error", error: result });
        return;
      }

      const decoded = decodeOutputArtifactList(result.data);
      if (!decoded) {
        setArtifactsState({
          kind: "error",
          error: {
            kind: "error",
            code: "invalid_payload",
            message: "Artifact list payload did not match expected shape.",
            exitCode: 1,
          },
        });
        return;
      }

      setArtifactsState({ kind: "ok", data: decoded });
      setDownloadState({ kind: "idle" });
    });
  };

  const downloadArtifact = (artifactId: string) => {
    const normalizedExecutionId = executionId.trim();
    if (!normalizedExecutionId) {
      setDownloadState({
        kind: "error",
        error: {
          kind: "error",
          code: "invalid_input",
          message: "Execution ID is required to download artifacts.",
          exitCode: 2,
        },
      });
      return;
    }

    setDownloadState({ kind: "downloading", artifactId });
    void api.outputArtifactDownload(normalizedExecutionId, artifactId).then((result) => {
      if (result.kind === "error") {
        setDownloadState({ kind: "error", error: result });
        return;
      }

      const decoded = decodeArtifactDownloadPayload(result.data);
      if (!decoded) {
        setDownloadState({
          kind: "error",
          error: {
            kind: "error",
            code: "invalid_payload",
            message: "Artifact download payload did not include byte content.",
            exitCode: 1,
          },
        });
        return;
      }

      triggerBrowserDownload(decoded.bytes, artifactId);
      setDownloadState({
        kind: "ok",
        message: `Downloaded ${artifactId} (${decoded.size_bytes} bytes).`,
      });
    });
  };

  return (
    <RouteSection
      title="Output"
      description="Run telemetry stream with poll fallback, JSONL search/filter, and artifact browsing."
    >
      <div className="grid">
        <section className="panel">
          <h2>Run Telemetry</h2>
          <div className="grid two">
            <label>
              Run ID
              <input
                placeholder="run-123"
                value={runId}
                onChange={(event) => setRunId(event.target.value)}
              />
            </label>
            <label>
              Task ID Filter
              <input
                placeholder="TASK-015"
                value={taskIdFilter}
                onChange={(event) => setTaskIdFilter(event.target.value)}
              />
            </label>
            <label>
              Phase ID Filter
              <input
                placeholder="implementation"
                value={phaseIdFilter}
                onChange={(event) => setPhaseIdFilter(event.target.value)}
              />
            </label>
          </div>
          <p>
            Connection state: <code>{telemetry.connectionState}</code>
          </p>
          <p>
            Events shown: <code>{recentTelemetryEntries.length}</code>
          </p>
          {telemetry.errorMessage ? (
            <p role="status" aria-live="polite">
              {telemetry.errorMessage}
            </p>
          ) : null}
          {recentTelemetryEntries.length === 0 ? (
            <EmptyState message="No run telemetry received yet. Enter a run ID to begin streaming." />
          ) : (
            <pre>{JSON.stringify(recentTelemetryEntries, null, 2)}</pre>
          )}
        </section>

        <section className="panel">
          <h2>JSONL Search and Filter</h2>
          <div className="grid two">
            <label>
              Source File
              <input
                placeholder="events.jsonl"
                value={sourceFileFilter}
                onChange={(event) => setSourceFileFilter(event.target.value)}
              />
            </label>
            <label>
              Contains Text
              <input
                placeholder="workflow-run"
                value={containsFilter}
                onChange={(event) => setContainsFilter(event.target.value)}
              />
            </label>
          </div>
          <div className="panel-actions">
            <button type="button" onClick={searchJsonl} disabled={jsonlState.kind === "loading"}>
              {jsonlState.kind === "loading" ? "Searching..." : "Search JSONL"}
            </button>
          </div>

          {jsonlState.kind === "error" ? <ErrorState error={jsonlState.error} /> : null}
          {jsonlState.kind === "ok" ? (
            <>
              <p>
                Matches: <code>{jsonlState.data.total_matches}</code>
                {jsonlState.data.truncated ? " (truncated to 500 entries)" : ""}
              </p>
              {jsonlState.data.entries.length === 0 ? (
                <EmptyState message="No JSONL entries matched the current filters." />
              ) : (
                <pre>{JSON.stringify(jsonlState.data.entries, null, 2)}</pre>
              )}
            </>
          ) : null}
        </section>

        <section className="panel">
          <h2>Artifact Inspector</h2>
          <div className="grid two">
            <label>
              Execution ID
              <input
                placeholder="exec-123"
                value={executionId}
                onChange={(event) => setExecutionId(event.target.value)}
              />
            </label>
          </div>
          <div className="panel-actions">
            <button type="button" onClick={loadArtifacts} disabled={artifactsState.kind === "loading"}>
              {artifactsState.kind === "loading" ? "Loading..." : "Load Artifacts"}
            </button>
          </div>

          {artifactsState.kind === "error" ? <ErrorState error={artifactsState.error} /> : null}
          {downloadState.kind === "error" ? <ErrorState error={downloadState.error} /> : null}
          {downloadState.kind === "ok" ? (
            <p role="status" aria-live="polite">
              {downloadState.message}
            </p>
          ) : null}

          {artifactsState.kind === "ok" ? (
            artifactsState.data.length === 0 ? (
              <EmptyState message="No artifacts found for this execution ID." />
            ) : (
              <ul className="output-artifact-list">
                {artifactsState.data.map((artifact) => (
                  <li key={artifact.artifact_id} className="output-artifact-row">
                    <div>
                      <p>
                        <strong>{artifact.artifact_id}</strong>
                      </p>
                      <p className="output-meta">
                        type <code>{artifact.artifact_type}</code>
                        {typeof artifact.size_bytes === "number"
                          ? `, size ${artifact.size_bytes} bytes`
                          : ""}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => downloadArtifact(artifact.artifact_id)}
                      disabled={
                        downloadState.kind === "downloading" &&
                        downloadState.artifactId === artifact.artifact_id
                      }
                    >
                      {downloadState.kind === "downloading" &&
                      downloadState.artifactId === artifact.artifact_id
                        ? "Downloading..."
                        : "Download"}
                    </button>
                  </li>
                ))}
              </ul>
            )
          ) : null}
        </section>
      </div>
    </RouteSection>
  );
}

export function ReviewHandoffPage() {
  const [runId, setRunId] = useState("");
  const [targetRole, setTargetRole] = useState("em");
  const [question, setQuestion] = useState("");
  const [contextJson, setContextJson] = useState("{}");
  const [submitState, setSubmitState] = useState<
    { kind: "idle" } | { kind: "ok"; data: unknown } | { kind: "error"; error: ApiError }
  >({ kind: "idle" });

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    let contextPayload: unknown;
    try {
      contextPayload = JSON.parse(contextJson || "{}");
    } catch {
      setSubmitState({
        kind: "error",
        error: {
          kind: "error",
          code: "invalid_context",
          message: "Context must be valid JSON.",
          exitCode: 2,
        },
      });
      return;
    }

    if (!isJsonValue(contextPayload)) {
      setSubmitState({
        kind: "error",
        error: {
          kind: "error",
          code: "invalid_context",
          message: "Context must be a valid JSON value.",
          exitCode: 2,
        },
      });
      return;
    }

    void api
      .reviewHandoff({
        run_id: runId,
        target_role: targetRole,
        question,
        context: contextPayload,
      })
      .then((result) => {
        if (result.kind === "error") {
          setSubmitState({ kind: "error", error: result });
          return;
        }

        setSubmitState({ kind: "ok", data: result.data });
      });
  };

  return (
    <RouteSection title="Review Handoff" description="Submit review handoff payloads to AO.">
      <form className="panel grid" onSubmit={onSubmit}>
        <label>
          Run ID
          <input value={runId} onChange={(event) => setRunId(event.target.value)} />
        </label>

        <label>
          Target Role
          <select value={targetRole} onChange={(event) => setTargetRole(event.target.value)}>
            <option value="em">em</option>
            <option value="reviewer">reviewer</option>
            <option value="qa">qa</option>
          </select>
        </label>

        <label>
          Question
          <textarea
            rows={3}
            required
            value={question}
            onChange={(event) => setQuestion(event.target.value)}
          />
        </label>

        <label>
          Context JSON
          <textarea
            rows={4}
            value={contextJson}
            onChange={(event) => setContextJson(event.target.value)}
          />
        </label>

        <div className="panel-actions">
          <button type="submit">Submit Handoff</button>
        </div>
      </form>

      {submitState.kind === "error" ? <ErrorState error={submitState.error} /> : null}
      {submitState.kind === "ok" ? <JsonPanel title="Response" data={submitState.data} /> : null}
      <DiagnosticsPanel
        title="Review Handoff Diagnostics"
        actionPrefixes={["reviews.handoff"]}
      />
    </RouteSection>
  );
}

export function NotFoundPage() {
  return (
    <RouteSection title="Not Found" description="The requested route does not exist.">
      <ErrorState
        error={{
          kind: "error",
          code: "not_found",
          message: "Unknown route. Return to the dashboard.",
          exitCode: 3,
        }}
      />
      <p>
        <Link to="/dashboard">Go to dashboard</Link>
      </p>
    </RouteSection>
  );
}

function RouteSection(props: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="panel" aria-label={props.title}>
      <h1>{props.title}</h1>
      <p>{props.description}</p>
      {props.children}
    </section>
  );
}

function ResourceStateView<TData>(props: {
  state: ResourceState<TData>;
  emptyMessage: string;
  render: (data: TData) => ReactNode;
}) {
  if (props.state.status === "loading") {
    return <LoadingState message="Loading data..." />;
  }

  if (props.state.status === "error") {
    return <ErrorState error={props.state.error} />;
  }

  if (props.state.status === "empty") {
    return <EmptyState message={props.emptyMessage} />;
  }

  return <>{props.render(props.state.data)}</>;
}

function LoadingState(props: { message: string }) {
  return <div className="loading-box">{props.message}</div>;
}

function EmptyState(props: { message: string }) {
  return <div className="empty-box">{props.message}</div>;
}

function ErrorState(props: { error: ApiError }) {
  return (
    <div className="error-box" role="alert">
      <strong>Error:</strong> {props.error.code}
      <div>{props.error.message}</div>
      <div>exit code {props.error.exitCode}</div>
    </div>
  );
}

function JsonPanel(props: { title: string; data: unknown }) {
  return (
    <div className="panel">
      <h2>{props.title}</h2>
      <pre>{JSON.stringify(props.data, null, 2)}</pre>
    </div>
  );
}

function normalizeOptionalText(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function decodeOutputRunJsonlPayload(value: unknown): OutputRunJsonlPayload | null {
  if (!isRecord(value)) {
    return null;
  }

  const runId = value["run_id"];
  const totalMatches = value["total_matches"];
  const truncated = value["truncated"];
  const entriesValue = value["entries"];
  if (typeof runId !== "string") {
    return null;
  }
  if (typeof totalMatches !== "number" || !Number.isFinite(totalMatches)) {
    return null;
  }
  if (typeof truncated !== "boolean") {
    return null;
  }
  if (!Array.isArray(entriesValue)) {
    return null;
  }

  const entries = entriesValue.filter(isRunTelemetryEntry);
  return {
    run_id: runId,
    total_matches: totalMatches,
    truncated,
    entries,
  };
}

function isRunTelemetryEntry(value: unknown): value is RunTelemetryEntry {
  if (!isRecord(value)) {
    return false;
  }

  const cursor = value["cursor"];
  const sourceFile = value["source_file"];
  const line = value["line"];
  return (
    typeof cursor === "number" &&
    Number.isFinite(cursor) &&
    cursor > 0 &&
    typeof sourceFile === "string" &&
    typeof line === "string"
  );
}

function decodeOutputArtifactList(value: unknown): OutputArtifactInfo[] | null {
  if (!Array.isArray(value)) {
    return null;
  }

  const artifacts: OutputArtifactInfo[] = [];
  for (const item of value) {
    if (!isRecord(item)) {
      continue;
    }

    const artifactId = item["artifact_id"];
    const artifactType = item["artifact_type"];
    const filePath = item["file_path"];
    const sizeBytes = item["size_bytes"];
    if (typeof artifactId !== "string" || typeof artifactType !== "string") {
      continue;
    }

    artifacts.push({
      artifact_id: artifactId,
      artifact_type: artifactType,
      ...(typeof filePath === "string" ? { file_path: filePath } : {}),
      ...(typeof sizeBytes === "number" && Number.isFinite(sizeBytes)
        ? { size_bytes: sizeBytes }
        : {}),
    });
  }

  return artifacts;
}

function decodeArtifactDownloadPayload(value: unknown): {
  size_bytes: number;
  bytes: Uint8Array;
} | null {
  if (!isRecord(value)) {
    return null;
  }

  const sizeBytes = value["size_bytes"];
  const rawBytes = value["bytes"];
  if (typeof sizeBytes !== "number" || !Number.isFinite(sizeBytes) || !Array.isArray(rawBytes)) {
    return null;
  }

  const bytes = new Uint8Array(rawBytes.length);
  for (let index = 0; index < rawBytes.length; index += 1) {
    const value = rawBytes[index];
    if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > 255) {
      return null;
    }
    bytes[index] = value;
  }

  return {
    size_bytes: sizeBytes,
    bytes,
  };
}

function triggerBrowserDownload(bytes: Uint8Array, artifactId: string) {
  const blob = new Blob([bytes]);
  const objectUrl = window.URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = objectUrl;
  anchor.download = artifactId.split("/").pop() ?? artifactId;
  anchor.rel = "noopener";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.URL.revokeObjectURL(objectUrl);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isJsonValue(value: unknown): value is RequestJsonValue {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return true;
  }

  if (Array.isArray(value)) {
    return value.every((item) => isJsonValue(item));
  }

  if (typeof value === "object" && value !== null) {
    return Object.values(value).every((item) => isJsonValue(item));
  }

  return false;
}
