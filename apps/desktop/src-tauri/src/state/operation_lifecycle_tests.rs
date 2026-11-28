use super::*;

async fn state_with_runner(dir: &Path) -> AppState {
    let path = dir.join("runner.toml");
    std::fs::write(
        &path,
        "server_url = 'http://127.0.0.1:8080'\nclient_id = 'fixture'\nmax_concurrent_jobs = 2\n",
    )
    .unwrap();
    let state = AppState::new(dir.into(), dir.join("resources")).unwrap();
    let mut slot = state.core.lock().await;
    let core = slot.as_mut().unwrap();
    core.config.persistent_environment = None;
    core.config.runtime = Some(StoredRuntime {
        server_url: "http://127.0.0.1:8080".into(),
        server_env_file: None,
        runner_config: Some(path),
        user_token_file: None,
        runner_client_id: Some("fixture".into()),
        project_id: None,
        runtime_project_id: None,
    });
    drop(slot);
    state
}

#[tokio::test]
async fn stalled_runner_settings_read_leaves_core_available_and_discards_stale_results() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_runner(dir.path()).await;
    let supervisor = state.supervisor.lock().await;
    let mut read = Box::pin(state.runner_settings());
    tokio::select! {
        biased;
        _ = &mut read => panic!("the runner ownership observation is blocked"),
        _ = std::future::ready(()) => {}
    }
    // Simulate a target change while the old reader waits on the supervisor.
    state
        .core
        .try_lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .config
        .runtime
        .as_mut()
        .unwrap()
        .runner_client_id = Some("replacement".into());
    drop(supervisor);
    assert_eq!(read.await.err().unwrap().code, "runtime_selection_changed");

    // A previous filesystem worker still has its permit. Another read must
    // time out without borrowing the Core or spawning another filesystem worker.
    tokio::time::pause();
    let _reader = state.runner_settings_reader.lock().await;
    let mut read = Box::pin(state.runner_settings());
    tokio::select! {
        biased;
        _ = &mut read => panic!("the fixture owns the read permit"),
        _ = std::future::ready(()) => {}
    }
    state
        .update_tunnel_proxy(TunnelProxyMode::Direct, None)
        .await
        .unwrap();
    assert_eq!(read.await.err().unwrap().code, "desktop_state_unavailable");
    assert!(state.get_state().current_operation.is_none());
}

#[tokio::test]
async fn settings_read_checks_admission_before_and_after_file_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_runner(dir.path()).await;
    let reader = state.runner_settings_reader.lock().await;
    let mut started_idle = Box::pin(state.runner_settings());
    tokio::select! {
        biased;
        _ = &mut started_idle => panic!("file reader permit is held"),
        _ = std::future::ready(()) => {}
    }
    // Admission precedes Core checkout. The Core and its target still look valid.
    let owner = state
        .operations
        .admit(DesktopOperationKind::RunnerSettingsUpdate, false)
        .unwrap();
    let started_busy = tokio::select! {
        biased;
        result = state.runner_settings() => result,
        _ = std::future::ready(()) => panic!("a read behind an admitted writer waited for file I/O"),
    };
    drop(reader);
    let finished_busy = started_idle.await;
    for result in [started_busy, finished_busy] {
        let error = result.err().expect("the admitted writer must be reported");
        assert_eq!(error.code, "desktop_operation_busy");
        assert_eq!(
            error.diagnostic_ref().unwrap().operation_id.as_ref(),
            Some(&owner.id)
        );
    }
    owner.finish(&Ok::<(), DesktopError>(()));
    assert_eq!(
        state.runner_settings().await.unwrap().max_concurrent_jobs,
        Some(2)
    );
}

