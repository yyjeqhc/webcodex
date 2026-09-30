use super::*;

#[test]
#[cfg(unix)]
fn corrupt_durable_record_after_possible_dispatch_fails_closed_without_redispatch() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_corruptdurable01";
    let request = start_request(&manager, &root, run, BTreeMap::new());
    assert!(manager.handle(request.clone(), &projects).error.is_none());
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        1
    );

    manager.runs.lock().unwrap().remove(run);
    fs::write(manager.store.state_path(run), b"{corrupt-json").unwrap();
    let corrupt = manager.handle(request.clone(), &projects);
    assert_eq!(
        corrupt.dispatch_state,
        CodingAgentDispatchState::OutcomeUnknown
    );
    assert_eq!(
        corrupt.error.as_ref().map(|error| error.code.as_str()),
        Some("coding_agent_durable_state_unavailable")
    );

    fs::remove_file(manager.store.state_path(run)).unwrap();
    let missing = manager.handle(request.clone(), &projects);
    assert_eq!(
        missing.dispatch_state,
        CodingAgentDispatchState::OutcomeUnknown
    );
    assert_eq!(
        missing.error.as_ref().map(|error| error.code.as_str()),
        Some("coding_agent_durable_state_unavailable")
    );
    let store_root = manager.store.root.clone();
    drop(manager);
    let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
    let after_restart = restarted.handle(
        start_request(&restarted, &root, run, BTreeMap::new()),
        &projects,
    );
    assert_eq!(
        after_restart.dispatch_state,
        CodingAgentDispatchState::OutcomeUnknown
    );
    assert_eq!(
        after_restart
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("coding_agent_durable_state_unavailable")
    );
    assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1,
            "missing or corrupt durable state after a possible prompt must never become retry authority"
        );
}

#[test]
fn durable_store_replaces_existing_state_cross_platform() {
    let temp = TempDir::new().unwrap();
    let store = DurableRunStore::new(temp.path().join("store"));
    let timestamp = now();
    let mut record = DurableRunRecord {
        schema_version: STORE_SCHEMA_VERSION,
        run_id: "wc_agent_run_replace_state01".to_string(),
        intent_fingerprint: "fingerprint".to_string(),
        authority_fingerprint: "auth_replace".to_string(),
        runtime_project_id: "agent:test:demo".to_string(),
        provider_id: "codex".to_string(),
        provider_instance_id: "acp_replace".to_string(),
        state: CodingAgentRunState::Starting,
        execution_state: CodingAgentExecutionState::NotStarted,
        dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
        created_at: timestamp,
        updated_at: timestamp,
        terminal: None,
    };
    store.write(&record).unwrap();
    record.state = CodingAgentRunState::Running;
    record.execution_state = CodingAgentExecutionState::OutcomeUnknown;
    record.dispatch_phase = DurableDispatchPhase::PromptDispatchMayHaveOccurred;
    record.updated_at = timestamp.saturating_add(1);
    store.write(&record).unwrap();
    let restored = store.read(&record.run_id).unwrap().unwrap();
    assert_eq!(
        restored.dispatch_phase,
        DurableDispatchPhase::PromptDispatchMayHaveOccurred
    );
    assert_eq!(
        restored.execution_state,
        CodingAgentExecutionState::OutcomeUnknown
    );
}
#[test]
#[cfg(unix)]
fn pre_barrier_restart_is_not_started_and_child_tree_is_reaped() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let store_root = temp.path().join("store");
    fs::create_dir_all(&store_root).unwrap();
    let initial = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
    let provider = initial.providers().remove(0);
    drop(initial);
    let timestamp = now();
    DurableRunStore::new(store_root.clone())
        .write(&DurableRunRecord {
            schema_version: STORE_SCHEMA_VERSION,
            run_id: "wc_agent_run_prebarrier01".to_string(),
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
        })
        .unwrap();
    let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
    let recovered = restarted
        .runs
        .lock()
        .unwrap()
        .get("wc_agent_run_prebarrier01")
        .unwrap()
        .snapshot();
    assert_eq!(recovered.state, CodingAgentRunState::Failed);
    assert_eq!(
        recovered.execution_state,
        CodingAgentExecutionState::NotStarted
    );

    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "spawn_descendant");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_reaptree0001";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects
        )
        .error
        .is_none());
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    let descendant = wire_log(&temp)
        .iter()
        .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
        .unwrap();
    let proc_path = PathBuf::from(format!("/proc/{descendant}"));
    let deadline = Instant::now() + Duration::from_secs(3);
    while proc_path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !proc_path.exists(),
        "ACP descendant process survived ManagedChild cleanup"
    );
}

#[test]
#[cfg(unix)]
fn post_barrier_crash_is_lost_and_restart_never_redispatches() {
    let (manager, run, obs) = run_scenario("crash_after_prompt", BTreeMap::new());
    assert_eq!(obs.run.state, CodingAgentRunState::Lost);
    let cfg = AcpConfig {
        max_concurrent_runs: manager.max_concurrent_runs,
        permission_timeout_secs: 1,
        forced_config: manager.forced_config.clone(),
        agents: manager
            .providers
            .values()
            .map(|p| p.config.clone())
            .collect(),
    };
    let restarted = CodingAgentManager::with_store(&cfg, manager.store.root.clone()).unwrap();
    assert_eq!(
        restarted
            .runs
            .lock()
            .unwrap()
            .get(&run)
            .unwrap()
            .snapshot()
            .state,
        CodingAgentRunState::Lost
    );
}

#[test]
#[cfg(unix)]
fn durable_record_contains_no_prompt_or_event_bodies() {
    let (manager, run, _) = run_scenario("end", BTreeMap::new());
    let bytes = fs::read(manager.store.state_path(&run)).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("inspect"));
    assert!(!text.contains("hello"));
    assert!(!text.contains("thinking"));
}
