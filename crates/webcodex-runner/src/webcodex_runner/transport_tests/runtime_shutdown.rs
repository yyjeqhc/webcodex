
#[test]
fn runtime_shutdown_is_fast_ordered_and_runs_once_without_resources() {
    let cfg = test_runner_config("http://127.0.0.1:1".to_string());
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(300));
    let started = Instant::now();
    let first = runtime.shutdown();
    let second = runtime.shutdown();
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "empty shutdown was not fast"
    );
    assert_eq!(runtime.coordinator.run_count(), 1);
    assert_eq!(first.phases, second.phases);
    assert_eq!(
        first
            .phases
            .iter()
            .map(|phase| phase.phase)
            .collect::<Vec<_>>(),
        vec![
            "signal_received",
            "stop_accepting_work",
            "config_reload_stop",
            "queued_jobs_cancel",
            "active_jobs_signal",
            "active_jobs_drain",
            "external_providers_stop",
            "browser_runtimes_stop",
            "lsp_servers_stop",
            "background_threads_join",
            "shutdown_complete",
        ]
    );
    let lines = first.log_lines();
    assert_eq!(lines.len(), 1, "idle shutdown should stay concise");
    assert!(lines[0].starts_with("webcodex-runner shutdown complete "));
}

#[test]
fn runtime_completion_log_follows_bounded_background_cleanup() {
    let cfg = test_runner_config("http://127.0.0.1:1".to_string());
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(500));
    let (background_ready_tx, background_ready_rx) = std::sync::mpsc::channel();
    let (background_release_tx, background_release_rx) = std::sync::mpsc::channel();
    runtime.register_background_thread(thread::spawn(move || {
        background_ready_tx.send(()).unwrap();
        background_release_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("test must release background cleanup");
    }));
    background_ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("background cleanup fixture did not start");

    let shutdown_runtime = runtime.clone();
    let (report_tx, report_rx) = std::sync::mpsc::channel();
    let shutdown = thread::spawn(move || {
        report_tx.send(shutdown_runtime.shutdown()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    while !runtime.config.is_stopping() {
        assert!(
            Instant::now() < deadline,
            "shutdown never entered stop-accepting phase"
        );
        thread::yield_now();
    }
    assert!(
        matches!(
            report_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "shutdown completed while registered background cleanup was still blocked"
    );
    background_release_tx.send(()).unwrap();
    let report = report_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("shutdown did not complete after background cleanup was released");
    shutdown.join().unwrap();

    let background = report
        .phases
        .iter()
        .find(|phase| phase.phase == "background_threads_join")
        .unwrap();
    assert_eq!(
        background.status,
        super::super::shutdown::ShutdownPhaseStatus::Completed
    );
    let lines = report.log_lines();
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("webcodex-runner shutdown complete "),
        "completion log was not last"
    );
}

#[test]
fn runtime_shutdown_global_budget_bounds_unjoinable_background_thread() {
    let cfg = test_runner_config("http://127.0.0.1:1".to_string());
    let budget = Duration::from_millis(80);
    let runtime = RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), budget);
    runtime.register_background_thread(thread::spawn(|| {
        thread::sleep(Duration::from_millis(400));
    }));

    let started = Instant::now();
    let report = runtime.shutdown();
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(50) && elapsed < Duration::from_millis(250),
        "global shutdown budget was not enforced: {elapsed:?}"
    );
    let background = report
        .phases
        .iter()
        .find(|phase| phase.phase == "background_threads_join")
        .unwrap();
    assert_eq!(
        background.status,
        super::super::shutdown::ShutdownPhaseStatus::TimedOut
    );
    assert_eq!(runtime.coordinator.run_count(), 1);
    assert!(
        report
            .log_lines()
            .last()
            .unwrap()
            .starts_with("webcodex-runner shutdown complete "),
        "completion must be emitted after the timed-out cleanup attempt"
    );
}

