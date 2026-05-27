//! Concurrency contract tests for [`PluginHost`].
//!
//! Each test scripts a "fake plugin" inside the test process using
//! `tokio::io::duplex` pipes. The fake plugin reads JSON-RPC frames the host
//! writes and replies with whatever the scenario demands. This lets us drive
//! the host's request/response router under tightly controlled timing
//! without spawning real subprocesses.
//!
//! The scenarios mirror the contract at
//! `docs/architecture/plugin-host-concurrency.md`:
//!
//! 1. Concurrent requests round-trip independently.
//! 2. Out-of-order responses route to the right awaiter.
//! 3. Notifications fan out to every subscriber.
//! 4. Plugin crash mid-request returns `ConnectionLost`.
//! 5. Graceful shutdown drains pending awaiters with `ConnectionLost`.
//! 6. `request_with_timeout` surfaces timeout when the plugin is silent.
//! 7. Broadcast capacity from a manifest hint produces `Lagged` for slow
//!    subscribers.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use animus_plugin_protocol::{RpcNotification, RpcRequest, RpcResponse};
use orchestrator_plugin_host::{HostError, PluginHost, NOTIFICATION_BROADCAST_CAPACITY_ENV};
use serde_json::json;
use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::task::JoinHandle;

/// Helper: spawn a fake-plugin task that loops over inbound requests and lets
/// the test driver supply each response. The returned `cmd_tx` channel takes
/// `FakeCommand` directives which the fake plugin executes asynchronously.
fn spawn_scripted_plugin(buffer: usize) -> ScriptedPlugin {
    let (host_reader, plugin_writer) = duplex(buffer);
    let (plugin_reader, host_writer) = duplex(buffer);
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<FakeCommand>(32);
    let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel::<RpcRequest>();

    // Reader task: parse the host's outbound frames as RpcRequest, push to req_rx.
    let plugin_reader_task: JoinHandle<()> = tokio::spawn(async move {
        let mut reader = BufReader::new(plugin_reader);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let value: serde_json::Value = match serde_json::from_str(trimmed) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    // Treat both requests and notifications as RpcRequest with
                    // optional id. Notifications won't have an id; we forward
                    // them anyway so test drivers can observe handshake frames
                    // like `initialized` or `exit`.
                    if let Ok(req) = serde_json::from_value::<RpcRequest>(value) {
                        let _ = req_tx.send(req);
                    }
                }
            }
        }
    });

    // Writer task: receive FakeCommand directives and act on them. Wrapping the
    // writer in an Option so we can drop it to simulate plugin death.
    let writer_arc = Arc::new(tokio::sync::Mutex::new(Some(plugin_writer)));
    let writer_arc_for_task = writer_arc.clone();
    let plugin_writer_task: JoinHandle<()> = tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                FakeCommand::Send(frame) => {
                    let mut writer_opt = writer_arc_for_task.lock().await;
                    let Some(writer) = writer_opt.as_mut() else { continue };
                    let mut line = match serde_json::to_string(&frame) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    line.push('\n');
                    if writer.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = writer.flush().await;
                }
                FakeCommand::CloseStdout => {
                    let mut writer_opt = writer_arc_for_task.lock().await;
                    // Dropping the writer closes the duplex stream's host-side
                    // read half (EOF), which is exactly what we want.
                    writer_opt.take();
                }
            }
        }
    });

    ScriptedPlugin {
        host_reader,
        host_writer,
        cmd_tx,
        req_rx: Mutex::new(Some(req_rx)),
        _reader_task: plugin_reader_task,
        _writer_task: plugin_writer_task,
    }
}

/// Commands a test driver sends to the fake plugin task.
enum FakeCommand {
    /// Write a JSON frame to the host.
    Send(serde_json::Value),
    /// Close the host-side stdout so the host observes EOF.
    CloseStdout,
}

