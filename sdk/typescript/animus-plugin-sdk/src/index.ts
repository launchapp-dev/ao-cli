// Public API surface for `@launchapp-dev/animus-plugin-sdk`.
//
// We re-export only the entrypoints plugin authors should reach for. The
// internal wire protocol helpers are also exported for advanced/test use, but
// are documented as low-level.

// --- top-level author entrypoint ---
export { definePlugin } from './plugin.js';
export type { PluginSpec, PluginHandle } from './plugin.js';

// --- role contracts ---
export type {
  CallContext,
  HealthReport,
  LogStorageBackend,
  Provider,
  ProviderRunParams,
  ProviderRunResult,
  Subject,
  SubjectBackend,
  SubjectCallContext,
  SubjectCreateRequest,
  SubjectListParams,
  SubjectListResult,
  SubjectPatch,
  SubjectStatus,
  TransportBackend,
  TriggerBackend,
  TriggerEvent,
} from './roles.js';

// --- handshake helpers (rarely needed directly) ---
export { buildInitializeResult, buildManifest, validateInitializeParams } from './handshake.js';
export type { PluginIdentity } from './handshake.js';

// --- low-level wire (advanced) ---
export { createWire, encodeFrame, errorResponse, okResponse, parseFrame } from './wire.js';
export type { FrameHandler, Wire, WireOptions } from './wire.js';

// --- protocol constants & shared types ---
export {
  ErrorCode,
  PluginKind,
  PROTOCOL_VERSION,
} from './types/index.js';
export type {
  EnvRequirement,
  HealthCheckResult,
  HealthStatus,
  HostCapabilities,
  HostInfo,
  InitializeParams,
  InitializeResult,
  McpTool,
  PluginCapabilities,
  PluginInfo,
  PluginKindString,
  PluginManifest,
  RpcError,
  RpcId,
  RpcNotification,
  RpcRequest,
  RpcResponse,
} from './types/index.js';
