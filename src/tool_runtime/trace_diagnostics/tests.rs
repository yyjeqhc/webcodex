use super::*;
use std::sync::Arc;

fn admin() -> AuthContext {
    let mut auth = AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.role = Some("admin".into());
    auth.is_bootstrap = true;
    auth
}
fn request(value: Value) -> ToolCall {
    ToolCall::from_tool_name("read_tool_trace", value).unwrap()
}

fn fixture() -> (tempfile::TempDir, Arc<crate::Database>, ToolRuntime) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&tmp.path().join("trace-query.db")).unwrap());
    let session: webcodex_store::models::ActionSessionRecord = serde_json::from_value(json!({
        "session_id":"diagnostics-fixture","status":"open","created_at":1,"updated_at":1,
        "total_actions":0,"success_count":0,"failed_count":0,"timeout_or_unknown_count":0,
        "warning_count":0,"total_duration_ms":0,"changed_files_count":0,"job_ids_count":0
    }))
    .unwrap();
    db.insert_action_session(&session).unwrap();
    for (index, window, project, tool, meaningful) in [
        (
            1,
            Some("a".repeat(64)),
            "agent:special:one",
            "read_files",
            true,
        ),
        (2, None, "agent:special:one", "read_files", true),
        (
            3,
            Some("b".repeat(64)),
            "agent:special:other",
            "run_shell",
            true,
        ),
        (
            4,
            Some("a".repeat(64)),
            "agent:special:one",
            "get_work_result_state",
            false,
        ),
    ] {
        let record: webcodex_store::models::ActionEventRecord = serde_json::from_value(json!({
            "event_id":format!("event-{index}"),"session_id":"diagnostics-fixture",
            "started_at":index,"ended_at":index,"duration_ms":10,"endpoint":"/mcp",
            "operation":tool,"action_name":"toolsCall","project":project,"status":"success",
            "changed_files_json":"[]","ids_json":"{}","summary_json":"{}",
            "client_window_key":window,"client_window_source":"openai-session",
            "server_trace_id":format!("00000000-0000-4000-8000-{index:012}"),
            "request_observed_at_ms":index*1000,"response_handed_at_ms":index*1000+10,
            "window_meaningful":meaningful
        }))
        .unwrap();
        db.insert_action_event(&record).unwrap();
    }
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    (tmp, db, runtime)
}

#[tokio::test]
async fn trace_query_filters_pages_and_never_infers_missing_window() {
    let (_tmp, db, runtime) = fixture();
    let first=runtime.read_tool_trace_diagnostic(request(json!({"query":{
        "project":"agent:special:one","tool_name":"read_files","since_ms":0,"until_ms":10_000},"limit":1})),Some(&admin())).await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["calls"][0]["window_key"], Value::Null);
    assert_eq!(first.output["windows"], json!([]));
    assert_eq!(first.output["next_offset"], 1);
    let second = runtime
        .read_tool_trace_diagnostic(
            request(json!({"query":first.output["query"],"limit":1,"offset":1})),
            Some(&admin()),
        )
        .await;
    assert!(second.success);
    assert_eq!(second.output["windows"][0]["window_key"], "a".repeat(64));
    assert_eq!(second.output["next_offset"], Value::Null);
    let escaped = runtime
        .read_tool_trace_diagnostic(
            request(json!({"query":{
        "tool_name":"read_files' OR 1=1 --","since_ms":0,"until_ms":10_000}})),
            Some(&admin()),
        )
        .await;
    assert!(escaped.success);
    assert_eq!(escaped.output["returned_count"], 0);
    assert_eq!(
        db.list_action_events_with_count("diagnostics-fixture", 20)
            .unwrap()
            .0,
        4
    );
}

#[tokio::test]
async fn trace_query_uses_exact_observed_time_not_a_second_coarse_clock() {
    let (_tmp, db, runtime) = fixture();
    let mut row = db
        .list_action_events("diagnostics-fixture", 20)
        .unwrap()
        .into_iter()
        .find(|row| row.event_id == "event-1")
        .unwrap();
    row.event_id = "clock-distinct".into();
    row.server_trace_id = Some("00000000-0000-4000-8000-000000000005".into());
    row.started_at = 999;
    row.request_observed_at_ms = Some(1501);
    db.insert_action_event(&row).unwrap();
    let result = runtime
        .read_tool_trace_diagnostic(
            request(json!({"query":{"since_ms":1501,"until_ms":1501}})),
            Some(&admin()),
        )
        .await;
    assert!(result.success);
    assert_eq!(result.output["returned_count"], 1);
    assert_eq!(result.output["calls"][0]["observed_at_ms"], 1501);
}

#[tokio::test]
async fn diagnostic_authority_and_ambiguous_selectors_fail_before_reading() {
    let runtime = ToolRuntime::new_for_tests();
    let ordinary = crate::auth::shared_key_context("not-admin");
    for auth in [None, Some(&ordinary)] {
        let result = runtime
            .read_tool_trace_diagnostic(request(json!({})), auth)
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "insufficient_scope");
    }
    for args in [
        json!({"trace_ref":"00000000-0000-4000-8000-000000000001","query":{}}),
        json!({"payload_index":0}),
        json!({"query":{"window_key":"not-a-window"}}),
        json!({"query":{"since_ms":5,"until_ms":4}}),
        json!({"query":{"since_ms":0,"until_ms":32*86_400_000_i64}}),
    ] {
        let result = runtime
            .read_tool_trace_diagnostic(request(args), Some(&admin()))
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_trace_request");
    }
}

#[test]
fn bootstrap_not_observed_semantic_state_survives_existing_telemetry_projection() {
    let value =
        super::super::model_ergonomics_telemetry::invocation::BootstrapFacts::from_output(&json!({
            "semantic_navigation":{"status":"not_observed","supported":true,"available":null}
        }));
    assert_eq!(
        serde_json::to_value(value).unwrap()["semantic_status"],
        "not_observed"
    );
}
