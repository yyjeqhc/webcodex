use super::*;

#[test]
fn outbound_writer_blocking_sink_does_not_block_lifecycle_owner() {
    let state = Arc::new((Mutex::new(BlockingWriteState::default()), Condvar::new()));
    let threads = BackgroundThreads::default();
    let mut writer = AcpOutboundWriter::spawn(
        BlockingWrite {
            state: Arc::clone(&state),
        },
        &threads,
    )
    .unwrap();
    let pending = writer.start_frame(vec![b'x'; 1024]).unwrap();
    wait_for_blocking_write(&state);

    let cancelled = AtomicBool::new(true);
    let outcome = wait_outbound_write(
        pending,
        Instant::now() + Duration::from_secs(1),
        Some(&cancelled),
        None,
    );
    assert!(matches!(
        outcome,
        OutboundWriteOutcome::Interrupted(OutboundInterruption::Cancelled)
    ));
    assert_eq!(threads.pending(), 1);

    // The production owner uses ManagedChild::terminate_tree to make a blocked
    // pipe write return. Releasing this deterministic sink models that exact
    // post-interruption effect without relying on pipe capacity or sleeps.
    release_blocking_write(&state);
    writer.close();
    assert!(writer.wait_finished_until(Instant::now() + Duration::from_secs(1)));
    let joined = threads.join_until(Instant::now() + Duration::from_secs(1));
    assert_eq!(joined.timed_out, 0);
    assert_eq!(joined.panicked, 0);
    assert_eq!(threads.pending(), 0);
}

#[test]
fn acknowledged_outbound_write_wins_over_later_cancellation() {
    let threads = BackgroundThreads::default();
    let mut writer = AcpOutboundWriter::spawn(Vec::<u8>::new(), &threads).unwrap();
    let pending = writer.start_frame(b"prompt\n".to_vec()).unwrap();
    writer.close();
    // Join the writer, not a sleep: its completion acknowledgement is now
    // definitely queued. The blocking-sink test covers the opposite ordering.
    let joined = threads.join_until(Instant::now() + Duration::from_secs(1));
    assert_eq!(joined.timed_out, 0);
    assert_eq!(joined.panicked, 0);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        wait_outbound_write(pending, Instant::now(), Some(&cancelled), None),
        OutboundWriteOutcome::Written
    ));
}

#[test]
#[cfg(unix)]
fn acp_v1_sequence_cwd_config_and_normalized_updates_are_exact() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_sequence0001";
    let response = manager.handle(
        start_request(&manager, &root, run, BTreeMap::new()),
        &projects,
    );
    assert!(response.error.is_none(), "{:?}", response.error);
    let observation = wait_for_terminal_observation(&manager, run);
    assert_eq!(observation.run.state, CodingAgentRunState::Completed);

    let log = wire_log(&temp);
    assert_eq!(
        received_methods(&log),
        vec!["initialize", "session/new", "session/prompt"]
    );
    let initialize = log
        .iter()
        .find_map(|entry| {
            (entry.pointer("/recv/method").and_then(Value::as_str) == Some("initialize"))
                .then(|| entry.pointer("/recv").unwrap())
        })
        .unwrap();
    assert_eq!(
        initialize.pointer("/params/clientCapabilities"),
        Some(&json!({}))
    );
    let session_new = log
        .iter()
        .find_map(|entry| {
            (entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/new"))
                .then(|| entry.pointer("/recv").unwrap())
        })
        .unwrap();
    assert_eq!(
        session_new.pointer("/params/cwd").and_then(Value::as_str),
        Some(root.to_string_lossy().as_ref())
    );
    assert_eq!(session_new.pointer("/params/mcpServers"), Some(&json!([])));

    for kind in [
        CodingAgentEventKind::AgentMessage,
        CodingAgentEventKind::Reasoning,
        CodingAgentEventKind::Plan,
        CodingAgentEventKind::TerminalActivity,
        CodingAgentEventKind::FileChange,
        CodingAgentEventKind::Usage,
        CodingAgentEventKind::Terminal,
    ] {
        assert!(
            observation.events.iter().any(|event| event.kind == kind),
            "missing {kind:?}"
        );
    }
    let usage = observation
        .events
        .iter()
        .find(|event| event.kind == CodingAgentEventKind::Usage)
        .and_then(|event| event.usage.as_ref())
        .unwrap();
    assert_eq!(usage.used_tokens, Some(53));
    assert_eq!(usage.context_window_tokens, Some(200));
    assert_eq!(usage.cost_amount.as_deref(), Some("0.045"));
    assert_eq!(usage.cost_currency.as_deref(), Some("USD"));
}

