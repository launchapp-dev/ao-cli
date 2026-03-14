import { Link, useLocation } from "react-router-dom";
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
import { WorkflowConfigDocument } from "@/lib/graphql/generated/graphql";
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

export function McpServersPage() {
  const [result] = useQuery({ query: WorkflowConfigDocument });
  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const config = data?.workflowConfig;
  const mcpServers = config?.mcpServers ?? [];
  const tools = config?.tools ?? [];
  const schedules = config?.schedules ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">MCP Servers</h1>
        <p className="text-sm text-muted-foreground mt-1">Configure MCP tool servers for agent workflows</p>
      </div>

      <SettingsNav />

      {mcpServers.length === 0 ? (
        <Card className="border-border/40 bg-card/60">
          <CardContent className="pt-3 pb-3 px-4">
            <p className="text-sm text-muted-foreground text-center py-4">No MCP servers configured.</p>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {mcpServers.map((srv) => (
            <Card key={srv.name} className="border-border/40 bg-card/60">
              <CardHeader className="pb-2 pt-3 px-4">
                <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">MCP Server</CardTitle>
              </CardHeader>
              <CardContent className="px-4 pb-4 space-y-3">
                <p className="font-mono text-primary text-sm">{srv.name}</p>
                <p className="font-mono text-xs text-foreground/70">{srv.command} {srv.args.join(" ")}</p>
                {srv.transport && (
                  <Badge variant="outline" className="text-[10px] h-4 px-1.5">{srv.transport}</Badge>
                )}
                {srv.tools.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {srv.tools.map((t) => (
                      <Badge key={t} variant="secondary" className="text-[10px] h-4 px-1.5 font-mono">{t}</Badge>
                    ))}
                  </div>
                )}
                {srv.env.length > 0 && (
                  <div className="space-y-1">
                    {srv.env.map((e) => (
                      <div key={e.key} className="flex items-center gap-2 text-xs">
                        <span className="text-muted-foreground/60 font-mono">{e.key}</span>
                        <span className="text-foreground/70 font-mono">{e.value}</span>
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {tools.length > 0 && (
        <div className="space-y-3">
          <SectionHeading>Tools</SectionHeading>
          <div className="grid gap-3 md:grid-cols-2">
            {tools.map((t) => (
              <Card key={t.name} className="border-border/40 bg-card/60">
                <CardContent className="pt-3 pb-3 px-4 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-sm text-foreground/90">{t.name}</span>
                    <span className="text-xs text-muted-foreground font-mono">{t.executable}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    {t.supportsMcp && <Badge variant="secondary" className="text-[10px] h-4 px-1.5">MCP</Badge>}
                    {t.supportsWrite && <Badge variant="secondary" className="text-[10px] h-4 px-1.5">Write</Badge>}
                    {t.contextWindow != null && (
                      <span className="text-xs text-muted-foreground">{t.contextWindow.toLocaleString()} ctx</span>
                    )}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      )}

      {schedules.length > 0 && (
        <div className="space-y-3">
          <SectionHeading>Schedules</SectionHeading>
          <div className="grid gap-3 md:grid-cols-2">
            {schedules.map((s) => (
              <Card key={s.id} className="border-border/40 bg-card/60">
                <CardContent className="pt-3 pb-3 px-4 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-foreground/90">{s.id}</span>
                    <Badge variant={s.enabled ? "secondary" : "outline"} className="text-[10px] h-4 px-1.5">
                      {s.enabled ? "enabled" : "disabled"}
                    </Badge>
                  </div>
                  <p className="font-mono text-xs text-foreground/70">{s.cron}</p>
                  {s.workflowRef && (
                    <span className="text-xs text-muted-foreground">{s.workflowRef}</span>
                  )}
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
  const [result] = useQuery({ query: WorkflowConfigDocument });
  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const config = data?.workflowConfig;
  const profiles = config?.agentProfiles ?? [];
  const catalog = config?.phaseCatalog ?? [];

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Agent Profiles</h1>
        <p className="text-sm text-muted-foreground mt-1">View agent configurations and capabilities</p>
      </div>

      <SettingsNav />

      {profiles.length === 0 ? (
        <Card className="border-border/40 bg-card/60">
          <CardContent className="pt-3 pb-3 px-4">
            <p className="text-sm text-muted-foreground text-center py-4">No agent profiles configured.</p>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {profiles.map((p) => (
            <Card key={p.name} className="border-border/40 bg-card/60">
              <CardHeader className="pb-2 pt-3 px-4">
                <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Agent Profile</CardTitle>
              </CardHeader>
              <CardContent className="px-4 pb-4 space-y-3">
                <p className="font-mono text-primary text-sm">{p.name}</p>
                {p.description && <p className="text-xs text-muted-foreground">{p.description}</p>}
                {p.role && <Badge variant="outline" className="text-[10px] h-4 px-1.5">{p.role}</Badge>}
                <div className="space-y-1">
                  {p.model && (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="text-muted-foreground/60">model</span>
                      <span className="text-foreground/70 font-mono">{p.model}</span>
                    </div>
                  )}
                  {p.tool && (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="text-muted-foreground/60">tool</span>
                      <span className="text-foreground/70 font-mono">{p.tool}</span>
                    </div>
                  )}
                </div>
                {p.mcpServers.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {p.mcpServers.map((s) => (
                      <Badge key={s} variant="secondary" className="text-[10px] h-4 px-1.5 font-mono">{s}</Badge>
                    ))}
                  </div>
                )}
                {p.skills.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {p.skills.map((s) => (
                      <Badge key={s} variant="outline" className="text-[10px] h-4 px-1.5 font-mono">{s}</Badge>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {catalog.length > 0 && (
        <div className="space-y-3">
          <SectionHeading>Phase Catalog</SectionHeading>
          <Card className="border-border/40 bg-card/60">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-36">ID</TableHead>
                  <TableHead>Label</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>Category</TableHead>
                  <TableHead>Tags</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {catalog.map((c) => (
                  <TableRow key={c.id}>
                    <TableCell className="font-mono text-xs">{c.id}</TableCell>
                    <TableCell className="text-sm">{c.label}</TableCell>
                    <TableCell className="text-xs text-muted-foreground">{c.description}</TableCell>
                    <TableCell><Badge variant="outline" className="text-[10px] h-4 px-1.5">{c.category}</Badge></TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {c.tags.map((t) => (
                          <Badge key={t} variant="secondary" className="text-[10px] h-4 px-1.5">{t}</Badge>
                        ))}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Card>
        </div>
      )}
    </div>
  );
}
