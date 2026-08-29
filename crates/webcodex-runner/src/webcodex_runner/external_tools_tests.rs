use super::*;
use crate::shell_protocol::ShellAgentShellRequest;
use std::env;
use std::fs;
use std::process::Command;
use std::sync::{Arc, OnceLock, Weak};
use tempfile::TempDir;

#[path = "external_tools/experimental_tests.rs"]
mod experimental_tests;

/// Routing tests that operate inside the fixture root and are not about the
/// filesystem boundary. `RunnerPolicy::default()` is fail-closed, so these opt
/// out explicitly; `router_rejects_absolute_parent_and_symlink_escape_paths`
/// deliberately keeps the default to assert the boundary still rejects.
fn permissive_test_policy() -> RunnerPolicy {
    RunnerPolicy {
        allow_cwd_anywhere: true,
        ..RunnerPolicy::default()
    }
}

static FAKE_SERVER: OnceLock<Mutex<Weak<FakeBinary>>> = OnceLock::new();

// Toolhelp thread snapshots are a system-wide resource. Running many
// CREATE_SUSPENDED fake MCP fixtures concurrently on Windows can consume the
// intentionally tight one-second protocol budget even though each fixture is
// correct in isolation. Serialize only these test functions; concurrency
// exercised inside an individual test remains unchanged.
#[cfg(windows)]
static FAKE_MCP_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
fn serialize_fake_mcp_test() -> std::sync::MutexGuard<'static, ()> {
    FAKE_MCP_TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(not(windows))]
struct FakeMcpTestSerialGuard;

#[cfg(not(windows))]
fn serialize_fake_mcp_test() -> FakeMcpTestSerialGuard {
    FakeMcpTestSerialGuard
}

struct FakeBinary {
    _temp: TempDir,
    path: PathBuf,
}

fn fake_binary() -> Arc<FakeBinary> {
    let cache = FAKE_SERVER.get_or_init(|| Mutex::new(Weak::new()));
    let mut cached = cache.lock().unwrap();
    if let Some(binary) = cached.upgrade() {
        return binary;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join(format!(
        "webcodex-claude-mcp-fake{}",
        env::consts::EXE_SUFFIX
    ));
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/webcodex_runner/fake_claude_mcp.rs");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let result = Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_claude_mcp_fake")
        .arg(source)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let binary = Arc::new(FakeBinary {
        _temp: temp,
        path: output,
    });
    *cached = Arc::downgrade(&binary);
    binary
}

struct Fixture {
    provider: ClaudeCodeMcpProvider,
    config: ClaudeCodeMcpConfig,
    _fake: Arc<FakeBinary>,
    _temp: TempDir,
    root: PathBuf,
    marker: PathBuf,
}

impl Fixture {
    fn new(scenario: &str) -> Self {
        // The fake server binary is compiled on first use and spawned fresh;
        // under parallel full-suite load a 1s MCP timeout flakes. The
        // timeout-specific tests pass an explicit short timeout.
        Self::with_timeout(scenario, 10)
    }

    fn with_timeout(scenario: &str, timeout_secs: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let requested_root = temp.path().join("project");
        fs::create_dir_all(requested_root.join("src")).unwrap();
        // Production request routing canonicalizes the project root before it
        // reaches the provider. Mirror that here so macOS `/var` -> `/private/var`
        // aliases cannot make fake absolute search output look out-of-project.
        let root = fs::canonicalize(requested_root).unwrap();
        fs::write(root.join("edit.txt"), "before\n").unwrap();
        fs::write(root.join("src/lib.rs"), "zero\nneedle\n").unwrap();
        let marker = temp.path().join("marker.log");
        let fake = fake_binary();
        let config = ClaudeCodeMcpConfig {
            enabled: true,
            command: fake.path.to_string_lossy().to_string(),
            args: vec![scenario.to_string(), marker.to_string_lossy().to_string()],
            mapping: HashMap::from([(
                "search_project_text".to_string(),
                "fake_search".to_string(),
            )]),
            timeout_secs,
        };
        let provider = ClaudeCodeMcpProvider::new(config.clone());
        Self {
            provider,
            config,
            _fake: fake,
            _temp: temp,
            root,
            marker,
        }
    }

