import {
  createContext,
  ReactNode,
  useContext,
  useMemo,
} from "react";
import { useQuery } from "@/lib/graphql/client";

type ProjectSummary = {
  id: string;
  name: string;
  path?: string;
};

type ProjectContextSource = "route-param" | "cached-selection" | "server-active" | "none";

export type ProjectContextValue = {
  activeProjectId: string | null;
  source: ProjectContextSource;
  projects: ProjectSummary[];
  setActiveProjectId: (projectId: string | null) => void;
};

export type ResolveProjectContextInput = {
  routeProjectId: string | null;
  cachedProjectId: string | null;
  serverActiveProjectId: string | null;
};

const STORAGE_KEY = "ao.web.active_project";

const PROJECTS_QUERY = `
  query Projects {
    projects { id name path }
    projectsActive { id name path }
  }
`;

const ProjectContext = createContext<ProjectContextValue | null>(null);

export function resolveProjectContext(
  input: ResolveProjectContextInput,
): Pick<ProjectContextValue, "activeProjectId" | "source"> {
  if (input.routeProjectId) {
    return {
      activeProjectId: input.routeProjectId,
      source: "route-param",
    };
  }

  if (input.cachedProjectId) {
    return {
      activeProjectId: input.cachedProjectId,
      source: "cached-selection",
    };
  }

  if (input.serverActiveProjectId) {
    return {
      activeProjectId: input.serverActiveProjectId,
      source: "server-active",
    };
  }

  return {
    activeProjectId: null,
    source: "none",
  };
}

export function ProjectContextProvider(props: {
  routeProjectId: string | null;
  children: ReactNode;
}) {
  const [{ data }] = useQuery({ query: PROJECTS_QUERY });

  const projects: ProjectSummary[] = data?.projects ?? [];
  const activeProjects: ProjectSummary[] = data?.projectsActive ?? [];
  const serverActiveProjectId = activeProjects.length > 0 ? activeProjects[0].id : null;

  const cachedProjectId = useMemo(() => {
    if (typeof window === "undefined") return null;
    return window.localStorage.getItem(STORAGE_KEY);
  }, []);

  const resolved = resolveProjectContext({
    routeProjectId: props.routeProjectId,
    cachedProjectId,
    serverActiveProjectId,
  });

  const value = useMemo<ProjectContextValue>(() => {
    return {
      activeProjectId: resolved.activeProjectId,
      source: resolved.source,
      projects,
      setActiveProjectId: (projectId) => {
        if (typeof window !== "undefined") {
          if (projectId) {
            window.localStorage.setItem(STORAGE_KEY, projectId);
          } else {
            window.localStorage.removeItem(STORAGE_KEY);
          }
        }
      },
    };
  }, [projects, resolved.activeProjectId, resolved.source]);

  return <ProjectContext.Provider value={value}>{props.children}</ProjectContext.Provider>;
}

export function useProjectContext() {
  const context = useContext(ProjectContext);

  if (!context) {
    throw new Error("useProjectContext must be used inside ProjectContextProvider");
  }

  return context;
}
