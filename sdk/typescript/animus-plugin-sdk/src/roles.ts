// Role-shaped TypeScript interfaces. Each role is the contract a plugin author
// implements; the SDK wires the methods to JSON-RPC handlers.
//
// MVP scope (this skeleton):
//   - SubjectBackend: fully wired in `definePlugin` (list/get/create/update/next/status).
//   - Provider / TriggerBackend / TransportBackend / LogStorageBackend: signatures
//     are defined here so authors get IntelliSense, but the dispatcher will
//     respond with `MethodNotFound` until later waves flesh out the wiring.
//
// All payload shapes use `unknown` for now; T2 will swap them for generated
// types from the JSON Schemas under `schemas/animus-subject-protocol/`.

/** Generic context passed to every role method (extensible). */
export interface CallContext {
  /** Original JSON-RPC request id (for logging / cancellation). */
  request_id: string | number | null;
  /** AbortSignal that fires when the host sends `$/cancelRequest`. */
  signal?: AbortSignal;
}

// ---- subject_backend -------------------------------------------------------

/**
 * Normalized subject status (mirrors Rust `SubjectStatus`, kebab-case wire form).
 * Backends should translate from their native status via workflow YAML
 * `status_map`.
 */
export type SubjectStatus = 'ready' | 'in-progress' | 'blocked' | 'done' | 'cancelled';

/**
 * A single subject record returned by a subject backend.
 *
 * Wire-required fields (per `crates/animus-subject-protocol::Subject`):
 *   - id, kind, title, status, created_at, updated_at
 *
 * `created_at` / `updated_at` must be ISO-8601 timestamps. The SDK will
 * auto-fill missing `status`/`created_at`/`updated_at` on list results to
 * keep the hello-world demo usable, but production backends should set them
 * explicitly. T2 will replace this with the generated wire type.
 */
export interface Subject {
  /** Stable id, e.g. `"task:TASK-123"` or `"req:REQ-9"`. */
  id: string;
  /** Subject kind (e.g. `"task"`, `"requirement"`). */
  kind: string;
  /** Human-readable title. */
  title: string;
  /** Normalized status. */
  status: SubjectStatus;
  /** ISO-8601 creation timestamp. */
  created_at: string;
  /** ISO-8601 last-update timestamp. */
  updated_at: string;
  /** Optional free-form fields. */
  description?: string;
  priority?: number;
  assignee?: string;
  labels?: string[];
  url?: string;
  /** Implementation-defined custom fields. */
  custom?: Record<string, unknown>;
}

/**
 * Params for `<kind>/list` — matches Rust `SubjectFilter` (top-level fields,
 * not nested under `filter`). All fields are optional and combined with AND
 * semantics. The SDK pre-fills `kind: [ctx.kind]` if the host sends no kind
 * filter, so authors can ignore the param entirely for single-kind backends.
 */
export interface SubjectListParams {
  status?: SubjectStatus[];
  kind?: string[];
  assignee?: string[];
  labels_any?: string[];
  labels_all?: string[];
  updated_since?: string;
  cursor?: string;
  limit?: number;
}

/**
 * Result of `<kind>/list`. Field names MUST match the Rust `SubjectList` struct
 * in `crates/animus-subject-protocol`: `subjects` (not `items`) +
 * `fetched_at` (ISO-8601 timestamp). The SDK fills `fetched_at` automatically
 * when an author returns just `{ subjects }`.
 */
export interface SubjectListResult {
  subjects: Subject[];
  next_cursor?: string | null;
  /** ISO-8601 timestamp; auto-filled by the SDK if omitted. */
  fetched_at?: string;
}

/** Context passed to every subject-backend method. `kind` is parsed from the
 *  RPC method by the SDK so authors don't have to. */
export interface SubjectCallContext extends CallContext {
  /** Subject kind extracted from the JSON-RPC method (e.g. "task"). */
  kind: string;
}

/**
 * Wire shape of `<kind>/create` params, mirroring Rust `SubjectCreateRequest`.
 * The host serializes it directly as the JSON-RPC `params`, so authors get
 * the fields as top-level keys (NOT nested under `subject`).
 */
