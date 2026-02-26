import { useEffect, useRef, useState } from "react";

import { requestAo } from "../api/client";

export type RunTelemetryConnectionState =
  | "idle"
  | "connecting"
  | "streaming"
  | "polling"
  | "error";

export type RunTelemetryEntry = {
  cursor: number;
  source_file: string;
  line: string;
  timestamp_hint?: string;
  payload?: unknown;
};

type RunTelemetryBatch = {
  cursor: number;
  entries: RunTelemetryEntry[];
};

const MAX_TELEMETRY_ENTRIES = 300;
const POLL_INTERVAL_MS = 1_500;

export function useRunTelemetry(options: {
  runId: string;
  taskId?: string;
  phaseId?: string;
  enabled?: boolean;
}) {
  const normalizedRunId = options.runId.trim();
  const taskId = options.taskId?.trim();
  const phaseId = options.phaseId?.trim();
  const isEnabled = options.enabled ?? true;
  const [connectionState, setConnectionState] = useState<RunTelemetryConnectionState>("idle");
  const [entries, setEntries] = useState<RunTelemetryEntry[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const lastCursorRef = useRef(0);

  useEffect(() => {
    if (!isEnabled || normalizedRunId.length === 0) {
      setConnectionState("idle");
      setEntries([]);
      setErrorMessage(null);
      lastCursorRef.current = 0;
      return;
    }

    let isCancelled = false;
    let retryTimer: number | null = null;
    let streamAbortController: AbortController | null = null;

    const appendEntries = (nextEntries: RunTelemetryEntry[]) => {
      if (nextEntries.length === 0) {
        return;
      }

      setEntries((current) => {
        const merged = [...current];
        const seen = new Set(current.map((entry) => entry.cursor));
        for (const entry of nextEntries) {
          if (seen.has(entry.cursor)) {
            continue;
          }
          merged.push(entry);
          seen.add(entry.cursor);
        }

        merged.sort((left, right) => left.cursor - right.cursor);
        return merged.length > MAX_TELEMETRY_ENTRIES
          ? merged.slice(merged.length - MAX_TELEMETRY_ENTRIES)
          : merged;
      });
    };

    const startPolling = () => {
      if (isCancelled) {
        return;
      }

      setConnectionState("polling");

      const pollOnce = async () => {
        if (isCancelled) {
          return;
        }

        const batchResult = await requestAo<unknown>(
          buildTelemetryPollPath(normalizedRunId, {
            after: lastCursorRef.current,
            taskId,
            phaseId,
            limit: 200,
          }),
        );

        if (batchResult.kind === "error") {
          setConnectionState("error");
          setErrorMessage(`${batchResult.code}: ${batchResult.message}`);
          retryTimer = window.setTimeout(() => {
            void pollOnce();
          }, POLL_INTERVAL_MS);
          return;
        }

        const decoded = decodeRunTelemetryBatch(batchResult.data);
        if (!decoded) {
          setConnectionState("error");
          setErrorMessage("Invalid telemetry response payload.");
          retryTimer = window.setTimeout(() => {
            void pollOnce();
          }, POLL_INTERVAL_MS);
          return;
        }

        lastCursorRef.current = decoded.cursor;
        appendEntries(decoded.entries);
        setErrorMessage(null);

        retryTimer = window.setTimeout(() => {
          void pollOnce();
        }, decoded.entries.length > 0 ? 250 : POLL_INTERVAL_MS);
      };

      void pollOnce();
    };

    const connectStream = async () => {
      streamAbortController = new AbortController();
      setConnectionState("connecting");
      setErrorMessage(null);

      const headers: Record<string, string> = {
        Accept: "text/event-stream",
      };
      if (lastCursorRef.current > 0) {
        headers["Last-Event-ID"] = String(lastCursorRef.current);
      }

      const response = await fetch(
        buildTelemetryStreamPath(normalizedRunId, {
          taskId,
          phaseId,
          limit: 200,
        }),
        {
          method: "GET",
          headers,
          signal: streamAbortController.signal,
        },
      );

      if (!response.ok || !response.body) {
        throw new Error(`stream unavailable (${response.status})`);
      }

      setConnectionState("streaming");

      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      let eventName = "";
      let dataLines: string[] = [];

      const flushEvent = () => {
        if (dataLines.length === 0) {
          eventName = "";
          return;
        }
        if (eventName && eventName !== "run-telemetry") {
          eventName = "";
          dataLines = [];
          return;
        }

        try {
          const payload = JSON.parse(dataLines.join("\n"));
          const decodedEntry = decodeRunTelemetryEntry(payload);
          if (!decodedEntry) {
            return;
          }

          lastCursorRef.current = Math.max(lastCursorRef.current, decodedEntry.cursor);
          appendEntries([decodedEntry]);
        } finally {
          eventName = "";
          dataLines = [];
        }
      };

      while (!isCancelled) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }

        buffer += value;
        while (true) {
          const newLineIndex = buffer.indexOf("\n");
          if (newLineIndex < 0) {
            break;
          }

          let line = buffer.slice(0, newLineIndex);
          buffer = buffer.slice(newLineIndex + 1);
          if (line.endsWith("\r")) {
            line = line.slice(0, -1);
          }
          if (line === "") {
            flushEvent();
            continue;
          }
          if (line.startsWith(":")) {
            continue;
          }

          const separator = line.indexOf(":");
          const field = separator >= 0 ? line.slice(0, separator) : line;
          let fieldValue = separator >= 0 ? line.slice(separator + 1) : "";
          if (fieldValue.startsWith(" ")) {
            fieldValue = fieldValue.slice(1);
          }

          if (field === "event") {
            eventName = fieldValue;
            continue;
          }
          if (field === "id") {
            const parsedCursor = Number.parseInt(fieldValue, 10);
            if (Number.isFinite(parsedCursor) && parsedCursor > 0) {
              lastCursorRef.current = Math.max(lastCursorRef.current, parsedCursor);
            }
            continue;
          }
          if (field === "data") {
            dataLines.push(fieldValue);
          }
        }
      }

      throw new Error("stream closed");
    };

    void connectStream().catch((error) => {
      if (isCancelled || streamAbortController?.signal.aborted) {
        return;
      }
      setErrorMessage(error instanceof Error ? error.message : "Stream unavailable.");
      startPolling();
    });

    return () => {
      isCancelled = true;
      streamAbortController?.abort();
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [isEnabled, normalizedRunId, phaseId, taskId]);

  return {
    connectionState,
    entries,
    errorMessage,
  };
}

function decodeRunTelemetryBatch(payload: unknown): RunTelemetryBatch | null {
  if (!isRecord(payload)) {
    return null;
  }

  const cursor = payload["cursor"];
  if (typeof cursor !== "number" || !Number.isFinite(cursor) || cursor < 0) {
    return null;
  }

  const entriesValue = payload["entries"];
  if (!Array.isArray(entriesValue)) {
    return null;
  }

  const entries: RunTelemetryEntry[] = [];
  for (const value of entriesValue) {
    const entry = decodeRunTelemetryEntry(value);
    if (!entry) {
      continue;
    }
    entries.push(entry);
  }

  return {
    cursor,
    entries,
  };
}

function decodeRunTelemetryEntry(value: unknown): RunTelemetryEntry | null {
  if (!isRecord(value)) {
    return null;
  }

  const cursor = value["cursor"];
  const sourceFile = value["source_file"];
  const line = value["line"];
  if (typeof cursor !== "number" || !Number.isFinite(cursor) || cursor <= 0) {
    return null;
  }
  if (typeof sourceFile !== "string" || typeof line !== "string") {
    return null;
  }

  const timestampHint = value["timestamp_hint"];
  return {
    cursor,
    source_file: sourceFile,
    line,
    ...(typeof timestampHint === "string" ? { timestamp_hint: timestampHint } : {}),
    ...(value["payload"] !== undefined ? { payload: value["payload"] } : {}),
  };
}

function buildTelemetryPollPath(
  runId: string,
  options: {
    after?: number;
    limit?: number;
    taskId?: string;
    phaseId?: string;
  },
): string {
  const query = new URLSearchParams();
  if (options.after !== undefined) {
    query.set("after", String(options.after));
  }
  if (options.limit !== undefined) {
    query.set("limit", String(options.limit));
  }
  if (options.taskId && options.taskId.length > 0) {
    query.set("task_id", options.taskId);
  }
  if (options.phaseId && options.phaseId.length > 0) {
    query.set("phase_id", options.phaseId);
  }

  const basePath = `/api/v1/output/runs/${encodeURIComponent(runId)}/telemetry`;
  const queryValue = query.toString();
  return queryValue.length > 0 ? `${basePath}?${queryValue}` : basePath;
}

function buildTelemetryStreamPath(
  runId: string,
  options: {
    limit?: number;
    taskId?: string;
    phaseId?: string;
  },
): string {
  const query = new URLSearchParams();
  if (options.limit !== undefined) {
    query.set("limit", String(options.limit));
  }
  if (options.taskId && options.taskId.length > 0) {
    query.set("task_id", options.taskId);
  }
  if (options.phaseId && options.phaseId.length > 0) {
    query.set("phase_id", options.phaseId);
  }

  const basePath = `/api/v1/output/runs/${encodeURIComponent(runId)}/telemetry/stream`;
  const queryValue = query.toString();
  return queryValue.length > 0 ? `${basePath}?${queryValue}` : basePath;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
