//! Broadcaster for daemon-side `workflow/events` subscriptions.
//!
//! Subscribers register an optional [`WorkflowEventFilter`] and receive a
//! non-blocking [`mpsc::Receiver`] of [`WorkflowEvent`]s that pass the
//! filter. Slow subscribers do NOT block the emitter — when a subscriber's
//! buffer is full the event is dropped for *that* subscriber only (with a
//! `tracing::warn!`) and the broadcaster continues to fan out to the
//! remaining subscribers.
//!
//! Subscriptions are per-connection: they live only as long as the
//! control-socket connection that opened them. The daemon does not persist
//! subscriptions across restarts.

use std::sync::Arc;

use animus_control_protocol::types::WorkflowEvent;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;

const DEFAULT_SUBSCRIBER_BUFFER: usize = 256;

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

struct SubscriberSlot {
    id: SubscriptionId,
    sender: mpsc::Sender<WorkflowEvent>,
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
    /// that yields filtered events until the subscription is dropped or
    /// [`Self::unsubscribe`] is called.
    pub fn subscribe(self: &Arc<Self>, filter: WorkflowEventFilter) -> (SubscriptionId, mpsc::Receiver<WorkflowEvent>) {
        self.subscribe_with_buffer(filter, DEFAULT_SUBSCRIBER_BUFFER)
    }

    pub fn subscribe_with_buffer(
        self: &Arc<Self>,
        filter: WorkflowEventFilter,
        buffer: usize,
    ) -> (SubscriptionId, mpsc::Receiver<WorkflowEvent>) {
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
        let mut delivered = 0usize;
        let mut closed_ids: Vec<SubscriptionId> = Vec::new();
        {
            let guard = self.subscribers.lock().expect("workflow event subscribers mutex poisoned");
            for slot in guard.iter() {
                if !slot.filter.matches(&event) {
                    continue;
                }
                match slot.sender.try_send(event.clone()) {
                    Ok(()) => delivered += 1,
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
            let e = rx_all.recv().await.expect("rx_all closed");
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
            let e = rx_wf1.recv().await.expect("rx_wf1 closed");
            wf1_seen.push(e.kind);
        }
        assert_eq!(wf1_seen, vec!["phase_started".to_string(), "phase_completed".to_string()]);

        let mut kind_seen = Vec::new();
        for _ in 0..2 {
            let e = rx_kind.recv().await.expect("rx_kind closed");
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

        let a = rx.recv().await.unwrap();
        let b = rx.recv().await.unwrap();
        assert_eq!(a.kind, "phase_started");
        assert_eq!(b.kind, "phase_completed");
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
