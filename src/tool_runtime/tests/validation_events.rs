use super::support::*;
use crate::tool_runtime::cargo::parse_cargo_test_run_metadata;
use crate::tool_runtime::sessions::{
    self, SessionGuards, SessionStore, SessionTransport, MAX_VALIDATION_EXCERPT_CHARS,
};
use crate::tool_runtime::validation_events::{
    validation_kind_for_tool, validation_summary_for_session,
};
use crate::tool_runtime::validation_parser::{
    NO_STABLE_DIAGNOSTICS_REASON, PARSER_KIND, PARSER_VERSION,
    VALIDATION_OUTPUT_METADATA_ABSENT_REASON,
};
use crate::tool_runtime::{ExecutionPurpose, ExecutionShell, SessionMode, ToolCall};
use serde_json::{json, Value};

#[test]
fn validation_like_tool_calls_are_classified_correctly() {
    for (tool_name, validation_kind) in [
        ("cargo_fmt", "format"),
        ("cargo_check", "check"),
        ("cargo_test", "test"),
    ] {
        assert_eq!(validation_kind_for_tool(tool_name), Some(validation_kind));
    }

    assert_eq!(validation_kind_for_tool("run_shell"), None);
    assert_eq!(validation_kind_for_tool("apply_unified_diff"), None);
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
    assert!(event.get("input_summary").is_none());
    assert!(event["identity"]
        .as_str()
        .is_some_and(|identity| identity.starts_with("target:")));
    assert_eq!(event["execution_source"], "cargo_check");
    assert_eq!(event["purpose"], "validation");
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
    assert_eq!(validation["parser"]["version"], 3);
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
    assert_eq!(diagnostics["diagnostics"][0]["severity"], "error");
    assert_eq!(diagnostics["diagnostics"][0]["code"], "E0308");
    assert_eq!(diagnostics["diagnostics"][0]["file"], "src/lib.rs");
    assert_eq!(diagnostics["diagnostics"][0]["line"], 12);
    assert_eq!(diagnostics["diagnostics"][0]["column"], 5);
    assert_eq!(diagnostics["diagnostics"][0]["message"], "mismatched types");
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
    assert_eq!(
        diagnostics["failed_test_details"][0]["name"],
        "tests::fails"
    );
    assert_eq!(
        diagnostics["failed_test_details"][0]["failure_kind"],
        "unknown"
    );
    assert_eq!(diagnostics["failed_test_details_truncated"], false);
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
fn validation_summary_exposes_failed_test_details_on_latest_and_latest_failure() {
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
    let expected_names = ["tests::first", "tests::second", "tests::third"];

    for path in ["latest", "latest_failure"] {
        let diagnostics = &validation[path]["diagnostics"];
        assert_eq!(diagnostics["available"], true, "{path}");
        assert_eq!(diagnostics["diagnostic_count"], 3, "{path}");
        let names: Vec<&str> = diagnostics["failed_test_details"]
            .as_array()
            .unwrap()
            .iter()
            .map(|detail| detail["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, expected_names, "{path}");
        assert_eq!(
            diagnostics["failed_test_details_truncated"], false,
            "{path}"
        );
        assert_eq!(diagnostics["test_summary"]["failed"], 3, "{path}");
        assert_eq!(diagnostics["truncated"], false, "{path}");
    }
}

#[test]
fn cargo_test_run_metadata_counts_only_executed_tests() {
    let metadata = parse_cargo_test_run_metadata(
        "running 7 tests\n\
         test result: ok. 1 passed; 0 failed; 3 ignored; 2 measured; 4 filtered out\n",
    );

    assert!(metadata.tests_detected);
    assert_eq!(metadata.tests_run_count, Some(1));
    assert_eq!(metadata.zero_tests_run, Some(false));

    let measured_only = parse_cargo_test_run_metadata(
        "running 2 tests\n\
         test result: ok. 0 passed; 0 failed; 0 ignored; 2 measured; 0 filtered out\n",
    );
    assert_eq!(measured_only.tests_run_count, Some(0));
    assert_eq!(measured_only.zero_tests_run, Some(true));
}

#[test]
fn cargo_test_run_metadata_reports_ignored_only_as_zero_executed() {
    let metadata = parse_cargo_test_run_metadata(
        "running 1 test\n\
         test ignored_only ... ignored\n\
         test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n",
    );

    assert!(metadata.tests_detected);
    assert_eq!(metadata.tests_run_count, Some(0));
    assert_eq!(metadata.zero_tests_run, Some(true));
}

#[test]
fn cargo_test_run_metadata_counts_failed_tests_as_executed() {
    let metadata = parse_cargo_test_run_metadata(
        "running 3 tests\n\
         test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
    );

    assert!(metadata.tests_detected);
    assert_eq!(metadata.tests_run_count, Some(3));
    assert_eq!(metadata.zero_tests_run, Some(false));
}

#[test]
fn cargo_test_run_metadata_aggregates_complete_harness_summaries() {
    let metadata = parse_cargo_test_run_metadata(
        "running 5 tests\n\
         test result: ok. 2 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out\n\n\
         running 6 tests\n\
         test result: FAILED. 1 passed; 1 failed; 4 ignored; 0 measured; 0 filtered out\n",
    );

    assert!(metadata.tests_detected);
    assert_eq!(metadata.tests_run_count, Some(4));
    assert_eq!(metadata.tests_passed, Some(3));
    assert_eq!(metadata.tests_failed, Some(1));
    assert_eq!(metadata.zero_tests_run, Some(false));
}

#[test]
fn cargo_test_run_metadata_keeps_positive_aggregate_when_last_harness_is_zero() {
    let metadata = parse_cargo_test_run_metadata(
        "running 2788 tests\n\
         test result: ok. 2788 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\n\
         running 0 tests\n\
         test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    );
    assert_eq!(metadata.tests_passed, Some(2788));
    assert_eq!(metadata.tests_failed, Some(0));
    assert_eq!(metadata.tests_run_count, Some(2788));
    assert_eq!(metadata.zero_tests_run, Some(false));
}

#[test]
fn cargo_test_run_metadata_does_not_guess_from_running_or_partial_summary() {
    for output in [
        "running 10 tests\n",
        "running 10 tests\ntest result: ok. 10 passed;\n",
        "test result: ok. 10 passed; 0 failed\n",
        "test result: unknown. 10 passed; 0 failed; 0 ignored\n",
        "test result: FAILED. 2 passed; 1 failed; 0 ignored\n\
         test result: ok. 4 passed;\n",
    ] {
        let metadata = parse_cargo_test_run_metadata(output);
        assert!(metadata.tests_detected, "{output:?}");
        assert_eq!(metadata.tests_run_count, None, "{output:?}");
        assert_eq!(metadata.tests_passed, None, "{output:?}");
        assert_eq!(metadata.tests_failed, None, "{output:?}");
        assert_eq!(metadata.zero_tests_run, None, "{output:?}");
    }
}

#[test]
fn cargo_test_session_metadata_preserves_explicit_unproven_count_authority() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo", "filter": "focused"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "test result: ok. 10 passed; 0 failed\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": null,
            "zero_tests_run": null
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let event = &validation["latest"];

    assert_eq!(event["success"], true);
    assert_eq!(event["tests_detected"], true);
    assert!(event["tests_run_count"].is_null());
    assert!(event["zero_tests_run"].is_null());
    assert!(event["detected_summary"]["tests_run_count"].is_null());
    assert!(event["detected_summary"]["zero_tests_run"].is_null());
    assert_eq!(event["diagnostics"]["test_summary"]["passed"], 10);
    assert_eq!(event["diagnostics"]["test_summary"]["failed"], 0);
}

#[test]
fn legacy_persisted_cargo_test_metadata_absence_uses_diagnostics_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 10);
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo", "filter": "legacy"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "test result: ok. 2 passed; 0 failed; 0 ignored\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 2,
            "zero_tests_run": false
        }),
    );
    store.flush_persistence();
    drop(store);

    let mut ledger_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let events = ledger_json["sessions"][0]["events"].as_array_mut().unwrap();
    let finished = events
        .iter_mut()
        .find(|event| event["kind"] == "tool_call_finished" && event["tool_name"] == "cargo_test")
        .unwrap();
    let output_summary = finished["validation_output_summary"]
        .as_object_mut()
        .unwrap();
    output_summary.remove("tests_run_count");
    output_summary.remove("zero_tests_run");
    std::fs::write(&ledger, serde_json::to_vec(&ledger_json).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 10);
    let session = restored.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    let event = &validation["latest"];

    assert_eq!(event["tests_detected"], true);
    assert_eq!(event["tests_run_count"], 2);
    assert_eq!(event["zero_tests_run"], false);
    assert_eq!(event["diagnostics"]["test_summary"]["passed"], 2);
    assert_eq!(event["diagnostics"]["test_summary"]["failed"], 0);
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
fn run_shell_without_declared_validation_purpose_is_not_validation_evidence() {
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
    assert!(finished.validation_output_summary.is_some());
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
fn unified_diff_mutation_does_not_create_validation_lifecycle() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "apply_unified_diff",
        json!({
            "project": "agent:eval:demo",
            "diff_present": true,
            "deny_sensitive_paths": true,
        }),
        true,
        json!({"applied": true, "can_apply": true}),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["available"], false);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["events_total"], 0);
    assert_eq!(validation["reason"], "no_validation_tool_invoked");
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
fn different_validation_identity_success_does_not_resolve_failure() {
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
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
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
fn failed_cargo_test_followed_by_unproven_success_remains_historically_unresolved() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let arguments = json!({"project": "agent:eval:demo", "filter": "focused"});
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        arguments.clone(),
        false,
        json!({"exit_code": 101}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        arguments,
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "test result: ok. 10 passed; 0 failed\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": null,
            "zero_tests_run": null
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["latest"]["success"], true);
    assert!(validation["latest"]["tests_run_count"].is_null());
    assert!(validation["latest"]["zero_tests_run"].is_null());
    assert_eq!(
        validation["latest"]["diagnostics"]["test_summary"]["passed"],
        10
    );
    assert_eq!(
        validation["latest"]["diagnostics"]["test_summary"]["failed"],
        0
    );
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
}

