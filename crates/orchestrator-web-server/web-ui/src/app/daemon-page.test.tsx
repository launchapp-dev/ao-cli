// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

  it("opens modal safeguards and enforces exact typed phrase before execution", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Stop Daemon" }));

    const dialog = screen.getByRole("dialog", { name: "Review High-Risk Action" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByText("STOP DAEMON")).toBeTruthy();
    expect(apiMocks.daemonStop).not.toHaveBeenCalled();

    const confirmButton = screen.getByRole("button", { name: "Confirm and Execute" }) as HTMLButtonElement;
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "stop daemon" },
    });
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "  STOP DAEMON  " },
    });
    expect(confirmButton.disabled).toBe(false);
  });

  it("records dry-run preview for high-risk actions without mutating API calls", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Clear Daemon Logs" }));
    fireEvent.click(screen.getByRole("button", { name: "Run Dry-Run Preview" }));

    expect(apiMocks.daemonClearLogs).not.toHaveBeenCalled();
    expect(screen.getByRole("status").textContent).toContain(
      "Dry-run preview ready for Clear daemon logs.",
    );
    expect(screen.getByRole("dialog", { name: "Review High-Risk Action" })).toBeTruthy();
  });

  it("executes confirmed high-risk actions and records successful auditable feedback", async () => {
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Stop Daemon" }));
    fireEvent.change(screen.getByLabelText("Confirmation phrase"), {
      target: { value: "STOP DAEMON" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Confirm and Execute" }));

    await waitFor(() => {
      expect(apiMocks.daemonStop).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByRole("dialog", { name: "Review High-Risk Action" })).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("stop ok");
    const feedbackPanel = screen.getByRole("heading", { name: "Action Feedback" }).closest("div");
    expect(feedbackPanel).toBeTruthy();
    expect(within(feedbackPanel!).getByText(/^daemon\.stop$/)).toBeTruthy();
    expect(within(feedbackPanel!).getByText(/^ok$/)).toBeTruthy();
    expect(within(feedbackPanel!).getByText(/stop ok/)).toBeTruthy();
    expect(within(feedbackPanel!).getByText(/Correlation ID:/)).toBeTruthy();
    expect(within(feedbackPanel!).getByText(/ao-web-/)).toBeTruthy();
  });

  it("supports escape dismissal and restores focus to the triggering control", async () => {
    await renderDaemonPage();

    const stopButton = screen.getByRole("button", { name: "Stop Daemon" });
    fireEvent.click(stopButton);

    const dialog = screen.getByRole("dialog", { name: "Review High-Risk Action" });
    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Review High-Risk Action" })).toBeNull();
      expect(document.activeElement).toBe(stopButton);
    });

    expect(apiMocks.daemonStop).not.toHaveBeenCalled();
  });

  it("executes medium-risk actions directly and renders auditable failures", async () => {
    apiMocks.daemonPause.mockReturnValue(errorResult("conflict", "daemon already paused", 4));
    await renderDaemonPage();

    fireEvent.click(screen.getByRole("button", { name: "Pause Daemon" }));

    await waitFor(() => {
      expect(apiMocks.daemonPause).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByRole("dialog", { name: "Review High-Risk Action" })).toBeNull();
    expect(screen.getByRole("alert").textContent).toContain("Error: daemon_action_failed");
    expect(screen.getByRole("alert").textContent).toContain("conflict: daemon already paused");
    expect(screen.getByText(/^daemon\.pause$/)).toBeTruthy();
    expect(screen.getByText(/conflict: daemon already paused/)).toBeTruthy();
  });

  it("prevents duplicate submissions while an action request is pending", async () => {
    let resolvePause: ((value: unknown) => void) | null = null;
    apiMocks.daemonPause.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolvePause = resolve;
        }),
    );

    await renderDaemonPage();

    const pauseButton = screen.getByRole("button", { name: "Pause Daemon" }) as HTMLButtonElement;
    fireEvent.click(pauseButton);

    await waitFor(() => {
      expect(pauseButton.disabled).toBe(true);
      expect(apiMocks.daemonPause).toHaveBeenCalledTimes(1);
    });

    fireEvent.click(pauseButton);
    expect(apiMocks.daemonPause).toHaveBeenCalledTimes(1);

    resolvePause?.({
      kind: "ok",
      data: {
        message: "pause delayed ok",
      },
    });

    await waitFor(() => {
      expect(pauseButton.disabled).toBe(false);
    });
  });

  it("keeps daemon feedback bounded to 50 records with most-recent-first ordering", async () => {
    await renderDaemonPage();

    const startButton = screen.getByRole("button", { name: "Start Daemon" });
    for (let index = 0; index < 55; index += 1) {
      fireEvent.click(startButton);
      await waitFor(() => {
        expect(apiMocks.daemonStart).toHaveBeenCalledTimes(index + 1);
      });
    }

    expect(screen.getAllByText(/^daemon\.start$/).length).toBe(50);
  });
});