#[test]
#[cfg(unix)]
fn explicit_config_is_ordered_and_invalid_config_never_prompts() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_config000001";
    let response = manager.handle(
        start_request(
            &manager,
            &root,
            run,
            BTreeMap::from([(
                "mode".to_string(),
                CodingAgentConfigValue::String("read-only".to_string()),
            )]),
        ),
        &projects,
    );
    assert!(response.error.is_none());
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(
        received_methods(&wire_log(&temp)),
        vec![
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/prompt"
        ]
    );

    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_badconfig001";
    let response = manager.handle(
        start_request(
            &manager,
            &root,
            run,
            BTreeMap::from([(
                "not-advertised".to_string(),
                CodingAgentConfigValue::String("x".to_string()),
            )]),
        ),
        &projects,
    );
    assert!(response.error.is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal.execution_state,
        CodingAgentExecutionState::NotStarted
    );
    assert!(!received_methods(&wire_log(&temp))
        .iter()
        .any(|method| method == "session/prompt"));
}

#[test]
#[cfg(unix)]
fn cancel_permission_and_unsupported_requests_are_fail_closed() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "permission_hold");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_permcancel01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects
        )
        .error
        .is_none());
    wait_for_snapshot(&manager, run, |snapshot| {
        snapshot.state == CodingAgentRunState::WaitingPermission
    });
    let cancel = manager.handle(
        CodingAgentRequest::Cancel(CodingAgentCancelRequest {
            run_id: run.to_string(),
        }),
        &projects,
    );
    assert!(cancel.error.is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Cancelled);
    let log = wire_log(&temp);
    let permission_cancel = log
        .iter()
        .position(|entry| {
            entry.pointer("/recv/id").and_then(Value::as_u64) == Some(99)
                && entry
                    .pointer("/recv/result/outcome/outcome")
                    .and_then(Value::as_str)
                    == Some("cancelled")
        })
        .unwrap();
    let prompt_cancel = log
        .iter()
        .position(|entry| {
            entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/cancel")
        })
        .unwrap();
    assert!(
        permission_cancel < prompt_cancel,
        "pending permission must be completed before prompt cancel"
    );

    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "unsupported_callback");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_unsupported1";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Cancelled);
    let log = wire_log(&temp);
    assert!(log
        .iter()
        .any(|entry| entry.pointer("/recv/error/code").and_then(Value::as_i64) == Some(-32601)));
    assert!(log.iter().any(
        |entry| entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/cancel")
    ));
}

#[test]
fn bounded_text_never_exceeds_utf8_byte_budget() {
    let max = webcodex_core::coding_agent::CODING_AGENT_MAX_EVENT_TEXT_BYTES;
    let cases = [
        "a".repeat(max),
        "a".repeat(max + 1),
        "é".repeat(max / "é".len() + 2),
        "€".repeat(max / "€".len() + 2),
        "🦀".repeat(max / "🦀".len() + 2),
        "z".repeat(max * 4),
    ];
    for input in cases {
        let output = bounded_text(&input);
        assert!(output.len() <= max, "{} > {max}", output.len());
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
        if input.len() <= max {
            assert_eq!(output, input);
        } else {
            assert!(output.ends_with('…'));
        }
    }
}

#[test]
#[cfg(unix)]
fn event_ring_capacity_and_continuation_are_bounded() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "many_events");
    let cfg = fake_config(exe, args);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_manyevents01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects
        )
        .error
        .is_none());
    wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    let entry = manager.runs.lock().unwrap().get(run).unwrap().clone();
    let first = entry.observe(Some(0), 32, 0).unwrap();
    assert!(first.run.state.terminal());
    assert!(first.history_lost);
    assert!(first.has_more);
    assert_eq!(first.events.len(), 32);
    assert!(first.first_retained_sequence > 1);
    let second = entry.observe(Some(first.next_sequence), 32, 0).unwrap();
    assert!(second.run.state.terminal());
    assert!(!second.events.is_empty());
    assert!(second.events.first().unwrap().sequence > first.events.last().unwrap().sequence);

    let latest = entry.state.lock().unwrap().next_sequence.saturating_sub(1);
    let error = entry
        .observe(Some(latest.saturating_add(1)), 32, 0)
        .unwrap_err();
    assert!(
        error.contains("ahead of latest emitted sequence"),
        "{error}"
    );
    let response = manager.handle(
        CodingAgentRequest::Observe(webcodex_core::coding_agent::CodingAgentObserveRequest {
            run_id: run.to_string(),
            after_sequence: Some(latest.saturating_add(1)),
            limit: 32,
            wait_secs: 0,
        }),
        &projects,
    );
    assert_eq!(
        response.error.as_ref().map(|error| error.code.as_str()),
        Some("invalid_coding_agent_observation_cursor")
    );
}

