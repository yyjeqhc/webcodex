use super::support::*;
use crate::tool_runtime::cargo::parse_cargo_test_run_metadata;
use crate::tool_runtime::sessions::{SessionStore, SessionTransport, MAX_VALIDATION_EXCERPT_CHARS};
use crate::tool_runtime::validation_events::{
    validation_kind_for_tool, validation_summary_for_session,
};
use crate::tool_runtime::validation_parser::{
    NO_STABLE_DIAGNOSTICS_REASON, PARSER_KIND, PARSER_VERSION,
    VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
};
use crate::tool_runtime::{SessionMode, ToolCall};
use serde_json::{json, Value};

#[test]
fn validation_like_tool_calls_are_classified_correctly() {
    for (tool_name, validation_kind) in [
        ("cargo_fmt", "format"),
        ("cargo_check", "check"),
        ("cargo_test", "test"),
        ("validate_patch", "patch_preflight"),
        ("apply_patch_checked", "patch_apply_checked"),
    ] {
        assert_eq!(validation_kind_for_tool(tool_name), Some(validation_kind));
    }

    assert_eq!(validation_kind_for_tool("run_shell"), None);
    assert_eq!(validation_kind_for_tool("read_file"), None);
}

#[test]
fn validation_summary_is_unavailable_without_validation_events() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "read_file",
        json!({"project": "agent:eval:demo", "path": "src/lib.rs"}),
        true,
        json!({}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["available"], false);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["reason"], "no_validation_tool_invoked");
    assert_eq!(validation["source"], "session_ledger");
    assert_eq!(validation["events_total"], 0);
    assert!(validation["events"].as_array().unwrap().is_empty());
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(validation["parser"]["kind"], PARSER_KIND);
    assert_eq!(validation["parser"]["version"], PARSER_VERSION);
    assert_eq!(
        validation["parser"]["source"],
        "bounded_validation_metadata"
    );
    assert_eq!(validation["parser"]["raw_output_exposed"], false);
    assert_eq!(
        validation["parser"]["limitations"],
        json!([
            "bounded validation excerpts only",
            "deterministic evidence extraction; no root-cause inference",
            "no full stdout/stderr bodies; incomplete excerpts may omit fields or report unknown"
        ])
    );
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert!(validation.get("latest_success").is_none());
    assert!(validation.get("latest_failure").is_none());
    assert!(validation.get("latest").is_some());
    assert!(validation["latest"].is_null());
    assert_eq!(validation["latest_status"], "not_run");
    assert_eq!(validation["historical_failures"]["count"], 0);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
}

#[test]
fn cargo_check_success_produces_validation_event() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({
            "project": "agent:eval:demo",
            "cwd": null,
            "all_targets": true,
            "timeout_secs": 60,
        }),
        true,
        json!({"exit_code": 0}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let event = &validation["latest_success"];

    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "passed");
    assert!(validation["reason"].is_null());
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["successes"], 1);
    assert_eq!(validation["failures"], 0);
    assert_eq!(event["tool_name"], "cargo_check");
    assert_eq!(event["validation_kind"], "check");
    assert_eq!(event["success"], true);
    assert_eq!(event["exit_code"], 0);
    assert_eq!(event["summary"], "cargo_check succeeded");
    assert_eq!(event["input_summary"]["project"], "agent:eval:demo");
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert!(event.get("diagnostics").is_none());
}

#[test]
fn validation_output_metadata_without_stable_diagnostics_makes_parser_available() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let diagnostics = &validation["latest_success"]["diagnostics"];

    assert_eq!(validation["available"], true);
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["parser"]["available"], true);
    assert_eq!(validation["status"], "passed");
    assert!(validation["reason"].is_null());
    assert_eq!(validation["parser"]["kind"], PARSER_KIND);
    assert_eq!(validation["parser"]["version"], 2);
    assert_eq!(
        validation["parser"]["source"],
        "bounded_validation_metadata"
    );
    assert_eq!(validation["parser"]["raw_output_exposed"], false);
    assert!(validation["parser"].get("reason").is_none());
    assert_eq!(diagnostics["available"], false);
    assert_eq!(diagnostics["parser"], PARSER_KIND);
    assert_eq!(diagnostics["reason"], NO_STABLE_DIAGNOSTICS_REASON);
    assert_no_raw_validation_output_fields(&validation, "validation summary");
}