    fn context<'a>(&'a self, path: &'a str) -> ToolExecutionContext<'a> {
        ToolExecutionContext {
            project_root: &self.root,
            target: self.root.join(path),
            max_output_bytes: MAX_MCP_OUTPUT_BYTES,
            timeout_secs: self.config.timeout_secs,
        }
    }

    fn starts(&self) -> usize {
        fs::read_to_string(&self.marker)
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "start")
            .count()
    }
}

fn search_request() -> Value {
    json!({
        "pattern": "needle",
        "path": ".",
        "limit": 20,
        "context_before": 0,
        "context_after": 0,
        "include_globs": [],
        "exclude_globs": [],
        "result_mode": "matches",
    })
}

#[test]
fn search_argument_mapping_preserves_regex_and_escapes_literal_patterns() {
    let fixture = Fixture::new("normal");
    let context = fixture.context(".");

    let regex = search_request();
    let regex_args = build_arguments(ProviderCapability::SearchProjectText, &regex, &context)
        .expect("regex search arguments");
    assert_eq!(regex_args["pattern"], "needle");

    let mut literal = search_request();
    literal["pattern"] = json!("RuntimeInfo { value.* }");
    literal["pattern_mode"] = json!("literal");
    let literal_args = build_arguments(ProviderCapability::SearchProjectText, &literal, &context)
        .expect("literal search arguments");
    assert_eq!(literal_args["pattern"], r"RuntimeInfo \{ value\.\* \}");
}

fn call_search(fixture: &Fixture) -> Result<Value, ProviderError> {
    fixture.provider.call(
        ProviderCapability::SearchProjectText,
        search_request(),
        fixture.context("."),
    )
}

fn pending_count(provider: &ClaudeCodeMcpProvider) -> usize {
    provider
        .projects
        .lock()
        .unwrap()
        .values()
        .map(|client| client.connection.pending.lock().unwrap().len())
        .sum()
}

fn process_ids(provider: &ClaudeCodeMcpProvider) -> Vec<u32> {
    provider
        .projects
        .lock()
        .unwrap()
        .values()
        .map(|client| client.connection.child.lock().unwrap().id())
        .collect()
}

