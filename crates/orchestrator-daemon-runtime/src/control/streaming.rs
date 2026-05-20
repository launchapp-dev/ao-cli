//! Broadcast buses for streaming control-server methods.
//!
//! Each streaming method (`subject/watch`, `daemon/events`, `daemon/logs`)
//! drains a [`tokio::sync::broadcast`] channel created at daemon startup
//! and forwards each item as a JSON-RPC notification on the originating
//! connection. The bus types here are deliberately thin — they exist so
//! the daemon's existing event-handling code paths can publish into them
//! without taking a hard dep on the control protocol.
//!
//! Anti-deadlock rules:
//!
//! - Buses use [`tokio::sync::broadcast`] (lock-free MPMC channel). No
//!   `tokio::sync::Mutex` anywhere on the publish path.
//! - Buses are cloneable [`Arc`] handles set once at startup; the daemon
//!   never replaces them at runtime.
//! - Subscribers tolerate `Lagged` errors by dropping the lagged batch
//!   and continuing — slow clients never block fast publishers.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

/// Default in-flight buffer for each broadcast channel.
///
/// Chosen large enough that a handful of slow clients don't drop events
/// during normal operation but small enough that a stuck client doesn't
/// hold megabytes of memory. Subscribers handle `Lagged` by dropping the
/// lagged batch.
pub const DEFAULT_BUS_CAPACITY: usize = 1024;

/// Fan-out channel for `daemon/events` notifications.
///
/// The daemon's `handle_event` hook publishes a JSON-typed snapshot of
/// each [`crate::DaemonRunEvent`] into this bus; per-connection stream
/// drivers fan out to subscribed clients. JSON payload (rather than the
/// concrete enum) keeps the bus crate-boundary-friendly — the in-tree
/// enum is internal, but its serialized shape matches
/// [`animus_control_protocol::DaemonRunEvent`].
#[derive(Clone)]
pub struct DaemonEventBus {
    inner: Arc<broadcast::Sender<Value>>,
}

impl DaemonEventBus {
    /// Allocate a bus with [`DEFAULT_BUS_CAPACITY`] in-flight slots.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUS_CAPACITY)
    }

    /// Allocate a bus with a custom in-flight capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { inner: Arc::new(tx) }
    }

    /// Publish a serialized event onto the bus.
    ///
    /// Returns the number of active subscribers that received the event
    /// (always `>=0`). Returns `0` when no subscribers are connected.
    pub fn publish(&self, event: Value) -> usize {
        self.inner.send(event).unwrap_or(0)
    }

    /// Open a new subscription. Each subscriber receives every event
    /// published after this call. Slow subscribers may receive
    /// [`broadcast::error::RecvError::Lagged`] when the buffer overflows;
    /// the streaming driver in [`super::connection`] handles that by
    /// dropping the lagged batch.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.subscribe()
    }

    /// Number of active subscribers. Useful for the daemon status snapshot.
    pub fn subscriber_count(&self) -> usize {
        self.inner.receiver_count()
    }
}

impl Default for DaemonEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DaemonEventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonEventBus").field("subscriber_count", &self.subscriber_count()).finish()
    }
}

/// Fan-out channel for `daemon/logs` notifications.
///
/// Mirrors [`DaemonEventBus`] but carries log-entry-shaped JSON. Wired by
/// the daemon's [`crate::log_storage::LogStorageDispatch`] entrypoint —
/// when an entry lands on the in-tree fallback or is forwarded from a
/// plugin, the daemon publishes a serialized [`animus_log_storage_protocol::LogEntry`]
/// into this bus.
#[derive(Clone)]
pub struct DaemonLogBus {
    inner: Arc<broadcast::Sender<Value>>,
}

impl DaemonLogBus {
    /// Allocate a bus with [`DEFAULT_BUS_CAPACITY`] in-flight slots.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUS_CAPACITY)
    }

    /// Allocate a bus with a custom in-flight capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { inner: Arc::new(tx) }
    }

    /// Publish a serialized log entry onto the bus.
    pub fn publish(&self, entry: Value) -> usize {
        self.inner.send(entry).unwrap_or(0)
    }

    /// Open a new subscription.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.subscribe()
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.inner.receiver_count()
    }
}

impl Default for DaemonLogBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DaemonLogBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonLogBus").field("subscriber_count", &self.subscriber_count()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn event_bus_publishes_to_subscribers() {
        let bus = DaemonEventBus::new();
        let mut sub = bus.subscribe();
        let n = bus.publish(serde_json::json!({"kind": "tick"}));
        assert_eq!(n, 1, "one subscriber should receive");
        let event = sub.recv().await.expect("recv");
        assert_eq!(event, serde_json::json!({"kind": "tick"}));
    }

    #[tokio::test]
    async fn event_bus_publish_with_no_subscribers_returns_zero() {
        let bus = DaemonEventBus::new();
        let n = bus.publish(serde_json::json!({"kind": "tick"}));
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn log_bus_publishes_to_subscribers() {
        let bus = DaemonLogBus::new();
        let mut sub = bus.subscribe();
        bus.publish(serde_json::json!({"level": "info", "message": "ok"}));
        let entry = sub.recv().await.expect("recv");
        assert_eq!(entry.get("level").unwrap(), "info");
    }
}
