use crate::tool_runtime::cargo::parse_cargo_test_counts;
use crate::tool_runtime::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, NO_STABLE_DIAGNOSTICS_REASON,
    PARSER_KIND, PARSER_LIMITATIONS, PARSER_VERSION,
};

#[test]
fn structured_parser_v2_identity_and_limit_contract_are_stable() {
    assert_eq!(PARSER_KIND, "structured_validation_parser");
    assert_eq!(PARSER_VERSION, 2);
    assert_eq!(
        PARSER_LIMITATIONS,
        [
            "bounded validation excerpts only",
            "deterministic evidence extraction; no root-cause inference",
            "no full stdout/stderr bodies; incomplete excerpts may omit fields or report unknown",
        ]
    );
}

#[test]
fn cargo_check_parser_returns_sorted_deduplicated_bounded_diagnostics() {
    let mut stderr = String::from(
        "warning: unused value\n --> src/z.rs:9:3\n\
         error[E0308]: mismatched types\n --> src/lib.rs:42:9\n\
         error[E0308]: mismatched types\n --> src/lib.rs:42:9\n\
         error[E0425]: missing name\n --> src/a.rs:2:1\n",
    );
    for index in 0..22 {
        stderr.push_str(&format!(
            "error[E0001]: bounded diagnostic {index}\n --> src/generated_{index}.rs:1:1\n"
        ));
    }

    let diagnostics = parse_cargo_check_diagnostics("", &stderr, false);

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(25));
    assert_eq!(diagnostics.returned_diagnostic_count, 20);
    assert_eq!(diagnostics.diagnostics.len(), 20);
    assert_eq!(diagnostics.diagnostics_truncated, true);
    assert_eq!(diagnostics.invalid_diagnostics_omitted, 0);
    assert_eq!(
        diagnostics.first_diagnostic,
        diagnostics.diagnostics.first().cloned()
    );
    assert_eq!(diagnostics.diagnostics[0].severity, "error");
    assert_eq!(diagnostics.diagnostics[0].file.as_deref(), Some("src/a.rs"));
    assert_eq!(diagnostics.diagnostics[0].message, "missing name");
}

#[test]
fn cargo_check_parser_sanitizes_messages_and_omits_unsafe_spans() {
    let unicode = "界".repeat(300);
    let diagnostics = parse_cargo_check_diagnostics(
        "",
        &format!(
            "\u{1b}[31merror[E0308]\u{1b}[0m: {unicode}\u{7}\n --> /root/private.rs:4:2\n\
             warning: safe warning\n --> ../escape.rs:0:0\n\
             error: uri span\n --> file:///tmp/private.rs:9:1\n\
             note: TOKEN=must-not-leak\nhelp: run dangerous command\n"
        ),
        true,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(3));
    assert_eq!(diagnostics.returned_diagnostic_count, 3);
    assert_eq!(diagnostics.diagnostics_truncated, true);
    assert_eq!(diagnostics.diagnostics[0].message.chars().count(), 240);
    assert!(diagnostics
        .diagnostics
        .iter()
        .all(|item| item.file.is_none()));
    assert!(diagnostics
        .diagnostics
        .iter()
        .all(|item| item.line.is_none() && item.column.is_none()));
    let encoded = serde_json::to_string(&diagnostics).unwrap();
    for forbidden in [
        "\u{1b}",
        "\u{7}",
        "/root",
        "../",
        "file://",
        "TOKEN=",
        "dangerous command",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "leaked {forbidden:?}: {encoded}"
        );
    }
}

#[test]
fn cargo_check_parser_counts_invalid_diagnostics_without_guessing() {
    let diagnostics = parse_cargo_check_diagnostics(
        "",
        "error[E0308]: TOKEN=secret\n --> src/lib.rs:1:1\n\
         error[E0425]: safe message\n --> src/lib.rs:not-a-line:1\n",
        false,
    );

    assert_eq!(diagnostics.diagnostic_count, Some(1));
    assert_eq!(diagnostics.returned_diagnostic_count, 1);
    assert_eq!(diagnostics.invalid_diagnostics_omitted, 1);
    assert_eq!(diagnostics.diagnostics[0].message, "safe message");
    assert_eq!(diagnostics.diagnostics[0].file, None);
}

