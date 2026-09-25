//! Focused tests for the canonical `work_on_project` coding entry point.
//!
//! `work_on_project` validates one of two project sources plus the task inputs,
//! invokes the shared coding workflow engine, and projects a compact startup
//! result. It never binds a current window, never guesses a recent Session, and
//! never falls back to a credential-wide Session.

use super::reconnect::dispatch_coding_call_in_window;
use super::support::*;
use crate::db::{NewGoal, NewGoalStep};
use crate::lsp_bridge::{RunnerLspRequest, RunnerLspResultEnvelope, AGENT_LSP_REQUEST_KIND};
use crate::runner_protocol::{RunnerCapabilities, RunnerResultPayload, RunnerResultRequest};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
    ToolProtocolCapabilities, ToolTransport,
};
use crate::tool_runtime::permissions::{AuthorityMode, PermissionEvaluator};
use crate::tool_runtime::sessions::{SessionEvent, SessionGuards};
use crate::tool_runtime::{
    registered_tool_specs, SessionMode, StartupDetail, ToolCall, ToolResult, ToolRuntime,
};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use webcodex_core::plugin::{
    PluginGatewayRequest, PluginGatewayResponse, PluginGatewayResponsePayload,
    PluginSelectionAnnotations, ProjectPluginCatalog, ProjectPluginCatalogEntry,
};
use webcodex_core::project_instructions::{
    InstructionSourceScope, LoadedInstructionCandidate, ProjectInstructionsSnapshot,
};
use webcodex_core::runner_instruction::{
    RunnerInstructionSnapshotResponse, RUNNER_INSTRUCTION_REQUEST_KIND,
    RUNNER_INSTRUCTION_RESPONSE_FORMAT,
};
use webcodex_core::runner_skill::{
    RunnerSkillDescriptor, RunnerSkillListResponse, RunnerSkillRequest,
    RUNNER_SKILL_RESPONSE_FORMAT,
};

fn record_window_activity_fixture(
    db: &std::sync::Arc<crate::Database>,
    auth: &crate::auth::AuthContext,
    window_id: &str,
    project: &str,
    operation: &str,
    linked_session: Option<(&str, crate::action_audit_sessions::WorkflowSessionRelation)>,
    recorder_gap_session_id: Option<&str>,
    at_ms: i64,
) {
    let window = crate::client_window::ClientWindow::for_test(window_id);
    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
    crate::action_audit_sessions::record_action_event(
        db,
        crate::action_audit_sessions::ActionAuditEventInput {
            explicit_session_id: None,
            session_title: None,
            endpoint: "/mcp".to_string(),
            action_name: "toolsCall".to_string(),
            operation: Some(operation.to_string()),
            project: Some(project.to_string()),
            principal_kind: None,
            principal_user_id: None,
            oauth_client_id: None,
            status: "success".to_string(),
            http_status: Some(200),
            started_at: at_ms / 1000,
            ended_at: at_ms / 1000,
            duration_ms: 1,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({}),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(window.key().to_string()),
            client_window_source: Some(window.source().to_string()),
            server_trace_id: Some(format!("fixture-{at_ms}")),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(at_ms),
            window_ended_at_ms: Some(at_ms + 1),
            request_observed_at_ms: None,
            response_handed_at_ms: None,
            window_transition_kind: None,
            response_streaming: None,
            window_continuity_eligible: None,
            window_meaningful: true,
            recorder_gap_session_id: recorder_gap_session_id.map(str::to_string),
            workflow_links: linked_session
                .map(|(session_id, relation)| {
                    vec![crate::action_audit_sessions::ActionAuditWorkflowLinkInput {
                        workflow_session_id: session_id.to_string(),
                        relation,
                        project: Some(project.to_string()),
                    }]
                })
                .unwrap_or_default(),
        },
    );
}

async fn call_hygiene_in_window_with_local_runner(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    recording_session_id: Option<&str>,
    business_session_id: Option<&str>,
    auth: &crate::auth::AuthContext,
    window_id: &str,
) -> crate::tool_runtime::kernel::ToolCallOutcome {
    call_hygiene_in_window_with_local_runner_transport(
        runtime,
        client_id,
        project,
        recording_session_id,
        business_session_id,
        auth,
        window_id,
        ToolTransport::Mcp,
    )
    .await
}

async fn call_hygiene_in_window_with_local_runner_transport(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    recording_session_id: Option<&str>,
    business_session_id: Option<&str>,
    auth: &crate::auth::AuthContext,
    window_id: &str,
    transport: ToolTransport,
) -> crate::tool_runtime::kernel::ToolCallOutcome {
    call_hygiene_in_window_with_local_runner_metadata(
        runtime,
        client_id,
        project,
        recording_session_id,
        business_session_id,
        auth,
        window_id,
        transport,
        ToolInvocationMetadata::default(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn call_hygiene_in_window_with_local_runner_metadata(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    recording_session_id: Option<&str>,
    business_session_id: Option<&str>,
    auth: &crate::auth::AuthContext,
    window_id: &str,
    transport: ToolTransport,
    invocation_metadata: ToolInvocationMetadata,
) -> crate::tool_runtime::kernel::ToolCallOutcome {
    let runtime_for_task = runtime.clone();
    let project = project.to_string();
    let recording_session_id = recording_session_id.map(str::to_string);
    let business_session_id = business_session_id.map(str::to_string);
    let auth = auth.clone();
    let window_id = window_id.to_string();
    let task = tokio::spawn(async move {
        let window = crate::client_window::ClientWindow::for_test(&window_id);
        let mut arguments = json!({"project": project});
        if let Some(session_id) = business_session_id {
            arguments["session_id"] = json!(session_id);
        }
        runtime_for_task
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "workspace_hygiene_check".to_string(),
                    arguments,
                },
                ToolCallContext {
                    transport,
                    session_id: recording_session_id.as_deref(),
                    auth: Some(&auth),
                    window: Some(&window),
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                invocation_metadata,
                ToolProtocolCapabilities::default(),
            )
            .await
    });

    // These tests exercise Session/window recording semantics, not Git or shell
    // integration. Complete the one expected hygiene diagnostic in-memory so a
    // missed fixture response cannot fall through to the 30-second production
    // script timeout. The absolute deadline is a real wall-clock test bound.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "window-correlation hygiene fixture did not finish within 2 seconds for {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            assert_eq!(request.kind, "run_internal_posix_script");
            let script = request
                .script
                .as_ref()
                .expect("hygiene diagnostics must use the typed internal script payload");
            assert_eq!(
                script.script,
                crate::tool_runtime::hygiene::hygiene_diagnostic_command(),
                "Session fixture must not hide an unexpected hygiene subprocess"
            );
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                "\n@@WEBCODEX_HYGIENE_STATUS@@0\n",
                "",
            )
            .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
    task.await.unwrap()
}

fn work_on_project_call(project: &str, instruction: &str, session_id: Option<&str>) -> ToolCall {
    ToolCall::WorkOnProject {
        project: project.to_string(),
        client_id: None,
        path: None,
        mode: None,
        base_ref: None,
        instruction: instruction.to_string(),
        guidance_profile: Default::default(),
        include_extension_catalog: false,
        session_id: session_id.map(str::to_string),
    }
}

fn work_on_project_call_with_extensions(
    project: &str,
    instruction: &str,
    include_extension_catalog: bool,
) -> ToolCall {
    ToolCall::WorkOnProject {
        project: project.to_string(),
        client_id: None,
        path: None,
        mode: None,
        base_ref: None,
        instruction: instruction.to_string(),
        guidance_profile: Default::default(),
        include_extension_catalog,
        session_id: None,
    }
}

fn write_project_skill(root: &Path, package: &str, name: &str, description: &str, body: &str) {
    let dir = root.join(".agents/skills").join(package);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n{body}"),
    )
    .unwrap();
}

fn startup_plugin_catalog_fixture() -> ProjectPluginCatalog {
    ProjectPluginCatalog {
        catalog_revision: format!("wc_plugcat_{}", webcodex_core::compact::encode([0xaa; 32])),
        total_count: 1,
        entries: vec![ProjectPluginCatalogEntry {
            plugin: "repo-context".to_string(),
            name: "Repo Context".to_string(),
            tool: "repo_context".to_string(),
            title: Some("Repository context".to_string()),
            description: Some("Compact Git and Cargo context".to_string()),
            annotations: PluginSelectionAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            },
        }],
    }
}

async fn complete_startup_plugin_catalog_request(
    runtime: &ToolRuntime,
    request: crate::runner_protocol::RunnerRequest,
) {
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: request.client_id,
                runner_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: None,
                error: None,
            },
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: Some(PluginGatewayResponse::success(
                PluginGatewayResponsePayload::ProjectCatalog {
                    catalog: startup_plugin_catalog_fixture(),
                },
            )),
            coding_agent: None,
        })
        .await
        .unwrap();
}

fn path_work_on_project_call(
    client_id: &str,
    path: &str,
    instruction: &str,
    session_id: Option<&str>,
) -> ToolCall {
    ToolCall::WorkOnProject {
        project: String::new(),
        client_id: Some(client_id.to_string()),
        path: Some(path.to_string()),
        mode: None,
        base_ref: None,
        instruction: instruction.to_string(),
        guidance_profile: Default::default(),
        include_extension_catalog: false,
        session_id: session_id.map(str::to_string),
    }
}

fn worktree_work_on_project_call(
    client_id: &str,
    path: &str,
    instruction: &str,
    base_ref: Option<&str>,
    session_id: Option<&str>,
) -> ToolCall {
    ToolCall::WorkOnProject {
        project: String::new(),
        client_id: Some(client_id.to_string()),
        path: Some(path.to_string()),
        mode: Some("worktree".to_string()),
        base_ref: base_ref.map(str::to_string),
        instruction: instruction.to_string(),
        guidance_profile: Default::default(),
        include_extension_catalog: false,
        session_id: session_id.map(str::to_string),
    }
}

fn managed_fixture_git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run managed-worktree fixture git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn seed_managed_tool_runtime_fixture(source: &Path, worktree: &Path) -> String {
    std::fs::create_dir_all(source).unwrap();
    managed_fixture_git(source, &["init"]);
    managed_fixture_git(
        source,
        &["config", "user.email", "webcodex@example.invalid"],
    );
    managed_fixture_git(source, &["config", "user.name", "WebCodex Test"]);
    std::fs::write(source.join("hello.txt"), "committed\n").unwrap();
    managed_fixture_git(source, &["add", "hello.txt"]);
    managed_fixture_git(source, &["commit", "-m", "seed"]);
    let sha = managed_fixture_git(source, &["rev-parse", "HEAD"]);
    let worktree_arg = worktree.to_string_lossy().to_string();
    managed_fixture_git(
        source,
        &[
            "worktree",
            "add",
            "--detach",
            worktree_arg.as_str(),
            sha.as_str(),
        ],
    );
    std::fs::write(source.join("hello.txt"), "dirty source only\n").unwrap();
    sha
}

fn managed_source_root_fingerprint() -> String {
    format!("wc_projroot_{}", "1".repeat(64))
}

fn managed_target_root_fingerprint() -> String {
    format!("wc_projroot_{}", "2".repeat(64))
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_with_managed_worktree_runner(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    source_path: &str,
    managed_path: &str,
    agent_project_id: &str,
    base_ref: &str,
    base_sha: &str,
    source_dirty: bool,
    outcome: &str,
    registered: bool,
    first_indeterminate: bool,
) -> (ToolResult, Vec<Value>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth_context(None, true);
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut prepare_payloads = Vec::new();
    let mut sent_indeterminate = false;
    loop {
        if task.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "managed-worktree coding call did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            if request.kind == "prepare_managed_worktree" {
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                assert_eq!(payload["path"], source_path);
                prepare_payloads.push(payload);
                if first_indeterminate && !sent_indeterminate {
                    sent_indeterminate = true;
                    let response = json!({
                        "error_code": "operation_indeterminate",
                        "error_kind": "operation_indeterminate",
                        "failure_kind": "operation_indeterminate",
                        "state_changed": true,
                    });
                    complete_patch_agent_request(
                        runtime,
                        client_id,
                        &request.request_id,
                        1,
                        &response.to_string(),
                        "",
                    )
                    .await;
                    continue;
                }
                let response = json!({
                    "id": format!("agent:{client_id}:{agent_project_id}"),
                    "agent_project_id": agent_project_id,
                    "client_id": client_id,
                    "name": agent_project_id,
                    "path": managed_path,
                    "kind": "auto_registered",
                    "registration_source": "auto_registered",
                    "description": null,
                    "allow_patch": true,
                    "disabled": false,
                    "revision": format!("sha256:{}", "b".repeat(64)),
                    "root_fingerprint": managed_target_root_fingerprint(),
                    "lineage": {
                        "kind": "managed_worktree_source",
                        "source_project_id": "source",
                        "source_root_fingerprint": managed_source_root_fingerprint(),
                        "base_sha": base_sha,
                    },
                    "source": "managed_worktree",
                    "outcome": outcome,
                    "registered": registered,
                    "created_config": registered,
                    "changed": registered,
                    "recovered": !registered,
                    "managed": true,
                    "base_ref": base_ref,
                    "base_sha": base_sha,
                    "source_dirty": source_dirty,
                });
                complete_patch_agent_request(
                    runtime,
                    client_id,
                    &request.request_id,
                    0,
                    &response.to_string(),
                    "",
                )
                .await;
            } else if request.kind == AGENT_LSP_REQUEST_KIND {
                complete_patch_agent_request(
                    runtime,
                    client_id,
                    &request.request_id,
                    0,
                    &RunnerLspResultEnvelope::err(
                        "lsp_status_unavailable",
                        "fixture intentionally has no language server",
                    )
                    .to_stdout_json(),
                    "",
                )
                .await;
            } else {
                complete_agent_request_by_running_locally(runtime, client_id, request).await;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    (task.await.unwrap(), prepare_payloads)
}

/// Drive any coding startup to completion while recording every Runner request.
/// The typed LSP status probe gets a valid bounded error envelope; file/Git and
/// overview requests use the existing local fixture implementation.
async fn dispatch_recording_startup_requests(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&crate::auth::AuthContext>,
    window_id: &str,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        let window_id = window_id.to_string();
        async move {
            let window = crate::client_window::ClientWindow::for_test(&window_id);
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_window(
                    call,
                    auth.as_ref(),
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                    Default::default(),
                    Some(&window),
                )
                .await
        }
    });
    record_startup_requests(runtime, client_id, task).await
}

fn runner_instruction_snapshot_stdout(body: &str, generation: u64) -> String {
    let snapshot = ProjectInstructionsSnapshot::from_candidates(
        vec![LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Runner,
            path: "runner/0/AGENTS.md".to_string(),
            content: body.to_string(),
            total_lines: body.lines().count(),
            full_sha256: None,
        }],
        true,
    );
    serde_json::to_string(&RunnerInstructionSnapshotResponse {
        format: RUNNER_INSTRUCTION_RESPONSE_FORMAT.to_string(),
        generation,
        scan_complete: snapshot.scan_complete,
        files: snapshot.files,
    })
    .unwrap()
}

async fn dispatch_recording_startup_requests_with_runner_instructions(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&crate::auth::AuthContext>,
    window_id: &str,
    runner_instruction_stdout: &str,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        let window_id = window_id.to_string();
        async move {
            let window = crate::client_window::ClientWindow::for_test(&window_id);
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_window(
                    call,
                    auth.as_ref(),
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                    Default::default(),
                    Some(&window),
                )
                .await
        }
    });
    record_startup_requests_with_runner_instruction_response(
        runtime,
        client_id,
        task,
        Some(runner_instruction_stdout),
    )
    .await
}

async fn dispatch_recording_coding_workflow_diagnostic(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    instruction: &str,
    detail: StartupDetail,
    auth: Option<&crate::auth::AuthContext>,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        let instruction = instruction.to_string();
        let auth = auth.cloned();
        async move {
            runtime
                .start_coding_workflow_for_test(
                    project,
                    None,
                    None,
                    Some(instruction),
                    SessionMode::Normal,
                    false,
                    false,
                    detail,
                    None,
                    None,
                    auth.as_ref(),
                    None,
                    None,
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                )
                .await
        }
    });
    record_startup_requests(runtime, client_id, task).await
}

async fn record_startup_requests(
    runtime: &ToolRuntime,
    client_id: &str,
    task: tokio::task::JoinHandle<ToolResult>,
) -> (ToolResult, Vec<String>) {
    record_startup_requests_with_runner_instruction_response(runtime, client_id, task, None).await
}

async fn record_startup_requests_with_runner_instruction_response(
    runtime: &ToolRuntime,
    client_id: &str,
    task: tokio::task::JoinHandle<ToolResult>,
    runner_instruction_stdout: Option<&str>,
) -> (ToolResult, Vec<String>) {
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut request_kinds = Vec::new();
    loop {
        if task.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "coding startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?}; serviced requests: {request_kinds:?}"
        );
        let Some(request) = probe_patch_agent_request(runtime, client_id).await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        request_kinds.push(request.kind.clone());
        if request.kind == RUNNER_INSTRUCTION_REQUEST_KIND {
            let stdout = runner_instruction_stdout
                .expect("instruction-runtime fixture requires a configured Runner response");
            complete_patch_agent_request(runtime, client_id, &request.request_id, 0, stdout, "")
                .await;
        } else if request.kind == AGENT_LSP_REQUEST_KIND {
            assert_eq!(
                request.lsp.as_ref().map(|payload| &payload.request),
                Some(&RunnerLspRequest::Status)
            );
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                &RunnerLspResultEnvelope::err(
                    "lsp_status_unavailable",
                    "fixture intentionally has no language server",
                )
                .to_stdout_json(),
                "",
            )
            .await;
        } else {
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        }
    }
    (task.await.unwrap(), request_kinds)
}

