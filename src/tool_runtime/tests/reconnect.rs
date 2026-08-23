//! Cross-process continuity tests: runner disconnect/reconnect, durable exact
//! Workflow Session binding recovery, stale registration semantics,
//! meaningful-activity scoping, and mixed-version diagnostics.

use super::support::*;
use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::shell_protocol::{
    AgentBuildInfo, AgentHostContext, ShellClientCapabilities, ShellClientRegisterRequest,
    ShellJobOpRequest, AGENT_PROTOCOL_VERSION_POLLING_V1, AGENT_PROTOCOL_VERSION_POLLING_V2,
    AGENT_PROTOCOL_VERSION_QUIC_V1, AGENT_PROTOCOL_VERSION_QUIC_V2,
    AGENT_PROTOCOL_VERSION_WEBSOCKET_V1, AGENT_PROTOCOL_VERSION_WEBSOCKET_V2,
};
use crate::tool_runtime::tool_inputs::{SessionMode, StartupDetail};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::Value;
use std::sync::Arc;

fn rewrite_persisted_session_as_legacy(
    ledger: &std::path::Path,
    session_id: &str,
    keep_durable_bindings: bool,
) {
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(ledger).unwrap()).unwrap();
    let session = value["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|session| session["session_id"] == session_id)
        .expect("persisted Session");
    assert!(session
        .as_object_mut()
        .unwrap()
        .remove("owner_authority_fingerprint")
        .is_some());
    if !keep_durable_bindings {
        value["durable_current_bindings"] = Value::Array(Vec::new());
    }
    std::fs::write(ledger, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn persisted_session_authority_fingerprint(
    ledger: &std::path::Path,
    session_id: &str,
) -> Option<String> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(ledger).unwrap()).unwrap();
    value["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["session_id"] == session_id)
        .and_then(|session| session["owner_authority_fingerprint"].as_str())
        .map(str::to_string)
}

fn rewrite_persisted_session_with_malformed_authority(ledger: &std::path::Path, session_id: &str) {
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(ledger).unwrap()).unwrap();
    let session = value["sessions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|session| session["session_id"] == session_id)
        .expect("persisted Session");
    session["owner_authority_fingerprint"] = Value::String("not-a-valid-fingerprint".to_string());
    std::fs::write(ledger, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn register_request(
    client_id: &str,
    instance: &str,
    process_started_at: Option<i64>,
    build: Option<AgentBuildInfo>,
    protocol: &str,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        client_id: client_id.to_string(),
        agent_instance_id: instance.to_string(),
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: Some(ShellClientCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            jobs: true,
            async_jobs: true,
            async_shell_jobs: true,
            ..Default::default()
        }),
        projects: Some(vec![registered_project("proj", "/tmp/reconnect-proj")]),
        agent_protocol_version: Some(protocol.to_string()),
        policy: None,
        process_started_at,
        build,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
    }
}

async fn layers(runtime: &ToolRuntime) -> Value {
    let status = runtime.runtime_status(None).await;
    assert!(status.success);
    status.output["connection_layers"].clone()
}

fn assert_layer_contract(layer: &Value, context: &str) {
    for field in [
        "status",
        "observed_at",
        "source",
        "age_secs",
        "stale_after_secs",
        "reason_code",
    ] {
        assert!(
            layer.get(field).is_some(),
            "{context} layer missing contract field {field}: {layer}"
        );
    }
}

#[tokio::test]
async fn runner_disconnect_and_reconnect_change_layers_independently() {
    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(register_request(
            "rc-agent",
            "inst-a",
            Some(1_000),
            None,
            "polling-v1",
        ))
        .await
        .unwrap();

    // Connected: every runner-derived layer is a real observation.
    let connected = layers(&runtime).await;
    for name in [
        "runner_process",
        "server_transport",
        "server_registration",
        "project_registry",
        "connector_endpoint",
        "session_binding",
        "last_successful_tool_call",
    ] {
        assert_layer_contract(&connected[name], name);
    }
    assert_eq!(connected["runner_process"]["status"], "ready");
    assert_eq!(
        connected["runner_process"]["source"],
        "runner_process_report"
    );
    assert_eq!(connected["runner_process"]["process_started_at"], 1_000);
    assert_eq!(connected["server_transport"]["status"], "connected");
    assert_eq!(
        connected["server_transport"]["connection_instance"],
        "inst-a"
    );
    assert_eq!(connected["server_registration"]["status"], "registered");
    assert_eq!(connected["project_registry"]["status"], "registered");
    // Connector runtime is not configured in this process.
    assert_eq!(connected["connector_endpoint"]["status"], "not_configured");
    assert_eq!(
        connected["connector_endpoint"]["reason_code"],
        "connector_runtime_disabled"
    );

    // Disconnect: layers change independently; stale registration is not ready.
    runtime
        .shell_clients
        .reconcile_disconnect("rc-agent", "inst-a")
        .await;
    let disconnected = layers(&runtime).await;
    assert_eq!(disconnected["runner_process"]["status"], "stale");
    assert_eq!(
        disconnected["runner_process"]["reason_code"],
        "heartbeat_expired"
    );
    assert_eq!(disconnected["server_transport"]["status"], "disconnected");
    assert!(disconnected["server_transport"]["disconnected_at"].is_i64());
    assert_eq!(disconnected["server_registration"]["status"], "stale");
    assert_eq!(
        disconnected["server_registration"]["reason_code"],
        "registration_instance_disconnected"
    );
    assert_eq!(disconnected["project_registry"]["status"], "stale");
    assert_eq!(
        disconnected["project_registry"]["reason_code"],
        "providing_runner_disconnected"
    );

    // Reconnect with a NEW process instance: new connection replaces the old
    // state, the project re-registers, and no server restart was needed.
    runtime
        .shell_clients
        .register(register_request(
            "rc-agent",
            "inst-b",
            Some(2_000),
            None,
            "polling-v1",
        ))
        .await
        .unwrap();
    let reconnected = layers(&runtime).await;
    assert_eq!(reconnected["runner_process"]["status"], "ready");
    assert_eq!(reconnected["runner_process"]["process_started_at"], 2_000);
    assert_eq!(
        reconnected["server_transport"]["connection_instance"],
        "inst-b"
    );
    assert_eq!(reconnected["server_transport"]["status"], "connected");
    assert_eq!(reconnected["server_registration"]["status"], "registered");
    assert_eq!(
        reconnected["server_registration"]["runner_instance"],
        "inst-b"
    );
    assert_eq!(reconnected["project_registry"]["status"], "registered");

    // Calls recover: a dispatched shell tool reaches the new instance.
    let project = crate::tool_runtime::agent_project_runtime_id("rc-agent", "proj");
    let run = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "echo back".to_string(),
                        session_id: None,
                        timeout_secs: Some(5),
                        cwd: None,
                        purpose: None,
                        shell: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_agent_request_for_instance(&runtime, "rc-agent", "inst-b").await;
    complete_patch_agent_request_for_instance(
        &runtime,
        "rc-agent",
        "inst-b",
        &req.request_id,
        0,
        "back\n",
        "",
    )
    .await;
    let response = run.await.unwrap();
    assert!(response.success, "{:?}", response.error);
}

#[tokio::test]
async fn stale_heartbeat_without_disconnect_is_not_ready() {
    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(register_request(
            "stale-agent",
            "inst-a",
            None,
            None,
            "polling-v1",
        ))
        .await
        .unwrap();
    runtime
        .shell_clients
        .set_last_seen_for_test("stale-agent", chrono::Utc::now().timestamp() - 3600)
        .await;
    let stale = layers(&runtime).await;
    assert_eq!(stale["runner_process"]["status"], "stale");
    assert_eq!(stale["server_registration"]["status"], "stale");
    assert_eq!(stale["project_registry"]["status"], "stale");
    // Stale must never be projected as ready/connected.
    assert_ne!(stale["server_transport"]["status"], "connected");
}

#[tokio::test]
async fn no_runner_layers_are_not_observed_with_reason_codes() {
    let runtime = test_runtime();
    let empty = layers(&runtime).await;
    assert_eq!(empty["runner_process"]["status"], "not_observed");
    assert_eq!(
        empty["runner_process"]["reason_code"],
        "no_runner_registered"
    );
    assert_eq!(empty["server_transport"]["status"], "not_observed");
    assert_eq!(empty["server_registration"]["status"], "not_observed");
    assert_eq!(empty["project_registry"]["status"], "not_configured");
    assert_eq!(
        empty["session_binding"]["reason_code"],
        "exact_binding_requires_window_and_project_observation"
    );
    assert_eq!(empty["session_binding"]["process_local_cache"], true);
    assert_eq!(empty["session_binding"]["durable_exact_binding"], true);
    assert_eq!(empty["session_binding"]["restored_after_restart"], true);
    assert_eq!(
        empty["session_binding"]["requires_stable_window_identity"],
        true
    );
    assert_eq!(empty["session_binding"]["missing_identity_fallback"], false);
    assert_eq!(empty["last_successful_tool_call"]["status"], "not_observed");
    assert_eq!(
        empty["last_successful_tool_call"]["reason_code"],
        "no_meaningful_tool_calls_recorded"
    );
}

#[tokio::test]
async fn meaningful_activity_is_scoped_and_not_refreshed_by_status_calls() {
    let runtime = test_runtime();

    // runtime_status / discovery calls are not meaningful activity.
    for _ in 0..3 {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RuntimeStatus {
                    compact: false,
                    summary_only: false,
                    client_id: None,
                },
                None,
            )
            .await;
        assert!(result.success);
    }
    let before = layers(&runtime).await;
    assert_eq!(
        before["last_successful_tool_call"]["status"],
        "not_observed"
    );

    // A session start is real work and is recorded with principal scope.
    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: None,
                title: Some("continuity".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                execution_context: None,
            },
            None,
        )
        .await;
    assert!(result.success);

    let after = layers(&runtime).await;
    let last = &after["last_successful_tool_call"];
    assert_eq!(last["status"], "observed");
    assert_eq!(last["tool"], "start_session");
    assert_eq!(last["scope"], "principal");
    assert_eq!(last["surface"], "api");
    assert!(last["principal_kind"].is_string());
    let observed_at = last["observed_at"].as_i64().unwrap();

    // Additional status polling must not refresh the observation.
    for _ in 0..3 {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RuntimeStatus {
                    compact: false,
                    summary_only: false,
                    client_id: None,
                },
                None,
            )
            .await;
        assert!(result.success);
    }
    let still = layers(&runtime).await;
    assert_eq!(still["last_successful_tool_call"]["tool"], "start_session");
    assert_eq!(
        still["last_successful_tool_call"]["observed_at"]
            .as_i64()
            .unwrap(),
        observed_at
    );
}

