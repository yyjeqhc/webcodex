use super::*;
use std::future::Future;
use std::task::Poll;

fn key() -> ObservationKey {
    ObservationKey {
        state_generation: 7,
        operation_generation: 2,
        environment_id: Some("fixture-environment".into()),
    }
}

fn heartbeat() -> RuntimeHeartbeat {
    RuntimeHeartbeat::from(&webcodex_environment::RuntimeObservation {
        server_reachable: true,
        authenticated: true,
        runner_online: Some(true),
        projects_visible: vec!["fixture-project".into()],
        ..Default::default()
    })
}

#[test]
fn unchanged_heartbeats_do_not_extend_native_display_evidence() {
    let cache = RuntimeObservationCache::default();
    let now = Instant::now();
    cache.complete_fresh(cache.ticket(), key(), Ok(()), Some(heartbeat()), now, now);
    let waiting = cache.ticket();
    let later = now + Duration::from_secs(2);
    assert!(cache.cached_heartbeat(&key(), later).is_some());
    assert!(cache.complete_cached(cache.ticket(), &key(), later));
    assert!(cache
        .completed_since(waiting, &key(), later)
        .unwrap()
        .is_ok());
    // A queued caller waking late cannot prolong the hit it was waiting for.
    assert!(cache
        .completed_since(waiting, &key(), now + DISPLAY_OBSERVATION_TTL)
        .is_none());
    assert!(cache
        .cached_heartbeat(&key(), now + DISPLAY_OBSERVATION_TTL)
        .is_none());
    assert!(!cache.complete_cached(cache.ticket(), &key(), now + DISPLAY_OBSERVATION_TTL));
}

#[test]
fn invalidation_rejects_a_late_observation_and_changed_targets_never_reuse_it() {
    let cache = RuntimeObservationCache::default();
    let old = cache.ticket();
    cache.invalidate();
    cache.complete_fresh(
        old,
        key(),
        Ok(()),
        Some(heartbeat()),
        Instant::now(),
        Instant::now(),
    );
    assert!(cache.cached_heartbeat(&key(), Instant::now()).is_none());
    assert!(cache.completed_since(old, &key(), Instant::now()).is_none());

    cache.complete_fresh(
        cache.ticket(),
        key(),
        Ok(()),
        Some(heartbeat()),
        Instant::now(),
        Instant::now(),
    );
    let variants = [
        ObservationKey {
            environment_id: Some("another-environment".into()),
            ..key()
        },
        ObservationKey {
            operation_generation: 3,
            ..key()
        },
        ObservationKey {
            state_generation: 8,
            ..key()
        },
    ];
    for changed in variants {
        assert!(cache.cached_heartbeat(&changed, Instant::now()).is_none());
        assert!(cache
            .completed_since(old, &changed, Instant::now())
            .is_none());
    }
}

#[test]
fn failed_observations_are_shared_only_with_existing_waiters() {
    let cache = RuntimeObservationCache::default();
    let waiting = cache.ticket();
    let error = DesktopError::new("fixture_unavailable", "Unavailable", "Retry");
    cache.complete_fresh(
        waiting,
        key(),
        Err(error),
        None,
        Instant::now(),
        Instant::now(),
    );
    assert_eq!(
        cache
            .completed_since(waiting, &key(), Instant::now())
            .unwrap()
            .unwrap_err()
            .code,
        "fixture_unavailable"
    );
    assert!(cache.cached_heartbeat(&key(), Instant::now()).is_none());
    assert!(cache
        .completed_since(cache.ticket(), &key(), Instant::now())
        .is_none());
}

#[test]
fn a_slow_fresh_read_is_shared_with_waiters_but_is_already_expired_for_display_reuse() {
    let cache = RuntimeObservationCache::default();
    let waiting = cache.ticket();
    let started = Instant::now();
    let completed = started + Duration::from_secs(10);
    cache.complete_fresh(
        waiting,
        key(),
        Ok(()),
        Some(heartbeat()),
        started,
        completed,
    );
    assert!(cache.cached_heartbeat(&key(), completed).is_none());
    assert!(cache
        .completed_since(waiting, &key(), completed)
        .unwrap()
        .is_ok());
    assert!(cache
        .completed_since(waiting, &key(), completed + DISPLAY_OBSERVATION_TTL)
        .is_none());
}

