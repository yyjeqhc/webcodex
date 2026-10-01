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
        ToolResult::ok(
            json!({"changed":true,"files":[{"path":"x","read_revision":8535043794784493_u64}]}),
        ),
        ToolResult::ok(json!({"execution_state":"pending","continuation":{
            "follow_up_kind":"fallback_recovery","tool":"observe_jobs",
            "arguments":{"items":[{"job_id":"job-probe","after_observation_token":"opaque"}]}
        }})),
        ToolResult::ok(json!({"output_truncated":true,"suggested_call":{
            "follow_up_kind":"mechanically_followable","tool":"read_files",
            "arguments":{"project":"agent:r:p","items":[{"path":"x","start_line":20,"expected_read_revision":8535043794784493_u64}]}
        }})),
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

#[test]
fn mcp_execution_failure_logs_have_one_default_copy_and_preserve_recovery() {
    let stdout = "EXECUTION_STDOUT_SENTINEL\n".repeat(80);
    let stderr = "EXECUTION_STDERR_SENTINEL\n".repeat(80);
    for timed_out in [false, true] {
        let prefix = if timed_out {
            "Command timed out after 60s.\nCommand definitely started, but WebCodex cannot prove its side effects ended with the timeout.\nOutput tails before timeout:\n"
        } else {
            "Command exited with status 1.\nNo files were modified by WebCodex itself; command side effects, if any, are from the invoked command.\n"
        };
        let guidance = if timed_out {
            "Retry guidance: do not blindly retry. First inspect the actual Job, process, service, and target state."
        } else {
            "Retry guidance: inspect stderr/stdout above, then fix the reported issue or use a narrower tool."
        };
        let error = format!("{prefix}stdout_tail:\n{stdout}\nstderr_tail:\n{stderr}\n{guidance}");
        let output = json!({
            "execution_state": if timed_out {"timed_out"} else {"completed"},
            "command_started": true, "command_ok": false,
            "exit_code": if timed_out {json!(null)} else {json!(1)},
            "failure_kind": if timed_out {"timeout"} else {"command_exit_nonzero"},
            "tool_failure": false, "stdout_tail": stdout, "stderr_tail": stderr,
            "stdout_truncated": true, "stderr_truncated": false,
            "continuation": {"tool":"observe_jobs", "arguments":{"items":[{"job_id":"original-job"}]}}
        });
        let original = json!({"success":false, "output":output, "error":error});
        for presentation in [
            McpToolResultPresentation::Standard,
            McpToolResultPresentation::OpenAiStructuredFailureCompat,
        ] {
            let rendered = mcp_runtime_tool_result_fallback_with_compat(
                ToolResult::err_with_output(&error, output.clone()),
                false,
                presentation,
            );
            assert_eq!(rendered["structuredContent"]["success"], false);
            assert_eq!(rendered["structuredContent"]["output"], output);
            let compact_error = rendered["structuredContent"]["error"].as_str().unwrap();
            assert!(compact_error.starts_with(prefix));
            assert!(compact_error.contains("Retry guidance:"));
            assert!(!compact_error.contains("SENTINEL"));
            assert!(!rendered["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("SENTINEL"));
            assert_eq!(
                rendered["isError"],
                presentation == McpToolResultPresentation::Standard
            );
            let bytes = serde_json::to_vec(&rendered).unwrap();
            // Even the full MCP envelope is smaller than the old structured result alone.
            assert!(bytes.len() < serde_json::to_vec(&original).unwrap().len());
            assert_eq!(
                String::from_utf8(bytes)
                    .unwrap()
                    .matches("EXECUTION_STDOUT_SENTINEL")
                    .count(),
                80
            );
            if timed_out {
                assert!(compact_error.contains("cannot prove its side effects ended"));
                assert!(compact_error.contains("do not blindly retry"));
            } else {
                assert!(compact_error.contains("command side effects"));
                assert!(compact_error.contains("inspect the output logs"));
            }
        }
        let compat = mcp_runtime_tool_result_fallback_with_compat(
            ToolResult::err_with_output(error, output),
            true,
            McpToolResultPresentation::Standard,
        );
        assert_eq!(
            compat["content"][0]["text"],
            serde_json::to_string(&compat["structuredContent"]).unwrap()
        );
    }
}

#[test]
fn mcp_execution_log_projection_preserves_faults_unknown_and_unmatched_prose() {
    let error = "Original fault.\nstdout_tail:\noutput\nstderr_tail:\ndiagnostic\nDo not retry.";
    for (success, kind, tool_failure, stdout) in [
        (false, "tool_fault", true, "output"),
        (false, "timeout", true, "output"),
        (false, "outcome_unknown", false, "output"),
        (false, "command_exit_nonzero", false, "different output"),
        (true, "command_exit_nonzero", false, "output"),
    ] {
        let result = ToolResult {
            success,
            error: Some(error.to_string()),
            output: json!({
                "failure_kind":kind, "tool_failure":tool_failure,
                "stdout_tail":stdout, "stderr_tail":"diagnostic",
                "recovery_kind":"reobserve", "execution_state":"outcome_unknown"
            }),
        };
        let expected = serde_json::to_value(&result).unwrap();
        let rendered = mcp_runtime_tool_result_fallback_with_compat(
            result,
            false,
            McpToolResultPresentation::Standard,
        );
        assert_eq!(rendered["structuredContent"], expected);
    }
}
