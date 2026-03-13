import { FormEvent, ReactNode, useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { useQuery, useMutation } from "urql";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Textarea } from "@/components/ui/textarea";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useDaemonEvents } from "../lib/events/use-daemon-events";

const DASHBOARD_QUERY = `
  query Dashboard {
    taskStats { total byStatus byPriority }
    daemonHealth { healthy status runnerConnected activeDaemons: activeAgents daemonPid }
    agentRuns { runId taskId taskTitle workflowId phaseId status }
    systemInfo { platform version daemonStatus projectRoot }
  }
`;

const TASKS_QUERY = `
  query Tasks($status: String, $search: String) {
    tasks(status: $status, search: $search) {
      id title status statusRaw priority priorityRaw taskType taskTypeRaw
      tags linkedRequirementIds
    }
    taskStats { total byStatus byPriority }
  }
`;

const TASKS_PRIORITIZED_QUERY = `
  query TasksPrioritized {
    tasksPrioritized {
      id title status statusRaw priority priorityRaw taskType taskTypeRaw tags
    }
  }
`;

const TASK_DETAIL_QUERY = `
  query TaskDetail($id: ID!) {
    task(id: $id) {
      id title description status statusRaw priority priorityRaw
      taskType taskTypeRaw risk scope complexity tags
      linkedRequirementIds
      checklist { id description completed }
      dependencies { taskId type }
    }
  }
`;

const WORKFLOWS_QUERY = `
  query Workflows($status: String) {
    workflows(status: $status) {
      id taskId workflowRef status statusRaw currentPhase totalReworks
      phases { phaseId status startedAt completedAt attempt errorMessage }
    }
  }
`;

const WORKFLOW_DETAIL_QUERY = `
  query WorkflowDetail($id: ID!) {
    workflow(id: $id) {
      id taskId workflowRef status statusRaw currentPhase totalReworks
      phases { phaseId status startedAt completedAt attempt errorMessage }
      decisions { timestamp phaseId source decision targetPhase reason confidence risk }
    }
    workflowCheckpoints(workflowId: $id) { id phase timestamp data }
  }
`;

const QUEUE_QUERY = `
  query Queue {
    queue { taskId title priority status waitTime position }
    queueStats { depth readyCount heldCount avgWait throughput }
  }
`;

const DAEMON_QUERY = `
  query Daemon {
    daemonStatus { healthy status statusRaw runnerConnected activeAgents maxAgents projectRoot }
    daemonHealth { healthy status runnerConnected runnerPid activeAgents daemonPid }
    agentRuns { runId taskId taskTitle workflowId phaseId status }
    daemonLogs(limit: 50) { timestamp level message }
  }
`;

const PROJECTS_QUERY = `
  query Projects {
    projects { id name path description archived }
    projectsActive { id name path }
  }
`;

const PROJECT_DETAIL_QUERY = `
  query ProjectDetail($id: ID!) {
    project(id: $id) { id name path description type techStack archived }
  }
`;

const REQUIREMENT_DETAIL_QUERY = `
  query RequirementDetail($id: ID!) {
    requirement(id: $id) {
      id title description priority priorityRaw status statusRaw
      requirementType tags linkedTaskIds
    }
  }
`;

const VISION_QUERY = `
  query Vision {
    vision { title summary goals targetAudience successCriteria constraints raw }
  }
`;

const UPDATE_TASK_STATUS = `
  mutation UpdateTaskStatus($id: ID!, $status: String!) {
    updateTaskStatus(id: $id, status: $status) { id status statusRaw }
  }
`;

const RUN_WORKFLOW = `
  mutation RunWorkflow($taskId: String!, $workflowRef: String) {
    runWorkflow(taskId: $taskId, workflowRef: $workflowRef) { id taskId status statusRaw }
  }
`;

const PAUSE_WORKFLOW = `mutation PauseWorkflow($id: ID!) { pauseWorkflow(id: $id) { id status } }`;
const RESUME_WORKFLOW = `mutation ResumeWorkflow($id: ID!) { resumeWorkflow(id: $id) { id status } }`;
const CANCEL_WORKFLOW = `mutation CancelWorkflow($id: ID!) { cancelWorkflow(id: $id) { id status } }`;

