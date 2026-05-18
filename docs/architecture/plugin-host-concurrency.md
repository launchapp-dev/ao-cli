# Plugin-host concurrency + cancellation contract

## Status

Target contract. Design only — no Rust changes land in this commit. Implementation
follows in a separate PR per the architect's note about not bundling design with
delivery. This doc is the gate the follow-up implementation writes against.

Closes audit gaps:

- **#8** — `PluginHost::send_and_receive` is single-flight and deadlocks on
  out-of-order responses.
- **#7** — `PluginSessionBackend::dispatch_cancel` spawns a NEW plugin process,
  so cancel is a no-op against the original session.

## Why (current bugs)

### #8: single-flight + OOO deadlock

`crates/orchestrator-plugin-host/src/host.rs:268-297` (`send_and_receive`) takes
`&mut self`, writes a request, then loops on `transport.read_message` until it
sees a frame whose `id` matches the awaited request id. Three failure modes:

1. **Concurrent requests serialize.** `request()` takes `&mut self` at
   `host.rs:339`. Two callers can never overlap. Throughput is one in-flight
   RPC per plugin process.
2. **OOO responses deadlock.** JSON-RPC 2.0 allows the plugin to answer
   request 2 before request 1. The current loop sees a non-matching id, falls
   off the `if response.id.as_ref() == Some(&expected_id)` check at line 293,
   and continues looping — meaning it reads the OOO response off the wire and
   silently discards it. The awaiter for that id (if one ever existed) never
   sees its response. We get away with this today only because every caller
   serializes its requests behind `&mut self`.
3. **Notification/response interleaving is the same task.** Notifications get
   forwarded to `notification_tx` (line 281-287) on the way to looking for a
   response. That works, but it means a slow notification subscriber backpressures
   the request path.

### #7: cancel spawns a NEW process

`crates/llm-cli-wrapper/src/session/plugin_backend.rs:308-342`
(`dispatch_cancel`) spawns a fresh plugin via `PluginHost::spawn_with_options`,
handshakes, sends `agent/cancel { session_id }`, shuts down. The original plugin
process that owns the live session keeps running. The new process has no state
about that session. The cancel "succeeds" while the original agent keeps
streaming, burning tokens, possibly writing files. This is the bug that breaks
mid-flight Ctrl-C and `animus task cancel` against any long-running stream.

Both bugs share a root cause: `PluginHost` is modeled as a per-call value with
`&mut self` APIs. The fix is a shared, concurrent host with a real router.

## Lifecycle model

Plugin processes today fall into two camps:

- **Long-lived** — triggers (`trigger_supervisor.rs:223-319`), subjects
  (`subject_router.rs:27-64`), and registry probes hold a `PluginHost` open
  across many `request()` calls.
- **Short-lived per call** — providers
  (`plugin_backend.rs:dispatch`, `dispatch_cancel`) spawn per `agent/run` and
  per `agent/cancel`. This is what breaks cancel.

**Decision: long-lived for all three plugin kinds.** A discovered plugin is
spawned once at first use and the same process serves every subsequent
`agent/run` / `agent/cancel` / `agent/resume` for that backend. The host is
the only thing that knows the session is in flight, so the host is the only
thing that can route the cancel to the same process.

**Tradeoff to accept**:

- Handshake runs once at spawn — `env_required` is locked at that point. If
  the operator changes a secret, the supervisor must restart the plugin to
  pick it up. This matches the existing trigger model.
- One plugin crash takes down every in-flight session on that plugin
  simultaneously. Mitigation: the trigger supervisor's already-shipped
  exponential-backoff restart loop (8bfeeeaf) extends to providers, and
  in-flight callers see `HostError::ConnectionLost` so the upper layer can
  re-enqueue / retry.
- Memory footprint scales with one process per provider, not per session.

**Tradeoff explicitly rejected**: pooling N processes per provider. Adds
session-affinity logic for diminishing returns; defer until we have a workload
that demands it.

