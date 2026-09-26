use super::super::work_result::{
    build_work_result_projection, work_result_state_version, MAX_WORK_RESULT_FILES,
};
use super::super::*;
use super::support::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn counts(conflicted: u64) -> Value {
    json!({
        "modified": 2,
        "added": 1,
        "deleted": 0,
        "renamed": 0,
        "copied": 0,
        "untracked": 1,
        "conflicted": conflicted,
        "staged": 0,
        "unstaged": 3
    })
}

fn validation(status: &str, latest: &str, successes: u64, failures: u64) -> Value {
    json!({
        "status": status,
        "latest_status": latest,
        "successes": successes,
        "failures": failures
    })
}

fn current_validation(status: &str, unresolved: u64, gaps: u64) -> Value {
    json!({
        "status": status,
        "unresolved_failure_count": unresolved,
        "evidence_gap_event_count": gaps
    })
}

fn review(total: u64) -> Value {
    json!({
        "available": true,
        "total": total,
        "read_only_inspection_count": total,
        "search_count": 0,
        "diff_review_count": total,
        "workspace_review_count": total,
        "hygiene_review_count": 0,
        "tools": ["show_changes", "git_review_summary"]
    })
}

#[test]
fn work_result_projection_is_sparse_bounded_and_honest() {
    let mut files = Vec::new();
    for index in 0..10 {
        files.push(json!({
            "path": format!("src/file_{index}.rs"),
            "status": if index == 0 { "added" } else { "modified" },
            "kind": "tracked",
            "staged": false,
            "unstaged": true,
            "additions": index + 1,
            "deletions": index
        }));
    }
    files.push(json!({"path": "src/binary.bin", "status": "modified", "kind": "tracked"}));
    files.push(json!({"path": "../escape", "status": "modified", "additions": 9, "deletions": 9}));
    let workspace = json!({
        "git_available": true,
        "clean": false,
        "branch": "feature/work-result",
        "head": {"commit": "a".repeat(40), "short": "aaaaaaaa"},
        "counts": counts(1),
        "files_total": files.len(),
        "files_truncated": false,
        "files": files
    });
    let projected = build_work_result_projection(
        "agent:special:demo",
        &format!("wc_sess_{}", "1".repeat(32)),
        true,
        &workspace,
        &validation("mixed", "passed", 8, 1),
        &current_validation("failed", 1, 2),
        &review(2),
        false,
    );
    assert_eq!(
        projected["workspace"]["files"].as_array().unwrap().len(),
        MAX_WORK_RESULT_FILES
    );
    assert_eq!(projected["workspace"]["truncated"], true);
    assert_eq!(projected["workspace"]["line_stats_partial"], true);
    assert_eq!(projected["workspace"]["counts"]["conflicted"], 1);
    assert_eq!(projected["validation"]["status"], "mixed");
    assert_eq!(projected["validation"]["current_status"], "failed");
    assert_eq!(projected["validation"]["unresolved_failures"], 1);
    assert_eq!(projected["validation"]["evidence_gaps"], 2);
    assert_eq!(projected["review"]["total"], 2);
    assert_eq!(projected["validation"]["history_partial"], false);
    assert_eq!(projected["review"]["history_partial"], false);
    assert!(projected["review"].get("passed").is_none());
    let serialized = projected.to_string();
    for private in ["stdout", "stderr", "job_id", "continuation", "message_body"] {
        assert!(!serialized.contains(private));
    }
}

#[test]
fn work_result_preserves_unproven_source_without_hiding_historical_execution_success() {
    let projected = build_work_result_projection(
        "agent:special:demo",
        "wc_sess_0123456789abcdef",
        true,
        &json!({"git_available":true,"clean":true}),
        &validation("passed", "passed", 1, 0),
        &current_validation("unproven", 0, 0),
        &review(0),
        false,
    );
    assert_eq!(projected["validation"]["status"], "passed");
    assert_eq!(projected["validation"]["successes"], 1);
    assert_eq!(projected["validation"]["current_status"], "unproven");
}

