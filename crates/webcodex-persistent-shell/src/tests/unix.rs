use super::*;
use crate::unix::launch::create_control_pipe;
use std::sync::Barrier;

/// Whether `pid` is effectively terminated: it no longer exists (`ESRCH`),
/// or it has become a zombie (`Z` / dead `X`) in `/proc/<pid>/stat` and will
/// never execute again.
///
/// This cannot rely on `kill(pid, 0)` alone: in containers whose PID 1 does
/// not reap orphaned children, a terminated background process lingers in
/// the process table as a zombie and `kill(pid, 0)` keeps returning 0. A
/// process that is still running or sleeping (states such as `R`, `S`, `D`)
/// is reported as alive, so a shutdown that failed to terminate its
/// descendants still fails the test.
#[cfg(unix)]
fn process_is_effectively_terminated(pid: i32) -> bool {
    // SAFETY: signal 0 performs an existence check without delivering a
    // signal. The pid came from the test itself.
    if unsafe { libc::kill(pid, 0) } == -1 {
        // ESRCH (or EPERM against another user's process, which cannot
        // happen here) means the process is gone.
        return true;
    }
    // The process still exists in the table. Read its state and treat a
    // zombie/dead state as terminated.
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Reaped between the kill probe and the read.
            return true;
        }
        Err(_) => {
            // Cannot determine; fall back to the kill probe (still exists).
            return false;
        }
    };
    // `/proc/<pid>/stat` is `pid (comm) state ...`; `comm` may contain
    // spaces and parentheses, so take the first token after the last `)`.
    let state = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .unwrap_or("");
    state == "Z" || state == "X"
}

#[cfg(unix)]
fn launch(root: &Path, shell_id: &str, session_id: &str) -> ShellLaunch {
    ShellLaunch {
        identity: ShellIdentity {
            shell_id: shell_id.to_string(),
            workflow_session_id: session_id.to_string(),
            runtime_project_id: "agent:oe:test".to_string(),
            executor: "local".to_string(),
            client_id: None,
        },
        dialect: "bash".to_string(),
        profile: None,
        program: "bash".to_string(),
        args: vec!["--noprofile".to_string(), "--norc".to_string()],
        initial_cwd: root.to_path_buf(),
        env: std::env::vars().collect(),
        initialization: None,
        max_output_bytes: 4096,
    }
}

#[cfg(unix)]
fn exec(
    manager: &PersistentShellManager,
    shell_id: &str,
    session: &str,
    command: &str,
) -> ShellExecResult {
    manager
        .exec(
            shell_id,
            session,
            "agent:oe:test",
            command,
            Duration::from_secs(3),
        )
        .unwrap()
}

#[test]
fn control_pipe_descriptors_are_close_on_exec() {
    let (reader, writer) = create_control_pipe().unwrap();
    for fd in [reader.as_raw_fd(), writer.as_raw_fd()] {
        // SAFETY: each fd remains owned by its `File` during this query.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_ne!(flags, -1, "failed to inspect descriptor flags");
        assert_ne!(flags & libc::FD_CLOEXEC, 0, "FD_CLOEXEC was not set");
    }
}

#[test]
fn external_transport_freezes_login_cwd_from_first_control_frame() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let spec = launch(
        temp.path(),
        "wc_shell_external_login",
        "wc_sess_external_login",
    );
    let process = spawn_shell_process(&spec).unwrap();
    let opened = manager
        .open_with_transport(
            spec.identity,
            spec.dialect,
            spec.profile,
            PathBuf::new(),
            spec.initialization,
            Box::new(process),
        )
        .unwrap();
    let login_cwd = temp.path().canonicalize().unwrap();
    assert_eq!(opened.cwd, login_cwd);
    assert_eq!(opened.initial_cwd, login_cwd);

    let changed = exec(
        &manager,
        "wc_shell_external_login",
        "wc_sess_external_login",
        "cd /tmp",
    );
    let physical_tmp = Path::new("/tmp").canonicalize().unwrap();
    assert_eq!(changed.cwd, physical_tmp);

    let status = manager
        .status(
            "wc_shell_external_login",
            "wc_sess_external_login",
            "agent:oe:test",
        )
        .unwrap();
    assert_eq!(status.cwd, physical_tmp);
    assert_eq!(status.initial_cwd, login_cwd);

    let closed = manager
        .close(
            "wc_shell_external_login",
            "wc_sess_external_login",
            "agent:oe:test",
            "explicit_close",
        )
        .unwrap();
    assert_eq!(closed.summary.initial_cwd, login_cwd);
}