#[test]
fn different_check_success_after_zero_tests_does_not_resolve_test_failure() {
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
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
}

#[test]
fn same_validation_identity_success_resolves_failure_without_deleting_history() {
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
            "stdout_tail": "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 1,
            "zero_tests_run": false
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);

    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(
        validation["resolved_failures"]["events"][0]["unresolved_failure"],
        false
    );
    assert_eq!(validation["events_total"], 2);
}

#[test]
fn generic_test_success_without_structured_counts_resolves_same_identity_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let input = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "run_process",
        &json!({
            "project": "agent:eval:demo",
            "executable": "cargo",
            "args": ["test", "focused", "-p", "webcodex"],
            "cwd": ".",
            "purpose": "test"
        }),
    );
    let identity = input["validation_target_id"]
        .as_str()
        .expect("generic validation identity")
        .to_string();
    assert_eq!(input["validation_tool"], "cargo_test");

    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        input.clone(),
        false,
        json!({
            "exit_code": 101,
            "purpose": "test",
            "execution_state": "completed",
            "stdout_tail": "generic harness failed\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false
        }),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        input,
        true,
        json!({
            "exit_code": 0,
            "purpose": "test",
            "execution_state": "completed",
            "stdout_tail": "generic harness passed\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false
        }),
    );

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let raw_finished = summary
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_finished" && event.tool_name == "run_process")
        .collect::<Vec<_>>();
    assert_eq!(raw_finished.len(), 2);
    assert_eq!(raw_finished[0].status.as_deref(), Some("failed"));
    assert_eq!(raw_finished[1].status.as_deref(), Some("succeeded"));

    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["events_total"], 2);
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], true);
    assert_eq!(validation["historical_failures"]["unresolved"], false);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(validation["latest"]["execution_source"], "run_process");
    assert_eq!(validation["latest"]["validation_kind"], "test");
    assert_eq!(validation["latest"]["identity"], identity);
    assert!(validation["latest"]["tests_run_count"].is_null());
    assert!(validation["latest"]["zero_tests_run"].is_null());
}

#[test]
fn proven_generic_cargo_test_resolves_same_generic_and_structured_target_only() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let generic_input = json!({
        "project": "agent:eval:demo", "executable": "cargo",
        "args": ["test", "focused", "-p", "webcodex"], "cwd": ".", "purpose": "test"
    });
    let generic_input = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "run_process",
        &generic_input,
    );
    let failed_output = json!({
        "exit_code": 101, "purpose": "test",
        "stdout_tail": "running 1 test\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
        "tests_detected": true, "tests_run_count": 1, "zero_tests_run": false
    });
    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        generic_input.clone(),
        false,
        failed_output.clone(),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        generic_input.clone(),
        true,
        json!({
            "exit_code": 0, "purpose": "test",
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "zero_tests_run": false
        }),
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(50)).unwrap());
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);

    assert!(validation["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| {
            event["identity"]
                .as_str()
                .is_some_and(|identity| identity.starts_with("target:"))
        }));

    let cross = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &cross.session_id,
        "run_process",
        generic_input,
        false,
        failed_output,
    );
    record_finished_tool(
        &store,
        &cross.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo", "package": "webcodex", "filter": "different", "cwd": ".",
            "all_targets": false, "all_features": false, "no_default_features": false,
            "no_run": false, "features": null
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "zero_tests_run": false
        }),
    );
    let before_structured =
        validation_summary_for_session(&store.summary(&cross.session_id, Some(50)).unwrap());
    assert_eq!(before_structured["unresolved_failures"]["count"], 1);

    record_finished_tool(
        &store,
        &cross.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo", "package": "webcodex", "filter": "focused", "cwd": ".",
            "all_targets": false, "all_features": false, "no_default_features": false,
            "no_run": false, "features": null
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "zero_tests_run": false
        }),
    );
    let validation =
        validation_summary_for_session(&store.summary(&cross.session_id, Some(50)).unwrap());
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
}

