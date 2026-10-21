//! One captured Session evidence view for closeout, handoff and passive results.
//! Validation owns its typed evidence; JSON is produced only for presentation.
use super::closeout_facts::{
    CloseoutFacts, HygieneFacts, JobFacts, Observation, ToolFailureFacts, WorkspaceFacts,
};
use super::ToolRuntime;
use crate::auth::AuthContext;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use webcodex_tool_contracts::{
    runtime_tool_session_evidence_policy, ToolFailureEvidence, ToolReviewEvidence,
};
use webcodex_validation::ValidationReport;
use webcodex_workflow_session::{
    canonical_tool_call_finished_events, tool_failure_evidence_from_events, SessionEvent,
    SessionSummary, ToolFailureSummary, TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
};

pub(crate) const CLOSEOUT_SESSION_EVENT_LIMIT: usize = 200;

pub(crate) struct CloseoutSessionEvidence {
    pub(crate) summary: SessionSummary,
    pub(crate) validation: ValidationReport,
    pub(crate) failures: CloseoutToolFailures,
    pub(crate) review: ReviewEvidence,
    // Existing Session fence only; no new public generation or authority.
    pub(crate) revision: Option<(u64, u64)>,
    validation_requested: bool,
}

impl CloseoutSessionEvidence {
    pub(crate) fn new(
        summary: SessionSummary,
        validation: ValidationReport,
        revision: Option<(u64, u64)>,
    ) -> Self {
        let failures = CloseoutToolFailures::from_summary(&summary, &validation, 10);
        let review = ReviewEvidence::from_events(&summary.events);
        Self {
            summary,
            validation,
            failures,
            review,
            revision,
            validation_requested: true,
        }
    }

    pub(crate) fn validation_value(&self, limit: usize) -> Value {
        if self.validation_requested {
            self.validation.bounded_value(limit)
        } else {
            ValidationReport::skipped().to_value()
        }
    }

