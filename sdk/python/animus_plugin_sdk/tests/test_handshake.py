"""Handshake (initialize / manifest) tests."""

from __future__ import annotations

from animus_plugin_sdk import (
    PROTOCOL_VERSION,
    EnvRequirement,
    HostInfo,
    InitializeParams,
    PluginCapabilities,
    PluginIdentity,
    PluginKind,
    build_initialize_result,
    build_manifest,
    validate_initialize_params,
)


def _identity() -> PluginIdentity:
    return PluginIdentity(
        name="hello-subjects",
        version="0.1.0",
        description="example",
        plugin_kind=PluginKind.SUBJECT_BACKEND,
    )


def test_build_manifest_dedupes_capabilities() -> None:
    caps = PluginCapabilities(methods=["task/list", "task/get"])
    manifest = build_manifest(
        _identity(),
        caps,
        extra_capabilities=["task/list", "subject_kind:task"],
    )
    assert manifest.protocol_version == PROTOCOL_VERSION
    assert manifest.capabilities == [
        "task/list",
        "task/get",
        "subject_kind:task",
    ]
    assert manifest.plugin_kind == PluginKind.SUBJECT_BACKEND


def test_build_manifest_env_required() -> None:
    caps = PluginCapabilities()
    manifest = build_manifest(
        _identity(),
        caps,
        env_required=[EnvRequirement(name="MY_API_TOKEN", required=True)],
        notification_buffer_size=64,
    )
    assert manifest.env_required[0].name == "MY_API_TOKEN"
    assert manifest.notification_buffer_size == 64


def test_build_initialize_result_carries_protocol_version() -> None:
    caps = PluginCapabilities(methods=["task/list"])
    result = build_initialize_result(_identity(), caps)
    assert result.protocol_version == PROTOCOL_VERSION
    assert result.plugin_info.name == "hello-subjects"
    assert result.plugin_info.plugin_kind == PluginKind.SUBJECT_BACKEND
    assert result.capabilities.methods == ["task/list"]


def test_validate_initialize_params_accepts_matching_major() -> None:
    params = InitializeParams(
        protocol_version="1.0.0",
        host_info=HostInfo(name="animus", version="0.4.14"),
    )
    assert validate_initialize_params(params) is None


def test_validate_initialize_params_accepts_minor_drift_within_major() -> None:
    params = InitializeParams(
        protocol_version="1.42.99",
        host_info=HostInfo(name="animus", version="0.4.14"),
    )
    assert validate_initialize_params(params) is None


def test_validate_initialize_params_rejects_major_mismatch() -> None:
    params = InitializeParams(
        protocol_version="2.0.0",
        host_info=HostInfo(name="animus", version="0.4.14"),
    )
    msg = validate_initialize_params(params)
    assert msg is not None
    assert "incompatible protocol version" in msg


def test_validate_initialize_params_rejects_missing_version() -> None:
    # Build the params bypassing validation so we can pass an empty string.
    params = InitializeParams.model_construct(
        protocol_version="",
        host_info=HostInfo(name="animus", version="0.4.14"),
    )
    msg = validate_initialize_params(params)
    assert msg is not None
    assert "did not advertise" in msg
