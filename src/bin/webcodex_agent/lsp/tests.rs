use super::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier, OnceLock, Weak};
use std::time::{Duration, Instant};
use tempfile::TempDir;

static FAKE_SERVER: OnceLock<Mutex<Weak<FakeServerBinary>>> = OnceLock::new();

struct FakeServerBinary {
    _temp: TempDir,
    path: PathBuf,
}

fn fake_server_binary() -> Arc<FakeServerBinary> {
    let cache = FAKE_SERVER.get_or_init(|| Mutex::new(Weak::new()));
    let mut cached = cache.lock().unwrap();
    if let Some(binary) = cached.upgrade() {
        return binary;
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest.join("src/bin/webcodex_agent/lsp/fake_server.rs");
    let temp = tempfile::tempdir().unwrap();
    let output = temp
        .path()
        .join(format!("webcodex-lsp-fake{}", env::consts::EXE_SUFFIX));
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let result = Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_lsp_fake")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run rustc for fake LSP server");
    assert!(
        result.status.success(),
        "fake LSP server compilation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let binary = Arc::new(FakeServerBinary {
        _temp: temp,
        path: output,
    });
    *cached = Arc::downgrade(&binary);
    binary
}

struct Fixture {
    // Drop the supervisor before the temporary directory so the fake server
    // can persist its graceful-exit marker during supervisor Drop.
    supervisor: LspSupervisor,
    _fake: Arc<FakeServerBinary>,
    _temp: TempDir,
    root: PathBuf,
    marker: PathBuf,
    exit_marker: PathBuf,
}

impl Fixture {
    fn new(scenario: &str) -> Self {
        Self::with_limits(scenario, 4, Duration::from_secs(60))
    }

    fn with_limits(scenario: &str, maximum: usize, idle_ttl: Duration) -> Self {
        Self::with_config(scenario, maximum, idle_ttl, Duration::from_millis(300), true)
    }

    /// Fixture for tests that pin explicit `cleanup_idle` return values; the
    /// background reaper would race those assertions.
    fn with_manual_cleanup(scenario: &str, maximum: usize, idle_ttl: Duration) -> Self {
        Self::with_config(
            scenario,
            maximum,
            idle_ttl,
            Duration::from_millis(300),
            false,
        )
    }

    fn with_config(
        scenario: &str,
        maximum: usize,
        idle_ttl: Duration,
        shutdown_timeout: Duration,
        background_reaper: bool,
    ) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let marker = temp.path().join("starts.marker");
        let exit_marker = temp.path().join("exit.marker");
        let fake = fake_server_binary();
        let command = LspCommand::new(fake.path.clone())
            .arg(scenario)
            .arg(marker.as_os_str())
            .arg(exit_marker.as_os_str())
            .env("WEBCODEX_LSP_FAKE", "1");
        let supervisor = LspSupervisor::new(LspSupervisorConfig {
            rust_analyzer: Some(command),
            max_servers_per_project: 1,
            max_servers_per_agent: maximum,
            request_timeout: Duration::from_millis(300),
            initialize_timeout: Duration::from_secs(2),
            shutdown_timeout,
            idle_ttl,
            background_reaper,
        });
        Self {
            supervisor,
            _fake: fake,
            _temp: temp,
            root,
            marker,
            exit_marker,
        }
    }

    fn starts(&self) -> usize {
        fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("start:"))
            .count()
    }

    fn start_pids(&self) -> Vec<u32> {
        fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.strip_prefix("start:"))
            .filter_map(|rest| rest.split(':').next())
            .filter_map(|pid| pid.parse().ok())
            .collect()
    }
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

