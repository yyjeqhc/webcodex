use super::*;

#[test]
#[ignore = "opt-in real Codex ACP dogfood; requires local Codex auth and network"]
#[cfg(unix)]
fn real_codex_acp_opt_in_dogfood() {
    let temp = TempDir::new().unwrap();
    let root = std::env::current_dir().unwrap();
    let projects = temp.path().join("project-registry");
    fs::create_dir_all(&projects).unwrap();
    fs::write(
        projects.join("dogfood.toml"),
        format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
    )
    .unwrap();

    let mut env_from_env = BTreeMap::new();
    // This list is dogfood/test-owned and intentionally explicit. Production
    // providers inherit nothing unless the operator declares each mapping.
    for name in [
        "HOME",
        "PATH",
        "USER",
        "SHELL",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ] {
        if std::env::var_os(name).is_some() {
            env_from_env.insert(name.to_string(), name.to_string());
        }
    }
    let cfg = AcpConfig {
        max_concurrent_runs: 1,
        permission_timeout_secs: 3,
        forced_config: BTreeMap::new(),
        agents: vec![AcpAgentConfig {
            id: "codex".to_string(),
            name: "Codex ACP dogfood".to_string(),
            executable: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@agentclientprotocol/codex-acp".to_string(),
            ],
            env_from_env,
            allowed_config_options: Vec::new(),
        }],
    };
    let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
    let run = "wc_agent_run_realcodexdogfood01";
    let provider = manager.providers().remove(0);
    let request = CodingAgentRequest::Start(webcodex_core::coding_agent::CodingAgentStartRequest {
            run_id: run.to_string(),
            intent_fingerprint: "real-codex-dogfood-v1".to_string(),
            authority_fingerprint: "auth_real_codex_dogfood".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            project_root: root.to_string_lossy().into_owned(),
            provider_id: "codex".to_string(),
            provider_instance_id: provider.provider_instance_id,
            instruction: "Read Cargo.toml only and reply with the WebCodex package version in one short sentence. Do not modify files, run builds, install dependencies, or request elevated permissions.".to_string(),
            config: BTreeMap::new(),
            timeout_secs: 180,
        });
    let admitted = manager.handle(request, &projects);
    assert!(
        admitted.error.is_none(),
        "admission failed: {:?}",
        admitted.error
    );
    let deadline = Instant::now() + Duration::from_secs(150);
    let terminal = loop {
        let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        if snapshot.state.terminal() {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "real Codex ACP dogfood timed out: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(100));
    };
    let observation = manager
        .runs
        .lock()
        .unwrap()
        .get(run)
        .unwrap()
        .observe(None, 64, 0)
        .expect("retained real Run cursor must be valid");
    assert_eq!(
        terminal.state,
        CodingAgentRunState::Completed,
        "terminal={terminal:?}; events={:?}",
        observation.events
    );
    assert_eq!(
        terminal
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.stop_reason.as_deref()),
        Some("end_turn")
    );
    assert!(
        observation
            .events
            .iter()
            .any(|event| event.kind == CodingAgentEventKind::AgentMessage),
        "real Codex ACP produced no normalized agent message: {:?}",
        observation.events
    );
    assert!(
        observation.events.iter().any(|event| matches!(
            event.kind,
            CodingAgentEventKind::ToolActivity
                | CodingAgentEventKind::TerminalActivity
                | CodingAgentEventKind::Usage
        )),
        "real Codex ACP produced no normalized activity: {:?}",
        observation.events
    );
}