#[test]
fn generic_validation_scope_and_complex_script_identity_fail_closed() {
    use crate::tool_runtime::tool_audit::{
        run_process_validation_identity, run_script_validation_identity,
    };
    let x = run_process_validation_identity(
        "cargo",
        &[
            "test".into(),
            "focused".into(),
            "-p".into(),
            "webcodex".into(),
        ],
        None,
        Some("."),
        Some("test"),
    )
    .unwrap();
    let y = run_process_validation_identity(
        "cargo",
        &[
            "test".into(),
            "other".into(),
            "-p".into(),
            "webcodex".into(),
        ],
        None,
        Some("."),
        Some("test"),
    )
    .unwrap();
    assert!(x.identity.starts_with("target:"));
    assert!(y.identity.starts_with("target:"));
    assert_ne!(x.identity, y.identity);
    let complex = run_script_validation_identity(
        "bash",
        "set -e; cargo test focused; echo done",
        &[],
        None,
        Some("."),
        Some("test"),
    )
    .unwrap();
    assert!(complex.identity.starts_with("command:"));
    assert!(complex.validation_tool.is_none());
}

#[test]
fn complex_run_script_failure_keeps_generic_identity_and_cannot_be_resolved_by_structured_pass() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let script = "set -e\ncargo test focused -p webcodex\necho done";
    let input = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "run_script",
        &json!({
            "project": "agent:eval:demo", "language": "bash", "script": script,
            "args": [], "stdin": null, "cwd": ".", "purpose": "test"
        }),
    );
    assert!(input["execution_identity"]
        .as_str()
        .is_some_and(|identity| identity.starts_with("command:")));
    assert!(input.get("validation_target_id").is_none());
    assert!(!serde_json::to_string(&input).unwrap().contains(script));
    record_finished_tool(
        &store,
        &session.session_id,
        "run_script",
        input,
        false,
        json!({
            "exit_code": 101, "purpose": "test",
            "stdout_tail": "running 1 test\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "tests_passed": 0,
            "tests_failed": 1, "zero_tests_run": false
        }),
    );
    let before =
        validation_summary_for_session(&store.summary(&session.session_id, Some(50)).unwrap());
    assert_eq!(before["unresolved_failures"]["count"], 1);
    assert_eq!(before["latest"]["tests_run_count"], 1);
    assert_eq!(before["latest"]["tests_passed"], 0);
    assert_eq!(before["latest"]["tests_failed"], 1);

    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo", "package": "webcodex", "filter": "focused", "cwd": ".",
            "all_targets": false, "all_features": false, "no_default_features": false,
            "no_run": false, "features": null
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "tests_passed": 1,
            "tests_failed": 0, "zero_tests_run": false
        }),
    );
    let after =
        validation_summary_for_session(&store.summary(&session.session_id, Some(50)).unwrap());
    assert_eq!(after["unresolved_failures"]["count"], 1);
    assert_eq!(after["resolved_failures"]["count"], 0);
}

#[test]
fn generic_job_failure_identity_survives_restart_and_resolves_with_structured_success() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 20);
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let input = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "run_process",
        &json!({
            "project": "agent:eval:demo", "executable": "cargo",
            "args": ["test", "focused", "-p", "webcodex"], "cwd": ".", "purpose": "test"
        }),
    );
    let target = input["validation_target_id"].as_str().unwrap().to_string();
    let terminal_output = json!({
        "exit_code": 101, "purpose": "test", "validation_tool": "cargo_test",
        "stdout_tail": "running 1 test\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
        "tests_detected": true, "tests_run_count": 1, "tests_passed": 0,
        "tests_failed": 1, "zero_tests_run": false
    });
    let terminal_summary = crate::tool_runtime::sessions::execution_output_summary_for_tool_result(
        "run_process",
        &terminal_output,
    );
    assert!(store.record_validation_job_terminal(
        &session.session_id,
        "job_generic_failed",
        &["job_generic_failed"],
        "run_process",
        Some("agent:eval:demo".to_string()),
        &target,
        None,
        "failed",
        Some(101),
        Some(false),
        Some(100),
        Some(110),
        Some(10_000),
        terminal_summary,
    ));
    let failed = validation_summary_for_session(&store.summary(&session.session_id, None).unwrap());
    assert_eq!(failed["unresolved_failures"]["count"], 1);
    assert_eq!(failed["latest"]["identity"], target);
    store.flush_persistence();
    drop(store);

    let restored = SessionStore::with_persistence(&ledger, 10, 20);
    record_finished_tool(
        &restored,
        &session.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo", "package": "webcodex", "filter": "focused", "cwd": ".",
            "all_targets": false, "all_features": false, "no_default_features": false,
            "no_run": false, "features": null
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 1, "tests_passed": 1,
            "tests_failed": 0, "zero_tests_run": false
        }),
    );
    let resolved =
        validation_summary_for_session(&restored.summary(&session.session_id, None).unwrap());
    assert_eq!(resolved["resolved_failures"]["count"], 1);
    assert_eq!(resolved["unresolved_failures"]["count"], 0);
}

#[test]
fn contradictory_legacy_cargo_counts_are_downgraded_to_unproven() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({"project": "agent:eval:demo", "filter": "focused"}),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "", "stdout_truncated": false, "stderr_truncated": false,
            "tests_detected": true, "tests_run_count": 0, "zero_tests_run": true
        }),
    );
    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(50)).unwrap());
    assert!(validation["latest"]["tests_run_count"].is_null());
    assert!(validation["latest"]["tests_passed"].is_null());
    assert!(validation["latest"]["tests_failed"].is_null());
    assert!(validation["latest"]["zero_tests_run"].is_null());
}

