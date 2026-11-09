use super::{session_log_arguments_for_tool_request, ToolCall, ToolCallAuditProjection};
use serde_json::{json, Value};

fn assert_request_projection(tool: &str, arguments: Value, expected: Value) {
    let call = ToolCall::from_tool_name(tool, arguments.clone())
        .unwrap_or_else(|error| panic!("{tool}: {error}"));
    assert_eq!(call.session_log_arguments(), expected, "{tool}: typed");
    assert_eq!(
        session_log_arguments_for_tool_request(tool, &arguments),
        expected,
        "{tool}: request"
    );
}

#[test]
fn audit_request_fields_preserve_null_false_zero_and_arrays() {
    for (tool, arguments, expected) in [
        (
            "list_tools",
            json!({"summary_only": false, "limit": 0}),
            json!({"category": null, "features": null, "summary_only": false, "limit": 0}),
        ),
        (
            "stop_job",
            json!({"project": "demo", "job_id": "job-1", "confirm": false}),
            json!({"project": "demo", "job_id": "job-1", "confirm": false}),
        ),
        (
            "delete_project_files",
            json!({
                "project": "demo", "paths": ["src/a.rs", "src/b.rs"],
                "session_id": format!("wc_sess_{}", "1".repeat(32))
            }),
            json!({"project": "demo", "paths": ["src/a.rs", "src/b.rs"]}),
        ),
    ] {
        assert_request_projection(tool, arguments, expected);
    }
}

#[test]
fn job_continuation_audit_omits_binding_ids_from_valid_requests() {
    let wait_id = format!("wc_job_wait_{}", "a".repeat(16));
    let binding_id = format!("wc_host_binding_{}", "A".repeat(22));
    for tool in [
        "bind_job_terminal_continuation",
        "get_job_terminal_continuation_state",
        "prepare_job_terminal_continuation",
        "unbind_job_terminal_continuation",
    ] {
        assert_request_projection(
            tool,
            json!({"wait_id": wait_id, "binding_id": binding_id}),
            json!({"wait_id": wait_id}),
        );
    }
    let attempt_id = format!("wc_job_delivery_{}", "b".repeat(16));
    assert_request_projection(
        "finish_job_terminal_continuation",
        json!({
            "wait_id": wait_id, "binding_id": binding_id,
            "attempt_id": attempt_id, "outcome": "accepted"
        }),
        json!({"wait_id": wait_id, "attempt_id": attempt_id, "outcome": "accepted"}),
    );
}

#[test]
fn unknown_and_malformed_audit_requests_have_no_raw_argument_fallback() {
    for (tool, arguments) in [
        ("unknown_audit_tool", json!({"secret": "PRIVATE_INPUT"})),
        ("list_tools", json!({"summary_only": "PRIVATE_INPUT"})),
        ("stop_job", json!({"project": "PRIVATE_INPUT"})),
    ] {
        assert!(ToolCall::from_tool_name(tool, arguments.clone()).is_err());
        assert_eq!(
            session_log_arguments_for_tool_request(tool, &arguments),
            json!({}),
            "{tool}"
        );
    }
}

#[cfg(feature = "workspace-checkpoints")]
#[test]
fn checkpoint_audit_fields_keep_optional_values_and_omit_recorder_session() {
    for (tool, fields) in [
        ("list_workspace_checkpoints", json!({"limit": null})),
        (
            "read_workspace_checkpoint",
            json!({"checkpoint_id": "checkpoint-1", "include_diff_stat": false}),
        ),
        (
            "restore_workspace_checkpoint",
            json!({"checkpoint_id": "checkpoint-1", "confirm": false}),
        ),
        (
            "delete_workspace_checkpoint",
            json!({"checkpoint_id": "checkpoint-1", "confirm": true}),
        ),
    ] {
        let mut expected = fields;
        expected["project"] = json!("demo");
        let mut arguments = expected.clone();
        arguments["session_id"] = json!(format!("wc_sess_{}", "1".repeat(32)));
        assert_request_projection(tool, arguments, expected);
    }
}