struct ScriptedPlugin {
    host_reader: DuplexStream,
    host_writer: DuplexStream,
    cmd_tx: tokio::sync::mpsc::Sender<FakeCommand>,
    /// Take()'d by the test driver to drain inbound requests.
    req_rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<RpcRequest>>>,
    _reader_task: JoinHandle<()>,
    _writer_task: JoinHandle<()>,
}

impl ScriptedPlugin {
    /// Build the host wired up to this scripted plugin's pipes.
    fn into_host(self) -> (PluginHost, PluginDriver) {
        let host = PluginHost::from_streams("scripted", self.host_reader, self.host_writer);
        let driver = PluginDriver {
            cmd_tx: self.cmd_tx,
            req_rx: self.req_rx.lock().unwrap().take().expect("req_rx not already taken"),
            _reader_task: self._reader_task,
            _writer_task: self._writer_task,
        };
        (host, driver)
    }

    /// Variant for tests that need explicit broadcast capacity control.
    fn into_host_with_capacity(self, capacity: usize) -> (PluginHost, PluginDriver) {
        let host = PluginHost::from_streams_with_capacity("scripted", self.host_reader, self.host_writer, capacity);
        let driver = PluginDriver {
            cmd_tx: self.cmd_tx,
            req_rx: self.req_rx.lock().unwrap().take().expect("req_rx not already taken"),
            _reader_task: self._reader_task,
            _writer_task: self._writer_task,
        };
        (host, driver)
    }
}

/// Handle used by the test driver to script plugin behavior post-handshake.
struct PluginDriver {
    cmd_tx: tokio::sync::mpsc::Sender<FakeCommand>,
    req_rx: tokio::sync::mpsc::UnboundedReceiver<RpcRequest>,
    _reader_task: JoinHandle<()>,
    _writer_task: JoinHandle<()>,
}

impl PluginDriver {
    async fn next_request(&mut self) -> RpcRequest {
        self.req_rx.recv().await.expect("plugin reader stream closed before next request arrived")
    }

    async fn send_frame(&self, value: serde_json::Value) {
        self.cmd_tx.send(FakeCommand::Send(value)).await.expect("send to fake plugin should not fail");
    }

    async fn send_response(&self, id: serde_json::Value, result: serde_json::Value) {
        let response = RpcResponse::ok(Some(id), result);
        let frame = serde_json::to_value(&response).expect("encode response");
        self.send_frame(frame).await;
    }

    async fn send_notification(&self, method: &str, params: Option<serde_json::Value>) {
        let notification = RpcNotification::new(method, params);
        let frame = serde_json::to_value(&notification).expect("encode notification");
        self.send_frame(frame).await;
    }

    async fn close_stdout(&self) {
        self.cmd_tx.send(FakeCommand::CloseStdout).await.expect("send close to fake plugin");
    }
}

// ---------------------------------------------------------------------------
// Scenario 1: concurrent requests round-trip independently.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_requests_round_trip_independently() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();
    let host = Arc::new(host);

    // Fire 4 concurrent request() calls. The plugin will respond to each only
    // after all 4 have arrived — proving they're not serialized at the host
    // boundary.
    let h1 = {
        let host = host.clone();
        tokio::spawn(async move { host.request("a", Some(json!({"i": 1}))).await })
    };
    let h2 = {
        let host = host.clone();
        tokio::spawn(async move { host.request("b", Some(json!({"i": 2}))).await })
    };
    let h3 = {
        let host = host.clone();
        tokio::spawn(async move { host.request("c", Some(json!({"i": 3}))).await })
    };
    let h4 = {
        let host = host.clone();
        tokio::spawn(async move { host.request("d", Some(json!({"i": 4}))).await })
    };

    // Collect all 4 incoming requests before responding to any.
    let r1 = driver.next_request().await;
    let r2 = driver.next_request().await;
    let r3 = driver.next_request().await;
    let r4 = driver.next_request().await;

    // Respond in REVERSE order to make the OOO behavior explicit.
    driver.send_response(r4.id.clone().unwrap(), json!({"echo": "d"})).await;
    driver.send_response(r3.id.clone().unwrap(), json!({"echo": "c"})).await;
    driver.send_response(r2.id.clone().unwrap(), json!({"echo": "b"})).await;
    driver.send_response(r1.id.clone().unwrap(), json!({"echo": "a"})).await;

    let (a, b, c, d) = (
        h1.await.expect("join").expect("request a"),
        h2.await.expect("join").expect("request b"),
        h3.await.expect("join").expect("request c"),
        h4.await.expect("join").expect("request d"),
    );
    assert_eq!(a, json!({"echo": "a"}));
    assert_eq!(b, json!({"echo": "b"}));
    assert_eq!(c, json!({"echo": "c"}));
    assert_eq!(d, json!({"echo": "d"}));
}