#[test]
fn work_result_state_version_matches_buffered_projection_hash() {
    let projection = build_work_result_projection(
        "agent:special:项目-🦀",
        &format!("wc_sess_{}", "9".repeat(32)),
        true,
        &json!({
            "git_available": true,
            "clean": false,
            "branch": "feature/escaped-\\-\"-分支",
            "counts": counts(0),
            "files_total": 2,
            "files": [
                {"path": "src/日本語.rs", "status": "modified", "kind": "tracked", "additions": 2, "deletions": 1},
                {"path": "src/quoted_\\\".rs", "status": "added", "kind": "tracked", "additions": 3, "deletions": 0}
            ]
        }),
        &json!({
            "status": "mixed",
            "latest_status": "failed",
            "successes": 7,
            "failures": 2,
            "history": [{"kind": "test", "name": "unicode::你好"}]
        }),
        &json!({"status": "failed", "unresolved_failure_count": 1, "evidence_gap_event_count": 2}),
        &json!({"available": true, "total": 2, "tools": ["show_changes", "git_review_summary"]}),
        true,
    );
    let expected = format!(
        "wr2_{:x}",
        Sha256::digest(serde_json::to_vec(&projection).unwrap())
    );
    assert_eq!(work_result_state_version(&projection), expected);
    assert!(projection.get("state_version").is_none());
}

#[test]
fn work_result_projection_handles_clean_non_git_and_unknown_validation_without_invention() {
    let clean = build_work_result_projection(
        "agent:special:demo",
        &format!("wc_sess_{}", "2".repeat(32)),
        true,
        &json!({
            "git_available": true,
            "clean": true,
            "counts": counts(0),
            "files_total": 0,
            "files": []
        }),
        &validation("not_run", "not_run", 0, 0),
        &current_validation("not_run", 0, 0),
        &json!({"available": true, "total": 0}),
        false,
    );
    assert_eq!(clean["workspace"]["clean"], true);
    assert_eq!(clean["workspace"]["additions"], 0);
    assert_eq!(clean["workspace"]["deletions"], 0);
    assert_eq!(clean["validation"]["current_status"], "not_run");
    assert_eq!(clean["review"]["total"], 0);

    let unavailable = build_work_result_projection(
        "agent:special:demo",
        &format!("wc_sess_{}", "3".repeat(32)),
        true,
        &json!({
            "git_available": false,
            "non_git_project": true,
            "counts": {},
            "files_total": 0,
            "files": []
        }),
        &json!({"status": "future_value", "latest_status": "future_value"}),
        &json!({"status": "future_value"}),
        &Value::Null,
        false,
    );
    assert_eq!(unavailable["workspace"]["git_available"], false);
    assert_eq!(unavailable["workspace"]["reason_code"], "non_git_project");
    assert_eq!(unavailable["validation"]["status"], "unknown");
    assert_eq!(unavailable["validation"]["current_status"], "unknown");
    assert_eq!(unavailable["review"]["available"], false);
}

#[test]
fn work_result_projection_marks_bounded_history_partial_without_inventing_absence() {
    let workspace = json!({
        "git_available": true,
        "clean": true,
        "counts": counts(0),
        "files_total": 0,
        "files": []
    });
    let session_id = format!("wc_sess_{}", "4".repeat(32));
    let partial = build_work_result_projection(
        "agent:special:demo",
        &session_id,
        true,
        &workspace,
        &validation("not_run", "not_run", 0, 0),
        &current_validation("unknown", 0, 0),
        &json!({"available": false, "total": 0}),
        true,
    );
    assert_eq!(partial["validation"]["history_partial"], true);
    assert_eq!(partial["validation"]["status"], "unknown");
    assert_eq!(partial["validation"]["latest_status"], "unknown");
    assert_eq!(partial["validation"]["current_status"], "unknown");
    assert_eq!(partial["review"]["history_partial"], true);
    assert_eq!(partial["review"]["total"], 0);

    for current in ["unproven", "failed", "stale"] {
        let projected = build_work_result_projection(
            "agent:special:demo",
            &session_id,
            true,
            &workspace,
            &validation("not_run", "not_run", 0, 0),
            &current_validation(current, 0, 0),
            &json!({"available": false, "total": 0}),
            true,
        );
        assert_eq!(projected["validation"]["current_status"], current);
        assert_eq!(projected["validation"]["history_partial"], true);
    }
}

