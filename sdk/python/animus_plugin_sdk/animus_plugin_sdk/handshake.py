"""Handshake helpers: `--manifest` payload and `initialize` reply.

Mirrors the TS SDK's `handshake.ts`. Shapes track
`crates/animus-plugin-protocol` and the JSON Schema artifacts at
`schemas/animus-plugin-protocol/_all.json`. Will be swapped for
codegen-generated types in a follow-up pass (see
`animus_plugin_sdk/types/__init__.py`).
"""

from __future__ import annotations

from dataclasses import dataclass

from .types import (
    PROTOCOL_VERSION,
    EnvRequirement,
    InitializeParams,
    InitializeResult,
    PluginCapabilities,
    PluginInfo,
    PluginManifest,
)


@dataclass
class PluginIdentity:
    """Inputs describing a plugin's static identity."""

    name: str
    version: str
    description: str
    plugin_kind: str


def build_manifest(
    identity: PluginIdentity,
    capabilities: PluginCapabilities,
    *,
    env_required: list[EnvRequirement] | None = None,
    notification_buffer_size: int | None = None,
    extra_capabilities: list[str] | None = None,
) -> PluginManifest:
    """Build a flat `PluginManifest` for emission via `--manifest`.

    The host's preflight scans `manifest.capabilities` for
    `subject_kind:<kind>` tokens. Callers should pass any such tokens via
    `extra_capabilities` so that a Python subject backend that claims
    `task` is recognized as satisfying `subject_kind:task` without needing
    to spawn.
    """
    methods = list(capabilities.methods or [])
    extras = list(extra_capabilities or [])
    seen: set[str] = set()
    merged: list[str] = []
    for c in methods + extras:
        if c not in seen:
            seen.add(c)
            merged.append(c)
    return PluginManifest(
        name=identity.name,
        version=identity.version,
        plugin_kind=identity.plugin_kind,
        description=identity.description,
        protocol_version=PROTOCOL_VERSION,
        capabilities=merged,
        env_required=list(env_required or []),
        notification_buffer_size=notification_buffer_size,
    )


def build_initialize_result(
    identity: PluginIdentity,
    capabilities: PluginCapabilities,
) -> InitializeResult:
    """Build the `initialize` reply payload."""
    return InitializeResult(
        protocol_version=PROTOCOL_VERSION,
        plugin_info=PluginInfo(
            name=identity.name,
            version=identity.version,
            plugin_kind=identity.plugin_kind,
            description=identity.description,
        ),
        capabilities=capabilities,
    )


def validate_initialize_params(params: InitializeParams) -> str | None:
    """Inspect an `initialize` payload; return `None` if compatible.

    Current rule (matches the Rust host's posture): strict major-version
    match. A `1.x` plugin accepts any `1.x` host but rejects `0.x` or
    `2.x`. Returns a human-readable reason when not compatible.
    """
    host_pv = params.protocol_version
    if not isinstance(host_pv, str) or not host_pv:
        return "host did not advertise a protocol_version"
    host_major = host_pv.split(".", 1)[0]
    plugin_major = PROTOCOL_VERSION.split(".", 1)[0]
    if host_major != plugin_major:
        return f"incompatible protocol version: host={host_pv}, plugin={PROTOCOL_VERSION}"
    return None


__all__ = [
    "PluginIdentity",
    "build_initialize_result",
    "build_manifest",
    "validate_initialize_params",
]