// ---------------------------------------------------------------------------
// Scenario 2: out-of-order responses match the correct awaiter.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn out_of_order_responses_match_correct_awaiter() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();
    let host = Arc::new(host);

    let h_first = {
        let host = host.clone();
        tokio::spawn(async move { host.request("first", None).await })
    };
    let h_second = {
        let host = host.clone();
        tokio::spawn(async move { host.request("second", None).await })
    };

    let req_first = driver.next_request().await;
    let req_second = driver.next_request().await;

    // Respond to the SECOND request first.
    driver.send_response(req_second.id.clone().unwrap(), json!({"who": "second"})).await;
    // Then respond to the first.
    driver.send_response(req_first.id.clone().unwrap(), json!({"who": "first"})).await;

    let first = h_first.await.expect("join").expect("first");
    let second = h_second.await.expect("join").expect("second");
    assert_eq!(first, json!({"who": "first"}), "the first awaiter must see the first request's response");
    assert_eq!(second, json!({"who": "second"}), "the second awaiter must see the second request's response");
}

// ---------------------------------------------------------------------------
// Scenario 3: notifications fan out to multiple subscribers.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn notifications_fan_out_to_multiple_subscribers() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, driver) = scripted.into_host();

    let mut s1 = host.subscribe_notifications();
    let mut s2 = host.subscribe_notifications();
    let mut s3 = host.subscribe_notifications();

    driver.send_notification("agent/output", Some(json!({"text": "hello"}))).await;

    let n1 = s1.recv().await.expect("s1 recv");
    let n2 = s2.recv().await.expect("s2 recv");
    let n3 = s3.recv().await.expect("s3 recv");

    for n in &[n1, n2, n3] {
        assert_eq!(n.method, "agent/output");
        assert_eq!(n.params.as_ref().and_then(|p| p.get("text")), Some(&json!("hello")));
    }
}

// ---------------------------------------------------------------------------
// Scenario 4: concurrent request during plugin crash returns ConnectionLost.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_request_during_plugin_crash_returns_connection_lost() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();
    let host = Arc::new(host);

    let h = {
        let host = host.clone();
        tokio::spawn(async move { host.request("never-responds", None).await })
    };
    // Wait for the host to actually write the request.
    let _ = driver.next_request().await;
    // Now close the plugin's stdout, simulating a crash.
    driver.close_stdout().await;

    let outcome = h.await.expect("join");
    let err = outcome.expect_err("must surface error after plugin crash");
    let message = err.message.clone();
    assert!(message.contains("plugin connection lost"), "expected ConnectionLost in error message, got: {message}");
    // And the internal-error code is what we mapped HostError → RpcError onto.
    assert_eq!(err.code, animus_plugin_protocol::error_codes::INTERNAL_ERROR);
}

