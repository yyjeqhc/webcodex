//! Cargo test execution-count evidence derived from complete harness summaries.

use webcodex_core::validation_evidence::parse_complete_cargo_test_summary_counts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CargoTestRunMetadata {
    pub tests_detected: bool,
    pub tests_run_count: Option<u64>,
    pub tests_passed: Option<u64>,
    pub tests_failed: Option<u64>,
    pub zero_tests_run: Option<bool>,
    pub count_evidence_reason: &'static str,
}

pub fn parse_cargo_test_run_metadata(text: &str) -> CargoTestRunMetadata {
    let mut tests_run_count = 0_u64;
    let mut tests_passed = 0_u64;
    let mut tests_failed = 0_u64;
    let mut complete_summary_found = false;
    let mut incomplete_summary_found = false;
    let mut tests_detected = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("running ") {
            let mut parts = rest.split_whitespace();
            if parts
                .next()
                .is_some_and(|count| count.parse::<u64>().is_ok())
                && parts
                    .next()
                    .is_some_and(|label| label == "test" || label == "tests")
            {
                // `running N tests` includes ignored items. It is useful only
                // as a harness-detection signal, never as executed-count proof.
                tests_detected = true;
            }
        }

        if !line.contains("test result:") {
            continue;
        }
        tests_detected = true;
        match parse_complete_cargo_test_summary_counts(line) {
            Some((passed, failed)) => {
                complete_summary_found = true;
                tests_passed = tests_passed.saturating_add(passed);
                tests_failed = tests_failed.saturating_add(failed);
                tests_run_count = tests_run_count
                    .saturating_add(passed)
                    .saturating_add(failed);
            }
            None => {
                // A partial/malformed summary makes the aggregate unproven;
                // do not promote counts observed in other retained sections.
                incomplete_summary_found = true;
            }
        }
    }

    if complete_summary_found && !incomplete_summary_found {
        CargoTestRunMetadata {
            tests_detected,
            tests_run_count: Some(tests_run_count),
            tests_passed: Some(tests_passed),
            tests_failed: Some(tests_failed),
            zero_tests_run: Some(tests_run_count == 0),
            count_evidence_reason: "complete_summary",
        }
    } else {
        CargoTestRunMetadata {
            tests_detected,
            tests_run_count: None,
            tests_passed: None,
            tests_failed: None,
            zero_tests_run: None,
            count_evidence_reason: if incomplete_summary_found {
                "partial_harness_summary"
            } else {
                "no_complete_summary"
            },
        }
    }
}
