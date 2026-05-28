"""Role contracts for Animus plugins.

Each role is the contract a plugin author implements; the SDK wires the
methods to JSON-RPC handlers. Uses `typing.Protocol` so authors can
implement via duck-typing without inheriting from an ABC.

MVP scope (this skeleton):
  - SubjectBackend: fully wired in `define_plugin` (list/get/create/update/
    next/status).
  - Provider / TriggerBackend / TransportBackend / LogStorageBackend:
    Protocol signatures defined here so authors get IntelliSense, but the
    dispatcher will respond with `MethodNotFound` until later waves flesh
    out the wiring.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol, runtime_checkable

from .types import (
    HealthStatus,
    RpcId,
    Subject,
    SubjectCreateRequest,
    SubjectListParams,
    SubjectListResult,
    SubjectPatch,
)


@dataclass
class CallContext:
    """Generic context passed to every role method (extensible)."""

    request_id: RpcId = None
    """Original JSON-RPC request id (for logging / correlation)."""


@dataclass
class SubjectCallContext(CallContext):
    """Context passed to every subject-backend method.

    `kind` is parsed from the RPC method by the SDK so authors don't have
    to (e.g. method `"task/list"` → `ctx.kind == "task"`).
    """

    kind: str = ""


@dataclass
class HealthReport:
    """Result of an optional `health()` hook on any role impl."""

    status: HealthStatus = "healthy"
    last_error: str | None = None
    uptime_ms: int | None = None
    memory_usage_bytes: int | None = None


# ---- subject_backend -------------------------------------------------------


@runtime_checkable
class SubjectBackend(Protocol):
    """A subject backend serves one or more subject kinds.

    Required: `list`, `get`, `schema`.
    Optional: `create`, `update`, `status`, `next`, `health`.

    The SDK calls `schema()` once at startup to surface the backend's
    declared subject kinds; the daemon's subject_router uses the kind list
    to route inbound `<kind>/<verb>` RPCs.
    """

    def list(
        self,
        params: SubjectListParams,
        ctx: SubjectCallContext,
    ) -> SubjectListResult: ...

    def get(
        self,
        params: dict[str, Any],
        ctx: SubjectCallContext,
    ) -> Subject | None: ...

    def schema(self) -> dict[str, Any]:
        """Return a description of supported subject kinds + fields."""
        ...

    # Optional verbs — define when supported. The dispatcher uses
    # `hasattr(impl, '<verb>')` checks before routing.

    # def create(self, params: SubjectCreateRequest, ctx: SubjectCallContext) -> Subject: ...
    # def update(self, params: dict[str, Any], ctx: SubjectCallContext) -> Subject: ...
    # def status(self, params: dict[str, Any], ctx: SubjectCallContext) -> Subject: ...
    # def next(self, params: dict[str, Any], ctx: SubjectCallContext) -> Subject | None: ...
    # def health(self, ctx: CallContext) -> HealthReport: ...


# ---- provider --------------------------------------------------------------


@dataclass
class ProviderRunParams:
    prompt: str
    cwd: str
    model: str | None = None
    session_id: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass
class ProviderRunResult:
    session_id: str
    output: str
    exit_code: int
    duration_ms: int
    extra: dict[str, Any] = field(default_factory=dict)


@runtime_checkable
class Provider(Protocol):
    """Skeleton only — dispatcher returns `MethodNotFound` for now."""

    def run_agent(
        self,
        params: ProviderRunParams,
        ctx: CallContext,
    ) -> ProviderRunResult: ...

    # Optional:
    # def resume_agent(self, params: ProviderRunParams, ctx: CallContext) -> ProviderRunResult: ...
    # def cancel(self, params: dict[str, Any], ctx: CallContext) -> None: ...


# ---- trigger_backend -------------------------------------------------------


@dataclass
class TriggerEvent:
    trigger_id: str
    event_id: str
    payload: Any


@runtime_checkable
class TriggerBackend(Protocol):
    """Skeleton only — dispatcher returns `MethodNotFound` for now."""

    def watch(self, params: dict[str, Any], ctx: CallContext) -> None: ...

    # Optional:
    # def ack(self, params: dict[str, Any], ctx: CallContext) -> None: ...


# ---- transport_backend -----------------------------------------------------


@runtime_checkable
class TransportBackend(Protocol):
    """Skeleton only — dispatcher returns `MethodNotFound` for now."""

    def start(self, params: dict[str, Any], ctx: CallContext) -> dict[str, Any]: ...

    def info(self, ctx: CallContext) -> dict[str, Any]: ...

    def shutdown(self, ctx: CallContext) -> None: ...


# ---- log_storage_backend ---------------------------------------------------


@runtime_checkable
class LogStorageBackend(Protocol):
    """Skeleton only — dispatcher returns `MethodNotFound` for now."""

    def store(self, params: dict[str, Any], ctx: CallContext) -> None: ...

    def query(self, params: dict[str, Any], ctx: CallContext) -> list[Any]: ...


__all__ = [
    "CallContext",
    "HealthReport",
    "LogStorageBackend",
    "Provider",
    "ProviderRunParams",
    "ProviderRunResult",
    "Subject",
    "SubjectBackend",
    "SubjectCallContext",
    "SubjectCreateRequest",
    "SubjectListParams",
    "SubjectListResult",
    "SubjectPatch",
    "TransportBackend",
    "TriggerBackend",
    "TriggerEvent",
]
