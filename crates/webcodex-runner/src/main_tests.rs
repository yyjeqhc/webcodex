use super::*;
use crate::webcodex_runner::config::validate_shell_config;
#[cfg(target_os = "linux")]
use crate::webcodex_runner::run_shell_with_profiles_in_sandbox;
use crate::webcodex_runner::{
    handle_project_lifecycle_op, handle_project_op_with_temporary_projects_root,
    handle_resolve_or_register_project,
};
static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII restore for environment variables mutated by tests: restores the
/// previous value (or absence) on drop, even when the test panics, so a
/// failure cannot leak env state into later tests.
struct EnvGuard {
    restored: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn new() -> Self {
        EnvGuard {
            restored: Vec::new(),
        }
    }

    fn set(mut self, name: &'static str, value: &str) -> Self {
        self.capture(name);
        std::env::set_var(name, value);
        self
    }

    fn remove(mut self, name: &'static str) -> Self {
        self.capture(name);
        std::env::remove_var(name);
        self
    }

    fn capture(&mut self, name: &'static str) {
        self.restored.push((name, std::env::var_os(name)));
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.restored.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// Policy for tests that exercise shell/profile behavior inside a temp dir
/// rather than the filesystem boundary itself. `AgentPolicy::default()` is
/// now fail-closed (empty `allowed_roots` reaches nothing), so these tests
/// opt out of the boundary explicitly instead of leaning on a permissive
/// production default.
fn unrestricted_test_policy() -> AgentPolicy {
    AgentPolicy {
        allow_cwd_anywhere: true,
        ..AgentPolicy::default()
    }
}

fn test_config(projects_dir: PathBuf) -> AgentConfig {
    AgentConfig {
        server_url: "http://127.0.0.1:8000".to_string(),
        token: "test-token".to_string(),
        client_id: "oe".to_string(),
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        host_context: None,
        projects_dir: Some(projects_dir),
        temporary_projects_root: None,
        poll_interval_ms: 1000,
        capabilities: None,
        max_concurrent_jobs: None,
        policy: unrestricted_test_policy(),
        shell: ShellConfig::default(),
        ssh: SshConfig::default(),
        transport: None,
        websocket_connect_timeout_secs: default_websocket_connect_timeout_secs(),
        quic: None,
        tool_providers: Default::default(),
    }
}

fn runtime_config(cfg: &AgentConfig) -> Arc<ReloadableAgentConfig> {
    Arc::new(ReloadableAgentConfig::new(cfg.clone(), PathBuf::new()))
}

#[test]
fn bounded_response_body_reader_stops_after_limit_plus_one() {
    let mut reader = std::io::Cursor::new(vec![b'x'; 66]);
    let body = read_bounded_response_body(&mut reader, None, 64).unwrap();
    assert!(body.exceeded_limit);
    assert_eq!(body.bytes.len(), 64);
    assert_eq!(
        reader.position(),
        65,
        "the bounded reader must not consume the unbounded remainder"
    );
}

#[test]
fn response_decode_distinguishes_empty_eof_and_complete_syntax_errors() {
    for bytes in [b"".as_slice(), br#"{"success":true,"request":"#.as_slice()] {
        let error = decode_json_response::<ShellAgentPollResponse>(
            AGENT_POLL_PATH,
            reqwest::StatusCode::OK,
            "application/json",
            BoundedResponseBody {
                bytes: bytes.to_vec(),
                exceeded_limit: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, AgentHttpErrorKind::DecodeTransient);
    }

    let error = decode_json_response::<ShellAgentPollResponse>(
        AGENT_POLL_PATH,
        reqwest::StatusCode::OK,
        "application/json",
        BoundedResponseBody {
            bytes: b"{not-json".to_vec(),
            exceeded_limit: false,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, AgentHttpErrorKind::ProtocolDecode);
    assert!(error.summary.contains("serde_category=syntax"));
    assert!(!error.to_string().contains("{not-json"));
}

#[test]
fn protocol_decode_diagnostics_omit_queries_credentials_and_response_values() {
    let content_type = reqwest::header::HeaderValue::from_static(
        "application/json; authorization=Bearer SECRET-TOKEN",
    );
    let content_type = bounded_response_content_type(Some(&content_type), "SECRET-TOKEN");
    let error = decode_json_response::<ShellAgentPollResponse>(
        "/api/shell/agent/poll?token=SECRET-TOKEN",
        reqwest::StatusCode::OK,
        &content_type,
        BoundedResponseBody {
            bytes: br#"{"success":"SECRET-TOKEN","request":null,"error":null}"#.to_vec(),
            exceeded_limit: false,
        },
    )
    .unwrap_err();
    let message = error.to_string();
    assert_eq!(error.kind, AgentHttpErrorKind::ProtocolDecode);
    assert!(
        message.contains("content_type=application/json"),
        "{message}"
    );
    assert!(!message.contains('?'), "{message}");
    assert!(!message.contains("SECRET-TOKEN"), "{message}");
    assert!(!message.contains("authorization"), "{message}");
    assert!(!message.contains('\n'), "{message}");
}

#[test]
fn result_400_is_classified_permanent_with_bounded_structured_reason() {
    let error = AgentHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        r#"{"success":false,"error":"unknown or expired shell request: req-1"}"#,
    );
    assert_eq!(error.kind, AgentHttpErrorKind::ClientRejected);
    let message = error.to_string();
    assert!(
        message.contains("server rejected /api/shell/agent/result request"),
        "{message}"
    );
    assert!(message.contains("HTTP 400 Bad Request"), "{message}");
    assert!(
        message.contains("unknown or expired shell request: req-1"),
        "{message}"
    );
}

#[test]
fn result_4xx_html_bodies_stay_permanent_and_never_leak_markup() {
    let bad_request = AgentHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        "<html>\n<body><h1>400 Bad Request</h1><center>nginx</center></body>\n</html>",
    );
    assert_eq!(bad_request.kind, AgentHttpErrorKind::ClientRejected);
    assert!(!bad_request.to_string().contains("<html"), "{bad_request}");

    let too_large = AgentHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "<html><center>nginx</center><center>413 Request Entity Too Large</center></html>",
    );
    assert_eq!(too_large.kind, AgentHttpErrorKind::ClientRejected);
    assert!(!too_large.to_string().contains("nginx"), "{too_large}");
}

#[test]
fn result_400_structured_reason_is_bounded_for_large_json_bodies() {
    let huge = format!(r#"{{"success":false,"error":"{}"}}"#, "x".repeat(10_000));
    let error = AgentHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        &huge,
    );
    assert_eq!(error.kind, AgentHttpErrorKind::ClientRejected);
    let message = error.to_string();
    assert!(
        message.chars().count() < 300,
        "unbounded message: {} chars",
        message.chars().count()
    );
}

#[test]
fn http_status_classification_keeps_retryable_auth_and_gateway_kinds() {
    let cases = [
        (reqwest::StatusCode::UNAUTHORIZED, AgentHttpErrorKind::Auth),
        (reqwest::StatusCode::FORBIDDEN, AgentHttpErrorKind::Auth),
        (reqwest::StatusCode::NOT_FOUND, AgentHttpErrorKind::NotFound),
        (
            reqwest::StatusCode::REQUEST_TIMEOUT,
            AgentHttpErrorKind::Status,
        ),
        (
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            AgentHttpErrorKind::Status,
        ),
        (
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            AgentHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::BAD_GATEWAY,
            AgentHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            AgentHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::GATEWAY_TIMEOUT,
            AgentHttpErrorKind::ServerUnavailable,
        ),
    ];
    for (status, expected) in cases {
        let error = AgentHttpError::status("/api/shell/agent/result", status, "{}");
        assert_eq!(error.kind, expected, "status {status}");
    }
}

#[test]
fn register_recovery_classification_is_strict_about_lease_conflicts() {
    let lease = AgentHttpError::status(
        AGENT_REGISTER_PATH,
        reqwest::StatusCode::BAD_REQUEST,
        r#"{"success":false,"error":"agent client oe is already online with a different instance"}"#,
    );
    let lease = RegisterError::from_http(lease, "oe");
    assert_eq!(
        lease.recovery_action(),
        RegisterRecoveryAction::WaitForLease
    );

    for body in [
        r#"{"success":false,"error":"agent client identity is unavailable"}"#,
        r#"{"success":false,"error":"agent token owner is 'alice'; cannot register owner 'bob'"}"#,
        r#"{"success":false,"error":"agent client oe is already online"}"#,
    ] {
        let rejected =
            AgentHttpError::status(AGENT_REGISTER_PATH, reqwest::StatusCode::BAD_REQUEST, body);
        let rejected = RegisterError::from_http(rejected, "oe");
        assert_eq!(
            rejected.recovery_action(),
            RegisterRecoveryAction::Fatal,
            "{body}"
        );
    }
}

#[test]
fn poll_recovery_actions_separate_transport_session_and_fatal_errors() {
    let transient = PollError::from_http(
        AgentHttpError::status(
            AGENT_POLL_PATH,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{}",
        ),
        "oe",
    );
    assert_eq!(
        transient.recovery_action(),
        PollingRecoveryAction::RetryPoll
    );

    let missing_session = PollError::from_http(
        AgentHttpError::status(
            AGENT_POLL_PATH,
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"success":false,"error":"unknown shell client: oe"}"#,
        ),
        "oe",
    );
    assert_eq!(
        missing_session.recovery_action(),
        PollingRecoveryAction::ReRegister
    );

    let ordinary_400 = PollError::from_http(
        AgentHttpError::status(
            AGENT_POLL_PATH,
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"success":false,"error":"invalid poll payload"}"#,
        ),
        "oe",
    );
    assert_eq!(ordinary_400.recovery_action(), PollingRecoveryAction::Fatal);
}

#[test]
fn tls_configuration_markers_are_fatal_but_dns_and_eof_are_not() {
    assert!(looks_like_fatal_tls_request(
        "error: invalid peer certificate: UnknownIssuer"
    ));
    assert!(looks_like_fatal_tls_request(
        "tls error: no application protocol; ALPN mismatch"
    ));
    assert!(!looks_like_fatal_tls_request(
        "dns error: temporary failure in name resolution"
    ));
    assert!(!looks_like_fatal_tls_request(
        "connection closed: unexpected EOF"
    ));
}

#[test]
fn submit_fatal_error_classes_map_to_terminal_poll_contract() {
    assert!(PollError::from_submit(SubmitResultError::FatalAuth("auth".into())).is_terminal());
    assert!(
        PollError::from_submit(SubmitResultError::FatalProtocol("missing".into())).is_terminal()
    );
    assert!(PollError::from_submit(SubmitResultError::FatalConfig("tls".into())).is_terminal());
    assert!(
        PollError::from_submit(SubmitResultError::TransportClosed("closed".into())).is_terminal()
    );
    let shutdown = PollError::from_submit(SubmitResultError::Shutdown("process shutdown".into()));
    assert!(!shutdown.is_terminal());
    assert!(shutdown.is_shutdown());
}

fn reload_toml(
    client_id: &str,
    max_jobs: Option<usize>,
    max_timeout: u64,
    max_output: usize,
    shell_program: &str,
    strategy: &str,
    claude_enabled: bool,
    claude_command: &str,
    search_mapping: &str,
) -> String {
    let max_jobs = max_jobs
        .map(|value| format!("max_concurrent_jobs = {value}\n"))
        .unwrap_or_default();
    format!(
        r#"server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "{client_id}"
owner = "alice"
poll_interval_ms = 1000
{max_jobs}
# Explicit projects_dir: load_config materializes the default from the
# per-user config base, which depends on ambient HOME/USERPROFILE that other
# tests mutate.
projects_dir = "projects.d"
policy.allow_raw_shell = true
policy.allow_cwd_anywhere = true
policy.allowed_roots = ["/"]
policy.max_timeout_secs = {max_timeout}
policy.max_output_bytes = {max_output}
shell.program = "{shell_program}"
shell.args = ["-c"]
tool_providers.strategy = "{strategy}"
tool_providers.claude_code.enabled = {claude_enabled}
tool_providers.claude_code.command = "{claude_command}"
tool_providers.claude_code.args = ["mcp", "serve"]
tool_providers.claude_code.timeout_secs = 30
[tool_providers.claude_code.mapping]
search_project_text = "{search_mapping}"
"#
    )
}

fn reload_fixture() -> (tempfile::TempDir, PathBuf, ReloadableAgentConfig) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        reload_toml(
            "oe",
            None,
            60,
            1024,
            "sh",
            "native",
            false,
            "claude",
            "project_search_generation_1",
        ),
    )
    .unwrap();
    let runtime = ReloadableAgentConfig::new(load_config(&path).unwrap(), path.clone());
    (tmp, path, runtime)
}

#[test]
fn reload_field_classification_is_exhaustive_and_allowlisted() {
    let startup = test_config(PathBuf::from("projects-a"));
    let mut hot_only = startup.clone();
    hot_only.policy.max_timeout_secs += 1;
    hot_only.shell.program = "bash".to_string();
    hot_only.tool_providers.strategy =
        webcodex_runner::config::ToolProviderStrategy::ClaudeCodeThenNative;
    assert!(webcodex_runner::config::restart_required_fields(&startup, &hot_only).is_empty());

    let mut changed = hot_only;
    changed.server_url.push_str("/other");
    changed.token.push('2');
    changed.client_id.push('2');
    changed.display_name = Some("changed".to_string());
    changed.owner = Some("changed".to_string());
    changed.hostname = Some("changed".to_string());
    changed.host_context = Some(shell_protocol::AgentHostContext {
        role: Some("primary_development".to_string()),
        runtime: Some("Prefer this Runner for ordinary development.".to_string()),
        ..Default::default()
    });
    changed.projects_dir = Some(PathBuf::from("projects-b"));
    changed.temporary_projects_root = Some(PathBuf::from("/tmp/webcodex-temporary"));
    changed.poll_interval_ms += 1;
    changed.capabilities = Some(ShellClientCapabilities::default());
    changed.max_concurrent_jobs = Some(4);
    changed.transport = Some(TRANSPORT_QUIC.to_string());
    changed.websocket_connect_timeout_secs += 1;
    changed.quic = Some(quic_client_config());
    assert_eq!(
            webcodex_runner::config::restart_required_fields(&startup, &changed).join(" "),
            "capabilities client_id display_name hostname host_context max_concurrent_jobs owner poll_interval_ms projects_dir temporary_projects_root quic server_url token transport websocket_connect_timeout_secs"
        );
}

#[test]
fn valid_reload_switches_one_complete_generation_and_preserves_old_snapshot() {
    let (_tmp, path, runtime) = reload_fixture();
    let old = runtime.snapshot();

    assert_eq!(
        old.external_tools.configured_search_tool_name(),
        Some("project_search_generation_1")
    );
    assert_eq!(
        old.external_tools.status().claude_code.process_state,
        "not_started"
    );

    std::fs::write(
        &path,
        reload_toml(
            "oe",
            None,
            120,
            2048,
            "bash",
            "claude_code_then_native",
            false,
            "claude",
            "project_search_generation_2",
        ),
    )
    .unwrap();
    let status = runtime.reload();
    let new = runtime.snapshot();

    assert_eq!(status.last_reload_result, "success");
    assert_eq!(status.generation, 2);
    assert!(!status.restart_required);
    assert_eq!(
        (
            old.generation,
            old.policy.max_timeout_secs,
            old.shell.program.as_str()
        ),
        (1, 60, "sh")
    );
    assert_eq!(old.external_tools.status().strategy, "native");
    assert_eq!(
        old.external_tools.configured_search_tool_name(),
        Some("project_search_generation_1")
    );
    assert_eq!(
        (
            new.policy.max_timeout_secs,
            new.policy.max_output_bytes,
            new.shell.program.as_str()
        ),
        (120, 2048, "bash")
    );
    assert_eq!(
        new.external_tools.status().strategy,
        "claude_code_then_native"
    );
    assert_eq!(
        new.external_tools.configured_search_tool_name(),
        Some("project_search_generation_2")
    );
    assert_eq!(
        new.external_tools.status().claude_code.process_state,
        "not_started"
    );
}

#[test]
fn failed_reload_keeps_generation_and_can_recover() {
    let (_tmp, path, runtime) = reload_fixture();
    let old = runtime.snapshot();

    std::fs::remove_file(&path).unwrap();
    let status = runtime.reload();
    assert_eq!(status.generation, 1);
    assert_eq!(
        status.last_reload_error_code.as_deref(),
        Some("config_read_failed")
    );

    for (candidate, code) in [
        ("{ invalid toml".to_string(), "config_parse_failed"),
        (
            reload_toml(
                "oe",
                None,
                60,
                1024,
                "",
                "native",
                false,
                "claude",
                "project_search_generation_1",
            ),
            "config_validation_failed",
        ),
        (
            reload_toml(
                "oe",
                None,
                60,
                1024,
                "sh",
                "native",
                true,
                "",
                "project_search_generation_1",
            ),
            "provider_config_invalid",
        ),
    ] {
        std::fs::write(&path, candidate).unwrap();
        let status = runtime.reload();
        assert_eq!(status.generation, 1);
        assert_eq!(status.last_reload_result, "failure");
        assert_eq!(status.last_reload_error_code.as_deref(), Some(code));
    }
    assert_eq!(old.policy.max_timeout_secs, 60);
    let serialized = serde_json::to_string(&runtime.snapshot().reload_status()).unwrap();
    assert!(!serialized.contains(path.to_string_lossy().as_ref()));
    assert!(!serialized.contains("test-token"));

    std::fs::write(
        &path,
        reload_toml(
            "oe",
            None,
            90,
            1024,
            "sh",
            "native",
            false,
            "claude",
            "project_search_generation_2",
        ),
    )
    .unwrap();
    assert_eq!(runtime.reload().generation, 2);
    assert_eq!(runtime.snapshot().policy.max_timeout_secs, 90);
}

#[test]
fn mixed_reload_applies_hot_fields_and_reports_static_restart_fields() {
    let (_tmp, path, runtime) = reload_fixture();
    std::fs::write(
        &path,
        reload_toml(
            "oe-new",
            Some(8),
            180,
            4096,
            "bash",
            "native",
            false,
            "claude",
            "project_search_generation_2",
        ),
    )
    .unwrap();

    let status = runtime.reload();
    let active = runtime.snapshot();
    assert_eq!(status.last_reload_result, "partial");
    assert!(status.restart_required);
    assert_eq!(
        status.restart_required_fields,
        ["client_id", "max_concurrent_jobs"]
    );
    assert_eq!(
        (
            active.policy.max_timeout_secs,
            active.policy.max_output_bytes,
            active.shell.program.as_str()
        ),
        (180, 4096, "bash")
    );
}

fn quic_client_config() -> QuicClientConfig {
    QuicClientConfig {
        server_addr: "v4.example.test:8443".to_string(),
        server_name: "v4.example.test".to_string(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: default_quic_connect_timeout_secs(),
        keepalive_interval_secs: default_quic_keepalive_interval_secs(),
    }
}

#[test]
fn agent_config_defaults_transport_to_websocket_without_quic_section() {
    // No transport field and no [quic] section: default stays websocket.
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert!(cfg.transport.is_none());
    assert!(cfg.quic.is_none());
    assert_eq!(effective_transport(&cfg), TRANSPORT_WEBSOCKET);
    assert_eq!(
        cfg.websocket_connect_timeout_secs,
        default_websocket_connect_timeout_secs()
    );
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
}

#[test]
fn agent_config_rejects_zero_websocket_connect_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
websocket_connect_timeout_secs = 0
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(
        err.contains("websocket_connect_timeout_secs must be > 0"),
        "{err}"
    );
}

#[test]
fn agent_config_rejects_relative_temporary_projects_root() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
temporary_projects_root = "temporary"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(
        err.contains("temporary_projects_root must be a non-empty absolute path"),
        "{err}"
    );
}

#[test]
fn agent_config_accepts_transport_quic_with_quic_section() {
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "quic"

[quic]
server_addr = "v4.example.test:8443"
server_name = "v4.example.test"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.transport.as_deref(), Some("quic"));
    let quic = cfg.quic.expect("quic section");
    assert_eq!(quic.server_addr, "v4.example.test:8443");
    assert_eq!(quic.server_name, "v4.example.test");
    // Defaults applied.
    assert_eq!(quic.alpn, "webcodex-runner/1");
    assert_eq!(quic.connect_timeout_secs, 10);
    assert_eq!(quic.keepalive_interval_secs, 20);
}

#[test]
fn agent_config_accepts_transport_auto() {
    let toml = r#"
server_url = "http://127.0.0.1:8000"
token = "t"
client_id = "oe"
transport = "auto"
"#;
    let cfg: AgentConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.transport.as_deref(), Some(TRANSPORT_AUTO));
    assert_eq!(effective_transport(&cfg), TRANSPORT_AUTO);
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
}

#[test]
fn auto_transport_plan_tries_quic_then_websocket_then_polling() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some(TRANSPORT_AUTO.to_string());
    cfg.quic = Some(quic_client_config());
    assert_eq!(
        auto_transport_plan(&cfg),
        vec![TRANSPORT_QUIC, TRANSPORT_WEBSOCKET, TRANSPORT_POLLING]
    );
}

#[test]
fn strict_quic_transport_still_requires_quic_section() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some(TRANSPORT_QUIC.to_string());
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("transport=quic requires a [quic] section"));
    assert_eq!(effective_transport(&cfg), TRANSPORT_QUIC);
}

#[test]
fn resolve_quic_config_errors_when_section_missing() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("[quic]"), "err was: {err}");
}

#[test]
fn resolve_quic_config_errors_when_server_addr_or_name_missing() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());

    // Missing server_addr.
    cfg.quic = Some(QuicClientConfig {
        server_addr: "  ".to_string(),
        server_name: "v4.example.test".to_string(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 20,
    });
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("server_addr"), "err was: {err}");

    // Missing server_name.
    cfg.quic = Some(QuicClientConfig {
        server_addr: "v4.example.test:8443".to_string(),
        server_name: String::new(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: 10,
        keepalive_interval_secs: 20,
    });
    let err = resolve_quic_config(&cfg).unwrap_err();
    assert!(err.contains("server_name"), "err was: {err}");
}