## Request/response routing

One `Transport` per plugin process. Stdin and stdout are split. A single reader
task owns the stdout half; the writer half lives behind a `Mutex<WriteHalf>`
(writes don't need to interleave with reads, and serialized writes keep frame
boundaries clean on a line-delimited protocol).

The reader task is the router:

- Read one JSON-RPC frame.
- If the frame has an `id` field → look it up in `HashMap<RequestId,
  oneshot::Sender<RpcResponse>>` (the pending map). Deliver the response to
  the matching awaiter. If the id is unknown, log + drop (response for a
  request we never made, or whose awaiter already gave up).
- If the frame has no `id` → it's a notification. Push to
  `broadcast::Sender<RpcNotification>` so any number of subscribers can fan
  out.
- If the frame is malformed → log at `warn` and continue. A malformed frame
  is not fatal to the channel — the protocol is line-delimited so the next
  newline resyncs us.

`PluginHost` becomes `Arc<PluginHostInner>`. All RPC methods take `&self`. The
type is `Clone` (the inner is `Arc`-wrapped). The pending map lives in the
inner under `Mutex<HashMap<u64, oneshot::Sender<RpcResponse>>>`. Request id
allocation moves to `AtomicU64::fetch_add(1, Ordering::Relaxed)`.

Public API:

```rust
impl PluginHost {
    pub async fn spawn_with_options(
        binary: &Path,
        args: &[&str],
        options: PluginSpawnOptions,
    ) -> Result<Self, HostError>;

    pub async fn handshake(&self) -> Result<InitializeResult, HostError>;

    /// Send a request, await its response. Multiple concurrent calls are
    /// independent — they share the transport but each gets its own pending
    /// entry. No serialization.
    pub async fn request(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<Value, RpcError>;

    /// Same as `request` but with a per-call deadline. Opt-in; default
    /// `request()` has no timeout (suitable for streaming agent/run calls).
    pub async fn request_with_timeout(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, RpcError>;

    /// Fire-and-forget notification (no id, no response expected).
    pub async fn notify(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<(), HostError>;

    /// Subscribe to plugin-emitted notifications. Each subscriber gets its
    /// own broadcast::Receiver. Returns the receiver; the underlying sender
    /// is owned by the reader task.
    pub fn subscribe_notifications(&self) -> broadcast::Receiver<RpcNotification>;

    /// Health probes.
    pub async fn ping(&self) -> Result<(), HostError>;
    pub async fn health_check(&self) -> Result<HealthCheckResult, HostError>;

    /// Graceful shutdown: sends `shutdown` RPC, waits up to 2s for ack, then
    /// kills the child. Consumes the last Arc clone — callers holding other
    /// clones see `HostError::ConnectionLost` on subsequent calls.
    pub async fn shutdown(self) -> Result<(), HostError>;
}
```

The existing `send_and_receive` private helper is deleted. Existing callers of
`host.request(...)` continue to compile against the new `&self` signature.

## Cancellation model

**Decision: Option A — out-of-band cancel as a normal RPC.**

The plugin keeps an internal `SessionRegistry` of active sessions, keyed by the
`session_id` returned in the `agent/run` response. When the host calls
`agent/cancel { session_id }`, the plugin looks up the session, cancels the
in-flight work, emits a final `agent/error` or `agent/output` notification, and
responds to the `cancel` RPC with `{ "cancelled": true }`. The original
`agent/run` request future then resolves (either with the partial result or
with a structured "cancelled" error code — TBD in the protocol crate).

This relies on the plugin being internally concurrent: the runtime must be able
to handle a second RPC frame while a first is still running. That's already
the assumption for triggers and subjects; providers gain it via the
`SessionRegistry` helper.

Plugin-side helper (lives in
`~/animus-staging/animus-protocol/animus-plugin-runtime/`, not this repo):

- `SessionRegistry::register(session_id, CancelHandle)` — called by the plugin
  at the top of its `agent/run` implementation.