#[test]
fn cargo_check_parser_does_not_return_note_or_help_bodies() {
    let diagnostics = parse_cargo_check_diagnostics(
        "",
        "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n\
         note: private implementation detail\n\
         help: execute this suggested command\n",
        false,
    );

    let encoded = serde_json::to_string(&diagnostics).unwrap();
    assert!(encoded.contains("mismatched types"));
    assert!(!encoded.contains("private implementation detail"));
    assert!(!encoded.contains("execute this suggested command"));
}

#[test]
fn cargo_test_parser_returns_bounded_failure_details_without_payloads() {
    let output = "test tests::asserts ... FAILED\n\
                  thread 'tests::asserts' panicked at src/tests.rs:84:5:\n\
                  assertion `left == right` failed\n\
                  left: supersecret-left\n\
                  right: supersecret-right\n\
                  test tests::panics ... FAILED\n\
                  thread 'tests::panics' panicked at /root/private.rs:9:2:\n\
                  private panic body\n\
                  stack backtrace:\n\
                  0: private::frame\n\
                  test tests::unknown ... FAILED\n\
                  test tests::asserts ... FAILED\n\
                  test result: FAILED. 0 passed; 3 failed; 0 ignored\n";
    let diagnostics = parse_cargo_test_diagnostics(output, "", false);

    assert_eq!(
        diagnostics.failed_tests,
        vec!["tests::asserts", "tests::panics", "tests::unknown"]
    );
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::asserts")
    );
    assert_eq!(diagnostics.failed_test_details.len(), 3);
    assert_eq!(diagnostics.failed_test_details[0].name, "tests::asserts");
    assert_eq!(diagnostics.failed_test_details[0].failure_kind, "assertion");
    assert_eq!(
        diagnostics.failed_test_details[0].file.as_deref(),
        Some("src/tests.rs")
    );
    assert_eq!(diagnostics.failed_test_details[0].line, Some(84));
    assert_eq!(diagnostics.failed_test_details[0].column, Some(5));
    assert_eq!(diagnostics.failed_test_details[1].failure_kind, "panic");
    assert_eq!(diagnostics.failed_test_details[1].file, None);
    assert_eq!(diagnostics.failed_test_details[2].failure_kind, "unknown");
    let encoded = serde_json::to_string(&diagnostics).unwrap();
    for forbidden in [
        "supersecret",
        "private panic body",
        "stack backtrace",
        "private::frame",
        "/root",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "leaked {forbidden:?}: {encoded}"
        );
    }
}

#[test]
fn cargo_test_parser_caps_failed_test_details_at_twenty_in_first_seen_order() {
    let mut output = String::new();
    for index in 1..=22 {
        output.push_str(&format!("test tests::case_{index} ... FAILED\n"));
    }
    output.push_str("test result: FAILED. 0 passed; 22 failed; 0 ignored\n");

    let diagnostics = parse_cargo_test_diagnostics(&output, "", true);

    assert_eq!(diagnostics.failed_tests.len(), 20);
    assert_eq!(diagnostics.failed_test_details.len(), 20);
    assert_eq!(diagnostics.failed_tests[0], "tests::case_1");
    assert_eq!(diagnostics.failed_tests[19], "tests::case_20");
    assert_eq!(diagnostics.failed_test_details[19].name, "tests::case_20");
    assert!(diagnostics
        .failed_test_details
        .iter()
        .all(|detail| detail.failure_kind == "unknown"));
    assert_eq!(diagnostics.failed_tests_truncated, true);
}

#[test]
fn cargo_test_parser_extracts_compile_diagnostics_before_tests_run() {
    let diagnostics = parse_cargo_test_diagnostics(
        "",
        "error[E0308]: mismatched types\n --> src/lib.rs:8:3\n",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.test_summary, None);
    assert!(diagnostics.failed_tests.is_empty());
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert_eq!(diagnostics.diagnostics[0].code.as_deref(), Some("E0308"));
}