#[test]
fn resolve_quic_config_accepts_valid_section() {
    let mut cfg = test_config(PathBuf::from("/tmp/x"));
    cfg.transport = Some("quic".to_string());
    cfg.quic = Some(quic_client_config());
    let resolved = resolve_quic_config(&cfg).unwrap();
    assert_eq!(resolved.server_addr, "v4.example.test:8443");
    assert_eq!(resolved.server_name, "v4.example.test");
}

#[test]
fn resolve_quic_server_addrs_accepts_hostname_port() {
    let addrs = resolve_quic_server_addrs("localhost:8443").unwrap();
    assert!(addrs.iter().any(|addr| addr.port() == 8443));
}

#[test]
fn resolve_quic_server_addrs_rejects_missing_port() {
    let err = resolve_quic_server_addrs("localhost").unwrap_err();
    assert!(err.contains("failed to resolve"), "err was: {err}");
}

#[test]
fn quic_client_bind_addr_matches_remote_address_family() {
    let v4: SocketAddr = "127.0.0.1:8443".parse().unwrap();
    let v6: SocketAddr = "[::1]:8443".parse().unwrap();
    assert!(quic_client_bind_addr_for(v4).is_ipv4());
    assert!(quic_client_bind_addr_for(v6).is_ipv6());
}

#[test]
fn agent_cli_help_and_version_exit_before_runtime() {
    match parse_agent_args(["--help"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("Usage: webcodex-runner"));
            assert!(!stdout.contains("webcodex-runner init"));
            assert!(stderr.is_empty());
        }
        other => panic!("expected help exit, got {other:?}"),
    }
    match parse_agent_args(["--version"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.starts_with(&format!(
                "webcodex-runner {} (commit ",
                env!("CARGO_PKG_VERSION")
            )));
            assert!(stdout.trim_end().ends_with(')'));
            assert_ne!(
                stdout,
                format!("webcodex-runner {}\n", env!("CARGO_PKG_VERSION"))
            );
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn agent_cli_has_no_init_alias() {
    let error = parse_agent_args(["init"]).unwrap_err();
    assert!(error.contains("unknown argument: init"));
}

#[test]
fn agent_version_output_includes_build_metadata() {
    match parse_agent_args(["-V"]).unwrap() {
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 0);
            assert!(stdout.contains("commit "));
            assert!(stdout.starts_with("webcodex-runner "));
            assert!(stderr.is_empty());
        }
        other => panic!("expected version exit, got {other:?}"),
    }
}

#[test]
fn agent_cli_legacy_runtime_args_are_preserved() {
    let action = parse_agent_args(["--config", "/tmp/agent.toml", "--once"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: PathBuf::from("/tmp/agent.toml"),
            once: true,
        }
    );
}

#[test]
fn agent_cli_profile_derives_default_config_path() {
    let action = parse_agent_args(["--profile", "special"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: client_profile_agent_config("special").unwrap(),
            once: false,
        }
    );
}

#[test]
fn agent_cli_explicit_config_overrides_profile() {
    let action = parse_agent_args(["--profile", "special", "--config", "/tmp/agent.toml"]).unwrap();
    assert_eq!(
        action,
        AgentCliAction::Run {
            config_path: PathBuf::from("/tmp/agent.toml"),
            once: false,
        }
    );
}

#[test]
fn agent_cli_rejects_unsafe_profile() {
    let err = parse_agent_args(["--profile", "../x"]).unwrap_err();
    assert_eq!(err, CLIENT_PROFILE_ERROR);
}

#[test]
fn empty_tokens_config_parser_accepts_empty_and_whitespace_token() {
    for token in ["", "   "] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
                &path,
                format!(
                    "server_url = \"http://127.0.0.1:8000\"\ntoken = \"{}\"\nclient_id = \"open-agent\"\n[policy]\nallow_cwd_anywhere = true\n",
                    token
                ),
            )
            .unwrap();

        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.token, token);
        assert_eq!(non_empty_token(&cfg.token), None);
    }
}

#[test]
fn agent_config_host_context_is_normalized_closed_and_restart_scoped() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"
projects_dir = "projects.d"

[host_context]
role = " server_host "
runtime = " Prefer this Runner for Server-host operations. "
service = "Use the ordinary host-local service mechanism."

[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();
    let cfg = load_config(&path).unwrap();
    let context = cfg.host_context.as_ref().expect("host context");
    assert_eq!(context.role.as_deref(), Some("server_host"));
    assert_eq!(
        context.runtime.as_deref(),
        Some("Prefer this Runner for Server-host operations.")
    );

    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"
projects_dir = "projects.d"
[host_context]
role = "server_host"
arbitrary = "not allowed"
[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("arbitrary"), "{err}");
}

#[test]
fn agent_config_without_shell_section_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell, ShellConfig::default());
    assert_eq!(cfg.shell.max_persistent_shells, 8);
    assert_eq!(cfg.shell.persistent_shell_idle_timeout_secs, 30 * 60);
}

#[test]
fn agent_config_persistent_shell_limits_are_validated() {
    let mut shell = ShellConfig {
        max_persistent_shells: 0,
        ..Default::default()
    };
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("max_persistent_shells"));

    shell.max_persistent_shells = 65;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("max_persistent_shells"));

    shell.max_persistent_shells = 8;
    shell.persistent_shell_idle_timeout_secs = 0;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("persistent_shell_idle_timeout_secs"));

    shell.persistent_shell_idle_timeout_secs = 86_401;
    assert!(validate_shell_config(&shell)
        .unwrap_err()
        .contains("persistent_shell_idle_timeout_secs"));
}

#[test]
fn agent_config_loads_named_ssh_resources_without_authentication_material() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[ssh.resources.tmp]
host = "tmp"
default_cwd = "/opt/webcodex-edge"

[ssh.resources.no_default]
host = "ops-alias"
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    let tmp = cfg.ssh.resources.get("tmp").unwrap();
    assert_eq!(tmp.host, "tmp");
    assert_eq!(tmp.default_cwd.as_deref(), Some("/opt/webcodex-edge"));
    assert_eq!(
        cfg.ssh
            .resources
            .get("no_default")
            .and_then(|resource| resource.default_cwd.as_deref()),
        None
    );
}

#[test]
fn agent_config_shell_profiles_parse() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "sh"
args = ["-c"]
init_script = '''
export RUST_BACKTRACE=1
'''

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/bin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"

[shell.profiles.py-venv]
description = "Project-local Python virtual environment"
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell.default_profile.as_deref(), Some("rust"));
    let rust = cfg.shell.profiles.get("rust").unwrap();
    assert_eq!(rust.description.as_deref(), Some("Rust development tools"));
    assert_eq!(rust.program.as_deref(), Some("sh"));
    assert_eq!(rust.args.as_ref().unwrap(), &vec!["-c".to_string()]);
    assert_eq!(
        rust.env.get("CARGO_HOME").map(String::as_str),
        Some("/root/.cargo")
    );
    assert!(rust
        .init_script
        .as_deref()
        .unwrap()
        .contains("RUST_BACKTRACE=1"));
    assert!(cfg.shell.profiles.contains_key("py-venv"));
}

#[test]
fn agent_config_shell_default_profile_must_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
program = "sh"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("default_profile"), "{err}");
    assert!(err.contains("missing"), "{err}");
}

#[test]
fn agent_config_shell_profile_name_must_be_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles."bad/name"]
program = "sh"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("shell profile name"), "{err}");
    assert!(err.contains("slash"), "{err}");
}

#[test]
fn agent_config_shell_profile_type_errors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust]
args = "-c"
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("args"), "{err}");
}

#[test]
fn agent_config_shell_profile_env_type_errors_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell.profiles.rust.env]
PATH = ["/root/.cargo/bin"]
"#,
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("failed to parse config"), "{err}");
    assert!(err.contains("env"), "{err}");
}

#[test]
fn agent_config_shell_errors_do_not_include_init_script_body() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    let secret = "DO_NOT_LEAK_THIS_INLINE_SCRIPT_BODY";
    std::fs::write(
        &path,
        format!(
            r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
default_profile = "missing"

[shell.profiles.rust]
init_script = '''
export SECRET={}
'''
"#,
            secret
        ),
    )
    .unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.contains("default_profile"), "{err}");
    assert!(!err.contains(secret), "{err}");
}

#[test]
fn agent_project_toml_parse_sorts_hook_names() {
    let project = parse_agent_project_toml(
        r#"
id = "webcodex"
path = "/root/git/webcodex"
kind = "rust"
shell_profile = "rust"

[hooks]
precommit = ["cargo test"]
doctor = ["git status --short"]
"#,
    )
    .unwrap();
    let summary = agent_project_summary(&project, 123456, false);
    assert_eq!(summary.id, "webcodex");
    assert_eq!(summary.name.as_deref(), Some("webcodex"));
    assert_eq!(summary.path, "/root/git/webcodex");
    assert_eq!(summary.kind.as_deref(), Some("rust"));
    assert_eq!(summary.hooks, vec!["doctor", "precommit"]);
    assert_eq!(summary.updated_at, 123456);
    assert_eq!(summary.git_branch, None);
    assert_eq!(project.shell_profile.as_deref(), Some("rust"));
}

#[test]
fn agent_project_toml_rejects_invalid_id() {
    let err = parse_agent_project_toml(
        r#"
id = "bad id"
path = "/tmp/webcodex"
"#,
    )
    .unwrap_err();
    assert!(err.contains("ASCII letters"));
}

#[test]
fn agent_project_toml_hints_when_server_projects_format_is_used() {
    let err = parse_agent_project_toml(
        r#"
[projects.smoke]
path = "/root/webcodex-smoke"
"#,
    )
    .unwrap_err();
    assert!(err.contains("missing field"), "{err}");
    assert!(err.contains("server projects.toml"), "{err}");
    assert!(
        err.contains("Agent projects.d files must use top-level fields"),
        "{err}"
    );
    assert!(err.contains("id = \"smoke\""), "{err}");
    assert!(err.contains("path = \"/path/to/repo\""), "{err}");
}

#[test]
fn agent_project_toml_rejects_invalid_shell_profile() {
    let err = parse_agent_project_toml(
        r#"
id = "demo"
path = "/tmp/webcodex"
shell_profile = "../rust"
"#,
    )
    .unwrap_err();
    assert!(err.contains("project.shell_profile"), "{err}");
}

#[test]
fn missing_projects_dir_returns_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("missing-projects.d");
    let projects = load_agent_project_summaries_from_dir(&missing);
    assert!(projects.is_empty());
}

#[test]
fn phase_e2_max_concurrent_jobs_normalizes_to_inventory_capacity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    assert_eq!(DEFAULT_MAX_CONCURRENT_JOBS, 4);
    assert_eq!(max_concurrent_jobs(&cfg), DEFAULT_MAX_CONCURRENT_JOBS);

    cfg.max_concurrent_jobs = Some(0);
    assert_eq!(max_concurrent_jobs(&cfg), 1);

    cfg.max_concurrent_jobs = Some(1);
    assert_eq!(max_concurrent_jobs(&cfg), 1);

    cfg.max_concurrent_jobs = Some(4);
    assert_eq!(max_concurrent_jobs(&cfg), 4);

    cfg.max_concurrent_jobs = Some(8);
    assert_eq!(max_concurrent_jobs(&cfg), 8);

    cfg.max_concurrent_jobs = Some(64);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(65);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(128);
    assert_eq!(max_concurrent_jobs(&cfg), 64);

    cfg.max_concurrent_jobs = Some(usize::MAX);
    assert_eq!(max_concurrent_jobs(&cfg), 64);
}

#[test]
fn phase_e2_polling_dispatch_and_job_execution_concurrency_defaults_are_independent() {
    assert_eq!(POLLING_DISPATCH_MAX_IN_FLIGHT, 2);
    assert_eq!(DEFAULT_MAX_CONCURRENT_JOBS, 4);
    assert_ne!(POLLING_DISPATCH_MAX_IN_FLIGHT, DEFAULT_MAX_CONCURRENT_JOBS);
}

// ---------------------------------------------------------------------------
// Stage 2G: platform-aware shell test helpers.
//
// The default shell is `sh -c` on Unix and native PowerShell on Windows, so
// tests that exercise the default shell use dialect-appropriate command text.
// PowerShell output goes through `[Console]::Out` / `[Console]::Error` (no
// host line terminators), and profile init snippets use the platform's
// variable syntax.
// ---------------------------------------------------------------------------

/// Command text that writes `text` to stdout with no trailing newline, using
/// this platform's default shell dialect.
#[cfg(windows)]
fn shell_echo(text: &str) -> String {
    format!("[Console]::Out.Write('{}')", text.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_echo(text: &str) -> String {
    format!("printf %s '{}'", text.replace('\'', "'\\''"))
}

/// Command text that writes `text` to stderr with no trailing newline.
#[cfg(windows)]
fn shell_echo_err(text: &str) -> String {
    format!("[Console]::Error.Write('{}')", text.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_echo_err(text: &str) -> String {
    format!("printf %s '{}' >&2", text.replace('\'', "'\\''"))
}

/// Command text that writes the value of environment variable `name`.
#[cfg(windows)]
fn shell_env_var(name: &str) -> String {
    format!("[Console]::Out.Write($env:{name})")
}

#[cfg(not(windows))]
fn shell_env_var(name: &str) -> String {
    format!("printf %s \"${}\"", name)
}

/// Command text that echoes stdin back to stdout.
#[cfg(windows)]
fn shell_stdin_cat() -> String {
    "[Console]::Out.Write([Console]::In.ReadToEnd())".to_string()
}

#[cfg(not(windows))]
fn shell_stdin_cat() -> String {
    "cat".to_string()
}

/// Command text that writes `ran` into `path` (a side-effect marker proving a
/// command executed).
#[cfg(windows)]
fn shell_write_file(path: &Path) -> String {
    format!(
        "[IO.File]::WriteAllText({}, 'ran')",
        shell_tree_quote(&path.to_string_lossy())
    )
}

#[cfg(not(windows))]
fn shell_write_file(path: &Path) -> String {
    format!("printf ran > {}", shell_tree_quote(&path.to_string_lossy()))
}

/// Command text that prints `absent` when `name` is not set and `present`
/// when it is set (even to an empty value).
#[cfg(windows)]
fn shell_if_else_env_present(name: &str) -> String {
    format!(
        "if ($null -eq $env:{name}) {{ [Console]::Out.Write('absent') }} else {{ [Console]::Out.Write('present') }}"
    )
}

#[cfg(not(windows))]
fn shell_if_else_env_present(name: &str) -> String {
    format!("if [ -z \"${{{name}+x}}\" ]; then printf absent; else printf present; fi")
}

/// Write a shell init script for this platform's default shell into `dir`
/// that exports `name=value`, and return its path.
#[cfg(windows)]
fn write_init_script(dir: &Path, name: &str, value: &str) -> PathBuf {
    let init = dir.join("init.ps1");
    // PowerShell 5.1 decodes .ps1 files with the system ANSI code page unless
    // a UTF-8 BOM is present; the BOM keeps non-ASCII script bodies intact.
    let mut content = "\u{FEFF}".to_string();
    content.push_str(&format!("$env:{name} = '{}'\n", value.replace('\'', "''")));
    std::fs::write(&init, content).unwrap();
    init
}

#[cfg(not(windows))]
fn write_init_script(dir: &Path, name: &str, value: &str) -> PathBuf {
    let init = dir.join("init.sh");
    std::fs::write(&init, format!("export {name}={value}\n")).unwrap();
    init
}

/// Profile init-script snippet that exports `name=value`, matching this
/// platform's default shell dialect.
#[cfg(windows)]
fn profile_init_export(name: &str, value: &str) -> String {
    format!("$env:{name} = '{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn profile_init_export(name: &str, value: &str) -> String {
    format!("export {name}={value}")
}

#[test]
fn shell_config_default_preserves_sh_c_behavior() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_echo("default-ok"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("default-ok"));
}

#[test]
fn shell_config_default_shell_is_platform_native() {
    let shell = ShellConfig::default();
    #[cfg(windows)]
    {
        // The default Windows shell is native PowerShell; the default command
        // must execute without any sh/Git Bash/WSL on PATH.
        assert_eq!(shell.program, "powershell.exe");
        assert!(shell.args.iter().any(|arg| arg == "-Command"));
        assert!(
            shell.args.iter().any(|arg| arg == "-NoProfile"),
            "default must not load the user's interactive profile"
        );
        let result = run_shell(
            &unrestricted_test_policy(),
            &shell,
            None,
            &shell_echo("power-default-ok"),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("power-default-ok"));
    }
    #[cfg(not(windows))]
    {
        assert_eq!(shell.program, "sh");
        assert_eq!(shell.args, vec!["-c".to_string()]);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn inspect_shell_real_smoke_reads_checks_and_blocks_project_writes() {
    if crate::command_sandbox::inspect_sandbox_available().is_err() {
        // The fail-closed unavailable path has a dedicated sandbox test.
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(&projects_dir).unwrap();
    let manifest =
        "[package]\nname = \"inspect-runner-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    let lockfile = "# This file is automatically @generated by Cargo.\n\
                        # It is not intended for manual editing.\n\
                        version = 3\n\n\
                        [[package]]\n\
                        name = \"inspect-runner-smoke\"\n\
                        version = \"0.1.0\"\n";
    std::fs::write(project.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(project.join("Cargo.lock"), lockfile).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&project)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "inspect@example.invalid"]);
    git(&["config", "user.name", "Inspect Smoke"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "seed"]);

    let run_inspect = |command: &str| {
        run_shell_with_profiles_in_sandbox(
            1,
            &unrestricted_test_policy(),
            &ShellConfig::default(),
            &projects_dir,
            &PreparedShellProfileCache::default(),
            Some(project.to_string_lossy().as_ref()),
            command,
            None,
            60,
            None,
            Some(crate::command_sandbox::INSPECT_SANDBOX_MODE),
        )
    };

    let inspection = run_inspect(
        "if command -v rg >/dev/null 2>&1; then rg 'inspect-runner-smoke' Cargo.toml; fi \
             && git status --short \
             && cargo check --offline \
             && printf scratch-ok > \"$TMPDIR/proof\" \
             && test \"$(cat \"$TMPDIR/proof\")\" = scratch-ok",
    );
    assert_eq!(inspection.exit_code, Some(0), "{inspection:?}");
    assert!(!project.join("target").exists());

    for command in [
        "printf created > created.txt",
        "printf changed > Cargo.toml",
        "truncate -s 0 Cargo.toml",
        "rm Cargo.toml",
        "mv Cargo.toml renamed.toml",
        "sh -c 'printf child > child.txt'",
    ] {
        let denied = run_inspect(command);
        assert_ne!(denied.exit_code, Some(0), "{command}: {denied:?}");
    }
    assert_eq!(
        std::fs::read_to_string(project.join("Cargo.toml")).unwrap(),
        manifest
    );
    assert!(!project.join("created.txt").exists());
    assert!(!project.join("child.txt").exists());
    assert!(!project.join("renamed.toml").exists());

    let normal = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(project.to_string_lossy().as_ref()),
        "printf normal-ok > normal.txt",
        None,
        10,
        None,
    );
    assert_eq!(normal.exit_code, Some(0), "{normal:?}");
    assert_eq!(
        std::fs::read_to_string(project.join("normal.txt")).unwrap(),
        "normal-ok"
    );
}

#[cfg(unix)]
#[test]
fn shell_config_path_prepend_discovers_fake_executable() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).unwrap();
    let exe = bin_dir.join("webcodex-fake-tool");
    std::fs::write(&exe, "#!/bin/sh\nprintf fake-tool-ok\n").unwrap();
    let mut perms = std::fs::metadata(&exe).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&exe, perms).unwrap();
    let shell = ShellConfig {
        path_prepend: vec![bin_dir],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "webcodex-fake-tool",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("fake-tool-ok"));
}

#[cfg(windows)]
#[test]
fn shell_config_path_prepend_discovers_fake_executable_and_keeps_windows_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let bin_dir = tmp.path().join("bin dir");
    std::fs::create_dir(&bin_dir).unwrap();
    // PowerShell executes .cmd files from PATH through cmd.exe; `<nul
    // set /p` writes exactly the payload with no trailing newline, and the
    // explicit `exit /b 0` keeps cmd's errorlevel (set /p with an empty
    // variable name leaves it 1) from leaking into $LASTEXITCODE.
    let exe = bin_dir.join("webcodex-fake-tool.cmd");
    std::fs::write(
        &exe,
        "@echo off\r\n<nul set /p \"=fake-tool-ok\"\r\nexit /b 0\r\n",
    )
    .unwrap();
    let shell = ShellConfig {
        path_prepend: vec![bin_dir],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "webcodex-fake-tool.cmd",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("fake-tool-ok"), "{result:?}");

    // path_prepend must extend the inherited Windows PATH (spelled `Path` in
    // the process block), not replace it: the prepended directory comes
    // first and the System32 entries survive.
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("PATH"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    let path = result.stdout.unwrap();
    let dir_text = tmp.path().join("bin dir").to_string_lossy().to_string();
    assert!(
        path.starts_with(&dir_text),
        "prepended directory is not first in PATH: {path}"
    );
    assert!(
        path.contains("System32"),
        "inherited Windows PATH lost: {path}"
    );
}

#[test]
fn shell_config_dialect_field_parses_and_validates() {
    use crate::webcodex_runner::config::ShellDialect;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
dialect = "powershell"

[shell.profiles.custom]
dialect = "posix"
program = "sh"
args = ["-c"]
"#,
    )
    .unwrap();
    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell.dialect, Some(ShellDialect::PowerShell));
    assert_eq!(
        cfg.shell.profiles.get("custom").unwrap().dialect,
        Some(ShellDialect::Posix)
    );

    // Unknown dialect values are rejected explicitly at parse time.
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
dialect = "cmd"
"#,
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("powershell"), "{err}");
    assert!(err.contains("cmd"), "{err}");
}

