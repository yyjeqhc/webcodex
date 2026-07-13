use super::support::*;
use crate::lsp_bridge::{
    AgentLspRequest, AgentLspResultEnvelope, LspAvailabilityStatus, LspCommandSource,
    LspServerStatusEntry, LspStatusResult, AGENT_LSP_REQUEST_KIND,
};
use crate::shell_protocol::{
    ShellAgentResultRequest, ShellClientCapabilities,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
};
use crate::tool_runtime::{
    known_tool_names, registered_tool_specs, SessionMode, ToolCall, ToolResult, ToolRuntime,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn semantic_capabilities(enabled: bool) -> ShellClientCapabilities {
    ShellClientCapabilities {
        shell: true,
        git: true,
        file_read: true,
        file_write: true,
        lsp_read_only_navigation: enabled,
        ..Default::default()
    }
}

async fn register_semantic_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &std::path::Path,
    lsp_capable: bool,
) -> String {
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        semantic_capabilities(lsp_capable),
        vec![registered_project(project_id, &root.to_string_lossy())],
    )
    .await;
    crate::tool_runtime::agent_project_runtime_id(client_id, project_id)
}

fn start_call(project: String, compact_startup: bool, mode: SessionMode) -> ToolCall {
    ToolCall::StartCodingTask {
        project,
        title: Some("semantic navigation startup".to_string()),
        mode,
        deny_write_tools: false,
        deny_shell_tools: false,
        include_runtime_status: Some(false),
        compact_startup,
        include_git: Some(false),
        include_recent_commits: Some(false),
        include_rules: Some(false),
        include_tool_manifest: Some(false),
        tool_manifest_intent: None,
        tool_manifest_categories: None,
        tool_manifest_limit: None,
        bind_current: false,
    }
}

fn spawn_start(
    runtime: &ToolRuntime,
    project: String,
    compact_startup: bool,
    mode: SessionMode,
) -> tokio::task::JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    tokio::spawn(async move {
        runtime
            .dispatch_with_auth(
                start_call(project, compact_startup, mode),
                Some(&auth_context(None, true)),
            )
            .await
    })
}

async fn next_semantic_status_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> crate::shell_protocol::ShellAgentShellRequest {
    let mut request = None;
    for _ in 0..200 {
        request = next_patch_agent_request(runtime, client_id).await;
        if request.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let request = request.expect("semantic navigation status request");
    assert_eq!(request.kind, AGENT_LSP_REQUEST_KIND);
    assert!(request.command.is_empty());
    assert!(request.stdin.is_none());
    let payload = request.lsp.as_ref().expect("typed LSP payload");
    assert_eq!(payload.request, AgentLspRequest::Status);
    request
}

async fn complete_status_envelope(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: &str,
    envelope: AgentLspResultEnvelope,
) {
    complete_patch_agent_request(
        runtime,
        client_id,
        request_id,
        0,
        &envelope.to_stdout_json(),
        "",
    )
    .await;
}

fn status_result(
    project_id: &str,
    detected_rust: bool,
    status: LspAvailabilityStatus,
    position_encoding: Option<&str>,
) -> LspStatusResult {
    LspStatusResult {
        project: project_id.to_string(),
        detected_languages: if detected_rust {
            vec!["rust".to_string()]
        } else {
            Vec::new()
        },
        servers: vec![LspServerStatusEntry {
            language: "rust".to_string(),
            server: "rust-analyzer".to_string(),
            available: status != LspAvailabilityStatus::Unavailable,
            running: status == LspAvailabilityStatus::Running,
            status,
            source: Some(LspCommandSource::Path),
            position_encoding: position_encoding.map(str::to_string),
        }],
        warnings: Vec::new(),
    }
}

async fn start_with_status(
    status: LspAvailabilityStatus,
    position_encoding: Option<&str>,
) -> ToolResult {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "semantic-agent", "demo", temp.path(), true).await;
    let task = spawn_start(&runtime, project, false, SessionMode::Normal);
    let request = next_semantic_status_request(&runtime, "semantic-agent").await;
    complete_status_envelope(
        &runtime,
        "semantic-agent",
        &request.request_id,
        AgentLspResultEnvelope::ok(status_result("demo", true, status, position_encoding)),
    )
    .await;
    task.await.unwrap()
}

