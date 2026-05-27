// `definePlugin(spec)` — single entrypoint for authoring an Animus plugin.
//
// Authors describe their plugin (identity + role + impl) and the SDK:
//   1. handles `--manifest` CLI shortcut
//   2. runs the stdio JSON-RPC loop
//   3. dispatches `initialize`, `$/ping`, `health/check`, `shutdown`, `exit`,
//      and role methods
//   4. forwards unknown methods as `MethodNotFound`
//
// MVP role coverage:
//   - subject_backend: dispatched (subject/list, subject/get, optional verbs)
//   - provider / trigger_backend / transport_backend / log_storage_backend:
//     dispatcher returns `MethodNotFound` until the relevant wave wires them.

import process from 'node:process';
import { stdout as nodeStdout } from 'node:process';

import {
  buildInitializeResult,
  buildManifest,
  validateInitializeParams,
  type PluginIdentity,
} from './handshake.js';
import type { Subject } from './roles.js';
import {
  type LogStorageBackend,
  type Provider,
  type SubjectBackend,
  type TransportBackend,
  type TriggerBackend,
} from './roles.js';
import { createWire, errorResponse, okResponse, type Wire } from './wire.js';
import {
  ErrorCode,
  PluginKind,
  type EnvRequirement,
  type HealthCheckResult,
  type InitializeParams,
  type PluginCapabilities,
  type PluginManifest,
  type RpcId,
  type RpcRequest,
  type RpcResponse,
} from './types/index.js';

type RoleSpec =
  | {
      kind: typeof PluginKind.SubjectBackend;
      impl: SubjectBackend;
      subject_kinds?: string[];
      projections?: string[];
    }
  | {
      kind: typeof PluginKind.Provider;
      impl: Provider;
    }
  | {
      kind: typeof PluginKind.TriggerBackend;
      impl: TriggerBackend;
    }
  | {
      kind: typeof PluginKind.TransportBackend;
      impl: TransportBackend;
      capabilities?: string[];
    }
  | {
      kind: typeof PluginKind.LogStorageBackend;
      impl: LogStorageBackend;
    };

export type PluginSpec = RoleSpec & {
  name: string;
  version: string;
  description: string;
  env_required?: EnvRequirement[];
  /** Author hint for the host's notification broadcast channel capacity. */
  notification_buffer_size?: number | null;
  /** Optional override of the inbound stream (for testing). */
  input?: NodeJS.ReadableStream;
  /** Optional override of the outbound stream (for testing). */
  output?: NodeJS.WritableStream;
  /** Skip the `--manifest` CLI shortcut (useful in tests). */
  skipCliArgs?: boolean;
};

export interface PluginHandle {
  /** Drive the JSON-RPC loop until the input stream closes. */
  run(): Promise<void>;
  /** Static manifest for this plugin (also what `--manifest` prints). */
  manifest(): PluginManifest;
  /** Build the `initialize` reply (exposed for tests). */
  initialize(params: InitializeParams): RpcResponse;
}

function deriveCapabilities(spec: PluginSpec): PluginCapabilities {
  if (spec.kind === PluginKind.SubjectBackend) {
    const kinds = spec.subject_kinds ?? [];
    // The SubjectRouter dispatches `<kind>/list`, `<kind>/get`, etc. Advertise
    // each method for every declared kind so the host's `methods` introspection
    // matches what we accept on the wire.
    const verbs: string[] = ['list', 'get'];
    if (spec.impl.create) verbs.push('create');
    if (spec.impl.update) verbs.push('update');
    if (spec.impl.status) verbs.push('status');
    if (spec.impl.next) verbs.push('next');
    const methods: string[] = [];
    if (kinds.length === 0) {
      // No kinds declared yet — fall back to advertising the bare verbs (still
      // useful for `health/check`-only smoke plugins).
      for (const v of verbs) methods.push(`subject/${v}`);
    } else {
      for (const k of kinds) {
        for (const v of verbs) methods.push(`${k}/${v}`);
      }
    }
    return {
      methods,
      streaming: false,
      progress: false,
      cancellation: false,
      subject_kinds: kinds,
      projections: spec.projections ?? [],
    };
  }
  // Non-subject roles are skeleton-only in 0.1.0: dispatchRole returns
  // MethodNotFound for every domain method. Advertising methods we cannot
  // actually serve would let preflight pick this plugin as a provider /
  // trigger / transport / log backend and then fail every real call. Until
  // those role dispatchers are wired, surface an EMPTY method list so the
  // host's discovery still sees the plugin (`health/check` is always
  // implemented by the base dispatcher) but doesn't route domain calls here.
  // `transport_backend` callers can still pass `spec.capabilities` for
  // non-domain hints (e.g. role tags) — kept on the manifest via
  // `extra_capabilities` rather than methods.
  if (
    spec.kind === PluginKind.Provider ||
    spec.kind === PluginKind.TriggerBackend ||
    spec.kind === PluginKind.TransportBackend ||
    spec.kind === PluginKind.LogStorageBackend
  ) {
    return { methods: [], streaming: false, progress: false, cancellation: false };
  }
  return { methods: [] };
}

