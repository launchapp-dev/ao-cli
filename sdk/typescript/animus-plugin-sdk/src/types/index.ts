// TODO(T2): replace this file with auto-generated `./generated.ts` derived from
// `schemas/animus-{plugin,subject}-protocol/*.json`. These hand-typed stubs are
// the minimal subset needed for the skeleton to compile; they intentionally
// mirror the Rust source-of-truth in
// `crates/animus-plugin-protocol/src/lib.rs` so swap-in is non-breaking.

/** Protocol version this SDK was built against. Bump when the Rust constant moves. */
export const PROTOCOL_VERSION = '1.0.0' as const;

/** Plugin kind discriminator strings (mirror `PLUGIN_KIND_*` constants). */
export const PluginKind = {
  Provider: 'provider',
  SubjectBackend: 'subject_backend',
  TaskBackend: 'task_backend',
  TriggerBackend: 'trigger_backend',
  LogStorageBackend: 'log_storage_backend',
  /** Not yet a first-class Rust enum variant; treated as `custom` over the wire. */
  TransportBackend: 'transport_backend',
  Custom: 'custom',
} as const;
export type PluginKindString = (typeof PluginKind)[keyof typeof PluginKind];

/** JSON-RPC 2.0 request id; per spec a string, number, or null. */
export type RpcId = string | number | null;

// TODO(T2): replace with generated.RpcRequest
export interface RpcRequest {
  jsonrpc: '2.0';
  method: string;
  id?: RpcId;
  params?: unknown;
}

// TODO(T2): replace with generated.RpcNotification
export interface RpcNotification {
  jsonrpc: '2.0';
  method: string;
  params?: unknown;
}

// TODO(T2): replace with generated.RpcError
export interface RpcError {
  code: number;
  message: string;
  data?: unknown;
}

// TODO(T2): replace with generated.RpcResponse
export interface RpcResponse {
  jsonrpc: '2.0';
  id: RpcId;
  result?: unknown;
  error?: RpcError | null;
}

// TODO(T2): replace with generated.EnvRequirement
export interface EnvRequirement {
  name: string;
  description?: string | null;
  required?: boolean;
  sensitive?: boolean;
}

// TODO(T2): replace with generated.PluginManifest
export interface PluginManifest {
  name: string;
  version: string;
  plugin_kind: PluginKindString | string;
  description: string;
  protocol_version: string;
  capabilities?: string[];
  env_required?: EnvRequirement[];
  notification_buffer_size?: number | null;
}

// TODO(T2): replace with generated.PluginInfo
export interface PluginInfo {
  name: string;
  version: string;
  plugin_kind: PluginKindString | string;
  description?: string | null;
}

// TODO(T2): replace with generated.McpTool
export interface McpTool {
  name: string;
  description?: string | null;
  input_schema?: unknown;
}

// TODO(T2): replace with generated.PluginCapabilities
export interface PluginCapabilities {
  methods?: string[];
  streaming?: boolean;
  progress?: boolean;
  cancellation?: boolean;
  projections?: string[];
  subject_kinds?: string[];
  mcp_tools?: McpTool[];
}

// TODO(T2): replace with generated.HostCapabilities
export interface HostCapabilities {
  streaming?: boolean;
  progress?: boolean;
  cancellation?: boolean;
}

// TODO(T2): replace with generated.HostInfo
export interface HostInfo {
  name: string;
  version: string;
}

// TODO(T2): replace with generated.InitializeParams
export interface InitializeParams {
  protocol_version: string;
  host_info: HostInfo;
  capabilities: HostCapabilities;
}

// TODO(T2): replace with generated.InitializeResult
export interface InitializeResult {
  protocol_version: string;
  plugin_info: PluginInfo;
  capabilities: PluginCapabilities;
}

// TODO(T2): replace with generated.HealthCheckResult / HealthStatus
export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy';
export interface HealthCheckResult {
  status: HealthStatus;
  uptime_ms?: number | null;
  memory_usage_bytes?: number | null;
  last_error?: string | null;
}

/** JSON-RPC 2.0 standard + Animus-specific error codes. */
export const ErrorCode = {
  ParseError: -32700,
  InvalidRequest: -32600,
  MethodNotFound: -32601,
  InvalidParams: -32602,
  InternalError: -32603,
  /** Animus-specific: plugin shutting down. */
  ServerShutdown: -32099,
} as const;