#[tokio::test]
async fn durable_current_binding_restores_same_window_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);

    // "First server process": start a coding task with a current binding.
    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "restart-agent", "proj", &project_root).await;
    let start = dispatch_start_coding_task_with_local_agent(
        &runtime1,
        "restart-agent",
        ToolCall::StartCodingTask {
            project: project.clone(),
            client_id: None,
            path: None,
            temporary_project_name: None,
            title: Some("restart continuity".to_string()),
            mode: SessionMode::Normal,
            deny_write_tools: false,
            deny_shell_tools: false,
            detail: StartupDetail::Full,
            resume_session_id: None,
            bind_current: true,
            new_session: false,
            execution_context: None,
        },
    )
    .await;
    assert!(start.success, "{:?}", start.error);
    let session_id = start.output["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(start.output["session"]["current_binding"]["bound"], true);
    assert_eq!(
        start.output["connection_state"]["session_binding"]["status"],
        "bound"
    );
    assert_eq!(runtime1.sessions.status().durable_binding_count, 1);
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    // "Restarted server process": same ledger and exact transport window,
    // initially with an empty process-local cache.
    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "restart-agent", "proj", &project_root).await;
    assert_eq!(runtime2.sessions.process_local_binding_count_for_test(), 0);
    let restored_status = runtime2.sessions.status();
    assert_eq!(restored_status.restored_sessions, 1);
    assert_eq!(restored_status.restored_binding_count, 1);
    assert_eq!(restored_status.durable_binding_count, 1);

    // Startup resolves the durable exact binding, restores the local
    // cache, and appends a new instruction to the original Workflow Session.
    let restarted = dispatch_start_coding_task_with_local_agent(
        &runtime2,
        "restart-agent",
        ToolCall::StartCodingTask {
            project: project.clone(),
            client_id: None,
            path: None,
            temporary_project_name: None,
            title: Some("new post-restart context".to_string()),
            mode: SessionMode::Normal,
            deny_write_tools: false,
            deny_shell_tools: false,
            detail: StartupDetail::Full,
            resume_session_id: None,
            bind_current: true,
            new_session: false,
            execution_context: None,
        },
    )
    .await;
    assert!(restarted.success, "{:?}", restarted.error);
    assert_eq!(restarted.output["session"]["session_id"], session_id);
    assert_eq!(restarted.output["session"]["continuation"], "continued");
    assert_eq!(restarted.output["session"]["reused"], true);
    assert_eq!(
        runtime2
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    assert_eq!(runtime2.sessions.process_local_binding_count_for_test(), 1);
    let binding = &restarted.output["connection_state"]["session_binding"];
    assert_eq!(binding["status"], "bound");
    assert_eq!(binding["process_local_cache"], true);
    assert_eq!(binding["durable_exact_binding"], true);
    assert_eq!(binding["restored_after_restart"], true);
    assert!(binding["durable_resume"]
        .as_str()
        .unwrap()
        .contains("same exact principal"));

    let summary = runtime2.sessions.summary(&session_id, Some(10)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("restart continuity"));
    let instructions: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "task_instruction")
        .collect();
    assert_eq!(instructions.len(), 2);
    assert_eq!(
        instructions[0].instruction.as_deref(),
        Some("restart continuity")
    );
    assert_eq!(
        instructions[1].instruction.as_deref(),
        Some("new post-restart context")
    );
}