async fn dispatch_startup_with_plugin_catalog(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: &crate::auth::AuthContext,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut request_kinds = Vec::new();
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "Plugin-aware startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?}: {request_kinds:?}"
        );
        let Some(request) = probe_agent_request_for_instance(runtime, client_id, "inst").await
        else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        if let Some(PluginGatewayRequest::ProjectCatalog { ref project_id }) =
            request.plugin_gateway
        {
            assert_eq!(project_id, "demo");
            request_kinds.push("plugin_project_catalog".to_string());
            complete_startup_plugin_catalog_request(runtime, request).await;
        } else if request.kind == AGENT_LSP_REQUEST_KIND {
            request_kinds.push(request.kind.clone());
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                &RunnerLspResultEnvelope::err(
                    "lsp_status_unavailable",
                    "fixture intentionally has no language server",
                )
                .to_stdout_json(),
                "",
            )
            .await;
        } else {
            request_kinds.push(request.kind.clone());
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        }
    }
    (task.await.unwrap(), request_kinds)
}

async fn dispatch_startup_with_configured_skill_catalog(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: &crate::auth::AuthContext,
    configured_skill: RunnerSkillDescriptor,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut request_kinds = Vec::new();
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "configured-Skill startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?}: {request_kinds:?}"
        );
        let Some(request) = probe_patch_agent_request(runtime, client_id).await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        request_kinds.push(request.kind.clone());
        if request.kind == "skill" {
            let operation: RunnerSkillRequest = serde_json::from_str(
                request
                    .content
                    .as_deref()
                    .expect("typed Runner Skill request"),
            )
            .unwrap();
            assert!(matches!(operation, RunnerSkillRequest::List));
            runtime
                .runner_registry
                .complete(RunnerResultRequest {
                    client_id: client_id.to_string(),
                    runner_instance_id: "inst".to_string(),
                    request_id: request.request_id,
                    exit_code: Some(0),
                    stdout: Some(
                        serde_json::to_string(&RunnerSkillListResponse {
                            format: RUNNER_SKILL_RESPONSE_FORMAT.to_string(),
                            skills: vec![configured_skill.clone()],
                            invalid_count: 0,
                            diagnostics: Vec::new(),
                            discovery_truncated: true,
                        })
                        .unwrap(),
                    ),
                    stderr: Some(String::new()),
                    stdout_truncated: false,
                    stderr_truncated: false,
                    duration_ms: Some(1),
                    error: None,
                })
                .await
                .unwrap();
        } else if request.kind == AGENT_LSP_REQUEST_KIND {
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                &RunnerLspResultEnvelope::err(
                    "lsp_status_unavailable",
                    "fixture intentionally has no language server",
                )
                .to_stdout_json(),
                "",
            )
            .await;
        } else {
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        }
    }
    (task.await.unwrap(), request_kinds)
}

async fn dispatch_startup_without_window(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&crate::auth::AuthContext>,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        async move {
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_window(
                    call,
                    auth.as_ref(),
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                    Default::default(),
                    None,
                )
                .await
        }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "coding startup without window did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

async fn dispatch_with_path_runner(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    agent_project_id: &str,
    project_path: &str,
    outcome: &str,
    registered: bool,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth_context(None, true);
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    loop {
        if task.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "path-based coding call did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            if request.kind == "resolve_or_register_project" {
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                assert_eq!(payload["path"], project_path);
                let response = json!({
                    "id": format!("agent:{client_id}:{agent_project_id}"),
                    "agent_project_id": agent_project_id,
                    "client_id": client_id,
                    "name": agent_project_id,
                    "path": project_path,
                    "kind": "auto_registered",
                    "description": null,
                    "allow_patch": true,
                    "disabled": false,
                    "revision": format!("sha256:{}", "a".repeat(64)),
                    "source": "path",
                    "outcome": outcome,
                    "registered": registered,
                    "created_config": registered,
                    "changed": registered,
                    "recovered": !registered,
                });
                complete_patch_agent_request(
                    runtime,
                    client_id,
                    &request.request_id,
                    0,
                    &response.to_string(),
                    "",
                )
                .await;
            } else {
                complete_agent_request_by_running_locally(runtime, client_id, request).await;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

fn instruction_events(runtime: &ToolRuntime, session_id: &str) -> Vec<SessionEvent> {
    runtime
        .sessions
        .summary(session_id, Some(200))
        .unwrap()
        .events
        .into_iter()
        .filter(|event| event.kind == "task_instruction")
        .collect()
}

fn valid_work_on_project_projection_input() -> serde_json::Value {
    json!({
        "detail": "standard",
        "session": {
            "session_id": "wc_sess_0123456789abcdef",
            "continuation": "created",
            "execution_context": {},
        },
        "project": {
            "resolved_id": "agent:wop:demo",
        },
        "project_resolution": {
            "source": "project",
            "outcome": "resolved_existing_project",
            "resolved_project": "agent:wop:demo",
            "registered": false,
        },
        "workspace": {
            "status": "clean",
            "git": {
                "status": "clean",
                "reason_code": null,
            },
            "git_available": true,
            "branch": "main",
            "head": "0123456789abcdef0123456789abcdef01234567",
            "clean": true,
            "conflicts": 0,
        },
        "workflow": crate::tool_runtime::startup_brief::builtin_coding_workflow_projection(Default::default()),
        "instructions": {
            "status": "loaded",
            "sources": [],
            "content_included": true,
            "truncated": false,
            "total_chars": 0,
        },
        "semantic_navigation": {
            "supported": false,
            "available": false,
            "status": "not_applicable",
            "capability": null,
            "reason_code": "project_not_agent_backed",
        },
        "repository": {
            "status": "unavailable",
            "reason_code": "not_requested_by_work_on_project",
        },
        "continuation": {
            "suggested_next_actions": {
                "items": [],
            },
            "jobs": {
                "active_count": 0,
                "blocking_active_count": 0,
                "nonblocking_active_count": 0,
                "recovering_count": 0,
                "terminal_pending_count": 0,
                "latest_status": "not_observed",
            },
        },
        "blockers": [],
        "warnings": [],
        "startup_verdict": {
            "status": "pass",
            "blocking": false,
            "suggested_next_actions": [],
        },
    })
}

#[test]
fn work_on_project_schema_and_registration() {
    let specs = registered_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(names.contains(&"work_on_project"), "missing from specs");

    // Workflow bootstrap mutates durable Session state without adding a new
    // interactive approval; its existing runtime:read authority is unchanged.
    let metadata = crate::tool_runtime::metadata::lookup_tool_metadata("work_on_project").unwrap();
    assert_eq!(
        metadata.effect,
        crate::tool_runtime::metadata::ToolEffect::Mutate
    );
    assert_eq!(
        metadata.risk,
        crate::tool_runtime::metadata::ToolRisk::WorkflowManage
    );
    assert_eq!(
        metadata.approval,
        crate::tool_runtime::metadata::ToolApprovalPolicy::None
    );
    assert_eq!(
        metadata.idempotency,
        crate::tool_runtime::metadata::ToolIdempotency::NonIdempotent
    );
    assert!(!metadata.destructive);
    assert!(!metadata.shell_like);
    assert!(metadata.requires_project);
    assert_eq!(
        metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require("runtime:read")
    );
    assert_eq!(
        crate::tool_runtime::tool_definition::runtime_tool_category("work_on_project"),
        "workflow"
    );
    let definition =
        crate::tool_runtime::tool_definition::lookup_tool_definition("work_on_project").unwrap();
    assert_eq!(
        definition.runner_capability,
        Some(crate::tool_runtime::RunnerCapabilityRequirement::GitOrShell)
    );
    assert!(!definition.requires_explicit_business_session());

    // Keep the model-facing schema simple enough for reliable host projection.
    // Project-source exclusivity remains authoritative in ToolCall/runtime parsing.
    let spec = spec_named(&specs, "work_on_project");
    assert_eq!(spec.input_schema["type"], "object");
    assert_eq!(required_fields(spec), vec!["instruction"]);
    assert_eq!(spec.input_schema["additionalProperties"], false);
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(!props.contains_key("include_project_instructions"));
    assert!(!props.contains_key("include_workflow_guidance"));
    for field in [
        "project",
        "client_id",
        "path",
        "mode",
        "base_ref",
        "instruction",
        "include_extension_catalog",
        "session_id",
    ] {
        assert!(
            props.contains_key(field),
            "missing explicit {field} property"
        );
    }
    assert_eq!(props["project"]["minLength"], 1);
    assert!(
        props["path"].get("pattern").is_none(),
        "path portability is enforced by ToolCall/runtime parsing, not a POSIX-only schema pattern"
    );
    assert_eq!(props["instruction"]["minLength"], 1);
    assert_eq!(
        props["instruction"]["maxLength"],
        crate::tool_runtime::sessions::MAX_CODING_INSTRUCTION_CHARS
    );
    assert_eq!(props["session_id"]["type"], "string");
    assert_eq!(
        props["session_id"]["pattern"],
        "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
    );
    assert_eq!(props["include_extension_catalog"]["type"], "boolean");
    assert_eq!(props["include_extension_catalog"]["default"], true);
    for keyword in [
        "oneOf",
        "anyOf",
        "allOf",
        "not",
        "dependentRequired",
        "if",
        "then",
        "else",
    ] {
        assert!(
            spec.input_schema.get(keyword).is_none(),
            "work_on_project model schema must not use top-level {keyword}"
        );
    }
    let schema_accepts = |value: Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &value,
            &spec.input_schema,
        )
        .is_ok()
    };
    assert!(schema_accepts(
        json!({"project": SAMPLE_PROJECT, "instruction": "do it"})
    ));
    assert!(!schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "instruction": "do it",
        "include_project_instructions": false
    })));
    assert!(!schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "instruction": "do it",
        "include_workflow_guidance": false
    })));
    assert!(schema_accepts(json!({
        "client_id": "special",
        "path": "/root/git/example",
        "instruction": "do it"
    })));
    assert!(
        schema_accepts(json!({"instruction": "runtime must select the source"})),
        "model schema intentionally advertises a safe source-selection superset"
    );
    assert!(schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "client_id": "special",
        "path": "/root/git/example",
        "instruction": "runtime must reject ambiguity"
    })));

    // The canonical entry must not expose internal diagnostic controls.
    for hidden in [
        "resume_session_id",
        "deny_write_tools",
        "deny_shell_tools",
        "execution_context",
        "detail",
        "temporary_project_name",
    ] {
        assert!(
            !props.contains_key(hidden),
            "work_on_project schema must not expose {hidden}"
        );
    }

    // Output schema describes the compact projection fields.
    let output = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let output_props = output["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "session_id",
        "session_ref",
        "project",
        "resolved_project",
        "project_ref",
        "project_resolution",
        "continuation",
        "execution_context",
        "readiness",
        "workspace",
        "worktree",
        "repository",
        "instructions",
        "semantic_navigation",
        "extensions",
        "jobs",
        "blockers",
        "warnings",
        "suggested_call",
        "suggested_next_actions",
    ] {
        assert!(
            output_props.contains_key(field),
            "work_on_project output schema should include {field}"
        );
    }
    for hidden in [
        "workflow",
        "runtime_status",
        "connection_state",
        "authority",
        "tool_manifest",
        "recommended_flow",
        "startup_verdict",
        "git",
        "continuation_feedback",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            !output_props.contains_key(hidden),
            "work_on_project output schema must not include {hidden}"
        );
    }

    // ToolCall parsing maps the wrapper's session_id to the business accessor.
    let call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do the thing",
            "session_id": "wc_sess_target"
        }),
    )
    .unwrap();
    match &call {
        ToolCall::WorkOnProject {
            include_extension_catalog,
            session_id,
            ..
        } => {
            assert!(*include_extension_catalog);
            assert_eq!(session_id.as_deref(), Some("wc_sess_target"));
        }
        _ => panic!("expected WorkOnProject"),
    }
    assert_eq!(call.project(), Some(SAMPLE_PROJECT));
    assert_eq!(call.session_id(), Some("wc_sess_target"));

    let compact = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do the thing without repeating static context",
            "include_extension_catalog": false
        }),
    )
    .unwrap();
    match compact {
        ToolCall::WorkOnProject {
            include_extension_catalog,
            ..
        } => {
            assert!(!include_extension_catalog);
        }
        _ => panic!("expected WorkOnProject"),
    }
    for removed in ["include_project_instructions", "include_workflow_guidance"] {
        let mut args = json!({"project": SAMPLE_PROJECT, "instruction": "do it"});
        args[removed] = json!(false);
        assert!(ToolCall::from_tool_name("work_on_project", args).is_err());
    }

    let audit = super::super::tool_audit::session_log_arguments_for_tool_request(
        "work_on_project",
        &json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do not persist this full instruction body",
            "include_extension_catalog": false
        }),
    );
    assert!(audit.get("include_project_instructions").is_none());
    assert!(audit.get("include_workflow_guidance").is_none());
    assert_eq!(audit["include_extension_catalog"], false);
    assert_eq!(audit["instruction_present"], true);
    assert!(audit["instruction_summary"].is_string());
    assert!(audit.get("instruction").is_none());
}

#[tokio::test]
async fn work_on_project_extension_catalog_is_defaulted_bounded_and_skips_all_extension_discovery_when_disabled(
) {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    write_project_skill(
        root.path(),
        "00-alpha",
        "duplicate-skill",
        "Alpha selection metadata",
        "ALPHA_PRIVATE_BODY_MUST_NOT_LEAK",
    );
    write_project_skill(
        root.path(),
        "01-beta",
        "duplicate-skill",
        "Beta selection metadata",
        "BETA_PRIVATE_BODY_MUST_NOT_LEAK",
    );
    for index in 2..26 {
        write_project_skill(
            root.path(),
            &format!("{index:02}-bulk"),
            &format!("bulk-{index:02}"),
            &format!("Bulk selection metadata {index:02} {}", "d".repeat(380)),
            "BULK_PRIVATE_BODY_MUST_NOT_LEAK",
        );
    }

    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-ext-skills", "demo", root.path()).await;
    let auth = bootstrap_auth_context();

    let (without_extensions, without_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-skills",
        work_on_project_call_with_extensions(&project, "without extensions", false),
        Some(&auth),
        "wop-ext-skills-off",
    )
    .await;
    assert!(without_extensions.success, "{:?}", without_extensions.error);
    assert!(without_extensions.output.get("extensions").is_none());
    assert!(!without_requests.iter().any(|kind| matches!(
        kind.as_str(),
        "file_skill_list_packages" | "file_skill_read_file"
    )));

    let (with_extensions, with_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-skills",
        work_on_project_call_with_extensions(&project, "with extensions", true),
        Some(&auth),
        "wop-ext-skills-on",
    )
    .await;
    assert!(with_extensions.success, "{:?}", with_extensions.error);
    assert!(with_requests
        .iter()
        .any(|kind| kind == "file_skill_list_packages"));
    assert!(with_requests
        .iter()
        .any(|kind| kind == "file_skill_read_file"));

    let skills = &with_extensions.output["extensions"]["skills"];
    assert_eq!(skills["status"], "available");
    assert_eq!(skills["total_count"], 26);
    assert_eq!(skills["truncated"], true);
    assert!(skills["returned_count"].as_u64().unwrap() < 26);
    let entries = skills["entries"].as_array().unwrap();
    assert!(
        entries.len() >= 2,
        "duplicate fixtures must fit the bounded prefix"
    );
    assert_eq!(entries[0]["name"], "duplicate-skill");
    assert_eq!(entries[0]["name_conflict"], true);
    assert_eq!(entries[0]["source_scope"], "project");
    assert_eq!(entries[0]["trust"], "project_content");
    assert_eq!(entries[1]["name"], "duplicate-skill");
    assert_eq!(entries[1]["name_conflict"], true);

    let plugins = &with_extensions.output["extensions"]["plugins"];
    assert_eq!(plugins["status"], "unavailable");
    assert_eq!(plugins["reason_code"], "plugin_runtime_unavailable");
    let serialized = with_extensions.output.to_string();
    for secret in [
        "ALPHA_PRIVATE_BODY_MUST_NOT_LEAK",
        "BETA_PRIVATE_BODY_MUST_NOT_LEAK",
        "BULK_PRIVATE_BODY_MUST_NOT_LEAK",
        "SKILL.md",
    ] {
        assert!(
            !serialized.contains(secret),
            "startup leaked Skill body/path marker: {secret}"
        );
    }
    let extension_bytes = serde_json::to_vec(&with_extensions.output["extensions"])
        .unwrap()
        .len();
    assert!(
        extension_bytes
            <= crate::tool_runtime::startup_brief::STARTUP_EXTENSION_CATALOG_HARD_MAX_BYTES,
        "extension payload exceeded hard bound: {extension_bytes}"
    );
    let without_bytes = serde_json::to_vec(&without_extensions.output)
        .unwrap()
        .len();
    let with_bytes = serde_json::to_vec(&with_extensions.output).unwrap().len();
    assert!(with_bytes <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES);
    println!(
        "work_on_project_extension_catalog_bytes without={without_bytes} with={with_bytes} increase={}",
        with_bytes.saturating_sub(without_bytes)
    );
}

