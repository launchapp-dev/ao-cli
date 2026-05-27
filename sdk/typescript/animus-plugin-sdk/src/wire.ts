// Newline-delimited JSON-RPC 2.0 transport over a Node Readable/Writable pair
// (stdin/stdout by default).
//
// Mirrors the Rust runtime in `crates/animus-plugin-runtime/src/lib.rs`:
//   - one JSON-RPC frame per line of stdin
//   - replies + server-streaming notifications are newline-terminated JSON
//     written to stdout
//   - parse errors on a single line are logged-and-skipped (do not kill the
//     read loop)
//   - empty/whitespace-only lines are ignored
//
// Stdout is reserved for protocol frames; SDK diagnostics MUST go to stderr.

import type { Readable, Writable } from 'node:stream';
import { stdin as nodeStdin, stdout as nodeStdout } from 'node:process';
import { StringDecoder } from 'node:string_decoder';

import type { RpcId, RpcNotification, RpcRequest, RpcResponse } from './types/index.js';

/** A handler invoked for every parsed inbound JSON-RPC frame.
 *  Return `undefined` for notifications (no response expected). */
export type FrameHandler = (frame: RpcRequest) => Promise<RpcResponse | undefined> | RpcResponse | undefined;

export interface WireOptions {
  /** Readable stream to consume newline-delimited JSON-RPC frames from. Defaults to process.stdin. */
  input?: Readable;
  /** Writable stream to write newline-terminated JSON-RPC frames to. Defaults to process.stdout. */
  output?: Writable;
  /** Logger for invalid frames / errors. Defaults to `console.error`. */
  logger?: (msg: string, err?: unknown) => void;
}

export interface Wire {
  /** Send a JSON-RPC response. */
  sendResponse(response: RpcResponse): Promise<void>;
  /** Send a JSON-RPC server-streaming notification (no `id`). */
  sendNotification(notification: RpcNotification): Promise<void>;
  /** Convenience: build + send a notification from method/params. */
  notify(method: string, params?: unknown): Promise<void>;
  /** Begin consuming the input stream. Resolves when the stream closes (EOF). */
  run(handler: FrameHandler): Promise<void>;
}

const defaultLogger = (msg: string, err?: unknown): void => {
  if (err !== undefined) {
    process.stderr.write(`[animus-plugin-sdk] ${msg}: ${String(err)}\n`);
  } else {
    process.stderr.write(`[animus-plugin-sdk] ${msg}\n`);
  }
};

/**
 * Encode a single JSON-RPC frame as a UTF-8 newline-terminated string.
 * Exposed for tests; production code should go through `Wire.sendResponse` /
 * `Wire.sendNotification`.
 */
export function encodeFrame(frame: RpcResponse | RpcNotification | RpcRequest): string {
  return `${JSON.stringify(frame)}\n`;
}

/**
 * Parse one trimmed line into an `RpcRequest`. Throws on invalid JSON or
 * structurally invalid JSON-RPC (missing `jsonrpc: "2.0"` or `method`).
 */
export function parseFrame(line: string): RpcRequest {
  const value = JSON.parse(line) as unknown;
  if (typeof value !== 'object' || value === null) {
    throw new Error('frame is not a JSON object');
  }
  const obj = value as Record<string, unknown>;
  if (obj.jsonrpc !== '2.0') {
    throw new Error(`unsupported jsonrpc version: ${String(obj.jsonrpc)}`);
  }
  if (typeof obj.method !== 'string' || obj.method.length === 0) {
    throw new Error('frame missing string `method`');
  }
  const frame: RpcRequest = {
    jsonrpc: '2.0',
    method: obj.method,
  };
  if ('id' in obj) {
    frame.id = obj.id as RpcId;
  }
  if ('params' in obj) {
    frame.params = obj.params;
  }
  return frame;
}

class WriteQueue {
  private chain: Promise<void> = Promise.resolve();
  constructor(private readonly output: Writable) {}

  enqueue(payload: string): Promise<void> {
    const next = this.chain.then(
      () =>
        new Promise<void>((resolve, reject) => {
          const ok = this.output.write(payload, (err) => {
            if (err) reject(err);
            else resolve();
          });
          if (!ok) {
            this.output.once('drain', () => {
              // write callback still fires; nothing to do here.
            });
          }
        }),
    );
    // Swallow rejections in the chain so a single write failure doesn't poison
    // the queue; the original caller still sees the rejection.
    this.chain = next.catch(() => undefined);
    return next;
  }
}