#[cfg(unix)]
#[test]
fn external_transport_replaces_symlink_seed_with_physical_cwd() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let physical = temp.path().join("physical");
    let logical = temp.path().join("logical");
    std::fs::create_dir(&physical).unwrap();
    symlink(&physical, &logical).unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let spec = launch(
        &logical,
        "wc_shell_external_symlink",
        "wc_sess_external_symlink",
    );
    let process = spawn_shell_process(&spec).unwrap();
    let opened = manager
        .open_with_transport(
            spec.identity,
            spec.dialect,
            spec.profile,
            logical,
            spec.initialization,
            Box::new(process),
        )
        .unwrap();
    let physical = physical.canonicalize().unwrap();
    assert_eq!(opened.cwd, physical);
    assert_eq!(opened.initial_cwd, physical);

    exec(
        &manager,
        "wc_shell_external_symlink",
        "wc_sess_external_symlink",
        "cd /tmp",
    );
    let status = manager
        .status(
            "wc_shell_external_symlink",
            "wc_sess_external_symlink",
            "agent:oe:test",
        )
        .unwrap();
    assert_eq!(status.cwd, Path::new("/tmp").canonicalize().unwrap());
    assert_eq!(status.initial_cwd, physical);
}

#[test]
fn local_initial_cwd_stays_at_launch_directory_when_initialization_changes_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let mut spec = launch(temp.path(), "wc_shell_local_init", "wc_sess_local_init");
    spec.initialization = Some("cd /tmp".to_string());
    let opened = manager.open(spec).unwrap();

    assert_eq!(opened.initial_cwd, temp.path());
    assert_eq!(opened.cwd, Path::new("/tmp").canonicalize().unwrap());
}

#[test]
fn preserves_cwd_environment_variables_functions_umask_and_shell_variables() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("nested")).unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_state", "wc_sess_state"))
        .unwrap();

    assert_eq!(
        exec(&manager, "wc_shell_state", "wc_sess_state", "cd nested").exit_code,
        Some(0)
    );
    let spoofed = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "PWD=/; pwd() { printf /; }",
    );
    assert_eq!(
        spoofed.cwd,
        temp.path().join("nested").canonicalize().unwrap()
    );
    exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "cd .; unset -f pwd",
    );
    exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "export WC_TEST_VALUE=ready; WC_LOCAL=value; wc_fn() { printf 'fn:%s' \"$WC_LOCAL\"; }; umask 027",
    );
    let observed = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "printf '%s:%s:' \"$PWD\" \"$WC_TEST_VALUE\"; wc_fn; printf ':%s' \"$(umask)\"",
    );
    assert!(observed.stdout.contains("nested:ready:fn:value:0027"));
    exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "unset WC_TEST_VALUE",
    );
    let unset = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "printf '%s' \"${WC_TEST_VALUE-unset}\"",
    );
    assert_eq!(unset.stdout, "unset");
    let hardened_control = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "WC_SAVED_PATH=$PATH; enable -n printf; PATH=/definitely-missing",
    );
    assert_eq!(hardened_control.exit_code, Some(0));
    assert_eq!(hardened_control.shell_state, ShellState::Running);
    exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "PATH=$WC_SAVED_PATH; enable printf; unset WC_SAVED_PATH",
    );
    exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "command() { /usr/bin/printf command-fn; }; printf() { /usr/bin/printf printf-fn; }; pwd() { /usr/bin/printf pwd-fn; }",
    );
    let shadowed_builtins = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "command; printf; pwd",
    );
    assert_eq!(shadowed_builtins.stdout, "command-fnprintf-fnpwd-fn");
    assert_eq!(shadowed_builtins.shell_state, ShellState::Running);
}