#[tokio::test]
async fn work_on_project_extension_catalog_includes_runner_local_configured_skill() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        "wop-ext-configured-skill",
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            skill_runtime: true,
            ..Default::default()
        },
    )
    .await;
    let auth = bootstrap_auth_context();
    let configured_id = "wc_skill_IiIiIiIiIiIiIiIiIiIiIg".to_string();
    let configured_revision = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let (result, requests) = dispatch_startup_with_configured_skill_catalog(
        &runtime,
        "wop-ext-configured-skill",
        work_on_project_call_with_extensions(&project, "discover configured Skill", true),
        &auth,
        RunnerSkillDescriptor::Configured {
            skill_id: configured_id.clone(),
            name: "operator-live-guidance".to_string(),
            description: "Configured live Skill metadata".to_string(),
            definition_revision: configured_revision.to_string(),
        },
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(requests.iter().any(|kind| kind == "skill"));
    let skills = &result.output["extensions"]["skills"];
    assert_eq!(skills["status"], "available");
    assert_eq!(skills["total_count"], 1);
    assert_eq!(skills["returned_count"], 1);
    assert_eq!(skills["truncated"], true);
    assert!(skills["discovery_hint"].is_string());
    let entry = &skills["entries"][0];
    assert_eq!(entry["skill_id"], configured_id);
    assert_eq!(entry["name"], "operator-live-guidance");
    assert_eq!(entry["description"], "Configured live Skill metadata");
    assert_eq!(entry["source_scope"], "runner");
    assert_eq!(entry["trust"], "operator_configured_guidance");
    assert_eq!(entry["name_conflict"], false);
    assert!(!result.output.to_string().contains(configured_revision));
}

#[tokio::test]
async fn work_on_project_plugin_extension_uses_project_catalog_without_binding_or_schema_leakage() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        "wop-ext-plugin",
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            native_tool_plugins: true,
            ..Default::default()
        },
    )
    .await;
    let auth = bootstrap_auth_context();
    let bindings_before = runtime.plugin_gateway.binding_count();
    let (result, requests) = dispatch_startup_with_plugin_catalog(
        &runtime,
        "wop-ext-plugin",
        work_on_project_call_with_extensions(&project, "discover plugin", true),
        &auth,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(requests.iter().any(|kind| kind == "plugin_project_catalog"));
    assert_eq!(runtime.plugin_gateway.binding_count(), bindings_before);

    let plugins = &result.output["extensions"]["plugins"];
    assert_eq!(plugins["status"], "available");
    assert_eq!(
        plugins["catalog_revision"],
        format!("wc_plugcat_{}", webcodex_core::compact::encode([0xaa; 32]))
    );
    assert_eq!(plugins["total_count"], 1);
    assert_eq!(plugins["returned_count"], 1);
    assert_eq!(plugins["truncated"], false);
    let entry = &plugins["entries"][0];
    assert_eq!(entry["plugin"], "repo-context");
    assert_eq!(entry["tool"], "repo_context");
    assert_eq!(entry["annotations"]["readOnlyHint"], true);
    assert_eq!(entry["annotations"]["destructiveHint"], false);
    let serialized = result.output["extensions"].to_string();
    for forbidden in [
        root.path().to_string_lossy().as_ref(),
        "inputSchema",
        "outputSchema",
        "provider_instance_id",
        "binding",
        "command",
        "argv",
        "cwd",
        "env",
        "stderr",
        "pid",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "startup leaked {forbidden}: {serialized}"
        );
    }
}

#[tokio::test]
async fn work_on_project_plugin_extension_fails_closed_without_plugin_inspect_scope() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    write_project_skill(
        root.path(),
        "alpha",
        "alpha",
        "Visible Skill metadata",
        "PRIVATE_SCOPE_TEST_BODY",
    );
    let runtime = ToolRuntime::new_for_tests();
    let auth = open_auth_context();
    let project = register_runner_project_at_path_with_auth(
        &runtime,
        "wop-ext-scope",
        "demo",
        root.path(),
        &auth,
    )
    .await;
    assert!(!auth.has_scope(crate::auth::SCOPE_PLUGIN_INSPECT));
    let (result, requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-scope",
        work_on_project_call_with_extensions(&project, "scope bounded startup", true),
        Some(&auth),
        "wop-ext-scope-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["extensions"]["skills"]["status"], "available");
    assert_eq!(
        result.output["extensions"]["plugins"]["status"],
        "unavailable"
    );
    assert_eq!(
        result.output["extensions"]["plugins"]["reason_code"],
        "plugin_inspect_scope_unavailable"
    );
    assert!(!requests.iter().any(|kind| kind == "plugin_gateway"));
    assert!(!result
        .output
        .to_string()
        .contains("PRIVATE_SCOPE_TEST_BODY"));
}

#[test]
fn work_on_project_tool_call_enforces_authoritative_source_contract() {
    assert!(
        ToolCall::from_tool_name("work_on_project", json!({})).is_err(),
        "project source and instruction are required"
    );
    assert!(
        ToolCall::from_tool_name("work_on_project", json!({"project": SAMPLE_PROJECT})).is_err(),
        "instruction is required"
    );
    let project_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({"project": SAMPLE_PROJECT, "instruction": "do it"}),
    )
    .unwrap();
    assert_eq!(project_call.project(), Some(SAMPLE_PROJECT));
    let path_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "special",
            "path": "/root/git/example",
            "instruction": "do it"
        }),
    )
    .unwrap();
    assert!(path_call.project().is_none());
    let worktree_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "special",
            "path": "/root/git/example",
            "mode": "worktree",
            "base_ref": "origin/main",
            "instruction": "do it in isolation"
        }),
    )
    .unwrap();
    match worktree_call {
        ToolCall::WorkOnProject { mode, base_ref, .. } => {
            assert_eq!(mode.as_deref(), Some("worktree"));
            assert_eq!(base_ref.as_deref(), Some("origin/main"));
        }
        _ => panic!("expected WorkOnProject"),
    }
    for invalid in [
        json!({"instruction": "no source"}),
        json!({"project": SAMPLE_PROJECT, "client_id": "special", "instruction": "mixed"}),
        json!({"project": SAMPLE_PROJECT, "path": "/root/git/example", "instruction": "mixed"}),
        json!({"project": SAMPLE_PROJECT, "client_id": "special", "path": "/root/git/example", "instruction": "mixed"}),
        json!({"client_id": "special", "instruction": "missing path"}),
        json!({"path": "/root/git/example", "instruction": "missing client"}),
    ] {
        assert!(
            ToolCall::from_tool_name("work_on_project", invalid.clone()).is_err(),
            "authoritative parser accepted invalid source form: {invalid}"
        );
    }
    // The schema declares additionalProperties: false so advanced
    // internal diagnostic controls are not part of the canonical entry surface.
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "work_on_project")
        .unwrap();
    assert_eq!(spec.input_schema["additionalProperties"], false);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for hidden in [
        "resume_session_id",
        "deny_write_tools",
        "deny_shell_tools",
        "execution_context",
        "detail",
        "temporary_project_name",
    ] {
        assert!(
            !props.contains_key(hidden),
            "work_on_project schema must not expose {hidden}"
        );
    }
}

#[test]
fn work_on_project_projection_fails_closed_when_required_field_is_missing() {
    let mut output = valid_work_on_project_projection_input();
    output["session"]
        .as_object_mut()
        .unwrap()
        .remove("session_id");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
    assert!(result.output["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("session_id")));
}

#[test]
fn work_on_project_projection_preserves_session_ref() {
    let mut output = valid_work_on_project_projection_input();
    output["session"]["session_ref"] = json!("~s12");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_ref"], "~s12");
}

#[test]
fn work_on_project_projection_emits_typed_window_session_correlation() {
    let mut correlation = crate::tool_runtime::ToolCallCorrelation::default();
    let result =
        crate::tool_runtime::coding_task::project_work_on_project_output_with_correlation_for_test(
            SAMPLE_PROJECT.to_string(),
            valid_work_on_project_projection_input(),
            &mut correlation,
        );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        correlation.resolved_project.as_deref(),
        Some("agent:wop:demo")
    );
    assert_eq!(correlation.workflow_sessions.len(), 1);
    let link = &correlation.workflow_sessions[0];
    assert_eq!(link.session_id, "wc_sess_0123456789abcdef");
    assert_eq!(link.project.as_deref(), Some("agent:wop:demo"));
    assert_eq!(
        link.relation,
        crate::tool_runtime::WorkflowSessionCorrelationRelation::WorkOnProject
    );
}

#[test]
fn work_on_project_projection_fails_closed_for_wrong_field_type() {
    let mut output = valid_work_on_project_projection_input();
    output["workspace"]["conflicts"] = json!("0");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
}

#[test]
fn work_on_project_projection_keeps_safe_coding_agent_discovery_and_rejects_private_fields() {
    let mut input = valid_work_on_project_projection_input();
    input["coding_agent_providers"] = json!([{"provider_id":"pi", "name":"Pi Agent"}]);
    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        input.clone(),
    );
    assert!(result.success);
    assert_eq!(
        result.output["coding_agent_providers"],
        input["coding_agent_providers"]
    );
    input["coding_agent_providers"][0]["provider_instance_id"] = json!("private");
    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        input,
    );
    assert!(!result.success);
}

#[test]
fn work_on_project_projection_fails_closed_for_noncanonical_workflow() {
    let mut output = valid_work_on_project_projection_input();
    output["workflow"]["version"] =
        json!(crate::tool_runtime::startup_brief::BUILTIN_CODING_WORKFLOW_VERSION + 1);

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["field"], "workflow");
    assert_eq!(result.output["state_changed"], true);
}

#[test]
fn work_on_project_projection_does_not_default_missing_instruction_sources() {
    let mut output = valid_work_on_project_projection_input();
    output["instructions"]
        .as_object_mut()
        .unwrap()
        .remove("sources");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
    assert!(result.output["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("sources")));
}

#[test]
fn work_on_project_projection_is_sparse_for_defaults_and_keeps_noteworthy_state() {
    let default_result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        valid_work_on_project_projection_input(),
    );
    assert!(default_result.success, "{:?}", default_result.error);
    for omitted in [
        "project_resolution",
        "execution_context",
        "readiness",
        "repository",
        "jobs",
        "blockers",
        "warnings",
        "suggested_next_actions",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            default_result.output.get(omitted).is_none(),
            "boring default field {omitted} should be omitted: {}",
            default_result.output
        );
    }
    assert_eq!(default_result.output["workspace"]["status"], "clean");
    assert_eq!(default_result.output["workspace"]["git"]["status"], "clean");
    assert!(default_result.output["workspace"]["git"]["reason_code"].is_null());
    assert!(default_result.output["workspace"]["branch"].is_string());
    assert!(default_result.output["workspace"]["head"].is_string());
    for omitted in ["git_available", "clean", "conflicts"] {
        assert!(default_result.output["workspace"].get(omitted).is_none());
    }
    assert_eq!(
        default_result.output["instructions"]["content_included"],
        true
    );
    for omitted in ["changed_sources", "truncated", "total_chars"] {
        assert!(default_result.output["instructions"].get(omitted).is_none());
    }

    let mut noteworthy = valid_work_on_project_projection_input();
    noteworthy["session"]["execution_context"] = json!({"default_cwd": "src"});
    noteworthy["project_resolution"] = json!({
        "source": "path",
        "outcome": "auto_registered",
        "resolved_project": "agent:wop:demo",
        "registered": true,
    });
    noteworthy["workspace"]["status"] = json!("blocked");
    noteworthy["workspace"]["conflicts"] = json!(2);
    noteworthy["repository"] = json!({
        "status": "unavailable",
        "reason_code": "probe_failed",
    });
    noteworthy["continuation"]["jobs"]["active_count"] = json!(1);
    noteworthy["continuation"]["jobs"]["latest_status"] = json!("running");
    noteworthy["blockers"] = json!(["workspace_conflicts"]);
    noteworthy["warnings"] = json!(["active_jobs_present"]);
    noteworthy["startup_verdict"] = json!({
        "status": "fail",
        "blocking": true,
        "suggested_next_actions": ["inspect or await blocking active jobs"],
    });
    let noteworthy_result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        noteworthy,
    );
    assert!(noteworthy_result.success, "{:?}", noteworthy_result.error);
    assert_eq!(
        noteworthy_result.output["project_resolution"]["outcome"],
        "auto_registered"
    );
    assert_eq!(
        noteworthy_result.output["execution_context"]["default_cwd"],
        "src"
    );
    assert_eq!(noteworthy_result.output["readiness"]["status"], "fail");
    assert_eq!(
        noteworthy_result.output["repository"]["reason_code"],
        "probe_failed"
    );
    assert_eq!(noteworthy_result.output["workspace"]["conflicts"], 2);
    assert_eq!(noteworthy_result.output["jobs"]["active_count"], 1);
    assert_eq!(noteworthy_result.output["jobs"]["latest_status"], "running");
    assert_eq!(
        noteworthy_result.output["blockers"],
        json!(["workspace_conflicts"])
    );
    assert_eq!(
        noteworthy_result.output["warnings"],
        json!(["active_jobs_present"])
    );
    assert_eq!(
        noteworthy_result.output["suggested_next_actions"],
        json!(["inspect or await blocking active jobs"])
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": noteworthy_result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("noteworthy sparse output must match schema: {error}"));
}

#[tokio::test]
async fn work_on_project_without_session_id_always_creates_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-create", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let result = dispatch_coding_call_in_window(
        &runtime,
        "wop-create",
        work_on_project_call("demo", "first root instruction", None),
        Some(&auth),
        "wop-create-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);

    // Compact projection keeps identity and omits boring default metadata.
    let session_id = result.output["session_id"].as_str().unwrap().to_string();
    assert!(session_id.starts_with("wc_sess_"));
    assert_eq!(result.output["project"], "demo");
    assert_eq!(result.output["resolved_project"], project);
    assert_eq!(result.output["continuation"], "created");
    for omitted in [
        "project_resolution",
        "execution_context",
        "repository",
        "jobs",
        "blockers",
        "suggested_next_actions",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "boring default field {omitted} should be omitted: {}",
            result.output
        );
    }
    // A failed advisory status probe remains non-blocking, while static model
    // guidance is now opt-in through context_request rather than primary output.
    assert!(result.output.get("readiness").is_none());
    assert!(result.output.get("warnings").is_none());
    assert_eq!(
        result.output["semantic_navigation"]["status"],
        "probe_failed"
    );
    assert_eq!(
        result.output["semantic_navigation"]["available"],
        Value::Null
    );
    assert!(result.output.get("workflow").is_none());
    assert!(result.output["instructions"].is_object());
    assert!(result.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(result.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
    for hidden in [
        "runtime_status",
        "connection_state",
        "authority",
        "tool_manifest",
        "recommended_flow",
        "startup_verdict",
        "git",
        "continuation_feedback",
    ] {
        assert!(
            !result.output.as_object().unwrap().contains_key(hidden),
            "compact output must not include {hidden}"
        );
    }

    // A new active normal session was created with the instruction as root.
    let summary = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    let instructions = instruction_events(&runtime, &session_id);
    assert_eq!(instructions.len(), 1);
    assert_eq!(
        instructions[0].instruction.as_deref(),
        Some("first root instruction")
    );

    // A second call in the same window/project without session_id must create a
    // distinct Workflow Session instead of continuing the first implicitly.
    let second = dispatch_coding_call_in_window(
        &runtime,
        "wop-create",
        work_on_project_call("demo", "second root instruction", None),
        Some(&auth),
        "wop-create-window",
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    let second_session_id = second.output["session_id"].as_str().unwrap();
    assert_ne!(second_session_id, session_id);
    assert_eq!(second.output["continuation"], "created");
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        2
    );

    // Compact workspace/instruction projection reflects the underlying brief.
    assert!(result.output["workspace"]["branch"].is_string());
    assert!(result.output["instructions"]["status"].is_string());

    // The actual compact projection validates against its output schema.
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("compact output must match its schema: {error}"));

    // B: an ordinary project tool stays unrecorded when neither a business
    // Session nor an explicit recording_session_id is supplied, even in the
    // same stable window that created the Workflow Sessions above.
    let first_before = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();
    let second_before = runtime
        .sessions
        .summary(second_session_id, Some(200))
        .unwrap()
        .events
        .len();
    let ordinary = call_hygiene_in_window_with_local_runner_transport(
        &runtime,
        "wop-create",
        &project,
        None,
        None,
        &auth,
        "ordinary_project_tool_without_explicit_session_is_unrecorded",
        ToolTransport::Api,
    )
    .await;
    assert!(ordinary.success, "{:?}", ordinary.error_status);
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        first_before
    );
    assert_eq!(
        runtime
            .sessions
            .summary(second_session_id, Some(200))
            .unwrap()
            .events
            .len(),
        second_before
    );
}