fn agent_request(
    kind: &str,
    root: &Path,
    path: &str,
    content: Option<Value>,
) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "request".to_string(),
        client_id: "client".to_string(),
        kind: kind.to_string(),
        job_id: None,
        cwd: Some(root.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: content.map(|value| value.to_string()),
        max_bytes: Some(MAX_MCP_OUTPUT_BYTES),
        expected_sha256: None,
        expected_prefix: None,
        start_line: Some(1),
        end_line: Some(20),
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: None,
        // The effective MCP deadline is min(request, policy, config); a 1s
        // request pin would ignore the fixture's generous spawn budget and
        // flake under parallel load (first call races server startup).
        timeout_secs: 10,
        requested_by: "test".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
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

fn schema_fields(tool: &DiscoveredTool) -> Vec<String> {
    let mut fields = tool
        .fields
        .iter()
        .map(|name| sanitize_name(name))
        .collect::<Vec<_>>();
    fields.sort();
    fields.truncate(32);
    fields
}

fn discovery_inventory(client: &ProjectMcpClient) -> Value {
    let mut tools = client
        .tools
        .iter()
        .map(|(name, tool)| {
            json!({"name": sanitize_name(name), "schema_fields": schema_fields(tool)})
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    tools.truncate(32);
    Value::Array(tools)
}

fn real_tool_name(
    client: &ProjectMcpClient,
    capability: ProviderCapability,
    env_key: &str,
) -> Result<String, String> {
    if let Ok(name) = env::var(env_key) {
        let Some(tool) = client.tools.get(&name) else {
            return Err(format!(
                "{env_key} selected {:?}, but discovered tools were {}",
                sanitize_name(&name),
                discovery_inventory(client)
            ));
        };
        let fields = schema_fields(tool);
        let missing = required_fields(capability)
            .iter()
            .filter(|field| !fields.iter().any(|actual| actual == **field))
            .collect::<Vec<_>>();
        return if missing.is_empty() {
            Ok(name)
        } else {
            Err(format!(
                "{env_key} selected {:?}; missing schema fields {:?}; actual fields {:?}",
                sanitize_name(&name),
                missing,
                fields
            ))
        };
    }
    let needle = "grep";
    let candidates = client
        .tools
        .iter()
        .filter(|(name, tool)| {
            name.to_ascii_lowercase().contains(needle)
                && required_fields(capability)
                    .iter()
                    .all(|field| tool.fields.contains(*field))
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        Ok(candidates[0].clone())
    } else {
        Err(format!(
            "expected one schema-compatible {needle} tool with fields {:?}; candidates {:?}; discovery {}",
            required_fields(capability),
            candidates,
            discovery_inventory(client)
        ))
    }
}

#[test]
fn provider_is_disabled_by_default_and_missing_command_is_nonfatal() {
    let _serial = serialize_fake_mcp_test();
    let parsed: ToolProvidersConfig = toml::from_str(
        r#"
strategy = "claude_code_then_native"
[claude_code]
enabled = true
[claude_code.mapping]
search_project_text = "project_search"
"#,
    )
    .unwrap();
    assert_eq!(parsed.strategy, ToolProviderStrategy::ClaudeCodeThenNative);
    assert_eq!(
        parsed.claude_code.mapping["search_project_text"],
        "project_search"
    );

    let disabled = ClaudeCodeMcpProvider::new(ClaudeCodeMcpConfig::default());
    assert!(!disabled.status().available);

    let missing = ClaudeCodeMcpConfig {
        enabled: true,
        command: "/definitely/missing/claude".to_string(),
        ..Default::default()
    };
    let provider = ClaudeCodeMcpProvider::new(missing);
    assert!(!provider.status().available);

    let root = tempfile::tempdir().unwrap();
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCode,
        claude_code: ClaudeCodeMcpConfig::default(),
    });
    let mut request = agent_request("run_shell", root.path(), ".", None);
    request.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    request.stdin = Some(search_request().to_string());
    let ExternalRoute::Handled(result) = router.route(&permissive_test_policy(), &request) else {
        panic!("disabled provider routed to native");
    };
    assert!(result.stdout.unwrap().contains("claude_code_unavailable"));
}

#[test]
fn status_reports_discovery_mapping_process_and_bounded_error() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    assert_eq!(fixture.provider.status().process_state, "not_started");
    let output = call_search(&fixture).unwrap();
    assert!(output.as_str().unwrap().contains("src/lib.rs:2:needle"));
    let status = fixture.provider.status();
    assert_eq!(status.version.as_deref(), Some("Claude Fake 1.2.3"));
    assert_eq!(status.process_state, "running");
    assert_eq!(
        status.discovered_tool_names,
        [
            "Bash",
            "Edit",
            "Read",
            "TaskCreate",
            "Write",
            "fake_edit",
            "fake_search"
        ]
    );
    assert_eq!(status.capabilities["search_project_text"], "available");
    assert_eq!(status.last_error_code, None);
    assert!(status
        .discovered_tool_names
        .iter()
        .all(|name| name.chars().count() <= 120));
    let serialized = serde_json::to_string(&status).unwrap();
    let root_text = fixture.root.to_string_lossy();
    let marker_text = fixture.marker.to_string_lossy();
    for forbidden in [
        root_text.as_ref(),
        marker_text.as_ref(),
        fixture.config.command.as_str(),
        "stderr",
        "environment",
        "token",
        "cookie",
    ] {
        assert!(!serialized.contains(forbidden), "status leaked {forbidden}");
    }
    let mut mismatched = fixture.config.clone();
    mismatched
        .mapping
        .insert("search_project_text".to_string(), "fake_edit".to_string());
    let mismatched = ClaudeCodeMcpProvider::new(mismatched);
    mismatched
        .project_client(&fixture.root, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let status = mismatched.status();
    assert_eq!(
        status.capabilities["search_project_text"],
        "schema_mismatch"
    );
    let marker = fs::read_to_string(&fixture.marker).unwrap();
    assert!(marker.contains(r#""method":"tools/list""#));
}

#[test]
fn search_mapping_normalizes_results() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCode,
        claude_code: fixture.config.clone(),
    });
    let mut search = agent_request("run_shell", &fixture.root, ".", None);
    search.command = format!("{EXTERNAL_SEARCH_REQUEST_PREFIX}\nignored native command");
    search.stdin = Some(search_request().to_string());
    let ExternalRoute::Handled(search) = router.route(&permissive_test_policy(), &search) else {
        panic!("search routed to native");
    };
    assert!(search.stdout.unwrap().contains("src/lib.rs:2:needle"));
    assert!(fs::read_to_string(&fixture.marker)
        .unwrap()
        .contains(r#""output_mode":"content""#));

    let status = router.status();
    let call = status.claude_code.last_call.unwrap();
    assert_eq!(call.capability, "search_project_text");
    assert_eq!(call.selected_provider, "claude_code");
    assert!(!call.fallback_used);
    assert_eq!(call.result, "success");
    assert_eq!(call.write_state, None);
    assert_eq!(call.error_code, None);
}

#[test]
fn fallback_and_failure_routes_record_bounded_last_call_evidence() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let mut unmapped_search = fixture.config.clone();
    unmapped_search.mapping.remove("search_project_text");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCodeThenNative,
        claude_code: unmapped_search,
    });
    let mut search = agent_request("run_shell", &fixture.root, ".", None);
    search.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    search.stdin = Some(search_request().to_string());
    let ExternalRoute::NativeFallback(fallback) = router.route(&permissive_test_policy(), &search)
    else {
        panic!("unmapped search did not request Native fallback");
    };
    router.complete_native_fallback(
        fallback,
        &CommandResult {
            exit_code: Some(0),
            stdout: Some("native search".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        },
    );
    let status = router.status();
    let call = status.claude_code.last_call.unwrap();
    assert_eq!(call.selected_provider, "native");
    assert!(call.fallback_used);
    assert_eq!(call.result, "success");
    assert_eq!(call.write_state, None);
    assert_eq!(call.error_code, None);
    assert_eq!(
        status.claude_code.last_error_code.as_deref(),
        Some("provider_capability_unavailable")
    );
}

#[test]
fn status_revisions_are_changed_only_and_registration_reads_latest_snapshot() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCode,
        claude_code: fixture.config.clone(),
    });
    let (_, initial_revision) = router.registration_status();
    router.mark_status_reported(initial_revision);
    assert!(router.claim_status_update().is_none());

    let mut search = agent_request("run_shell", &fixture.root, ".", None);
    search.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    search.stdin = Some(search_request().to_string());
    assert!(matches!(
        router.route(&permissive_test_policy(), &search),
        ExternalRoute::Handled(_)
    ));
    let (update, revision) = router.claim_status_update().unwrap();
    assert_eq!(
        update.claude_code.last_call.as_ref().unwrap().capability,
        "search_project_text"
    );
    assert!(
        router.claude.state.status.try_lock().is_ok(),
        "status claim retained the Provider lock across transport send"
    );
    assert!(router.claim_status_update().is_none());
    // A newer state cannot overtake an already claimed snapshot. Once the
    // first snapshot is reported, the next claim observes the newer revision.
    router
        .claude
        .record_error(&ProviderError::new("mcp_protocol_error"));
    assert!(router.claim_status_update().is_none());
    router.mark_status_reported(revision);
    let (_, newer_revision) = router.claim_status_update().unwrap();
    assert!(newer_revision > revision);
    // A failed best-effort metadata send releases only the status claim; the
    // already computed state remains available for retry.
    router.release_status_update(newer_revision);
    assert!(router.claim_status_update().is_some());

    let (registered, latest_revision) = router.registration_status();
    assert!(latest_revision > initial_revision);
    assert_eq!(registered.claude_code.process_state, "running");
    assert!(registered.claude_code.last_call.is_some());
}

