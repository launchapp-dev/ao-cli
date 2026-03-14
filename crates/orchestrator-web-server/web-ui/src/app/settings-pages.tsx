import { Link, useLocation } from "react-router-dom";
import { useQuery } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { WorkflowDefinitionsDocument } from "@/lib/graphql/generated/graphql";
import { PageLoading, PageError, SectionHeading } from "./shared";

function SettingsNav() {
  const { pathname } = useLocation();
  const links = [
    { to: "/settings/mcp", label: "MCP Servers" },
    { to: "/settings/agents", label: "Agent Profiles" },
  ];
  return (
    <div className="flex gap-2 mb-4">
      {links.map((l) => (
        <Link
          key={l.to}
          to={l.to}
          className={`text-sm px-2 py-1 rounded-md transition-colors ${
            pathname === l.to
              ? "text-primary font-medium bg-primary/8"
              : "text-muted-foreground hover:text-foreground/80"
          }`}
        >
          {l.label}
        </Link>
      ))}
    </div>
  );
}

const MCP_YAML_EXAMPLE = `mcp_servers:
  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/mcp-filesystem"]
    env:
      ROOT_DIR: "."
  custom-tools:
    command: "./tools/mcp-server"
    args: ["--port", "3100"]`;

export function McpServersPage() {
  const [result] = useQuery({ query: WorkflowDefinitionsDocument });
  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const definitions = data?.workflowDefinitions ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">MCP Servers</h1>
        <p className="text-sm text-muted-foreground mt-1">Configure MCP tool servers for agent workflows</p>
      </div>

      <SettingsNav />

      <Card className="border-border/40 bg-card/60">
        <CardHeader className="pb-2 pt-3 px-4">
          <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Configuration</CardTitle>
        </CardHeader>
        <CardContent className="px-4 pb-4 space-y-3">
          <p className="text-sm text-muted-foreground">
            MCP servers are configured in <code className="font-mono text-[11px] text-foreground/70 bg-background/50 px-1 py-0.5 rounded">.ao/workflows/custom.yaml</code> under the <code className="font-mono text-[11px] text-foreground/70 bg-background/50 px-1 py-0.5 rounded">mcp_servers</code> key.
          </p>
          <pre className="font-mono text-[11px] text-foreground/70 bg-background/50 p-3 rounded-md overflow-x-auto">
            {MCP_YAML_EXAMPLE}
          </pre>
          <p className="text-xs text-muted-foreground/60">
            Visual MCP server management coming soon. Use the{" "}
            <Link to="/workflows/builder" className="text-primary/80 hover:text-primary transition-colors">
              workflow builder
            </Link>{" "}
            to configure workflows that reference MCP servers.
          </p>
        </CardContent>
      </Card>

      {definitions.length > 0 && (
        <div className="space-y-3">
          <SectionHeading>Workflows Using MCP</SectionHeading>
          <div className="grid gap-3 md:grid-cols-2">
            {definitions.map((def) => (
              <Card key={def.id} className="border-border/40 bg-card/60">
                <CardContent className="pt-3 pb-3 px-4 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-medium text-foreground/90">{def.name}</span>
                    <Badge variant="outline" className="text-[10px] h-4 px-1.5 font-mono border-border/40">
                      {def.id}
                    </Badge>
                  </div>
                  {def.description && (
                    <p className="text-xs text-muted-foreground">{def.description}</p>
                  )}
                  <div className="flex flex-wrap gap-1">
                    {def.phases.map((phase) => (
                      <Badge
                        key={phase}
                        variant="secondary"
                        className="text-[10px] h-4 px-1.5 font-mono"
                      >
                        {phase}
                      </Badge>
                    ))}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export function AgentProfilesPage() {
  const [result] = useQuery({ query: WorkflowDefinitionsDocument });
  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const definitions = data?.workflowDefinitions ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Agent Profiles</h1>
        <p className="text-sm text-muted-foreground mt-1">View agent configurations and capabilities</p>
      </div>

      <SettingsNav />

      <Card className="border-border/40 bg-card/60">
        <CardHeader className="pb-2 pt-3 px-4">
          <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Configuration</CardTitle>
        </CardHeader>
        <CardContent className="px-4 pb-4 space-y-3">
          <p className="text-sm text-muted-foreground">
            Agent profiles are defined in <code className="font-mono text-[11px] text-foreground/70 bg-background/50 px-1 py-0.5 rounded">.ao/state/agent-runtime-config.v2.json</code> and control which models and tools agents use per workflow phase.
          </p>
          <p className="text-xs text-muted-foreground/60">
            Each profile specifies a <code className="font-mono text-[10px] text-foreground/60 bg-background/50 px-1 py-0.5 rounded">model</code> and <code className="font-mono text-[10px] text-foreground/60 bg-background/50 px-1 py-0.5 rounded">tool</code> that override compiled defaults. Set fields to <code className="font-mono text-[10px] text-foreground/60 bg-background/50 px-1 py-0.5 rounded">null</code> to use compiled defaults.
          </p>
        </CardContent>
      </Card>

      {definitions.length > 0 && (
        <div className="space-y-3">
          <SectionHeading>Workflow Phase Sequences</SectionHeading>
          <div className="space-y-3">
            {definitions.map((def) => (
              <Card key={def.id} className="border-border/40 bg-card/60">
                <CardHeader className="pb-2 pt-3 px-4">
                  <CardTitle className="text-sm font-medium text-foreground/90 flex items-center justify-between">
                    {def.name}
                    <Badge variant="outline" className="text-[10px] h-4 px-1.5 font-mono border-border/40 font-normal">
                      {def.id}
                    </Badge>
                  </CardTitle>
                </CardHeader>
                <CardContent className="px-4 pb-3 space-y-2">
                  {def.description && (
                    <p className="text-xs text-muted-foreground">{def.description}</p>
                  )}
                  <div className="flex items-center gap-1 flex-wrap">
                    {def.phases.map((phase, i) => (
                      <span key={phase} className="flex items-center gap-1">
                        <Badge variant="secondary" className="text-[10px] h-5 px-2 font-mono">
                          {phase}
                        </Badge>
                        {i < def.phases.length - 1 && (
                          <span className="text-muted-foreground/30 text-xs">&rarr;</span>
                        )}
                      </span>
                    ))}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      )}

      {definitions.length === 0 && (
        <Card className="border-border/40 bg-card/60">
          <CardContent className="pt-3 pb-3 px-4">
            <p className="text-sm text-muted-foreground text-center py-4">
              No workflow definitions found. Create workflows in the{" "}
              <Link to="/workflows/builder" className="text-primary/80 hover:text-primary transition-colors">
                workflow builder
              </Link>.
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