#[tokio::test]
async fn same_window_recorder_gap_is_visible_without_backfilling_session_ledger() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("window-activity.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(window_db.clone());
    let project = register_runner_project_at_path(&runtime, "wop-gap", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let window_id = "wop-recorder-gap-window";
    let window = crate::client_window::ClientWindow::for_test(window_id);

    // T1: the first work_on_project creates S and establishes the canonical
    // Window <-> Session relation even though no outer recorder existed yet.
    let created = dispatch_coding_call_in_window(
        &runtime,
        "wop-gap",
        work_on_project_call("demo", "create recorder-gap fixture", None),
        Some(&auth),
        window_id,
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let session_id = created.output["session_id"].as_str().unwrap().to_string();
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    // T2: an explicitly authorized outer recorder advances S normally.
    let recorded = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        Some(&session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(recorded.success, "{:?}", recorded.error_status);
    assert!(recorded.correlation.recorder_gap_session_id.is_none());
    assert!(recorded.correlation.business_session_id.is_none());
    assert!(recorded.correlation.workflow_sessions.iter().any(|link| {
        link.session_id == session_id
            && link.relation == crate::tool_runtime::WorkflowSessionCorrelationRelation::Recording
    }));
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "workspace_hygiene_check",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::Recording,
        )),
        None,
        2_000,
    );
    let recorded_event_count = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();
    let guidance = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "same-window attention without recorder".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();

    // T3: omitting recording_session_id does not block business execution and
    // does not forge a Session event. The exact same Window/principal/Project
    // affinity produces a bounded recovery hint and a Window-only gap fact.
    let unrecorded = call_hygiene_in_window_with_local_runner(
        &runtime, "wop-gap", &project, None, None, &auth, window_id,
    )
    .await;
    assert!(unrecorded.success, "{:?}", unrecorded.error_status);
    assert!(unrecorded.correlation.business_session_id.is_none());
    assert_eq!(
        unrecorded.correlation.recorder_gap_session_id.as_deref(),
        Some(session_id.as_str())
    );
    let unrecorded_result = unrecorded.result.as_ref().expect("tool result");
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["status"],
        "recording_session_missing"
    );
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["candidate_session_id"],
        session_id
    );
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["project"],
        project
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["session_id"],
        session_id
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["source"],
        "window_affinity"
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["messages"][0]["message_id"],
        guidance.message_id
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["messages"][0]["message"],
        "same-window attention without recorder"
    );
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        recorded_event_count,
        "missing recorder must not backfill or mutate the Workflow Session ledger"
    );

    let acknowledged = call_hygiene_in_window_with_local_runner_metadata(
        &runtime,
        "wop-gap",
        &project,
        None,
        None,
        &auth,
        window_id,
        ToolTransport::Mcp,
        ToolInvocationMetadata {
            ack_session_message_ids: vec![guidance.message_id.clone()],
            ..Default::default()
        },
    )
    .await;
    assert!(acknowledged.success, "{:?}", acknowledged.error_status);
    let acknowledged = acknowledged.result.unwrap();
    assert_eq!(
        acknowledged.output["session_attention"]["session_id"],
        session_id
    );
    assert_eq!(
        acknowledged.output["session_attention"]["ack"]["accepted_count"],
        1
    );
    assert!(acknowledged.output["session_attention"]["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(runtime
        .sessions
        .list_messages(
            &session_id,
            crate::tool_runtime::sessions::ListSessionMessagesFilter {
                message_id: Some(guidance.message_id.clone()),
                ..Default::default()
            }
        )
        .unwrap()[0]
        .first_ack_observed_at
        .is_some());
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        recorded_event_count,
        "fallback ACK must not create a recorder ToolCall event"
    );

    let forgotten = call_hygiene_in_window_with_local_runner(
        &runtime, "wop-gap", &project, None, None, &auth, window_id,
    )
    .await;
    assert_eq!(
        forgotten.result.unwrap().output["session_attention"]["messages"][0]["message_id"],
        guidance.message_id,
        "historical first ACK observation is not durable resolution"
    );

    // The recorder gap remains auditable, but an exact business Session equal to
    // the authorized same-Window candidate makes the model-facing reminder
    // redundant. The explicit business Session still receives its canonical
    // tool event; no recorder event is forged.
    let business_bound = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        None,
        Some(&session_id),
        &auth,
        window_id,
    )
    .await;
    assert!(business_bound.success, "{:?}", business_bound.error_status);
    assert_eq!(
        business_bound
            .correlation
            .recorder_gap_session_id
            .as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        business_bound.correlation.business_session_id.as_deref(),
        Some(session_id.as_str())
    );
    assert!(business_bound
        .result
        .as_ref()
        .expect("business-bound result")
        .output
        .get("workflow_recording_attention")
        .is_none());
    assert!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len()
            > recorded_event_count,
        "explicit business Session must retain canonical business recording"
    );

    let different_session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("different business session".to_string()),
    );
    let different_business = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        None,
        Some(&different_session.session_id),
        &auth,
        window_id,
    )
    .await;
    assert!(
        different_business.success,
        "{:?}",
        different_business.error_status
    );
    assert_eq!(
        different_business
            .correlation
            .recorder_gap_session_id
            .as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        different_business
            .correlation
            .business_session_id
            .as_deref(),
        Some(different_session.session_id.as_str())
    );
    assert_eq!(
        different_business
            .result
            .as_ref()
            .expect("different-business result")
            .output["workflow_recording_attention"]["candidate_session_id"],
        session_id
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "workspace_hygiene_check",
        None,
        unrecorded.correlation.recorder_gap_session_id.as_deref(),
        3_000,
    );

    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    let window_rows = window_db
        .list_window_activity_events(window.key(), Some((&principal_kind, &principal_id)), 20)
        .unwrap();
    assert_eq!(
        window_rows
            .iter()
            .filter(|event| event.recorder_gap_session_id.as_deref() == Some(session_id.as_str()))
            .count(),
        1
    );

    // T4: explicitly echoing S restores normal recording. The original gap is
    // retained as history, but does not propagate to the recovered call.
    let recovered = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        Some(&session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(
        recovered.success,
        "transport={:?} result={:?}",
        recovered.error_status, recovered.result
    );
    assert!(recovered.correlation.recorder_gap_session_id.is_none());
    assert!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len()
            > recorded_event_count
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "workspace_hygiene_check",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::Recording,
        )),
        None,
        4_000,
    );
    let final_rows = window_db
        .list_window_activity_events(window.key(), Some((&principal_kind, &principal_id)), 20)
        .unwrap();
    assert_eq!(
        final_rows
            .iter()
            .filter(|event| event.recorder_gap_session_id.is_some())
            .count(),
        1,
        "recorder recovery must not propagate the prior gap"
    );
    let affinity = window_db
        .latest_window_workflow_affinity(window.key(), &principal_kind, &principal_id, &project)
        .unwrap()
        .unwrap();
    assert_eq!(affinity.workflow_session_id, session_id);
    assert_eq!(affinity.relation, "recording");
}