#[test]
fn cargo_check_parser_extracts_e_code_and_file_span() {
    let diagnostics = parse_cargo_check_diagnostics(
        "",
        "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n  |\n",
        true,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.parser, PARSER_KIND);
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    let first = diagnostics.first_diagnostic.as_ref().unwrap();
    assert_eq!(first.severity, "error");
    assert_eq!(first.code.as_deref(), Some("E0308"));
    assert_eq!(first.file.as_deref(), Some("src/lib.rs"));
    assert_eq!(first.line, Some(12));
    assert_eq!(first.column, Some(5));
    assert_eq!(diagnostics.truncated, Some(true));
}

#[test]
fn cargo_check_parser_handles_warning_without_e_code() {
    let diagnostics = parse_cargo_check_diagnostics(
        "",
        "warning: unused variable: `value`\n --> src/lib.rs:3:9\n",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    let first = diagnostics.first_diagnostic.as_ref().unwrap();
    assert_eq!(first.severity, "warning");
    assert_eq!(first.code, None);
    assert_eq!(first.file.as_deref(), Some("src/lib.rs"));
    assert_eq!(first.line, Some(3));
    assert_eq!(first.column, Some(9));
    assert_eq!(diagnostics.truncated, Some(false));
}

#[test]
fn cargo_check_parser_returns_unavailable_for_unrelated_text() {
    let diagnostics = parse_cargo_check_diagnostics(
        "Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s",
        "",
        false,
    );

    assert_eq!(diagnostics.available, false);
    assert_eq!(diagnostics.parser, PARSER_KIND);
    assert_eq!(diagnostics.reason, Some(NO_STABLE_DIAGNOSTICS_REASON));
    assert_eq!(diagnostics.first_diagnostic, None);
}

#[test]
fn cargo_test_parser_extracts_summary_counts() {
    let diagnostics = parse_cargo_test_diagnostics(
        "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "",
        true,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.parser, PARSER_KIND);
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(2));
    assert_eq!(summary.failed, Some(1));
    assert_eq!(summary.ignored, Some(0));
    assert_eq!(diagnostics.truncated, Some(true));
}

#[test]
fn cargo_test_parser_extracts_first_failed_test_name() {
    let diagnostics = parse_cargo_test_diagnostics(
        "running 1 test\ntest tests::example_fails ... FAILED\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    assert_eq!(
        diagnostics.failed_tests,
        vec!["tests::example_fails".to_string()]
    );
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::example_fails")
    );
    assert_eq!(diagnostics.failed_tests_truncated, false);
}

#[test]
fn cargo_test_parser_extracts_bounded_failed_test_names() {
    let diagnostics = parse_cargo_test_diagnostics(
        "test tests::first_failure ... FAILED\n\
         test tests::second_failure ... FAILED\n\
         test tests::third_failure ... FAILED\n\
         test result: FAILED. 7 passed; 3 failed; 1 ignored\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(3));
    assert_eq!(
        diagnostics.failed_tests,
        vec![
            "tests::first_failure".to_string(),
            "tests::second_failure".to_string(),
            "tests::third_failure".to_string(),
        ]
    );
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::first_failure")
    );
    assert_eq!(diagnostics.failed_tests_truncated, false);
    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(7));
    assert_eq!(summary.failed, Some(3));
    assert_eq!(summary.ignored, Some(1));
}

#[test]
fn cargo_test_parser_dedupes_failed_test_names_in_first_seen_order() {
    let diagnostics = parse_cargo_test_diagnostics(
        "test tests::alpha ... FAILED\n\
         test tests::beta ... FAILED\n\
         test tests::alpha ... FAILED\n\
         test tests::gamma ... FAILED\n\
         test result: FAILED. 0 passed; 3 failed; 0 ignored\n",
        "",
        false,
    );

    assert_eq!(
        diagnostics.failed_tests,
        vec![
            "tests::alpha".to_string(),
            "tests::beta".to_string(),
            "tests::gamma".to_string(),
        ]
    );
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::alpha")
    );
    assert_eq!(diagnostics.failed_tests_truncated, false);
    assert_eq!(diagnostics.diagnostic_count, Some(3));
}

