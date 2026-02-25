// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const apiMocks = vi.hoisted(() => ({
  daemonHealth: vi.fn(),
  daemonLogs: vi.fn(),
  daemonStart: vi.fn(),
  daemonPause: vi.fn(),
  daemonResume: vi.fn(),
  daemonStop: vi.fn(),
  daemonClearLogs: vi.fn(),
}));

vi.mock("../lib/api/client", () => ({
  api: {
    daemonHealth: apiMocks.daemonHealth,
    daemonLogs: apiMocks.daemonLogs,
    daemonStart: apiMocks.daemonStart,
    daemonPause: apiMocks.daemonPause,
    daemonResume: apiMocks.daemonResume,
    daemonStop: apiMocks.daemonStop,
    daemonClearLogs: apiMocks.daemonClearLogs,
  },
  firstApiError: (...results: Array<{ kind: "ok" | "error" }>) =>
    results.find((result) => result.kind === "error") ?? null,
}));

import { DaemonPage } from "./screens";

function okResult<TData>(data: TData) {
  return Promise.resolve({
    kind: "ok" as const,
    data,
  });
}

function errorResult(code: string, message: string, exitCode = 1) {
  return Promise.resolve({
    kind: "error" as const,
    code,
    message,
    exitCode,
  });
}

async function renderDaemonPage() {
  render(<DaemonPage />);
  await waitFor(() => {
    expect(apiMocks.daemonHealth).toHaveBeenCalledTimes(1);
    expect(apiMocks.daemonLogs).toHaveBeenCalledTimes(1);
  });
}

describe("DaemonPage high-risk safeguards", () => {
  beforeEach(() => {
    for (const mock of Object.values(apiMocks)) {
      mock.mockReset();
    }

    apiMocks.daemonHealth.mockReturnValue(okResult({ status: "healthy" }));
    apiMocks.daemonLogs.mockReturnValue(
      okResult([
        {
          timestamp: "2026-02-25T10:00:00.000Z",
          level: "info",
          message: "daemon booted",
        },
      ]),
    );
    apiMocks.daemonStart.mockReturnValue(okResult({ message: "start ok" }));
    apiMocks.daemonPause.mockReturnValue(okResult({ message: "pause ok" }));
    apiMocks.daemonResume.mockReturnValue(okResult({ message: "resume ok" }));
    apiMocks.daemonStop.mockReturnValue(okResult({ message: "stop ok" }));
    apiMocks.daemonClearLogs.mockReturnValue(okResult({ message: "clear ok" }));
  });

  it("gates high-risk stop action with typed confirmation and does not execute immediately", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    expect(screen.getByRole("heading", { name: "Review High-Risk Action" })).toBeTruthy();
    expect(screen.getByText(/Type/i)).toBeTruthy();
    expect(screen.getByText("STOP")).toBeTruthy();
    expect(apiMocks.daemonStop).not.toHaveBeenCalled();

    const confirmButton = screen.getByRole("button", { name: "Run Dry-Run Preview" }) as HTMLButtonElement;
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "  stop  " },
    });

    expect(confirmButton.disabled).toBe(false);
  });

  it("records dry-run preview for high-risk actions without mutating API calls", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Clear Logs" }));
    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "CLEAR LOGS" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Run Dry-Run Preview" }));

    expect(apiMocks.daemonClearLogs).not.toHaveBeenCalled();
    expect(screen.getByRole("status").textContent).toContain("Dry-run preview completed for Clear Logs.");
    expect(screen.queryByRole("heading", { name: "Review High-Risk Action" })).toBeNull();
    expect(screen.getByText(/"outcome": "preview"/)).toBeTruthy();
    expect(screen.getByText(/"action": "clear"/)).toBeTruthy();
  });

  it("executes confirmed high-risk actions and records successful audit entries", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "STOP" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: "Preview only (dry-run, no API call)" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm and Execute" }));

    await waitFor(() => {
      expect(apiMocks.daemonStop).toHaveBeenCalledTimes(1);
    });

    expect(screen.getByRole("status").textContent).toContain("Stop completed.");
    expect(screen.getByText(/"mode": "execute"/)).toBeTruthy();
    expect(screen.getByText(/"outcome": "ok"/)).toBeTruthy();
    expect(screen.getByText(/"action": "stop"/)).toBeTruthy();
  });

  it("executes medium-risk actions directly and renders auditable failures", async () => {
    apiMocks.daemonPause.mockReturnValue(errorResult("conflict", "daemon already paused", 4));
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Pause" }));

    await waitFor(() => {
      expect(apiMocks.daemonPause).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByRole("heading", { name: "Review High-Risk Action" })).toBeNull();
    expect(screen.getByRole("alert").textContent).toContain("Error: daemon_action_failed");
    expect(screen.getByRole("alert").textContent).toContain("conflict: daemon already paused");
    expect(screen.getByText(/"outcome": "error"/)).toBeTruthy();
    expect(screen.getByText(/"action": "pause"/)).toBeTruthy();
  });
});