#[test]
fn runtime_shutdown_wakes_and_joins_reload_listener() {
    let cfg = test_runner_config("http://127.0.0.1:1".to_string());
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(500));
    let config = Arc::clone(&runtime.config);
    runtime.register_reload_thread(thread::spawn(move || {
        while !config.is_stopping() {
            thread::yield_now();
        }
    }));
    let report = runtime.shutdown();
    let reload = report
        .phases
        .iter()
        .find(|phase| phase.phase == "config_reload_stop")
        .unwrap();
    assert_eq!(
        reload.status,
        super::super::shutdown::ShutdownPhaseStatus::Completed
    );
    assert_eq!(reload.resources, 1);
}

fn test_project(id: &str) -> RunnerProjectSummary {
    RunnerProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: format!("/tmp/{}", id),
        allow_patch: true,
        kind: Some("repo".to_string()),
        registration_source: None,
        description: None,
        hooks: vec!["check".to_string()],
        disabled: false,
        revision: None,
        root_fingerprint: None,
        lineage: None,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: 123,
        shell_profile: None,
    }
}

fn header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut buf = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).expect("read request");
        assert!(n > 0, "client closed before sending a complete request");
        buf.extend_from_slice(&chunk[..n]);
        let Some(end) = header_end(&buf) else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buf[..end]);
        let expected = end + 4 + content_length(&headers);
        if buf.len() >= expected {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

async fn read_async_http_headers(stream: &mut tokio::net::TcpStream) -> String {
    use tokio::io::AsyncReadExt;

    let mut bytes = Vec::new();
    loop {
        assert!(bytes.len() < 64 * 1024, "test HTTP header exceeded bound");
        let mut byte = [0u8; 1];
        let read = stream.read(&mut byte).await.unwrap();
        assert!(read > 0, "peer closed before HTTP headers completed");
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return String::from_utf8(bytes).unwrap();
        }
    }
}

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
}

fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    )
    .unwrap();
}

fn start_polling_http_server(
    poll_status: &str,
    poll_content_type: &str,
    poll_body: &str,
    once: bool,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let poll_count = Arc::new(AtomicUsize::new(0));
    let server_poll_count = Arc::clone(&poll_count);
    let poll_status = poll_status.to_string();
    let poll_content_type = poll_content_type.to_string();
    let poll_body = poll_body.to_string();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert_eq!(request_path(&request), "/api/shell/agent/register");
        let response = register_inventory_support_response();
        write_http_response(
            &mut stream,
            response.status,
            response.content_type,
            &response.body,
        );

        if once {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert_eq!(request_path(&request), "/api/shell/agent/poll");
            let body = request
                .find("\r\n\r\n")
                .map(|index| &request[index + 4..])
                .unwrap_or_default();
            let response = project_inventory_poll_response(body)
                .expect("once-mode first poll must publish canonical project inventory");
            server_poll_count.fetch_add(1, Ordering::SeqCst);
            write_http_response(
                &mut stream,
                response.status,
                response.content_type,
                &response.body,
            );
        } else {
            let mut stream = accept_business_poll(&listener);
            server_poll_count.fetch_add(1, Ordering::SeqCst);
            write_http_response(&mut stream, &poll_status, &poll_content_type, &poll_body);
        }
    });
    (format!("http://{}", addr), poll_count, server)
}

fn start_auto_fallback_http_server(
    poll_status: &str,
    poll_content_type: &str,
    poll_body: &str,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let poll_count = Arc::new(AtomicUsize::new(0));
    let server_poll_count = Arc::clone(&poll_count);
    let poll_status = poll_status.to_string();
    let poll_content_type = poll_content_type.to_string();
    let poll_body = poll_body.to_string();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert_eq!(request_path(&request), "/api/agents/ws");
        write_http_response(
            &mut stream,
            "503 Service Unavailable",
            "text/plain",
            "websocket unavailable",
        );

        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert_eq!(request_path(&request), "/api/shell/agent/register");
        let response = register_inventory_support_response();
        write_http_response(
            &mut stream,
            response.status,
            response.content_type,
            &response.body,
        );

        let mut stream = accept_business_poll(&listener);
        server_poll_count.fetch_add(1, Ordering::SeqCst);
        write_http_response(&mut stream, &poll_status, &poll_content_type, &poll_body);

        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert_eq!(request_path(&request), "/api/shell/agent/register");
        let response = register_inventory_support_response();
        write_http_response(
            &mut stream,
            response.status,
            response.content_type,
            &response.body,
        );

        let mut stream = accept_business_poll(&listener);
        server_poll_count.fetch_add(1, Ordering::SeqCst);
        write_http_response(
            &mut stream,
            "404 Not Found",
            "application/json",
            r#"{"success":false,"error":"poll endpoint missing"}"#,
        );
    });
    (format!("http://{}", addr), poll_count, server)
}