#[test]
#[cfg(unix)]
fn forced_config_is_applied_before_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe, args);
    force_policy(&mut cfg);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedapply01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Completed);
    assert_eq!(
        received_config_ids(&wire_log(&temp)),
        vec!["model".to_string(), "reasoning_effort".to_string()]
    );
    assert_eq!(
        received_methods(&wire_log(&temp)),
        vec![
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/set_config_option",
            "session/prompt",
        ]
    );
}

#[test]
#[cfg(unix)]
fn missing_forced_config_fails_without_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "end");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("policy-model".to_string()),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedmissing01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_not_advertised")
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn forced_config_must_be_reflected_by_provider() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_not_applied");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("policy-model".to_string()),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedreflect01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_not_applied")
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn caller_may_repeat_but_not_override_forced_config() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("policy-model".to_string()),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let same_run = "wc_agent_run_forcedsame0001";
    assert!(manager
        .handle(
            start_request(
                &manager,
                &root,
                same_run,
                BTreeMap::from([(
                    "model".to_string(),
                    CodingAgentConfigValue::String("policy-model".to_string()),
                )]),
            ),
            &projects,
        )
        .error
        .is_none());
    assert_eq!(
        wait_for_snapshot(&manager, same_run, |snapshot| snapshot.state.terminal()).state,
        CodingAgentRunState::Completed
    );

    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("policy-model".to_string()),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let conflict_run = "wc_agent_run_forcedconflict01";
    assert!(manager
        .handle(
            start_request(
                &manager,
                &root,
                conflict_run,
                BTreeMap::from([(
                    "model".to_string(),
                    CodingAgentConfigValue::String("default-model".to_string()),
                )]),
            ),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, conflict_run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_conflict")
    );
    assert!(received_config_ids(&wire_log(&temp)).is_empty());
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn forced_config_is_reasserted_after_caller_side_effects() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_reset_by_caller");
    let mut cfg = fake_config(exe, args);
    force_policy(&mut cfg);
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedreassert01";
    assert!(manager
        .handle(
            start_request(
                &manager,
                &root,
                run,
                BTreeMap::from([(
                    "mode".to_string(),
                    CodingAgentConfigValue::String("read-only".to_string()),
                )]),
            ),
            &projects,
        )
        .error
        .is_none());
    assert_eq!(
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal()).state,
        CodingAgentRunState::Completed
    );
    assert_eq!(
        received_config_ids(&wire_log(&temp)),
        vec![
            "model".to_string(),
            "reasoning_effort".to_string(),
            "mode".to_string(),
            "model".to_string(),
            "reasoning_effort".to_string(),
        ]
    );
}

#[test]
#[cfg(unix)]
fn global_forced_config_is_multi_provider_admission_policy() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe.clone(), args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("policy-model".to_string()),
    );
    cfg.agents.push(AcpAgentConfig {
        id: "limited".to_string(),
        name: "Limited".to_string(),
        executable: exe,
        args: vec!["end".to_string()],
        env_from_env: BTreeMap::new(),
        allowed_config_options: vec!["mode".to_string()],
    });
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();

    let supported_run = "wc_agent_run_forcedmultiok";
    assert!(manager
        .handle(
            start_request_for_provider(&manager, &root, supported_run, "codex", BTreeMap::new(),),
            &projects,
        )
        .error
        .is_none());
    assert_eq!(
        wait_for_snapshot(&manager, supported_run, |snapshot| snapshot
            .state
            .terminal())
        .state,
        CodingAgentRunState::Completed
    );
    let prompts_after_supported = received_methods(&wire_log(&temp))
        .iter()
        .filter(|method| method.as_str() == "session/prompt")
        .count();
    assert_eq!(prompts_after_supported, 1);

    let limited_run = "wc_agent_run_forcedmultifail";
    assert!(manager
        .handle(
            start_request_for_provider(&manager, &root, limited_run, "limited", BTreeMap::new(),),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, limited_run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_not_advertised")
    );
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        1,
        "unsupported provider must fail before prompt dispatch"
    );
}

