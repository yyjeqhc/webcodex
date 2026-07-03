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
        diagnostics.first_failed_test.as_deref(),
        Some("tests::example_fails")
    );
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
