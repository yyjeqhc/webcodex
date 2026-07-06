//! Tests for `session_handoff_summary` — read-only structured handoff tool.

use super::super::*;
use super::support::*;
use crate::auth::AuthContext;
use crate::shell_protocol::ShellClientCapabilities;
use crate::tool_runtime::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use crate::tool_runtime::sessions::SessionTransport;
use crate::tool_runtime::validation_events::validation_summary_for_session;
use crate::tool_runtime::validation_parser::VALIDATION_OUTPUT_METADATA_ABSENT_REASON;
use serde_json::{json, Value};

// =========================================================================
// 1. Known / spec / metadata / manifest consistency
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_is_known_and_in_specs() {
    assert!(is_known_tool_name("session_handoff_summary"));
    let specs = registered_tool_specs();
    assert!(
        specs.iter().any(|s| s.name == "session_handoff_summary"),
        "session_handoff_summary must appear in tool_specs"
    );
    assert!(
        specs.iter().all(|spec| is_known_tool_name(&spec.name)),
        "tool_specs must remain a subset of known parser names"
    );
    assert!(
        crate::tool_runtime::metadata::lookup_tool_metadata("session_handoff_summary").is_some()
    );
    // tool_manifest session category must include the new tool.
    let runtime = test_runtime();
    let manifest = runtime
        .dispatch(ToolCall::ToolManifest {
            category: Some("session".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(manifest.success, "{:?}", manifest.error);
    let tools = manifest.output["tools"]
        .as_array()
        .expect("manifest tools array");
    assert!(
        tools.iter().any(|t| t["name"] == "session_handoff_summary"),
        "session category must include session_handoff_summary: {:?}",
        tools
    );
}

// =========================================================================
// 2. Message board state
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_returns_message_board_state() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("handoff board".to_string()));
    let sid = session.session_id.clone();

    post_session_message(&runtime, &sid, "todo", "implement handoff tests");
    post_session_message(&runtime, &sid, "risk", "scope creep");
    post_session_message(&runtime, &sid, "question", "which scope?");
    post_session_message(&runtime, &sid, "guidance", "keep read-only");
    post_session_message(&runtime, &sid, "progress", "wired handoff dispatch");
    post_session_message(&runtime, &sid, "decision", "no LLM summarization");

    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid.clone(),
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_id"], sid);
    assert_eq!(result.output["title"], "handoff board");
    assert_eq!(result.output["mode"], "normal");

    // Counts
    assert_eq!(result.output["counts"]["messages"], 6);
    assert_eq!(result.output["counts"]["open_todos"], 1);
    assert_eq!(result.output["counts"]["open_risks"], 1);
    assert_eq!(result.output["counts"]["open_questions"], 1);
    assert_eq!(result.output["counts"]["open_guidance"], 1);

    // Open items
    assert_eq!(result.output["open_todos"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["open_risks"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["open_questions"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["open_guidance"].as_array().unwrap().len(), 1);

    // Recent progress / decisions
    assert_eq!(
        result.output["recent_progress"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        result.output["recent_decisions"].as_array().unwrap().len(),
        1
    );

    // No workspace or checkpoints when project is absent.
    assert!(result.output.get("workspace").is_none());
    assert!(result.output.get("checkpoints").is_none());

    // suggested_next_actions should be present and bounded.
    let actions = result.output["suggested_next_actions"]
        .as_array()
        .expect("suggested_next_actions array");
    assert!(!actions.is_empty());
}

// =========================================================================
// 3. Recent failed tool calls
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_includes_recent_failed_tools() {
    let runtime = runtime_with_agent_project("handoff-fail");
    register_agent(
        &runtime,
        "handoff-fail",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("handoff-fail");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("failed calls".to_string()));
    let sid = session.session_id.clone();

    // Dispatch a read_file that will fail (agent file_read succeeds but path
    // validation / response handling makes it a failed tool call).
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let sid = sid.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "definitely_does_not_exist.md".to_string(),
                        session_id: Some(sid),
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "handoff-fail", "inst")
        .await
        .expect("read_file should enqueue an agent request");
    // Return an error to simulate a failed read.
    complete_patch_agent_request(
        &runtime,
        "handoff-fail",
        &req.request_id,
        1,
        "",
        "file not found",
    )
    .await;
    let read_result = task.await.unwrap();
    assert!(!read_result.success, "read_file should have failed");

    // Now call handoff.
    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid,
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    let failed = result.output["recent_failed_tools"]
        .as_array()
        .expect("recent_failed_tools array");
    assert!(
        !failed.is_empty(),
        "should include at least one failed tool"
    );
    assert_eq!(failed[0]["tool_name"], "read_file");
    // Must not leak raw sensitive input.
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(
        !serialized.contains("definitely_does_not_exist.md"),
        "raw input path must not leak: {serialized}"
    );
}

#[tokio::test]
async fn expected_stop_job_failures_are_classified_without_permission_noise() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "expected-stop",
        &auth,
        ShellClientCapabilities::default(),
        vec![
            registered_project("alpha", "/tmp/expected-stop-alpha"),
            registered_project("beta", "/tmp/expected-stop-beta"),
        ],
    )
    .await;
    let session_project = "agent:expected-stop:alpha".to_string();
    let request_project = "agent:expected-stop:beta".to_string();
    let session = runtime.sessions.start_session(
        Some(session_project),
        Some("expected stop failures".to_string()),
    );
    let sid = session.session_id.clone();

    let wrong_project = call_recorded_tool(
        &runtime,
        &sid,
        "stop_job",
        json!({
            "project": request_project,
            "job_id": "job-negative",
            "confirm": true,
            "expected_failure": true,
            "expected_failure_kind": "session_project_mismatch",
            "assertion_name": "wrong-project stop_job negative path"
        }),
        Some(&auth),
    )
    .await;
    assert!(!wrong_project.success);
    assert_eq!(
        wrong_project.output["failure_kind"],
        "session_project_mismatch"
    );
    assert_eq!(wrong_project.output["command_started"], false);
    assert!(wrong_project.output.get("permission").is_none());

    let confirm_required = call_recorded_tool(
        &runtime,
        &sid,
        "stop_job",
        json!({
            "project": "agent:expected-stop:alpha",
            "job_id": "job-negative",
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "confirmation_required",
            "assertion_name": "stop_job requires confirm=true"
        }),
        Some(&auth),
    )
    .await;
    assert!(!confirm_required.success);
    assert_eq!(
        confirm_required.output["failure_kind"],
        "confirmation_required"
    );
    assert_eq!(confirm_required.output["command_started"], false);
    assert!(confirm_required.output.get("permission").is_none());

    let summary = runtime.sessions.summary(&sid, Some(20)).unwrap();
    let finished: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished" && event.tool_name == "stop_job")
        .collect();
    assert_eq!(finished.len(), 2);
    assert!(finished
        .iter()
        .all(|event| event.expected_failure == Some(true)));
    assert!(finished.iter().all(
        |event| event.failure_expectation_result.as_deref() == Some("matched_expected_failure")
    ));

    let handoff = handoff_summary(&runtime, &sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 2);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(
        handoff.output["tool_failures"]["expectation_mismatch_count"],
        0
    );
    assert_eq!(
        handoff.output["tool_failures"]["unexpected_success_count"],
        0
    );
    assert_eq!(
        handoff.output["expected_failed_tool_calls"][0]["assertion_name"],
        "stop_job requires confirm=true"
    );
    let actions = handoff.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|action| action == "expected failure assertions matched"));
    assert!(!actions.iter().any(|action| action
        .as_str()
        .unwrap_or("")
        .contains("review unexpected failed tool calls")));
    assert!(!actions.iter().any(|action| action
        .as_str()
        .unwrap_or("")
        .contains("review recent failed tool calls")));
}

