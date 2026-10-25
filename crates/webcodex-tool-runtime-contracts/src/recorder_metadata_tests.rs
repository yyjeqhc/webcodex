//! Recorder-metadata integration tests around the canonical ToolCall parser.

use crate::recorder_metadata::parse_tool_call_with_recorder_metadata;
use serde_json::json;
use webcodex_tool_contracts::ToolCall;

#[test]
fn from_tool_name_records_and_strips_testing_metadata_before_parsing() {
    let (call, metadata) = parse_tool_call_with_recorder_metadata(
        "list_jobs",
        json!({
            "status": "failed",
            "expected_failure": true,
            "expected_failure_kind": "job_not_found",
            "assertion_name": "missing job negative path"
        }),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::ListJobs { .. }));
    assert!(metadata.expectation.expected_failure);
    assert_eq!(
        metadata.expectation.expected_failure_kind.as_deref(),
        Some("job_not_found")
    );
    assert_eq!(
        metadata.expectation.assertion_name.as_deref(),
        Some("missing job negative path")
    );
}

#[test]
fn from_tool_name_records_public_result_expectations_before_parsing() {
    let (call, metadata) = parse_tool_call_with_recorder_metadata(
        "run_process",
        json!({
            "project": "demo",
            "executable": "git",
            "args": ["merge-base", "--is-ancestor", "a", "b"],
            "result_expectation": "observe",
            "accepted_exit_codes": [1, 0, 1]
        }),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::RunProcess { .. }));
    assert!(!metadata.expectation.expected_failure);
    assert_eq!(
        metadata.expectation.result_expectation.as_deref(),
        Some("observe")
    );
    assert_eq!(metadata.expectation.accepted_exit_codes, vec![0, 1]);

    let (call, metadata) = parse_tool_call_with_recorder_metadata(
        "cargo_test",
        json!({
            "project": "demo",
            "filter": "reproduce_bug",
            "result_expectation": "failure"
        }),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::CargoTest { .. }));
    assert!(metadata.expectation.expected_failure);
    assert_eq!(
        metadata.expectation.result_expectation.as_deref(),
        Some("failure")
    );
    assert!(metadata.expectation.accepted_exit_codes.is_empty());
}

#[test]
fn from_tool_name_rejects_unsafe_result_expectation_combinations() {
    let invalid = [
        (
            "run_process",
            json!({
                "project": "demo",
                "executable": "git",
                "result_expectation": "maybe"
            }),
        ),
        (
            "run_process",
            json!({
                "project": "demo",
                "executable": "git",
                "result_expectation": "failure",
                "accepted_exit_codes": [0, 1]
            }),
        ),
        (
            "cargo_test",
            json!({
                "project": "demo",
                "accepted_exit_codes": [0, 1]
            }),
        ),
        (
            "cargo_fmt",
            json!({
                "project": "demo",
                "result_expectation": "failure"
            }),
        ),
        (
            "cargo_fmt",
            json!({
                "project": "demo",
                "check": false,
                "result_expectation": "observe"
            }),
        ),
        (
            "list_jobs",
            json!({
                "result_expectation": "observe"
            }),
        ),
    ];

    for (tool, arguments) in invalid {
        let error = parse_tool_call_with_recorder_metadata(tool, arguments).unwrap_err();
        assert!(
            error.contains("result_expectation")
                || error.contains("accepted_exit_codes")
                || error.contains("result expectation"),
            "{tool}: {error}"
        );
    }
}

#[test]
fn from_tool_name_rejects_removed_failure_kind_alias_as_tool_input() {
    let error = parse_tool_call_with_recorder_metadata(
        "list_jobs",
        json!({
            "expected_failure": true,
            "test_expect_failure_kind": "job_not_found",
            "assertion_name": "removed alias"
        }),
    )
    .unwrap_err();
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("[field]"), "{error}");
    assert!(error.contains("see input schema"), "{error}");
    assert!(!error.contains("test_expect_failure_kind"), "{error}");
    assert!(!error.contains("job_not_found"), "{error}");
}
