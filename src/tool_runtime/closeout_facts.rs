//! Typed closeout policy. No transport, runtime state or presentation JSON lives here.
//! Observation failure is not a successful observation whose counters happen to be zero.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Observation<T> {
    #[default]
    NotChecked,
    Failed,
    Observed(T),
}

impl<T> Observation<T> {
    pub(crate) fn observed(&self) -> Option<&T> {
        match self {
            Self::Observed(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn was_checked(&self) -> bool {
        !matches!(self, Self::NotChecked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceFacts {
    pub(crate) clean: Option<bool>,
    pub(crate) conflicts: Option<u64>,
    pub(crate) non_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HygieneFacts {
    pub(crate) clean: Option<bool>,
    pub(crate) secret_like_paths: u64,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct JobFacts {
    pub(crate) blocking_active_count: u64,
    pub(crate) terminal_pending_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ToolFailureFacts {
    pub(crate) expected_count: u64,
    pub(crate) unexpected_count: u64,
    pub(crate) actionable_unexpected_count: u64,
    pub(crate) non_actionable_unexpected_count: u64,
    pub(crate) expectation_mismatch_count: u64,
    pub(crate) unexpected_success_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationStatus {
    Passed,
    Failed,
    NotRun,
    Stale,
    Unproven,
    Inconclusive,
    Mixed,
    Unknown,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ValidationFacts {
    pub(crate) status: Option<ValidationStatus>,
    pub(crate) current_evidence_status: Option<ValidationStatus>,
    pub(crate) successes: u64,
    pub(crate) failures: u64,
    pub(crate) resolved_failure_count: u64,
    pub(crate) unresolved_failure_count: u64,
    pub(crate) current_unresolved_failure_count: u64,
    pub(crate) historical_failures_resolved: bool,
    pub(crate) historical_failures_unresolved: bool,
    pub(crate) evidence_gap_count: u64,
    pub(crate) cargo_test_zero_tests: bool,
    pub(crate) skipped: bool,
}

impl ValidationFacts {
    pub(crate) fn current_status(&self) -> Option<ValidationStatus> {
        self.current_evidence_status.or(self.status)
    }
    pub(crate) fn validation_skipped(&self) -> bool {
        matches!(
            self.current_status(),
            Some(ValidationStatus::NotRun | ValidationStatus::Stale)
        ) || self.skipped
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CloseoutFacts {
    pub(crate) workspace: Observation<WorkspaceFacts>,
    // None means this consumer does not request a hygiene assessment at all.
    pub(crate) hygiene: Option<Observation<HygieneFacts>>,
    pub(crate) jobs: JobFacts,
    pub(crate) validation: ValidationFacts,
    pub(crate) failures: ToolFailureFacts,
    pub(crate) review_evidence_total: u64,
}

impl CloseoutFacts {
    pub(crate) fn workspace_clean(&self) -> Option<bool> {
        self.workspace.observed().and_then(|facts| facts.clean)
    }
    pub(crate) fn workspace_conflicts(&self) -> Option<u64> {
        self.workspace.observed().and_then(|facts| facts.conflicts)
    }
    pub(crate) fn hygiene_observed(&self) -> Option<&HygieneFacts> {
        self.hygiene.as_ref().and_then(Observation::observed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Pass,
    Warn,
    Fail,
}
impl TaskStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegrityStatus {
    Clean,
    Warning,
    Error,
}
impl IntegrityStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceHistoryStatus {
    Clean,
    MixedResolved,
    MixedUnresolved,
    Failed,
}
impl EvidenceHistoryStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::MixedResolved => "mixed_resolved",
            Self::MixedUnresolved => "mixed_unresolved",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloseoutDecision {
    pub(crate) blocking_reasons: Vec<&'static str>,
    pub(crate) task_warning_reasons: Vec<&'static str>,
    pub(crate) advisories: Vec<&'static str>,
    pub(crate) integrity_errors: Vec<&'static str>,
    pub(crate) integrity_warnings: Vec<&'static str>,
    pub(crate) informational_notes: Vec<&'static str>,
    pub(crate) actions: Vec<String>,
    pub(crate) task_status: TaskStatus,
    pub(crate) integrity_status: IntegrityStatus,
    pub(crate) legacy_status: TaskStatus,
    pub(crate) history_status: EvidenceHistoryStatus,
}
impl CloseoutDecision {
    pub(crate) fn blocking(&self) -> bool {
        self.task_status == TaskStatus::Fail
    }
    pub(crate) fn legacy_blocking(&self) -> bool {
        self.blocking() || self.integrity_status == IntegrityStatus::Error
    }
}

pub(crate) const UNPROVEN_SOURCE_REVIEW_ACTION: &str =
    "review source_state and external workspace stability; rerunning validation alone cannot prove current source";
pub(crate) const VALIDATION_IDENTITY_REUSE_ACTION: &str =
    "address the current validation failure; when intentionally rerunning it, reuse the original assertion_name when supplied and the same validation identity";

pub(crate) fn evaluate_closeout(
    facts: &CloseoutFacts,
    initial_actions: &[String],
) -> CloseoutDecision {
    let mut blocking_reasons = Vec::new();
    let mut warning_reasons = Vec::new();
    let mut integrity_errors = Vec::new();
    let mut integrity_warnings = Vec::new();
    let mut informational_notes = Vec::new();
    let mut actions = initial_actions.to_vec();
    match &facts.workspace {
        Observation::NotChecked => {
            push_unique(&mut warning_reasons, "workspace_not_checked");
            push_action(
                &mut actions,
                "run read_workspace_changes before final handoff",
            );
        }
        Observation::Failed => {
            push_unique(&mut warning_reasons, "workspace_observation_failed");
            push_action(
                &mut actions,
                "resolve workspace observation failure before closeout",
            );
        }
        Observation::Observed(workspace) if !workspace.non_git && workspace.conflicts.is_none() => {
            push_unique(&mut warning_reasons, "workspace_observation_incomplete");
            push_action(
                &mut actions,
                "run read_workspace_changes before final handoff",
            );
        }
        Observation::Observed(_) => {}
    }
    if facts.workspace_conflicts().is_some_and(|count| count > 0) {
        push_unique(&mut blocking_reasons, "workspace_conflicts");
        push_action(&mut actions, "resolve workspace conflicts before closeout");
    } else if facts.workspace_clean() == Some(false) {
        push_unique(&mut warning_reasons, "workspace_dirty");
        push_action(
            &mut actions,
            "review workspace changes with read_workspace_changes",
        );
    }
    match facts.hygiene.as_ref() {
        Some(Observation::NotChecked) => {
            push_unique(&mut warning_reasons, "hygiene_not_checked");
            push_action(&mut actions, "run check_workspace_hygiene before closeout");
        }
        Some(Observation::Failed) => {
            push_unique(&mut warning_reasons, "hygiene_observation_failed");
            push_action(
                &mut actions,
                "resolve workspace hygiene observation failure before closeout",
            );
        }
        _ => {}
    }
    if let Some(hygiene) = facts.hygiene_observed() {
        if hygiene.clean == Some(false) {
            push_unique(&mut warning_reasons, "workspace_hygiene_findings");
            push_action(&mut actions, "review workspace hygiene before closeout");
        }
        if hygiene.secret_like_paths > 0 {
            push_unique(&mut blocking_reasons, "sensitive_path_risk");
            push_action(&mut actions, "review secret-like paths before closeout");
        }
        if hygiene.truncated {
            push_unique(&mut warning_reasons, "workspace_hygiene_truncated");
        }
    }
    if facts.jobs.blocking_active_count > 0 {
        push_unique(&mut blocking_reasons, "blocking_active_jobs");
        push_action(&mut actions, "stop or await blocking active jobs");
    }
    if facts.jobs.terminal_pending_count > 0 {
        push_unique(&mut warning_reasons, "jobs_terminal_pending");
    }

    let failures = &facts.failures;
    if failures.actionable_unexpected_count > 0 {
        push_unique(&mut blocking_reasons, "unexpected_tool_failures");
        push_action(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    }
    if failures.expectation_mismatch_count > 0 {
        push_unique(&mut blocking_reasons, "expectation_mismatches");
        push_unique(&mut integrity_errors, "expectation_mismatches");
        push_action(
            &mut actions,
            "review result expectation mismatches before proceeding",
        );
    }
    if failures.unexpected_success_count > 0 {
        push_unique(&mut integrity_warnings, "unexpected_successes");
        push_action(
            &mut actions,
            "review failure expectations that unexpectedly succeeded",
        );
    }
    if failures.expected_count > 0
        && failures.unexpected_count == 0
        && failures.expectation_mismatch_count == 0
        && failures.unexpected_success_count == 0
    {
        push_unique(
            &mut informational_notes,
            "declared result expectations matched",
        );
    }
    if failures.non_actionable_unexpected_count > 0 {
        push_unique(
            &mut informational_notes,
            "non-actionable failed tool calls are retained as historical/process evidence",
        );
    }
    let validation = &facts.validation;
    if validation.evidence_gap_count > 0 {
        push_unique(
            &mut informational_notes,
            "validation evidence gaps are retained separately from correctness failures",
        );
    }
    let history_status = if validation.historical_failures_resolved {
        EvidenceHistoryStatus::MixedResolved
    } else {
        match validation.status {
            Some(ValidationStatus::Mixed) => EvidenceHistoryStatus::MixedUnresolved,
            Some(ValidationStatus::Failed) => EvidenceHistoryStatus::Failed,
            _ if validation.historical_failures_unresolved => {
                EvidenceHistoryStatus::MixedUnresolved
            }
            _ => EvidenceHistoryStatus::Clean,
        }
    };
    match validation.current_status() {
        Some(ValidationStatus::NotRun) => {
            if facts.review_evidence_total > 0 {
                push_unique(
                    &mut warning_reasons,
                    "validation_not_run_with_review_evidence",
                );
                push_action(
                    &mut actions,
                    "decide whether task-appropriate validation is needed before closeout",
                );
            } else {
                push_unique(&mut warning_reasons, "validation_not_run");
                push_action(
                    &mut actions,
                    "run validation or review before closeout when applicable",
                );
            }
        }
        Some(ValidationStatus::Stale) => {
            push_unique(&mut warning_reasons, "validation_stale_after_changes");
            push_action(
                &mut actions,
                "run task-appropriate validation when warranted",
            );
        }
        Some(ValidationStatus::Failed) if validation.current_unresolved_failure_count > 0 => {
            push_unique(&mut blocking_reasons, "validation_failed");
            push_action(&mut actions, VALIDATION_IDENTITY_REUSE_ACTION);
        }
        Some(ValidationStatus::Unproven) => {
            push_unique(&mut warning_reasons, "validation_inconclusive");
            push_action(&mut actions, UNPROVEN_SOURCE_REVIEW_ACTION);
        }
        Some(ValidationStatus::Inconclusive) => {
            push_unique(&mut warning_reasons, "validation_inconclusive");
            push_action(&mut actions, "run validation that proves the intended test assertion, or explicitly set require_tests=false when zero tests are intentional");
        }
        Some(ValidationStatus::Unknown) | None => {
            push_unique(&mut warning_reasons, "validation_unknown");
        }
        Some(_) => {}
    }
    if validation.historical_failures_resolved {
        push_unique(
            &mut informational_notes,
            "historical validation failures were resolved by later successful validation",
        );
    } else if validation.historical_failures_unresolved
        && validation.current_unresolved_failure_count == 0
    {
        push_unique(&mut informational_notes, "historical validation failures remain without exact identity resolution but are not current workspace blockers");
    }
    if validation.cargo_test_zero_tests {
        push_unique(&mut integrity_warnings, "cargo_test_zero_tests");
        push_action(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
    }
    if actions.is_empty() {
        actions.push("proceed with handoff or closeout".to_string());
    }
    let task_status = if !blocking_reasons.is_empty() {
        TaskStatus::Fail
    } else if warning_reasons.is_empty() {
        TaskStatus::Pass
    } else {
        TaskStatus::Warn
    };
    let integrity_status = if !integrity_errors.is_empty() {
        IntegrityStatus::Error
    } else if !integrity_warnings.is_empty() {
        IntegrityStatus::Warning
    } else {
        IntegrityStatus::Clean
    };
    let task_warning_reasons = warning_reasons.clone();
    for reason in &integrity_warnings {
        push_unique(&mut warning_reasons, *reason);
    }
    let legacy_status =
        if task_status == TaskStatus::Fail || integrity_status == IntegrityStatus::Error {
            TaskStatus::Fail
        } else if task_status == TaskStatus::Warn || integrity_status == IntegrityStatus::Warning {
            TaskStatus::Warn
        } else {
            TaskStatus::Pass
        };
    CloseoutDecision {
        blocking_reasons,
        task_warning_reasons,
        advisories: warning_reasons,
        integrity_errors,
        integrity_warnings,
        informational_notes,
        actions,
        task_status,
        integrity_status,
        legacy_status,
        history_status,
    }
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}
fn push_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

#[cfg(test)]
mod tests;
