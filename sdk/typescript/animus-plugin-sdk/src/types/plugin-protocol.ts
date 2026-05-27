// AUTO-GENERATED FROM ../../../schemas/animus-plugin-protocol/_all.json — DO NOT EDIT BY HAND.
// Regenerate via: pnpm run codegen

/**
 * Discriminant identifying the role a plugin plays in the host.
 *
 * Wire representation is the snake_case string in the inner literal
 * union; unknown values round-trip as `string` to preserve forward-
 * compat with hosts that introduce new kinds.
 *
 * Mirrors the `PLUGIN_KIND_*` constants in
 * `crates/animus-plugin-protocol/src/lib.rs`.
 */
export type PluginKind = "provider" | "subject_backend" | "task_backend" | "trigger_backend" | "log_storage_backend" | "custom" | (string & {});
/**
 * One environment variable a plugin asks the host to forward at spawn time.
 *
 * The host treats `name` as the source of truth: only matching variables are
 * passed through the `env_clear()` boundary. `description` and `sensitive`
 * are informational hints surfaced in `animus plugin info` and the install
 * flow so operators can decide whether a plugin's secret requirements are
 * reasonable before granting it access.
 *
 * When `required` is set, the host emits a warning at spawn time if the
 * variable isn't present in the daemon's own environment. The host never
 * refuses to spawn over a missing required var — that decision belongs to
 * the plugin itself, which sees the missing variable during its own startup.
 */
export interface EnvRequirement {
  /**
   * Optional human-readable explanation of what the variable is used for.
   */
  description?: string | null;
  /**
   * Environment variable name (e.g. `"OPENAI_API_KEY"`).
   */
  name: string;
  /**
   * When `true`, the host emits a warning at spawn time if the variable is
   * not set in the daemon's environment.
   */
  required?: boolean;
  /**
   * Hint that this variable carries a secret. Informational only — does not
   * change spawn behavior. Used to drive warnings in install output and
   * `animus plugin info` listings.
   */
  sensitive?: boolean;
}

/**
 * Response to `health/check`.
 */
export interface HealthCheckResult {
  /**
   * Most recent error message, if any.
   */
  last_error?: string | null;
  /**
   * Resident-set memory usage in bytes, if cheap to determine.
   */
  memory_usage_bytes?: number | null;
  status: HealthStatus;
  /**
   * Milliseconds since the plugin process started.
   */
  uptime_ms?: number | null;
}

/**
 * Health status emitted by `health/check`.
 *
 * Hosts surface this in `animus daemon health` and may use it to gate work
 * (e.g. drain in-flight subjects from a `Degraded` plugin before restart).
 */
export type HealthStatus = "healthy" | "degraded" | "unhealthy";

/**
 * Capabilities the host advertises during the handshake.
 *
 * Plugins may use these to enable optional features. The host promises to
 * honor any capability it advertises.
 */
export interface HostCapabilities {
  /**
   * Host may issue `$/cancelRequest` notifications to cancel in-flight
   * requests.
   */
  cancellation?: boolean;
  /**
   * Host accepts `$/progress` notifications.
   */
  progress?: boolean;
  /**
   * Host accepts server-streaming notifications carrying the original
   * request id.
   */
  streaming?: boolean;
}

/**
 * Identity of the host issuing the `initialize` call.
 *
 * Plugins may log this for debugging or vary behavior based on the host
 * version (e.g. enabling features only available in newer hosts).
 */
export interface HostInfo {
  /**
   * Conventionally `"animus"` for the official Animus daemon.
   */
  name: string;
  /**
   * Semver of the host.
   */
  version: string;
}

/**
 * Parameters sent from host to plugin in the `initialize` request.
 *
 * This is the first request the host sends after the plugin process starts.
 * The plugin should validate `protocol_version` and return an
 * [`InitializeResult`] or an error if the versions are incompatible.
 */
export interface InitializeParams {
  capabilities: HostCapabilities;
  host_info: HostInfo;
  /**
   * Protocol version the host speaks. See [`PROTOCOL_VERSION`].
   */
  protocol_version: string;
}

/**
 * Plugin's response to `initialize`.
 *
 * The host inspects `protocol_version` for compatibility and stores
 * `capabilities` for the lifetime of the plugin connection so it can avoid
 * calling unsupported methods.
 */
export interface InitializeResult {
  capabilities: PluginCapabilities;
  plugin_info: PluginInfo;
  /**
   * Protocol version the plugin speaks. See [`PROTOCOL_VERSION`].
   */
  protocol_version: string;
}

/**
 * Description of an MCP tool exposed by a custom plugin.
 *
 * Hosts that bridge MCP can re-expose these tools to MCP clients without
 * the plugin author writing MCP-specific code.
 */
export interface McpTool {
  /**
   * Human-readable description.
   */
  description?: string | null;
  /**
   * JSON Schema describing the tool's input.
   */
  input_schema?: unknown;
  /**
   * MCP tool name.
   */
  name: string;
}