#[tokio::test]
async fn legacy_project_session_authority_migrates_via_work_on_project_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let alice = shared_key_auth_context("legacy-migration-alice");
    let alice_oauth = oauth_bridge_auth_context(
        "legacy-migration-alice",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_PROJECT_WRITE,
        ],
    );
    let expected_fingerprint =
        super::super::session_context::workflow_session_authority_fingerprint(Some(&alice))
            .unwrap();

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-migration-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-migration-agent",
        coding_start_call(&project, "legacy root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-migration-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_as_legacy(&ledger, &session_id, true);
    assert_eq!(
        persisted_session_authority_fingerprint(&ledger, &session_id),
        None
    );

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-migration-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    assert_eq!(runtime2.sessions.process_local_binding_count_for_test(), 0);
    assert_eq!(runtime2.sessions.status().restored_binding_count, 1);
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );

    let denied_before_migration = runtime2
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id: session_id.clone(),
                limit: None,
            },
            Some(&alice),
        )
        .await;
    assert!(!denied_before_migration.success);
    assert_eq!(
        denied_before_migration.output["error_kind"],
        "session_authority_denied"
    );

    // The old durable CurrentSessionKey is intentionally presentation-specific.
    // Canonical shared-key grouping applies only after this one-time migration.
    let oauth_first = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-migration-agent",
        work_on_project_resume_call(&project, "oauth cannot claim legacy binding", &session_id),
        Some(&alice_oauth),
        "legacy-migration-window",
    )
    .await;
    assert!(!oauth_first.success);
    assert_eq!(
        oauth_first.output["error_kind"],
        "legacy_session_authority_unverifiable"
    );
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );

    let migrated = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-migration-agent",
        work_on_project_resume_call(&project, "legacy migration continuation", &session_id),
        Some(&alice),
        "legacy-migration-window",
    )
    .await;
    assert!(migrated.success, "{:?}", migrated.error);
    assert_eq!(
        runtime2
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    let authority = runtime2
        .sessions
        .session_target_authority(&session_id)
        .unwrap();
    assert_eq!(authority.0.as_deref(), Some(project.as_str()));
    assert_eq!(authority.1.as_deref(), Some(expected_fingerprint.as_str()));
    let summary = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| event.kind == "task_instruction")
            .count(),
        2
    );

    let discussion = runtime2
        .dispatch_with_auth(
            ToolCall::SessionDiscussionSummary {
                session_id: session_id.clone(),
                limit: Some(10),
            },
            Some(&alice),
        )
        .await;
    assert!(discussion.success, "{:?}", discussion.error);
    runtime2.sessions.flush_persistence();
    assert_eq!(
        persisted_session_authority_fingerprint(&ledger, &session_id).as_deref(),
        Some(expected_fingerprint.as_str())
    );
    drop(runtime2);

    let runtime3 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime3,
        "legacy-migration-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    assert_eq!(
        runtime3
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1
            .as_deref(),
        Some(expected_fingerprint.as_str())
    );
    let alice_read = runtime3
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id: session_id.clone(),
                limit: None,
            },
            Some(&alice),
        )
        .await;
    assert!(alice_read.success, "{:?}", alice_read.error);

    let oauth_read = runtime3
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id,
                limit: None,
            },
            Some(&alice_oauth),
        )
        .await;
    assert!(oauth_read.success, "{:?}", oauth_read.error);
}

#[tokio::test]
async fn legacy_project_session_authority_requires_historical_durable_binding() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let alice = shared_key_auth_context("legacy-no-proof-alice");

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-no-proof-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-no-proof-agent",
        coding_start_call(&project, "legacy root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-no-proof-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_as_legacy(&ledger, &session_id, false);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-no-proof-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    assert_eq!(runtime2.sessions.status().durable_binding_count, 0);
    let process_local_key = super::super::session_context::current_session_key(
        Some(&alice),
        crate::tool_runtime::sessions::SessionTransport::Mcp,
        &project,
        &project_root.to_string_lossy(),
        Some(&ClientWindow::for_test("legacy-no-proof-window")),
    )
    .unwrap();
    runtime2
        .sessions
        .insert_process_local_binding_only_for_test(process_local_key, &session_id);
    assert_eq!(runtime2.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime2.sessions.status().durable_binding_count, 0);
    let before = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    let denied = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-no-proof-agent",
        coding_resume_call(&project, "must not claim by id", &session_id, false),
        Some(&alice),
        "legacy-no-proof-window",
    )
    .await;
    assert!(!denied.success);
    assert_eq!(
        denied.output["error_kind"],
        "legacy_session_authority_unverifiable"
    );
    assert_eq!(denied.output["state_changed"], false);
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );
    let after = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(
        runtime2
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
}

#[tokio::test]
async fn legacy_project_session_authority_malformed_fingerprint_is_not_migratable() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let alice = shared_key_auth_context("legacy-malformed-alice");

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-malformed-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-malformed-agent",
        coding_start_call(&project, "malformed root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-malformed-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_with_malformed_authority(&ledger, &session_id);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-malformed-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let restored_authority = runtime2
        .sessions
        .session_target_authority(&session_id)
        .unwrap()
        .1;
    assert!(restored_authority.is_some());
    assert_ne!(
        restored_authority.as_deref(),
        Some(
            super::super::session_context::workflow_session_authority_fingerprint(Some(&alice))
                .unwrap()
                .as_str()
        )
    );
    let before = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    let denied = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-malformed-agent",
        coding_resume_call(&project, "must not migrate malformed", &session_id, false),
        Some(&alice),
        "legacy-malformed-window",
    )
    .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "session_authority_denied");
    assert_eq!(denied.output["state_changed"], false);
    let after = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        restored_authority
    );
    runtime2.sessions.flush_persistence();
    drop(runtime2);

    let runtime3 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    assert!(runtime3
        .sessions
        .session_target_authority(&session_id)
        .unwrap()
        .1
        .is_some());
}

#[tokio::test]
async fn legacy_project_session_authority_binding_proof_is_window_and_transport_exact() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let alice = shared_key_auth_context("legacy-exact-proof-alice");

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-exact-proof-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-exact-proof-agent",
        coding_start_call(&project, "legacy root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-exact-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_as_legacy(&ledger, &session_id, true);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-exact-proof-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let before = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    let wrong_window = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-exact-proof-agent",
        coding_resume_call(&project, "wrong window", &session_id, false),
        Some(&alice),
        "legacy-other-window",
    )
    .await;
    assert!(!wrong_window.success);
    assert_eq!(
        wrong_window.output["error_kind"],
        "legacy_session_authority_unverifiable"
    );
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );

    let wrong_transport = dispatch_start_coding_task_in_window_with_transport(
        &runtime2,
        "legacy-exact-proof-agent",
        coding_resume_call(&project, "wrong transport", &session_id, false),
        Some(&alice),
        "legacy-exact-window",
        crate::tool_runtime::sessions::SessionTransport::Api,
    )
    .await;
    assert!(!wrong_transport.success);
    assert_eq!(
        wrong_transport.output["error_kind"],
        "legacy_session_authority_unverifiable"
    );
    let after_denials = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(after_denials.events.len(), before.events.len());
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );

    let exact = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-exact-proof-agent",
        coding_resume_call(&project, "exact historical proof", &session_id, false),
        Some(&alice),
        "legacy-exact-window",
    )
    .await;
    assert!(exact.success, "{:?}", exact.error);
    assert_eq!(exact.output["session"]["session_id"], session_id);
    assert!(runtime2
        .sessions
        .session_target_authority(&session_id)
        .unwrap()
        .1
        .is_some());
}