function validateSpec(spec: PluginSpec): void {
  if (!spec.name || typeof spec.name !== 'string') {
    throw new TypeError('definePlugin: `name` is required');
  }
  if (!spec.version || typeof spec.version !== 'string') {
    throw new TypeError('definePlugin: `version` is required');
  }
  if (!spec.description || typeof spec.description !== 'string') {
    throw new TypeError('definePlugin: `description` is required');
  }
  if (!spec.kind) {
    throw new TypeError('definePlugin: `kind` is required');
  }
  const valid = new Set<string>(Object.values(PluginKind));
  if (!valid.has(spec.kind)) {
    throw new TypeError(`definePlugin: unknown kind '${spec.kind}'`);
  }
  if (!spec.impl || typeof spec.impl !== 'object') {
    throw new TypeError('definePlugin: `impl` is required');
  }
  if (spec.kind === PluginKind.SubjectBackend) {
    const impl = spec.impl as SubjectBackend;
    if (typeof impl.list !== 'function') throw new TypeError('subject_backend impl must implement list()');
    if (typeof impl.get !== 'function') throw new TypeError('subject_backend impl must implement get()');
    return;
  }
  // Only `subject_backend` has its dispatcher wired in 0.1.0. Reject every
  // other kind at construction — the daemon discovers plugins by
  // `manifest.plugin_kind` and would route real calls (agent/run, trigger/watch,
  // log/append, MCP tool invocations) to a plugin that can't answer.
  throw new Error(
    `definePlugin: kind '${spec.kind}' is not yet wired in the TS SDK (0.1.0). ` +
      'Only subject_backend is supported in this release. ' +
      'See README.md "Roles" table for the roadmap.',
  );
}

function notImplemented(id: RpcId | undefined, method: string, kind: string): RpcResponse {
  return errorResponse(
    id,
    ErrorCode.MethodNotFound,
    `method '${method}' not implemented in TS SDK for kind '${kind}' yet`,
  );
}

function buildHealthOk(): HealthCheckResult {
  return { status: 'healthy', uptime_ms: null, memory_usage_bytes: null, last_error: null };
}

/**
 * Ensure a subject record carries the wire-mandatory fields the Rust daemon
 * expects (`status`, `created_at`, `updated_at`). Authors who hand back a
 * sparse `{ id, kind, title }` for hello-world examples get a sensible default
 * (`ready`, now, now) instead of an undecodable response.
 *
 * This is a safety net, not an excuse to skip the fields — production
 * backends should set all three explicitly from their source-of-truth.
 */
function ensureWireSubject(s: Subject | (Partial<Subject> & { id: string; kind: string; title: string })): Subject {
  const nowIso = new Date().toISOString();
  return {
    status: 'ready',
    created_at: nowIso,
    updated_at: nowIso,
    ...s,
  } as Subject;
}

/**
 * Author-facing entrypoint. Returns a handle whose `run()` drives the JSON-RPC
 * loop until stdin closes.
 *
 * @example
 * definePlugin({
 *   kind: 'subject_backend',
 *   name: 'hello-subjects',
 *   version: '0.1.0',
 *   description: 'Hard-coded sample subject backend',
 *   subject_kinds: ['task'],
 *   impl: {
 *     list: () => ({ items: [{ id: 'task:1', kind: 'task', title: 'hello' }] }),
 *     get: ({ id }) => ({ id, kind: 'task', title: 'hello' }),
 *   },
 * }).run();
 */
