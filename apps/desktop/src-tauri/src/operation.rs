use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::error::{DesktopError, DesktopResult};
use crate::models::{DesktopOperationKind, DesktopOperationPhase, DesktopOperationSnapshot};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

// This is an outer backstop for cooperative Desktop operations, including
// multi-step setup. Individual commands keep their shorter deadlines.
const OPERATION_BUDGET: Duration = Duration::from_secs(10 * 60);
const STOP_OBSERVATION_BUDGET: Duration = Duration::from_secs(30);

pub(crate) use webcodex_environment::unified_update::CancellationSignal;

#[derive(Clone)]
pub(crate) struct CancellationContext {
    operation: CancellationSignal,
    shutdown: CancellationSignal,
    deadline: Option<Instant>,
}

impl CancellationContext {
    pub(crate) fn new(operation: CancellationSignal, shutdown: CancellationSignal) -> Self {
        Self {
            operation,
            shutdown,
            deadline: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn never() -> Self {
        Self {
            operation: CancellationSignal::new(),
            shutdown: CancellationSignal::new(),
            deadline: None,
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.deadline.is_some_and(|at| Instant::now() >= at)
            || self.shutdown.is_cancelled()
            || self.operation.is_cancelled()
    }

    pub(crate) fn check(&self) -> DesktopResult<()> {
        if self.is_cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }

    pub(crate) async fn cancelled(&self) {
        let signals = async {
            tokio::select! {
                _ = self.operation.cancelled() => {},
                _ = self.shutdown.cancelled() => {},
            }
        };
        if let Some(at) = self.deadline {
            let _ = tokio::time::timeout_at(at, signals).await;
        } else {
            signals.await;
        }
    }
}

pub(crate) fn cancelled_error() -> DesktopError {
    DesktopError::new(
        "desktop_operation_cancelled",
        "The Desktop operation was stopped before its outcome was fully confirmed",
        "Observe the current runtime state before retrying any interrupted step.",
    )
}

pub(crate) fn core_unavailable() -> DesktopError {
    DesktopError::new(
        "desktop_core_unavailable",
        "Desktop cannot verify its internal operation state",
        "Fully quit and reopen Desktop, then check the runtime and Activity before repeating the operation.",
    )
}

pub(crate) fn busy_error(current: &DesktopOperationSnapshot) -> DesktopError {
    let error = if current.phase == DesktopOperationPhase::Overdue {
        DesktopError::new(
            "desktop_operation_unsettled",
            "The Desktop operation has not confirmed completion within its time budget",
            "Check Activity and the affected service before taking further action.",
        )
    } else {
        DesktopError::new(
            "desktop_operation_busy",
            "Another Desktop operation is still running",
            "Check the current operation in Desktop.",
        )
    };
    error
        .with_details(serde_json::json!({
            "operation_id": current.id,
            "operation_kind": current.kind.as_str(),
        }))
        .with_operation_context(Some(&current.id), None, None, None)
}

struct ActiveOperation {
    snapshot: DesktopOperationSnapshot,
    cancellation: CancellationSignal,
    report_delay_at: Instant,
}

impl ActiveOperation {
    fn snapshot(&self) -> DesktopOperationSnapshot {
        let mut snapshot = self.snapshot.clone();
        if Instant::now() >= self.report_delay_at {
            snapshot.phase = DesktopOperationPhase::Overdue;
            // A cooperative operation's deadline or an explicit Stop has
            // already requested cancellation. Its owner still settles effects.
            snapshot.cancellable = false;
        }
        snapshot
    }
}

enum OperationSlot {
    Idle,
    Active(ActiveOperation),
    Interrupted(DesktopError),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdmissionPhase {
    Preparing,
    Dispatched,
    Finished,
}

pub(crate) struct OperationAdmission<'a> {
    controller: &'a OperationController,
    pub(crate) id: String,
    pub(crate) kind: DesktopOperationKind,
    cancellation: CancellationSignal,
    cancel_at: Option<Instant>,
    phase: AdmissionPhase,
}

impl OperationAdmission<'_> {
    pub(crate) fn cancellation_context(&self, shutdown: CancellationSignal) -> CancellationContext {
        CancellationContext {
            operation: self.cancellation.clone(),
            shutdown,
            deadline: self.cancel_at,
        }
    }

    pub(crate) fn mark_dispatched(&mut self) {
        self.phase = AdmissionPhase::Dispatched;
    }

    pub(crate) fn finish<T>(mut self, result: &DesktopResult<T>) {
        self.controller.finish(&self.id, result);
        self.phase = AdmissionPhase::Finished;
    }
}

impl Drop for OperationAdmission<'_> {
    fn drop(&mut self) {
        if self.phase != AdmissionPhase::Finished {
            self.cancellation.cancel();
            self.controller
                .abandon(&self.id, self.phase == AdmissionPhase::Dispatched);
        }
    }
}

pub(crate) struct OperationController {
    active: Mutex<OperationSlot>,
    next_id: AtomicU64,
    activity: ActivityLog,
}

impl OperationController {
    pub(crate) fn generation(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed)
    }

