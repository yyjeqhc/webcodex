use super::jobs::register_job_agent_for_auth;
use super::support::shared_key_auth_context;
use crate::action_audit_sessions::{
    record_action_event, ActionAuditEventInput, ActionAuditWorkflowLinkInput,
    WorkflowSessionRelation,
};
use crate::client_window::ClientWindow;
use crate::tool_runtime::ToolRuntime;
use serde_json::json;
use std::sync::Arc;

fn record_event(
    db: &Arc<crate::Database>,
    auth: &crate::auth::AuthContext,
    window: &ClientWindow,
    project: &str,
    session: &str,
    operation: &str,
    at: i64,
    handed: i64,
    meaningful: bool,
) {
    let (kind, id) = crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
    record_action_event(
        db,
        ActionAuditEventInput {
            explicit_session_id: None,
            session_title: None,
            endpoint: "/mcp".into(),
            action_name: "toolsCall".into(),
            operation: Some(operation.into()),
            project: Some(project.into()),
            principal_kind: None,
            principal_user_id: None,
            oauth_client_id: None,
            status: "success".into(),
            http_status: Some(200),
            started_at: at / 1000,
            ended_at: handed / 1000,
            duration_ms: handed - at,
            error_summary: None,
            warning_summary: None,
            changed_files: vec![],
            ids: json!({}),
            summary: json!({"arguments":"SECRET_ARGUMENT","output":"SECRET_OUTPUT"}),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(window.key().into()),
            client_window_source: Some(window.source().into()),
            server_trace_id: Some(format!("trace-{at}")),
            principal_correlation_kind: Some(kind),
            principal_correlation_id: Some(id),
            window_started_at_ms: Some(at),
            window_ended_at_ms: Some(handed),
            request_observed_at_ms: Some(at),
            response_handed_at_ms: Some(handed),
            window_transition_kind: Some(if at == 1000 { "unavailable" } else { "serial" }.into()),
            response_streaming: Some(false),
            window_continuity_eligible: Some(meaningful),
            window_meaningful: meaningful,
            recorder_gap_session_id: None,
            workflow_links: vec![ActionAuditWorkflowLinkInput {
                workflow_session_id: session.into(),
                relation: WorkflowSessionRelation::Recording,
                project: Some(project.into()),
            }],
        },
    );
}