export function definePlugin(spec: PluginSpec): PluginHandle {
  validateSpec(spec);
  const identity: PluginIdentity = {
    name: spec.name,
    version: spec.version,
    description: spec.description,
    plugin_kind: spec.kind,
  };
  const capabilities = deriveCapabilities(spec);
  // For subject backends, also surface `subject_kind:<kind>` capability tokens
  // so the daemon's preflight + doctor can recognize coverage from the manifest
  // alone (without spawning the plugin).
  const extraCaps: string[] = [];
  if (spec.kind === PluginKind.SubjectBackend && spec.subject_kinds) {
    // TODO(codex-p2): the daemon's preflight (`plugin_preflight::covers_subject_kind`)
    // currently does exact-string matching on these tokens, so a wildcard
    // `task.*` is announced as `subject_kind:task.*` but won't match a
    // preflight requirement for `subject_kind:task.foo`. Routing at runtime
    // still works (SubjectRouter does glob matching). Fix requires a Rust-
    // side change to teach preflight about `.*` suffixes; tracked for the
    // next wave. For now we emit the raw token verbatim.
    for (const k of spec.subject_kinds) extraCaps.push(`subject_kind:${k}`);
  }
  if (spec.kind === PluginKind.TransportBackend && spec.capabilities) {
    for (const c of spec.capabilities) extraCaps.push(c);
  }
  const manifestPayload: PluginManifest = buildManifest(identity, capabilities, {
    env_required: spec.env_required,
    notification_buffer_size: spec.notification_buffer_size,
    extra_capabilities: extraCaps,
  });

  const handle: PluginHandle = {
    manifest: () => manifestPayload,
    initialize: (params) => {
      const incompat = validateInitializeParams(params);
      if (incompat) {
        return errorResponse(null, ErrorCode.InvalidRequest, incompat);
      }
      return okResponse(null, buildInitializeResult(identity, capabilities));
    },
    run: () => runLoop(spec, manifestPayload, identity, capabilities),
  };
  return handle;
}

async function runLoop(
  spec: PluginSpec,
  manifestPayload: PluginManifest,
  identity: PluginIdentity,
  capabilities: PluginCapabilities,
): Promise<void> {
  if (!spec.skipCliArgs) {
    // Honor `--manifest` / `-m`: print to stdout, exit 0. Honor `--help` / `-h`.
    const args = process.argv.slice(2);
    if (args.includes('--manifest') || args.includes('-m')) {
      // Wait for the write callback so the manifest fully flushes before exit
      // — Node pipe writes are async, and a bare `process.exit` after `write`
      // can truncate the discovery output.
      await new Promise<void>((resolve, reject) => {
        nodeStdout.write(`${JSON.stringify(manifestPayload)}\n`, (err) => {
          if (err) reject(err);
          else resolve();
        });
      });
      process.exit(0);
    }
    if (args.includes('--help') || args.includes('-h')) {
      await new Promise<void>((resolve) => {
        process.stderr.write(
          `${identity.name} ${identity.version} - Animus STDIO plugin\n` +
            'Usage:\n' +
            `  ${identity.name} --manifest    Print plugin manifest as JSON and exit\n` +
            `  ${identity.name}               Run JSON-RPC loop on stdin/stdout\n`,
          () => resolve(),
        );
      });
      process.exit(0);
    }
  }

  const wire: Wire = createWire({
    input: spec.input as NodeJS.ReadableStream | undefined as never,
    output: spec.output as NodeJS.WritableStream | undefined as never,
  });

  await wire.run((frame) => dispatch(frame, wire, spec, identity, capabilities));
}

