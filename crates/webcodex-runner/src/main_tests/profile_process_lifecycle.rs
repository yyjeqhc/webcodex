use super::*;

#[cfg(unix)]
#[test]
fn shell_job_profile_prepare_stop_reaps_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("prepare.pid");
    let started_marker = tmp.path().join("prepare-started.txt");
    let init_script = format!(
        "echo $$ > {}; : > {}; sleep 60",
        shell_tree_quote(&pid_file.to_string_lossy()),
        shell_tree_quote(&started_marker.to_string_lossy())
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
    let policy = unrestricted_test_policy();
    let cache = PreparedShellProfileCache::default();
    let cwd = tmp.path().to_string_lossy().to_string();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = started_marker.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell_with_profiles(
        1,
        &policy,
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        "true",
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("profile prepare stopped during runner shutdown"),
        "{result:?}"
    );
    let prepare_pid = std::fs::read_to_string(&pid_file)
        .expect("read prepare pid")
        .trim()
        .parse::<u32>()
        .expect("parse prepare pid");
    assert!(
        wait_until_process_dead(prepare_pid, Duration::from_secs(10), "prepare"),
        "profile prepare tree survived stop"
    );
}

#[cfg(windows)]
#[test]
fn shell_profile_prepare_timeout_cleans_up_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "prepare-timeout");
    // The init snippet hangs (helper keepalive plus an infinite loop), with a
    // descendant holding the capture pipes; the 30s prepare timeout must
    // terminate the whole tree.
    let init_script = format!(
        "{};\nwhile ($true) {{ Start-Sleep -Seconds 1 }}",
        markers.keepalive_command(&helper)
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
    let start = Instant::now();
    let result = run_shell_with_profiles(
        1,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "true",
        None,
        10,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "prepare tree markers were never written: {result:?}"
    );
    let err = result.error.expect("prepare should time out");
    assert!(
        err.contains("profile prepare timed out after 30 seconds"),
        "{err}"
    );
    assert!(
        start.elapsed() >= Duration::from_secs(29),
        "prepare timed out too early: {:?}",
        start.elapsed()
    );
    markers.assert_tree_dead("prepare-timeout");
}

#[cfg(windows)]
#[test]
fn shell_profile_prepare_stop_cleans_up_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "prepare-stop");
    let pid_file = tmp.path().join("prepare-stop.pid");
    // Write the prepare process pid, hang the direct prepare process, and
    // keep a descendant holding the capture pipes. The helper writes its own
    // parent marker after spawning the descendant.
    let init_script = format!(
        "[IO.File]::WriteAllText({}, [string]$PID); \
         {}; \
         while ($true) {{ Start-Sleep -Seconds 1 }}",
        shell_tree_quote(&pid_file.to_string_lossy()),
        markers.keepalive_command(&helper)
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
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    // Wait for the helper's parent marker (written after the descendant is
    // spawned and the pid file exists) so the stop lands after every marker
    // this test asserts on is on disk.
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell_with_profiles(
        1,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "true",
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("profile prepare stopped during runner shutdown"),
        "{result:?}"
    );
    let prepare_pid = std::fs::read_to_string(&pid_file)
        .expect("read prepare pid")
        .trim()
        .parse::<u32>()
        .expect("parse prepare pid");
    assert!(
        wait_until_process_dead(prepare_pid, Duration::from_secs(10), "prepare"),
        "profile prepare process survived stop"
    );
    markers.assert_tree_dead("prepare-stop");
}
