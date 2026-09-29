use super::*;
use serde_json::json;

#[test]
fn validation_audit_keeps_bounded_counts_and_assertion_without_payloads() {
    let output = json!({
        "execution_state": "completed",
        "command_started": true,
        "command_completed": true,
        "command_ok": true,
        "passed": true,
        "exit_code": 0,
        "tests_detected": true,
        "tests_run_count": 12,
        "tests_passed": 12,
        "tests_failed": 0,
        "zero_tests_run": false,
        "require_tests": true,
        "no_run": false,
        "test_count_assertion": {
            "minimum_tests": 1,
            "actual_tests_run": 12,
            "status": "passed",
            "reason_code": "minimum_satisfied",
            "evidence_reason_code": "complete_summary"
        },
        "source_state": {
            "freshness": "unproven",
            "observed_mutation_fence": "uncrossed",
            "source_fence": "PRIVATE_FENCE"
        },
        "command_summary": "PRIVATE_COMMAND",
        "stdout_tail": "PRIVATE_STDOUT",
        "stderr_tail": "PRIVATE_STDERR",
        "diagnostics": {"diagnostics":[{"message":"PRIVATE_DIAGNOSTIC"}]}
    });
    let audit = canonical_execution_audit_result_for_tool("cargo_test", &output);
    assert_eq!(audit["execution_state"], "completed");
    assert_eq!(audit["tests_run_count"], 12);
    assert_eq!(audit["tests_passed"], 12);
    assert_eq!(audit["tests_failed"], 0);
    assert_eq!(audit["zero_tests_run"], false);
    assert_eq!(audit["require_tests"], true);
    assert_eq!(audit["test_count_assertion"]["status"], "passed");
    assert_eq!(audit["source_state"]["freshness"], "unproven");
    assert_eq!(
        audit["source_state"]["observed_mutation_fence"],
        "uncrossed"
    );
    let serialized = serde_json::to_string(&audit).unwrap();
    for private in [
        "PRIVATE_COMMAND",
        "PRIVATE_STDOUT",
        "PRIVATE_STDERR",
        "PRIVATE_DIAGNOSTIC",
        "PRIVATE_FENCE",
    ] {
        assert!(!serialized.contains(private));
    }
    assert!(audit.get("command_summary").is_none());
    assert!(audit.get("diagnostics").is_none());
}

#[test]
fn execution_audit_keeps_bounded_failure_and_recovery_classification() {
    let audit = canonical_execution_audit_result_for_tool(
        "run_shell",
        &json!({
            "execution_state":"outcome_unknown",
            "command_started":true,
            "command_completed":false,
            "command_ok":false,
            "failure_kind":"outcome_unknown",
            "recovery_kind":"reconcile",
            "stdout_tail":"PRIVATE_OUTPUT"
        }),
    );
    assert_eq!(audit["failure_kind"], "outcome_unknown");
    assert_eq!(audit["recovery_kind"], "reconcile");
    assert!(!audit.to_string().contains("PRIVATE_OUTPUT"));
}

#[test]
fn text_validation_audit_keeps_warning_error_counts() {
    let audit = canonical_execution_audit_result_for_tool(
        "cargo_check",
        &json!({
            "execution_state":"completed",
            "command_started":true,
            "command_completed":true,
            "passed":true,
            "warnings_count":3,
            "errors_count":0,
            "stdout_tail":"PRIVATE_OUTPUT"
        }),
    );
    assert_eq!(audit["warnings_count"], 3);
    assert_eq!(audit["errors_count"], 0);
    assert!(!audit.to_string().contains("PRIVATE_OUTPUT"));
}