#[test]
fn cargo_check_finished_event_records_safe_validation_output_summary() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let stderr_tail = format!(
        "{}\nAuthorization: Bearer supersecret\nerror[E0308]: mismatched types\n --> src/lib.rs:12:5\n",
        "x".repeat(MAX_VALIDATION_EXCERPT_CHARS + 200)
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({
            "exit_code": 101,
            "stdout": "full stdout body must not be ledgered",
            "stderr": "full stderr body must not be ledgered",
            "stdout_tail": "api_key=supersecret\nsafe stdout line\n",
            "stderr_tail": stderr_tail,
            "stdout_truncated": false,
            "stderr_truncated": true,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let finished = session
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();
    let output_summary = finished.validation_output_summary.as_ref().unwrap();
    let stdout_excerpt = output_summary["stdout_tail_excerpt"].as_str().unwrap();
    let stderr_excerpt = output_summary["stderr_tail_excerpt"].as_str().unwrap();

    assert_eq!(output_summary["tool_name"], "cargo_check");
    assert_eq!(
        output_summary["max_excerpt_chars"],
        MAX_VALIDATION_EXCERPT_CHARS
    );
    assert!(stdout_excerpt.contains("safe stdout line"));
    assert!(!stdout_excerpt.contains("supersecret"));
    assert!(stderr_excerpt.contains("error[E0308]"));
    assert!(stderr_excerpt.contains("--> src/lib.rs:12:5"));
    assert!(!stderr_excerpt.contains("Authorization"));
    assert!(!stderr_excerpt.contains("supersecret"));
    assert!(stdout_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert!(stderr_excerpt.chars().count() <= MAX_VALIDATION_EXCERPT_CHARS);
    assert_eq!(output_summary["stdout_truncated"], true);
    assert_eq!(output_summary["stderr_truncated"], true);

    let serialized = serde_json::to_string(finished).unwrap();
    for leaked in [
        "full stdout body must not be ledgered",
        "full stderr body must not be ledgered",
        "api_key=supersecret",
        "Authorization: Bearer supersecret",
    ] {
        assert!(
            !serialized.contains(leaked),
            "session event leaked unsafe validation output {leaked}: {serialized}"
        );
    }
    for raw_key in [
        "\"stdout\":",
        "\"stderr\":",
        "\"stdout_tail\":",
        "\"stderr_tail\":",
    ] {
        assert!(
            !serialized.contains(raw_key),
            "session event stored raw output key {raw_key}: {serialized}"
        );
    }
}

#[test]
fn validation_summary_wires_cargo_check_diagnostics_from_captured_excerpt() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let diagnostics = &validation["latest_failure"]["diagnostics"];

    assert_eq!(validation["parser"]["available"], true);
    assert_eq!(validation["parser"]["kind"], PARSER_KIND);
    assert!(validation["parser"].get("reason").is_none());
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["parser"], PARSER_KIND);
    assert_eq!(diagnostics["diagnostic_count"], 1);
    assert_eq!(diagnostics["returned_diagnostic_count"], 1);
    assert_eq!(diagnostics["diagnostics_truncated"], false);
    assert_eq!(diagnostics["invalid_diagnostics_omitted"], 0);
    assert_eq!(diagnostics["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(diagnostics["first_diagnostic"]["severity"], "error");
    assert_eq!(diagnostics["first_diagnostic"]["code"], "E0308");
    assert_eq!(diagnostics["first_diagnostic"]["file"], "src/lib.rs");
    assert_eq!(diagnostics["first_diagnostic"]["line"], 12);
    assert_eq!(diagnostics["first_diagnostic"]["column"], 5);
    assert_eq!(
        diagnostics["first_diagnostic"]["message"],
        "mismatched types"
    );
    assert_eq!(
        diagnostics["first_diagnostic"],
        diagnostics["diagnostics"][0]
    );
    assert_eq!(
        validation["latest_failure"]["failure_kind"],
        "compile_error"
    );
    assert_eq!(diagnostics["truncated"], false);
    assert_no_raw_validation_output_fields(&validation, "validation summary");
}

#[test]
fn validation_summary_wires_cargo_test_summary_from_captured_excerpt() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "running 1 test\ntest tests::fails ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let diagnostics = &validation["latest_failure"]["diagnostics"];

    assert_eq!(validation["parser"]["available"], true);
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["diagnostic_count"], 1);
    assert_eq!(diagnostics["test_summary"]["passed"], 0);
    assert_eq!(diagnostics["test_summary"]["failed"], 1);
    assert_eq!(diagnostics["test_summary"]["ignored"], 0);
    assert_eq!(diagnostics["first_failed_test"], "tests::fails");
    assert_eq!(diagnostics["failed_tests"], json!(["tests::fails"]));
    assert_eq!(
        diagnostics["failed_test_details"][0]["name"],
        "tests::fails"
    );
    assert_eq!(
        diagnostics["failed_test_details"][0]["failure_kind"],
        "unknown"
    );
    assert_eq!(diagnostics["failed_tests_truncated"], false);
    assert_eq!(diagnostics["truncated"], false);
    assert_eq!(validation["latest_failure"]["failure_kind"], "test_failure");
}

#[test]
fn validation_failure_kind_prefers_safe_metadata_and_specific_evidence() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    for (tool, output, expected) in [
        (
            "cargo_test",
            json!({
                "exit_code": 101,
                "stdout_tail": "",
                "stderr_tail": "error[E0308]: mismatched types\n --> src/lib.rs:2:1\n",
                "stdout_truncated": false,
                "stderr_truncated": false
            }),
            "compile_error",
        ),
        (
            "cargo_test",
            json!({
                "exit_code": -1,
                "failure_kind": "timeout",
                "stdout_tail": "test tests::name ... FAILED\n",
                "stderr_tail": "",
                "stdout_truncated": false,
                "stderr_truncated": false
            }),
            "timeout",
        ),
        (
            "cargo_fmt",
            json!({
                "exit_code": 1,
                "stdout_tail": "Diff in src/lib.rs:1:\n-old\n+new\n",
                "stderr_tail": "",
                "stdout_truncated": false,
                "stderr_truncated": false
            }),
            "format_diff",
        ),
        (
            "cargo_check",
            json!({
                "exit_code": 2,
                "stdout_tail": "",
                "stderr_tail": "process exited without diagnostics\n",
                "stdout_truncated": false,
                "stderr_truncated": false
            }),
            "process_exit",
        ),
    ] {
        record_finished_tool(
            &store,
            &session.session_id,
            tool,
            json!({"project": "agent:eval:demo"}),
            false,
            output,
        );
        let summary = store.summary(&session.session_id, Some(100)).unwrap();
        let validation = validation_summary_for_session(&summary);
        assert_eq!(validation["latest"]["failure_kind"], expected, "{tool}");
    }
}