#[test]
fn router_rejects_absolute_parent_and_symlink_escape_paths() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCode,
        claude_code: fixture.config.clone(),
    });
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.txt"), "before").unwrap();
    let cases = [
        "../outside.txt".to_string(),
        outside
            .path()
            .join("outside.txt")
            .to_string_lossy()
            .to_string(),
    ];
    for path in cases {
        let mut search = agent_request("run_shell", &fixture.root, ".", None);
        search.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
        search.stdin = Some(search_request_with_path(&path).to_string());
        let ExternalRoute::Handled(result) = router.route(&RunnerPolicy::default(), &search) else {
            panic!("unsafe path routed to native");
        };
        let output: Value = serde_json::from_str(result.stdout.as_deref().unwrap()).unwrap();
        assert_eq!(output["code"], "provider_path_rejected");
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), fixture.root.join("escape")).unwrap();
        let mut search = agent_request("run_shell", &fixture.root, ".", None);
        search.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
        search.stdin = Some(search_request_with_path("escape/outside.txt").to_string());
        let ExternalRoute::Handled(result) = router.route(&RunnerPolicy::default(), &search) else {
            panic!("symlink escape routed to native");
        };
        assert!(result.stdout.unwrap().contains("provider_path_rejected"));
    }
}

fn search_request_with_path(path: &str) -> Value {
    let mut request = search_request();
    request["path"] = json!(path);
    request
}

