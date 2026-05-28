"""End-to-end tests for `define_plugin` + the subject_backend dispatcher."""

from __future__ import annotations

import io
import json
from typing import Any

import pytest

from animus_plugin_sdk import (
    PROTOCOL_VERSION,
    PluginKind,
    Subject,
    SubjectCallContext,
    SubjectListParams,
    SubjectListResult,
    define_plugin,
)


class _ManualBackend:
    """In-memory subject backend used to exercise the dispatcher."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []

    def schema(self) -> dict[str, Any]:
        return {"kinds": ["task"]}

    def list(self, params: SubjectListParams, ctx: SubjectCallContext) -> SubjectListResult:
        self.calls.append(("list", params.model_dump(exclude_none=True)))
        return SubjectListResult(
            subjects=[
                Subject(
                    id="task:1",
                    kind=ctx.kind,
                    title="hello",
                    status="ready",
                    created_at="2026-01-01T00:00:00Z",
                    updated_at="2026-01-01T00:00:00Z",
                )
            ],
            fetched_at="2026-01-01T00:00:00Z",
        )

    def get(self, params: dict[str, Any], ctx: SubjectCallContext) -> Subject | None:
        self.calls.append(("get", dict(params)))
        if params.get("id") == "task:missing":
            return None
        return Subject(
            id=params["id"],
            kind=ctx.kind,
            title="hello",
            status="ready",
            created_at="2026-01-01T00:00:00Z",
            updated_at="2026-01-01T00:00:00Z",
        )

    def create(self, params: dict[str, Any], ctx: SubjectCallContext) -> Subject:
        self.calls.append(("create", dict(params)))
        return Subject(
            id="task:new",
            kind=ctx.kind,
            title=params["title"],
            status="ready",
            created_at="2026-01-01T00:00:00Z",
            updated_at="2026-01-01T00:00:00Z",
            description=params.get("description"),
        )


def _run_plugin_with_frames(impl: Any, frames: list[str]) -> list[dict[str, Any]]:
    """Drive `define_plugin(...).run()` with a fake stdin and capture stdout."""
    inbound = io.StringIO("".join(line + "\n" for line in frames))
    outbound = io.StringIO()
    handle = define_plugin(
        kind=PluginKind.SUBJECT_BACKEND,
        impl=impl,
        name="hello-subjects",
        version="0.1.0",
        description="test",
        subject_kinds=["task"],
        input=inbound,
        output=outbound,
        skip_cli_args=True,
    )
    handle.run()
    outbound.seek(0)
    return [json.loads(line) for line in outbound.getvalue().splitlines() if line.strip()]


def test_define_plugin_requires_subject_backend_methods() -> None:
    class Bad:
        pass

    with pytest.raises(TypeError, match="list"):
        define_plugin(
            kind=PluginKind.SUBJECT_BACKEND,
            impl=Bad(),
            name="bad",
            version="0.1.0",
            description="missing methods",
            subject_kinds=["task"],
        )


def test_define_plugin_rejects_unwired_kinds() -> None:
    class Stub:
        def run_agent(self, *_args, **_kwargs):
            raise NotImplementedError

    with pytest.raises(ValueError, match="not yet wired"):
        define_plugin(
            kind=PluginKind.PROVIDER,
            impl=Stub(),
            name="prov",
            version="0.1.0",
            description="stub",
        )


def test_define_plugin_rejects_unknown_kind() -> None:
    with pytest.raises(TypeError, match="unknown kind"):
        define_plugin(
            kind="not-a-real-kind",
            impl=object(),
            name="x",
            version="0.1.0",
            description="x",
        )


def test_manifest_advertises_methods_and_subject_kind() -> None:
    handle = define_plugin(
        kind=PluginKind.SUBJECT_BACKEND,
        impl=_ManualBackend(),
        name="hello-subjects",
        version="0.1.0",
        description="example",
        subject_kinds=["task"],
        env_required=["MY_API_TOKEN"],
    )
    manifest = handle.manifest()
    assert manifest.protocol_version == PROTOCOL_VERSION
    assert "task/list" in manifest.capabilities
    assert "task/get" in manifest.capabilities
    assert "subject_kind:task" in manifest.capabilities
    assert manifest.env_required[0].name == "MY_API_TOKEN"


def test_initialize_round_trip() -> None:
    impl = _ManualBackend()
    init_frame = {
        "jsonrpc": "2.0",
        "method": "initialize",
        "id": 1,
        "params": {
            "protocol_version": "1.0.0",
            "host_info": {"name": "animus", "version": "0.4.14"},
            "capabilities": {},
        },
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(init_frame)])
    assert len(responses) == 1
    assert responses[0]["id"] == 1
    assert responses[0]["result"]["protocol_version"] == PROTOCOL_VERSION
    assert responses[0]["result"]["plugin_info"]["name"] == "hello-subjects"


def test_initialize_rejects_major_mismatch() -> None:
    impl = _ManualBackend()
    init_frame = {
        "jsonrpc": "2.0",
        "method": "initialize",
        "id": 1,
        "params": {
            "protocol_version": "2.0.0",
            "host_info": {"name": "animus", "version": "0.4.14"},
            "capabilities": {},
        },
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(init_frame)])
    assert len(responses) == 1
    error = responses[0]["error"]
    assert "incompatible protocol version" in error["message"]


def test_health_check_default_healthy() -> None:
    impl = _ManualBackend()
    frame = {"jsonrpc": "2.0", "method": "health/check", "id": 2}
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    assert responses[0]["result"]["status"] == "healthy"


def test_ping_responds() -> None:
    impl = _ManualBackend()
    frame = {"jsonrpc": "2.0", "method": "$/ping", "id": 3}
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    assert responses[0]["result"] == {}


def test_subject_list_dispatch() -> None:
    impl = _ManualBackend()
    frame = {
        "jsonrpc": "2.0",
        "method": "task/list",
        "id": 10,
        "params": {"filter": {"status": ["ready"]}},
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    assert len(responses) == 1
    payload = responses[0]["result"]
    assert payload["subjects"][0]["id"] == "task:1"
    assert payload["subjects"][0]["kind"] == "task"
    assert payload["fetched_at"] == "2026-01-01T00:00:00Z"
    # The dispatcher rewrote `kind` to the routed kind ([task]).
    list_call = next(c for c in impl.calls if c[0] == "list")
    assert list_call[1]["kind"] == ["task"]
    assert list_call[1]["status"] == ["ready"]


def test_subject_get_not_found_maps_to_error() -> None:
    impl = _ManualBackend()
    frame = {
        "jsonrpc": "2.0",
        "method": "task/get",
        "id": 11,
        "params": {"id": "task:missing"},
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    err = responses[0]["error"]
    assert "not found" in err["message"]
    assert err["data"]["category"] == "not_found"


def test_subject_get_returns_subject() -> None:
    impl = _ManualBackend()
    frame = {
        "jsonrpc": "2.0",
        "method": "task/get",
        "id": 12,
        "params": {"id": "task:123"},
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    result = responses[0]["result"]
    assert result["id"] == "task:123"
    assert result["kind"] == "task"
    assert result["status"] == "ready"


def test_subject_create_normalizes_body_to_description() -> None:
    impl = _ManualBackend()
    frame = {
        "jsonrpc": "2.0",
        "method": "task/create",
        "id": 13,
        "params": {"title": "do work", "body": "details"},
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    result = responses[0]["result"]
    assert result["title"] == "do work"
    assert result["description"] == "details"
    create_call = next(c for c in impl.calls if c[0] == "create")
    # `body` was renamed and `kind` was injected.
    assert create_call[1]["kind"] == "task"
    assert "body" not in create_call[1]
    assert create_call[1]["description"] == "details"


def test_subject_method_for_unknown_kind_is_method_not_found() -> None:
    impl = _ManualBackend()
    frame = {
        "jsonrpc": "2.0",
        "method": "requirement/list",
        "id": 14,
        "params": {},
    }
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    assert responses[0]["error"]["code"] == -32601


def test_unknown_method_returns_method_not_found() -> None:
    impl = _ManualBackend()
    frame = {"jsonrpc": "2.0", "method": "task/wat", "id": 15, "params": {}}
    responses = _run_plugin_with_frames(impl, [json.dumps(frame)])
    assert responses[0]["error"]["code"] == -32601
