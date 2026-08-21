use super::*;

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
    changed.mcp_gateway.request_timeout_secs += 1;
    changed.transport = Some(TRANSPORT_QUIC.to_string());
    changed.websocket_connect_timeout_secs += 1;
    changed.quic = Some(quic_client_config());
    assert_eq!(
            webcodex_runner::config::restart_required_fields(&startup, &changed).join(" "),
            "capabilities client_id display_name hostname host_context max_concurrent_jobs mcp_gateway owner poll_interval_ms projects_dir temporary_projects_root quic server_url token transport websocket_connect_timeout_secs"
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
