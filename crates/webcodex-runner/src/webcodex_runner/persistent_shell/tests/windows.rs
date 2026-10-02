use super::super::config::SshResourceConfig;
use super::super::ssh::SshConnectionPool;
use super::*;
use std::collections::BTreeMap;
use webcodex_core::runner_protocol::RunnerRequest;

fn request(action: &str, shell_id: &str, command: Option<&str>) -> RunnerRequest {
    RunnerRequest {
        login: false,
        shell: None,
        request_id: format!("req-{action}"),
        client_id: "msi".to_string(),
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
            workflow_session_id: "wc_sess_x0hjuH0xLj6xHOl7".to_string(),
            runtime_project_id: "agent:msi:demo".to_string(),
            cwd: None,
            shell: None,
            command: command.map(str::to_string),
            timeout_secs: Some(5),
            purpose: None,
        }),
    }
}

fn ssh_request(
    action: &str,
    shell_id: &str,
    resource: &str,
    command: Option<&str>,
) -> RunnerRequest {
    serde_json::from_value(serde_json::json!({
        "request_id": format!("req-ssh-{action}-{shell_id}"),
        "client_id": "msi",
        "kind": "persistent_shell",
        "command": command.unwrap_or(""),
        "timeout_secs": 5,
        "requested_by": "tester",
        "created_at": 0,
        "job_context": {
            "runtime_project_id": "agent:msi:demo",
            "workflow_session_id": "wc_sess_x0hjuH0xLj6xHOl7",
            "ssh_resource": resource,
            "project_cwd": ".",
            "purpose": "other",
            "shell": "remote",
            "command_preview": "",
            "validation_steps": []
        },
        "persistent_shell": {
            "action": action,
            "shell_id": shell_id,
            "workflow_session_id": "wc_sess_x0hjuH0xLj6xHOl7",
            "runtime_project_id": "agent:msi:demo",
            "cwd": null,
            "shell": "bash",
            "command": command,
            "timeout_secs": 5,
            "purpose": null
        }
    }))
    .expect("build Windows named-SSH persistent-shell request")
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, RunnerPolicy) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let projects = temp.path().join("project-registry");
    std::fs::create_dir_all(project.join("sub")).unwrap();
    std::fs::create_dir_all(&projects).unwrap();
    let escaped = project.to_string_lossy().replace('\\', "\\\\");
    std::fs::write(
        projects.join("demo.toml"),
        format!("id = \"demo\"\npath = \"{escaped}\"\n"),
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
fn runner_windows_local_persistent_shell_preserves_state_and_existing_protocol() {
    let (_temp, _project, projects, mut policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());

    let opened = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("open", "wc_shell_windows_runner", None),
    );
    assert_eq!(opened.shell_state, "running");
    assert_eq!(opened.shell.as_deref(), Some("powershell"));

    let set = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("$env:WC_RUNNER_STATE='ready'; Set-Location -LiteralPath 'sub'; $WC_LOCAL='beta'; function WC_FN { [Console]::Out.Write('fn') }"),
        ),
    );
    assert_eq!(set.exit_code, Some(0));
    let observed = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("[Console]::Out.Write($env:WC_RUNNER_STATE + '|' + (Get-Location).Path + '|' + $WC_LOCAL + '|'); WC_FN"),
        ),
    );
    assert!(observed.stdout.starts_with("ready|"), "{}", observed.stdout);
    assert!(
        observed.stdout.contains("\\sub|beta|fn"),
        "{}",
        observed.stdout
    );

    let unicode = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("[Console]::Out.Write(\"hello`r`n中文`r`n🙂\")"),
        ),
    );
    assert_eq!(unicode.stdout, "hello\r\n中文\r\n🙂");

    let failed = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("Write-Error 'expected-runner-failure'"),
        ),
    );
    assert_eq!(failed.exit_code, Some(1));
    assert_eq!(failed.shell_state, "running");
    assert!(failed.stderr.contains("expected-runner-failure"));

    let next = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("[Console]::Out.Write('still-running')"),
        ),
    );
    assert_eq!(next.stdout, "still-running");
    assert!(
        next.stderr.is_empty(),
        "previous stderr leaked: {}",
        next.stderr
    );

    policy.max_output_bytes = 1024;
    let bounded = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request(
            "exec",
            "wc_shell_windows_runner",
            Some("[Console]::Out.Write('x' * 5000)"),
        ),
    );
    assert!(bounded.stdout_truncated);
    assert!(bounded.stdout.len() <= policy.max_output_bytes);

    let closed = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &request("close", "wc_shell_windows_runner", None),
    );
    assert_eq!(closed.shell_state, "closed");
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn windows_launch_uses_configured_powershell_profile_and_rejects_payload_modes() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_path_buf();
    let mut profiles = BTreeMap::new();
    profiles.insert(
        "modern".to_string(),
        ShellProfileConfig {
            program: Some("pwsh.exe".to_string()),
            args: Some(vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
            ]),
            init_script: Some("$env:WC_PROFILE='ready'".to_string()),
            ..ShellProfileConfig::default()
        },
    );
    let shell = ShellConfig {
        profiles,
        ..ShellConfig::default()
    };
    let project = RunnerProjectShellContext {
        allow_patch: true,
        id: "demo".to_string(),
        path: cwd.to_string_lossy().to_string(),
        shell_profile: Some("modern".to_string()),
    };
    let open = request("open", "wc_shell_profile_mapping", None);
    let operation = open.persistent_shell.as_ref().unwrap();
    let launch = build_launch_at_cwd(
        &shell,
        open.client_id.as_str(),
        operation,
        &project,
        cwd.clone(),
        4096,
    )
    .unwrap();
    assert_eq!(launch.program, "pwsh.exe");
    assert_eq!(launch.dialect, "powershell");
    assert_eq!(launch.args, vec!["-NoProfile", "-NonInteractive"]);
    assert_eq!(
        launch.initialization.as_deref(),
        Some("$env:WC_PROFILE='ready'")
    );
    assert_eq!(
        dialect_for_program("pwsh.exe"),
        Some(ShellDialect::PowerShell)
    );

    let mut explicit = request("open", "wc_shell_explicit_sh", None);
    explicit.persistent_shell.as_mut().unwrap().shell = Some("bash".to_string());
    let error = build_launch_at_cwd(
        &ShellConfig::default(),
        explicit.client_id.as_str(),
        explicit.persistent_shell.as_ref().unwrap(),
        &RunnerProjectShellContext {
            allow_patch: true,
            id: "demo".to_string(),
            path: cwd.to_string_lossy().to_string(),
            shell_profile: None,
        },
        cwd.clone(),
        4096,
    )
    .unwrap_err();
    assert_eq!(error.0, "persistent_shell_dialect_unsupported");

    let mut invalid_shell = ShellConfig::default();
    invalid_shell.args = vec!["-NoProfile".to_string()];
    let default_project = RunnerProjectShellContext {
        allow_patch: true,
        id: "demo".to_string(),
        path: cwd.to_string_lossy().to_string(),
        shell_profile: None,
    };
    let invalid = request("open", "wc_shell_bad_args", None);
    let error = build_launch_at_cwd(
        &invalid_shell,
        invalid.client_id.as_str(),
        invalid.persistent_shell.as_ref().unwrap(),
        &default_project,
        cwd,
        4096,
    )
    .unwrap_err();
    assert_eq!(error.0, "persistent_shell_config_invalid");
}

