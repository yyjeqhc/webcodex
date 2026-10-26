//! One admitted blocking history read at a time; waiting never occupies a Tokio
//! worker and cancellation cannot create an unbounded queue of detached reads.
//! The permit travels with the blocking closure until it actually finishes.
use super::RuntimeConsoleError;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use webcodex_store::HistoryReadBudget;
use webcodex_store::{
    models::{WindowActivityEventRecord, WindowSessionLinkSummaryRecord},
    Database,
};

static HISTORY_READ: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));
const HISTORY_BUDGET: Duration = Duration::from_secs(5);

/// Fixed call-site phase/outcome labels only. Drop records cancelled futures as
/// well as normal completion, including requests still waiting for admission.
pub(super) struct Phase {
    started: Instant,
    phase: &'static str,
    outcome: &'static str,
}
impl Phase {
    pub(super) fn start(phase: &'static str) -> Self {
        Self {
            started: Instant::now(),
            phase,
            outcome: "cancelled",
        }
    }
    pub(super) fn finish(&mut self, outcome: &'static str) {
        self.outcome = outcome;
    }
}
impl Drop for Phase {
    fn drop(&mut self) {
        tracing::debug!(target: "webcodex::phase", phase=self.phase, outcome=self.outcome,
            elapsed_ms=self.started.elapsed().as_secs_f64()*1000.0, "Console read phase");
    }
}

fn record_worker_diagnostics(budget: &HistoryReadBudget, outcome: &'static str) {
    let diagnostics = budget.diagnostics();
    tracing::debug!(target: "webcodex::phase", phase="console_history_store", outcome,
        sql_statements=diagnostics.sql_statements,
        sql_ms=diagnostics.sql_duration.as_secs_f64()*1000.0,
        connection_attempts=diagnostics.connection_attempts,
        connection_wait_ms=diagnostics.connection_wait.as_secs_f64()*1000.0,
        dirty_repair_attempts=diagnostics.dirty_repair_attempts,
        dirty_windows_repaired=diagnostics.dirty_windows_repaired,
        dirty_repair_ms=diagnostics.dirty_repair_duration.as_secs_f64()*1000.0,
        "historical Store work completed");
}

struct CancelOnDrop(HistoryReadBudget);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn exhausted() -> RuntimeConsoleError {
    RuntimeConsoleError::Request {
        status: 503,
        message: "Historical read budget exhausted; retry the read",
    }
}

pub(super) async fn run<T: Send + 'static>(
    db: &Arc<Database>,
    read: impl FnOnce(&Database) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, RuntimeConsoleError> {
    run_bounded(db, HISTORY_READ.clone(), HISTORY_BUDGET, read).await
}

async fn run_bounded<T: Send + 'static>(
    db: &Arc<Database>,
    lane: Arc<tokio::sync::Semaphore>,
    timeout: Duration,
    read: impl FnOnce(&Database) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, RuntimeConsoleError> {
    let mut phase = Phase::start("console_history");
    let deadline = Instant::now() + timeout;
    // Opt in before offloading so an unobserved Console read does not install
    // SQLite profile callbacks. Explicit Store diagnostics remain enabled by default.
    let profile_sql = tracing::enabled!(target: "webcodex::phase", tracing::Level::DEBUG);
    let budget = HistoryReadBudget::until(deadline).with_sql_profile(profile_sql);
    let _cancel = CancelOnDrop(budget.clone());
    let until = tokio::time::Instant::from_std(deadline);
    let mut admission = Phase::start("console_history_lane_queue");
    let permit = match tokio::time::timeout_at(until, lane.acquire_owned()).await {
        Ok(Ok(permit)) => {
            admission.finish("admitted");
            permit
        }
        Err(_) => {
            admission.finish("budget_exhausted");
            phase.finish("budget_exhausted");
            return Err(exhausted());
        }
        Ok(Err(_)) => {
            admission.finish("closed");
            phase.finish("error");
            return Err(RuntimeConsoleError::Internal);
        }
    };
    drop(admission);
    let db = Arc::clone(db);
    let worker_budget = budget.clone();
    let mut blocking_queue = Phase::start("console_history_blocking_queue");
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let span = tracing::Span::current();
    let result = tokio::time::timeout_at(
        until,
        tokio::task::spawn_blocking(move || {
            // The permit is released only when the actual worker has unwound. An
            // aborted HTTP future cancels only this budget, never the next reader.
            let _permit = permit;
            tracing::dispatcher::with_default(&dispatch, || {
                span.in_scope(|| {
                    blocking_queue.finish(if worker_budget.exhausted() {
                        "budget_exhausted"
                    } else {
                        "started"
                    });
                    drop(blocking_queue);
                    let mut worker = Phase::start("console_history_worker");
                    let result = db.with_history_read_budget(worker_budget.clone(), read);
                    let outcome = if worker_budget.exhausted() {
                        "budget_exhausted"
                    } else if result.is_ok() {
                        "ok"
                    } else {
                        "error"
                    };
                    worker.finish(outcome);
                    record_worker_diagnostics(&worker_budget, outcome);
                    result
                })
            })
        }),
    )
    .await;
    let result = match result {
        Err(_) => Err(exhausted()),
        Ok(Err(_)) => Err(RuntimeConsoleError::Internal),
        Ok(Ok(Err(_))) if budget.exhausted() => Err(exhausted()),
        Ok(Ok(result)) => result.map_err(|_| RuntimeConsoleError::Internal),
    };
    phase.finish(if budget.exhausted() {
        "budget_exhausted"
    } else if result.is_ok() {
        "ok"
    } else {
        "error"
    });
    result
}
fn owned(principal: Option<(&str, &str)>) -> Option<(String, String)> {
    principal.map(|(kind, id)| (kind.to_owned(), id.to_owned()))
}
fn borrowed(principal: &Option<(String, String)>) -> Option<(&str, &str)> {
    principal
        .as_ref()
        .map(|(kind, id)| (kind.as_str(), id.as_str()))
}
/// Synchronous page read under the caller's admitted history budget.
pub(super) fn read_events(
    db: &Database,
    key: &str,
    principal: Option<(&str, &str)>,
    limit: usize,
    composition: bool,
) -> anyhow::Result<Vec<WindowActivityEventRecord>> {
    if composition {
        db.list_window_activity_events_with_code_mode_composition(key, principal, limit)
    } else {
        db.list_window_activity_events(key, principal, limit)
    }
}

pub(super) async fn events(
    db: &Arc<Database>,
    key: &str,
    principal: Option<(&str, &str)>,
    limit: usize,
    composition: bool,
) -> Result<Vec<WindowActivityEventRecord>, RuntimeConsoleError> {
    let key = key.to_owned();
    let principal = owned(principal);
    run(db, move |db| {
        read_events(db, &key, borrowed(&principal), limit, composition)
    })
    .await
}
pub(super) async fn linked_windows(
    db: &Arc<Database>,
    session: &str,
    principal: Option<(&str, &str)>,
    limit: usize,
) -> Result<Vec<WindowSessionLinkSummaryRecord>, RuntimeConsoleError> {
    let session = session.to_owned();
    let principal = owned(principal);
    run(db, move |db| {
        db.list_session_linked_windows(&session, borrowed(&principal), limit)
    })
    .await
}

#[cfg(test)]
mod tests;