export interface SubjectCreateRequest {
  kind: string;
  title: string;
  description?: string;
  status?: SubjectStatus;
  priority?: number;
  assignee?: string;
  labels?: string[];
  parent?: string;
  url?: string;
  custom?: Record<string, unknown>;
}

/**
 * Wire shape of `<kind>/update` `patch` payload, mirroring Rust
 * `SubjectPatch`. All fields are optional; missing fields are not modified.
 *
 * `assignee` uses a tri-state convention to disambiguate "not modified"
 * (`undefined`) from "explicitly clear" (`null`) from "set to X" (`"X"`) —
 * the same shape as Rust's `Option<Option<String>>`.
 *
 * Labels are split into add/remove sets to avoid lost-write races on the
 * labels list as a whole. Authors should NOT pass a `labels` array here.
 */
export interface SubjectPatch {
  status?: SubjectStatus;
  /** Tri-state: `undefined` = no change, `null` = clear, `string` = set. */
  assignee?: string | null;
  labels_add?: string[];
  labels_remove?: string[];
  comment?: string;
  custom?: Record<string, unknown>;
}

/** Result of an optional `health()` hook on any role impl. */
export interface HealthReport {
  status: 'healthy' | 'degraded' | 'unhealthy';
  /** Optional human-readable note (surfaces in `animus plugin health`). */
  last_error?: string | null;
  uptime_ms?: number | null;
  memory_usage_bytes?: number | null;
}

export interface SubjectBackend {
  list(params: SubjectListParams, ctx: SubjectCallContext): Promise<SubjectListResult> | SubjectListResult;
  get(params: { id: string }, ctx: SubjectCallContext): Promise<Subject | null> | Subject | null;
  create?(params: SubjectCreateRequest, ctx: SubjectCallContext): Promise<Subject> | Subject;
  update?(params: { id: string; patch: SubjectPatch }, ctx: SubjectCallContext): Promise<Subject> | Subject;
  status?(params: { id: string; status: SubjectStatus }, ctx: SubjectCallContext): Promise<Subject> | Subject;
  next?(params: Record<string, never>, ctx: SubjectCallContext): Promise<Subject | null> | Subject | null;
  /**
   * Optional health probe. When set, `health/check` returns whatever this
   * function reports instead of the default `healthy`. Use for upstream
   * service checks, required env-var validation, etc.
   */
  health?(ctx: CallContext): Promise<HealthReport> | HealthReport;
}

// ---- provider --------------------------------------------------------------
// Skeleton only; wave 0.4.x already ships first-class Rust providers. The TS
// SDK surfaces this for completeness so JS providers (e.g. a Vercel AI SDK
// wrapper) can be authored later.

export interface ProviderRunParams {
  prompt: string;
  model?: string;
  session_id?: string;
  cwd: string;
  [key: string]: unknown;
}

export interface ProviderRunResult {
  session_id: string;
  output: string;
  exit_code: number;
  duration_ms: number;
  [key: string]: unknown;
}

export interface Provider {
  run(params: ProviderRunParams, ctx: CallContext): Promise<ProviderRunResult>;
  cancel?(params: { session_id: string }, ctx: CallContext): Promise<void>;
  resume?(params: ProviderRunParams & { session_id: string }, ctx: CallContext): Promise<ProviderRunResult>;
}

// ---- trigger_backend -------------------------------------------------------

export interface TriggerEvent {
  trigger_id: string;
  event_id: string;
  payload: unknown;
}

export interface TriggerBackend {
  /** Long-running: emit events to the host via `notify("trigger/event", ...)`. */
  watch(params: { trigger_id: string; config: unknown }, ctx: CallContext): Promise<void>;
  ack?(params: { trigger_id: string; event_id: string }, ctx: CallContext): Promise<void>;
}

// ---- transport_backend -----------------------------------------------------

export interface TransportBackend {
  /** Implementation-defined; host calls `transport/start` to spawn the server. */
  start(params: { config: unknown }, ctx: CallContext): Promise<{ endpoint: string }>;
  stop?(ctx: CallContext): Promise<void>;
}

// ---- log_storage_backend ---------------------------------------------------

export interface LogStorageBackend {
  append(params: { stream: string; entries: unknown[] }, ctx: CallContext): Promise<void>;
  query(params: { stream: string; filter?: unknown; limit?: number }, ctx: CallContext): Promise<unknown[]>;
}