#[test]
fn windows_named_ssh_resource_routes_remote_without_changing_local_powershell() {
    let (_temp, _project, projects, policy) = fixture();
    let shell = ShellConfig::default();
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());

    let mut local_bash = request("open", "wc_shell_local_bash", None);
    local_bash.persistent_shell.as_mut().unwrap().shell = Some("bash".to_string());
    let local = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &local_bash,
    );
    assert_eq!(
        local.error_code.as_deref(),
        Some("persistent_shell_dialect_unsupported"),
        "Windows local shell must remain PowerShell: {local:?}"
    );

    let remote = manager.handle(
        &policy,
        &shell,
        &SshConfig::default(),
        1,
        &projects,
        &ssh_request("open", "wc_shell_remote_missing", "missing", None),
    );
    assert_eq!(
        remote.error_code.as_deref(),
        Some("ssh_persistent_shell_spawn_failed"),
        "named resource must route to SSH before local PowerShell validation: {remote:?}"
    );
    assert!(
        remote
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ssh_resource_not_found")),
        "missing named SSH resource must fail from SSH authority: {remote:?}"
    );
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn windows_named_ssh_persistent_shell_real_transport_opt_in() {
    let Ok(host) = std::env::var("WEBCODEX_TEST_WINDOWS_SSH_HOST") else {
        eprintln!("skipping Windows SSH persistent-shell integration test; WEBCODEX_TEST_WINDOWS_SSH_HOST is unset");
        return;
    };
    let (_temp, _project, projects, mut policy) = fixture();
    let shell = ShellConfig::default();
    let mut resources = BTreeMap::new();
    resources.insert(
        "dogfood".to_string(),
        SshResourceConfig {
            host,
            default_cwd: Some("/tmp".to_string()),
        },
    );
    let config = SshConfig { resources };
    let manager = PersistentShellManager::new(&shell, SshConnectionPool::default());

    let opened = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request("open", "wc_shell_win_ssh_state", "dogfood", None),
    );
    assert_eq!(opened.shell_state, "running", "{opened:?}");
    assert_eq!(opened.shell.as_deref(), Some("bash"), "{opened:?}");
    assert_eq!(opened.cwd.as_deref(), Some("/tmp"), "{opened:?}");

    let setup = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_state",
            "dogfood",
            Some("export WC_WIN_SSH=ready; cd /tmp; wc_win_fn() { printf fn; }"),
        ),
    );
    assert_eq!(setup.exit_code, Some(0), "{setup:?}");
    let observed = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_state",
            "dogfood",
            Some("printf '%s|%s|' \"$WC_WIN_SSH\" \"$PWD\"; wc_win_fn; printf '|中文🙂'; printf '错误🙂' >&2"),
        ),
    );
    assert_eq!(observed.exit_code, Some(0), "{observed:?}");
    assert!(
        observed.stdout.contains("ready|/tmp|fn|中文🙂"),
        "{observed:?}"
    );
    assert!(observed.stderr.contains("错误🙂"), "{observed:?}");
    assert!(!observed.stdout.contains("WCPS"), "{observed:?}");
    assert!(!observed.stderr.contains("WCPS"), "{observed:?}");

    let failed = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request("exec", "wc_shell_win_ssh_state", "dogfood", Some("false")),
    );
    assert_eq!(failed.exit_code, Some(1), "{failed:?}");
    assert_eq!(failed.shell_state, "running", "{failed:?}");
    let next = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_state",
            "dogfood",
            Some("printf clean"),
        ),
    );
    assert_eq!(next.stdout, "clean", "{next:?}");
    assert!(next.stderr.is_empty(), "previous stderr leaked: {next:?}");

    policy.max_output_bytes = 1024;
    let bounded = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_state",
            "dogfood",
            Some("i=0; while [ \"$i\" -lt 5000 ]; do printf x; i=$((i+1)); done"),
        ),
    );
    assert!(bounded.stdout_truncated, "{bounded:?}");
    assert!(
        bounded.stdout.len() <= policy.max_output_bytes,
        "{bounded:?}"
    );

    let mut removed = config.clone();
    removed.resources.remove("dogfood");
    let removed_status = manager.handle(
        &policy,
        &shell,
        &removed,
        7,
        &projects,
        &ssh_request("status", "wc_shell_win_ssh_state", "dogfood", None),
    );
    assert_eq!(
        removed_status.error_code.as_deref(),
        Some("shell_reset_required"),
        "{removed_status:?}"
    );
    assert_eq!(manager.active_count(), 0, "{removed_status:?}");

    let generation_open = manager.handle(
        &policy,
        &shell,
        &config,
        7,
        &projects,
        &ssh_request("open", "wc_shell_win_ssh_generation", "dogfood", None),
    );
    assert_eq!(
        generation_open.shell_state, "running",
        "{generation_open:?}"
    );
    let stale = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("status", "wc_shell_win_ssh_generation", "dogfood", None),
    );
    assert_eq!(
        stale.error_code.as_deref(),
        Some("shell_reset_required"),
        "{stale:?}"
    );
    assert_eq!(manager.active_count(), 0, "{stale:?}");

    let reopened = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("open", "wc_shell_win_ssh_timeout", "dogfood", None),
    );
    assert_eq!(reopened.shell_state, "running", "{reopened:?}");
    let mut timeout_request = ssh_request(
        "exec",
        "wc_shell_win_ssh_timeout",
        "dogfood",
        Some("sleep 3; printf late"),
    );
    timeout_request
        .persistent_shell
        .as_mut()
        .unwrap()
        .timeout_secs = Some(1);
    let timed_out = manager.handle(&policy, &shell, &config, 8, &projects, &timeout_request);
    assert_eq!(timed_out.execution_state, "timed_out", "{timed_out:?}");
    assert_ne!(timed_out.shell_state, "running", "{timed_out:?}");
    assert_eq!(
        timed_out.error_code.as_deref(),
        Some("shell_reset_required"),
        "{timed_out:?}"
    );
    let after_timeout = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_timeout",
            "dogfood",
            Some("printf forbidden"),
        ),
    );
    assert!(!after_timeout.command_started, "{after_timeout:?}");

    let reopened = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("open", "wc_shell_win_ssh_exit", "dogfood", None),
    );
    assert_eq!(reopened.shell_state, "running", "{reopened:?}");
    let exited = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("exec", "wc_shell_win_ssh_exit", "dogfood", Some("exit 7")),
    );
    assert_eq!(exited.shell_state, "exited", "{exited:?}");
    assert_eq!(exited.exit_code, Some(7), "{exited:?}");
    let after_exit = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request(
            "exec",
            "wc_shell_win_ssh_exit",
            "dogfood",
            Some("printf forbidden"),
        ),
    );
    assert!(!after_exit.command_started, "{after_exit:?}");

    let reopened = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("open", "wc_shell_win_ssh_close", "dogfood", None),
    );
    assert_eq!(reopened.shell_state, "running", "{reopened:?}");
    let closed = manager.handle(
        &policy,
        &shell,
        &config,
        8,
        &projects,
        &ssh_request("close", "wc_shell_win_ssh_close", "dogfood", None),
    );
    assert_eq!(closed.shell_state, "closed", "{closed:?}");
    assert_eq!(manager.active_count(), 0, "{closed:?}");
}
