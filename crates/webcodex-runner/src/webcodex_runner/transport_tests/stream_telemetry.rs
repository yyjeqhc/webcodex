
#[test]
fn runner_stream_telemetry_is_fail_open() {
    observe_runtime_metric_fail_open(|| panic!("synthetic metric sink failure"));
}

#[test]
fn runner_stream_telemetry_dimensions_are_closed_and_payload_safe() {
    assert_eq!(
        [
            StreamTransport::WebSocket.name(),
            StreamTransport::Quic.name()
        ],
        ["websocket", "quic"]
    );
    assert_eq!(
        [
            RunnerStreamMetricOutcome::Success.as_str(),
            RunnerStreamMetricOutcome::Closed.as_str(),
            RunnerStreamMetricOutcome::Backpressure.as_str(),
            RunnerStreamMetricOutcome::TransportError.as_str(),
            RunnerStreamMetricOutcome::Timeout.as_str(),
        ],
        [
            "success",
            "closed",
            "backpressure",
            "transport_error",
            "timeout"
        ]
    );
    assert_eq!(bounded_stream_envelope_kind("result"), "result");
    assert_eq!(bounded_stream_envelope_kind("job_update"), "job_update");
    assert_eq!(
        bounded_stream_envelope_kind("project_inventory_status"),
        "project_inventory"
    );
    assert_eq!(
        bounded_stream_envelope_kind("runtime_metadata"),
        "provider_metadata"
    );
    assert_eq!(
        bounded_stream_envelope_kind("/private/path?token=secret"),
        "control"
    );
}

#[test]
fn runner_stream_telemetry_failed_outcomes_never_emit_success_latency_samples() {
    let duration = Some(Duration::from_millis(9));
    assert_eq!(
        successful_stream_duration(RunnerStreamMetricOutcome::Success, duration),
        duration
    );
    assert_eq!(
        successful_stream_duration(RunnerStreamMetricOutcome::Closed, duration),
        None
    );
    assert_eq!(
        successful_stream_duration(RunnerStreamMetricOutcome::Backpressure, duration),
        None
    );
    assert_eq!(
        successful_stream_duration(RunnerStreamMetricOutcome::TransportError, duration),
        None
    );
    assert_eq!(
        successful_stream_duration(RunnerStreamMetricOutcome::Timeout, duration),
        None
    );
}

#[test]
fn runner_stream_control_admission_preserves_best_effort_full_and_closed_semantics() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let (tx, mut rx) = mpsc::channel(1);
        assert!(try_send_runner_stream_control(
            transport,
            &tx,
            RunnerEnvelope::Pong { ts: 1 },
        ));
        assert!(
            !try_send_runner_stream_control(transport, &tx, RunnerEnvelope::Pong { ts: 2 },),
            "a full best-effort control queue must reject immediately"
        );
        assert!(matches!(rx.try_recv(), Ok(RunnerEnvelope::Pong { ts: 1 })));
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        drop(rx);
        assert!(
            !try_send_runner_stream_control(transport, &tx, RunnerEnvelope::Pong { ts: 3 },),
            "a closed control queue must stay non-blocking and fail"
        );
    }
}

fn test_push_sink(
    transport: StreamTransport,
    capacity: usize,
) -> (RunnerSink, mpsc::Receiver<RunnerEnvelope>) {
    let (tx, rx) = mpsc::channel(capacity);
    let sink = match transport {
        StreamTransport::WebSocket => RunnerSink::WebSocket {
            tx,
            client_id: "metric-client".to_string(),
            runner_instance_id: "metric-instance".to_string(),
        },
        StreamTransport::Quic => RunnerSink::Quic {
            tx,
            client_id: "metric-client".to_string(),
            runner_instance_id: "metric-instance".to_string(),
        },
    };
    (sink, rx)
}

fn test_command_result(duration_ms: Option<u64>) -> CommandResult {
    CommandResult {
        exit_code: Some(0),
        stdout: Some("ok\n".to_string()),
        stderr: None,
        duration_ms,
        error: None,
    }
}

fn test_job_update() -> RunnerJobUpdateRequest {
    RunnerJobUpdateRequest {
        client_id: "metric-client".to_string(),
        runner_instance_id: "metric-instance".to_string(),
        job_id: "metric-job".to_string(),
        request_id: Some("metric-request".to_string()),
        update_seq: Some(1),
        status: "running".to_string(),
        stdout_chunk: None,
        stderr_chunk: None,
        log_snapshot: None,
        exit_code: None,
        duration_ms: None,
        error: None,
        command_execution_state: None,
        validation_progress: None,
        test_count_evidence: None,
        activity: None,
        finished: false,
    }
}

