use super::*;

fn clean_facts() -> CloseoutFacts {
    CloseoutFacts {
        workspace: Observation::Observed(WorkspaceFacts {
            clean: Some(true),
            conflicts: Some(0),
            non_git: false,
        }),
        hygiene: Some(Observation::Observed(HygieneFacts {
            clean: Some(true),
            secret_like_paths: 0,
            truncated: false,
        })),
        validation: ValidationFacts {
            status: Some(ValidationStatus::Passed),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn unchecked_failed_and_successful_zero_observations_are_distinct() {
    let mut facts = clean_facts();
    assert_eq!(evaluate_closeout(&facts, &[]).task_status, TaskStatus::Pass);
    facts.workspace = Observation::NotChecked;
    assert!(evaluate_closeout(&facts, &[])
        .advisories
        .contains(&"workspace_not_checked"));
    facts.workspace = Observation::Failed;
    let failed = evaluate_closeout(&facts, &[]);
    assert!(failed.advisories.contains(&"workspace_observation_failed"));
    assert!(!failed.advisories.contains(&"workspace_not_checked"));
    assert_eq!(failed.task_status, TaskStatus::Warn);
}

#[test]
fn omitted_hygiene_is_not_a_failed_or_unrequested_assessment() {
    let mut facts = clean_facts();
    facts.hygiene = None;
    assert_eq!(evaluate_closeout(&facts, &[]).task_status, TaskStatus::Pass);
    facts.hygiene = Some(Observation::NotChecked);
    assert!(evaluate_closeout(&facts, &[])
        .advisories
        .contains(&"hygiene_not_checked"));
    facts.hygiene = Some(Observation::Failed);
    let failed = evaluate_closeout(&facts, &[]);
    assert!(failed.advisories.contains(&"hygiene_observation_failed"));
    assert_eq!(failed.task_status, TaskStatus::Warn);
}

#[test]
fn conflicts_are_hard_blockers_not_duplicate_dirty_advisories() {
    let mut facts = clean_facts();
    facts.workspace = Observation::Observed(WorkspaceFacts {
        clean: Some(false),
        conflicts: Some(1),
        non_git: false,
    });
    let result = evaluate_closeout(&facts, &[]);
    assert!(result.blocking());
    assert_eq!(result.blocking_reasons, ["workspace_conflicts"]);
    assert!(!result.advisories.contains(&"workspace_dirty"));
}

#[test]
fn unknown_conflict_count_does_not_silently_become_observed_zero() {
    let mut facts = clean_facts();
    facts.workspace = Observation::Observed(WorkspaceFacts {
        clean: Some(true),
        conflicts: None,
        non_git: false,
    });
    assert!(evaluate_closeout(&facts, &[])
        .advisories
        .contains(&"workspace_observation_incomplete"));
    facts.workspace = Observation::Observed(WorkspaceFacts {
        clean: None,
        conflicts: None,
        non_git: true,
    });
    assert_eq!(evaluate_closeout(&facts, &[]).task_status, TaskStatus::Pass);
    assert_eq!(facts.workspace_clean(), None);
}

#[test]
fn hygiene_risk_and_active_jobs_remain_independent_blockers() {
    let mut facts = clean_facts();
    facts.hygiene = Some(Observation::Observed(HygieneFacts {
        clean: Some(false),
        secret_like_paths: 1,
        truncated: true,
    }));
    facts.jobs = JobFacts {
        blocking_active_count: 1,
        terminal_pending_count: 1,
    };
    let result = evaluate_closeout(&facts, &[]);
    assert_eq!(
        result.blocking_reasons,
        ["sensitive_path_risk", "blocking_active_jobs"]
    );
    assert!(result.advisories.contains(&"workspace_hygiene_truncated"));
    assert!(result.advisories.contains(&"jobs_terminal_pending"));
}

#[test]
fn only_current_unresolved_validation_failure_blocks() {
    let mut facts = clean_facts();
    facts.validation.status = Some(ValidationStatus::Failed);
    facts.validation.current_evidence_status = Some(ValidationStatus::Failed);
    facts.validation.current_unresolved_failure_count = 1;
    facts.validation.historical_failures_unresolved = true;
    assert!(evaluate_closeout(&facts, &[])
        .blocking_reasons
        .contains(&"validation_failed"));
    facts.validation.current_evidence_status = Some(ValidationStatus::Passed);
    facts.validation.current_unresolved_failure_count = 0;
    let current_pass = evaluate_closeout(&facts, &[]);
    assert!(!current_pass.blocking());
    assert_eq!(current_pass.history_status, EvidenceHistoryStatus::Failed);
    assert!(current_pass
        .informational_notes
        .iter()
        .any(|note| note.contains("not current workspace blockers")));
}

#[test]
fn stale_history_and_unproven_success_never_upgrade_current_evidence() {
    let mut facts = clean_facts();
    facts.validation.current_evidence_status = Some(ValidationStatus::Stale);
    facts.validation.current_unresolved_failure_count = 1;
    let stale = evaluate_closeout(&facts, &[]);
    assert!(!stale.blocking());
    assert!(stale.advisories.contains(&"validation_stale_after_changes"));
    facts.validation.current_evidence_status = Some(ValidationStatus::Unproven);
    let unproven = evaluate_closeout(&facts, &[]);
    assert_eq!(unproven.task_status, TaskStatus::Warn);
    assert!(unproven
        .actions
        .iter()
        .any(|action| action == UNPROVEN_SOURCE_REVIEW_ACTION));
}

#[test]
fn expectation_integrity_is_separate_from_task_correctness() {
    let mut facts = clean_facts();
    facts.failures.unexpected_success_count = 1;
    let result = evaluate_closeout(&facts, &[]);
    assert_eq!(result.task_status, TaskStatus::Pass);
    assert_eq!(result.integrity_status, IntegrityStatus::Warning);
    assert_eq!(result.legacy_status, TaskStatus::Warn);
    assert!(result.task_warning_reasons.is_empty());
    facts.failures.expectation_mismatch_count = 1;
    let mismatch = evaluate_closeout(&facts, &[]);
    assert!(mismatch.blocking());
    assert_eq!(mismatch.integrity_status, IntegrityStatus::Error);
}

#[test]
fn historical_non_actionable_failures_do_not_block() {
    let mut facts = clean_facts();
    facts.failures.unexpected_count = 2;
    facts.failures.non_actionable_unexpected_count = 2;
    let result = evaluate_closeout(&facts, &[]);
    assert!(!result.blocking());
    assert!(result
        .informational_notes
        .iter()
        .any(|note| note.contains("non-actionable")));
    facts.failures.actionable_unexpected_count = 1;
    assert!(evaluate_closeout(&facts, &[]).blocking());
}

#[test]
fn review_only_work_is_advisory_not_mandatory_test_execution() {
    let mut facts = clean_facts();
    facts.validation.status = Some(ValidationStatus::NotRun);
    facts.review_evidence_total = 1;
    let result = evaluate_closeout(&facts, &[]);
    assert!(!result.blocking());
    assert!(result
        .advisories
        .contains(&"validation_not_run_with_review_evidence"));
}

#[test]
fn guidance_deduplicates_against_captured_initial_actions() {
    let mut facts = clean_facts();
    facts.jobs.blocking_active_count = 1;
    let initial = vec!["stop or await blocking active jobs".to_string()];
    let result = evaluate_closeout(&facts, &initial);
    assert_eq!(result.actions, initial);
}
