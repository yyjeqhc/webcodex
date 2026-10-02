use super::*;
use std::collections::BTreeMap;
use webcodex_core::runner_protocol::RunnerRequest;

fn request(action: &str, shell_id: &str, command: Option<&str>) -> RunnerRequest {
    RunnerRequest {
        login: false,
        request_id: format!("req-{action}"),
        client_id: "agent-1".to_string(),
        kind: "persistent_shell".to_string(),
        job_id: None,
        cwd: None,
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: command.unwrap_or_default().to_string(),
        shell: None,
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 5,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        plugin_gateway: None,
        coding_agent: None,
        persistent_shell: Some(PersistentShellRequest {
            action: action.to_string(),
            shell_id: shell_id.to_string(),
            workflow_session_id: "wc_sess_n_gsG5blnjZHfyYD".to_string(),
            runtime_project_id: "agent:agent-1:demo".to_string(),
            cwd: None,
            shell: Some("bash".to_string()),
            command: command.map(str::to_string),
            timeout_secs: Some(5),
            purpose: None,
        }),
    }
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, RunnerPolicy) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let projects = temp.path().join("project-registry");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(project.join("sub")).unwrap();
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(
        projects.join("demo.toml"),
        format!("id = \"demo\"\npath = \"{}\"\n", project.display()),
    )
    .unwrap();
    let policy = RunnerPolicy {
        allow_raw_shell: true,
        allow_cwd_anywhere: false,
        allowed_roots: vec![project.clone()],
        max_timeout_secs: 30,
        max_output_bytes: 16 * 1024,
    };
    (temp, project, projects, policy)
}

#[test]
fn runner_preserves_state_and_rechecks_raw_shell_policy() {
    let (_temp, _project, projects, policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    let mut denied_open_policy = policy.clone();
    denied_open_policy.allow_raw_shell = false;
    let denied_open = manager.handle(
        &denied_open_policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("open", "wc_shell_denied_open", None),
    );
    assert_eq!(
        denied_open.error_code.as_deref(),
        Some("raw_shell_disabled")
    );
    assert_eq!(manager.active_count(), 0);

    let opened = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("open", "wc_shell_runner", None),
    );
    assert_eq!(opened.shell_state, "running");

    let exported = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_runner",
            Some("export WC_RUNNER_STATE=ready; cd sub; wc_runner_fn() { printf fn; }"),
        ),
    );
    assert_eq!(exported.exit_code, Some(0));
    let observed = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_runner",
            Some("printf '%s:%s:' \"$WC_RUNNER_STATE\" \"$PWD\"; wc_runner_fn"),
        ),
    );
    assert!(observed.stdout.starts_with("ready:"));
    assert!(observed.stdout.ends_with(":fn"));

    let mut denied_policy = policy;
    denied_policy.allow_raw_shell = false;
    let denied = manager.handle(
        &denied_policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("exec", "wc_shell_runner", Some("printf denied")),
    );
    assert_eq!(denied.error_code.as_deref(), Some("raw_shell_disabled"));
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn rejected_exec_keeps_authoritative_running_state() {
    let (_temp, _project, projects, policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    assert_eq!(
        manager
            .handle(
                &policy,
                &shell,
                &SshConfig::default(),
                1,
                &projects,
                &request("open", "wc_shell_rejected_exec", None),
            )
            .shell_state,
        "running"
    );

    let mut invalid = request("exec", "wc_shell_rejected_exec", Some("printf ignored"));
    invalid.persistent_shell.as_mut().unwrap().timeout_secs = Some(policy.max_timeout_secs + 1);
    let rejected = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &invalid,
    );
    assert_eq!(
        rejected.error_code.as_deref(),
        Some("persistent_shell_invalid_timeout")
    );
    assert_eq!(rejected.shell_state, "running");
    assert_eq!(manager.active_count(), 1);

    let oversized = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_rejected_exec",
            Some(&"x".repeat(RAW_SHELL_COMMAND_MAX_BYTES + 1)),
        ),
    );
    assert_eq!(
        oversized.error_code.as_deref(),
        Some("persistent_shell_invalid_command")
    );
    assert_eq!(oversized.shell_state, "running");

    let observed = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_rejected_exec",
            Some("printf still-running"),
        ),
    );
    assert_eq!(observed.stdout, "still-running");
}

