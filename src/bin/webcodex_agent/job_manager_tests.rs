use super::*;

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

#[cfg(target_os = "linux")]
fn process_running(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(") ")
        .and_then(|(_, rest)| rest.chars().next())
        .is_some_and(|state| state != 'Z')
}