#[test]
fn zero_tests_success_is_not_classified_as_test_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
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

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["latest"]["failure_kind"], "unknown");
}

#[test]
fn validation_summary_exposes_failed_tests_on_latest_and_latest_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "test tests::first ... FAILED\n\
        test tests::second ... FAILED\n\
        test tests::third ... FAILED\n\
        test result: FAILED. 7 passed; 3 failed; 1 ignored\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let expected_names = json!(["tests::first", "tests::second", "tests::third"]);

    for path in ["latest", "latest_failure"] {
        let diagnostics = &validation[path]["diagnostics"];
        assert_eq!(diagnostics["available"], true, "{path}");
        assert_eq!(diagnostics["diagnostic_count"], 3, "{path}");
        assert_eq!(diagnostics["failed_tests"], expected_names, "{path}");
        assert_eq!(diagnostics["first_failed_test"], "tests::first", "{path}");
        assert_eq!(diagnostics["failed_tests_truncated"], false, "{path}");
        assert_eq!(diagnostics["test_summary"]["failed"], 3, "{path}");
        assert_eq!(diagnostics["truncated"], false, "{path}");
    }
}

#[test]
fn cargo_test_run_metadata_sums_mixed_rust_test_harness_sections() {
    let metadata = parse_cargo_test_run_metadata(
        "running 0 tests\n\
         test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\n\
         running 1 test\n\
         test archived_items_do_not_count_toward_total_quantity ... ok\n\
         test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    );

    assert_eq!(metadata.tests_detected, true);
    assert_eq!(metadata.tests_run_count, Some(1));
    assert_eq!(metadata.zero_tests_run, Some(false));
}

#[test]
fn validation_summary_exposes_cargo_test_zero_tests_metadata() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 0,
            "zero_tests_run": true
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let event = &validation["latest_success"];

    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["cargo_test_zero_tests_run"], true);
    assert_eq!(validation["historical_failures"]["count"], 0);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
    assert_eq!(event["tool_name"], "cargo_test");
    assert_eq!(event["tests_detected"], true);
    assert_eq!(event["tests_run_count"], 0);
    assert_eq!(event["zero_tests_run"], true);
    assert_no_raw_validation_output_fields(&validation, "validation summary");
}