/**
 * Capabilities the plugin advertises during the handshake.
 *
 * `methods` is the closed set of domain methods the plugin implements; the
 * host uses it to skip calls the plugin would reject anyway. `subject_kinds`
 * and `mcp_tools` are supplemental hints for subject-backend and
 * custom-plugin kinds respectively.
 */
export interface PluginCapabilities {
  /**
   * Plugin honors `$/cancelRequest` notifications.
   */
  cancellation?: boolean;
  /**
   * MCP tools exposed by the plugin (custom plugins only).
   */
  mcp_tools?: McpTool[];
  /**
   * Concrete methods the plugin implements (e.g. `["subject/list",
   * "subject/get", "subject/update"]`).
   */
  methods?: string[];
  /**
   * Plugin honors `$/progress` notifications.
   */
  progress?: boolean;
  /**
   * Optional projection names the plugin can serve (subject backends
   * only). Hosts may request a projection by name in calls that opt into
   * projected views.
   */
  projections?: string[];
  /**
   * Plugin emits server-streaming notifications.
   */
  streaming?: boolean;
  /**
   * Subject kinds the plugin can produce (subject backends only).
   */
  subject_kinds?: string[];
}

/**
 * Identity of the plugin returned in the `initialize` response.
 */
export interface PluginInfo {
  /**
   * Human-readable description.
   */
  description?: string | null;
  /**
   * Plugin's published name (e.g. `"animus-subject-linear"`).
   */
  name: string;
  /**
   * One of the `PLUGIN_KIND_*` constants. Prefer
   * [`PluginInfo::plugin_kind`] to read this as a typed [`PluginKind`].
   */
  plugin_kind: PluginKind;
  /**
   * Plugin's semver.
   */
  version: string;
}

/**
 * One-shot manifest emitted when a plugin is invoked with `--manifest`.
 *
 * This is the discovery surface used by `animus plugin install` and similar
 * tooling that needs to know what a binary is before spawning it as a
 * long-running stdio child. The shape mirrors [`InitializeResult`] but is
 * flat for ease of static parsing.
 */
export interface PluginManifest {
  /**
   * Methods implemented by the plugin.
   */
  capabilities?: string[];
  /**
   * Human-readable description.
   */
  description: string;
  /**
   * Environment variables the plugin needs the host to forward at spawn
   * time.
   *
   * The plugin host clears the daemon's process environment before spawning
   * a plugin (`env_clear()`) and only forwards a minimal universal shell
   * allowlist (`PATH`, `HOME`, `TMPDIR`, `LANG`, `LC_ALL`, `RUST_LOG`,
   * `RUST_BACKTRACE`, `TZ`) plus the variables declared here. Plugins that
   * need an `OPENAI_API_KEY`, `LINEAR_API_TOKEN`, etc. must list them in
   * this field; otherwise they will be missing at runtime even though the
   * daemon's environment had them set.
   *
   * Defaults to empty for back-compat: plugins built against earlier
   * versions of the protocol crate simply opt into zero secrets.
   */
  env_required?: EnvRequirement[];
  /**
   * Plugin name (matches [`PluginInfo::name`]).
   */
  name: string;
  /**
   * Author-supplied hint for the size of the host's notification broadcast
   * channel for this plugin process.
   *
   * Plugin authors who know they emit bursts of notifications (e.g. a
   * chatty streaming `agent/run` that fans out hundreds of `agent/output`
   * frames before a slow subscriber catches up) can request a larger
   * channel here. The host picks the channel capacity in priority order:
   *
   * 1. This manifest field (when set and non-zero).
   * 2. `ANIMUS_PLUGIN_BROADCAST_CAPACITY` env override (when set and
   *    parseable as a non-zero `usize`).
   * 3. The host's compiled default (currently 256).
   *
   * Capacity is fixed for a given plugin process lifetime — the underlying
   * `tokio::sync::broadcast` channel cannot be resized at runtime. To
   * change the capacity, restart the plugin process so the host can pick
   * up the new hint.
   */
  notification_buffer_size?: number | null;
  /**
   * One of the `PLUGIN_KIND_*` constants. Prefer
   * [`PluginManifest::kind`] to read this as a typed [`PluginKind`].
   */
  plugin_kind: PluginKind;
  /**
   * Protocol version the plugin was built against.
   */
  protocol_version: string;
  /**
   * Plugin semver.
   */
  version: string;
}

/**
 * JSON-RPC 2.0 error payload.
 *
 * `code` is one of the constants in [`error_codes`] or an
 * implementation-specific value in the reserved JSON-RPC range. `data` is
 * optional structured detail that the host can surface in logs or pass back
 * to the originating CLI/MCP caller.
 */
export interface RpcError {
  /**
   * Error code; see [`error_codes`].
   */
  code: number;
  /**
   * Optional structured detail.
   */
  data?: unknown;
  /**
   * Short human-readable description.
   */
  message: string;
}

/**
 * A JSON-RPC 2.0 notification frame.
 *
 * Notifications are fire-and-forget — they have no `id` and the recipient
 * never replies. Server-streaming results from a single request id (e.g.
 * `subject/changed` watch events) are also delivered as notifications; in
 * that case the original request id is carried inside `params`.
 */