#[tokio::test]
async fn window_affinity_attention_is_strict_and_explicit_recorder_keeps_precedence() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("attention-strict.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(window_db.clone());
    let project =
        register_runner_project_at_path(&runtime, "attention-strict", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let window_id = "attention-strict-window";
    let affinity = runtime
        .sessions
        .start_session(Some(project.clone()), Some("affinity target".to_string()));
    let affinity_message = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: affinity.session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "affinity-only guidance".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &affinity.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    let wrong_window = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        "attention-strict-other-window",
    )
    .await;
    assert!(wrong_window.success);
    assert!(wrong_window
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let other_principal = auth_context(Some("different-principal"), true);
    let wrong_principal = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &other_principal,
        window_id,
    )
    .await;
    assert!(wrong_principal.success);
    assert!(wrong_principal
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let wrong_project_window = "attention-strict-wrong-project";
    let unrelated_project = "agent:other:unregistered-project";
    let unrelated = runtime.sessions.start_session(
        Some(unrelated_project.to_string()),
        Some("wrong project".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        wrong_project_window,
        unrelated_project,
        "work_on_project",
        Some((
            &unrelated.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        2_000,
    );
    let wrong_project = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        wrong_project_window,
    )
    .await;
    assert!(wrong_project.success);
    assert!(wrong_project
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let closed_window = "attention-strict-closed";
    let closed = runtime
        .sessions
        .start_session(Some(project.clone()), Some("closed affinity".to_string()));
    record_window_activity_fixture(
        &window_db,
        &auth,
        closed_window,
        &project,
        "work_on_project",
        Some((
            &closed.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        3_000,
    );
    runtime.sessions.close_session(&closed.session_id).unwrap();
    let inactive = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        closed_window,
    )
    .await;
    assert!(inactive.success);
    assert!(inactive
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let recorder = runtime
        .sessions
        .start_session(Some(project.clone()), Some("explicit recorder".to_string()));
    let recorder_message = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: recorder.session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "explicit recorder guidance".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::Normal,
            },
            true,
        )
        .unwrap();
    let explicit = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        Some(&recorder.session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(explicit.success);
    let explicit_output = &explicit.result.as_ref().unwrap().output;
    assert_eq!(
        explicit_output["session_attention"]["session_id"],
        recorder.session_id
    );
    assert_eq!(
        explicit_output["session_attention"]["source"],
        "recording_session"
    );
    assert_eq!(
        explicit_output["session_attention"]["messages"][0]["message_id"],
        recorder_message.message_id
    );
    assert_ne!(
        explicit_output["session_attention"]["messages"][0]["message_id"],
        affinity_message.message_id
    );

    let resolution_without_recorder = call_hygiene_in_window_with_local_runner_metadata(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        window_id,
        ToolTransport::Mcp,
        ToolInvocationMetadata {
            session_message_resolution: Some(
                crate::tool_runtime::sessions::ToolCallSessionMessageResolution {
                    message_id: affinity_message.message_id.clone(),
                    resolution: "must stay explicit".to_string(),
                },
            ),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        resolution_without_recorder.error_status,
        Some(
            crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments {
                message: "session_message_resolution requires recording_session_id".to_string(),
            }
        )
    );
    assert!(resolution_without_recorder.result.is_none());
    let retained = runtime
        .sessions
        .list_messages(
            &affinity.session_id,
            crate::tool_runtime::sessions::ListSessionMessagesFilter {
                message_id: Some(affinity_message.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        retained[0].status,
        crate::tool_runtime::sessions::SessionMessageStatus::Open
    );
    assert!(retained[0].resolution.is_none());
}

#[tokio::test]
async fn workflow_resume_context_is_window_principal_scoped_bounded_and_non_authoritative() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("workflow-resume.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests()
        .with_window_activity_database(window_db.clone())
        .with_project_reference_database(window_db.clone());
    let project =
        register_runner_project_at_path(&runtime, "workflow-resume", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let owner_authority = crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth))
        .expect("test auth has stable Workflow Session authority");
    let start_owned_session = |project: Option<String>, title: Option<String>| {
        runtime
            .sessions
            .start_session_with_options(
                crate::tool_runtime::sessions::SessionCreateOptions::new(
                    project,
                    title,
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(owner_authority.clone())),
            )
            .unwrap()
    };
    let window_id = "workflow-resume-window";
    let window = crate::client_window::ClientWindow::for_test(window_id);

    assert_eq!(
        runtime
            .workflow_resume_context_projection_for_test(None, Some(&auth))
            .await
            .unwrap_err(),
        "client_window_unavailable"
    );

    let empty = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(empty["count"], 0);
    assert!(empty.get("suggested_call").is_none());

    let first = start_owned_session(
        Some(project.clone()),
        Some("implementation recovery candidate".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &first.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    let single = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(single["count"], 1);
    assert_eq!(single["candidates"][0]["session_id"], first.session_id);
    let first_ref = single["candidates"][0]["session_ref"]
        .as_str()
        .expect("authorized discovery candidate should expose a short Session selector");
    assert!(first_ref.starts_with("~s"));
    assert_eq!(
        single["suggested_call"]["arguments"]["session_id"],
        first_ref
    );

    let other_window = crate::client_window::ClientWindow::for_test("workflow-resume-other");
    let hidden_by_window = runtime
        .workflow_resume_context_projection_for_test(Some(&other_window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(hidden_by_window["count"], 0);

    let other_principal = auth_context(Some("different-user"), false);
    let hidden_by_principal = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&other_principal))
        .await
        .unwrap();
    assert_eq!(hidden_by_principal["count"], 0);

    let inaccessible_project = "agent:missing:workflow-resume";
    let inaccessible = start_owned_session(
        Some(inaccessible_project.to_string()),
        Some("must-not-leak-secret-title".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        inaccessible_project,
        "work_on_project",
        Some((
            &inaccessible.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_500,
    );
    let hidden_by_project_auth = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(hidden_by_project_auth["count"], 1);
    let hidden_json = serde_json::to_string(&hidden_by_project_auth).unwrap();
    assert!(!hidden_json.contains(&inaccessible.session_id));
    assert!(!hidden_json.contains("must-not-leak-secret-title"));

    let second = start_owned_session(
        Some(project.clone()),
        Some("independent review recovery candidate".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &second.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        2_000,
    );
    let multiple = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(multiple["count"], 2);
    assert_eq!(multiple["candidates"][0]["session_id"], second.session_id);
    assert_eq!(multiple["candidates"][1]["session_id"], first.session_id);
    assert!(multiple.get("suggested_call").is_none());
    runtime.sessions.close_session(&first.session_id).unwrap();
    let active_only = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(active_only["count"], 1);
    assert_eq!(
        active_only["candidates"][0]["session_id"],
        second.session_id
    );
    let second_ref = active_only["candidates"][0]["session_ref"]
        .as_str()
        .expect("remaining authorized candidate should keep its short selector");
    assert_eq!(
        active_only["suggested_call"]["arguments"]["session_id"],
        second_ref
    );

    let mut newest_session_id = String::new();
    for index in 0..8 {
        let extra = start_owned_session(
            Some(project.clone()),
            Some(format!("bounded recovery candidate {index}")),
        );
        newest_session_id = extra.session_id.clone();
        record_window_activity_fixture(
            &window_db,
            &auth,
            window_id,
            &project,
            "work_on_project",
            Some((
                &extra.session_id,
                crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
            )),
            None,
            3_000 + i64::from(index),
        );
    }
    let bounded = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(bounded["count"], 8);
    assert_eq!(bounded["truncated"], true);
    assert_eq!(
        bounded["candidates"][0]["session_id"], newest_session_id,
        "newest active candidate must sort first"
    );
    assert!(bounded.get("suggested_call").is_none());

    let scan_bound_window_id = "workflow-resume-scan-bound";
    let scan_bound_window = crate::client_window::ClientWindow::for_test(scan_bound_window_id);
    for index in 0..100 {
        let missing_session = format!("wc_sess_scan{index:012}");
        record_window_activity_fixture(
            &window_db,
            &auth,
            scan_bound_window_id,
            &project,
            "work_on_project",
            Some((
                &missing_session,
                crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
            )),
            None,
            10_000 + i64::from(index),
        );
    }
    let scan_bound = runtime
        .workflow_resume_context_projection_for_test(Some(&scan_bound_window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(scan_bound["count"], 0);
    assert_eq!(
        scan_bound["truncated"], true,
        "saturating the bounded relation scan must not claim exhaustive recovery"
    );

    assert_eq!(
        runtime.sessions.lifecycle_state(&second.session_id),
        Some(crate::tool_runtime::sessions::SessionLifecycle::Active),
        "projection must not alter Session lifecycle"
    );
}

#[tokio::test]
async fn managed_worktree_bootstrap_recovers_same_operation_and_binds_session_to_final_project() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let managed = root.path().join("managed");
    let base_sha = seed_managed_tool_runtime_fixture(&source, &managed);
    let source_path = source.canonicalize().unwrap().to_string_lossy().to_string();
    let managed_path = managed
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let source_status_before = managed_fixture_git(&source, &["status", "--porcelain"]);
    let reference_db_dir = tempfile::tempdir().unwrap();
    let reference_db = std::sync::Arc::new(
        crate::Database::open(&reference_db_dir.path().join("project-refs.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(reference_db);
    let client_id = "wop-managed";
    let mut source_project = registered_project("source", &source_path);
    source_project.root_fingerprint = Some(managed_source_root_fingerprint());
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            managed_worktree: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![source_project],
    )
    .await;

    let (first, payloads) = dispatch_with_managed_worktree_runner(
        &runtime,
        client_id,
        worktree_work_on_project_call(
            client_id,
            &source_path,
            "work in an isolated checkout",
            Some("HEAD"),
            None,
        ),
        &source_path,
        &managed_path,
        "managed-a1b2c3d4",
        "HEAD",
        &base_sha,
        true,
        "managed_worktree_recovered",
        false,
        true,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(
        payloads.len(),
        2,
        "indeterminate response should re-observe once"
    );
    assert_eq!(payloads[0]["operation_id"], payloads[1]["operation_id"]);
    assert!(payloads[0]["resume_project_id"].is_null());
    assert_eq!(payloads[0]["base_ref"], "HEAD");
    let project = "agent:wop-managed:managed-a1b2c3d4";
    assert_eq!(first.output["resolved_project"], project);
    assert_eq!(first.output["project"], project);
    let project_ref = first.output["project_ref"]
        .as_str()
        .expect("managed worktree bootstrap must return a short Project ref")
        .to_string();
    assert!(project_ref.starts_with("~p"));
    assert!(project_ref.len() < project.len());
    assert_eq!(first.output["worktree"]["managed"], true);
    assert_eq!(first.output["worktree"]["base_ref"], "HEAD");
    assert_eq!(first.output["worktree"]["base_sha"], base_sha);
    assert_eq!(first.output["worktree"]["source_dirty"], true);
    assert_eq!(
        first.output["knowledge_association"]["kind"],
        "managed_worktree_source"
    );
    assert_eq!(first.output["knowledge_association"]["status"], "available");
    assert_eq!(
        first.output["knowledge_association"]["source_project"],
        "agent:wop-managed:source"
    );
    assert_eq!(first.output["knowledge_association"]["base_sha"], base_sha);
    assert_eq!(first.output["knowledge_association"]["read_through"], false);
    assert_eq!(
        first.output["project_resolution"]["source"],
        "managed_worktree"
    );
    assert_eq!(
        first.output["project_resolution"]["outcome"],
        "managed_worktree_recovered"
    );
    assert_eq!(
        first.output["project_resolution"]["registered"], true,
        "lost-response recovery must preserve call-level registration mutation"
    );
    assert!(first.output["project_resolution"].get("worktree").is_none());
    let compact = first.output.to_string();
    assert!(!compact.contains(&source_path));
    assert!(!compact.contains(&managed_path));
    assert!(!compact.contains(&managed_source_root_fingerprint()));
    assert!(!compact.contains("managed_operation_id"));
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let session = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(session.project.as_deref(), Some(project));

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert!(listed.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == project));

    let read = ToolCall::from_tool_name(
        "read_files",
        json!({"project": project_ref, "session_id": session_id, "items": [{"path": "hello.txt"}]}),
    )
    .unwrap();
    let read =
        dispatch_startup_without_window(&runtime, client_id, read, Some(&auth_context(None, true)))
            .await;
    assert!(read.success, "{:?}", read.error);
    assert!(read.output["items"][0]["output"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("committed")));
    assert_eq!(
        std::fs::read_to_string(source.join("hello.txt")).unwrap(),
        "dirty source only\n"
    );
    assert_eq!(
        managed_fixture_git(&source, &["status", "--porcelain"]),
        source_status_before
    );

    let (resumed, resume_payloads) = dispatch_with_managed_worktree_runner(
        &runtime,
        client_id,
        worktree_work_on_project_call(
            client_id,
            &source_path,
            "continue the exact isolated checkout",
            None,
            Some(&session_id),
        ),
        &source_path,
        &managed_path,
        "managed-a1b2c3d4",
        "HEAD",
        &base_sha,
        true,
        "managed_worktree_recovered",
        false,
        false,
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session_id"], session_id);
    assert_eq!(resumed.output["continuation"], "resumed_explicitly");
    assert_eq!(resumed.output["project_ref"], project_ref);
    assert_eq!(resume_payloads.len(), 1);
    assert_eq!(resume_payloads[0]["resume_project_id"], "managed-a1b2c3d4");
    assert!(resume_payloads[0]["base_ref"].is_null());
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(50))
            .unwrap()
            .project
            .as_deref(),
        Some(project)
    );

    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({"success": true, "output": first.output});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("managed worktree output must match schema: {error}"));
}

#[tokio::test]
async fn managed_worktree_invalid_arguments_and_authority_fail_before_runner_mutation() {
    let runtime = ToolRuntime::new_for_tests();
    let root = tempfile::tempdir().unwrap();
    let source_path = root
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let invalid = runtime
        .dispatch(ToolCall::WorkOnProject {
            project: String::new(),
            client_id: Some("unresolved-runner".to_string()),
            path: Some(source_path.clone()),
            mode: Some("checkout".to_string()),
            base_ref: Some("main".to_string()),
            instruction: "must fail before resolution".to_string(),
            guidance_profile: Default::default(),
            include_extension_catalog: false,
            session_id: None,
        })
        .await;
    assert!(!invalid.success);
    assert_eq!(invalid.output["error_kind"], "invalid_arguments");
    assert_eq!(invalid.output["field"], "base_ref");
    assert_eq!(invalid.output["state_changed"], false);

    let client_id = "wop-managed-no-cap";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            project_path_registration: true,
            managed_worktree: false,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let unavailable = runtime
        .dispatch_with_auth(
            worktree_work_on_project_call(client_id, &source_path, "no mutation", None, None),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!unavailable.success);
    assert_eq!(
        unavailable.output["error_kind"],
        "agent_capability_unavailable"
    );
    assert_eq!(unavailable.output["state_changed"], false);
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    let restricted = ToolRuntime::new_for_tests()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::Restricted));
    let restricted_client = "wop-managed-restricted";
    register_agent_with_projects(
        &restricted,
        restricted_client,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            managed_worktree: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let denied = restricted
        .dispatch_with_auth(
            worktree_work_on_project_call(
                restricted_client,
                &source_path,
                "permission must precede prepare",
                None,
                None,
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "permission_denied");
    assert!(probe_patch_agent_request(&restricted, restricted_client)
        .await
        .is_none());
}

#[tokio::test]
async fn source_project_session_cannot_resume_a_managed_worktree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    init_git_repo(&source);
    let source_path = source.canonicalize().unwrap().to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-managed-source-session";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            managed_worktree: true,
            ..Default::default()
        },
        vec![registered_project("source", &source_path)],
    )
    .await;
    let source_project = format!("agent:{client_id}:source");
    let auth = auth_context(None, true);
    let started = dispatch_coding_call_in_window(
        &runtime,
        client_id,
        work_on_project_call(&source_project, "source task", None),
        Some(&auth),
        "source-session-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let source_path = source_path.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    worktree_work_on_project_call(
                        client_id,
                        &source_path,
                        "must not switch workspace",
                        None,
                        Some(&session_id),
                    ),
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "prepare_managed_worktree");
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(payload["resume_project_id"], "source");
    let error = json!({
        "error_code": "managed_worktree_resume_mismatch",
        "error_kind": "managed_worktree_resume_mismatch",
        "failure_kind": "managed_worktree_resume_mismatch",
        "state_changed": false,
    });
    complete_patch_agent_request(
        &runtime,
        client_id,
        &request.request_id,
        1,
        &error.to_string(),
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "managed_worktree_resume_mismatch"
    );
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(instruction_events(&runtime, &session_id).len(), 1);
    assert_eq!(runtime.list_projects(Some(&auth)).await.output["count"], 1);
}

#[tokio::test]
async fn path_source_unknown_runner_returns_parser_ready_discovery_recovery() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-missing-runner";
    let auth = auth_context(None, true);

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "work_on_project".to_string(),
                arguments: json!({
                    "client_id": client_id,
                    "path": project_path,
                    "instruction": "recover missing runner",
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(
        outcome.error_status.is_none(),
        "unexpected transport error: {:?}",
        outcome.error_status
    );
    let result = outcome.result.expect("tool result");

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_runner");
    assert_eq!(result.output["failure_kind"], "unknown_runner");
    assert_eq!(result.output["client_id"], client_id);
    assert_eq!(result.output["state_changed"], false);
    let suggested = &result.output["suggested_call"];
    assert_eq!(suggested["tool"], "list_runners");
    assert_eq!(
        suggested["arguments"],
        json!({"include_projects": false, "summary_only": true})
    );
    assert!(ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .is_ok());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({"success": false, "output": result.output, "error": result.error});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("unknown Runner recovery must match schema: {error}"));
}

#[tokio::test]
async fn path_source_auto_registers_reuses_and_supports_canonical_coding_entry() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    std::fs::write(root.path().join("hello.txt"), "hello\n").unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-path";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let first = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(client_id, &project_path, "first path instruction", None),
        "repo-a1b2c3d4",
        &project_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["project_resolution"]["source"], "path");
    assert_eq!(
        first.output["project_resolution"]["outcome"],
        "auto_registered"
    );
    assert_eq!(first.output["project_resolution"]["registered"], true);
    assert_eq!(first.output["permission"]["status"], "auto_approved");
    assert_eq!(first.output["permission"]["tool_name"], "register_project");
    assert_eq!(
        first.output["resolved_project"],
        "agent:wop-path:repo-a1b2c3d4"
    );
    assert!(
        !first.output.to_string().contains(&project_path),
        "compact work_on_project output leaked the absolute input path"
    );
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    let second = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &project_path,
            "second path instruction",
            Some(&session_id),
        ),
        "repo-a1b2c3d4",
        &project_path,
        "reused_existing_registration",
        false,
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["session_id"], session_id);
    assert_eq!(second.output["continuation"], "resumed_explicitly");
    assert_eq!(second.output["permission"]["status"], "auto_approved");
    assert_eq!(second.output["permission"]["tool_name"], "register_project");
    assert_eq!(
        second.output["project_resolution"]["outcome"],
        "reused_existing_registration"
    );
    assert_eq!(instruction_events(&runtime, &session_id).len(), 2);

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert!(listed.success);
    assert!(listed.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["id"] == "agent:wop-path:repo-a1b2c3d4"
            && project["source"] == "auto_registered"));

    let read = ToolCall::from_tool_name(
        "read_files",
        json!({
            "project": "agent:wop-path:repo-a1b2c3d4",
            "session_id": session_id,
            "items": [{"path": "hello.txt"}]
        }),
    )
    .unwrap();
    let read = dispatch_with_path_runner(
        &runtime,
        client_id,
        read,
        "repo-a1b2c3d4",
        &project_path,
        "reused_existing_registration",
        false,
    )
    .await;
    assert!(read.success, "{:?}", read.error);
    assert!(read.output["items"][0]["output"]["text"]
        .as_str()
        .is_some_and(|content| content.contains("hello")));
}

#[tokio::test]
async fn path_source_explicit_session_mismatch_fails_before_registration() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    init_git_repo(first_root.path());
    init_git_repo(second_root.path());
    let first_path = first_root.path().canonicalize().unwrap();
    let second_path = second_root.path().canonicalize().unwrap();
    let first_path = first_path.to_string_lossy().to_string();
    let second_path = second_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-path-mismatch";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let first = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(client_id, &first_path, "first project", None),
        "first-a1b2c3d4",
        &first_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(first.success);
    let session_id = first.output["session_id"].as_str().unwrap();
    let mismatch = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &second_path,
            "must not fall back",
            Some(session_id),
        ),
        "second-a1b2c3d4",
        &second_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    assert_eq!(mismatch.output["state_changed"], false);
    assert!(mismatch.output.get("permission").is_none());
    assert!(mismatch.output.get("project_resolution").is_none());
    assert_eq!(
        mismatch.output["request_project"],
        format!("path:{client_id}:{second_path}")
    );
    assert_eq!(instruction_events(&runtime, session_id).len(), 1);

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert_eq!(listed.output["count"], 1);
}

#[tokio::test]
async fn path_source_unknown_session_fails_before_registration() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-path-unknown";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &project_path,
            "unknown must not fall back",
            Some("wc_sess_fedcba9876543210"),
        ),
        "unknown-a1b2c3d4",
        &project_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_session_id");
    assert!(result.output.get("permission").is_none());
    assert!(result.output.get("project_resolution").is_none());
    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert_eq!(listed.output["count"], 0);
}

#[tokio::test]
async fn path_source_cross_project_recording_session_fails_before_registration() {
    let recorder_root = tempfile::tempdir().unwrap();
    let target_root = tempfile::tempdir().unwrap();
    init_git_repo(recorder_root.path());
    init_git_repo(target_root.path());
    let target_path = target_root.path().canonicalize().unwrap();
    let target_path = target_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let recorder_project = register_runner_project_at_path(
        &runtime,
        "wop-recorder-owner",
        "recorder",
        recorder_root.path(),
    )
    .await;
    let target_client = "wop-recorder-target";
    register_agent_with_projects(
        &runtime,
        target_client,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let auth = auth_context(None, true);
    let recorder = runtime.sessions.start_session(
        Some(recorder_project.clone()),
        Some("path recorder boundary".to_string()),
    );
    let recorder_session_id = recorder.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let target_path = target_path.clone();
        let recorder_session_id = recorder_session_id.clone();
        async move {
            runtime
                .call_tool_with_context(
                    crate::tool_runtime::kernel::ToolCallRequest {
                        tool_name: "work_on_project".to_string(),
                        arguments: json!({
                            "client_id": target_client,
                            "path": target_path,
                            "instruction": "bootstrap a different project through its path"
                        }),
                    },
                    crate::tool_runtime::kernel::ToolCallContext {
                        transport: crate::tool_runtime::kernel::ToolTransport::Api,
                        session_id: Some(&recorder_session_id),
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: true,
                        host_file_import_trust:
                            crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                    },
                )
                .await
        }
    });

    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    loop {
        if task.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "kernel path bootstrap did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {target_client}"
        );
        if let Some(request) = probe_patch_agent_request(&runtime, target_client).await {
            if request.kind == "resolve_or_register_project" {
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                assert_eq!(payload["path"], target_path);
                let response = json!({
                    "id": "agent:wop-recorder-target:target-a1b2c3d4",
                    "agent_project_id": "target-a1b2c3d4",
                    "client_id": target_client,
                    "name": "target-a1b2c3d4",
                    "path": target_path,
                    "kind": "auto_registered",
                    "description": null,
                    "allow_patch": true,
                    "disabled": false,
                    "revision": format!("sha256:{}", "a".repeat(64)),
                    "source": "path",
                    "outcome": "auto_registered",
                    "registered": true,
                    "created_config": true,
                    "changed": true,
                    "recovered": false,
                });
                complete_patch_agent_request(
                    &runtime,
                    target_client,
                    &request.request_id,
                    0,
                    &response.to_string(),
                    "",
                )
                .await;
            } else {
                complete_agent_request_by_running_locally(&runtime, target_client, request).await;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    let outcome = task.await.unwrap();
    assert!(!outcome.success);
    let result = outcome.result.expect("work_on_project mismatch result");
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    assert_eq!(result.output["failure_kind"], "session_project_mismatch");
    assert_eq!(result.output["state_changed"], false);
    assert!(result.output.get("permission").is_none());
    assert_eq!(result.output["session_project"], recorder_project);
    assert_eq!(
        result.output["request_project"],
        format!("path:{target_client}:{target_path}")
    );
    let listed = runtime.list_projects(Some(&auth)).await;
    assert_eq!(listed.output["count"], 1);
    let summary = runtime
        .sessions
        .summary(&recorder_session_id, Some(50))
        .expect("recording session summary");
    let event = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "work_on_project")
        .expect("recorded work_on_project event");
    assert_eq!(
        event.failure_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(
        event.error_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert!(event.warning_kind.is_none());
    assert_eq!(
        event.session_project.as_deref(),
        Some(recorder_project.as_str())
    );
    assert_eq!(
        event.request_project.as_deref(),
        Some(format!("path:{target_client}:{target_path}").as_str())
    );
    assert!(event.permission.is_none());
}

#[tokio::test]
async fn path_source_requires_project_write_scope_before_runner_enqueue() {
    let root = tempfile::tempdir().unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let auth = managed_oauth_auth_context("path-read-only", Some("path-read-only-hash"));
    register_agent_projects_for_auth(
        &runtime,
        "oauth-client",
        &auth,
        RunnerCapabilities {
            shell: true,
            git: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            path_work_on_project_call("oauth-client", &project_path, "must not register", None),
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "insufficient_scope");
    assert_eq!(
        result.output["required_scope"],
        crate::auth::SCOPE_PROJECT_WRITE
    );
    assert_eq!(result.output["state_changed"], false);
}

#[tokio::test]
async fn path_source_respects_restricted_authority_before_runner_enqueue() {
    let root = tempfile::tempdir().unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::Restricted));
    let client_id = "wop-path-restricted";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            path_work_on_project_call(client_id, &project_path, "must not register", None),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "permission_denied");
    assert_eq!(result.output["permission"]["status"], "denied");
    assert_eq!(result.output["permission"]["tool_name"], "register_project");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn work_on_project_continues_exact_session_and_appends_instruction() {
    let root = tempfile::tempdir().unwrap();
    let goal_store = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let goal_db = std::sync::Arc::new(
        crate::db::Database::open(&goal_store.path().join("goals.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_communication_database(goal_db);
    let project =
        register_runner_project_at_path(&runtime, "wop-continue", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    assert!(
        first.output.get("goal_context").is_none(),
        "fresh Session must not fabricate active Goal context"
    );
    let before = instruction_events(&runtime, &session_id);
    assert_eq!(before.len(), 1);

    let created = runtime.create_goal_with_plan(
        Some(&auth),
        NewGoal {
            title: "Continue exact Goal".into(),
            objective: "Reuse this Goal on normal Workflow Session re-entry.".into(),
            controller_agent_id: None,
            completion_conditions: vec!["Normal re-entry reuses exact Goal identity".into()],
            steps: vec![
                NewGoalStep {
                    id: "inspect".into(),
                    title: "Inspect".into(),
                },
                NewGoalStep {
                    id: "verify".into(),
                    title: "Verify".into(),
                },
            ],
            idempotency_key: "wop-goal-create".into(),
        },
    );
    assert!(created.success, "{:?}", created.output);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let associated = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            goal_id.clone(),
            session_id.clone(),
            "wop-goal-link".into(),
        )
        .await;
    assert!(associated.success, "{:?}", associated.output);
    let checkpoint = runtime.checkpoint_goal(
        Some(&auth),
        goal_id.clone(),
        2,
        crate::db::GoalCheckpoint {
            completed_step_ids: vec!["inspect".into()],
            current_step_id: Some("verify".into()),
            summary: "Inspect complete; verify next.".into(),
        },
        "wop-goal-checkpoint".into(),
    );
    assert!(checkpoint.success, "{:?}", checkpoint.output);

    let continued = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call(&project, "follow-up instruction", Some(&session_id)),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert_eq!(continued.output["session_id"], session_id);
    assert_eq!(continued.output["continuation"], "resumed_explicitly");
    assert_eq!(continued.output["goal_context"]["available"], true);
    assert_eq!(continued.output["goal_context"]["truncated"], false);
    assert_eq!(
        continued.output["goal_context"]["goals"],
        json!([{
            "goal_id": goal_id,
            "revision": 3,
            "incomplete_step_count": 1,
            "current_step": {"id": "verify", "title": "Verify"},
            "next_action": "checkpoint_goal"
        }])
    );
    assert!(first.output.get("workflow").is_none());
    assert!(continued.output.get("workflow").is_none());

    let second = runtime.create_goal(
        Some(&auth),
        "Second active Goal".into(),
        "Remain explicit when multiple active Goals share one Session.".into(),
        "wop-second-goal-create".into(),
    );
    assert!(second.success, "{:?}", second.output);
    let second_goal_id = second.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_link = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            second_goal_id.clone(),
            session_id.clone(),
            "wop-second-goal-link".into(),
        )
        .await;
    assert!(second_link.success, "{:?}", second_link.output);
    let continued_with_multiple = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call(&project, "choose explicit active Goal", Some(&session_id)),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(
        continued_with_multiple.success,
        "{:?}",
        continued_with_multiple.error
    );
    let goals = continued_with_multiple.output["goal_context"]["goals"]
        .as_array()
        .unwrap();
    assert_eq!(goals.len(), 2);
    let mut returned_ids = goals
        .iter()
        .map(|goal| goal["goal_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    returned_ids.sort();
    let mut expected_ids = vec![goal_id, second_goal_id];
    expected_ids.sort();
    assert_eq!(returned_ids, expected_ids);
    assert!(
        continued_with_multiple.output["goal_context"]
            .get("selected_goal_id")
            .is_none(),
        "startup Goal context must never auto-select among active Goals"
    );

    // Explicit resume reuses exactly one Session and appends one instruction.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    // Follow-up instruction appended; root title preserved.
    let events = instruction_events(&runtime, &session_id);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].instruction.as_deref(), Some("root objective"));
    assert_eq!(
        events[1].instruction.as_deref(),
        Some("follow-up instruction")
    );
    assert_eq!(
        events[2].instruction.as_deref(),
        Some("choose explicit active Goal")
    );
    let summary = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("root objective"));
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    assert!(
        !serde_json::to_string(&summary)
            .unwrap()
            .contains("webcodex.coding_workflow"),
        "workflow projection must not become Session state"
    );
}

#[tokio::test]
async fn work_on_project_exact_resume_omits_goal_context_without_active_goal() {
    let root = tempfile::tempdir().unwrap();
    let goal_store = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let goal_db = std::sync::Arc::new(
        crate::db::Database::open(&goal_store.path().join("goals.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_communication_database(goal_db);
    let project =
        register_runner_project_at_path(&runtime, "wop-no-goal", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-no-goal",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-no-goal-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    let resumed = dispatch_coding_call_in_window(
        &runtime,
        "wop-no-goal",
        work_on_project_call(&project, "ordinary continuation", Some(&session_id)),
        Some(&auth),
        "wop-no-goal-window",
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session_id"], session_id);
    assert_eq!(resumed.output["continuation"], "resumed_explicitly");
    assert!(
        resumed.output.get("goal_context").is_none(),
        "exact Session re-entry with zero active Goals must keep startup sparse"
    );
}

#[tokio::test]
async fn work_on_project_failures_never_create_or_fall_back() {
    let dir = tempfile::tempdir().unwrap();
    let root_a = dir.path().join("a");
    let root_b = dir.path().join("b");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    init_git_repo(&root_a);
    init_git_repo(&root_b);
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-fail",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            ..Default::default()
        },
        vec![
            registered_project("a", &root_a.to_string_lossy()),
            registered_project("b", &root_b.to_string_lossy()),
        ],
    )
    .await;
    let project_a = crate::tool_runtime::runner_project_runtime_id("wop-fail", "a");
    let project_b = crate::tool_runtime::runner_project_runtime_id("wop-fail", "b");
    let auth = auth_context(None, true);

    // Create a stable active session on project A, plus a closed one.
    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "stable session", None),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(first.success);
    let active_id = first.output["session_id"].as_str().unwrap().to_string();
    let closed_id = runtime
        .sessions
        .start_session_with_guards(
            Some(project_a.clone()),
            Some("closed project A".to_string()),
            SessionMode::Normal,
            SessionGuards::default(),
        )
        .session_id;
    runtime.sessions.close_session(&closed_id).unwrap();

    // Unknown Session: no creation, structured unknown_session_id failure.
    let unknown = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(
            &project_a,
            "must not create",
            Some("wc_sess_1111111111111111"),
        ),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!unknown.success);
    assert_eq!(unknown.output["error_kind"], "unknown_session_id");

    // Closed Session: no creation, structured session_closed failure.
    let closed = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "must not reopen", Some(&closed_id)),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!closed.success);
    assert_eq!(closed.output["error_kind"], "session_closed");
    assert_eq!(closed.output["lifecycle"], "closed");

    // Project mismatch: no fallback to any other session.
    let mismatch = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_b, "must not cross", Some(&active_id)),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    assert_eq!(mismatch.output["session_project"], project_a);
    assert_eq!(mismatch.output["request_project"], project_b);

    // Invalid Session id fails before execution (no session created).
    let invalid = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "must not run", Some("not-a-session")),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!invalid.success);
    assert_eq!(invalid.output["error_kind"], "invalid_session_id");

    // Nothing new was created and the active session is unchanged.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project_a)),
        1
    );
    let events = instruction_events(&runtime, &active_id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].instruction.as_deref(), Some("stable session"));
}