fn assert_optional_sections_disabled(output: &Value) {
    assert_eq!(output["runtime_status"], Value::Null);
    assert_eq!(output["git"], Value::Null);
    assert_eq!(output["rules"], Value::Null);
    assert!(output.get("tool_manifest").is_none());
    assert!(output.get("semantic_navigation").is_some());
}

#[tokio::test]
async fn coding_task_semantic_navigation_available_is_recommended_and_bounded() {
    let result = start_with_status(LspAvailabilityStatus::Available, None).await;
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["supported"], true);
    assert_eq!(semantic["available"], true);
    assert_eq!(semantic["recommended"], true);
    assert_eq!(semantic["status"], "available");
    assert_eq!(semantic["language"], "rust");
    assert_eq!(semantic["server"], "rust-analyzer");
    assert_eq!(semantic["position_encoding"], Value::Null);
    assert_eq!(
        semantic["tools"],
        json!([
            "lsp_status",
            "document_symbols",
            "goto_definition",
            "find_references",
            "document_diagnostics",
            "hover",
            "workspace_symbols"
        ])
    );
    assert_eq!(
        semantic["preferred_flow"],
        json!([
            "document_symbols",
            "goto_definition",
            "find_references",
            "hover",
            "read_file",
            "search_project_text"
        ])
    );
    assert_eq!(
        semantic["limitations"],
        json!([
            "rust_only",
            "read_only",
            "workspace_only",
            "no_dependency_navigation",
            "full_text_sync_only"
        ])
    );
    assert_eq!(semantic["reason_code"], Value::Null);
    assert!(result.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .starts_with("wc_sess_"));
    assert_eq!(result.output["startup_verdict"]["status"], "warn");
    assert_eq!(result.output["warnings"], json!([]));
    assert_optional_sections_disabled(&result.output);
}

#[tokio::test]
async fn coding_task_semantic_navigation_running_propagates_position_encoding() {
    for encoding in ["utf-8", "utf-16", "utf-32"] {
        let result = start_with_status(LspAvailabilityStatus::Running, Some(encoding)).await;
        let semantic = &result.output["semantic_navigation"];
        assert_eq!(semantic["status"], "running");
        assert_eq!(semantic["available"], true);
        assert_eq!(semantic["recommended"], true);
        assert_eq!(semantic["position_encoding"], encoding);
    }
}