#[tokio::test]
async fn settings_read_discards_a_candidate_rolled_back_by_a_completed_write() {
    use crate::webcodex::settings::{stage_job_concurrency, target, JobConcurrencyUpdate};

    let dir = tempfile::tempdir().unwrap();
    let state = state_with_runner(dir.path()).await;
    let permit = state.runner_settings_reader.lock().await;
    let mut read = Box::pin(state.runner_settings());
    tokio::select! {
        biased;
        _ = &mut read => panic!("file reader permit is held"),
        _ = std::future::ready(()) => {}
    }
    let (owner, _cancellation, core, _baseline) = state
        .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
        .await
        .unwrap();
    let runtime = core.runner_settings_runtime().unwrap();
    let edit = stage_job_concurrency(
        &runtime,
        JobConcurrencyUpdate {
            target: target(&runtime).unwrap(),
            expected: Some(2),
            limit: 4,
        },
    )
    .unwrap();
    let mut slot = state.core.lock().await;
    drop(permit);
    tokio::select! {
        biased;
        _ = &mut read => panic!("the final Core check is blocked"),
        _ = state.runner_settings_reader.lock() => {}
    }
    // The worker has read 4 and released its permit. Roll back before allowing
    // that result to be checked against the same Runner identity.
    assert!(edit.rollback_if_unchanged().unwrap());
    *slot = Some(core);
    owner.finish(&Err::<(), _>(cancelled_error()));
    drop(slot);
    assert_eq!(read.await.err().unwrap().code, "runtime_selection_changed");
    assert_eq!(
        state.runner_settings().await.unwrap().max_concurrent_jobs,
        Some(2)
    );
}

#[tokio::test(start_paused = true)]
async fn preparation_releases_admission_on_drop_stop_and_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_owned(), dir.path().join("resources")).unwrap();
    let core = state.core.lock().await;
    for end in ["drop", "stop", "timeout"] {
        let mut preparing =
            Box::pin(state.begin_operation(DesktopOperationKind::RuntimeProbe, true));
        tokio::select! {
            biased;
            _ = &mut preparing => panic!("the fixture still owns the core lock"),
            _ = std::future::ready(()) => {}
        }
        let observed = state.get_state().current_operation.unwrap();
        match end {
            "drop" => drop(preparing),
            "stop" => {
                state.cancel_operation(&observed.id).unwrap();
                assert_eq!(
                    preparing.await.err().unwrap().code,
                    "desktop_operation_cancelled"
                );
            }
            _ => {
                tokio::time::advance(OPERATION_SLOT_TIMEOUT).await;
                assert_eq!(
                    preparing.await.err().unwrap().code,
                    "desktop_operation_start_timeout"
                );
            }
        }
        assert!(state.operations.observe().unwrap().is_none());
        assert!(core.is_some());
    }
    drop(core);
    let (operation, cancellation, core, baseline) = state
        .begin_operation(DesktopOperationKind::RuntimeProbe, true)
        .await
        .unwrap();
    let result = Ok(core.snapshot.clone());
    state
        .finish_operation(operation, cancellation, core, baseline, result)
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn stop_interrupts_a_dispatched_operation_waiting_to_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().into(), dir.path().join("resources")).unwrap();
    let (owner, cancellation, core, baseline) = state
        .begin_operation(DesktopOperationKind::RuntimeResume, true)
        .await
        .unwrap();
    let supervisor = state.supervisor.lock().await;
    let mut spawn = Box::pin(core.spawn_owned(
        ProcessKey::LocalRunner,
        std::process::Command::new(dir.path().join("must-not-launch")),
        false,
        &cancellation,
    ));
    tokio::select! {
        biased;
        _ = &mut spawn => panic!("the supervisor is still locked"),
        _ = std::future::ready(()) => {}
    }
    state.cancel_operation(&owner.id).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(1), spawn)
        .await
        .expect("Stop must interrupt the lock wait")
        .err()
        .unwrap();
    assert_eq!(error.code, "desktop_operation_cancelled");
    drop(supervisor);
    let error = state
        .finish_operation(owner, cancellation, core, baseline, Err(error))
        .await
        .unwrap_err();
    assert_eq!(error.code, "desktop_operation_cancelled");
    assert!(state.get_state().current_operation.is_none());
    assert!(state.core.lock().await.is_some());
}