- `SessionRegistry::cancel(session_id) -> bool` — wired into the plugin's
  default `agent/cancel` handler.
- `CancelHandle::cancellation_token() -> CancellationToken` — passed into
  whatever the plugin awaits so dropping it propagates cooperatively.

If a plugin's manifest reports no cancellation capability, the host returns
`HostError::CapabilityNotSupported` immediately. We do not attempt the RPC.

Option B (per-session sub-channels routed through `agent/run` streaming) is
explicitly rejected: it requires a second protocol-level concept (streaming
RPC) that JSON-RPC over stdio doesn't have natively, and the
notification-broadcast surface already does the streaming job.

## Backpressure + ordering

- **Notifications via `broadcast::Sender<RpcNotification>`**, capacity 256.
  Subscribers that lag past the capacity see `RecvError::Lagged(skipped)` —
  they're responsible for either keeping up or surfacing the loss. Capacity
  256 mirrors the session-backend convention in `plugin_backend.rs:168`
  (currently uses an mpsc with 64; broadcast needs more headroom because slow
  subscribers no longer block fast ones).
- **Responses via `oneshot::Sender<RpcResponse>`**, unbounded by definition.
  The writer never blocks on the reader.
- **Ordering**: within a single subscriber, notifications arrive in
  plugin-emit order (broadcast preserves per-receiver order). Cross-subscriber
  ordering is not guaranteed.
- **Late responses**: a response whose id is no longer in the pending map
  (the awaiter timed out, was cancelled, was never made) is logged at `debug`
  and dropped. Not an error.

## Failure modes + supervision

- **Plugin process crashes mid-call**: the reader task observes EOF on stdout,
  exits, and as it exits it drains the pending map — every
  `oneshot::Sender` is dropped, which causes every awaiting `request()` to
  return `HostError::ConnectionLost`. The broadcast sender is dropped too;
  subscribers observe a closed channel.
- **Plugin hangs (no response)**: per-call timeout via `request_with_timeout`
  is the recommended pattern. Default `request()` has no timeout because the
  primary in-flight call is `agent/run` which can legitimately stream for
  30+ minutes. Health probes (`ping`, `health/check`) keep their 2s timeouts.
- **Daemon shutdown**: `host.shutdown()` sends the `shutdown` RPC, awaits ack
  with a 2s deadline, then kills the child. All clones of the host see
  `HostError::ConnectionLost` on their next call.
- **Supervisor integration**: the trigger supervisor's
  exponential-backoff restart loop (`trigger_supervisor.rs`, commit
  `8bfeeeaf`) is the model for providers too. The new
  `PluginSessionBackend` holds an `Arc<PluginHost>` produced by a supervisor;
  when `ConnectionLost` surfaces, the supervisor respawns and rebuilds the
  Arc. In-flight callers either retry against the new host or surface a
  recoverable error to the caller. Up to `MAX_RESTART_ATTEMPTS` (currently 5)
  before the supervisor gives up.

## Migration plan

Order of code changes. Each step is independently mergeable.

1. **Host internal rewrite (in-tree).** Replace
   `PluginHost::send_and_receive` with the reader-task + pending-map
   architecture. Keep `request()` / `notify()` / `subscribe_notifications()`
   signatures backwards-compatible except for the `&mut self` → `&self`
   change. All existing callsites (`registry.rs`, `subject_router.rs`,
   `subject_adapter.rs`, `trigger_supervisor.rs`, `plugin_backend.rs`,
   `ops_plugin.rs`, `ops_mcp/plugin_tools.rs`) compile with mechanical
   `let mut host` → `let host` edits. Net: closes #8 in isolation.
2. **Provider host lifecycle (in-tree).** Introduce a per-provider host
   cache in `SessionBackendResolver`. `PluginSessionBackend::dispatch` and
   `dispatch_cancel` take an `Arc<PluginHost>` from the cache instead of
   calling `PluginHost::spawn_with_options` directly. Cancel becomes a real
   `host.request("agent/cancel", ...)` routed through the same process. Net:
   closes #7.