fn record_work_result_window_event(
    db: &std::sync::Arc<crate::Database>,
    auth: &crate::auth::AuthContext,
    window: &crate::client_window::ClientWindow,
    project: &str,
    operation: &str,
    at_ms: i64,
    meaningful: bool,
) {
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
            duration_ms: 10,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({}),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(window.key().to_string()),
            client_window_source: Some(window.source().to_string()),
            server_trace_id: Some(format!("work-result-{at_ms}")),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(at_ms),
            window_ended_at_ms: Some(at_ms + 10),
            request_observed_at_ms: Some(at_ms),
            response_handed_at_ms: Some(at_ms + 10),
            window_transition_kind: Some(
                if meaningful { "serial" } else { "unavailable" }.to_string(),
            ),
            response_streaming: Some(false),
            window_continuity_eligible: Some(meaningful),
            window_meaningful: meaningful,
            recorder_gap_session_id: None,
            workflow_links: Vec::new(),
        },
    );
}

fn record_work_result_window_session_context(
    db: &std::sync::Arc<crate::Database>,
    auth: &crate::auth::AuthContext,
    window: &crate::client_window::ClientWindow,
    project: &str,
    session_id: &str,
    at_ms: i64,
) {
    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
    crate::action_audit_sessions::record_action_event(
        db,
        crate::action_audit_sessions::ActionAuditEventInput {
            explicit_session_id: None,
            session_title: None,
            endpoint: "/mcp".to_string(),
            action_name: "toolsCall".to_string(),
            operation: Some("read_files".to_string()),
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
            server_trace_id: Some(format!("work-result-session-context-{at_ms}")),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(at_ms),
            window_ended_at_ms: Some(at_ms + 1),
            request_observed_at_ms: Some(at_ms),
            response_handed_at_ms: Some(at_ms + 1),
            window_transition_kind: Some("serial".to_string()),
            response_streaming: Some(false),
            window_continuity_eligible: Some(true),
            window_meaningful: true,
            recorder_gap_session_id: None,
            workflow_links: vec![crate::action_audit_sessions::ActionAuditWorkflowLinkInput {
                workflow_session_id: session_id.to_string(),
                relation: crate::action_audit_sessions::WorkflowSessionRelation::Recording,
                project: Some(project.to_string()),
            }],
        },
    );
}

