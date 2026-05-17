//! `SubjectBackend` trait and normalized `Subject` schema.
//!
//! Animus dispatches `SubjectDispatch` envelopes off a queue and into
//! `workflow-runner` subprocesses. The set of subjects available for dispatch
//! comes from one or more *subject backends* — pluggable sources of work
//! items. Native `animus task` is a backend; so are external systems of
//! record like Linear, Jira, GitHub Issues, Notion, Asana, Zendesk, and
//! anything else with an API.
//!
//! This crate defines:
//!
//! - The normalized cross-backend [`Subject`] schema and its supporting types
//!   ([`SubjectId`], [`SubjectStatus`], [`SubjectFilter`], [`SubjectPatch`],
//!   [`SubjectList`], [`SubjectSchema`]).
//! - The Rust-side [`SubjectBackend`] trait that plugin authors implement.
//! - The JSON-RPC method-name constants used on the wire (e.g.
//!   [`METHOD_SUBJECT_LIST`]).
//! - [`BackendError`] mapping backend failures to JSON-RPC error responses.
//! - The [`SubjectChangedEvent`] notification shape used by `subject/watch`.
//!
//! Plugin authors typically depend on this crate alongside
//! [`animus-plugin-runtime`], implement [`SubjectBackend`] for their type, and
//! call `animus_plugin_runtime::subject_backend_main(my_backend).await` from
//! `main`.

#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::pin::Pin;

use animus_plugin_protocol::{error_codes, HealthCheckResult, RpcError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// =====================================================================
// Method-name constants (the JSON-RPC wire methods)
// =====================================================================

/// `subject/list` — return ready/filtered subjects for dispatch.
pub const METHOD_SUBJECT_LIST: &str = "subject/list";

/// `subject/get` — fetch a single subject by id.
pub const METHOD_SUBJECT_GET: &str = "subject/get";

/// `subject/update` — apply a [`SubjectPatch`] to a subject.
pub const METHOD_SUBJECT_UPDATE: &str = "subject/update";

/// `subject/watch` — start a server-streaming subscription. Optional;
/// polling-only backends respond with [`error_codes::METHOD_NOT_SUPPORTED`].
pub const METHOD_SUBJECT_WATCH: &str = "subject/watch";

/// `subject/schema` — capability declaration; returns [`SubjectSchema`].
pub const METHOD_SUBJECT_SCHEMA: &str = "subject/schema";

/// `subject/changed` — notification method emitted by `subject/watch`
/// streams.
pub const NOTIFICATION_SUBJECT_CHANGED: &str = "subject/changed";

// =====================================================================
// Subject identity
// =====================================================================

/// Backend-qualified identifier for a subject.
///
/// Convention is `"<backend>:<native_id>"`, e.g. `"linear:ENG-123"`,
/// `"jira:PROJ-456"`, `"github:owner/repo#789"`, `"native:TASK-001"`. The
/// daemon treats the value as opaque; only the originating backend
/// interprets the native portion.
///
/// The `backend:` prefix is reserved. Plugin authors should always emit
/// prefixed ids so cross-backend collisions are impossible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectId(pub String);

impl SubjectId {
    /// Construct a new id from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for SubjectId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SubjectId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// =====================================================================
// Normalized status
// =====================================================================

/// Normalized cross-backend subject status.
///
/// Backend-native states (`"Backlog"`, `"In Review"`, `"Won't Fix"`, ...) map
/// into one of these five via the `status_map` declared per-subject in
/// workflow YAML. The mapping lives in configuration, not code, so adding a
/// new backend never requires extending this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubjectStatus {
    /// Eligible for dispatch.
    Ready,
    /// Currently being worked by a workflow run (or by a human, in the
    /// upstream system).
    InProgress,
    /// Cannot proceed; awaiting unblock.
    Blocked,
    /// Successfully completed.
    Done,
    /// Abandoned without completion.
    Cancelled,
}

// =====================================================================
// The Subject schema
// =====================================================================

/// A normalized cross-backend representation of a unit of dispatchable work.
///
/// Subjects flow from backends into the daemon's dispatch queue and back as
/// updates after a workflow run completes. Backend-specific fields the
/// daemon doesn't interpret live in [`Subject::custom`] and are addressable
/// from workflow YAML via templating (e.g.
/// `{{subject.custom.story_points}}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    /// Backend-qualified identifier. See [`SubjectId`].
    pub id: SubjectId,

    /// Subject kind. Backend-defined. Examples: `"task"`, `"issue"`,
    /// `"epic"`, `"ticket"`, `"document"`, `"lead"`, `"contract"`,
    /// `"incident"`.
    pub kind: String,

    /// Short human-readable title.
    pub title: String,

    /// Long-form description (markdown encouraged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Normalized status. See [`SubjectStatus`].
    pub status: SubjectStatus,

    /// Optional priority on a 0..=4 scale: 0 = none, 1 = low, 2 = medium,
    /// 3 = high, 4 = critical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,

    /// Free-form assignee identifier. Format is backend-specific; commonly
    /// a username, email, or `"agent:<name>"` for an Animus agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Labels/tags. Backend-defined string set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    /// Parent subject, if any (e.g. an epic for an issue).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<SubjectId>,

    /// Child subjects, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SubjectId>,

    /// Permalink to the subject in its native system, if one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// When the subject was first created in its native system.
    pub created_at: DateTime<Utc>,

    /// When the subject was last updated in its native system.
    pub updated_at: DateTime<Utc>,

    /// Backend-specific fields the daemon does not interpret. Workflows
    /// can read these via templating.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, Value>,
}