3. **Provider supervisor (in-tree).** Lift the trigger supervisor's restart
   loop into a generic `PluginProcessSupervisor` and use it for providers.
   This is the failure-mode story above.
4. **Plugin runtime `SessionRegistry` (separate repo
   `animus-plugin-runtime`).** Helper that providers register sessions with
   so `agent/cancel` dispatch is uniform. Required so step 2's cancel actually
   does something inside the plugin.
5. **CLI provider plugin updates (separate repos
   `animus-provider-{claude,codex,gemini,opencode}`).** Each provider wires
   its `agent/run` implementation through `SessionRegistry`. Until each
   ships, the host's `agent/cancel` falls back to `HostError::CapabilityNotSupported`
   for that provider.

Steps 1–3 ship as a single PR in this repo. Steps 4–5 land in the standalone
plugin repos and are version-pinned via the manifest protocol version.

## Test plan

New file: `crates/orchestrator-plugin-host/tests/concurrency.rs`. Each test
spawns a scripted plugin (duplex pipes + a tokio task acting as the plugin) and
drives the host through it.

- `concurrent_requests_do_not_serialize` — fire 8 `request()` calls in
  parallel, scripted plugin responds to all of them in reverse order; assert
  every caller gets the right response and total wall time is ~1× per-call
  latency, not 8×.
- `out_of_order_responses_route_to_correct_awaiter` — send requests 1, 2, 3;
  scripted plugin emits responses 3, 1, 2; assert each `request()` future
  resolves with the right payload.
- `notification_fanout_to_multiple_subscribers` — two `subscribe_notifications`
  receivers; plugin emits 10 notifications; both receivers see all 10 in
  order.
- `cancel_stops_target_session_only` — start two concurrent `agent/run`
  calls (different session_ids); cancel one; assert the other completes
  normally and the cancelled one returns the cancelled marker.
- `plugin_crash_returns_connection_lost` — scripted plugin closes stdout
  mid-call; assert every pending `request()` resolves with
  `HostError::ConnectionLost` and the broadcast receiver observes
  `RecvError::Closed`.
- `shutdown_is_graceful` — issue `shutdown()`, scripted plugin acks within
  the 2s budget; assert child exits cleanly and subsequent `request()` on a
  cloned host returns `ConnectionLost`.
- `request_with_timeout_fires` — scripted plugin never responds;
  `request_with_timeout(_, _, 100ms)` resolves with a timeout error within
  150ms.

The trigger supervisor's existing tests cover the restart loop and are not
re-litigated here.

## Open questions

These are the calls the human reviewer makes before implementation starts.

1. **Long-lived providers — agreed?** The bias in this doc is yes. The cost is
   one process per provider held open for the daemon's lifetime; the benefit
   is cancel actually working. Alternative: keep providers short-lived and
   pass the original `agent/run` cancellation token via a fresh process —
   doesn't work, the plugin has no way to find the running session.
2. **Per-call timeout default — none, or some?** Recommendation: no default.
   Provider `agent/run` legitimately takes 30+ minutes. Caller opts into
   `request_with_timeout`. Counter-argument: belt-and-suspenders against
   hung plugins. If we want a default, suggest 6 hours (longer than any sane
   agent run, short enough to GC zombies).
3. **`broadcast` capacity — 256?** Mirrors `plugin_backend.rs:168`-ish (which
   uses 64 today, mpsc). Bigger is safer against lag-disconnect at modest
   memory cost.
4. **Cancel against a plugin that doesn't advertise the capability — error or
   silent no-op?** Recommendation: `HostError::CapabilityNotSupported`,
   surfaced clearly. Silent no-op is what we have today (#7) and it's the
   worst of both worlds.
5. **Restart attempts shared with triggers, or per-kind?** Recommendation:
   share the `MAX_RESTART_ATTEMPTS=5` constant. If providers prove flakier,
   split later.