const DAEMON_START = `mutation { daemonStart }`;
const DAEMON_STOP = `mutation { daemonStop }`;
const DAEMON_PAUSE = `mutation { daemonPause }`;
const DAEMON_RESUME = `mutation { daemonResume }`;
const DAEMON_CLEAR_LOGS = `mutation { daemonClearLogs }`;

const QUEUE_HOLD = `mutation QueueHold($taskId: String!, $reason: String) { queueHold(taskId: $taskId, reason: $reason) }`;
const QUEUE_RELEASE = `mutation QueueRelease($taskId: String!) { queueRelease(taskId: $taskId) }`;

const REVIEW_HANDOFF = `
  mutation ReviewHandoff($targetRole: String!, $question: String!, $context: String) {
    reviewHandoff(targetRole: $targetRole, question: $question, context: $context)
  }
`;

function statusColor(status: string): "default" | "secondary" | "destructive" | "outline" {
  const s = status.toLowerCase().replace(/[_\s]/g, "-");
  if (["done", "completed", "approved", "implemented"].includes(s)) return "default";
  if (["in-progress", "running", "inprogress"].includes(s)) return "secondary";
  if (["blocked", "failed", "cancelled", "crashed"].includes(s)) return "destructive";
  return "outline";
}

function priorityColor(p: string): "default" | "secondary" | "destructive" | "outline" {
  const v = (p || "").toLowerCase();
  if (v === "critical") return "destructive";
  if (v === "high") return "secondary";
  return "outline";
}

function PageLoading() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-8 w-48" />
      <Skeleton className="h-32 w-full" />
      <Skeleton className="h-32 w-full" />
    </div>
  );
}