#[test]
fn protocol_failures_are_bounded_and_unknown_ids_are_ignored() {
    let _serial = serialize_fake_mcp_test();
    for (scenario, expected) in [
        ("invalid_json", "mcp_invalid_json"),
        ("oversized", "mcp_message_too_large"),
    ] {
        let fixture = Fixture::new(scenario);
        let error = call_search(&fixture).unwrap_err();
        assert_eq!(error.code, expected);
        assert_eq!(pending_count(&fixture.provider), 0);
        assert_eq!(
            fixture.provider.status().last_error_code.as_deref(),
            Some(expected)
        );
    }

    let fixture = Fixture::new("unknown_id");
    assert!(call_search(&fixture).is_ok());

    let fixture = Fixture::new("server_request");
    assert!(call_search(&fixture).is_ok());
    assert!(fs::read_to_string(&fixture.marker)
        .unwrap()
        .contains("server_request_error_received"));
}

#[test]
fn process_exit_clears_pending_and_next_call_restarts_lazily() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("restart_once");
    assert!(call_search(&fixture).is_err());
    assert!(wait_until(Duration::from_secs(1), || {
        pending_count(&fixture.provider) == 0
            && fixture.provider.status().process_state == "stopped"
    }));
    assert!(call_search(&fixture).is_ok());
    assert_eq!(fixture.starts(), 2);
    assert_eq!(fixture.provider.status().process_state, "running");
    fixture.provider.shutdown();
    let status = fixture.provider.status();
    assert_eq!(status.process_state, "stopped");
    assert!(!status.available);
}

#[test]
fn timeout_removes_pending_request() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("timeout");
    let error = call_search(&fixture).unwrap_err();
    assert_eq!(error.code, "mcp_request_timeout");
    assert_eq!(pending_count(&fixture.provider), 0);
    let status = fixture.provider.status();
    assert_eq!(status.process_state, "stopped");
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("mcp_request_timeout")
    );

    // An unmapped capability with claude_code_then_native requests a Native
    // fallback instead of a hard provider error.
    let mut unmapped = fixture.config.clone();
    unmapped.mapping.remove("search_project_text");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCodeThenNative,
        claude_code: unmapped,
    });
    let mut request = agent_request("run_shell", &fixture.root, ".", None);
    request.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    request.stdin = Some(search_request().to_string());
    assert!(matches!(
        router.route(&permissive_test_policy(), &request),
        ExternalRoute::NativeFallback(_)
    ));
}

#[cfg(unix)]
#[test]
fn provider_shutdown_reaps_a_normally_terminating_process_once() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let client = fixture
        .provider
        .project_client(&fixture.root, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let pid = client.connection.child.lock().unwrap().id();

    let outcome = fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_millis(500));
    assert_eq!(outcome.connections, 1);
    assert_eq!(outcome.timed_out, 0);
    assert!(
        wait_until(Duration::from_secs(1), || !process_exists(pid)),
        "provider process remained after shutdown"
    );

    let repeated = Instant::now();
    let second = fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_millis(500));
    assert_eq!(second.connections, 0);
    assert!(
        repeated.elapsed() < Duration::from_millis(100),
        "idempotent provider shutdown re-armed a wait"
    );
}