#[test]
fn lsp_supervisor_is_lazy_and_reuses_one_process_for_concurrent_project_calls() {
    let fixture = Fixture::new("normal");
    assert!(!fixture.marker.exists());
    let barrier = Arc::new(Barrier::new(7));
    let handles = (0..6)
        .map(|index| {
            let supervisor = fixture.supervisor.clone();
            let root = fixture.root.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                supervisor
                    .request(
                        &root,
                        LspServerKind::RustAnalyzer,
                        "fake/echo",
                        json!({"index": index}),
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for handle in handles {
        assert_eq!(handle.join().unwrap()["method"], "fake/echo");
    }
    assert_eq!(fixture.starts(), 1);
    let first = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let second = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.process_id(), second.process_id());
}

#[test]
fn lsp_supervisor_uses_distinct_processes_for_distinct_projects() {
    let fixture = Fixture::new("normal");
    let second_root = fixture._temp.path().join("second-project");
    fs::create_dir(&second_root).unwrap();
    let first = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let second = fixture
        .supervisor
        .server_for_test(&second_root, LspServerKind::RustAnalyzer)
        .unwrap();
    assert_ne!(first.process_id(), second.process_id());
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn lsp_supervisor_enforces_agent_capacity() {
    let fixture = Fixture::with_limits("normal", 1, Duration::from_secs(60));
    let second_root = fixture._temp.path().join("second-project");
    fs::create_dir(&second_root).unwrap();
    fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    assert!(matches!(
        fixture
            .supervisor
            .server_for_test(&second_root, LspServerKind::RustAnalyzer),
        Err(LspError::CapacityExceeded { limit: 1 })
    ));
}

#[test]
fn lsp_jsonrpc_handles_interleaved_notifications_and_multiple_request_ids() {
    let fixture = Fixture::new("interleaved");
    for method in ["fake/one", "fake/two", "fake/three"] {
        let result = fixture
            .supervisor
            .request(
                &fixture.root,
                LspServerKind::RustAnalyzer,
                method,
                json!({}),
            )
            .unwrap();
        assert_eq!(result["method"], method);
    }
    assert_eq!(fixture.starts(), 1);
}

#[test]
fn lsp_jsonrpc_surfaces_errors_and_ignores_unknown_response_ids() {
    let errors = Fixture::new("json_error");
    let error = errors
        .supervisor
        .request(
            &errors.root,
            LspServerKind::RustAnalyzer,
            "fake/error",
            json!({}),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        LspError::JsonRpc {
            code: -32001,
            ref message,
            data: Some(_)
        } if message == "fake failure"
    ));

    let unknown = Fixture::new("unknown_id");
    let result = unknown
        .supervisor
        .request(
            &unknown.root,
            LspServerKind::RustAnalyzer,
            "fake/known",
            json!({}),
        )
        .unwrap();
    assert_eq!(result["method"], "fake/known");
}

#[test]
fn lsp_jsonrpc_replies_method_not_found_to_server_requests() {
    let fixture = Fixture::new("server_request");
    let result = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/clientRequest",
            json!({}),
        )
        .unwrap();
    assert_eq!(result["method"], "fake/clientRequest");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    assert_eq!(server.status(), LspServerStatus::Running);
}

#[test]
fn lsp_request_timeout_removes_pending_request() {
    let fixture = Fixture::new("timeout");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let error = fixture
        .supervisor
        .request_with_timeout(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/timeout",
            json!({}),
            Duration::from_millis(40),
        )
        .unwrap_err();
    assert!(matches!(error, LspError::RequestTimeout { .. }));
    assert_eq!(server.pending_count(), 0);
}

#[test]
fn lsp_request_timeout_sends_cancel_request() {
    let fixture = Fixture::new("timeout_cancel");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    // initialize used id=1; next business request is id=2.
    let error = fixture
        .supervisor
        .request_with_timeout(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/timeout",
            json!({}),
            Duration::from_millis(40),
        )
        .unwrap_err();
    assert!(matches!(error, LspError::RequestTimeout { .. }));
    assert_eq!(server.pending_count(), 0);
    assert!(wait_until(Duration::from_secs(1), || {
        fs::read_to_string(&fixture.marker)
            .unwrap_or_default()
            .contains("cancel:")
    }));
    let marker = fs::read_to_string(&fixture.marker).unwrap();
    let cancel_line = marker
        .lines()
        .find(|line| line.starts_with("cancel:"))
        .expect("cancelRequest should reach the fake server");
    assert!(
        cancel_line.contains(r#""method":"$/cancelRequest""#)
            || cancel_line.contains(r#""method": "$/cancelRequest""#),
        "cancel line: {cancel_line}"
    );
    // params.id must match the timed-out request id (2 after initialize=1).
    assert!(
        cancel_line.contains(r#""id":2"#) || cancel_line.contains(r#""id": 2"#),
        "cancel line should reference request id 2: {cancel_line}"
    );
    assert_eq!(server.status(), LspServerStatus::Running);
    // Late unknown responses must not corrupt pending state.
    assert_eq!(server.pending_count(), 0);
}

#[test]
fn lsp_pending_request_receives_server_exit_error() {
    let fixture = Fixture::new("crash_request");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let error = server
        .request("fake/crash", json!({}), Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(error, LspError::ServerExited);
    assert_eq!(server.pending_count(), 0);
    assert_eq!(server.status(), LspServerStatus::Crashed);
}

#[test]
fn lsp_supervisor_restarts_once_then_succeeds() {
    let fixture = Fixture::new("restart_then_success");
    let result = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/restart",
            json!({}),
        )
        .unwrap();
    assert_eq!(result["method"], "fake/restart");
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn lsp_supervisor_never_restarts_more_than_once_per_call() {
    let fixture = Fixture::new("restart_exhausted");
    let error = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/crash",
            json!({}),
        )
        .unwrap_err();
    assert!(
        matches!(error, LspError::RestartExhausted(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn lsp_supervisor_restarts_malformed_alive_process_once_then_succeeds() {
    let fixture = Fixture::new("malformed_alive_then_success");
    let result = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/echo",
            json!({}),
        )
        .unwrap();
    assert_eq!(result["method"], "fake/echo");
    assert_eq!(fixture.starts(), 2);
    // First process must have been reaped even though it stayed alive after
    // emitting malformed JSON.
    let pids = fixture.start_pids();
    assert_eq!(pids.len(), 2);
    assert!(wait_until(Duration::from_secs(2), || !process_exists(
        pids[0]
    )));
    assert!(process_exists(pids[1]));
}

#[test]
fn lsp_supervisor_malformed_alive_exhausts_restart_without_timeout() {
    let fixture = Fixture::new("malformed_alive_always");
    let started = Instant::now();
    let error = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/echo",
            json!({}),
        )
        .unwrap_err();
    assert!(
        matches!(error, LspError::RestartExhausted(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(fixture.starts(), 2);
    // Must not degrade into waiting full request timeouts for a dead reader.
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "restart path took too long: {:?}",
        started.elapsed()
    );
    for pid in fixture.start_pids() {
        assert!(wait_until(Duration::from_secs(2), || !process_exists(pid)));
    }
}

#[test]
fn lsp_initialize_failure_consumes_the_single_restart_budget() {
    let fixture = Fixture::new("initialize_exit");
    let error = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/unreachable",
            json!({}),
        )
        .unwrap_err();
    assert!(
        matches!(error, LspError::RestartExhausted(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn lsp_exit_immediately_after_initialize_is_detected_and_bounded() {
    let fixture = Fixture::new("exit_after_initialize");
    let error = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/unreachable",
            json!({}),
        )
        .unwrap_err();
    assert!(
        matches!(error, LspError::RestartExhausted(_)),
        "unexpected error: {error:?}"
    );
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn lsp_malformed_json_and_invalid_content_length_are_distinct() {
    let malformed = Fixture::new("malformed_json");
    let server = malformed
        .supervisor
        .server_for_test(&malformed.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let error = server
        .request("fake/malformed", json!({}), Duration::from_secs(1))
        .unwrap_err();
    assert!(matches!(error, LspError::MalformedMessage(_)));

    let invalid = Fixture::new("invalid_length");
    let server = invalid
        .supervisor
        .server_for_test(&invalid.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let error = server
        .request("fake/invalid", json!({}), Duration::from_secs(1))
        .unwrap_err();
    assert!(matches!(error, LspError::ProtocolError(_)));
}

#[test]
fn lsp_position_encoding_uses_server_capability_or_utf16_default() {
    for (scenario, expected) in [
        ("utf8", PositionEncoding::Utf8),
        ("utf16", PositionEncoding::Utf16),
        ("utf32", PositionEncoding::Utf32),
        ("normal", PositionEncoding::Utf16),
    ] {
        let fixture = Fixture::new(scenario);
        let server = fixture
            .supervisor
            .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
            .unwrap();
        assert_eq!(server.position_encoding(), expected, "scenario={scenario}");
    }
}

#[test]
fn lsp_initialize_uses_constrained_rust_analyzer_profile() {
    let fixture = Fixture::new("normal");
    let _server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let marker = fs::read_to_string(&fixture.marker).unwrap();
    let initialize_line = marker
        .lines()
        .find(|line| line.starts_with("initialize:"))
        .expect("fake server should record the initialize request body");
    let body_json = initialize_line
        .strip_prefix("initialize:")
        .expect("initialize: prefix");
    let body: serde_json::Value =
        serde_json::from_str(body_json).expect("initialize body must be valid JSON");
    assert_eq!(body["method"], "initialize");

    let params = body
        .get("params")
        .expect("initialize request must include params");
    let options = params
        .get("initializationOptions")
        .expect("initializationOptions must be present for the constrained profile");

    // Fail if any safety field is removed, restored to defaults, or nested wrong.
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false)),
        "cargo.buildScripts.enable must be false: {options}"
    );
    assert_eq!(
        options.pointer("/cargo/noDeps"),
        Some(&json!(true)),
        "cargo.noDeps must be true: {options}"
    );
    assert_eq!(
        options.pointer("/procMacro/enable"),
        Some(&json!(false)),
        "procMacro.enable must be false: {options}"
    );
    assert_eq!(
        options.get("checkOnSave"),
        Some(&json!(false)),
        "checkOnSave must be false: {options}"
    );
    assert_eq!(
        options.pointer("/files/watcher"),
        Some(&json!("server")),
        "files.watcher must be \"server\": {options}"
    );
    assert_eq!(
        options.pointer("/cachePriming/enable"),
        Some(&json!(false)),
        "cachePriming.enable must be false: {options}"
    );

    let canonical = fs::canonicalize(&fixture.root).unwrap();
    let expected_root_uri = Url::from_directory_path(&canonical).unwrap().to_string();
    assert_eq!(
        params.get("rootUri").and_then(Value::as_str),
        Some(expected_root_uri.as_str()),
        "rootUri must be the canonical project root"
    );

    let encodings = params
        .pointer("/capabilities/general/positionEncodings")
        .and_then(Value::as_array)
        .expect("positionEncodings capability must be present");
    let encoding_strings: Vec<&str> = encodings.iter().filter_map(Value::as_str).collect();
    assert!(
        encoding_strings.contains(&"utf-8")
            && encoding_strings.contains(&"utf-16")
            && encoding_strings.contains(&"utf-32"),
        "positionEncodings must include utf-8, utf-16, and utf-32: {encodings:?}"
    );
}

#[test]
fn lsp_crashed_connection_reaps_immediately_without_full_shutdown_deadline() {
    // Crashed-but-alive child must not wait the full shutdown timeout before
    // kill/wait. Use a deliberately large budget so the difference is obvious.
    let shutdown_timeout = Duration::from_secs(1);
    let fixture = Fixture::with_config(
        "malformed_alive_always",
        4,
        Duration::from_secs(3600),
        shutdown_timeout,
        false,
    );
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    let error = server
        .request("fake/malformed", json!({}), Duration::from_secs(1))
        .unwrap_err();
    assert!(matches!(error, LspError::MalformedMessage(_)));
    assert_eq!(server.status(), LspServerStatus::Crashed);
    assert!(
        process_exists(pid),
        "precondition: child stays alive after malformed response"
    );

    let started = Instant::now();
    // cleanup_idle reaps unusable Running slots via the shared shutdown path.
    assert_eq!(fixture.supervisor.cleanup_idle(), 1);
    let elapsed = started.elapsed();

    // configured timeout = 1s; expected completion well under half that budget
    // with normal scheduling tolerance (not a tight flaky ms boundary).
    assert!(
        elapsed < Duration::from_millis(500),
        "crashed connection reap took {elapsed:?}, expected well under {shutdown_timeout:?}"
    );
    assert!(
        elapsed < shutdown_timeout,
        "crashed connection must not consume the full shutdown deadline: {elapsed:?}"
    );
    assert!(wait_until(Duration::from_secs(2), || !process_exists(pid)));
    assert_eq!(fixture.supervisor.server_count_for_test(), 0);
}

#[test]
fn lsp_stderr_capture_is_bounded() {
    let fixture = Fixture::new("stderr_flood");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || server.stderr_len() > 0));
    assert!(server.stderr_len() <= MAX_STDERR_BYTES);
}

#[test]
fn lsp_shutdown_and_drop_reap_the_child_process() {
    let fixture = Fixture::new("normal");
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    fixture.supervisor.shutdown();
    assert!(wait_until(Duration::from_secs(1), || fixture
        .exit_marker
        .exists()));
    assert!(wait_until(Duration::from_secs(1), || !process_exists(pid)));

    let dropped = Fixture::new("normal");
    let server = dropped
        .supervisor
        .server_for_test(&dropped.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    let Fixture {
        supervisor,
        _fake,
        _temp,
        root: _,
        marker: _,
        exit_marker,
    } = dropped;
    drop(server);
    drop(supervisor);
    assert!(wait_until(Duration::from_secs(1), || exit_marker.exists()));
    assert!(wait_until(Duration::from_secs(1), || !process_exists(pid)));
    drop(_fake);
    drop(_temp);
}

#[test]
fn lsp_shutdown_uses_single_deadline_against_hanging_server() {
    // Shutdown timeout 200ms. Multiplied waits would approach 600–800ms+.
    let shutdown_timeout = Duration::from_millis(200);
    let fixture = Fixture::with_config(
        "shutdown_hang",
        4,
        Duration::from_secs(60),
        shutdown_timeout,
        false,
    );
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    // Keep a pending waiter so shutdown must fail_pending as well.
    let pending = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.request("fake/hang", json!({}), Duration::from_secs(5)))
    };
    assert!(wait_until(Duration::from_secs(1), || server
        .pending_count()
        > 0));

    let started = Instant::now();
    fixture.supervisor.shutdown();
    let elapsed = started.elapsed();

    // Single deadline + scheduling slack, far below 3–4× the configured timeout.
    assert!(
        elapsed < shutdown_timeout + Duration::from_millis(400),
        "shutdown took {elapsed:?}, budget was {shutdown_timeout:?}"
    );
    assert!(
        elapsed < shutdown_timeout.saturating_mul(3),
        "shutdown looked like stacked timeouts: {elapsed:?}"
    );
    assert!(wait_until(Duration::from_secs(1), || !process_exists(pid)));
    let pending_result = pending.join().unwrap();
    assert!(
        matches!(
            pending_result,
            Err(LspError::ServerExited) | Err(LspError::RequestTimeout { .. })
        ),
        "pending request should be woken: {pending_result:?}"
    );
}

#[test]
fn lsp_initialize_timeout_cleanup_uses_configured_shutdown_budget() {
    let shutdown_timeout = Duration::from_millis(150);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let marker = temp.path().join("starts.marker");
    let exit_marker = temp.path().join("exit.marker");
    let fake = fake_server_binary();
    let command = LspCommand::new(fake.path.clone())
        .arg("initialize_hang")
        .arg(marker.as_os_str())
        .arg(exit_marker.as_os_str());
    let supervisor = LspSupervisor::new(LspSupervisorConfig {
        rust_analyzer: Some(command),
        max_servers_per_project: 1,
        max_servers_per_agent: 4,
        request_timeout: Duration::from_millis(300),
        initialize_timeout: Duration::from_millis(80),
        shutdown_timeout,
        idle_ttl: Duration::from_secs(60),
        background_reaper: true,
    });

    let started = Instant::now();
    let error = supervisor
        .request(&root, LspServerKind::RustAnalyzer, "fake/nope", json!({}))
        .unwrap_err();
    let elapsed = started.elapsed();
    assert!(
        matches!(
            error,
            LspError::RestartExhausted(_) | LspError::InitializeFailed(_)
        ),
        "unexpected error: {error:?}"
    );
    // Two attempts each: initialize_timeout + shutdown_timeout, plus slack.
    // Must stay well below using the multi-second DEFAULT_SHUTDOWN_TIMEOUT.
    assert!(
        elapsed < Duration::from_secs(2),
        "initialize cleanup used an oversized budget: {elapsed:?}"
    );
    assert!(
        elapsed < DEFAULT_SHUTDOWN_TIMEOUT.saturating_mul(2),
        "cleanup appears to use DEFAULT_SHUTDOWN_TIMEOUT: {elapsed:?}"
    );
    let starts = fs::read_to_string(&marker)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with("start:"))
        .count();
    assert_eq!(starts, 2);
    for line in fs::read_to_string(&marker).unwrap_or_default().lines() {
        if let Some(rest) = line.strip_prefix("start:") {
            if let Some(pid) = rest.split(':').next().and_then(|p| p.parse::<u32>().ok()) {
                assert!(wait_until(Duration::from_secs(2), || !process_exists(pid)));
            }
        }
    }
    drop(supervisor);
    drop(fake);
    drop(temp);
}

#[test]
fn lsp_idle_cleanup_is_explicit_and_bounded() {
    let fixture = Fixture::with_manual_cleanup("normal", 4, Duration::ZERO);
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    drop(server);
    assert_eq!(fixture.supervisor.cleanup_idle(), 1);
    assert!(wait_until(Duration::from_secs(1), || !process_exists(pid)));
}

#[test]
fn lsp_idle_cleanup_skips_active_pending_requests() {
    let fixture = Fixture::with_manual_cleanup("timeout", 4, Duration::ZERO);
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    let server_for_request = Arc::clone(&server);
    let handle = std::thread::spawn(move || {
        server_for_request.request("fake/timeout", json!({}), Duration::from_millis(250))
    });
    assert!(wait_until(Duration::from_secs(1), || server
        .pending_count()
        > 0));
    assert_eq!(fixture.supervisor.cleanup_idle(), 0);
    assert_eq!(fixture.supervisor.server_count_for_test(), 1);
    let error = handle.join().unwrap().unwrap_err();
    assert!(matches!(error, LspError::RequestTimeout { .. }));
    assert_eq!(server.pending_count(), 0);
    // After the request completes, idle TTL=0 allows cleanup.
    assert_eq!(fixture.supervisor.cleanup_idle(), 1);
    assert!(wait_until(Duration::from_secs(1), || !process_exists(pid)));
    assert_eq!(fixture.supervisor.server_count_for_test(), 0);
}

#[test]
fn lsp_idle_cleanup_reaps_crashed_alive_server_immediately() {
    let fixture =
        Fixture::with_manual_cleanup("malformed_alive_always", 4, Duration::from_secs(3600));
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    let error = server
        .request("fake/malformed", json!({}), Duration::from_secs(1))
        .unwrap_err();
    assert!(matches!(error, LspError::MalformedMessage(_)));
    assert_eq!(server.status(), LspServerStatus::Crashed);
    // Process may still be alive, but connection is unusable — cleanup must
    // ignore the long idle TTL and free capacity immediately.
    assert!(process_exists(pid));
    assert_eq!(fixture.supervisor.cleanup_idle(), 1);
    assert!(wait_until(Duration::from_secs(2), || !process_exists(pid)));
    assert_eq!(fixture.supervisor.server_count_for_test(), 0);
}

#[test]
fn lsp_background_reaper_reclaims_idle_capacity_without_explicit_cleanup() {
    // Production agents never call cleanup_idle directly; idle_ttl must be
    // honored by the built-in background reaper or capacity leaks forever.
    let fixture = Fixture::with_limits("normal", 1, Duration::from_millis(100));
    let server = fixture
        .supervisor
        .server_for_test(&fixture.root, LspServerKind::RustAnalyzer)
        .unwrap();
    let pid = server.process_id();
    drop(server);
    assert_eq!(fixture.supervisor.server_count_for_test(), 1);
    assert!(
        wait_until(Duration::from_secs(5), || fixture
            .supervisor
            .server_count_for_test()
            == 0),
        "background reaper must reclaim the idle server after idle_ttl"
    );
    assert!(wait_until(Duration::from_secs(2), || !process_exists(pid)));
    // Freed capacity must be reusable: at max_servers_per_agent=1 a second
    // project start only succeeds because the idle slot was reclaimed.
    let second_root = fixture._temp.path().join("project-second");
    fs::create_dir(&second_root).unwrap();
    fixture
        .supervisor
        .server_for_test(&second_root, LspServerKind::RustAnalyzer)
        .expect("capacity must recover after background reaping");
}

#[test]
fn lsp_project_root_is_canonical_and_external_uris_are_not_trusted() {
    let fixture = Fixture::new("normal");
    let canonical = fs::canonicalize(&fixture.root).unwrap();
    let result = fixture
        .supervisor
        .request(
            &fixture.root,
            LspServerKind::RustAnalyzer,
            "fake/root",
            json!({}),
        )
        .unwrap();
    assert_eq!(result["cwd"], canonical.display().to_string());
    let marker = fs::read_to_string(&fixture.marker).unwrap();
    let root_uri = Url::from_directory_path(&canonical).unwrap().to_string();
    assert!(marker.contains(&root_uri));

    let inside = fixture.root.join("inside.rs");
    fs::write(&inside, "fn main() {}\n").unwrap();
    let inside_uri = Url::from_file_path(&inside).unwrap();
    assert!(matches!(
        classify_uri_against_project_root(&canonical, inside_uri.as_str()),
        ProjectUriClassification::InsideProject(_)
    ));
    let outside = fixture._temp.path().join("outside.rs");
    fs::write(&outside, "outside\n").unwrap();
    let outside_uri = Url::from_file_path(outside).unwrap();
    assert_eq!(
        classify_uri_against_project_root(&canonical, outside_uri.as_str()),
        ProjectUriClassification::OutsideProject
    );
    assert_eq!(
        classify_uri_against_project_root(&canonical, "https://example.test/file.rs"),
        ProjectUriClassification::Unsupported
    );
}

#[test]
fn lsp_rejects_missing_or_non_directory_project_roots_before_spawn() {
    let fixture = Fixture::new("normal");
    let missing = fixture._temp.path().join("missing");
    assert!(matches!(
        fixture.supervisor.request(
            &missing,
            LspServerKind::RustAnalyzer,
            "fake/nope",
            json!({})
        ),
        Err(LspError::InvalidProjectRoot(_))
    ));
    let file = fixture._temp.path().join("file");
    fs::write(&file, "not a directory").unwrap();
    assert!(matches!(
        fixture
            .supervisor
            .request(&file, LspServerKind::RustAnalyzer, "fake/nope", json!({})),
        Err(LspError::InvalidProjectRoot(_))
    ));
    assert!(!fixture.marker.exists());
}

#[test]
fn lsp_command_resolution_uses_explicit_env_then_path_without_shell() {
    let fake = fake_server_binary();
    let explicit = LspSupervisor::new(LspSupervisorConfig {
        rust_analyzer: Some(LspCommand::new(fake.path.as_os_str())),
        ..LspSupervisorConfig::default()
    });
    assert_eq!(
        explicit.availability(LspServerKind::RustAnalyzer),
        LspServerStatus::Available
    );

    let supervisor = LspSupervisor::default();
    let from_env = supervisor
        .resolve_command_from_sources(
            LspServerKind::RustAnalyzer,
            Some(fake.path.as_os_str().to_owned()),
            Some(OsStr::new("")),
        )
        .unwrap();
    assert_eq!(from_env.program, fake.path.as_os_str());

    let path_dir = tempfile::tempdir().unwrap();
    let analyzer = path_dir.path().join("rust-analyzer");
    fs::copy(&fake.path, &analyzer).unwrap();
    let path = env::join_paths([path_dir.path()]).unwrap();
    let from_path = supervisor
        .resolve_command_from_sources(LspServerKind::RustAnalyzer, None, Some(&path))
        .unwrap();
    assert_eq!(from_path.program, analyzer.as_os_str());
    let empty_path = tempfile::tempdir().unwrap();
    assert!(supervisor
        .resolve_command_from_sources(
            LspServerKind::RustAnalyzer,
            None,
            Some(empty_path.path().as_os_str())
        )
        .is_none());

    let spaced = tempfile::tempdir().unwrap();
    let program = spaced.path().join("fake server with spaces");
    fs::hard_link(&fake.path, &program).unwrap();
    let project = tempfile::tempdir().unwrap();
    let marker = spaced.path().join("marker");
    let exit_marker = spaced.path().join("exit");
    let supervisor = LspSupervisor::new(LspSupervisorConfig {
        rust_analyzer: Some(
            LspCommand::new(program)
                .arg("normal")
                .arg(marker.as_os_str())
                .arg(exit_marker.as_os_str()),
        ),
        shutdown_timeout: Duration::from_millis(300),
        initialize_timeout: Duration::from_secs(2),
        ..LspSupervisorConfig::default()
    });
    let value = supervisor
        .request(
            project.path(),
            LspServerKind::RustAnalyzer,
            "fake/direct-command",
            json!({}),
        )
        .unwrap();
    assert_eq!(value["method"], "fake/direct-command");
}

#[cfg(target_os = "linux")]
fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_exists(_pid: u32) -> bool {
    false
}