fn run_polling_runner_against_server(
    poll_status: &str,
    poll_content_type: &str,
    poll_body: &str,
    once: bool,
) -> (Result<(), String>, usize) {
    let (server_url, poll_count, server) =
        start_polling_http_server(poll_status, poll_content_type, poll_body, once);
    let tmp = tempfile::tempdir().unwrap();
    let cfg = polling_runner_config(server_url, tmp.path().join("project-registry"));
    let runtime = test_runtime(&cfg);
    let shutdown = Arc::new(AtomicBool::new(false));
    let failsafe = Arc::clone(&shutdown);
    let (failsafe_cancel_tx, failsafe_cancel_rx) = std::sync::mpsc::channel();
    let failsafe_thread = thread::spawn(move || {
        if failsafe_cancel_rx
            .recv_timeout(Duration::from_secs(2))
            .is_err()
        {
            failsafe.store(true, Ordering::SeqCst);
        }
    });
    let result = run_polling_runner_with_shutdown(cfg, once, "inst-poll-test", shutdown, &runtime);
    let _ = failsafe_cancel_tx.send(());
    failsafe_thread.join().unwrap();
    server.join().unwrap();
    (result, poll_count.load(Ordering::SeqCst))
}

/// Scripted step for the sequential fake agent HTTP server. Each step
/// asserts the endpoint the runner is expected to call next, which is
/// what distinguishes "kept polling" from "re-registered or resubmitted".
enum ScriptStep {
    Register,
    RegisterResponse {
        status: &'static str,
        body: &'static str,
    },
    RegisterTypedResponse {
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    },
    PollDeliver(&'static str),
    #[cfg(unix)]
    PollDeliverRequest(RunnerRequest),
    PollEmpty,
    PollResponse {
        status: &'static str,
        body: &'static str,
    },
    PollTypedResponse {
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    },
    PollOversized {
        declared_len: usize,
    },
    PollClose,
    Result {
        status: &'static str,
        body: &'static str,
    },
}

impl ScriptStep {
    fn expected_path(&self) -> &'static str {
        match self {
            Self::Register | Self::RegisterResponse { .. } | Self::RegisterTypedResponse { .. } => {
                "/api/shell/agent/register"
            }
            Self::PollDeliver(_)
            | Self::PollEmpty
            | Self::PollResponse { .. }
            | Self::PollTypedResponse { .. }
            | Self::PollOversized { .. }
            | Self::PollClose => "/api/shell/agent/poll",
            #[cfg(unix)]
            Self::PollDeliverRequest(_) => "/api/shell/agent/poll",
            Self::Result { .. } => "/api/shell/agent/result",
        }
    }
}

struct ScriptedServer {
    server_url: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    shutdown: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

struct ConcurrentHttpResponse {
    status: &'static str,
    content_type: &'static str,
    body: String,
}

impl ConcurrentHttpResponse {
    fn json(body: impl Into<String>) -> Self {
        Self {
            status: "200 OK",
            content_type: "application/json",
            body: body.into(),
        }
    }
}

struct ConcurrentPollingServer {
    server_url: String,
    done: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

impl ConcurrentPollingServer {
    fn finish(self) {
        self.done.store(true, Ordering::SeqCst);
        self.handle.join().unwrap();
    }
}

struct PollingRunnerHandle {
    result_rx: std::sync::mpsc::Receiver<Result<(), String>>,
    handle: thread::JoinHandle<()>,
}

impl PollingRunnerHandle {
    #[cfg(feature = "runner-real-process-tests")]
    fn assert_pending(&self, context: &str) {
        match self.result_rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("{context}: polling runner disconnected before reporting a result")
            }
            Ok(result) => panic!("{context}: polling runner returned early: {result:?}"),
        }
    }