export interface RpcNotification {
  /**
   * Always `"2.0"`.
   */
  jsonrpc: string;
  /**
   * JSON-RPC method name.
   */
  method: string;
  /**
   * Notification parameters.
   */
  params?: unknown;
}

/**
 * A JSON-RPC 2.0 request frame.
 *
 * `id` is `Some` for requests that expect a response. Notifications use
 * [`RpcNotification`] instead and have no `id`. `params` is structurally
 * typed via [`Value`] so the runtime can dispatch to method-specific
 * deserializers.
 */
export interface RpcRequest {
  /**
   * Request id. `None` indicates a notification (use [`RpcNotification`]
   * instead in that case; this field exists to round-trip permissively).
   */
  id?: string | number | null;
  /**
   * Always `"2.0"`.
   */
  jsonrpc: string;
  /**
   * JSON-RPC method name (e.g. `"initialize"`, `"subject/list"`).
   */
  method: string;
  /**
   * Method parameters; structurally validated by the receiving handler.
   */
  params?: unknown;
}

/**
 * A JSON-RPC 2.0 response frame.
 *
 * Exactly one of `result` or `error` should be set. Use [`RpcResponse::ok`]
 * or [`RpcResponse::err`] to construct correctly-shaped responses.
 */
export interface RpcResponse {
  /**
   * Error payload. Mutually exclusive with `result`.
   */
  error?: RpcError | null;
  /**
   * Echoes the id of the originating request. `None` only when the request
   * id could not be determined (e.g. parse error on the request frame).
   */
  id?: string | number | null;
  /**
   * Always `"2.0"`.
   */
  jsonrpc: string;
  /**
   * Successful result. Mutually exclusive with `error`.
   */
  result?: unknown;
}

/**
 * Parameters sent from host to plugin in the `trigger/ack` notification.
 *
 * The host emits this after it has accepted an event for processing. Plugins
 * use it to persist a cursor or trim a server-side queue.
 */
export interface TriggerAckParams {
  /**
   * The `event_id` being acknowledged.
   */
  event_id: string;
  /**
   * Optional status the host wants to report. See [`TriggerAckStatus`].
   */
  status?: TriggerAckStatus | null;
}

/**
 * Trigger ack status. Wire representation is a snake_case string; unknown values round-trip via Other.
 */
export type TriggerAckStatus = "dispatched" | "queued" | "unmatched" | "skipped" | "failed" | "shutdown" | (string & {});

/**
 * Trigger action hint. Wire representation is a snake_case string; unknown values round-trip via Other.
 */
export type TriggerActionHint = "create_task" | "run_workflow" | (string & {});

/**
 * A trigger event emitted by a trigger backend plugin.
 *
 * Plugins deliver these as `trigger/event` JSON-RPC notifications. The host
 * routes the event to the matching trigger configuration; what the host
 * does next depends on `action_hint` and `subject_id`:
 *
 * - `subject_id` is set → the host resolves the subject (via the configured
 *   subject backend) and may kick the subject's assigned workflow.
 * - `action_hint` is `Some(TriggerActionHint::CreateTask)` → the host creates
 *   a new task with `payload` as input context.
 * - Otherwise the host enqueues the event against the trigger's
 *   `workflow_ref` (if configured) using the existing webhook dispatch path.
 */
export interface TriggerEvent {
  /**
   * Optional hint for what the host should do. Plugins may omit this and
   * let the host fall back to the trigger config's `workflow_ref`.
   */
  action_hint?: TriggerActionHint | null;
  /**
   * Unique event id assigned by the plugin. Used by the host to send back
   * `trigger/ack`. Plugins should make this stable across restarts when
   * possible so duplicate deliveries can be deduplicated.
   */
  event_id: string;
  /**
   * Event payload. Forwarded to the spawned workflow as input.
   */
  payload?: unknown;
  /**
   * Optional subject the event refers to (e.g. a Linear issue id). When
   * set, the host may resolve the subject via its configured subject
   * backend and kick the subject's assigned workflow.
   */
  subject_id?: string | null;
  /**
   * Optional subject kind for `subject_id` (e.g. `"issue"`, `"task"`).
   */
  subject_kind?: string | null;
  /**
   * Logical trigger id this event belongs to. Matches the `id` of a
   * `WorkflowTrigger` in the project's workflow YAML.
   */
  trigger_id?: string | null;
}

/**
 * Parameters sent from host to plugin in the `trigger/watch` request.
 *
 * Trigger backend plugins receive this once during startup. After replying
 * to the request the plugin emits `trigger/event` notifications whenever it
 * observes something the host should react to. The plugin keeps watching
 * until it receives a `shutdown` request or its stdio closes.
 */
export interface TriggerWatchParams {
  /**
   * Plugin-specific configuration forwarded from project workflow YAML.
   */
  config?: unknown;
  /**
   * Optional resume cursor from a previous run; semantics are
   * plugin-defined. Plugins should ignore it if unrecognized.
   */
  cursor?: unknown;
}
