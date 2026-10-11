use super::*;

#[test]
fn post_prompt_failed_persistence_failure_is_lost_without_original_error_truth() {
    let temp = TempDir::new().unwrap();
    let cfg = fake_config("unused-provider".to_string(), Vec::new());
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_persistfailed01";
    let entry = seed_terminal_test_run(
        &manager,
        run,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred,
        CodingAgentRunState::Running,
        CodingAgentExecutionState::Started,
    );
    manager.store.fail_next_terminal_writes(1);

    manager.finish_failed(
        run,
        &entry,
        "prompt_error",
        "provider prompt failed".to_string(),
    );

    assert_terminal_persistence_uncertain(
        &entry,
        CodingAgentRunState::Lost,
        CodingAgentExecutionState::OutcomeUnknown,
    );
    assert_ne!(
        entry
            .snapshot()
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("prompt_error")
    );
    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(
        durable.dispatch_phase,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred
    );
    assert_eq!(durable.terminal, None);
}

#[test]
fn setup_timeout_persistence_failure_is_failed_not_started() {
    let temp = TempDir::new().unwrap();
    let cfg = fake_config("unused-provider".to_string(), Vec::new());
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_persisttimeout01";
    let entry = seed_terminal_test_run(
        &manager,
        run,
        DurableDispatchPhase::BeforePromptBarrier,
        CodingAgentRunState::Starting,
        CodingAgentExecutionState::NotStarted,
    );
    manager.store.fail_next_terminal_writes(1);

    manager.setup_timeout(run, &entry, "initialize");

    assert_terminal_persistence_uncertain(
        &entry,
        CodingAgentRunState::Failed,
        CodingAgentExecutionState::NotStarted,
    );
    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(
        durable.dispatch_phase,
        DurableDispatchPhase::BeforePromptBarrier
    );
    assert_eq!(durable.terminal, None);
}

#[test]
fn mark_lost_persistence_failure_replaces_specific_lost_reason() {
    let temp = TempDir::new().unwrap();
    let cfg = fake_config("unused-provider".to_string(), Vec::new());
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_persistlost001";
    let entry = seed_terminal_test_run(
        &manager,
        run,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred,
        CodingAgentRunState::Running,
        CodingAgentExecutionState::Started,
    );
    manager.store.fail_next_terminal_writes(1);

    manager.mark_lost(run, &entry, "coding_agent_transport_lost");

    assert_terminal_persistence_uncertain(
        &entry,
        CodingAgentRunState::Lost,
        CodingAgentExecutionState::OutcomeUnknown,
    );
    assert_ne!(
        entry
            .snapshot()
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_transport_lost")
    );
}

#[test]
#[cfg(unix)]
fn completed_terminal_is_persisted_before_live_publication_and_restart() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let store_root = temp.path().join("store");
    let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
    let run = "wc_agent_run_persistorder001";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    let gate = Arc::new(TerminalWriteGate::default());
    manager
        .store
        .set_terminal_write_gate(Some(Arc::clone(&gate)));

    assert!(manager.handle(request.clone(), &projects).error.is_none());
    gate.wait_until_reached();
    let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
    let before = entry.observe(None, 64, 0).unwrap();
    assert!(!before.run.state.terminal());
    assert_ne!(before.run.state, CodingAgentRunState::Completed);
    assert!(before
        .events
        .iter()
        .all(|event| event.kind != CodingAgentEventKind::Terminal));
    let durable_before = manager.store.read(run).unwrap().unwrap();
    assert_eq!(
        durable_before.dispatch_phase,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred
    );
    assert_ne!(durable_before.state, CodingAgentRunState::Completed);

    gate.release();
    manager.store.set_terminal_write_gate(None);
    let observation = wait_for_terminal_observation(&manager, run);
    assert_eq!(observation.run.state, CodingAgentRunState::Completed);
    assert_eq!(
        observation.run.execution_state,
        CodingAgentExecutionState::Completed
    );
    assert_eq!(
        observation
            .run
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.stop_reason.as_deref()),
        Some(CODING_AGENT_STOP_REASON_END_TURN)
    );
    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
    assert_eq!(durable.state, CodingAgentRunState::Completed);
    assert_eq!(
        durable.execution_state,
        CodingAgentExecutionState::Completed
    );
    assert_eq!(durable.terminal, observation.run.terminal);
    assert_eq!(durable.updated_at, observation.run.updated_at);
    assert_eq!(prompt_count(&temp), 1);
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);

    let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
    let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(restored.state, CodingAgentRunState::Completed);
    assert_eq!(
        restored.execution_state,
        CodingAgentExecutionState::Completed
    );
    assert!(restarted.handle(request, &projects).error.is_none());
    assert_eq!(prompt_count(&temp), 1);
}