#[tokio::test]
async fn legacy_project_session_authority_upgrade_is_atomic_with_failed_continuation() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let alice = shared_key_auth_context("legacy-atomic-alice");

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-atomic-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-atomic-agent",
        coding_start_call(&project, "legacy root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-atomic-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_as_legacy(&ledger, &session_id, true);
    let ledger_before_failure = std::fs::read(&ledger).unwrap();

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-atomic-agent",
        "proj",
        &project_root,
        &alice,
    )
    .await;
    let before = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    runtime2
        .sessions
        .fail_next_coding_continuity_precommit_for_test();
    let failed = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-atomic-agent",
        coding_resume_call(&project, "atomic migration", &session_id, false),
        Some(&alice),
        "legacy-atomic-window",
    )
    .await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["error_kind"],
        "coding_continuity_commit_failed"
    );
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );
    let after_failure = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(after_failure.events.len(), before.events.len());
    assert_eq!(runtime2.sessions.status().durable_binding_count, 1);
    runtime2.sessions.flush_persistence();
    assert_eq!(std::fs::read(&ledger).unwrap(), ledger_before_failure);

    let retried = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-atomic-agent",
        coding_resume_call(&project, "atomic migration", &session_id, false),
        Some(&alice),
        "legacy-atomic-window",
    )
    .await;
    assert!(retried.success, "{:?}", retried.error);
    assert!(runtime2
        .sessions
        .session_target_authority(&session_id)
        .unwrap()
        .1
        .is_some());
    let after_retry = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(
        after_retry
            .events
            .iter()
            .filter(|event| event.instruction.as_deref() == Some("atomic migration"))
            .count(),
        1
    );
}

#[tokio::test]
async fn legacy_project_session_authority_rejects_recycled_project_without_exact_binding() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let shell_clients = Arc::new(
        crate::shell_client::ShellClientRegistry::with_shared_key_limits_for_test(4, 8, 1),
    );
    let alice = shared_key_auth_context("legacy-recycled-authority-a");
    let bob = shared_key_auth_context("legacy-recycled-authority-b");

    let runtime1 = ToolRuntime::new_for_tests_with_shell_clients(shell_clients.clone())
        .with_session_ledger(&ledger);
    let project = register_agent_project_at_path_with_auth(
        &runtime1,
        "legacy-recycled-client",
        "recycled-project",
        &project_root,
        &alice,
    )
    .await;
    let started = dispatch_start_coding_task_in_window(
        &runtime1,
        "legacy-recycled-client",
        coding_start_call(&project, "legacy recycled root", SessionMode::Normal, false),
        Some(&alice),
        "legacy-recycled-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);
    rewrite_persisted_session_as_legacy(&ledger, &session_id, true);

    let runtime2 = ToolRuntime::new_for_tests_with_shell_clients(shell_clients.clone())
        .with_session_ledger(&ledger);
    let expired_at = chrono::Utc::now().timestamp() - 100;
    shell_clients
        .set_last_seen_for_test("legacy-recycled-client", expired_at)
        .await;
    let _ = shell_clients.list_clients_for_auth(Some(&alice)).await;
    assert!(shell_clients
        .get_client_view("legacy-recycled-client")
        .await
        .is_none());
    let recycled_project = register_agent_project_at_path_with_auth(
        &runtime2,
        "legacy-recycled-client",
        "recycled-project",
        &project_root,
        &bob,
    )
    .await;
    assert_eq!(recycled_project, project);

    let before = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    let denied_resume = dispatch_start_coding_task_in_window(
        &runtime2,
        "legacy-recycled-client",
        coding_resume_call(&project, "Bob must not claim", &session_id, false),
        Some(&bob),
        "legacy-recycled-window",
    )
    .await;
    assert!(!denied_resume.success);
    assert_eq!(
        denied_resume.output["error_kind"],
        "legacy_session_authority_unverifiable"
    );
    assert_eq!(denied_resume.output["state_changed"], false);

    let denied_read = runtime2
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id: session_id.clone(),
                limit: None,
            },
            Some(&bob),
        )
        .await;
    assert!(!denied_read.success);
    assert_eq!(denied_read.output["error_kind"], "session_authority_denied");
    assert_eq!(
        runtime2
            .sessions
            .session_target_authority(&session_id)
            .unwrap()
            .1,
        None
    );
    let after = runtime2.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.messages.total, before.messages.total);
    assert_eq!(after.lifecycle, before.lifecycle);
}

#[tokio::test]
async fn durable_current_binding_does_not_cross_windows_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let auth = auth_context(None, true);

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "window-restart-agent", "proj", &project_root)
            .await;
    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "window-restart-agent",
        coding_start_call(&project, "window A root", SessionMode::Normal, false),
        Some(&auth),
        "durable-window-a",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let first_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "window-restart-agent", "proj", &project_root).await;
    let other_window = dispatch_start_coding_task_in_window(
        &runtime2,
        "window-restart-agent",
        coding_start_call(&project, "window B root", SessionMode::Normal, false),
        Some(&auth),
        "durable-window-b",
    )
    .await;
    assert!(other_window.success, "{:?}", other_window.error);
    assert_ne!(other_window.output["session"]["session_id"], first_id);
    assert_eq!(other_window.output["session"]["continuation"], "created");
    assert_eq!(
        runtime2
            .sessions
            .active_session_count_for_test(Some(&project)),
        2
    );
}

#[tokio::test]
async fn durable_current_binding_does_not_cross_changed_canonical_root_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let first_root = dir.path().join("first");
    let second_root = dir.path().join("second");
    std::fs::create_dir_all(&first_root).unwrap();
    std::fs::create_dir_all(&second_root).unwrap();
    init_git_repo(&first_root);
    init_git_repo(&second_root);
    let auth = auth_context(None, true);

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "moving-restart-agent", "demo", &first_root)
            .await;
    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "moving-restart-agent",
        coding_start_call(&project, "first root", SessionMode::Normal, false),
        Some(&auth),
        "durable-moving-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let first_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let moved_project =
        register_agent_project_at_path(&runtime2, "moving-restart-agent", "demo", &second_root)
            .await;
    assert_eq!(moved_project, project);
    let moved = dispatch_start_coding_task_in_window(
        &runtime2,
        "moving-restart-agent",
        coding_start_call(&project, "second root", SessionMode::Normal, false),
        Some(&auth),
        "durable-moving-window",
    )
    .await;
    assert!(moved.success, "{:?}", moved.error);
    assert_ne!(moved.output["session"]["session_id"], first_id);
    assert_eq!(moved.output["session"]["continuation"], "created");
}

#[tokio::test]
async fn durable_current_binding_new_session_rebinds_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let auth = auth_context(None, true);

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "new-session-agent", "proj", &project_root).await;
    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "new-session-agent",
        coding_start_call(&project, "old root", SessionMode::Normal, false),
        Some(&auth),
        "durable-new-session-window",
    )
    .await;
    let isolated = dispatch_start_coding_task_in_window(
        &runtime1,
        "new-session-agent",
        coding_start_call(&project, "new root", SessionMode::Normal, true),
        Some(&auth),
        "durable-new-session-window",
    )
    .await;
    assert!(first.success && isolated.success);
    let old_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let new_id = isolated.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(old_id, new_id);
    assert!(runtime1.sessions.summary(&old_id, None).is_some());
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "new-session-agent", "proj", &project_root).await;
    let continued = dispatch_start_coding_task_in_window(
        &runtime2,
        "new-session-agent",
        coding_start_call(&project, "continue new root", SessionMode::Normal, false),
        Some(&auth),
        "durable-new-session-window",
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert_eq!(continued.output["session"]["session_id"], new_id);
    assert_eq!(continued.output["session"]["continuation"], "continued");
    assert!(runtime2.sessions.summary(&old_id, None).is_some());
    assert_eq!(
        runtime2
            .sessions
            .active_session_count_for_test(Some(&project)),
        2
    );
}