function PageError({ message }: { message: string }) {
  return (
    <Alert variant="destructive">
      <AlertTitle>Error</AlertTitle>
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

export function DashboardPage() {
  const [result] = useQuery({ query: DASHBOARD_QUERY });
  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const stats = data?.taskStats;
  const health = data?.daemonHealth;
  const agents = data?.agentRuns ?? [];
  const sys = data?.systemInfo;

  const byStatus: Record<string, number> = stats?.byStatus ? JSON.parse(stats.byStatus) : {};

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard label="Total Tasks" value={stats?.total ?? 0} />
        <StatCard label="In Progress" value={byStatus["in-progress"] ?? 0} />
        <StatCard label="Ready" value={byStatus["ready"] ?? 0} />
        <StatCard label="Blocked" value={byStatus["blocked"] ?? 0} />
      </div>

      <div className="grid md:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Daemon</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm">
            <div className="flex items-center gap-2">
              <Badge variant={health?.healthy ? "default" : "destructive"}>
                {health?.status ?? "unknown"}
              </Badge>
              {health?.runnerConnected && <span className="text-muted-foreground">runner connected</span>}
            </div>
            <p className="text-muted-foreground">Active agents: {health?.activeDaemons ?? 0}</p>
            {health?.daemonPid && <p className="text-muted-foreground">PID: {health.daemonPid}</p>}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">System</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm text-muted-foreground">
            {sys?.version && <p>Version: {sys.version}</p>}
            {sys?.platform && <p>Platform: {sys.platform}</p>}
            {sys?.projectRoot && <p className="truncate">Root: {sys.projectRoot}</p>}
          </CardContent>
        </Card>
      </div>

      {agents.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Active Agents</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Run</TableHead>
                  <TableHead>Task</TableHead>
                  <TableHead>Phase</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {agents.map((a: any) => (
                  <TableRow key={a.runId}>
                    <TableCell className="font-mono text-xs">{a.runId}</TableCell>
                    <TableCell>
                      {a.taskId ? <Link to={`/tasks/${a.taskId}`} className="underline">{a.taskTitle ?? a.taskId}</Link> : "-"}
                    </TableCell>
                    <TableCell className="font-mono text-xs">{a.phaseId ?? "-"}</TableCell>
                    <TableCell><Badge variant={statusColor(a.status)}>{a.status}</Badge></TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: number | string }) {
  return (
    <Card>
      <CardContent className="pt-4">
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className="text-2xl font-bold">{value}</p>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

export function TasksPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const statusFilter = searchParams.get("status") ?? "";
  const searchQuery = searchParams.get("search") ?? "";

  const [result] = useQuery({
    query: TASKS_QUERY,
    variables: { status: statusFilter || undefined, search: searchQuery || undefined },
  });
  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const tasks = data?.tasks ?? [];
  const stats = data?.taskStats;
  const byStatus: Record<string, number> = stats?.byStatus ? JSON.parse(stats.byStatus) : {};

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Tasks</h1>
        <span className="text-sm text-muted-foreground">{tasks.length} tasks</span>
      </div>

      <div className="grid grid-cols-3 md:grid-cols-6 gap-2">
        {["backlog", "ready", "in-progress", "blocked", "done", "cancelled"].map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => {
              const next = new URLSearchParams(searchParams);
              if (statusFilter === s) next.delete("status");
              else next.set("status", s);
              setSearchParams(next);
            }}
            className={`rounded-md border px-2 py-1 text-xs text-center transition-colors ${
              statusFilter === s ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
            }`}
          >
            {s} ({byStatus[s] ?? 0})
          </button>
        ))}
      </div>

      <Input
        placeholder="Search tasks..."
        value={searchQuery}
        onChange={(e) => {
          const next = new URLSearchParams(searchParams);
          if (e.target.value) next.set("search", e.target.value);
          else next.delete("search");
          setSearchParams(next);
        }}
        className="max-w-sm"
      />

      {tasks.length === 0 ? (
        <p className="text-sm text-muted-foreground py-8 text-center">No tasks match filters.</p>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-28">ID</TableHead>
                <TableHead>Title</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Priority</TableHead>
                <TableHead>Type</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tasks.map((t: any) => (
                <TableRow key={t.id}>
                  <TableCell>
                    <Link to={`/tasks/${t.id}`} className="font-mono text-xs underline">{t.id}</Link>
                  </TableCell>
                  <TableCell className="font-medium">{t.title}</TableCell>
                  <TableCell><Badge variant={statusColor(t.statusRaw)}>{t.statusRaw}</Badge></TableCell>
                  <TableCell><Badge variant={priorityColor(t.priorityRaw)}>{t.priorityRaw}</Badge></TableCell>
                  <TableCell className="text-xs text-muted-foreground">{t.taskTypeRaw}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Card>
      )}
    </div>
  );
}

export function TaskDetailPage() {
  const { taskId } = useParams();
  const [result] = useQuery({ query: TASK_DETAIL_QUERY, variables: { id: taskId } });
  const [, updateStatus] = useMutation(UPDATE_TASK_STATUS);
  const [targetStatus, setTargetStatus] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const task = data?.task;
  if (!task) return <PageError message={`Task ${taskId} not found.`} />;

  const applyStatus = async () => {
    if (!targetStatus) return;
    const { error: mutErr } = await updateStatus({ id: taskId, status: targetStatus });
    if (mutErr) {
      setFeedback({ kind: "error", message: mutErr.message });
    } else {
      setFeedback({ kind: "ok", message: `Status updated to ${targetStatus}.` });
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-muted-foreground font-mono">{task.id}</p>
        <h1 className="text-2xl font-semibold tracking-tight">{task.title}</h1>
        <div className="flex gap-2 mt-2">
          <Badge variant={statusColor(task.statusRaw)}>{task.statusRaw}</Badge>
          <Badge variant={priorityColor(task.priorityRaw)}>{task.priorityRaw}</Badge>
          <Badge variant="outline">{task.taskTypeRaw}</Badge>
        </div>
      </div>

      {task.description && (
        <Card>
          <CardContent className="pt-4 text-sm whitespace-pre-wrap">{task.description}</CardContent>
        </Card>
      )}

      <div className="grid md:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Status Transition</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            <select
              value={targetStatus}
              onChange={(e) => setTargetStatus(e.target.value)}
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
            >
              <option value="">Select status...</option>
              {["backlog", "ready", "in-progress", "blocked", "on-hold", "done", "cancelled"].map((s) => (
                <option key={s} value={s}>{s}</option>
              ))}
            </select>
            <Button size="sm" onClick={applyStatus} disabled={!targetStatus || targetStatus === task.statusRaw}>
              Apply
            </Button>
            {feedback?.kind === "ok" && <p className="text-sm text-green-600">{feedback.message}</p>}
            {feedback?.kind === "error" && <p className="text-sm text-destructive">{feedback.message}</p>}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Details</CardTitle></CardHeader>
          <CardContent className="text-sm space-y-1">
            <p>Risk: <Badge variant="outline">{task.risk}</Badge></p>
            <p>Scope: <Badge variant="outline">{task.scope}</Badge></p>
            <p>Complexity: <Badge variant="outline">{task.complexity}</Badge></p>
            {task.tags.length > 0 && (
              <div className="flex gap-1 flex-wrap pt-1">
                {task.tags.map((t: string) => <Badge key={t} variant="outline" className="text-xs">{t}</Badge>)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {task.checklist.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Checklist</CardTitle></CardHeader>
          <CardContent>
            <ul className="space-y-1">
              {task.checklist.map((item: any) => (
                <li key={item.id} className="flex items-center gap-2 text-sm">
                  <span className={item.completed ? "text-green-600" : "text-muted-foreground"}>
                    {item.completed ? "✓" : "○"}
                  </span>
                  <span className={item.completed ? "line-through text-muted-foreground" : ""}>{item.description}</span>
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      {task.dependencies.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Dependencies</CardTitle></CardHeader>
          <CardContent>
            <ul className="space-y-1">
              {task.dependencies.map((dep: any) => (
                <li key={dep.taskId} className="text-sm">
                  <Link to={`/tasks/${dep.taskId}`} className="font-mono underline">{dep.taskId}</Link>
                  <span className="text-muted-foreground ml-2">{dep.type}</span>
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      {task.linkedRequirementIds.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Linked Requirements</CardTitle></CardHeader>
          <CardContent>
            <div className="flex gap-2 flex-wrap">
              {task.linkedRequirementIds.map((id: string) => (
                <Link key={id} to={`/planning/requirements/${id}`}>
                  <Badge variant="outline" className="font-mono">{id}</Badge>
                </Link>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Workflows
// ---------------------------------------------------------------------------

export function WorkflowsPage() {
  const [statusFilter, setStatusFilter] = useState<string>("");
  const [result, reexecute] = useQuery({
    query: WORKFLOWS_QUERY,
    variables: { status: statusFilter || undefined },
  });
  const [, runWf] = useMutation(RUN_WORKFLOW);
  const [, pauseWf] = useMutation(PAUSE_WORKFLOW);
  const [, resumeWf] = useMutation(RESUME_WORKFLOW);
  const [, cancelWf] = useMutation(CANCEL_WORKFLOW);
  const [runTaskId, setRunTaskId] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const workflows = data?.workflows ?? [];

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

  const counts = useMemo(() => {
    const c = { running: 0, paused: 0, completed: 0, failed: 0 };
    for (const w of workflows) {
      const s = (w.statusRaw || "").toLowerCase();
      if (s === "running") c.running++;
      else if (s === "paused") c.paused++;
      else if (s === "completed") c.completed++;
      else if (s === "failed") c.failed++;
    }
    return c;
  }, [workflows]);

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
          {workflows.map((wf: any) => (
            <Card key={wf.id}>
              <CardContent className="pt-4">
                <div className="flex items-start justify-between">
                  <div>
                    <Link to={`/workflows/${wf.id}`} className="font-mono text-sm underline">{wf.id}</Link>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      Task: <Link to={`/tasks/${wf.taskId}`} className="underline">{wf.taskId}</Link>
                      {wf.currentPhase && <> &middot; Phase: {wf.currentPhase}</>}
                      {wf.totalReworks > 0 && <> &middot; {wf.totalReworks} reworks</>}
                    </p>
                  </div>
                  <Badge variant={statusColor(wf.statusRaw)}>{wf.statusRaw}</Badge>
                </div>

                {wf.phases?.length > 0 && (
                  <div className="flex gap-1 mt-2 flex-wrap">
                    {wf.phases.map((p: any) => (
                      <span
                        key={p.phaseId}
                        className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-mono ${
                          p.status === "completed" ? "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200" :
                          p.status === "running" ? "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200" :
                          p.status === "failed" ? "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200" :
                          "bg-muted text-muted-foreground"
                        }`}
                      >
                        {p.phaseId}
                      </span>
                    ))}
                  </div>
                )}

                <div className="flex gap-1 mt-2">
                  <Button size="sm" variant="outline" onClick={() => onAction(wf.id, "pause")} disabled={wf.statusRaw !== "running"}>Pause</Button>
                  <Button size="sm" variant="outline" onClick={() => onAction(wf.id, "resume")} disabled={wf.statusRaw !== "paused"}>Resume</Button>
                  <Button size="sm" variant="destructive" onClick={() => onAction(wf.id, "cancel")} disabled={["completed", "failed", "cancelled"].includes(wf.statusRaw)}>Cancel</Button>
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
  const [result] = useQuery({ query: WORKFLOW_DETAIL_QUERY, variables: { id: workflowId } });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const wf = data?.workflow;
  if (!wf) return <PageError message={`Workflow ${workflowId} not found.`} />;

  const checkpoints = data?.workflowCheckpoints ?? [];
  const decisions = wf.decisions ?? [];

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-muted-foreground font-mono">{wf.id}</p>
        <h1 className="text-2xl font-semibold tracking-tight">
          Workflow for <Link to={`/tasks/${wf.taskId}`} className="underline">{wf.taskId}</Link>
        </h1>
        <div className="flex gap-2 mt-2">
          <Badge variant={statusColor(wf.statusRaw)}>{wf.statusRaw}</Badge>
          {wf.workflowRef && <Badge variant="outline">{wf.workflowRef}</Badge>}
          {wf.totalReworks > 0 && <Badge variant="outline">{wf.totalReworks} reworks</Badge>}
        </div>
      </div>

      <Card>
        <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Phase Timeline</CardTitle></CardHeader>
        <CardContent>
          <div className="space-y-2">
            {wf.phases.map((p: any, i: number) => (
              <div key={p.phaseId} className="flex items-start gap-3">
                <div className="flex flex-col items-center">
                  <div className={`h-3 w-3 rounded-full ${
                    p.status === "completed" ? "bg-green-500" :
                    p.status === "running" ? "bg-blue-500 animate-pulse" :
                    p.status === "failed" ? "bg-red-500" :
                    "bg-muted-foreground/30"
                  }`} />
                  {i < wf.phases.length - 1 && <div className="w-px h-6 bg-border" />}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm">{p.phaseId}</span>
                    <Badge variant={statusColor(p.status)} className="text-[10px]">{p.status}</Badge>
                    {p.attempt > 1 && <span className="text-xs text-muted-foreground">attempt {p.attempt}</span>}
                  </div>
                  {p.errorMessage && <p className="text-xs text-destructive mt-0.5">{p.errorMessage}</p>}
                  {(p.startedAt || p.completedAt) && (
                    <p className="text-xs text-muted-foreground">
                      {p.startedAt && <>Started: {p.startedAt}</>}
                      {p.completedAt && <> &middot; Completed: {p.completedAt}</>}
                    </p>
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
                {decisions.map((d: any, i: number) => (
                  <TableRow key={i}>
                    <TableCell className="font-mono text-xs">{d.phaseId}</TableCell>
                    <TableCell>{d.decision}</TableCell>
                    <TableCell className="font-mono text-xs">{d.targetPhase ?? "-"}</TableCell>
                    <TableCell>{(d.confidence * 100).toFixed(0)}%</TableCell>
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
              {checkpoints.map((cp: any) => (
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
    query: WORKFLOW_DETAIL_QUERY,
    variables: { id: workflowId },
  });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const checkpoints = data?.workflowCheckpoints ?? [];
  const cp = checkpoints.find((c: any) => c.id === checkpoint);

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

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

export function QueuePage() {
  const [result, reexecute] = useQuery({ query: QUEUE_QUERY });
  const [, holdMut] = useMutation(QUEUE_HOLD);
  const [, releaseMut] = useMutation(QUEUE_RELEASE);
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const entries = data?.queue ?? [];
  const stats = data?.queueStats;

  const onHold = async (taskId: string) => {
    const { error: err } = await holdMut({ taskId });
    if (err) setFeedback({ kind: "error", message: err.message });
    else {
      setFeedback({ kind: "ok", message: `Held ${taskId}.` });
      reexecute({ requestPolicy: "network-only" });
    }
  };

  const onRelease = async (taskId: string) => {
    const { error: err } = await releaseMut({ taskId });
    if (err) setFeedback({ kind: "error", message: err.message });
    else {
      setFeedback({ kind: "ok", message: `Released ${taskId}.` });
      reexecute({ requestPolicy: "network-only" });
    }
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Queue</h1>

      <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
        <StatCard label="Depth" value={stats?.depth ?? 0} />
        <StatCard label="Ready" value={stats?.readyCount ?? 0} />
        <StatCard label="Held" value={stats?.heldCount ?? 0} />
        <StatCard label="Avg Wait" value={stats?.avgWait != null ? `${stats.avgWait.toFixed(1)}s` : "-"} />
        <StatCard label="Throughput" value={stats?.throughput != null ? `${stats.throughput.toFixed(1)}/hr` : "-"} />
      </div>

      {feedback && (
        <Alert variant={feedback.kind === "error" ? "destructive" : "default"}>
          <AlertDescription>{feedback.message}</AlertDescription>
        </Alert>
      )}

      {entries.length === 0 ? (
        <p className="text-sm text-muted-foreground py-8 text-center">Queue is empty.</p>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-8">#</TableHead>
                <TableHead>Task</TableHead>
                <TableHead>Title</TableHead>
                <TableHead>Priority</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Wait</TableHead>
                <TableHead className="w-32">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {entries.map((entry: any) => (
                <TableRow key={entry.taskId}>
                  <TableCell className="text-xs text-muted-foreground">{entry.position ?? "-"}</TableCell>
                  <TableCell>
                    <Link to={`/tasks/${entry.taskId}`} className="font-mono text-xs underline">{entry.taskId}</Link>
                  </TableCell>
                  <TableCell>{entry.title ?? "-"}</TableCell>
                  <TableCell>{entry.priority && <Badge variant={priorityColor(entry.priority)}>{entry.priority}</Badge>}</TableCell>
                  <TableCell>{entry.status && <Badge variant={statusColor(entry.status)}>{entry.status}</Badge>}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">{entry.waitTime != null ? `${entry.waitTime.toFixed(0)}s` : "-"}</TableCell>
                  <TableCell>
                    <div className="flex gap-1">
                      <Button size="sm" variant="outline" className="h-6 text-xs" onClick={() => onHold(entry.taskId)}>Hold</Button>
                      <Button size="sm" variant="outline" className="h-6 text-xs" onClick={() => onRelease(entry.taskId)}>Release</Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

export function DaemonPage() {
  const [result, reexecute] = useQuery({ query: DAEMON_QUERY });
  const [, startMut] = useMutation(DAEMON_START);
  const [, stopMut] = useMutation(DAEMON_STOP);
  const [, pauseMut] = useMutation(DAEMON_PAUSE);
  const [, resumeMut] = useMutation(DAEMON_RESUME);
  const [, clearLogsMut] = useMutation(DAEMON_CLEAR_LOGS);
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const status = data?.daemonStatus;
  const health = data?.daemonHealth;
  const agents = data?.agentRuns ?? [];
  const logs = data?.daemonLogs ?? [];

  const runAction = async (label: string, fn: () => Promise<any>) => {
    const { error: err } = await fn();
    if (err) setFeedback({ kind: "error", message: err.message });
    else {
      setFeedback({ kind: "ok", message: `${label} successful.` });
      reexecute({ requestPolicy: "network-only" });
    }
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Daemon</h1>

      {feedback && (
        <Alert variant={feedback.kind === "error" ? "destructive" : "default"}>
          <AlertDescription>{feedback.message}</AlertDescription>
        </Alert>
      )}

      <div className="grid md:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Status</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            <div className="flex items-center gap-2">
              <Badge variant={status?.healthy ? "default" : "destructive"}>{status?.statusRaw ?? "unknown"}</Badge>
              {status?.runnerConnected && <span className="text-xs text-muted-foreground">runner connected</span>}
            </div>
            <p className="text-sm text-muted-foreground">Active agents: {status?.activeAgents ?? 0}{status?.maxAgents ? ` / ${status.maxAgents}` : ""}</p>
            {status?.projectRoot && <p className="text-xs text-muted-foreground truncate">Root: {status.projectRoot}</p>}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Controls</CardTitle></CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => runAction("Start", () => startMut({}))}>Start</Button>
              <Button size="sm" variant="outline" onClick={() => runAction("Pause", () => pauseMut({}))}>Pause</Button>
              <Button size="sm" variant="outline" onClick={() => runAction("Resume", () => resumeMut({}))}>Resume</Button>
              <Button size="sm" variant="destructive" onClick={() => runAction("Stop", () => stopMut({}))}>Stop</Button>
              <Button size="sm" variant="outline" onClick={() => runAction("Clear Logs", () => clearLogsMut({}))}>Clear Logs</Button>
            </div>
          </CardContent>
        </Card>
      </div>

      {agents.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Active Agents</CardTitle></CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Run</TableHead>
                  <TableHead>Task</TableHead>
                  <TableHead>Phase</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {agents.map((a: any) => (
                  <TableRow key={a.runId}>
                    <TableCell className="font-mono text-xs">{a.runId}</TableCell>
                    <TableCell>{a.taskId ? <Link to={`/tasks/${a.taskId}`} className="underline">{a.taskTitle ?? a.taskId}</Link> : "-"}</TableCell>
                    <TableCell className="font-mono text-xs">{a.phaseId ?? "-"}</TableCell>
                    <TableCell><Badge variant={statusColor(a.status)}>{a.status}</Badge></TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {logs.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Logs</CardTitle></CardHeader>
          <CardContent>
            <div className="max-h-80 overflow-y-auto font-mono text-xs space-y-0.5">
              {logs.map((log: any, i: number) => (
                <div key={i} className="flex gap-2">
                  <span className="text-muted-foreground shrink-0">{log.timestamp ?? ""}</span>
                  <span className={log.level === "ERROR" ? "text-destructive" : "text-foreground"}>{log.message ?? ""}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

export function ProjectsPage() {
  const [result] = useQuery({ query: PROJECTS_QUERY });
  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const projects = data?.projects ?? [];
  const active = data?.projectsActive ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Projects</h1>
      {active.length > 0 && (
        <p className="text-sm text-muted-foreground">Active: {active.map((p: any) => p.name).join(", ")}</p>
      )}
      {projects.length === 0 ? (
        <p className="text-sm text-muted-foreground py-8 text-center">No projects found.</p>
      ) : (
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
          {projects.map((p: any) => (
            <Link key={p.id} to={`/projects/${p.id}`}>
              <Card className="hover:border-foreground/20 transition-colors">
                <CardContent className="pt-4">
                  <p className="font-medium">{p.name}</p>
                  {p.path && <p className="text-xs text-muted-foreground truncate">{p.path}</p>}
                  {p.description && <p className="text-sm text-muted-foreground mt-1 line-clamp-2">{p.description}</p>}
                  {p.archived && <Badge variant="outline" className="mt-1">archived</Badge>}
                </CardContent>
              </Card>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}

export function ProjectDetailPage() {
  const { projectId } = useParams();
  const [result] = useQuery({ query: PROJECT_DETAIL_QUERY, variables: { id: projectId } });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const project = data?.project;
  if (!project) return <PageError message={`Project ${projectId} not found.`} />;

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">{project.name}</h1>
      {project.path && <p className="text-sm text-muted-foreground">{project.path}</p>}
      {project.description && <p className="text-sm">{project.description}</p>}
      <div className="flex gap-2 flex-wrap">
        {project.type && <Badge variant="outline">{project.type}</Badge>}
        {project.archived && <Badge variant="outline">archived</Badge>}
        {(project.techStack ?? []).map((t: string) => <Badge key={t} variant="outline">{t}</Badge>)}
      </div>
    </div>
  );
}

export function RequirementDetailPage() {
  const params = useParams();
  const requirementId = params.requirementId ?? params.projectId ?? "";
  const [result] = useQuery({ query: REQUIREMENT_DETAIL_QUERY, variables: { id: requirementId } });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const req = data?.requirement;
  if (!req) return <PageError message={`Requirement ${requirementId} not found.`} />;

  return (
    <div className="space-y-4">
      <div>
        <p className="text-sm text-muted-foreground font-mono">{req.id}</p>
        <h1 className="text-2xl font-semibold tracking-tight">{req.title}</h1>
        <div className="flex gap-2 mt-2">
          <Badge variant={statusColor(req.statusRaw)}>{req.statusRaw}</Badge>
          <Badge variant={priorityColor(req.priorityRaw)}>{req.priorityRaw}</Badge>
          {req.requirementType && <Badge variant="outline">{req.requirementType}</Badge>}
        </div>
      </div>
      {req.description && (
        <Card>
          <CardContent className="pt-4 text-sm whitespace-pre-wrap">{req.description}</CardContent>
        </Card>
      )}
      {req.linkedTaskIds?.length > 0 && (
        <Card>
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Linked Tasks</CardTitle></CardHeader>
          <CardContent>
            <div className="flex gap-2 flex-wrap">
              {req.linkedTaskIds.map((id: string) => (
                <Link key={id} to={`/tasks/${id}`}><Badge variant="outline" className="font-mono">{id}</Badge></Link>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

export function EventsPage() {
  const { connectionState, events } = useDaemonEvents();

  const mostRecent = useMemo(() => [...events].reverse().slice(0, 50), [events]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Events</h1>
        <Badge variant={connectionState === "open" ? "default" : "outline"}>{connectionState}</Badge>
      </div>
      {mostRecent.length === 0 ? (
        <p className="text-sm text-muted-foreground py-8 text-center">No events received yet.</p>
      ) : (
        <Card>
          <CardContent className="pt-4">
            <div className="max-h-[600px] overflow-y-auto space-y-2">
              {mostRecent.map((evt: any, i: number) => (
                <div key={evt.id ?? i} className="border-b border-border pb-2 last:border-0">
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-[10px]">{evt.event_type ?? "event"}</Badge>
                    <span className="text-xs text-muted-foreground">{evt.timestamp ?? ""}</span>
                  </div>
                  <pre className="text-xs mt-1 overflow-x-auto">{JSON.stringify(evt.data ?? evt, null, 2)}</pre>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Review Handoff
// ---------------------------------------------------------------------------

export function ReviewHandoffPage() {
  const [, handoff] = useMutation(REVIEW_HANDOFF);
  const [targetRole, setTargetRole] = useState("em");
  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!question.trim()) return;
    const { error } = await handoff({
      targetRole,
      question: question.trim(),
      context: context.trim() || undefined,
    });
    if (error) setFeedback({ kind: "error", message: error.message });
    else {
      setFeedback({ kind: "ok", message: "Review handoff submitted." });
      setQuestion("");
      setContext("");
    }
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Review Handoff</h1>

      {feedback && (
        <Alert variant={feedback.kind === "error" ? "destructive" : "default"}>
          <AlertDescription>{feedback.message}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardContent className="pt-4">
          <form onSubmit={onSubmit} className="space-y-4">
            <div>
              <label className="text-sm font-medium">Target Role</label>
              <select
                value={targetRole}
                onChange={(e) => setTargetRole(e.target.value)}
                className="mt-1 h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              >
                <option value="em">em</option>
                <option value="reviewer">reviewer</option>
                <option value="qa">qa</option>
              </select>
            </div>
            <div>
              <label className="text-sm font-medium">Question</label>
              <Textarea
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                rows={3}
                required
                className="mt-1"
              />
            </div>
            <div>
              <label className="text-sm font-medium">Context (optional)</label>
              <Textarea
                value={context}
                onChange={(e) => setContext(e.target.value)}
                rows={3}
                className="mt-1"
              />
            </div>
            <Button type="submit">Submit Handoff</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Not Found
// ---------------------------------------------------------------------------

export function NotFoundPage() {
  return (
    <div className="space-y-4 py-12 text-center">
      <h1 className="text-4xl font-bold">404</h1>
      <p className="text-muted-foreground">The requested page does not exist.</p>
      <Link to="/dashboard">
        <Button variant="outline">Go to Dashboard</Button>
      </Link>
    </div>
  );
}
