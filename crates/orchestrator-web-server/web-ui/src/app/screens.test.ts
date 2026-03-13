import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const screensPath = resolve(import.meta.dirname, "./screens.tsx");

describe("screens module", () => {
  it("exports all required page components", () => {
    const source = readFileSync(screensPath, "utf8");

    const requiredExports = [
      "DashboardPage",
      "TasksPage",
      "TaskCreatePage",
      "TaskDetailPage",
      "WorkflowsPage",
      "WorkflowDetailPage",
      "WorkflowCheckpointPage",
      "QueuePage",
      "DaemonPage",
      "ProjectsPage",
      "ProjectDetailPage",
      "RequirementDetailPage",
      "EventsPage",
      "ReviewHandoffPage",
      "NotFoundPage",
    ];

    for (const name of requiredExports) {
      expect(source).toContain(`export function ${name}(`);
    }
  });

  it("uses GraphQL queries via urql hooks", () => {
    const source = readFileSync(screensPath, "utf8");

    expect(source).toContain("useQuery");
    expect(source).toContain("useMutation");
    expect(source).toContain('from "urql"');
  });
});