#[test]
fn finish_coding_task_remains_optional_and_advisory() {
    let specs = registered_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(names.contains(&"finish_coding_task"), "still public");

    let finish = spec_named(&specs, "finish_coding_task");
    let description = finish.description.to_lowercase();
    for phrase in [
        "optional",
        "advisory",
        "does not decide task completion",
        "generate the user-facing final report",
    ] {
        assert!(
            description.contains(phrase),
            "finish_coding_task description must include {phrase}: {description}"
        );
    }
    assert!(
        finish.description.contains("does not"),
        "finish_coding_task description must be explicit about non-authority"
    );

    // The default coding manifest intent does not mark finish as the required
    // final step: it is the last optional evidence snapshot in the list.
    let coding = crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .expect("coding intent");
    assert!(coding.tools.contains(&"work_on_project"));
    assert!(coding.tools.contains(&"finish_coding_task"));
    assert!(
        coding
            .tools
            .iter()
            .position(|t| *t == "finish_coding_task")
            .unwrap()
            > coding
                .tools
                .iter()
                .position(|t| *t == "work_on_project")
                .unwrap()
    );
}

/// Seed a representative Rust-style repository for the startup overview. The
/// files are committed so the tracked git index (the overview's project
/// boundary) includes every fixture entry; sensitive/build paths stay
/// excluded by the overview's own path policy.
fn seed_coding_repository(root: &std::path::Path, agents_body: &str) {
    init_git_repo(root);
    std::fs::write(
        root.join("AGENTS.md"),
        format!("# Repository rules\n\n{agents_body}\n"),
    )
    .unwrap();
    for path in [
        "README.md",
        "Cargo.toml",
        "src/lib.rs",
        "tests/basic.rs",
        "docs/index.md",
        "scripts/check.sh",
        ".github/workflows/ci.yml",
        "src/generated/deep/path.rs",
    ] {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"fixture contents must never be read").unwrap();
    }
    // Untracked/build/sensitive paths must never appear in the overview.
    std::fs::write(root.join(".env"), b"SECRET=do-not-leak").unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::write(root.join("target/debug/output"), b"binary").unwrap();
    for cmd in [
        "git add -A",
        "git commit -m 'seed fixture'",
        "git config status.showUntrackedFiles all",
    ] {
        let (exit_code, stdout, stderr, _) =
            crate::tool_runtime::helpers::run_command_sync(cmd, root, 30);
        assert_eq!(exit_code, 0, "{cmd}\n{stdout}{stderr}");
    }
}

/// Overwrite `AGENTS.md` in place (still tracked) so a follow-up resume sees a
/// changed fingerprint without a commit.
fn overwrite_agents_rule(root: &std::path::Path, body: &str) {
    std::fs::write(
        root.join("AGENTS.md"),
        format!("# Repository rules\n\n{body}\n"),
    )
    .unwrap();
}

#[tokio::test]
async fn work_on_project_new_task_is_lightweight_and_preserves_startup_context() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Preserve unrelated changes.");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-repo",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("wop-repo", "demo");
    let auth = auth_context(None, true);

    let (result, request_kinds) = dispatch_recording_startup_requests(
        &runtime,
        "wop-repo",
        work_on_project_call(&project, "start on the repository", None),
        Some(&auth),
        "wop-repo-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);

    // resolved_project is the full runtime project id.
    assert_eq!(result.output["resolved_project"], project);
    // Neither an inconclusive LSP probe nor an intentionally skipped
    // repository overview is a readiness warning.
    assert!(result.output.get("repository").is_none());
    assert!(result.output.get("readiness").is_none());
    assert!(result.output.get("warnings").is_none());
    assert_eq!(
        result.output["semantic_navigation"]["status"],
        "probe_failed"
    );
    assert_eq!(
        result.output["semantic_navigation"]["available"],
        Value::Null
    );

    // Runner request evidence: rules, Git, and LSP probes remain; repository
    // overview is not merely hidden from JSON, it is never enqueued.
    assert!(
        request_kinds.iter().any(|kind| kind == "file_read"),
        "repository rules were not observed: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .any(|kind| kind == "run_internal_posix_script"),
        "Git/workspace inspection was not executed through the internal POSIX runtime: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .any(|kind| kind == AGENT_LSP_REQUEST_KIND),
        "semantic navigation was not probed: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .all(|kind| kind != "file_project_overview"),
        "work_on_project unexpectedly enqueued an overview: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .all(|kind| kind != RUNNER_INSTRUCTION_REQUEST_KIND),
        "an older Runner without instruction_runtime must not receive the new request: {request_kinds:?}"
    );

    // Instructions are still observed, but the primary projection is metadata-only.
    let instructions = &result.output["instructions"];
    assert_eq!(instructions["status"], "loaded");
    assert!(instructions.get("content_included").is_none());
    assert!(instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["path"] == "AGENTS.md"
            && source["fingerprint"].is_string()
            && source.get("content").is_none()));

    // Semantic navigation block exists and is deterministic.
    assert!(result.output["semantic_navigation"].is_object());
    assert!(result.output["semantic_navigation"]["status"].is_string());

    // No noteworthy Job state means no jobs block at all.
    assert!(result.output.get("jobs").is_none());

    // No full diagnostics leak.
    for hidden in [
        "runtime_status",
        "connection_state",
        "authority",
        "tool_manifest",
        "recommended_flow",
        "startup_verdict",
        "git",
        "continuation_feedback",
    ] {
        assert!(
            !result.output.as_object().unwrap().contains_key(hidden),
            "compact output must not include {hidden}"
        );
    }

    // Exactly one fresh Session exists.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    // Schema validates.
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("compact output must match its schema: {error}"));
    let bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(bytes <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES);
    assert!(
        !result
            .output
            .to_string()
            .contains(&root.path().to_string_lossy().to_string()),
        "compact output leaked the absolute repository path"
    );
}

#[tokio::test]
async fn runner_global_instructions_compose_change_and_repeat_across_projects() {
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();
    seed_coding_repository(root_a.path(), "project A rule");
    init_git_repo(root_b.path());

    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-global",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            instruction_runtime: true,
            ..Default::default()
        },
        vec![
            registered_project("a", &root_a.path().to_string_lossy()),
            registered_project("b", &root_b.path().to_string_lossy()),
        ],
    )
    .await;
    let project_a = crate::tool_runtime::runner_project_runtime_id("wop-global", "a");
    let project_b = crate::tool_runtime::runner_project_runtime_id("wop-global", "b");
    let auth = auth_context(None, true);
    let global_v1 = runner_instruction_snapshot_stdout("runner global v1", 7);

    let (first, first_requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "wop-global",
        work_on_project_call(&project_a, "project A task", None),
        Some(&auth),
        "wop-global-window",
        &global_v1,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let first_sources = first.output["instructions"]["sources"].as_array().unwrap();
    assert_eq!(first_sources[0]["source_scope"], "runner");
    assert_eq!(first_sources[0]["path"], "runner/0/AGENTS.md");
    assert!(first_sources[0].get("content").is_none());
    assert_eq!(first_sources[1]["source_scope"], "project");
    assert_eq!(first_sources[1]["path"], "AGENTS.md");
    assert!(first_sources[1].get("content").is_none());
    let first_runner_fingerprint = first_sources[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    let first_session_id = first.output["session_id"].as_str().unwrap().to_string();

    let durable_summary = runtime
        .sessions
        .summary(&first_session_id, Some(20))
        .unwrap()
        .project_instructions
        .expect("instruction summary");
    let durable_json = serde_json::to_string(&durable_summary).unwrap();
    assert!(durable_json.contains("runner/0/AGENTS.md"));
    assert!(!durable_json.contains("runner global v1"));
    assert!(!durable_json.contains("project A rule"));

    // The same ChatGPT window opening another Project must observe the Runner-global
    // source again; v1 intentionally has no cross-Project model-context suppression.
    let (second_project, second_requests) =
        dispatch_recording_startup_requests_with_runner_instructions(
            &runtime,
            "wop-global",
            work_on_project_call(&project_b, "project B task", None),
            Some(&auth),
            "wop-global-window",
            &global_v1,
        )
        .await;
    assert!(second_project.success, "{:?}", second_project.error);
    assert!(second_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let second_sources = second_project.output["instructions"]["sources"]
        .as_array()
        .unwrap();
    assert_eq!(second_sources.len(), 1);
    assert_eq!(second_sources[0]["source_scope"], "runner");
    assert!(second_sources[0].get("content").is_none());

    // Explicit body suppression remains one shared instruction projection switch;
    // it does not create a special retention protocol for Runner-global sources.
    let (suppressed, suppressed_requests) =
        dispatch_recording_startup_requests_with_runner_instructions(
            &runtime,
            "wop-global",
            work_on_project_call(&project_b, "project B metadata-only task", None),
            Some(&auth),
            "wop-global-window",
            &global_v1,
        )
        .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(suppressed_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let suppressed_runner = suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_scope"] == "runner")
        .unwrap();
    assert!(suppressed_runner.get("content").is_none());

    // File contents are live independently from config generation. Even a stale or
    // malformed Runner response that reuses the prior upstream fingerprint cannot
    // hide different visible content from Server-side continuation detection.
    let mut global_v1_wire: serde_json::Value = serde_json::from_str(&global_v1).unwrap();
    let mut global_v2_wire: serde_json::Value =
        serde_json::from_str(&runner_instruction_snapshot_stdout("runner global v2", 7)).unwrap();
    global_v2_wire["files"][0]["fingerprint"] = global_v1_wire["files"][0]["fingerprint"].take();
    let global_v2 = global_v2_wire.to_string();
    let (changed, changed_requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "wop-global",
        work_on_project_call(
            &project_a,
            "resume after global edit",
            Some(&first_session_id),
        ),
        Some(&auth),
        "wop-global-window",
        &global_v2,
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert!(changed_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(changed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("runner/0/AGENTS.md")));
    let changed_runner = changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_scope"] == "runner")
        .unwrap();
    assert!(changed_runner.get("content").is_none());
    assert_ne!(changed_runner["fingerprint"], first_runner_fingerprint);
}

#[tokio::test]
async fn work_on_project_omits_instruction_bodies_even_for_a_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "caller already knows this rule");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(
        &runtime,
        "wop-instruction-projection",
        "demo",
        root.path(),
    )
    .await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-instruction-projection",
        work_on_project_call(&project, "first task", None),
        Some(&auth),
        "wop-instruction-projection-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first.output["instructions"]
        .get("content_included")
        .is_none());
    let first_session_id = first.output["session_id"].as_str().unwrap().to_string();

    let (second, request_kinds) = dispatch_recording_startup_requests(
        &runtime,
        "wop-instruction-projection",
        work_on_project_call(&project, "second independent task", None),
        Some(&auth),
        "wop-instruction-projection-window",
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    let second_session_id = second.output["session_id"].as_str().unwrap().to_string();
    assert_ne!(second_session_id, first_session_id);
    assert_eq!(second.output["continuation"], "created");

    let instructions = &second.output["instructions"];
    assert_eq!(instructions["status"], "loaded");
    assert!(instructions.get("content_included").is_none());
    let agents_source = instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .expect("AGENTS.md metadata");
    assert!(agents_source.get("content").is_none());
    assert!(agents_source.get("headings").is_none());
    assert!(agents_source.get("truncated").is_none());
    assert!(agents_source["fingerprint"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
    assert!(
        request_kinds.iter().any(|kind| kind == "file_read"),
        "instruction files must still be observed when their bodies are omitted: {request_kinds:?}"
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": second.output.clone() });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| {
            panic!("sparse instruction metadata must match output schema: {error}")
        });

    let summary = runtime
        .sessions
        .summary(&second_session_id, Some(20))
        .unwrap();
    let snapshot = summary
        .project_instructions
        .expect("fresh Workflow Session instruction summary");
    assert!(snapshot.loaded);
    let stored_agents = snapshot
        .files
        .iter()
        .find(|file| file.path == "AGENTS.md")
        .expect("stored AGENTS.md summary");
    assert_eq!(
        Some(stored_agents.fingerprint.as_str()),
        agents_source["fingerprint"].as_str()
    );
}

#[tokio::test]
async fn work_on_project_fresh_window_discovers_only_owning_runner_acp_and_admits_it() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Review source without changing files");
    let runtime = ToolRuntime::new_for_tests();
    let provider = |id: &str, name: &str| webcodex_core::coding_agent::CodingAgentProvider {
        provider_id: id.to_owned(),
        name: name.to_owned(),
        provider_instance_id: format!("private-provider-{id}"),
    };
    let project = register_runner_project_at_path_with_coding_agents(
        &runtime,
        "wop-acp",
        "demo",
        root.path(),
        Some(vec![provider("pi", "Pi Agent")]),
    )
    .await;
    register_runner_project_at_path_with_coding_agents(
        &runtime,
        "other-acp",
        "other",
        root.path(),
        Some(vec![provider("codex", "Codex Agent")]),
    )
    .await;
    let auth = auth_context(None, true);
    for window in ["fresh-acp-window-a", "fresh-acp-window-b"] {
        let result = dispatch_coding_call_in_window(
            &runtime,
            "wop-acp",
            work_on_project_call(
                &project,
                "Review the repository without changing files",
                None,
            ),
            Some(&auth),
            window,
        )
        .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output["coding_agent_providers"],
            json!([{"provider_id":"pi","name":"Pi Agent"}])
        );
        assert!(!result.output.to_string().contains("private-provider-"));
        let advertised = result.output["coding_agent_providers"][0]["provider_id"]
            .as_str()
            .unwrap();
        assert!(runtime
            .prepare_coding_agent_start(
                project.clone(),
                advertised.to_owned(),
                format!("discover-{window}"),
                "Read-only module review".to_owned(),
                None,
                Some(10),
                Some(&auth),
            )
            .await
            .is_ok());
        let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &json!({"success": true, "output": result.output}),
            &schema,
        )
        .unwrap();
    }
}