#[cfg(unix)]
#[test]
fn unresponsive_provider_is_killed_reaped_and_wakes_pending_request() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::with_timeout("ignore_term", 5);
    let client = fixture
        .provider
        .project_client(&fixture.root, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let connection = Arc::clone(&client.connection);
    let pid = connection.child.lock().unwrap().id();
    let request_connection = Arc::clone(&connection);
    let request = std::thread::spawn(move || {
        request_connection.request(
            "tools/call",
            json!({"name":"fake_search","arguments":{}}),
            Duration::from_secs(5),
            WriteState::NotSubmitted,
        )
    });
    assert!(wait_until(Duration::from_secs(1), || {
        lock_unpoison(&connection.pending).len() == 1
    }));

    let started = Instant::now();
    let outcome = fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_millis(600));
    let elapsed = started.elapsed();
    assert_eq!(outcome.connections, 1);
    assert!(
        elapsed < Duration::from_millis(900),
        "provider shutdown exceeded its shared deadline: {elapsed:?}"
    );
    let error = request.join().unwrap().unwrap_err();
    assert_eq!(error.code, "mcp_connection_closed");
    assert_eq!(lock_unpoison(&connection.pending).len(), 0);
    assert!(
        wait_until(Duration::from_secs(1), || !process_exists(pid)),
        "SIGTERM-ignoring provider process survived SIGKILL"
    );
}

#[cfg(unix)]
#[test]
fn provider_request_timeout_racing_shutdown_is_idempotent() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::with_timeout("ignore_term", 1);
    let client = fixture
        .provider
        .project_client(&fixture.root, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let connection = Arc::clone(&client.connection);
    let pid = connection.child.lock().unwrap().id();
    let request_connection = Arc::clone(&connection);
    // Hold stdin until the request has registered its pending sender. The
    // request timeout starts only after the write completes, so this gives the
    // test a deterministic pre-timeout synchronization point without changing
    // the timeout-vs-shutdown race under test.
    let stdin_guard = lock_unpoison(&connection.stdin);
    let request = std::thread::spawn(move || {
        request_connection.request(
            "tools/call",
            json!({"name":"fake_search","arguments":{}}),
            Duration::from_millis(100),
            WriteState::NotSubmitted,
        )
    });
    assert!(wait_until(Duration::from_secs(1), || {
        lock_unpoison(&connection.pending).len() == 1
    }));
    drop(stdin_guard);
    std::thread::sleep(Duration::from_millis(80));

    let outcome = fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_millis(600));
    assert_eq!(outcome.connections, 1);
    let error = request.join().unwrap().unwrap_err();
    assert!(
        matches!(error.code, "mcp_request_timeout" | "mcp_connection_closed"),
        "{}",
        error.code
    );
    assert!(
        wait_until(Duration::from_secs(1), || !process_exists(pid)),
        "provider process survived concurrent timeout and shutdown"
    );
    let repeated = Instant::now();
    fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_millis(600));
    assert!(repeated.elapsed() < Duration::from_millis(100));
}

#[test]
fn native_strategy_does_not_start_claude() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::new("normal");
    let router = ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::Native,
        claude_code: fixture.config.clone(),
    });
    let mut request = agent_request("run_shell", &fixture.root, ".", None);
    request.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    request.stdin = Some(search_request().to_string());
    assert!(matches!(
        router.route(&permissive_test_policy(), &request),
        ExternalRoute::Native
    ));
    assert_eq!(fixture.starts(), 0);
}