async function dispatch(
  frame: RpcRequest,
  wire: Wire,
  spec: PluginSpec,
  identity: PluginIdentity,
  capabilities: PluginCapabilities,
): Promise<RpcResponse | undefined> {
  const id = frame.id;
  const method = frame.method;

  // Notifications: never respond. The host's graceful shutdown sequence is
  // `shutdown` (request) → `exit` (notification with no id); honor the latter
  // by exiting cleanly so we don't get force-killed after the grace period.
  //
  // Per JSON-RPC 2.0, ONLY a missing `id` makes a frame a notification;
  // `id: null` is still a request (the type allows null). Treat null and
  // numeric/string ids the same when dispatching.
  if (id === undefined) {
    if (method === 'exit') {
      setImmediate(() => process.exit(0));
      return undefined;
    }
    if (method === 'initialized' || method.startsWith('$/')) {
      return undefined;
    }
    // Drop unknown notifications silently to match Rust runtime.
    return undefined;
  }

  switch (method) {
    case 'initialize': {
      const params = (frame.params ?? {}) as InitializeParams;
      const incompat = validateInitializeParams(params);
      if (incompat) {
        return errorResponse(id, ErrorCode.InvalidRequest, incompat);
      }
      // Use the shared helper so the protocol version flows from
      // `PROTOCOL_VERSION` instead of being hardcoded — otherwise a future
      // version bump would silently desync the run-loop reply from the
      // manifest.
      return okResponse(id, buildInitializeResult(identity, capabilities));
    }
    case '$/ping':
      return okResponse(id, {});
    case 'health/check': {
      // Delegate to the backend's optional `health()` hook if present so
      // subject backends that depend on upstream services can degrade
      // gracefully. Default to `healthy` for backends without a probe.
      if (spec.kind === PluginKind.SubjectBackend && typeof spec.impl.health === 'function') {
        try {
          const report = await spec.impl.health({ request_id: id });
          return okResponse(id, {
            status: report.status,
            uptime_ms: report.uptime_ms ?? null,
            memory_usage_bytes: report.memory_usage_bytes ?? null,
            last_error: report.last_error ?? null,
          });
        } catch (err) {
          return okResponse(id, {
            status: 'unhealthy',
            uptime_ms: null,
            memory_usage_bytes: null,
            last_error: `health probe threw: ${String(err)}`,
          });
        }
      }
      return okResponse(id, buildHealthOk());
    }
    case 'shutdown':
      return okResponse(id, {});
    case 'exit':
      // Acknowledge then exit on next tick so the response flushes.
      setImmediate(() => process.exit(0));
      return okResponse(id, {});
    default:
      return dispatchRole(id, frame, wire, spec);
  }
}

