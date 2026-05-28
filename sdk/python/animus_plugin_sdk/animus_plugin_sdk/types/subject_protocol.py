"""Hand-written subset of `animus-subject-protocol` wire types.

TODO(codegen): replace when datamodel-codegen pipeline lands. Mirrors the
Rust source-of-truth in `crates/animus-subject-protocol/src/lib.rs`. The
intentionally permissive shape (extra fields allowed, unknown statuses
round-trip as strings) is the Python equivalent of the Rust `Other(String)`
enum pattern used to keep plugins forward-compatible with new daemon
versions.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field

# `Literal` is used for static type narrowing; raw string is accepted at
# runtime because we set `extra="allow"` and validators are permissive.
# TODO(codegen): replace when datamodel-codegen pipeline lands.
SubjectStatus = Literal["ready", "in-progress", "blocked", "done", "cancelled"]


class _PermissiveModel(BaseModel):
    model_config = ConfigDict(extra="allow", populate_by_name=True)


class Subject(_PermissiveModel):
    """A single subject record returned by a subject backend.

    Wire-required fields (per Rust `Subject`):
      - id, kind, title, status, created_at, updated_at

    The SDK auto-fills missing `status`/`created_at`/`updated_at` for hello-
    world demos, but production backends should set them explicitly.
    TODO(codegen): replace when datamodel-codegen pipeline lands.
    """

    id: str
    kind: str
    title: str
    status: str = "ready"
    created_at: str = ""
    updated_at: str = ""
    description: str | None = None
    priority: int | None = None
    assignee: str | None = None
    labels: list[str] = Field(default_factory=list)
    url: str | None = None
    custom: dict[str, Any] = Field(default_factory=dict)


class SubjectListParams(_PermissiveModel):
    """Mirrors Rust `SubjectFilter`. TODO(codegen): replace when codegen lands."""

    status: list[str] | None = None
    kind: list[str] | None = None
    assignee: list[str] | None = None
    labels_any: list[str] | None = None
    labels_all: list[str] | None = None
    updated_since: str | None = None
    cursor: str | None = None
    limit: int | None = None


class SubjectListResult(_PermissiveModel):
    """Mirrors Rust `SubjectList`. TODO(codegen): replace when codegen lands."""

    subjects: list[Subject] = Field(default_factory=list)
    next_cursor: str | None = None
    fetched_at: str | None = None


class SubjectCreateRequest(_PermissiveModel):
    """Mirrors Rust `SubjectCreateRequest`. TODO(codegen): replace when codegen lands."""

    kind: str
    title: str
    description: str | None = None
    status: str | None = None
    priority: int | None = None
    assignee: str | None = None
    labels: list[str] = Field(default_factory=list)
    parent: str | None = None
    url: str | None = None
    custom: dict[str, Any] = Field(default_factory=dict)


class SubjectPatch(_PermissiveModel):
    """Mirrors Rust `SubjectPatch`. Tri-state `assignee`: missing = no change,
    explicit `null` = clear, string = set.

    Labels split into add/remove to avoid lost-write races.
    TODO(codegen): replace when datamodel-codegen pipeline lands.
    """

    status: str | None = None
    assignee: str | None = None
    labels_add: list[str] = Field(default_factory=list)
    labels_remove: list[str] = Field(default_factory=list)
    comment: str | None = None
    custom: dict[str, Any] = Field(default_factory=dict)


__all__ = [
    "Subject",
    "SubjectCreateRequest",
    "SubjectListParams",
    "SubjectListResult",
    "SubjectPatch",
    "SubjectStatus",
]