#[tokio::test]
async fn durable_current_binding_explicit_unbind_prevents_restart_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let auth = auth_context(None, true);
    let window = ClientWindow::for_test("durable-unbind-window");

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "unbind-agent", "proj", &project_root).await;
    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "unbind-agent",
        coding_start_call(&project, "bound root", SessionMode::Normal, false),
        Some(&auth),
        "durable-unbind-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let old_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let unbound = runtime1
        .dispatch_with_auth_transport_options_and_metadata_with_sandbox(
            ToolCall::UnbindCurrentSession {
                project: project.clone(),
            },
            Some(&auth),
            crate::tool_runtime::sessions::SessionTransport::Mcp,
            true,
            false,
            Default::default(),
            None,
            Some(&window),
        )
        .await;
    assert!(unbound.success, "{:?}", unbound.error);
    assert_eq!(unbound.output["had_binding"], true);
    assert_eq!(runtime1.sessions.status().durable_binding_count, 0);
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "unbind-agent", "proj", &project_root).await;
    let restarted = dispatch_start_coding_task_in_window(
        &runtime2,
        "unbind-agent",
        coding_start_call(&project, "fresh after unbind", SessionMode::Normal, false),
        Some(&auth),
        "durable-unbind-window",
    )
    .await;
    assert!(restarted.success, "{:?}", restarted.error);
    assert_ne!(restarted.output["session"]["session_id"], old_id);
    assert!(runtime2.sessions.summary(&old_id, None).is_some());
}

#[tokio::test]
async fn durable_current_binding_close_prevents_restart_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let auth = auth_context(None, true);

    let runtime1 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime1, "close-agent", "proj", &project_root).await;
    let first = dispatch_start_coding_task_in_window(
        &runtime1,
        "close-agent",
        coding_start_call(&project, "closable root", SessionMode::Normal, false),
        Some(&auth),
        "durable-close-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let closed_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let closed = runtime1
        .dispatch_with_auth(
            ToolCall::CloseSession {
                session_id: closed_id.clone(),
            },
            Some(&auth),
        )
        .await;
    assert!(closed.success, "{:?}", closed.error);
    assert_eq!(runtime1.sessions.status().durable_binding_count, 0);
    runtime1.sessions.flush_persistence();
    drop(runtime1);

    let runtime2 = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&runtime2, "close-agent", "proj", &project_root).await;
    let restarted = dispatch_start_coding_task_in_window(
        &runtime2,
        "close-agent",
        coding_start_call(&project, "fresh after close", SessionMode::Normal, false),
        Some(&auth),
        "durable-close-window",
    )
    .await;
    assert!(restarted.success, "{:?}", restarted.error);
    assert_ne!(restarted.output["session"]["session_id"], closed_id);
    assert_eq!(
        runtime2.sessions.lifecycle_state(&closed_id),
        Some(crate::tool_runtime::sessions::SessionLifecycle::Closed)
    );
}

#[tokio::test]
async fn agent_job_lost_on_disconnect_stays_terminal_after_reconnect() {
    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(register_request(
            "job-agent",
            "inst-a",
            None,
            None,
            "polling-v1",
        ))
        .await
        .unwrap();

    // Start an async agent job and let the agent pick it up.
    let job = runtime
        .shell_clients
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("job-agent".to_string()),
                cwd: None,
                command: Some("sleep 60".to_string()),
                timeout_secs: Some(120),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "reconnect-test".to_string(),
        )
        .await
        .unwrap();
    let _req = wait_for_agent_request_for_instance(&runtime, "job-agent", "inst-a").await;

    // Transport drops mid-job: job authority is not silently completed.
    runtime
        .shell_clients
        .reconcile_disconnect("job-agent", "inst-a")
        .await;
    let jobs = runtime.shell_clients.list_jobs(None).await;
    let lost = jobs
        .iter()
        .find(|info| info.job_id == job.job_id)
        .expect("job still queryable after disconnect");
    assert_eq!(lost.status, "lost");
    let first_ended_at = lost.ended_at.expect("lost job has terminal timestamp");

    // Reconnect with a new instance: the terminal state must not be
    // resurrected or duplicated.
    runtime
        .shell_clients
        .register(register_request(
            "job-agent",
            "inst-b",
            None,
            None,
            "polling-v1",
        ))
        .await
        .unwrap();
    let jobs = runtime.shell_clients.list_jobs(None).await;
    let still_lost = jobs
        .iter()
        .find(|info| info.job_id == job.job_id)
        .expect("job still queryable after reconnect");
    assert_eq!(still_lost.status, "lost");
    assert_eq!(still_lost.ended_at, Some(first_ended_at));
}

#[tokio::test]
async fn version_compatibility_accepts_all_normalized_legacy_wire_forms() {
    let runtime = test_runtime();
    let cases = [
        (
            "polling-inline",
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            "polling",
            "inline",
        ),
        (
            "polling-paged",
            AGENT_PROTOCOL_VERSION_POLLING_V2,
            "polling",
            "paged",
        ),
        (
            "websocket-inline",
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            "websocket",
            "inline",
        ),
        (
            "websocket-paged",
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V2,
            "websocket",
            "paged",
        ),
        (
            "quic-inline",
            AGENT_PROTOCOL_VERSION_QUIC_V1,
            "quic",
            "inline",
        ),
        (
            "quic-paged",
            AGENT_PROTOCOL_VERSION_QUIC_V2,
            "quic",
            "paged",
        ),
    ];

    for (client_id, protocol, transport, _) in cases.iter().copied() {
        runtime
            .shell_clients
            .register(register_request(client_id, "inst", None, None, protocol))
            .await
            .unwrap();
        runtime
            .shell_clients
            .set_transport(client_id, transport)
            .await
            .unwrap();
    }

    let status = runtime.runtime_status(None).await;
    assert!(status.success);
    let runners = status.output["version_compatibility"]["runners"]
        .as_array()
        .unwrap();
    let clients = status.output["agents"]["clients"].as_array().unwrap();
    for (client_id, raw_protocol, transport, inventory_strategy) in cases.iter().copied() {
        let runner = runners
            .iter()
            .find(|runner| runner["client_id"] == client_id)
            .unwrap_or_else(|| panic!("runner {client_id} missing"));
        assert_eq!(runner["agent_protocol_version"], raw_protocol);
        assert_eq!(runner["protocol_supported"], true);
        assert_eq!(runner["protocol_compatibility"], "v1");
        assert_eq!(runner["project_inventory_strategy"], inventory_strategy);
        assert_eq!(runner["status"], "compatible");

        let client = clients
            .iter()
            .find(|client| client["client_id"] == client_id)
            .unwrap_or_else(|| panic!("client {client_id} missing"));
        assert_eq!(client["transport"], transport);
    }
}

