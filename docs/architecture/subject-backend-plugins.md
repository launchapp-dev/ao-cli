# Subject Backend Plugins

> **Status (2026-05-17):** The v0.1.0 contract is **shipped**. The protocol
> crates (`animus-subject-protocol`, `animus-plugin-protocol`,
> `animus-plugin-runtime`) live at
> [`launchapp-dev/animus-protocol`](https://github.com/launchapp-dev/animus-protocol)
> (5-crate workspace, `v0.1.0`, green CI). The first reference subject backend
> is [`launchapp-dev/animus-subject-linear`](https://github.com/launchapp-dev/animus-subject-linear)
> (`v0.1.0`, green CI). New backends scaffold from
> [`launchapp-dev/animus-plugin-template`](https://github.com/launchapp-dev/animus-plugin-template)
> via `animus plugin new --kind subject --name <name>`.

## Purpose

Animus dispatches `SubjectDispatch` envelopes off a queue and into
`workflow-runner` subprocesses. The internal model is documented in
[Subject Dispatch Daemon](./subject-dispatch-daemon.md). Today the only
producers of subjects are the built-in `task` and `requirement` stores in
`.animus/`. This document defines the plugin contract that lets external systems
(Linear, Jira, GitHub Issues, Notion, Asana, Zendesk, anything with an API)
act as first-class subject sources, on equal footing with the built-ins.

The motivation is straightforward: most customers will not migrate their work
out of the system of record they already use. Animus's value is the
orchestration runtime above the source of truth, not a replacement for it.

This is a v0.4.0 surface and is breaking insofar as native `animus task`
becomes "one backend among many" rather than the privileged default — but
backward-compatible for users who never declare a `subject_type:` on their
workflows.

## Where this fits

```
                  ┌──────────────────────────────────────────────────┐
                  │              Daemon Runtime                      │
                  │                                                  │
 ┌──────────────┐ │    ┌────────────────┐    ┌──────────────┐       │
 │ Ingress      │ │    │ SubjectDispatch│    │ workflow-    │       │
 │ Surfaces     │─┼───▶│ envelope queue │───▶│ runner       │       │
 └──────────────┘ │    └────────────────┘    └──────┬───────┘       │
                  │            ▲                    │               │
                  │            │              Execution facts       │
                  │            │                    │               │
                  └────────────┼────────────────────┼───────────────┘
                               │                    ▼
                  ┌────────────────────────────┐ ┌──────────────────────┐
                  │   Subject Backends         │ │     Projectors       │
                  │   (plugin processes)       │ │                      │
                  │                            │ │ - task projector     │
                  │ - animus-subject-native    │ │ - subject projector  │
                  │ - animus-subject-linear    │ │   (writes status     │
                  │ - animus-subject-jira      │ │    back to plugin)   │
                  │ - animus-subject-github    │ │ - notification       │
                  │ - animus-subject-notion    │ └──────────────────────┘
                  │ - ...                      │
                  └────────────────────────────┘
```

Subject backends sit at two integration points:

1. **Ingress.** The daemon asks a configured backend "what's ready to dispatch
   for me?" via `subject/list`. Returned subjects become `SubjectDispatch`
   envelopes.
2. **Projection.** When a workflow run completes, the subject projector calls
   `subject/update` back into the originating backend so the external system
   reflects the new status (e.g. transitioning a Linear ticket to "In Review",
   attaching the PR URL to a Jira issue, closing a GitHub Issue).

Native `animus task` is implemented as an in-process backend that satisfies
the same trait — no plugin process, but the dispatch path looks identical
from the daemon's view.

## The plugin contract

Subject backends are stdio plugins of kind `subject_backend` (already
reserved in `crates/orchestrator-plugin-protocol/src/lib.rs:12`). They speak
the existing newline-delimited JSON-RPC 2.0 protocol with the standard
`initialize` / `initialized` / `health/check` / `$/ping` lifecycle. The
subject-specific surface adds these methods:

### `subject/list`

Return the set of subjects matching a filter. The daemon calls this on every
tick to discover ready work.

```
→ {
    "filter": {
      "status": ["ready"],
      "assignee": ["me"],
      "kind": ["task", "issue"],
      "labels_any": ["backlog"],
      "updated_since": "2026-05-10T00:00:00Z",
      "cursor": null,
      "limit": 50
    }
  }

← {
    "subjects": [Subject, ...],
    "next_cursor": "opaque-string-or-null",
    "fetched_at": "2026-05-13T14:00:00Z"
  }
```

### `subject/get`

Return a single subject by id. Used by the daemon when it needs fresh state
for a specific dispatch (e.g. before retrying a stalled workflow).

```
→ { "id": "linear:ENG-123" }
← { "subject": Subject }
```

### `subject/update`

Apply a patch to a subject. Used by the subject projector when a workflow
emits state changes.

```
→ {
    "id": "linear:ENG-123",
    "patch": {
      "status": "in_progress",
      "assignee": "agent:default",
      "comment": "Workflow run wf-7b8a... started",
      "custom": { "pr_url": "https://github.com/.../pull/42" }
    }
  }
← { "subject": Subject }
```

Patches are merge semantics. Use an explicit `null` to clear a field.
Backends translate normalized fields to their native shape (see Status
mapping below).

### `subject/watch` (optional)

Server-streaming notifications when subjects change in the external system.
Implementations that wrap webhook-receiving backends emit notifications as
they arrive. Implementations that wrap polling-only APIs return
`method_not_supported` and the daemon falls back to scheduled `subject/list`
calls.

```
← (notification) {
    "method": "subject/changed",
    "params": { "id": "linear:ENG-123", "change": "status", "subject": Subject }
  }
```

### `subject/schema`

Capability declaration. Returns the backend's supported features so the
daemon can adapt behavior without runtime guessing.

```
→ {}
← {
    "kinds": ["issue", "epic"],
    "status_values": ["ready", "in_progress", "blocked", "done", "cancelled"],
    "supports_watch": true,
    "supports_create": false,
    "supports_pagination": true,
    "native_status_values": ["Backlog", "Todo", "In Progress", "In Review", "Done", "Cancelled"],
    "custom_fields": [{"key": "priority", "type": "enum", "values": ["P0","P1","P2","P3"]}]
  }
```

## The Subject schema

Normalized cross-backend representation. Defined in a new
`animus-subject-protocol` crate so plugin authors and the daemon agree on the
wire shape.

```rust
pub struct Subject {
    /// Backend-qualified identifier, e.g. "linear:ENG-123", "jira:PROJ-456",
    /// "github:owner/repo#789", "native:TASK-001". Opaque to the daemon.
    pub id: SubjectId,

    /// Subject kind. Backend-defined. Examples: "task", "issue", "epic",
    /// "ticket", "document", "lead", "contract", "incident".
    pub kind: String,

    pub title: String,
    pub description: Option<String>,

    /// Normalized status; backend maps via status_map in workflow YAML.
    pub status: SubjectStatus,

    /// Optional 0..=4 priority. 0=none, 1=low, 2=medium, 3=high, 4=critical.
    pub priority: Option<u8>,

    /// Free-form assignee identifier. Format is backend-specific.
    pub assignee: Option<String>,

    pub labels: Vec<String>,
    pub parent: Option<SubjectId>,
    pub children: Vec<SubjectId>,

    /// Permalink to the subject in its native system.
    pub url: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Backend-specific fields the daemon does not interpret but workflows
    /// can read via templating (`{{subject.custom.story_points}}`).
    pub custom: BTreeMap<String, Value>,
}

pub enum SubjectStatus {
    Ready,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}
```

`SubjectStatus` is intentionally narrow. Backend-native states map into these
five via configuration, not code.

## The Rust trait

What plugin authors implement. Lives in `animus-subject-protocol`.

```rust
#[async_trait]
pub trait SubjectBackend: Send + Sync {
    async fn list(&self, filter: SubjectFilter) -> Result<SubjectList, BackendError>;
    async fn get(&self, id: &SubjectId) -> Result<Subject, BackendError>;
    async fn update(&self, id: &SubjectId, patch: SubjectPatch) -> Result<Subject, BackendError>;
    async fn watch(&self) -> Option<EventStream>;
    fn schema(&self) -> SubjectSchema;
    async fn health(&self) -> Result<HealthCheckResult, BackendError>;
}
```

The generalized `animus-plugin-runtime` crate provides `subject_backend_main()`
that consumes an `impl SubjectBackend` and runs the stdio JSON-RPC loop,
handling `initialize`, `initialized`, `health/check`, `$/ping`, and
dispatching subject methods to the trait. Plugin authors write the trait
impl and a 5-line `main.rs`.

## Workflow YAML binding

Subjects are configured per project, referenced per workflow.

```yaml
subjects:
  linear-eng:
    plugin: animus-subject-linear
    config:
      api_token_env: LINEAR_API_TOKEN
      team: ENG
    status_map:
      ready:       ["Backlog", "Todo"]
      in_progress: ["In Progress", "In Review"]
      blocked:     ["Blocked"]
      done:        ["Done"]
      cancelled:   ["Cancelled"]

  github-issues:
    plugin: animus-subject-github
    config:
      repo: launchapp-dev/animus
      auth_env: GITHUB_TOKEN
    status_map:
      ready:       ["open:no-assignee"]
      in_progress: ["open:assigned"]
      done:        ["closed"]

workflows:
  - id: linear-impl
    subject_type: linear-eng   # references subjects.linear-eng above
    phases: [...]

  - id: oss-triage
    subject_type: github-issues
    phases: [...]

  - id: internal-roadmap
    # No subject_type: defaults to the native task backend.
    phases: [...]
```

Workflows without an explicit `subject_type:` continue to use the native
backend, preserving every existing workflow's behavior.

## Native task migration

`animus task` is reimplemented as `animus-subject-native`, an in-process backend
that satisfies the `SubjectBackend` trait. It does not run as a separate
process; the daemon links it directly. The trait impl reads and writes
`~/.animus/<repo-scope>/state/tasks.v1.json` exactly as today.

This means:

- `animus task create/list/update/status` continue to work, but under the
  hood call `subject/*` methods on the native backend.
- Workflows that reference task IDs (`TASK-001`) continue to work; the
  native backend exposes them as `Subject` with `id = "native:TASK-001"`.
- The `animus.task.*` MCP tool surface is preserved as a stable contract (per
  the [naming contract](./naming-contract.md)). Internally these tools
  delegate to the native backend.

## Authentication

Plugins read secrets via environment variables, declared per-subject in the
workflow YAML:

```yaml
subjects:
  linear-eng:
    plugin: animus-subject-linear
    config:
      api_token_env: LINEAR_API_TOKEN
```

The daemon passes only the named env var through to the spawned plugin
process. Secrets are not stored in `.animus/`. CI/cloud installations
populate the env from their own secret store.

A separate `secrets/` API for ephemeral credential delivery is out of scope
for v0.4.0.

## Pagination

`subject/list` is cursor-paginated. Plugins return `next_cursor` (opaque
string) when more data is available; the daemon re-issues `subject/list`
with `filter.cursor` set to fetch the next page. `null` cursor means
exhausted.

The daemon caps page count per tick to avoid runaway scans against slow
backends. The cap is configurable per subject in workflow YAML
(`max_pages_per_tick: 5` default).

## Caching

The daemon caches `subject/list` results per `(subject_type, filter)` for a
short TTL (default 30s) to avoid hammering external APIs on every tick.
Cache is invalidated when a `subject/changed` notification arrives via
`subject/watch`, or on `subject/update` calls originating from a workflow.

## Open questions, resolved (v0.1.0)

The questions previously left open here were closed during the v0.1.0
protocol-shape pass. The shipped decisions:

1. **Subject IDs are opaque strings with a reserved `<backend>:` prefix.**
   `linear:ENG-123`, `native:TASK-001`, etc. The `SubjectId` newtype in
   `animus-subject-protocol` wraps a `String` and never opens it up to
   structured matching. Backends with overlapping id formats stay
   disambiguated by their prefix.

2. **`parent` and `children` are metadata only in v0.1.0.** The trait
   captures them on `Subject`, but the daemon does not block child dispatch
   on parent status. Workflows that need parent-gated dispatch must encode
   that in YAML for now. Dispatch-time dependency enforcement is on the
   v0.5.x roadmap.

3. **`animus.task.*` MCP tools accept bare `TASK-001` and prefix
   internally to `native:TASK-001`.** The MCP layer normalizes the id
   before handing it to the native `SubjectBackend` impl, so existing
   v0.3.x agent prompts that pass `TASK-001` continue to work unchanged.

4. **Plugin config secrets are env-var indirection only.** No support for
   inline `api_token: "..."` in workflow YAML; secrets stay out of the
   repo. The daemon passes only the specific env var(s) named in
   `config.<key>_env` through to the spawned plugin process. Reference
   impl: `animus-subject-linear` reads `LINEAR_API_TOKEN` via this
   indirection.

5. **`subject/create` is not in v0.1.0.** Workflows operate on
   pre-existing subjects (Linear tickets that already exist, native tasks
   created via `animus task create`). Backend-mediated creation is on the
   v0.5.x roadmap; the trait will gain an optional `create()` then.

6. **Backend versioning policy: SemVer minor compatibility.** Plugins
   declare a `protocol_version` in their manifest. The daemon accepts any
   plugin whose declared version is in the same `0.1.x` minor range as
   the host's `animus-subject-protocol` dep. Pre-1.0, minor bumps may
   break the wire; the daemon refuses to spawn an incompatible plugin
   with a clear error. Post-1.0, the policy will become "same major
   version".

7. **Cross-backend joins remain out of scope.** A workflow declares a
   single `subject_type:` and operates over one backend per dispatch. A
   workflow that needs to update a GitHub PR linked from a Linear ticket
   does it via tool calls inside a phase, not by joining backends at the
   dispatch layer.

8. **Plugin distribution: "any public GitHub repo with a manifest" is the
   v0.1.0 model.** Each backend is its own repo named
   `animus-subject-<name>` (or `animus-provider-<name>` / `animus-trigger-<name>`)
   under any GitHub org. `animus plugin install <owner/repo>[@tag]`
   resolves the latest (or pinned) release and installs the architecture-matched
   binary asset. An Animus-curated marketplace is not in v0.1.0; the
   convention-over-discovery model intentionally leaves the social/curation
   layer to GitHub's own search + the `launchapp-dev/awesome-ai-coding-tools`
   list.

## Acceptance shape for v0.4.0

- `animus-subject-protocol` crate published with `SubjectBackend` trait +
  Subject schema + JSON-RPC method definitions.
- `animus-plugin-runtime` generalized to host subject backends alongside
  the existing provider backends.
- Native `animus task` migrated to satisfy the trait, no behavior
  change for existing users.
- `animus-subject-linear` shipped as the first standalone reference plugin
  in its own repo (`launchapp-dev/animus-subject-linear`).
- `animus plugin new --kind subject --name <name>` scaffolds a new
  subject backend from a template.
- Workflow YAML supports `subject_type:` and `subjects:` blocks.
- Contract test in this repo exercises the full lifecycle against
  `animus-subject-mock` (analogous to `animus-provider-mock`).

Subsequent 0.4.x patches add Jira, GitHub Issues, Notion, and Asana
backends, each its own repo.