#[tokio::test]
async fn unexpected_failure_remains_actionable_in_handoff() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("unexpected failure".to_string()));
    let sid = session.session_id.clone();

    let result = call_recorded_tool(
        &runtime,
        &sid,
        "job_status",
        json!({"job_id": "missing-job"}),
        None,
    )
    .await;
    assert!(!result.success);

    let handoff = handoff_summary(&runtime, &sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 1);
    assert_eq!(
        handoff.output["unexpected_failed_tool_calls"][0]["tool_name"],
        "job_status"
    );
    let actions = handoff.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions.iter().any(|action| {
        action.as_str().unwrap_or("") == "review unexpected failed tool calls before proceeding"
    }));
}

#[tokio::test]
async fn expectation_mismatch_and_unexpected_success_are_visible() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "expect-mismatch",
        &auth,
        ShellClientCapabilities::default(),
        vec![registered_project("demo", "/tmp/expect-mismatch-demo")],
    )
    .await;
    let project = "agent:expect-mismatch:demo".to_string();

    let mismatch_session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("mismatch".to_string()));
    let mismatch_sid = mismatch_session.session_id.clone();
    let mismatch = call_recorded_tool(
        &runtime,
        &mismatch_sid,
        "stop_job",
        json!({
            "project": project,
            "job_id": "job-negative",
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "session_project_mismatch",
            "assertion_name": "wrong expected failure kind"
        }),
        Some(&auth),
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["failure_kind"], "confirmation_required");
    let handoff = handoff_summary(&runtime, &mismatch_sid).await;
    assert_eq!(
        handoff.output["tool_failures"]["expectation_mismatch_count"],
        1
    );
    assert_eq!(
        handoff.output["expectation_mismatches"][0]["actual_failure_kind"],
        "confirmation_required"
    );
    assert!(handoff.output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str().unwrap_or("")
            == "review expected failure mismatches before proceeding"));

    let success_session = runtime
        .sessions
        .start_session(None, Some("unexpected success".to_string()));
    let success_sid = success_session.session_id.clone();
    let success = call_recorded_tool(
        &runtime,
        &success_sid,
        "list_projects",
        json!({
            "expected_failure": true,
            "assertion_name": "list_projects should not fail"
        }),
        None,
    )
    .await;
    assert!(success.success, "{:?}", success.error);
    let handoff = handoff_summary(&runtime, &success_sid).await;
    assert_eq!(
        handoff.output["tool_failures"]["unexpected_success_count"],
        1
    );
    assert_eq!(
        handoff.output["unexpected_success_tool_calls"][0]["tool_name"],
        "list_projects"
    );
    assert!(handoff.output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str().unwrap_or("")
            == "review expected-failure assertions that unexpectedly succeeded"));
}

#[tokio::test]
async fn direct_typed_dispatch_preserves_failure_expectation_metadata() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "direct-expected-stop",
        &auth,
        ShellClientCapabilities::default(),
        vec![
            registered_project("alpha", "/tmp/direct-expected-stop-alpha"),
            registered_project("beta", "/tmp/direct-expected-stop-beta"),
        ],
    )
    .await;
    let session_project = "agent:direct-expected-stop:alpha".to_string();
    let request_project = "agent:direct-expected-stop:beta".to_string();
    let session = runtime
        .sessions
        .start_session(Some(session_project), Some("direct expected".to_string()));
    let sid = session.session_id.clone();

    let wrong_project = call_typed_tool_with_metadata(
        &runtime,
        "stop_job",
        json!({
            "project": request_project,
            "job_id": "job-negative",
            "session_id": &sid,
            "confirm": true,
            "expected_failure": true,
            "expected_failure_kind": "session_project_mismatch",
            "assertion_name": "direct wrong-project stop_job"
        }),
        Some(&auth),
    )
    .await;
    assert!(!wrong_project.success);
    assert_eq!(
        wrong_project.output["failure_kind"],
        "session_project_mismatch"
    );
    assert_eq!(wrong_project.output["command_started"], false);
    assert!(wrong_project.output.get("permission").is_none());

    let confirm_required = call_typed_tool_with_metadata(
        &runtime,
        "stop_job",
        json!({
            "project": "agent:direct-expected-stop:alpha",
            "job_id": "job-negative",
            "session_id": &sid,
            "confirm": false,
            "expected_failure": true,
            "test_expect_failure_kind": "confirmation_required",
            "assertion_name": "direct stop_job confirm gate"
        }),
        Some(&auth),
    )
    .await;
    assert!(!confirm_required.success);
    assert_eq!(
        confirm_required.output["failure_kind"],
        "confirmation_required"
    );
    assert_eq!(confirm_required.output["command_started"], false);
    assert!(confirm_required.output.get("permission").is_none());

    let summary = runtime.sessions.summary(&sid, Some(20)).unwrap();
    let started: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_started" && event.tool_name == "stop_job")
        .collect();
    assert_eq!(started.len(), 2);
    assert!(started
        .iter()
        .all(|event| event.expected_failure == Some(true)));
    assert!(started
        .iter()
        .any(|event| event.assertion_name.as_deref() == Some("direct wrong-project stop_job")));
    assert!(started
        .iter()
        .any(|event| event.assertion_name.as_deref() == Some("direct stop_job confirm gate")));

    let finished: Vec<_> = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished" && event.tool_name == "stop_job")
        .collect();
    assert_eq!(finished.len(), 2);
    assert!(finished
        .iter()
        .all(|event| event.expected_failure == Some(true)));
    assert!(finished.iter().all(
        |event| event.failure_expectation_result.as_deref() == Some("matched_expected_failure")
    ));
    assert!(finished.iter().any(|event| {
        event.expected_failure_kind.as_deref() == Some("session_project_mismatch")
            && event.actual_failure_kind.as_deref() == Some("session_project_mismatch")
    }));
    assert!(finished.iter().any(|event| {
        event.expected_failure_kind.as_deref() == Some("confirmation_required")
            && event.actual_failure_kind.as_deref() == Some("confirmation_required")
    }));

    let handoff = handoff_summary_only(&runtime, &sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 2);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(
        handoff.output["tool_failures"]["expectation_mismatch_count"],
        0
    );
    assert_eq!(
        handoff.output["tool_failures"]["unexpected_success_count"],
        0
    );
    let actions = handoff.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|action| action == "expected failure assertions matched"));
    assert!(!actions.iter().any(|action| action
        .as_str()
        .unwrap_or("")
        .contains("review unexpected failed tool calls")));
    assert_ne!(handoff.output["verdict"]["status"], "fail");
    assert_reason_list_contains(
        &handoff.output["verdict"],
        "warning_reasons",
        "expected_failures_matched",
    );

    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "direct-mixed", "demo", tmp.path()).await;
    let auth = bootstrap_auth_context();
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("direct mixed".to_string()));
    let sid = session.session_id.clone();

    let mismatch = call_typed_tool_with_metadata(
        &runtime,
        "stop_job",
        json!({
            "project": &project,
            "job_id": "job-negative",
            "session_id": &sid,
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "session_project_mismatch",
            "assertion_name": "direct wrong expected kind"
        }),
        Some(&auth),
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["failure_kind"], "confirmation_required");

    let unexpected_success = call_typed_tool_with_metadata_and_agent(
        &runtime,
        "direct-mixed",
        "show_changes",
        json!({
            "project": &project,
            "session_id": &sid,
            "include_diff": false,
            "expected_failure": true,
            "assertion_name": "direct show_changes expected failure"
        }),
        Some(auth.clone()),
    )
    .await;
    assert!(unexpected_success.success, "{:?}", unexpected_success.error);

    let unexpected = call_typed_tool_with_metadata(
        &runtime,
        "stop_job",
        json!({
            "project": &project,
            "job_id": "job-missing",
            "session_id": &sid,
            "confirm": true
        }),
        Some(&auth),
    )
    .await;
    assert!(!unexpected.success);
    assert_eq!(unexpected.output["failure_kind"], "job_not_found");

    let summary = runtime.sessions.summary(&sid, Some(30)).unwrap();
    let mismatch_event = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("direct wrong expected kind")
        })
        .expect("mismatch finished event");
    assert_eq!(
        mismatch_event.actual_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        mismatch_event.failure_expectation_result.as_deref(),
        Some("expectation_mismatch")
    );
    let success_event = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("direct show_changes expected failure")
        })
        .expect("unexpected success finished event");
    assert_eq!(success_event.status.as_deref(), Some("succeeded"));
    assert_eq!(
        success_event.failure_expectation_result.as_deref(),
        Some("unexpected_success")
    );

    let handoff = handoff_summary_only(&runtime, &sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 0);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 1);
    assert_eq!(
        handoff.output["tool_failures"]["expectation_mismatch_count"],
        1
    );
    assert_eq!(
        handoff.output["tool_failures"]["unexpected_success_count"],
        1
    );
    let actions = handoff.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions.iter().any(|action| {
        action.as_str().unwrap_or("") == "review unexpected failed tool calls before proceeding"
    }));
    assert!(actions.iter().any(|action| {
        action.as_str().unwrap_or("") == "review expected failure mismatches before proceeding"
    }));
    assert!(actions.iter().any(|action| {
        action.as_str().unwrap_or("")
            == "review expected-failure assertions that unexpectedly succeeded"
    }));
    assert_eq!(handoff.output["verdict"]["status"], "fail");
    assert_reason_list_contains(
        &handoff.output["verdict"],
        "blocking_reasons",
        "unexpected_tool_failures",
    );
    assert_reason_list_contains(
        &handoff.output["verdict"],
        "blocking_reasons",
        "expectation_mismatches",
    );
    assert_reason_list_contains(
        &handoff.output["verdict"],
        "blocking_reasons",
        "unexpected_successes",
    );
}