async fn present_window_once(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    auth: &crate::auth::AuthContext,
    window: &crate::client_window::ClientWindow,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        let auth = auth.clone();
        let window = window.clone();
        async move {
            runtime
                .present_work_result_for_window(project, None, Some(&auth), Some(&window))
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    complete_agent_request_by_running_locally(runtime, client_id, request).await;
    task.await.unwrap()
}

async fn refresh_window_work_result_once(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    auth: &crate::auth::AuthContext,
    window: &crate::client_window::ClientWindow,
) -> ToolResult {
    use super::super::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        let auth = auth.clone();
        let window = window.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "work_result_state".into(),
                        arguments: json!({"project": project}),
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: None,
                        auth: Some(&auth),
                        window: Some(&window),
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    ToolInvocationMetadata::default(),
                    ToolProtocolCapabilities {
                        work_result_app: true,
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    complete_agent_request_by_running_locally(runtime, client_id, request).await;
    let outcome = task.await.unwrap();
    assert!(outcome.error_status.is_none(), "{:?}", outcome.error_status);
    outcome.result.unwrap()
}

async fn refresh_once(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    session_id: &str,
    auth: &crate::auth::AuthContext,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        let session_id = session_id.to_string();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::WorkResultState {
                        project,
                        session_id: Some(session_id),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    complete_agent_request_by_running_locally(runtime, client_id, request).await;
    task.await.unwrap()
}

#[tokio::test]
async fn work_result_window_card_needs_no_session_and_uses_all_window_activity() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let audit = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(
        crate::Database::open(&audit.path().join("work-result-window.db")).unwrap(),
    );
    let runtime = test_runtime().with_window_activity_database(db.clone());
    let project =
        register_runner_project_at_path(&runtime, "work-result-window", "demo", tmp.path()).await;
    let other_tmp = tempfile::tempdir().unwrap();
    init_git_repo(other_tmp.path());
    commit_file(other_tmp.path(), "README.md", "other\n", "initial");
    let other_project = register_runner_project_at_path(
        &runtime,
        "work-result-window-other",
        "other",
        other_tmp.path(),
    )
    .await;
    let auth = auth_context(None, true);
    let window = crate::client_window::ClientWindow::for_test("work-result-window-card");

    record_work_result_window_event(&db, &auth, &window, &project, "read_files", 1_000, true);
    record_work_result_window_event(
        &db,
        &auth,
        &window,
        &project,
        "runtime_status",
        2_000,
        false,
    );
    record_work_result_window_event(
        &db,
        &auth,
        &window,
        &other_project,
        "read_files",
        3_000,
        true,
    );

    let result =
        present_window_once(&runtime, "work-result-window", &project, &auth, &window).await;
    assert!(result.success, "{:?}", result.error);
    let work = &result.output["work_result"];
    assert_eq!(work["project"], project);
    assert!(work.get("session_id").is_none());
    assert!(work.get("session").is_none());
    assert_eq!(work["collaboration"]["available"], false);
    assert_eq!(work["window_activity"]["events_observed"], 3);
    assert_eq!(work["window_activity"]["events_returned"], 3);
    let events = work["window_activity"]["events"].as_array().unwrap();
    assert!(events
        .iter()
        .all(|event| event["server_trace_id"].is_string()));
    assert_eq!(
        events
            .iter()
            .filter(|event| event["tool_name"] == "read_files")
            .count(),
        2
    );
    assert!(events
        .iter()
        .any(|event| event["tool_name"] == "runtime_status"));
    assert!(events
        .iter()
        .any(|event| event["meaningful"] == false && event["label"] == "Observed Runtime status"));
    assert!(events
        .iter()
        .any(|event| event["meaningful"] == true && event["ended_at_ms"] == 3_010));
    assert_eq!(work["activity"]["last_activity_at_ms"], 3_010);
    assert_eq!(work["activity"]["last"]["label"], "Read project files");
}

#[tokio::test]
async fn work_result_state_reauthorizes_exact_identity_and_refresh_does_not_record_target_session()
{
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "work-result", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Work Result refresh".to_string()),
    );
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();

    let first = refresh_once(
        &runtime,
        "work-result",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let first_version = first.output["work_result"]["state_version"]
        .as_str()
        .unwrap()
        .to_string();
    let second = refresh_once(
        &runtime,
        "work-result",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["work_result"]["state_version"], first_version);

    std::fs::write(tmp.path().join("README.md"), "changed\n").unwrap();
    let changed = refresh_once(
        &runtime,
        "work-result",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert_ne!(
        changed.output["work_result"]["state_version"],
        first_version
    );

    let alias_state = runtime
        .work_result_state("demo".to_string(), session.session_id.clone(), Some(&auth))
        .await;
    assert!(!alias_state.success);
    assert_eq!(
        alias_state.output["error_kind"],
        "work_result_project_not_exact"
    );
    let alias_present = runtime
        .present_work_result("demo".to_string(), session.session_id.clone(), Some(&auth))
        .await;
    assert!(!alias_present.success);
    assert_eq!(
        alias_present.output["error_kind"],
        "work_result_project_not_exact"
    );
    let dispatched_alias = runtime
        .dispatch_with_auth(
            ToolCall::WorkResultState {
                project: "demo".to_string(),
                session_id: Some(session.session_id.clone()),
            },
            Some(&auth),
        )
        .await;
    assert!(!dispatched_alias.success);
    assert_eq!(
        dispatched_alias.output["error_kind"],
        "work_result_project_not_exact"
    );
    assert!(
        probe_patch_agent_request(&runtime, "work-result")
            .await
            .is_none(),
        "a non-canonical project alias must fail before workspace observation, including through top-level dispatch"
    );

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(
        ToolCall::WorkResultState {
            project: project.clone(),
            session_id: Some(session.session_id.clone())
        }
        .session_id(),
        None
    );
    assert_eq!(
        ToolCall::PresentWorkResult {
            project: project.clone(),
            session_id: Some(session.session_id.clone())
        }
        .session_id(),
        None
    );

    let other_tmp = tempfile::tempdir().unwrap();
    init_git_repo(other_tmp.path());
    commit_file(other_tmp.path(), "README.md", "other\n", "other");
    let other_project =
        register_runner_project_at_path(&runtime, "work-result-other", "other", other_tmp.path())
            .await;
    let mismatch = runtime
        .work_result_state(
            other_project.clone(),
            session.session_id.clone(),
            Some(&auth),
        )
        .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    let present_mismatch = runtime
        .present_work_result(other_project, session.session_id.clone(), Some(&auth))
        .await;
    assert!(!present_mismatch.success);
    assert_eq!(
        present_mismatch.output["error_kind"],
        "session_project_mismatch"
    );
}

#[tokio::test]
async fn work_result_progress_uses_latest_meaningful_session_activity() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project = register_runner_project_at_path(
        &runtime,
        "work-result-meaningful-progress",
        "demo",
        tmp.path(),
    )
    .await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Work Result meaningful progress".to_string()),
    );

    let meaningful = runtime.sessions.record_tool_call_started_with_options(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "show_changes",
        &json!({"project": project, "include_diff": false}),
        Some(project.clone()),
        crate::tool_runtime::sessions::session_tool_contract("show_changes"),
    );
    runtime.sessions.record_tool_call_finished(
        meaningful,
        true,
        &json!({"git_available": true, "clean": true, "files": [], "files_total": 0}),
        None,
        None,
    );

    let presentation = runtime.sessions.record_tool_call_started_with_options(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Mcp,
        "present_work_result",
        &json!({"project": project, "session_id": session.session_id}),
        Some(project.clone()),
        crate::tool_runtime::sessions::session_tool_contract("present_work_result"),
    );
    runtime.sessions.record_tool_call_finished(
        presentation,
        true,
        &json!({"work_result": {"project": project, "session_id": session.session_id}}),
        None,
        None,
    );

    let state = refresh_once(
        &runtime,
        "work-result-meaningful-progress",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(state.success, "{:?}", state.error);
    assert_eq!(
        state.output["work_result"]["session"]["latest_activity"]["tool"], "show_changes",
        "presentation-only Session events must not masquerade as work progress"
    );
    let workflow = &state.output["work_result"]["workflow"];
    let activity = workflow["activity"].as_array().unwrap();
    assert_eq!(
        activity.len(),
        1,
        "paired calls appear once; presentation calls are excluded"
    );
    assert_eq!(activity[0]["stage"], "review");
    assert_eq!(activity[0]["label"], "Reviewed changes");
    assert_eq!(activity[0]["state"], "succeeded");
    assert!(activity[0].get("tool").is_none());
    assert!(activity[0].get("paths").is_none());
    assert_eq!(workflow["history_partial"], false);
    assert_eq!(
        state.output["work_result"]["session"]["latest_activity"]["kind"],
        "tool_call_finished"
    );
    assert!(
        state.output["work_result"]["session"]["events_total"]
            .as_u64()
            .unwrap()
            >= 4,
        "session event count remains an honest total rather than a tool-call count"
    );
}

#[tokio::test]
async fn work_result_refresh_reobserves_validation_source_staleness_without_recording() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "work-result-source", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Work Result source refresh".to_string()),
    );

    let start_fence = runtime
        .validation_sources
        .capture(&project)
        .expect("source fence");
    let initial_source = runtime
        .validation_sources
        .observe(&project, Some(&start_fence));
    assert_eq!(
        initial_source.freshness,
        webcodex_core::validation_source::ValidationFreshness::Unproven
    );
    assert_eq!(
        initial_source.observed_mutation_fence,
        webcodex_core::validation_source::ObservedMutationFence::Uncrossed
    );

    let started = runtime.sessions.record_tool_call_started_with_options(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "cargo_check",
        &json!({"project": "demo"}),
        Some(project.clone()),
        crate::tool_runtime::sessions::session_tool_contract("cargo_check"),
    );
    runtime.sessions.record_tool_call_finished(
        started,
        true,
        &json!({
            "terminal": true,
            "command_started": true,
            "command_completed": true,
            "execution_state": "completed",
            "exit_code": 0,
            "stdout_tail": "",
            "stderr_tail": "",
            "source_state": initial_source,
        }),
        None,
        None,
    );

    let before = runtime.sessions.summary(&session.session_id, None).unwrap();
    let persisted_finished = before
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "cargo_check")
        .expect("persisted validation finish");
    assert_eq!(
        persisted_finished.resolved_project.as_deref(),
        Some(project.as_str())
    );
    assert_eq!(
        persisted_finished
            .validation_output_summary
            .as_ref()
            .and_then(|value| value.pointer("/source_state/start_fence/epoch"))
            .and_then(Value::as_str),
        Some(start_fence.epoch.as_str())
    );
    let initial = refresh_once(
        &runtime,
        "work-result-source",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(initial.success, "{:?}", initial.error);
    assert_eq!(
        initial.output["work_result"]["validation"]["current_status"],
        "unproven"
    );

    let mutation = runtime
        .validation_sources
        .begin(&project)
        .expect("mutation observation");
    mutation.finish(&ToolResult::ok(json!({
        "execution_state": "completed",
        "state_changed": true,
    })));

    let mut reobserved_events = before.events.clone();
    runtime.refresh_validation_source_states(&mut reobserved_events);
    let reobserved = reobserved_events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "cargo_check")
        .and_then(|event| event.validation_output_summary.as_ref())
        .and_then(|value| value.get("source_state"))
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<webcodex_core::validation_source::ValidationSourceState>(value)
                .ok()
        })
        .expect("reobserved source state");
    assert_eq!(
        reobserved.freshness,
        webcodex_core::validation_source::ValidationFreshness::Stale
    );

    let stale = refresh_once(
        &runtime,
        "work-result-source",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(stale.success, "{:?}", stale.error);
    assert_eq!(
        stale.output["work_result"]["validation"]["current_status"],
        "stale"
    );
    assert_eq!(
        stale.output["work_result"]["validation"]["reason"],
        "validation_source_fence_crossed"
    );

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.updated_at, before.updated_at);
}