#[tokio::test]
async fn version_compatibility_reports_stable_mismatch_facts() {
    let runtime = test_runtime();
    let server_version = env!("CARGO_PKG_VERSION");

    // Same package version + supported protocol remains compatible even when
    // exact source differs. Source alignment is a separate diagnostic axis.
    let server_build = crate::build_info::runtime_build_info();
    let different_commit = format!(
        "{}-different",
        server_build.git_commit.unwrap_or("server-source")
    );
    runtime
        .shell_clients
        .register(register_request(
            "same-version-different-source",
            "inst-1",
            None,
            Some(AgentBuildInfo {
                version: Some(server_version.to_string()),
                git_commit: Some(different_commit),
                git_dirty: Some(false),
            }),
            "polling-v1",
        ))
        .await
        .unwrap();
    // Different build version → version_mismatch (connected ≠ compatible).
    runtime
        .shell_clients
        .register(register_request(
            "old-build",
            "inst-2",
            None,
            Some(AgentBuildInfo {
                version: Some("0.0.1".to_string()),
                git_commit: None,
                git_dirty: None,
            }),
            "websocket-v1",
        ))
        .await
        .unwrap();
    // Unsupported protocol identities fail registration and therefore never
    // become a diagnostic-but-operational runtime client.
    let unsupported = runtime
        .shell_clients
        .register(register_request(
            "legacy",
            "inst-3",
            None,
            None,
            "prehistoric-v0",
        ))
        .await
        .unwrap_err();
    assert_eq!(unsupported, "agent_protocol_version is unsupported");

    let status = runtime.runtime_status(None).await;
    assert!(status.success);
    let compat = &status.output["version_compatibility"];
    assert_eq!(compat["status"], "version_mismatch");
    assert_eq!(compat["server"]["version"], server_version);
    let runners = compat["runners"].as_array().unwrap();
    let by_id = |id: &str| {
        runners
            .iter()
            .find(|runner| runner["client_id"] == id)
            .unwrap_or_else(|| panic!("runner {id} missing"))
    };
    let different_source = by_id("same-version-different-source");
    assert_eq!(different_source["status"], "compatible");
    assert_eq!(different_source["version_matches_server"], true);
    assert_eq!(different_source["source_alignment"]["status"], "different");
    assert_eq!(
        different_source["source_alignment"]["reason_code"],
        "runner_git_commit_differs_from_server"
    );
    assert_eq!(compat["source_alignment"]["status"], "different");
    assert!(different_source.get("build_matches_server").is_none());

    assert_eq!(by_id("old-build")["status"], "version_mismatch");
    assert_eq!(
        by_id("old-build")["reason_code"],
        "runner_version_differs_from_server"
    );
    assert!(by_id("old-build")["action"]
        .as_str()
        .unwrap()
        .contains("align"));
    let compact = crate::tool_runtime::runtime_info::compact_runtime_status(&status.output);
    let compact_runner = compact["agents"]["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|runner| runner["client_id"] == "same-version-different-source")
        .unwrap();
    assert_eq!(compact_runner["version_matches_server"], true);
    assert_eq!(compact_runner["source_alignment"]["status"], "different");
    assert!(compact_runner.get("build_matches_server").is_none());
    assert_eq!(
        compact["version_compatibility"]["source_alignment"]["status"],
        "different"
    );

    // No secrets/paths in the diagnostics.
    let text = compat.to_string().to_lowercase();
    assert!(!text.contains("token"));
    assert!(!text.contains("/root/"));
}

#[tokio::test]
async fn runner_host_context_projects_to_full_list_and_compact_runtime() {
    let runtime = test_runtime();
    let mut request = register_request("sf", "inst-host-context", None, None, "polling-v1");
    request.host_context = Some(AgentHostContext {
        role: Some("server_host".to_string()),
        runtime: Some("Prefer this Runner for operations on its own host.".to_string()),
        service: Some("Use the ordinary host-local service mechanism.".to_string()),
        network: None,
        architecture: Some("Hosts the WebCodex Server/control plane.".to_string()),
    });
    runtime.shell_clients.register(request).await.unwrap();

    let status = runtime.runtime_status(None).await;
    assert!(status.success);
    let full = status.output["agents"]["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_id"] == "sf")
        .unwrap();
    assert_eq!(full["host_context"]["source"], "runner_config");
    assert_eq!(full["host_context"]["role"], "server_host");
    assert_eq!(
        full["host_context"]["architecture"],
        "Hosts the WebCodex Server/control plane."
    );

    let compact = crate::tool_runtime::runtime_info::compact_runtime_status(&status.output);
    let compact_sf = compact["agents"]["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_id"] == "sf")
        .unwrap();
    assert_eq!(compact_sf["agent_instance_id"], "inst-host-context");
    assert_eq!(compact_sf["host_context"]["role"], "server_host");
    assert!(compact_sf.get("capabilities").is_none());
    assert!(compact_sf.get("policy").is_none());

    let listed = runtime.list_agents(None).await;
    assert!(listed.success);
    let listed_sf = listed.output["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_id"] == "sf")
        .unwrap();
    assert_eq!(listed_sf["host_context"]["source"], "runner_config");
    assert_eq!(listed_sf["host_context"]["role"], "server_host");

    // Same-instance reconnect republishes the current startup context; it does
    // not depend on the previous transport record for this descriptive fact.
    let mut reconnect = register_request("sf", "inst-host-context", None, None, "websocket-v1");
    reconnect.host_context = Some(AgentHostContext {
        role: Some("server_host".to_string()),
        network: Some("Internal destinations normally use the direct path.".to_string()),
        ..Default::default()
    });
    runtime.shell_clients.register(reconnect).await.unwrap();
    let after_reconnect = runtime.runtime_status(None).await;
    let sf = after_reconnect.output["agents"]["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_id"] == "sf")
        .unwrap();
    assert_eq!(
        sf["host_context"]["network"],
        "Internal destinations normally use the direct path."
    );
}

#[tokio::test]
async fn runner_host_context_is_revalidated_at_server_registration() {
    let runtime = test_runtime();
    let mut request = register_request("bad-context", "inst-bad", None, None, "polling-v1");
    request.host_context = Some(AgentHostContext {
        role: Some("Server Host".to_string()),
        ..Default::default()
    });
    let err = runtime.shell_clients.register(request).await.unwrap_err();
    assert!(err.contains("host_context.role"), "{err}");
    assert!(runtime
        .shell_clients
        .get_client_view("bad-context")
        .await
        .is_none());
}

/// Drive a `start_coding_task` dispatch while servicing the fake agent's git
/// inspection requests locally.
pub(in crate::tool_runtime::tests) async fn dispatch_start_coding_task_with_local_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
) -> crate::tool_runtime::ToolResult {
    let bootstrap = auth_context(None, true);
    dispatch_start_coding_task_in_window(
        runtime,
        client_id,
        call,
        Some(&bootstrap),
        "reconnect-window",
    )
    .await
}

pub(in crate::tool_runtime::tests) async fn dispatch_start_coding_task_in_window(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&AuthContext>,
    window_id: &str,
) -> crate::tool_runtime::ToolResult {
    dispatch_start_coding_task_in_window_with_transport(
        runtime,
        client_id,
        call,
        auth,
        window_id,
        crate::tool_runtime::sessions::SessionTransport::Mcp,
    )
    .await
}

async fn dispatch_start_coding_task_in_window_with_transport(
    runtime: &ToolRuntime,
    client_id: &str,
    call: ToolCall,
    auth: Option<&AuthContext>,
    window_id: &str,
    transport: crate::tool_runtime::sessions::SessionTransport,
) -> crate::tool_runtime::ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        let window_id = window_id.to_string();
        async move {
            let window = ClientWindow::for_test(&window_id);
            runtime
                .dispatch_with_auth_transport_options_and_metadata_with_sandbox(
                    call,
                    auth.as_ref(),
                    transport,
                    true,
                    false,
                    Default::default(),
                    None,
                    Some(&window),
                )
                .await
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "start_coding_task did not finish within the 10-second test deadline"
        );
        if let Some(req) = runtime
            .shell_clients
            .poll(crate::shell_protocol::ShellAgentPollRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
        {
            let (exit_code, stdout, stderr) = run_agent_shell_request_locally(&req);
            complete_patch_agent_request(
                runtime,
                client_id,
                &req.request_id,
                exit_code,
                &stdout,
                &stderr,
            )
            .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    }
    task.await.unwrap()
}

