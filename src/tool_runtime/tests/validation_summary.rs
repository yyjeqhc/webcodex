use super::support::*;
use crate::auth::scopes::{oauth_scope_policy_for_runtime_tool, OAuthToolScopePolicy};
use crate::auth::SCOPE_PROJECT_READ;
use crate::shell_protocol::ShellClientCapabilities;
use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk};
use crate::tool_runtime::registry::output_schema_for_tool;
use crate::tool_runtime::sessions::{SessionCreateOptions, SessionGuards, SessionTransport};
use crate::tool_runtime::tool_definition::known_tool_names;
use crate::tool_runtime::{registered_tool_specs, SessionMode, ToolCall, ToolRuntime};
use serde_json::{json, Value};

#[test]
fn validation_summary_registration_schema_metadata_and_openapi_are_synchronized() {
    let call = ToolCall::from_tool_name(
        "validation_summary",
        json!({
            "project": SAMPLE_PROJECT,
            "session_id": "wc_sess_explicit",
            "limit": 7
        }),
    )
    .expect("validation_summary should parse");
    match call {
        ToolCall::ValidationSummary {
            project,
            session_id,
            limit,
        } => {
            assert_eq!(project, SAMPLE_PROJECT);
            assert_eq!(session_id, "wc_sess_explicit");
            assert_eq!(limit, Some(7));
        }
        other => panic!("expected validation_summary, got {other:?}"),
    }

    let specs = registered_tool_specs();
    assert_eq!(specs.len(), 76);
    assert_eq!(known_tool_names().count(), 76);
    let spec = spec_named(&specs, "validation_summary");
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "session_id"])
    );
    assert_eq!(spec.input_schema["properties"]["limit"]["minimum"], 1);
    assert_eq!(spec.input_schema["properties"]["limit"]["maximum"], 100);
    assert!(spec.description.to_lowercase().contains("does not run"));

    let output = output_schema_for_tool("validation_summary");
    let output = &output["properties"]["output"];
    assert_eq!(output["additionalProperties"], false);
    for field in ["project", "session_id", "validation"] {
        assert!(output["properties"].get(field).is_some(), "missing {field}");
    }
    assert_eq!(
        output["properties"]["validation"]["additionalProperties"],
        false
    );

    let metadata = lookup_tool_metadata("validation_summary").unwrap();
    assert_eq!(metadata.risk, ToolRisk::ReadOnly);
    assert_eq!(metadata.oauth_scope, Some(SCOPE_PROJECT_READ));
    assert_eq!(metadata.requires_project, true);
    assert_eq!(metadata.read_only, true);
    assert_eq!(metadata.destructive, false);
    assert_eq!(metadata.shell_like, false);
    assert_eq!(
        oauth_scope_policy_for_runtime_tool("validation_summary"),
        OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ)
    );

    let openapi = crate::openapi::build_openapi_spec();
    let tool_call = &openapi["components"]["schemas"]["ToolCallRequest"];
    let description = tool_call["properties"]["tool"]["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("validation_summary"));
    for field in ["project", "session_id", "limit"] {
        assert!(
            tool_call["properties"].get(field).is_some(),
            "missing {field}"
        );
    }
    assert_eq!(tool_call["additionalProperties"], false);
    let operation_count: usize = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|methods| methods.as_object().unwrap().len())
        .sum();
    assert_eq!(operation_count, 25);
}