    fn finish(self, timeout: Duration, context: &str) -> Result<(), String> {
        match self.result_rx.recv_timeout(timeout) {
            Ok(result) => {
                if self.handle.join().is_err() {
                    panic!("{context}: polling runner thread panicked after reporting its result");
                }
                result
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                if self.handle.join().is_err() {
                    panic!("{context}: polling runner thread panicked before reporting its result");
                }
                panic!("{context}: polling runner exited without reporting a result");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("{context}: polling runner exceeded hard timeout {timeout:?}");
            }
        }
    }
}

fn spawn_polling_runner(
    cfg: RunnerConfig,
    runtime: RunnerRuntimeState,
    once: bool,
    instance_id: &str,
    shutdown: Arc<AtomicBool>,
) -> PollingRunnerHandle {
    let instance_id = instance_id.to_string();
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let result = run_polling_runner_with_shutdown(cfg, once, &instance_id, shutdown, &runtime);
        let _ = result_tx.send(result);
    });
    PollingRunnerHandle { result_rx, handle }
}

fn start_concurrent_polling_server(
    handler: Arc<dyn Fn(&str, &str) -> ConcurrentHttpResponse + Send + Sync>,
) -> ConcurrentPollingServer {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let server_done = Arc::clone(&done);
    let handle = thread::spawn(move || {
        let mut connections = Vec::new();
        while !server_done.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // The listener is nonblocking only so this fixture can poll
                    // its shutdown flag. Accepted connections use the blocking
                    // read_http_request helper with a bounded read timeout; make
                    // that contract explicit because Windows may inherit the
                    // listener's nonblocking mode onto accepted sockets.
                    stream.set_nonblocking(false).unwrap();
                    let handler = Arc::clone(&handler);
                    connections.push(thread::spawn(move || {
                        let request = read_http_request(&mut stream);
                        let path = request_path(&request).to_string();
                        let body = request
                            .find("\r\n\r\n")
                            .map(|index| request[index + 4..].to_string())
                            .unwrap_or_default();
                        let response = with_project_inventory_ack(&body, handler(&path, &body));
                        write_http_response(
                            &mut stream,
                            response.status,
                            response.content_type,
                            &response.body,
                        );
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("concurrent polling server accept failed: {error}"),
            }
        }
        for connection in connections {
            connection.join().unwrap();
        }
    });
    ConcurrentPollingServer {
        server_url: format!("http://{addr}"),
        done,
        handle,
    }
}

fn sync_file_request(request_id: &str) -> RunnerRequest {
    RunnerRequest {
        login: false,
        shell: None,
        request_id: request_id.to_string(),
        client_id: "oe".to_string(),
        kind: "file_read".to_string(),
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
        command: String::new(),
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
        persistent_shell: None,
    }
}

#[cfg(unix)]
fn polling_shell_request(request_id: &str, cwd: &Path, command: String) -> RunnerRequest {
    RunnerRequest {
        login: false,
        shell: None,
        request_id: request_id.to_string(),
        client_id: "oe".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().into_owned()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command,
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        plugin_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
fn polling_job_request(
    request_id: &str,
    job_id: &str,
    cwd: &Path,
    command: String,
) -> RunnerRequest {
    let mut request = polling_shell_request(request_id, cwd, command);
    request.kind = "start_job".to_string();
    request.job_id = Some(job_id.to_string());
    request.job_context = Some(crate::webcodex_runner::job_manager::test_job_context(
        cwd,
        Vec::new(),
    ));
    request
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
fn polling_persistent_shell_request(
    request_id: &str,
    action: &str,
    shell_id: &str,
    command: Option<String>,
) -> RunnerRequest {
    let mut request = sync_file_request(request_id);
    request.kind = "persistent_shell".to_string();
    request.command = command.clone().unwrap_or_default();
    request.timeout_secs = 30;
    request.persistent_shell = Some(webcodex_core::runner_protocol::PersistentShellRequest {
        action: action.to_string(),
        shell_id: shell_id.to_string(),
        workflow_session_id: "wc_sess_lkaw44QDG0J6FfyY".to_string(),
        runtime_project_id: "agent:oe:demo".to_string(),
        cwd: None,
        shell: Some("bash".to_string()),
        command,
        timeout_secs: Some(30),
        purpose: Some("test".to_string()),
    });
    request
}

#[cfg(unix)]
fn posix_quote(value: &Path) -> String {
    super::super::shell::shell_quote(&value.to_string_lossy())
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
fn gated_marker_command(started: &Path, release: &Path, marker: &Path, value: &str) -> String {
    format!(
        "printf '%s\\n' '{}' >> {}; : > {}; while [ ! -f {} ]; do sleep 0.01; done; printf '%s\\n' '{}'",
        value,
        posix_quote(marker),
        posix_quote(started),
        posix_quote(release),
        value,
    )
}

#[cfg(feature = "runner-real-process-tests")]
fn poll_delivery_response(request: Option<&RunnerRequest>) -> ConcurrentHttpResponse {
    let request = request
        .map(serde_json::to_value)
        .transpose()
        .unwrap()
        .unwrap_or(serde_json::Value::Null);
    ConcurrentHttpResponse::json(
        serde_json::json!({"success": true, "request": request, "error": null}).to_string(),
    )
}

#[cfg(feature = "runner-real-process-tests")]
fn register_success_response() -> ConcurrentHttpResponse {
    register_inventory_support_response()
}

fn register_inventory_support_response() -> ConcurrentHttpResponse {
    ConcurrentHttpResponse::json(
        serde_json::json!({
            "success": true,
            "client": {
                "client_id": "oe",
                "agent_instance_id": "inst-project-inventory",
                "status": "online",
                "connected": true,
                "last_seen": 1,
                "capabilities": {},
                "pending_requests": 0,
                "projects": [],
                "agent_protocol_generation": RUNNER_PROTOCOL_GENERATION_V2.get(),
                "project_inventory": {
                    "sync_state": "pending",
                    "generation": null,
                    "total_reported": null,
                    "total_synced": 0,
                    "last_error_code": null,
                    "last_sync_at": null,
                    "max_summaries_per_page": PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                    "max_serialized_bytes_per_page": PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
                }
            },
            "error": null
        })
        .to_string(),
    )
}

fn poll_inventory_response(status: &ShellProjectInventoryStatus) -> ConcurrentHttpResponse {
    ConcurrentHttpResponse::json(
        serde_json::json!({
            "success": true,
            "request": null,
            "error": null,
            "project_inventory": status
        })
        .to_string(),
    )
}

fn project_inventory_poll_response(body: &str) -> Option<ConcurrentHttpResponse> {
    let payload: serde_json::Value = serde_json::from_str(body).ok()?;
    let page = payload.get("project_inventory_page")?.as_object()?;
    let generation = page.get("generation")?.as_str()?;
    let total_reported = page.get("total_reported")?.as_u64()? as usize;
    let complete = page.get("complete")?.as_bool()?;
    let page_index = page.get("page_index")?.as_u64()? as usize;
    let page_len = page.get("projects")?.as_array()?.len();
    let total_synced = if complete {
        total_reported
    } else {
        (page_index * PROJECT_INVENTORY_PAGE_MAX_SUMMARIES + page_len).min(total_reported)
    };
    Some(poll_inventory_response(&inventory_status(
        if complete { "complete" } else { "in_progress" },
        generation,
        total_reported,
        total_synced,
    )))
}

fn with_project_inventory_ack(
    request_body: &str,
    mut response: ConcurrentHttpResponse,
) -> ConcurrentHttpResponse {
    if response.status != "200 OK" || response.content_type != "application/json" {
        return response;
    }
    let Some(inventory_response) = project_inventory_poll_response(request_body) else {
        return response;
    };
    let Ok(mut response_json) = serde_json::from_str::<serde_json::Value>(&response.body) else {
        return response;
    };
    let Some(response_object) = response_json.as_object_mut() else {
        return response;
    };
    if response_object.contains_key("project_inventory") {
        return response;
    }
    let inventory_json: serde_json::Value = serde_json::from_str(&inventory_response.body).unwrap();
    response_object.insert(
        "project_inventory".to_string(),
        inventory_json["project_inventory"].clone(),
    );
    response.body = response_json.to_string();
    response
}

fn accept_business_poll(listener: &StdTcpListener) -> TcpStream {
    loop {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert_eq!(request_path(&request), "/api/shell/agent/poll");
        let body = request
            .find("\r\n\r\n")
            .map(|index| &request[index + 4..])
            .unwrap_or_default();
        if let Some(response) = project_inventory_poll_response(body) {
            write_http_response(
                &mut stream,
                response.status,
                response.content_type,
                &response.body,
            );
            continue;
        }
        return stream;
    }
}

#[cfg(feature = "runner-real-process-tests")]
fn result_success_response() -> ConcurrentHttpResponse {
    ConcurrentHttpResponse::json(r#"{"success":true}"#)
}

fn polling_offline_success_response() -> ConcurrentHttpResponse {
    ConcurrentHttpResponse::json(r#"{"success":true,"error":null}"#)
}

#[cfg(all(unix, feature = "runner-real-process-tests"))]
fn job_update_success_response() -> ConcurrentHttpResponse {
    ConcurrentHttpResponse::json(r#"{"success":true,"job":null,"error":null}"#)
}

fn accept_with_deadline(listener: &StdTcpListener, deadline: Duration) -> TcpStream {
    let start = Instant::now();
    listener.set_nonblocking(true).unwrap();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                listener.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    start.elapsed() < deadline,
                    "scripted server timed out waiting for the next agent request"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("scripted server accept failed: {e}"),
        }
    }
}

fn start_scripted_runner_server(steps: Vec<ScriptStep>) -> ScriptedServer {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_shutdown = Arc::clone(&shutdown);
    let handle = thread::spawn(move || {
        let mut steps = steps.into_iter().map(Some).collect::<Vec<_>>();
        let mut remaining = steps.len();
        while remaining > 0 {
            let mut stream = accept_with_deadline(&listener, Duration::from_secs(8));
            let request = read_http_request(&mut stream);
            let path = request_path(&request).to_string();
            let body = request
                .find("\r\n\r\n")
                .map(|index| request[index + 4..].to_string())
                .unwrap_or_default();
            if path == "/api/shell/agent/poll" {
                if let Some(response) = project_inventory_poll_response(&body) {
                    write_http_response(
                        &mut stream,
                        response.status,
                        response.content_type,
                        &response.body,
                    );
                    continue;
                }
            }
            recorded.lock().unwrap().push((path.clone(), body.clone()));
            let Some(index) = steps.iter().position(|step| {
                step.as_ref()
                    .is_some_and(|step| step.expected_path() == path)
            }) else {
                // Normal background dispatch may issue the next poll before a
                // fast worker reaches its scripted result response. When the
                // remaining script contains only result work for that turn,
                // answer the speculative poll as empty without consuming or
                // reordering the result script.
                if path == "/api/shell/agent/poll"
                    && steps.iter().any(|step| {
                        step.as_ref()
                            .is_some_and(|step| step.expected_path() == "/api/shell/agent/result")
                    })
                {
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        r#"{"success":true,"request":null,"error":null}"#,
                    );
                    continue;
                }
                panic!("scripted server has no remaining response for {path}");
            };
            let step = steps[index].take().expect("matched scripted step");
            remaining -= 1;
            match step {
                ScriptStep::Register => {
                    assert_eq!(path, "/api/shell/agent/register");
                    let response = register_inventory_support_response();
                    write_http_response(
                        &mut stream,
                        response.status,
                        response.content_type,
                        &response.body,
                    );
                }
                ScriptStep::RegisterResponse { status, body } => {
                    assert_eq!(path, "/api/shell/agent/register");
                    write_http_response(&mut stream, status, "application/json", body);
                }
                ScriptStep::RegisterTypedResponse {
                    status,
                    content_type,
                    body,
                } => {
                    assert_eq!(path, "/api/shell/agent/register");
                    write_http_response(&mut stream, status, content_type, body);
                }
                ScriptStep::PollDeliver(request_id) => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    let request_json =
                        serde_json::to_string(&sync_file_request(request_id)).unwrap();
                    let body = format!(
                        r#"{{"success":true,"request":{},"error":null}}"#,
                        request_json
                    );
                    write_http_response(&mut stream, "200 OK", "application/json", &body);
                }
                #[cfg(unix)]
                ScriptStep::PollDeliverRequest(request) => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    let request_json = serde_json::to_string(&request).unwrap();
                    let body = format!(
                        r#"{{"success":true,"request":{},"error":null}}"#,
                        request_json
                    );
                    write_http_response(&mut stream, "200 OK", "application/json", &body);
                }
                ScriptStep::PollEmpty => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        r#"{"success":true,"request":null,"error":null}"#,
                    );
                }
                ScriptStep::PollResponse { status, body } => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    write_http_response(&mut stream, status, "application/json", body);
                }
                ScriptStep::PollTypedResponse {
                    status,
                    content_type,
                    body,
                } => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    write_http_response(&mut stream, status, content_type, body);
                }
                ScriptStep::PollOversized { declared_len } => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    write!(
                            stream,
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            declared_len
                        )
                        .unwrap();
                }
                ScriptStep::PollClose => {
                    assert_eq!(path, "/api/shell/agent/poll", "expected poll, got {path}");
                    // Dropping the accepted socket without a response
                    // exercises connection-closed / early-EOF recovery.
                }
                ScriptStep::Result { status, body } => {
                    assert_eq!(
                        path, "/api/shell/agent/result",
                        "expected result submission, got {path}"
                    );
                    write_http_response(&mut stream, status, "application/json", body);
                }
            }
        }
        server_shutdown.store(true, Ordering::SeqCst);
    });
    ScriptedServer {
        server_url: format!("http://{}", addr),
        requests,
        shutdown,
        handle,
    }
}

