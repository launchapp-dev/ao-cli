import { FormEvent, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery, useMutation } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsTrigger, TabsList } from "@/components/ui/tabs";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  WorkflowsDocument,
  WorkflowDetailDocument,
  RunWorkflowDocument,
  PauseWorkflowDocument,
  ResumeWorkflowDocument,
  CancelWorkflowDocument,
  ApprovePhaseDocument,
} from "@/lib/graphql/generated/graphql";
import { statusColor, PageLoading, PageError, StatCard } from "./shared";

export function WorkflowsPage() {
  const [statusFilter, setStatusFilter] = useState<string>("");
  const [result, reexecute] = useQuery({
    query: WorkflowsDocument,
    variables: { status: statusFilter || undefined },
  });
  const [, runWf] = useMutation(RunWorkflowDocument);
  const [, pauseWf] = useMutation(PauseWorkflowDocument);
  const [, resumeWf] = useMutation(ResumeWorkflowDocument);
  const [, cancelWf] = useMutation(CancelWorkflowDocument);
  const [runTaskId, setRunTaskId] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const { data, fetching, error } = result;
  const workflows = data?.workflows ?? [];

  const counts = useMemo(() => {
    const c = { running: 0, paused: 0, completed: 0, failed: 0 };
    for (const w of workflows) {
      const s = (w.statusRaw ?? "").toLowerCase();
      if (s === "running") c.running++;
      else if (s === "paused") c.paused++;
      else if (s === "completed") c.completed++;
      else if (s === "failed") c.failed++;
    }
    return c;
  }, [workflows]);

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const onRun = async (e: FormEvent) => {
    e.preventDefault();
    if (!runTaskId.trim()) return;
    const { error: err } = await runWf({ taskId: runTaskId.trim() });
    if (err) setFeedback({ kind: "error", message: err.message });
    else {
      setFeedback({ kind: "ok", message: `Workflow started for ${runTaskId}.` });
      setRunTaskId("");
      reexecute({ requestPolicy: "network-only" });
    }
  };

  const onAction = async (id: string, action: "pause" | "resume" | "cancel") => {
    const fn = action === "pause" ? pauseWf : action === "resume" ? resumeWf : cancelWf;
    const { error: err } = await fn({ id });
    if (err) setFeedback({ kind: "error", message: err.message });
    else {
      setFeedback({ kind: "ok", message: `${action} applied to ${id}.` });
      reexecute({ requestPolicy: "network-only" });
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Workflows</h1>
        <span className="text-sm text-muted-foreground">{workflows.length} total</span>
      </div>

      <div className="grid grid-cols-4 gap-2">
        <StatCard label="Running" value={counts.running} />
        <StatCard label="Paused" value={counts.paused} />
        <StatCard label="Completed" value={counts.completed} />
        <StatCard label="Failed" value={counts.failed} />
      </div>

      <div className="flex gap-2 items-end">
        <form onSubmit={onRun} className="flex gap-2 items-end">
          <Input
            placeholder="Task ID (e.g. TASK-014)"
            value={runTaskId}
            onChange={(e) => setRunTaskId(e.target.value)}
            className="w-48"
          />
          <Button type="submit" size="sm">Run Workflow</Button>
        </form>
      </div>

      {feedback && (
        <Alert variant={feedback.kind === "error" ? "destructive" : "default"}>
          <AlertDescription>{feedback.message}</AlertDescription>
        </Alert>
      )}

      <Tabs defaultValue="all" onValueChange={(v) => setStatusFilter(v === "all" ? "" : v)}>
        <TabsList>
          <TabsTrigger value="all">All</TabsTrigger>
          <TabsTrigger value="running">Running</TabsTrigger>
          <TabsTrigger value="paused">Paused</TabsTrigger>
          <TabsTrigger value="completed">Completed</TabsTrigger>
          <TabsTrigger value="failed">Failed</TabsTrigger>
        </TabsList>
      </Tabs>

      {workflows.length === 0 ? (
        <p className="text-sm text-muted-foreground py-8 text-center">No workflows found.</p>
      ) : (
        <div className="space-y-3">
          {workflows.map((wf) => (
            <Card key={wf.id}>
              <CardContent className="pt-4">
                <div className="flex items-start justify-between">
                  <div>
                    <Link to={`/workflows/${wf.id}`} className="font-mono text-sm underline">{wf.id}</Link>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      Task: <Link to={`/tasks/${wf.taskId}`} className="underline">{wf.taskId}</Link>
                      {wf.currentPhase && <> &middot; Phase: {wf.currentPhase}</>}
                      {(wf.totalReworks ?? 0) > 0 && <> &middot; {wf.totalReworks} reworks</>}
                    </p>
                  </div>
                  <Badge variant={statusColor(wf.statusRaw ?? "")}>{wf.statusRaw}</Badge>
                </div>

                {(wf.phases ?? []).length > 0 && (
                  <div className="flex gap-1 mt-2 flex-wrap">
                    {wf.phases!.map((p) => (
                      <Badge key={p.phaseId} variant={statusColor(p.status ?? "")} className="text-[10px] font-mono">
                        {p.phaseId}
                      </Badge>
                    ))}
                  </div>
                )}

                <div className="flex gap-1 mt-2">
                  <Button size="sm" variant="outline" onClick={() => onAction(wf.id, "pause")} disabled={wf.statusRaw !== "running"}>Pause</Button>
                  <Button size="sm" variant="outline" onClick={() => onAction(wf.id, "resume")} disabled={wf.statusRaw !== "paused"}>Resume</Button>
                  <Button size="sm" variant="destructive" onClick={() => onAction(wf.id, "cancel")} disabled={["completed", "failed", "cancelled"].includes(wf.statusRaw ?? "")}>Cancel</Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

export function WorkflowDetailPage() {
  const { workflowId } = useParams();
  const [result, reexecute] = useQuery({ query: WorkflowDetailDocument, variables: { id: workflowId! } });
  const [, pauseWf] = useMutation(PauseWorkflowDocument);
  const [, resumeWf] = useMutation(ResumeWorkflowDocument);
  const [, cancelWf] = useMutation(CancelWorkflowDocument);
  const [, approvePhase] = useMutation(ApprovePhaseDocument);
  const [wfMessage, setWfMessage] = useState<string | null>(null);
  const [wfOperating, setWfOperating] = useState(false);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [approveTarget, setApproveTarget] = useState<string | null>(null);
  const [approveNote, setApproveNote] = useState("");

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const wf = data?.workflow;
  if (!wf) return <PageError message={`Workflow ${workflowId} not found.`} />;

  const checkpoints = data?.workflowCheckpoints ?? [];
  const decisions = wf.decisions ?? [];

  const wfAction = async (label: string, fn: () => Promise<any>) => {
    setWfOperating(true);
    setWfMessage(null);
    const res = await fn();
    setWfOperating(false);
    if (res.error) {
      setWfMessage(`Error: ${res.error.message}`);
    } else {
      setWfMessage(`${label} successful.`);
      reexecute({ requestPolicy: "network-only" });
    }
  };

  const isRunning = wf.statusRaw === "running";
  const isPaused = wf.statusRaw === "paused";
  const isTerminal = ["completed", "failed", "cancelled"].includes(wf.statusRaw ?? "");

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between">
        <div>
          <p className="text-sm text-muted-foreground font-mono">{wf.id}</p>
          <h1 className="text-2xl font-semibold tracking-tight">
            Workflow for <Link to={`/tasks/${wf.taskId}`} className="underline">{wf.taskId}</Link>
          </h1>
          <div className="flex gap-2 mt-2">
            <Badge variant={statusColor(wf.statusRaw ?? "")}>{wf.statusRaw}</Badge>
            {wf.workflowRef && <Badge variant="outline">{wf.workflowRef}</Badge>}
            {(wf.totalReworks ?? 0) > 0 && <Badge variant="outline">{wf.totalReworks} reworks</Badge>}
          </div>
        </div>
        {!isTerminal && (
          <div className="flex items-center gap-2">
            {isRunning && (
              <Button variant="secondary" disabled={wfOperating} onClick={() => wfAction("Pause", () => pauseWf({ id: workflowId! }))}>
                Pause
              </Button>
            )}
            {isPaused && (
              <Button variant="secondary" disabled={wfOperating} onClick={() => wfAction("Resume", () => resumeWf({ id: workflowId! }))}>
                Resume
              </Button>
            )}
            {confirmCancel ? (
              <>
                <Button variant="destructive" disabled={wfOperating} onClick={() => { setConfirmCancel(false); wfAction("Cancel", () => cancelWf({ id: workflowId! })); }}>
                  Confirm Cancel
                </Button>
                <Button variant="outline" onClick={() => setConfirmCancel(false)}>Back</Button>
              </>
            ) : (
              <Button variant="destructive" disabled={wfOperating} onClick={() => setConfirmCancel(true)}>
                Cancel Workflow
              </Button>
            )}
          </div>
        )}
      </div>

      {wfMessage && (
        <Alert variant={wfMessage.startsWith("Error") ? "destructive" : "default"}>
          <AlertDescription>{wfMessage}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Phase Timeline</CardTitle></CardHeader>
        <CardContent>
          <div className="space-y-2">
            {(wf.phases ?? []).map((p, i) => (
              <div key={p.phaseId} className="flex items-start gap-3">
                <div className="flex flex-col items-center">
                  <div className={`h-3 w-3 rounded-full ${
                    p.status === "completed" ? "bg-[var(--ao-success)]" :
                    p.status === "running" ? "bg-[var(--ao-running)] animate-pulse" :
                    p.status === "failed" ? "bg-destructive" :
                    "bg-muted-foreground/30"
                  }`} />
                  {i < (wf.phases ?? []).length - 1 && <div className="w-px h-6 bg-border" />}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm">{p.phaseId}</span>
                    <Badge variant={statusColor(p.status ?? "")} className="text-[10px]">{p.status}</Badge>
                    {(p.attempt ?? 0) > 1 && <span className="text-xs text-muted-foreground">attempt {p.attempt}</span>}
                  </div>
                  {p.errorMessage && <p className="text-xs text-destructive mt-0.5">{p.errorMessage}</p>}
                  {(p.startedAt || p.completedAt) && (
                    <p className="text-xs text-muted-foreground">
                      {p.startedAt && <>Started: {p.startedAt}</>}
                      {p.completedAt && <> &middot; Completed: {p.completedAt}</>}
                    </p>
                  )}
                  {wf.currentPhase === p.phaseId && !isTerminal && p.status !== "completed" && (
                    approveTarget === p.phaseId ? (
                      <div className="flex items-center gap-2 mt-1">
                        <Input
                          value={approveNote}
                          onChange={(e) => setApproveNote(e.target.value)}
                          placeholder="Approval note (optional)..."
                          className="h-7 text-xs max-w-xs"
                        />
                        <Button
                          size="sm"
                          disabled={wfOperating}
                          onClick={() => {
                            setApproveTarget(null);
                            wfAction("Phase approval", () => approvePhase({ workflowId: workflowId!, phaseId: p.phaseId, note: approveNote || null }));
                            setApproveNote("");
                          }}
                        >
                          Confirm Approve
                        </Button>
                        <Button size="sm" variant="outline" onClick={() => { setApproveTarget(null); setApproveNote(""); }}>
                          Cancel
                        </Button>
                      </div>
                    ) : (
                      <Button size="sm" variant="outline" className="mt-1" disabled={wfOperating} onClick={() => setApproveTarget(p.phaseId)}>
                        Approve Phase
                      </Button>
                    )
                  )}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {decisions.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Decisions</CardTitle></CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Phase</TableHead>
                  <TableHead>Decision</TableHead>
                  <TableHead>Target</TableHead>
                  <TableHead>Confidence</TableHead>
                  <TableHead>Source</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {decisions.map((d, i) => (
                  <TableRow key={i}>
                    <TableCell className="font-mono text-xs">{d.phaseId}</TableCell>
                    <TableCell>{d.decision}</TableCell>
                    <TableCell className="font-mono text-xs">{d.targetPhase ?? "-"}</TableCell>
                    <TableCell>{((d.confidence ?? 0) * 100).toFixed(0)}%</TableCell>
                    <TableCell className="text-xs text-muted-foreground">{d.source}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {checkpoints.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Checkpoints</CardTitle></CardHeader>
          <CardContent>
            <ul className="space-y-2">
              {checkpoints.map((cp) => (
                <li key={cp.id} className="text-sm">
                  <Link
                    to={`/workflows/${workflowId}/checkpoints/${cp.id}`}
                    className="font-mono underline"
                  >
                    {cp.id}
                  </Link>
                  <span className="text-muted-foreground ml-2">{cp.phase}</span>
                  {cp.timestamp && <span className="text-muted-foreground ml-2">{cp.timestamp}</span>}
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

export function WorkflowCheckpointPage() {
  const { workflowId, checkpoint } = useParams();
  const [result] = useQuery({
    query: WorkflowDetailDocument,
    variables: { id: workflowId! },
  });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const checkpoints = data?.workflowCheckpoints ?? [];
  const cp = checkpoints.find((c) => c.id === checkpoint);

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Checkpoint {checkpoint}</h1>
      <p className="text-sm text-muted-foreground">
        Workflow: <Link to={`/workflows/${workflowId}`} className="underline font-mono">{workflowId}</Link>
      </p>
      {cp ? (
        <Card>
          <CardContent className="pt-4">
            <pre className="text-xs overflow-auto">{cp.data ?? "No data"}</pre>
          </CardContent>
        </Card>
      ) : (
        <PageError message={`Checkpoint ${checkpoint} not found.`} />
      )}
    </div>
  );
}