#[tokio::test]
async fn generic_call_runtime_tool_preserves_flattened_failure_expectations() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "generic-expect", "demo", tmp.path()).await;
    let auth = bootstrap_auth_context();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("generic expectations".to_string()),
    );
    let sid = session.session_id.clone();

    let matched = call_kernel_tool(
        &runtime,
        "stop_job",
        json!({
            "project": &project,
            "job_id": "job-negative",
            "session_id": &sid,
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "confirmation_required",
            "assertion_name": "generic matched confirmation"
        }),
        None,
        Some(&auth),
    )
    .await;
    assert!(!matched.success);
    assert_eq!(matched.output["failure_kind"], "confirmation_required");

    let mismatch = call_kernel_tool(
        &runtime,
        "stop_job",
        json!({
            "project": &project,
            "job_id": "job-negative",
            "session_id": &sid,
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "session_project_mismatch",
            "assertion_name": "generic mismatch confirmation"
        }),
        None,
        Some(&auth),
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["failure_kind"], "confirmation_required");

    let unexpected_success = call_kernel_tool_with_agent(
        &runtime,
        "generic-expect",
        "show_changes",
        json!({
            "project": &project,
            "session_id": &sid,
            "include_diff": false,
            "expected_failure": true,
            "assertion_name": "generic show_changes expected failure"
        }),
        None,
        Some(auth.clone()),
    )
    .await;
    assert!(unexpected_success.success, "{:?}", unexpected_success.error);

    let summary = runtime.sessions.summary(&sid, Some(30)).unwrap();
    let matched_started = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_started"
                && event.assertion_name.as_deref() == Some("generic matched confirmation")
        })
        .expect("matched started event");
    assert_eq!(matched_started.expected_failure, Some(true));
    let matched_finished = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("generic matched confirmation")
        })
        .expect("matched finished event");
    assert_eq!(
        matched_finished.expected_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        matched_finished.actual_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        matched_finished.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );
    let mismatch_finished = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("generic mismatch confirmation")
        })
        .expect("mismatch finished event");
    assert_eq!(
        mismatch_finished.actual_failure_kind.as_deref(),
        Some("confirmation_required")
    );
    assert_eq!(
        mismatch_finished.failure_expectation_result.as_deref(),
        Some("expectation_mismatch")
    );
    let success_finished = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("generic show_changes expected failure")
        })
        .expect("unexpected success finished event");
    assert_eq!(success_finished.status.as_deref(), Some("succeeded"));
    assert_eq!(
        success_finished.failure_expectation_result.as_deref(),
        Some("unexpected_success")
    );

    let handoff = handoff_summary_only(&runtime, &sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 1);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(
        handoff.output["tool_failures"]["expectation_mismatch_count"],
        1
    );
    assert_eq!(
        handoff.output["tool_failures"]["unexpected_success_count"],
        1
    );

    let finish = finish_coding_task_summary_only_no_hygiene(
        &runtime,
        "generic-expect",
        project.clone(),
        &sid,
    )
    .await;
    assert!(finish.success, "{:?}", finish.error);
    assert_eq!(finish.output["tool_failures"]["expected_count"], 1);
    assert_eq!(finish.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(
        finish.output["tool_failures"]["expectation_mismatch_count"],
        1
    );
    assert_eq!(
        finish.output["tool_failures"]["unexpected_success_count"],
        1
    );
}

#[tokio::test]
async fn generic_call_runtime_tool_recording_session_preserves_failure_expectations() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "generic-recording", "demo", tmp.path()).await;
    let auth = bootstrap_auth_context();
    let business_session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("business session".to_string()));
    let tracking_session = runtime
        .sessions
        .start_session(None, Some("tracking session".to_string()));
    let business_sid = business_session.session_id.clone();
    let tracking_sid = tracking_session.session_id.clone();

    let result = call_kernel_tool(
        &runtime,
        "stop_job",
        json!({
            "project": &project,
            "job_id": "job-negative",
            "session_id": &business_sid,
            "confirm": false,
            "expected_failure": true,
            "expected_failure_kind": "confirmation_required",
            "assertion_name": "recording wrapper confirmation"
        }),
        Some(&tracking_sid),
        Some(&auth),
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "confirmation_required");
    assert_eq!(result.output["command_started"], false);
    assert!(result.output.get("permission").is_none());

    for sid in [&tracking_sid, &business_sid] {
        let summary = runtime.sessions.summary(sid, Some(20)).unwrap();
        let event = summary
            .events
            .iter()
            .find(|event| {
                event.kind == "tool_call_finished"
                    && event.assertion_name.as_deref() == Some("recording wrapper confirmation")
            })
            .expect("finished event");
        assert_eq!(event.expected_failure, Some(true));
        assert_eq!(
            event.expected_failure_kind.as_deref(),
            Some("confirmation_required")
        );
        assert_eq!(
            event.actual_failure_kind.as_deref(),
            Some("confirmation_required")
        );
        assert_eq!(
            event.failure_expectation_result.as_deref(),
            Some("matched_expected_failure")
        );
    }

    let handoff = handoff_summary_only(&runtime, &business_sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 1);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 0);
}