#[tokio::test]
async fn work_result_state_marks_truncated_session_evidence_partial_without_recording_refresh() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "work-result-partial", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Work Result bounded evidence".to_string()),
    );
    seed_recovery_events(&runtime, &session.session_id, &project, 110);
    let bounded = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    assert!(bounded.events_truncated);
    let before = runtime.sessions.summary(&session.session_id, None).unwrap();

    let state = refresh_once(
        &runtime,
        "work-result-partial",
        &project,
        &session.session_id,
        &auth,
    )
    .await;
    assert!(state.success, "{:?}", state.error);
    assert_eq!(
        state.output["work_result"]["validation"]["history_partial"],
        true
    );
    assert_eq!(
        state.output["work_result"]["validation"]["current_status"],
        "unknown"
    );
    assert_eq!(
        state.output["work_result"]["review"]["history_partial"],
        true
    );
    assert_eq!(state.output["work_result"]["review"]["total"], 0);

    let after = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert_eq!(after.events_total, before.events_total);
    assert_eq!(after.updated_at, before.updated_at);
}

#[tokio::test]
async fn work_result_state_fails_closed_for_foreign_session_authority() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "work-result-auth", "demo", tmp.path()).await;
    let bob = shared_key_auth_context("work-result-bob");
    let alice = shared_key_auth_context("work-result-alice");
    let fingerprint = workflow_session_authority_fingerprint(Some(&bob)).unwrap();
    let session = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("private Work Result".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(fingerprint)),
        )
        .unwrap();
    let denied = runtime
        .work_result_state(project.clone(), session.session_id.clone(), Some(&alice))
        .await;
    assert!(!denied.success);
    assert!(denied.output.get("work_result").is_none());
    assert!(denied
        .error
        .as_deref()
        .is_some_and(|error| !error.is_empty()));
    let present_denied = runtime
        .present_work_result(project, session.session_id, Some(&alice))
        .await;
    assert!(!present_denied.success);
    assert!(present_denied.output.get("work_result").is_none());
    assert!(present_denied
        .error
        .as_deref()
        .is_some_and(|error| !error.is_empty()));
    assert!(
        probe_patch_agent_request(&runtime, "work-result-auth")
            .await
            .is_none(),
        "an inaccessible Session must fail before any workspace refresh request"
    );
}

