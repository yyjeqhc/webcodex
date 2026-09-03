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
        crate::tool_runtime::sessions::session_tool_contract("run_process"),
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

#[tokio::test]
async fn promoted_run_shell_preserves_assertion_identity_in_terminal_validation_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime =
        test_runtime().with_structured_execution_sync_wait(std::time::Duration::from_millis(40));
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "validation-shell-job",
        &auth,
        crate::shell_protocol::ShellClientCapabilities {
            shell: true,
            async_shell_jobs: true,
            ..Default::default()
        },
        vec![registered_project("demo", &tmp.path().to_string_lossy())],
    )
    .await;
    let project = "agent:validation-shell-job:demo".to_string();
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();
    let assertion_name = "promoted shell validation";
    let expected_identity =
        crate::tool_runtime::tool_audit::assertion_validation_identity(assertion_name);
    let (call, recorder_metadata) = ToolCall::from_tool_name_with_recorder_metadata(
        "run_shell",
        json!({
            "project": project,
            "command": "printf validation-shell; sleep 30",
            "session_id": session_id,
            "timeout_secs": 120,
            "cwd": ".",
            "purpose": "test",
            "shell": "bash",
            "assertion_name": assertion_name,
        }),
    )
    .unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth_transport_options_and_metadata(
                    call,
                    Some(&auth),
                    SessionTransport::Mcp,
                    recorder_metadata,
                )
                .await
        }
    });
    let request = wait_for_agent_request_for_client(&runtime, "validation-shell-job").await;
    assert_eq!(request.kind, "start_job");
    let job_id = request.job_id.clone().expect("promoted shell Job id");
    runtime
        .runner_registry
        .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
            client_id: "validation-shell-job".to_string(),
            agent_instance_id: "inst-validation-shell-job".to_string(),
            update_seq: None,
            job_id: job_id.clone(),
            request_id: Some(request.request_id.clone()),
            status: "running".to_string(),
            stdout_chunk: Some("validation-shell\n".to_string()),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["job_id"], job_id);

    runtime
        .runner_registry
        .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
            client_id: "validation-shell-job".to_string(),
            agent_instance_id: "inst-validation-shell-job".to_string(),
            update_seq: None,
            job_id,
            request_id: Some(request.request_id),
            status: "completed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: Some("validation-shell passed\n".to_string()),
            stderr_tail: Some(String::new()),
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(12),
            error: None,
            command_execution_state: Some(
                crate::shell_protocol::ShellCommandExecutionState::Completed,
            ),
            validation_progress: None,
            finished: true,
        })
        .await
        .unwrap();

    let summary = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project,
                session_id,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(summary.success, "{:?}", summary.error);
    let validation = &summary.output["validation"];
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    let latest = &validation["latest"];
    assert_eq!(latest["execution_source"], "run_shell");
    assert_eq!(latest["validation_kind"], "test");
    assert_eq!(latest["identity"], expected_identity);
    assert_eq!(latest["assertion_name"], assertion_name);
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
    assert_eq!(validation["latest"]["assertion_name"], assertion);
    assert!(validation["latest"]["identity"]
        .as_str()
        .is_some_and(
            |identity| identity.starts_with("assertion:") && !identity.contains(assertion)
        ));
}

#[test]
fn unresolved_generic_failure_exposes_recoverable_assertion_label() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:unresolved";
    let session = store.start_session(Some(project.to_string()), None);
    let assertion = "websocket reconnect unresolved";
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "failing-command",
        Some(assertion),
        false,
    );

    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    let unresolved = &validation["unresolved_failures"]["events"][0];
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(
        unresolved["identity"],
        assertion_validation_identity(assertion)
    );
    assert_eq!(unresolved["assertion_name"], assertion);
    assert!(unresolved["identity"]
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
fn validation_without_assertion_omits_recovery_label() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:no-label";
    let session = store.start_session(Some(project.to_string()), None);
    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "legacy-command",
        None,
        false,
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    let event = &validation["unresolved_failures"]["events"][0];
    assert!(event.get("assertion_name").is_none());
    assert!(event["identity"].as_str().is_some_and(|identity| {
        identity.starts_with("command:") || identity.starts_with("target:")
    }));
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
fn generic_assertion_success_cannot_resolve_structured_failure_with_hidden_assertion_metadata() {
    let store = SessionStore::default();
    let project = "agent:validation-identity:structured-isolation";
    let session = store.start_session(Some(project.to_string()), None);
    let assertion = "shared structured label";

    let mut arguments = sample_tool_args("cargo_test");
    arguments["project"] = json!(project);
    // Hidden recorder metadata remains supported for internal evidence fixtures,
    // but it must not turn a structured validation failure into a generic
    // assertion-equivalence member that a later run_process success can resolve.
    arguments["assertion_name"] = json!(assertion);
    let (call, metadata) = ToolCall::from_tool_name_with_recorder_metadata("cargo_test", arguments)
        .expect("hidden cargo_test assertion metadata");
    let start = store.record_tool_call_started_with_metadata(
        Some(&session.session_id),
        SessionTransport::Mcp,
        "cargo_test",
        &call.session_log_arguments(),
        Some(project.to_string()),
        metadata,
        crate::tool_runtime::sessions::session_tool_contract("cargo_test"),
    );
    store.record_tool_call_finished(
        start,
        false,
        &json!({
            "exit_code": 101,
            "execution_state": "completed",
            "tests_detected": true,
            "tests_run_count": 1,
            "tests_passed": 0,
            "tests_failed": 1,
            "zero_tests_run": false,
        }),
        Some("structured test failed"),
        None,
    );

    record_run_process(
        &store,
        &session.session_id,
        project,
        "test",
        "generic-success",
        Some(assertion),
        true,
    );

    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(
        validation["resolved_failures"]["count"], 0,
        "generic validation success must not resolve a structured failure merely because hidden recorder metadata reused the same assertion label: {validation}"
    );
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
