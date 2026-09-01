use super::*;

#[test]
fn dispatch_request_run_shell_sends_result_over_sink() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );

    type SinkFactory = fn(&str) -> (RunnerSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, client_id, expected_stdout) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "wsok"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-ok"),
    ] {
        let (sink, mut rx) = make_sink(client_id);
        let request = ShellAgentShellRequest {
            request_id: format!("req-{label}"),
            client_id: client_id.to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some(tmp.path().to_string_lossy().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: shell_echo(expected_stdout),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            job_context: None,
            mcp_gateway: None,
            coding_agent: None,
            persistent_shell: None,
        };
        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();
        assert!(ran, "{label}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.request_id, format!("req-{label}"));
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some(expected_stdout));
                assert_eq!(
                    payload.command_execution_state,
                    Some(ShellCommandExecutionState::Completed)
                );
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn dispatch_request_detached_process_job_enters_job_manager_without_generic_result() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-detached-dispatch".to_string(),
        client_id: "ws-client".to_string(),
        kind: "start_detached_process_job".to_string(),
        job_id: Some("job-detached-dispatch".to_string()),
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: Some(shell_protocol::ShellProcessArgv {
            executable: "never-started".to_string(),
            args: Vec::new(),
        }),
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("detached Job rejection update") {
        AgentEnvelope::JobUpdate { payload } => {
            assert_eq!(payload.job_id, "job-detached-dispatch");
            assert_eq!(payload.status, "failed");
            assert!(payload
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("missing recovery context"));
        }
        other => panic!(
            "detached Job dispatch must enqueue into JobManager, got {}",
            other.kind()
        ),
    }
}

#[test]
fn dispatch_request_internal_search_uses_posix_runtime_not_configured_shell_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // The generated search program is POSIX shell. It must not inherit an
    // arbitrary configured shell parser (PowerShell is the Windows production
    // case); use a guaranteed-failing program here to prove the bypass.
    cfg.shell.program = if cfg!(windows) {
        "powershell".to_string()
    } else {
        "/bin/false".to_string()
    };
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let marker = r#"{"webcodex_search":{"backend":"grep","feature_unavailable":false}}"#;
    let request = ShellAgentShellRequest {
        request_id: "req-internal-search".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: format!(
            "{}\nprintf '%s\\n' '{}'\n",
            shell_protocol::EXTERNAL_SEARCH_REQUEST_PREFIX,
            marker
        ),
        process: None,
        script: None,
        stdin: Some("{}".to_string()),
        timeout_secs: if cfg!(windows) { 30 } else { 10 },
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("internal search result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert!(payload
                .result
                .stdout
                .as_deref()
                .unwrap_or_default()
                .contains("webcodex_search"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[cfg(unix)]
#[test]
fn dispatch_request_internal_posix_script_ignores_configured_shell_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.shell.program = "/bin/false".to_string();
    cfg.shell.dialect = Some(crate::webcodex_runner::config::ShellDialect::PowerShell);
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-internal-posix".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_internal_posix_script".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: Some(shell_protocol::ShellScriptPayload {
            language: shell_protocol::ShellScriptLanguage::Sh,
            script: "printf 'internal-posix-dispatch-ok\\n'\n".to_string(),
            args: Vec::new(),
        }),
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("internal POSIX result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(
                payload.result.stdout.as_deref(),
                Some("internal-posix-dispatch-ok\n")
            );
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_run_shell_rejects_oversized_wire_command_before_start() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-oversized-shell".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: "x".repeat(shell_protocol::RAW_SHELL_WIRE_MAX_BYTES + 1),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    let env = rx.try_recv().expect("rejection envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-oversized-shell");
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
            assert!(payload
                .result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("invalid_raw_shell_request"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_structured_process_uses_typed_argv_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = tmp.path().join(format!(
        "process-argv-helper{}",
        std::env::consts::EXE_SUFFIX
    ));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/process_argv_helper.rs");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_process_argv_helper")
        .arg(fixture)
        .arg("-o")
        .arg(&helper)
        .output()
        .expect("run rustc for process argv helper");
    assert!(
        compile.status.success(),
        "process argv helper compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let marker = tmp.path().join("marker");

    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-structured-process".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: Some(shell_protocol::ShellProcessArgv {
            executable: helper.to_string_lossy().into_owned(),
            args: vec![
                "argv".to_string(),
                "$(touch marker)".to_string(),
                "; touch marker".to_string(),
            ],
        }),
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured process result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
            let stdout = payload.result.stdout.unwrap();
            assert!(stdout.contains("$(touch marker)"));
            assert!(stdout.contains("; touch marker"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());

    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");
    let (sink, mut rx) = ws_sink("ws-client");
    let malformed = ShellAgentShellRequest {
        request_id: "req-structured-process-malformed".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: shell_write_file(&shell_fallback_marker),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured process rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
}

#[cfg(unix)]
#[test]
fn dispatch_request_structured_script_uses_typed_file_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let observed_path = tmp.path().join("observed-script-path");
    let marker = tmp.path().join("marker");
    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");

    let request = ShellAgentShellRequest {
        request_id: "req-structured-script".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_script".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: Some(shell_protocol::ShellScriptPayload {
            language: shell_protocol::ShellScriptLanguage::Sh,
            script: "printf '%s' \"$0\" > \"$1\"\nprintf '%s\\n' \"$2\"\n".to_string(),
            args: vec![
                observed_path.to_string_lossy().into_owned(),
                "; touch marker".to_string(),
            ],
        }),
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };
    let mut malformed = request.clone();
    malformed.request_id = "req-structured-script-malformed".to_string();
    malformed.command = shell_write_file(&shell_fallback_marker);
    malformed.script = None;

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured script result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(payload.result.stdout.as_deref(), Some("; touch marker\n"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());
    let temporary_path =
        PathBuf::from(std::fs::read_to_string(&observed_path).expect("script path evidence"));
    assert!(!temporary_path.starts_with(tmp.path()));
    assert!(!temporary_path.exists());

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured script rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
}
