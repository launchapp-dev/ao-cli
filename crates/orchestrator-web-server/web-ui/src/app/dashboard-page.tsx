import { Link } from "react-router-dom";
import { useQuery } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { DashboardDocument } from "@/lib/graphql/generated/graphql";
import { statusColor, StatusDot, PageLoading, PageError, StatCard } from "./shared";

export function DashboardPage() {
  const [result] = useQuery({ query: DashboardDocument });
  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const stats = data?.taskStats;
  const health = data?.daemonHealth;
  const agents = data?.agentRuns ?? [];
  const sys = data?.systemInfo;

  const byStatus: Record<string, number> = stats?.byStatus ? JSON.parse(stats.byStatus) : {};
  const byPriority: Record<string, number> = stats?.byPriority ? JSON.parse(stats.byPriority) : {};
  const inProgress = byStatus["in-progress"] ?? 0;
  const blocked = byStatus["blocked"] ?? 0;
  const failed = byStatus["failed"] ?? 0;
  const ready = byStatus["ready"] ?? 0;

  const priorityCritical = byPriority["critical"] ?? 0;
  const priorityHigh = byPriority["high"] ?? 0;
  const priorityMedium = byPriority["medium"] ?? 0;
  const priorityLow = byPriority["low"] ?? 0;
  const priorityTotal = priorityCritical + priorityHigh + priorityMedium + priorityLow;

  const needsAttention = blocked > 0 || failed > 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-xl font-semibold tracking-tight">Dashboard</h1>
              <StatusDot status={health?.healthy ? "healthy" : "error"} />
              <span className="text-xs text-muted-foreground">
                {health?.status ?? "unknown"}
              </span>
            </div>
            <p className="text-xs text-muted-foreground/60 mt-0.5 font-mono">
              {sys?.projectRoot ?? "no project loaded"}
            </p>
          </div>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" asChild>
          <Link to="/tasks/new">New Task</Link>
        </Button>
        <Button variant="outline" size="sm" asChild>
          <Link to="/workflows/dispatch/task">Run Workflow</Link>
        </Button>
        <Button variant="outline" size="sm" asChild>
          <Link to="/workflows/builder">Build Workflow</Link>
        </Button>
        <Button variant="outline" size="sm" asChild>
          <Link to="/queue">View Queue</Link>
        </Button>
      </div>

      {needsAttention && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="pt-3 pb-3 px-4">
            <p className="text-xs uppercase tracking-wider text-amber-500/80 font-medium mb-2">Attention Required</p>
            <div className="space-y-1">
              {blocked > 0 && (
                <Link to="/tasks?status=blocked" className="flex items-center gap-2 text-sm text-foreground/80 hover:text-foreground transition-colors">
                  <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                  <span>{blocked} task{blocked !== 1 ? "s" : ""} blocked</span>
                </Link>
              )}
              {failed > 0 && (
                <Link to="/tasks?status=failed" className="flex items-center gap-2 text-sm text-foreground/80 hover:text-foreground transition-colors">
                  <span className="h-1.5 w-1.5 rounded-full bg-destructive" />
                  <span>{failed} task{failed !== 1 ? "s" : ""} failed</span>
                </Link>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <StatCard label="Total" value={stats?.total ?? 0} />
        <StatCard label="In Progress" value={inProgress} accent={inProgress > 0} />
        <StatCard label="Ready" value={ready} />
        <StatCard label="Blocked" value={blocked} />
      </div>

      <div className="grid md:grid-cols-3 gap-4">
        <div className="md:col-span-2 space-y-4">
          {agents.length > 0 ? (
            <Card className="border-border/40 bg-card/60 overflow-hidden">
              <CardHeader className="pb-2 pt-3 px-4">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Active Agents</CardTitle>
                  <Badge variant="outline" className="text-[10px] h-4 px-1.5 font-mono border-primary/20 text-primary/70">
                    {agents.length}
                  </Badge>
                </div>
              </CardHeader>
              <CardContent className="px-0 pb-0">
                <Table>
                  <TableHeader>
                    <TableRow className="border-border/30 hover:bg-transparent">
                      <TableHead className="text-[10px] uppercase tracking-wider h-7">Run</TableHead>
                      <TableHead className="text-[10px] uppercase tracking-wider h-7">Task</TableHead>
                      <TableHead className="text-[10px] uppercase tracking-wider h-7">Phase</TableHead>
                      <TableHead className="text-[10px] uppercase tracking-wider h-7">Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {agents.map((a) => (
                      <TableRow key={a.runId} className="border-border/20 hover:bg-accent/30">
                        <TableCell className="font-mono text-[11px] text-muted-foreground py-2">{a.runId}</TableCell>
                        <TableCell className="py-2">
                          {a.taskId ? (
                            <Link to={`/tasks/${a.taskId}`} className="text-primary/80 hover:text-primary text-xs transition-colors">
                              {a.taskTitle ?? a.taskId}
                            </Link>
                          ) : (
                            <span className="text-muted-foreground/40">-</span>
                          )}
                        </TableCell>
                        <TableCell className="font-mono text-[11px] text-muted-foreground py-2">{a.phaseId ?? "-"}</TableCell>
                        <TableCell className="py-2">
                          <div className="flex items-center gap-1.5">
                            <StatusDot status={a.status} />
                            <span className="text-[11px]">{a.status}</span>
                          </div>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          ) : (
            <Card className="border-border/40 bg-card/60">
              <CardHeader className="pb-2 pt-3 px-4">
                <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Activity</CardTitle>
              </CardHeader>
              <CardContent className="px-4 pb-4">
                <div className="flex flex-col items-center justify-center py-8 gap-2">
                  <p className="text-sm text-muted-foreground/60">No agents running</p>
                  <p className="text-xs text-muted-foreground/40">Start a workflow to see agent activity here</p>
                </div>
              </CardContent>
            </Card>
          )}

          {priorityTotal > 0 && (
            <Card className="border-border/40 bg-card/60">
              <CardHeader className="pb-2 pt-3 px-4">
                <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Priority Distribution</CardTitle>
              </CardHeader>
              <CardContent className="px-4 pb-3">
                <div className="flex h-2 rounded-full overflow-hidden bg-muted/20">
                  {priorityCritical > 0 && (
                    <div
                      className="bg-destructive transition-all"
                      style={{ width: `${(priorityCritical / priorityTotal) * 100}%` }}
                      title={`Critical: ${priorityCritical}`}
                    />
                  )}
                  {priorityHigh > 0 && (
                    <div
                      className="bg-[var(--ao-amber)] transition-all"
                      style={{ width: `${(priorityHigh / priorityTotal) * 100}%` }}
                      title={`High: ${priorityHigh}`}
                    />
                  )}
                  {priorityMedium > 0 && (
                    <div
                      className="bg-muted-foreground/40 transition-all"
                      style={{ width: `${(priorityMedium / priorityTotal) * 100}%` }}
                      title={`Medium: ${priorityMedium}`}
                    />
                  )}
                  {priorityLow > 0 && (
                    <div
                      className="bg-border transition-all"
                      style={{ width: `${(priorityLow / priorityTotal) * 100}%` }}
                      title={`Low: ${priorityLow}`}
                    />
                  )}
                </div>
                <div className="flex gap-4 mt-2 text-[10px] text-muted-foreground">
                  {priorityCritical > 0 && (
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-destructive" />
                      {priorityCritical} critical
                    </span>
                  )}
                  {priorityHigh > 0 && (
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-[var(--ao-amber)]" />
                      {priorityHigh} high
                    </span>
                  )}
                  {priorityMedium > 0 && (
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/40" />
                      {priorityMedium} medium
                    </span>
                  )}
                  {priorityLow > 0 && (
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-border" />
                      {priorityLow} low
                    </span>
                  )}
                </div>
              </CardContent>
            </Card>
          )}
        </div>

        <div className="space-y-4">
          <Card className="border-border/40 bg-card/60">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">System Health</CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-3 space-y-2">
              <div className="flex items-center gap-2">
                <StatusDot status={health?.healthy ? "healthy" : "error"} />
                <span className="text-sm font-mono">{health?.status ?? "unknown"}</span>
              </div>
              <div className="space-y-1.5 text-xs">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Agents</span>
                  <span className="font-mono text-foreground/70">{health?.activeDaemons ?? 0} active</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Runner</span>
                  <span className="font-mono text-foreground/70">
                    {health?.runnerConnected ? (
                      <Badge variant="outline" className="text-[10px] h-4 px-1.5 border-[var(--ao-success-border)] text-[var(--ao-success)]">connected</Badge>
                    ) : (
                      <Badge variant="outline" className="text-[10px] h-4 px-1.5 border-border/40 text-muted-foreground/60">disconnected</Badge>
                    )}
                  </span>
                </div>
                {health?.daemonPid && (
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">PID</span>
                    <span className="font-mono text-foreground/70">{health.daemonPid}</span>
                  </div>
                )}
                {sys?.version && (
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Version</span>
                    <span className="font-mono text-foreground/70">{sys.version}</span>
                  </div>
                )}
                {sys?.platform && (
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Platform</span>
                    <span className="font-mono text-foreground/70">{sys.platform}</span>
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