#[test]
fn validation_job_terminal_identity_survives_event_eviction_and_restart_without_refreshing_activity(
) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 4);
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let target = "target:aaaaaaaaaaaaaaaaaaaaaaaa";
    let job_id = "job_terminal_success";
    let retained = [job_id];
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({
            "project": "agent:eval:demo",
            "validation_target_id": target,
        }),
        false,
        json!({"exit_code": 101}),
    );
    let before_materialize = store.summary(&session.session_id, None).unwrap();
    let authoritative_finished_at = before_materialize.updated_at;
    // Force reconciliation wall-clock time into a later second. Synthetic Job
    // terminal evidence must still use the authoritative execution timestamp.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert!(store.record_validation_job_terminal(
        &session.session_id,
        job_id,
        &retained,
        "cargo_check",
        Some("agent:eval:demo".to_string()),
        target,
        None,
        "completed",
        Some(0),
        Some(true),
        Some(authoritative_finished_at.saturating_sub(1)),
        Some(authoritative_finished_at),
        Some(1000),
        None,
    ));

    let materialized = store.summary(&session.session_id, None).unwrap();
    assert_eq!(materialized.updated_at, authoritative_finished_at);
    let validation = validation_summary_for_session(&materialized);
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(
        materialized
            .events
            .iter()
            .filter(|event| event.kind == "validation_job_terminal")
            .count(),
        1
    );

    // Push well beyond this test store's four-event retention cap. The durable
    // materialization identity must outlive the evidence event itself while the
    // authoritative Job can still be a reconciliation candidate.
    for index in 0..3 {
        record_finished_tool(
            &store,
            &session.session_id,
            "read_file",
            json!({"project": "agent:eval:demo", "path": format!("src/{index}.rs")}),
            true,
            json!({}),
        );
    }
    let after_eviction = store.summary(&session.session_id, None).unwrap();
    assert!(after_eviction
        .events
        .iter()
        .all(|event| event.kind != "validation_job_terminal"));
    let events_total_before_repeat = after_eviction.events_total;
    let updated_at_before_repeat = after_eviction.updated_at;
    assert!(
        !store.record_validation_job_terminal(
            &session.session_id,
            job_id,
            &retained,
            "cargo_check",
            Some("agent:eval:demo".to_string()),
            target,
            None,
            "completed",
            Some(0),
            Some(true),
            Some(authoritative_finished_at.saturating_sub(1)),
            Some(authoritative_finished_at),
            Some(1000),
            None,
        ),
        "FIFO eviction must not make an authoritative terminal Job materializable again"
    );
    let after_repeat = store.summary(&session.session_id, None).unwrap();
    assert_eq!(after_repeat.events_total, events_total_before_repeat);
    assert_eq!(after_repeat.updated_at, updated_at_before_repeat);
    assert!(after_repeat
        .events
        .iter()
        .all(|event| event.kind != "validation_job_terminal"));

    store.flush_persistence();
    drop(store);
    let restored = SessionStore::with_persistence(&ledger, 10, 4);
    let restored_before_repeat = restored.summary(&session.session_id, None).unwrap();
    assert!(restored_before_repeat
        .events
        .iter()
        .all(|event| event.kind != "validation_job_terminal"));
    assert!(
        !restored.record_validation_job_terminal(
            &session.session_id,
            job_id,
            &retained,
            "cargo_check",
            Some("agent:eval:demo".to_string()),
            target,
            None,
            "completed",
            Some(0),
            Some(true),
            Some(authoritative_finished_at.saturating_sub(1)),
            Some(authoritative_finished_at),
            Some(1000),
            None,
        ),
        "restart within authoritative Job retention must preserve idempotence"
    );
    let restored_after_repeat = restored.summary(&session.session_id, None).unwrap();
    assert_eq!(
        restored_after_repeat.events_total,
        restored_before_repeat.events_total
    );
    assert_eq!(
        restored_after_repeat.updated_at,
        restored_before_repeat.updated_at
    );
}

#[test]
fn persisted_validation_job_materialization_ids_are_additive_sanitized_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 10, 10);
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    store.flush_persistence();
    drop(store);

    let mut ledger_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = ledger_json["sessions"][0].as_object_mut().unwrap();
    // Missing field remains valid for pre-feature ledgers.
    record.remove("materialized_validation_job_ids");
    std::fs::write(&ledger, serde_json::to_vec(&ledger_json).unwrap()).unwrap();
    let legacy = SessionStore::with_persistence(&ledger, 10, 10);
    assert!(legacy.summary(&session.session_id, None).is_some());
    legacy.flush_persistence();
    drop(legacy);

    let mut ledger_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let record = ledger_json["sessions"][0].as_object_mut().unwrap();
    let mut ids = (0..70)
        .map(|index| Value::String(format!("restored-job-{index:02}")))
        .collect::<Vec<_>>();
    ids.extend([
        Value::String("bad/id".to_string()),
        Value::String(" padded-job".to_string()),
        Value::String("duplicate-job".to_string()),
        Value::String("duplicate-job".to_string()),
    ]);
    record.insert(
        "materialized_validation_job_ids".to_string(),
        Value::Array(ids),
    );
    std::fs::write(&ledger, serde_json::to_vec(&ledger_json).unwrap()).unwrap();

    let restored = SessionStore::with_persistence(&ledger, 10, 10);
    record_finished_tool(
        &restored,
        &session.session_id,
        "read_file",
        json!({"project": "agent:eval:demo", "path": "src/sanitize.rs"}),
        true,
        json!({}),
    );
    restored.flush_persistence();
    drop(restored);
    let canonical: Value =
        serde_json::from_str(&std::fs::read_to_string(&ledger).unwrap()).unwrap();
    let ids = canonical["sessions"][0]["materialized_validation_job_ids"]
        .as_array()
        .unwrap();
    assert_eq!(
        ids.len(),
        crate::shell_protocol::JOB_INVENTORY_MAX_TERMINAL_JOBS
    );
    assert!(ids.iter().all(|value| {
        value
            .as_str()
            .is_some_and(crate::tool_runtime::helpers::is_safe_job_id)
    }));
    assert_eq!(ids.first().and_then(Value::as_str), Some("restored-job-07"));
    assert_eq!(ids.last().and_then(Value::as_str), Some("duplicate-job"));
    assert_eq!(
        ids.iter()
            .filter(|value| value.as_str() == Some("duplicate-job"))
            .count(),
        1
    );
}

#[test]
fn structured_validation_target_resolves_equivalent_semantic_arguments() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo",
            "cwd": "./crate/",
            "filter": "  oversized_result  ",
            "all_targets": true,
            "all_features": false,
            "no_default_features": false,
            "features": "  serde  ",
            "package": "  webcodex  ",
            "no_run": false,
            "timeout_secs": 30,
        }),
        false,
        json!({"exit_code": 101}),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        json!({
            "project": "agent:eval:demo",
            "cwd": "crate",
            "filter": "oversized_result",
            "all_targets": true,
            "all_features": false,
            "no_default_features": false,
            "features": "serde",
            "package": "webcodex",
            "no_run": false,
            "timeout_secs": 120,
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 1,
            "zero_tests_run": false
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    let identity = validation["resolved_failures"]["events"][0]["identity"]
        .as_str()
        .unwrap();
    assert!(identity.starts_with("target:"), "{identity}");
}

#[test]
fn cargo_test_target_identity_includes_effective_count_contract() {
    let target = |arguments| {
        super::super::tool_audit::structured_validation_target_identity("cargo_test", &arguments)
            .unwrap()
    };
    let require_one = target(json!({
        "cwd": ".",
        "filter": "focused",
        "require_tests": true
    }));
    let min_one = target(json!({
        "cwd": ".",
        "filter": "focused",
        "min_tests": 1
    }));
    let min_six = target(json!({
        "cwd": ".",
        "filter": "focused",
        "require_tests": true,
        "min_tests": 6
    }));
    let compatible_default = target(json!({
        "cwd": ".",
        "filter": "focused"
    }));
    assert_eq!(require_one, min_one);
    assert_ne!(require_one, min_six);
    assert_ne!(require_one, compatible_default);
}

#[test]
fn same_validation_identity_success_in_another_project_does_not_resolve_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:alpha".to_string()), None);
    let shared_target = json!({
        "cwd": "crate",
        "package": "pkg",
        "filter": "focus",
        "features": "feat"
    });

    let mut failed = shared_target.clone();
    failed["project"] = json!("agent:eval:alpha");
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        failed,
        false,
        json!({"exit_code": 101}),
    );

    let mut succeeded_elsewhere = shared_target;
    succeeded_elsewhere["project"] = json!("agent:eval:bravo");
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_test",
        succeeded_elsewhere,
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n",
            "stderr_tail": "",
        }),
    );

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(
        validation["unresolved_failures"]["events"][0]["project"],
        "agent:eval:alpha"
    );
}