#[test]
fn sessions_are_isolated_and_one_shot_shell_does_not_inherit_state() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_a", "wc_sess_a"))
        .unwrap();
    manager
        .open(launch(temp.path(), "wc_shell_b", "wc_sess_b"))
        .unwrap();
    exec(
        &manager,
        "wc_shell_a",
        "wc_sess_a",
        "export WC_ISOLATED=only_a",
    );
    let other = exec(
        &manager,
        "wc_shell_b",
        "wc_sess_b",
        "printf '%s' \"${WC_ISOLATED-unset}\"",
    );
    assert_eq!(other.stdout, "unset");
    let one_shot = Command::new("sh")
        .arg("-c")
        .arg("printf '%s' \"${WC_ISOLATED-unset}\"")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&one_shot.stdout), "unset");
    assert_eq!(
        manager.close_project("agent:oe:test", "project_disabled"),
        2
    );
    assert_eq!(manager.active_count(), 0);
}

#[test]
fn only_one_active_shell_per_session_and_old_id_is_stale_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_old", "wc_sess_one"))
        .unwrap();
    let mismatch = manager
        .status("wc_shell_old", "wc_sess_other", "agent:oe:other-project")
        .unwrap_err();
    assert_eq!(mismatch.code, "persistent_shell_not_found");
    let duplicate = manager
        .open(launch(temp.path(), "wc_shell_duplicate", "wc_sess_one"))
        .unwrap_err();
    assert_eq!(duplicate.code, "persistent_shell_already_open");
    manager
        .close(
            "wc_shell_old",
            "wc_sess_one",
            "agent:oe:test",
            "explicit_close",
        )
        .unwrap();
    manager
        .open(launch(temp.path(), "wc_shell_new", "wc_sess_one"))
        .unwrap();
    let stale = manager
        .exec(
            "wc_shell_old",
            "wc_sess_one",
            "agent:oe:test",
            "true",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(stale.code, "persistent_shell_stale");
}

#[test]
fn global_shell_limit_is_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits {
        max_shells: 1,
        ..ShellLimits::default()
    });
    manager
        .open(launch(temp.path(), "wc_shell_limit_a", "wc_sess_limit_a"))
        .unwrap();
    let limited = manager
        .open(launch(temp.path(), "wc_shell_limit_b", "wc_sess_limit_b"))
        .unwrap_err();
    assert_eq!(limited.code, "persistent_shell_limit_reached");
    assert_eq!(manager.active_count(), 1);
}

#[test]
#[ignore = "manual real-process timing: depends on shell teardown and OS scheduling"]
fn close_is_idempotent_and_exit_is_observable() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_close", "wc_sess_close"))
        .unwrap();
    let first = manager
        .close(
            "wc_shell_close",
            "wc_sess_close",
            "agent:oe:test",
            "explicit_close",
        )
        .unwrap();
    assert!(!first.already_closed);
    let second = manager
        .close(
            "wc_shell_close",
            "wc_sess_close",
            "agent:oe:test",
            "explicit_close",
        )
        .unwrap();
    assert!(second.already_closed);

    manager
        .open(launch(temp.path(), "wc_shell_exit", "wc_sess_exit"))
        .unwrap();
    let exited = exec(&manager, "wc_shell_exit", "wc_sess_exit", "exit 7");
    assert_eq!(exited.shell_state, ShellState::Exited);
    assert!(exited.command_completed);
    assert_eq!(exited.exit_code, Some(7));

    manager
        .open(launch(
            temp.path(),
            "wc_shell_exit_background",
            "wc_sess_exit_background",
        ))
        .unwrap();
    let background_exit = exec(
        &manager,
        "wc_shell_exit_background",
        "wc_sess_exit_background",
        "sleep 30 & echo $! > background.pid; exit 0",
    );
    assert_eq!(background_exit.shell_state, ShellState::Exited);
    let background_pid: i32 = std::fs::read_to_string(temp.path().join("background.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if process_is_effectively_terminated(background_pid) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    // The background child must be terminated: gone from the process table
    // (ESRCH) or reduced to a zombie that will never run again. The
    // container's PID 1 does not reap orphans, so a lingering zombie is
    // still a successful shutdown.
    assert!(
        process_is_effectively_terminated(background_pid),
        "background child {background_pid} must be terminated"
    );
}

#[test]
fn initialization_output_is_not_attributed_to_the_first_command() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let mut spec = launch(temp.path(), "wc_shell_init", "wc_sess_init");
    spec.initialization = Some("printf initialization-output".to_string());
    manager.open(spec).unwrap();

    let first = exec(
        &manager,
        "wc_shell_init",
        "wc_sess_init",
        "printf user-output",
    );
    assert_eq!(first.stdout, "user-output");
}

