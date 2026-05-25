//! Broadcaster for daemon-side `workflow/events` subscriptions.
//!
//! Subscribers register an optional [`WorkflowEventFilter`] and receive a
//! non-blocking [`mpsc::Receiver`] of [`SubscriberItem`]s — `Event(...)`
//! for normal fan-out, `Closed { reason }` as a terminal frame the
//! connection driver turns into a `subscription/closed` JSON-RPC
//! notification. Slow subscribers do NOT block the emitter — when a
//! subscriber's buffer is full the event is dropped for *that* subscriber
//! only (with a `tracing::warn!`) and the broadcaster continues to fan
//! out to the remaining subscribers.
//!
//! Subscriptions are per-connection: they live only as long as the
//! control-socket connection that opened them. The daemon does not persist
//! subscriptions across restarts. A subscription that pinned its filter to
//! a single `workflow_id` is auto-closed when `workflow_completed` or
//! `workflow_failed` arrives for that workflow.

use std::sync::Arc;

use animus_control_protocol::types::WorkflowEvent;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;

const DEFAULT_SUBSCRIBER_BUFFER: usize = 256;

/// Workflow kinds whose arrival implicitly closes any subscription
/// filtered to that specific workflow id. The terminal `subscription/closed`
/// notification fires *after* the matching event is delivered so clients
/// see the final event and then a clean stream end.
const TERMINAL_WORKFLOW_KINDS: &[&str] = &["workflow_completed", "workflow_failed"];

/// Opaque identifier for a single subscription. Returned by
/// [`WorkflowEventBroadcaster::subscribe`] and consumed by
/// [`WorkflowEventBroadcaster::unsubscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub u64);

/// Filter applied per-subscriber. All present filters AND together: an
/// event is delivered when its `workflow_id` matches (or the filter is
/// `None`) AND its `kind` is in `kinds` (or `kinds` is `None`).
#[derive(Debug, Clone, Default)]
pub struct WorkflowEventFilter {
    pub workflow_id: Option<String>,
    pub kinds: Option<Vec<String>>,
}

impl WorkflowEventFilter {
    fn matches(&self, event: &WorkflowEvent) -> bool {
        if let Some(ref wf) = self.workflow_id {
            if event.workflow_id != *wf {
                return false;
            }
        }
        if let Some(ref kinds) = self.kinds {
            if !kinds.iter().any(|k| k == &event.kind) {
                return false;
            }
        }
        true
    }
}

/// What a per-subscriber channel carries. `Event` is the normal fan-out
/// payload; `Closed` is the terminal frame the connection driver should
/// translate into a `subscription/closed` JSON-RPC notification before
/// shutting down the driver task.
#[derive(Debug, Clone)]
pub enum SubscriberItem {
    Event(WorkflowEvent),
    Closed { reason: String },
}

struct SubscriberSlot {
    id: SubscriptionId,
    sender: mpsc::Sender<SubscriberItem>,
    filter: WorkflowEventFilter,
}

/// Fan-out hub for `workflow/events`.
///
/// One instance per daemon. Cloned [`Arc`] handles are passed to:
/// (a) the control-server connection layer, which calls
/// [`Self::subscribe`] when a client opens a `workflow/events` subscription;
/// (b) the workflow runner emitter shim, which calls [`Self::emit`] at
/// phase / workflow boundaries.
#[derive(Default)]
pub struct WorkflowEventBroadcaster {
    next_id: AtomicU64,
    subscribers: Mutex<Vec<SubscriberSlot>>,
}

impl WorkflowEventBroadcaster {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Open a subscription. Returns the subscription id plus a receiver
    /// that yields filtered [`SubscriberItem`]s until the subscription is
    /// dropped, [`Self::unsubscribe`] is called, or a terminal close item
    /// is sent (see [`Self::close_subscription`] and the implicit
    /// workflow_completed/workflow_failed close).
    pub fn subscribe(
        self: &Arc<Self>,
        filter: WorkflowEventFilter,
    ) -> (SubscriptionId, mpsc::Receiver<SubscriberItem>) {
        self.subscribe_with_buffer(filter, DEFAULT_SUBSCRIBER_BUFFER)
    }

    pub fn subscribe_with_buffer(
        self: &Arc<Self>,
        filter: WorkflowEventFilter,
        buffer: usize,
    ) -> (SubscriptionId, mpsc::Receiver<SubscriberItem>) {
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel(buffer.max(1));
        let mut guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
        guard.push(SubscriberSlot { id, sender: tx, filter });
        (id, rx)
    }

    /// Drop a subscription by id. Called on client disconnect.
    pub fn unsubscribe(&self, id: SubscriptionId) {
        let mut guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
        guard.retain(|s| s.id != id);
    }