fn run_polling_runner_against_scripted_server(
    server: &ScriptedServer,
    once: bool,
) -> Result<(), String> {
    let runner_shutdown = Arc::clone(&server.shutdown);
    let failsafe_shutdown = Arc::clone(&server.shutdown);
    let server_url = server.server_url.clone();
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
    let runner = thread::spawn(move || {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = polling_runner_config(server_url, tmp.path().join("project-registry"));
        let runtime = test_runtime(&cfg);
        let result =
            run_polling_runner_with_shutdown(cfg, once, "inst-script", runner_shutdown, &runtime);
        let _ = result_tx.send(result);
    });
    match result_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(result) => {
            runner
                .join()
                .expect("scripted polling runner thread panicked after reporting its result");
            result
        }
        Err(error @ std::sync::mpsc::RecvTimeoutError::Timeout) => {
            failsafe_shutdown.store(true, Ordering::SeqCst);
            panic!("scripted polling runner exceeded hard timeout: {error}");
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            runner
                .join()
                .expect("scripted polling runner thread panicked before reporting its result");
            panic!("scripted polling runner exited without reporting a result");
        }
    }
}

fn recorded_result_bodies(requests: &Mutex<Vec<(String, String)>>) -> Vec<String> {
    requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(path, _)| path == "/api/shell/agent/result")
        .map(|(_, body)| body.clone())
        .collect()
}

fn recorded_paths(requests: &Mutex<Vec<(String, String)>>) -> Vec<String> {
    requests
        .lock()
        .unwrap()
        .iter()
        .map(|(path, _)| path.clone())
        .collect()
}

fn recorded_path_count(requests: &Mutex<Vec<(String, String)>>, expected: &str) -> usize {
    requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(path, _)| path == expected)
        .count()
}
