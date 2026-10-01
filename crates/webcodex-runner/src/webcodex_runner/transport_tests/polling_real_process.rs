
#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process timing: coordinates concurrent shell dispatch completion"]
fn runner_real_process_polling_long_ordinary_dispatch_does_not_pin_and_results_stay_correlated_exactly_once(
) {
    let temp = tempfile::tempdir().unwrap();
    let started_a = temp.path().join("a-started");
    let release_a = temp.path().join("a-release");
    let marker_a = temp.path().join("a-marker");
    let request_a = polling_shell_request(
        "req-slow-a",
        temp.path(),
        gated_marker_command(&started_a, &release_a, &marker_a, "dispatch-a"),
    );
    let request_b = polling_shell_request(
        "req-fast-b",
        temp.path(),
        "printf '%s\\n' 'dispatch-b'".to_string(),
    );

    let poll_count = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = std::sync::mpsc::channel::<String>();
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let results = Arc::clone(&results);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        let event_tx = event_tx.clone();
        let request_a = request_a.clone();
        let request_b = request_b.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                match index {
                    0 => poll_delivery_response(Some(&request_a)),
                    1 => {
                        let _ = event_tx.send("poll-b".to_string());
                        poll_delivery_response(Some(&request_b))
                    }
                    _ => poll_delivery_response(None),
                }
            }
            "/api/shell/agent/result" => {
                let body: serde_json::Value = serde_json::from_str(body).unwrap();
                let request_id = body["request_id"].as_str().unwrap().to_string();
                results.lock().unwrap().push(body);
                let _ = event_tx.send(format!("result-{request_id}"));
                if request_id == "req-slow-a" {
                    runner_shutdown.store(true, Ordering::SeqCst);
                }
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected polling test endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-e1-correlation",
        Arc::clone(&runner_shutdown),
    );

    assert_eq!(
        event_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "poll-b",
        "the next poll must reach the Server before A is released"
    );
    wait_for_path(
        &started_a,
        Instant::now() + Duration::from_secs(5),
        "slow request A to start",
    );
    assert_eq!(
        event_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "result-req-fast-b",
        "B must complete while A is still blocked"
    );
    assert!(!release_a.exists());
    std::fs::write(&release_a, "release\n").unwrap();
    assert_eq!(
        event_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "result-req-slow-a"
    );

    runner
        .finish(Duration::from_secs(10), "polling correlation runner")
        .expect("polling runner should shut down cleanly");
    server.finish();

    let results = results.lock().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["request_id"], "req-fast-b");
    assert_eq!(results[0]["stdout"], "dispatch-b\n");
    assert_eq!(results[1]["request_id"], "req-slow-a");
    assert_eq!(results[1]["stdout"], "dispatch-a\n");
    assert_eq!(
        std::fs::read_to_string(&marker_a).unwrap().lines().count(),
        1,
        "poll continuation and out-of-order completion must not replay A"
    );
    assert!(poll_count.load(Ordering::SeqCst) >= 2);
    assert_eq!(runtime.dispatches.active(), 0);
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process timing: coordinates multiple gated shell workers"]
fn runner_real_process_polling_dispatch_bound_backpressures_without_a_local_pending_queue() {
    let temp = tempfile::tempdir().unwrap();
    let mut requests = Vec::new();
    let mut started = Vec::new();
    let mut releases = Vec::new();
    let mut markers = Vec::new();
    for label in ["a", "b", "c", "d", "e"] {
        let started_path = temp.path().join(format!("{label}-started"));
        let release_path = temp.path().join(format!("{label}-release"));
        let marker_path = temp.path().join(format!("{label}-marker"));
        requests.push(polling_shell_request(
            &format!("req-bound-{label}"),
            temp.path(),
            gated_marker_command(
                &started_path,
                &release_path,
                &marker_path,
                &format!("bound-{label}"),
            ),
        ));
        started.push(started_path);
        releases.push(release_path);
        markers.push(marker_path);
    }

    let poll_count = Arc::new(AtomicUsize::new(0));
    let result_count = Arc::new(AtomicUsize::new(0));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let (fifth_poll_tx, fifth_poll_rx) = std::sync::mpsc::sync_channel(1);
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let result_count = Arc::clone(&result_count);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        let requests = requests.clone();
        Arc::new(move |path: &str, _body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                if index == POLLING_DISPATCH_MAX_IN_FLIGHT {
                    let _ = fifth_poll_tx.send(());
                }
                poll_delivery_response(requests.get(index))
            }
            "/api/shell/agent/result" => {
                if result_count.fetch_add(1, Ordering::SeqCst) + 1 == requests.len() {
                    runner_shutdown.store(true, Ordering::SeqCst);
                }
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected polling bound endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-polling-bound",
        Arc::clone(&runner_shutdown),
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    for path in &started[..POLLING_DISPATCH_MAX_IN_FLIGHT] {
        wait_for_path(
            path,
            deadline,
            "polling workers up to the fixed bound to start",
        );
    }
    assert_eq!(runtime.dispatches.active(), POLLING_DISPATCH_MAX_IN_FLIGHT);
    assert!(
        fifth_poll_rx
            .recv_timeout(Duration::from_millis(200))
            .is_err(),
        "the Runner dequeued an N+1 request while all polling dispatch slots were occupied"
    );

    std::fs::write(&releases[0], "release\n").unwrap();
    fifth_poll_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("releasing one slot must allow exactly the N+1 poll");
    wait_for_path(
        &started[POLLING_DISPATCH_MAX_IN_FLIGHT],
        Instant::now() + Duration::from_secs(5),
        "N+1 polling worker to start",
    );
    assert_eq!(
        runtime.dispatches.active(),
        POLLING_DISPATCH_MAX_IN_FLIGHT,
        "active polling dispatches exceeded the fixed bound"
    );
    for release in &releases[1..] {
        std::fs::write(release, "release\n").unwrap();
    }

    runner
        .finish(Duration::from_secs(10), "bounded polling runner")
        .expect("bounded polling runner should shut down cleanly");
    server.finish();
    assert_eq!(result_count.load(Ordering::SeqCst), requests.len());
    for marker in markers {
        assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    }
    assert_eq!(runtime.dispatches.active(), 0);
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process timing: compares Job and ordinary shell scheduling"]
fn runner_real_process_polling_job_start_dispatches_behind_one_long_ordinary_request() {
    let temp = tempfile::tempdir().unwrap();
    let started_a = temp.path().join("ordinary-started");
    let release_a = temp.path().join("ordinary-release");
    let marker_a = temp.path().join("ordinary-marker");
    let job_marker = temp.path().join("job-marker");
    let request_a = polling_shell_request(
        "req-ordinary",
        temp.path(),
        gated_marker_command(&started_a, &release_a, &marker_a, "ordinary"),
    );
    let request_b = polling_job_request(
        "req-job-start",
        "job-behind-ordinary",
        temp.path(),
        format!("printf '%s\\n' 'job-ran' > {}", posix_quote(&job_marker)),
    );

    let poll_count = Arc::new(AtomicUsize::new(0));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let ordinary_done = Arc::new(AtomicBool::new(false));
    let job_done = Arc::new(AtomicBool::new(false));
    let (job_tx, job_rx) = std::sync::mpsc::sync_channel(1);
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        let ordinary_done = Arc::clone(&ordinary_done);
        let job_done = Arc::clone(&job_done);
        let request_a = request_a.clone();
        let request_b = request_b.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                match index {
                    0 => poll_delivery_response(Some(&request_a)),
                    1 => poll_delivery_response(Some(&request_b)),
                    _ => poll_delivery_response(None),
                }
            }
            "/api/shell/agent/job_update" => {
                let update: serde_json::Value = serde_json::from_str(body).unwrap();
                if update["job_id"] == "job-behind-ordinary"
                    && update["finished"] == true
                    && !job_done.swap(true, Ordering::SeqCst)
                {
                    let _ = job_tx.send(());
                    if ordinary_done.load(Ordering::SeqCst) {
                        runner_shutdown.store(true, Ordering::SeqCst);
                    }
                }
                job_update_success_response()
            }
            "/api/shell/agent/result" => {
                ordinary_done.store(true, Ordering::SeqCst);
                if job_done.load(Ordering::SeqCst) {
                    runner_shutdown.store(true, Ordering::SeqCst);
                }
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected Job-behind-ordinary endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-e1-job",
        Arc::clone(&runner_shutdown),
    );

    job_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Job B must finish while ordinary A remains gated");
    assert!(started_a.exists());
    assert!(!release_a.exists());
    assert_eq!(std::fs::read_to_string(&job_marker).unwrap(), "job-ran\n");
    std::fs::write(&release_a, "release\n").unwrap();

    runner
        .finish(Duration::from_secs(10), "Job-behind-ordinary runner")
        .expect("Job-behind-ordinary runner should shut down cleanly");
    server.finish();
    assert_eq!(
        std::fs::read_to_string(&marker_a).unwrap().lines().count(),
        1
    );
    assert_eq!(runtime.dispatches.active(), 0);
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process lifecycle: waits for a gated --once shell dispatch"]
fn runner_real_process_polling_once_waits_for_its_tracked_ordinary_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let started = temp.path().join("once-started");
    let release = temp.path().join("once-release");
    let marker = temp.path().join("once-marker");
    let request = polling_shell_request(
        "req-once-slow",
        temp.path(),
        gated_marker_command(&started, &release, &marker, "once"),
    );
    let poll_count = Arc::new(AtomicUsize::new(0));
    let result_count = Arc::new(AtomicUsize::new(0));
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let result_count = Arc::clone(&result_count);
        let request = request.clone();
        Arc::new(move |path: &str, _body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                poll_delivery_response((index == 0).then_some(&request))
            }
            "/api/shell/agent/result" => {
                result_count.fetch_add(1, Ordering::SeqCst);
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected polling --once endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        true,
        "inst-e1-once",
        Arc::new(AtomicBool::new(false)),
    );

    wait_for_path(
        &started,
        Instant::now() + Duration::from_secs(5),
        "--once request to start",
    );
    runner.assert_pending("--once returned while its ordinary dispatch was still active");
    std::fs::write(&release, "release\n").unwrap();
    runner
        .finish(Duration::from_secs(5), "--once polling runner")
        .expect("--once runner should complete successfully");
    server.finish();

    assert_eq!(poll_count.load(Ordering::SeqCst), 1);
    assert_eq!(result_count.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    assert!(runtime
        .dispatches
        .wait_until(Instant::now() + Duration::from_secs(1)));
    assert!(
        runtime.background_threads.pending() <= 1,
        "the --once worker must remain in the shutdown-owned registry"
    );
    runtime.shutdown();
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process lifecycle: waits for a gated --once Job drain"]
fn runner_real_process_polling_once_preserves_job_manager_drain_before_exit() {
    let temp = tempfile::tempdir().unwrap();
    let started = temp.path().join("once-job-started");
    let release = temp.path().join("once-job-release");
    let marker = temp.path().join("once-job-marker");
    let request = polling_job_request(
        "req-once-job",
        "job-once-drain",
        temp.path(),
        gated_marker_command(&started, &release, &marker, "once-job"),
    );
    let poll_count = Arc::new(AtomicUsize::new(0));
    let (terminal_tx, terminal_rx) = std::sync::mpsc::sync_channel(1);
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let request = request.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                poll_delivery_response((index == 0).then_some(&request))
            }
            "/api/shell/agent/job_update" => {
                let update: serde_json::Value = serde_json::from_str(body).unwrap();
                if update["finished"] == true {
                    let _ = terminal_tx.send(());
                }
                job_update_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected polling --once Job endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        true,
        "inst-e1-once-job",
        Arc::new(AtomicBool::new(false)),
    );

    wait_for_path(
        &started,
        Instant::now() + Duration::from_secs(5),
        "--once Job to start",
    );
    runner.assert_pending("--once returned before JobManager drained its active Job");
    std::fs::write(&release, "release\n").unwrap();
    terminal_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("--once Job terminal update");
    runner
        .finish(Duration::from_secs(5), "--once Job polling runner")
        .expect("--once Job runner should complete successfully");
    server.finish();

    assert_eq!(poll_count.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    assert!(!runtime.jobs.has_work());
    runtime.shutdown();
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process timing: validates shutdown against an active shell dispatch"]
fn runner_real_process_polling_shutdown_with_active_background_dispatch_is_bounded_and_non_replaying(
) {
    let temp = tempfile::tempdir().unwrap();
    let started = temp.path().join("shutdown-started");
    let never_release = temp.path().join("shutdown-release");
    let marker = temp.path().join("shutdown-marker");
    let request = polling_shell_request(
        "req-shutdown-active",
        temp.path(),
        gated_marker_command(&started, &never_release, &marker, "shutdown"),
    );
    let poll_count = Arc::new(AtomicUsize::new(0));
    let result_bodies = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let result_bodies = Arc::clone(&result_bodies);
        let request = request.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let index = poll_count.fetch_add(1, Ordering::SeqCst);
                poll_delivery_response((index == 0).then_some(&request))
            }
            "/api/shell/agent/result" => {
                result_bodies
                    .lock()
                    .unwrap()
                    .push(serde_json::from_str(body).unwrap());
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected active-shutdown endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_secs(2));
    let shutdown = Arc::new(AtomicBool::new(false));
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-e1-shutdown",
        Arc::clone(&shutdown),
    );

    wait_for_path(
        &started,
        Instant::now() + Duration::from_secs(5),
        "shutdown fixture dispatch to start",
    );
    let shutdown_started = Instant::now();
    shutdown.store(true, Ordering::SeqCst);
    runner
        .finish(Duration::from_secs(5), "active-shutdown polling runner")
        .expect("active-shutdown runner should exit cleanly");
    assert!(
        shutdown_started.elapsed() < Duration::from_secs(3),
        "shutdown exceeded its bounded cleanup budget"
    );
    let polls_after_completion = poll_count.load(Ordering::SeqCst);
    server.finish();
    assert_eq!(
        poll_count.load(Ordering::SeqCst),
        polls_after_completion,
        "polling continued after shutdown completed"
    );

    assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    assert_eq!(runtime.dispatches.active(), 0);
    assert_eq!(runtime.background_threads.pending(), 0);
    let results = result_bodies.lock().unwrap();
    assert!(results.len() <= 1);
    if let Some(result) = results.first() {
        assert_ne!(
            result["command_execution_state"], "not_started",
            "shutdown after dispatch must not rewrite lifecycle truth as pre-start"
        );
    }
}

#[test]
#[cfg(feature = "runner-real-process-tests")]
#[ignore = "manual real-process timing: project registration may spawn Git and uses long readiness fences"]
fn runner_real_process_polling_background_project_operation_invalidates_the_project_cache() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let project_registry_dir = temp.path().join("project-registry");
    std::fs::create_dir_all(&project).unwrap();
    let mut request = sync_file_request("req-register-project");
    request.kind = "register_project".to_string();
    request.stdin = Some(
        serde_json::json!({
            "id": "e1-project",
            "name": "E1 project",
            "path": project,
            "allow_patch": true
        })
        .to_string(),
    );

    let poll_count = Arc::new(AtomicUsize::new(0));
    let project_result_seen = Arc::new(AtomicBool::new(false));
    let refreshed_seen = Arc::new(AtomicBool::new(false));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let (refreshed_tx, refreshed_rx) = std::sync::mpsc::sync_channel(1);
    let handler = {
        let poll_count = Arc::clone(&poll_count);
        let project_result_seen = Arc::clone(&project_result_seen);
        let refreshed_seen = Arc::clone(&refreshed_seen);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        let request = request.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let payload: serde_json::Value = serde_json::from_str(body).unwrap();
                if let Some(response) = project_inventory_poll_response(body) {
                    let refreshed = payload["project_inventory_page"]["projects"]
                        .as_array()
                        .is_some_and(|projects| {
                            projects.iter().any(|project| project["id"] == "e1-project")
                        });
                    if refreshed
                        && project_result_seen.load(Ordering::SeqCst)
                        && !refreshed_seen.swap(true, Ordering::SeqCst)
                    {
                        let _ = refreshed_tx.send(());
                        runner_shutdown.store(true, Ordering::SeqCst);
                    }
                    response
                } else {
                    assert!(
                        payload["projects"].is_null(),
                        "ordinary business poll must not carry project inventory"
                    );
                    let index = poll_count.fetch_add(1, Ordering::SeqCst);
                    poll_delivery_response((index == 0).then_some(&request))
                }
            }
            "/api/shell/agent/result" => {
                project_result_seen.store(true, Ordering::SeqCst);
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected project-cache endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    // Windows service accounts can have std::env::temp_dir() under
    // C:\Windows\SystemTemp; production policy correctly rejects project
    // roots under C:\Windows unless an explicit allowed_roots entry
    // authorizes them. This test exercises polling project-cache
    // invalidation, not project-root safety policy, so authorize this
    // test's own temp root explicitly.
    let mut cfg = polling_runner_config(server.server_url.clone(), project_registry_dir.clone());
    cfg.policy.allowed_roots = vec![temp.path().to_path_buf()];
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-e1-project-cache",
        Arc::clone(&runner_shutdown),
    );

    // The register_project round trip is an actual project operation (it may
    // spawn git); on a loaded runner the poll that observes the refreshed
    // cache can arrive well after a few seconds, so budget generously.
    refreshed_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("a later poll must carry refreshed project metadata");
    runner
        .finish(Duration::from_secs(30), "project-cache polling runner")
        .expect("project-cache runner should shut down cleanly");
    server.finish();
    assert!(project_registry_dir.join("e1-project.toml").exists());
    assert!(poll_count.load(Ordering::SeqCst) >= 2);
    assert!(refreshed_seen.load(Ordering::SeqCst));
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
#[test]
#[ignore = "manual real-process lifecycle: coordinates a real persistent shell with close"]
fn runner_real_process_polling_persistent_shell_exec_remains_responsive_to_close() {
    #[derive(Default)]
    struct PersistentState {
        open_delivered: bool,
        open_done: bool,
        exec_delivered: bool,
        close_delivered: bool,
        result_ids: Vec<String>,
    }

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let project_registry_dir = temp.path().join("project-registry");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&project_registry_dir).unwrap();
    std::fs::write(
        project_registry_dir.join("demo.toml"),
        format!(
            "id = \"demo\"\npath = {:?}\nallow_patch = true\n",
            project.to_string_lossy()
        ),
    )
    .unwrap();
    let started = project.join("persistent-started");
    let marker = project.join("persistent-marker");
    let shell_id = "wc_shell_polling_e1";
    let open = polling_persistent_shell_request("req-ps-open", "open", shell_id, None);
    let exec = polling_persistent_shell_request(
        "req-ps-exec",
        "exec",
        shell_id,
        Some(format!(
            "printf '%s\\n' 'ran' >> {}; : > {}; sleep 30",
            posix_quote(&marker),
            posix_quote(&started)
        )),
    );
    let close = polling_persistent_shell_request("req-ps-close", "close", shell_id, None);
    let state = Arc::new(Mutex::new(PersistentState::default()));
    let allow_close = Arc::new(AtomicBool::new(false));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let handler = {
        let state = Arc::clone(&state);
        let allow_close = Arc::clone(&allow_close);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        let open = open.clone();
        let exec = exec.clone();
        let close = close.clone();
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => register_success_response(),
            "/api/shell/agent/poll" => {
                let mut state = state.lock().unwrap();
                if !state.open_delivered {
                    state.open_delivered = true;
                    poll_delivery_response(Some(&open))
                } else if state.open_done && !state.exec_delivered {
                    state.exec_delivered = true;
                    poll_delivery_response(Some(&exec))
                } else if state.exec_delivered
                    && allow_close.load(Ordering::SeqCst)
                    && !state.close_delivered
                {
                    state.close_delivered = true;
                    poll_delivery_response(Some(&close))
                } else {
                    poll_delivery_response(None)
                }
            }
            "/api/shell/agent/persistent_shell_result" => {
                let result: serde_json::Value = serde_json::from_str(body).unwrap();
                let request_id = result["request_id"].as_str().unwrap().to_string();
                let mut state = state.lock().unwrap();
                if request_id == "req-ps-open" {
                    state.open_done = true;
                }
                state.result_ids.push(request_id);
                if state.result_ids.iter().any(|id| id == "req-ps-exec")
                    && state.result_ids.iter().any(|id| id == "req-ps-close")
                {
                    runner_shutdown.store(true, Ordering::SeqCst);
                }
                result_success_response()
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected persistent-shell polling endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let cfg = polling_runner_config(server.server_url.clone(), project_registry_dir);
    let runtime = test_runtime(&cfg);
    let runner = spawn_polling_runner(
        cfg,
        runtime.clone(),
        false,
        "inst-e1-persistent",
        Arc::clone(&runner_shutdown),
    );

    wait_for_path(
        &started,
        Instant::now() + Duration::from_secs(5),
        "persistent-shell exec to start",
    );
    allow_close.store(true, Ordering::SeqCst);
    runner
        .finish(Duration::from_secs(10), "persistent-shell polling runner")
        .expect("persistent-shell polling runner should shut down cleanly");
    server.finish();

    let mut result_ids = state.lock().unwrap().result_ids.clone();
    result_ids.sort();
    assert_eq!(
        result_ids,
        vec![
            "req-ps-close".to_string(),
            "req-ps-exec".to_string(),
            "req-ps-open".to_string(),
        ]
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap().lines().count(), 1);
    assert_eq!(runtime.persistent_shells.active_count(), 0);
    assert_eq!(runtime.dispatches.active(), 0);
    assert_eq!(runtime.background_threads.pending(), 0);
}