#[tokio::test]
async fn current_window_activity_is_scoped_sanitized_and_reports_observed_timing_only() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("window.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("window-owner");
    let other_auth = shared_key_auth_context("window-other");
    register_job_agent_for_auth(&runtime, "window-runner", "repo", &auth).await;
    let project = "agent:window-runner:repo";
    let window = ClientWindow::for_test("window-current");
    let other_window = ClientWindow::for_test("window-other");
    assert_eq!(
        runtime
            .current_window_activity(None, Some(&auth), None, false)
            .await
            .output["reason_code"],
        "window_identity_unavailable"
    );
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_visible",
        "read_files",
        1000,
        1010,
        true,
    );
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_visible",
        "observe_jobs",
        4010,
        4025,
        true,
    );
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_visible",
        "current_window_activity",
        5000,
        5001,
        false,
    );
    record_event(
        &db,
        &auth,
        &other_window,
        project,
        "wc_sess_visible",
        "read_files",
        6000,
        6010,
        true,
    );
    record_event(
        &db,
        &other_auth,
        &window,
        project,
        "wc_sess_foreign",
        "read_files",
        7000,
        7010,
        true,
    );
    let result = runtime
        .current_window_activity(Some(&window), Some(&auth), Some(20), false)
        .await;
    assert!(result.success);
    assert_eq!(result.output["status"], "available");
    let events = result.output["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["tool_name"], "observe_jobs");
    assert_eq!(events[0]["response_handed_at_ms"], 4025);
    assert_eq!(events[0]["http_status"], 200);
    assert!(events[0]["next_call_gap_ms"].is_null());
    assert_eq!(events[1]["next_call_gap_ms"], 3000);
    assert_eq!(events[1]["service_ms"], 10);
    assert_eq!(
        events[0]["workflow_sessions"][0]["workflow_session_id"],
        "wc_sess_visible"
    );
    assert_eq!(result.output["summary"]["events_scanned"], 3);
    assert_eq!(result.output["summary"]["meaningful_call_count"], 2);
    assert_eq!(result.output["summary"]["observe_jobs_count"], 1);
    assert_eq!(
        result.output["summary"]["observe_jobs_ratio_denominator"],
        2
    );
    assert_eq!(result.output["summary"]["handler_returned_count"], 3);
    assert_eq!(
        result.output["summary"]["max_observed_next_call_gap_ms"],
        3000
    );
    assert_eq!(result.output["summary"]["observed_next_call_gap_count"], 1);
    assert_eq!(result.output["summary"]["gaps_lt_1s"], 0);
    assert_eq!(result.output["summary"]["gaps_lt_2s"], 0);
    assert_eq!(result.output["summary"]["gaps_lt_5s"], 1);
    assert_eq!(result.output["summary"]["gaps_ge_5s"], 0);
    assert_eq!(
        result.output["summary"]["total_positive_observed_next_call_gap_ms"],
        3000
    );
    assert_eq!(result.output["summary"]["total_service_ms"], 26);
    let serialized = serde_json::to_string(&result.output).unwrap();
    for forbidden in [
        "SECRET_ARGUMENT",
        "SECRET_OUTPUT",
        "window-owner",
        "window-other",
        "client_id",
        "runner_instance_id",
        "cwd",
    ] {
        assert!(!serialized.contains(forbidden), "{forbidden}");
    }
    assert!(serialized.len() <= 24 * 1024);
    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    let before_revision = runtime.window_activity.meaningful_revision();
    let self_guard = runtime.window_activity.start_observed(
        &window,
        "self-trace",
        "tools/call",
        Some("current_window_activity"),
        Some((&principal_kind, &principal_id)),
        8000,
    );
    assert_eq!(
        runtime.window_activity.meaningful_revision(),
        before_revision
    );
    let observed = crate::tool_request_trace::scope_active_trace(
        Some("self-trace".into()),
        runtime.current_window_activity(Some(&window), Some(&auth), Some(20), true),
    )
    .await;
    assert!(observed.output["active_requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|request| request["server_trace_id"] != "self-trace"));
    drop(self_guard);
    let schema = crate::tool_runtime::registry::output_schema_for_tool("current_window_activity");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &json!({"success":true,"output":result.output}),
        &schema,
    )
    .unwrap();
    let mut revoked = auth.clone();
    revoked
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_PROJECT_READ);
    let hidden = runtime
        .current_window_activity(Some(&window), Some(&revoked), None, false)
        .await;
    assert_eq!(hidden.output["events"], json!([]));
    revoked
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_RUNTIME_READ);
    assert_eq!(
        runtime
            .current_window_activity(Some(&window), Some(&revoked), None, false)
            .await
            .output["reason_code"],
        "runtime_read_unavailable"
    );
    let mut anonymous = auth.clone();
    anonymous.kind = crate::auth::AuthKind::OpenAnonymous;
    assert_eq!(
        runtime
            .current_window_activity(Some(&window), Some(&anonymous), None, false)
            .await
            .output["reason_code"],
        "principal_identity_unavailable"
    );
}

#[tokio::test]
async fn current_window_activity_clamps_rows_and_has_deterministic_bounded_summary() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("bounded-window.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("bounded-window-owner");
    register_job_agent_for_auth(&runtime, "bounded-window-runner", "repo", &auth).await;
    let project = "agent:bounded-window-runner:repo";
    let window = ClientWindow::for_test("bounded-window");
    for index in 0..210 {
        let at = 1000 + index * 1000;
        record_event(
            &db,
            &auth,
            &window,
            project,
            "wc_sess_bounded",
            "read_files",
            at,
            at + 10,
            true,
        );
    }
    let first = runtime
        .current_window_activity(Some(&window), Some(&auth), Some(200), false)
        .await
        .output;
    let second = runtime
        .current_window_activity(Some(&window), Some(&auth), Some(200), false)
        .await
        .output;
    let returned = first["events"].as_array().unwrap().len();
    assert!(returned > 50);
    assert!(returned <= 200);
    assert_eq!(first["summary"], second["summary"]);
    assert_eq!(first["summary"]["events_scanned"], 210);
    assert_eq!(first["truncated"], true);
    assert!(serde_json::to_vec(&first).unwrap().len() <= 96 * 1024);
    let mut revoked = auth.clone();
    revoked
        .scopes
        .retain(|scope| scope != crate::auth::SCOPE_PROJECT_READ);
    let hidden = runtime
        .current_window_activity(Some(&window), Some(&revoked), Some(50), true)
        .await
        .output;
    assert!(hidden["events"].as_array().unwrap().is_empty());
    assert_eq!(hidden["summary"]["events_scanned"], 0);
    assert_eq!(hidden["truncated"], false);
}

