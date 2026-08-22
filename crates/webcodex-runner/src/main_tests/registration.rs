use super::*;

fn bootstrap_project(index: usize) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: format!("project-{index:04}"),
        name: Some(format!("Project {index:04}")),
        path: format!("/tmp/project-{index:04}"),
        allow_patch: true,
        kind: Some("repo".to_string()),
        description: None,
        hooks: Vec::new(),
        disabled: false,
        revision: Some(format!("sha256:{}", "a".repeat(64))),
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: 1,
        shell_profile: None,
    }
}

fn bootstrap_active_job(project_index: usize) -> ShellJobSnapshot {
    ShellJobSnapshot {
        job_id: "job-bootstrap-active".to_string(),
        request_id: "request-bootstrap-active".to_string(),
        status: "running".to_string(),
        update_seq: 1,
        created_at: 1_700_000_000,
        started_at: Some(1_700_000_001),
        ended_at: None,
        exit_code: None,
        duration_ms: None,
        error: None,
        command_execution_state: None,
        context: ShellJobContext {
            runtime_project_id: Some(format!("agent:oe:project-{project_index:04}")),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(format!("/tmp/project-{project_index:04}")),
            cwd: Some(format!("/tmp/project-{project_index:04}")),
            purpose: Some("test".to_string()),
            shell: Some("bash".to_string()),
            command_preview: "sleep 30".to_string(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: None,
        },
        stdout: ShellJobStreamSnapshot::default(),
        stderr: ShellJobStreamSnapshot::default(),
        validation_progress: None,
    }
}

#[test]
fn large_inventory_registration_bootstrap_keeps_active_job_project_within_legacy_bound() {
    let projects = (0..65).map(bootstrap_project).collect::<Vec<_>>();
    let job_inventory = ShellJobInventory {
        active_complete: true,
        jobs: vec![bootstrap_active_job(64)],
    };

    let bootstrap = project_registration_bootstrap("oe", &projects, &job_inventory);
    assert!(bootstrap.paged_inventory);
    let summaries = bootstrap.projects.expect("active Job project bootstrap");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "project-0064");
    assert!(summaries.len() <= PROJECT_INVENTORY_INLINE_MAX_SUMMARIES);
    assert!(
        serde_json::to_vec(&summaries).unwrap().len()
            <= PROJECT_REGISTRATION_BOOTSTRAP_MAX_SERIALIZED_BYTES
    );
    assert_eq!(bootstrap.job_inventory.jobs.len(), 1);
    assert_eq!(
        bootstrap.job_inventory.jobs[0]
            .context
            .runtime_project_id
            .as_deref(),
        Some("agent:oe:project-0064")
    );
}

#[test]
fn mcp_gateway_register_request_projects_bounded_provider_inventory_without_local_launch_details() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.mcp_gateway.providers = vec![webcodex_runner::config::McpGatewayProviderConfig {
        id: "local-tools".to_string(),
        name: "Local tools".to_string(),
        executable: "/private/operator/bin/local-tools-mcp".to_string(),
        args: vec!["--secret-profile".to_string()],
        cwd: Some("/private/operator/provider-workdir".to_string()),
        env_from_env: std::collections::BTreeMap::from([(
            "GITHUB_TOKEN".to_string(),
            "OPERATOR_GITHUB_TOKEN".to_string(),
        )]),
        timeout_secs: Some(5),
    }];

    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "runner-instance",
        0,
    );
    let providers = body
        .policy
        .as_ref()
        .and_then(|policy| policy.mcp_gateway_providers.as_ref())
        .expect("MCP provider inventory");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].provider_id, "local-tools");
    assert_eq!(providers[0].name, "Local tools");
    assert!(!providers[0].provider_instance_id.is_empty());
    let serialized = serde_json::to_string(providers).unwrap();
    assert!(!serialized.contains("/private/operator/bin"));
    assert!(!serialized.contains("--secret-profile"));
    assert!(!serialized.contains("/private/operator/provider-workdir"));
    assert!(!serialized.contains("GITHUB_TOKEN"));
    assert!(!serialized.contains("OPERATOR_GITHUB_TOKEN"));
    assert!(!serialized.contains("timeout_secs"));
}