#[test]
fn structured_validation_target_does_not_cross_semantic_scope() {
    let cases = [
        (
            "cwd",
            json!({"project":"agent:eval:demo","cwd":"crate_a","package":"pkg","filter":"focus","features":"feat"}),
            json!({"project":"agent:eval:demo","cwd":"crate_b","package":"pkg","filter":"focus","features":"feat"}),
        ),
        (
            "package",
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg_a","filter":"focus","features":"feat"}),
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg_b","filter":"focus","features":"feat"}),
        ),
        (
            "filter",
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg","filter":"focus_a","features":"feat"}),
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg","filter":"focus_b","features":"feat"}),
        ),
        (
            "features",
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg","filter":"focus","features":"feat_a"}),
            json!({"project":"agent:eval:demo","cwd":"crate","package":"pkg","filter":"focus","features":"feat_b"}),
        ),
    ];

    for (label, failed_args, successful_args) in cases {
        let store = SessionStore::default();
        let session = store.start_session(Some("agent:eval:demo".to_string()), None);
        record_finished_tool(
            &store,
            &session.session_id,
            "cargo_test",
            failed_args,
            false,
            json!({"exit_code": 101}),
        );
        record_finished_tool(
            &store,
            &session.session_id,
            "cargo_test",
            successful_args,
            true,
            json!({"exit_code": 0}),
        );

        let session = store.summary(&session.session_id, Some(50)).unwrap();
        let validation = validation_summary_for_session(&session);
        assert_eq!(
            validation["resolved_failures"]["count"], 0,
            "{label}: {validation}"
        );
        assert_eq!(
            validation["unresolved_failures"]["count"], 1,
            "{label}: {validation}"
        );
    }
}

#[test]
fn same_assertion_identity_resolves_only_its_own_failure() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    for (assertion_name, success, exit_code) in [
        ("release_check", false, 101),
        ("other_check", true, 0),
        ("release_check", true, 0),
    ] {
        record_finished_tool(
            &store,
            &session.session_id,
            "cargo_check",
            json!({
                "project": "agent:eval:demo",
                "assertion_name": assertion_name,
            }),
            success,
            json!({"exit_code": exit_code}),
        );
    }

    let session = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(
        validation["resolved_failures"]["events"][0]["identity"],
        crate::tool_runtime::tool_audit::assertion_validation_identity("release_check")
    );
}

#[test]
fn public_validation_assertion_label_hides_structured_and_unsafe_historical_metadata() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:label-safety".to_string()), None);
    let hidden_structured = "internal structured assertion";
    record_finished_tool(
        &store,
        &session.session_id,
        "cargo_check",
        json!({
            "project": "agent:eval:label-safety",
            "assertion_name": hidden_structured,
        }),
        true,
        json!({"exit_code": 0}),
    );
    let unsafe_legacy = "Bearer historical-validation-secret";
    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        json!({
            "project": "agent:eval:label-safety",
            "purpose": "test",
            "command": "legacy validation",
            "assertion_name": unsafe_legacy,
        }),
        false,
        json!({
            "exit_code": 1,
            "purpose": "test",
            "execution_state": "completed",
        }),
    );

    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(50)).unwrap());
    let events = validation["events"].as_array().unwrap();
    let structured = events
        .iter()
        .find(|event| event["tool_name"] == "cargo_check")
        .unwrap();
    let generic = events
        .iter()
        .find(|event| event["tool_name"] == "run_process")
        .unwrap();
    assert!(structured.get("assertion_name").is_none());
    assert!(generic.get("assertion_name").is_none());
    let serialized = validation.to_string();
    assert!(!serialized.contains(hidden_structured));
    assert!(!serialized.contains(unsafe_legacy));
}

#[test]
fn materialized_run_script_terminal_preserves_recoverable_assertion_label() {
    let store = SessionStore::default();
    let project = "agent:eval:script-terminal";
    let session = store.start_session(Some(project.to_string()), None);
    let assertion_name = "promoted script validation";
    let identity = crate::tool_runtime::tool_audit::assertion_validation_identity(assertion_name);
    let terminal_output = json!({
        "exit_code": 0,
        "purpose": "test",
        "execution_state": "completed",
        "language": "bash",
        "stdout_tail": "script validation passed\n",
        "stderr_tail": "",
        "stdout_truncated": false,
        "stderr_truncated": false,
    });
    let terminal_summary = crate::tool_runtime::sessions::execution_output_summary_for_tool_result(
        "run_script",
        &terminal_output,
    );
    assert!(store.record_validation_job_terminal(
        &session.session_id,
        "job_script_assertion",
        &["job_script_assertion"],
        "run_script",
        Some(project.to_string()),
        &identity,
        Some(assertion_name),
        "completed",
        Some(0),
        Some(true),
        Some(100),
        Some(110),
        Some(10_000),
        terminal_summary,
    ));

    let validation =
        validation_summary_for_session(&store.summary(&session.session_id, Some(20)).unwrap());
    let latest = &validation["latest"];
    assert_eq!(latest["execution_source"], "run_script");
    assert_eq!(latest["identity"], identity);
    assert_eq!(latest["assertion_name"], assertion_name);
}

#[tokio::test]
async fn run_shell_declared_validation_enters_unified_summary_with_shell_and_root_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "validation-shell", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("shell validation".to_string()));

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "cargo test focused".to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: Some(".".to_string()),
                        purpose: Some(ExecutionPurpose::Test),
                        shell: Some(ExecutionShell::Bash),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "validation-shell").await;
    assert_eq!(request.kind, "run_shell");
    assert!(request.command.starts_with("exec bash -c "));
    complete_patch_agent_request(
        &runtime,
        "validation-shell",
        &request.request_id,
        0,
        "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n",
        "",
    )
    .await;
    let execution = task.await.unwrap();
    assert!(execution.success, "{:?}", execution.error);
    assert_eq!(execution.output["cwd"], ".");
    assert_eq!(execution.output["shell"], "bash");
    assert_eq!(execution.output["purpose"], "test");

    let summary = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project,
                session_id: session.session_id,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(summary.success, "{:?}", summary.error);
    let event = &summary.output["validation"]["latest"];
    assert_eq!(event["execution_source"], "run_shell");
    assert_eq!(event["purpose"], "test");
    assert_eq!(event["validation_kind"], "test");
    assert_eq!(event["cwd"], ".");
    assert_eq!(event["shell"], "bash");
    assert_eq!(event["tests_run_count"], 1);
}

