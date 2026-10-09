use super::*;

#[tokio::test]
async fn concurrent_exit_waits_for_first_cleanup_owner() {
    let root = tempfile::tempdir().unwrap();
    let state =
        Arc::new(AppState::new(root.path().join("state"), root.path().join("resources")).unwrap());
    let held_owner = state.supervisor.lock().await;
    let first_state = Arc::clone(&state);
    let first = tokio::spawn(async move { first_state.shutdown().await });
    state.shutdown_signal.cancelled().await;
    // The first cleanup cannot finish until it owns the supervisor. A second
    // exit must wait for that cleanup, rather than release the Job Objects.
    assert!(
        tokio::time::timeout(Duration::from_millis(30), state.shutdown())
            .await
            .is_err()
    );
    assert!(!state.shutdown_complete.is_cancelled());
    drop(held_owner);
    first.await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), state.shutdown())
        .await
        .unwrap();
    assert!(state.shutdown_complete.is_cancelled());
}

#[tokio::test]
async fn session_shutdown_lock_contention_obeys_owner_deadline() {
    let root = tempfile::tempdir().unwrap();
    let state = AppState::new(root.path().join("state"), root.path().join("resources")).unwrap();
    let held_owner = state.supervisor.lock().await;
    tokio::time::timeout(
        Duration::from_secs(1),
        state.shutdown_until(Deadline::after(Duration::from_millis(30))),
    )
    .await
    .unwrap();
    assert!(state.shutdown_signal.is_cancelled());
    drop(held_owner);
}