#[test]
fn cargo_test_parser_keeps_up_to_twenty_failed_tests() {
    let mut stdout = String::new();
    for i in 1..=12 {
        stdout.push_str(&format!("test tests::case_{i} ... FAILED\n"));
    }
    stdout.push_str("test result: FAILED. 0 passed; 12 failed; 0 ignored\n");

    let diagnostics = parse_cargo_test_diagnostics(&stdout, "", false);

    assert_eq!(diagnostics.failed_tests.len(), 12);
    assert_eq!(diagnostics.failed_tests[0], "tests::case_1");
    assert_eq!(diagnostics.failed_tests[11], "tests::case_12");
    assert_eq!(diagnostics.failed_tests_truncated, false);
    assert_eq!(diagnostics.diagnostic_count, Some(12));
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::case_1")
    );
}

#[test]
fn cargo_test_parser_marks_failed_tests_truncated_when_tail_truncated_and_summary_exceeds_names() {
    let diagnostics = parse_cargo_test_diagnostics(
        "test tests::one ... FAILED\n\
         test tests::two ... FAILED\n\
         test result: FAILED. 0 passed; 5 failed; 0 ignored\n",
        "",
        true,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(5));
    assert_eq!(diagnostics.failed_tests.len(), 2);
    assert_eq!(
        diagnostics.failed_tests,
        vec!["tests::one".to_string(), "tests::two".to_string()]
    );
    assert_eq!(diagnostics.failed_tests_truncated, true);
    assert_eq!(diagnostics.truncated, Some(true));
}

#[test]
fn cargo_test_parser_aggregates_multiple_harness_summaries() {
    let diagnostics = parse_cargo_test_diagnostics(
        "running 2 tests\n\
         test result: ok. 2 passed; 0 failed; 1 ignored\n\
         test tests::broken ... FAILED\n\
         test result: FAILED. 3 passed; 1 failed; 0 ignored\n\
         running 0 tests\n\
         test result: ok. 0 passed; 0 failed; 2 ignored\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, true);
    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(5));
    assert_eq!(summary.failed, Some(1));
    assert_eq!(summary.ignored, Some(3));
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    assert_eq!(diagnostics.failed_tests, vec!["tests::broken".to_string()]);
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::broken")
    );
    assert_eq!(diagnostics.failed_tests_truncated, false);
}

#[test]
fn cargo_test_parser_aggregates_when_first_harness_passes_and_later_fails() {
    // First summary has failed=0; later harness fails. Must not keep first-summary-wins.
    let diagnostics = parse_cargo_test_diagnostics(
        "test result: ok. 4 passed; 0 failed; 0 ignored\n\
         test tests::later_fail ... FAILED\n\
         test result: FAILED. 1 passed; 2 failed; 1 ignored\n",
        "",
        false,
    );

    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(5));
    assert_eq!(summary.failed, Some(2));
    assert_eq!(summary.ignored, Some(1));
    assert_eq!(diagnostics.diagnostic_count, Some(2));
    assert_eq!(
        diagnostics.failed_tests,
        vec!["tests::later_fail".to_string()]
    );
    assert_eq!(diagnostics.failed_tests_truncated, false);
}

#[test]
fn cargo_test_parser_aggregated_truncation_uses_summed_failed_count() {
    // summary1 failed=2 + summary2 failed=3 = 5; only 2 names captured with truncated tail.
    let diagnostics = parse_cargo_test_diagnostics(
        "test tests::one ... FAILED\n\
         test tests::two ... FAILED\n\
         test result: FAILED. 0 passed; 2 failed; 0 ignored\n\
         test result: FAILED. 1 passed; 3 failed; 0 ignored\n",
        "",
        true,
    );

    assert_eq!(diagnostics.diagnostic_count, Some(5));
    assert_eq!(diagnostics.failed_tests.len(), 2);
    assert_eq!(diagnostics.failed_tests_truncated, true);
    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(1));
    assert_eq!(summary.failed, Some(5));
}

