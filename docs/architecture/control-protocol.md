# Control Protocol

## Status

Architecture description — captures current state and lays out the target Phase 1
cleanup. Not a breaking change spec. Existing CLI flags, MCP tool shapes, and
GraphQL schema all stay; the cleanup is internal.

Cross-reference: [plugin-host-concurrency.md](plugin-host-concurrency.md) covers
the read-side of the control surface (live log tails, event subscriptions) and
its concurrency model. The `*-plugins.md` series covers the daemon's *outbound*
protocols.

## Why

The daemon's outbound plugin protocols (subject, provider, trigger) are formal:
standalone crates, JSON-RPC over stdio, semver-versioned, manifest-probed,
signature-verified. The schema is something a third party can build against.

The *inbound* control surfaces — how a human, an agent, or another process
asks the daemon to do something — are not formal in the same way. There are
three of them today, they share a lot of code in `orchestrator-cli::services::operations`
and `orchestrator-core::services`, but each binding evolves on its own clock
and nothing names the union "the control protocol". As the project grows
(mobile clients, Slack bots, voice surfaces, IDE extensions, third-party UIs),
we want one stable contract that all of those can target.

## The current control surface

Three transports today:

1. **CLI** — `animus <verb> [args]`. The binary is `crates/orchestrator-cli`.
   Args parsed by `clap` via `crates/orchestrator-cli/src/cli_types/*_types.rs`.
   Each command calls a handler in
   `crates/orchestrator-cli/src/services/operations/ops_*.rs`, which returns a
   typed result rendered through the `animus.cli.v1` JSON envelope when
   `--json` is set (see `docs/reference/json-envelope.md`).
2. **MCP** — `animus.<group>.<verb>` JSON-RPC tools. Wired in
   `crates/orchestrator-cli/src/services/operations/ops_mcp/`. Each `*_tools.rs`
   registers tools that adapt typed `*Input` structs (with `schemars`-generated
   JSON Schema) into the same library functions the CLI calls. Documented in
   `docs/reference/mcp-tools.md`.