#[test]
fn exec_reapplies_current_output_limit() {
    let (_temp, _project, projects, mut policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    assert_eq!(
        manager
            .handle(
                &policy,
                &shell,
                &SshConfig::default(),
                1,
                &projects,
                &request("open", "wc_shell_output_policy", None),
            )
            .shell_state,
        "running"
    );

    policy.max_output_bytes = 1024;
    let result = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_output_policy",
            Some("i=0; while [ \"$i\" -lt 5000 ]; do printf x; i=$((i+1)); done"),
        ),
    );
    assert!(result.stdout_truncated);
    assert!(result.stdout.len() <= policy.max_output_bytes);
}

#[test]
fn runner_profile_initialization_runs_once_at_open() {
    let (_temp, _project, projects, policy) = fixture();
    let mut profiles = BTreeMap::new();
    profiles.insert(
        "persistent".to_string(),
        ShellProfileConfig {
            program: Some("bash".to_string()),
            init_script: Some("WC_PROFILE_COUNT=1; export WC_PROFILE_COUNT".to_string()),
            ..ShellProfileConfig::default()
        },
    );
    let mut shell = ShellConfig {
        default_profile: Some("persistent".to_string()),
        profiles,
        ..ShellConfig::default()
    };
    shell.max_persistent_shells = 2;
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    assert_eq!(
        manager
            .handle(
                &policy,
                &shell,
                &SshConfig::default(),
                1,
                &projects,
                &request("open", "wc_shell_profile", None),
            )
            .shell_state,
        "running"
    );
    let first = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_profile",
            Some("printf %s \"$WC_PROFILE_COUNT\"; WC_PROFILE_COUNT=2"),
        ),
    );
    assert_eq!(first.stdout, "1");
    let second = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_profile",
            Some("printf %s \"$WC_PROFILE_COUNT\""),
        ),
    );
    assert_eq!(second.stdout, "2");
}

#[test]
fn runner_rejects_profile_initialization_outside_project() {
    let (_temp, _project, projects, policy) = fixture();
    let mut profiles = BTreeMap::new();
    profiles.insert(
        "escaping".to_string(),
        ShellProfileConfig {
            init_script: Some("cd ..".to_string()),
            ..ShellProfileConfig::default()
        },
    );
    let shell = ShellConfig {
        default_profile: Some("escaping".to_string()),
        profiles,
        ..ShellConfig::default()
    };
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());

    let result = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("open", "wc_shell_profile_escape", None),
    );
    assert_eq!(
        result.error_code.as_deref(),
        Some("persistent_shell_cwd_outside_project")
    );
    assert_ne!(result.shell_state, "running");
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn runner_closes_shell_that_moves_outside_project() {
    let (_temp, _project, projects, policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    assert_eq!(
        manager
            .handle(
                &policy,
                &shell,
                &SshConfig::default(),
                1,
                &projects,
                &request("open", "wc_shell_boundary", None),
            )
            .shell_state,
        "running"
    );

    let escaped = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("exec", "wc_shell_boundary", Some("cd ..")),
    );
    assert_eq!(escaped.error_code.as_deref(), Some("shell_reset_required"));
    assert_ne!(escaped.shell_state, "running");
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn runner_rejects_runtime_project_identity_mismatch() {
    let (_temp, _project, projects, policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());
    let mut wrong = request("open", "wc_shell_wrong", None);
    wrong.persistent_shell.as_mut().unwrap().runtime_project_id = "agent:other:demo".to_string();
    let result = manager.handle(&policy, &shell, &SshConfig::default(), 1, &projects, &wrong);
    assert_eq!(
        result.error_code.as_deref(),
        Some("persistent_shell_project_mismatch")
    );
    assert_eq!(manager.active_count(), 0);
}