#[test]
fn computer_register_request_announces_platform_capability_and_protocol_version() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // A stale or hand-edited config cannot force capability advertisement:
    // registration replaces it with the result of the real host probe.
    cfg.capabilities = Some(ShellClientCapabilities {
        sandbox_inspect_commands: true,
        computer_observe: true,
        computer_application_discovery: true,
        computer_application_launch: true,
        computer_display_observe: true,
        computer_snapshot_region: true,
        computer_accessibility_observe: true,
        computer_element_state: true,
        computer_control: true,
        computer_scroll_to_element: true,
        computer_key_input: true,
        computer_window_activate: true,
        computer_text_input: true,
        project_lifecycle: false,
        project_path_registration: false,
        ..Default::default()
    });
    for (version, expected_str) in [
        (AGENT_PROTOCOL_VERSION_POLLING_V1, "polling-v1"),
        (AGENT_PROTOCOL_VERSION_WEBSOCKET_V1, "websocket-v1"),
        (AGENT_PROTOCOL_VERSION_QUIC_V1, "quic-v1"),
    ] {
        let body = build_register_request(&cfg, Vec::new(), version, "inst-1", 0);
        let caps = body.capabilities.as_ref().expect("transport capabilities");
        assert!(caps.structured_go_test_tool, "{expected_str}");
        assert!(caps.structured_go_test_packages, "{expected_str}");
        assert!(caps.structured_file_delete, "{expected_str}");
        assert_eq!(body.agent_instance_id, "inst-1");
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(version),
            "version mismatch for {expected_str}"
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some(expected_str));
    }
    // Also verify capabilities are advertised (check once for polling).
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let caps = body.capabilities.expect("agent registers capabilities");
    assert!(caps.shell);
    assert!(caps.file_read);
    assert!(caps.file_write);
    assert!(caps.artifact_export_chunk_read);
    assert!(caps.artifact_export_streaming_metadata);
    assert!(caps.structured_file_delete);
    assert!(caps.async_jobs);
    assert!(caps.async_shell_jobs);
    assert!(caps.structured_validation_argv);
    assert!(caps.structured_go_test_json);
    assert!(caps.structured_go_test_tool);
    assert!(caps.structured_go_test_packages);
    assert!(caps.structured_process_argv);
    assert!(caps.structured_script_payload);
    assert!(caps.internal_posix_script);
    assert!(caps.structured_execution_jobs);
    assert!(caps.lsp_read_only_navigation);
    assert!(caps.lsp_call_hierarchy);
    assert!(caps.project_lifecycle);
    assert!(caps.project_path_registration);
    assert_eq!(
        caps.computer_observe,
        cfg!(any(target_os = "macos", windows)),
        "computer observation is advertised only when this Runner binary has a supported native implementation"
    );
    assert_eq!(
        caps.computer_application_discovery,
        cfg!(any(target_os = "macos", windows)),
        "computer application discovery is advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_application_launch,
        cfg!(any(target_os = "macos", windows)),
        "computer application launch is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_clipboard_read,
        cfg!(any(target_os = "macos", windows)),
        "clipboard read is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_clipboard_write,
        cfg!(any(target_os = "macos", windows)),
        "clipboard write is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_display_observe,
        cfg!(any(target_os = "macos", windows)),
        "full-display observation is independently advertised only by exact native macOS/Windows display backends"
    );
    assert_eq!(
        caps.computer_snapshot_region,
        cfg!(any(target_os = "macos", windows)),
        "computer region snapshot is independently advertised only when native window capture is supported"
    );
    assert_eq!(
        caps.computer_accessibility_observe,
        cfg!(any(target_os = "macos", windows)),
        "computer accessibility observation is advertised only by native AX/UIA implementations"
    );
    assert_eq!(
        caps.computer_element_state,
        cfg!(any(target_os = "macos", windows)),
        "computer element state is independently advertised only by native AX/UIA implementations"
    );
    assert_eq!(
        caps.computer_control,
        cfg!(any(target_os = "macos", windows)),
        "computer control is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_scroll_to_element,
        cfg!(any(target_os = "macos", windows)),
        "computer scroll-to-element is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_key_input,
        cfg!(any(target_os = "macos", windows)),
        "computer key input is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_window_activate,
        cfg!(any(target_os = "macos", windows)),
        "computer window activation is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_text_input,
        cfg!(any(target_os = "macos", windows)),
        "computer text input is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.sandbox_inspect_commands,
        crate::command_sandbox::inspect_sandbox_available().is_ok()
    );
}

#[test]
fn phase_e2_register_request_reports_effective_job_concurrency_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    for (configured, expected) in [
        (None, 4),
        (Some(0), 1),
        (Some(1), 1),
        (Some(8), 8),
        (Some(64), 64),
        (Some(65), 64),
        (Some(128), 64),
    ] {
        cfg.max_concurrent_jobs = configured;
        for protocol in [
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            AGENT_PROTOCOL_VERSION_QUIC_V1,
        ] {
            let body = build_register_request(&cfg, Vec::new(), protocol, "inst-limit", 0);
            assert_eq!(
                body.job_concurrency_limit,
                Some(expected),
                "configured={configured:?} protocol={protocol}"
            );
        }
    }
}

#[test]
fn register_request_carries_sanitized_shell_profiles_summary() {
    // A config with one profile carrying a secret env value and a secret
    // init_script body. The sanitized summary must report the profile name,
    // has_init_script=true, and env_keys_count, but MUST NOT include the env
    // value or the init_script body.
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    let secret_env = "DO_NOT_LEAK_THIS_ENV_VALUE";
    let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
    cfg.shell = shell_with_profiles(
        Some("rust"),
        vec![(
            "rust",
            ShellProfileConfig {
                program: Some("sh".to_string()),
                args: Some(vec!["-c".to_string()]),
                env: profile_env(&[("SECRET_KEY", secret_env)]),
                init_script: Some(secret_script.to_string()),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let policy = body.policy.expect("agent registers a policy");
    let summary = policy
        .shell_profiles
        .as_ref()
        .expect("sanitized shell profiles summary is present");
    assert_eq!(summary.default_profile.as_deref(), Some("rust"));
    assert_eq!(summary.configured_count, 1);
    assert_eq!(summary.profiles.len(), 1);
    let entry = &summary.profiles[0];
    assert_eq!(entry.name, "rust");
    assert!(entry.has_init_script);
    assert_eq!(entry.env_keys_count, 1);
    assert_eq!(entry.program, "sh");
    assert_eq!(entry.args_count, 1);
    // Sanitization: the rendered summary never carries env values or the
    // init_script body.
    let rendered = serde_json::to_string(summary).unwrap();
    assert!(!rendered.contains(secret_env), "{rendered}");
    assert!(!rendered.contains(secret_script), "{rendered}");
}