#[tokio::test]
async fn early_failure_paths_preserve_failure_expectation_metadata() {
    let runtime = test_runtime();
    let invalid_session = runtime
        .sessions
        .start_session(None, Some("invalid arguments expected".to_string()));
    let invalid_sid = invalid_session.session_id.clone();

    let invalid = call_kernel_tool(
        &runtime,
        "read_file",
        json!({
            "project": "demo",
            "session_id": &invalid_sid,
            "expected_failure": true,
            "expected_failure_kind": "invalid_arguments",
            "assertion_name": "missing read_file path"
        }),
        Some(&invalid_sid),
        None,
    )
    .await;
    assert!(!invalid.success);

    let summary = runtime.sessions.summary(&invalid_sid, Some(20)).unwrap();
    let event = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("missing read_file path")
        })
        .expect("invalid arguments finished event");
    assert_eq!(event.expected_failure, Some(true));
    assert_eq!(
        event.actual_failure_kind.as_deref(),
        Some("invalid_arguments")
    );
    assert_eq!(
        event.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );

    let guard_session = runtime.sessions.start_session_with_guards(
        None,
        Some("guard expected".to_string()),
        SessionMode::ReadOnly,
        crate::tool_runtime::sessions::SessionGuards::default(),
    );
    let guard_sid = guard_session.session_id.clone();
    let guard = call_typed_tool_with_metadata(
        &runtime,
        "stop_job",
        json!({
            "project": "demo",
            "job_id": "job-negative",
            "session_id": &guard_sid,
            "confirm": true,
            "expected_failure": true,
            "expected_failure_kind": "session_guard_denied",
            "assertion_name": "read-only stop_job guard"
        }),
        None,
    )
    .await;
    assert!(!guard.success);
    assert_eq!(guard.output["error_kind"], "session_guard_denied");

    let summary = runtime.sessions.summary(&guard_sid, Some(20)).unwrap();
    let event = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.assertion_name.as_deref() == Some("read-only stop_job guard")
        })
        .expect("session guard finished event");
    assert_eq!(event.expected_failure, Some(true));
    assert_eq!(
        event.actual_failure_kind.as_deref(),
        Some("session_guard_denied")
    );
    assert_eq!(
        event.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );

    let handoff = handoff_summary_only(&runtime, &guard_sid).await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["tool_failures"]["expected_count"], 1);
    assert_eq!(handoff.output["tool_failures"]["unexpected_count"], 0);
}

#[tokio::test]
async fn session_handoff_summary_only_is_compact() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("compact handoff".to_string()));
    let sid = session.session_id.clone();
    let _ = call_recorded_tool(
        &runtime,
        &sid,
        "job_status",
        json!({
            "job_id": "missing-job",
            "expected_failure": true,
            "expected_failure_kind": "job_not_found",
            "assertion_name": "missing job status"
        }),
        None,
    )
    .await;

    let result = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "session_handoff_summary",
                json!({
                    "session_id": sid,
                    "summary_only": true,
                    "include_workspace": false,
                    "include_checkpoints": false
                }),
            )
            .unwrap(),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["summary_only"], true);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["jobs"]["active_count"], 0);
    assert_eq!(result.output["jobs"]["blocking_active_count"], 0);
    assert_eq!(result.output["jobs"]["nonblocking_active_count"], 0);
    assert_eq!(result.output["jobs"]["terminal_pending_count"], 0);
    assert_eq!(result.output["permissions"]["total_approved_count"], 0);
    assert_eq!(result.output["tool_failures"]["expected_count"], 1);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 0);
    assert!(result.output["tool_failures"]
        .get("expectation_mismatch_count")
        .is_some());
    assert!(result.output["tool_failures"]
        .get("unexpected_success_count")
        .is_some());
    assert_eq!(result.output["validation"]["status"], "not_run");
    assert_eq!(
        result.output["validation"]["reason"],
        "no_validation_tool_invoked"
    );
    assert_eq!(result.output["validation"]["latest_status"], "not_run");
    assert_eq!(
        result.output["validation"]["historical_failures"]["count"],
        0
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["resolved"],
        false
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        false
    );
    assert!(result.output["warnings"].is_array());
    assert!(result.output["suggested_next_actions"].is_array());
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "warn");
    assert_eq!(verdict["blocking"], false);
    assert_reason_list_contains(verdict, "warning_reasons", "expected_failures_matched");
    assert!(verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str() == Some("run validation before closeout when applicable")));
    assert!(!verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str() == Some("run validation before closeout when available")));
    assert_compact_verdict_safe(verdict, "summary_only handoff verdict");
    let serialized = serde_json::to_string(&result.output).unwrap();
    for forbidden in [
        "recent_events",
        "recent_failed_tools",
        "stdout",
        "stderr",
        "tail",
        "excerpt",
        "command",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "summary_only handoff leaked {forbidden}: {serialized}"
        );
    }
}

// =========================================================================
// 4. Active jobs summary
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_includes_active_jobs_and_clears_after_stop() {
    let runtime = test_runtime();
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "handoff-jobs",
        &auth,
        caps,
        vec![registered_project("demo", "/tmp/handoff-jobs-demo")],
    )
    .await;
    let project = "agent:handoff-jobs:demo".to_string();
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("handoff jobs".to_string()));
    let sid = session.session_id.clone();
    let run = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "printf handoff-secret-output".to_string(),
                session_id: Some(sid.clone()),
                timeout_secs: None,
                cwd: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let job_id = run.output["job_id"].as_str().unwrap().to_string();

    let active = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: sid.clone(),
                project: Some(project.clone()),
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(false),
                summary_only: false,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(active.success, "{:?}", active.error);
    assert_eq!(active.output["jobs"]["active_count"], 1);
    assert_eq!(active.output["jobs"]["running_count"], 1);
    assert_eq!(active.output["jobs"]["stop_requested_count"], 0);
    assert_eq!(active.output["jobs"]["terminal_pending_count"], 0);
    assert_eq!(active.output["jobs"]["blocking_active_count"], 1);
    assert_eq!(active.output["jobs"]["nonblocking_active_count"], 0);
    assert_eq!(active.output["jobs"]["recent"][0]["job_id"], job_id);
    assert!(active.output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "active_jobs_present" && warning["blocking"] == true));
    assert_no_raw_validation_output_fields(&active.output["jobs"], "handoff jobs summary");
    let serialized = serde_json::to_string(&active.output["jobs"]).unwrap();
    assert!(!serialized.contains("handoff-secret-output"));

    let stop = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: project.clone(),
                job_id,
                session_id: Some(sid.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(stop.success, "{:?}", stop.error);

    let stopped = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: sid,
                project: Some(project),
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(false),
                summary_only: false,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(stopped.success, "{:?}", stopped.error);
    assert_eq!(stopped.output["jobs"]["active_count"], 0);
    assert_eq!(stopped.output["jobs"]["blocking_active_count"], 0);
    assert_eq!(stopped.output["jobs"]["stop_requested_count"], 0);
    assert!(stopped.output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|warning| warning["kind"] != "active_jobs_present"));
}