// ---------------------------------------------------------------------------
// Scenario 5: graceful shutdown drains pending awaiters with ConnectionLost.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graceful_shutdown_drains_pending() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();
    // Keep a clone for the in-flight request — the shutdown() consumes one.
    let host_for_request = host.clone();
    let h_pending = tokio::spawn(async move { host_for_request.request("will-be-dropped", None).await });

    let _ = driver.next_request().await;

    // shutdown() consumes self and closes the child + reader. The shutdown RPC
    // races with the close — the fake plugin never responds, so shutdown's
    // 2s budget will elapse and proceed to drop the child.
    let shutdown = tokio::spawn(async move { host.shutdown().await });

    // Eventually the pending request must surface ConnectionLost (the reader
    // task drains the pending map on its way out).
    let outcome = tokio::time::timeout(Duration::from_secs(10), h_pending).await;
    let joined = outcome.expect("h_pending must complete before deadline").expect("join");
    let err = joined.expect_err("pending request must surface error after shutdown");
    assert_eq!(err.code, animus_plugin_protocol::error_codes::INTERNAL_ERROR);
    assert!(err.message.contains("plugin connection lost"));

    let _ = tokio::time::timeout(Duration::from_secs(15), shutdown).await;
}

// ---------------------------------------------------------------------------
// Scenario 6: request_with_timeout returns Timeout when plugin is silent.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_with_timeout_returns_timeout_when_plugin_silent() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();

    let started = Instant::now();
    let outcome = host.request_with_timeout("never-responds", None, Duration::from_millis(50)).await;
    let elapsed = started.elapsed();

    let err = outcome.expect_err("must surface timeout when plugin is silent");
    assert_eq!(err.code, animus_plugin_protocol::error_codes::TIMEOUT);
    assert!(elapsed < Duration::from_secs(2), "timeout fired too late: {elapsed:?}");

    // The fake plugin should still have observed the request frame — proving
    // we didn't bail before writing. Pull it off the queue so the test exits
    // cleanly.
    let _ = driver.next_request().await;
}

// ---------------------------------------------------------------------------
// Scenario 7: broadcast capacity honors manifest hint (via from_streams_with_capacity).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn broadcast_capacity_honors_manifest_hint() {
    // Spawn with capacity 8 (matching the "manifest hint" scenario). A
    // subscriber that doesn't recv() before the buffer overruns sees
    // RecvError::Lagged on the next recv(). The "manifest hint" lives in
    // PluginManifest.notification_buffer_size; for tests we use the
    // equivalent from_streams_with_capacity helper.
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, driver) = scripted.into_host_with_capacity(8);

    // Subscribe BEFORE the plugin starts emitting — broadcast only delivers
    // notifications to subscribers that existed when the notification was
    // sent.
    let mut slow = host.subscribe_notifications();

    // Emit 20 notifications back-to-back. The slow subscriber never recv()s
    // while this happens, so its buffer overruns.
    for i in 0..20u32 {
        driver.send_notification("burst", Some(json!({"i": i}))).await;
    }

    // Give the reader task time to push every notification into the
    // broadcast channel.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now drain the slow subscriber. With capacity 8 and 20 notifications
    // queued, we expect at least one Lagged on the way through.
    let mut observed_lagged = false;
    let mut delivered = 0;
    loop {
        match slow.try_recv() {
            Ok(_) => delivered += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                observed_lagged = true;
                assert!(skipped >= 1, "Lagged should report >=1 skipped frame, got {skipped}");
            }
            Err(_) => break,
        }
    }
    assert!(
        observed_lagged,
        "slow subscriber must observe Lagged after the broadcast buffer overruns (capacity=8, sent=20); delivered={delivered}"
    );
}

// ---------------------------------------------------------------------------
// Extra: verify the env override path so we don't silently regress that
// branch of `resolve_broadcast_capacity` when the host is restructured.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn explicit_capacity_override_overruns_on_burst() {
    // Sanity check that small capacities produce Lagged when bursts overflow.
    // The env var path is covered by the unit test
    // `resolve_capacity_priority_order` in host.rs.
    let _ = NOTIFICATION_BROADCAST_CAPACITY_ENV; // ensure the const is reachable

    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, driver) = scripted.into_host_with_capacity(4);
    let mut slow = host.subscribe_notifications();

    for i in 0..10u32 {
        driver.send_notification("burst", Some(json!({"i": i}))).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut observed_lagged = false;
    loop {
        match slow.try_recv() {
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                observed_lagged = true;
            }
            Err(_) => break,
        }
    }
    assert!(observed_lagged, "capacity=4 must cause slow subscriber to lag after 10 notifications");
}

