use super::*;

#[test]
#[cfg(unix)]
fn initialize_wait_consumes_total_run_deadline() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_initialize");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_totalinitialize01";
    let started_at = Instant::now();
    let started = manager.handle(
        start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 1),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert!(started_at.elapsed() < Duration::from_secs(3));
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(snapshot.state, CodingAgentRunState::Failed);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|t| t.error_code.as_deref()),
        Some("coding_agent_setup_timeout")
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn config_setup_cumulatively_consumes_total_run_deadline() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "slow_configs");
    let mut cfg = fake_config(exe, args);
    cfg.agents[0].allowed_config_options = ["one", "two", "three", "four"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_totalconfigs001";
    let config = ["one", "two", "three", "four"]
        .into_iter()
        .map(|key| {
            (
                key.to_string(),
                CodingAgentConfigValue::String("b".to_string()),
            )
        })
        .collect();
    let started = manager.handle(
        start_request_with_timeout(&manager, &root, run, config, 2),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(snapshot.state, CodingAgentRunState::Failed);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|t| t.error_code.as_deref()),
        Some("coding_agent_setup_timeout")
    );
    let log = wire_log(&temp);
    let methods = received_methods(&log);
    assert!(
        methods
            .iter()
            .filter(|method| method.as_str() == "session/set_config_option")
            .count()
            < 4,
        "the cumulative budget must prevent a fourth configuration admission: {methods:?}"
    );
    let completed_configs = log
        .iter()
        .filter(|entry| entry.get("config_applied").is_some())
        .count();
    assert!(
        completed_configs < 4,
        "the total run deadline must expire before all slow config responses complete: {log:?}"
    );
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn deadline_after_durable_prompt_barrier_still_prevents_prompt_write() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_deadlinebarrier01";
    *manager.prompt_after_barrier_test_delay.lock().unwrap() = Some(Duration::from_millis(1100));
    let started = manager.handle(
        start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 1),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    *manager.prompt_after_barrier_test_delay.lock().unwrap() = None;
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(snapshot.state, CodingAgentRunState::Failed);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|t| t.error_code.as_deref()),
        Some("coding_agent_setup_timeout")
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn blocked_max_prompt_write_respects_total_deadline_and_reaps_tree() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_promptbackpressure01";
    let started_at = Instant::now();
    // Process startup and ACP negotiation are setup for this assertion, not
    // the behavior under test. Hosted macOS VMs have already demonstrated
    // that a three-second total budget can expire before the fake provider
    // reaches the intended blocked ChildStdin state. Use the same bounded
    // ten-second Run budget as the adjacent blocked-write lifecycle tests;
    // the assertions below still require the permanently blocked write to
    // terminate at the total Run deadline and preserve uncertainty/reaping.
    let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("stdin_stopped.ready"));
    wait_for_prompt_handoff(&manager, run);
    let observation_deadline = started_at + Duration::from_secs(13);
    let snapshot = wait_for_snapshot_until(&manager, run, observation_deadline, |snapshot| {
        snapshot.state.terminal()
    });
    assert!(
        Instant::now() < observation_deadline,
        "blocked prompt write escaped the total Run deadline"
    );
    assert_eq!(snapshot.state, CodingAgentRunState::Lost);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
    assert_eq!(
        *manager
            .runs
            .lock()
            .unwrap()
            .get(run)
            .unwrap()
            .prompt_dispatch
            .lock()
            .unwrap(),
        PromptDispatchGateState::PromptDispatchMayHaveOccurred
    );
    let log = wire_log(&temp);
    let _startup_pid = log
        .iter()
        .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
        .unwrap();
    let _descendant_pid = log
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(drain.panicked, 0);
    assert_eq!(manager.worker_count(), 0);
    #[cfg(target_os = "linux")]
    {
        wait_for_proc_exit(_startup_pid);
        wait_for_proc_exit(_descendant_pid);
    }
}

#[test]
#[cfg(unix)]
fn cancel_returns_while_max_prompt_write_is_blocked() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_cancelblockedprompt1";
    let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("stdin_stopped.ready"));
    wait_for_prompt_handoff(&manager, run);

    let cancel_started = Instant::now();
    let cancelled = manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    assert!(
        cancel_started.elapsed() < Duration::from_secs(1),
        "Cancel waited for blocked ChildStdin write"
    );
    let cancel_snapshot = match cancelled.payload.unwrap() {
        CodingAgentResponsePayload::Cancel { run } => run,
        other => panic!("unexpected cancel payload: {other:?}"),
    };
    assert_ne!(
        cancel_snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(snapshot.state, CodingAgentRunState::Lost);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
    let log = wire_log(&temp);
    let _startup_pid = log
        .iter()
        .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
        .unwrap();
    let _descendant_pid = log
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(manager.worker_count(), 0);
    #[cfg(target_os = "linux")]
    {
        wait_for_proc_exit(_startup_pid);
        wait_for_proc_exit(_descendant_pid);
    }
}

