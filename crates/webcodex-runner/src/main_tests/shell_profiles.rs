use super::*;

#[test]
fn prepared_profile_env_is_available_to_run_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                env: profile_env(&[("WEBCODEX_TEST_PROFILE", "from_env")]),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("from_env"));
}

#[test]
fn prepared_profile_init_script_export_is_available_to_run_shell() {
    let tmp = tempfile::tempdir().unwrap();
    // The profile inherits the platform default shell (`sh -c` on Unix,
    // PowerShell on Windows) and exports a variable its init snippet; the
    // captured environment snapshot must reach later commands.
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(profile_init_export(
                    "WEBCODEX_TEST_PROFILE",
                    "from_snapshot",
                )),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("from_snapshot"));
}

#[test]
fn prepared_profile_failure_reports_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                // `exit 4` terminates the prepare shell with 4 on both the
                // POSIX and PowerShell dialects.
                init_script: Some("exit 4".to_string()),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        "true",
    );
    let err = result.error.expect("prepare should fail");
    assert!(err.contains("failed to prepare shell profile"), "{err}");
    assert!(err.contains("exit code 4"), "{err}");
}

#[test]
fn prepared_profile_init_script_is_project_relative() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    // Windows virtual environments activate through `.venv/Scripts/
    // Activate.ps1`; Unix through `.venv/bin/activate`.
    #[cfg(windows)]
    let activate_rel = ".venv/Scripts/Activate.ps1";
    #[cfg(not(windows))]
    let activate_rel = ".venv/bin/activate";
    let activate = project_dir.join(activate_rel);
    std::fs::create_dir_all(activate.parent().unwrap()).unwrap();
    std::fs::write(
        &activate,
        format!(
            "{}\n",
            profile_init_export("WEBCODEX_PROJECT_VENV", "project_local")
        ),
    )
    .unwrap();
    write_agent_project(&projects_dir, "demo", &project_dir, Some("py-venv"));
    let shell = shell_with_profiles(
        None,
        vec![(
            "py-venv",
            ShellProfileConfig {
                init_script: Some(format!(". {activate_rel}")),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        &projects_dir,
        &PreparedShellProfileCache::default(),
        &project_dir,
        &shell_env_var("WEBCODEX_PROJECT_VENV"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("project_local"));
}

#[test]
fn project_shell_profile_overrides_default_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir_all(&project_dir).unwrap();
    write_agent_project(&projects_dir, "demo", &project_dir, Some("project"));
    let shell = shell_with_profiles(
        Some("default"),
        vec![
            (
                "default",
                ShellProfileConfig {
                    env: profile_env(&[("WEBCODEX_TEST_PROFILE", "default")]),
                    ..ShellProfileConfig::default()
                },
            ),
            (
                "project",
                ShellProfileConfig {
                    env: profile_env(&[("WEBCODEX_TEST_PROFILE", "project")]),
                    ..ShellProfileConfig::default()
                },
            ),
        ],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        &projects_dir,
        &PreparedShellProfileCache::default(),
        &project_dir,
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("project"));
}

fn wait_for_job_stdout(rx: &mut tokio::sync::mpsc::Receiver<AgentEnvelope>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stdout = String::new();
    while Instant::now() < deadline {
        match rx.try_recv() {
            Ok(AgentEnvelope::JobUpdate { payload }) => {
                if let Some(snapshot) = payload.log_snapshot {
                    stdout = snapshot.stdout.tail;
                } else if let Some(chunk) = payload.stdout_chunk {
                    stdout.push_str(&chunk);
                }
                if payload.finished {
                    return stdout;
                }
            }
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
    panic!("timed out waiting for job completion; stdout so far: {stdout:?}");
}

#[test]
fn prepared_profile_run_shell_and_run_job_see_same_env() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir_all(&project_dir).unwrap();
    write_agent_project(&projects_dir, "demo", &project_dir, Some("test"));
    let shell = shell_with_profiles(
        None,
        vec![(
            "test",
            ShellProfileConfig {
                env: profile_env(&[("WEBCODEX_TEST_PROFILE", "same")]),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let jobs = JobManager::new(1);
    let shell_result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        &projects_dir,
        &jobs.prepared_profiles,
        &project_dir,
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(shell_result.stdout.as_deref(), Some("same"));

    let (sink, mut rx) = ws_sink("ws-client");
    let lsp = webcodex_runner::LspSupervisor::default();
    let mut cfg = test_config(projects_dir.clone());
    cfg.shell = shell.clone();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &projects_dir,
        &lsp,
        shell_job_request(&project_dir, &shell_env_var("WEBCODEX_TEST_PROFILE")),
    )
    .unwrap();
    assert_eq!(wait_for_job_stdout(&mut rx), "same");
}

#[test]
fn prepared_profile_init_script_runs_once_per_project_profile_generation() {
    let tmp = tempfile::tempdir().unwrap();
    let counter = tmp.path().join("prepare-count");
    #[cfg(windows)]
    let init_script = format!(
        "$n = 0\ntry {{ $n = [int](Get-Content -Raw {}) }} catch {{ }}\n\
         $n = $n + 1\nSet-Content -Path {} -Value $n\n{}",
        shell_tree_quote(&counter.to_string_lossy()),
        shell_tree_quote(&counter.to_string_lossy()),
        profile_init_export("WEBCODEX_TEST_PROFILE", "counted"),
    );
    #[cfg(not(windows))]
    let init_script = format!(
            "count=$(cat {:?} 2>/dev/null || echo 0)\ncount=$((count + 1))\nprintf '%s\\n' \"$count\" > {:?}\n{}",
            counter.to_string_lossy(),
            counter.to_string_lossy(),
            profile_init_export("WEBCODEX_TEST_PROFILE", "counted"),
        );
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let cache = PreparedShellProfileCache::default();
    for _ in 0..2 {
        let result = run_profile_shell(
            &unrestricted_test_policy(),
            &shell,
            tmp.path(),
            &cache,
            tmp.path(),
            &shell_env_var("WEBCODEX_TEST_PROFILE"),
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("counted"));
    }
    assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "1");
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell_with_profiles(
        2,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
        None,
        10,
        None,
    );
    assert_eq!(result.stdout.as_deref(), Some("counted"));
    assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "2");

    // A late request that still holds generation 1 may prepare its own
    // snapshot, but it must not evict the already-cached active generation.
    let stale = run_shell_with_profiles(
        1,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
        None,
        10,
        None,
    );
    assert_eq!(stale.stdout.as_deref(), Some("counted"));
    assert_eq!(std::fs::read_to_string(&counter).unwrap().trim(), "3");

    let current = run_shell_with_profiles(
        2,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
        None,
        10,
        None,
    );
    assert_eq!(current.stdout.as_deref(), Some("counted"));
    assert_eq!(std::fs::read_to_string(counter).unwrap().trim(), "3");
    assert_eq!(cache.len(), 1);
}

#[test]
fn prepared_profile_init_script_stdout_noise_does_not_break_env_capture() {
    let tmp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let init_script =
        "Write-Output 'noise before env'\n$env:WEBCODEX_TEST_PROFILE = 'ok'".to_string();
    #[cfg(not(windows))]
    let init_script = "echo noise before env\nexport WEBCODEX_TEST_PROFILE=ok".to_string();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("ok"));
}

#[cfg(unix)]
#[test]
fn prepared_profile_prepare_reaps_background_pipe_holder() {
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("prepare-background-pipe-holder.pid");
    let init_script = format!(
            "sleep 60 & background_pid=$!; printf '%s' \"$background_pid\" > {}; export WEBCODEX_TEST_PROFILE=ready",
            shell_quote_path(&pid_file)
        );
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                program: Some("/bin/sh".to_string()),
                args: Some(vec!["-c".to_string()]),
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let policy = unrestricted_test_policy();
    let cache = PreparedShellProfileCache::default();
    let projects_dir = tmp.path().to_path_buf();
    let cwd = tmp.path().to_string_lossy().to_string();
    let worker_shell = shell.clone();
    let worker_policy = policy.clone();
    let worker_cache = cache.clone();
    let worker_projects_dir = projects_dir.clone();
    let worker_cwd = cwd.clone();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = run_shell_with_profiles(
            1,
            &worker_policy,
            &worker_shell,
            &worker_projects_dir,
            &worker_cache,
            Some(&worker_cwd),
            &shell_env_var("WEBCODEX_TEST_PROFILE"),
            None,
            10,
            None,
        );
        let _ = result_tx.send(result);
    });

    let received = result_rx.recv_timeout(Duration::from_secs(5));
    if received.is_err() {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|contents| contents.trim().parse::<u32>().ok())
        {
            // SAFETY: the PID was written by this test's background
            // command. This failure-path cleanup targets only that PID.
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }
    worker.join().expect("prepared profile worker panicked");
    let result = received.unwrap_or_else(|error| {
        panic!("prepared profile prepare did not return within its bound: {error}")
    });

    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("ready"), "{result:?}");
    assert_eq!(cache.len(), 1, "prepared profile cache was not established");
    assert_descendant_reaped(&pid_file);
}