#[tokio::test]
async fn session_handoff_summary_treats_stop_requested_as_nonblocking() {
    let runtime = test_runtime();
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "handoff-stop-pending",
        &auth,
        caps,
        vec![registered_project("demo", "/tmp/handoff-stop-pending-demo")],
    )
    .await;
    let project = "agent:handoff-stop-pending:demo".to_string();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("handoff stop pending".to_string()),
    );
    let sid = session.session_id.clone();
    let run = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "printf handoff-stop-pending-secret".to_string(),
                session_id: Some(sid.clone()),
                timeout_secs: None,
                cwd: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let job_id = run.output["job_id"].as_str().unwrap().to_string();
    let start_req = next_agent_request_for_client(&runtime, "handoff-stop-pending")
        .await
        .expect("agent should receive start_job");
    assert_eq!(start_req.kind, "start_job");

    let stop = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: project.clone(),
                job_id: job_id.clone(),
                session_id: Some(sid.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(stop.success, "{:?}", stop.error);
    assert_eq!(stop.output["status_after"], "stop_requested");

    let summary = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: sid,
                project: Some(project),
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(false),
                summary_only: false,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(summary.success, "{:?}", summary.error);
    assert_eq!(summary.output["jobs"]["active_count"], 1);
    assert_eq!(summary.output["jobs"]["running_count"], 0);
    assert_eq!(summary.output["jobs"]["stop_requested_count"], 1);
    assert_eq!(summary.output["jobs"]["terminal_pending_count"], 1);
    assert_eq!(summary.output["jobs"]["blocking_active_count"], 0);
    assert_eq!(summary.output["jobs"]["nonblocking_active_count"], 1);
    assert_eq!(summary.output["jobs"]["recent"][0]["job_id"], job_id);
    let warnings = summary.output["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .all(|warning| warning["kind"] != "active_jobs_present"));
    assert!(warnings.iter().any(|warning| {
        warning["kind"] == "jobs_terminal_pending" && warning["blocking"] == false
    }));
    assert_no_raw_validation_output_fields(&summary.output["jobs"], "handoff jobs summary");
    let serialized = serde_json::to_string(&summary.output["jobs"]).unwrap();
    assert!(!serialized.contains("handoff-stop-pending-secret"));
}

// =========================================================================
// 5. Unknown session
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_unknown_session() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: "wc_sess_unknown".to_string(),
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_session_id");
    assert_eq!(result.output["session_id"], "wc_sess_unknown");
}

// =========================================================================
// 6. Read-only session allowed
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_read_only_session_allowed() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session_with_guards(
        None,
        Some("readonly handoff".to_string()),
        SessionMode::ReadOnly,
        crate::tool_runtime::sessions::SessionGuards::default(),
    );
    let sid = session.session_id.clone();

    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid.clone(),
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_id"], sid);
    assert_eq!(result.output["mode"], "read_only");
    assert_eq!(result.output["permissions"]["required_count"], 0);
    assert_eq!(result.output["permissions"]["auto_approved_count"], 0);
    assert_eq!(result.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(result.output["permissions"]["approved_count"], 0);
    assert_eq!(result.output["permissions"]["total_approved_count"], 0);
    assert!(result.output["permissions"]["recent"]
        .as_array()
        .unwrap()
        .is_empty());
}

// =========================================================================
// 7. Validation summary
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_includes_validation_by_default_from_session_ledger() {
    let runtime = test_runtime();
    let project = "agent:eval:demo".to_string();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("validation handoff".to_string()),
    );
    let sid = session.session_id.clone();

    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({
            "project": project,
            "all_targets": true,
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout": "must not leak",
            "stderr_tail": "must not leak",
        }),
    );

    let expected_summary = runtime.sessions.summary(&sid, Some(200)).unwrap();
    let expected_validation = validation_summary_for_session(&expected_summary);
    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid,
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    let validation = &result.output["validation"];
    assert_eq!(validation, &expected_validation);
    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "passed");
    assert!(validation["reason"].is_null());
    assert_eq!(validation["source"], "session_ledger");
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["successes"], 1);
    assert_eq!(validation["failures"], 0);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest_success"]["validation_kind"], "check");
    assert_eq!(validation["latest_success"]["success"], true);
    assert_eq!(validation["latest_success"]["exit_code"], 0);
    assert_eq!(
        validation["latest_success"]["summary"],
        "cargo_check succeeded"
    );
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert!(validation["latest_success"].get("diagnostics").is_none());
    assert_no_raw_validation_output_fields(validation, "handoff validation summary");
}

#[tokio::test]
async fn session_handoff_summary_can_omit_validation() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("omit validation".to_string()));
    let sid = session.session_id.clone();
    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );

    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid,
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: Some(false),
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("validation").is_none());
}

#[tokio::test]
async fn session_handoff_summary_validation_unavailable_without_validation_events() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("inspect-only validation".to_string()));
    let sid = session.session_id.clone();
    record_handoff_tool_event(
        &runtime,
        &sid,
        "read_file",
        json!({"project": "agent:eval:demo", "path": "src/lib.rs"}),
        true,
        json!({}),
    );

    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid,
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: None,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    let validation = &result.output["validation"];
    assert_eq!(validation["available"], false);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["reason"], "no_validation_tool_invoked");
    assert_eq!(validation["source"], "session_ledger");
    assert_eq!(validation["events_total"], 0);
    assert!(validation["events"].as_array().unwrap().is_empty());
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert!(validation.get("latest_success").is_none());
    assert!(validation.get("latest_failure").is_none());
}

// =========================================================================
// 7. Workspace — clean git project
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_with_workspace_clean_project() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project = register_agent_project_at_path(&runtime, "hw", "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("workspace handoff".to_string()));
    let sid = session.session_id.clone();

    let result = dispatch_handoff_with_agent(&runtime, "hw", sid, Some(project), true, false).await;

    assert!(result.success, "{:?}", result.error);
    let workspace = &result.output["workspace"];
    assert_eq!(workspace["git_available"], true);
    assert_eq!(workspace["non_git_project"], false);
    assert_eq!(workspace["clean"], true);
    assert!(workspace["branch"].as_str().is_some());
    assert!(workspace["head"]["short"].as_str().is_some());
    assert_eq!(workspace["changed_files_count"], 0);
    // Must not include hunks or full diff.
    assert!(workspace.get("hunks").is_none());
    assert!(workspace.get("files").is_none());
    assert!(workspace.get("diff_stat").is_none());
}