#[cfg(unix)]
#[test]
fn retiring_router_keeps_inflight_search_alive_then_reaps_its_process() {
    let _serial = serialize_fake_mcp_test();
    let fixture = Fixture::with_timeout("delayed", 2);
    let old = Arc::new(ExternalToolRouter::new(&ToolProvidersConfig {
        strategy: ToolProviderStrategy::ClaudeCode,
        claude_code: fixture.config.clone(),
    }));
    let weak = Arc::downgrade(&old);
    let mut request = agent_request("run_shell", &fixture.root, ".", None);
    request.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    request.stdin = Some(search_request().to_string());
    let worker_router = Arc::clone(&old);
    let worker = std::thread::spawn(move || {
        assert!(matches!(
            worker_router.route(&permissive_test_policy(), &request),
            ExternalRoute::Handled(_)
        ));
    });
    assert!(wait_until(Duration::from_secs(1), || {
        fs::read_to_string(&fixture.marker)
            .unwrap_or_default()
            .contains(r#""method":"tools/call""#)
    }));
    let pid = process_ids(&old.claude)[0];

    let replacement = ExternalToolRouter::new(&ToolProvidersConfig::default());
    let mut new_request = agent_request("run_shell", &fixture.root, ".", None);
    new_request.command = EXTERNAL_SEARCH_REQUEST_PREFIX.to_string();
    new_request.stdin = Some(search_request().to_string());
    assert!(matches!(
        replacement.route(&permissive_test_policy(), &new_request),
        ExternalRoute::Native
    ));
    drop(old);
    assert!(
        weak.upgrade().is_some(),
        "in-flight search lost its old router"
    );
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);

    worker.join().unwrap();
    assert!(wait_until(Duration::from_secs(1), || weak
        .upgrade()
        .is_none()));
    assert!(wait_until(Duration::from_secs(1), || {
        (unsafe { libc::kill(pid as i32, 0) }) == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }));
}