async function dispatchRole(
  id: RpcId,
  frame: RpcRequest,
  wire: Wire,
  spec: PluginSpec,
): Promise<RpcResponse> {
  const method = frame.method;
  const ctx = { request_id: id };

  if (spec.kind === PluginKind.SubjectBackend) {
    const impl = spec.impl;
    // Methods arrive as `<kind>/<verb>` (e.g. "task/list") per
    // `crates/orchestrator-plugin-host/src/subject_router.rs`. Bare
    // `subject/<verb>` is also accepted for direct callers / smoke tests.
    const slash = method.indexOf('/');
    if (slash < 1) {
      return errorResponse(id, ErrorCode.MethodNotFound, `unknown method '${method}'`);
    }
    const prefix = method.slice(0, slash);
    const verb = method.slice(slash + 1);
    const declaredKinds = spec.subject_kinds ?? [];
    const kind = prefix === 'subject' ? declaredKinds[0] ?? 'subject' : prefix;
    // Guard: if subject_kinds is declared, reject other prefixes outright.
    // Mirror `SubjectRouter`'s glob matching: a declared `"foo.*"` accepts any
    // kind starting with `"foo."`.
    const matchesDeclared = (incoming: string): boolean => {
      for (const decl of declaredKinds) {
        if (decl === incoming) return true;
        if (decl.endsWith('.*')) {
          const stem = decl.slice(0, -1); // keep trailing "."
          if (incoming.startsWith(stem)) return true;
        }
      }
      return false;
    };
    if (prefix !== 'subject' && declaredKinds.length > 0 && !matchesDeclared(prefix)) {
      return errorResponse(
        id,
        ErrorCode.MethodNotFound,
        `plugin does not serve subject kind '${prefix}'`,
      );
    }
    const subjectCtx = { ...ctx, kind };
    // The kind is carried by the method prefix on the wire. Authors expect to
    // see it via `ctx.kind`, but some impls (especially `create`) also need it
    // as a top-level param. Inject it when missing so authors don't see
    // `undefined` where the type contract promises a value.
    const rawParams = (frame.params ?? {}) as Record<string, unknown>;
    try {
      switch (verb) {
        case 'list': {
          // Wire shape varies by caller:
          //   - daemon control surface sends `{ filter: SubjectFilter }`
          //   - direct routed callers may send a flat SubjectFilter
          // Normalize both into a flat shape so the SDK's SubjectListParams
          // contract is honored. Multi-kind callers (e.g. the CLI sending
          // `filter.kind=[task, requirement]`) are routed by the host once
          // per kind but the original kinds list is forwarded verbatim — so
          // we ALWAYS replace `kind` with `[routed-kind]` to prevent a `task`
          // backend from being asked to honor `requirement` in its filter.
          const flat =
            rawParams.filter && typeof rawParams.filter === 'object'
              ? ({ ...(rawParams.filter as Record<string, unknown>) } as Record<string, unknown>)
              : ({ ...rawParams } as Record<string, unknown>);
          const listParams = {
            ...flat,
            kind: [kind],
          };
          const out = await impl.list(listParams as never, subjectCtx);
          const filled = {
            subjects: (out.subjects ?? []).map(ensureWireSubject),
            ...(out.next_cursor !== undefined ? { next_cursor: out.next_cursor } : {}),
            fetched_at: out.fetched_at ?? new Date().toISOString(),
          };
          return okResponse(id, filled);
        }
        case 'get': {
          const out = await impl.get(rawParams as never, subjectCtx);
          if (out === null || out === undefined) {
            // The daemon's subject_get path decodes the result directly as a
            // WireSubject and treats a missing payload as a decode error.
            // Translate the SDK's permitted "null = not found" return into
            // the canonical not-found RpcError shape from
            // `animus_subject_protocol::BackendError::NotFound`.
            const subjectId = (rawParams as { id?: unknown }).id;
            return errorResponse(
              id,
              ErrorCode.InvalidParams,
              `not found: subject '${typeof subjectId === 'string' ? subjectId : '?'}'`,
              { category: 'not_found' },
            );
          }
          return okResponse(id, ensureWireSubject(out));
        }
        case 'create': {
          if (!impl.create) return notImplemented(id, method, spec.kind);
          // Backfill required `SubjectCreateRequest.kind` from the routed kind
          // so direct callers that omit it still hit the impl with a valid
          // payload (the daemon control surface fills it, but routed clients
          // may not). Map the CLI's `body` field onto the SDK's documented
          // `description` so authors only have to read one name regardless of
          // caller.
          // Spread first, then force `kind` to the routed value — a caller
          // who sends `task/create` with `params.kind = 'requirement'` is
          // bug, not a feature. Match the list dispatcher's authoritative-
          // route-kind behavior.
          const createParams: Record<string, unknown> = { ...rawParams, kind };
          if (createParams.body !== undefined && createParams.description === undefined) {
            createParams.description = createParams.body;
            delete createParams.body;
          }
          return okResponse(id, ensureWireSubject(await impl.create(createParams as never, subjectCtx)));
        }
        case 'update':
          if (!impl.update) return notImplemented(id, method, spec.kind);
          return okResponse(id, ensureWireSubject(await impl.update(rawParams as never, subjectCtx)));
        case 'status':
          if (!impl.status) return notImplemented(id, method, spec.kind);
          return okResponse(id, ensureWireSubject(await impl.status(rawParams as never, subjectCtx)));
        case 'next': {
          if (!impl.next) return notImplemented(id, method, spec.kind);
          const out = await impl.next(rawParams as never, subjectCtx);
          return okResponse(id, out ? ensureWireSubject(out) : null);
        }
        default:
          return errorResponse(id, ErrorCode.MethodNotFound, `unknown method '${method}'`);
      }
    } catch (err) {
      return errorResponse(id, ErrorCode.InternalError, `subject backend error: ${String(err)}`);
    }
  }

  // Non-subject roles: skeleton only — every domain method responds with a
  // structured `MethodNotFound`. We still keep `wire` in scope so future
  // streaming-capable roles (provider/trigger) can capture it via closure.
  void wire;
  return notImplemented(id, method, spec.kind);
}