#[tokio::test]
async fn session_handoff_summary_only_verdict_allows_clean_workspace_without_failures() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "handoff-clean-verdict", "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("clean handoff".to_string()));
    let sid = session.session_id.clone();

    let result = dispatch_handoff_summary_only_with_agent(
        &runtime,
        "handoff-clean-verdict",
        sid,
        Some(project),
        true,
        false,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["summary_only"], true);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 0);
    assert_ne!(result.output["verdict"]["status"], "fail");
    assert_eq!(result.output["verdict"]["blocking"], false);
    assert_workflow_verdict_shape(&result.output["verdict"]);
    assert_compact_verdict_safe(&result.output["verdict"], "clean handoff verdict");
}

#[tokio::test]
async fn session_handoff_summary_only_warns_for_resolved_historical_validation_failures() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "handoff-resolved-validation", "demo", tmp.path())
            .await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("resolved validation handoff".to_string()),
    );
    let sid = session.session_id.clone();

    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_test",
        json!({
            "project": project,
            "expected_failure": true,
            "expected_failure_kind": "validation_failed",
            "assertion_name": "pre-fix validation should fail"
        }),
        false,
        json!({
            "exit_code": 101,
            "failure_kind": "validation_failed"
        }),
    );
    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({"project": project}),
        true,
        json!({"exit_code": 0}),
    );

    let result = dispatch_handoff_summary_only_with_agent(
        &runtime,
        "handoff-resolved-validation",
        sid,
        Some(project),
        true,
        false,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(result.output["validation"]["status"], "mixed");
    assert_eq!(result.output["validation"]["latest_status"], "passed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["count"],
        1
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["resolved"],
        true
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        false
    );
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "warn");
    assert_eq!(verdict["blocking"], false);
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "validation_historical_failures_resolved",
    );
    assert_reason_list_not_contains(verdict, "blocking_reasons", "validation_mixed");
    assert!(verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str()
            == Some(
                "historical validation failures were resolved by later successful validation"
            )));
}

#[tokio::test]
async fn session_handoff_summary_only_verdict_fails_for_failed_validation() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("failed validation handoff".to_string()));
    let sid = session.session_id.clone();

    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_test",
        json!({
            "project": "agent:eval:demo",
            "expected_failure": true,
            "expected_failure_kind": "validation_failed",
            "assertion_name": "validation failure remains blocking"
        }),
        false,
        json!({
            "exit_code": 101,
            "failure_kind": "validation_failed"
        }),
    );

    let result = handoff_summary_only(&runtime, &sid).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["validation"]["status"], "failed");
    assert_eq!(result.output["validation"]["latest_status"], "failed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        true
    );
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], true);
    assert_reason_list_contains(verdict, "blocking_reasons", "validation_failed");
}

#[tokio::test]
async fn session_handoff_summary_only_verdict_fails_for_unresolved_mixed_validation() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        None,
        Some("unresolved mixed validation handoff".to_string()),
    );
    let sid = session.session_id.clone();

    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );
    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_test",
        json!({
            "project": "agent:eval:demo",
            "expected_failure": true,
            "expected_failure_kind": "validation_failed",
            "assertion_name": "later validation failure remains blocking"
        }),
        false,
        json!({
            "exit_code": 101,
            "failure_kind": "validation_failed"
        }),
    );

    let result = handoff_summary_only(&runtime, &sid).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["validation"]["status"], "mixed");
    assert_eq!(result.output["validation"]["latest_status"], "failed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        true
    );
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], true);
    assert_reason_list_contains(verdict, "blocking_reasons", "validation_mixed");
    assert_reason_list_not_contains(
        verdict,
        "warning_reasons",
        "validation_historical_failures_resolved",
    );
}

// =========================================================================
// 8. Non-git project does not fail the whole tool
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_non_git_project_does_not_fail_whole_tool() {
    let tmp = tempfile::tempdir().unwrap();
    // Intentionally do NOT init a git repo.
    let runtime = test_runtime();
    let project = register_agent_project_at_path(&runtime, "ng", "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("non-git handoff".to_string()));
    let sid = session.session_id.clone();

    let result = dispatch_handoff_with_agent(&runtime, "ng", sid, Some(project), true, false).await;

    assert!(
        result.success,
        "non-git project must not fail the whole handoff: {:?}",
        result.error
    );
    let workspace = &result.output["workspace"];
    assert_eq!(workspace["git_available"], false);
    assert_eq!(workspace["non_git_project"], true);
    // Session info should still be present.
    assert!(result.output["session_id"].as_str().is_some());
    assert_eq!(result.output["mode"], "normal");
    // Warnings should mention the non-git situation.
    let warnings = workspace["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| {
            let s = serde_json::to_string(w).unwrap_or_default();
            s.contains("git") || s.contains("unavailable")
        }),
        "warnings should mention git unavailability: {:?}",
        warnings
    );
}

// =========================================================================
// 9. Checkpoint — latest last_known_good selection
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_includes_latest_last_known_good_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");

    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-handoff", "agent-proj", root).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("checkpoint handoff".to_string()),
    );
    let sid = session.session_id.clone();

    // Create a snapshot checkpoint (not last_known_good).
    let _snap = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-handoff",
        handoff_checkpoint_create_call(
            project.clone(),
            Some("snapshot"),
            Some("snapshot"),
            &[],
            None,
        ),
    )
    .await;

    // Create a last_known_good with validation passed.
    let lkg_passed = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-handoff",
        handoff_checkpoint_create_call(
            project.clone(),
            Some("lkg passed"),
            Some("last_known_good"),
            &["stable"],
            Some(handoff_checkpoint_validation(
                Some("passed"),
                &["cargo test"],
                Some("all green"),
            )),
        ),
    )
    .await;
    assert!(lkg_passed.success, "{:?}", lkg_passed.error);

    // Create another last_known_good with validation failed (older timestamp
    // is not guaranteed, but the passed one should win regardless).
    let _lkg_failed = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-handoff",
        handoff_checkpoint_create_call(
            project.clone(),
            Some("lkg failed"),
            Some("last_known_good"),
            &[],
            Some(handoff_checkpoint_validation(
                Some("failed"),
                &["cargo test"],
                Some("broken"),
            )),
        ),
    )
    .await;

    // Call handoff with checkpoints but no workspace (to avoid git agent req).
    let result =
        dispatch_handoff_with_agent(&runtime, "ckpt-handoff", sid, Some(project), false, true)
            .await;

    assert!(result.success, "{:?}", result.error);
    let checkpoints = &result.output["checkpoints"];
    let lkg = &checkpoints["latest_last_known_good"];
    assert_eq!(lkg["kind"], "last_known_good");
    assert_eq!(lkg["validation_status"], "passed");
    assert_eq!(lkg["title"], "lkg passed");

    // Must not include validation.commands.
    let lkg_serialized = serde_json::to_string(lkg).unwrap();
    assert!(
        !lkg_serialized.contains("commands"),
        "must not include validation.commands: {lkg_serialized}"
    );
    assert!(
        !lkg_serialized.contains("cargo test"),
        "must not include validation command bodies: {lkg_serialized}"
    );

    // Recent list should be bounded.
    let recent = checkpoints["recent"]
        .as_array()
        .expect("recent checkpoints array");
    assert!(!recent.is_empty());
    assert!(recent.len() <= 10);
}

// =========================================================================
// 10. Output is bounded
// =========================================================================