#[test]
#[cfg(unix)]
fn forced_boolean_config_is_applied_before_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "feature_flag".to_string(),
        CodingAgentConfigValue::Bool(true),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedbool0001";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Completed);
    assert_eq!(
        received_config_ids(&wire_log(&temp)),
        vec!["feature_flag".to_string()]
    );
    let log = wire_log(&temp);
    let set = log
        .iter()
        .filter_map(|entry| entry.get("recv"))
        .find(|recv| {
            recv.get("method").and_then(Value::as_str) == Some("session/set_config_option")
        })
        .unwrap();
    assert_eq!(
        set.pointer("/params/type").and_then(Value::as_str),
        Some("boolean")
    );
    assert_eq!(
        set.pointer("/params/value").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
#[cfg(unix)]
fn duplicate_forced_config_id_fails_closed_without_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_duplicate");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "feature_flag".to_string(),
        CodingAgentConfigValue::Bool(true),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedduplicate01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_invalid")
    );
    assert!(received_config_ids(&wire_log(&temp)).is_empty());
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn illegal_forced_select_value_fails_without_prompt() {
    let temp = crate::tests::executable_tempdir();
    let (exe, args) = fake_agent(&temp, "forced_configs");
    let mut cfg = fake_config(exe, args);
    cfg.forced_config.insert(
        "model".to_string(),
        CodingAgentConfigValue::String("not-advertised-model".to_string()),
    );
    let projects = project_fixture(&temp);
    let root = temp.path().join("repo");
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_forcedinvalid01";
    assert!(manager
        .handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        )
        .error
        .is_none());
    let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    assert_eq!(terminal.state, CodingAgentRunState::Failed);
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.error_code.as_deref()),
        Some("coding_agent_forced_config_invalid")
    );
    assert!(received_config_ids(&wire_log(&temp)).is_empty());
    assert_eq!(
        received_methods(&wire_log(&temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count(),
        0
    );
}

#[test]
#[cfg(unix)]
fn fake_acp_normalizes_activity_and_terminal_matrix() {
    let (manager, run, obs) = run_scenario("end", BTreeMap::new());
    assert_eq!(obs.run.state, CodingAgentRunState::Completed);
    let durable = manager.store.read(&run).unwrap().unwrap();
    assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
    assert_eq!(durable.state, CodingAgentRunState::Completed);
    assert_eq!(
        durable.execution_state,
        CodingAgentExecutionState::Completed
    );
    assert_eq!(durable.terminal, obs.run.terminal);
    assert!(obs
        .events
        .iter()
        .any(|e| e.kind == CodingAgentEventKind::AgentMessage));
    assert!(obs
        .events
        .iter()
        .any(|e| e.kind == CodingAgentEventKind::Reasoning));
    assert!(obs
        .events
        .iter()
        .any(|e| e.kind == CodingAgentEventKind::TerminalActivity));
    for scenario in [
        "cancelled",
        "max_tokens",
        "max_turn_requests",
        "refusal",
        "unknown",
    ] {
        let (manager, run, obs) = run_scenario(scenario, BTreeMap::new());
        let expected = if scenario == "cancelled" {
            CodingAgentRunState::Cancelled
        } else {
            CodingAgentRunState::Failed
        };
        assert_eq!(obs.run.state, expected);
        assert_eq!(
            obs.run.execution_state,
            CodingAgentExecutionState::Completed
        );
        let durable = manager.store.read(&run).unwrap().unwrap();
        assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
        assert_eq!(durable.state, expected);
        assert_eq!(
            durable.execution_state,
            CodingAgentExecutionState::Completed
        );
        assert_eq!(durable.terminal, obs.run.terminal);
    }
}

#[test]
#[cfg(unix)]
fn config_and_permission_paths_are_fail_closed() {
    let (_, _, obs) = run_scenario(
        "end",
        BTreeMap::from([(
            "mode".to_string(),
            CodingAgentConfigValue::String("read-only".to_string()),
        )]),
    );
    assert_eq!(obs.run.state, CodingAgentRunState::Completed);
    let (_, _, permission) = run_scenario("permission", BTreeMap::new());
    assert_eq!(permission.run.state, CodingAgentRunState::Cancelled);
    assert!(permission
        .events
        .iter()
        .any(|e| e.kind == CodingAgentEventKind::PermissionRequest));
}