    pub(crate) fn new(activity: ActivityLog) -> Self {
        Self {
            active: Mutex::new(OperationSlot::Idle),
            next_id: AtomicU64::new(0),
            activity,
        }
    }

    pub(crate) fn admit(
        &self,
        kind: DesktopOperationKind,
        cancellable: bool,
    ) -> DesktopResult<OperationAdmission<'_>> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*active {
            OperationSlot::Active(current) => return Err(busy_error(&current.snapshot())),
            OperationSlot::Interrupted(error) => return Err(error.clone()),
            OperationSlot::Idle => {}
        }

        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = format!("desktop-operation-{sequence}");
        let snapshot = DesktopOperationSnapshot {
            id: id.clone(),
            kind,
            phase: DesktopOperationPhase::Running,
            started_at_ms: now_ms(),
            cancellable,
        };
        let cancellation = CancellationSignal::new();
        self.activity.push(
            ActivityEventKind::OperationStarted,
            "desktop",
            ActivityLevel::Info,
            format!("Desktop operation started: {}", kind.as_str()),
        );
        let deadline = Instant::now() + OPERATION_BUDGET;
        *active = OperationSlot::Active(ActiveOperation {
            report_delay_at: deadline,
            snapshot,
            cancellation: cancellation.clone(),
        });
        Ok(OperationAdmission {
            controller: self,
            cancel_at: cancellable.then_some(deadline),
            phase: AdmissionPhase::Preparing,
            id,
            kind,
            cancellation,
        })
    }

    pub(crate) fn current(&self) -> Option<DesktopOperationSnapshot> {
        self.observe().ok().flatten()
    }

    pub(crate) fn observe(&self) -> DesktopResult<Option<DesktopOperationSnapshot>> {
        let slot = self.active.lock().unwrap_or_else(|p| p.into_inner());
        match &*slot {
            OperationSlot::Idle => Ok(None),
            OperationSlot::Active(current) => Ok(Some(current.snapshot())),
            OperationSlot::Interrupted(error) => Err(error.clone()),
        }
    }

    pub(crate) fn missing_core_error(&self) -> DesktopError {
        match self.observe() {
            Ok(Some(current)) => busy_error(&current),
            Ok(None) => core_unavailable(),
            Err(error) => error,
        }
    }

    pub(crate) fn cancel(&self, observed_id: &str) -> DesktopResult<DesktopOperationSnapshot> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let OperationSlot::Active(current) = &mut *active else {
            return Err(operation_not_current(observed_id, None));
        };
        if current.snapshot.id != observed_id {
            return Err(operation_not_current(
                observed_id,
                Some(&current.snapshot.id),
            ));
        }
        if !current.snapshot.cancellable {
            return Err(DesktopError::new(
                "desktop_operation_not_cancellable",
                "The observed Desktop operation cannot be interrupted safely",
                "Wait for the current cleanup operation to finish.",
            ));
        }
        if current.snapshot.phase != DesktopOperationPhase::Cancelling {
            current.snapshot.phase = DesktopOperationPhase::Cancelling;
            current.report_delay_at = current
                .report_delay_at
                .min(Instant::now() + STOP_OBSERVATION_BUDGET);
            current.cancellation.cancel();
            self.activity.push(
                ActivityEventKind::OperationCancelRequested,
                "desktop",
                ActivityLevel::Info,
                format!(
                    "Stop requested for Desktop operation: {}",
                    current.snapshot.kind.as_str()
                ),
            );
        }
        Ok(current.snapshot())
    }

    pub(crate) fn cancel_active_for_shutdown(&self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let OperationSlot::Active(current) = &mut *active else {
            return;
        };
        if current.snapshot.phase != DesktopOperationPhase::Cancelling {
            current.snapshot.phase = DesktopOperationPhase::Cancelling;
            current.report_delay_at = current
                .report_delay_at
                .min(Instant::now() + STOP_OBSERVATION_BUDGET);
            self.activity.push(
                ActivityEventKind::OperationCancelRequested,
                "desktop",
                ActivityLevel::Info,
                format!(
                    "Shutdown requested stop for Desktop operation: {}",
                    current.snapshot.kind.as_str()
                ),
            );
        }
        current.cancellation.cancel();
    }

    fn finish<T>(&self, operation_id: &str, result: &DesktopResult<T>) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let OperationSlot::Active(current) = &*active else {
            return;
        };
        if current.snapshot.id != operation_id {
            return;
        }
        if let Err(error) = result {
            if error.code == "desktop_operation_cancelled" {
                self.activity.push(
                    ActivityEventKind::OperationCancelled,
                    "desktop",
                    ActivityLevel::Info,
                    format!(
                        "Desktop operation stopped: {}",
                        current.snapshot.kind.as_str()
                    ),
                );
            } else {
                self.activity.push(
                    ActivityEventKind::OperationFailed,
                    "desktop",
                    ActivityLevel::Error,
                    format!(
                        "Desktop operation failed: {} ({})",
                        current.snapshot.kind.as_str(),
                        error.code
                    ),
                );
            }
        }
        *active = OperationSlot::Idle;
    }

    fn abandon(&self, id: &str, dispatched: bool) {
        let mut slot = self.active.lock().unwrap_or_else(|p| p.into_inner());
        let OperationSlot::Active(current) = &*slot else {
            return;
        };
        if current.snapshot.id != id {
            return;
        }
        self.activity.push(
            ActivityEventKind::OperationFailed,
            "desktop",
            ActivityLevel::Error,
            format!(
                "Desktop operation owner exited: {}",
                current.snapshot.kind.as_str()
            ),
        );
        *slot = if dispatched {
            // A worker or an external effect can outlive its caller. A new
            // Desktop process must re-observe the durable owners before retry.
            OperationSlot::Interrupted(core_unavailable().with_operation_context(
                Some(id),
                None,
                None,
                None,
            ))
        } else {
            OperationSlot::Idle
        };
    }

    pub(crate) async fn wait_until_idle(&self, deadline: Instant) -> bool {
        loop {
            if self.current().is_none() {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            tokio::time::sleep(std::cmp::min(
                std::time::Duration::from_millis(20),
                deadline - now,
            ))
            .await;
        }
    }
}