#[tokio::test]
async fn work_on_project_omits_static_guidance_from_primary_output() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "keep repository guidance visible");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-workflow-projection", "demo", root.path())
            .await;
    let auth = auth_context(None, true);

    let result = dispatch_coding_call_in_window(
        &runtime,
        "wop-workflow-projection",
        work_on_project_call(&project, "caller already knows the static workflow", None),
        Some(&auth),
        "wop-workflow-projection-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("workflow").is_none());
    assert!(result.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(result.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));

    let session_id = result.output["session_id"].as_str().unwrap();
    let summary = runtime.sessions.summary(session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));

    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("workflow-omitted output must match schema: {error}"));
}

#[tokio::test]
async fn work_on_project_static_projection_is_not_inferred_from_window_or_session_state() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "static caller-explicit rule");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-explicit", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "first", None),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    assert!(first.output.get("workflow").is_none());

    let repeated = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "repeat true", Some(&session_id)),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(repeated.success, "{:?}", repeated.error);
    assert!(repeated.output.get("workflow").is_none());
    assert_eq!(repeated.output["instructions"]["status"], "reused");
    assert!(repeated.output["instructions"]
        .get("content_included")
        .is_none());

    let suppressed = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(
            &project,
            "caller suppresses static content",
            Some(&session_id),
        ),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(suppressed.output.get("workflow").is_none());
    assert_eq!(suppressed.output["instructions"]["status"], "reused");
    assert!(suppressed.output["instructions"]
        .get("content_included")
        .is_none());
    let suppressed_agents = suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap();
    assert!(suppressed_agents["fingerprint"].is_string());
    assert!(suppressed_agents.get("content").is_none());
    assert!(suppressed_agents.get("headings").is_none());
    assert!(suppressed_agents.get("read_more").is_none());

    let restored_other_window = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "true in another window", Some(&session_id)),
        Some(&auth),
        "different-window",
    )
    .await;
    assert!(
        restored_other_window.success,
        "{:?}",
        restored_other_window.error
    );
    assert!(restored_other_window.output.get("workflow").is_none());
    assert!(restored_other_window.output["instructions"]
        .get("content_included")
        .is_none());

    let no_window = dispatch_startup_without_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "true without window", Some(&session_id)),
        Some(&auth),
    )
    .await;
    assert!(no_window.success, "{:?}", no_window.error);
    assert!(no_window.output.get("workflow").is_none());
    assert!(no_window.output["instructions"]
        .get("content_included")
        .is_none());
}

#[tokio::test]
async fn work_on_project_suppressed_instruction_bodies_still_track_changed_rules() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "old body");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-suppressed-change", "demo", root.path())
            .await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "first", None),
        Some(&auth),
        "window-a",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let old_fingerprint = first.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap()["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();

    let long_changed_body = std::iter::once("new body while projection is suppressed".to_string())
        .chain((0..500).map(|index| format!("suppressed-line-{index}")))
        .collect::<Vec<_>>()
        .join("\n");
    overwrite_agents_rule(root.path(), &long_changed_body);
    let changed = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "observe change without body", Some(&session_id)),
        Some(&auth),
        "window-a",
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(changed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("AGENTS.md")));
    let changed_agents = changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap();
    assert_ne!(changed_agents["fingerprint"], old_fingerprint);
    assert!(changed_agents.get("content").is_none());
    assert!(changed_agents.get("headings").is_none());
    assert!(changed_agents.get("read_more").is_none());

    let projected = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "project current body", Some(&session_id)),
        Some(&auth),
        "window-b",
    )
    .await;
    assert!(projected.success, "{:?}", projected.error);
    assert_eq!(projected.output["instructions"]["status"], "reused");
    assert!(projected.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(projected.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
}

#[tokio::test]
async fn work_on_project_exact_resume_reuses_rules_and_detects_changes() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "first rule body");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(&runtime, "wop-reuse", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    // Exact resume with unchanged rules keeps delta metadata while static bodies remain opt-in.
    let reused = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "follow-up", Some(&session_id)),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(reused.success, "{:?}", reused.error);
    assert_eq!(reused.output["session_id"], session_id);
    assert_eq!(reused.output["continuation"], "resumed_explicitly");
    let reused_instructions = &reused.output["instructions"];
    assert_eq!(reused_instructions["status"], "reused");
    assert!(reused_instructions.get("content_included").is_none());
    assert!(reused_instructions.get("changed_sources").is_none());
    let reused_agents = reused_instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .expect("reused AGENTS.md source");
    assert!(reused_agents.get("content").is_none());
    assert!(reused_agents.get("headings").is_none());
    assert!(reused_agents["fingerprint"].is_string());

    // Change the rule then resume: status=changed, changed_sources includes it.
    overwrite_agents_rule(root.path(), "changed rule body");
    let changed = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "after rule change", Some(&session_id)),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert_eq!(changed.output["session_id"], session_id);
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(
        changed.output["instructions"]["changed_sources"]
            .as_array()
            .unwrap()
            .contains(&json!("AGENTS.md")),
        "{:?}",
        changed.output["instructions"]["changed_sources"]
    );
    assert!(changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["path"] == "AGENTS.md"
            && source["fingerprint"].is_string()
            && source.get("content").is_none()));
}

#[tokio::test]
async fn work_on_project_sizes_and_runner_request_reduction_are_stable() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Keep the focused startup safe.");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-size",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("wop-size", "demo");
    let auth = auth_context(None, true);

    let (fresh, fresh_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-size",
        work_on_project_call(&project, "fresh lightweight startup", None),
        Some(&auth),
        "wop-size-fresh",
    )
    .await;
    assert!(fresh.success, "{:?}", fresh.error);
    let session_id = fresh.output["session_id"].as_str().unwrap().to_string();

    let (reused, reused_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-size",
        work_on_project_call(&project, "unchanged continuation", Some(&session_id)),
        Some(&auth),
        "wop-size-reused",
    )
    .await;
    assert!(reused.success, "{:?}", reused.error);
    assert_eq!(reused.output["instructions"]["status"], "reused");
    assert!(reused.output["instructions"]
        .get("content_included")
        .is_none());

    let (standard, standard_requests) = dispatch_recording_coding_workflow_diagnostic(
        &runtime,
        "wop-size",
        &project,
        "same fixture standard startup",
        StartupDetail::Standard,
        Some(&auth),
    )
    .await;
    assert!(standard.success, "{:?}", standard.error);
    assert_eq!(standard.output["repository"]["status"], "available");

    for output in [&fresh.output, &reused.output] {
        for omitted in [
            "repository",
            "execution_context",
            "jobs",
            "blockers",
            "deterministic",
            "llm_summary",
        ] {
            assert!(
                output.get(omitted).is_none(),
                "work_on_project boring default field {omitted} should be omitted: {output}"
            );
        }
    }

    let fresh_overviews = fresh_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    let reused_overviews = reused_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    let standard_overviews = standard_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    assert_eq!(fresh_overviews, 0);
    assert_eq!(reused_overviews, 0);
    assert_eq!(standard_overviews, 1);
    assert_eq!(
        standard_requests.len(),
        fresh_requests.len() + 1,
        "the identical advanced fixture should add only the overview request"
    );
    assert_eq!(
        reused_requests.len() + 1,
        fresh_requests.len(),
        "fresh startup pays exactly one Git baseline probe; exact continuation must preserve the durable baseline without re-probing it"
    );
    let fresh_bytes = serde_json::to_vec(&fresh.output).unwrap().len();
    let reused_bytes = serde_json::to_vec(&reused.output).unwrap().len();
    let standard_bytes = serde_json::to_vec(&standard.output).unwrap().len();
    eprintln!(
        "work_on_project_fixture_bytes fresh={fresh_bytes} unchanged_continuation={reused_bytes} standard={standard_bytes}; runner_requests fresh={} unchanged_continuation={} standard={}",
        fresh_requests.len(),
        reused_requests.len(),
        standard_requests.len()
    );
    let hard_max = crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES;
    assert!(fresh_bytes < hard_max);
    assert!(reused_bytes < hard_max);
    assert!(standard_bytes <= hard_max);
    assert!(fresh_bytes < standard_bytes);
    assert!(reused_bytes < standard_bytes);
    // The compact primary projection excludes static instruction bodies and workflow
    // guidance; keep it tightly bounded while leaving modest protocol headroom.
    assert!(
        fresh_bytes <= 4832,
        "fresh work_on_project projection regressed above the sparse context budget: {fresh_bytes} bytes"
    );
    assert!(
        reused_bytes <= 4932,
        "unchanged work_on_project projection regressed above the sparse continuation budget: {reused_bytes} bytes"
    );
}

#[tokio::test]
async fn coding_workflow_standard_repository_overview_timeout_is_nonblocking() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite overview timeout");
    // Tight overview timeout so the probe expires quickly.
    let runtime = ToolRuntime::new_for_tests()
        .with_repository_overview_probe_timeout(std::time::Duration::from_millis(50));
    let project =
        register_runner_project_at_path(&runtime, "wop-timeout", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        async move {
            runtime
                .start_coding_workflow_for_test(
                    project,
                    None,
                    None,
                    Some("start despite overview timeout".to_string()),
                    SessionMode::Normal,
                    false,
                    false,
                    StartupDetail::Standard,
                    None,
                    None,
                    Some(&auth),
                    None,
                    None,
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                )
                .await
        }
    });

    // Service the git/instruction probes but never the overview request.
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut overview_request = None;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "overview-timeout startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?}"
        );
        let Some(request) = probe_patch_agent_request(&runtime, "wop-timeout").await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        if request.kind == "file_project_overview" {
            overview_request = Some(request.request_id.clone());
            // Intentionally never complete it; the probe must time out.
            continue;
        }
        let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
        complete_patch_agent_request(
            &runtime,
            "wop-timeout",
            &request.request_id,
            exit_code,
            &stdout,
            &stderr,
        )
        .await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);

    // Overview unavailable with the deterministic reason; session still works.
    assert_eq!(result.output["repository"]["status"], "unavailable");
    assert_eq!(
        result.output["repository"]["reason_code"],
        "unsupported_or_unavailable"
    );
    assert!(result.output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning == "repository_overview_unavailable"));
    let session_id = result.output["session"]["session_id"].as_str().unwrap();
    assert!(session_id.starts_with("wc_sess_"));
    let summary = runtime.sessions.summary(session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));

    // The timed-out overview request was cancelled server-side.
    if let Some(request_id) = overview_request {
        let expired = runtime
            .runner_registry
            .complete(crate::runner_protocol::RunnerResultRequest {
                client_id: "wop-timeout".to_string(),
                runner_instance_id: "inst".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("{}".to_string()),
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .expect_err("timed-out overview probe must remove pending waiter");
        assert!(
            expired.contains("unknown or expired shell request"),
            "{expired}"
        );
    }
}

/// Drive a coding-workflow diagnostic to completion, completing the
/// `file_project_overview` probe with `overview_stdout` (exit code 0, no error)
/// while servicing every other agent request locally. Returns the startup
/// result and the overview request id that was answered.
async fn dispatch_coding_workflow_diagnostic_with_overview_stdout(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    instruction: &str,
    detail: StartupDetail,
    overview_stdout: String,
    auth: Option<&crate::auth::AuthContext>,
) -> (crate::tool_runtime::ToolResult, Option<String>) {
    use crate::tool_runtime::sessions::SessionTransport;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        let project = project.to_string();
        let instruction = instruction.to_string();
        async move {
            runtime
                .start_coding_workflow_for_test(
                    project,
                    None,
                    None,
                    Some(instruction),
                    SessionMode::Normal,
                    false,
                    false,
                    detail,
                    None,
                    None,
                    auth.as_ref(),
                    None,
                    None,
                    SessionTransport::Mcp,
                )
                .await
        }
    });

    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut overview_request_id = None;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "overview startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {client_id}"
        );
        let Some(request) = probe_patch_agent_request(runtime, client_id).await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        if request.kind == "file_project_overview" {
            overview_request_id = Some(request.request_id.clone());
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                &overview_stdout,
                "",
            )
            .await;
            continue;
        }
        let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
        complete_patch_agent_request(
            runtime,
            client_id,
            &request.request_id,
            exit_code,
            &stdout,
            &stderr,
        )
        .await;
    }
    let result = task.await.unwrap();
    (result, overview_request_id)
}

/// Build a structurally-valid root overview (depth 2 / limit 120) so a test can
/// mutate one field and observe fail-closed behavior. The fixture repo must
/// already be seeded and committed.
fn valid_agent_overview_stdout(
    runtime: &ToolRuntime,
    client_id: &str,
    root: &std::path::Path,
) -> String {
    let _ = (runtime, client_id);
    let overview = crate::project_overview::build_project_overview(root, ".", Some(2), Some(120))
        .expect("valid agent overview fixture");
    overview.to_string()
}

