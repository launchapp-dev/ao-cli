import { Link } from "react-router-dom";
import { useQuery } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
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
  const inProgress = byStatus["in-progress"] ?? 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Dashboard</h1>
          <p className="text-xs text-muted-foreground/60 mt-0.5 font-mono">
            {sys?.projectRoot ?? "no project loaded"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <StatusDot status={health?.healthy ? "healthy" : "error"} />
          <span className="text-xs text-muted-foreground">
            {health?.status ?? "unknown"}
          </span>
        </div>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3" style={{ animationDelay: "50ms" }}>
        <StatCard label="Total" value={stats?.total ?? 0} />
        <StatCard label="In Progress" value={inProgress} accent={inProgress > 0} />
        <StatCard label="Ready" value={byStatus["ready"] ?? 0} />
        <StatCard label="Blocked" value={byStatus["blocked"] ?? 0} />
      </div>

      <div className="grid md:grid-cols-2 gap-4">
        <Card className="border-border/40 bg-card/60">
          <CardHeader className="pb-2 pt-3 px-4">
            <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Daemon</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 px-4 pb-3">
            <div className="flex items-center gap-2">
              <StatusDot status={health?.healthy ? "healthy" : "error"} />
              <span className="text-sm font-mono">{health?.status ?? "unknown"}</span>
              {health?.runnerConnected && (
                <Badge variant="outline" className="text-[10px] h-4 px-1.5 border-primary/20 text-primary/70">runner</Badge>
              )}
            </div>
            <div className="flex gap-4 text-xs text-muted-foreground">
              <span>Agents: <span className="font-mono text-foreground/70">{health?.activeDaemons ?? 0}</span></span>
              {health?.daemonPid && <span>PID: <span className="font-mono text-foreground/70">{health.daemonPid}</span></span>}
            </div>
          </CardContent>
        </Card>

        <Card className="border-border/40 bg-card/60">
          <CardHeader className="pb-2 pt-3 px-4">
            <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">System</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 px-4 pb-3 text-xs">
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
          </CardContent>
        </Card>
      </div>

      {agents.length > 0 && (
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
      )}
    </div>
  );
}