#[tokio::test]
async fn coding_task_semantic_navigation_initializing_is_available_not_recommended() {
    let result = start_with_status(LspAvailabilityStatus::Initializing, None).await;
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "initializing");
    assert_eq!(semantic["available"], true);
    assert_eq!(semantic["recommended"], false);
    assert_eq!(semantic["preferred_flow"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_crashed_does_not_lower_startup_verdict() {
    let result = start_with_status(LspAvailabilityStatus::Crashed, None).await;
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "crashed");
    assert_eq!(semantic["available"], true);
    assert_eq!(semantic["recommended"], false);
    assert_eq!(semantic["reason_code"], "server_crashed");
    assert_eq!(result.output["startup_verdict"]["status"], "warn");
    assert_eq!(result.output["warnings"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_unavailable_is_nonblocking() {
    let result = start_with_status(LspAvailabilityStatus::Unavailable, None).await;
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["supported"], true);
    assert_eq!(semantic["available"], false);
    assert_eq!(semantic["recommended"], false);
    assert_eq!(semantic["status"], "unavailable");
    assert_eq!(semantic["reason_code"], "server_unavailable");
    assert_eq!(result.output["startup_verdict"]["status"], "warn");
    assert_eq!(result.output["warnings"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_non_rust_agent_is_not_applicable() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "non-rust-agent", "demo", temp.path(), true).await;
    let task = spawn_start(&runtime, project, false, SessionMode::Normal);
    let request = next_semantic_status_request(&runtime, "non-rust-agent").await;
    complete_status_envelope(
        &runtime,
        "non-rust-agent",
        &request.request_id,
        AgentLspResultEnvelope::ok(status_result(
            "demo",
            false,
            LspAvailabilityStatus::Available,
            None,
        )),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["supported"], true);
    assert_eq!(semantic["status"], "not_applicable");
    assert_eq!(semantic["reason_code"], "rust_not_detected");
    assert_eq!(semantic["tools"], json!([]));
    assert_eq!(semantic["preferred_flow"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_legacy_agent_is_not_enqueued() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "legacy-agent", "demo", temp.path(), false).await;
    let result = runtime
        .dispatch_with_auth(
            start_call(project, false, SessionMode::Normal),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["supported"], false);
    assert_eq!(semantic["status"], "agent_capability_unavailable");
    assert_eq!(semantic["reason_code"], "lsp_capability_not_advertised");
    assert_eq!(semantic["tools"], json!([]));
    assert!(next_patch_agent_request(&runtime, "legacy-agent")
        .await
        .is_none());
}

#[tokio::test]
async fn coding_task_semantic_navigation_disconnected_agent_is_nonblocking() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "offline-agent", "demo", temp.path(), true).await;
    runtime
        .shell_clients
        .reconcile_disconnect("offline-agent", "inst")
        .await;
    let result = runtime
        .dispatch_with_auth(
            start_call(project, false, SessionMode::Normal),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "agent_unavailable");
    assert_eq!(semantic["reason_code"], "agent_not_connected");
    assert_eq!(result.output["startup_verdict"]["status"], "warn");
    assert_eq!(result.output["warnings"], json!([]));
    assert!(next_patch_agent_request(&runtime, "offline-agent")
        .await
        .is_none());
}

#[tokio::test]
async fn coding_task_semantic_navigation_timeout_uses_one_budget_and_cancels_waiter() {
    let runtime = test_runtime().with_semantic_navigation_probe_timeout(Duration::from_millis(25));
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "timeout-agent", "demo", temp.path(), true).await;
    let started = Instant::now();
    let task = spawn_start(&runtime, project, false, SessionMode::Normal);
    let request = next_semantic_status_request(&runtime, "timeout-agent").await;
    let result = task.await.unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "probe_timeout");
    assert_eq!(semantic["reason_code"], "status_probe_timed_out");
    assert_eq!(semantic["preferred_flow"], json!([]));
    assert_eq!(result.output["warnings"], json!([]));
    assert_eq!(result.output["startup_verdict"]["status"], "warn");

    let expired = runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "timeout-agent".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some("{}".to_string()),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .expect_err("timed-out startup probe must remove pending waiter");
    assert!(expired.contains("unknown or expired shell request"));

    let session_id = result.output["session"]["session_id"].as_str().unwrap();
    let summary = runtime.sessions.summary(session_id, Some(20)).unwrap();
    assert!(summary
        .events
        .iter()
        .all(|event| event.tool_name != "lsp_status"));
    assert_eq!(summary.counts.failed, 0);
}

#[tokio::test]
async fn coding_task_semantic_navigation_malformed_result_is_sanitized() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "malformed-agent", "demo", temp.path(), true).await;
    let task = spawn_start(&runtime, project, false, SessionMode::Normal);
    let request = next_semantic_status_request(&runtime, "malformed-agent").await;
    complete_patch_agent_request(
        &runtime,
        "malformed-agent",
        &request.request_id,
        0,
        r#"{"raw":"/private/secret","stdout":"do not leak"}"#,
        "stderr must not leak",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "probe_failed");
    assert_eq!(semantic["reason_code"], "malformed_agent_result");
    let serialized = semantic.to_string();
    for forbidden in ["/private/secret", "do not leak", "stderr"] {
        assert!(!serialized.contains(forbidden), "{serialized}");
    }
    assert_eq!(result.output["warnings"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_agent_failure_uses_fixed_reason_code() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "failed-agent", "demo", temp.path(), true).await;
    let task = spawn_start(&runtime, project, false, SessionMode::Normal);
    let request = next_semantic_status_request(&runtime, "failed-agent").await;
    complete_status_envelope(
        &runtime,
        "failed-agent",
        &request.request_id,
        AgentLspResultEnvelope::err("lsp_protocol_error", "private raw failure detail"),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic["status"], "probe_failed");
    assert_eq!(semantic["reason_code"], "status_probe_failed");
    assert!(!semantic.to_string().contains("private raw failure detail"));
    assert_eq!(result.output["warnings"], json!([]));
}