#[tokio::test]
async fn completed_run_job_validation_enters_handoff_from_job_authority() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let auth = open_auth_context();
    let capabilities = crate::shell_protocol::ShellClientCapabilities {
        async_shell_jobs: true,
        ..Default::default()
    };
    register_agent_projects_for_auth(
        &runtime,
        "validation-job",
        &auth,
        capabilities,
        vec![registered_project("demo", &tmp.path().to_string_lossy())],
    )
    .await;
    let project = "agent:validation-job:demo".to_string();
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("job validation".to_string()));

    let assertion_name = "direct run job validation";
    let expected_identity =
        crate::tool_runtime::tool_audit::assertion_validation_identity(assertion_name);
    let (call, recorder_metadata) = ToolCall::from_tool_name_with_recorder_metadata(
        "run_job",
        json!({
            "project": project,
            "command": "cargo test focused",
            "session_id": session.session_id,
            "timeout_secs": 30,
            "cwd": ".",
            "purpose": "test",
            "shell": "bash",
            "assertion_name": assertion_name,
        }),
    )
    .unwrap();
    let execution = runtime
        .dispatch_with_auth_transport_options_and_metadata(
            call,
            Some(&auth),
            SessionTransport::Mcp,
            recorder_metadata,
        )
        .await;
    assert!(execution.success, "{:?}", execution.error);
    assert_eq!(execution.output["cwd"], ".");
    assert_eq!(execution.output["shell"], "bash");
    let job_id = execution.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_agent_request_for_client(&runtime, "validation-job").await;
    assert_eq!(request.kind, "start_job");
    runtime
        .shell_clients
        .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
            client_id: "validation-job".to_string(),
            agent_instance_id: "inst-validation-job".to_string(),
            update_seq: None,
            job_id: job_id.clone(),
            request_id: Some(request.request_id),
            status: "completed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: Some(
                "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n".to_string(),
            ),
            stderr_tail: Some(String::new()),
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(12),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: true,
        })
        .await
        .unwrap();

    let handoff = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: session.session_id,
                project: Some(project),
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(true),
                summary_only: true,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["validation"]["status"], "passed");
    let event = &handoff.output["validation"]["latest"];
    assert_eq!(event["execution_source"], "run_job");
    assert_eq!(event["purpose"], "test");
    assert_eq!(event["execution_state"], "completed");
    assert_eq!(event["exit_code"], 0);
    assert_eq!(event["identity"], expected_identity);
    assert_eq!(event["assertion_name"], assertion_name);
    assert_eq!(
        handoff.output["facts"]["executions"][0]["identity"],
        event["identity"]
    );
    assert_eq!(handoff.output["hard_blockers"], json!([]));
}

#[tokio::test]
async fn promoted_run_process_cargo_test_materializes_canonical_validation_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime =
        test_runtime().with_structured_execution_sync_wait(std::time::Duration::from_millis(40));
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "validation-process-job",
        &auth,
        crate::shell_protocol::ShellClientCapabilities {
            shell: true,
            async_jobs: true,
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_process_argv: true,
            structured_execution_jobs: true,
            ..Default::default()
        },
        vec![registered_project("demo", &tmp.path().to_string_lossy())],
    )
    .await;
    let project = "agent:validation-process-job:demo".to_string();
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let assertion_name = "promoted process validation";
    let expected_identity =
        crate::tool_runtime::tool_audit::assertion_validation_identity(assertion_name);
    let (call, recorder_metadata) = ToolCall::from_tool_name_with_recorder_metadata(
        "run_process",
        json!({
            "project": project,
            "executable": "cargo",
            "args": ["test", "focused", "-p", "webcodex"],
            "session_id": session_id,
            "timeout_secs": 121,
            "cwd": ".",
            "purpose": "test",
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
    let request = wait_for_agent_request_for_client(&runtime, "validation-process-job").await;
    assert_eq!(request.kind, "start_process_job");
    assert_eq!(request.process.as_ref().unwrap().executable, "cargo");
    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    let job_id = handoff.output["job_id"].as_str().unwrap().to_string();
    let admitted = runtime.shell_clients.get_job(&job_id).await.unwrap();
    let metadata = admitted.structured_execution.as_ref().unwrap();
    assert_eq!(metadata.execution_source, "run_process");
    assert_eq!(metadata.validation_tool.as_deref(), Some("cargo_test"));
    assert_eq!(metadata.assertion_name.as_deref(), Some(assertion_name));
    let target = metadata
        .validation_identity
        .as_deref()
        .expect("admission-derived validation identity")
        .to_string();
    assert_eq!(target, expected_identity);
    assert!(target.starts_with("assertion:"));

    runtime
        .shell_clients
        .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
            client_id: "validation-process-job".to_string(),
            agent_instance_id: "inst-validation-process-job".to_string(),
            update_seq: None,
            job_id: job_id.clone(),
            request_id: Some(request.request_id),
            status: "completed".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: Some(
                "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
                    .to_string(),
            ),
            stderr_tail: Some(String::new()),
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(12),
            error: None,
            command_execution_state: Some(crate::shell_protocol::ShellCommandExecutionState::Completed),
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
    assert_eq!(latest["execution_source"], "run_process");
    assert_eq!(latest["validation_kind"], "test");
    assert_eq!(latest["identity"], target);
    assert_eq!(latest["assertion_name"], assertion_name);
    assert_eq!(latest["tests_run_count"], 1);
    assert_eq!(latest["tests_passed"], 1);
    assert_eq!(latest["tests_failed"], 0);
    assert_eq!(latest["zero_tests_run"], false);
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
            ToolCall::WorkOnProject {
                project: project.clone(),
                client_id: None,
                path: None,
                instruction: "validation finish".to_string(),
                session_id: None,
                include_project_instructions: true,
                include_workflow_guidance: true,
            },
            Some(&auth),
        )
        .await;
    assert!(start.success, "{:?}", start.error);
    let session_id = start.output["session_id"].as_str().unwrap().to_string();

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
    let req = wait_for_patch_agent_request(&runtime, "validation-finish").await;
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
                        require_tests: None,
                        min_tests: None,
                        timeout_secs: Some(60),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "validation-finish").await;
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
    let req = wait_for_patch_agent_request(&runtime, "validation-finish").await;
    assert_internal_posix_script_contains(&req, "git status --porcelain=v1 -b");
    let show_changes_stdout =
        crate::tool_runtime::framed_clean_show_changes_test_stdout("add lib", false);
    complete_patch_agent_request(
        &runtime,
        "validation-finish",
        &req.request_id,
        0,
        &show_changes_stdout,
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
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: session_id.clone(),
                project: None,
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(true),
                summary_only: false,
                limit: None,
            },
            Some(&auth),
        )
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
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: session_id.clone(),
                project: None,
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(true),
                summary_only: true,
                limit: None,
            },
            Some(&auth),
        )
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
    let req = wait_for_patch_agent_request(&runtime, "validation-finish").await;
    let show_changes_stdout =
        crate::tool_runtime::framed_clean_show_changes_test_stdout("add lib", false);
    complete_patch_agent_request(
        &runtime,
        "validation-finish",
        &req.request_id,
        0,
        &show_changes_stdout,
        "",
    )
    .await;
    let finish_compact = finish_compact_task.await.unwrap();
    assert!(finish_compact.success, "{:?}", finish_compact.error);
    assert_eq!(finish_compact.output["validation"]["status"], "mixed");
    assert_eq!(finish_compact.output["validation"]["successes"], 1);
    assert_eq!(finish_compact.output["validation"]["failures"], 1);
    assert_eq!(
        finish_compact.output["validation"]["unresolved_failure_count"],
        1
    );
    assert!(finish_compact.output["validation"].get("events").is_none());
    assert!(finish_compact.output["validation"].get("latest").is_none());
}