#[tokio::test]
async fn validation_summary_is_guard_safe_read_only_and_does_not_pollute_ledger() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "validation-summary-safe", "demo", tmp.path())
            .await;
    let session = runtime
        .sessions
        .start_session_with_options(SessionCreateOptions {
            project: Some(project.clone()),
            title: Some("validation summary safe".to_string()),
            mode: SessionMode::ReadOnly,
            guards: SessionGuards {
                deny_write_tools: true,
                deny_shell_tools: true,
            },
            project_instructions: None,
        });
    let auth = bootstrap_auth_context();

    let first = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: project.clone(),
                session_id: session.session_id.clone(),
                limit: None,
            },
            Some(&auth),
        )
        .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["project"], project);
    assert_eq!(first.output["session_id"], session.session_id);
    assert_eq!(first.output["validation"]["available"], false);
    assert_eq!(first.output["validation"]["status"], "not_run");
    assert_eq!(first.output["validation"]["events_total"], 0);
    assert_eq!(first.output["validation"]["parser"]["available"], false);
    assert_eq!(
        first.output["validation"]["parser"]["raw_output_exposed"],
        false
    );

    let second = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: project.clone(),
                session_id: session.session_id.clone(),
                limit: Some(100),
            },
            Some(&auth),
        )
        .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["validation"], first.output["validation"]);
    assert!(runtime
        .sessions
        .summary(&session.session_id, Some(100))
        .unwrap()
        .events
        .is_empty());
    assert!(
        next_patch_agent_request(&runtime, "validation-summary-safe")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn validation_summary_preserves_history_bounds_and_safe_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "validation-summary-history", "demo", tmp.path())
            .await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("history".to_string()));

    let mut diagnostic_output = String::new();
    for index in 0..25 {
        diagnostic_output.push_str(&format!("error: m{index}\n --> s/f{index}:1:1\n"));
    }
    record_validation_event(
        &runtime,
        &session.session_id,
        "cargo_check",
        &project,
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": diagnostic_output,
            "stdout_truncated": false,
            "stderr_truncated": true,
            "failure_kind": "validation_failed"
        }),
    );
    record_validation_event(
        &runtime,
        &session.session_id,
        "cargo_check",
        &project,
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "Finished dev profile\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false
        }),
    );

    let auth = bootstrap_auth_context();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: project.clone(),
                session_id: session.session_id.clone(),
                limit: Some(1),
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    let validation = &result.output["validation"];
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], true);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
    assert_eq!(validation["events_total"], 2);
    assert_eq!(validation["events"].as_array().unwrap().len(), 1);
    let diagnostics = &validation["latest_failure"]["diagnostics"];
    assert_eq!(diagnostics["diagnostic_count"], 25);
    assert_eq!(diagnostics["returned_diagnostic_count"], 20);
    assert_eq!(diagnostics["diagnostics_truncated"], true);
    assert_eq!(diagnostics["diagnostics"].as_array().unwrap().len(), 20);
    assert_eq!(validation["parser"]["kind"], "structured_validation_parser");
    assert_eq!(validation["parser"]["version"], 3);
    assert_safe_public_validation_output(&result.output);

    let min_clamped = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: project.clone(),
                session_id: session.session_id.clone(),
                limit: Some(0),
            },
            Some(&auth),
        )
        .await;
    assert_eq!(
        min_clamped.output["validation"]["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for _ in 0..103 {
        record_validation_event(
            &runtime,
            &session.session_id,
            "cargo_check",
            &project,
            true,
            json!({"exit_code": 0}),
        );
    }
    let max_clamped = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project,
                session_id: session.session_id,
                limit: Some(1_000),
            },
            Some(&auth),
        )
        .await;
    assert_eq!(max_clamped.output["validation"]["events_total"], 100);
    assert_eq!(
        max_clamped.output["validation"]["events"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
}

#[tokio::test]
async fn validation_summary_keeps_zero_tests_from_resolving_cargo_test_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "validation-summary-zero", "demo", tmp.path())
            .await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("zero tests".to_string()));
    record_validation_event(
        &runtime,
        &session.session_id,
        "cargo_test",
        &project,
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "test tests::fails ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed"
        }),
    );
    record_validation_event(
        &runtime,
        &session.session_id,
        "cargo_test",
        &project,
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 0,
            "zero_tests_run": true
        }),
    );

    let auth = bootstrap_auth_context();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project,
                session_id: session.session_id,
                limit: None,
            },
            Some(&auth),
        )
        .await;
    assert_eq!(result.output["validation"]["status"], "mixed");
    assert_eq!(result.output["validation"]["latest_status"], "passed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["resolved"],
        false
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        true
    );
}

#[tokio::test]
async fn validation_summary_rejects_unknown_mismatched_and_unauthorized_sessions() {
    let runtime = test_runtime();
    let owner = auth_context(Some("owner"), false);
    let intruder = auth_context(Some("intruder"), false);
    let projects = vec![
        named_registered_project("validation-summary-auth", "one", "One", "/tmp/one", 1),
        named_registered_project("validation-summary-auth", "two", "Two", "/tmp/two", 2),
    ];
    register_agent_projects_for_auth(
        &runtime,
        "validation-summary-auth",
        &owner,
        ShellClientCapabilities::default(),
        projects,
    )
    .await;
    let one = "agent:validation-summary-auth:one".to_string();
    let two = "agent:validation-summary-auth:two".to_string();
    let bootstrap = bootstrap_auth_context();
    let session = runtime
        .sessions
        .start_session(Some(one.clone()), Some("auth".to_string()));

    let unknown = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: one.clone(),
                session_id: "wc_sess_unknown".to_string(),
                limit: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!unknown.success);
    assert_eq!(unknown.output["error_kind"], "unknown_session_id");

    let mismatch = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: two,
                session_id: session.session_id.clone(),
                limit: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");

    let unauthorized = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: one,
                session_id: session.session_id,
                limit: None,
            },
            Some(&intruder),
        )
        .await;
    assert!(!unauthorized.success);
    let error = unauthorized.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("forbidden")
            || error.contains("owner")
            || error.contains("unknown shell client"),
        "unexpected auth error: {error:?}"
    );
}

#[tokio::test]
async fn validation_summary_is_present_in_validation_tool_manifest_without_new_action() {
    let runtime = test_runtime();
    let manifest = runtime
        .dispatch(ToolCall::ToolManifest {
            category: Some("validation".to_string()),
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(manifest.success, "{:?}", manifest.error);
    assert!(manifest.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "validation_summary"));
}

fn record_validation_event(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    project: &str,
    success: bool,
    output: Value,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &json!({"project": project}),
    );
    runtime.sessions.record_tool_call_finished(
        start,
        success,
        &output,
        (!success).then_some("validation failed"),
        None,
    );
}

fn assert_safe_public_validation_output(value: &Value) {
    for key in [
        "stdout",
        "stderr",
        "stdout_tail",
        "stderr_tail",
        "stdout_tail_excerpt",
        "stderr_tail_excerpt",
        "validation_output_summary",
        "command",
        "env",
        "environment",
    ] {
        assert!(
            !json_contains_key(value, key),
            "public output leaked {key}: {value}"
        );
    }
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