#[test]
fn shell_config_default_environment_is_inherited() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let _env = EnvGuard::new().set("WEBCODEX_INHERITED_TEST", "inherited-ok");
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_env_var("WEBCODEX_INHERITED_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("inherited-ok"));
}

#[test]
fn shell_config_env_values_are_available() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let shell = ShellConfig {
        env: HashMap::from([("WEBCODEX_TEST_VALUE".to_string(), "env-ok".to_string())]),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_TEST_VALUE"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("env-ok"));
}

#[test]
fn shell_config_init_script_is_sourced() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let init = write_init_script(tmp.path(), "WEBCODEX_INIT_TEST", "init-ok");
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_INIT_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("init-ok"));
}

#[test]
fn shell_config_init_script_awkward_path_is_sourced() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    // Space, single quote, and non-ASCII characters in the init script path.
    let init_dir = tmp.path().join("init dir '脚本");
    std::fs::create_dir_all(&init_dir).unwrap();
    let init = write_init_script(&init_dir, "WEBCODEX_INIT_TEST", "init-ok");
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_INIT_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("init-ok"));
}

#[test]
fn shell_job_init_script_failure_blocks_command_and_reports_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    // Windows: `exit 3` inside the dot-sourced script terminates the process
    // with 3. Unix: `false` fails the sourced script so `&&` short-circuits
    // with exit 1. Either way the requested command must never run.
    #[cfg(windows)]
    let (init_content, expected_exit) = ("exit 3\n", Some(3));
    #[cfg(not(windows))]
    let (init_content, expected_exit) = ("false\n", Some(1));
    let init = if cfg!(windows) {
        tmp.path().join("fail.ps1")
    } else {
        tmp.path().join("fail.sh")
    };
    std::fs::write(&init, init_content).unwrap();
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let marker = tmp.path().join("command-ran.txt");
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_write_file(&marker),
        None,
        10,
        None,
    );
    assert!(
        !marker.exists(),
        "command ran despite init script failure: {result:?}"
    );
    assert_eq!(result.exit_code, expected_exit, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[test]
fn shell_config_bash_like_args_are_respected_when_available() {
    if !Path::new("/bin/bash").exists() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let shell = ShellConfig {
        program: "/bin/bash".to_string(),
        args: vec!["-lc".to_string()],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "[[ 1 -eq 1 ]] && printf bash-ok",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("bash-ok"));
}

fn shell_with_profiles(
    default_profile: Option<&str>,
    profiles: Vec<(&str, ShellProfileConfig)>,
) -> ShellConfig {
    ShellConfig {
        default_profile: default_profile.map(str::to_string),
        profiles: profiles
            .into_iter()
            .map(|(name, profile)| (name.to_string(), profile))
            .collect(),
        ..ShellConfig::default()
    }
}

fn profile_env(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn write_agent_project(projects_dir: &Path, id: &str, path: &Path, shell_profile: Option<&str>) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let shell_profile = shell_profile
        .map(|profile| format!("shell_profile = {:?}\n", profile))
        .unwrap_or_default();
    std::fs::write(
        projects_dir.join(format!("{}.toml", id)),
        format!(
            "id = {:?}\npath = {:?}\nname = {:?}\n{}",
            id,
            path.to_string_lossy(),
            id,
            shell_profile
        ),
    )
    .unwrap();
}

fn run_profile_shell(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: &Path,
    command: &str,
) -> CommandResult {
    let cwd = cwd.to_string_lossy().to_string();
    run_shell_with_profiles(
        1,
        policy,
        shell,
        projects_dir,
        cache,
        Some(&cwd),
        command,
        None,
        10,
        None,
    )
}

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

fn shell_job_request(cwd: &Path, command: &str) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "req-job".to_string(),
        client_id: "ws-client".to_string(),
        kind: "start_job".to_string(),
        job_id: Some("job-profile".to_string()),
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: command.to_string(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: Some(test_job_context(cwd, Vec::new())),
        persistent_shell: None,
    }
}

#[test]
fn runner_recovery_context_rejects_cross_product_go_test_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let mut request = shell_job_request(temp.path(), "");
    request.kind = "start_validation_job".to_string();
    let cargo_step = ShellJobValidationStep {
        name: "test".to_string(),
        program: "cargo".to_string(),
        args: vec!["test".to_string(), "tool_runtime".to_string()],
        env: Vec::new(),
    };
    request.command = serde_json::to_string(&vec![cargo_step.clone()]).unwrap();
    let context = request.job_context.as_mut().unwrap();
    context.purpose = Some("validation".to_string());
    context.validation_steps = vec!["test".to_string()];
    context.validation = Some(shell_protocol::ShellJobValidationMetadata {
        tool: "go_test".to_string(),
        kind: "test".to_string(),
        steps: vec![cargo_step],
        effective_timeout_secs: 1800,
        sync_wait_secs: 10,
        adapter: "go_test".to_string(),
    });
    let context = context.clone();

    let error = validate_runner_job_context(&context, &request, "ws-client").unwrap_err();
    assert!(error.contains("validation metadata is invalid"), "{error}");
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

fn wait_for_job_envelope(
    rx: &mut tokio::sync::mpsc::Receiver<AgentEnvelope>,
    message: &str,
) -> AgentEnvelope {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match rx.try_recv() {
            Ok(envelope) => return envelope,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => panic!("{message}"),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("{message}: channel disconnected")
            }
        }
    }
}

fn file_read_request(
    cwd: &Path,
    path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    max_bytes: Option<usize>,
) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "req-file-read".to_string(),
        client_id: "agent-1".to_string(),
        kind: "file_read".to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: None,
        max_bytes,
        expected_sha256: None,
        expected_prefix: None,
        start_line,
        end_line,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    }
}

fn line_edit_json(result: CommandResult) -> serde_json::Value {
    assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
    assert!(
        result.error.is_none(),
        "unexpected error: {:?}",
        result.error
    );
    serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
}

fn file_read_json(result: CommandResult) -> serde_json::Value {
    assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
    assert!(
        result.error.is_none(),
        "unexpected error: {:?}",
        result.error
    );
    serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
}

#[test]
fn agent_file_read_without_range_preserves_plain_text_output() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("small.txt"), "one\ntwo\n").unwrap();

    let out = handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "small.txt", None, None, Some(1024)),
    );

    assert_eq!(out.exit_code, Some(0), "unexpected result: {out:?}");
    assert_eq!(out.stdout.as_deref(), Some("one\ntwo\n"));
}

#[cfg(unix)]
#[test]
fn agent_file_read_rejects_symlink_escape_even_when_policy_allows_target() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "outside-secret").unwrap();
    std::os::unix::fs::symlink(&secret, project.path().join("leak.txt")).unwrap();

    let mut policy = project_policy(project.path());
    policy.allowed_roots.push(outside.path().to_path_buf());
    let out = handle_file_request(
        &policy,
        &file_read_request(project.path(), "leak.txt", None, None, Some(1024)),
    );

    assert_eq!(out.exit_code, None);
    assert_eq!(out.error.as_deref(), Some("read_file failed: invalid_path"));
    assert!(!out.stdout.unwrap_or_default().contains("outside-secret"));
}

#[test]
fn agent_file_read_range_reads_large_file_subset_under_max_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let mut content = String::new();
    for n in 1..=500 {
        content.push_str(&format!("line-{n:04}\n"));
    }
    let expected_sha256 = sha256_hex_bytes(content.as_bytes());
    std::fs::write(tmp.path().join("large.txt"), content).unwrap();

    let out = file_read_json(handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "large.txt", Some(250), Some(252), Some(1024)),
    ));

    assert_eq!(out["format"], "webcodex.file_read_range.v1");
    assert_eq!(out["content"], "line-0250\nline-0251\nline-0252");
    assert_eq!(out["total_lines"], 500);
    assert_eq!(out["start_line"], 250);
    assert_eq!(out["limit"], 3);
    assert_eq!(out["sha256"], expected_sha256);
}

#[test]
fn agent_file_read_range_beyond_total_lines_returns_empty_content() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("short.txt"), "one\ntwo\nthree\n").unwrap();

    let out = file_read_json(handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "short.txt", Some(10), Some(12), Some(1024)),
    ));

    assert_eq!(out["format"], "webcodex.file_read_range.v1");
    assert_eq!(out["content"], "");
    assert_eq!(out["total_lines"], 3);
    assert_eq!(out["start_line"], 10);
    assert_eq!(out["limit"], 3);
}

#[test]
fn agent_file_read_range_preserves_empty_selected_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("blank.txt"), "\nsecond\nthird\n").unwrap();

    let out = file_read_json(handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "blank.txt", Some(1), Some(2), Some(1024)),
    ));

    assert_eq!(out["format"], "webcodex.file_read_range.v1");
    assert_eq!(out["content"], "\nsecond");
    assert_eq!(out["total_lines"], 3);
    assert_eq!(out["start_line"], 1);
    assert_eq!(out["limit"], 2);
}

#[test]
fn agent_file_read_range_output_obeys_max_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("limited.txt"), "alpha\nbeta\n").unwrap();

    let out = handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "limited.txt", Some(1), Some(1), Some(4)),
    );

    assert!(out.exit_code.is_none(), "unexpected success: {out:?}");
    assert_eq!(
        out.error.as_deref(),
        Some("read_file failed: range_too_large")
    );
    assert!(out.stdout.is_none());
}

#[test]
fn agent_file_read_range_rejects_serialized_envelope_expansion_before_stdout() {
    for (name, byte, len) in [
        ("nul.txt", 0x00, 48 * 1024),
        ("quote.txt", b'\"', 140 * 1024),
        ("backslash.txt", b'\\', 140 * 1024),
        ("control.txt", 0x01, 48 * 1024),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let policy = project_policy(tmp.path());
        std::fs::write(tmp.path().join(name), vec![byte; len]).unwrap();
        let out = handle_file_request(
            &policy,
            &file_read_request(tmp.path(), name, Some(1), Some(1), Some(512 * 1024)),
        );
        assert!(
            out.exit_code.is_none(),
            "unexpected success for {name}: {out:?}"
        );
        assert_eq!(out.error.as_deref(), Some("range output too large"));
        assert!(
            out.stdout.is_none(),
            "oversized envelope reached stdout for {name}"
        );
    }
}

#[test]
fn agent_file_read_precheck_distinguishes_missing_and_non_file() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::create_dir(tmp.path().join("directory")).unwrap();

    let missing = handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "missing.txt", Some(1), Some(1), Some(1024)),
    );
    assert_eq!(
        missing.error.as_deref(),
        Some("read_file failed: not_found")
    );
    assert!(missing.stdout.is_none());

    let directory = handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "directory", Some(1), Some(1), Some(1024)),
    );
    assert_eq!(
        directory.error.as_deref(),
        Some("read_file failed: not_file")
    );
    assert!(directory.stdout.is_none());
}

#[test]
fn agent_file_read_range_large_file_small_range_uses_shared_core() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let line = "x".repeat(64);
    let body = (0..300_000)
        .map(|_| line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let expected_sha = sha256_hex_bytes(body.as_bytes());
    std::fs::write(tmp.path().join("big.txt"), body).unwrap();

    let out = file_read_json(handle_file_request(
        &policy,
        &file_read_request(
            tmp.path(),
            "big.txt",
            Some(150_000),
            Some(150_002),
            Some(512 * 1024),
        ),
    ));
    assert_eq!(out["format"], "webcodex.file_read_range.v1");
    assert_eq!(
        out["content"],
        [line.as_str(), line.as_str(), line.as_str()].join("\n")
    );
    assert_eq!(out["total_lines"], 300_000);
    assert_eq!(out["start_line"], 150_000);
    assert_eq!(out["limit"], 3);
    assert_eq!(out["sha256"], expected_sha);
}

#[test]
fn agent_file_read_range_errors_never_include_absolute_path() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());

    // Missing target: the error must be a stable message, not the resolved
    // absolute path (`resolved.display()`).
    let missing = handle_file_request(
        &policy,
        &file_read_request(tmp.path(), "absent.txt", Some(1), Some(1), Some(128)),
    );
    assert!(missing.exit_code.is_none());
    assert_eq!(
        missing.error.as_deref(),
        Some("read_file failed: not_found")
    );
    let err = missing.error.expect("error");
    assert!(
        !err.contains(tmp.path().to_string_lossy().as_ref()),
        "error leaked absolute path: {err}"
    );
}

fn apply_text_edits_request(
    cwd: &Path,
    path: &str,
    mut payload: serde_json::Value,
) -> ShellAgentShellRequest {
    if payload.get("changes").is_none() {
        let expected_sha256 = payload
            .get("expected_file_sha256")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                sha256_hex_bytes(&std::fs::read(cwd.join(path)).unwrap_or_default())
            });
        payload = serde_json::json!({
            "dry_run": payload.get("dry_run").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "changes": [{
                "kind": "edit",
                "path": path,
                "expected_sha256": expected_sha256,
                "edits": payload.get("edits").cloned().unwrap_or_else(|| serde_json::json!([]))
            }]
        });
    }
    ShellAgentShellRequest {
        request_id: "req-apply-text-edits".to_string(),
        client_id: "agent-1".to_string(),
        kind: "file_apply_text_edits".to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: Some(payload.to_string()),
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
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    }
}

fn json_file_op_request(
    cwd: &Path,
    kind: &str,
    path: &str,
    payload: serde_json::Value,
) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: format!("req-{kind}"),
        client_id: "agent-1".to_string(),
        kind: kind.to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: Some(payload.to_string()),
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
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    }
}

#[test]
fn structured_delete_project_files_is_os_neutral_file_only_and_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("delete-me.txt"), "content").unwrap();

    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": ["delete-me.txt", "missing.txt"]}),
    );
    assert!(request.command.is_empty());
    let result = handle_file_request(&policy, &request);
    assert_eq!(result.exit_code, Some(0), "{:?}", result.error);
    assert!(!tmp.path().join("delete-me.txt").exists());
    let output: serde_json::Value =
        serde_json::from_str(result.stdout.as_deref().expect("structured JSON result")).unwrap();
    assert_eq!(
        output["deleted_paths"],
        serde_json::json!(["delete-me.txt", "missing.txt"])
    );
    assert_eq!(output["missing_paths"], serde_json::json!([]));
    assert_eq!(output["refused_paths"], serde_json::json!([]));

    std::fs::create_dir(tmp.path().join("directory-target")).unwrap();
    let directory = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": ["directory-target"]}),
    );
    let result = handle_file_request(&policy, &directory);
    assert_eq!(
        result.error.as_deref(),
        Some("delete_project_files refuses directory targets")
    );
    assert!(tmp.path().join("directory-target").is_dir());

    for path in [".", "../escape", ".env", "target/cache"] {
        let refused = json_file_op_request(
            tmp.path(),
            "file_delete_project_files",
            ".",
            serde_json::json!({"paths": [path]}),
        );
        let result = handle_file_request(&policy, &refused);
        assert_eq!(
            result.error.as_deref(),
            Some("delete_project_files request contains a refused path"),
            "{path}"
        );
    }

    let too_many = (0..65)
        .map(|index| format!("file-{index}.txt"))
        .collect::<Vec<_>>();
    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": too_many}),
    );
    let result = handle_file_request(&policy, &request);
    assert_eq!(
        result.error.as_deref(),
        Some("delete_project_files request contains a refused path")
    );
}

#[test]
fn structured_delete_project_files_errors_do_not_leak_absolute_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let absolute = tmp.path().join("secret.txt").to_string_lossy().to_string();
    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": [absolute]}),
    );
    let result = handle_file_request(&policy, &request);
    let error = result.error.expect("absolute path must be rejected");
    assert_eq!(
        error,
        "delete_project_files request contains a refused path"
    );
    assert!(!error.contains(&tmp.path().to_string_lossy().to_string()));
}

fn fake_zip_eocd_with_entries(entries: u16) -> Vec<u8> {
    let mut bytes = b"PK\x05\x06".to_vec();
    bytes.extend_from_slice(&[0, 0]); // disk number
    bytes.extend_from_slice(&[0, 0]); // central directory disk
    bytes.extend_from_slice(&entries.to_le_bytes());
    bytes.extend_from_slice(&entries.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]); // central directory size
    bytes.extend_from_slice(&[0, 0, 0, 0]); // central directory offset
    bytes.extend_from_slice(&[0, 0]); // comment length
    bytes
}

fn append_fake_zip_entry(
    bytes: &mut Vec<u8>,
    central_directory: &mut Vec<u8>,
    name: &str,
    content: &[u8],
    deflate: bool,
) {
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write as _;

    let compression_method = if deflate { 8_u16 } else { 0_u16 };
    let compressed = if deflate {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    } else {
        content.to_vec()
    };
    let local_offset = u32::try_from(bytes.len()).unwrap();
    let compressed_size = u32::try_from(compressed.len()).unwrap();
    let uncompressed_size = u32::try_from(content.len()).unwrap();
    let name_len = u16::try_from(name.len()).unwrap();

    bytes.extend_from_slice(b"PK\x03\x04");
    bytes.extend_from_slice(&20_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&compression_method.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&compressed_size.to_le_bytes());
    bytes.extend_from_slice(&uncompressed_size.to_le_bytes());
    bytes.extend_from_slice(&name_len.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&compressed);

    central_directory.extend_from_slice(b"PK\x01\x02");
    central_directory.extend_from_slice(&20_u16.to_le_bytes());
    central_directory.extend_from_slice(&20_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&compression_method.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u32.to_le_bytes());
    central_directory.extend_from_slice(&compressed_size.to_le_bytes());
    central_directory.extend_from_slice(&uncompressed_size.to_le_bytes());
    central_directory.extend_from_slice(&name_len.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u32.to_le_bytes());
    central_directory.extend_from_slice(&local_offset.to_le_bytes());
    central_directory.extend_from_slice(name.as_bytes());
}

fn fake_ooxml_zip(
    main_part: &str,
    main_content_type: &str,
    malformed_content_types: bool,
) -> Vec<u8> {
    let content_types = if malformed_content_types {
        b"<Types".to_vec()
    } else {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/{main_part}" ContentType="{main_content_type}"/></Types>"#
        )
        .into_bytes()
    };
    let mut bytes = Vec::new();
    let mut central_directory = Vec::new();
    append_fake_zip_entry(
        &mut bytes,
        &mut central_directory,
        "[Content_Types].xml",
        &content_types,
        true,
    );
    append_fake_zip_entry(
        &mut bytes,
        &mut central_directory,
        main_part,
        b"<root/>",
        false,
    );
    let central_offset = u32::try_from(bytes.len()).unwrap();
    let central_size = u32::try_from(central_directory.len()).unwrap();
    bytes.extend_from_slice(&central_directory);
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&central_size.to_le_bytes());
    bytes.extend_from_slice(&central_offset.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}

fn fake_zip_central_entry_offset(bytes: &[u8], expected_name: &str) -> usize {
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap();
    let entry_count = u16::from_le_bytes(bytes[eocd + 10..eocd + 12].try_into().unwrap());
    let mut cursor = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    for _ in 0..entry_count {
        assert_eq!(&bytes[cursor..cursor + 4], b"PK\x01\x02");
        let name_len =
            u16::from_le_bytes(bytes[cursor + 28..cursor + 30].try_into().unwrap()) as usize;
        let extra_len =
            u16::from_le_bytes(bytes[cursor + 30..cursor + 32].try_into().unwrap()) as usize;
        let comment_len =
            u16::from_le_bytes(bytes[cursor + 32..cursor + 34].try_into().unwrap()) as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        if &bytes[name_start..name_end] == expected_name.as_bytes() {
            return cursor;
        }
        cursor = name_end + extra_len + comment_len;
    }
    panic!("missing fake ZIP central entry {expected_name}");
}

fn artifact_upload_temp_paths(
    root: &Path,
    artifact_path: &str,
    upload_id: &str,
) -> (PathBuf, PathBuf) {
    let target = root.join(artifact_path);
    let parent = target.parent().expect("artifact path parent");
    (
        parent.join(format!(".wc-upload-{upload_id}.part")),
        parent.join(format!(".wc-upload-{upload_id}.json")),
    )
}

fn directory_contains_name_prefix(dir: &Path, prefix: &str) -> bool {
    if !dir.exists() {
        return false;
    }
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| name.starts_with(prefix))
}

fn assert_upload_temp_files_exist(root: &Path, artifact_path: &str, upload_id: &str) {
    let (part, sidecar) = artifact_upload_temp_paths(root, artifact_path, upload_id);
    assert!(
        part.exists(),
        "missing upload part file: {}",
        part.display()
    );
    assert!(
        sidecar.exists(),
        "missing upload sidecar file: {}",
        sidecar.display()
    );
    let parent = part.parent().unwrap();
    assert!(
        !directory_contains_name_prefix(parent, ".pd-upload-"),
        "legacy .pd upload temp files must not be created in {}",
        parent.display()
    );
}

fn assert_no_upload_temp_files(root: &Path, artifact_path: &str) {
    let target = root.join(artifact_path);
    let Some(parent) = target.parent() else {
        return;
    };
    assert!(
        !directory_contains_name_prefix(parent, ".wc-upload-"),
        "upload temp files remained in {}",
        parent.display()
    );
    assert!(
        !directory_contains_name_prefix(parent, ".pd-upload-"),
        "legacy .pd upload temp files remained in {}",
        parent.display()
    );
}