#[test]
#[cfg(unix)]
fn completion_persistence_failure_publishes_only_lost_and_never_redispatches() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let store_root = temp.path().join("store");
    let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
    let run = "wc_agent_run_persistendfail1";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    let gate = Arc::new(TerminalWriteGate::default());
    manager.store.fail_next_terminal_writes(1);
    manager
        .store
        .set_terminal_write_gate(Some(Arc::clone(&gate)));

    assert!(manager.handle(request.clone(), &projects).error.is_none());
    gate.wait_until_reached();
    let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
    let before = entry.observe(None, 64, 0).unwrap();
    assert!(!before.run.state.terminal());
    assert_ne!(before.run.state, CodingAgentRunState::Completed);
    assert!(before
        .events
        .iter()
        .all(|event| event.kind != CodingAgentEventKind::Terminal));
    gate.release();
    manager.store.set_terminal_write_gate(None);

    let observation = wait_for_terminal_observation(&manager, run);
    assert_terminal_persistence_uncertain(
        &entry,
        CodingAgentRunState::Lost,
        CodingAgentExecutionState::OutcomeUnknown,
    );
    assert!(observation.events.iter().all(|event| {
        event.kind != CodingAgentEventKind::Terminal
            || event.label.as_deref() != Some(CODING_AGENT_STOP_REASON_END_TURN)
    }));
    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(
        durable.dispatch_phase,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred
    );
    assert_ne!(durable.state, CodingAgentRunState::Completed);
    assert_eq!(prompt_count(&temp), 1);
    assert!(manager.handle(request.clone(), &projects).error.is_none());
    assert_eq!(prompt_count(&temp), 1);
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);

    let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
    let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(restored.state, CodingAgentRunState::Lost);
    assert_eq!(
        restored.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
    assert!(restarted.handle(request, &projects).error.is_none());
    assert_eq!(prompt_count(&temp), 1);
}

#[test]
#[cfg(unix)]
fn pre_prompt_cancel_persistence_failure_is_failed_and_restart_never_prompts() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "block_initialize");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let store_root = temp.path().join("store");
    let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
    let run = "wc_agent_run_persistcancel01";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    manager.store.fail_next_terminal_writes(1);

    assert!(manager.handle(request.clone(), &projects).error.is_none());
    wait_for_path(&temp.path().join("initialize.ready"));
    let cancelled = manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    assert!(cancelled.error.is_none());
    let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
    assert_terminal_persistence_uncertain(
        &entry,
        CodingAgentRunState::Failed,
        CodingAgentExecutionState::NotStarted,
    );
    assert_eq!(prompt_count(&temp), 0);
    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(
        durable.dispatch_phase,
        DurableDispatchPhase::BeforePromptBarrier
    );
    assert_eq!(durable.terminal, None);
    assert!(manager.handle(request.clone(), &projects).error.is_none());
    assert_eq!(prompt_count(&temp), 0);

    fs::write(temp.path().join("initialize.release"), b"release").unwrap();
    let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
    assert_eq!(drain.timed_out, 0);
    let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
    let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
    assert_eq!(restored.state, CodingAgentRunState::Failed);
    assert_eq!(
        restored.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert!(restarted.handle(request, &projects).error.is_none());
    assert_eq!(prompt_count(&temp), 0);
}

#[test]
fn concurrent_pre_prompt_terminal_transitions_commit_one_terminal_truth() {
    let temp = TempDir::new().unwrap();
    let cfg = fake_config("unused-provider".to_string(), Vec::new());
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let provider = manager.providers().remove(0);
    let run = "wc_agent_run_terminalrace01";
    let timestamp = now();
    let record = DurableRunRecord {
        schema_version: STORE_SCHEMA_VERSION,
        run_id: run.to_string(),
        intent_fingerprint: "fingerprint".to_string(),
        authority_fingerprint: "auth_test".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        provider_id: "codex".to_string(),
        provider_instance_id: provider.provider_instance_id,
        state: CodingAgentRunState::Starting,
        execution_state: CodingAgentExecutionState::NotStarted,
        dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
        created_at: timestamp,
        updated_at: timestamp,
        terminal: None,
    };
    manager.store.write(&record).unwrap();
    let entry = Arc::new(RunEntry::new(record.snapshot(0)));
    manager
        .runs
        .lock()
        .unwrap()
        .insert(run.to_string(), Arc::clone(&entry));

    let race = Arc::new(std::sync::Barrier::new(3));
    let failure_manager = Arc::clone(&manager);
    let failure_entry = Arc::clone(&entry);
    let failure_race = Arc::clone(&race);
    let failure = thread::spawn(move || {
        failure_race.wait();
        failure_manager.setup_failure(run, &failure_entry, "setup_failed", "setup failed");
    });
    let cancel_manager = Arc::clone(&manager);
    let cancel_entry = Arc::clone(&entry);
    let cancel_race = Arc::clone(&race);
    let cancel = thread::spawn(move || {
        cancel_race.wait();
        cancel_manager.finish_pre_prompt_cancelled(run, &cancel_entry);
    });
    race.wait();
    failure.join().unwrap();
    cancel.join().unwrap();

    let snapshot = entry.snapshot();
    assert!(matches!(
        snapshot.state,
        CodingAgentRunState::Cancelled | CodingAgentRunState::Failed
    ));
    assert_eq!(
        snapshot.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    validate_coding_agent_run_snapshot(&snapshot).unwrap();

    let durable = manager.store.read(run).unwrap().unwrap();
    assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
    assert_eq!(durable.state, snapshot.state);
    assert_eq!(durable.execution_state, snapshot.execution_state);
    assert_eq!(durable.terminal, snapshot.terminal);

    let live = entry.state.lock().unwrap();
    assert_eq!(
        live.events
            .iter()
            .filter(|event| event.kind == CodingAgentEventKind::Terminal)
            .count(),
        1,
        "a terminal race emitted more than one terminal event"
    );
}
