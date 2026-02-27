#![cfg_attr(not(test), allow(dead_code))]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, Notify, OwnedSemaphorePermit, Semaphore};

type AgentSlotFuture = Pin<Box<dyn Future<Output = AgentSlotResult> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentSlotContext {
    pub(super) slot_id: String,
    pub(super) workflow_id: Option<String>,
    pub(super) task_id: Option<String>,
    pub(super) phase_id: Option<String>,
}

impl AgentSlotContext {
    pub(super) fn new(slot_id: impl Into<String>) -> Self {
        Self {
            slot_id: slot_id.into(),
            workflow_id: None,
            task_id: None,
            phase_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AgentSlotOutcome {
    Success,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentSlotResult {
    pub(super) context: AgentSlotContext,
    pub(super) outcome: AgentSlotOutcome,
}

impl AgentSlotResult {
    pub(super) fn success(context: AgentSlotContext) -> Self {
        Self {
            context,
            outcome: AgentSlotOutcome::Success,
        }
    }

    pub(super) fn failed(context: AgentSlotContext, message: impl Into<String>) -> Self {
        Self {
            context,
            outcome: AgentSlotOutcome::Failed(message.into()),
        }
    }

    pub(super) fn is_failed(&self) -> bool {
        matches!(self.outcome, AgentSlotOutcome::Failed(_))
    }
}

pub(super) struct AgentSlot {
    context: AgentSlotContext,
    run: AgentSlotFuture,
}

impl AgentSlot {
    pub(super) fn new<Fut>(context: AgentSlotContext, run: Fut) -> Self
    where
        Fut: Future<Output = AgentSlotResult> + Send + 'static,
    {
        Self {
            context,
            run: Box::pin(run),
        }
    }

    fn context(&self) -> &AgentSlotContext {
        &self.context
    }

    fn into_future(self) -> AgentSlotFuture {
        self.run
    }
}

struct AgentPoolState {
    active_count: AtomicUsize,
    total_spawned: AtomicUsize,
    total_completed: AtomicUsize,
    total_failed: AtomicUsize,
    completion_tx: mpsc::UnboundedSender<AgentSlotResult>,
    drained_notify: Notify,
}

pub(super) struct AgentPool {
    semaphore: Arc<Semaphore>,
    accepting: AtomicBool,
    state: Arc<AgentPoolState>,
    completion_rx: Mutex<Option<mpsc::UnboundedReceiver<AgentSlotResult>>>,
}

impl AgentPool {
    pub(super) fn new(pool_size: usize) -> Self {
        let (completion_tx, completion_rx) = mpsc::unbounded_channel();
        Self {
            semaphore: Arc::new(Semaphore::new(pool_size)),
            accepting: AtomicBool::new(true),
            state: Arc::new(AgentPoolState {
                active_count: AtomicUsize::new(0),
                total_spawned: AtomicUsize::new(0),
                total_completed: AtomicUsize::new(0),
                total_failed: AtomicUsize::new(0),
                completion_tx,
                drained_notify: Notify::new(),
            }),
            completion_rx: Mutex::new(Some(completion_rx)),
        }
    }

    pub(super) fn take_completion_receiver(
        &self,
    ) -> Option<mpsc::UnboundedReceiver<AgentSlotResult>> {
        let mut guard = self
            .completion_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.take()
    }

    pub(super) async fn spawn_agent(&self, slot: AgentSlot) -> bool {
        if !self.accepting.load(Ordering::Acquire) {
            return false;
        }

        let permit = match self.semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return false,
        };

        if !self.accepting.load(Ordering::Acquire) {
            drop(permit);
            return false;
        }

        self.spawn_with_permit(slot, permit);
        true
    }

    pub(super) fn try_spawn(&self, slot: AgentSlot) -> bool {
        if !self.accepting.load(Ordering::Acquire) {
            return false;
        }

        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => return false,
        };

        if !self.accepting.load(Ordering::Acquire) {
            drop(permit);
            return false;
        }

        self.spawn_with_permit(slot, permit);
        true
    }

    pub(super) async fn drain(&self) {
        self.accepting.store(false, Ordering::Release);
        self.semaphore.close();

        loop {
            let notified = self.state.drained_notify.notified();
            if self.active_count() == 0 {
                break;
            }
            notified.await;
        }
    }

    pub(super) fn active_count(&self) -> usize {
        self.state.active_count.load(Ordering::Acquire)
    }

    pub(super) fn is_full(&self) -> bool {
        !self.accepting.load(Ordering::Acquire) || self.semaphore.available_permits() == 0
    }

    pub(super) fn total_spawned(&self) -> usize {
        self.state.total_spawned.load(Ordering::Acquire)
    }

    pub(super) fn total_completed(&self) -> usize {
        self.state.total_completed.load(Ordering::Acquire)
    }

    pub(super) fn total_failed(&self) -> usize {
        self.state.total_failed.load(Ordering::Acquire)
    }

    fn spawn_with_permit(&self, slot: AgentSlot, permit: OwnedSemaphorePermit) {
        self.state.total_spawned.fetch_add(1, Ordering::AcqRel);
        self.state.active_count.fetch_add(1, Ordering::AcqRel);

        let state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let slot_context = slot.context().clone();
            let join_result = tokio::spawn(async move { slot.into_future().await }).await;
            let completion = match join_result {
                Ok(result) => result,
                Err(error) => AgentSlotResult::failed(
                    slot_context,
                    format!("agent slot panicked or was cancelled: {error}"),
                ),
            };

            if completion.is_failed() {
                state.total_failed.fetch_add(1, Ordering::AcqRel);
            }
            state.total_completed.fetch_add(1, Ordering::AcqRel);

            let _ = state.completion_tx.send(completion);
            drop(permit);

            let previous = state.active_count.fetch_sub(1, Ordering::AcqRel);
            if previous <= 1 {
                state.drained_notify.notify_waiters();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    fn waiting_slot(slot_id: &str, release_rx: oneshot::Receiver<()>, fail: bool) -> AgentSlot {
        let context = AgentSlotContext::new(slot_id.to_string());
        let result_context = context.clone();
        AgentSlot::new(context, async move {
            let _ = release_rx.await;
            if fail {
                AgentSlotResult::failed(result_context, "simulated failure")
            } else {
                AgentSlotResult::success(result_context)
            }
        })
    }

    fn immediate_success_slot(slot_id: &str) -> AgentSlot {
        let context = AgentSlotContext::new(slot_id.to_string());
        AgentSlot::new(
            context.clone(),
            async move { AgentSlotResult::success(context) },
        )
    }

    #[tokio::test]
    async fn try_spawn_is_non_blocking_and_enforces_capacity() {
        let pool = AgentPool::new(2);
        let mut completions = pool
            .take_completion_receiver()
            .expect("completion receiver");

        let (release_one_tx, release_one_rx) = oneshot::channel();
        let (release_two_tx, release_two_rx) = oneshot::channel();

        assert!(pool.try_spawn(waiting_slot("slot-1", release_one_rx, false)));
        assert!(pool.try_spawn(waiting_slot("slot-2", release_two_rx, false)));
        assert_eq!(pool.active_count(), 2);
        assert!(pool.is_full());
        assert!(!pool.try_spawn(immediate_success_slot("slot-3")));

        let _ = release_one_tx.send(());
        let _ = release_two_tx.send(());
        let _ = completions.recv().await;
        let _ = completions.recv().await;
        pool.drain().await;
    }

    #[tokio::test]
    async fn permits_are_released_after_completion() {
        let pool = AgentPool::new(1);
        let mut completions = pool
            .take_completion_receiver()
            .expect("completion receiver");

        let (release_first_tx, release_first_rx) = oneshot::channel();
        assert!(pool.try_spawn(waiting_slot("slot-1", release_first_rx, false)));
        assert!(!pool.try_spawn(immediate_success_slot("slot-2")));

        let _ = release_first_tx.send(());
        let _ = completions.recv().await;

        assert!(pool.try_spawn(immediate_success_slot("slot-3")));
        let result = completions.recv().await.expect("slot completion");
        assert_eq!(result.context.slot_id, "slot-3");

        pool.drain().await;
    }

    #[tokio::test]
    async fn spawn_agent_waits_for_available_permit() {
        let pool = Arc::new(AgentPool::new(1));
        let mut completions = pool
            .take_completion_receiver()
            .expect("completion receiver");

        let (release_first_tx, release_first_rx) = oneshot::channel();
        assert!(pool.try_spawn(waiting_slot("slot-1", release_first_rx, false)));

        let pool_for_spawn = Arc::clone(&pool);
        let spawn_handle = tokio::spawn(async move {
            pool_for_spawn
                .spawn_agent(immediate_success_slot("slot-2"))
                .await
        });

        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!spawn_handle.is_finished());

        let _ = release_first_tx.send(());
        assert!(timeout(Duration::from_secs(1), spawn_handle)
            .await
            .expect("spawn should resolve")
            .expect("spawn join should succeed"));

        let _ = completions.recv().await;
        let _ = completions.recv().await;
        pool.drain().await;
    }

    #[tokio::test]
    async fn pool_tracks_spawn_completion_and_failures() {
        let pool = AgentPool::new(3);
        let mut completions = pool
            .take_completion_receiver()
            .expect("completion receiver");

        assert!(pool.try_spawn(immediate_success_slot("slot-success")));

        let (release_failed_tx, release_failed_rx) = oneshot::channel();
        assert!(pool.try_spawn(waiting_slot("slot-failed", release_failed_rx, true)));
        let _ = release_failed_tx.send(());

        let fail_fast_context = AgentSlotContext::new("slot-fail-fast");
        assert!(
            pool.try_spawn(AgentSlot::new(fail_fast_context.clone(), async move {
                AgentSlotResult::failed(fail_fast_context, "runtime error")
            },))
        );

        pool.drain().await;

        let mut seen = Vec::new();
        for _ in 0..3 {
            let result = completions.recv().await.expect("completion event");
            seen.push(result.context.slot_id);
        }
        seen.sort();
        assert_eq!(seen, vec!["slot-fail-fast", "slot-failed", "slot-success"]);

        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.total_spawned(), 3);
        assert_eq!(pool.total_completed(), 3);
        assert_eq!(pool.total_failed(), 2);
    }

    #[tokio::test]
    async fn drain_stops_admissions_and_waits_for_active_slots() {
        let pool = Arc::new(AgentPool::new(1));
        let mut completions = pool
            .take_completion_receiver()
            .expect("completion receiver");

        let (release_tx, release_rx) = oneshot::channel();
        assert!(pool.try_spawn(waiting_slot("slot-1", release_rx, false)));

        let pool_for_drain = Arc::clone(&pool);
        let drain_handle = tokio::spawn(async move {
            pool_for_drain.drain().await;
        });

        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!drain_handle.is_finished());
        assert!(!pool.try_spawn(immediate_success_slot("slot-after-drain")));

        let _ = release_tx.send(());
        let _ = completions.recv().await;

        timeout(Duration::from_secs(1), drain_handle)
            .await
            .expect("drain should finish")
            .expect("drain join should succeed");
        assert_eq!(pool.active_count(), 0);
    }
}
