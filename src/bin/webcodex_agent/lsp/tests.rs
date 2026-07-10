use super::*;
use serde_json::json;
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
            shutdown_timeout: Duration::from_millis(300),
            idle_ttl,
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
fn lsp_idle_cleanup_is_explicit_and_bounded() {
    let fixture = Fixture::with_limits("normal", 4, Duration::ZERO);
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
