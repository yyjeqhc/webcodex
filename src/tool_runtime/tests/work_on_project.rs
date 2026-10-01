//! Focused tests for the canonical `work_on_project` coding entry point.
//!
//! `work_on_project` validates an explicit project source or exact checkout Session plus the task inputs,
//! invokes the shared coding workflow engine, and projects a compact startup
//! result. It never binds a current window, never guesses a recent Session, and
//! never falls back to a credential-wide Session.

mod session_resume;

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
                    tool_name: "check_workspace_hygiene".to_string(),
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

fn project_worktree_work_on_project_call(
    project: &str,
    instruction: &str,
    base_ref: Option<&str>,
    session_id: Option<&str>,
) -> ToolCall {
    ToolCall::WorkOnProject {
        project: project.to_string(),
        client_id: None,
        path: None,
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
        async move {
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context(
                    call,
                    Some(&auth),
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                    Default::default(),
                    None,
                    true,
                    vec!["project.instructions".into()],
                    Default::default(),
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
                    "root_fingerprint": format!("wc_projroot_{}", "b".repeat(64)),
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
include!("work_on_project/surface_and_projection.rs");
include!("work_on_project/bootstrap_and_path_source.rs");
include!("work_on_project/resume_and_instructions.rs");
include!("work_on_project/workflow_and_overview.rs");
