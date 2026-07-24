use super::*;
use serde_json::json;

#[cfg(target_os = "linux")]
#[test]
fn job_manager_stop_terminates_the_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let mut command = configured_shell_job_command(
        &ShellConfig::default(),
        "sleep 60 & echo $! > descendant.pid; wait",
    )
    .unwrap();
    let child = Arc::new(Mutex::new(
        command
            .current_dir(temp.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ));
    let leader_pid = child.lock().unwrap().id();
    let pid_file = temp.path().join("descendant.pid");
    for _ in 0..200 {
        if pid_file.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let descendant_pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    assert!(process_running(leader_pid));
    assert!(process_running(descendant_pid));

    let manager = JobManager::new(1);
    let stop_requested = Arc::new(AtomicBool::new(false));
    manager.jobs.lock().unwrap().insert(
        "process-group-job".into(),
        RunningJob {
            client_id: "test-agent".into(),
            child: Some(child.clone()),
            stop_requested: stop_requested.clone(),
        },
    );
    manager.stop("process-group-job").unwrap();
    assert!(stop_requested.load(Ordering::SeqCst));

    for _ in 0..200 {
        let leader_exited = child.lock().unwrap().try_wait().unwrap().is_some();
        if leader_exited && !process_running(descendant_pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.lock().unwrap().try_wait().unwrap().is_some());
    assert!(
        !process_running(descendant_pid),
        "descendant {descendant_pid} survived process-group cancellation"
    );
}

#[cfg(unix)]
#[test]
fn process_group_signal_errors_distinguish_gone_permission_and_other_failures() {
    assert_eq!(
        classify_process_group_signal_error(
            42,
            libc::SIGTERM,
            std::io::Error::from_raw_os_error(libc::ESRCH),
        ),
        Ok(false)
    );
    let permission = classify_process_group_signal_error(
        42,
        libc::SIGTERM,
        std::io::Error::from_raw_os_error(libc::EPERM),
    )
    .unwrap_err();
    assert!(permission.contains("permission"));
    let other = classify_process_group_signal_error(
        42,
        libc::SIGTERM,
        std::io::Error::from_raw_os_error(libc::EINVAL),
    )
    .unwrap_err();
    assert!(other.contains("Invalid argument"));
}

#[cfg(unix)]
#[test]
fn validation_job_progress_is_executor_owned_and_fail_fast() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let cargo = bin.join("cargo");
    std::fs::write(
        &cargo,
        "#!/bin/sh\ncase \"$1\" in\nfmt) echo 'format passed';;\ncheck) exit 7;;\ntest) touch should-not-run;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let sink = AgentSink::WebSocket {
        tx,
        client_id: "validation-agent".into(),
        agent_instance_id: "validation-instance".into(),
    };
    let steps = vec![
        ShellJobValidationStep {
            name: "format".into(),
            program: "cargo".into(),
            args: vec!["fmt".into(), "--".into(), "--check".into()],
        },
        ShellJobValidationStep {
            name: "check".into(),
            program: "cargo".into(),
            args: vec!["check".into(), "--all-targets".into()],
        },
        ShellJobValidationStep {
            name: "test".into(),
            program: "cargo".into(),
            args: vec!["test".into()],
        },
    ];
    let mut shell = ShellConfig::default();
    shell.path_prepend.push(bin);
    let manager = JobManager::new(1);
    manager.enqueue(
        sink,
        AgentPolicy::default(),
        shell,
        temp.path().join("projects.d"),
        serde_json::from_value(json!({
            "request_id": "validation-request",
            "client_id": "validation-agent",
            "kind": "start_validation_job",
            "job_id": "validation-job",
            "cwd": temp.path(),
            "command": serde_json::to_string(&steps).unwrap(),
            "timeout_secs": 10,
            "requested_by": "test",
            "created_at": 1
        }))
        .unwrap(),
    );
    let mut updates = Vec::new();
    for _ in 0..500 {
        while let Ok(envelope) = rx.try_recv() {
            if let AgentEnvelope::JobUpdate { payload } = envelope {
                let finished = payload.finished;
                updates.push(payload);
                if finished {
                    break;
                }
            }
        }
        if updates.last().is_some_and(|update| update.finished) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let final_update = updates.last().expect("validation job emitted updates");
    assert!(final_update.finished);
    assert_eq!(final_update.status, "failed");
    assert_eq!(final_update.exit_code, Some(7));
    assert_eq!(
        final_update.validation_progress,
        Some(ShellJobValidationProgress {
            completed: 1,
            current_step: None,
            failed_step: Some("check".into()),
        })
    );
    assert!(updates.iter().any(|update| {
        update.validation_progress
            == Some(ShellJobValidationProgress {
                completed: 1,
                current_step: Some("check".into()),
                failed_step: None,
            })
    }));
    assert!(!temp.path().join("should-not-run").exists());
}

#[cfg(unix)]
#[test]
fn validation_spawn_failure_is_infrastructure_without_failed_assertion() {
    let temp = tempfile::tempdir().unwrap();
    let mut shell = ShellConfig::default();
    shell.env.insert(
        "PATH".to_string(),
        temp.path().to_string_lossy().into_owned(),
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let sink = AgentSink::WebSocket {
        tx,
        client_id: "validation-agent".into(),
        agent_instance_id: "validation-instance".into(),
    };
    let manager = JobManager::new(1);
    manager.enqueue(
        sink,
        AgentPolicy::default(),
        shell,
        temp.path().join("projects.d"),
        serde_json::from_value(json!({
            "request_id": "spawn-failure-request",
            "client_id": "validation-agent",
            "kind": "start_validation_job",
            "job_id": "spawn-failure-job",
            "cwd": temp.path(),
            "command": serde_json::to_string(&[ShellJobValidationStep {
                name: "check".into(),
                program: "cargo".into(),
                args: vec!["check".into(), "--all-targets".into()],
            }]).unwrap(),
            "timeout_secs": 10,
            "requested_by": "test",
            "created_at": 1
        }))
        .unwrap(),
    );
    let update = (0..100)
        .find_map(|_| {
            let update = rx.try_recv().ok().and_then(|envelope| match envelope {
                AgentEnvelope::JobUpdate { payload } => Some(payload),
                _ => None,
            });
            if update.is_none() {
                std::thread::sleep(Duration::from_millis(10));
            }
            update
        })
        .expect("validation spawn failure update");
    assert!(update.finished);
    assert_eq!(update.status, "failed");
    assert_eq!(update.exit_code, None);
    assert_eq!(
        update.error.as_deref(),
        Some(VALIDATION_STEP_SPAWN_FAILED_CODE)
    );
    assert_eq!(
        update.validation_progress,
        Some(ShellJobValidationProgress {
            completed: 0,
            current_step: None,
            failed_step: None,
        })
    );
}

#[cfg(unix)]
#[test]
fn python_module_probe_reports_tool_unavailable_without_running_recipe() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let python = temp.path().join("python");
    let probe_output = temp.path().join("module");
    std::fs::write(
        &python,
        "#!/bin/sh\nprintf '%s' \"$4\" > \"$PROBE_OUTPUT\"\nexit 42\n",
    )
    .unwrap();
    std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut shell = ShellConfig::default();
    shell.env.insert(
        "PATH".to_string(),
        temp.path().to_string_lossy().into_owned(),
    );
    shell.env.insert(
        "PROBE_OUTPUT".to_string(),
        probe_output.to_string_lossy().into_owned(),
    );
    let step = ShellJobValidationStep {
        name: "test".into(),
        program: "python".into(),
        args: ["-B", "-m", "unittest", "discover", "-v"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    };
    assert!(!validation_module_available(
        &shell,
        None,
        temp.path(),
        &step
    ));
    assert_eq!(std::fs::read_to_string(probe_output).unwrap(), "unittest");
    assert!(!temp.path().join("recipe-ran").exists());
}

#[cfg(target_os = "linux")]
fn process_running(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(") ")
        .and_then(|(_, rest)| rest.chars().next())
        .is_some_and(|state| state != 'Z')
}
