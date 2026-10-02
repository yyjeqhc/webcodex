use super::*;

#[test]
fn sparse_successful_hygiene_report_is_observed_zero_not_missing_evidence() {
    let report = json!({"clean": true, "git_available": true});
    let observed = hygiene_observation(&report, true, true);
    assert_eq!(observed.observed().unwrap().secret_like_paths, 0);
    assert!(!observed.observed().unwrap().truncated);
    assert_eq!(
        hygiene_observation(&report, true, false),
        Observation::Failed
    );
    assert_eq!(
        hygiene_observation(&report, false, false),
        Observation::NotChecked
    );
    assert_eq!(
        hygiene_observation(&Value::Null, true, true),
        Observation::Failed
    );
}

#[test]
fn failed_workspace_does_not_trust_a_clean_legacy_display() {
    let legacy = json!({"clean": true, "counts": {}, "git_available": false});
    assert_eq!(
        workspace_observation(&legacy, true, false),
        Observation::Failed
    );
    assert_eq!(
        workspace_observation(&legacy, false, true),
        Observation::NotChecked
    );
}

#[test]
fn validation_adapter_preserves_current_history_and_zero_test_rules() {
    let validation = json!({"status":"mixed", "successes":1,
        "historical_failures":{"resolved":true,"unresolved":false},
        "unresolved_failures":{"count":2}, "current_evidence":{"status":"passed","unresolved_failure_count":0},
        "latest_status":"inconclusive", "cargo_test_zero_tests_run":true});
    let facts = closeout_facts(
        Observation::NotChecked,
        None,
        &Value::Null,
        &validation,
        &Value::Null,
        &Value::Null,
    );
    assert_eq!(
        facts.validation.current_status(),
        Some(ValidationStatus::Passed)
    );
    assert_eq!(facts.validation.current_unresolved_failure_count, 0);
    assert!(facts.validation.historical_failures_resolved);
    assert!(facts.validation.cargo_test_zero_tests);
    assert_eq!(
        evaluate_closeout(&facts, &[]).history_status,
        EvidenceHistoryStatus::MixedResolved
    );
}

#[test]
fn changing_display_paths_or_counts_cannot_change_the_captured_decision() {
    let workspace = json!({"clean":false,"counts":{"conflicted":1}});
    let facts = closeout_facts(
        workspace_observation(&workspace, true, true),
        None,
        &Value::Null,
        &json!({"status":"passed"}),
        &Value::Null,
        &Value::Null,
    );
    let mut display =
        json!({"workspace_clean":true,"workspace_conflicts":0,"validation":{"status":"passed"}});
    let policy = install_closeout_decision(&mut display, &facts, &[]);
    assert!(policy.blocking());
    assert_eq!(display["facts"]["workspace_state"]["conflicts"], 1);
    assert_eq!(display["task_outcome"]["blocking"], true);
    let mut entirely_different_display = json!({"arbitrary_layout":{"workspace":"clean"}});
    assert_eq!(
        install_closeout_decision(&mut entirely_different_display, &facts, &[]),
        policy
    );
    assert_eq!(
        entirely_different_display["task_outcome"],
        display["task_outcome"]
    );
}
