import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  PlanningRequirementDetailPage,
  PlanningVisionPage,
} from "./planning-screens";

function okEnvelope(data: unknown) {
  return {
    schema: "ao.cli.v1",
    ok: true,
    data,
  };
}

function errorEnvelope(code: string, message: string, exitCode: number) {
  return {
    schema: "ao.cli.v1",
    ok: false,
    error: {
      code,
      message,
      exit_code: exitCode,
    },
  };
}

function jsonResponse(payload: unknown): Response {
  return {
    json: async () => payload,
  } as Response;
}

describe("planning screens", () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
  });

  it("renders a recoverable not_found state for requirement deep links", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse(errorEnvelope("not_found", "requirement not found", 3)),
    );

    render(
      <MemoryRouter initialEntries={["/planning/requirements/REQ-404"]}>
        <Routes>
          <Route
            path="/planning/requirements/:requirementId"
            element={<PlanningRequirementDetailPage />}
          />
        </Routes>
      </MemoryRouter>,
    );

    expect(
      await screen.findByText(
        "Requirement not found. It may have been deleted or moved.",
      ),
    ).toBeDefined();
    expect(
      screen.getByRole("link", { name: "Back to Requirements List" }),
    ).toBeDefined();
  });

  it("renders labeled controls for first-run vision authoring", async () => {
    fetchMock.mockResolvedValue(jsonResponse(okEnvelope(null)));

    render(
      <MemoryRouter initialEntries={["/planning/vision"]}>
        <Routes>
          <Route path="/planning/vision" element={<PlanningVisionPage />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(await screen.findByLabelText("Project Name")).toBeDefined();
    expect(screen.getByLabelText("Problem Statement")).toBeDefined();
    expect(screen.getByRole("button", { name: "Save Vision" })).toBeDefined();
  });
});