#[test]
#[cfg(unix)]
fn shutdown_remains_bounded_while_max_prompt_write_is_blocked() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_shutdownblockedprompt";
    let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("stdin_stopped.ready"));
    wait_for_prompt_handoff(&manager, run);

    let stop_started = Instant::now();
    manager.stop_accepting();
    assert!(
        stop_started.elapsed() < Duration::from_secs(1),
        "stop_accepting waited for blocked ChildStdin write"
    );
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(4));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(drain.panicked, 0);
    assert_eq!(manager.worker_count(), 0);
    let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(snapshot.state, CodingAgentRunState::Lost);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
    let log = wire_log(&temp);
    let _startup_pid = log
        .iter()
        .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
        .unwrap();
    let _descendant_pid = log
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();
    #[cfg(target_os = "linux")]
    {
        wait_for_proc_exit(_startup_pid);
        wait_for_proc_exit(_descendant_pid);
    }
}

#[test]
#[cfg(unix)]
fn blocked_cancel_notification_is_bounded_by_cancel_grace() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_cancel_write");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_blockedcancelwrite1";
    let started = manager.handle(
        start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 30),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("prompt_read.ready"));
    wait_for_snapshot(&manager, run, |snapshot| {
        snapshot.execution_state == CodingAgentExecutionState::Started
    });
    assert!(
        notification_frame("session/cancel", json!({"sessionId":"s".repeat(70_000)}),)
            .unwrap()
            .len()
            > 64 * 1024,
        "cancel backpressure fixture must exceed the measured special Linux pipe capacity"
    );

    let cancel_started = Instant::now();
    let cancelled = manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    assert!(
        cancel_started.elapsed() < Duration::from_secs(1),
        "Cancel waited on the later session/cancel write"
    );
    let cancel_snapshot = match cancelled.payload.unwrap() {
        CodingAgentResponsePayload::Cancel { run } => run,
        other => panic!("unexpected cancel payload: {other:?}"),
    };
    assert_ne!(
        cancel_snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    let terminal_started = Instant::now();
    let drain = manager.drain_workers_until(
        Instant::now() + ACP_CANCEL_GRACE + ACP_IO_CLEANUP_TIMEOUT + Duration::from_secs(1),
    );
    assert_eq!(drain.timed_out, 0);
    assert_eq!(drain.panicked, 0);
    let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert!(
        terminal_started.elapsed()
            < ACP_CANCEL_GRACE + ACP_IO_CLEANUP_TIMEOUT + Duration::from_secs(1),
        "blocked session/cancel escaped ACP_CANCEL_GRACE plus cleanup bound"
    );
    assert_eq!(snapshot.state, CodingAgentRunState::Lost);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
    assert_eq!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_cancel_write_uncertain")
    );
    let log = wire_log(&temp);
    let _startup_pid = log
        .iter()
        .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
        .unwrap();
    let _descendant_pid = log
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();
    assert_eq!(manager.worker_count(), 0);
    #[cfg(target_os = "linux")]
    {
        wait_for_proc_exit(_startup_pid);
        wait_for_proc_exit(_descendant_pid);
    }
}

#[test]
#[cfg(unix)]
fn cancel_during_initialize_never_dispatches_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_initialize");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_cancelinitialize01";
    let started = manager.handle(
        start_request(&manager, &root, run, BTreeMap::new()),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("initialize.ready"));

    let cancelled = manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    let snapshot = match cancelled.payload.unwrap() {
        CodingAgentResponsePayload::Cancel { run } => run,
        other => panic!("unexpected cancel payload: {other:?}"),
    };
    assert_eq!(snapshot.state, CodingAgentRunState::Cancelled);
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        snapshot
            .terminal
            .as_ref()
            .and_then(|t| t.stop_reason.as_deref()),
        None
    );
    fs::write(temp.path().join("initialize.release"), b"release").unwrap();
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    assert_eq!(drain.panicked, 0);
    let methods = received_methods(&wire_log(&temp));
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.as_str() == "session/cancel")
            .count(),
        0
    );
    let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
    assert_eq!(
        final_snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
}