// Use the lower-level HostError APIs internally to also exercise the timeout
// host error type.
#[tokio::test]
async fn host_error_timeout_display_includes_duration() {
    let err = HostError::Timeout(Duration::from_millis(123));
    let message = format!("{err}");
    assert!(message.contains("123"), "Timeout error should mention the duration; got: {message}");
}

// ---------------------------------------------------------------------------
// Transport-lifecycle contract.
//
// Spec (animus-transport-protocol v0.1.13, spec.md §13): the host MUST drive
// `initialize` → `transport/start` → (work) → `transport/shutdown` → generic
// `shutdown` on every transport_backend plugin. Pre-spec plugins respond to
// `transport/shutdown` with METHOD_NOT_FOUND and the host MUST treat that as
// a no-op so legacy launchapp-dev transports keep working during the
// ecosystem upgrade.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shutdown_transport_drains_then_generic_shutdown_runs() {
    use animus_plugin_protocol::error_codes;

    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();

    // Record the methods the host issues, in order. Spec demands:
    //   initialize → work → transport/shutdown → shutdown → exit
    let methods: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let driver_methods = methods.clone();
    let driver_task: JoinHandle<()> = tokio::spawn(async move {
        loop {
            let req = match tokio::time::timeout(Duration::from_secs(5), driver.next_request()).await {
                Ok(req) => req,
                Err(_) => return,
            };
            driver_methods.lock().unwrap().push(req.method.clone());
            // Respond to anything with an id. Notifications (no id) are
            // observed but get no reply.
            if let Some(id) = req.id.clone() {
                match req.method.as_str() {
                    "initialize" => {
                        driver
                            .send_response(
                                id,
                                json!({
                                    "protocol_version": animus_plugin_protocol::PROTOCOL_VERSION,
                                    "plugin_info": {
                                        "name": "mock-transport",
                                        "version": "0.1.0",
                                        "plugin_kind": "transport_backend",
                                    },
                                    "capabilities": {},
                                }),
                            )
                            .await;
                    }
                    "transport/shutdown" => {
                        driver.send_response(id, json!({"shutdown": true})).await;
                    }
                    "shutdown" => {
                        driver.send_response(id, json!({})).await;
                    }
                    _ => {
                        // Reply OK to anything else (work). The test exercises
                        // a single `request("work", ...)` call between the
                        // handshake and shutdown.
                        driver.send_response(id, json!({"ok": true})).await;
                    }
                }
            }
        }
    });

    // Drive the spec lifecycle.
    host.handshake().await.expect("handshake");
    host.request("work", Some(json!({"step": 1}))).await.expect("work");
    host.shutdown_transport().await.expect("transport/shutdown should succeed");
    host.shutdown().await.expect("generic shutdown");
    drop(driver_task);

    // Verify the ordering. `initialized` is a notification the host sends
    // post-handshake; it shows up between `initialize` and `work`.
    let recorded = methods.lock().unwrap().clone();
    let positions = |needle: &str| -> Option<usize> { recorded.iter().position(|m| m == needle) };

    let p_init = positions("initialize").expect("initialize must be sent");
    let p_work = positions("work").expect("work must be sent");
    let p_t_shutdown =
        positions("transport/shutdown").expect("transport/shutdown must be sent BEFORE generic shutdown");
    let p_shutdown = positions("shutdown").expect("generic shutdown must be sent");

    assert!(p_init < p_work, "initialize must precede work; got {recorded:?}");
    assert!(p_work < p_t_shutdown, "work must precede transport/shutdown; got {recorded:?}");
    assert!(
        p_t_shutdown < p_shutdown,
        "transport/shutdown must precede generic shutdown (spec §13.4); got {recorded:?}"
    );

    // Defensive: make sure no shutdown was sent before transport/shutdown.
    assert_eq!(
        recorded.iter().filter(|m| m.as_str() == "transport/shutdown").count(),
        1,
        "transport/shutdown should be sent exactly once; got {recorded:?}"
    );
    assert_eq!(
        recorded.iter().filter(|m| m.as_str() == "shutdown").count(),
        1,
        "generic shutdown should be sent exactly once; got {recorded:?}"
    );

    // Sanity: the JSON-RPC method names match the wire constants exported by
    // the host crate so a future rename surfaces here too.
    assert_eq!(orchestrator_plugin_host::TRANSPORT_METHOD_START, "transport/start");
    assert_eq!(orchestrator_plugin_host::TRANSPORT_METHOD_SHUTDOWN, "transport/shutdown");
    let _ = error_codes::METHOD_NOT_FOUND; // keep import used
}