#[tokio::test]
async fn session_handoff_summary_output_is_bounded() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("bounded handoff".to_string()));
    let sid = session.session_id.clone();

    // Post many messages.
    for i in 0..50 {
        post_session_message(
            &runtime,
            &sid,
            "todo",
            &format!("todo item {i} with some padding text to make it longer"),
        );
        post_session_message(&runtime, &sid, "progress", &format!("progress {i}"));
    }

    // Record many tool call events (succeeded).
    for _ in 0..40 {
        let start = runtime.sessions.record_tool_call_started(
            Some(&sid),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "list_tools",
            &json!({}),
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
    }

    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: sid,
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: None,
            summary_only: false,
            limit: Some(5),
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    // Open todos should be bounded by MAX_OPEN_ITEMS (20) regardless of limit.
    let open_todos = result.output["open_todos"]
        .as_array()
        .expect("open_todos array");
    assert!(
        open_todos.len() <= 20,
        "open_todos should be bounded: {}",
        open_todos.len()
    );
    // Recent progress should be bounded by MAX_RECENT_PROGRESS (10).
    let recent_progress = result.output["recent_progress"]
        .as_array()
        .expect("recent_progress array");
    assert!(
        recent_progress.len() <= 10,
        "recent_progress should be bounded: {}",
        recent_progress.len()
    );
    // Message bodies should be truncated.
    for todo in open_todos {
        let msg = todo["message"].as_str().unwrap_or("");
        assert!(
            msg.chars().count() <= 243,
            "message should be bounded: {} chars",
            msg.chars().count()
        );
    }
    // Events count is reported but the events themselves are not returned.
    assert!(result.output["counts"]["events"].as_u64().unwrap() > 0);
    assert!(result.output.get("events").is_none());
}

// =========================================================================
// 11. Metadata / MCP / OpenAPI consistency
// =========================================================================

#[test]
fn session_handoff_summary_metadata_mcp_openapi_consistency() {
    // readOnlyHint must be true.
    let spec = registered_tool_specs()
        .into_iter()
        .find(|s| s.name == "session_handoff_summary")
        .expect("session_handoff_summary spec");
    assert_eq!(spec.annotations["readOnlyHint"], true);
    assert_eq!(spec.annotations["destructiveHint"], false);
    assert_eq!(spec.annotations["openWorldHint"], false);
    let input_props = spec.input_schema["properties"]
        .as_object()
        .expect("handoff input properties");
    assert!(
        input_props.contains_key("include_validation"),
        "session_handoff_summary input schema should expose include_validation"
    );
    assert!(
        input_props.contains_key("summary_only"),
        "session_handoff_summary input schema should expose summary_only"
    );
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("handoff output properties");
    assert!(
        output_props.contains_key("validation"),
        "session_handoff_summary output schema should expose validation"
    );
    assert!(
        output_props.contains_key("permissions"),
        "session_handoff_summary output schema should expose permissions"
    );
    assert!(
        output_props.contains_key("tool_failures"),
        "session_handoff_summary output schema should expose tool_failures"
    );
    assert!(
        output_props.contains_key("verdict"),
        "session_handoff_summary output schema should expose verdict"
    );
    let description = spec.description.to_lowercase();
    for phrase in [
        "ledger-derived validation",
        "bounded tails",
        "safe result metadata",
        "validation.parser.available",
    ] {
        assert!(
            description.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {description}"
        );
    }

    // Metadata: read-only, runtime:read scope.
    let metadata = crate::tool_runtime::metadata::lookup_tool_metadata("session_handoff_summary")
        .expect("metadata");
    assert!(metadata.read_only);
    assert!(!metadata.destructive);
    assert!(!metadata.shell_like);
    assert_eq!(metadata.oauth_scope, Some("runtime:read"));

    // OpenAPI operation count must stay 25 after demoting compatibility edits.
    let spec = crate::openapi::build_openapi_spec();
    let tool_desc = &spec["components"]["schemas"]["ToolCallRequest"]["properties"]["tool"]
        ["description"]
        .as_str()
        .unwrap();
    assert!(
        tool_desc.contains("session_handoff_summary"),
        "OpenAPI ToolCallRequest.tool should list session_handoff_summary"
    );
    let tool_props = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .expect("ToolCallRequest properties");
    assert!(
        tool_props.contains_key("include_validation"),
        "OpenAPI ToolCallRequest should expose flattened include_validation"
    );
    assert!(
        tool_props.contains_key("include_workspace"),
        "OpenAPI ToolCallRequest should expose flattened include_workspace"
    );
    assert!(
        tool_props.contains_key("include_checkpoints"),
        "OpenAPI ToolCallRequest should expose flattened include_checkpoints"
    );
    for field in [
        "summary_only",
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
    ] {
        assert!(
            tool_props.contains_key(field),
            "OpenAPI ToolCallRequest should expose flattened {field}"
        );
    }
    let count: usize = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|m| m.as_object().unwrap().len())
        .sum();
    assert_eq!(count, 25, "OpenAPI operation count must remain 25");
}

// =========================================================================
// Helpers
// =========================================================================

fn post_session_message(runtime: &ToolRuntime, session_id: &str, kind: &str, message: &str) {
    use crate::tool_runtime::sessions::{
        PostSessionMessageInput, SessionMessageKind, SessionMessagePriority,
    };
    let kind = match kind {
        "note" => SessionMessageKind::Note,
        "proposal" => SessionMessageKind::Proposal,
        "question" => SessionMessageKind::Question,
        "answer" => SessionMessageKind::Answer,
        "decision" => SessionMessageKind::Decision,
        "risk" => SessionMessageKind::Risk,
        "progress" => SessionMessageKind::Progress,
        "guidance" => SessionMessageKind::Guidance,
        "todo" => SessionMessageKind::Todo,
        _ => panic!("unknown message kind: {kind}"),
    };
    runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: session_id.to_string(),
            kind,
            message: message.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();
}

fn handoff_checkpoint_create_call(
    project: String,
    title: Option<&str>,
    kind: Option<&str>,
    labels: &[&str],
    validation: Option<CheckpointValidationInput>,
) -> ToolCall {
    ToolCall::WorkspaceCheckpointCreate {
        project,
        title: title.map(str::to_string),
        note: None,
        include_untracked: Some(false),
        kind: kind.map(str::to_string),
        labels: labels.iter().map(|label| (*label).to_string()).collect(),
        validation,
        session_id: None,
    }
}

fn handoff_checkpoint_validation(
    status: Option<&str>,
    commands: &[&str],
    summary: Option<&str>,
) -> CheckpointValidationInput {
    CheckpointValidationInput {
        status: status.map(str::to_string),
        commands: commands.iter().map(|c| (*c).to_string()).collect(),
        summary: summary.map(str::to_string),
    }
}

fn record_handoff_tool_event(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    success: bool,
    output: Value,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &arguments,
    );
    let error = (!success).then_some("tool failed");
    runtime
        .sessions
        .record_tool_call_finished(start, success, &output, error, None);
}

fn json_contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(key) || map.values().any(|value| json_contains_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| json_contains_key(value, key)),
        _ => false,
    }
}

fn assert_no_raw_validation_output_fields(value: &Value, context: &str) {
    for key in [
        "stdout",
        "stderr",
        "stdout_tail",
        "stderr_tail",
        "stdout_tail_excerpt",
        "stderr_tail_excerpt",
        "validation_output_summary",
    ] {
        assert!(
            !json_contains_key(value, key),
            "{context} must not include {key}: {value}"
        );
    }
}