#[test]
fn prepared_profile_errors_do_not_leak_init_script_body() {
    let tmp = tempfile::tempdir().unwrap();
    let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
    #[cfg(windows)]
    let failing_init = format!("$env:SECRET = '{secret}'\nexit 1");
    #[cfg(not(windows))]
    let failing_init = format!("export SECRET={secret}\nfalse");
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(failing_init),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        "true",
    );
    let err = result.error.expect("prepare should fail");
    assert!(err.contains("failed to prepare shell profile"), "{err}");
    assert!(!err.contains(secret), "{err}");
}

#[test]
fn prepared_profile_filters_webcodex_token_env() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(Some("test"), vec![("test", ShellProfileConfig::default())]);
    // Windows environment names are case-insensitive, so mixed-case spellings
    // must be filtered too; Unix is case-sensitive and only the exact name
    // can be inherited or configured.
    #[cfg(windows)]
    let spellings = ["WEBCODEX_TOKEN", "WebCodex_Token", "authorization"];
    #[cfg(not(windows))]
    let spellings = ["WEBCODEX_TOKEN"];
    for spelling in spellings {
        let saved = std::env::var_os(spelling);
        std::env::set_var(spelling, "secret-token");
        let result = run_profile_shell(
            &unrestricted_test_policy(),
            &shell,
            tmp.path(),
            &PreparedShellProfileCache::default(),
            tmp.path(),
            &shell_if_else_env_present(spelling),
        );
        match saved {
            Some(value) => std::env::set_var(spelling, value),
            None => std::env::remove_var(spelling),
        }
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("absent"), "{result:?}");
    }
}