#[tokio::test]
async fn shutdown_transport_swallows_method_not_found_for_legacy_plugins() {
    use animus_plugin_protocol::error_codes;

    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, mut driver) = scripted.into_host();

    let driver_task: JoinHandle<()> = tokio::spawn(async move {
        loop {
            let req = match tokio::time::timeout(Duration::from_secs(5), driver.next_request()).await {
                Ok(req) => req,
                Err(_) => return,
            };
            let Some(id) = req.id.clone() else { continue };
            match req.method.as_str() {
                "initialize" => {
                    driver
                        .send_response(
                            id,
                            json!({
                                "protocol_version": animus_plugin_protocol::PROTOCOL_VERSION,
                                "plugin_info": {
                                    "name": "legacy-transport",
                                    "version": "0.1.0",
                                    "plugin_kind": "transport_backend",
                                },
                                "capabilities": {},
                            }),
                        )
                        .await;
                }
                "transport/shutdown" => {
                    // Pre-spec plugins do not dispatch `transport/*` and
                    // reply with the generic JSON-RPC METHOD_NOT_FOUND error.
                    let response = animus_plugin_protocol::RpcResponse::err(
                        Some(id),
                        animus_plugin_protocol::RpcError {
                            code: error_codes::METHOD_NOT_FOUND,
                            message: "method not found".to_string(),
                            data: None,
                        },
                    );
                    let frame = serde_json::to_value(&response).unwrap();
                    driver.send_frame(frame).await;
                }
                "shutdown" => {
                    driver.send_response(id, json!({})).await;
                }
                _ => {
                    driver.send_response(id, json!({})).await;
                }
            }
        }
    });

    host.handshake().await.expect("handshake");
    // Spec compliance: even when the plugin doesn't implement
    // `transport/shutdown`, the host must succeed (log deprecation + continue)
    // so legacy launchapp-dev transports keep serving.
    host.shutdown_transport().await.expect("transport/shutdown METHOD_NOT_FOUND must be a no-op");
    host.shutdown().await.expect("generic shutdown");
    drop(driver_task);
}

// Per the contract: subsequent request() on a cloned host returns
// ConnectionLost after shutdown.
#[tokio::test]
async fn request_after_shutdown_returns_connection_lost() {
    let scripted = spawn_scripted_plugin(64 * 1024);
    let (host, _driver) = scripted.into_host();
    let cloned = host.clone();
    let _ = host.shutdown().await;

    let err = cloned.request("post-shutdown", None).await.expect_err("request after shutdown must return an error");
    assert_eq!(err.code, animus_plugin_protocol::error_codes::INTERNAL_ERROR);
    assert!(err.message.contains("plugin connection lost"), "expected ConnectionLost message; got: {}", err.message);
}