#[test]
fn parse_cargo_test_counts_aggregates_multiple_harness_summaries() {
    let (passed, failed) = parse_cargo_test_counts(
        "test result: ok. 2 passed; 0 failed; 1 ignored\n\
         test result: FAILED. 3 passed; 1 failed; 0 ignored\n\
         test result: ok. 0 passed; 0 failed; 2 ignored\n",
    );
    assert_eq!(passed, Some(5));
    assert_eq!(failed, Some(1));
}

#[test]
fn parse_cargo_test_counts_does_not_use_last_summary_wins() {
    let (passed, failed) = parse_cargo_test_counts(
        "test result: FAILED. 10 passed; 4 failed\n\
         test result: ok. 1 passed; 0 failed\n",
    );
    assert_eq!(passed, Some(11));
    assert_eq!(failed, Some(4));
}

#[test]
fn cargo_test_parser_passing_run_returns_empty_failed_tests() {
    let diagnostics = parse_cargo_test_diagnostics(
        "test result: ok. 12 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(diagnostics.diagnostic_count, Some(0));
    assert!(diagnostics.failed_tests.is_empty());
    assert_eq!(diagnostics.first_failed_test, None);
    assert_eq!(diagnostics.failed_tests_truncated, false);
    let summary = diagnostics.test_summary.as_ref().unwrap();
    assert_eq!(summary.passed, Some(12));
    assert_eq!(summary.failed, Some(0));
    assert_eq!(summary.ignored, Some(2));
}

#[test]
fn cargo_test_parser_ignores_unsafe_text_and_invalid_test_names() {
    let diagnostics = parse_cargo_test_diagnostics(
        "thread 'x' panicked at 'TOKEN=secret'\n\
         assertion failed: left == right\n\
         /root/private/path\n\
         test invalid name with spaces ... FAILED\n\
         test tests::safe_fail ... FAILED\n\
         test result: FAILED. 0 passed; 1 failed; 0 ignored\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, true);
    assert_eq!(
        diagnostics.failed_tests,
        vec!["tests::safe_fail".to_string()]
    );
    assert_eq!(
        diagnostics.first_failed_test.as_deref(),
        Some("tests::safe_fail")
    );
    assert_eq!(diagnostics.diagnostic_count, Some(1));
    assert_eq!(diagnostics.failed_tests_truncated, false);

    let json = serde_json::to_string(&diagnostics).unwrap();
    for raw in [
        "TOKEN=secret",
        "assertion failed",
        "left == right",
        "/root/private/path",
        "invalid name with spaces",
        "panicked",
    ] {
        assert!(
            !json.contains(raw),
            "parser output must not contain unsafe text {raw:?}: {json}"
        );
    }
}

#[test]
fn parser_never_returns_raw_stdout_or_stderr_text() {
    let check = parse_cargo_check_diagnostics(
        "",
        "error[E0425]: cannot find value `secret_payload` in this scope\n --> src/lib.rs:8:1\n",
        true,
    );
    let test = parse_cargo_test_diagnostics(
        "test tests::example_fails ... FAILED\npanic payload that must not appear\n",
        "",
        true,
    );

    let check_json = serde_json::to_string(&check).unwrap();
    let test_json = serde_json::to_string(&test).unwrap();
    for raw in [
        "cannot find value",
        "secret_payload",
        "panic payload",
        "must not appear",
    ] {
        assert!(
            !check_json.contains(raw) && !test_json.contains(raw),
            "parser output must not contain raw tail text {raw:?}: {check_json} {test_json}"
        );
    }
}

#[test]
fn successful_cargo_check_may_have_no_diagnostics() {
    let diagnostics = parse_cargo_check_diagnostics(
        "Checking demo v0.1.0\nFinished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s\n",
        "",
        false,
    );

    assert_eq!(diagnostics.available, false);
    assert_eq!(diagnostics.reason, Some(NO_STABLE_DIAGNOSTICS_REASON));
}