#[test]
fn file_save_project_artifact_writes_binary_and_blocks_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/tiny.png";
    let content_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        [0x89, b'P', b'N', b'G'],
    );
    let payload = serde_json::json!({
        "path": path,
        "content_base64": content_base64,
        "mime_type": "image/png",
        "overwrite": false,
        "max_bytes": 1024,
    });

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_save_project_artifact",
            path,
            payload.clone(),
        ),
    ));

    assert_eq!(out["path"], path);
    assert_eq!(out["bytes_written"], 4);
    assert_eq!(out["mime_type"], "image/png");
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        std::fs::read(tmp.path().join(path)).unwrap(),
        vec![0x89, b'P', b'N', b'G']
    );
    let parent = tmp.path().join("artifacts/imports");
    assert!(
        !directory_contains_name_prefix(&parent, ".wc-artifact-"),
        "atomic artifact temp file should not remain"
    );
    assert!(
        !directory_contains_name_prefix(&parent, ".pd-artifact-"),
        "legacy .pd artifact temp file should not remain"
    );

    let out2 = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(tmp.path(), "file_save_project_artifact", path, payload),
    ));
    assert!(out2["error"]
        .as_str()
        .unwrap()
        .contains("overwrite is false"));
}

#[test]
fn file_save_and_upload_begin_accept_office_mimes() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let cases = [
        (
            "artifacts/imports/report.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            "artifacts/imports/deck.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        (
            "artifacts/imports/book.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    ];

    for (path, mime) in cases {
        let content_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"office-artifact",
        );
        let saved = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_save_project_artifact",
                path,
                serde_json::json!({
                    "path": path,
                    "content_base64": content_base64,
                    "mime_type": mime,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(saved["mime_type"], mime, "{path}");

        let upload_path = path.replacen("artifacts/imports/", "artifacts/uploads/", 1);
        let upload = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                &upload_path,
                serde_json::json!({
                    "path": upload_path,
                    "expected_bytes": 0,
                    "expected_sha256": null,
                    "mime_type": mime,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(upload["mime_type"], mime, "{path}");
        let upload_id = upload["upload_id"].as_str().unwrap();
        assert_upload_temp_files_exist(tmp.path(), &upload_path, upload_id);
    }
}

#[test]
fn file_read_project_artifact_metadata_counts_zip_without_extracting() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let zip_path = tmp.path().join("sample.zip");
    std::fs::write(&zip_path, fake_zip_eocd_with_entries(2)).unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "sample.zip",
            serde_json::json!({"path": "sample.zip", "max_bytes": 1024}),
        ),
    ));

    assert_eq!(out["mime_type"], "application/zip");
    assert_eq!(out["archive_entries_count"], 2);
    assert!(
        out["modified_at"].as_u64().unwrap() > 0,
        "modified_at should be unix timestamp seconds"
    );
    assert!(!tmp.path().join("a.txt").exists());
    assert!(!tmp.path().join("b.txt").exists());
}

#[test]
fn file_read_project_artifact_detects_ooxml_mime_from_package_content() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let cases = [
        (
            "sample.docx",
            "word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            "sample.pptx",
            "ppt/presentation.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        (
            "sample.xlsx",
            "xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    ];

    for (path, main_part, main_content_type, expected_mime) in cases {
        let bytes = fake_ooxml_zip(main_part, main_content_type, false);
        std::fs::write(tmp.path().join(path), &bytes).unwrap();
        let metadata = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact_metadata",
                path,
                serde_json::json!({"path": path, "max_bytes": 64 * 1024}),
            ),
        ));
        assert_eq!(metadata["mime_type"], expected_mime, "{path}");
        assert!(metadata.get("archive_entries_count").is_none(), "{path}");

        let read = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_read_project_artifact",
                path,
                serde_json::json!({
                    "path": path,
                    "offset": 0,
                    "length": 16,
                    "max_file_bytes": 64 * 1024,
                }),
            ),
        ));
        assert_eq!(read["mime_type"], expected_mime, "{path}");
        assert_eq!(read["file_bytes"], bytes.len(), "{path}");
        assert_eq!(read["bytes_returned"], 16, "{path}");
    }
}

#[test]
fn file_read_project_artifact_does_not_trust_ooxml_extension_or_malformed_package() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());

    std::fs::write(tmp.path().join("spoof.docx"), fake_zip_eocd_with_entries(0)).unwrap();
    let spoof = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "spoof.docx",
            serde_json::json!({"path": "spoof.docx", "max_bytes": 1024}),
        ),
    ));
    assert_eq!(spoof["mime_type"], "application/zip");

    std::fs::write(tmp.path().join("not-a-zip.docx"), b"plain bytes").unwrap();
    let non_zip = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "not-a-zip.docx",
            serde_json::json!({"path": "not-a-zip.docx", "max_bytes": 1024}),
        ),
    ));
    assert!(non_zip["mime_type"].is_null());

    let malformed = fake_ooxml_zip(
        "word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        true,
    );
    std::fs::write(tmp.path().join("broken.docx"), malformed).unwrap();
    let broken = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "broken.docx",
            serde_json::json!({"path": "broken.docx", "max_bytes": 64 * 1024}),
        ),
    ));
    assert_eq!(broken["mime_type"], "application/zip");
}

#[test]
fn file_read_project_artifact_rejects_ooxml_main_part_with_invalid_local_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let main_part = "word/document.xml";
    let main_content_type =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

    let mut invalid_offset = fake_ooxml_zip(main_part, main_content_type, false);
    let central_entry = fake_zip_central_entry_offset(&invalid_offset, main_part);
    invalid_offset[central_entry + 42..central_entry + 46].copy_from_slice(&1_u32.to_le_bytes());
    std::fs::write(tmp.path().join("bad-offset.docx"), invalid_offset).unwrap();
    let bad_offset = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "bad-offset.docx",
            serde_json::json!({"path": "bad-offset.docx", "max_bytes": 64 * 1024}),
        ),
    ));
    assert_eq!(bad_offset["mime_type"], "application/zip");

    let mut mismatched_name = fake_ooxml_zip(main_part, main_content_type, false);
    let central_entry = fake_zip_central_entry_offset(&mismatched_name, main_part);
    let local_offset = u32::from_le_bytes(
        mismatched_name[central_entry + 42..central_entry + 46]
            .try_into()
            .unwrap(),
    ) as usize;
    let local_name_start = local_offset + 30;
    assert_eq!(
        &mismatched_name[local_name_start..local_name_start + main_part.len()],
        main_part.as_bytes()
    );
    mismatched_name[local_name_start] = b'v';
    std::fs::write(tmp.path().join("bad-local-name.docx"), mismatched_name).unwrap();
    let bad_name = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            "bad-local-name.docx",
            serde_json::json!({"path": "bad-local-name.docx", "max_bytes": 64 * 1024}),
        ),
    ));
    assert_eq!(bad_name["mime_type"], "application/zip");
}

#[test]
fn file_read_project_artifact_reads_binary_chunks() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let bytes = [0, 159, 146, 150, b'a', b'b', b'c', b'd'];
    std::fs::write(tmp.path().join("data.bin"), bytes).unwrap();

    let first = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact",
            "data.bin",
            serde_json::json!({"path": "data.bin", "offset": 0, "length": 4, "max_file_bytes": 1024}),
        ),
    ));
    assert_eq!(first["file_bytes"], bytes.len());
    assert!(first["sha256"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
    assert!(first.get("mime_type").is_some());
    assert_eq!(first["offset"], 0);
    assert_eq!(first["bytes_returned"], 4);
    assert_eq!(first["next_offset"], 4);
    assert_eq!(first["truncated"], true);
    assert_eq!(first["eof"], false);
    assert_eq!(
        first["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..4])
    );

    let second = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact",
            "data.bin",
            serde_json::json!({"path": "data.bin", "offset": 4, "length": 20, "max_file_bytes": 1024}),
        ),
    ));
    assert_eq!(second["sha256"], first["sha256"]);
    assert_eq!(second["offset"], 4);
    assert_eq!(second["bytes_returned"], bytes.len() - 4);
    assert_eq!(second["next_offset"], bytes.len());
    assert_eq!(second["truncated"], false);
    assert_eq!(second["eof"], true);
    assert_eq!(
        second["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[4..])
    );

    let at_eof = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact",
            "data.bin",
            serde_json::json!({"path": "data.bin", "offset": bytes.len(), "length": 4, "max_file_bytes": 1024}),
        ),
    ));
    assert_eq!(at_eof["bytes_returned"], 0);
    assert_eq!(at_eof["next_offset"], bytes.len());
    assert_eq!(at_eof["truncated"], false);
    assert_eq!(at_eof["eof"], true);
}

#[test]
fn file_read_project_artifact_export_chunk_reads_only_requested_segments() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let bytes = vec![0x5a; 70 * 1024];
    std::fs::write(tmp.path().join("export.bin"), &bytes).unwrap();

    let first = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "export.bin",
            serde_json::json!({
                "path": "export.bin",
                "expected_file_bytes": bytes.len(),
                "offset": 0,
                "length": 64 * 1024
            }),
        ),
    ));
    assert_eq!(first["file_bytes"], bytes.len());
    assert_eq!(first["offset"], 0);
    assert_eq!(first["bytes_returned"], 64 * 1024);
    assert_eq!(first["next_offset"], 64 * 1024);
    assert_eq!(first["truncated"], true);
    assert_eq!(first["eof"], false);
    assert!(first.get("sha256").is_none());
    assert!(first.get("mime_type").is_none());
    let first_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        first["content_base64"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(first_bytes, bytes[..64 * 1024]);

    let final_chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "export.bin",
            serde_json::json!({
                "path": "export.bin",
                "expected_file_bytes": bytes.len(),
                "offset": 64 * 1024,
                "length": 64 * 1024
            }),
        ),
    ));
    assert_eq!(final_chunk["offset"], 64 * 1024);
    assert_eq!(final_chunk["bytes_returned"], 6 * 1024);
    assert_eq!(final_chunk["next_offset"], bytes.len());
    assert_eq!(final_chunk["truncated"], false);
    assert_eq!(final_chunk["eof"], true);
    let final_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        final_chunk["content_base64"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(final_bytes, bytes[64 * 1024..]);

    let wrong_size = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "export.bin",
            serde_json::json!({
                "path": "export.bin",
                "expected_file_bytes": bytes.len() - 1,
                "offset": 0,
                "length": 1
            }),
        ),
    ));
    assert_eq!(wrong_size["error_kind"], "snapshot_changed");

    let ten_mib = 10 * 1024 * 1024;
    let boundary = std::fs::File::create(tmp.path().join("boundary.bin")).unwrap();
    boundary.set_len(ten_mib as u64).unwrap();
    let boundary_read = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "boundary.bin",
            serde_json::json!({
                "path": "boundary.bin",
                "expected_file_bytes": ten_mib,
                "offset": ten_mib - 1,
                "length": 64 * 1024
            }),
        ),
    ));
    assert_eq!(boundary_read["bytes_returned"], 1);
    assert_eq!(boundary_read["next_offset"], ten_mib);
    assert_eq!(boundary_read["eof"], true);

    let above_whole_payload =
        std::fs::File::create(tmp.path().join("above-whole-payload.bin")).unwrap();
    above_whole_payload.set_len((ten_mib + 1) as u64).unwrap();
    let above_whole_payload_read = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "above-whole-payload.bin",
            serde_json::json!({
                "path": "above-whole-payload.bin",
                "expected_file_bytes": ten_mib + 1,
                "offset": ten_mib,
                "length": 1
            }),
        ),
    ));
    assert_eq!(above_whole_payload_read["bytes_returned"], 1);
    assert_eq!(above_whole_payload_read["next_offset"], ten_mib + 1);
    assert_eq!(above_whole_payload_read["eof"], true);

    let export_max = 256 * 1024 * 1024;
    let max_file = std::fs::File::create(tmp.path().join("export-max.bin")).unwrap();
    max_file.set_len(export_max as u64).unwrap();
    let max_read = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "export-max.bin",
            serde_json::json!({
                "path": "export-max.bin",
                "expected_file_bytes": export_max,
                "offset": export_max - 1,
                "length": 1
            }),
        ),
    ));
    assert_eq!(max_read["bytes_returned"], 1);
    assert_eq!(max_read["next_offset"], export_max);
    assert_eq!(max_read["eof"], true);

    let too_large = std::fs::File::create(tmp.path().join("export-too-large.bin")).unwrap();
    too_large.set_len((export_max + 1) as u64).unwrap();
    let rejected = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_export_chunk",
            "export-too-large.bin",
            serde_json::json!({
                "path": "export-too-large.bin",
                "expected_file_bytes": export_max + 1,
                "offset": 0,
                "length": 1
            }),
        ),
    ));
    assert!(rejected["error"].as_str().unwrap().contains("maximum"));
}

#[test]
fn file_read_project_artifact_metadata_streams_above_whole_payload_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let ten_mib = 10 * 1024 * 1024;
    let export_max = 256 * 1024 * 1024;
    let path = "large-export.pdf";
    let file = std::fs::File::create(tmp.path().join(path)).unwrap();
    file.set_len((ten_mib + 1) as u64).unwrap();

    let metadata = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            serde_json::json!({
                "path": path,
                "max_bytes": export_max,
                "allow_missing": false
            }),
        ),
    ));
    assert!(metadata.get("error").is_none(), "metadata: {metadata:?}");
    assert_eq!(metadata["bytes"], ten_mib + 1);
    assert_eq!(metadata["mime_type"], "application/pdf");
    assert_eq!(metadata["sha256"].as_str().unwrap().len(), 64);

    let whole_payload_bound = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            serde_json::json!({
                "path": path,
                "max_bytes": ten_mib,
                "allow_missing": false
            }),
        ),
    ));
    assert!(whole_payload_bound["error"]
        .as_str()
        .unwrap()
        .contains("too large"));

    let invalid_max = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            serde_json::json!({
                "path": path,
                "max_bytes": export_max + 1,
                "allow_missing": false
            }),
        ),
    ));
    assert!(invalid_max["error"].as_str().unwrap().contains("maximum"));
}

#[test]
fn file_artifact_upload_chunks_finish_and_abort() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/upload.bin";
    let bytes = b"abcdefgh";
    let expected_sha256 = sha256_hex_bytes(bytes);

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": bytes.len(),
                "expected_sha256": expected_sha256,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    assert!(upload_id.starts_with("wc_upload_"));
    assert_eq!(begin["received_bytes"], 0);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..4]);
    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": first,
                "max_chunk_bytes": 4,
            }),
        ),
    ));
    assert_eq!(out["received_bytes"], 4);
    assert_eq!(out["next_offset"], 4);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let second = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[4..]);
    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 4,
                "content_base64": second,
                "max_chunk_bytes": 4,
            }),
        ),
    ));
    assert_eq!(out["received_bytes"], bytes.len());

    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
            }),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(finish["bytes"], bytes.len());
    assert_eq!(finish["sha256"], sha256_hex_bytes(bytes));
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), bytes);
    assert_no_upload_temp_files(tmp.path(), path);

    let abort_path = "artifacts/imports/abort.bin";
    let begin_abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            abort_path,
            serde_json::json!({
                "path": abort_path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let abort_upload_id = begin_abort["upload_id"].as_str().unwrap();
    assert_upload_temp_files_exist(tmp.path(), abort_path, abort_upload_id);
    let abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            abort_path,
            serde_json::json!({
                "path": abort_path,
                "upload_id": abort_upload_id,
            }),
        ),
    ));
    assert_eq!(abort["aborted"], true);
    assert!(!tmp.path().join(abort_path).exists());
    assert_no_upload_temp_files(tmp.path(), abort_path);
}

#[test]
fn file_artifact_upload_finish_detects_ooxml_mime_from_file() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/streamed.docx";
    let bytes = fake_ooxml_zip(
        "word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        false,
    );
    let expected_sha256 = sha256_hex_bytes(&bytes);

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": bytes.len(),
                "expected_sha256": expected_sha256,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": bytes.len(),
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let content_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    let chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": content_base64,
                "max_chunk_bytes": bytes.len(),
            }),
        ),
    ));
    assert_eq!(chunk["received_bytes"], bytes.len());

    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id}),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(finish["bytes"], bytes.len());
    assert_eq!(finish["sha256"], sha256_hex_bytes(&bytes));
    assert_eq!(
        finish["mime_type"],
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), bytes);
    assert_no_upload_temp_files(tmp.path(), path);
}

#[test]
fn file_artifact_upload_begin_rejects_validation_and_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());

    let sensitive = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            ".env",
            serde_json::json!({
                "path": ".env",
                "expected_bytes": 1,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert!(sensitive["error"]
        .as_str()
        .unwrap()
        .contains("sensitive artifact path"));

    let bad_hash_path = "artifacts/imports/bad-hash.txt";
    let bad_hash = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            bad_hash_path,
            serde_json::json!({
                "path": bad_hash_path,
                "expected_bytes": 1,
                "expected_sha256": "not-a-sha",
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert!(bad_hash["error"]
        .as_str()
        .unwrap()
        .contains("expected_sha256 must be"));

    let too_large_path = "artifacts/imports/too-large.txt";
    let too_large = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            too_large_path,
            serde_json::json!({
                "path": too_large_path,
                "expected_bytes": 5,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 4,
            }),
        ),
    ));
    assert_eq!(too_large["error"], "expected_bytes exceeds max_bytes");

    let unsafe_octet_path = "artifacts/imports/raw.bin";
    let unsafe_octet = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            unsafe_octet_path,
            serde_json::json!({
                "path": unsafe_octet_path,
                "expected_bytes": 1,
                "expected_sha256": null,
                "mime_type": "application/octet-stream",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let unsafe_octet_error = unsafe_octet["error"].as_str().unwrap();
    assert!(unsafe_octet_error.contains(".artifact"));
    assert!(unsafe_octet_error.contains(".txt"));
    assert!(unsafe_octet_error.contains("artifacts/smoke/<name>.artifact"));
    assert_eq!(unsafe_octet["failure_kind"], "policy_rejected");

    let existing_path = "artifacts/imports/existing.txt";
    std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
    std::fs::write(tmp.path().join(existing_path), b"old").unwrap();
    let existing = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            existing_path,
            serde_json::json!({
                "path": existing_path,
                "expected_bytes": 3,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert_eq!(existing["error"], "file exists and overwrite is false");
    assert_eq!(
        std::fs::read(tmp.path().join(existing_path)).unwrap(),
        b"old"
    );

    #[cfg(unix)]
    {
        let symlink_path = "artifacts/imports/link.txt";
        let victim = tmp.path().join("victim.txt");
        std::fs::write(&victim, b"victim").unwrap();
        std::os::unix::fs::symlink(&victim, tmp.path().join(symlink_path)).unwrap();
        let symlink = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                symlink_path,
                serde_json::json!({
                    "path": symlink_path,
                    "expected_bytes": 3,
                    "expected_sha256": null,
                    "mime_type": "text/plain",
                    "overwrite": true,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(
            symlink["error"],
            "refusing to overwrite symlink artifact path"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    }
}

#[test]
fn file_artifact_upload_chunk_rejects_validation_and_keeps_final_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/chunk.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024 * 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();

    let invalid_id = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": "bad",
                "offset": 0,
                "content_base64": "YQ==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(invalid_id["error"]
        .as_str()
        .unwrap()
        .contains("upload_id must start"));

    let invalid_base64 = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "not valid base64!",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(invalid_base64["error"]
        .as_str()
        .unwrap()
        .contains("invalid base64"));

    let empty = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(empty["error"]
        .as_str()
        .unwrap()
        .contains("decoded chunk must contain at least 1 byte"));

    let too_large = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![b'x'; 64 * 1024 + 1],
    );
    let too_large = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": too_large,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(too_large["error"], "decoded chunk too large");

    let first = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc"),
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(first["received_bytes"], 3);
    assert!(!tmp.path().join(path).exists());

    let wrong_offset = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(
        wrong_offset["error"],
        "offset does not match received_bytes"
    );
    assert_eq!(wrong_offset["received_bytes"], 3);
    assert_eq!(wrong_offset["next_offset"], 3);

    let other_path = "artifacts/imports/other.bin";
    let mismatch = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            other_path,
            serde_json::json!({
                "path": other_path,
                "upload_id": upload_id.clone(),
                "offset": 3,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(
        mismatch["error"],
        "upload_id does not belong to requested path"
    );
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[test]
fn file_artifact_upload_finish_validation_failures_keep_retry_state() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/retry.bin";

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": 4,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc");
    let chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": first,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(chunk["received_bytes"], 3);

    let failed_finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        failed_finish["error"],
        "uploaded byte count does not match expected_bytes"
    );
    assert_eq!(failed_finish["committed"], false);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let retry_chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 3,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(retry_chunk["received_bytes"], 4);
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"abcd");
    assert_no_upload_temp_files(tmp.path(), path);

    let sha_path = "artifacts/imports/bad-sha.bin";
    let bad_sha = "0000000000000000000000000000000000000000000000000000000000000000";
    let begin_sha = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            sha_path,
            serde_json::json!({
                "path": sha_path,
                "expected_bytes": null,
                "expected_sha256": bad_sha,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let sha_upload_id = begin_sha["upload_id"].as_str().unwrap().to_string();
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abcd");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            sha_path,
            serde_json::json!({
                "path": sha_path,
                "upload_id": sha_upload_id.clone(),
                "offset": 0,
                "content_base64": data,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    let sha_failed = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            sha_path,
            serde_json::json!({"path": sha_path, "upload_id": sha_upload_id.clone()}),
        ),
    ));
    assert_eq!(
        sha_failed["error"],
        "uploaded sha256 does not match expected_sha256"
    );
    assert_eq!(sha_failed["committed"], false);
    assert!(!tmp.path().join(sha_path).exists());
    assert_upload_temp_files_exist(tmp.path(), sha_path, &sha_upload_id);
}

#[test]
fn file_artifact_upload_finish_refuses_late_target_when_overwrite_false() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/race.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    std::fs::write(tmp.path().join(path), b"old").unwrap();
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(finish["error"], "file exists and overwrite is false");
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"old");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[cfg(unix)]
#[test]
fn file_artifact_upload_finish_refuses_late_symlink_even_with_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/symlink-race.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": true,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    let victim = tmp.path().join("victim-race.bin");
    std::fs::write(&victim, b"victim").unwrap();
    std::os::unix::fs::symlink(&victim, tmp.path().join(path)).unwrap();
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        finish["error"],
        "refusing to overwrite symlink artifact path"
    );
    assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[test]
fn file_artifact_upload_abort_rejects_wrong_ids_and_cleans_only_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/abort-target.bin";
    std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
    std::fs::write(tmp.path().join(path), b"final").unwrap();
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": true,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"temp");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));

    let missing = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            serde_json::json!({"path": path, "upload_id": "wc_upload_missing"}),
        ),
    ));
    assert!(missing["error"]
        .as_str()
        .unwrap()
        .contains("upload not found"));
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let other_path = "artifacts/imports/abort-other.bin";
    let mismatch = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            other_path,
            serde_json::json!({"path": other_path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        mismatch["error"],
        "upload_id does not belong to requested path"
    );
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(abort["aborted"], true);
    assert_eq!(abort["received_bytes"], 4);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
    assert_no_upload_temp_files(tmp.path(), path);
}