    pub(crate) fn facts(
        &self,
        workspace: Observation<WorkspaceFacts>,
        hygiene: Option<Observation<HygieneFacts>>,
        jobs: JobFacts,
    ) -> CloseoutFacts {
        CloseoutFacts {
            workspace,
            hygiene,
            jobs,
            validation: if self.validation_requested {
                self.validation.facts()
            } else {
                ValidationReport::skipped().facts()
            },
            failures: self.failures.facts(),
            review_evidence_total: self.review.total,
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn collect_closeout_session_evidence(
        &self,
        summary: &SessionSummary,
        validation_requested: bool,
        limit: usize,
        auth: Option<&AuthContext>,
    ) -> CloseoutSessionEvidence {
        let revision = self.sessions.handoff_revision(&summary.session_id);
        let summary = self
            .sessions
            .summary(&summary.session_id, Some(CLOSEOUT_SESSION_EVENT_LIMIT))
            .unwrap_or_else(|| summary.clone());
        self.observe_closeout_session_evidence(summary, revision, validation_requested, limit, auth)
            .await
    }

    pub(crate) async fn observe_closeout_session_evidence(
        &self,
        summary: SessionSummary,
        revision: Option<(u64, u64)>,
        validation_requested: bool,
        limit: usize,
        auth: Option<&AuthContext>,
    ) -> CloseoutSessionEvidence {
        // Job materialization may enrich the validation report and append
        // authoritative terminal evidence. It must not replace the task snapshot
        // already captured before workspace/Job reads. The original revision
        // detects concurrent corrections and requires an exact handoff reread.
        let validation = if validation_requested {
            Some(
                self.validation_report_for_session_with_jobs(&summary, limit, auth)
                    .await,
            )
        } else {
            None
        };
        let summary = self.refresh_validation_source_summary(&summary);
        let validation = validation
            .unwrap_or_else(|| ValidationReport::from_events(&summary, &summary.events, limit));
        let mut evidence = CloseoutSessionEvidence::new(summary, validation, revision);
        evidence.validation_requested = validation_requested;
        evidence
    }

    pub(crate) fn passive_closeout_session_evidence(
        &self,
        summary: &SessionSummary,
        limit: usize,
    ) -> CloseoutSessionEvidence {
        let summary = self.refresh_validation_source_summary(summary);
        let validation = ValidationReport::from_events(&summary, &summary.events, limit);
        // Startup may intentionally supply the pre-instruction attempt. Do not
        // associate that historical snapshot with a newly sampled live fence.
        CloseoutSessionEvidence::new(summary, validation, None)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CloseoutToolFailures {
    #[serde(flatten)]
    summary: ToolFailureSummary,
    non_actionable_unexpected_count: u64,
    actionable_unexpected_count: u64,
    #[serde(skip)]
    pub(crate) actionable_unexpected_event_ids: HashSet<String>,
}

impl CloseoutToolFailures {
    fn from_summary(summary: &SessionSummary, validation: &ValidationReport, limit: usize) -> Self {
        let raw = tool_failure_evidence_from_events(&summary.events, limit);
        let unexpected_events = canonical_tool_call_finished_events(&summary.events)
            .into_iter()
            .filter(|event| unexpected_failure_event(event))
            .collect::<Vec<_>>();
        let non_actionable_event_ids = unexpected_events
            .iter()
            .copied()
            .filter(|event| {
                validation
                    .non_actionable_tool_failure_event_ids()
                    .contains(&event.event_id)
                    || unexpected_failure_is_proven_non_actionable(event)
            })
            .map(|event| event.event_id.clone())
            .collect::<HashSet<_>>();
        let non_actionable_unexpected_count =
            (non_actionable_event_ids.len() as u64).min(raw.unexpected_count as u64);
        let actionable_unexpected_event_ids = unexpected_events
            .into_iter()
            .filter(|event| !non_actionable_event_ids.contains(&event.event_id))
            .map(|event| event.event_id.clone())
            .collect();
        Self {
            actionable_unexpected_count: (raw.unexpected_count as u64)
                .saturating_sub(non_actionable_unexpected_count),
            non_actionable_unexpected_count,
            actionable_unexpected_event_ids,
            summary: raw,
        }
    }

    pub(crate) fn facts(&self) -> ToolFailureFacts {
        ToolFailureFacts {
            expected_count: self.summary.expected_count as u64,
            unexpected_count: self.summary.unexpected_count as u64,
            actionable_unexpected_count: self.actionable_unexpected_count,
            non_actionable_unexpected_count: self.non_actionable_unexpected_count,
            expectation_mismatch_count: self.summary.expectation_mismatch_count as u64,
            unexpected_success_count: self.summary.unexpected_success_count as u64,
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("closeout failures serialize")
    }

    pub(crate) fn with_display_events(&self, events: &[SessionEvent]) -> Value {
        let mut display = self.clone();
        let recent = tool_failure_evidence_from_events(events, 10);
        display.summary.recent_expected = recent.recent_expected;
        display.summary.recent_unexpected = recent.recent_unexpected;
        display.summary.recent_mismatches = recent.recent_mismatches;
        display.summary.recent_unexpected_successes = recent.recent_unexpected_successes;
        display.to_value()
    }
}

fn unexpected_failure_event(event: &SessionEvent) -> bool {
    event.kind == "tool_call_finished"
        && event.status.as_deref() == Some("failed")
        && event
            .failure_expectation_result
            .as_deref()
            .unwrap_or(TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE)
            == TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE
}

fn unexpected_failure_is_proven_non_actionable(event: &SessionEvent) -> bool {
    let effect = event.effect_evidence.as_ref();
    let execution_state = effect.and_then(|effect| effect.execution_state.as_deref());
    if event.failure_kind.as_deref() == Some("outcome_unknown")
        || event.error_kind.as_deref() == Some("outcome_unknown")
        || execution_state == Some("outcome_unknown")
    {
        return false;
    }
    // ExecutionPurpose is caller-declared evidence metadata, not effect-safety
    // proof. Once a generic shell/process command has started, a failed result
    // remains actionable regardless of whether its declared purpose was
    // diagnostic, validation, or anything else.
    if effect.and_then(|effect| effect.command_started) == Some(true)
        || matches!(
            execution_state,
            Some("started" | "completed" | "cancelled" | "timed_out")
        )
    {
        return false;
    }
    if effect.and_then(|effect| effect.command_started) == Some(false)
        || execution_state == Some("not_started")
    {
        return true;
    }
    let failure_evidence = runtime_tool_session_evidence_policy(&event.tool_name).failure;
    if effect.and_then(|effect| effect.state_changed) == Some(false)
        && ((!event.shell_like && !event.git_like)
            || failure_evidence == ToolFailureEvidence::ProvenNoStateChangeNonActionable)
    {
        return true;
    }
    event.read_like && !event.write_like && !event.shell_like && !event.git_like
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReviewEvidence {
    available: bool,
    source: &'static str,
    read_only_inspection_count: u64,
    search_count: u64,
    diff_review_count: u64,
    workspace_review_count: u64,
    hygiene_review_count: u64,
    pub(crate) total: u64,
    tools: Vec<String>,
}

impl ReviewEvidence {
    pub(crate) fn from_events(events: &[SessionEvent]) -> Self {
        let mut evidence = Self {
            available: true,
            source: "session_ledger",
            read_only_inspection_count: 0,
            search_count: 0,
            diff_review_count: 0,
            workspace_review_count: 0,
            hygiene_review_count: 0,
            total: 0,
            tools: Vec::new(),
        };
        for event in canonical_tool_call_finished_events(events) {
            if event.status.as_deref() != Some("succeeded") {
                continue;
            }
            match runtime_tool_session_evidence_policy(&event.tool_name).review {
                ToolReviewEvidence::None => continue,
                ToolReviewEvidence::ReadOnlyInspection => evidence.read_only_inspection_count += 1,
                ToolReviewEvidence::Search => evidence.search_count += 1,
                ToolReviewEvidence::DiffReview => evidence.diff_review_count += 1,
                ToolReviewEvidence::WorkspaceReview => {
                    evidence.workspace_review_count += 1;
                    if event.diff_review_like {
                        evidence.diff_review_count += 1;
                    }
                }
                ToolReviewEvidence::HygieneReview => {
                    evidence.workspace_review_count += 1;
                    evidence.hygiene_review_count += 1;
                }
            }
            evidence.total += 1;
            if !evidence.tools.contains(&event.tool_name) {
                evidence.tools.push(event.tool_name.clone());
            }
        }
        evidence
    }
    pub(crate) fn to_value(&self) -> Value {
        json!(self)
    }
}