#[tokio::test]
async fn coding_workflow_standard_repository_overview_rejects_malformed_runner_responses() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite malformed overview");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-malformed", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let valid = valid_agent_overview_stdout(&runtime, "wop-malformed", root.path());
    let valid_value: serde_json::Value = serde_json::from_str(&valid).unwrap();

    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("absolute path", {
            let mut v = valid_value.clone();
            v["top_level"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "/etc/passwd", "kind": "file"}));
            v
        }),
        ("parent traversal", {
            let mut v = valid_value.clone();
            v["manifests"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "../outside/Cargo.toml", "kind": "rust_manifest"}));
            v
        }),
        ("request boundary mismatch", {
            let mut v = valid_value.clone();
            v["scan"]["max_depth"] = json!(4);
            v["scan"]["limit"] = json!(500);
            v["path"] = json!("src");
            v
        }),
        ("unknown project type", {
            let mut v = valid_value.clone();
            v["project_types"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind": "cobol", "evidence": []}));
            v
        }),
        ("unknown key-file kind", {
            let mut v = valid_value.clone();
            v["key_files"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "README.md", "kind": "mystery", "reason": "x"}));
            v
        }),
        ("unknown warning", {
            let mut v = valid_value.clone();
            v["warnings"]
                .as_array_mut()
                .unwrap()
                .push(json!("nuclear_launch_detected"));
            v
        }),
        ("returned_entry_count as string", {
            let mut v = valid_value.clone();
            v["scan"]["returned_entry_count"] = json!("plenty");
            v
        }),
        ("warnings as object", {
            let mut v = valid_value.clone();
            v["warnings"] = json!({"note": "not an array"});
            v
        }),
        ("duplicate top-level paths", {
            let mut v = valid_value.clone();
            let top = v["top_level"].as_array_mut().unwrap();
            top.push(top[0].clone());
            v
        }),
    ];

    for (label, payload) in cases {
        let stdout = payload.to_string();
        let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
            &runtime,
            "wop-malformed",
            &project,
            label,
            StartupDetail::Standard,
            stdout,
            Some(&auth),
        )
        .await;
        assert!(
            result.success,
            "{label}: task must still succeed: {:?}",
            result.error
        );
        assert!(
            overview_id.is_some(),
            "{label}: overview probe must be issued"
        );
        let repository = &result.output["repository"];
        assert_eq!(
            repository["status"], "unavailable",
            "{label}: malformed Runner response must fail closed"
        );
        assert_eq!(
            repository["reason_code"], "unsupported_or_unavailable",
            "{label}: deterministic reason code"
        );
        // No raw stdout, stderr, error text, or absolute paths leak into the
        // model-facing compact output.
        let serialized = result.output.to_string();
        assert!(
            !serialized.contains("runner_secret"),
            "{label}: extra Runner field leaked"
        );
        assert!(
            !serialized.contains("/etc/passwd") && !serialized.contains("/absolute/leak"),
            "{label}: absolute path leaked"
        );
        assert!(
            !serialized.contains("nuclear_launch_detected") && !serialized.contains("cobol"),
            "{label}: malformed enum leaked"
        );
        assert!(
            !serialized.contains("../outside"),
            "{label}: traversal path leaked"
        );
        assert!(
            serialized.len() <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES,
            "{label}: compact output exceeded 30 KiB"
        );
        // The deterministic unavailable warning is surfaced.
        assert!(
            result.output["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| warning == "repository_overview_unavailable"),
            "{label}: repository_overview_unavailable warning missing"
        );
        // A session is still created despite the malformed overview.
        let session_id = result.output["session"]["session_id"].as_str().unwrap();
        assert!(
            session_id.starts_with("wc_sess_"),
            "{label}: session not created"
        );
    }
}

#[tokio::test]
async fn coding_workflow_standard_overview_strips_unknown_runner_fields_and_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite extra runner fields");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(&runtime, "wop-strip", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let valid = valid_agent_overview_stdout(&runtime, "wop-strip", root.path());
    let mut payload: serde_json::Value = serde_json::from_str(&valid).unwrap();
    // A malicious/defensive Runner adds an oversized `scan` extra field and
    // top-level unknowns (including an absolute path). The contract must not
    // fail on mere extras — it must strip them and keep the formal fields only,
    // so the model output stays small and free of leaked content.
    payload["scan"]["padding"] = json!("X".repeat(40_000));
    payload["scan"]["nested"] = json!({"deep": json!(["Y".repeat(10_000), 1, 2])});
    payload["runner_secret"] = json!("/absolute/leak");

    let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
        &runtime,
        "wop-strip",
        &project,
        "strip extras",
        StartupDetail::Standard,
        payload.to_string(),
        Some(&auth),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(overview_id.is_some(), "overview probe must be issued");
    let repository = &result.output["repository"];
    assert_eq!(
        repository["status"], "available",
        "extras must be stripped, not rejected"
    );

    // scan keeps exactly the 5 fixed formal fields; padding/nested dropped.
    let scan = &repository["scan"];
    assert!(scan.is_object());
    let mut scan_keys: Vec<&str> = scan
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    scan_keys.sort_unstable();
    assert_eq!(
        scan_keys,
        [
            "limit",
            "max_depth",
            "returned_entry_count",
            "truncated",
            "truncation_reason"
        ],
        "scan must keep only the fixed fields: {scan:?}"
    );
    assert_eq!(scan["max_depth"], 2);
    assert_eq!(scan["limit"], 120);

    let serialized = result.output.to_string();
    assert!(!serialized.contains("padding"), "scan padding leaked");
    assert!(
        !serialized.contains("runner_secret"),
        "extra runner field leaked"
    );
    assert!(
        !serialized.contains("/absolute/leak"),
        "absolute path leaked"
    );
    assert!(
        serialized.len() <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES,
        "compact output exceeded 30 KiB after stripping"
    );
}

#[tokio::test]
async fn coding_workflow_standard_and_full_accept_valid_repository_overview() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load with valid overview");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-valid-overview", "demo", root.path()).await;

    let auth = auth_context(None, true);
    for detail in [StartupDetail::Standard, StartupDetail::Full] {
        let stdout = valid_agent_overview_stdout(&runtime, "wop-valid-overview", root.path());
        let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
            &runtime,
            "wop-valid-overview",
            &project,
            "valid overview",
            detail,
            stdout,
            Some(&auth),
        )
        .await;
        assert!(result.success, "{detail:?}: {:?}", result.error);
        assert!(
            overview_id.is_some(),
            "{detail:?}: overview probe must be issued"
        );
        let brief = crate::tool_runtime::startup_brief::startup_brief_from_output(&result.output)
            .expect("advanced startup brief");
        let semantic = &brief["semantic_navigation"];
        assert_eq!(semantic["status"], "probe_failed");
        assert_eq!(semantic["available"], Value::Null);
        assert_eq!(semantic["reason_code"], "malformed_agent_result");
        assert!(!brief["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning == "semantic_navigation_unavailable"));
        if detail == StartupDetail::Full {
            // Full diagnostics retain the original probe status and reason.
            assert_eq!(
                result.output["semantic_navigation"]["status"],
                "probe_failed"
            );
            assert_eq!(
                result.output["semantic_navigation"]["available"],
                Value::Null
            );
            assert_eq!(
                result.output["semantic_navigation"]["reason_code"],
                "malformed_agent_result"
            );
        }
        let repository = &brief["repository"];
        assert_eq!(
            repository["status"], "available",
            "{detail:?}: valid response must be accepted"
        );
        // scan projection keeps only the fixed fields, no extras.
        let scan = &repository["scan"];
        assert!(scan.is_object());
        assert_eq!(scan.as_object().unwrap().len(), 5);
        assert_eq!(scan["max_depth"], 2);
        assert_eq!(scan["limit"], 120);
        // Rust is detected via the committed Cargo.toml fixture.
        let types = repository["project_types"]["items"].as_array().unwrap();
        assert!(types.iter().any(|kind| kind["kind"] == "rust"));
        assert!(repository["manifests"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|manifest| manifest["path"] == "Cargo.toml"));
        assert!(!repository["key_files"]["items"]
            .as_array()
            .unwrap()
            .is_empty());
        let serialized = repository.to_string();
        assert!(!serialized.contains(&root.path().to_string_lossy().to_string()));

        let schema =
            crate::tool_runtime::registry::coding_workflow_diagnostic_output_schema_for_test();
        let instance = json!({"success": true, "output": result.output});
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
            .unwrap_or_else(|error| {
                panic!("{detail:?} coding workflow diagnostic must match strict schema: {error}")
            });
    }
}

#[tokio::test]
async fn work_on_project_guidance_profile_is_request_local_and_not_durable() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "profile-independent project rule");
    let state = tempfile::tempdir().unwrap();
    let ledger = state.path().join("sessions.json");
    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_runner_project_at_path(&runtime, "wop-profile", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-profile",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first.output.get("workflow").is_none());
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let before =
        serde_json::to_value(runtime.sessions.summary(&session_id, Some(50)).unwrap()).unwrap();
    let mut cases = vec![Some("direct"), Some("host_code_mode")];
    #[cfg(feature = "experimental-code-mode")]
    cases.push(Some("code_mode"));
    cases.push(None); // Same Window/Session must not remember the last profile.
    let mut baseline = None;
    let mut requests_baseline = None;
    for profile in cases {
        let mut args = json!({"project": project, "instruction": "continue the task", "session_id": session_id, "include_extension_catalog": false});
        if let Some(profile) = profile {
            args["guidance_profile"] = json!(profile);
        }
        let call = ToolCall::from_tool_name("work_on_project", args).unwrap();
        let audit =
            crate::tool_runtime::tool_audit::ToolCallAuditProjection::session_log_arguments(&call);
        assert!(
            audit.get("guidance_profile").is_none(),
            "presentation selection must not leak into Session event arguments"
        );
        let (result, mut requests) = dispatch_recording_startup_requests(
            &runtime,
            "wop-profile",
            call,
            Some(&auth),
            "same-window",
        )
        .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_id"], session_id);
        assert_eq!(result.output["continuation"], "resumed_explicitly");
        assert!(result.output.get("workflow").is_none());
        let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &json!({"success":true,"output":result.output}),
            &schema,
        )
        .unwrap();
        assert!(serde_json::to_vec(&result.output).unwrap().len() < 30 * 1024);
        if let Some(expected) = &baseline {
            assert_eq!(
                &result.output, expected,
                "guidance profile must not change the primary work_on_project projection"
            );
        } else {
            baseline = Some(result.output);
        }
        requests.sort();
        if let Some(expected) = &requests_baseline {
            assert_eq!(
                &requests, expected,
                "profile must not change startup probes"
            );
        } else {
            requests_baseline = Some(requests);
        }
    }
    let after =
        serde_json::to_value(runtime.sessions.summary(&session_id, Some(50)).unwrap()).unwrap();
    for field in [
        "session_id",
        "project",
        "title",
        "mode",
        "guards",
        "execution_context",
    ] {
        assert_eq!(before[field], after[field], "{field}");
    }
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    runtime.sessions.flush_persistence();
    let persisted = fs::read_to_string(&ledger).unwrap();
    for absent in [
        "guidance_profile",
        "tool_strategy",
        "webcodex.coding_workflow",
        "code_mode",
    ] {
        assert!(
            !persisted.contains(absent),
            "profile must not become durable business state: {absent}"
        );
    }
    let restored = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    assert_eq!(
        restored
            .sessions
            .summary(&session_id, Some(50))
            .unwrap()
            .title
            .as_deref(),
        Some("root objective")
    );
}

#[tokio::test]
async fn runner_instruction_refresh_keeps_local_changes_and_observes_suppressed_removals() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "LOCAL_INITIAL_RULE");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "instruction-refresh",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            instruction_runtime: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("instruction-refresh", "demo");
    let auth = auth_context(None, true);
    let mut global: RunnerInstructionSnapshotResponse = serde_json::from_str(
        &runner_instruction_snapshot_stdout(&"g".repeat(32 * 1024), 7),
    )
    .unwrap();
    for index in 1..16 {
        let mut file = global.files[0].clone();
        file.path = format!("runner/{index}/extra.md");
        file.content.clear();
        file.chars = 0;
        file.truncated = true;
        global.files.push(file);
    }
    let global = serde_json::to_string(&global).unwrap();
    let (first, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "initial", None),
        Some(&auth),
        "instruction-window",
        &global,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let id = first.output["session_id"].as_str().unwrap();
    let sources = first.output["instructions"]["sources"].as_array().unwrap();
    assert!(sources.iter().all(|source| source.get("content").is_none()));
    assert!(sources.iter().any(|source| source["path"] == "AGENTS.md"));
    assert_eq!(sources.len(), 17);
    assert!(sources[0].get("read_more").is_none());
    assert!(serde_json::to_vec(&first.output).unwrap().len() <= 30 * 1024);

    std::fs::write(root.path().join("AGENTS.md"), "LOCAL_UPDATED_RULE").unwrap();
    let (failed, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "global refresh fails", Some(id)),
        Some(&auth),
        "instruction-window",
        "invalid snapshot",
    )
    .await;
    assert!(failed.success, "{:?}", failed.error);
    assert_eq!(failed.output["instructions"]["status"], "unavailable");
    assert!(failed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("AGENTS.md")));
    let summary = runtime
        .sessions
        .summary(id, None)
        .unwrap()
        .project_instructions
        .unwrap();
    let updated = summary
        .files
        .iter()
        .find(|file| file.path == "AGENTS.md")
        .unwrap()
        .fingerprint
        .clone();

    std::fs::remove_file(root.path().join("AGENTS.md")).unwrap();
    let (suppressed, requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "observe deletion silently", Some(id)),
        Some(&auth),
        "instruction-window",
        "invalid snapshot",
    )
    .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    assert!(suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
    let summary = runtime
        .sessions
        .summary(id, None)
        .unwrap()
        .project_instructions
        .unwrap();
    assert!(summary.files.iter().all(|file| file.fingerprint != updated));
    assert!(summary.files.iter().all(|file| file.path != "AGENTS.md"));

    let empty = serde_json::to_string(&RunnerInstructionSnapshotResponse {
        format: RUNNER_INSTRUCTION_RESPONSE_FORMAT.into(),
        generation: 7,
        scan_complete: true,
        files: Vec::new(),
    })
    .unwrap();
    let (removed, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "global removed", Some(id)),
        Some(&auth),
        "instruction-window",
        &empty,
    )
    .await;
    assert!(removed.success, "{:?}", removed.error);
    assert!(removed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("runner/0/AGENTS.md")));
    assert!(removed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source["source_scope"] != "runner"));
}

#[tokio::test]
async fn work_on_project_distinguishes_unavailable_from_inconclusive_lsp_probes() {
    use crate::lsp_bridge::{LspAvailabilityStatus, LspServerStatusEntry, LspStatusResult};
    use std::time::{Duration, Instant};

    for (status, reason) in [
        ("probe_timeout", "status_probe_timed_out"),
        ("probe_failed", "status_probe_failed"),
        ("unavailable", "server_unavailable"),
    ] {
        let root = tempfile::tempdir().unwrap();
        init_git_repo(root.path());
        commit_file(root.path(), "README.md", "# fixture\n", "seed");
        let runtime = ToolRuntime::new_for_tests()
            .with_semantic_navigation_probe_timeout(Duration::from_millis(100));
        let project =
            register_runner_project_at_path(&runtime, "wop-probe-state", "demo", root.path()).await;
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        work_on_project_call(&project, "continue normal coding", None),
                        Some(&auth_context(None, true)),
                    )
                    .await
            }
        });
        let deadline = Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
        let mut probe_count = 0;
        while !task.is_finished() {
            assert!(Instant::now() < deadline, "startup stuck for {status}");
            let Some(request) = probe_patch_agent_request(&runtime, "wop-probe-state").await else {
                tokio::time::sleep(Duration::from_millis(2)).await;
                continue;
            };
            if request.kind != AGENT_LSP_REQUEST_KIND {
                complete_agent_request_by_running_locally(&runtime, "wop-probe-state", request)
                    .await;
                continue;
            }
            probe_count += 1;
            if status == "probe_timeout" {
                // Leave exactly this probe unanswered; the normal timeout path
                // must cancel it without blocking the rest of coding startup.
                continue;
            }
            let envelope = if status == "unavailable" {
                RunnerLspResultEnvelope::ok(LspStatusResult {
                    project: "demo".to_string(),
                    detected_languages: vec!["rust".to_string()],
                    servers: vec![LspServerStatusEntry {
                        language: "rust".to_string(),
                        server: "rust-analyzer".to_string(),
                        available: false,
                        running: false,
                        status: LspAvailabilityStatus::Unavailable,
                        source: None,
                        position_encoding: None,
                    }],
                    warnings: vec![],
                })
            } else {
                RunnerLspResultEnvelope::err("lsp_protocol_error", "probe did not conclude")
            };
            complete_patch_agent_request(
                &runtime,
                "wop-probe-state",
                &request.request_id,
                0,
                &envelope.to_stdout_json(),
                "",
            )
            .await;
        }
        let result = task.await.unwrap();
        assert!(result.success, "{status}: {result:?}");
        assert_eq!(probe_count, 1);
        let semantic = &result.output["semantic_navigation"];
        assert_eq!(semantic["supported"], true);
        assert_eq!(semantic["status"], status);
        assert_eq!(semantic["reason_code"], reason);
        if status == "unavailable" {
            assert_eq!(semantic["available"], false);
            assert_eq!(result.output["readiness"]["status"], "warn");
            assert_eq!(result.output["readiness"]["blocking"], false);
            assert_eq!(
                result.output["warnings"],
                json!(["semantic_navigation_unavailable"])
            );
        } else {
            assert_eq!(semantic["available"], Value::Null);
            assert!(result.output.get("readiness").is_none(), "{result:?}");
            assert!(result.output.get("warnings").is_none(), "{result:?}");
        }
        assert!(semantic.get("action_required").is_none());
        assert!(result.output.get("blockers").is_none());
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::to_value(&result).unwrap(),
            &crate::tool_runtime::registry::output_schema_for_tool("work_on_project"),
        )
        .unwrap();
    }
}