#[cfg(unix)]
#[test]
fn file_project_artifact_ops_reject_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("outside.bin");
    std::fs::write(&outside, b"outside-secret-content").unwrap();
    std::os::unix::fs::symlink(&outside, root.path().join("leak.bin")).unwrap();
    let mut policy = project_policy(root.path());
    policy.allowed_roots.push(outside_dir.path().to_path_buf());

    let read = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            root.path(),
            "file_read_project_artifact",
            "leak.bin",
            serde_json::json!({"path":"leak.bin","offset":0,"length":8,"max_file_bytes":1024}),
        ),
    ));
    assert_eq!(read["error"], "artifact path escapes project root");
    assert!(!read.to_string().contains("outside-secret-content"));

    let export_chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            root.path(),
            "file_read_project_artifact_export_chunk",
            "leak.bin",
            serde_json::json!({
                "path":"leak.bin",
                "expected_file_bytes":8,
                "offset":0,
                "length":8
            }),
        ),
    ));
    assert_eq!(export_chunk["error"], "artifact path escapes project root");
    assert!(!export_chunk.to_string().contains("outside-secret-content"));

    let metadata = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            root.path(),
            "file_read_project_artifact_metadata",
            "leak.bin",
            serde_json::json!({"path":"leak.bin","max_bytes":1024}),
        ),
    ));
    assert_eq!(metadata["error"], "artifact path escapes project root");
    assert!(!metadata.to_string().contains("outside-secret-content"));

    let save = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            root.path(),
            "file_save_project_artifact",
            "leak.bin",
            serde_json::json!({
                "path":"leak.bin",
                "content_base64":"bmV3",
                "mime_type":"text/plain",
                "overwrite":true,
                "max_bytes":1024
            }),
        ),
    ));
    assert_eq!(save["error"], "refusing to overwrite symlink artifact path");
    assert_eq!(
        std::fs::read(&outside).expect("outside file remains readable"),
        b"outside-secret-content"
    );
    assert!(!save.to_string().contains("outside-secret-content"));
}

#[test]
fn file_write_project_file_creates_parent_dirs_and_reports_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("nested/new.txt");

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "nested/new.txt",
            serde_json::json!({
                "path": "nested/new.txt",
                "content": "line1\nline2\n",
                "overwrite": false,
                "expected_sha256": null,
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], true);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["bytes_written"], 12);
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert!(out["warning"].is_null());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "line1\nline2\n");
}

#[test]
fn file_write_project_file_rejects_existing_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "new",
                "overwrite": false,
                "expected_sha256": null,
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["overwritten"], false);
    assert!(out["error"].as_str().unwrap().contains("overwrite"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn file_write_project_file_rejects_string_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "new",
                "overwrite": "false",
                "expected_sha256": null,
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["error"], "overwrite must be a boolean");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn file_write_project_file_enforces_sha_and_prefix_guards() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();
    let original_sha = sha256_hex_bytes("original".as_bytes());

    let sha_ok = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "v1 replaced",
                "overwrite": true,
                "expected_sha256": original_sha,
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(sha_ok["overwritten"], true);
    assert!(sha_ok["warning"].is_null());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 replaced");

    let prefix_ok = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "v1 final",
                "overwrite": true,
                "expected_sha256": null,
                "expected_content_prefix": "v1 ",
            }),
        ),
    ));
    assert_eq!(prefix_ok["overwritten"], true);
    assert!(prefix_ok["warning"].is_null());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");

    let sha_bad = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "bad",
                "overwrite": true,
                "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(sha_bad["created"], false);
    assert!(sha_bad["error"].as_str().unwrap().contains("sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");
}

#[test]
fn file_write_project_file_warns_on_unguarded_overwrite_and_rejects_bad_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "v2 content").unwrap();

    let prefix_bad = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "bad",
                "overwrite": true,
                "expected_sha256": null,
                "expected_content_prefix": "v1 ",
            }),
        ),
    ));
    assert_eq!(prefix_bad["created"], false);
    assert!(prefix_bad["error"].as_str().unwrap().contains("prefix"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 content");

    let unguarded = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "unguarded",
                "overwrite": true,
                "expected_sha256": null,
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(unguarded["overwritten"], true);
    assert!(unguarded["warning"]
        .as_str()
        .unwrap()
        .contains("expected_sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unguarded");

    let nul = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "new.txt",
            serde_json::json!({
                "content": "a\u{0000}b",
                "overwrite": false,
                "expected_sha256": null,
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(nul["created"], false);
    assert!(nul["error"].as_str().unwrap().contains("NUL"));
    assert!(!tmp.path().join("new.txt").exists());
}

#[test]
fn file_apply_text_edits_applies_multi_file_transaction() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
    std::fs::write(tmp.path().join("c.txt"), "gamma\n").unwrap();
    let hash = |path: &str| sha256_hex_bytes(&std::fs::read(tmp.path().join(path)).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "a.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "a.txt",
                        "expected_sha256": hash("a.txt"),
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {"kind": "create", "path": "nested/new.txt", "content": "new\n"},
                    {"kind": "delete", "path": "b.txt", "expected_sha256": hash("b.txt")},
                    {"kind": "rename", "path": "c.txt", "to_path": "moved/c.txt", "expected_sha256": hash("c.txt")}
                ]
            }),
        ),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(out["applied_count"], 4);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
        "ALPHA\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("nested/new.txt")).unwrap(),
        "new\n"
    );
    assert!(!tmp.path().join("b.txt").exists());
    assert!(!tmp.path().join("c.txt").exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("moved/c.txt")).unwrap(),
        "gamma\n"
    );
    assert_eq!(out["files"].as_array().unwrap().len(), 4);
}

#[test]
fn file_apply_text_edits_hash_conflict_keeps_every_file_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
    let a_hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("a.txt")).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "a.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "a.txt",
                        "expected_sha256": a_hash,
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {
                        "kind": "delete",
                        "path": "b.txt",
                        "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    }
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "sha256_conflict");
    assert_eq!(out["change_index"], 1);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("b.txt")).unwrap(),
        "beta\n"
    );
}

#[test]
fn file_apply_text_edits_rejects_resolved_path_aliases() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/a.txt"), "alpha\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("src/a.txt")).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "src/a.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "src/a.txt",
                        "expected_sha256": hash,
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {
                        "kind": "delete",
                        "path": "src//a.txt",
                        "expected_sha256": hash
                    }
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "path_overlap");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src/a.txt")).unwrap(),
        "alpha\n"
    );
}

#[test]
fn file_apply_text_edits_replace_exact_writes_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
}

#[test]
fn file_apply_text_edits_dry_run_does_not_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "dry_run": true,
                "edits": [
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["changed"], false);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
}

#[test]
fn file_apply_text_edits_rejects_missing_match_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "missing", "new_text": "x"}
                ]
            }),
        ),
    ));
    let msg = out["error"].as_str().unwrap();
    assert!(msg.contains("match text was not found"));
    assert!(msg.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
}

#[test]
fn file_apply_text_edits_rejects_ambiguous_match_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup-dup\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "dup", "new_text": "x"}
                ]
            }),
        ),
    ));
    let msg = out["error"].as_str().unwrap();
    assert!(msg.contains("matched 2 times"));
    assert!(msg.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup-dup\n");
}

#[test]
fn file_apply_text_edits_expected_file_sha256_mismatch_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "expected_file_sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdead",
                "edits": [
                    {"kind": "replace_exact", "old_text": "alpha", "new_text": "beta"}
                ]
            }),
        ),
    ));
    let err = out["error"].as_str().unwrap();
    assert!(err.contains("expected_sha256 does not match"));
    assert!(err.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
}

#[test]
fn file_apply_text_edits_insert_before_after_and_delete_exact() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "insert_after", "anchor_text": "alpha\n", "new_text": "ALPHA-AFTER\n"},
                    {"kind": "delete_exact", "old_text": "beta\n"},
                    {"kind": "insert_before", "anchor_text": "gamma\n", "new_text": "GAMMA-BEFORE\n"}
                ]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["applied_count"], 1);
    assert_eq!(out["files"][0]["edits"].as_array().unwrap().len(), 3);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "alpha\nALPHA-AFTER\nGAMMA-BEFORE\ngamma\n"
    );
}

#[test]
fn file_apply_text_edits_rejects_overlapping_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "abcdef\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "abc", "new_text": "ABC"},
                    {"kind": "replace_exact", "old_text": "cde", "new_text": "CDE"}
                ]
            }),
        ),
    ));
    let err = out["error"].as_str().unwrap();
    assert!(err.contains("edits overlap"));
    assert!(err.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "abcdef\n");
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

#[cfg(windows)]
#[test]
fn shell_job_filters_sensitive_env_case_insensitive() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    // The plain (non-profile) path removes sensitive keys from the child
    // environment; Windows removal must be case-insensitive like the OS.
    for spelling in [
        "WEBCODEX_TOKEN",
        "WebCodex_User_Token",
        "Authorization",
        "webcodex_agent_token",
    ] {
        let _env = EnvGuard::new().set(spelling, "secret-token");
        let result = run_shell(
            &cfg.policy,
            &ShellConfig::default(),
            Some(&cwd),
            &shell_if_else_env_present(spelling),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("absent"), "{result:?}");
    }

    // A configured shell env must not be able to re-insert a secret after the
    // inherited environment was scrubbed. Exercise canonical and mixed-case
    // spellings because Windows environment names are case-insensitive.
    for spelling in ["WEBCODEX_TOKEN", "WebCodex_User_Token", "authorization"] {
        let shell = ShellConfig {
            env: HashMap::from([(spelling.to_string(), "configured-secret".to_string())]),
            ..ShellConfig::default()
        };
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            &shell_if_else_env_present(spelling),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(
            result.stdout.as_deref(),
            Some("absent"),
            "configured sensitive env leaked: {result:?}"
        );
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

#[test]
fn shell_job_success_and_failure_results_are_structured() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let success = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &format!("{}; {}", shell_echo("hello"), shell_echo_err("warn")),
        None,
        10,
        None,
    );
    assert_eq!(success.exit_code, Some(0));
    assert_eq!(success.stdout.as_deref(), Some("hello"));
    assert_eq!(success.stderr.as_deref(), Some("warn"));
    assert!(success.error.is_none());

    let failure = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "exit 7",
        None,
        10,
        None,
    );
    assert_eq!(failure.exit_code, Some(7));
    assert!(failure.error.is_none());
}

#[test]
fn shell_job_writes_stdin_to_child() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &shell_stdin_cat(),
        Some("stdin payload\n"),
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("stdin payload\n"));
    assert!(result.error.is_none());
}

#[cfg(unix)]
#[test]
fn shell_job_preserves_result_when_child_closes_stdin_early() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    // Larger than a pipe buffer, so write_all observes the closed reader
    // instead of winning the race by buffering the whole payload.
    let input = "unused payload\n".repeat(128 * 1024);

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "exec 0<&-; printf capability-unavailable; exit 23",
        Some(&input),
        10,
        None,
    );

    assert_eq!(result.exit_code, Some(23), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("capability-unavailable"));
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(unix)]
#[test]
fn shell_job_rejects_cwd_symlink_escape() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join("outside")).unwrap();
    let policy = AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![project.path().to_path_buf()],
        ..AgentPolicy::default()
    };

    let result = run_shell(
        &policy,
        &ShellConfig::default(),
        Some(project.path().join("outside").to_string_lossy().as_ref()),
        "pwd",
        None,
        10,
        None,
    );

    assert_eq!(result.exit_code, None);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("outside allowed_roots")));
}

#[test]
fn shell_job_timeout_returns_timeout_error() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "sleep 2",
        None,
        1,
        None,
    );
    assert_eq!(result.exit_code, Some(-1));
    assert_eq!(result.error.as_deref(), Some("command timed out"));
    assert!(result
        .stderr
        .as_deref()
        .unwrap_or_default()
        .contains("command timed out after 1 seconds"));
}

#[cfg(unix)]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn long_lived_descendant_command(pid_file: &Path) -> String {
    format!(
        "sleep 60 & descendant=$!; printf '%s' \"$descendant\" > {}; wait",
        shell_quote_path(pid_file)
    )
}

#[cfg(unix)]
fn wait_until(timeout: Duration, predicate: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

#[cfg(unix)]
fn descendant_is_gone(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        let zombie = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.chars().next())
            .is_some_and(|state| state == 'Z');
        if zombie {
            // A zombie cannot execute and proves the process-group signal
            // terminated the descendant. Minimal container PID 1
            // implementations may leave adopted zombies visible long
            // after the runner has exhausted everything it can reap.
            return true;
        }
    }
    // SAFETY: signal 0 only probes the PID written by this test command;
    // it does not deliver a signal to the process.
    let missing = unsafe { libc::kill(pid as i32, 0) == -1 };
    missing && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(unix)]
struct DescendantCleanup {
    pid: u32,
}

#[cfg(unix)]
impl DescendantCleanup {
    fn disarm(&mut self) {
        self.pid = 0;
    }
}

#[cfg(unix)]
impl Drop for DescendantCleanup {
    fn drop(&mut self) {
        if self.pid != 0 {
            // SAFETY: the PID was created by this test. This is a
            // best-effort failure-path cleanup and never targets a group.
            unsafe {
                libc::kill(self.pid as i32, libc::SIGKILL);
            }
        }
    }
}

#[cfg(unix)]
fn assert_descendant_reaped(pid_file: &Path) {
    assert!(
        wait_until(Duration::from_secs(2), || pid_file.exists()),
        "descendant pid file was not created: {}",
        pid_file.display()
    );
    let pid = std::fs::read_to_string(pid_file)
        .expect("read descendant pid file")
        .trim()
        .parse::<u32>()
        .expect("parse descendant pid");
    let mut cleanup = DescendantCleanup { pid };
    assert!(
        wait_until(Duration::from_secs(5), || descendant_is_gone(pid)),
        "descendant {pid} survived synchronous shell cancellation"
    );
    cleanup.disarm();
}

#[cfg(unix)]
#[test]
fn shell_job_timeout_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("timeout-descendant.pid");

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        1,
        None,
    );

    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("command timed out after 1 seconds"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[cfg(unix)]
#[test]
fn shell_job_timeout_profile_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(Some("test"), vec![("test", ShellProfileConfig::default())]);
    let policy = unrestricted_test_policy();
    let cache = PreparedShellProfileCache::default();
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("profile-timeout-descendant.pid");

    // Exercise the production request path directly rather than the
    // test-only `run_shell` wrapper.
    let result = run_shell_with_profiles(
        1,
        &policy,
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        1,
        None,
    );

    assert_eq!(cache.len(), 1, "prepared profile path was not used");
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[cfg(windows)]
#[test]
fn shell_job_powershell_statement_error_is_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "Write-Error 'expected failure'",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(1), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(windows)]
#[test]
fn shell_job_powershell_last_success_overrides_stale_native_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "cmd.exe /d /c exit 7; Write-Output 'final-ok'",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(
        result.stdout.as_deref().map(str::trim_end),
        Some("final-ok"),
        "{result:?}"
    );
}

#[test]
fn shell_job_stop_flag_is_best_effort() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let stop_requested = AtomicBool::new(true);

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "sleep 2",
        None,
        10,
        Some(&stop_requested),
    );
    assert_eq!(result.exit_code, Some(-1));
    assert_eq!(result.error.as_deref(), Some("job stopped"));
    assert!(result
        .stderr
        .as_deref()
        .unwrap_or_default()
        .contains("job stopped by request"));
}

#[cfg(unix)]
#[test]
fn shell_job_stop_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("stop-descendant.pid");
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_pid_file = pid_file.clone();
    let stopper = std::thread::spawn(move || {
        let created = wait_until(Duration::from_secs(2), || stop_pid_file.exists());
        stop_flag.store(true, Ordering::SeqCst);
        created
    });

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("job stopped by request"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[test]
fn shell_job_stdout_stderr_are_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.policy.max_output_bytes = 8;
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &format!(
            "{}; {}",
            shell_echo("0123456789"),
            shell_echo_err("abcdefghij")
        ),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    let stdout = result.stdout.unwrap();
    let stderr = result.stderr.unwrap();
    assert_eq!(stdout.len(), 8);
    assert!(stdout.starts_with("[...]\n"), "{stdout:?}");
    assert_eq!(stderr.len(), 8);
    assert!(stderr.starts_with("[...]\n"), "{stderr:?}");
}

// ---------------------------------------------------------------------------
// Stage 2F: shell.rs ManagedChild lifecycle coverage.
//
// The shell execution path owns its process tree through ManagedChild (a
// private process group on Unix, a kill-on-close Job Object on Windows). These
// tests drive the cross-platform `validation_tree_helper` fixture through the
// real configured shell (`sh -c` on Unix, PowerShell on Windows) and probe
// descendant pids with a platform-native liveness probe — never with
// taskkill / Stop-Process / wmic / `ps` and never through shell quoting of
// process listings.
// ---------------------------------------------------------------------------

/// Compiled copy of the `validation_tree_helper` fixture, kept alive for the
/// whole test process so its binary path never disappears under a running
/// descendant (same pattern as the validation lifecycle tests).
struct ShellTreeHelper {
    _temp: tempfile::TempDir,
    path: PathBuf,
}

static SHELL_TREE_HELPER: std::sync::OnceLock<std::sync::Arc<ShellTreeHelper>> =
    std::sync::OnceLock::new();

fn shell_tree_helper() -> PathBuf {
    SHELL_TREE_HELPER
        .get_or_init(|| {
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src/webcodex_runner/validation/validation_tree_helper.rs");
            let temp = tempfile::tempdir().unwrap();
            let output = temp
                .path()
                .join(format!("shell-tree-helper{}", std::env::consts::EXE_SUFFIX));
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let result = std::process::Command::new(rustc)
                .arg("--edition=2021")
                .arg("--crate-name=webcodex_shell_tree_helper")
                .arg(&source)
                .arg("-o")
                .arg(&output)
                .output()
                .expect("run rustc for shell tree helper");
            assert!(
                result.status.success(),
                "shell tree helper compilation failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            std::sync::Arc::new(ShellTreeHelper {
                _temp: temp,
                path: output,
            })
        })
        .path
        .clone()
}

/// Single-quote `value` for the platform test shell. Windows uses PowerShell
/// ('' escapes an embedded quote); Unix uses POSIX sh ('\'' escapes one).
#[cfg(windows)]
fn shell_tree_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_tree_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build the shell command line that runs the fixture helper with `args`.
///
/// Windows drives the helper through PowerShell with single-quoted paths (no
/// cmd.exe quote-parsing pitfalls) and appends `exit $LASTEXITCODE` so the
/// helper's exit status becomes the shell's exit status. Unix uses the POSIX
/// shell directly.
fn shell_tree_command(helper: &Path, args: &[String]) -> String {
    let mut parts: Vec<String> = vec![shell_tree_quote(&helper.to_string_lossy())];
    parts.extend(args.iter().map(|arg| shell_tree_quote(arg)));
    let joined = parts.join(" ");
    #[cfg(windows)]
    {
        format!("& {joined}; exit $LASTEXITCODE")
    }
    #[cfg(not(windows))]
    {
        joined
    }
}

/// Test shell that can actually run on this platform: PowerShell on Windows
/// (cmd.exe quote parsing and missing `sleep` make POSIX-style commands
/// unusable), the default `sh -c` on Unix.
#[cfg(windows)]
fn shell_tree_test_shell() -> ShellConfig {
    ShellConfig {
        program: "powershell.exe".to_string(),
        args: vec!["-NoProfile".to_string(), "-Command".to_string()],
        ..ShellConfig::default()
    }
}

#[cfg(not(windows))]
fn shell_tree_test_shell() -> ShellConfig {
    ShellConfig::default()
}

/// Shell timeout used by the tree tests: Windows needs headroom for
/// PowerShell startup, Unix shells start instantly.
fn shell_tree_test_timeout_secs() -> u64 {
    if cfg!(windows) {
        5
    } else {
        1
    }
}

/// Platform-native liveness probe, so the tree tests never shell out to
/// `tasklist` / `ps` / PowerShell / wmic and never depend on shell quoting.
#[cfg(windows)]
fn shell_tree_process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: OpenProcess returns a handle or NULL; NULL means the pid no
    // longer exists (or is inaccessible, which also means not ours).
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0u32;
    // SAFETY: `handle` is valid; `exit_code` is a valid out-param.
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    // SAFETY: close the handle we opened.
    unsafe { CloseHandle(handle) };
    ok == 1 && exit_code == 259 // 259 == STILL_ACTIVE
}

#[cfg(target_os = "linux")]
fn shell_tree_process_alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(") ")
        .and_then(|(_, rest)| rest.chars().next())
        .is_some_and(|state| state != 'Z')
}

#[cfg(all(unix, not(target_os = "linux")))]
fn shell_tree_process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 is an existence probe; the pid comes from our own
    // helper subprocess.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_until_file(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until_process_dead(pid: u32, timeout: Duration, tag: &str) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !shell_tree_process_alive(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            eprintln!("wait_until_process_dead({tag}): pid {pid} still alive");
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Parse `KEY=<pid>` from a marker file written by the fixture helper.
fn read_marker_pid(marker: &Path, key: &str) -> u32 {
    let text = std::fs::read_to_string(marker).expect("read pid marker");
    text.lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or_else(|| panic!("marker {marker:?} missing {key}: {text}"))
}