#[test]
fn shutdown_reaps_descendant_and_stdout_closes_without_leaks() {
    let _serial = serialize_fake_mcp_test();
    // The fake server spawns a descendant that inherits the piped stdout,
    // then the direct child exits immediately. This exercises the core
    // managed-tree semantic: the direct child exiting is NOT the whole tree
    // exiting, and shutdown must terminate the descendant and close stdout.
    let fixture = Fixture::new("spawn_descendant");
    let client = fixture
        .provider
        .project_client(&fixture.root, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let connection = Arc::clone(&client.connection);

    // Capture the descendant pid the fake server recorded before exiting.
    let descendant_pid = {
        let marker = fs::read_to_string(&fixture.marker).unwrap_or_default();
        marker
            .lines()
            .find_map(|line| line.strip_prefix("GRANDCHILD_PID="))
            .and_then(|value| value.trim().parse::<u32>().ok())
    };
    assert!(
        descendant_pid.is_some(),
        "fake server should have spawned a descendant before exiting"
    );
    let descendant_pid = descendant_pid.unwrap();
    let direct_pid = connection.child.lock().unwrap().id();
    assert_ne!(
        direct_pid, descendant_pid,
        "direct child and descendant must be distinct"
    );

    // The direct child exited on its own, but the tree (descendant) is alive
    // and still holds the stdout write end, so the reader must stay alive.
    assert!(wait_until(Duration::from_secs(1), || process_exists(
        descendant_pid
    )));
    assert!(
        connection.is_alive(),
        "connection died while the descendant still owned stdout"
    );

    let outcome = fixture
        .provider
        .shutdown_until(Instant::now() + Duration::from_secs(2));
    assert_eq!(outcome.connections, 1);
    assert_eq!(
        outcome.timed_out, 0,
        "shutdown must reap within the deadline"
    );
    assert_eq!(outcome.failures, 0);

    assert!(
        wait_until(Duration::from_secs(2), || !process_exists(descendant_pid)),
        "descendant {descendant_pid} survived MCP shutdown"
    );
    assert!(
        wait_until(Duration::from_secs(1), || !process_exists(direct_pid)),
        "direct child {direct_pid} survived MCP shutdown"
    );
}

#[test]
fn opt_in_real_claude_mcp_probe() {
    let _serial = serialize_fake_mcp_test();
    if env::var("WEBCODEX_PROBE_CLAUDE_PROVIDER").as_deref() != Ok("1") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let config = ClaudeCodeMcpConfig {
        enabled: true,
        ..Default::default()
    };
    let provider = ClaudeCodeMcpProvider::new(config);
    provider
        .project_client(root.path(), Instant::now() + Duration::from_secs(30))
        .unwrap_or_else(|error| panic!("Claude MCP probe failed: {}", error.code));
    let status = provider.status();
    assert!(status.available);
    assert_eq!(status.process_state, "running");
    println!(
        "{}",
        serde_json::to_string(&status).expect("provider status must serialize")
    );
    provider.shutdown();
}

#[test]
fn opt_in_real_claude_mcp_smoke() {
    let _serial = serialize_fake_mcp_test();
    if env::var("WEBCODEX_TEST_CLAUDE_MCP").as_deref() != Ok("1") {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("claude-smoke-project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("fixture.txt"), "zero\nneedle\n").unwrap();
    fs::write(root.join("edit.txt"), "before\n").unwrap();
    let root = root.canonicalize().unwrap();

    let mut config = ClaudeCodeMcpConfig {
        enabled: true,
        ..Default::default()
    };
    let provider = ClaudeCodeMcpProvider::new(config.clone());
    let client = provider
        .project_client(&root, Instant::now() + Duration::from_secs(30))
        .unwrap_or_else(|error| panic!("real claude MCP initialization failed: {}", error.code));
    let status = provider.status();
    assert!(status.available, "Claude MCP did not become available");
    eprintln!(
        "claude_mcp_version={:?} discovery={}",
        status.version,
        discovery_inventory(&client)
    );

    let grep_tool = real_tool_name(
        &client,
        ProviderCapability::SearchProjectText,
        "WEBCODEX_TEST_CLAUDE_GREP_TOOL",
    );

    if let Ok(name) = &grep_tool {
        config
            .mapping
            .insert("search_project_text".to_string(), name.clone());
        let search_context = ToolExecutionContext {
            project_root: &root,
            target: root.clone(),
            max_output_bytes: MAX_MCP_OUTPUT_BYTES,
            timeout_secs: 30,
        };
        let search = client
            .call(
                ProviderCapability::SearchProjectText,
                search_request(),
                &search_context,
                &config,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap_or_else(|error| panic!("real Grep call failed with {}", error.code));
        let matched = search
            .as_str()
            .is_some_and(|text| text.contains("fixture.txt") && text.contains("needle"));
        assert!(
            matched,
            "real Grep did not return the temporary fixture: {search}"
        );
        eprintln!("claude_mcp_grep_result matched={matched}");
    }

    let process_groups = process_ids(&provider);
    assert!(
        !process_groups.is_empty(),
        "provider did not retain its child"
    );
    provider.shutdown();
    #[cfg(unix)]
    for pid in process_groups {
        assert!(
            wait_until(Duration::from_secs(2), || !process_exists(pid)),
            "Claude process {pid} remained after provider shutdown"
        );
    }
    eprintln!("claude_mcp_shutdown process_groups_reaped=true");
    if let Err(error) = grep_tool {
        eprintln!("claude_mcp_grep_unavailable={}", sanitize_name(&error));
    }
}

#[test]
fn normalized_search_exit_status_distinguishes_matches_from_no_match() {
    let marker = r#"{"webcodex_search":{"backend":"claude_code","feature_unavailable":false}}"#;
    assert_eq!(normalized_search_exit_code(marker), 1);
    assert_eq!(
        normalized_search_exit_code(&format!("{marker}\nsrc/lib.rs:1:needle")),
        0
    );
}

#[test]
fn normalize_search_result_rejects_untrusted_paths_instead_of_claiming_no_match() {
    let root = tempfile::tempdir().unwrap();
    let context = ToolExecutionContext {
        project_root: root.path(),
        target: root.path().to_path_buf(),
        max_output_bytes: MAX_MCP_OUTPUT_BYTES,
        timeout_secs: 30,
    };
    let result = json!({
        "content": [{
            "type": "text",
            "text": "/private/provider/NEVER_RETURN.rs:1:needle"
        }]
    });

    let error = normalize_search_result(&result, &context).unwrap_err();
    assert_eq!(error.code, "provider_output_untrusted");
}

/// Whether a process with `pid` is still alive, probed natively (no tasklist,
/// no shelling out).
#[cfg(target_os = "macos")]
fn process_exists(pid: u32) -> bool {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let bytes = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if bytes == size as libc::c_int {
        let info = unsafe { info.assume_init() };
        return info.pbi_status != libc::SZOMB;
    }
    !(bytes == 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 is an existence probe; the pid comes from our own child.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
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