#[tokio::test]
async fn current_window_activity_gap_buckets_use_exact_observed_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("gap-buckets.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("gap-buckets-owner");
    register_job_agent_for_auth(&runtime, "gap-buckets-runner", "repo", &auth).await;
    let project = "agent:gap-buckets-runner:repo";
    let window = ClientWindow::for_test("gap-buckets-window");
    let gaps = [
        999_i64, 1000, 1999, 2000, 4999, 5000, 9999, 10_000, 30_000, 120_000,
    ];

    let mut at = 1000_i64;
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_gap_buckets",
        "read_files",
        at,
        at + 10,
        true,
    );
    for gap in gaps {
        at += 10 + gap;
        record_event(
            &db,
            &auth,
            &window,
            project,
            "wc_sess_gap_buckets",
            "read_files",
            at,
            at + 10,
            true,
        );
    }

    let output = runtime
        .current_window_activity(Some(&window), Some(&auth), Some(50), false)
        .await
        .output;
    let summary = &output["summary"];
    assert_eq!(summary["observed_next_call_gap_count"], 10);
    assert_eq!(summary["gaps_lt_1s"], 1);
    assert_eq!(summary["gaps_lt_2s"], 3);
    assert_eq!(summary["gaps_lt_5s"], 5);
    assert_eq!(summary["gaps_ge_5s"], 5);
    assert_eq!(summary["gaps_ge_10s"], 3);
    assert_eq!(summary["gaps_ge_30s"], 2);
    assert_eq!(summary["gaps_ge_120s"], 1);
    assert_eq!(
        summary["total_positive_observed_next_call_gap_ms"],
        gaps.into_iter().sum::<i64>()
    );
    assert_eq!(summary["total_service_ms"], 110);
    assert_eq!(summary["max_service_ms"], 10);
    assert_eq!(summary["max_observed_next_call_gap_ms"], 120_000);
}

#[tokio::test]
async fn current_window_activity_never_infers_overlap_from_a_short_gap() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("overlap-facts.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("overlap-facts-owner");
    register_job_agent_for_auth(&runtime, "overlap-facts-runner", "repo", &auth).await;
    let project = "agent:overlap-facts-runner:repo";
    let window = ClientWindow::for_test("overlap-facts-window");

    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_overlap_facts",
        "git_status",
        1000,
        1010,
        true,
    );
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_overlap_facts",
        "runtime_status",
        1110,
        1120,
        true,
    );
    db.conn_for_tests()
        .execute(
            "UPDATE action_events SET window_transition_kind = 'overlap' WHERE server_trace_id = 'trace-1110'",
            [],
        )
        .unwrap();

    let output = runtime
        .current_window_activity(Some(&window), Some(&auth), None, false)
        .await
        .output;
    let summary = &output["summary"];
    assert_eq!(summary["overlapping_call_count"], 1);
    assert_eq!(summary["serial_call_count"], 0);
    assert_eq!(summary["observed_next_call_gap_count"], 0);
    assert_eq!(summary["gaps_lt_1s"], 0);
    assert!(output["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["next_call_gap_ms"].is_null()));
}

#[tokio::test]
async fn current_window_activity_preserves_streaming_handoff_without_service_completion() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("streaming-window.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("streaming-window-owner");
    register_job_agent_for_auth(&runtime, "streaming-window-runner", "repo", &auth).await;
    let project = "agent:streaming-window-runner:repo";
    let window = ClientWindow::for_test("streaming-window");
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_stream",
        "read_files",
        1000,
        1050,
        true,
    );
    db.conn_for_tests()
        .execute(
            "UPDATE action_events SET response_streaming = 1 WHERE server_trace_id = 'trace-1000'",
            [],
        )
        .unwrap();
    let output = runtime
        .current_window_activity(Some(&window), Some(&auth), None, false)
        .await
        .output;
    let event = &output["events"][0];
    assert_eq!(event["response_streaming"], true);
    assert_eq!(event["response_handed_at_ms"], 1050);
    assert!(event.get("service_ms").is_none());
    assert!(event["next_call_gap_ms"].is_null());
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn current_window_activity_malformed_persisted_composition_fails_safe() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("malformed-window.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let auth = shared_key_auth_context("malformed-window-owner");
    register_job_agent_for_auth(&runtime, "malformed-window-runner", "repo", &auth).await;
    let project = "agent:malformed-window-runner:repo";
    let window = ClientWindow::for_test("malformed-window");
    record_event(
        &db,
        &auth,
        &window,
        project,
        "wc_sess_malformed",
        "read_files",
        1000,
        1010,
        true,
    );
    db.conn_for_tests().execute(
        "UPDATE action_events SET summary_json = '{malformed' WHERE server_trace_id = 'trace-1000'",
        [],
    ).unwrap();
    let output = runtime
        .current_window_activity(Some(&window), Some(&auth), None, false)
        .await
        .output;
    assert_eq!(output["status"], "available");
    assert_eq!(output["events"].as_array().unwrap().len(), 1);
    assert!(output["events"][0].get("code_mode_composition").is_none());
    assert_eq!(
        output["summary"]["returned_nested_code_mode_child_count"],
        0
    );
}