/// Marker paths and the keepalive command for the two-argument
/// `spawn-descendant-keepalive` / `spawn-descendant` fixtures.
struct ShellTreeMarkers {
    parent: PathBuf,
    alive: PathBuf,
}

impl ShellTreeMarkers {
    fn in_dir(tmp: &std::path::Path, tag: &str) -> Self {
        Self {
            parent: tmp.join(format!("{tag}-parent.txt")),
            alive: tmp.join(format!("{tag}-alive.txt")),
        }
    }

    fn keepalive_command(&self, helper: &Path) -> String {
        shell_tree_command(
            helper,
            &[
                "spawn-descendant-keepalive".to_string(),
                self.parent.to_string_lossy().into_owned(),
                self.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        )
    }

    /// Both pids must be dead after cancellation; `PARENT_PID` and
    /// `DESCENDANT_PID` are both written to the parent marker.
    fn assert_tree_dead(&self, tag: &str) {
        let parent = read_marker_pid(&self.parent, "PARENT_PID");
        let descendant = read_marker_pid(&self.parent, "DESCENDANT_PID");
        assert!(
            wait_until_process_dead(parent, Duration::from_secs(10), &format!("{tag}-parent")),
            "tree parent {parent} survived {tag}"
        );
        assert!(
            wait_until_process_dead(
                descendant,
                Duration::from_secs(10),
                &format!("{tag}-descendant")
            ),
            "tree descendant {descendant} survived {tag}"
        );
    }
}

#[test]
fn shell_job_normal_success_preserves_output_and_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &["sleep".to_string(), "0".to_string(), "7".to_string()],
        ),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(7), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("VALIDATION_HELPER_STDOUT"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("VALIDATION_HELPER_STDERR"),
        "{result:?}"
    );
    assert!(
        result.duration_ms.unwrap_or(u64::MAX) < 30_000,
        "unbounded shell run: {result:?}"
    );
}

#[test]
fn shell_job_timeout_kills_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "timeout");
    let timeout_secs = shell_tree_test_timeout_secs();

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        timeout_secs,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "tree markers were never written: {result:?}"
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains(&format!("command timed out after {timeout_secs} seconds")),
        "{result:?}"
    );
    markers.assert_tree_dead("timeout");
}

#[test]
fn shell_job_stop_kills_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "stop");
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("job stopped by request"),
        "{result:?}"
    );
    markers.assert_tree_dead("stop");
}

#[test]
fn shell_job_parent_exit_first_descendant_holds_pipe() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "orphan");

    // `spawn-descendant`: the helper spawns a sleeping descendant that
    // inherits the capture pipes, waits until it is provably alive, then
    // exits 0. The direct shell child therefore exits while the descendant
    // is still running and still holding the stdout/stderr write ends.
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &[
                "spawn-descendant".to_string(),
                markers.parent.to_string_lossy().into_owned(),
                markers.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        ),
        None,
        30,
        None,
    );

    // Direct-child exit code is preserved even though the descendant was
    // still alive at that point.
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("DESCENDANT_PID="),
        "stdout did not reach EOF with the helper output: {result:?}"
    );
    // The descendant was sleeping (total 120s) when the direct child exited;
    // the whole-tree cleanup must terminate it instead of waiting for the
    // sleep to finish.
    let descendant = read_marker_pid(&markers.parent, "DESCENDANT_PID");
    assert!(
        wait_until_process_dead(descendant, Duration::from_secs(10), "orphan-descendant"),
        "descendant {descendant} survived whole-tree cleanup after direct child exit"
    );
    assert!(
        result.duration_ms.unwrap_or(u64::MAX) < 30_000,
        "runner waited for the descendant's natural sleep: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn shell_job_unix_graceful_sigterm_responsive_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();

    // `sigterm-marker`: the helper installs a SIGTERM handler that writes
    // SIGTERM_HANDLED to the captured stdout. SIGKILL cannot be caught, so
    // the marker only appears when the graceful phase delivered SIGTERM and
    // the tree exited on its own — no force escalation required.
    let result = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(&cwd),
        &shell_tree_command(&helper, &["sigterm-marker".to_string(), "60".to_string()]),
        None,
        1,
        None,
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("SIGTERM_HANDLED"),
        "graceful SIGTERM handler did not run: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn shell_job_unix_sigterm_resistant_tree_escalates() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "resist");

    // `ignore-term-keepalive`: the helper (and its descendant) ignore
    // SIGTERM, so the 50ms graceful phase cannot end the tree; only the
    // force escalation (SIGKILL) finishes it, within the cleanup deadline.
    let result = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &[
                "ignore-term-keepalive".to_string(),
                markers.parent.to_string_lossy().into_owned(),
                markers.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        ),
        None,
        1,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "tree markers were never written: {result:?}"
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    markers.assert_tree_dead("resist");
}

#[test]
fn shell_job_repeated_stop_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let stop_requested = Arc::new(AtomicBool::new(false));

    let first = ShellTreeMarkers::in_dir(tmp.path(), "repeat-1");
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = first.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &first.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );
    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    first.assert_tree_dead("repeat-1");

    // A second run against the same already-set flag must stop promptly and
    // clean up its own freshly spawned tree without a panic or deadlock.
    let second = ShellTreeMarkers::in_dir(tmp.path(), "repeat-2");
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &second.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    if wait_until_file(&second.parent, Duration::from_secs(5)) {
        // If the tree got far enough to write markers, it must be dead.
        second.assert_tree_dead("repeat-2");
    }
}

#[test]
fn shell_job_timeout_racing_stop_is_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "race");
    let timeout_secs = shell_tree_test_timeout_secs();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        // Set the stop flag shortly before the timeout would fire; either
        // outcome (stop or timeout) is legitimate for the shell API.
        let delay = if cfg!(windows) { 3600 } else { 600 };
        std::thread::sleep(Duration::from_millis(delay));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        timeout_secs,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert!(
        matches!(
            result.error.as_deref(),
            Some("command timed out" | "job stopped")
        ),
        "unexpected race outcome: {result:?}"
    );
    markers.assert_tree_dead("race");
}

#[test]
fn shell_job_spawn_failure_preserves_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("no-such-shell-program");
    let shell = ShellConfig {
        program: missing.to_string_lossy().into_owned(),
        ..ShellConfig::default()
    };
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell,
        None,
        "true",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, None, "{result:?}");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.starts_with("failed to spawn command: "),
        "spawn error semantics changed: {result:?}"
    );
}

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

// ---------------------------------------------------------------------------
// Stage 2G: Windows-native PowerShell shell semantics.
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn shell_job_native_exe_nonzero_exit_code_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    // No fixture-level `exit $LASTEXITCODE`: the PowerShell command wrapper
    // must propagate the native executable's exit code on its own.
    let command = format!(
        "& {} sleep 0 3",
        shell_tree_quote(&helper.to_string_lossy())
    );
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &command,
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(3), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(windows)]
#[test]
fn shell_job_unicode_stdout_stderr_env_and_cwd() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let unicode_cwd = tmp.path().join("unicode cwd 测试");
    std::fs::create_dir_all(&unicode_cwd).unwrap();
    let cwd = unicode_cwd.to_string_lossy().to_string();

    // Unicode stdout and stderr. The PowerShell wrapper installs the UTF-8
    // console encodings before the command, so this never depends on the
    // machine's legacy console code page.
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        "[Console]::Out.Write('café ☃ 测试'); [Console]::Error.Write('err 測試')",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("café ☃ 测试"));
    assert_eq!(result.stderr.as_deref(), Some("err 測試"));

    // Unicode environment value inherited from the parent process.
    let saved = std::env::var_os("WEBCODEX_UNICODE_ENV");
    std::env::set_var("WEBCODEX_UNICODE_ENV", "值 测试");
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_env_var("WEBCODEX_UNICODE_ENV"),
        None,
        10,
        None,
    );
    match saved {
        Some(value) => std::env::set_var("WEBCODEX_UNICODE_ENV", value),
        None => std::env::remove_var("WEBCODEX_UNICODE_ENV"),
    }
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("值 测试"));

    // Unicode cwd: the shell reports its working directory verbatim.
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        "[Console]::Out.Write((Get-Location).Path)",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("测试"),
        "{result:?}"
    );
}

#[cfg(windows)]
#[test]
fn prepared_profile_unicode_env_round_trip_and_unicode_init_path() {
    let tmp = tempfile::tempdir().unwrap();
    // The init script lives in a directory with spaces and non-ASCII
    // characters, and exports a Unicode value that must survive the whole
    // snapshot pipeline: PowerShell `Get-ChildItem Env:` -> UTF-8 NUL
    // payload -> Runner parse -> environment of later commands.
    let init_dir = tmp.path().join("init 目录");
    std::fs::create_dir_all(&init_dir).unwrap();
    let init = init_dir.join("profile 脚本.ps1");
    // UTF-8 BOM: PowerShell 5.1 otherwise decodes .ps1 files with the system
    // ANSI code page and corrupts the non-ASCII value.
    let mut content = "\u{FEFF}".to_string();
    content.push_str("$env:WEBCODEX_TEST_PROFILE = 'café 值'\n");
    std::fs::write(&init, content).unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(format!(". {}", shell_tree_quote(&init.to_string_lossy()))),
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
    assert_eq!(result.stdout.as_deref(), Some("café 值"), "{result:?}");
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

#[test]
fn computer_register_request_announces_platform_capability_and_protocol_version() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // A stale or hand-edited config cannot force capability advertisement:
    // registration replaces it with the result of the real host probe.
    cfg.capabilities = Some(ShellClientCapabilities {
        sandbox_inspect_commands: true,
        computer_observe: true,
        computer_snapshot_region: true,
        computer_accessibility_observe: true,
        computer_element_state: true,
        computer_control: true,
        computer_scroll_to_element: true,
        computer_key_input: true,
        computer_window_activate: true,
        computer_text_input: true,
        project_lifecycle: false,
        project_path_registration: false,
        ..Default::default()
    });
    for (version, expected_str) in [
        (AGENT_PROTOCOL_VERSION_POLLING_V1, "polling-v1"),
        (AGENT_PROTOCOL_VERSION_WEBSOCKET_V1, "websocket-v1"),
        (AGENT_PROTOCOL_VERSION_QUIC_V1, "quic-v1"),
    ] {
        let body = build_register_request(&cfg, Vec::new(), version, "inst-1", 0);
        let caps = body.capabilities.as_ref().expect("transport capabilities");
        assert!(caps.structured_go_test_tool, "{expected_str}");
        assert!(caps.structured_go_test_packages, "{expected_str}");
        assert!(caps.structured_file_delete, "{expected_str}");
        assert_eq!(body.agent_instance_id, "inst-1");
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(version),
            "version mismatch for {expected_str}"
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some(expected_str));
    }
    // Also verify capabilities are advertised (check once for polling).
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let caps = body.capabilities.expect("agent registers capabilities");
    assert!(caps.shell);
    assert!(caps.file_read);
    assert!(caps.file_write);
    assert!(caps.artifact_export_chunk_read);
    assert!(caps.artifact_export_streaming_metadata);
    assert!(caps.structured_file_delete);
    assert!(caps.async_jobs);
    assert!(caps.async_shell_jobs);
    assert!(caps.structured_validation_argv);
    assert!(caps.structured_go_test_json);
    assert!(caps.structured_go_test_tool);
    assert!(caps.structured_go_test_packages);
    assert!(caps.structured_process_argv);
    assert!(caps.structured_script_payload);
    assert!(caps.structured_execution_jobs);
    assert!(caps.lsp_read_only_navigation);
    assert!(caps.lsp_call_hierarchy);
    assert!(caps.project_lifecycle);
    assert!(caps.project_path_registration);
    assert_eq!(
        caps.computer_observe,
        cfg!(any(target_os = "macos", windows)),
        "computer observation is advertised only when this Runner binary has a supported native implementation"
    );
    assert_eq!(
        caps.computer_snapshot_region,
        cfg!(any(target_os = "macos", windows)),
        "computer region snapshot is independently advertised only when native window capture is supported"
    );
    assert_eq!(
        caps.computer_accessibility_observe,
        cfg!(target_os = "macos"),
        "computer accessibility observation is advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_element_state,
        cfg!(target_os = "macos"),
        "computer element state is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_control,
        cfg!(target_os = "macos"),
        "computer control is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_scroll_to_element,
        cfg!(target_os = "macos"),
        "computer scroll-to-element is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_key_input,
        cfg!(target_os = "macos"),
        "computer key input is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_window_activate,
        cfg!(target_os = "macos"),
        "computer window activation is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.computer_text_input,
        cfg!(target_os = "macos"),
        "computer text input is independently advertised only by the macOS native implementation"
    );
    assert_eq!(
        caps.sandbox_inspect_commands,
        crate::command_sandbox::inspect_sandbox_available().is_ok()
    );
}

#[test]
fn phase_e2_register_request_reports_effective_job_concurrency_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    for (configured, expected) in [
        (None, 4),
        (Some(0), 1),
        (Some(1), 1),
        (Some(8), 8),
        (Some(64), 64),
        (Some(65), 64),
        (Some(128), 64),
    ] {
        cfg.max_concurrent_jobs = configured;
        for protocol in [
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            AGENT_PROTOCOL_VERSION_QUIC_V1,
        ] {
            let body = build_register_request(&cfg, Vec::new(), protocol, "inst-limit", 0);
            assert_eq!(
                body.job_concurrency_limit,
                Some(expected),
                "configured={configured:?} protocol={protocol}"
            );
        }
    }
}

#[test]
fn register_request_carries_sanitized_shell_profiles_summary() {
    // A config with one profile carrying a secret env value and a secret
    // init_script body. The sanitized summary must report the profile name,
    // has_init_script=true, and env_keys_count, but MUST NOT include the env
    // value or the init_script body.
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    let secret_env = "DO_NOT_LEAK_THIS_ENV_VALUE";
    let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
    cfg.shell = shell_with_profiles(
        Some("rust"),
        vec![(
            "rust",
            ShellProfileConfig {
                program: Some("sh".to_string()),
                args: Some(vec!["-c".to_string()]),
                env: profile_env(&[("SECRET_KEY", secret_env)]),
                init_script: Some(secret_script.to_string()),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let policy = body.policy.expect("agent registers a policy");
    let summary = policy
        .shell_profiles
        .as_ref()
        .expect("sanitized shell profiles summary is present");
    assert_eq!(summary.default_profile.as_deref(), Some("rust"));
    assert_eq!(summary.configured_count, 1);
    assert_eq!(summary.profiles.len(), 1);
    let entry = &summary.profiles[0];
    assert_eq!(entry.name, "rust");
    assert!(entry.has_init_script);
    assert_eq!(entry.env_keys_count, 1);
    assert_eq!(entry.program, "sh");
    assert_eq!(entry.args_count, 1);
    // Sanitization: the rendered summary never carries env values or the
    // init_script body.
    let rendered = serde_json::to_string(summary).unwrap();
    assert!(!rendered.contains(secret_env), "{rendered}");
    assert!(!rendered.contains(secret_script), "{rendered}");
}

// ------------------------------------------------------------------------
// WebSocket transport helpers + shared dispatch over a WebSocket sink
// ------------------------------------------------------------------------

#[test]
fn server_url_to_ws_converts_http_https_and_rejects_bare() {
    assert_eq!(
        server_url_to_ws("http://127.0.0.1:8080", "/api/agents/ws").unwrap(),
        "ws://127.0.0.1:8080/api/agents/ws"
    );
    assert_eq!(
        server_url_to_ws("https://example.com/", "/api/agents/ws").unwrap(),
        "wss://example.com/api/agents/ws"
    );
    // Already a ws(s) URL passes through.
    assert_eq!(
        server_url_to_ws("wss://example.com", "/api/agents/ws").unwrap(),
        "wss://example.com/api/agents/ws"
    );
    assert!(server_url_to_ws("ftp://x", "/api/agents/ws").is_err());
}

#[test]
fn generated_agent_instance_id_is_non_empty_uuid_like() {
    // `run_agent` generates the instance id the same way; verify the
    // format here without driving the full agent loop.
    let id = uuid::Uuid::new_v4().to_string();
    assert!(!id.is_empty());
    // Canonical UUID v4 is 36 chars: 8-4-4-4-12 hex groups.
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    // The register builder carries it through unchanged.
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let body = build_register_request(&cfg, Vec::new(), AGENT_PROTOCOL_VERSION_POLLING_V1, &id, 0);
    assert_eq!(body.agent_instance_id, id);
    assert!(!body.agent_instance_id.is_empty());
}

fn ws_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    (
        AgentSink::WebSocket {
            tx,
            client_id: client_id.to_string(),
            agent_instance_id: "ws-inst".to_string(),
        },
        rx,
    )
}

fn quic_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    (
        AgentSink::Quic {
            tx,
            client_id: client_id.to_string(),
            agent_instance_id: "quic-inst".to_string(),
        },
        rx,
    )
}

#[test]
fn sink_submit_result_sends_result_envelope() {
    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client, expected_instance) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "ws-inst"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-inst"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let result = CommandResult {
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(3),
            error: None,
        };
        assert_eq!(
            sink.submit_result("req-9".to_string(), result).unwrap(),
            webcodex_runner::ResultSubmission::Accepted,
            "{label}"
        );
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.result.agent_instance_id, expected_instance,
                    "{label}"
                );
                assert_eq!(payload.result.request_id, "req-9");
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some("hi"));
                assert_eq!(payload.command_execution_state, None);
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_send_job_update_sends_job_update_envelope() {
    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client) in [
        ("ws", ws_sink as SinkFactory, "ws-client"),
        ("quic", quic_sink as SinkFactory, "quic-client"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let body = ShellAgentJobUpdateRequest {
            client_id: expected_client.to_string(),
            agent_instance_id: sink.agent_instance_id().to_string(),
            job_id: "job-1".to_string(),
            request_id: Some("req-1".to_string()),
            update_seq: None,
            status: "running".to_string(),
            stdout_chunk: Some(format!("{label}-chunk")),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };
        sink.send_job_update(&body).unwrap();
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::JobUpdate { payload } => {
                assert_eq!(payload.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.agent_instance_id,
                    sink.agent_instance_id(),
                    "{label}"
                );
                assert_eq!(payload.job_id, "job-1", "{label}");
                assert_eq!(payload.status, "running", "{label}");
                assert_eq!(
                    payload.stdout_chunk.as_deref(),
                    Some(format!("{label}-chunk").as_str()),
                    "{label}"
                );
            }
            other => panic!("{label}: expected job_update, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_try_send_job_update_preserves_full_ws_and_quic_queue_for_retry() {
    for label in ["ws", "quic"] {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tx.try_send(AgentEnvelope::Ping { ts: 11 }).unwrap();
        let sink = match label {
            "ws" => AgentSink::WebSocket {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            "quic" => AgentSink::Quic {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            _ => unreachable!(),
        };
        let body = ShellAgentJobUpdateRequest {
            client_id: "stream-client".to_string(),
            agent_instance_id: "stream-instance".to_string(),
            job_id: "job-full".to_string(),
            request_id: Some("request-full".to_string()),
            update_seq: Some(2),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };

        assert_eq!(sink.try_send_job_update(&body), Ok(false), "{label}");
        assert!(matches!(rx.try_recv(), Ok(AgentEnvelope::Ping { ts: 11 })));
        assert_eq!(sink.try_send_job_update(&body), Ok(true), "{label}");
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEnvelope::JobUpdate { payload }) if payload.job_id == "job-full"
        ));
    }
}

#[cfg(unix)]
#[test]
fn job_manager_stop_all_clears_queue_and_requests_running_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(1);
    let stop_requested = Arc::new(AtomicBool::new(false));
    let mut running_command =
        configured_shell_job_command(&ShellConfig::default(), "sleep 60").unwrap();
    running_command
        .current_dir(tmp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let running_child = Arc::new(Mutex::new(
        ManagedChild::spawn(&mut running_command).unwrap(),
    ));
    let running_pid = lock_unpoison(&running_child).id();
    jobs.jobs.lock().unwrap().insert(
        "running-job".to_string(),
        RunningJob {
            client_id: "ws-client".to_string(),
            agent_instance_id: "ws-instance".to_string(),
            snapshot: test_job_snapshot("running-job"),
            child: Some(Arc::clone(&running_child)),
            stop_requested: stop_requested.clone(),
            slot_reserved: true,
        },
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-queued".to_string(),
        client_id: "ws-client".to_string(),
        kind: "start_job".to_string(),
        job_id: Some("queued-job".to_string()),
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: ": > queued-started".to_string(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 60,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: Some(test_job_context(tmp.path(), Vec::new())),
        persistent_shell: None,
    };
    let mut rejected_request = request.clone();
    rejected_request.request_id = "req-after-shutdown".to_string();
    rejected_request.job_id = Some("job-after-shutdown".to_string());

    jobs.enqueue(
        sink,
        PendingJobStart {
            generation: 1,
            policy: cfg.policy.clone(),
            shell: cfg.shell.clone(),
            ssh: cfg.ssh.clone(),
            projects_dir: projects_dir(&cfg).unwrap(),
            request,
        },
    );
    match wait_for_job_envelope(&mut rx, "queued status was sent") {
        AgentEnvelope::JobUpdate { payload } => {
            assert_eq!(payload.job_id, "queued-job");
            assert_eq!(payload.status, "agent_queued");
        }
        other => panic!("expected job_update, got {:?}", other.kind()),
    }
    assert_eq!(jobs.queued.lock().unwrap().len(), 1);

    jobs.stop_all();

    assert!(stop_requested.load(Ordering::SeqCst));
    assert!(jobs.queued.lock().unwrap().is_empty());
    assert!(lock_unpoison(&running_child).try_wait().unwrap().is_some());
    assert!(!job_manager_tests::process_running(running_pid));
    assert!(
        !tmp.path().join("queued-started").exists(),
        "queued job started during shutdown"
    );

    let (rejected_sink, mut rejected_rx) = ws_sink("ws-client");
    jobs.enqueue(
        rejected_sink,
        PendingJobStart {
            generation: 1,
            policy: cfg.policy.clone(),
            shell: cfg.shell.clone(),
            ssh: cfg.ssh.clone(),
            projects_dir: projects_dir(&cfg).unwrap(),
            request: rejected_request,
        },
    );
    assert!(jobs.queued.lock().unwrap().is_empty());
    let rejected = (0..2)
        .find_map(
            |_| match wait_for_job_envelope(&mut rejected_rx, "shutdown update was sent") {
                AgentEnvelope::JobUpdate { payload } if payload.finished => Some(payload),
                AgentEnvelope::JobUpdate { .. } => None,
                other => panic!("expected job_update, got {:?}", other.kind()),
            },
        )
        .expect("shutdown rejection terminal update was sent");
    assert_eq!(rejected.job_id, "job-after-shutdown");
    assert_eq!(rejected.status, "failed");
    assert!(rejected.finished);
    assert_eq!(rejected.error.as_deref(), Some("runner is shutting down"));
}

#[test]
fn file_request_kind_includes_edit_and_basic_ops() {
    for kind in [
        "file_read",
        "file_write",
        "file_list",
        "file_project_overview",
        "file_delete_project_files",
        "file_write_project_file",
        "file_apply_text_edits",
    ] {
        assert!(
            is_file_request_kind(kind),
            "{kind} should route to file handler"
        );
    }
    // Removed legacy edit request kinds no longer route to the file handler.
    for kind in [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
    ] {
        assert!(
            !is_file_request_kind(kind),
            "{kind} must no longer be a file request kind"
        );
    }
    assert!(!is_file_request_kind("run_shell"));
}

#[test]
fn project_overview_agent_request_returns_metadata_without_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("Cargo.toml"), "private manifest content").unwrap();
    std::fs::write(tmp.path().join("README.md"), "private readme content").unwrap();
    std::fs::write(tmp.path().join(".env"), "TOKEN=not-returned").unwrap();
    let request = json_file_op_request(
        tmp.path(),
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 2, "limit": 200}),
    );

    let output = line_edit_json(handle_file_request(&policy, &request));
    assert_eq!(output["schema_version"], 1);
    assert_eq!(output["deterministic"], true);
    assert!(output.to_string().contains("Cargo.toml"));
    assert!(!output.to_string().contains("private manifest content"));
    assert!(!output.to_string().contains("TOKEN=not-returned"));
    assert!(!output.to_string().contains(".env"));
    assert!(!output
        .to_string()
        .contains(&tmp.path().display().to_string()));
}