fn coding_start_call(
    project: &str,
    instruction: &str,
    mode: SessionMode,
    new_session: bool,
) -> ToolCall {
    ToolCall::StartCodingTask {
        project: project.to_string(),
        client_id: None,
        path: None,
        temporary_project_name: None,
        title: Some(instruction.to_string()),
        mode,
        deny_write_tools: false,
        deny_shell_tools: false,
        detail: StartupDetail::Standard,
        resume_session_id: None,
        bind_current: true,
        new_session,
        execution_context: None,
    }
}

fn coding_resume_call(
    project: &str,
    instruction: &str,
    session_id: &str,
    bind_current: bool,
) -> ToolCall {
    ToolCall::StartCodingTask {
        project: project.to_string(),
        client_id: None,
        path: None,
        temporary_project_name: None,
        title: Some(instruction.to_string()),
        mode: SessionMode::Normal,
        deny_write_tools: false,
        deny_shell_tools: false,
        detail: StartupDetail::Standard,
        resume_session_id: Some(session_id.to_string()),
        bind_current,
        new_session: false,
        execution_context: None,
    }
}

fn work_on_project_resume_call(project: &str, instruction: &str, session_id: &str) -> ToolCall {
    ToolCall::WorkOnProject {
        project: project.to_string(),
        client_id: None,
        path: None,
        instruction: instruction.to_string(),
        include_project_instructions: true,
        include_workflow_guidance: true,
        session_id: Some(session_id.to_string()),
    }
}

#[tokio::test]
async fn start_coding_task_reuses_same_window_project_and_appends_instruction() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "continuity-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "continuity-agent",
        coding_start_call(&project, "root objective", SessionMode::Normal, false),
        Some(&auth),
        "continuity-window",
    )
    .await;
    let second = dispatch_start_coding_task_in_window(
        &runtime,
        "continuity-agent",
        coding_start_call(&project, "follow-up objective", SessionMode::Normal, false),
        Some(&auth),
        "continuity-window",
    )
    .await;

    assert!(first.success, "{:?}", first.error);
    assert!(second.success, "{:?}", second.error);
    let first_id = first.output["session"]["session_id"].as_str().unwrap();
    let second_id = second.output["session"]["session_id"].as_str().unwrap();
    assert_eq!(second_id, first_id);
    assert_eq!(second.output["session"]["continuation"], "continued");
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    let summary = runtime.sessions.summary(first_id, Some(20)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("root objective"));
    let instructions: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "task_instruction")
        .collect();
    assert_eq!(instructions.len(), 2);
    assert_eq!(
        instructions[1].instruction.as_deref(),
        Some("follow-up objective")
    );
    assert_eq!(instructions[1].requested_mode.as_deref(), Some("normal"));
    assert_eq!(instructions[1].capability_changed, Some(false));
    assert_eq!(instructions[1].context_refreshed, Some(true));
}

#[tokio::test]
async fn start_coding_task_explicit_new_session_preserves_previous_session() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "isolation-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "isolation-agent",
        coding_start_call(&project, "first root", SessionMode::Normal, false),
        Some(&auth),
        "isolation-window",
    )
    .await;
    let isolated = dispatch_start_coding_task_in_window(
        &runtime,
        "isolation-agent",
        coding_start_call(&project, "isolated root", SessionMode::Normal, true),
        Some(&auth),
        "isolation-window",
    )
    .await;
    assert!(first.success && isolated.success);
    let first_id = first.output["session"]["session_id"].as_str().unwrap();
    let isolated_id = isolated.output["session"]["session_id"].as_str().unwrap();
    assert_ne!(first_id, isolated_id);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        2
    );
    let old = runtime.sessions.summary(first_id, Some(10)).unwrap();
    assert_eq!(old.title.as_deref(), Some("first root"));
    assert_eq!(
        old.lifecycle,
        crate::tool_runtime::sessions::SessionLifecycle::Active
    );
}

#[tokio::test]
async fn start_coding_task_bindings_are_isolated_by_window() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "window-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);
    let first_a = dispatch_start_coding_task_in_window(
        &runtime,
        "window-agent",
        coding_start_call(&project, "window A", SessionMode::Normal, false),
        Some(&auth),
        "window-a",
    )
    .await;
    let first_b = dispatch_start_coding_task_in_window(
        &runtime,
        "window-agent",
        coding_start_call(&project, "window B", SessionMode::Normal, false),
        Some(&auth),
        "window-b",
    )
    .await;
    let again_a = dispatch_start_coding_task_in_window(
        &runtime,
        "window-agent",
        coding_start_call(&project, "window A again", SessionMode::Normal, false),
        Some(&auth),
        "window-a",
    )
    .await;
    assert!(first_a.success && first_b.success && again_a.success);
    assert_ne!(
        first_a.output["session"]["session_id"],
        first_b.output["session"]["session_id"]
    );
    assert_eq!(
        first_a.output["session"]["session_id"],
        again_a.output["session"]["session_id"]
    );
}

#[tokio::test]
async fn start_coding_task_switches_projects_and_restores_each_binding() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    init_git_repo(dir_a.path());
    init_git_repo(dir_b.path());
    let runtime = ToolRuntime::new_for_tests();
    let project_a =
        register_agent_project_at_path(&runtime, "project-a-agent", "a", dir_a.path()).await;
    let project_b =
        register_agent_project_at_path(&runtime, "project-b-agent", "b", dir_b.path()).await;
    let auth = auth_context(None, true);

    let first_a = dispatch_start_coding_task_in_window(
        &runtime,
        "project-a-agent",
        coding_start_call(&project_a, "work on A", SessionMode::Normal, false),
        Some(&auth),
        "project-switch-window",
    )
    .await;
    let first_b = dispatch_start_coding_task_in_window(
        &runtime,
        "project-b-agent",
        coding_start_call(&project_b, "work on B", SessionMode::Normal, false),
        Some(&auth),
        "project-switch-window",
    )
    .await;
    let again_a = dispatch_start_coding_task_in_window(
        &runtime,
        "project-a-agent",
        coding_start_call(&project_a, "return to A", SessionMode::Normal, false),
        Some(&auth),
        "project-switch-window",
    )
    .await;
    assert!(first_a.success && first_b.success && again_a.success);
    assert_ne!(
        first_a.output["session"]["session_id"],
        first_b.output["session"]["session_id"]
    );
    assert_eq!(
        first_a.output["session"]["session_id"],
        again_a.output["session"]["session_id"]
    );
}