#[tokio::test(start_paused = true)]
async fn completed_write_returns_its_core_after_prolonged_lock_contention() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().into(), dir.path().join("resources")).unwrap();
    let (operation, cancellation, mut core, baseline) = state
        .begin_operation(DesktopOperationKind::TunnelProxyUpdate, false)
        .await
        .unwrap();
    let id = operation.id.clone();
    let result = core
        .update_tunnel_proxy(TunnelProxyMode::Direct, None, &cancellation)
        .await;
    let blocked = state.core.lock().await;
    let (result, ()) = tokio::join!(
        state.finish_operation(operation, cancellation, core, baseline, result),
        async {
            tokio::time::sleep(OPERATION_SLOT_TIMEOUT + Duration::from_secs(1)).await;
            assert_eq!(state.operations.current().map(|op| op.id), Some(id));
            drop(blocked);
        }
    );
    assert!(result.unwrap().current_operation.is_none());
    assert_eq!(
        state
            .core
            .lock()
            .await
            .as_ref()
            .unwrap()
            .config
            .tunnel_proxy
            .mode,
        TunnelProxyMode::Direct
    );
    state
        .update_tunnel_proxy(TunnelProxyMode::Auto, None)
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn delayed_failure_cleanup_returns_the_original_error_and_releases_admission() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().into(), dir.path().join("resources")).unwrap();
    let (owner, cancellation, core, baseline) = state
        .begin_operation(DesktopOperationKind::RuntimeResume, true)
        .await
        .unwrap();
    let id = owner.id.clone();
    let supervisor = state.supervisor.lock().await;
    let (result, ()) = tokio::join!(
        state.finish_operation(owner, cancellation, core, baseline, Err(cancelled_error())),
        async {
            tokio::time::sleep(OPERATION_SLOT_TIMEOUT + Duration::from_secs(1)).await;
            assert_eq!(state.operations.current().map(|op| op.id), Some(id.clone()));
            drop(supervisor);
        }
    );
    let error = result.unwrap_err();
    assert_eq!(error.code, "desktop_operation_cancelled");
    assert_eq!(
        error.diagnostic_ref().unwrap().operation_id.as_ref(),
        Some(&id)
    );
    assert!(state.get_state().current_operation.is_none());
    state
        .update_tunnel_proxy(TunnelProxyMode::Direct, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn abandoned_dispatched_owner_reports_uncertainty_and_fences_later_writes() {
    for panic in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let state =
            Arc::new(AppState::new(dir.path().into(), dir.path().join("resources")).unwrap());
        let (started, observed) = tokio::sync::oneshot::channel();
        let worker_state = Arc::clone(&state);
        let worker = tokio::spawn(async move {
            let (owner, _cancel, _core, _baseline) = worker_state
                .begin_operation(DesktopOperationKind::LocalSetup, true)
                .await
                .unwrap();
            started.send(owner.id.clone()).unwrap();
            if panic {
                panic!("injected owner failure");
            }
            std::future::pending::<()>().await;
        });
        let id = observed.await.unwrap();
        if !panic {
            worker.abort();
        }
        let ended = worker.await.unwrap_err();
        assert_eq!(ended.is_panic(), panic);
        let snapshot = state.get_state();
        assert!(snapshot.current_operation.is_none());
        let error = snapshot.runtime_error.unwrap();
        assert_eq!(error.code, "desktop_core_unavailable");
        assert_eq!(
            error.diagnostic_ref().unwrap().operation_id.as_ref(),
            Some(&id)
        );
        assert!(!error.diagnostic_ref().unwrap().retry_allowed);
        assert_eq!(
            state.runner_settings().await.err().unwrap().code,
            error.code
        );
        assert_eq!(
            state
                .update_tunnel_proxy(TunnelProxyMode::Direct, None)
                .await
                .unwrap_err(),
            error
        );
        assert_eq!(
            state.cancel_operation(&id).unwrap_err().code,
            "desktop_operation_not_current"
        );
    }
}

#[tokio::test]
async fn missing_core_without_an_owner_is_an_internal_error_for_reads_and_writes() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().into(), dir.path().join("resources")).unwrap();
    let _missing = state.core.lock().await.take().unwrap();
    for error in [
        state.diagnostics().await.err().unwrap(),
        state.runner_settings().await.err().unwrap(),
        state.runtime_settings().await.err().unwrap(),
        state.path_inventory().await.err().unwrap(),
        state.refresh_runtime_status().await.unwrap_err(),
        state
            .update_tunnel_proxy(TunnelProxyMode::Direct, None)
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error.code, "desktop_core_unavailable");
        assert!(error.diagnostic_ref().unwrap().operation_id.is_none());
    }
    assert!(state.operations.current().is_none());
}