#[test]
fn dispatch_request_edit_routes_to_file_handler() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let (sink, mut rx) = ws_sink("ws-client");
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let request = ShellAgentShellRequest {
        request_id: "req-edit".to_string(),
        client_id: "ws-client".to_string(),
        kind: "file_write_project_file".to_string(),
        job_id: None,
        cwd: Some(cwd),
        path: Some("new.txt".to_string()),
        content: Some(
            serde_json::json!({
                "path": "new.txt",
                "content": "new content\n",
                "overwrite": false,
            })
            .to_string(),
        ),
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
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    let pdir = projects_dir(&cfg).unwrap();
    let lsp = webcodex_runner::LspSupervisor::default();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let ran = dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &lsp,
        request,
    )
    .unwrap();
    assert!(ran);
    let env = rx.try_recv().expect("result envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-edit");
            assert_eq!(payload.result.exit_code, Some(0));
            let stdout = payload
                .result
                .stdout
                .expect("file handler returns JSON stdout");
            assert!(stdout.contains("\"created\":true"), "stdout was {stdout}");
            assert_eq!(
                std::fs::read_to_string(tmp.path().join("new.txt")).unwrap(),
                "new content\n"
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_rejects_unsupported_file_kinds_without_starting_command() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let kinds = [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
        "file_future_unknown_operation",
    ];

    for (index, kind) in kinds.into_iter().enumerate() {
        let marker_name = format!("unsupported-file-marker-{index}");
        let target_name = format!("unsupported-file-target-{index}.txt");
        let marker = tmp.path().join(&marker_name);
        let target = tmp.path().join(&target_name);
        std::fs::write(&target, "original\n").unwrap();
        let command = format!(
            "printf shell-ran > {marker_name}; printf modified > {target_name}; printf shell-stdout"
        );
        let request: ShellAgentShellRequest = serde_json::from_value(serde_json::json!({
            "request_id": format!("req-unsupported-file-{index}"),
            "client_id": "ws-client",
            "kind": kind,
            "cwd": tmp.path().to_string_lossy(),
            "path": target_name,
            "content": "replacement",
            "command": command,
            "old_text": "old",
            "pattern": "needle",
            "line": 10,
            "timeout_secs": 10,
            "requested_by": "tester",
            "created_at": 0,
        }))
        .unwrap();
        let (sink, mut rx) = ws_sink("ws-client");

        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();

        assert!(ran, "{kind}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.exit_code, None, "{kind}");
                assert_eq!(payload.result.stdout, None, "{kind}");
                assert_eq!(
                    payload.result.error.as_deref(),
                    Some(
                        "unsupported_file_request_kind: unsupported file request kind; command was not started"
                    ),
                    "{kind}"
                );
            }
            other => panic!("{kind}: expected result, got {:?}", other.kind()),
        }
        assert!(!marker.exists(), "{kind}: shell marker was created");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "original\n",
            "{kind}: target file was modified"
        );
    }
}

#[test]
fn dispatch_request_run_shell_sends_result_over_sink() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );

    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, client_id, expected_stdout) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "wsok"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-ok"),
    ] {
        let (sink, mut rx) = make_sink(client_id);
        let request = ShellAgentShellRequest {
            request_id: format!("req-{label}"),
            client_id: client_id.to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some(tmp.path().to_string_lossy().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: shell_echo(expected_stdout),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();
        assert!(ran, "{label}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.request_id, format!("req-{label}"));
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some(expected_stdout));
                assert_eq!(
                    payload.command_execution_state,
                    Some(ShellCommandExecutionState::Completed)
                );
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn dispatch_request_internal_search_uses_posix_runtime_not_configured_shell_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // The generated search program is POSIX shell. It must not inherit an
    // arbitrary configured shell parser (PowerShell is the Windows production
    // case); use a guaranteed-failing program here to prove the bypass.
    cfg.shell.program = if cfg!(windows) {
        "powershell".to_string()
    } else {
        "/bin/false".to_string()
    };
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let marker = r#"{"webcodex_search":{"backend":"grep","feature_unavailable":false}}"#;
    let request = ShellAgentShellRequest {
        request_id: "req-internal-search".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: format!(
            "{}\nprintf '%s\\n' '{}'\n",
            shell_protocol::EXTERNAL_SEARCH_REQUEST_PREFIX,
            marker
        ),
        process: None,
        script: None,
        stdin: Some("{}".to_string()),
        timeout_secs: if cfg!(windows) { 30 } else { 10 },
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("internal search result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert!(payload
                .result
                .stdout
                .as_deref()
                .unwrap_or_default()
                .contains("webcodex_search"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_run_shell_rejects_oversized_wire_command_before_start() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-oversized-shell".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: "x".repeat(shell_protocol::RAW_SHELL_WIRE_MAX_BYTES + 1),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    let env = rx.try_recv().expect("rejection envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-oversized-shell");
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
            assert!(payload
                .result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("invalid_raw_shell_request"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_structured_process_uses_typed_argv_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = tmp.path().join(format!(
        "process-argv-helper{}",
        std::env::consts::EXE_SUFFIX
    ));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/process_argv_helper.rs");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_process_argv_helper")
        .arg(fixture)
        .arg("-o")
        .arg(&helper)
        .output()
        .expect("run rustc for process argv helper");
    assert!(
        compile.status.success(),
        "process argv helper compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let marker = tmp.path().join("marker");

    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-structured-process".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: Some(shell_protocol::ShellProcessArgv {
            executable: helper.to_string_lossy().into_owned(),
            args: vec![
                "argv".to_string(),
                "$(touch marker)".to_string(),
                "; touch marker".to_string(),
            ],
        }),
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured process result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
            let stdout = payload.result.stdout.unwrap();
            assert!(stdout.contains("$(touch marker)"));
            assert!(stdout.contains("; touch marker"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());

    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");
    let (sink, mut rx) = ws_sink("ws-client");
    let malformed = ShellAgentShellRequest {
        request_id: "req-structured-process-malformed".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: shell_write_file(&shell_fallback_marker),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured process rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
}

#[cfg(unix)]
#[test]
fn dispatch_request_structured_script_uses_typed_file_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let observed_path = tmp.path().join("observed-script-path");
    let marker = tmp.path().join("marker");
    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");

    let request = ShellAgentShellRequest {
        request_id: "req-structured-script".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_script".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
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
        script: Some(shell_protocol::ShellScriptPayload {
            language: shell_protocol::ShellScriptLanguage::Sh,
            script: "printf '%s' \"$0\" > \"$1\"\nprintf '%s\\n' \"$2\"\n".to_string(),
            args: vec![
                observed_path.to_string_lossy().into_owned(),
                "; touch marker".to_string(),
            ],
        }),
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    let mut malformed = request.clone();
    malformed.request_id = "req-structured-script-malformed".to_string();
    malformed.command = shell_write_file(&shell_fallback_marker);
    malformed.script = None;

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured script result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(payload.result.stdout.as_deref(), Some("; touch marker\n"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());
    let temporary_path =
        PathBuf::from(std::fs::read_to_string(&observed_path).expect("script path evidence"));
    assert!(!temporary_path.starts_with(tmp.path()));
    assert!(!temporary_path.exists());

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured script rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
}

fn project_policy(root: &Path) -> AgentPolicy {
    AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![root.to_path_buf()],
        ..AgentPolicy::default()
    }
}

fn project_request(kind: &str, payload: serde_json::Value) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: format!("req-{}", kind),
        client_id: "oe".to_string(),
        kind: kind.to_string(),
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
        stdin: Some(payload.to_string()),
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    }
}

fn project_ok(result: CommandResult) -> serde_json::Value {
    assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
    assert!(
        result.error.is_none(),
        "unexpected error: {:?}",
        result.error
    );
    serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
}

fn project_err(result: CommandResult) -> String {
    if let Some(error) = result.error {
        return error;
    }
    assert_ne!(
        result.exit_code,
        Some(0),
        "unexpected success: {:?}",
        result
    );
    serde_json::from_str::<serde_json::Value>(result.stdout.as_deref().expect("error json"))
        .unwrap()["error_code"]
        .as_str()
        .expect("error_code")
        .to_string()
}

fn project_error_value(result: CommandResult) -> serde_json::Value {
    assert_ne!(
        result.exit_code,
        Some(0),
        "unexpected success: {:?}",
        result
    );
    assert!(result.error.is_none(), "unexpected raw error: {:?}", result);
    serde_json::from_str(result.stdout.as_deref().expect("error json")).unwrap()
}

#[test]
fn register_project_writes_valid_toml_into_projects_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "path": project_dir.to_string_lossy(),
            "description": "A demo project",
            "allow_patch": false
        }),
    );

    let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(value["created_config"], true);
    assert_eq!(value["overwritten"], false);

    let content = std::fs::read_to_string(projects_dir.join("demo.toml")).unwrap();
    let parsed = parse_agent_project_toml(&content).unwrap();
    assert_eq!(parsed.id, "demo");
    assert_eq!(parsed.name.as_deref(), Some("Demo"));
    assert_eq!(parsed.path, project_dir.to_string_lossy());
    assert!(!parsed.allow_patch);
}

#[test]
fn resolve_or_register_project_persists_and_reuses_canonical_directory_without_touching_it() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("Example Repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "unchanged").unwrap();
    let target_entries_before = std::fs::read_dir(&project_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let policy = project_policy(tmp.path());

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.join(".").to_string_lossy()}),
        ),
    ));
    assert_eq!(first["outcome"], "auto_registered");
    assert_eq!(first["registered"], true);
    assert_eq!(first["changed"], true);
    let project_id = first["agent_project_id"].as_str().unwrap();
    assert!(project_id.starts_with("example-repo-"), "{project_id}");
    assert!(project_id.len() <= 64);

    let config_path = projects_dir.join(format!("{project_id}.toml"));
    let persisted =
        parse_agent_project_toml(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(persisted.id, project_id);
    assert_eq!(
        Path::new(&persisted.path),
        project_dir.canonicalize().unwrap()
    );
    assert_eq!(persisted.kind.as_deref(), Some("auto_registered"));
    assert!(persisted.allow_patch);
    assert_eq!(
        std::fs::read_to_string(project_dir.join("keep.txt")).unwrap(),
        "unchanged"
    );
    let target_entries_after = std::fs::read_dir(&project_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(target_entries_after, target_entries_before);
    assert!(!project_dir.join(".git").exists());

    let reloaded = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, project_id);
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["registered"], false);
    assert_eq!(second["agent_project_id"], project_id);
    assert_eq!(
        std::fs::read_dir(&projects_dir).unwrap().count(),
        1,
        "retry created a duplicate registration"
    );
}

#[cfg(unix)]
#[test]
fn resolve_or_register_project_reuses_symlink_target() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let link = tmp.path().join("repo-link");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    symlink(&project_dir, &link).unwrap();
    let policy = project_policy(tmp.path());

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": link.to_string_lossy()}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["agent_project_id"], first["agent_project_id"]);
    assert_eq!(
        Path::new(second["path"].as_str().unwrap()),
        project_dir.canonicalize().unwrap()
    );
}

#[test]
fn resolve_or_register_project_prefers_manual_id_and_distinguishes_same_basenames() {
    let tmp = tempfile::tempdir().unwrap();
    let first_parent = tmp.path().join("first");
    let second_parent = tmp.path().join("second");
    let manual_dir = tmp.path().join("manual");
    let projects_dir = tmp.path().join("projects.d");
    for directory in [
        first_parent.join("repo"),
        second_parent.join("repo"),
        manual_dir.clone(),
    ] {
        std::fs::create_dir_all(directory).unwrap();
    }
    std::fs::create_dir(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("friendly.toml"),
        format!(
            "id = \"friendly\"\nname = \"Friendly\"\npath = {:?}\nallow_patch = true\n",
            manual_dir.to_string_lossy()
        ),
    )
    .unwrap();
    let policy = project_policy(tmp.path());

    let manual = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": manual_dir.join(".").to_string_lossy()}),
        ),
    ));
    assert_eq!(manual["outcome"], "reused_existing_registration");
    assert_eq!(manual["agent_project_id"], "friendly");

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": first_parent.join("repo").to_string_lossy()}),
        ),
    ));
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": second_parent.join("repo").to_string_lossy()}),
        ),
    ));
    assert_ne!(first["agent_project_id"], second["agent_project_id"]);
    assert!(first["agent_project_id"]
        .as_str()
        .unwrap()
        .starts_with("repo-"));
    assert!(second["agent_project_id"]
        .as_str()
        .unwrap()
        .starts_with("repo-"));
}

#[test]
fn resolve_or_register_project_fails_closed_for_disabled_and_ambiguous_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("disabled.toml"),
        format!(
            "id = \"disabled\"\npath = {:?}\ndisabled = true\n",
            project_dir.to_string_lossy()
        ),
    )
    .unwrap();
    let policy = project_policy(tmp.path());

    let disabled = project_error_value(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(disabled["error_kind"], "project_disabled");
    assert_eq!(disabled["matching_project_id"], "disabled");
    assert_eq!(disabled["state_changed"], false);
    assert!(
        !disabled.to_string().contains(project_dir.to_str().unwrap()),
        "disabled error leaked the absolute path"
    );

    std::fs::write(
        projects_dir.join("alpha.toml"),
        format!(
            "id = \"alpha\"\npath = {:?}\n",
            project_dir.to_string_lossy()
        ),
    )
    .unwrap();
    std::fs::write(
        projects_dir.join("zeta.toml"),
        format!(
            "id = \"zeta\"\npath = {:?}\n",
            project_dir.join(".").to_string_lossy()
        ),
    )
    .unwrap();
    let ambiguous = project_error_value(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(ambiguous["error_kind"], "ambiguous_project_path");
    assert_eq!(
        ambiguous["matching_project_ids"],
        serde_json::json!(["alpha", "disabled", "zeta"])
    );
    assert_eq!(ambiguous["state_changed"], false);
}

#[test]
fn resolve_or_register_project_rejects_invalid_non_directory_and_disallowed_paths() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let file = allowed.path().join("file.txt");
    std::fs::write(&file, "not a directory").unwrap();
    let policy = project_policy(allowed.path());

    for (path, expected) in [
        ("relative/path".to_string(), "invalid_project_path"),
        (
            allowed.path().join("missing").to_string_lossy().to_string(),
            "project_path_not_found",
        ),
        (
            file.to_string_lossy().to_string(),
            "project_path_not_directory",
        ),
        (
            outside.path().to_string_lossy().to_string(),
            "path_outside_allowed_roots",
        ),
    ] {
        let error = project_error_value(handle_resolve_or_register_project(
            &policy,
            &projects_dir,
            &project_request(
                "resolve_or_register_project",
                serde_json::json!({"path": path}),
            ),
        ));
        assert_eq!(error["error_kind"], expected);
        assert_eq!(error["state_changed"], false);
    }
    assert!(!projects_dir.exists());

    let unrestricted = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: Vec::new(),
        ..AgentPolicy::default()
    };
    // Dangerous system roots are platform-specific: `/etc` on Unix,
    // `C:\Windows` on Windows (drive roots are also rejected).
    #[cfg(windows)]
    let dangerous_path = "C:\\Windows";
    #[cfg(not(windows))]
    let dangerous_path = "/etc";
    let dangerous = project_error_value(handle_resolve_or_register_project(
        &unrestricted,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": dangerous_path}),
        ),
    ));
    assert_eq!(dangerous["error_kind"], "path_outside_allowed_roots");
}

#[cfg(windows)]
#[test]
fn resolve_or_register_project_rejects_unc_and_non_local_disk_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());

    // The raw path check must fire before canonicalization: these shares do
    // not exist, but the error is the platform rule, not "path not found".
    for unc_path in [
        r"\\server\share\repo",
        r"\\?\UNC\server\share\repo",
        r"\\.\device\repo",
        r"\\?\Volume{12345678-1234-1234-1234-123456789abc}\repo",
    ] {
        let error = project_error_value(handle_resolve_or_register_project(
            &policy,
            &projects_dir,
            &project_request(
                "resolve_or_register_project",
                serde_json::json!({"path": unc_path}),
            ),
        ));
        assert_eq!(
            error["error_kind"], "unc_project_path_unsupported",
            "{unc_path} must fail closed as an unsupported non-local-disk path"
        );
        assert_eq!(error["state_changed"], false);
    }
    assert!(!projects_dir.exists(), "no registration may be attempted");

    // An allowed_roots entry naming a UNC share must not bypass the rule.
    let unc_allowed = AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![PathBuf::from(r"\\server\share\repo")],
        ..AgentPolicy::default()
    };
    let error = project_error_value(handle_resolve_or_register_project(
        &unc_allowed,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": r"\\server\share\repo"}),
        ),
    ));
    assert_eq!(
        error["error_kind"], "unc_project_path_unsupported",
        "a UNC allowed_root must not make a UNC project root acceptable"
    );
}

#[cfg(windows)]
#[test]
fn resolve_or_register_project_accepts_local_drive_and_verbatim_disk_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());

    // Plain local-drive path registers normally.
    let plain = project_dir.to_string_lossy().to_string();
    assert!(Path::new(&plain).is_absolute());
    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": plain}),
        ),
    ));
    assert_eq!(first["outcome"], "auto_registered");
    let project_id = first["agent_project_id"].as_str().unwrap().to_string();

    // The canonicalized `\\?\C:\...` spelling of the same directory must
    // reuse the registration instead of minting a duplicate identity. The
    // verbatim form is built from the plain path: `canonicalize()` already
    // returns `\\?\`-prefixed paths on modern Rust, so re-prefixing those
    // would double the prefix.
    let raw = project_dir.to_string_lossy().to_string();
    let verbatim = if raw.starts_with(r"\\?\") {
        raw
    } else {
        // `\\?\` + the raw path: the prefix itself ends with a backslash.
        format!(r"\\?\{raw}")
    };
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": verbatim}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["agent_project_id"], project_id);
    assert_eq!(
        std::fs::read_dir(&projects_dir).unwrap().count(),
        1,
        "the \\\\?\\ spelling created a duplicate project identity"
    );
}

#[cfg(windows)]
#[test]
fn register_project_rejects_unc_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());

    let error = project_error_value(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": r"\\server\share\repo",
                "description": "UNC project",
                "allow_patch": false
            }),
        ),
    ));
    assert_eq!(error["error_code"], "unc_project_path_unsupported");
    assert!(!projects_dir.exists());
}

#[test]
fn concurrent_path_resolution_converges_on_one_registration() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let mut workers = Vec::new();
    for _ in 0..2 {
        let project_dir = project_dir.clone();
        let projects_dir = projects_dir.clone();
        let policy = policy.clone();
        workers.push(std::thread::spawn(move || {
            project_ok(handle_resolve_or_register_project(
                &policy,
                &projects_dir,
                &project_request(
                    "resolve_or_register_project",
                    serde_json::json!({"path": project_dir.to_string_lossy()}),
                ),
            ))
        }));
    }
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results[0]["agent_project_id"],
        results[1]["agent_project_id"]
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result["registered"] == true)
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(&projects_dir).unwrap().count(), 1);
}