    /// Send a terminal close item to one subscription and remove it. The
    /// connection driver translates the close item into a JSON-RPC
    /// `subscription/closed` notification on the wire.
    ///
    /// Returns `true` if the subscription was present (and the close
    /// item was either enqueued or the channel was already closed by the
    /// receiver — either way the slot is removed).
    pub fn close_subscription(&self, id: SubscriptionId, reason: impl Into<String>) -> bool {
        let reason = reason.into();
        let mut guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
        let Some(idx) = guard.iter().position(|s| s.id == id) else {
            return false;
        };
        let slot = guard.remove(idx);
        // Non-blocking try_send: if the receiver buffer is full we drop the
        // close item rather than back-pressure the caller. The receiver's
        // sender drop on slot removal still terminates the stream cleanly
        // (recv returns None) so clients don't hang.
        let _ = slot.sender.try_send(SubscriberItem::Closed { reason });
        true
    }

    /// Number of active subscribers — useful for tests and debug.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().expect("workflow event subscribers mutex poisoned").len()
    }

    /// Publish an event. Returns the number of subscribers that received
    /// the event (filter-matched and channel had capacity).
    ///
    /// Per-subscriber delivery is non-blocking [`mpsc::Sender::try_send`].
    /// A subscriber whose buffer is full sees the event dropped and a
    /// `tracing::warn!` recorded; the broadcaster continues to fan out
    /// to the remaining subscribers.
    pub fn emit(&self, event: WorkflowEvent) -> usize {
        crate::metrics::incr(&crate::metrics::labeled("subscription_events_total", &[("kind", event.kind.as_str())]));
        match event.kind.as_str() {
            "workflow_completed" => {
                crate::metrics::incr(&crate::metrics::labeled("workflow_runs_total", &[("status", "completed")]));
            }
            "workflow_failed" => {
                crate::metrics::incr(&crate::metrics::labeled("workflow_runs_total", &[("status", "failed")]));
            }
            "phase_completed" => {
                crate::metrics::incr(&crate::metrics::labeled("phase_executions_total", &[("status", "completed")]));
            }
            "phase_failed" => {
                crate::metrics::incr(&crate::metrics::labeled("phase_executions_total", &[("status", "failed")]));
            }
            _ => {}
        }
        let mut delivered = 0usize;
        let mut closed_ids: Vec<SubscriptionId> = Vec::new();
        let mut terminal_ids: Vec<SubscriptionId> = Vec::new();
        let is_terminal = TERMINAL_WORKFLOW_KINDS.contains(&event.kind.as_str());
        {
            let guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
            for slot in guard.iter() {
                if !slot.filter.matches(&event) {
                    continue;
                }
                match slot.sender.try_send(SubscriberItem::Event(event.clone())) {
                    Ok(()) => {
                        delivered += 1;
                        // Only auto-close subscribers whose filter pinned
                        // them to *this* workflow. Open-ended subscribers
                        // (no workflow_id filter) keep streaming across
                        // workflow boundaries.
                        if is_terminal && slot.filter.workflow_id.as_deref() == Some(event.workflow_id.as_str()) {
                            terminal_ids.push(slot.id);
                        }
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::warn!(
                            target: "animus.control.workflow_events",
                            subscription_id = slot.id.0,
                            workflow_id = %event.workflow_id,
                            kind = %event.kind,
                            "workflow event dropped: subscriber buffer full"
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        closed_ids.push(slot.id);
                    }
                }
            }
        }
        if !closed_ids.is_empty() {
            let mut guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
            guard.retain(|s| !closed_ids.contains(&s.id));
        }
        for sub_id in terminal_ids {
            self.close_subscription(sub_id, format!("workflow {} ended ({})", event.workflow_id, event.kind));
        }
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn evt(workflow_id: &str, kind: &str) -> WorkflowEvent {
        WorkflowEvent {
            workflow_id: workflow_id.to_string(),
            kind: kind.to_string(),
            payload: json!({}),
            occurred_at: Utc::now(),
        }
    }

    fn unwrap_event(item: SubscriberItem) -> WorkflowEvent {
        match item {
            SubscriberItem::Event(e) => e,
            SubscriberItem::Closed { reason } => panic!("expected event, got Closed({reason})"),
        }
    }

    #[tokio::test]
    async fn broadcaster_routes_events_to_matching_filter_only() {
        let bus = WorkflowEventBroadcaster::new();
        let (_id_all, mut rx_all) = bus.subscribe(WorkflowEventFilter::default());
        let (_id_wf1, mut rx_wf1) =
            bus.subscribe(WorkflowEventFilter { workflow_id: Some("wf-1".into()), kinds: None });
        let (_id_kind, mut rx_kind) =
            bus.subscribe(WorkflowEventFilter { workflow_id: None, kinds: Some(vec!["phase_completed".into()]) });

        bus.emit(evt("wf-1", "phase_started"));
        bus.emit(evt("wf-2", "phase_completed"));
        bus.emit(evt("wf-1", "phase_completed"));

        let mut all_kinds = Vec::new();
        for _ in 0..3 {
            let e = unwrap_event(rx_all.recv().await.expect("rx_all closed"));
            all_kinds.push((e.workflow_id, e.kind));
        }
        assert_eq!(
            all_kinds,
            vec![
                ("wf-1".to_string(), "phase_started".to_string()),
                ("wf-2".to_string(), "phase_completed".to_string()),
                ("wf-1".to_string(), "phase_completed".to_string()),
            ]
        );

        let mut wf1_seen = Vec::new();
        for _ in 0..2 {
            let e = unwrap_event(rx_wf1.recv().await.expect("rx_wf1 closed"));
            wf1_seen.push(e.kind);
        }
        assert_eq!(wf1_seen, vec!["phase_started".to_string(), "phase_completed".to_string()]);

        let mut kind_seen = Vec::new();
        for _ in 0..2 {
            let e = unwrap_event(rx_kind.recv().await.expect("rx_kind closed"));
            kind_seen.push(e.workflow_id);
        }
        assert_eq!(kind_seen, vec!["wf-2".to_string(), "wf-1".to_string()]);
    }

    #[tokio::test]
    async fn broadcaster_drops_when_subscriber_buffer_full() {
        let bus = WorkflowEventBroadcaster::new();
        let (_id, mut rx) = bus.subscribe_with_buffer(WorkflowEventFilter::default(), 2);

        let delivered_first = bus.emit(evt("wf-1", "phase_started"));
        let delivered_second = bus.emit(evt("wf-1", "phase_completed"));
        let delivered_third = bus.emit(evt("wf-1", "workflow_completed"));

        assert_eq!(delivered_first, 1);
        assert_eq!(delivered_second, 1);
        assert_eq!(delivered_third, 0, "third emit must be dropped (buffer full)");

        let a = unwrap_event(rx.recv().await.unwrap());
        let b = unwrap_event(rx.recv().await.unwrap());
        assert_eq!(a.kind, "phase_started");
        assert_eq!(b.kind, "phase_completed");
    }

    #[tokio::test]
    async fn close_subscription_delivers_terminal_item_and_removes_slot() {
        let bus = WorkflowEventBroadcaster::new();
        let (id, mut rx) = bus.subscribe(WorkflowEventFilter::default());
        assert_eq!(bus.subscriber_count(), 1);

        assert!(bus.close_subscription(id, "manual"));
        assert_eq!(bus.subscriber_count(), 0);

        match rx.recv().await.expect("close item must arrive") {
            SubscriberItem::Closed { reason } => assert_eq!(reason, "manual"),
            SubscriberItem::Event(_) => panic!("expected Closed"),
        }
        assert!(rx.recv().await.is_none(), "channel must end after close");
    }

    #[tokio::test]
    async fn terminal_workflow_kind_auto_closes_targeted_subscriber() {
        let bus = WorkflowEventBroadcaster::new();
        let (_targeted_id, mut rx_targeted) =
            bus.subscribe(WorkflowEventFilter { workflow_id: Some("wf-1".into()), kinds: None });
        let (_open_id, mut rx_open) = bus.subscribe(WorkflowEventFilter::default());

        bus.emit(evt("wf-1", "workflow_completed"));

        let first = rx_targeted.recv().await.expect("event must arrive");
        match first {
            SubscriberItem::Event(e) => assert_eq!(e.kind, "workflow_completed"),
            SubscriberItem::Closed { .. } => panic!("event before close"),
        }
        let second = rx_targeted.recv().await.expect("close must follow");
        match second {
            SubscriberItem::Closed { reason } => assert!(reason.contains("wf-1")),
            SubscriberItem::Event(_) => panic!("expected Closed"),
        }
        assert!(rx_targeted.recv().await.is_none(), "channel ends after terminal close");

        let open_first = rx_open.recv().await.expect("event delivered to open subscriber");
        assert!(matches!(open_first, SubscriberItem::Event(_)));
        // Open-ended subscriber must still be live (no auto-close).
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn broadcaster_emit_records_metrics() {
        let before_completed =
            crate::metrics::snapshot().counters.get("workflow_runs_total{status=completed}").copied().unwrap_or(0);
        let before_subscription = crate::metrics::snapshot()
            .counters
            .get("subscription_events_total{kind=workflow_completed}")
            .copied()
            .unwrap_or(0);

        let bus = WorkflowEventBroadcaster::new();
        bus.emit(evt("wf-metric", "workflow_completed"));

        let after = crate::metrics::snapshot();
        assert!(
            after.counters.get("workflow_runs_total{status=completed}").copied().unwrap_or(0) > before_completed,
            "workflow_runs_total{{status=completed}} must increment"
        );
        assert!(
            after.counters.get("subscription_events_total{kind=workflow_completed}").copied().unwrap_or(0)
                > before_subscription,
            "subscription_events_total{{kind=workflow_completed}} must increment"
        );
    }

    #[tokio::test]
    async fn unsubscribe_drops_sender() {
        let bus = WorkflowEventBroadcaster::new();
        let (id, _rx) = bus.subscribe(WorkflowEventFilter::default());
        assert_eq!(bus.subscriber_count(), 1);
        bus.unsubscribe(id);
        assert_eq!(bus.subscriber_count(), 0);
        let delivered = bus.emit(evt("wf-1", "phase_started"));
        assert_eq!(delivered, 0);
    }
}