// =====================================================================
// Filtering and listing
// =====================================================================

/// Filter passed to `subject/list`.
///
/// All fields are optional and combined with AND semantics. Empty `Vec`
/// fields mean "no constraint on this dimension". `cursor` is opaque to the
/// daemon — backends issue and accept their own pagination tokens.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubjectFilter {
    /// Match subjects whose status is one of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status: Vec<SubjectStatus>,

    /// Match subjects whose `kind` is one of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind: Vec<String>,

    /// Match subjects assigned to one of these identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignee: Vec<String>,

    /// Match subjects that have at least one of these labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels_any: Vec<String>,

    /// Match subjects that have all of these labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels_all: Vec<String>,

    /// Match subjects updated at or after this timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_since: Option<DateTime<Utc>>,

    /// Pagination cursor returned by a prior `subject/list` call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// Suggested page size. Backends are free to clamp this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Result of `subject/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectList {
    /// Subjects in this page.
    pub subjects: Vec<Subject>,

    /// Opaque cursor for the next page, or `None` if exhausted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,

    /// When the backend snapshot was taken. Used by the daemon for cache
    /// freshness reasoning.
    pub fetched_at: DateTime<Utc>,
}

// =====================================================================
// Patches
// =====================================================================

/// A patch applied to a subject via `subject/update`.
///
/// All fields are optional. Missing fields are not modified. The
/// double-`Option` on [`SubjectPatch::assignee`] distinguishes "not modified"
/// (`None`) from "explicitly clear" (`Some(None)`). Labels are partitioned
/// into add/remove sets to avoid lost-write races on the labels list as a
/// whole.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubjectPatch {
    /// Set the normalized status. Backends translate to their native value
    /// using the workflow-YAML `status_map`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SubjectStatus>,

    /// Set, change, or clear the assignee. `Some(None)` means clear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Option<String>>,

    /// Labels to add (deduplicated against existing labels).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels_add: Vec<String>,

    /// Labels to remove.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels_remove: Vec<String>,

    /// Optional comment to post alongside the update. Backends that don't
    /// support comments may surface this as a summary in their native
    /// activity log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Backend-specific custom fields to merge. An explicit JSON `null`
    /// value clears the field at that key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, Value>,
}

// =====================================================================
// Schema / capability declaration
// =====================================================================