3. **GraphQL / HTTP / Web UI** — externalized as standalone plugins under
   [`launchapp-dev`](https://github.com/launchapp-dev):
   `animus-transport-http`, `animus-transport-graphql`, and `animus-web-ui`.
   They consume the daemon's control RPC and surface it via their own
   transports. The in-tree CLI no longer ships an axum stack.

The operations exposed across these three transports overlap heavily:
workflow management (run/list/get/pause/resume/cancel/phase-approve/decisions/
checkpoints/definitions/config-validate), task lifecycle (create/list/get/
update/status/priority/deadline/assign/pause/resume/cancel/bulk/checklist/
history/next/stats), plugin management (list/info/install/uninstall/ping/call/
search/browse/update), queue (enqueue/list/hold/release/drop/reorder/stats),
daemon control (start/stop/pause/resume/status/health/logs/events/config),
plus project / requirements / vision / git / model / runner / output / history.

CLI and MCP cover the full surface; the external GraphQL plugin covers a
subset focused on what the web UI consumes (projects, workflows, tasks, queue,
requirements, skills, daemon, triggers, vision, reviews, event stream).

## Target Phase 1 architecture

Three transport bindings over one library layer:

- **Typed Request structs** per operation (e.g. `WorkflowRunRequest`,
  `TaskCreateRequest`, `PluginInstallRequest`).
- **Typed Output structs** per operation.
- Each transport binding is a thin adapter:

  - CLI: clap args → `Request` → `ops::run_x(Request)` → `Output` → JSON envelope.
  - MCP: JSON-RPC params (`schemars`-typed Input) → `Request` →
    `ops::run_x(Request)` → `Output` → JSON-RPC `CallToolResult`.
  - GraphQL: GQL input type → `Request` → `ops::run_x(Request)` → `Output` →
    GQL output type.

The library layer lives where most of it already does:
`crates/orchestrator-cli/src/services/operations/ops_*` (the orchestration
verbs) on top of `orchestrator-core::services` (the persistence + state
primitives). The cleanup moves the GraphQL resolvers onto the same `ops_*`
functions instead of reaching into the service hub directly, so all three
transports share one entry point per operation.

Net effect:

- A new operation is one library function. The three bindings come for free
  (thin manual wrappers today, optionally macro-generated later).
- Subscriptions live at the GraphQL layer (already the case for
  `event_stream.rs`) and read from the same underlying state; the library
  shape doesn't need to change to add them.
- A new transport (gRPC, REST, WebSocket-only) can be added without touching
  the library layer.

## Existing partial-unification examples

The pattern is already in tree for plugin commands:

- `c60cb49e` (`feat(plugin): add search/browse/update commands + improve list
  output`) introduced `PluginSearchRequest` / `PluginBrowseRequest` /
  `PluginUpdateRequest` and their matching `*Output` structs in
  `ops_plugin/marketplace.rs`. Both the CLI handler
  (`marketplace::handle_plugin_search`) and the MCP tool
  (`plugin_marketplace_tools::ao_plugin_search`) build the same `Request` from
  their respective inputs and call `run_plugin_search` / `run_plugin_browse` /
  `run_plugin_update`.
- `79f645da` (`feat(plugin): add public-repo install mode`) generalized
  `PluginInstallRequest` so `animus plugin install` (CLI) and
  `animus.plugin.install` (MCP) share `run_plugin_install`. The CLI parses
  clap args into the `Request`; the MCP tool maps a `PluginInstallInput`
  (`schemars`-derived JSON Schema) into the same `Request` with non-interactive
  defaults (`yes: true`).

These are the seed examples. Phase 1 applies the pattern across the rest of
the surface and extends it to GraphQL.

## Phase 1 implementation plan

Refactor cluster-by-cluster, not sweep-everything-at-once. Rough order by
impact / churn: workflow → task → plugin (finish what's started) → daemon →
queue → requirements / output / history / runner / model / git → mcp
registration cleanup once `Request`/`Output` are stable.

Properties to preserve:

- Backward-compatible: existing CLI flags + MCP shapes stay; the `Request`
  struct is reconstructed from the same parsed input.
- GraphQL resolvers gain a `Request` build step but the GQL schema does not
  change shape.
- Tests that drive `run_*` functions directly stay green; tests through clap
  or MCP get adapted.

Once the pattern is uniform, macro-driven binding generation is worth
revisiting — e.g. derive the MCP tool + GraphQL field from a single
`#[control_op]` annotation on the library function. Out of scope for Phase 1.

## Phase 2 (v0.5.x, out of scope here)

- React UI extracted to `launchapp-dev/animus-web-ui` (shipped in v0.4.12),
  consuming the GraphQL surface as an external client.
- Ship a versioned client SDK in TypeScript and Python, generated from the
  GraphQL schema (or from the `Request`/`Output` structs if we move to a
  schema-first approach like Smithy).
- Personal access token auth for third-party clients hitting GraphQL /
  any future REST surface.

## Phase 3 (v0.6+, out of scope here)

- Third-party "control plugins" that talk to the daemon over the documented
  protocol — Slack bots, mobile apps, IDE extensions consuming the SDK.
- These are *inbound* to the daemon and complement the existing outbound
  subject / provider / trigger plugin protocols.

## How this relates to logging + log storage

`animus logs tail`, `animus output tail`, GraphQL event subscriptions, and
the daemon-side log/event store all consume the same library layer on the
read path. A separate doc on logging will detail storage and retention; this
doc covers the *control* (mutation + query) surface. The concurrency model
that backs live tails — supervisor task, broadcast channels, cancellation —
is documented in [plugin-host-concurrency.md](plugin-host-concurrency.md).

## Open questions

- **Auth model for third-party control clients.** Personal access tokens are
  the obvious starting point, but scoping (per-project? per-operation? per-
  transport?) is unsettled. Out of scope for v0.4.x; flag once Phase 2 starts.
- **GraphQL schema versioning.** Today the schema evolves with the server.
  Once external clients depend on it, we need either field-level deprecation
  discipline or a `/graphql/v1` style major-version pinning. Decide before
  the SDK ships.
- **OpenAPI / Smithy generated client.** If we ever expose a REST or gRPC
  transport, generating clients from a schema doc beats hand-writing them.
  Worth a spike before committing to a hand-rolled SDK in Phase 2.