#[tokio::test]
async fn work_result_collaboration_is_window_first_without_creating_session() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let db =
        std::sync::Arc::new(crate::Database::open(&tmp.path().join("collaboration.db")).unwrap());
    let runtime = test_runtime()
        .with_window_activity_database(db.clone())
        .with_communication_database(db.clone());
    let project =
        register_runner_project_at_path(&runtime, "work-result-collab", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let window = crate::client_window::ClientWindow::for_test("operator-card");
    let before =
        present_window_once(&runtime, "work-result-collab", &project, &auth, &window).await;
    assert!(before.success, "{:?}", before.error);
    assert_eq!(
        before.output["work_result"]["collaboration"]["available"],
        true
    );
    assert!(before.output["work_result"]["session_id"].is_null());
    use super::super::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };
    let send = runtime.call_tool_with_invocation_metadata(
        ToolCallRequest { tool_name: "work_result_send_message".into(), arguments: json!({"project":project,"message":"Check tests","delivery_key":"card-1"}) },
        ToolCallContext { transport: ToolTransport::Mcp, session_id: None, auth: Some(&auth), window: Some(&window), record_oauth_scope_denials: false, host_file_import_trust: HostFileImportTrust::Untrusted },
        ToolInvocationMetadata::default(), ToolProtocolCapabilities {work_result_app:true,..Default::default()},
    ).await;
    assert!(send.error_status.is_none(), "{:?}", send.error_status);
    let send = send.result.unwrap();
    assert!(send.output.get("operator_messages").is_none());
    assert!(send.success, "{:?}", send.error);
    let replay = runtime
        .work_result_send_message(
            project.clone(),
            None,
            "Check tests".into(),
            "card-1".into(),
            Some(&auth),
            Some(&window),
        )
        .await;
    assert_eq!(send.output["message_id"], replay.output["message_id"]);
    assert_eq!(replay.output["replayed"], true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let window = window.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "work_result_state".into(),
                        arguments: json!({"project":project}),
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: None,
                        auth: Some(&auth),
                        window: Some(&window),
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    ToolInvocationMetadata::default(),
                    ToolProtocolCapabilities {
                        work_result_app: true,
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "work-result-collab").await;
    complete_agent_request_by_running_locally(&runtime, "work-result-collab", request).await;
    let state = task.await.unwrap();
    assert!(state.error_status.is_none(), "{:?}", state.error_status);
    let state = state.result.unwrap();
    assert!(state.success, "{:?}", state.error);
    assert!(state.output.get("operator_messages").is_none());
    assert_eq!(runtime.sessions.active_session_count_for_test(None), 0);
    assert!(state.output["work_result"]["session_id"].is_null());
    let messages = &state.output["work_result"]["collaboration"]["messages"];
    assert_eq!(messages[0]["message_id"], send.output["message_id"]);
    assert_eq!(messages[0]["source"], "operator");
    assert!(messages[0]["first_projected_at_ms"].is_null());
    let count: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT projection_count FROM window_operator_messages",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn work_result_and_runtime_console_share_window_collaboration_truth() {
    use super::super::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let db = std::sync::Arc::new(
        crate::Database::open(&tmp.path().join("collaboration-truth.db")).unwrap(),
    );
    let runtime = test_runtime()
        .with_window_activity_database(db.clone())
        .with_communication_database(db.clone());
    let project = register_runner_project_at_path(
        &runtime,
        "work-result-collaboration-truth",
        "demo",
        tmp.path(),
    )
    .await;
    let auth = auth_context(None, true);
    let window = crate::client_window::ClientWindow::for_test("collaboration-truth-window");
    let peer = crate::client_window::ClientWindow::for_test("collaboration-truth-peer");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("Context A".into()));
    let now = chrono::Utc::now().timestamp_millis();
    record_work_result_window_session_context(
        &db,
        &auth,
        &window,
        &project,
        &session.session_id,
        now - 2_000,
    );

    let plain = runtime
        .post_window_operator_message(
            window.key(),
            None,
            None,
            "No Session".into(),
            "truth-plain".into(),
            Some(&auth),
        )
        .await;
    assert!(plain.success, "{:?}", plain.error);
    let contextual = runtime
        .post_window_operator_message(
            window.key(),
            Some(&session.session_id),
            None,
            "Context A".into(),
            "truth-context".into(),
            Some(&auth),
        )
        .await;
    assert!(contextual.success, "{:?}", contextual.error);

    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    db.post_peer_message(webcodex_store::NewPeerMessage {
        principal_kind: principal_kind.clone(),
        principal_id: principal_id.clone(),
        sender_window_key: peer.key().to_string(),
        recipient_window_key: window.key().to_string(),
        sender_peer_id: peer.peer_id().to_string(),
        recipient_peer_id: window.peer_id().to_string(),
        kind: "progress".into(),
        priority: "normal".into(),
        message: "Inbound peer".into(),
        tags: Vec::new(),
        requires_ack: false,
        sender_session_id: None,
        sender_project: None,
        created_at_ms: now + 1,
    })
    .unwrap();
    db.post_peer_message(webcodex_store::NewPeerMessage {
        principal_kind,
        principal_id,
        sender_window_key: window.key().to_string(),
        recipient_window_key: peer.key().to_string(),
        sender_peer_id: window.peer_id().to_string(),
        recipient_peer_id: peer.peer_id().to_string(),
        kind: "progress".into(),
        priority: "normal".into(),
        message: "Outbound peer".into(),
        tags: Vec::new(),
        requires_ack: false,
        sender_session_id: Some(session.session_id.clone()),
        sender_project: Some(project.clone()),
        created_at_ms: now + 2,
    })
    .unwrap();

    let before = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
    let contextual_id = contextual.output["message_id"]
        .as_str()
        .unwrap()
        .to_string();
    let contextual_before = before["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["message_id"] == contextual_id)
        .unwrap();
    assert_eq!(contextual_before["context_session_id"], session.session_id);
    assert_eq!(contextual_before["context_project"], project);
    assert!(contextual_before["first_projected_at_ms"].is_null());

    let call = |metadata: ToolInvocationMetadata| {
        runtime.call_tool_with_invocation_metadata(
            ToolCallRequest {
                tool_name: "runtime_status".into(),
                arguments: json!({"compact": true}),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(&auth),
                window: Some(&window),
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
            metadata,
            ToolProtocolCapabilities::default(),
        )
    };
    let projected = call(ToolInvocationMetadata::default()).await;
    assert!(
        projected.error_status.is_none(),
        "{:?}",
        projected.error_status
    );
    let acknowledged = call(ToolInvocationMetadata {
        ack_session_message_ids: vec![contextual_id.clone()],
        ..Default::default()
    })
    .await;
    assert!(
        acknowledged.error_status.is_none(),
        "{:?}",
        acknowledged.error_status
    );

    let console = runtime.window_collaboration(Some(window.key()), Some(&auth), 10);
    let app = refresh_window_work_result_once(
        &runtime,
        "work-result-collaboration-truth",
        &project,
        &auth,
        &window,
    )
    .await;
    assert!(app.success, "{:?}", app.error);
    let app_collaboration = &app.output["work_result"]["collaboration"];
    assert_eq!(console["messages"], app_collaboration["messages"]);
    assert_eq!(console["truncated"], app_collaboration["truncated"]);
    let contextual_after = console["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["message_id"] == contextual_id)
        .unwrap();
    assert!(contextual_after["first_projected_at_ms"].is_number());
    assert!(contextual_after["first_ack_observed_at_ms"].is_number());
    assert_eq!(
        console["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["source"] == "peer" && row["direction"] == "inbound")
            .count(),
        1
    );
    assert_eq!(
        console["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["source"] == "peer" && row["direction"] == "outbound")
            .count(),
        1
    );
}

#[test]
fn work_result_tool_contract_requires_project_and_accepts_optional_session() {
    assert!(
        ToolCall::from_tool_name(
            "present_changes",
            json!({
                "project": "agent:x:y", "session_id": format!("wc_sess_{}", "1".repeat(32))
            })
        )
        .is_err(),
        "the retired presentation must not parse as a compatibility alias"
    );
    for name in ["present_work_result", "work_result_state"] {
        let project_only = ToolCall::from_tool_name(name, json!({"project": "agent:x:y"})).unwrap();
        assert_eq!(project_only.tool_name(), name);
        assert!(ToolCall::from_tool_name(
            name,
            json!({"session_id": format!("wc_sess_{}", "1".repeat(32))})
        )
        .is_err());
        let linked = ToolCall::from_tool_name(
            name,
            json!({
                "project": "agent:x:y",
                "session_id": format!("wc_sess_{}", "1".repeat(32))
            }),
        )
        .unwrap();
        assert_eq!(linked.tool_name(), name);
    }

    for incomplete in [
        json!({
            "project": "agent:x:y",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "delivery_key": "card-send-1"
        }),
        json!({
            "project": "agent:x:y",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "message": "hello"
        }),
    ] {
        assert!(ToolCall::from_tool_name("work_result_send_message", incomplete).is_err());
    }
    let send = ToolCall::from_tool_name(
        "work_result_send_message",
        json!({
            "project": "agent:x:y",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "message": "hello",
            "delivery_key": "card-send-1"
        }),
    )
    .unwrap();
    assert_eq!(send.tool_name(), "work_result_send_message");
}

#[path = "work_result/frozen_changes.rs"]
mod frozen_changes;