#[test]
fn auto_project_id_collision_extends_hash_without_overwriting() {
    use sha2::{Digest, Sha256};

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let other_dir = tmp.path().join("other");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(&other_dir).unwrap();
    std::fs::create_dir(&projects_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();
    // Match the Runner's project identity: raw bytes on Unix, normalized
    // (lowercased, `\\?\` stripped) on Windows.
    #[cfg(windows)]
    let identity = webcodex_agent_config::paths::normalize_path_identity(&canonical);
    #[cfg(not(windows))]
    let identity = canonical.to_string_lossy().to_string();
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let colliding_id = format!("repo-{}", &digest[..8]);
    let colliding_config = format!(
        "id = {:?}\npath = {:?}\n",
        colliding_id,
        other_dir.to_string_lossy()
    );
    std::fs::write(
        projects_dir.join(format!("{colliding_id}.toml")),
        &colliding_config,
    )
    .unwrap();

    let result = project_ok(handle_resolve_or_register_project(
        &project_policy(tmp.path()),
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    let generated = result["agent_project_id"].as_str().unwrap();
    assert_ne!(generated, colliding_id);
    assert_eq!(generated, format!("repo-{}", &digest[..12]));
    assert_eq!(
        std::fs::read_to_string(projects_dir.join(format!("{colliding_id}.toml"))).unwrap(),
        colliding_config
    );
}

#[test]
fn path_registration_publish_failure_leaves_no_config_or_temp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    webcodex_runner::projects::fail_next_project_publish_before_rename();

    let error = project_error_value(handle_resolve_or_register_project(
        &project_policy(tmp.path()),
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(error["error_kind"], "operation_failed");
    assert_eq!(error["state_changed"], false);
    assert_eq!(std::fs::read_dir(&projects_dir).unwrap().count(), 0);
}

#[test]
fn managed_temporary_project_is_registered_persistent_and_ordinary_project_compatible() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let policy = project_policy(tmp.path());
    let request = project_request(
        "create_project",
        serde_json::json!({
            "managed_temporary_project": true,
            "name": "Scratch task"
        }),
    );

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &request,
    ));
    let id = created["agent_project_id"].as_str().unwrap();
    let path = PathBuf::from(created["path"].as_str().unwrap());
    let canonical_root = temporary_root.canonicalize().unwrap();

    assert_eq!(created["source"], "managed_temporary");
    assert_eq!(created["kind"], "managed_temporary");
    assert_eq!(created["git_initialized"], true);
    assert_eq!(path.parent(), Some(canonical_root.as_path()));
    assert!(path.is_dir());
    assert!(path.join(".git").is_dir());
    assert_eq!(
        path.canonicalize().unwrap(),
        path,
        "the returned project path must be canonical"
    );

    let persisted = parse_agent_project_toml(
        &std::fs::read_to_string(projects_dir.join(format!("{id}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted.kind.as_deref(), Some("managed_temporary"));
    assert_eq!(persisted.path, path.to_string_lossy());

    // A fresh project-registry scan models a Runner restart: it finds the same
    // ordinary project record, including its source marker and canonical path.
    let reloaded = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, id);
    assert_eq!(reloaded[0].kind.as_deref(), Some("managed_temporary"));
    assert_eq!(reloaded[0].path, path.to_string_lossy());

    // The normal shell and structured project-overview paths receive the
    // registered path with no temporary-project special case.
    let shell = run_shell(
        &policy,
        &ShellConfig::default(),
        Some(path.to_string_lossy().as_ref()),
        &shell_echo("managed-shell"),
        None,
        10,
        None,
    );
    assert_eq!(shell.exit_code, Some(0), "{shell:?}");
    assert_eq!(shell.stdout.as_deref(), Some("managed-shell"));

    std::fs::write(path.join("README.md"), "managed\n").unwrap();
    let overview_request = json_file_op_request(
        &path,
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 1, "limit": 20}),
    );
    let overview = project_ok(handle_file_request(&policy, &overview_request));
    assert_eq!(overview["schema_version"], 1);
    assert!(overview.to_string().contains("README.md"));
}

#[test]
fn managed_temporary_project_rejects_path_traversal_and_never_overwrites_existing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let pre_existing = temporary_root.join("scratch");
    std::fs::create_dir(&pre_existing).unwrap();
    std::fs::write(pre_existing.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());

    let traversal = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "Scratch",
                "path": "../escape"
            }),
        ),
    ));
    assert_eq!(traversal, "invalid_request");
    assert!(!tmp.path().join("escape").exists());

    let path_like_name = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "../escape"
            }),
        ),
    ));
    assert_eq!(path_like_name, "invalid_request");

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "scratch"
            }),
        ),
    ));
    assert_ne!(
        created["path"].as_str(),
        Some(pre_existing.to_string_lossy().as_ref())
    );
    assert_eq!(
        std::fs::read_to_string(pre_existing.join("keep.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn managed_temporary_project_requires_root_inside_runner_policy() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let policy = project_policy(allowed.path());

    let error = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(outside.path()),
        &project_request(
            "create_project",
            serde_json::json!({"managed_temporary_project": true}),
        ),
    ));
    assert_eq!(error, "temporary_projects_root_outside_allowed_roots");
    assert!(!projects_dir.exists());
}

#[test]
fn create_post_rename_sync_failure_preserves_source_and_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let create_dir = tmp.path().join("created-after-rename");
    let policy = project_policy(tmp.path());
    webcodex_runner::projects::fail_next_project_parent_sync_after_rename();
    let error = project_err(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "create_project",
            serde_json::json!({
                "id":"indeterminate", "name":"Indeterminate",
                "description":"Preserve me", "path":create_dir.to_string_lossy(),
                "allow_patch":true, "template":"basic", "git_init":true
            }),
        ),
    ));
    assert_eq!(error, "operation_indeterminate");
    assert!(projects_dir.join("indeterminate.toml").is_file());
    assert!(create_dir.join("README.md").is_file());
    assert!(create_dir.join(".gitignore").is_file());
    assert!(create_dir.join(".git").is_dir());
}

#[test]
fn register_and_create_retries_converge_without_duplicate_side_effects() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let register_dir = tmp.path().join("existing");
    std::fs::create_dir(&register_dir).unwrap();
    let policy = project_policy(tmp.path());
    let register = project_request(
        "register_project",
        serde_json::json!({
            "id":"registered", "name":"Registered",
            "path":register_dir.to_string_lossy(), "allow_patch":true
        }),
    );
    let first = project_ok(handle_project_op(&policy, &projects_dir, &register));
    let retry = project_ok(handle_project_op(&policy, &projects_dir, &register));
    assert_eq!(retry["recovered"], true);
    assert_eq!(retry["changed"], false);
    assert_eq!(retry["revision"], first["revision"]);

    let create_dir = tmp.path().join("created");
    let create = project_request(
        "create_project",
        serde_json::json!({
            "id":"created", "name":"Created", "description":"Fixture",
            "path":create_dir.to_string_lossy(), "allow_patch":true,
            "template":"basic", "git_init":true,
            "allow_existing_empty":false
        }),
    );
    let created = project_ok(handle_project_op(&policy, &projects_dir, &create));
    let readme_before = std::fs::read(create_dir.join("README.md")).unwrap();
    let recovered = project_ok(handle_project_op(&policy, &projects_dir, &create));
    assert_eq!(recovered["recovered"], true);
    assert_eq!(recovered["changed"], false);
    assert_eq!(recovered["revision"], created["revision"]);
    assert_eq!(
        std::fs::read(create_dir.join("README.md")).unwrap(),
        readme_before
    );
    assert!(create_dir.join(".git").is_dir());

    let mismatch = project_err(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id":"registered", "name":"Different",
                "path":register_dir.to_string_lossy(), "allow_patch":true
            }),
        ),
    ));
    assert_eq!(mismatch, "project_already_exists");
}

#[test]
fn project_lifecycle_persists_state_and_unregister_preserves_source() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(project_dir.join(".git")).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());
    let registered = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy()
            }),
        ),
    ));
    let revision = registered["revision"].as_str().unwrap().to_string();

    let disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":revision}),
        ),
    ));
    assert_eq!(disabled["outcome"], "disabled");
    let retry_disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(retry_disabled["outcome"], "already_disabled");
    let disabled_revision = disabled["revision"].as_str().unwrap().to_string();
    let summaries = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].disabled);

    let stale = project_err(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(stale, "revision_conflict");

    let enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled_revision}),
        ),
    ));
    assert_eq!(enabled["outcome"], "enabled");
    let retry_enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled["revision"]}),
        ),
    ));
    assert_eq!(retry_enabled["outcome"], "already_enabled");

    let unregistered = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({
                "project_id":"demo",
                "expected_revision":enabled["revision"]
            }),
        ),
    ));
    assert_eq!(unregistered["outcome"], "unregistered");
    assert!(!projects_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());
    assert!(project_dir.join(".git").is_dir());

    let repeated = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(repeated["outcome"], "already_unregistered");

    let stale_tombstone = projects_dir.join(".demo.crash.toml.unregistering");
    std::fs::write(&stale_tombstone, "stale").unwrap();
    assert!(load_agent_project_summaries_from_dir(&projects_dir).is_empty());
    let recovered = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(recovered["outcome"], "already_unregistered");
    assert!(!stale_tombstone.exists());
}

#[test]
fn register_project_rejects_path_outside_allowed_roots() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let policy = project_policy(allowed.path());
    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "outside",
            "name": "Outside",
            "path": outside.path().to_string_lossy()
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "path_outside_allowed_roots");
    assert!(!projects_dir.join("outside.toml").exists());
}

#[test]
fn register_project_rejects_dangerous_subpaths_without_explicit_root() {
    let policy = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: Vec::new(),
        ..AgentPolicy::default()
    };

    // Dangerous system roots are platform-specific: the well-known Unix trees,
    // or the Windows OS trees (which must still be local-disk paths to reach
    // the dangerous-root check at all).
    #[cfg(windows)]
    let dangerous_paths: &[&str] = &[
        r"C:\Windows\System32\drivers\etc",
        r"C:\Program Files\WebCodex",
        r"C:\Program Files (x86)\something",
    ];
    #[cfg(not(windows))]
    let dangerous_paths: &[&str] = &[
        "/etc/nginx",
        "/usr/local",
        "/var/lib",
        "/proc/self",
        "/dev/shm",
    ];
    for path in dangerous_paths {
        let err = validate_project_path_policy(&policy, Path::new(path)).unwrap_err();
        assert!(err.contains("dangerous system root"), "{path}: {err}");
    }

    #[cfg(windows)]
    let safe_path = r"C:\Users\alice\projects";
    #[cfg(not(windows))]
    let safe_path = "/usr2/local";
    validate_project_path_policy(&policy, Path::new(safe_path)).unwrap();
}

#[test]
fn load_config_defaults_empty_allowed_roots_to_home() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(
            cfg.policy.allowed_roots,
            vec![home],
            "empty allowed_roots must default to HOME"
        );
    }
}

#[test]
fn load_config_defaults_allow_cwd_anywhere_to_false() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let base = "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n";

    // A config that omits `[policy]` entirely falls back to
    // `AgentPolicy::default()`; one that has `[policy]` without the field
    // falls back to the per-field serde default. Both must fail closed —
    // otherwise the agent runs with no filesystem boundary at all.
    for (label, body) in [
        ("no [policy] section", base.to_string()),
        (
            "[policy] without allow_cwd_anywhere",
            format!("{base}\n[policy]\nallow_raw_shell = true\n"),
        ),
    ] {
        let path = tmp.path().join("agent.toml");
        std::fs::write(&path, body).unwrap();
        let cfg = load_config(&path).unwrap();
        assert!(
            !cfg.policy.allow_cwd_anywhere,
            "{label}: allow_cwd_anywhere must default to false"
        );
    }
}

#[test]
fn default_policy_denies_paths_outside_allowed_roots() {
    // The shipped default must not resolve an absolute path outside the
    // configured roots. `AgentPolicy::default()` has no roots at all, so
    // every path is out of bounds.
    let policy = AgentPolicy::default();
    assert!(!policy.allow_cwd_anywhere);
    let err = resolve_requested_path(&policy, Some("/tmp"), "/etc/passwd")
        .expect_err("default policy must not reach /etc/passwd");
    assert!(err.contains("outside allowed_roots"), "{err}");

    // With HOME as the root — what `effective_allowed_roots` fills in — a
    // path inside the root still resolves, so the default is restrictive
    // rather than broken.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::write(root.join("in-bounds.txt"), "ok").unwrap();
    let scoped = AgentPolicy {
        allowed_roots: vec![root.clone()],
        ..AgentPolicy::default()
    };
    resolve_requested_path(&scoped, Some(root.to_str().unwrap()), "in-bounds.txt")
        .expect("in-bounds path must still resolve under the fail-closed default");
}

#[test]
fn load_config_explicit_allowed_roots_override_home_default() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n[policy]\nallowed_roots = [\"/root/git\"]\n",
        )
        .unwrap();
    let cfg = load_config(&path).unwrap();
    assert_eq!(
        cfg.policy.allowed_roots,
        vec![PathBuf::from("/root/git")],
        "explicit allowed_roots must override the HOME default"
    );
}

#[test]
fn load_config_empty_roots_without_home_and_no_cwd_anywhere_errors() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    // Windows derives the allowed-root default from USERPROFILE, so both
    // home sources must be absent to exercise the fail-closed branch.
    let _env = EnvGuard::new()
        .remove("HOME")
        .remove("USERPROFILE")
        .remove("APPDATA");
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n\
             [policy]\nallow_cwd_anywhere = false\n",
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("allowed_roots is empty"));
}

#[test]
fn register_project_overwrite_semantics_are_accurate() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let payload = |overwrite| {
        serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "path": project_dir.to_string_lossy(),
            "overwrite": overwrite
        })
    };

    let first = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(false)),
    ));
    assert_eq!(first["created_config"], true);
    assert_eq!(first["overwritten"], false);

    let retry = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(false)),
    ));
    assert_eq!(retry["recovered"], true);
    assert_eq!(retry["changed"], false);

    let overwritten = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(true)),
    ));
    assert_eq!(overwritten["created_config"], false);
    assert_eq!(overwritten["overwritten"], true);
}

#[test]
fn create_project_basic_creates_readme_and_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("new-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "basic",
            "name": "Basic",
            "path": project_dir.to_string_lossy(),
            "description": "Basic template",
            "template": "basic"
        }),
    );

    let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(value["created_directory"], true);
    assert!(project_dir.join("README.md").exists());
    assert!(project_dir.join(".gitignore").exists());
    assert!(std::fs::read_to_string(project_dir.join("README.md"))
        .unwrap()
        .contains("Basic template"));
}

#[test]
fn create_project_rejects_existing_non_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let keep = project_dir.join("keep.txt");
    std::fs::write(&keep, "keep").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "existing",
            "name": "Existing",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "path_not_empty");
    assert_eq!(std::fs::read_to_string(keep).unwrap(), "keep");
}

#[test]
fn create_project_rejects_unknown_template() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("new-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "badtemplate",
            "name": "Bad Template",
            "path": project_dir.to_string_lossy(),
            "template": "cargo"
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "invalid_request");
    assert!(!project_dir.exists());
}

#[test]
fn create_project_created_config_and_overwritten_semantics_are_accurate() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("empty-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let payload = |overwrite| {
        serde_json::json!({
            "id": "empty",
            "name": "Empty",
            "path": project_dir.to_string_lossy(),
            "template": "empty",
            "allow_existing_empty": true,
            "overwrite": overwrite
        })
    };

    let first = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("create_project", payload(false)),
    ));
    assert_eq!(first["created_directory"], true);
    assert_eq!(first["created_config"], true);
    assert_eq!(first["overwritten"], false);

    let second = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("create_project", payload(true)),
    ));
    assert_eq!(second["created_directory"], false);
    assert_eq!(second["created_config"], false);
    assert_eq!(second["overwritten"], true);
}

#[test]
fn create_project_cleanup_removes_only_files_created_on_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing-empty");
    std::fs::create_dir(&project_dir).unwrap();
    let projects_dir_file = tmp.path().join("projects.d-is-file");
    std::fs::write(&projects_dir_file, "not a dir").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "cleanup",
            "name": "Cleanup",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
    assert_eq!(err, "operation_failed");
    assert!(project_dir.exists());
    assert!(!project_dir.join("README.md").exists());
    assert!(!project_dir.join(".gitignore").exists());
}

#[test]
fn create_project_does_not_delete_pre_existing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing");
    std::fs::create_dir(&project_dir).unwrap();
    let pre_existing = project_dir.join("pre-existing.txt");
    std::fs::write(&pre_existing, "original").unwrap();
    let projects_dir_file = tmp.path().join("projects.d-is-file");
    std::fs::write(&projects_dir_file, "not a dir").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "keep",
            "name": "Keep",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
    assert_eq!(err, "path_not_empty");
    assert_eq!(std::fs::read_to_string(pre_existing).unwrap(), "original");
}

#[test]
fn agent_project_cache_invalidate_refreshes_after_project_op() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let mut cfg = test_config(projects_dir.clone());
    cfg.policy = project_policy(tmp.path());
    let mut cache = AgentProjectCache::default();
    assert!(cache.get(&cfg).is_empty());

    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "cached",
            "name": "Cached",
            "path": project_dir.to_string_lossy()
        }),
    );
    project_ok(handle_project_op(&cfg.policy, &projects_dir, &req));

    assert!(
        cache.get(&cfg).is_empty(),
        "cache should still be stale before invalidation"
    );
    cache.invalidate();
    let projects = cache.get(&cfg);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "cached");
}

#[test]
fn http_sink_client_id_matches_config() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let client = Client::new();
    let sink = AgentSink::Http(HttpSendConfig {
        client,
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: "inst-1".to_string(),
        shutdown: Arc::new(AtomicBool::new(false)),
    });
    assert_eq!(sink.client_id(), "oe");
    assert_eq!(sink.agent_instance_id(), "inst-1");
}

#[test]
fn empty_tokens_are_not_sent_as_credentials() {
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "").unwrap();
    assert!(request.headers().get(AUTHORIZATION).is_none());

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "   \t").unwrap();
    assert!(request.headers().get(AUTHORIZATION).is_none());

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "  abc123  ").unwrap();
    assert_eq!(
        request.headers().get(AUTHORIZATION).unwrap(),
        "Bearer abc123"
    );

    assert_eq!(non_empty_token(""), None);
    assert_eq!(non_empty_token("   \t"), None);
    assert_eq!(non_empty_token("  abc123  "), Some("abc123".to_string()));
}

#[test]
fn empty_tokens_http_register_omits_authorization_header() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 16 * 1024];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(
            request.starts_with("POST /api/shell/agent/register "),
            "unexpected request: {request}"
        );
        assert!(
            !request.to_ascii_lowercase().contains("authorization:"),
            "empty token must not send Authorization header: {request}"
        );
        let body = r#"{"success":true,"client":null,"error":null}"#;
        write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
    });

    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("projects.d"));
    cfg.server_url = format!("http://{}", addr);
    cfg.token = "   \t".to_string();

    let client = Client::builder().no_proxy().build().unwrap();
    let mut project_cache = AgentProjectCache::default();
    let runtime = ReloadableAgentConfig::new(cfg.clone(), PathBuf::new());
    register(
        &client,
        &cfg,
        &runtime,
        &mut project_cache,
        None,
        "inst-empty-token",
        0,
        &JobManager::new(1),
    )
    .unwrap();
    server.join().unwrap();
}

// ------------------------------------------------------------------------
// WebSocket session: Pong must be handled as keepalive, not unexpected
// ------------------------------------------------------------------------

#[tokio::test]
async fn websocket_session_accepts_pong_without_error_or_disconnect() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    // Minimal WS server. It:
    //   1. reads the agent's Register,
    //   2. sends a Registered ack,
    //   3. sends a Pong (the frame that previously triggered the noisy
    //      "ignoring unexpected envelope: pong" path),
    //   4. sends a Ping and waits for the agent's Pong reply — if the
    //      agent had exited on the Pong in step 3 it would never reply,
    //      and this receive would time out (failing the test),
    //   5. drops the socket so the agent's session returns cleanly.
    //
    // This both guards the "Pong is not unexpected" regression and proves
    // the session stays alive after a Pong.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

        // Read Register.
        let reg_msg = ws.next().await.unwrap().unwrap();
        let reg_env = AgentEnvelope::from_slice(reg_msg.into_text().unwrap().as_bytes()).unwrap();
        assert!(matches!(reg_env, AgentEnvelope::Register { .. }));

        // Ack register.
        let ack = AgentEnvelope::Registered {
            success: true,
            client: None,
            error: None,
        };
        ws.send(WsMessage::Text(ack.to_json().unwrap().into()))
            .await
            .unwrap();

        // Send a Pong — the agent must accept it as keepalive and stay
        // connected (this is the regression we are guarding against).
        let pong = AgentEnvelope::Pong { ts: 42 };
        ws.send(WsMessage::Text(pong.to_json().unwrap().into()))
            .await
            .unwrap();

        // Probe liveness: send a Ping and expect a Pong reply. If the
        // agent had broken out of its read loop on the Pong above, this
        // would time out.
        ws.send(WsMessage::Text(
            AgentEnvelope::Ping { ts: 7 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("agent did not reply to ping after pong (session exited on pong)")
            .expect("stream open")
            .expect("ok message");
        match AgentEnvelope::from_slice(reply.into_text().unwrap().as_bytes()).unwrap() {
            AgentEnvelope::Pong { ts } => assert_eq!(ts, 7),
            other => panic!("expected pong reply, got {:?}", other.kind()),
        }

        // Drop the socket; the agent's reader will error/EOF and the
        // session returns cleanly. Avoids a close-handshake that can hang
        // on a current-thread test runtime.
        drop(ws);
    });

    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.server_url = format!("http://{}", addr);
    cfg.transport = Some(TRANSPORT_WEBSOCKET.to_string());
    let runtime = AgentRuntimeState::new(&cfg, PathBuf::new());

    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        websocket_session(&cfg, Vec::new(), "inst-1", &runtime),
    )
    .await
    .expect("websocket_session did not complete in time");

    // The session must end (server dropped the socket) and must NOT have
    // returned an error — a Pong is normal keepalive traffic.
    assert!(
        outcome.is_ok(),
        "websocket_session errored on Pong (regression): {:?}",
        outcome
    );

    server_task.await.unwrap();
}
