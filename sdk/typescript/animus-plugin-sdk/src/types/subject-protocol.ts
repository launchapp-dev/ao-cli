// AUTO-GENERATED FROM ../../../schemas/animus-subject-protocol/_all.json — DO NOT EDIT BY HAND.
// Regenerate via: pnpm run codegen

/**
 * Categorization of a subject change event.
 */
export type ChangeKind = "created" | "updated" | "status-changed" | "deleted";

/**
 * The type of a custom field.
 */
export type CustomFieldKind = "string" | "number" | "bool" | "enum" | "date";

/**
 * Description of one custom field a backend exposes.
 */
export interface CustomFieldSpec {
  /**
   * Field key as it appears in [`Subject::custom`].
   */
  key: string;
  type: CustomFieldKind;
  /**
   * For [`CustomFieldKind::Enum`] fields, the enumerated values.
   */
  values?: string[] | null;
}

/**
 * A normalized cross-backend representation of a unit of dispatchable work.
 *
 * Subjects flow from backends into the daemon's dispatch queue and back as
 * updates after a workflow run completes. Backend-specific fields the
 * daemon doesn't interpret live in [`Subject::custom`] and are addressable
 * from workflow YAML via templating (e.g.
 * `{{subject.custom.story_points}}`).
 */
export interface Subject {
  /**
   * Free-form assignee identifier. Format is backend-specific; commonly
   * a username, email, or `"agent:<name>"` for an Animus agent.
   */
  assignee?: string | null;
  /**
   * Child subjects, if any.
   */
  children?: SubjectId[];
  /**
   * When the subject was first created in its native system.
   */
  created_at: string;
  /**
   * Backend-specific fields the daemon does not interpret. Workflows
   * can read these via templating.
   */
  custom?: {
    [k: string]: unknown;
  };
  /**
   * Long-form description (markdown encouraged).
   */
  description?: string | null;
  id: SubjectId;
  /**
   * Subject kind. Backend-defined. Examples: `"task"`, `"issue"`,
   * `"epic"`, `"ticket"`, `"document"`, `"lead"`, `"contract"`,
   * `"incident"`.
   */
  kind: string;
  /**
   * Labels/tags. Backend-defined string set.
   */
  labels?: string[];
  /**
   * Parent subject, if any (e.g. an epic for an issue).
   */
  parent?: SubjectId | null;
  /**
   * Optional priority on a 0..=4 scale: 0 = none, 1 = low, 2 = medium,
   * 3 = high, 4 = critical.
   */
  priority?: number | null;
  status: SubjectStatus;
  /**
   * Short human-readable title.
   */
  title: string;
  /**
   * When the subject was last updated in its native system.
   */
  updated_at: string;
  /**
   * Permalink to the subject in its native system, if one exists.
   */
  url?: string | null;
}

/**
 * Notification payload for `subject/changed`.
 */
export interface SubjectChangedEvent {
  change_kind: ChangeKind;
  id: SubjectId;
  subject: Subject;
}

/**
 * Filter passed to `subject/list`.
 *
 * All fields are optional and combined with AND semantics. Empty `Vec`
 * fields mean "no constraint on this dimension". `cursor` is opaque to the
 * daemon — backends issue and accept their own pagination tokens.
 */
export interface SubjectFilter {
  /**
   * Match subjects assigned to one of these identifiers.
   */
  assignee?: string[];
  /**
   * Pagination cursor returned by a prior `subject/list` call.
   */
  cursor?: string | null;
  /**
   * Match subjects whose `kind` is one of these.
   */
  kind?: string[];
  /**
   * Match subjects that have all of these labels.
   */
  labels_all?: string[];
  /**
   * Match subjects that have at least one of these labels.
   */
  labels_any?: string[];
  /**
   * Suggested page size. Backends are free to clamp this.
   */
  limit?: number | null;
  /**
   * Match subjects whose status is one of these.
   */
  status?: SubjectStatus[];
  /**
   * Match subjects updated at or after this timestamp.
   */
  updated_since?: string | null;
}

/**
 * Backend-qualified identifier for a subject.
 *
 * Convention is `"<backend>:<native_id>"`, e.g. `"linear:ENG-123"`,
 * `"jira:PROJ-456"`, `"github:owner/repo#789"`, `"native:TASK-001"`. The
 * daemon treats the value as opaque; only the originating backend
 * interprets the native portion.
 *
 * The `backend:` prefix is reserved. Plugin authors should always emit
 * prefixed ids so cross-backend collisions are impossible.
 */
export type SubjectId = string;

/**
 * Result of `subject/list`.
 */
export interface SubjectList {
  /**
   * When the backend snapshot was taken. Used by the daemon for cache
   * freshness reasoning.
   */
  fetched_at: string;
  /**
   * Opaque cursor for the next page, or `None` if exhausted.
   */
  next_cursor?: string | null;
  /**
   * Subjects in this page.
   */
  subjects: Subject[];
}

/**
 * A patch applied to a subject via `subject/update`.
 *
 * All fields are optional. Missing fields are not modified. The
 * double-`Option` on [`SubjectPatch::assignee`] distinguishes "not modified"
 * (`None`) from "explicitly clear" (`Some(None)`). Labels are partitioned
 * into add/remove sets to avoid lost-write races on the labels list as a
 * whole.
 */
export interface SubjectPatch {
  /**
   * Set, change, or clear the assignee. `Some(None)` means clear.
   */
  assignee?: string | null;
  /**
   * Optional comment to post alongside the update. Backends that don't
   * support comments may surface this as a summary in their native
   * activity log.
   */
  comment?: string | null;
  /**
   * Backend-specific custom fields to merge. An explicit JSON `null`
   * value clears the field at that key.
   */
  custom?: {
    [k: string]: unknown;
  };
  /**
   * Labels to add (deduplicated against existing labels).
   */
  labels_add?: string[];
  /**
   * Labels to remove.
   */
  labels_remove?: string[];
  /**
   * Set the normalized status. Backends translate to their native value
   * using the workflow-YAML `status_map`.
   */
  status?: SubjectStatus | null;
}

/**
 * Capability declaration returned by `subject/schema`.
 *
 * The daemon uses this to adapt behavior without runtime guessing — for
 * example, to skip `subject/watch` for polling-only backends, or to
 * pre-populate a UI with the subject's available custom-field values.
 */
export interface SubjectSchema {
  /**
   * Custom field declarations.
   */
  custom_fields?: CustomFieldSpec[];
  /**
   * Subject kinds this backend produces.
   */
  kinds: string[];
  /**
   * Native (pre-mapping) status values the backend uses upstream. Useful
   * for documenting `status_map` entries in workflow YAML.
   */
  native_status_values?: string[];
  /**
   * Normalized status values this backend can emit.
   */
  status_values: SubjectStatus[];
  /**
   * Whether the backend can create new subjects (reserved for v0.4.x).
   */
  supports_create: boolean;
  /**
   * Whether `subject/list` honors `cursor`.
   */
  supports_pagination: boolean;
  /**
   * Whether `subject/watch` is implemented.
   */
  supports_watch: boolean;
}

/**
 * Normalized cross-backend subject status.
 *
 * Backend-native states (`"Backlog"`, `"In Review"`, `"Won't Fix"`, ...) map
 * into one of these five via the `status_map` declared per-subject in
 * workflow YAML. The mapping lives in configuration, not code, so adding a
 * new backend never requires extending this enum.
 */
export type SubjectStatus = "ready" | "in-progress" | "blocked" | "done" | "cancelled";