#[test]
fn current_evidence_failure_then_content_change_then_different_success_passes() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_content_mutation(&store, &session.session_id, true);
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(validation["current_evidence"]["status"], "passed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
    assert_eq!(validation["current_evidence"]["stale_failure_count"], 1);
    assert_eq!(
        validation["current_evidence"]["boundary_reason"],
        "workspace_content_changed"
    );
}

#[test]
fn current_evidence_validation_started_before_content_change_then_pass_is_stale() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let validation_start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({"project": "agent:eval:demo"}),
    );
    record_content_mutation(&store, &session.session_id, true);
    store.record_tool_call_finished(validation_start, true, &json!({"exit_code": 0}), None, None);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
    assert_eq!(
        validation["current_evidence"]["evidence_after_latest_content_change"],
        false
    );
}

#[test]
fn current_evidence_validation_started_before_content_change_then_fail_is_stale() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let validation_start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_test",
        &json!({"project": "agent:eval:demo"}),
    );
    record_content_mutation(&store, &session.session_id, true);
    store.record_tool_call_finished(
        validation_start,
        false,
        &json!({"exit_code": 101, "failure_kind": "validation_failed"}),
        Some("validation failed"),
        None,
    );

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
    assert_eq!(validation["current_evidence"]["stale_failure_count"], 1);
}

#[test]
fn current_evidence_validation_started_after_content_change_can_pass() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_content_mutation(&store, &session.session_id, true);
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "passed");
    assert_eq!(validation["current_evidence"]["events_total"], 1);
    assert_eq!(
        validation["current_evidence"]["evidence_after_latest_content_change"],
        true
    );
}

#[test]
fn current_evidence_validation_started_before_new_attempt_does_not_enter_new_attempt() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let validation_start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({"project": "agent:eval:demo"}),
    );
    record_task_instruction(&store, &session.session_id, "review current workspace");
    store.record_tool_call_finished(validation_start, true, &json!({"exit_code": 0}), None, None);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["current_evidence"]["status"], "not_run");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
}

#[test]
fn validation_job_terminal_inherits_public_failure_expectation() {
    let store = SessionStore::default();
    let project = "agent:eval:demo";
    let session = store.start_session(Some(project.to_string()), None);
    let job_id = "job_expected_negative_validation";
    let target = "target:aaaaaaaaaaaaaaaaaaaaaaaa";

    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({
            "project": project,
            "result_expectation": "failure"
        }),
    );
    store.record_tool_call_finished(
        start,
        true,
        &json!({
            "job_id": job_id,
            "job_status": "running",
            "promoted_to_job": true,
            "execution_state": "running",
            "command_started": true,
            "command_completed": false,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false
        }),
        None,
        None,
    );

    assert!(store.record_validation_job_terminal(
        &session.session_id,
        job_id,
        &[job_id],
        "cargo_check",
        Some(project.to_string()),
        target,
        None,
        "failed",
        Some(101),
        Some(false),
        Some(100),
        Some(110),
        Some(10_000),
        None,
    ));

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let terminal = summary
        .events
        .iter()
        .find(|event| event.kind == "validation_job_terminal")
        .expect("materialized terminal validation event");
    assert_eq!(terminal.status.as_deref(), Some("failed"));
    assert_eq!(terminal.exit_code, Some(101));
    assert_eq!(terminal.result_expectation.as_deref(), Some("failure"));
    assert_eq!(
        terminal.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );

    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "expected");
    assert_eq!(validation["latest_status"], "expected");
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest"]["execution_success"], false);
    assert_eq!(validation["latest"]["expectation_satisfied"], true);
    assert_eq!(validation["expected_results"], 1);
    assert_eq!(validation["latest"]["exit_code"], 101);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
}

#[test]
fn validation_job_terminal_inherits_expectation_beyond_default_summary_window() {
    let store = SessionStore::default();
    let project = "agent:eval:demo";
    let session = store.start_session(Some(project.to_string()), None);
    let job_id = "job_expected_negative_validation_deep_history";
    let target = "target:bbbbbbbbbbbbbbbbbbbbbbbb";

    let start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({
            "project": project,
            "result_expectation": "failure"
        }),
    );
    store.record_tool_call_finished(
        start,
        true,
        &json!({
            "job_id": job_id,
            "job_status": "running",
            "promoted_to_job": true,
            "execution_state": "running",
            "command_started": true,
            "command_completed": false,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false
        }),
        None,
        None,
    );

    // Keep the Job admission inside the retained 200-event ledger while
    // pushing it outside the public/default 50-event Session summary window.
    for index in 0..30 {
        record_finished_tool(
            &store,
            &session.session_id,
            "read_file",
            json!({"project": project, "path": format!("src/noise-{index}.rs")}),
            true,
            json!({}),
        );
    }

    assert!(store.record_validation_job_terminal(
        &session.session_id,
        job_id,
        &[job_id],
        "cargo_check",
        Some(project.to_string()),
        target,
        None,
        "failed",
        Some(101),
        Some(false),
        Some(100),
        Some(110),
        Some(10_000),
        None,
    ));

    let summary = store.summary(&session.session_id, Some(200)).unwrap();
    let terminal = summary
        .events
        .iter()
        .find(|event| event.kind == "validation_job_terminal")
        .expect("materialized terminal validation event");
    assert_eq!(terminal.result_expectation.as_deref(), Some("failure"));
    assert_eq!(
        terminal.failure_expectation_result.as_deref(),
        Some("matched_expected_failure")
    );
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "expected");
    assert_eq!(validation["latest_status"], "expected");
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest"]["execution_success"], false);
    assert_eq!(validation["latest"]["expectation_satisfied"], true);
    assert_eq!(validation["expected_results"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
}

#[test]
fn expected_observation_failure_does_not_resolve_prior_same_identity_validation_failure() {
    let store = SessionStore::default();
    let project = "agent:eval:demo";
    let session = store.start_session(Some(project.to_string()), None);
    let assertion_name = "same regression remains failing";
    let failed_output = json!({
        "exit_code": 1,
        "failure_kind": "command_exit_nonzero",
        "command_started": true,
        "command_completed": true,
        "execution_state": "completed"
    });

    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        json!({
            "project": project,
            "purpose": "test",
            "assertion_name": assertion_name
        }),
        false,
        failed_output.clone(),
    );
    record_finished_tool(
        &store,
        &session.session_id,
        "run_process",
        json!({
            "project": project,
            "purpose": "test",
            "assertion_name": assertion_name,
            "result_expectation": "observe"
        }),
        false,
        failed_output,
    );

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["events_total"], 2);
    assert_eq!(validation["successes"], 0);
    assert_eq!(validation["failures"], 1);
    assert_eq!(validation["expected_results"], 1);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["latest_status"], "expected");
    assert_eq!(validation["historical_failures"]["count"], 1);
    assert_eq!(validation["historical_failures"]["resolved"], false);
    assert_eq!(validation["historical_failures"]["unresolved"], true);
    assert_eq!(validation["resolved_failures"]["count"], 0);
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(validation["current_evidence"]["status"], "failed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        1
    );
    assert_eq!(validation["latest"]["success"], false);
    assert_eq!(validation["latest"]["execution_success"], false);
    assert_eq!(validation["latest"]["expectation_satisfied"], true);
}

