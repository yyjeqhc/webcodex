use super::*;

fn fixture() -> (
    tempfile::TempDir,
    Arc<Database>,
    Arc<tokio::sync::Semaphore>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&tmp.path().join("history.db")).unwrap());
    (tmp, db, Arc::new(tokio::sync::Semaphore::new(1)))
}

#[tokio::test(flavor = "current_thread")]
async fn history_admission_deadline_never_starts_queued_sql() {
    let (_tmp, db, lane) = fixture();
    let permit = lane.clone().acquire_owned().await.unwrap();
    let result = run_bounded(
        &db,
        lane.clone(),
        Duration::from_millis(20),
        |_| -> anyhow::Result<()> { panic!("expired queued work must not execute") },
    )
    .await;
    assert!(matches!(
        result,
        Err(RuntimeConsoleError::Request { status: 503, .. })
    ));
    drop(permit);
    assert!(run_bounded(&db, lane, Duration::from_secs(2), |_| Ok(()))
        .await
        .is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn aborted_history_task_keeps_permit_until_owned_worker_exits() {
    let (_tmp, db, lane) = fixture();
    let (started, observed) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let task = tokio::spawn({
        let (db, lane) = (db.clone(), lane.clone());
        async move {
            run_bounded(&db, lane, Duration::from_secs(5), move |_| {
                let _ = started.send(());
                released.recv_timeout(Duration::from_secs(3))?;
                Ok(())
            })
            .await
        }
    });
    observed.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(
        lane.try_acquire().is_err(),
        "cancellation must not oversubscribe blocking work"
    );
    // Current-thread runtime remains responsive while the reader is blocked.
    db.get_or_create_project_reference(
        "p",
        "agent:r:p",
        &format!("wc_projroot_{}", "a".repeat(64)),
        1,
    )
    .unwrap();
    release.send(()).unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        run_bounded(&db, lane, Duration::from_secs(2), |db| {
            db.count_window_activity_summaries(None)
        }),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn executing_read_timeout_reports_unavailable_not_an_empty_page() {
    let (_tmp, db, lane) = fixture();
    let (release, released) = std::sync::mpsc::channel();
    let result = run_bounded(&db, lane.clone(), Duration::from_millis(20), move |_| {
        released.recv_timeout(Duration::from_secs(2))?;
        Ok(Vec::<u8>::new())
    })
    .await;
    assert!(matches!(
        result,
        Err(RuntimeConsoleError::Request { status: 503, .. })
    ));
    release.send(()).unwrap();
    assert!(run_bounded(&db, lane, Duration::from_secs(2), |_| Ok(()))
        .await
        .is_ok());
}