fn operation_not_current(observed_id: &str, current_id: Option<&str>) -> DesktopError {
    DesktopError::new(
        "desktop_operation_not_current",
        "The observed Desktop operation is no longer the current operation",
        "Refresh Desktop state before trying to stop an operation again.",
    )
    .with_details(serde_json::json!({
        "observed_operation_id": observed_id,
        "current_operation_id": current_id,
    }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn overdue_owners_keep_their_slot_until_settlement() {
        for cancellable in [true, false] {
            let controller = OperationController::new(ActivityLog::default());
            let mut operation = controller
                .admit(DesktopOperationKind::LocalSetup, cancellable)
                .unwrap();
            operation.mark_dispatched();
            let context = operation.cancellation_context(CancellationSignal::new());
            assert!(!context.is_cancelled());
            tokio::time::advance(OPERATION_BUDGET).await;
            assert_eq!(context.is_cancelled(), cancellable);
            if cancellable {
                context.cancelled().await;
            }
            let observed = controller.current().unwrap();
            assert_eq!(observed.phase, DesktopOperationPhase::Overdue);
            assert!(!observed.cancellable);
            let competing = controller
                .admit(DesktopOperationKind::LocalSetup, true)
                .err()
                .unwrap();
            assert_eq!(competing.code, "desktop_operation_unsettled");
            assert_eq!(
                competing.diagnostic_ref().unwrap().operation_id.as_ref(),
                Some(&operation.id)
            );
            operation.finish(&Ok::<_, DesktopError>(()));
            assert!(controller
                .admit(DesktopOperationKind::LocalSetup, true)
                .is_ok());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_stop_requests_do_not_extend_a_stalled_cleanup() {
        let controller = OperationController::new(ActivityLog::default());
        let operation = controller
            .admit(DesktopOperationKind::RuntimeResume, true)
            .unwrap();
        controller.cancel(&operation.id).unwrap();
        tokio::time::advance(STOP_OBSERVATION_BUDGET / 2).await;
        controller.cancel(&operation.id).unwrap();
        tokio::time::advance(STOP_OBSERVATION_BUDGET / 2).await;
        assert_eq!(
            controller.current().unwrap().phase,
            DesktopOperationPhase::Overdue
        );
        operation.finish(&Err::<(), _>(cancelled_error()));
        assert!(controller.observe().unwrap().is_none());
    }

    #[test]
    fn stale_cancel_never_targets_the_newer_operation() {
        let controller = OperationController::new(ActivityLog::default());
        let first = controller
            .admit(DesktopOperationKind::LocalSetup, true)
            .unwrap();
        controller.cancel(&first.id).unwrap();
        let first_result: DesktopResult<()> = Err(cancelled_error());
        let first_id = first.id.clone();
        first.finish(&first_result);

        let second = controller
            .admit(DesktopOperationKind::QuickShareStart, true)
            .unwrap();
        let error = controller.cancel(&first_id).unwrap_err();
        assert_eq!(error.code, "desktop_operation_not_current");
        assert_eq!(controller.current().unwrap().id, second.id);
        assert!(!second.cancellation.is_cancelled());
    }

    #[test]
    fn cancelling_operation_retains_the_mutation_slot_until_finish() {
        let controller = OperationController::new(ActivityLog::default());
        let first = controller
            .admit(DesktopOperationKind::LocalSetup, true)
            .unwrap();
        controller.cancel(&first.id).unwrap();
        let busy = controller
            .admit(DesktopOperationKind::RemoteSetup, true)
            .err()
            .expect("second mutation must fail fast");
        assert_eq!(busy.code, "desktop_operation_busy");

        let first_result: DesktopResult<()> = Err(cancelled_error());
        first.finish(&first_result);
        assert!(controller
            .admit(DesktopOperationKind::RemoteSetup, true)
            .is_ok());
    }
}