#[test]
fn heartbeat_changes_include_server_runner_credentials_and_project_visibility() {
    let ready = heartbeat();
    for changed in [
        RuntimeHeartbeat {
            server_reachable: false,
            ..ready.clone()
        },
        RuntimeHeartbeat {
            authenticated: false,
            ..ready.clone()
        },
        RuntimeHeartbeat {
            runner_online: Some(false),
            ..ready.clone()
        },
        RuntimeHeartbeat {
            projects_visible: Vec::new(),
            ..ready.clone()
        },
        RuntimeHeartbeat {
            diagnostics: vec![webcodex_environment::SetupDiagnostic::new(
                "authentication_required",
                "Authentication failed",
                "Sign in",
            )],
            ..ready.clone()
        },
    ] {
        assert_ne!(changed, ready);
    }
}

#[tokio::test]
async fn overlapping_background_requests_share_one_observation_without_admission() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf(), dir.path().join("resources")).unwrap();
    let guard = state.refresh_lock.lock().await;
    let mut first = Box::pin(state.observe_runtime_status());
    let mut second = Box::pin(state.observe_runtime_status());
    // Queue both requests before releasing the observer; no sleeps or timing race.
    std::future::poll_fn(|cx| {
        assert!(first.as_mut().poll(cx).is_pending());
        assert!(second.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(guard);
    let (first, second) = tokio::join!(first, second);
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.state_generation, second.state_generation);
    assert!(first.current_operation.is_none());
    assert!(second.current_operation.is_none());
    assert_eq!(state.runtime_observation.ticket().completion, 1);
    let before = second.state_generation;
    let refreshed = state.refresh_runtime_status().await.unwrap();
    assert!(
        refreshed.state_generation > before,
        "explicit refresh must run fresh"
    );
    assert_eq!(state.runtime_observation.ticket().completion, 2);
}

#[tokio::test]
async fn background_observation_keeps_diagnostics_readable() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf(), dir.path().join("resources")).unwrap();
    let (observed, diagnostics) = tokio::join!(state.observe_runtime_status(), state.diagnostics());
    assert!(observed.unwrap().current_operation.is_none());
    assert!(
        diagnostics.is_ok(),
        "observation must not claim an exclusive operation slot"
    );
}

#[tokio::test]
async fn operation_admission_and_completion_invalidate_display_observations() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf(), dir.path().join("resources")).unwrap();
    let cache = &state.runtime_observation;
    let old = cache.ticket();
    cache.complete_fresh(
        old,
        key(),
        Ok(()),
        Some(heartbeat()),
        Instant::now(),
        Instant::now(),
    );
    let (operation, cancellation, core, baseline) = state
        .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
        .await
        .unwrap();
    assert!(cache.cached_heartbeat(&key(), Instant::now()).is_none());
    assert_eq!(
        state.observe_runtime_status().await.unwrap_err().code,
        "desktop_operation_busy"
    );
    cache.complete_fresh(
        old,
        key(),
        Ok(()),
        Some(heartbeat()),
        Instant::now(),
        Instant::now(),
    );
    assert!(cache.cached_heartbeat(&key(), Instant::now()).is_none());
    let during = cache.ticket();
    cache.complete_fresh(
        during,
        key(),
        Ok(()),
        Some(heartbeat()),
        Instant::now(),
        Instant::now(),
    );
    state
        .finish_operation(
            operation,
            cancellation,
            core,
            baseline,
            Ok(state.get_state()),
        )
        .await
        .unwrap();
    assert!(cache.cached_heartbeat(&key(), Instant::now()).is_none());
    assert!(state.get_state().current_operation.is_none());
}

#[tokio::test]
async fn superseded_observation_cannot_publish_or_cache_an_old_operation_generation() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf(), dir.path().join("resources")).unwrap();
    let core = state.core.lock().await;
    let before = state.get_state().state_generation;
    let mut observed = Box::pin(state.observe_runtime_status());
    std::future::poll_fn(|cx| {
        assert!(observed.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let operation = state
        .operations
        .admit(DesktopOperationKind::RunnerSettingsUpdate, false)
        .unwrap();
    drop(core);
    let snapshot = observed.await.unwrap();
    assert_eq!(snapshot.state_generation, before);
    assert_eq!(
        snapshot.current_operation.as_ref().unwrap().kind,
        DesktopOperationKind::RunnerSettingsUpdate
    );
    assert_eq!(state.runtime_observation.ticket().completion, 0);
    operation.finish(&Ok::<(), DesktopError>(()));
}
