use crate::tool_runtime::cargo::parse_cargo_test_counts;
use crate::tool_runtime::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, NO_STABLE_DIAGNOSTICS_REASON,
    PARSER_KIND,
};

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
fn cargo_test_parser_caps_failed_tests_at_ten_and_marks_truncated() {
    let mut stdout = String::new();
    for i in 1..=12 {
        stdout.push_str(&format!("test tests::case_{i} ... FAILED\n"));
    }
    stdout.push_str("test result: FAILED. 0 passed; 12 failed; 0 ignored\n");

    let diagnostics = parse_cargo_test_diagnostics(&stdout, "", false);

    assert_eq!(diagnostics.failed_tests.len(), 10);
    assert_eq!(diagnostics.failed_tests[0], "tests::case_1");
    assert_eq!(diagnostics.failed_tests[9], "tests::case_10");
    assert!(!diagnostics
        .failed_tests
        .iter()
        .any(|name| name == "tests::case_11"));
    assert!(!diagnostics
        .failed_tests
        .iter()
        .any(|name| name == "tests::case_12"));
    assert_eq!(diagnostics.failed_tests_truncated, true);
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
