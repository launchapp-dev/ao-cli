import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  NavLink,
  Outlet,
  useLocation,
  useMatches,
  useNavigate,
} from "react-router-dom";
import {
  LayoutDashboard,
  ListTodo,
  GitBranch,
  Layers,
  FileText,
  Server,
  Activity,
  ClipboardCheck,
  Search,
  Menu,
  ChevronRight,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import {
  Dialog,
  DialogContent,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { ProjectContextProvider, useProjectContext } from "./project-context";
import { GraphQLProvider } from "@/lib/graphql/provider";

export const PRIMARY_NAV_ITEMS = [
  { to: "/dashboard", label: "Dashboard", icon: LayoutDashboard },
  { to: "/tasks", label: "Tasks", icon: ListTodo },
  { to: "/workflows", label: "Workflows", icon: GitBranch },
  { to: "/queue", label: "Queue", icon: Layers },
  { to: "/planning", label: "Planning", icon: FileText },
  { to: "/daemon", label: "Daemon", icon: Server },
  { to: "/events", label: "Events", icon: Activity },
  { to: "/reviews/handoff", label: "Review", icon: ClipboardCheck },
] as const;

export const MAIN_CONTENT_ID = "main-content";

export function AppShellLayout() {
  const routeProjectId = useRouteProjectId();

  return (
    <GraphQLProvider>
      <ProjectContextProvider routeProjectId={routeProjectId}>
        <AppShellFrame />
      </ProjectContextProvider>
    </GraphQLProvider>
  );
}

function AppShellFrame() {
  const [mobileOpen, setMobileOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();

  const projectContext = useProjectContext();

  useEffect(() => {
    setMobileOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setCommandOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const breadcrumbs = useMemo(() => {
    return location.pathname
      .split("/")
      .filter(Boolean)
      .map((s) => s.replace(/-/g, " "));
  }, [location.pathname]);

  const onProjectChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const id = e.target.value || null;
      projectContext.setActiveProjectId(id);
      if (id) navigate(`/projects/${id}`);
    },
    [projectContext, navigate],
  );

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <aside className="hidden md:flex w-56 flex-col border-r border-border bg-sidebar">
        <SidebarContent />
      </aside>

      <div className="flex flex-1 flex-col overflow-hidden">
        <header className="flex h-12 items-center gap-3 border-b border-border px-4 bg-card">
          <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
            <SheetTrigger asChild>
              <Button variant="ghost" size="icon" className="md:hidden">
                <Menu className="h-5 w-5" />
                <span className="sr-only">Toggle navigation</span>
              </Button>
            </SheetTrigger>
            <SheetContent side="left" className="w-56 p-0">
              <SidebarContent />
            </SheetContent>
          </Sheet>

          <nav aria-label="Breadcrumb" className="flex items-center gap-1 text-sm text-muted-foreground min-w-0">
            {breadcrumbs.map((crumb, i) => (
              <span key={i} className="flex items-center gap-1 capitalize truncate">
                {i > 0 && <ChevronRight className="h-3 w-3 shrink-0" />}
                <span className={i === breadcrumbs.length - 1 ? "text-foreground font-medium" : ""}>
                  {crumb}
                </span>
              </span>
            ))}
          </nav>

          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="hidden sm:flex items-center gap-2 text-muted-foreground text-xs"
              onClick={() => setCommandOpen(true)}
            >
              <Search className="h-3 w-3" />
              <span>Search...</span>
              <kbd className="pointer-events-none border border-border rounded px-1 text-[10px] bg-muted">
                ⌘K
              </kbd>
            </Button>

            <select
              value={projectContext.activeProjectId ?? ""}
              onChange={onProjectChange}
              className="h-8 rounded-md border border-input bg-background px-2 text-xs"
            >
              <option value="">No project</option>
              {projectContext.projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>

            <Badge variant="outline" className="text-xs hidden lg:inline-flex">
              {projectContext.source}
            </Badge>
          </div>
        </header>

        <main
          id={MAIN_CONTENT_ID}
          className="flex-1 overflow-y-auto p-4 md:p-6"
          tabIndex={-1}
        >
          <Outlet />
        </main>
      </div>

      <CommandPalette
        open={commandOpen}
        onOpenChange={setCommandOpen}
        navigate={navigate}
      />
    </div>
  );
}

function SidebarContent() {
  return (
    <div className="flex h-full flex-col">
      <div className="px-4 py-3">
        <h1 className="text-base font-semibold tracking-tight">AO</h1>
        <p className="text-xs text-muted-foreground">Agent Orchestrator</p>
      </div>
      <Separator />
      <nav className="flex-1 space-y-0.5 px-2 py-2" aria-label="Primary">
        {PRIMARY_NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors ${
                isActive
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
              }`
            }
          >
            <item.icon className="h-4 w-4 shrink-0" />
            {item.label}
          </NavLink>
        ))}
      </nav>
      <Separator />
      <div className="px-4 py-2">
        <NavLink
          to="/projects"
          className="text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          Projects
        </NavLink>
      </div>
    </div>
  );
}

function CommandPalette({
  open,
  onOpenChange,
  navigate,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  navigate: ReturnType<typeof useNavigate>;
}) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  const goTo = useCallback(
    (path: string) => {
      onOpenChange(false);
      navigate(path);
    },
    [navigate, onOpenChange],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key !== "Enter" || !query.trim()) return;
      const q = query.trim().toUpperCase();

      if (q.startsWith("TASK-")) {
        goTo(`/tasks/${q}`);
      } else if (q.startsWith("WF-") || q.startsWith("WORKFLOW-")) {
        goTo(`/workflows/${q}`);
      } else if (q.startsWith("REQ-")) {
        goTo(`/planning/requirements/${q}`);
      } else {
        goTo(`/tasks?search=${encodeURIComponent(query.trim())}`);
      }
    },
    [query, goTo],
  );

  const filteredNav = useMemo(() => {
    if (!query.trim()) return PRIMARY_NAV_ITEMS;
    const q = query.toLowerCase();
    return PRIMARY_NAV_ITEMS.filter(
      (item) =>
        item.label.toLowerCase().includes(q) ||
        item.to.toLowerCase().includes(q),
    );
  }, [query]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md p-0 gap-0">
        <div className="flex items-center border-b px-3">
          <Search className="h-4 w-4 text-muted-foreground shrink-0" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Go to TASK-XXX, REQ-XXX, or search..."
            className="border-0 focus-visible:ring-0 shadow-none"
          />
        </div>
        <div className="max-h-64 overflow-y-auto p-1">
          {filteredNav.map((item) => (
            <button
              key={item.to}
              type="button"
              onClick={() => goTo(item.to)}
              className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm text-left hover:bg-accent transition-colors"
            >
              <item.icon className="h-4 w-4 text-muted-foreground" />
              {item.label}
            </button>
          ))}
          {query.trim() && (
            <p className="px-3 py-2 text-xs text-muted-foreground">
              Press Enter to jump to ID or search tasks
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function useRouteProjectId(): string | null {
  const matches = useMatches();

  for (let index = matches.length - 1; index >= 0; index -= 1) {
    const params = matches[index].params as Record<string, string | undefined>;
    if (params.projectId) return params.projectId;
  }

  return null;
}
