//! One admitted blocking history read at a time; waiting never occupies a Tokio
//! worker and cancellation cannot create an unbounded queue of detached reads.
//! The permit travels with the blocking closure until it actually finishes.
use super::RuntimeConsoleError;
use std::sync::Arc;
use webcodex_store::{
    models::{
        WindowActivityEventRecord, WindowActivitySummaryRecord, WindowSessionLinkSummaryRecord,
        WindowWorkflowSessionSummaryRecord,
    },
    Database,
};

static HISTORY_READ: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

pub(super) async fn run<T: Send + 'static>(
    db: &Arc<Database>,
    read: impl FnOnce(&Database) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, RuntimeConsoleError> {
    let permit = HISTORY_READ
        .acquire()
        .await
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let db = Arc::clone(db);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read(&db)
    })
    .await
    .map_err(|_| RuntimeConsoleError::Internal)?
    .map_err(|_| RuntimeConsoleError::Internal)
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
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn blocking_history_does_not_block_the_runtime_or_spawn_unbounded_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&tmp.path().join("lane.db")).unwrap());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first_db = db.clone();
        let first = tokio::spawn(async move {
            run(&first_db, move |_| {
                let _ = started_tx.send(());
                release_rx.recv_timeout(std::time::Duration::from_secs(5))?;
                Ok(())
            })
            .await
        });
        started_rx.await.unwrap();
        // If history were executed synchronously the current-thread runtime
        // could not reach this point before the deliberate blocking read ends.
        let second_db = db.clone();
        let second = tokio::spawn(async move { run(&second_db, move |_| Ok(())).await });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());
        second.abort();
        db.get_or_create_project_reference(
            "principal",
            "agent:r:p",
            &format!("wc_projroot_{}", "a".repeat(64)),
            1,
        )
        .unwrap();
        release_tx.send(()).unwrap();
        first.await.unwrap().unwrap();
        run(&db, |_| Ok(())).await.unwrap();
    }
}
