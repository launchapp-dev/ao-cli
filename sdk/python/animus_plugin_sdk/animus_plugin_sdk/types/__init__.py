"""Wire types for `animus-plugin-sdk`.

TODO(codegen): replace hand-written subset with auto-generated bindings
derived from `schemas/animus-{plugin,subject}-protocol/_all.json`. The
Python equivalent of T2's TS codegen pipeline is `datamodel-code-generator`
(produces pydantic models from JSON Schema). Out of scope for the
skeleton — tracked as a follow-up ticket.
"""

from .plugin_protocol import (
    PROTOCOL_VERSION,
    EnvRequirement,
    ErrorCode,
    HealthCheckResult,
    HealthStatus,
    HostCapabilities,
    HostInfo,
    InitializeParams,
    InitializeResult,
    McpTool,
    PluginCapabilities,
    PluginInfo,
    PluginKind,
    PluginKindString,
    PluginManifest,
    RpcError,
    RpcId,
    RpcNotification,
    RpcRequest,
    RpcResponse,
)
from .subject_protocol import (
    Subject,
    SubjectCreateRequest,
    SubjectListParams,
    SubjectListResult,
    SubjectPatch,
    SubjectStatus,
)

__all__ = [
    "PROTOCOL_VERSION",
    "EnvRequirement",
    "ErrorCode",
    "HealthCheckResult",
    "HealthStatus",
    "HostCapabilities",
    "HostInfo",
    "InitializeParams",
    "InitializeResult",
    "McpTool",
    "PluginCapabilities",
    "PluginInfo",
    "PluginKind",
    "PluginKindString",
    "PluginManifest",
    "RpcError",
    "RpcId",
    "RpcNotification",
    "RpcRequest",
    "RpcResponse",
    "Subject",
    "SubjectCreateRequest",
    "SubjectListParams",
    "SubjectListResult",
    "SubjectPatch",
    "SubjectStatus",
]