#[tokio::test]
async fn coding_task_semantic_navigation_compact_read_only_keeps_full_shape() {
    let runtime = test_runtime();
    let temp = tempfile::tempdir().unwrap();
    let project =
        register_semantic_agent(&runtime, "compact-agent", "demo", temp.path(), true).await;
    let task = spawn_start(&runtime, project, true, SessionMode::ReadOnly);
    let request = next_semantic_status_request(&runtime, "compact-agent").await;
    complete_status_envelope(
        &runtime,
        "compact-agent",
        &request.request_id,
        AgentLspResultEnvelope::ok(status_result(
            "demo",
            true,
            LspAvailabilityStatus::Available,
            None,
        )),
    )
    .await;
    let result = task.await.unwrap();
    let semantic = &result.output["semantic_navigation"];
    assert_eq!(semantic.as_object().unwrap().len(), 11);
    assert_eq!(semantic["status"], "available");
    assert_eq!(semantic["tools"].as_array().unwrap().len(), 7);
    assert_eq!(semantic["preferred_flow"].as_array().unwrap().len(), 6);
    assert_eq!(result.output["session"]["mode"], "read_only");
    assert_eq!(result.output["session"]["guards"]["deny_write_tools"], true);
    assert_eq!(result.output["session"]["guards"]["deny_shell_tools"], true);
    assert_optional_sections_disabled(&result.output);
}

#[test]
fn coding_task_semantic_navigation_output_schema_is_explicit_and_surface_counts_are_stable() {
    let specs = registered_tool_specs();
    let runtime_tool_count = specs.len();
    assert_eq!(
        runtime_tool_count, 76,
        "runtime tool count must remain stable"
    );
    assert_eq!(runtime_tool_count, known_tool_names().count());
    let start = specs
        .iter()
        .find(|spec| spec.name == "start_coding_task")
        .unwrap();
    assert!(start.input_schema["properties"]
        .as_object()
        .unwrap()
        .get("include_semantic_navigation")
        .is_none());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("start_coding_task");
    let semantic = &schema["properties"]["output"]["properties"]["semantic_navigation"];
    assert_eq!(semantic["additionalProperties"], false);
    assert_eq!(
        semantic["properties"]["status"]["enum"],
        json!([
            "running",
            "available",
            "initializing",
            "crashed",
            "unavailable",
            "not_applicable",
            "agent_unavailable",
            "agent_capability_unavailable",
            "probe_timeout",
            "probe_failed"
        ])
    );
    assert_eq!(semantic["properties"]["tools"]["maxItems"], 7);
    assert_eq!(semantic["properties"]["preferred_flow"]["maxItems"], 6);
    assert!(semantic["properties"]["reason_code"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["type"] == "null"));
    assert_eq!(
        semantic["properties"]["reason_code"]["anyOf"][0]["enum"],
        json!([
            "project_not_agent_backed",
            "rust_not_detected",
            "agent_not_connected",
            "lsp_capability_not_advertised",
            "server_crashed",
            "server_unavailable",
            "status_probe_timed_out",
            "status_probe_failed",
            "malformed_agent_result"
        ])
    );

    let openapi = crate::openapi::build_openapi_spec();
    let operation_count: usize = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|methods| methods.as_object().unwrap().len())
        .sum();
    assert_eq!(operation_count, 25);
    assert!(!known_tool_names().any(|name| name == "semantic_navigation"));
    assert!(crate::shell_protocol::SHELL_CLIENT_CAPABILITY_NAMES
        .contains(&SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION));
}