/**
 * Create a `Wire` bound to the given (or default stdin/stdout) streams.
 *
 * Reads are line-buffered: a frame ends at the next `\n`. Frames that fail to
 * parse are logged via `options.logger` and skipped, matching the Rust
 * runtime's "tracing::warn + continue" behavior.
 */
export function createWire(options: WireOptions = {}): Wire {
  const input = options.input ?? nodeStdin;
  const output = options.output ?? nodeStdout;
  const log = options.logger ?? defaultLogger;
  const queue = new WriteQueue(output);

  const sendResponse: Wire['sendResponse'] = (response) => queue.enqueue(encodeFrame(response));
  const sendNotification: Wire['sendNotification'] = (notification) =>
    queue.enqueue(encodeFrame(notification));
  const notify: Wire['notify'] = (method, params) => {
    const frame: RpcNotification = params === undefined
      ? { jsonrpc: '2.0', method }
      : { jsonrpc: '2.0', method, params };
    return sendNotification(frame);
  };

  const run: Wire['run'] = (handler) =>
    new Promise<void>((resolve, reject) => {
      let buffer = '';
      let closed = false;
      // `StringDecoder` buffers partial multi-byte UTF-8 sequences across
      // chunk boundaries — `chunk.toString('utf8')` would replace a split
      // codepoint with U+FFFD and corrupt valid JSON containing non-ASCII
      // subject titles.
      const decoder = new StringDecoder('utf8');
      // Serialize request handling. Plugins can still kick off background work
      // inside their handler (and emit streaming notifications via
      // `notify`/`sendNotification`); the dispatch order matches inbound order
      // so request ids are responded to in arrival sequence by default.
      let dispatchChain: Promise<void> = Promise.resolve();

      const handleOne = async (line: string): Promise<void> => {
        const trimmed = line.trim();
        if (trimmed.length === 0) return;
        let frame: RpcRequest;
        try {
          frame = parseFrame(trimmed);
        } catch (err) {
          log('invalid JSON-RPC frame', err);
          return;
        }
        try {
          const response = await handler(frame);
          if (response !== undefined) {
            await sendResponse(response);
          }
        } catch (err) {
          log(`handler error for method '${frame.method}'`, err);
        }
      };

      const enqueue = (line: string): void => {
        dispatchChain = dispatchChain.then(() => handleOne(line));
      };

      const onData = (chunk: Buffer | string): void => {
        buffer += typeof chunk === 'string' ? chunk : decoder.write(chunk);
        let newlineIdx = buffer.indexOf('\n');
        while (newlineIdx !== -1) {
          const line = buffer.slice(0, newlineIdx);
          buffer = buffer.slice(newlineIdx + 1);
          enqueue(line);
          newlineIdx = buffer.indexOf('\n');
        }
      };

      const finish = async (): Promise<void> => {
        if (closed) return;
        closed = true;
        // Flush any bytes the decoder held while waiting for the rest of a
        // multi-byte sequence; emit them as part of the final line.
        buffer += decoder.end();
        if (buffer.length > 0) {
          enqueue(buffer);
          buffer = '';
        }
        // Drain serialized dispatch chain before resolving.
        await dispatchChain;
        resolve();
      };

      const onError = (err: Error): void => {
        if (closed) return;
        closed = true;
        reject(err);
      };

      input.on('data', onData);
      input.once('end', () => {
        void finish();
      });
      input.once('close', () => {
        void finish();
      });
      input.once('error', onError);
    });

  return { sendResponse, sendNotification, notify, run };
}

/** Build an `ok` response for the given request id. */
export function okResponse(id: RpcId | undefined, result: unknown): RpcResponse {
  return { jsonrpc: '2.0', id: id ?? null, result };
}

/** Build an `error` response for the given request id. */
export function errorResponse(id: RpcId | undefined, code: number, message: string, data?: unknown): RpcResponse {
  const error = data === undefined ? { code, message } : { code, message, data };
  return { jsonrpc: '2.0', id: id ?? null, error };
}