#[test]
fn run_shell_output_tail_does_not_create_validation_metadata_or_events() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "run_shell",
        json!({"project": "agent:eval:demo", "command": "cargo check"}),
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "error[E0308]: must not classify shell output\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let finished = session
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap();
    assert!(finished.validation_output_summary.is_none());
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["available"], false);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["reason"], "no_validation_tool_invoked");
    assert_eq!(validation["events_total"], 0);
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
}

#[test]
fn patch_validation_tools_are_classified_as_patch_validation() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "validate_patch",
        json!({
            "project": "agent:eval:demo",
            "patch_present": true,
            "deny_sensitive_paths": true,
        }),
        true,
        json!({"can_apply": true}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "apply_patch_checked",
        json!({
            "project": "agent:eval:demo",
            "patch_present": true,
            "deny_sensitive_paths": true,
        }),
        true,
        json!({}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let events = validation["events"].as_array().unwrap();

    assert_eq!(validation["available"], true);
    assert_eq!(validation["events_total"], 2);
    assert_eq!(events[0]["tool_name"], "validate_patch");
    assert_eq!(events[0]["validation_kind"], "patch_preflight");
    assert_eq!(events[1]["tool_name"], "apply_patch_checked");
    assert_eq!(events[1]["validation_kind"], "patch_apply_checked");
}

#[test]
fn latest_success_and_failure_follow_session_ledger_order() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101, "stdout": "omitted", "stderr": "omitted"}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0, "stdout": "omitted", "stderr": "omitted"}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "mixed");
    assert!(validation["reason"].is_null());
    assert_eq!(validation["events_total"], 3);
    assert_eq!(validation["successes"], 1);
    assert_eq!(validation["failures"], 2);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest_success"]["exit_code"], 0);
    assert_eq!(validation["latest_failure"]["tool_name"], "cargo_test");
    assert_eq!(validation["latest_failure"]["exit_code"], 101);
    assert!(validation["latest_failure"].get("diagnostics").is_none());
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert_no_raw_validation_output_fields(&validation, "validation summary");
}

#[test]
fn failed_validation_followed_by_success_marks_historical_failure_resolved() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], true);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
    assert_eq!(validation["latest"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest"]["success"], true);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest_failure"]["tool_name"], "cargo_test");
}

#[test]
fn failed_cargo_test_followed_by_zero_tests_remains_historically_unresolved() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 0,
            "zero_tests_run": true
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["latest"]["tool_name"], "cargo_test");
    assert_eq!(validation["latest"]["zero_tests_run"], true);
    assert_eq!(validation["cargo_test_zero_tests_run"], true);
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
}

#[test]
fn normal_success_after_zero_tests_resolves_historical_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 0,
            "zero_tests_run": true
        }),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], true);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
}

#[test]
fn successful_validation_followed_by_failure_marks_historical_failure_unresolved() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "failed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
    assert_eq!(validation["latest"]["tool_name"], "cargo_test");
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest_failure"]["tool_name"], "cargo_test");
}

