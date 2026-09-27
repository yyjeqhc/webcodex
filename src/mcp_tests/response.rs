use crate::mcp::response::{
    mcp_runtime_tool_result_fallback_with_compat, McpToolResultPresentation,
};
use crate::tool_runtime::ToolResult;
use serde_json::json;

#[test]
fn canonical_result_presentation_changes_only_is_error() {
    let cargo_stderr = "error[E0308]: mismatched types\n --> src/lib.rs:42:9\n";
    let cargo_diagnostics =
        webcodex_core::validation_evidence::parse_cargo_check_diagnostics("", cargo_stderr, false);
    assert_eq!(cargo_diagnostics.returned_diagnostic_count, 1);
    let results = [
        ToolResult::ok(json!({"count": 2})),
        ToolResult::err_with_output(
            "exact target matched multiple locations",
            json!({
                "execution_state": "not_started",
                "state_changed": false,
                "error_kind": "multiple_matches",
                "match_count": 2,
                "candidate_ranges": [{"start_line": 10, "end_line": 10}, {"start_line": 20, "end_line": 20}],
                "conflicting_edit_indices": [0, 1],
                "recovery": {"follow_up_kind": "mechanically_followable", "tool": "read_files", "arguments": {"project": "agent:r:p", "items": [{"path": "probe.txt"}]}}
            }),
        ),
        // Canonical process output intentionally does not echo expectation inputs.
        ToolResult::err_with_output(
            "process exited with code 1",
            json!({
                "execution_state": "completed", "exit_code": 1,
                "command_ok": false, "failure_kind": "command_exit_nonzero",
                "expectation_satisfied": true, "stdout_tail": "observed output", "stderr_tail": "diagnostics"
            }),
        ),
        ToolResult::err_with_output(
            "cargo check failed",
            json!({
                "execution_state": "completed", "command_completed": true,
                "terminal": true, "passed": false, "exit_code": 101,
                "errors_count": 1, "warnings_count": 0,
                "diagnostics": cargo_diagnostics,
                "stderr_tail": cargo_stderr, "stdout_tail": ""
            }),
        ),
        ToolResult::err_with_output(
            "completion receipt lost",
            json!({
                "execution_state": "outcome_unknown", "dispatch_certainty": "outcome_unknown",
                "state_changed": null, "failure_kind": "outcome_unknown",
                "recovery_kind": "reconcile",
                "recovery": {"follow_up_kind": "fallback_recovery", "tool": "observe_jobs", "arguments": {"items": [{"job_id": "job-probe"}]}}
            }),
        ),
    ];
    for result in results {
        let copy_result = || ToolResult {
            success: result.success,
            output: result.output.clone(),
            error: result.error.clone(),
        };
        for text_json_compat in [false, true] {
            let mut standard = mcp_runtime_tool_result_fallback_with_compat(
                copy_result(),
                text_json_compat,
                McpToolResultPresentation::Standard,
            );
            let openai = mcp_runtime_tool_result_fallback_with_compat(
                copy_result(),
                text_json_compat,
                McpToolResultPresentation::OpenAiStructuredFailureCompat,
            );
            assert_eq!(standard["isError"], !result.success);
            assert_eq!(openai["isError"], false);
            assert_eq!(
                openai["structuredContent"],
                json!({
                    "success": result.success, "output": result.output, "error": result.error,
                })
            );
            standard["isError"] = json!(false);
            assert_eq!(standard, openai, "only the presentation signal may change");
        }
    }
}

#[test]
fn runtime_result_keeps_compact_text_by_default() {
    let rendered = mcp_runtime_tool_result_fallback_with_compat(
        ToolResult::ok(json!({ "count": 2 })),
        false,
        McpToolResultPresentation::Standard,
    );
    assert_eq!(
        rendered["content"][0]["text"],
        "WebCodex tool completed successfully."
    );
    assert_eq!(rendered["structuredContent"]["output"]["count"], 2);
}

#[test]
fn text_json_compat_mirrors_runtime_structured_content() {
    let runtime = mcp_runtime_tool_result_fallback_with_compat(
        ToolResult::ok(json!({ "count": 2 })),
        true,
        McpToolResultPresentation::Standard,
    );
    assert_eq!(
        runtime["content"][0]["text"],
        serde_json::to_string(&runtime["structuredContent"]).unwrap()
    );
}
