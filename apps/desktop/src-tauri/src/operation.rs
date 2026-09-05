use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::error::{DesktopError, DesktopResult};
use crate::models::{DesktopOperationKind, DesktopOperationPhase, DesktopOperationSnapshot};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::time::Instant;

#[derive(Clone)]
pub(crate) struct CancellationSignal {
    sender: Arc<watch::Sender<bool>>,
}

impl CancellationSignal {
    pub(crate) fn new() -> Self {
        let (sender, _receiver) = watch::channel(false);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub(crate) fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    pub(crate) async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct CancellationContext {
    operation: Option<CancellationSignal>,
    shutdown: CancellationSignal,
}

impl CancellationContext {
    pub(crate) fn new(operation: CancellationSignal, shutdown: CancellationSignal) -> Self {
        Self {
            operation: Some(operation),
            shutdown,
        }
    }

    #[cfg(test)]
    pub(crate) fn never() -> Self {
        Self {
            operation: None,
            shutdown: CancellationSignal::new(),
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.shutdown.is_cancelled()
            || self
                .operation
                .as_ref()
                .is_some_and(CancellationSignal::is_cancelled)
    }

    pub(crate) fn check(&self) -> DesktopResult<()> {
        if self.is_cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }

    pub(crate) async fn cancelled(&self) {
        if let Some(operation) = &self.operation {
            tokio::select! {
                _ = operation.cancelled() => {},
                _ = self.shutdown.cancelled() => {},
            }
        } else {
            self.shutdown.cancelled().await;
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

struct ActiveOperation {
    snapshot: DesktopOperationSnapshot,
    cancellation: CancellationSignal,
}

#[derive(Clone)]
pub(crate) struct OperationAdmission {
    pub(crate) id: String,
    pub(crate) kind: DesktopOperationKind,
    pub(crate) cancellation: CancellationSignal,
}

pub(crate) struct OperationController {
    active: Mutex<Option<ActiveOperation>>,
    next_id: AtomicU64,
    activity: ActivityLog,
}

impl OperationController {
    pub(crate) fn new(activity: ActivityLog) -> Self {
        Self {
            active: Mutex::new(None),
            next_id: AtomicU64::new(0),
            activity,
        }
    }

    pub(crate) fn admit(
        &self,
        kind: DesktopOperationKind,
        cancellable: bool,
    ) -> DesktopResult<OperationAdmission> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(current) = active.as_ref() {
            return Err(DesktopError::new(
                "desktop_operation_busy",
                "Another Desktop operation is still running",
                "Wait for the current operation to finish or stop it before starting another one.",
            )
            .with_details(serde_json::json!({
                "operation_id": current.snapshot.id,
                "operation_kind": current.snapshot.kind.as_str(),
            })));
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
        *active = Some(ActiveOperation {
            snapshot,
            cancellation: cancellation.clone(),
        });
        Ok(OperationAdmission {
            id,
            kind,
            cancellation,
        })
    }

    pub(crate) fn current(&self) -> Option<DesktopOperationSnapshot> {
        self.active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|operation| operation.snapshot.clone())
    }

    pub(crate) fn cancel(&self, observed_id: &str) -> DesktopResult<DesktopOperationSnapshot> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(current) = active.as_mut() else {
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
        Ok(current.snapshot.clone())
    }

    pub(crate) fn cancel_active_for_shutdown(&self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(current) = active.as_mut() else {
            return;
        };
        if current.snapshot.phase != DesktopOperationPhase::Cancelling {
            current.snapshot.phase = DesktopOperationPhase::Cancelling;
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

    pub(crate) fn finish<T>(&self, operation_id: &str, result: &DesktopResult<T>) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(current) = active.as_ref() else {
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
        *active = None;
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

    #[test]
    fn stale_cancel_never_targets_the_newer_operation() {
        let controller = OperationController::new(ActivityLog::default());
        let first = controller
            .admit(DesktopOperationKind::LocalSetup, true)
            .unwrap();
        controller.cancel(&first.id).unwrap();
        let first_result: DesktopResult<()> = Err(cancelled_error());
        controller.finish(&first.id, &first_result);

        let second = controller
            .admit(DesktopOperationKind::QuickShareStart, true)
            .unwrap();
        let error = controller.cancel(&first.id).unwrap_err();
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
        controller.finish(&first.id, &first_result);
        assert!(controller
            .admit(DesktopOperationKind::RemoteSetup, true)
            .is_ok());
    }
}