#[test]
fn runner_stream_telemetry_websocket_and_quic_share_result_and_job_update_queue_semantics() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let (sink, mut rx) = test_push_sink(transport, 4);
        assert_eq!(
            sink.submit_result("result-request".to_string(), test_command_result(Some(4)))
                .unwrap(),
            ResultSubmission::Accepted
        );
        sink.send_job_update(&test_job_update()).unwrap();

        assert!(matches!(
            rx.blocking_recv(),
            Some(RunnerEnvelope::Result { .. })
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(RunnerEnvelope::JobUpdate { .. })
        ));
    }
}

#[test]
fn runner_stream_telemetry_closed_channel_preserves_transport_closed_result_semantics() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let (sink, rx) = test_push_sink(transport, 1);
        drop(rx);
        let error = sink
            .submit_result("closed-request".to_string(), test_command_result(Some(2)))
            .expect_err("closed writer channel must reject result submission");
        assert!(matches!(error, SubmitResultError::TransportClosed(_)));
    }
}

#[test]
fn runner_stream_telemetry_concurrent_results_remain_distinct_on_one_stream_writer_queue() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let (sink, mut rx) = test_push_sink(transport, 4);
        let left = sink.clone();
        let right = sink.clone();
        let left_thread = thread::spawn(move || {
            left.submit_result("left".to_string(), test_command_result(Some(1)))
                .unwrap();
        });
        let right_thread = thread::spawn(move || {
            right
                .submit_result("right".to_string(), test_command_result(Some(1)))
                .unwrap();
        });
        left_thread.join().unwrap();
        right_thread.join().unwrap();

        let mut request_ids = Vec::new();
        for _ in 0..2 {
            let Some(RunnerEnvelope::Result { payload }) = rx.blocking_recv() else {
                panic!("expected Result envelope");
            };
            request_ids.push(payload.result.request_id);
        }
        request_ids.sort();
        assert_eq!(request_ids, ["left", "right"]);
    }
}

fn test_runner_config(server_url: String) -> RunnerConfig {
    RunnerConfig {
        server_url,
        token: "test-token".to_string(),
        client_id: "oe".to_string(),
        display_name: Some("OE agent".to_string()),
        owner: Some("tester".to_string()),
        hostname: Some("oe-host".to_string()),
        host_context: None,
        project_registry_dir: None,
        poll_interval_ms: 10,
        capabilities: Some(RunnerCapabilities {
            git: true,
            ..RunnerCapabilities::default()
        }),
        max_concurrent_jobs: Some(1),
        // Transport tests run jobs in a temp dir and are not about the
        // filesystem boundary; RunnerPolicy::default() is fail-closed.
        policy: RunnerPolicy {
            allow_cwd_anywhere: true,
            ..RunnerPolicy::default()
        },
        transport: Some(TRANSPORT_WEBSOCKET.to_string()),
        websocket_connect_timeout_secs:
            crate::webcodex_runner::default_websocket_connect_timeout_secs(),
        quic: None,
        shell: ShellConfig::default(),
        skills: super::super::config::SkillsConfig::default(),
        instructions: super::super::config::InstructionsConfig::default(),
        ssh: Default::default(),
        tool_providers: Default::default(),
        mcp_gateway: Default::default(),
        plugins: Default::default(),
        acp: Default::default(),
    }
}

fn polling_runner_config(server_url: String, project_registry_dir: PathBuf) -> RunnerConfig {
    let mut cfg = test_runner_config(server_url);
    cfg.transport = Some(TRANSPORT_POLLING.to_string());
    cfg.project_registry_dir = Some(project_registry_dir);
    cfg
}

fn synthetic_project_summary(index: usize, path_bytes: Option<usize>) -> RunnerProjectSummary {
    let path = match path_bytes {
        Some(bytes) => format!("/{}", "x".repeat(bytes.saturating_sub(1))),
        None => format!("/tmp/project-{index:04}"),
    };
    RunnerProjectSummary {
        id: format!("project-{index:04}"),
        name: Some(format!("Project {index:04}")),
        path,
        allow_patch: true,
        kind: None,
        registration_source: None,
        description: None,
        hooks: Vec::new(),
        disabled: false,
        revision: Some(format!("sha256:{index:064x}")),
        root_fingerprint: None,
        lineage: None,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: index as i64,
        shell_profile: None,
    }
}

fn inventory_status(
    state: &str,
    generation: &str,
    total_reported: usize,
    total_synced: usize,
) -> ShellProjectInventoryStatus {
    ShellProjectInventoryStatus {
        sync_state: state.to_string(),
        generation: Some(generation.to_string()),
        total_reported: Some(total_reported),
        total_synced,
        last_error_code: None,
        last_sync_at: Some(1),
        max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
        max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
    }
}

