// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const apiMocks = vi.hoisted(() => ({
  outputRunJsonl: vi.fn(),
  outputArtifacts: vi.fn(),
  outputArtifactDownload: vi.fn(),
}));

const telemetryMock = vi.hoisted(() =>
  vi.fn(() => ({
    connectionState: "streaming" as const,
    entries: [
      {
        cursor: 1,
        source_file: "events.jsonl",
        line: "{\"event\":\"run-start\"}",
      },
    ],
    errorMessage: null,
  })),
);

vi.mock("../lib/api/client", async () => {
  const actual = await vi.importActual<typeof import("../lib/api/client")>(
    "../lib/api/client",
  );
  return {
    ...actual,
    api: {
      ...actual.api,
      outputRunJsonl: apiMocks.outputRunJsonl,
      outputArtifacts: apiMocks.outputArtifacts,
      outputArtifactDownload: apiMocks.outputArtifactDownload,
    },
  };
});

vi.mock("../lib/output/use-run-telemetry", () => ({
  useRunTelemetry: telemetryMock,
}));

import { OutputPage } from "./screens";

function okResult<TData>(data: TData) {
  return Promise.resolve({
    kind: "ok" as const,
    data,
  });
}

describe("OutputPage", () => {
  beforeEach(() => {
    for (const mock of Object.values(apiMocks)) {
      mock.mockReset();
    }
    telemetryMock.mockClear();

    apiMocks.outputRunJsonl.mockReturnValue(
      okResult({
        run_id: "run-1",
        total_matches: 1,
        truncated: false,
        entries: [
          {
            cursor: 1,
            source_file: "events.jsonl",
            line: "{\"event\":\"run-start\"}",
          },
        ],
      }),
    );
    apiMocks.outputArtifacts.mockReturnValue(
      okResult([
        {
          artifact_id: "logs/output.log",
          artifact_type: "log",
          size_bytes: 3,
        },
      ]),
    );
    apiMocks.outputArtifactDownload.mockReturnValue(
      okResult({
        artifact_id: "logs/output.log",
        execution_id: "exec-1",
        size_bytes: 3,
        bytes: [97, 98, 99],
      }),
    );
  });

  it("searches JSONL output using the current run and filters", async () => {
    render(<OutputPage />);

    fireEvent.change(screen.getByLabelText("Run ID"), { target: { value: "run-1" } });
    fireEvent.change(screen.getByLabelText("Contains Text"), {
      target: { value: "run-start" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Search JSONL" }));

    await waitFor(() => {
      expect(apiMocks.outputRunJsonl).toHaveBeenCalledTimes(1);
    });
    expect(apiMocks.outputRunJsonl).toHaveBeenCalledWith("run-1", {
      contains: "run-start",
      sourceFile: undefined,
      taskId: undefined,
      phaseId: undefined,
      limit: 500,
    });
    expect(screen.getByText(/Matches:/)).toBeTruthy();
  });

  it("lists artifacts and downloads selected artifact payload", async () => {
    const originalCreateObjectURL = (URL as typeof URL & {
      createObjectURL?: (object: Blob) => string;
    }).createObjectURL;
    const originalRevokeObjectURL = (URL as typeof URL & {
      revokeObjectURL?: (url: string) => void;
    }).revokeObjectURL;

    const createObjectUrlSpy = vi.fn(() => "blob:test");
    const revokeObjectUrlSpy = vi.fn();
    const anchorClickSpy = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});
    Object.defineProperty(URL, "createObjectURL", {
      value: createObjectUrlSpy,
      configurable: true,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      value: revokeObjectUrlSpy,
      configurable: true,
    });

    render(<OutputPage />);
    fireEvent.change(screen.getByLabelText("Execution ID"), { target: { value: "exec-1" } });
    fireEvent.click(screen.getByRole("button", { name: "Load Artifacts" }));

    await waitFor(() => {
      expect(apiMocks.outputArtifacts).toHaveBeenCalledWith("exec-1");
    });
    fireEvent.click(screen.getByRole("button", { name: "Download" }));

    await waitFor(() => {
      expect(apiMocks.outputArtifactDownload).toHaveBeenCalledWith("exec-1", "logs/output.log");
    });
    expect(createObjectUrlSpy).toHaveBeenCalledTimes(1);
    expect(revokeObjectUrlSpy).toHaveBeenCalledTimes(1);

    if (originalCreateObjectURL) {
      Object.defineProperty(URL, "createObjectURL", {
        value: originalCreateObjectURL,
        configurable: true,
      });
    } else {
      Reflect.deleteProperty(URL, "createObjectURL");
    }
    if (originalRevokeObjectURL) {
      Object.defineProperty(URL, "revokeObjectURL", {
        value: originalRevokeObjectURL,
        configurable: true,
      });
    } else {
      Reflect.deleteProperty(URL, "revokeObjectURL");
    }
    anchorClickSpy.mockRestore();
  });
});