#[tokio::test]
async fn finish_coding_task_validation_available_when_ledger_has_validation_events() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    commit_file(tmp.path(), "Cargo.toml", cargo_toml(), "add cargo manifest");
    commit_file(
        tmp.path(),
        "src/lib.rs",
        "pub fn value() -> i32 { 1 }\n",
        "add lib",
    );
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "validation-finish", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let start = runtime
        .dispatch_with_auth(
            ToolCall::StartCodingTask {
                project: project.clone(),
                title: Some("validation finish".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                include_runtime_status: Some(false),
                compact_startup: false,
                include_git: Some(false),
                include_recent_commits: Some(false),
                include_rules: Some(false),
                include_tool_manifest: Some(false),
                tool_manifest_intent: None,
                tool_manifest_categories: None,
                tool_manifest_limit: None,
                bind_current: false,
            },
            Some(&auth),
        )
        .await;
    assert!(start.success, "{:?}", start.error);
    let session_id = start.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let check_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoCheck {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        all_targets: Some(true),
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        timeout_secs: Some(60),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "validation-finish")
        .await
        .expect("cargo_check should enqueue an agent shell request");
    assert!(req.command.contains("cargo check --all-targets"));
    complete_patch_agent_request(&runtime, "validation-finish", &req.request_id, 0, "", "").await;
    let check = check_task.await.unwrap();
    assert!(check.success, "{:?}", check.error);
    assert_eq!(check.output["permission"]["required"], true);
    assert_eq!(check.output["permission"]["status"], "auto_approved");
    assert_eq!(check.output["permission"]["risk"], "validation");

    let test_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: None,
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        timeout_secs: Some(60),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "validation-finish")
        .await
        .expect("cargo_test should enqueue an agent shell request");
    assert!(req.command.contains("cargo test"));
    complete_patch_agent_request(
        &runtime,
        "validation-finish",
        &req.request_id,
        101,
        "running 1 test\n",
        "test failure details stay out of validation summary\n",
    )
    .await;
    let test = test_task.await.unwrap();
    assert!(!test.success);

    let finish_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "validation-finish")
        .await
        .expect("finish_coding_task should inspect changes through the agent");
    assert!(req.command.contains("git status --porcelain=v1 -b"));
    complete_patch_agent_request(
        &runtime,
        "validation-finish",
        &req.request_id,
        0,
        "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add lib\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n",
        "",
    )
    .await;
    let finish = finish_task.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);

    let validation = &finish.output["validation"];
    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "mixed");
    assert!(validation["reason"].is_null());
    assert_eq!(validation["source"], "session_ledger");
    assert_eq!(validation["events_total"], 2);
    assert_eq!(validation["successes"], 1);
    assert_eq!(validation["failures"], 1);
    assert_eq!(validation["latest_success"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest_success"]["validation_kind"], "check");
    assert_eq!(validation["latest_success"]["exit_code"], 0);
    assert_eq!(
        validation["latest_success"]["summary"],
        "cargo_check succeeded"
    );
    assert_eq!(validation["latest_failure"]["tool_name"], "cargo_test");
    assert_eq!(validation["latest_failure"]["validation_kind"], "test");
    assert_eq!(validation["latest_failure"]["exit_code"], 101);
    assert_eq!(validation["latest_failure"]["summary"], "cargo_test failed");
    assert_eq!(validation["parser"]["available"], true);
    assert_eq!(validation["parser"]["kind"], PARSER_KIND);
    assert!(validation["parser"].get("reason").is_none());
    assert_eq!(
        validation["latest_failure"]["diagnostics"]["available"],
        false
    );
    assert_eq!(
        validation["latest_failure"]["diagnostics"]["reason"],
        NO_STABLE_DIAGNOSTICS_REASON
    );
    assert_no_raw_validation_output_fields(validation, "finish validation summary");

    let handoff = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: session_id.clone(),
            project: None,
            include_workspace: Some(false),
            include_checkpoints: Some(false),
            include_validation: Some(true),
            summary_only: false,
            limit: None,
        })
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(
        handoff.output["validation"], finish.output["validation"],
        "handoff validation should match finish_coding_task validation for the same session ledger"
    );
    assert_no_raw_validation_output_fields(
        &handoff.output["validation"],
        "handoff validation summary",
    );

    let handoff_compact = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: session_id.clone(),
            project: None,
            include_workspace: Some(false),
            include_checkpoints: Some(false),
            include_validation: Some(true),
            summary_only: true,
            limit: None,
        })
        .await;
    assert!(handoff_compact.success, "{:?}", handoff_compact.error);
    assert_eq!(
        handoff_compact.output["validation"], finish.output["validation"],
        "summary_only handoff must preserve the full structured validation evidence"
    );

    let finish_compact_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "validation-finish")
        .await
        .expect("summary-only finish should inspect changes through the agent");
    complete_patch_agent_request(
        &runtime,
        "validation-finish",
        &req.request_id,
        0,
        "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add lib\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n",
        "",
    )
    .await;
    let finish_compact = finish_compact_task.await.unwrap();
    assert!(finish_compact.success, "{:?}", finish_compact.error);
    assert_eq!(
        finish_compact.output["validation"], finish.output["validation"],
        "summary_only finish must preserve the full structured validation evidence"
    );
    assert_no_raw_validation_output_fields(
        &finish_compact.output["validation"],
        "summary-only finish validation summary",
    );
}

fn record_finished_tool(
    store: &SessionStore,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    success: bool,
    output: Value,
) {
    let start = store.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &arguments,
    );
    let error = (!success).then_some("tool failed");
    store.record_tool_call_finished(start, success, &output, error, None);
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

fn cargo_toml() -> &'static str {
    "[package]\nname = \"validation-finish\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"
}