#[test]
#[ignore = "manual real-process timing: depends on concurrent shell scheduling"]
fn concurrent_exec_returns_busy_without_mixing_output() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_busy", "wc_sess_busy"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let worker_manager = manager.clone();
    let worker_barrier = Arc::clone(&barrier);
    let worker = thread::spawn(move || {
        worker_barrier.wait();
        worker_manager
            .exec(
                "wc_shell_busy",
                "wc_sess_busy",
                "agent:oe:test",
                "sleep 0.2; printf first",
                Duration::from_secs(2),
            )
            .unwrap()
    });
    barrier.wait();
    while !manager
        .status("wc_shell_busy", "wc_sess_busy", "agent:oe:test")
        .unwrap()
        .busy
    {
        thread::yield_now();
    }
    let busy = manager
        .exec(
            "wc_shell_busy",
            "wc_sess_busy",
            "agent:oe:test",
            "printf second",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(busy.code, "shell_busy");
    assert_eq!(worker.join().unwrap().stdout, "first");
}

#[test]
#[ignore = "manual real-process timing: depends on close-vs-exec scheduling"]
fn close_during_exec_never_resurrects_the_shell() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(
            temp.path(),
            "wc_shell_close_busy",
            "wc_sess_close_busy",
        ))
        .unwrap();
    let worker_manager = manager.clone();
    let worker = thread::spawn(move || {
        worker_manager.exec(
            "wc_shell_close_busy",
            "wc_sess_close_busy",
            "agent:oe:test",
            "sleep 5",
            Duration::from_secs(10),
        )
    });
    while !manager
        .status("wc_shell_close_busy", "wc_sess_close_busy", "agent:oe:test")
        .unwrap()
        .busy
    {
        thread::yield_now();
    }

    manager
        .close(
            "wc_shell_close_busy",
            "wc_sess_close_busy",
            "agent:oe:test",
            "workflow_session_closed",
        )
        .unwrap();
    if let Ok(result) = worker.join().unwrap() {
        assert_ne!(result.shell_state, ShellState::Running);
    }
    let closed = manager
        .status("wc_shell_close_busy", "wc_sess_close_busy", "agent:oe:test")
        .unwrap();
    assert_eq!(closed.state, ShellState::Closed);
    assert_eq!(manager.active_count(), 0);
    manager
        .open(launch(
            temp.path(),
            "wc_shell_after_close_busy",
            "wc_sess_close_busy",
        ))
        .unwrap();
}

#[test]
#[ignore = "manual real-process timing: contains deliberate wall-clock framing delay"]
fn marker_like_and_large_output_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let mut spec = launch(temp.path(), "wc_shell_output", "wc_sess_output");
    spec.max_output_bytes = 1024;
    manager.open(spec).unwrap();
    let result = exec(
        &manager,
        "wc_shell_output",
        "wc_sess_output",
        "(printf 'WCPS1 fake background marker\\n') & sleep 0.15; i=0; while [ \"$i\" -lt 5000 ]; do printf x; i=$((i+1)); done",
    );
    assert!(result.command_completed);
    assert!(result.duration_ms >= 100);
    assert!(result.stdout_truncated);
    assert!(result.stdout.len() <= 1024);
}

#[test]
fn output_sync_parser_strips_split_markers() {
    let buffer = Arc::new(Mutex::new(BoundedBuffer::new(4096)));
    let expected = Arc::new(Mutex::new(Some("token123".to_string())));
    let (sender, receiver) = mpsc::sync_channel(1);
    let marker = output_sync_marker(STDOUT_SYNC_MAGIC, "token123");
    let split = marker.len() / 2;
    let mut pending = b"before".to_vec();
    pending.extend_from_slice(&marker[..split]);
    let mut last = None;
    process_output_pending(
        &mut pending,
        &buffer,
        &expected,
        &sender,
        STDOUT_SYNC_MAGIC,
        &mut last,
    );
    assert!(receiver.try_recv().is_err());
    pending.extend_from_slice(&marker[split..]);
    pending.extend_from_slice(b"after");
    process_output_pending(
        &mut pending,
        &buffer,
        &expected,
        &sender,
        STDOUT_SYNC_MAGIC,
        &mut last,
    );
    assert_eq!(receiver.try_recv().unwrap(), "token123");
    assert_eq!(lock_unpoison(&buffer).snapshot_since(0).0, "beforeafter");
}

