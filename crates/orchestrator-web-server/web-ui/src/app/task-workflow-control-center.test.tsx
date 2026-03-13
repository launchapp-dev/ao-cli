// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  useQuery: vi.fn(),
  useMutation: vi.fn(),
}));

vi.mock("urql", async () => {
  const actual = await vi.importActual("urql");
  return {
    ...actual,
    useQuery: mocks.useQuery,
    useMutation: mocks.useMutation,
  };
});

vi.mock("@/lib/graphql/provider", () => ({
  GraphQLProvider: ({ children }: { children: React.ReactNode }) => children,
}));

import { TasksPage, WorkflowsPage } from "./screens";

function renderInRouter(element: ReactElement) {
  return render(<MemoryRouter>{element}</MemoryRouter>);
}

describe("task and workflow control center", () => {
  let executeMutation: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    executeMutation = vi.fn().mockResolvedValue({ data: {} });
    mocks.useMutation.mockReturnValue([{ fetching: false }, executeMutation]);
  });

  it("renders tasks list from GraphQL query results", async () => {
    mocks.useQuery.mockReturnValue([
      {
        data: {
          tasks: [
            {
              id: "TASK-001",
              title: "Build web UI",
              status: "In Progress",
              statusRaw: "in-progress",
              priority: "High",
              priorityRaw: "high",
              taskType: "Feature",
              taskTypeRaw: "feature",
              tags: [],
              linkedRequirementIds: [],
            },
          ],
          taskStats: { total: 1, byStatus: '{"in-progress":1}', byPriority: '{"high":1}' },
        },
        fetching: false,
        error: null,
      },
      vi.fn(),
    ]);

    renderInRouter(<TasksPage />);

    expect(screen.getByText("Tasks")).toBeTruthy();
    expect(screen.getByText("TASK-001")).toBeTruthy();
    expect(screen.getByText("Build web UI")).toBeTruthy();
    expect(screen.getByText("in-progress")).toBeTruthy();
  });

  it("shows empty state when no tasks match filters", () => {
    mocks.useQuery.mockReturnValue([
      {
        data: {
          tasks: [],
          taskStats: { total: 0, byStatus: "{}", byPriority: "{}" },
        },
        fetching: false,
        error: null,
      },
      vi.fn(),
    ]);

    renderInRouter(<TasksPage />);

    expect(screen.getByText("No tasks match filters.")).toBeTruthy();
  });

  it("renders workflows with run form and action buttons", async () => {
    mocks.useQuery.mockReturnValue([
      {
        data: {
          workflows: [
            {
              id: "wf-1",
              taskId: "TASK-014",
              workflowRef: null,
              status: "Running",
              statusRaw: "running",
              currentPhase: "implementation",
              totalReworks: 0,
              phases: [
                {
                  phaseId: "implementation",
                  status: "running",
                  startedAt: "2026-02-26T05:00:00Z",
                  completedAt: null,
                  attempt: 1,
                  errorMessage: null,
                },
              ],
            },
          ],
        },
        fetching: false,
        error: null,
      },
      vi.fn(),
    ]);

    renderInRouter(<WorkflowsPage />);

    expect(screen.getByText("Workflows")).toBeTruthy();
    expect(screen.getByText("wf-1")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Run Workflow" })).toBeTruthy();
  });

  it("shows empty state when no workflows exist", () => {
    mocks.useQuery.mockReturnValue([
      {
        data: { workflows: [] },
        fetching: false,
        error: null,
      },
      vi.fn(),
    ]);

    renderInRouter(<WorkflowsPage />);

    expect(screen.getByText("No workflows found.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Run Workflow" })).toBeTruthy();
  });

  it("executes run workflow mutation", async () => {
    mocks.useQuery.mockReturnValue([
      {
        data: { workflows: [] },
        fetching: false,
        error: null,
      },
      vi.fn(),
    ]);

    renderInRouter(<WorkflowsPage />);

    const input = screen.getByPlaceholderText("Task ID (e.g. TASK-014)");
    fireEvent.change(input, { target: { value: "TASK-101" } });
    fireEvent.click(screen.getByRole("button", { name: "Run Workflow" }));

    await waitFor(() => {
      expect(executeMutation).toHaveBeenCalledWith(
        expect.objectContaining({ taskId: "TASK-101" }),
      );
    });
  });
});
