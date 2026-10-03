//! One admitted blocking history read at a time; waiting never occupies a Tokio
//! worker and cancellation cannot create an unbounded queue of detached reads.
//! The permit travels with the blocking closure until it actually finishes.
use super::RuntimeConsoleError;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use webcodex_store::HistoryReadBudget;
use webcodex_store::{
    models::{
        WindowActivityEventRecord, WindowActivitySummaryRecord, WindowSessionLinkSummaryRecord,
        WindowWorkflowSessionSummaryRecord,
    },
    Database,
};

static HISTORY_READ: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));
const HISTORY_BUDGET: Duration = Duration::from_secs(5);

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
    let started = Instant::now();
    let deadline = started + timeout;
    let budget = HistoryReadBudget::until(deadline);
    let _cancel = CancelOnDrop(budget.clone());
    let until = tokio::time::Instant::from_std(deadline);
    let permit = tokio::time::timeout_at(until, lane.acquire_owned())
        .await
        .map_err(|_| exhausted())?
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let db = Arc::clone(db);
    let worker_budget = budget.clone();
    let result = tokio::time::timeout_at(
        until,
        tokio::task::spawn_blocking(move || {
            // The permit is released only when the actual worker has unwound. An
            // aborted HTTP future cancels only this budget, never the next reader.
            let _permit = permit;
            db.with_history_read_budget(worker_budget, read)
        }),
    )
    .await;
    tracing::debug!(target: "webcodex::phase", phase="console_history", elapsed_ms=started.elapsed().as_secs_f64()*1000.0,
        cancelled=budget.exhausted(), "historical read completed");
    match result {
        Err(_) => Err(exhausted()),
        Ok(Err(_)) => Err(RuntimeConsoleError::Internal),
        Ok(Ok(Err(_))) if budget.exhausted() => Err(exhausted()),
        Ok(Ok(result)) => result.map_err(|_| RuntimeConsoleError::Internal),
    }
}
fn owned(principal: Option<(&str, &str)>) -> Option<(String, String)> {
    principal.map(|(kind, id)| (kind.to_owned(), id.to_owned()))
}
fn borrowed(principal: &Option<(String, String)>) -> Option<(&str, &str)> {
    principal
        .as_ref()
        .map(|(kind, id)| (kind.as_str(), id.as_str()))
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
        if composition {
            db.list_window_activity_events_with_code_mode_composition(
                &key,
                borrowed(&principal),
                limit,
            )
        } else {
            db.list_window_activity_events(&key, borrowed(&principal), limit)
        }
    })
    .await
}
pub(super) async fn summary(
    db: &Arc<Database>,
    key: &str,
    principal: Option<(&str, &str)>,
) -> Result<Option<WindowActivitySummaryRecord>, RuntimeConsoleError> {
    let key = key.to_owned();
    let principal = owned(principal);
    run(db, move |db| {
        db.get_window_activity_summary(&key, borrowed(&principal))
    })
    .await
}
pub(super) async fn relations(
    db: &Arc<Database>,
    key: &str,
    principal: Option<(&str, &str)>,
    limit: usize,
) -> Result<Vec<WindowWorkflowSessionSummaryRecord>, RuntimeConsoleError> {
    let key = key.to_owned();
    let principal = owned(principal);
    run(db, move |db| {
        db.list_window_workflow_sessions(&key, borrowed(&principal), limit)
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
