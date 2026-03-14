import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DaemonDocument } from "@/lib/graphql/generated/graphql";
import { StatusDot, PageLoading, PageError, SectionHeading } from "./shared";

const PHASE_OUTPUT_QUERY = `query PhaseOutput($workflowId: ID!, $phaseId: String, $tail: Int) { phaseOutput(workflowId: $workflowId, phaseId: $phaseId, tail: $tail) { lines phaseId hasMore } }`;

function useElapsedTime(startedAt: string | null | undefined): string {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!startedAt) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [startedAt]);
  if (!startedAt) return "";
  const ms = Date.now() - new Date(startedAt).getTime();
  return formatDuration(ms);
}

function formatDuration(ms: number): string {
  if (ms < 0) return "0s";
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

function AgentOutputPreview({ workflowId, phaseId }: { workflowId: string; phaseId: string | null | undefined }) {
  const [result] = useQuery<{ phaseOutput: { lines: string[]; phaseId: string; hasMore: boolean } }>({
    query: PHASE_OUTPUT_QUERY,
    variables: { workflowId, phaseId: phaseId ?? undefined, tail: 10 },
    pause: !workflowId,
  });

  const lines = result.data?.phaseOutput?.lines ?? [];

  if (result.fetching && lines.length === 0) {
    return (
      <div className="rounded border border-border/30 bg-background/40 p-3">
        <p className="text-[11px] text-muted-foreground/40 font-mono">Loading output...</p>
      </div>
    );
  }

  if (lines.length === 0) {
    return (
      <div className="rounded border border-border/30 bg-background/40 p-3">
        <p className="text-[11px] text-muted-foreground/40 font-mono">No output yet</p>
      </div>
    );
  }

  return (
    <div className="rounded border border-border/30 bg-background/40 p-3 max-h-40 overflow-y-auto">
      <div className="space-y-0.5">
        {lines.map((line, i) => (
          <div key={i} className="font-mono text-[11px] text-foreground/60 whitespace-pre-wrap break-all">
            {line}
          </div>
        ))}
        <span className="inline-block w-1.5 h-3.5 bg-primary/60 animate-pulse" />
      </div>
    </div>
  );
}

function AgentCard({ agent }: { agent: { runId: string; taskId?: string | null; taskTitle?: string | null; workflowId?: string | null; phaseId?: string | null; status: string } }) {
  const elapsed = useElapsedTime(new Date().toISOString());

  return (
    <Card className="border-border/40 bg-card/60">
      <CardHeader className="pb-2 pt-3 px-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <StatusDot status={agent.status} />
            <span className="font-mono text-sm text-foreground/80">agent-{agent.runId}</span>
          </div>
          <Badge variant="secondary" className="text-[10px] h-5 px-2">{agent.status}</Badge>
        </div>
      </CardHeader>
      <CardContent className="px-4 pb-4 space-y-3">
        <div className="space-y-1 text-xs">
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Task:</span>
            {agent.taskId ? (
              <Link to={`/tasks/${agent.taskId}`} className="text-primary/80 hover:text-primary transition-colors">
                {agent.taskId} {agent.taskTitle ? `"${agent.taskTitle}"` : ""}
              </Link>
            ) : (
              <span className="text-muted-foreground/40">-</span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Workflow:</span>
            {agent.workflowId ? (
              <Link to={`/workflows/${agent.workflowId}`} className="text-primary/80 hover:text-primary transition-colors font-mono">
                {agent.workflowId}
              </Link>
            ) : (
              <span className="text-muted-foreground/40">-</span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Phase:</span>
            <span className="font-mono text-foreground/70">{agent.phaseId ?? "-"}</span>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Duration:</span>
            <span className="font-mono text-foreground/70">{elapsed || "-"}</span>
          </div>
        </div>

        {agent.workflowId && (
          <div className="space-y-1.5">
            <SectionHeading>Output (last 10 lines)</SectionHeading>
            <AgentOutputPreview workflowId={agent.workflowId} phaseId={agent.phaseId} />
          </div>
        )}

        <div className="flex items-center gap-2 pt-1">
          {agent.workflowId && (
            <>
              <Button size="sm" variant="outline" className="h-6 text-[11px] px-2" asChild>
                <Link to={`/workflows/${agent.workflowId}`}>View Full Output</Link>
              </Button>
              <Button size="sm" variant="ghost" className="h-6 text-[11px] px-2" asChild>
                <Link to={`/workflows/${agent.workflowId}`}>View Workflow</Link>
              </Button>
            </>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

export function AgentManagementPage() {
  const [result] = useQuery({ query: DaemonDocument });
  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const status = data?.daemonStatus;
  const health = data?.daemonHealth;
  const agents = data?.agentRuns ?? [];
  const activeCount = agents.filter((a: { status: string }) => a.status.toLowerCase() === "running").length;
  const maxAgents = status?.maxAgents ?? health?.activeAgents ?? 0;

  const overallHealth = agents.length > 0 ? "running" : health?.healthy ? "healthy" : "error";

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">Agents</h1>
        <span className="text-sm text-muted-foreground/70">
          {activeCount} active / {maxAgents} capacity
        </span>
        <StatusDot status={overallHealth} />
      </div>

      {agents.length === 0 ? (
        <Card className="border-border/40 bg-card/60">
          <CardContent className="py-12 flex flex-col items-center gap-3">
            <div className="w-10 h-10 rounded-full border border-border/40 flex items-center justify-center">
              <span className="text-muted-foreground/40 text-lg">&#x2699;</span>
            </div>
            <p className="text-sm text-muted-foreground">No agents running</p>
            <p className="text-xs text-muted-foreground/50">
              Dispatch a workflow to start an agent.{" "}
              <Link to="/workflows/dispatch/task" className="text-primary/80 hover:text-primary transition-colors underline">
                Go to dispatch
              </Link>
            </p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-2">
          <SectionHeading>Active Agents</SectionHeading>
          <div className="grid md:grid-cols-2 gap-4">
            {agents.map((a) => (
              <AgentCard key={a.runId} agent={a} />
            ))}
          </div>
        </div>
      )}

      <div className="space-y-2">
        <SectionHeading>System Capacity</SectionHeading>
        <Card className="border-border/40 bg-card/60">
          <CardContent className="space-y-1 px-4 py-3 text-xs">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Max Agents</span>
              <span className="font-mono text-foreground/70">{status?.maxAgents ?? "-"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Runner Connected</span>
              <span className="font-mono text-foreground/70">
                {health?.runnerConnected ? (
                  <Badge variant="outline" className="text-[10px] h-4 px-1.5 border-primary/20 text-primary/70">yes</Badge>
                ) : (
                  "no"
                )}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Runner PID</span>
              <span className="font-mono text-foreground/70">{health?.runnerPid ?? "-"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Daemon PID</span>
              <span className="font-mono text-foreground/70">{health?.daemonPid ?? "-"}</span>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