/// Write synthetic project registry TOML records. Synthetic project roots
/// intentionally do not create target directories on disk: project registry
/// scanning discovers the records faithfully while avoiding spawning hundreds
/// of extraneous Git subprocesses during transport tests.
fn write_synthetic_project_configs(project_registry_dir: &Path, root: &Path, count: usize) {
    std::fs::create_dir_all(project_registry_dir).unwrap();
    for index in 0..count {
        let path = root.join(format!("project-{index:04}"));
        std::fs::write(
            project_registry_dir.join(format!("project-{index:04}.toml")),
            format!(
                "id = \"project-{index:04}\"\nname = \"Project {index:04}\"\npath = {:?}\nallow_patch = true\n",
                path.to_string_lossy()
            ),
        )
        .unwrap();
    }
}

fn test_runtime(cfg: &RunnerConfig) -> RunnerRuntimeState {
    RunnerRuntimeState::new(cfg, PathBuf::new())
}

#[cfg(windows)]
struct WindowsTestHandle(usize);

#[cfg(windows)]
impl WindowsTestHandle {
    fn raw(&self) -> windows_sys::Win32::Foundation::HANDLE {
        self.0 as windows_sys::Win32::Foundation::HANDLE
    }

    fn close(&mut self) {
        if self.0 != 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.raw());
            }
            self.0 = 0;
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsTestHandle {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(windows)]
fn windows_test_pipe() -> (WindowsTestHandle, WindowsTestHandle) {
    use windows_sys::Win32::System::Pipes::CreatePipe;

    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    let created = unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) };
    assert_ne!(created, 0, "CreatePipe failed");
    (
        WindowsTestHandle(read as usize),
        WindowsTestHandle(write as usize),
    )
}

#[cfg(windows)]
#[tokio::test]
async fn windows_parent_pipe_lease_allows_registration_until_writer_closes() {
    use windows_sys::Win32::Storage::FileSystem::WriteFile;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (registered_tx, registered_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _register = read_register(&mut ws).await;
        send_registered_ack(&mut ws).await;
        let _ = registered_tx.send(());

        loop {
            let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .expect("Runner did not close after parent stdin EOF")
                .expect("WebSocket closed before Runner goodbye")
                .expect("Runner shutdown frame is valid");
            if !msg.is_text() {
                continue;
            }
            let envelope = RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap();
            if matches!(envelope, RunnerEnvelope::Goodbye { .. }) {
                break;
            }
        }
    });

    let cfg = test_runner_config(format!("http://{}", addr));
    let runtime = test_runtime(&cfg);
    let (read_pipe, mut write_pipe) = windows_test_pipe();
    let parent_listener =
        spawn_windows_pipe_parent_liveness_listener(read_pipe.0, runtime.clone()).unwrap();

    // Preserve the historical contract that stdin bytes are ignored. More
    // importantly, prove the pipe watcher does not mistake an open idle lease
    // for EOF while WebSocket registration and project inventory run.
    let payload = b"ignored-parent-lease-data";
    let mut bytes_written = 0_u32;
    let wrote = unsafe {
        WriteFile(
            write_pipe.raw(),
            payload.as_ptr(),
            payload.len() as u32,
            &mut bytes_written,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(wrote, 0, "WriteFile failed");
    assert_eq!(bytes_written as usize, payload.len());
    tokio::time::sleep(PARENT_PIPE_POLL_INTERVAL * 3).await;
    assert!(
        !runtime.shutdown_requested(),
        "open parent stdin pipe requested shutdown before registration"
    );

    let session_cfg = cfg.clone();
    let session_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        websocket_session(
            &session_cfg,
            vec![test_project("parent-pipe-registration")],
            "inst-parent-pipe",
            &session_runtime,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), registered_rx)
        .await
        .expect("Runner registration was blocked by the open parent stdin pipe")
        .expect("registration fixture ended before reporting readiness");
    assert!(
        !runtime.shutdown_requested(),
        "open parent stdin pipe requested shutdown after registration"
    );
    assert!(
        !session.is_finished(),
        "registered Runner session ended while the parent stdin pipe was still open"
    );

    write_pipe.close();

    let exit = tokio::time::timeout(Duration::from_secs(5), session)
        .await
        .expect("Runner did not stop after parent stdin EOF")
        .expect("Runner session task panicked")
        .expect("Runner session returned an error");
    assert_eq!(exit, RunnerSessionExit::Shutdown);
    server.await.unwrap();

    tokio::time::timeout(
        Duration::from_secs(1),
        tokio::task::spawn_blocking(move || parent_listener.join().unwrap()),
    )
    .await
    .expect("parent-liveness pipe watcher did not exit after EOF")
    .unwrap();
}

#[cfg(feature = "runner-real-process-tests")]
fn wait_for_path(path: &Path, deadline: Instant, context: &str) {
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {context}: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