#[test]
#[cfg(unix)]
fn cancel_and_prompt_gate_race_has_only_linearized_outcomes() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "wait_cancel");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_promptgaterace01";
    let race = Arc::new(std::sync::Barrier::new(2));
    *manager.prompt_dispatch_test_barrier.lock().unwrap() = Some(Arc::clone(&race));
    let started = manager.handle(
        start_request(&manager, &root, run, BTreeMap::new()),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);

    let cancel_manager = Arc::clone(&manager);
    let cancel_projects = projects.clone();
    let cancel_race = Arc::clone(&race);
    let cancel = thread::spawn(move || {
        cancel_race.wait();
        cancel_manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &cancel_projects,
        )
    });
    let cancelled = cancel.join().unwrap();
    assert!(cancelled.error.is_none(), "{:?}", cancelled.error);
    *manager.prompt_dispatch_test_barrier.lock().unwrap() = None;
    let final_snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    let methods = received_methods(&wire_log(&temp));
    let prompts = methods
        .iter()
        .filter(|m| m.as_str() == "session/prompt")
        .count();
    let cancels = methods
        .iter()
        .filter(|m| m.as_str() == "session/cancel")
        .count();
    assert!(
        prompts <= 1,
        "prompt dispatched more than once: {methods:?}"
    );
    match prompts {
        0 => {
            assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
            assert_eq!(
                final_snapshot.execution_state,
                CodingAgentExecutionState::NotStarted
            );
            assert_eq!(cancels, 0);
        }
        1 => {
            assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
            assert_eq!(
                final_snapshot.execution_state,
                CodingAgentExecutionState::Completed
            );
            assert_eq!(cancels, 1);
        }
        _ => unreachable!(),
    }
}

#[test]
#[cfg(unix)]
fn shutdown_during_admission_catches_published_run_before_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "wait_cancel");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_shutdownadmission01";
    let publish_barrier = Arc::new(std::sync::Barrier::new(2));
    *manager
        .admission_after_accepting_test_barrier
        .lock()
        .unwrap() = Some(Arc::clone(&publish_barrier));
    manager
        .admission_after_accepting_test_reached
        .store(false, Ordering::SeqCst);

    let start_manager = Arc::clone(&manager);
    let start_projects = projects.clone();
    let request = start_request(&manager, &root, run, BTreeMap::new());
    let start = thread::spawn(move || start_manager.handle(request, &start_projects));
    let reached_deadline = Instant::now() + Duration::from_secs(5);
    while !manager
        .admission_after_accepting_test_reached
        .load(Ordering::SeqCst)
    {
        assert!(Instant::now() < reached_deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let shutdown_manager = Arc::clone(&manager);
    let shutdown = thread::spawn(move || shutdown_manager.stop_accepting());
    let stopping_deadline = Instant::now() + Duration::from_secs(5);
    while manager.accepting.load(Ordering::Acquire) {
        assert!(Instant::now() < stopping_deadline);
        thread::sleep(Duration::from_millis(5));
    }
    publish_barrier.wait();
    let started = start.join().unwrap();
    assert!(started.error.is_none(), "{:?}", started.error);
    shutdown.join().unwrap();
    *manager
        .admission_after_accepting_test_barrier
        .lock()
        .unwrap() = None;
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
    assert_eq!(
        final_snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn shutdown_drains_setup_worker_and_reaps_provider_tree() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_initialize_tree");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_shutdowndrain01";
    let started = manager.handle(
        start_request(&manager, &root, run, BTreeMap::new()),
        &projects,
    );
    assert!(started.error.is_none(), "{:?}", started.error);
    wait_for_path(&temp.path().join("initialize.ready"));
    assert!(manager.worker_count() > 0);
    let log = wire_log(&temp);
    let _startup_pid = log
        .iter()
        .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
        .unwrap();
    let _descendant_pid = log
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();

    manager.stop_accepting();
    fs::write(temp.path().join("initialize.release"), b"release").unwrap();
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert!(drain.resources > 0);
    assert_eq!(drain.timed_out, 0);
    assert_eq!(drain.panicked, 0);
    assert_eq!(manager.worker_count(), 0);
    let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
    assert_eq!(
        final_snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count(),
        0
    );
    #[cfg(target_os = "linux")]
    {
        wait_for_proc_exit(_startup_pid);
        wait_for_proc_exit(_descendant_pid);
    }
}