/// Capability declaration returned by `subject/schema`.
///
/// The daemon uses this to adapt behavior without runtime guessing — for
/// example, to skip `subject/watch` for polling-only backends, or to
/// pre-populate a UI with the subject's available custom-field values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectSchema {
    /// Subject kinds this backend produces.
    pub kinds: Vec<String>,

    /// Normalized status values this backend can emit.
    pub status_values: Vec<SubjectStatus>,

    /// Whether `subject/watch` is implemented.
    pub supports_watch: bool,

    /// Whether the backend can create new subjects (reserved for v0.4.x).
    pub supports_create: bool,

    /// Whether `subject/list` honors `cursor`.
    pub supports_pagination: bool,

    /// Native (pre-mapping) status values the backend uses upstream. Useful
    /// for documenting `status_map` entries in workflow YAML.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_status_values: Vec<String>,

    /// Custom field declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomFieldSpec>,
}

/// Description of one custom field a backend exposes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomFieldSpec {
    /// Field key as it appears in [`Subject::custom`].
    pub key: String,
    /// Field type.
    #[serde(rename = "type")]
    pub kind: CustomFieldKind,
    /// For [`CustomFieldKind::Enum`] fields, the enumerated values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

/// The type of a custom field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomFieldKind {
    /// Free-form string.
    String,
    /// Numeric (integer or float).
    Number,
    /// Boolean.
    Bool,
    /// Enumerated string; allowed values in [`CustomFieldSpec::values`].
    Enum,
    /// Date/time (ISO 8601).
    Date,
}

// =====================================================================
// Watch streams
// =====================================================================

/// Stream of subject change events delivered by `subject/watch`.
///
/// Each item is sent on the wire as a [`NOTIFICATION_SUBJECT_CHANGED`]
/// notification carrying the original watch-request id in `params.id`.
pub type EventStream = Pin<Box<dyn Stream<Item = SubjectChangedEvent> + Send>>;

/// Notification payload for `subject/changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectChangedEvent {
    /// Affected subject id.
    pub id: SubjectId,
    /// What kind of change occurred.
    pub change_kind: ChangeKind,
    /// The subject's new state.
    pub subject: Subject,
}

/// Categorization of a subject change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// Subject was newly created upstream.
    Created,
    /// Subject was updated (any field).
    Updated,
    /// Subject's normalized status transitioned.
    StatusChanged,
    /// Subject was deleted upstream.
    Deleted,
}

// =====================================================================
// Errors
// =====================================================================

/// Errors a backend may return.
///
/// These map to JSON-RPC error responses via the [`From`] impl below.
/// Backend authors typically produce these directly from their trait
/// implementation; the runtime translates to wire-level [`RpcError`].
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// Subject does not exist.
    #[error("subject not found: {0}")]
    NotFound(String),

    /// Caller lacks permission for the requested action.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Request was malformed at the domain level (distinct from
    /// JSON-RPC `invalid_params` which catches wire-shape problems).
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Backend (or its upstream) is temporarily unavailable.
    #[error("backend unavailable: {0}")]
    Unavailable(String),

    /// Anything else.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<BackendError> for RpcError {
    fn from(error: BackendError) -> Self {
        match error {
            BackendError::NotFound(message) => RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("not found: {message}"),
                data: Some(serde_json::json!({"category": "not_found"})),
            },
            BackendError::PermissionDenied(message) => RpcError {
                code: error_codes::INVALID_REQUEST,
                message: format!("permission denied: {message}"),
                data: Some(serde_json::json!({"category": "permission_denied"})),
            },
            BackendError::InvalidRequest(message) => RpcError {
                code: error_codes::INVALID_PARAMS,
                message,
                data: Some(serde_json::json!({"category": "invalid_request"})),
            },
            BackendError::Unavailable(message) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: format!("backend unavailable: {message}"),
                data: Some(serde_json::json!({"category": "unavailable"})),
            },
            BackendError::Other(error) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: error.to_string(),
                data: Some(serde_json::json!({"category": "other"})),
            },
        }
    }
}

// =====================================================================
// The trait
// =====================================================================