#[tokio::test]
async fn failed_project_start_does_not_pollute_full_runtime_binding() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_agent_project_at_path(&runtime, "stable-agent", "a", dir.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "stable-agent",
        coding_start_call(&project, "stable A", SessionMode::Normal, false),
        Some(&auth),
        "failed-switch-window",
    )
    .await;
    assert!(first.success);

    let failed = dispatch_start_coding_task_in_window(
        &runtime,
        "stable-agent",
        coding_start_call(
            "agent:missing-agent:b",
            "failed B",
            SessionMode::Normal,
            false,
        ),
        Some(&auth),
        "failed-switch-window",
    )
    .await;
    assert!(!failed.success);
    assert_eq!(failed.output["error_kind"], "unknown_project");

    let again = dispatch_start_coding_task_in_window(
        &runtime,
        "stable-agent",
        coding_start_call(&project, "continue A", SessionMode::Normal, false),
        Some(&auth),
        "failed-switch-window",
    )
    .await;
    assert!(again.success);
    assert_eq!(
        first.output["session"]["session_id"],
        again.output["session"]["session_id"]
    );
}

#[tokio::test]
async fn start_coding_task_mode_upgrade_is_atomic_and_permission_checked() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    commit_file(
        dir.path(),
        "AGENTS.md",
        "# Test rules\n\nPreserve focused exploration.\n",
        "add rules",
    );
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    commit_file(
        dir.path(),
        "src/inspect.rs",
        "pub fn inspected() -> bool { true }\n",
        "add inspected source",
    );
    let runtime = ToolRuntime::new_for_tests();
    let read_auth = oauth_bridge_auth_context(
        "continuity-subject",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
        ],
    );
    let write_auth = oauth_bridge_auth_context(
        "continuity-subject",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_PROJECT_WRITE,
        ],
    );
    let project = register_agent_project_at_path_with_auth(
        &runtime,
        "oauth-client",
        "demo",
        dir.path(),
        &read_auth,
    )
    .await;
    let inspected = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_start_call(&project, "inspect first", SessionMode::Inspect, false),
        Some(&read_auth),
        "upgrade-window",
    )
    .await;
    assert!(inspected.success, "{:?}", inspected.error);
    let session_id = inspected.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let read = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        ToolCall::ReadFile {
            project: project.clone(),
            path: "src/inspect.rs".to_string(),
            session_id: Some(session_id.clone()),
            start_line: None,
            limit: None,
            with_line_numbers: None,
        },
        Some(&read_auth),
        "upgrade-window",
    )
    .await;
    assert!(read.success, "{:?}", read.error);

    let denied = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_start_call(&project, "request writes", SessionMode::Normal, false),
        Some(&read_auth),
        "upgrade-window",
    )
    .await;
    assert!(!denied.success);
    assert_eq!(
        denied.output["error_kind"],
        "session_capability_upgrade_denied"
    );
    let unchanged = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(unchanged.mode, SessionMode::Inspect);
    assert!(unchanged.guards.deny_write_tools);
    assert!(!unchanged.guards.deny_shell_tools);
    assert_eq!(
        unchanged
            .events
            .iter()
            .filter(|event| event.kind == "task_instruction")
            .count(),
        1
    );

    let upgraded = dispatch_start_coding_task_in_window(
        &runtime,
        "oauth-client",
        coding_start_call(&project, "enable writes", SessionMode::Normal, false),
        Some(&write_auth),
        "upgrade-window",
    )
    .await;
    assert!(upgraded.success, "{:?}", upgraded.error);
    assert_eq!(upgraded.output["session"]["session_id"], session_id);
    assert_eq!(upgraded.output["session"]["mode"], "normal");
    assert_eq!(upgraded.output["instructions"]["status"], "reused");
    assert_eq!(upgraded.output["instructions"]["content_included"], false);
    assert_eq!(
        upgraded.output["continuation"]["exploration"]["paths"]["items"],
        serde_json::json!(["src/inspect.rs"])
    );
    assert_eq!(
        upgraded.output["continuation"]["exploration"]["read_count"],
        1
    );
    assert_eq!(
        upgraded.output["continuation"]["exploration"]["complete"],
        true
    );
    let summary = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    let transition = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "task_instruction")
        .unwrap();
    assert_eq!(transition.previous_mode.as_deref(), Some("inspect"));
    assert_eq!(transition.requested_mode.as_deref(), Some("normal"));
    assert_eq!(transition.capability_changed, Some(true));
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| {
                event.kind == "tool_call_finished"
                    && event.tool_name == "read_file"
                    && event.status.as_deref() == Some("succeeded")
            })
            .count(),
        1,
        "start_coding_task must not reread ordinary explored source files"
    );
}

#[tokio::test]
async fn durable_current_binding_failed_continuity_commit_is_fully_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let project_root = dir.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    init_git_repo(&project_root);
    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_agent_project_at_path(&runtime, "fault-agent", "demo", &project_root).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "fault-agent",
        coding_start_call(&project, "stable root", SessionMode::ReadOnly, false),
        Some(&auth),
        "fault-window",
    )
    .await;
    assert!(first.success);
    let session_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    runtime.sessions.flush_persistence();
    let ledger_before_failure = std::fs::read(&ledger).unwrap();
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);

    runtime
        .sessions
        .fail_next_coding_continuity_precommit_for_test();
    let failed = dispatch_start_coding_task_in_window(
        &runtime,
        "fault-agent",
        coding_start_call(&project, "must roll back", SessionMode::Normal, false),
        Some(&auth),
        "fault-window",
    )
    .await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["error_kind"],
        "coding_continuity_commit_failed"
    );
    let unchanged = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(unchanged.mode, SessionMode::ReadOnly);
    assert_eq!(
        unchanged
            .events
            .iter()
            .filter(|event| event.kind == "task_instruction")
            .count(),
        1
    );
    assert_eq!(runtime.sessions.process_local_binding_count_for_test(), 1);
    assert_eq!(runtime.sessions.status().durable_binding_count, 1);
    runtime.sessions.flush_persistence();
    assert_eq!(std::fs::read(&ledger).unwrap(), ledger_before_failure);
    drop(runtime);

    // A restart after the failed attempt still resolves the original binding;
    // retrying appends exactly one copy of the rejected instruction.
    let restarted = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    register_agent_project_at_path(&restarted, "fault-agent", "demo", &project_root).await;
    assert_eq!(restarted.sessions.process_local_binding_count_for_test(), 0);
    assert_eq!(restarted.sessions.status().restored_binding_count, 1);
    let retried = dispatch_start_coding_task_in_window(
        &restarted,
        "fault-agent",
        coding_start_call(&project, "must roll back", SessionMode::Normal, false),
        Some(&auth),
        "fault-window",
    )
    .await;
    assert!(retried.success, "{:?}", retried.error);
    assert_eq!(retried.output["session"]["session_id"], session_id);
    assert_eq!(restarted.sessions.process_local_binding_count_for_test(), 1);
    let summary = restarted.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| event.instruction.as_deref() == Some("must roll back"))
            .count(),
        1
    );
}

#[tokio::test]
async fn changed_repository_root_does_not_reuse_project_id_binding() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    init_git_repo(first_root.path());
    init_git_repo(second_root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "moving-agent", "demo", first_root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "moving-agent",
        coding_start_call(&project, "first root", SessionMode::Normal, false),
        Some(&auth),
        "moving-window",
    )
    .await;
    register_agent_project_at_path(&runtime, "moving-agent", "demo", second_root.path()).await;
    let moved = dispatch_start_coding_task_in_window(
        &runtime,
        "moving-agent",
        coding_start_call(&project, "second root", SessionMode::Normal, false),
        Some(&auth),
        "moving-window",
    )
    .await;
    assert!(first.success && moved.success);
    assert_ne!(
        first.output["session"]["session_id"],
        moved.output["session"]["session_id"]
    );
    assert_eq!(moved.output["session"]["continuation"], "created");
}