#[test]
fn prepared_profile_missing_marker_is_reported_without_script_body() {
    let tmp = tempfile::tempdir().unwrap();
    let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
    // Windows: `exit 0` ends the prepare shell successfully before the marker
    // can be written. Unix: redirecting stdout away makes the marker
    // unreachable. Both report "env marker not found" without the body.
    #[cfg(windows)]
    let init_script = format!("$env:SECRET = '{secret}'\nexit 0");
    #[cfg(not(windows))]
    let init_script = format!("export SECRET={secret}\nexec >/dev/null");
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        "true",
    );
    let err = result.error.expect("prepare should fail");
    assert!(err.contains("env marker not found"), "{err}");
    assert!(!err.contains(secret), "{err}");
}

#[cfg(unix)]
#[test]
fn prepared_profile_env_payload_parse_failure_is_reported() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let fake_env = bin.join("env");
    std::fs::write(&fake_env, "#!/bin/sh\nprintf 'bad\\000'\n").unwrap();
    let mut perms = std::fs::metadata(&fake_env).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake_env, perms).unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                program: Some("/bin/sh".to_string()),
                args: Some(vec!["-c".to_string()]),
                env: profile_env(&[("PATH", bin.to_string_lossy().as_ref())]),
                init_script: Some("export WEBCODEX_TEST_PROFILE=ok".to_string()),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        "true",
    );
    let err = result.error.expect("prepare should fail");
    assert!(err.contains("entry missing '='"), "{err}");
}

#[test]
fn prepared_profile_program_spawn_failure_mentions_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                program: Some("/definitely/missing/webcodex-shell".to_string()),
                args: Some(vec!["-c".to_string()]),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        "true",
    );
    let err = result.error.expect("spawn should fail");
    assert!(
        err.contains("failed to spawn shell profile 'test'"),
        "{err}"
    );
}

#[test]
fn project_shell_profile_missing_profile_returns_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir_all(&project_dir).unwrap();
    write_agent_project(&projects_dir, "demo", &project_dir, Some("missing"));
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        &projects_dir,
        &PreparedShellProfileCache::default(),
        &project_dir,
        "true",
    );
    let err = result.error.expect("profile should be missing");
    assert!(
        err.contains("project 'demo' shell_profile 'missing'"),
        "{err}"
    );
}