/// What a subject backend plugin implements.
///
/// Backends are stateless from the trait's perspective — they read and write
/// against their upstream system on each call. The runtime handles the
/// JSON-RPC envelope, lifecycle methods, and (for streaming backends) wiring
/// the [`EventStream`] returned by [`SubjectBackend::watch`] into outgoing
/// [`NOTIFICATION_SUBJECT_CHANGED`] notifications.
///
/// # Example
///
/// ```ignore
/// use animus_subject_protocol::{
///     BackendError, EventStream, Subject, SubjectBackend, SubjectFilter, SubjectId,
///     SubjectList, SubjectPatch, SubjectSchema,
/// };
/// use animus_plugin_protocol::{HealthCheckResult, HealthStatus};
/// use async_trait::async_trait;
///
/// pub struct MyBackend;
///
/// #[async_trait]
/// impl SubjectBackend for MyBackend {
///     async fn list(&self, _filter: SubjectFilter) -> Result<SubjectList, BackendError> {
///         todo!()
///     }
///     async fn get(&self, _id: &SubjectId) -> Result<Subject, BackendError> {
///         todo!()
///     }
///     async fn update(&self, _id: &SubjectId, _patch: SubjectPatch) -> Result<Subject, BackendError> {
///         todo!()
///     }
///     async fn watch(&self) -> Option<EventStream> {
///         None
///     }
///     fn schema(&self) -> SubjectSchema {
///         todo!()
///     }
///     async fn health(&self) -> Result<HealthCheckResult, BackendError> {
///         Ok(HealthCheckResult {
///             status: HealthStatus::Healthy,
///             uptime_ms: None,
///             memory_usage_bytes: None,
///             last_error: None,
///         })
///     }
/// }
/// ```
#[async_trait]
pub trait SubjectBackend: Send + Sync + 'static {
    /// Return subjects matching `filter`. Called every daemon tick to
    /// discover ready work.
    async fn list(&self, filter: SubjectFilter) -> Result<SubjectList, BackendError>;

    /// Return one subject by id, or [`BackendError::NotFound`].
    async fn get(&self, id: &SubjectId) -> Result<Subject, BackendError>;

    /// Apply a patch and return the updated subject.
    async fn update(&self, id: &SubjectId, patch: SubjectPatch) -> Result<Subject, BackendError>;

    /// Open a stream of subject change events, or `None` if this backend is
    /// polling-only. Polling-only backends should also set
    /// [`SubjectSchema::supports_watch`] to `false`.
    async fn watch(&self) -> Option<EventStream>;

    /// Capability declaration. Should be cheap to compute (preferably a
    /// constant).
    fn schema(&self) -> SubjectSchema;

    /// Backend health. The daemon polls this on a schedule.
    async fn health(&self) -> Result<HealthCheckResult, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_status_serializes_kebab_case() {
        let v = serde_json::to_value(SubjectStatus::InProgress).unwrap();
        assert_eq!(v, serde_json::json!("in-progress"));
    }

    #[test]
    fn subject_id_round_trip() {
        let id = SubjectId::new("linear:ENG-123");
        let v = serde_json::to_value(&id).unwrap();
        assert_eq!(v, serde_json::json!("linear:ENG-123"));
        let back: SubjectId = serde_json::from_value(v).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn patch_double_option_distinguishes_clear_from_unset() {
        let unset = SubjectPatch::default();
        let clear = SubjectPatch { assignee: Some(None), ..Default::default() };
        let set_to_alice = SubjectPatch { assignee: Some(Some("alice".to_string())), ..Default::default() };

        // Unset should not serialize an `assignee` key at all.
        let unset_v = serde_json::to_value(&unset).unwrap();
        assert!(unset_v.get("assignee").is_none(), "unset should omit assignee");

        // Clear serializes as JSON null.
        let clear_v = serde_json::to_value(&clear).unwrap();
        assert_eq!(clear_v.get("assignee"), Some(&Value::Null));

        // Set serializes as the string.
        let set_v = serde_json::to_value(&set_to_alice).unwrap();
        assert_eq!(set_v.get("assignee"), Some(&Value::String("alice".into())));
    }

    #[test]
    fn backend_error_maps_to_rpc_error() {
        let rpc: RpcError = BackendError::NotFound("nope".into()).into();
        assert_eq!(rpc.code, error_codes::INVALID_PARAMS);
    }
}
