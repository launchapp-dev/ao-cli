"""Hand-written subset of `animus-plugin-protocol` wire types.

TODO(codegen): replace when datamodel-codegen pipeline lands. These models
mirror the Rust source-of-truth in `crates/animus-plugin-protocol/src/lib.rs`
and the JSON Schema artifacts at
`schemas/animus-plugin-protocol/_all.json`. They are the minimum required
for the skeleton to compile, validate handshake payloads, and pass
unknown-variant strings through (the Python equivalent of Rust's
`Other(String)` pattern is `pydantic.BaseModel` + permissive string fields).
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field

PROTOCOL_VERSION: Literal["1.0.0"] = "1.0.0"
"""Protocol version this SDK was built against. Match TS SDK exactly."""


PluginKindString = str
"""Plugin kind discriminator (kept as a string so unknown kinds round-trip)."""


class PluginKind:
    """Plugin kind constants (mirror Rust `PLUGIN_KIND_*`)."""

    PROVIDER: PluginKindString = "provider"
    SUBJECT_BACKEND: PluginKindString = "subject_backend"
    TASK_BACKEND: PluginKindString = "task_backend"
    TRIGGER_BACKEND: PluginKindString = "trigger_backend"
    LOG_STORAGE_BACKEND: PluginKindString = "log_storage_backend"
    TRANSPORT_BACKEND: PluginKindString = "transport_backend"
    CUSTOM: PluginKindString = "custom"

    ALL: frozenset[str] = frozenset(
        {
            "provider",
            "subject_backend",
            "task_backend",
            "trigger_backend",
            "log_storage_backend",
            "transport_backend",
            "custom",
        }
    )


class ErrorCode:
    """JSON-RPC 2.0 standard + Animus-specific error codes."""

    PARSE_ERROR: int = -32700
    INVALID_REQUEST: int = -32600
    METHOD_NOT_FOUND: int = -32601
    INVALID_PARAMS: int = -32602
    INTERNAL_ERROR: int = -32603
    SERVER_SHUTDOWN: int = -32099


# JSON-RPC 2.0 frame types ----------------------------------------------------

RpcId = str | int | None
"""JSON-RPC 2.0 request id — per spec a string, number, or null."""


class _PermissiveModel(BaseModel):
    """Base for wire types — extra fields preserved (forward compat)."""

    model_config = ConfigDict(extra="allow", populate_by_name=True)


class RpcRequest(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    jsonrpc: Literal["2.0"] = "2.0"
    method: str
    id: RpcId = None
    params: Any | None = None


class RpcNotification(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    jsonrpc: Literal["2.0"] = "2.0"
    method: str
    params: Any | None = None


class RpcError(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    code: int
    message: str
    data: Any | None = None


class RpcResponse(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    jsonrpc: Literal["2.0"] = "2.0"
    id: RpcId = None
    result: Any | None = None
    error: RpcError | None = None


# Plugin protocol payloads -----------------------------------------------------


class EnvRequirement(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    name: str
    description: str | None = None
    required: bool = True
    sensitive: bool = False


class McpTool(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    name: str
    description: str | None = None
    input_schema: Any | None = None


class PluginCapabilities(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    methods: list[str] = Field(default_factory=list)
    streaming: bool = False
    progress: bool = False
    cancellation: bool = False
    projections: list[str] = Field(default_factory=list)
    subject_kinds: list[str] = Field(default_factory=list)
    mcp_tools: list[McpTool] = Field(default_factory=list)


class HostCapabilities(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    streaming: bool = False
    progress: bool = False
    cancellation: bool = False


class HostInfo(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    name: str
    version: str


class PluginInfo(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    name: str
    version: str
    plugin_kind: PluginKindString
    description: str | None = None


class PluginManifest(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    name: str
    version: str
    plugin_kind: PluginKindString
    description: str
    protocol_version: str
    capabilities: list[str] = Field(default_factory=list)
    env_required: list[EnvRequirement] = Field(default_factory=list)
    notification_buffer_size: int | None = None


class InitializeParams(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    protocol_version: str
    host_info: HostInfo
    capabilities: HostCapabilities = Field(default_factory=HostCapabilities)


class InitializeResult(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    protocol_version: str
    plugin_info: PluginInfo
    capabilities: PluginCapabilities = Field(default_factory=PluginCapabilities)


HealthStatus = Literal["healthy", "degraded", "unhealthy"]


class HealthCheckResult(_PermissiveModel):
    """TODO(codegen): replace when datamodel-codegen pipeline lands."""

    status: HealthStatus = "healthy"
    uptime_ms: int | None = None
    memory_usage_bytes: int | None = None
    last_error: str | None = None


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
]
