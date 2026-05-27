import { PassThrough } from 'node:stream';
import { describe, expect, it } from 'vitest';

import {
  buildInitializeResult,
  buildManifest,
  createWire,
  definePlugin,
  encodeFrame,
  ErrorCode,
  errorResponse,
  okResponse,
  parseFrame,
  PluginKind,
  PROTOCOL_VERSION,
  validateInitializeParams,
  type RpcRequest,
  type RpcResponse,
  type SubjectBackend,
} from './index.js';

// ---- wire round-trip -------------------------------------------------------

describe('wire', () => {
  it('parses a valid initialize frame', () => {
    const raw = JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: { protocol_version: '1.0.0', host_info: { name: 'animus', version: 'x' }, capabilities: {} },
    });
    const frame = parseFrame(raw);
    expect(frame.method).toBe('initialize');
    expect(frame.id).toBe(1);
    expect((frame.params as { protocol_version: string }).protocol_version).toBe('1.0.0');
  });

  it('rejects non-2.0 frames', () => {
    expect(() => parseFrame(JSON.stringify({ jsonrpc: '1.0', method: 'x' }))).toThrow();
  });

  it('rejects frames missing method', () => {
    expect(() => parseFrame(JSON.stringify({ jsonrpc: '2.0', id: 1 }))).toThrow();
  });

  it('encodeFrame is newline-terminated UTF-8 JSON', () => {
    const payload = encodeFrame(okResponse(7, { ok: true }));
    expect(payload.endsWith('\n')).toBe(true);
    const parsed = JSON.parse(payload) as RpcResponse;
    expect(parsed.id).toBe(7);
    expect(parsed.jsonrpc).toBe('2.0');
  });

  it('round-trips request through createWire', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (chunk: Buffer) => captured.push(chunk.toString('utf8')));

    const wire = createWire({ input, output, logger: () => undefined });
    const handled: RpcRequest[] = [];

    const done = wire.run(async (frame) => {
      handled.push(frame);
      return okResponse(frame.id ?? null, { echoed: frame.method });
    });

    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 42, method: 'ping' })}\n`);
    input.end();
    await done;

    expect(handled).toHaveLength(1);
    expect(handled[0]?.method).toBe('ping');
    const joined = captured.join('');
    expect(joined).toContain('"id":42');
    expect(joined).toContain('"echoed":"ping"');
    expect(joined.endsWith('\n')).toBe(true);
  });

  it('preserves multi-byte UTF-8 across chunk boundaries', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (chunk: Buffer) => captured.push(chunk.toString('utf8')));
    const wire = createWire({ input, output, logger: () => undefined });
    const seen: RpcRequest[] = [];
    const done = wire.run(async (frame) => {
      seen.push(frame);
      return okResponse(frame.id ?? null, { ok: true });
    });

    // The 4-byte UTF-8 sequence for U+1F600 ("😀") split across two writes.
    const fullFrame = `${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'echo', params: { t: 'hi 😀' } })}\n`;
    const buf = Buffer.from(fullFrame, 'utf8');
    // Find the byte offset of the emoji and split inside it.
    const emojiByteStart = Buffer.from(fullFrame.slice(0, fullFrame.indexOf('😀')), 'utf8').length;
    input.write(buf.slice(0, emojiByteStart + 2)); // first 2 bytes of the 4-byte emoji
    input.write(buf.slice(emojiByteStart + 2));
    input.end();
    await done;
    expect(seen).toHaveLength(1);
    expect((seen[0]?.params as { t: string }).t).toBe('hi 😀');
  });

  it('treats id:null as a request, not a notification', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (chunk: Buffer) => captured.push(chunk.toString('utf8')));
    const wire = createWire({ input, output, logger: () => undefined });
    const done = wire.run(async (frame) => okResponse(frame.id ?? null, { method: frame.method }));
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: null, method: '$/ping' })}\n`);
    input.end();
    await done;
    const frames = captured
      .join('')
      .split('\n')
      .filter((l) => l.length > 0)
      .map((l) => JSON.parse(l) as RpcResponse);
    expect(frames).toHaveLength(1);
    expect(frames[0]?.id).toBeNull();
    expect((frames[0]?.result as { method: string }).method).toBe('$/ping');
  });

  it('skips invalid lines without killing the loop', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (chunk: Buffer) => captured.push(chunk.toString('utf8')));
    const wire = createWire({ input, output, logger: () => undefined });

    const handled: string[] = [];
    const done = wire.run(async (frame) => {
      handled.push(frame.method);
      return okResponse(frame.id ?? null, {});
    });

    input.write('{not json\n');
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'good' })}\n`);
    input.end();
    await done;
    expect(handled).toEqual(['good']);
  });
});

// ---- handshake -------------------------------------------------------------

describe('handshake', () => {
  it('buildManifest copies methods from capabilities', () => {
    const manifest = buildManifest(
      { name: 'p', version: '0.1.0', description: 'd', plugin_kind: PluginKind.SubjectBackend },
      { methods: ['subject/list', 'subject/get'] },
    );
    expect(manifest.protocol_version).toBe(PROTOCOL_VERSION);
    expect(manifest.capabilities).toEqual(['subject/list', 'subject/get']);
    expect(manifest.env_required).toEqual([]);
  });

  it('buildInitializeResult includes plugin_info + capabilities', () => {
    const result = buildInitializeResult(
      { name: 'p', version: '0.1.0', description: 'd', plugin_kind: PluginKind.SubjectBackend },
      { methods: ['subject/list'] },
    );
    expect(result.protocol_version).toBe(PROTOCOL_VERSION);
    expect(result.plugin_info.name).toBe('p');
    expect(result.capabilities.methods).toContain('subject/list');
  });

  it('validateInitializeParams accepts matching major', () => {
    expect(
      validateInitializeParams({
        protocol_version: '1.99.0',
        host_info: { name: 'animus', version: 'x' },
        capabilities: {},
      }),
    ).toBeNull();
  });

  it('validateInitializeParams rejects mismatched major', () => {
    expect(
      validateInitializeParams({
        protocol_version: '2.0.0',
        host_info: { name: 'animus', version: 'x' },
        capabilities: {},
      }),
    ).toMatch(/incompatible/);
  });
});

// ---- definePlugin (subject backend) ---------------------------------------

const NOW = '2026-05-27T00:00:00.000Z';
const sampleBackend: SubjectBackend = {
  list: () => ({
    subjects: [
      { id: 'task:1', kind: 'task', title: 'hello', status: 'ready', created_at: NOW, updated_at: NOW },
    ],
  }),
  get: ({ id }, ctx) => ({
    id,
    kind: ctx.kind,
    title: 'hello',
    status: 'ready',
    created_at: NOW,
    updated_at: NOW,
  }),
};

describe('definePlugin', () => {
  it('rejects missing name/version/description', () => {
    expect(() =>
      // @ts-expect-error - intentionally invalid for the test
      definePlugin({ kind: PluginKind.SubjectBackend, impl: sampleBackend, version: '1', description: 'd' }),
    ).toThrow();
  });

  it('rejects unknown kind', () => {
    expect(() =>
      // @ts-expect-error - intentionally invalid for the test
      definePlugin({ kind: 'nope', impl: {}, name: 'x', version: '0.1.0', description: 'd' }),
    ).toThrow();
  });

  it('rejects unwired non-subject roles in 0.1.0', () => {
    expect(() =>
      definePlugin({
        kind: PluginKind.Provider,
        name: 'p',
        version: '0.1.0',
        description: 'd',
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        impl: { run: async () => ({}) as any } as never,
      }),
    ).toThrow(/not yet wired/);
  });

  it('rejects subject_backend without list/get', () => {
    expect(() =>
      // @ts-expect-error - intentionally invalid for the test
      definePlugin({ kind: PluginKind.SubjectBackend, name: 'x', version: '0.1.0', description: 'd', impl: {} }),
    ).toThrow();
  });

  it('builds a valid manifest with kind-prefixed methods + subject_kind tokens', () => {
    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'animus-subject-sample',
      version: '0.1.0',
      description: 'sample',
      subject_kinds: ['task'],
      impl: sampleBackend,
    });
    const m = handle.manifest();
    expect(m.name).toBe('animus-subject-sample');
    expect(m.plugin_kind).toBe(PluginKind.SubjectBackend);
    // SubjectRouter dispatches `<kind>/<verb>` — we must advertise those.
    expect(m.capabilities).toContain('task/list');
    expect(m.capabilities).toContain('task/get');
    expect(m.capabilities).not.toContain('task/create');
    // Preflight scans for `subject_kind:<kind>` tokens.
    expect(m.capabilities).toContain('subject_kind:task');
  });

  it('dispatches subject/list end-to-end over the wire', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'animus-subject-sample',
      version: '0.1.0',
      description: 'sample',
      subject_kinds: ['task'],
      impl: sampleBackend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });

    const done = handle.run();

    input.write(
      `${JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: { protocol_version: '1.0.0', host_info: { name: 'animus', version: 'x' }, capabilities: {} },
      })}\n`,
    );
    // The host dispatches `<kind>/list` after SubjectRouter resolves the kind.
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'task/list', params: {} })}\n`);
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'health/check' })}\n`);
    input.end();
    await done;

    const frames = captured
      .join('')
      .split('\n')
      .filter((line) => line.length > 0)
      .map((line) => JSON.parse(line) as RpcResponse);

    expect(frames).toHaveLength(3);
    expect(frames[0]?.id).toBe(1);
    expect((frames[0]?.result as { protocol_version: string }).protocol_version).toBe('1.0.0');
    expect(frames[1]?.id).toBe(2);
    const listResult = frames[1]?.result as { subjects: unknown[]; fetched_at: string };
    expect(listResult.subjects).toHaveLength(1);
    expect(typeof listResult.fetched_at).toBe('string');
    expect(frames[2]?.id).toBe(3);
    expect((frames[2]?.result as { status: string }).status).toBe('healthy');
  });

  it('rejects subject calls for unknown kinds', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: sampleBackend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'requirement/list', params: {} })}\n`);
    input.end();
    await done;
    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    expect(frame.error?.code).toBe(ErrorCode.MethodNotFound);
    expect(frame.error?.message).toMatch(/requirement/);
  });

  it('unknown method returns MethodNotFound', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      impl: sampleBackend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });

    const done = handle.run();
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 9, method: 'bogus/op' })}\n`);
    input.end();
    await done;

    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    expect(frame.error?.code).toBe(ErrorCode.MethodNotFound);
  });

  it('errorResponse shape is JSON-RPC compliant', () => {
    const e = errorResponse(5, ErrorCode.InvalidParams, 'bad', { hint: 'pass kind' });
    expect(e.jsonrpc).toBe('2.0');
    expect(e.id).toBe(5);
    expect(e.error?.code).toBe(ErrorCode.InvalidParams);
    expect(e.error?.data).toEqual({ hint: 'pass kind' });
  });

  it('auto-fills wire-mandatory subject fields when backend omits them', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    const sparseBackend: SubjectBackend = {
      // Intentionally omit status/created_at/updated_at to exercise the safety
      // net. `as any` mirrors what a careless hello-world author might write.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      list: () => ({ subjects: [{ id: 'task:HELLO', kind: 'task', title: 'hi' } as any] }),
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      get: ({ id }, ctx) => ({ id, kind: ctx.kind, title: 'hi' } as any),
    };

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: sparseBackend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'task/list', params: {} })}\n`);
    input.end();
    await done;
    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    const result = frame.result as { subjects: Array<Record<string, unknown>> };
    expect(result.subjects[0]?.status).toBe('ready');
    expect(typeof result.subjects[0]?.created_at).toBe('string');
    expect(typeof result.subjects[0]?.updated_at).toBe('string');
  });

  it('backfills ctx.kind into create params and list kind filter', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    let createSeen: Record<string, unknown> | null = null;
    let listSeen: Record<string, unknown> | null = null;

    const backend: SubjectBackend = {
      list: (params) => {
        listSeen = params as unknown as Record<string, unknown>;
        return { subjects: [] };
      },
      get: ({ id }, ctx) => ({
        id,
        kind: ctx.kind,
        title: 't',
        status: 'ready',
        created_at: NOW,
        updated_at: NOW,
      }),
      create: (params, ctx) => {
        createSeen = params as unknown as Record<string, unknown>;
        return {
          id: 'task:NEW',
          kind: ctx.kind,
          title: (params as { title: string }).title,
          status: 'ready',
          created_at: NOW,
          updated_at: NOW,
        };
      },
    };

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: backend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    // List with NO kind filter — SDK should default it from the route.
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'task/list', params: {} })}\n`);
    // Create without `kind` field — SDK should inject it from the route.
    input.write(
      `${JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'task/create', params: { title: 'new' } })}\n`,
    );
    input.end();
    await done;

    expect(listSeen).not.toBeNull();
    expect((listSeen as { kind: string[] }).kind).toEqual(['task']);
    expect(createSeen).not.toBeNull();
    expect((createSeen as { kind: string }).kind).toBe('task');
    expect((createSeen as { title: string }).title).toBe('new');
  });

  it('delegates health/check to the backend impl when provided', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));
    const backend: SubjectBackend = {
      list: () => ({ subjects: [] }),
      get: () => null,
      health: () => ({ status: 'degraded', last_error: 'upstream slow' }),
    };
    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: backend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'health/check' })}\n`);
    input.end();
    await done;
    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    const result = frame.result as { status: string; last_error: string };
    expect(result.status).toBe('degraded');
    expect(result.last_error).toBe('upstream slow');
  });

  it('translates a null get result into a not-found error', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));
    const backend: SubjectBackend = {
      list: () => ({ subjects: [] }),
      get: () => null,
    };
    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: backend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    input.write(
      `${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'task/get', params: { id: 'task:MISSING' } })}\n`,
    );
    input.end();
    await done;
    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    expect(frame.error?.code).toBe(ErrorCode.InvalidParams);
    expect(frame.error?.message).toMatch(/not found.*MISSING/);
    expect((frame.error?.data as { category: string }).category).toBe('not_found');
  });

  it('unwraps daemon `{filter: ...}` envelope and CLI `body` field', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    let listSeen: Record<string, unknown> | null = null;
    let createSeen: Record<string, unknown> | null = null;
    const backend: SubjectBackend = {
      list: (params) => {
        listSeen = params as unknown as Record<string, unknown>;
        return { subjects: [] };
      },
      get: ({ id }, ctx) => ({
        id,
        kind: ctx.kind,
        title: 't',
        status: 'ready',
        created_at: NOW,
        updated_at: NOW,
      }),
      create: (params, ctx) => {
        createSeen = params as unknown as Record<string, unknown>;
        return {
          id: 'task:NEW',
          kind: ctx.kind,
          title: params.title,
          status: 'ready',
          created_at: NOW,
          updated_at: NOW,
        };
      },
    };
    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: backend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    // Daemon-wrapped list call.
    input.write(
      `${JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'task/list',
        params: { filter: { status: ['ready'], limit: 5 } },
      })}\n`,
    );
    // CLI-style create call carrying `body` instead of `description`.
    input.write(
      `${JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'task/create',
        params: { title: 'cli', body: 'long form text' },
      })}\n`,
    );
    input.end();
    await done;

    expect(listSeen).not.toBeNull();
    // Flat shape exposed to impl, not nested under `filter`.
    expect((listSeen as { status: string[] }).status).toEqual(['ready']);
    expect((listSeen as { limit: number }).limit).toBe(5);
    // `kind` filter still backfilled from the route.
    expect((listSeen as { kind: string[] }).kind).toEqual(['task']);

    expect(createSeen).not.toBeNull();
    expect((createSeen as { description: string }).description).toBe('long form text');
    expect((createSeen as { body?: string }).body).toBeUndefined();
    expect((createSeen as { kind: string }).kind).toBe('task');
  });

  it('accepts wildcard subject_kinds (e.g. "task.*" matches "task.foo")', async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const captured: string[] = [];
    output.on('data', (c: Buffer) => captured.push(c.toString('utf8')));

    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task.*'],
      impl: sampleBackend,
      input: input as unknown as NodeJS.ReadableStream,
      output: output as unknown as NodeJS.WritableStream,
      skipCliArgs: true,
    });
    const done = handle.run();
    input.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'task.tracked/list', params: {} })}\n`);
    input.end();
    await done;
    const frame = JSON.parse(captured.join('').trim()) as RpcResponse;
    expect(frame.error).toBeUndefined();
    expect((frame.result as { subjects: unknown[] }).subjects).toHaveLength(1);
  });

  it('initialize response uses the shared PROTOCOL_VERSION constant', () => {
    const handle = definePlugin({
      kind: PluginKind.SubjectBackend,
      name: 'p',
      version: '0.1.0',
      description: 'd',
      subject_kinds: ['task'],
      impl: sampleBackend,
    });
    const reply = handle.initialize({
      protocol_version: '1.0.0',
      host_info: { name: 'animus', version: 'x' },
      capabilities: {},
    });
    expect((reply.result as { protocol_version: string }).protocol_version).toBe(PROTOCOL_VERSION);
  });
});
