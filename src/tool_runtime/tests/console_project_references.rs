use super::run_console_reference_lookup;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn console_refs_external_writer_bounds_ten_waiters_without_replaying_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("refs.db");
    let db = Arc::new(crate::Database::open(&path).unwrap());
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let lane = Arc::new(tokio::sync::Semaphore::new(1));
    let started = Arc::new(AtomicUsize::new(0));
    let (first_started, observe_start) = tokio::sync::oneshot::channel();
    let first = tokio::spawn({
        let db = db.clone();
        let lane = lane.clone();
        let started = started.clone();
        async move {
            run_console_reference_lookup(lane, Duration::from_millis(30), move || {
                started.fetch_add(1, Ordering::SeqCst);
                let _ = first_started.send(());
                db.get_or_create_project_references(
                    "principal",
                    &[("agent:r:p", &format!("wc_projroot_{}", "1".repeat(64)))],
                    1,
                )
            })
            .await
        }
    });
    observe_start.await.unwrap();
    let mut tasks = vec![first];
    for _ in 1..10 {
        let db = db.clone();
        let lane = lane.clone();
        let started = started.clone();
        tasks.push(tokio::spawn(async move {
            run_console_reference_lookup(lane, Duration::from_millis(30), move || {
                started.fetch_add(1, Ordering::SeqCst);
                db.get_or_create_project_references(
                    "principal",
                    &[("agent:r:p", &format!("wc_projroot_{}", "1".repeat(64)))],
                    1,
                )
            })
            .await
        }));
    }
    // A five-second SQLite busy timeout must not multiply by the queued
    // callers. Each optional result expires while the external writer is held.
    tokio::time::timeout(Duration::from_secs(2), async {
        for task in tasks {
            assert!(task.await.unwrap().is_none());
        }
    })
    .await
    .unwrap();
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert!(
        lane.try_acquire().is_err(),
        "timed-out worker still owns admission"
    );
    writer.execute_batch("COMMIT").unwrap();
    let permit = tokio::time::timeout(Duration::from_secs(2), lane.acquire())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
    let record = db
        .lookup_project_reference("principal", 1)
        .unwrap()
        .unwrap();
    assert_eq!(record.canonical_project_id, "agent:r:p");
    assert!(db
        .lookup_project_reference("principal", 2)
        .unwrap()
        .is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn console_refs_cancelled_caller_cannot_release_the_blocking_worker_slot() {
    let lane = Arc::new(tokio::sync::Semaphore::new(1));
    let (started, observed) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let task = tokio::spawn({
        let lane = lane.clone();
        async move {
            run_console_reference_lookup(lane, Duration::from_secs(5), move || {
                let _ = started.send(());
                released.recv_timeout(Duration::from_secs(3)).unwrap();
            })
            .await
        }
    });
    observed.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(lane.try_acquire().is_err());
    assert!(
        run_console_reference_lookup(lane.clone(), Duration::from_millis(20), || panic!(
            "queued cancelled work must not start"
        ),)
        .await
        .is_none()
    );
    release.send(()).unwrap();
    assert_eq!(
        run_console_reference_lookup(lane, Duration::from_secs(2), || 42,).await,
        Some(42)
    );
}