#[test]
fn stdout_and_stderr_boundaries_are_synchronized_without_marker_leaks() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let mut spec = launch(temp.path(), "wc_shell_stream_sync", "wc_sess_stream_sync");
    spec.max_output_bytes = 64 * 1024;
    manager.open(spec).unwrap();
    let first = exec(
        &manager,
        "wc_shell_stream_sync",
        "wc_sess_stream_sync",
        "i=0; while [ \"$i\" -lt 20000 ]; do printf o; printf e >&2; i=$((i+1)); done",
    );
    assert_eq!(first.stdout.len(), 20000);
    assert_eq!(first.stderr.len(), 20000);
    assert!(!first.stdout.contains("WCPSO1"));
    assert!(!first.stderr.contains("WCPSE1"));
    let next = exec(
        &manager,
        "wc_shell_stream_sync",
        "wc_sess_stream_sync",
        "printf clean; printf error >&2",
    );
    assert_eq!(next.stdout, "clean");
    assert_eq!(next.stderr, "error");
}

#[test]
#[ignore = "manual real-process timing: validates a real shell timeout boundary"]
fn timeout_recovers_or_requires_reset_but_never_accepts_unsynchronized_work() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_timeout", "wc_sess_timeout"))
        .unwrap();
    let timed = manager
        .exec(
            "wc_shell_timeout",
            "wc_sess_timeout",
            "agent:oe:test",
            "sleep 5",
            Duration::from_millis(100),
        )
        .unwrap();
    assert_eq!(timed.execution_state, "timed_out");
    if timed.shell_state == ShellState::Running {
        let next = exec(
            &manager,
            "wc_shell_timeout",
            "wc_sess_timeout",
            "printf synchronized",
        );
        assert_eq!(next.stdout, "synchronized");
    } else {
        assert_eq!(timed.error_code.as_deref(), Some("shell_reset_required"));
        assert!(manager
            .exec(
                "wc_shell_timeout",
                "wc_sess_timeout",
                "agent:oe:test",
                "printf forbidden",
                Duration::from_secs(1),
            )
            .is_err());
    }
}

#[test]
#[ignore = "manual real-process timing: validates idle timeout against wall clock"]
fn idle_timeout_reclaims_only_idle_shells() {
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits {
        idle_timeout: Duration::from_secs(1),
        ..ShellLimits::default()
    });
    manager
        .open(launch(temp.path(), "wc_shell_idle", "wc_sess_idle"))
        .unwrap();
    thread::sleep(Duration::from_millis(1100));
    manager.sweep_idle();
    let status = manager
        .status("wc_shell_idle", "wc_sess_idle", "agent:oe:test")
        .unwrap();
    assert_eq!(status.state, ShellState::Closed);
    assert_eq!(status.close_reason.as_deref(), Some("idle_timeout"));
    assert_eq!(manager.active_count(), 0);

    manager
        .open(launch(
            temp.path(),
            "wc_shell_idle_busy",
            "wc_sess_idle_busy",
        ))
        .unwrap();
    let worker_manager = manager.clone();
    let worker = thread::spawn(move || {
        worker_manager
            .exec(
                "wc_shell_idle_busy",
                "wc_sess_idle_busy",
                "agent:oe:test",
                "sleep 1.3",
                Duration::from_secs(2),
            )
            .unwrap()
    });
    while !manager
        .status("wc_shell_idle_busy", "wc_sess_idle_busy", "agent:oe:test")
        .unwrap()
        .busy
    {
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(1100));
    assert_eq!(manager.sweep_idle(), 0);
    assert!(manager
        .status("wc_shell_idle_busy", "wc_sess_idle_busy", "agent:oe:test",)
        .unwrap()
        .state
        .is_active());
    assert!(worker.join().unwrap().command_completed);
}
