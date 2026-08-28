use super::support::*;
use crate::tool_runtime::sessions::{SessionStore, SessionTransport};
use crate::tool_runtime::tool_audit::{
    assertion_validation_identity, session_log_arguments_for_tool_request,
};
use crate::tool_runtime::validation_events::validation_summary_for_session;
use crate::tool_runtime::ToolCall;
use serde_json::{json, Value};

fn run_process_request(
    project: &str,
    purpose: &str,
    command_variant: &str,
    assertion_name: Option<&str>,
) -> Value {
    let mut request = json!({
        "project": project,
        "executable": "validation-harness",
        "args": ["--case", command_variant],
        "cwd": ".",
        "purpose": purpose,
    });
    if let Some(assertion_name) = assertion_name {
        request["assertion_name"] = json!(assertion_name);
    }
    request
}

fn record_run_process(
    store: &SessionStore,
    session_id: &str,
    project: &str,
    purpose: &str,
    command_variant: &str,
    assertion_name: Option<&str>,
    success: bool,
) {
    let request = run_process_request(project, purpose, command_variant, assertion_name);
    let (call, metadata) = ToolCall::from_tool_name_with_recorder_metadata("run_process", request)
        .expect("model-facing run_process input");
    let ledger_arguments = call.session_log_arguments();
    let start = store.record_tool_call_started_with_metadata(
        Some(session_id),
        SessionTransport::Mcp,
        "run_process",
        &ledger_arguments,
        Some(project.to_string()),
        metadata,
    );
    let output = json!({
        "exit_code": if success { 0 } else { 1 },
        "purpose": purpose,
        "execution_state": "completed",
        "stdout_tail": if success { "validation passed\n" } else { "validation failed\n" },
        "stderr_tail": "",
        "stdout_truncated": false,
        "stderr_truncated": false,
    });
    store.record_tool_call_finished(
        start,
        success,
        &output,
        (!success).then_some("validation failed"),
        None,
    );
}

#[test]
fn model_facing_generic_execution_accepts_bounded_assertion_name_and_rejects_malformed_values() {
    for tool_name in ["run_process", "run_script", "run_shell", "run_job"] {
        let mut arguments = sample_tool_args(tool_name);
        arguments["assertion_name"] = json!("websocket reconnect regression");
        let (_, metadata) = ToolCall::from_tool_name_with_recorder_metadata(tool_name, arguments)
            .unwrap_or_else(|error| panic!("{tool_name}: {error}"));
        assert_eq!(
            metadata.expectation.assertion_name.as_deref(),
            Some("websocket reconnect regression"),
            "{tool_name}"
        );
    }

    for invalid in [
        json!(""),
        json!("   "),
        json!("x".repeat(121)),
        json!("line\nbreak"),
        json!("Bearer supersecret"),
    ] {
        let mut arguments = sample_tool_args("run_process");
        arguments["assertion_name"] = invalid;
        let error = ToolCall::from_tool_name_with_recorder_metadata("run_process", arguments)
            .expect_err("invalid assertion_name must fail closed");
        assert!(error.contains("assertion_name"), "{error}");
    }
}

#[test]
fn same_model_assertion_resolves_changed_generic_command_without_rewriting_raw_history() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:demo";
    let session = store.start_session(Some(project.to_string()), None);
    let assertion = "websocket reconnect regression";
    let before = run_process_request(project, "test", "before-fix", Some(assertion));
    let after = run_process_request(project, "test", "after-fix", Some(assertion));
    assert_ne!(
        session_log_arguments_for_tool_request("run_process", &before)["execution_identity"],
        session_log_arguments_for_tool_request("run_process", &after)["execution_identity"],
        "fixture commands must differ under the legacy command-derived identity"
    );

    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "before-fix",
        Some(assertion),
        false,
    );
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "after-fix",
        Some(assertion),
        true,
    );

    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    let raw_finished = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished" && event.tool_name == "run_process")
        .collect::<Vec<_>>();
    assert_eq!(raw_finished.len(), 2);
    assert_eq!(raw_finished[0].status.as_deref(), Some("failed"));
    assert_eq!(raw_finished[1].status.as_deref(), Some("succeeded"));

    let validation = validation_summary_for_session(&summary);
    let expected_identity = assertion_validation_identity(assertion);
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], true);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["latest"]["identity"], expected_identity);
    assert!(validation["latest"]["identity"]
        .as_str()
        .is_some_and(
            |identity| identity.starts_with("assertion:") && !identity.contains(assertion)
        ));
}

#[test]
fn changed_generic_command_without_assertion_keeps_legacy_identity_isolation() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:compat";
    let session = store.start_session(Some(project.to_string()), None);
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "before-fix",
        None,
        false,
    );
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "after-fix",
        None,
        true,
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
}

#[test]
fn different_assertions_do_not_cross_resolve_in_same_project() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:isolation";
    let session = store.start_session(Some(project.to_string()), None);
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "same-command",
        Some("assertion A"),
        false,
    );
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "same-command",
        Some("assertion B"),
        true,
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
}

#[test]
fn same_assertion_never_cross_resolves_between_projects() {
    let store = SessionStore::default();
    let session = store.start_session(None, None);
    let assertion = "shared label";
    record_run_process(
        &store,
        &session.session_id,
        "agent:validation-identity:alpha",
        "test",
        "same-command",
        Some(assertion),
        false,
    );
    record_run_process(
        &store,
        &session.session_id,
        "agent:validation-identity:beta",
        "test",
        "same-command",
        Some(assertion),
        true,
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
}

#[test]
fn assertion_name_is_inert_for_non_validation_execution() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:diagnostic";
    let session = store.start_session(Some(project.to_string()), None);
    record_run_process(
        &store,
        &session.session_id,
        project,
        "diagnostic",
        "diagnostic-command",
        Some("must stay inert"),
        true,
    );
    let summary = store.summary(&session.session_id, Some(20)).unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .expect("raw execution fact");
    assert_eq!(finished.status.as_deref(), Some("succeeded"));
    assert_eq!(finished.assertion_name.as_deref(), Some("must stay inert"));
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["available"], false);
    assert_eq!(validation["events_total"], 0);
}
