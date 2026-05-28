"""Wire-level round-trip tests for the NDJSON JSON-RPC transport."""

from __future__ import annotations

import io
import json

import pytest

from animus_plugin_sdk import (
    ErrorCode,
    RpcRequest,
    Wire,
    create_wire,
    encode_frame,
    error_response,
    ok_response,
    parse_frame,
)


def test_parse_frame_minimal() -> None:
    frame = parse_frame('{"jsonrpc":"2.0","method":"initialize","id":1}')
    assert frame.jsonrpc == "2.0"
    assert frame.method == "initialize"
    assert frame.id == 1


def test_parse_frame_rejects_bad_jsonrpc_version() -> None:
    with pytest.raises(ValueError, match="unsupported jsonrpc"):
        parse_frame('{"jsonrpc":"1.0","method":"x"}')


def test_parse_frame_rejects_missing_method() -> None:
    with pytest.raises(ValueError, match="method"):
        parse_frame('{"jsonrpc":"2.0"}')


def test_parse_frame_rejects_non_object() -> None:
    with pytest.raises(ValueError):
        parse_frame("[]")


def test_parse_frame_rejects_invalid_json() -> None:
    with pytest.raises(ValueError, match="invalid JSON"):
        parse_frame("not json")


def test_encode_frame_is_newline_terminated() -> None:
    response = ok_response(7, {"hello": "world"})
    encoded = encode_frame(response)
    assert encoded.endswith("\n")
    parsed = json.loads(encoded)
    assert parsed["id"] == 7
    assert parsed["result"] == {"hello": "world"}
    assert parsed["jsonrpc"] == "2.0"


def test_error_response_shape() -> None:
    response = error_response(3, ErrorCode.METHOD_NOT_FOUND, "nope")
    encoded = json.loads(encode_frame(response))
    assert encoded["error"]["code"] == ErrorCode.METHOD_NOT_FOUND
    assert encoded["error"]["message"] == "nope"
    # `result` MUST be absent on an error response (JSON-RPC 2.0).
    assert "result" not in encoded


def test_encode_frame_preserves_null_result() -> None:
    """`result: null` is a valid reply (e.g. subject/next when nothing is ready).

    The encoder MUST keep the field rather than drop it via `exclude_none`,
    otherwise strict JSON-RPC clients see no `result` and no `error` and
    fail the response shape.
    """
    response = ok_response(7, None)
    encoded = json.loads(encode_frame(response))
    assert encoded["id"] == 7
    assert "result" in encoded
    assert encoded["result"] is None
    assert "error" not in encoded


def test_encode_frame_preserves_null_id() -> None:
    """JSON-RPC 2.0 allows `id: null`; the field must be emitted, not omitted."""
    response = ok_response(None, {"ok": True})
    encoded = json.loads(encode_frame(response))
    assert "id" in encoded
    assert encoded["id"] is None


def test_wire_round_trip_request_response() -> None:
    """Inbound request → handler → outbound response."""
    inbound = io.StringIO('{"jsonrpc":"2.0","method":"$/ping","id":42}\n')
    outbound = io.StringIO()
    wire = Wire(input=inbound, output=outbound)

    def handler(frame: RpcRequest):
        assert frame.method == "$/ping"
        return ok_response(frame.id, {"pong": True})

    wire.run(handler)
    outbound.seek(0)
    out_lines = outbound.getvalue().splitlines()
    assert len(out_lines) == 1
    parsed = json.loads(out_lines[0])
    assert parsed["id"] == 42
    assert parsed["result"] == {"pong": True}


def test_wire_skips_invalid_frames() -> None:
    """Bad JSON does not kill the loop; subsequent valid frames still dispatch."""
    inbound = io.StringIO('not json\n\n{"jsonrpc":"2.0","method":"$/ping","id":1}\n')
    outbound = io.StringIO()
    logs: list[str] = []

    def logger(msg: str, err: BaseException | None = None) -> None:
        logs.append(msg)

    wire = Wire(input=inbound, output=outbound, logger=logger)

    def handler(frame: RpcRequest):
        return ok_response(frame.id, {"ok": True})

    wire.run(handler)
    assert any("invalid JSON-RPC frame" in m for m in logs)
    out = outbound.getvalue().splitlines()
    assert len(out) == 1
    assert json.loads(out[0])["id"] == 1


def test_wire_handler_exception_surfaces_internal_error() -> None:
    inbound = io.StringIO('{"jsonrpc":"2.0","method":"boom","id":9}\n')
    outbound = io.StringIO()
    logs: list[str] = []

    def logger(msg: str, err: BaseException | None = None) -> None:
        logs.append(msg)

    wire = Wire(input=inbound, output=outbound, logger=logger)

    def handler(_frame):
        raise RuntimeError("kaboom")

    wire.run(handler)
    out = outbound.getvalue().splitlines()
    assert len(out) == 1
    payload = json.loads(out[0])
    assert payload["error"]["code"] == ErrorCode.INTERNAL_ERROR
    assert "kaboom" in payload["error"]["message"]
    assert any("handler error" in m for m in logs)


def test_create_wire_defaults_to_stdio() -> None:
    wire = create_wire()
    assert wire is not None
    # Smoke: we can build a Wire without explicit args.
    assert wire.input is not None
    assert wire.output is not None