async fn call_recorded_tool(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: Some(session_id),
                auth,
                record_oauth_scope_denials: true,
            },
        )
        .await;
    outcome.result.unwrap_or_else(|| {
        let detail = outcome.error_status.map(|status| format!("{status:?}"));
        ToolResult::err(detail.unwrap_or_else(|| "tool returned no result".to_string()))
    })
}

async fn call_typed_tool_with_metadata(
    runtime: &ToolRuntime,
    tool_name: &str,
    arguments: Value,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let (call, metadata) =
        ToolCall::from_tool_name_with_recorder_metadata(tool_name, arguments).unwrap();
    runtime
        .dispatch_with_auth_transport_options_and_metadata(
            call,
            auth,
            SessionTransport::Mcp,
            true,
            false,
            metadata,
        )
        .await
}

async fn call_typed_tool_with_metadata_and_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    tool_name: &str,
    arguments: Value,
    auth: Option<AuthContext>,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let tool_name = tool_name.to_string();
    let task = tokio::spawn(async move {
        call_typed_tool_with_metadata(&runtime_for_task, &tool_name, arguments, auth.as_ref()).await
    });
    complete_agent_shell_requests_until_finished(runtime, client_id, &task).await;
    task.await.unwrap()
}

async fn call_kernel_tool(
    runtime: &ToolRuntime,
    tool_name: &str,
    arguments: Value,
    recording_session_id: Option<&str>,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: recording_session_id,
                auth,
                record_oauth_scope_denials: true,
            },
        )
        .await;
    outcome.result.unwrap_or_else(|| {
        let detail = outcome.error_status.map(|status| format!("{status:?}"));
        ToolResult::err(detail.unwrap_or_else(|| "tool returned no result".to_string()))
    })
}

async fn call_kernel_tool_with_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    tool_name: &str,
    arguments: Value,
    recording_session_id: Option<String>,
    auth: Option<AuthContext>,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let tool_name = tool_name.to_string();
    let task = tokio::spawn(async move {
        call_kernel_tool(
            &runtime_for_task,
            &tool_name,
            arguments,
            recording_session_id.as_deref(),
            auth.as_ref(),
        )
        .await
    });
    complete_agent_shell_requests_until_finished(runtime, client_id, &task).await;
    task.await.unwrap()
}

async fn finish_coding_task_summary_only_no_hygiene(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    session_id: &str,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let session_id = session_id.to_string();
    let task = tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        runtime_for_task
            .dispatch_with_auth(
                ToolCall::FinishCodingTask {
                    project,
                    session_id,
                    summary_only: true,
                    include_diff: Some(false),
                    include_workspace: None,
                    include_hygiene: Some(false),
                    include_handoff: Some(false),
                    include_validation_summary: Some(false),
                },
                Some(&auth),
            )
            .await
    });
    complete_agent_shell_requests_until_finished(runtime, client_id, &task).await;
    task.await.unwrap()
}

async fn complete_agent_shell_requests_until_finished<T>(
    runtime: &ToolRuntime,
    client_id: &str,
    task: &tokio::task::JoinHandle<T>,
) {
    for _ in 0..200 {
        if task.is_finished() {
            return;
        }
        if let Some(req) = next_patch_agent_request(runtime, client_id).await {
            complete_agent_request_by_running_locally(runtime, client_id, req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    assert!(
        task.is_finished(),
        "tool did not finish after agent requests"
    );
}

async fn handoff_summary(runtime: &ToolRuntime, session_id: &str) -> ToolResult {
    runtime
        .dispatch(
            ToolCall::from_tool_name(
                "session_handoff_summary",
                json!({
                    "session_id": session_id,
                    "include_workspace": false,
                    "include_checkpoints": false
                }),
            )
            .unwrap(),
        )
        .await
}

async fn handoff_summary_only(runtime: &ToolRuntime, session_id: &str) -> ToolResult {
    runtime
        .dispatch(
            ToolCall::from_tool_name(
                "session_handoff_summary",
                json!({
                    "session_id": session_id,
                    "include_workspace": false,
                    "include_checkpoints": false,
                    "summary_only": true
                }),
            )
            .unwrap(),
        )
        .await
}

/// Dispatch `session_handoff_summary` through the agent path, completing any
/// agent shell requests (from the internal `show_changes` call) locally.
async fn dispatch_handoff_with_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    session_id: String,
    project: Option<String>,
    include_workspace: bool,
    include_checkpoints: bool,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch(ToolCall::SessionHandoffSummary {
                session_id,
                project,
                include_workspace: Some(include_workspace),
                include_checkpoints: Some(include_checkpoints),
                include_validation: Some(true),
                summary_only: false,
                limit: None,
            })
            .await
    });

    // If include_workspace is true, the internal show_changes call enqueues
    // an agent shell request. Complete it locally.
    if include_workspace {
        let req = next_patch_agent_request(runtime, client_id)
            .await
            .expect("handoff workspace should enqueue an agent shell request");
        complete_agent_request_by_running_locally(runtime, client_id, req).await;
    }

    task.await.unwrap()
}

async fn dispatch_handoff_summary_only_with_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    session_id: String,
    project: Option<String>,
    include_workspace: bool,
    include_checkpoints: bool,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch(ToolCall::SessionHandoffSummary {
                session_id,
                project,
                include_workspace: Some(include_workspace),
                include_checkpoints: Some(include_checkpoints),
                include_validation: Some(true),
                summary_only: true,
                limit: None,
            })
            .await
    });

    if include_workspace {
        let req = next_patch_agent_request(runtime, client_id)
            .await
            .expect("handoff workspace should enqueue an agent shell request");
        complete_agent_request_by_running_locally(runtime, client_id, req).await;
    }

    task.await.unwrap()
}

fn assert_reason_list_contains(verdict: &Value, key: &str, reason: &str) {
    let reasons = verdict[key].as_array().expect("reason list");
    assert!(
        reasons.iter().any(|value| value.as_str() == Some(reason)),
        "{key} should contain {reason}: {verdict}"
    );
}

fn assert_reason_list_not_contains(verdict: &Value, key: &str, reason: &str) {
    let reasons = verdict[key].as_array().expect("reason list");
    assert!(
        !reasons.iter().any(|value| value.as_str() == Some(reason)),
        "{key} should not contain {reason}: {verdict}"
    );
}

fn assert_workflow_verdict_shape(verdict: &Value) {
    let status = verdict["status"].as_str().expect("status string");
    assert!(
        matches!(status, "pass" | "warn" | "fail"),
        "unexpected verdict status {status}: {verdict}"
    );
    assert!(verdict["blocking"].is_boolean(), "blocking bool: {verdict}");
    for key in [
        "blocking_reasons",
        "warning_reasons",
        "suggested_next_actions",
    ] {
        assert!(verdict[key].is_array(), "{key} array: {verdict}");
    }
}

fn assert_compact_verdict_safe(value: &Value, context: &str) {
    let serialized = serde_json::to_string(value).unwrap();
    for forbidden in [
        "stdout", "stderr", "tail", "excerpt", "command", "token", "secret", "env",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "{context} leaked {forbidden}: {serialized}"
        );
    }
}
