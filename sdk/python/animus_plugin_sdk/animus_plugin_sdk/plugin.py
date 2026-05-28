"""`define_plugin(...)` — single entrypoint for authoring an Animus plugin.

Authors describe their plugin (identity + role + impl) and the SDK:

  1. handles `--manifest` CLI shortcut
  2. runs the stdio JSON-RPC loop
  3. dispatches `initialize`, `$/ping`, `health/check`, `shutdown`, `exit`,
     and role methods
  4. forwards unknown methods as `MethodNotFound`

MVP role coverage:
  - subject_backend: dispatched (`<kind>/list`, `<kind>/get`, optional verbs)
  - provider / trigger_backend / transport_backend / log_storage_backend:
    dispatcher returns `MethodNotFound` until the relevant wave wires them.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import IO, Any

from .handshake import (
    PluginIdentity,
    build_initialize_result,
    build_manifest,
    validate_initialize_params,
)
from .roles import HealthReport, SubjectBackend, SubjectCallContext
from .types import (
    EnvRequirement,
    ErrorCode,
    HealthCheckResult,
    InitializeParams,
    PluginCapabilities,
    PluginKind,
    PluginManifest,
    RpcId,
    RpcRequest,
    RpcResponse,
    Subject,
    SubjectListParams,
)
from .wire import Wire, create_wire, error_response, ok_response


@dataclass
class PluginSpec:
    """Author-supplied plugin description."""

    kind: str
    impl: Any
    name: str
    version: str
    description: str
    subject_kinds: list[str] = field(default_factory=list)
    projections: list[str] = field(default_factory=list)
    capabilities: list[str] = field(default_factory=list)
    env_required: list[EnvRequirement] = field(default_factory=list)
    notification_buffer_size: int | None = None
    # Test hooks (mirror TS SDK `input`/`output`/`skipCliArgs`).
    input: IO[str] | None = None
    output: IO[str] | None = None
    skip_cli_args: bool = False


@dataclass
class PluginHandle:
    """Returned from `define_plugin`. Drive `.run()` to enter the JSON-RPC loop."""

    _spec: PluginSpec
    _identity: PluginIdentity
    _capabilities: PluginCapabilities
    _manifest: PluginManifest

    def manifest(self) -> PluginManifest:
        """Static manifest for this plugin (also what `--manifest` prints)."""
        return self._manifest

    def initialize(self, params: InitializeParams) -> RpcResponse:
        """Build the `initialize` reply (exposed for tests)."""
        incompat = validate_initialize_params(params)
        if incompat:
            return error_response(None, ErrorCode.INVALID_REQUEST, incompat)
        return ok_response(
            None,
            build_initialize_result(self._identity, self._capabilities).model_dump(
                exclude_none=True
            ),
        )

    def run(self) -> None:
        """Drive the JSON-RPC loop until the input stream closes."""
        _run_loop(self._spec, self._manifest, self._identity, self._capabilities)


def _derive_capabilities(spec: PluginSpec) -> PluginCapabilities:
    if spec.kind == PluginKind.SUBJECT_BACKEND:
        kinds = list(spec.subject_kinds or [])
        impl = spec.impl
        verbs: list[str] = ["list", "get"]
        if _has_method(impl, "create"):
            verbs.append("create")
        if _has_method(impl, "update"):
            verbs.append("update")
        if _has_method(impl, "status"):
            verbs.append("status")
        if _has_method(impl, "next"):
            verbs.append("next")
        methods: list[str] = []
        if not kinds:
            methods.extend(f"subject/{v}" for v in verbs)
        else:
            for k in kinds:
                methods.extend(f"{k}/{v}" for v in verbs)
        return PluginCapabilities(
            methods=methods,
            streaming=False,
            progress=False,
            cancellation=False,
            subject_kinds=kinds,
            projections=list(spec.projections or []),
        )
    # Non-subject roles are skeleton-only in 0.1.0. Same posture as the TS SDK:
    # advertise no domain methods so the preflight does not route real calls
    # through us.
    if spec.kind in (
        PluginKind.PROVIDER,
        PluginKind.TRIGGER_BACKEND,
        PluginKind.TRANSPORT_BACKEND,
        PluginKind.LOG_STORAGE_BACKEND,
    ):
        return PluginCapabilities(methods=[], streaming=False, progress=False, cancellation=False)
    return PluginCapabilities(methods=[])


def _has_method(impl: Any, name: str) -> bool:
    return callable(getattr(impl, name, None))


def _validate_spec(spec: PluginSpec) -> None:
    if not spec.name:
        raise TypeError("define_plugin: `name` is required")
    if not spec.version:
        raise TypeError("define_plugin: `version` is required")
    if not spec.description:
        raise TypeError("define_plugin: `description` is required")
    if not spec.kind:
        raise TypeError("define_plugin: `kind` is required")
    if spec.kind not in PluginKind.ALL:
        raise TypeError(f"define_plugin: unknown kind '{spec.kind}'")
    if spec.impl is None:
        raise TypeError("define_plugin: `impl` is required")
    if spec.kind == PluginKind.SUBJECT_BACKEND:
        if not _has_method(spec.impl, "list"):
            raise TypeError("subject_backend impl must implement list()")
        if not _has_method(spec.impl, "get"):
            raise TypeError("subject_backend impl must implement get()")
        return
    # Match TS SDK posture: every other kind raises at construction. We surface
    # the Protocol types so authors can stub things out, but the runtime
    # dispatcher would route to a plugin that can't answer. Fail fast.
    raise ValueError(
        f"define_plugin: kind '{spec.kind}' is not yet wired in the Python SDK (0.1.0). "
        "Only subject_backend is supported in this release. "
        'See README.md "Roles" table for the roadmap.'
    )


def define_plugin(
    kind: str,
    impl: Any,
    *,
    name: str,
    version: str,
    description: str,
    subject_kinds: list[str] | None = None,
    projections: list[str] | None = None,
    capabilities: list[str] | None = None,
    env_required: list[EnvRequirement] | list[str] | None = None,
    notification_buffer_size: int | None = None,
    input: IO[str] | None = None,
    output: IO[str] | None = None,
    skip_cli_args: bool = False,
) -> PluginHandle:
    """Author-facing entrypoint.

    Example::

        define_plugin(
            kind="subject_backend",
            impl=MySubjectBackend(),
            name="hello-subjects",
            version="0.1.0",
            description="Hard-coded sample backend",
            subject_kinds=["task"],
            env_required=["MY_API_TOKEN"],
        ).run()
    """
    # Allow plain-string env_required for ergonomic call sites.
    normalized_env: list[EnvRequirement] = []
    for entry in env_required or []:
        if isinstance(entry, str):
            normalized_env.append(EnvRequirement(name=entry))
        else:
            normalized_env.append(entry)

    spec = PluginSpec(
        kind=kind,
        impl=impl,
        name=name,
        version=version,
        description=description,
        subject_kinds=list(subject_kinds or []),
        projections=list(projections or []),
        capabilities=list(capabilities or []),
        env_required=normalized_env,
        notification_buffer_size=notification_buffer_size,
        input=input,
        output=output,
        skip_cli_args=skip_cli_args,
    )
    _validate_spec(spec)
    identity = PluginIdentity(
        name=spec.name,
        version=spec.version,
        description=spec.description,
        plugin_kind=spec.kind,
    )
    capabilities_obj = _derive_capabilities(spec)
    extra_caps: list[str] = []
    if spec.kind == PluginKind.SUBJECT_BACKEND:
        # Surface `subject_kind:<kind>` so the daemon's preflight + doctor can
        # recognize coverage from the manifest alone (without spawning).
        # TODO(codex-p2): preflight `covers_subject_kind` does exact-string
        # matching; wildcard `task.*` is emitted verbatim and won't satisfy a
        # preflight requirement for `subject_kind:task.foo`. Routing still
        # works at runtime via SubjectRouter glob matching. Tracked upstream.
        for k in spec.subject_kinds:
            extra_caps.append(f"subject_kind:{k}")
    if spec.kind == PluginKind.TRANSPORT_BACKEND:
        extra_caps.extend(spec.capabilities)
    manifest = build_manifest(
        identity,
        capabilities_obj,
        env_required=spec.env_required,
        notification_buffer_size=spec.notification_buffer_size,
        extra_capabilities=extra_caps,
    )
    return PluginHandle(
        _spec=spec,
        _identity=identity,
        _capabilities=capabilities_obj,
        _manifest=manifest,
    )


# ---- run loop --------------------------------------------------------------


def _now_iso() -> str:
    return datetime.now(UTC).isoformat()


def _ensure_wire_subject(s: Subject | dict[str, Any]) -> dict[str, Any]:
    """Fill mandatory wire fields (`status`, `created_at`, `updated_at`).

    Safety net so hello-world examples that return sparse `Subject(id=..., kind=..., title=...)`
    still produce wire payloads the Rust host can decode. The Rust side
    parses `created_at` / `updated_at` as `DateTime<Utc>` — empty strings
    fail the decode, so we treat both "missing" and "empty string" as
    "fill with now()". Production backends should set the fields explicitly.
    """
    now_iso = _now_iso()
    if isinstance(s, Subject):
        data = s.model_dump(exclude_none=False)
    else:
        data = dict(s)
    if not data.get("status"):
        data["status"] = "ready"
    if not data.get("created_at"):
        data["created_at"] = now_iso
    if not data.get("updated_at"):
        data["updated_at"] = now_iso
    return data


def _not_implemented(request_id: RpcId, method: str, kind: str) -> RpcResponse:
    return error_response(
        request_id,
        ErrorCode.METHOD_NOT_FOUND,
        f"method '{method}' not implemented in Python SDK for kind '{kind}' yet",
    )


def _build_health_ok() -> dict[str, Any]:
    return HealthCheckResult(status="healthy").model_dump(exclude_none=False)


def _run_loop(
    spec: PluginSpec,
    manifest_payload: PluginManifest,
    identity: PluginIdentity,
    capabilities: PluginCapabilities,
) -> None:
    if not spec.skip_cli_args:
        args = sys.argv[1:]
        if "--manifest" in args or "-m" in args:
            payload = manifest_payload.model_dump(exclude_none=True)
            sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
            sys.stdout.flush()
            sys.exit(0)
        if "--help" in args or "-h" in args:
            sys.stderr.write(
                f"{identity.name} {identity.version} - Animus STDIO plugin\n"
                "Usage:\n"
                f"  {identity.name} --manifest    Print plugin manifest as JSON and exit\n"
                f"  {identity.name}               Run JSON-RPC loop on stdin/stdout\n"
            )
            sys.exit(0)

    wire = create_wire(input=spec.input, output=spec.output)

    def handler(frame: RpcRequest) -> RpcResponse | None:
        return _dispatch(frame, wire, spec, identity, capabilities)

    wire.run(handler)


# ---- dispatch --------------------------------------------------------------


def _dispatch(
    frame: RpcRequest,
    wire: Wire,
    spec: PluginSpec,
    identity: PluginIdentity,
    capabilities: PluginCapabilities,
) -> RpcResponse | None:
    request_id = frame.id
    method = frame.method

    # Notifications (no `id`): never respond. The host's graceful shutdown
    # is `shutdown` (request) → `exit` (notification). Per JSON-RPC 2.0,
    # only a missing `id` makes a frame a notification; `id: null` is still
    # a request.
    if (
        request_id is None
        and "id" not in (frame.model_extra or {})
        and frame.model_fields_set.isdisjoint({"id"})
    ):
        if method == "exit":
            sys.exit(0)
        if method == "initialized" or method.startswith("$/"):
            return None
        return None

    if method == "initialize":
        params_raw = frame.params or {}
        try:
            params = InitializeParams.model_validate(params_raw)
        except Exception as exc:
            return error_response(
                request_id,
                ErrorCode.INVALID_PARAMS,
                f"invalid initialize params: {exc!s}",
            )
        incompat = validate_initialize_params(params)
        if incompat:
            return error_response(request_id, ErrorCode.INVALID_REQUEST, incompat)
        return ok_response(
            request_id,
            build_initialize_result(identity, capabilities).model_dump(exclude_none=True),
        )
    if method == "$/ping":
        return ok_response(request_id, {})
    if method == "health/check":
        if spec.kind == PluginKind.SUBJECT_BACKEND and _has_method(spec.impl, "health"):
            try:
                report = spec.impl.health(SubjectCallContext(request_id=request_id))
                if isinstance(report, HealthReport):
                    result = {
                        "status": report.status,
                        "uptime_ms": report.uptime_ms,
                        "memory_usage_bytes": report.memory_usage_bytes,
                        "last_error": report.last_error,
                    }
                else:
                    result = dict(report)
                return ok_response(request_id, result)
            except Exception as exc:
                return ok_response(
                    request_id,
                    {
                        "status": "unhealthy",
                        "uptime_ms": None,
                        "memory_usage_bytes": None,
                        "last_error": f"health probe threw: {exc!s}",
                    },
                )
        return ok_response(request_id, _build_health_ok())
    if method == "shutdown":
        return ok_response(request_id, {})
    if method == "exit":
        # Acknowledge then exit. wire flushes synchronously so it's safe to
        # exit immediately after this response is sent.
        return ok_response(request_id, {})
    return _dispatch_role(request_id, frame, wire, spec)


def _dispatch_role(
    request_id: RpcId,
    frame: RpcRequest,
    wire: Wire,
    spec: PluginSpec,
) -> RpcResponse:
    method = frame.method
    _ = wire  # reserved for future streaming roles

    if spec.kind != PluginKind.SUBJECT_BACKEND:
        # Non-subject roles return MethodNotFound for every domain method;
        # matches the TS SDK posture.
        return _not_implemented(request_id, method, spec.kind)

    impl: SubjectBackend = spec.impl

    # Methods arrive as `<kind>/<verb>` per the SubjectRouter in
    # `crates/orchestrator-plugin-host/src/subject_router.rs`. Bare
    # `subject/<verb>` is also accepted for direct callers / smoke tests.
    slash = method.find("/")
    if slash < 1:
        return error_response(request_id, ErrorCode.METHOD_NOT_FOUND, f"unknown method '{method}'")
    prefix = method[:slash]
    verb = method[slash + 1 :]
    declared_kinds = list(spec.subject_kinds or [])
    kind = (
        declared_kinds[0]
        if (prefix == "subject" and declared_kinds)
        else ("subject" if prefix == "subject" else prefix)
    )

    def matches_declared(incoming: str) -> bool:
        for decl in declared_kinds:
            if decl == incoming:
                return True
            if decl.endswith(".*"):
                stem = decl[:-1]  # keep trailing "."
                if incoming.startswith(stem):
                    return True
        return False

    if prefix != "subject" and declared_kinds and not matches_declared(prefix):
        return error_response(
            request_id,
            ErrorCode.METHOD_NOT_FOUND,
            f"plugin does not serve subject kind '{prefix}'",
        )

    ctx = SubjectCallContext(request_id=request_id, kind=kind)
    raw_params = (frame.params or {}) if isinstance(frame.params, dict) else {}

    try:
        if verb == "list":
            # Wire shape varies by caller:
            #   - daemon control surface sends `{ filter: SubjectFilter }`
            #   - direct routed callers may send a flat SubjectFilter
            flat = (
                dict(raw_params["filter"])
                if isinstance(raw_params.get("filter"), dict)
                else dict(raw_params)
            )
            # ALWAYS replace `kind` with the routed kind so a `task` backend
            # cannot be asked to honor a `requirement` filter.
            flat["kind"] = [kind]
            params = SubjectListParams.model_validate(flat)
            list_out = impl.list(params, ctx)
            if hasattr(list_out, "model_dump"):
                out_dict = list_out.model_dump(exclude_none=False)
            else:
                out_dict = dict(list_out)
            subjects = [_ensure_wire_subject(s) for s in (out_dict.get("subjects") or [])]
            response: dict[str, Any] = {
                "subjects": subjects,
                "fetched_at": out_dict.get("fetched_at") or _now_iso(),
            }
            if out_dict.get("next_cursor") is not None:
                response["next_cursor"] = out_dict["next_cursor"]
            return ok_response(request_id, response)
        if verb == "get":
            get_out: Subject | None = impl.get(raw_params, ctx)
            if get_out is None:
                subject_id = raw_params.get("id", "?")
                return error_response(
                    request_id,
                    ErrorCode.INVALID_PARAMS,
                    f"not found: subject '{subject_id}'",
                    {"category": "not_found"},
                )
            return ok_response(request_id, _ensure_wire_subject(get_out))
        if verb == "create":
            create_fn = getattr(impl, "create", None)
            if not callable(create_fn):
                return _not_implemented(request_id, method, spec.kind)
            create_params = dict(raw_params)
            create_params["kind"] = kind
            # CLI sends `body`; SDK exposes `description`. Normalize.
            if create_params.get("body") is not None and create_params.get("description") is None:
                create_params["description"] = create_params.pop("body")
            return ok_response(request_id, _ensure_wire_subject(create_fn(create_params, ctx)))
        if verb == "update":
            update_fn = getattr(impl, "update", None)
            if not callable(update_fn):
                return _not_implemented(request_id, method, spec.kind)
            return ok_response(request_id, _ensure_wire_subject(update_fn(raw_params, ctx)))
        if verb == "status":
            status_fn = getattr(impl, "status", None)
            if not callable(status_fn):
                return _not_implemented(request_id, method, spec.kind)
            return ok_response(request_id, _ensure_wire_subject(status_fn(raw_params, ctx)))
        if verb == "next":
            next_fn = getattr(impl, "next", None)
            if not callable(next_fn):
                return _not_implemented(request_id, method, spec.kind)
            next_out: Subject | None = next_fn(raw_params, ctx)
            return ok_response(
                request_id,
                _ensure_wire_subject(next_out) if next_out is not None else None,
            )
        if verb == "schema":
            schema_fn = getattr(impl, "schema", None)
            if not callable(schema_fn):
                return ok_response(request_id, {"kinds": declared_kinds})
            return ok_response(request_id, schema_fn())
        return error_response(request_id, ErrorCode.METHOD_NOT_FOUND, f"unknown method '{method}'")
    except Exception as exc:
        return error_response(
            request_id,
            ErrorCode.INTERNAL_ERROR,
            f"subject backend error: {exc!s}",
        )


__all__ = [
    "PluginHandle",
    "PluginSpec",
    "define_plugin",
]