#[test]
fn current_evidence_validation_job_started_before_content_change_is_stale() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    let job_id = "job_current_evidence_overlap";
    let validation_start = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_check",
        &json!({"project": "agent:eval:demo"}),
    );
    store.record_tool_call_finished(
        validation_start,
        true,
        &json!({"job_id": job_id, "execution_state": "started"}),
        None,
        None,
    );
    record_content_mutation(&store, &session.session_id, true);
    assert!(store.record_validation_job_terminal(
        &session.session_id,
        job_id,
        &[job_id],
        "cargo_check",
        Some("agent:eval:demo".to_string()),
        "target:aaaaaaaaaaaaaaaaaaaaaaaa",
        None,
        "completed",
        Some(0),
        Some(true),
        Some(100),
        Some(110),
        Some(10_000),
        None,
    ));

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
}

#[test]
fn current_evidence_legacy_validation_without_exact_start_correlation_is_not_current() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_success(&store, &session.session_id, "cargo_check");

    let mut summary = store.summary(&session.session_id, Some(50)).unwrap();
    for event in &mut summary.events {
        if event.tool_name == "cargo_check" {
            event.call_id = None;
        }
    }
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["current_evidence"]["status"], "not_run");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
}

#[test]
fn current_evidence_different_success_without_mutation_keeps_failure_open() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "failed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        1
    );
    assert_eq!(validation["current_evidence"]["stale_failure_count"], 0);
    assert_eq!(
        validation["current_evidence"]["boundary_reason"],
        "attempt_start"
    );
}

#[test]
fn current_evidence_same_identity_failure_success_resolves_inside_window() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_check");
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "mixed");
    assert_eq!(validation["resolved_failures"]["count"], 1);
    assert_eq!(validation["unresolved_failures"]["count"], 0);
    assert_eq!(validation["current_evidence"]["status"], "passed");
    assert_eq!(validation["current_evidence"]["resolved_failure_count"], 1);
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
}

#[test]
fn current_evidence_pass_then_content_change_is_stale() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_success(&store, &session.session_id, "cargo_check");
    record_content_mutation(&store, &session.session_id, true);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
    assert_eq!(
        validation["current_evidence"]["evidence_after_latest_content_change"],
        false
    );
}

#[test]
fn current_evidence_failure_then_content_change_without_validation_is_stale_not_failed() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_content_mutation(&store, &session.session_id, true);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
    assert_eq!(validation["current_evidence"]["stale_failure_count"], 1);
}

#[test]
fn current_evidence_failed_noop_mutation_does_not_reset() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_finished_tool(
        &store,
        &session.session_id,
        "apply_text_edits",
        json!({"project": "agent:eval:demo", "changes": [{"kind": "edit", "path": "src/lib.rs"}]}),
        false,
        json!({"state_changed": false, "failure_kind": "stale_precondition"}),
    );
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "failed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        1
    );
}

#[test]
fn current_evidence_metadata_only_state_change_does_not_reset() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_finished_tool(
        &store,
        &session.session_id,
        "git_push",
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"state_changed": true}),
    );
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "failed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        1
    );
    assert_eq!(
        validation["current_evidence"]["boundary_reason"],
        "attempt_start"
    );
}

#[test]
fn current_evidence_second_content_change_resets_post_first_change_validation() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_content_mutation(&store, &session.session_id, true);
    record_validation_success(&store, &session.session_id, "cargo_check");
    record_content_mutation(&store, &session.session_id, true);

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "stale");
    assert_eq!(validation["current_evidence"]["events_total"], 0);
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
}

#[test]
fn current_evidence_old_attempt_failure_is_excluded() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_task_instruction(&store, &session.session_id, "review current workspace");
    record_finished_tool(
        &store,
        &session.session_id,
        "read_file",
        json!({"project": "agent:eval:demo", "path": "src/lib.rs"}),
        true,
        json!({}),
    );

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["unresolved_failures"]["count"], 1);
    assert_eq!(validation["current_evidence"]["status"], "not_run");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        0
    );
}

#[test]
fn current_evidence_legacy_mutation_without_effect_proof_is_conservative() {
    let store = SessionStore::default();
    let session = store.start_session(Some("agent:eval:demo".to_string()), None);
    record_validation_failure(&store, &session.session_id, "cargo_test");
    record_content_mutation(&store, &session.session_id, false);
    record_validation_success(&store, &session.session_id, "cargo_check");

    let summary = store.summary(&session.session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["current_evidence"]["status"], "failed");
    assert_eq!(
        validation["current_evidence"]["unresolved_failure_count"],
        1
    );
    assert_eq!(
        validation["current_evidence"]["boundary_reason"],
        "attempt_start"
    );
}

fn record_validation_failure(store: &SessionStore, session_id: &str, tool_name: &str) {
    record_finished_tool(
        store,
        session_id,
        tool_name,
        json!({"project": "agent:eval:demo"}),
        false,
        json!({"exit_code": 101, "failure_kind": "validation_failed"}),
    );
}

fn record_validation_success(store: &SessionStore, session_id: &str, tool_name: &str) {
    record_finished_tool(
        store,
        session_id,
        tool_name,
        json!({"project": "agent:eval:demo"}),
        true,
        json!({"exit_code": 0}),
    );
}

fn record_content_mutation(store: &SessionStore, session_id: &str, proven: bool) {
    record_finished_tool(
        store,
        session_id,
        "apply_text_edits",
        json!({"project": "agent:eval:demo", "changes": [{"kind": "edit", "path": "src/lib.rs"}]}),
        true,
        if proven {
            json!({"state_changed": true})
        } else {
            json!({})
        },
    );
}

fn record_task_instruction(store: &SessionStore, session_id: &str, instruction: &str) {
    store
        .ensure_coding_session(sessions::CodingSessionRequest {
            project: "agent:eval:demo".to_string(),
            authority_fingerprint: sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                .to_string(),
            resume_session_id: Some(session_id.to_string()),
            instruction: Some(instruction.to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap();
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
