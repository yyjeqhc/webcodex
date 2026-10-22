use super::access_control::{
    assert_runner_access_typed, job_visible_to_access as shell_job_visible_to_auth,
};
use super::execution_admission::ExecutionAdmissionError;
use super::jobs::{
    append_log_limited, assert_active_instance_locked, is_final_job_status, job_view,
    notify_job_heartbeat, notify_job_update, observe_job_terminal, parse_job_lifecycle,
    refresh_job_status_locked, select_log_lines,
};
use super::reconciliation::validate_stream_snapshot;
use super::requests::{
    enqueue_pending_request_locked, next_request_id, notify_runner_locked,
    retire_job_request_locked,
};
use super::state::{
    DetachedIdempotencyIntent, JobLifecycleState, JobObservationState, JobRecoveryPhase,
    JobRecoveryReason, JobRecoveryState, ShellJobRecord, ShellJobVisibility,
};
use super::validation::{validate_id, validate_runner_instance_id};
use super::{now_ts, RunnerFeature, RunnerRegistry};
use crate::DetachedInitiatorIdentity;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use webcodex_core::runner_operation::{
    RunnerInvocationMetadata, RunnerJobOperation, RunnerOperation,
};
use webcodex_core::runner_protocol::{
    validation_infrastructure_failure_code, RunnerJobUpdateRequest, RunnerRequest,
    ShellCommandExecutionState, ShellJobActivity, ShellJobActivityPhase, ShellJobActivitySource,
    ShellJobInfo, ShellJobValidationMetadata, ShellJobValidationStep, ShellProcessArgv,
    ShellScriptPayload, DETACHED_IDEMPOTENCY_KEY_MAX_BYTES,
};
use webcodex_core::runner_skill::RunnerSkillExecutionRequest;
use webcodex_core::workflow_session_contract::ExecutionShell;

mod start;
pub use start::{
    JobShellTarget, JobStartContext, JobStartExecution, JobStartRequest, JobValidationCorrelation,
    JobValidationPlan,
};

#[derive(Clone, Copy)]
struct ValidationProtocolError(&'static str);

/// Minimal typed facts for Runtime compact status. No Job DTO, command
/// preview, validation payload, stream cursor or terminal history is retained.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeJobAggregateSnapshot {
    pub active_count: usize,
    pub running_count: usize,
    pub queued_count: usize,
    pub recovering_count: usize,
    pub lost_after_reconcile_count: usize,
    pub by_runner: HashMap<String, RuntimeJobRunnerCounts>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeJobRunnerCounts {
    pub active: usize,
    pub running: usize,
    pub queued: usize,
}

/// Outcome of a bounded `job_log`/`job_tail` wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JobLogWaitOutcome {
    /// No wait was performed, or a returnable new state already existed at
    /// call time (a job advanced past `after_observation_token`).
    Immediate,
    /// A non-terminal update was observed after waiting.
    Updated,
    /// The job was (or became) terminal during the call.
    Terminal,
    /// The wait deadline elapsed with no observable change. This is a normal
    /// successful result, not a timeout failure.
    Timeout,
}

impl JobLogWaitOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Updated => "updated",
            Self::Terminal => "terminal",
            Self::Timeout => "timeout",
        }
    }
}

/// Wait metadata returned by a bounded `job_log`/`job_tail` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobLogWait {
    pub wait_outcome: JobLogWaitOutcome,
    pub waited_ms: u64,
    /// Whether the job changed relative to the supplied `after_observation_token`.
    /// Always false when no `after_observation_token` was provided.
    pub changed: bool,
    /// Whether a semantic/log/activity/lifecycle/recovery change happened after
    /// the supplied observation token. False for a sequence-only heartbeat.
    pub meaningful_changed: bool,
    /// Whether the only observed revision advance since the supplied token was
    /// sequence-only liveness traffic.
    pub heartbeat_changed: bool,
    /// Whether the job is terminal per the canonical job terminal definition.
    pub terminal: bool,
}

impl Default for JobLogWait {
    fn default() -> Self {
        Self {
            wait_outcome: JobLogWaitOutcome::Immediate,
            waited_ms: 0,
            changed: false,
            meaningful_changed: false,
            heartbeat_changed: false,
            terminal: false,
        }
    }
}

fn observation_change_flags(
    job: &ShellJobRecord,
    after: Option<&webcodex_core::job_observation::JobObservationToken>,
) -> (bool, bool, bool) {
    let Some(token) = after else {
        return (false, false, false);
    };
    let parent_changed = !token.matches_parent(&job.job_id, &job.observation.epoch);
    let revision = job.observation.revision.load(Ordering::Relaxed);
    let changed = parent_changed || token.revision != revision;
    let meaningful_changed = changed
        && (parent_changed
            || job
                .observation
                .last_meaningful_revision
                .load(Ordering::Relaxed)
                > token.revision);
    let heartbeat_changed = changed && !meaningful_changed;
    (changed, meaningful_changed, heartbeat_changed)
}

pub const MAX_JOB_TELEMETRY_SNAPSHOTS: usize = 9;

/// Content-free Server facts for bounded, observation-only audit correlation.
#[derive(Debug, Clone)]
pub struct JobTelemetrySnapshot {
    pub job_id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub terminal_observed_at: Option<i64>,
}

/// Frozen, read-only Server record for passive attention. Validation excerpts
/// stay internal and are bounded by the canonical retained Job log limits.
#[derive(Debug, Clone)]
pub struct JobAttentionSnapshot {
    pub job: ShellJobInfo,
    pub validation_output: Option<JobValidationOutput>,
    /// Canonical recovery overlay semantics, excluding routine timestamps.
    pub recovery: Option<(crate::JobRecoveryPhase, Option<crate::JobRecoveryReason>)>,
}

#[derive(Debug, Clone)]
pub struct JobValidationOutput {
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

/// Frozen Server-side observation details accompanying one public Job log
/// projection. The analysis context is bounded by the retained Server log and
/// is consumed only by validation/summary projection; it is never repeated as
/// model-facing output when the delta is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellJobLogObservation {
    pub wait: JobLogWait,
    pub log_delta_status: webcodex_core::job_observation::JobLogDeltaStatus,
    pub stdout_delta_reset: bool,
    pub stderr_delta_reset: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_returned_lines: usize,
    pub stderr_returned_lines: usize,
    pub analysis_stdout: String,
    pub analysis_stderr: String,
    pub analysis_truncated: bool,
}

impl std::ops::Deref for ShellJobLogObservation {
    type Target = JobLogWait;

    fn deref(&self) -> &Self::Target {
        &self.wait
    }
}

impl Default for ShellJobLogObservation {
    fn default() -> Self {
        Self {
            wait: JobLogWait::default(),
            log_delta_status: webcodex_core::job_observation::JobLogDeltaStatus::Baseline,
            stdout_delta_reset: false,
            stderr_delta_reset: false,
            stdout_truncated: false,
            stderr_truncated: false,
            stdout_returned_lines: 0,
            stderr_returned_lines: 0,
            analysis_stdout: String::new(),
            analysis_stderr: String::new(),
            analysis_truncated: false,
        }
    }
}

fn frozen_shell_job_log_projection(
    job: &ShellJobRecord,
    after: Option<&webcodex_core::job_observation::JobObservationToken>,
    since_stdout_line: Option<usize>,
    since_stderr_line: Option<usize>,
    tail_lines: Option<usize>,
    wait: JobLogWait,
) -> (
    ShellJobInfo,
    Option<String>,
    Option<String>,
    usize,
    usize,
    ShellJobLogObservation,
) {
    let (analysis_stdout, _, _, analysis_stdout_truncated) =
        select_log_lines(&job.stdout, None, None);
    let (analysis_stderr, _, _, analysis_stderr_truncated) =
        select_log_lines(&job.stderr, None, None);
    let analysis_stdout = analysis_stdout.unwrap_or_default();
    let analysis_stderr = analysis_stderr.unwrap_or_default();
    let analysis_truncated = analysis_stdout_truncated
        || analysis_stderr_truncated
        || job.stdout.first_retained_line > 1
        || job.stderr.first_retained_line > 1;
    let explicit_paging = since_stdout_line.is_some() || since_stderr_line.is_some();

    if explicit_paging {
        // Preserve the established explicit cursor/tail precedence exactly.
        // The cursor-less token prevents a later automatic observation from
        // assuming that this explicit page represented complete log receipt.
        let (stdout, next_stdout_line, _, stdout_truncated) =
            select_log_lines(&job.stdout, since_stdout_line, tail_lines);
        let (stderr, next_stderr_line, _, stderr_truncated) =
            select_log_lines(&job.stderr, since_stderr_line, tail_lines);
        let mut view = job_view(job);
        view.observation_token = webcodex_core::job_observation::JobObservationToken::new_baseline(
            job.job_id.clone(),
            &job.observation.epoch,
            job.observation.revision.load(Ordering::Relaxed),
        )
        .ok()
        .map(|token| token.encode());
        let observation = ShellJobLogObservation {
            wait,
            log_delta_status: webcodex_core::job_observation::JobLogDeltaStatus::Baseline,
            stdout_delta_reset: false,
            stderr_delta_reset: false,
            stdout_truncated,
            stderr_truncated,
            stdout_returned_lines: stdout.as_deref().unwrap_or_default().lines().count(),
            stderr_returned_lines: stderr.as_deref().unwrap_or_default().lines().count(),
            analysis_stdout,
            analysis_stderr,
            analysis_truncated,
        };
        return (
            view,
            stdout,
            stderr,
            next_stdout_line,
            next_stderr_line,
            observation,
        );
    }

    let epoch_matches =
        after.is_none_or(|token| token.matches_parent(&job.job_id, &job.observation.epoch));
    let base_mode = match after {
        None => webcodex_core::job_observation::JobLogSelectionMode::Baseline,
        Some(token) if token.requires_baseline() || !epoch_matches => {
            webcodex_core::job_observation::JobLogSelectionMode::Reset
        }
        Some(token) => webcodex_core::job_observation::JobLogSelectionMode::Delta {
            cursor: token
                .stdout_cursor
                .expect("cursor-aware token has stdout cursor"),
        },
    };
    let stderr_mode = match after {
        None => webcodex_core::job_observation::JobLogSelectionMode::Baseline,
        Some(token) if token.requires_baseline() || !epoch_matches => {
            webcodex_core::job_observation::JobLogSelectionMode::Reset
        }
        Some(token) => webcodex_core::job_observation::JobLogSelectionMode::Delta {
            cursor: token
                .stderr_cursor
                .expect("cursor-aware token has stderr cursor"),
        },
    };
    let stdout = webcodex_core::job_observation::project_log_stream(
        &job.stdout.tail,
        job.stdout.first_retained_line,
        job.stdout.next_line,
        job.stdout.truncated,
        tail_lines,
        base_mode,
        true,
    );
    let stderr = webcodex_core::job_observation::project_log_stream(
        &job.stderr.tail,
        job.stderr.first_retained_line,
        job.stderr.next_line,
        job.stderr.truncated,
        tail_lines,
        stderr_mode,
        true,
    );
    let mut view = job_view(job);
    view.observation_token = webcodex_core::job_observation::JobObservationToken::new(
        job.job_id.clone(),
        &job.observation.epoch,
        job.observation.revision.load(Ordering::Relaxed),
        stdout.next_line as u64,
        stderr.next_line as u64,
    )
    .ok()
    .map(|token| token.encode());
    let observation = ShellJobLogObservation {
        wait,
        log_delta_status: webcodex_core::job_observation::combined_delta_status(
            base_mode, &stdout, &stderr,
        ),
        stdout_delta_reset: stdout.delta_reset,
        stderr_delta_reset: stderr.delta_reset,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        stdout_returned_lines: stdout.returned_lines,
        stderr_returned_lines: stderr.returned_lines,
        analysis_stdout,
        analysis_stderr,
        analysis_truncated,
    };
    (
        view,
        Some(stdout.text),
        Some(stderr.text),
        stdout.next_line,
        stderr.next_line,
        observation,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JobPublicMutationSignature {
    lifecycle: JobLifecycleState,
    recovery: JobRecoveryState,
    last_update_seq: u64,
    stdout: super::state::ShellJobLogState,
    stderr: super::state::ShellJobLogState,
    error: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    command_execution_state: Option<ShellCommandExecutionState>,
    validation_progress: Option<webcodex_core::runner_protocol::ShellJobValidationProgress>,
    test_count_evidence: Option<webcodex_core::runner_protocol::ShellJobTestCountEvidence>,
    format_mutation: Option<webcodex_core::project_format::ProjectFormatMutationReport>,
    build_artifacts: Option<Vec<webcodex_core::project_build::ProjectBuildArtifact>>,
    activity: Option<ShellJobActivity>,
    recovered_after_server_restart: bool,
    reconciled_at: Option<i64>,
}

fn public_mutation_signature(job: &ShellJobRecord) -> JobPublicMutationSignature {
    JobPublicMutationSignature {
        lifecycle: job.lifecycle,
        recovery: job.recovery.clone(),
        last_update_seq: job.last_update_seq,
        stdout: job.stdout.clone(),
        stderr: job.stderr.clone(),
        error: job.error.clone(),
        started_at: job.started_at,
        ended_at: job.ended_at,
        exit_code: job.exit_code,
        duration_ms: job.duration_ms,
        command_execution_state: job.command_execution_state,
        validation_progress: job.validation_progress.clone(),
        test_count_evidence: job.test_count_evidence.clone(),
        format_mutation: job.format_mutation,
        build_artifacts: job.build_artifacts.clone(),
        activity: job.activity,
        recovered_after_server_restart: job.recovery.recovered_after_server_restart,
        reconciled_at: job.recovery.reconciled_at,
    }
}

fn invalid_progress(code: &'static str) -> Result<(), ValidationProtocolError> {
    Err(ValidationProtocolError(code))
}

fn validate_validation_progress(
    job: &ShellJobRecord,
    update: &RunnerJobUpdateRequest,
    lifecycle: JobLifecycleState,
) -> Result<(), ValidationProtocolError> {
    if job.validation_steps.is_empty() {
        return if update.validation_progress.is_none() {
            Ok(())
        } else {
            invalid_progress("validation_progress_unexpected")
        };
    }
    if job.validation_steps.iter().collect::<HashSet<_>>().len() != job.validation_steps.len() {
        return invalid_progress("validation_plan_invalid");
    }
    if update.finished && !lifecycle.is_terminal() {
        return invalid_progress("validation_progress_invalid");
    }
    let cancelling = matches!(
        lifecycle,
        JobLifecycleState::Stopped
            | JobLifecycleState::Cancelled
            | JobLifecycleState::Timeout
            | JobLifecycleState::TimedOut
            | JobLifecycleState::Lost
    );
    let Some(progress) = update.validation_progress.as_ref() else {
        if matches!(
            lifecycle,
            JobLifecycleState::Queued | JobLifecycleState::RunnerQueued
        ) || cancelling
        {
            return Ok(());
        }
        return invalid_progress("validation_progress_missing");
    };
    if matches!(
        lifecycle,
        JobLifecycleState::Queued | JobLifecycleState::RunnerQueued
    ) || progress.completed > job.validation_steps.len()
    {
        return invalid_progress("validation_progress_invalid");
    }
    let previous = job
        .validation_progress
        .as_ref()
        .map(|progress| progress.completed)
        .unwrap_or(0);
    // The `completed` cursor advances at most one step per update: an equal
    // value is an idempotent replay, `previous + 1` is the legitimate single
    // advance, and anything else is a protocol violation. Skipping ahead past
    // unreported steps (or regressing) corrupts the fail-fast validation plan,
    // so reject both directions rather than only the regression.
    if progress.completed < previous || progress.completed > previous.saturating_add(1) {
        return invalid_progress("validation_progress_invalid");
    }
    let no_active_step = progress.current_step.is_none() && progress.failed_step.is_none();
    let infrastructure_failure = lifecycle == JobLifecycleState::Failed
        && update.finished
        && update.exit_code.is_none()
        && update
            .error
            .as_deref()
            .and_then(validation_infrastructure_failure_code)
            .is_some();
    let valid = if cancelling {
        no_active_step
    } else if infrastructure_failure {
        progress.completed < job.validation_steps.len() && no_active_step
    } else if !lifecycle.is_terminal() {
        let expected = job.validation_steps.get(progress.completed);
        expected.map(String::as_str) == progress.current_step.as_deref()
            && progress.failed_step.is_none()
    } else if lifecycle == JobLifecycleState::Completed && update.exit_code == Some(0) {
        progress.completed == job.validation_steps.len() && no_active_step
    } else if lifecycle == JobLifecycleState::Failed {
        let expected = job.validation_steps.get(progress.completed);
        expected.map(String::as_str) == progress.failed_step.as_deref()
            && progress.current_step.is_none()
    } else {
        false
    };
    if valid {
        Ok(())
    } else if lifecycle == JobLifecycleState::Completed && update.exit_code == Some(0) {
        invalid_progress("validation_progress_incomplete")
    } else {
        invalid_progress("validation_progress_invalid")
    }
}

fn validate_test_count_evidence(
    job: &ShellJobRecord,
    update: &RunnerJobUpdateRequest,
    lifecycle: JobLifecycleState,
) -> Result<(), ValidationProtocolError> {
    let Some(evidence) = update.test_count_evidence.as_ref() else {
        return Ok(());
    };
    let cargo_test = job.validation.as_ref().is_some_and(|metadata| {
        matches!(
            metadata.adapter.as_str(),
            "cargo_test" | "node:vitest:test" | "node:jest:test"
        ) && metadata.kind == "test"
            && metadata.no_run != Some(true)
    });
    if !cargo_test || !lifecycle.is_terminal() || !update.finished || !evidence.is_valid() {
        return invalid_progress("test_count_evidence_invalid");
    }
    Ok(())
}

fn validation_activity_phase(step: &str) -> Option<ShellJobActivityPhase> {
    match step {
        "format" => Some(ShellJobActivityPhase::ValidationFormat),
        "check" => Some(ShellJobActivityPhase::ValidationCheck),
        "test" => Some(ShellJobActivityPhase::ValidationTest),
        _ => None,
    }
}

fn validate_job_activity(
    job: &ShellJobRecord,
    update: &RunnerJobUpdateRequest,
    lifecycle: JobLifecycleState,
) -> Result<(), ValidationProtocolError> {
    let Some(activity) = update.activity else {
        // Activity was added as an optional protocol field. Older Runners may
        // omit it without changing canonical Job lifecycle semantics.
        return Ok(());
    };
    if !activity.is_canonical()
        || !matches!(
            lifecycle,
            JobLifecycleState::Running | JobLifecycleState::StopRequested
        )
    {
        return invalid_progress("job_activity_invalid");
    }
    match activity.source {
        ShellJobActivitySource::RunnerExecution => {
            if !job.validation_steps.is_empty()
                || activity.phase != ShellJobActivityPhase::ProcessRunning
            {
                return invalid_progress("job_activity_invalid");
            }
        }
        ShellJobActivitySource::ValidationPlan => {
            let Some(current_step) = update
                .validation_progress
                .as_ref()
                .and_then(|progress| progress.current_step.as_deref())
            else {
                return invalid_progress("job_activity_invalid");
            };
            if validation_activity_phase(current_step) != Some(activity.phase) {
                return invalid_progress("job_activity_invalid");
            }
        }
        ShellJobActivitySource::CargoOutput => {
            let Some(progress) = update.validation_progress.as_ref() else {
                return invalid_progress("job_activity_invalid");
            };
            if progress.current_step.is_none() || job.validation_steps.is_empty() {
                return invalid_progress("job_activity_invalid");
            }
            // First-class validation metadata retains exact canonical argv, so
            // verify Cargo provenance when that stronger evidence is available.
            // Multi-step checks_run plans retain only step names server-side;
            // there the trusted Runner remains the bounded provenance boundary.
            if let Some(validation) = job.validation.as_ref() {
                if validation.steps.get(progress.completed).is_none_or(|step| {
                    step.program != "cargo"
                        || !(step.is_canonical()
                            || (validation.is_valid() && step.is_project_workspace_cargo()))
                }) {
                    return invalid_progress("job_activity_invalid");
                }
            }
        }
    }
    Ok(())
}

fn validate_command_execution_state(
    job: &ShellJobRecord,
    update: &RunnerJobUpdateRequest,
    lifecycle: JobLifecycleState,
) -> Result<(), ValidationProtocolError> {
    let terminal = lifecycle.is_terminal();
    if !terminal && update.command_execution_state.is_some() {
        return invalid_progress("command_execution_state_on_active_job");
    }
    if job.structured_execution.is_some() && terminal && update.command_execution_state.is_none() {
        return invalid_progress("structured_job_lifecycle_missing");
    }
    let Some(state) = update.command_execution_state else {
        return Ok(());
    };
    let valid = match state {
        ShellCommandExecutionState::NotStarted => {
            matches!(
                lifecycle,
                JobLifecycleState::Failed
                    | JobLifecycleState::Stopped
                    | JobLifecycleState::Cancelled
                    | JobLifecycleState::Lost
            ) && job.started_at.is_none()
        }
        ShellCommandExecutionState::OutcomeUnknown => matches!(
            lifecycle,
            JobLifecycleState::Failed | JobLifecycleState::Lost
        ),
        ShellCommandExecutionState::TimedOut => lifecycle.is_timed_out(),
        ShellCommandExecutionState::Completed => matches!(
            lifecycle,
            JobLifecycleState::Completed
                | JobLifecycleState::Failed
                | JobLifecycleState::Stopped
                | JobLifecycleState::Cancelled
        ),
    };
    if valid {
        Ok(())
    } else {
        invalid_progress("structured_job_lifecycle_invalid")
    }
}

fn validate_detached_idempotency_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > DETACHED_IDEMPOTENCY_KEY_MAX_BYTES
        || key.contains(['\0', '\r', '\n'])
    {
        return Err(format!(
            "detached idempotency_key must be 1..={DETACHED_IDEMPOTENCY_KEY_MAX_BYTES} bytes and contain no NUL/CR/LF"
        ));
    }
    Ok(())
}

fn detached_job_id_for_key(
    initiator: &DetachedInitiatorIdentity,
    key: &str,
) -> Result<String, String> {
    validate_detached_idempotency_key(key)?;
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-detached-initiation-v1\0");
    hasher.update(initiator.as_stable_principal().as_bytes());
    hasher.update(b"\0");
    hasher.update(key.as_bytes());
    Ok(format!(
        "detached_{}",
        webcodex_core::compact::encode(hasher.finalize())
    ))
}

// Caller holds the owning registry lock until insertion; never replace an
// unrelated Job, even when a candidate collides.
pub(crate) fn allocate_job_id<T>(
    jobs: &HashMap<String, T>,
    mut generate: impl FnMut() -> String,
) -> Result<String, String> {
    for _ in 0..16 {
        let candidate = generate();
        if !jobs.contains_key(&candidate) {
            return Ok(candidate);
        }
    }
    Err("job_id_allocation_exhausted".to_string())
}

impl RunnerRegistry {
    /// Validate the execution shape, then admit and atomically submit it against
    /// the current Registry state. No HTTP DTO or String error crosses this API.
    pub async fn start_job(
        &self,
        request: JobStartRequest,
        requested_by: String,
        access: Option<&crate::RunnerAccess>,
        detached_initiator: Option<&DetachedInitiatorIdentity>,
    ) -> Result<ShellJobInfo, ExecutionAdmissionError> {
        let codex = request.codex.clone();
        let plan = start::JobStartPlan::new(request)?;
        let requirements = plan.admission_requirements();
        let detached_intent = plan.detached_intent();
        let mut inner = self.inner.lock().await;
        let job_id = match plan.idempotency_key() {
            Some(key) => detached_job_id_for_key(
                detached_initiator
                    .ok_or_else(|| {
                        "detached idempotency requires a stable authenticated caller identity"
                            .to_string()
                    })
                    .map_err(ExecutionAdmissionError::permission_denied)?,
                key,
            )
            .map_err(ExecutionAdmissionError::invalid_request)?,
            None => allocate_job_id(&inner.jobs_by_id, || {
                format!("wc_job_{}", webcodex_core::compact::random_suffix::<12>())
            })
            .map_err(ExecutionAdmissionError::unavailable)?,
        };
        let job_kind = plan.job_kind();
        let client_id = plan.client_id.clone();
        let request_id = plan.request_id.clone();
        let created_at = plan.created_at;
        let visibility = plan.visibility.clone();
        let job_operation = plan.into_operation(job_id.clone());
        let context = job_operation
            .context()
            .expect("Job start has context")
            .clone();
        let request = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: request_id.clone(),
                client_id: client_id.clone(),
                requested_by,
                created_at,
            },
            RunnerOperation::Job(job_operation),
        )
        .map_err(ExecutionAdmissionError::invalid_request)?;
        let Some(runner) = inner.runners.get(&client_id) else {
            return Err(ExecutionAdmissionError::unknown_runner(client_id.clone()));
        };
        if access.is_some() {
            assert_runner_access_typed(access, runner)?;
        }
        requirements.check(
            &client_id,
            &runner.runner_features,
            &runner.runner_instance_id,
            self.inner.has_receipt_store(),
        )?;
        if context
            .runtime_project_id
            .as_deref()
            .is_some_and(|project| inner.unregistering_projects.contains_key(project))
        {
            return Err(ExecutionAdmissionError::target_changed(
                "project_unregister_in_progress".to_string(),
            ));
        }
        let runner_instance_id = runner.runner_instance_id.clone();
        let auth_group = runner.auth_group.clone();
        let owner_at_admission = runner.owner.clone();
        if let Some(intent) = detached_intent.as_ref() {
            if let Some(existing) = inner.jobs_by_id.get(&job_id) {
                if existing.kind != "run_detached_process" {
                    return Err(ExecutionAdmissionError::idempotency_conflict(
                        "idempotency key resolves to a non-detached Job",
                    ));
                }
                match existing.detached_idempotency_intent.as_ref() {
                    Some(existing_intent) if existing_intent == intent => {
                        return Ok(job_view(existing));
                    }
                    Some(_) => {
                        return Err(ExecutionAdmissionError::idempotency_conflict("idempotency key was already used for different detached process intent"));
                    }
                    None => {
                        return Err(ExecutionAdmissionError::idempotency_recovery(job_id));
                    }
                }
            }
        }
        enqueue_pending_request_locked(
            self.telemetry.as_ref(),
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            None,
            Some(job_id.clone()),
        )?;
        let job = ShellJobRecord {
            job_id: job_id.clone(),
            request_id: Some(request_id.clone()),
            client_id: client_id.clone(),
            auth_group,
            owner_at_admission,
            runner_instance_id,
            kind: job_kind.to_string(),
            project_id: context.runtime_project_id,
            session_id: context.workflow_session_id,
            ssh_resource: context.ssh_resource,
            cwd: context.cwd,
            project_cwd: context.project_cwd,
            purpose: context.purpose,
            shell: context.shell,
            command_preview: context.command_preview,
            detached_idempotency_intent: detached_intent,
            lifecycle: JobLifecycleState::Queued,
            created_at,
            started_at: None,
            ended_at: None,
            exit_code: None,
            duration_ms: None,
            stdout: Default::default(),
            stderr: Default::default(),
            error: None,
            command_execution_state: None,
            structured_execution: context.structured_execution,
            codex,
            validation_steps: context.validation_steps,
            validation: context.validation,
            validation_progress: None,
            test_count_evidence: None,
            format_mutation: None,
            build_artifacts: None,
            activity: None,
            last_update_seq: 0,
            visibility,

            recovery: JobRecoveryState::default(),
            observation: JobObservationState {
                receipt_candidates: self.inner.capture_candidates(),
                terminal_event_candidates: self.inner.capture_terminal_event_candidates(),
                ..JobObservationState::new(self.observation_epoch.clone())
            },
        };
        inner.request_to_job.insert(request_id, job_id.clone());
        inner.jobs_by_id.insert(job_id.clone(), job);
        notify_runner_locked(&inner, &client_id);
        Ok(job_view(
            inner.jobs_by_id.get(&job_id).expect("job just inserted"),
        ))
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub async fn hidden_job_ids_for_test(&self) -> Vec<String> {
        let inner = self.inner.read().await;
        let mut ids = inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility != ShellJobVisibility::Public)
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    /// The sole handoff/recovery visibility transition. Authorization, terminal
    /// races, cleanup ownership and the observation-token proof are checked
    /// under the same lock; retrying this transition never dispatches execution.
    pub async fn promote_hidden_job(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
    ) -> Result<ShellJobInfo, String> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.hidden_handoff_failure_for_test(false).await?;
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get(job_id)
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if !shell_job_visible_to_auth(auth, &inner, job) {
            return Err(format!("unknown shell job: {job_id}"));
        }
        let job = inner
            .jobs_by_id
            .get_mut(job_id)
            .expect("authorized job exists");
        if job.visibility == ShellJobVisibility::CleanupPending {
            return Err(format!("structured job cleanup is pending: {job_id}"));
        }
        // A conclusive terminal update may race the sync-wait deadline. Keep
        // those records hidden so the initiating structured tool call returns
        // the terminal result without leaving a redundant public Job. An
        // outcome_unknown terminal is different: the initiating result cannot
        // safely authorize a retry, so preserve the same durable Job as the
        // recovery identity instead of discarding the only reconciliation handle.
        let publish_terminal_recovery = job.lifecycle.is_terminal()
            && job.command_execution_state == Some(ShellCommandExecutionState::OutcomeUnknown);
        if !job.lifecycle.is_terminal() || publish_terminal_recovery {
            let view = job_view(job);
            if view.observation_token.is_none() {
                return Err(format!(
                    "structured job has no canonical observation token: {job_id}"
                ));
            }
            job.visibility = ShellJobVisibility::Public;
            return Ok(view);
        }
        Ok(job_view(job))
    }

    pub async fn get_hidden_job_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get(job_id)
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if !shell_job_visible_to_auth(auth, &inner, job) {
            return Err(format!("unknown shell job: {job_id}"));
        }
        Ok(job_view(job))
    }

    pub async fn job_terminal_registration_snapshot_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
    ) -> Result<crate::JobTerminalRegistrationSnapshot, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get(job_id)
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if job.visibility != ShellJobVisibility::Public
            || !shell_job_visible_to_auth(auth, &inner, job)
        {
            return Err(format!("unknown shell job: {job_id}"));
        }
        Ok(crate::receipts::registration_snapshot(job, now_ts()))
    }

    pub async fn hidden_job_log_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.hidden_handoff_failure_for_test(true).await?;
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get(job_id)
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if !shell_job_visible_to_auth(auth, &inner, job) {
            return Err(format!("unknown shell job: {job_id}"));
        }
        let (stdout, next_stdout_line, _, _) = select_log_lines(&job.stdout, None, tail_lines);
        let (stderr, next_stderr_line, _, _) = select_log_lines(&job.stderr, None, tail_lines);
        Ok((
            job_view(job),
            stdout,
            stderr,
            next_stdout_line,
            next_stderr_line,
        ))
    }

    /// Record hidden structured-execution cleanup synchronously. This is safe
    /// to call from a future's Drop implementation and is deliberately
    /// separate from stop delivery: the periodic registry lifecycle retries
    /// any intent whose immediate asynchronous processor is delayed.
    pub fn record_hidden_cleanup_intent(&self, job_id: String, auth: Option<crate::RunnerAccess>) {
        self.cleanup_intents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(job_id, auth);
    }

    pub async fn process_hidden_cleanup_intents(&self) {
        let intents = {
            let mut intents = self
                .cleanup_intents
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *intents)
        };
        let mut retry = Vec::new();
        for (job_id, auth) in intents {
            match self
                .cancel_hidden_job_for_auth(auth.as_ref(), &job_id)
                .await
            {
                Ok(_) => {}
                Err(error) if error.starts_with("unknown shell job:") => {}
                Err(_) => retry.push((job_id, auth)),
            }
        }
        if !retry.is_empty() {
            let mut intents = self
                .cleanup_intents
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            intents.extend(retry);
        }
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub fn has_hidden_cleanup_intent_for_test(&self, job_id: &str) -> bool {
        self.cleanup_intents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(job_id)
    }

    pub(crate) async fn cancel_hidden_job_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
    ) -> Result<bool, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get(job_id)
            .cloned()
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if job.visibility == ShellJobVisibility::Public
            || !shell_job_visible_to_auth(auth, &inner, &job)
        {
            return Err(format!("unknown shell job: {job_id}"));
        }
        if job.lifecycle == JobLifecycleState::Queued {
            if let Some(request_id) = job.request_id.as_deref() {
                retire_job_request_locked(&mut inner, &job.client_id, request_id);
            }
            inner.jobs_by_id.remove(job_id);
            return Ok(true);
        }
        if job.lifecycle.is_terminal() {
            inner.jobs_by_id.remove(job_id);
            return Ok(true);
        }
        inner
            .jobs_by_id
            .get_mut(job_id)
            .expect("job exists")
            .visibility = ShellJobVisibility::CleanupPending;
        if !job.recovery_active()
            && job.lifecycle.is_runner_active()
            && job.lifecycle != JobLifecycleState::StopRequested
        {
            let stop_request_id = next_request_id();
            let request = RunnerRequest::from_operation(
                RunnerInvocationMetadata {
                    request_id: stop_request_id.clone(),
                    client_id: job.client_id.clone(),
                    requested_by: "tool_runtime_cleanup".to_string(),
                    created_at: now_ts(),
                },
                RunnerOperation::Job(RunnerJobOperation::Stop {
                    job_id: job_id.to_string(),
                }),
            )?;
            enqueue_pending_request_locked(
                self.telemetry.as_ref(),
                &mut inner,
                &job.client_id,
                stop_request_id,
                request,
                None,
                Some(job_id.to_string()),
            )?;
            let record = inner.jobs_by_id.get_mut(job_id).expect("job exists");
            record.lifecycle = JobLifecycleState::StopRequested;
            record.error = Some("internal structured execution cleanup requested".to_string());
            notify_runner_locked(&inner, &job.client_id);
        }
        Ok(false)
    }

    /// Remove a Runner-backed Job record entirely.
    /// The caller must ensure the job is terminal or stopped first; removing a
    /// still-active record would orphan the runner process (its later updates
    /// then fail harmlessly as "unknown shell job"). Also drops any still
    /// pending start request so a queued-but-never-run job leaves no request
    /// behind.
    #[cfg(any(test, feature = "root-test-support"))]
    pub async fn remove_job_record(&self, job_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.remove(job_id) else {
            return false;
        };
        if let Some(request_id) = job.request_id {
            retire_job_request_locked(&mut inner, &job.client_id, &request_id);
        }
        true
    }

    /// Discard a terminal hidden Job after its result has already been projected
    /// into the initiating tool response.
    ///
    /// The exact discarded handle is retained only in the current Server
    /// process, bounded by the Runner terminal-inventory limits. A same-runner
    /// registration replay can then suppress that already-projected terminal
    /// evidence without changing fresh-Server conservative recovery. This is
    /// shared by typed process/script execution, structured validation, and
    /// long `run_shell`; all of them use `HiddenUntilHandoff` before projection.
    pub async fn remove_projected_hidden_terminal_job_record(&self, job_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return false;
        };
        let Some(request_id) = job.request_id.clone() else {
            return false;
        };
        if job.visibility != ShellJobVisibility::HiddenUntilHandoff || !job.lifecycle.is_terminal()
        {
            return false;
        }
        let client_id = job.client_id.clone();
        let runner_instance_id = job.runner_instance_id.clone();
        // Preserve same-instance reconnect suppression whenever the exact
        // Runner lease is still current. If that lease disappeared or was
        // replaced after the terminal snapshot was already projected, do not
        // strand the hidden terminal record forever: a replacement instance
        // cannot consume the retired lease, while a later registration after
        // the Server forgot the lease is conservatively equivalent to fresh
        // recovery and may reconstruct retained Runner terminal evidence.
        if let Some(runner) = inner
            .runners
            .get_mut(&client_id)
            .filter(|runner| runner.runner_instance_id == runner_instance_id)
        {
            // The suppression store predates raw-shell/validation hidden
            // handoff, but its proof is only the exact
            // runner/instance/job/request tuple.
            runner.remember_projected_structured_terminal(
                job_id.to_string(),
                request_id.clone(),
                now_ts(),
            );
        }

        let removed = inner
            .jobs_by_id
            .remove(job_id)
            .expect("projected hidden Job was checked under the registry lock");
        retire_job_request_locked(&mut inner, &removed.client_id, &request_id);
        true
    }

    pub async fn remove_projected_hidden_structured_job_record(&self, job_id: &str) -> bool {
        self.remove_projected_hidden_terminal_job_record(job_id)
            .await
    }

    pub async fn get_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        self.get_job_for_auth(None, job_id).await
    }

    pub async fn get_job_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        if job.visibility != ShellJobVisibility::Public
            || !shell_job_visible_to_auth(auth, &inner, job)
        {
            return Err(format!("unknown shell job: {}", job_id));
        }
        Ok(job_view(job))
    }

    /// Bounded caller-authorized immutable Job views for outer observability.
    /// Unlike ordinary Job reads this deliberately does not refresh lifecycle,
    /// touch receipts, contact a Runner, or wait for the registry lock. Missing,
    /// hidden, or contended metadata is omitted so telemetry can never change or
    /// delay the caller's tool outcome.
    pub fn try_job_observability_views_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_ids: &[&str],
    ) -> Option<Vec<ShellJobInfo>> {
        self.inner.try_read(|inner| {
            job_ids
                .iter()
                .take(8)
                .filter_map(|job_id| {
                    let job = inner.jobs_by_id.get(*job_id)?;
                    (job.visibility == ShellJobVisibility::Public
                        && shell_job_visible_to_auth(auth, inner, job))
                    .then(|| job_view(job))
                })
                .collect()
        })
    }

    /// Resolve one observability-only Project anchor for an exact Job set under
    /// one registry snapshot. Every Job must be Public, caller-visible, and carry
    /// the same non-empty immutable Project id; otherwise attribution fails closed.
    /// This never refreshes lifecycle state and never grants Project authority.
    pub async fn common_job_project_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_ids: &[&str],
    ) -> Option<String> {
        if job_ids.is_empty() {
            return None;
        }
        let inner = self.inner.read().await;
        let mut common: Option<String> = None;
        for job_id in job_ids {
            let job = inner.jobs_by_id.get(*job_id)?;
            if job.visibility != ShellJobVisibility::Public
                || !shell_job_visible_to_auth(auth, &inner, job)
            {
                return None;
            }
            let project = job.project_id.as_deref()?.trim();
            if project.is_empty() {
                return None;
            }
            match common.as_deref() {
                None => common = Some(project.to_string()),
                Some(existing) if existing == project => {}
                Some(_) => return None,
            }
        }
        common
    }

    pub async fn list_jobs(&self, limit: Option<usize>) -> Vec<ShellJobInfo> {
        self.list_jobs_for_auth(None, limit).await
    }

    pub async fn list_jobs_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        limit: Option<usize>,
    ) -> Vec<ShellJobInfo> {
        self.visible_job_records_for_auth(auth)
            .await
            .into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect()
    }

    /// Compact runtime status over all authorized Jobs without cloning or
    /// sorting full historical records and per-Job DTO metadata.
    pub async fn runtime_job_aggregate_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        client_id: Option<&str>,
    ) -> RuntimeJobAggregateSnapshot {
        #[cfg(any(test, feature = "root-test-support"))]
        self.full_job_history_scan_count
            .fetch_add(1, Ordering::Relaxed);
        let mut inner = self.inner.lock().await;
        let ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            refresh_job_status_locked(&mut inner, &id);
        }
        let mut result = RuntimeJobAggregateSnapshot::default();
        for job in inner.jobs_by_id.values().filter(|job| {
            job.visibility == ShellJobVisibility::Public
                && shell_job_visible_to_auth(auth, &inner, job)
                && client_id.is_none_or(|id| job.client_id == id)
        }) {
            // Aggregate the Registry's typed facts, not a wire-status round trip.
            // Recovery masks visible execution/queue counts here; the separate
            // capacity aggregate deliberately retains underlying slot ownership.
            let lifecycle = job.lifecycle;
            let recovering = job.recovery_active();
            let active = lifecycle.is_active();
            let running = !recovering
                && matches!(
                    lifecycle,
                    JobLifecycleState::Running | JobLifecycleState::StartedLegacy
                );
            let queued = !recovering
                && matches!(
                    lifecycle,
                    JobLifecycleState::Queued | JobLifecycleState::RunnerQueued
                );
            let in_slot = !recovering && lifecycle.occupies_execution_slot();
            result.active_count += usize::from(active);
            result.running_count += usize::from(running);
            result.queued_count += usize::from(queued);
            result.recovering_count += usize::from(recovering);
            result.lost_after_reconcile_count += usize::from(
                lifecycle == JobLifecycleState::Lost
                    && job
                        .recovery
                        .reason
                        .is_some_and(JobRecoveryReason::implies_lost_after_reconcile),
            );
            if active || in_slot || queued {
                let runner = result.by_runner.entry(job.client_id.clone()).or_default();
                runner.active += usize::from(active);
                runner.running += usize::from(in_slot);
                runner.queued += usize::from(queued);
            }
        }
        result
    }

    /// Complete caller-visible Runner Job set for aggregate observability.
    /// Unlike the public list projection, this has no display pagination:
    /// runtime counts must not silently drop an older active Job behind newer
    /// records. Authorization and public-visibility filtering are identical
    /// to `list_jobs_for_auth`.
    pub async fn list_all_jobs_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
    ) -> Vec<ShellJobInfo> {
        self.visible_job_records_for_auth(auth)
            .await
            .into_iter()
            .map(|job| job_view(&job))
            .collect()
    }

    /// Complete caller-visible Job set after exact static identity filters.
    /// Runner/Project/Session selection happens before lifecycle refresh so
    /// focused inventory queries do not refresh unrelated Jobs. Status is
    /// intentionally not accepted here because it depends on the refreshed
    /// lifecycle and must be applied by the caller afterwards.
    pub async fn list_jobs_for_auth_filtered(
        &self,
        auth: Option<&crate::RunnerAccess>,
        client_id: Option<&str>,
        project_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Vec<ShellJobInfo> {
        let mut inner = self.inner.lock().await;
        let candidate_ids = inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility == ShellJobVisibility::Public)
            .filter(|job| shell_job_visible_to_auth(auth, &inner, job))
            .filter(|job| {
                client_id
                    .map(|client_id| job.client_id == client_id)
                    .unwrap_or(true)
            })
            .filter(|job| {
                project_id
                    .map(|project_id| job.project_id.as_deref() == Some(project_id))
                    .unwrap_or(true)
            })
            .filter(|job| {
                session_id
                    .map(|session_id| job.session_id.as_deref() == Some(session_id))
                    .unwrap_or(true)
            })
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        #[cfg(any(test, feature = "root-test-support"))]
        self.filtered_job_refresh_count
            .fetch_add(candidate_ids.len(), Ordering::Relaxed);
        for job_id in candidate_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility == ShellJobVisibility::Public)
            .filter(|job| shell_job_visible_to_auth(auth, &inner, job))
            .filter(|job| {
                client_id
                    .map(|client_id| job.client_id == client_id)
                    .unwrap_or(true)
            })
            .filter(|job| {
                project_id
                    .map(|project_id| job.project_id.as_deref() == Some(project_id))
                    .unwrap_or(true)
            })
            .filter(|job| {
                session_id
                    .map(|session_id| job.session_id.as_deref() == Some(session_id))
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        jobs.into_iter().map(|job| job_view(&job)).collect()
    }

    /// Passive attention reads only the Server's current Job records. In particular,
    /// it must not refresh lifecycle or contact a Runner on an unrelated tool call.
    pub async fn snapshot_jobs_for_auth_filtered(
        &self,
        auth: Option<&crate::RunnerAccess>,
        project_id: &str,
        session_id: &str,
        active_limit: usize,
        terminal_limit: usize,
    ) -> Vec<JobAttentionSnapshot> {
        let inner = self.inner.read().await;
        Self::job_attention_snapshot_locked(
            &inner,
            auth,
            project_id,
            session_id,
            active_limit,
            terminal_limit,
            None,
        )
        .expect("unbudgeted snapshot")
    }

    /// Optional post-result observation never queues behind the registry. Work
    /// checks the shared deadline and retains only the two bounded top-K pages.
    pub fn try_snapshot_jobs_for_auth_filtered(
        &self,
        auth: Option<&crate::RunnerAccess>,
        project_id: &str,
        session_id: &str,
        active_limit: usize,
        terminal_limit: usize,
        deadline: std::time::Instant,
    ) -> Option<Vec<JobAttentionSnapshot>> {
        self.inner
            .try_read(|inner| {
                Self::job_attention_snapshot_locked(
                    inner,
                    auth,
                    project_id,
                    session_id,
                    active_limit,
                    terminal_limit,
                    Some(deadline),
                )
            })
            .flatten()
    }

    fn job_attention_snapshot_locked(
        inner: &crate::state::RunnerRegistryInner,
        auth: Option<&crate::RunnerAccess>,
        project_id: &str,
        session_id: &str,
        active_limit: usize,
        terminal_limit: usize,
        deadline: Option<std::time::Instant>,
    ) -> Option<Vec<JobAttentionSnapshot>> {
        let expired = || deadline.is_some_and(|d| std::time::Instant::now() >= d);
        if expired() {
            return None;
        }
        let active_limit =
            active_limit.min(webcodex_core::runner_protocol::JOB_INVENTORY_MAX_ACTIVE_JOBS);
        let terminal_limit = terminal_limit.min(32);
        let mut active = std::collections::BTreeMap::new();
        let mut terminal = std::collections::BTreeMap::new();
        for (index, job) in inner.jobs_by_id.values().enumerate() {
            if index % 64 == 0 && expired() {
                return None;
            }
            if job.visibility != ShellJobVisibility::Public
                || !shell_job_visible_to_auth(auth, inner, job)
                || job.project_id.as_deref() != Some(project_id)
                || job.session_id.as_deref() != Some(session_id)
            {
                continue;
            }
            if job.lifecycle.is_terminal() {
                let observed = job
                    .observation
                    .terminal_observed_at
                    .or(job.ended_at)
                    .unwrap_or(job.created_at);
                terminal.insert((observed, job.created_at, &job.job_id), job);
                if terminal.len() > terminal_limit {
                    terminal.pop_first();
                }
            } else {
                active.insert((job.created_at, &job.job_id), job);
                if active.len() > active_limit {
                    active.pop_first();
                }
            }
        }
        let mut snapshots = Vec::with_capacity(active.len() + terminal.len());
        for job in active
            .into_values()
            .rev()
            .chain(terminal.into_values().rev())
        {
            if expired() {
                return None;
            }
            let validation_output = (job.lifecycle.is_terminal()
                && (job.validation.is_some()
                    || job
                        .structured_execution
                        .as_ref()
                        .and_then(|metadata| metadata.validation_identity.as_ref())
                        .is_some()))
            .then(|| JobValidationOutput {
                stdout: job.stdout.tail.clone(),
                stderr: job.stderr.tail.clone(),
                truncated: job.stdout.truncated
                    || job.stderr.truncated
                    || job.stdout.first_retained_line > 1
                    || job.stderr.first_retained_line > 1,
            });
            snapshots.push(JobAttentionSnapshot {
                job: job_view(job),
                validation_output,
                recovery: job.recovery.phase.map(|phase| (phase, job.recovery.reason)),
            });
        }
        if expired() {
            None
        } else {
            Some(snapshots)
        }
    }
    /// Presentation-only re-observation of exact previously admitted Job handles.
    /// Never scans history, refreshes lifecycle, or performs post-unlock writes.
    /// Missing, inaccessible, recovering, lost and outcome-unknown Jobs do not
    /// prove a writer ended, even when their outer lifecycle is terminal.
    pub fn observation_jobs_ended_for_project(
        &self,
        auth: Option<&crate::RunnerAccess>,
        project: &str,
        job_ids: &[String],
    ) -> bool {
        if job_ids.len() > 128 {
            return false;
        }
        self.inner
            .try_read(|inner| {
                job_ids.iter().all(|id| {
                    inner.jobs_by_id.get(id).is_some_and(|job| {
                        job.project_id.as_deref() == Some(project)
                            && shell_job_visible_to_auth(auth, inner, job)
                            && job.lifecycle.is_terminal()
                            && job.lifecycle != JobLifecycleState::Lost
                            && job.command_execution_state
                                != Some(ShellCommandExecutionState::OutcomeUnknown)
                            && !job.recovery_active()
                    })
                })
            })
            .unwrap_or(false)
    }

    /// Exact ids are selected only from successful canonical result projections.
    pub fn try_job_telemetry_snapshots_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_ids: &[&str],
    ) -> Option<Vec<JobTelemetrySnapshot>> {
        self.inner.try_read(|inner| {
            job_ids
                .iter()
                .take(MAX_JOB_TELEMETRY_SNAPSHOTS)
                .filter_map(|id| {
                    let job = inner.jobs_by_id.get(*id)?;
                    if job.visibility != ShellJobVisibility::Public
                        || !shell_job_visible_to_auth(auth, inner, job)
                    {
                        return None;
                    }
                    Some(JobTelemetrySnapshot {
                        job_id: job.job_id.clone(),
                        project_id: job.project_id.clone(),
                        session_id: job.session_id.clone(),
                        terminal_observed_at: job.observation.terminal_observed_at,
                    })
                })
                .collect()
        })
    }

    async fn visible_job_records_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
    ) -> Vec<ShellJobRecord> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.full_job_history_scan_count
            .fetch_add(1, Ordering::Relaxed);
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility == ShellJobVisibility::Public)
            .filter(|job| shell_job_visible_to_auth(auth, &inner, job))
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        jobs
    }

    /// Count caller-visible active Jobs for exact Projects in O(P + A), where A
    /// is current active Jobs, independent of retained terminal history. Only
    /// authorized matching candidates are refreshed. Canonical records, not the
    /// derived active index, decide lifecycle, Project identity and visibility.
    pub async fn count_active_jobs_for_projects(
        &self,
        auth: Option<&crate::RunnerAccess>,
        runtime_project_ids: &[&str],
    ) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = runtime_project_ids
            .iter()
            .map(|id| ((*id).to_string(), 0))
            .collect();
        if counts.is_empty() {
            return counts;
        }
        let mut inner = self.inner.lock().await;
        #[cfg(any(test, feature = "root-test-support"))]
        self.project_job_scan_count.fetch_add(1, Ordering::Relaxed);
        let job_ids = inner.jobs_by_id.active_ids();
        for job_id in job_ids {
            let eligible = inner.jobs_by_id.get(&job_id).is_some_and(|job| {
                job.visibility == ShellJobVisibility::Public
                    && job
                        .project_id
                        .as_ref()
                        .is_some_and(|id| counts.contains_key(id))
                    && shell_job_visible_to_auth(auth, &inner, job)
            });
            if !eligible {
                continue;
            }
            #[cfg(any(test, feature = "root-test-support"))]
            self.project_job_candidate_refresh_count
                .fetch_add(1, Ordering::Relaxed);
            refresh_job_status_locked(&mut inner, &job_id);
            if let Some(job) = inner
                .jobs_by_id
                .get(&job_id)
                .filter(|job| job.lifecycle.is_active())
            {
                if let Some(count) = job.project_id.as_ref().and_then(|id| counts.get_mut(id)) {
                    *count += 1;
                }
            }
        }
        counts
    }

    /// Lightweight operator aggregate. No historical record/stream projection,
    /// sorting, or lifecycle refresh of terminal Jobs is performed.
    pub async fn active_job_counts_by_runner_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
    ) -> HashMap<String, usize> {
        self.active_job_summary_by_runner_for_auth(auth)
            .await
            .into_iter()
            .map(|(id, counts)| (id, counts.active))
            .collect()
    }

    pub async fn active_job_summary_by_runner_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
    ) -> HashMap<String, crate::ActiveJobAggregate> {
        let mut inner = self.inner.lock().await;
        let ids = inner.jobs_by_id.active_ids();
        let mut counts: HashMap<String, crate::ActiveJobAggregate> = HashMap::new();
        for id in ids {
            if !inner.jobs_by_id.get(&id).is_some_and(|job| {
                job.visibility == ShellJobVisibility::Public
                    && shell_job_visible_to_auth(auth, &inner, job)
            }) {
                continue;
            }
            refresh_job_status_locked(&mut inner, &id);
            if let Some(job) = inner
                .jobs_by_id
                .get(&id)
                .filter(|job| job.lifecycle.is_active())
            {
                let count = counts.entry(job.client_id.clone()).or_default();
                count.active += 1;
                count.running += usize::from(job.lifecycle.occupies_execution_slot());
                count.queued += usize::from(matches!(
                    job.lifecycle,
                    JobLifecycleState::Queued | JobLifecycleState::RunnerQueued
                ));
            }
        }
        counts
    }

    /// Single-Project observation shares the same unpaginated, authorized path.
    pub async fn count_active_jobs_for_project(
        &self,
        auth: Option<&crate::RunnerAccess>,
        runtime_project_id: &str,
    ) -> usize {
        self.count_active_jobs_for_projects(auth, &[runtime_project_id])
            .await
            .get(runtime_project_id)
            .copied()
            .unwrap_or(0)
    }

    /// Atomically fence new job starts and count all currently active jobs for
    /// a runtime project. The fence remains until `end_project_unregister`.
    pub async fn begin_project_unregister(
        &self,
        auth: Option<&crate::RunnerAccess>,
        runtime_project_id: &str,
    ) -> Result<usize, String> {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.active_ids();
        let mut active = 0;
        for job_id in job_ids {
            let eligible = inner.jobs_by_id.get(&job_id).is_some_and(|job| {
                job.project_id.as_deref() == Some(runtime_project_id)
                    && shell_job_visible_to_auth(auth, &inner, job)
            });
            if !eligible {
                continue;
            }
            refresh_job_status_locked(&mut inner, &job_id);
            active += usize::from(
                inner
                    .jobs_by_id
                    .get(&job_id)
                    .is_some_and(|job| job.lifecycle.is_active()),
            );
        }
        if active == 0 {
            *inner
                .unregistering_projects
                .entry(runtime_project_id.to_string())
                .or_insert(0) += 1;
        }
        Ok(active)
    }

    pub async fn end_project_unregister(&self, runtime_project_id: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(count) = inner.unregistering_projects.get_mut(runtime_project_id) {
            *count -= 1;
            if *count == 0 {
                inner.unregistering_projects.remove(runtime_project_id);
            }
        }
    }

    pub async fn list_jobs_for_runner(
        &self,
        client_id: &str,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ShellJobInfo>, String> {
        validate_id(client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        if !inner.runners.contains_key(client_id) {
            return Err(format!("unknown shell client: {}", client_id));
        }
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| job.client_id == client_id)
            .filter(|job| {
                status
                    .map(|status| status == job.public_status())
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        Ok(jobs
            .into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect())
    }

    /// Legacy immediate `job_log`: no wait parameters, returns the current
    /// snapshot and log segment without waiting. The wait outcome is
    /// `immediate`.
    pub async fn job_log(
        &self,
        job_id: &str,
        since_stdout_line: Option<usize>,
        since_stderr_line: Option<usize>,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
        let (job, stdout, stderr, next_stdout_line, next_stderr_line, _wait) = self
            .job_log_for_auth(
                None,
                job_id,
                since_stdout_line,
                since_stderr_line,
                tail_lines,
                None,
                None,
            )
            .await?;
        Ok((job, stdout, stderr, next_stdout_line, next_stderr_line))
    }

    /// Read bounded stdout/stderr for a job, optionally waiting once for the
    /// opaque observation token to change or for the job to become terminal.
    pub async fn job_log_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
        since_stdout_line: Option<usize>,
        since_stderr_line: Option<usize>,
        tail_lines: Option<usize>,
        after_observation_token: Option<&str>,
        wait_secs: Option<u64>,
    ) -> Result<
        (
            ShellJobInfo,
            Option<String>,
            Option<String>,
            usize,
            usize,
            ShellJobLogObservation,
        ),
        String,
    > {
        validate_id(job_id, "job_id")?;
        let after = after_observation_token
            .map(|value| {
                webcodex_core::job_observation::JobObservationToken::parse(value)
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        let deadline = wait_secs
            .map(|secs| tokio::time::Instant::now() + tokio::time::Duration::from_secs(secs));
        let mut waited_ms = 0u64;
        let mut waited = false;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        loop {
            let Some(job) = inner.jobs_by_id.get(job_id) else {
                return Err(format!("unknown shell job: {}", job_id));
            };
            if job.visibility != ShellJobVisibility::Public
                || !shell_job_visible_to_auth(auth, &inner, job)
            {
                return Err(format!("unknown shell job: {}", job_id));
            }
            let (changed, meaningful_changed, heartbeat_changed) =
                observation_change_flags(job, after.as_ref());
            let terminal = job.lifecycle.is_terminal();
            if wait_secs.is_none() || after.is_none() || changed || terminal {
                let wait_outcome = if changed {
                    if waited {
                        JobLogWaitOutcome::Updated
                    } else {
                        JobLogWaitOutcome::Immediate
                    }
                } else if terminal {
                    JobLogWaitOutcome::Terminal
                } else {
                    JobLogWaitOutcome::Immediate
                };
                let wait = JobLogWait {
                    wait_outcome,
                    waited_ms,
                    changed,
                    meaningful_changed,
                    heartbeat_changed,
                    terminal,
                };
                return Ok(frozen_shell_job_log_projection(
                    job,
                    after.as_ref(),
                    since_stdout_line,
                    since_stderr_line,
                    tail_lines,
                    wait,
                ));
            }

            let update_notify = job.observation.notify.clone();
            let notified = update_notify.notified();
            drop(inner);

            inner = self.inner.lock().await;
            refresh_job_status_locked(&mut inner, job_id);
            let Some(job) = inner.jobs_by_id.get(job_id) else {
                return Err(format!("unknown shell job: {}", job_id));
            };
            if job.visibility != ShellJobVisibility::Public
                || !shell_job_visible_to_auth(auth, &inner, job)
            {
                return Err(format!("unknown shell job: {}", job_id));
            }
            let (changed, meaningful_changed, heartbeat_changed) =
                observation_change_flags(job, after.as_ref());
            let terminal = job.lifecycle.is_terminal();
            if changed || terminal {
                let wait = JobLogWait {
                    wait_outcome: if terminal {
                        JobLogWaitOutcome::Terminal
                    } else {
                        JobLogWaitOutcome::Updated
                    },
                    waited_ms,
                    changed,
                    meaningful_changed,
                    heartbeat_changed,
                    terminal,
                };
                return Ok(frozen_shell_job_log_projection(
                    job,
                    after.as_ref(),
                    since_stdout_line,
                    since_stderr_line,
                    tail_lines,
                    wait,
                ));
            }
            drop(inner);

            let wait_started = tokio::time::Instant::now();
            let wake = tokio::time::timeout_at(
                deadline.expect("bounded wait requires a deadline"),
                notified,
            )
            .await;
            waited = true;
            waited_ms = waited_ms.saturating_add(wait_started.elapsed().as_millis() as u64);
            inner = self.inner.lock().await;
            refresh_job_status_locked(&mut inner, job_id);
            if wake.is_err() {
                let Some(job) = inner.jobs_by_id.get(job_id) else {
                    return Err(format!("unknown shell job: {}", job_id));
                };
                if job.visibility != ShellJobVisibility::Public
                    || !shell_job_visible_to_auth(auth, &inner, job)
                {
                    return Err(format!("unknown shell job: {}", job_id));
                }
                let (changed, meaningful_changed, heartbeat_changed) =
                    observation_change_flags(job, after.as_ref());
                let terminal = job.lifecycle.is_terminal();
                let wait = JobLogWait {
                    wait_outcome: if terminal {
                        JobLogWaitOutcome::Terminal
                    } else if changed {
                        JobLogWaitOutcome::Updated
                    } else {
                        JobLogWaitOutcome::Timeout
                    },
                    waited_ms,
                    changed,
                    meaningful_changed,
                    heartbeat_changed,
                    terminal,
                };
                return Ok(frozen_shell_job_log_projection(
                    job,
                    after.as_ref(),
                    since_stdout_line,
                    since_stderr_line,
                    tail_lines,
                    wait,
                ));
            }
        }
    }

    pub async fn stop_job(
        &self,
        job_id: &str,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        self.stop_job_for_auth(None, job_id, requested_by).await
    }

    pub async fn stop_job_for_auth(
        &self,
        auth: Option<&crate::RunnerAccess>,
        job_id: &str,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id).cloned() else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        if job.visibility != ShellJobVisibility::Public
            || !shell_job_visible_to_auth(auth, &inner, &job)
        {
            return Err(format!("unknown shell job: {}", job_id));
        }
        if job.recovery_active() {
            return Err(
                "runner_unavailable_recovering: wait for same-instance job reconciliation before retrying stop_job"
                    .to_string(),
            );
        }
        match job.lifecycle {
            JobLifecycleState::Queued => {
                if let Some(request_id) = &job.request_id {
                    retire_job_request_locked(&mut inner, &job.client_id, request_id);
                }
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                let terminal_now = now_ts();
                job.lifecycle = JobLifecycleState::Stopped;
                observe_job_terminal(job, terminal_now);
                job.ended_at = Some(terminal_now);
                job.error = Some("job stopped before Runner picked it up".to_string());
                if job.structured_execution.is_some() {
                    job.command_execution_state = Some(ShellCommandExecutionState::NotStarted);
                }
                notify_job_update(job);
                Ok(job_view(job))
            }
            JobLifecycleState::StopRequested => {
                Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists")))
            }
            JobLifecycleState::RunnerQueued | JobLifecycleState::Running => {
                let stop_request_id = next_request_id();
                let client_id = job.client_id.clone();
                let request = RunnerRequest::from_operation(
                    RunnerInvocationMetadata {
                        request_id: stop_request_id.clone(),
                        client_id: client_id.clone(),
                        requested_by,
                        created_at: now_ts(),
                    },
                    RunnerOperation::Job(RunnerJobOperation::Stop {
                        job_id: job_id.to_string(),
                    }),
                )?;
                enqueue_pending_request_locked(
                    self.telemetry.as_ref(),
                    &mut inner,
                    &client_id,
                    stop_request_id,
                    request,
                    None,
                    Some(job_id.to_string()),
                )?;
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.lifecycle = JobLifecycleState::StopRequested;
                job.error = Some("stop requested".to_string());
                notify_job_update(job);
                let notify_runner_id = job.client_id.clone();
                notify_runner_locked(&inner, &notify_runner_id);
                Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists")))
            }
            _ => Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists"))),
        }
    }

    /// Polling-transport job update entry point. Job ownership and update
    /// sequence rules decide acceptance; this path refreshes `last_seen` for
    /// the active instance. Used by the HTTP `/job_update` handler.
    pub async fn update_job(&self, body: RunnerJobUpdateRequest) -> Result<ShellJobInfo, String> {
        self.update_job_checked(body, None).await
    }

    /// Connection-scoped job update entry point for long-lived transports.
    /// Acceptance still follows the existing `agent_instance_id`, job
    /// ownership and `update_seq` rules so a legitimately-dispatched job's
    /// late update is not dropped just because the transport connection was
    /// replaced. A late update arriving on a stale same-instance connection,
    /// however, must not refresh the new connection's `last_seen` liveness.
    pub async fn update_job_for_connection(
        &self,
        body: RunnerJobUpdateRequest,
        connection_id: &str,
    ) -> Result<ShellJobInfo, String> {
        self.update_job_checked(body, Some(connection_id)).await
    }

    async fn update_job_checked(
        &self,
        body: RunnerJobUpdateRequest,
        expected_connection_id: Option<&str>,
    ) -> Result<ShellJobInfo, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.job_id, "job_id")?;
        validate_runner_instance_id(&body.runner_instance_id)?;
        if let Some(snapshot) = body.log_snapshot.as_ref() {
            validate_stream_snapshot(&snapshot.stdout, "job update stdout snapshot")?;
            validate_stream_snapshot(&snapshot.stderr, "job update stderr snapshot")?;
            if body.stdout_chunk.is_some() || body.stderr_chunk.is_some() {
                return Err(
                    "job update log_snapshot cannot be combined with chunk fields".to_string(),
                );
            }
        }
        let mut inner = self.inner.lock().await;
        // Reject job updates from a stale/replaced instance before refreshing
        // liveness or mutating job state.
        assert_active_instance_locked(&inner, &body.client_id, &body.runner_instance_id)?;
        let sequenced = inner.runners.get(&body.client_id).is_some_and(|runner| {
            runner
                .runner_features
                .supports(RunnerFeature::JobStateReconciliation)
        });
        let incoming_seq = if sequenced {
            Some(
                body.update_seq
                    .filter(|sequence| *sequence > 0)
                    .ok_or_else(|| {
                        "job_state_reconciliation update requires update_seq".to_string()
                    })?,
            )
        } else {
            body.update_seq
        };
        let incoming_status = body.status.trim();
        if sequenced && body.status != incoming_status {
            return Err(
                "job_state_reconciliation update status must be canonical without surrounding whitespace"
                    .to_string(),
            );
        }
        let incoming_lifecycle = parse_job_lifecycle(incoming_status).map_err(|_| {
            if sequenced {
                format!(
                    "job_state_reconciliation update status '{}' is invalid",
                    incoming_status
                )
            } else {
                format!("job update status '{}' is invalid", incoming_status)
            }
        })?;
        if sequenced && !(incoming_lifecycle.is_runner_active() || incoming_lifecycle.is_terminal())
        {
            return Err(format!(
                "job_state_reconciliation update status '{}' is invalid",
                incoming_status
            ));
        }
        if sequenced && body.finished != incoming_lifecycle.is_terminal() {
            return Err(
                "job_state_reconciliation update has inconsistent finished/status".to_string(),
            );
        }
        if sequenced
            && !incoming_lifecycle.is_terminal()
            && (body.exit_code.is_some() || body.duration_ms.is_some())
        {
            return Err(
                "active job_state_reconciliation update contains terminal result fields"
                    .to_string(),
            );
        }
        if sequenced
            && incoming_lifecycle == JobLifecycleState::Completed
            && body.exit_code != Some(0)
        {
            return Err(
                "completed job_state_reconciliation update requires exit_code=0".to_string(),
            );
        }
        // Refresh liveness only for the connection that currently holds the
        // transport lease. A late job update on a stale same-instance
        // connection is still applied below, but it must not make the new
        // connection appear online.
        if expected_connection_id.is_none()
            || inner
                .runners
                .get(&body.client_id)
                .is_some_and(|runner| runner.connection_id.as_deref() == expected_connection_id)
        {
            if let Some(runner) = inner.runners.get_mut(&body.client_id) {
                runner.last_seen = now_ts();
            }
        }
        let mut request_id_to_remove = None;
        let remove_cleanup_terminal;
        let view = {
            let Some(job) = inner.jobs_by_id.get_mut(&body.job_id) else {
                return Err(format!("unknown shell job: {}", body.job_id));
            };
            if job.client_id != body.client_id {
                return Err("job_id does not belong to client_id".to_string());
            }
            if job.runner_instance_id != body.runner_instance_id {
                return Err("job_id belongs to a replaced runner instance".to_string());
            }
            if body
                .request_id
                .as_deref()
                .is_some_and(|request_id| job.request_id.as_deref() != Some(request_id))
            {
                return Err("job update request_id does not match job_id".to_string());
            }
            if body.log_snapshot.is_some() && !sequenced {
                return Err(
                    "job update log_snapshot requires job_state_reconciliation capability"
                        .to_string(),
                );
            }
            if job.recovery_active() && sequenced && body.log_snapshot.is_none() {
                return Err(
                    "recovering job update requires an authoritative log_snapshot or register inventory"
                        .to_string(),
                );
            }
            let before = public_mutation_signature(job);
            if sequenced && incoming_seq.is_some_and(|sequence| sequence <= job.last_update_seq) {
                return Ok(job_view(job));
            }
            if job.lifecycle.is_terminal() {
                // Terminal class is server-authoritative once accepted.
                return Ok(job_view(job));
            }
            if body.log_snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.stdout.next_line < job.stdout.next_line
                    || snapshot.stderr.next_line < job.stderr.next_line
            }) {
                return Err(
                    "job update authoritative log snapshot regresses an absolute cursor"
                        .to_string(),
                );
            }
            self.telemetry.runner_job_update_accepted(
                body.request_id.as_deref(),
                &body.job_id,
                &body,
            );
            if let Err(error) = validate_validation_progress(job, &body, incoming_lifecycle)
                .and_then(|_| validate_test_count_evidence(job, &body, incoming_lifecycle))
                .and_then(|_| {
                    webcodex_core::project_format::validate_mutation_report(
                        &job.kind,
                        incoming_lifecycle.as_wire(),
                        body.exit_code,
                        body.command_execution_state,
                        body.format_mutation,
                        body.error.as_deref(),
                    )
                    .map_err(|_| ValidationProtocolError("format_mutation_invalid"))
                })
                .and_then(|_| {
                    webcodex_core::project_build::validate_build_artifact_report(
                        &job.kind,
                        incoming_lifecycle.as_wire(),
                        body.exit_code,
                        body.command_execution_state,
                        body.build_artifacts.as_deref(),
                    )
                    .map_err(ValidationProtocolError)
                })
                .and_then(|_| validate_command_execution_state(job, &body, incoming_lifecycle))
                .and_then(|_| validate_job_activity(job, &body, incoming_lifecycle))
            {
                let terminal_now = now_ts();
                job.lifecycle = JobLifecycleState::Failed;
                observe_job_terminal(job, terminal_now);
                job.ended_at = Some(terminal_now);
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.error = Some(format!("executor protocol violation: {}", error.0));
                job.activity = None;
                if job.structured_execution.is_some() {
                    job.command_execution_state = Some(if job.started_at.is_some() {
                        ShellCommandExecutionState::OutcomeUnknown
                    } else {
                        ShellCommandExecutionState::NotStarted
                    });
                }
                request_id_to_remove = job.request_id.clone();
            } else {
                let was_recovering = job.recovery_active();
                if let Some(snapshot) = body.log_snapshot {
                    super::jobs::replace_log_from_snapshot(&mut job.stdout, &snapshot.stdout);
                    super::jobs::replace_log_from_snapshot(&mut job.stderr, &snapshot.stderr);
                } else {
                    append_log_limited(&mut job.stdout, body.stdout_chunk);
                    append_log_limited(&mut job.stderr, body.stderr_chunk);
                }
                if body.validation_progress.is_some() {
                    job.validation_progress = body.validation_progress.clone();
                }
                if body.build_artifacts.is_some() {
                    job.build_artifacts = body.build_artifacts.clone();
                }
                if body.format_mutation.is_some() {
                    job.format_mutation = body.format_mutation;
                }
                if body.test_count_evidence.is_some() {
                    job.test_count_evidence = body.test_count_evidence.clone();
                }
                if body.activity.is_some() {
                    job.activity = body.activity;
                }
                if job.started_at.is_none()
                    && body.command_execution_state != Some(ShellCommandExecutionState::NotStarted)
                    && matches!(
                        incoming_lifecycle,
                        JobLifecycleState::Running
                            | JobLifecycleState::Completed
                            | JobLifecycleState::Failed
                            | JobLifecycleState::Stopped
                            | JobLifecycleState::Timeout
                    )
                {
                    job.started_at = Some(now_ts());
                }
                if !job.lifecycle.is_terminal() {
                    job.lifecycle = if incoming_lifecycle == JobLifecycleState::Queued
                        && job.started_at.is_some()
                    {
                        JobLifecycleState::RunnerQueued
                    } else {
                        incoming_lifecycle
                    };
                }
                if incoming_lifecycle.is_terminal() {
                    let terminal_now = now_ts();
                    job.lifecycle = incoming_lifecycle;
                    observe_job_terminal(job, terminal_now);
                    job.ended_at = Some(terminal_now);
                    job.exit_code = body.exit_code;
                    job.duration_ms = body.duration_ms;
                    job.error = body.error;
                    job.command_execution_state = body.command_execution_state;
                    job.activity = None;
                    if !was_recovering {
                        job.recovery.phase = job
                            .recovery
                            .reconciled_at
                            .map(|_| JobRecoveryPhase::Reconciled);
                    }
                    job.recovery.recovering_since = None;
                    request_id_to_remove = job.request_id.clone();
                } else if body.error.is_some() {
                    job.error = body.error;
                }
                if body.finished && !job.lifecycle.is_terminal() {
                    let terminal_now = now_ts();
                    job.lifecycle = if job.error.is_none() && job.exit_code == Some(0) {
                        JobLifecycleState::Completed
                    } else {
                        JobLifecycleState::Failed
                    };
                    observe_job_terminal(job, terminal_now);
                    job.ended_at = Some(terminal_now);
                    job.activity = None;
                    request_id_to_remove = job.request_id.clone();
                }
                if was_recovering {
                    job.recovery.phase = Some(JobRecoveryPhase::Reconciled);
                    job.recovery.reconciled_at = Some(now_ts());
                    job.recovery.reason = Some(JobRecoveryReason::SameInstanceUpdateReconciliation);
                    job.recovery.recovering_since = None;
                }
            }
            if let Some(sequence) = incoming_seq {
                job.last_update_seq = sequence;
            }
            let after = public_mutation_signature(job);
            if after != before {
                let mut without_sequence = after.clone();
                without_sequence.last_update_seq = before.last_update_seq;
                if without_sequence == before {
                    notify_job_heartbeat(job);
                } else {
                    notify_job_update(job);
                }
            }
            remove_cleanup_terminal =
                job.visibility == ShellJobVisibility::CleanupPending && job.lifecycle.is_terminal();
            job_view(job)
        };
        if let Some(request_id) = request_id_to_remove {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
        }
        if remove_cleanup_terminal {
            inner.jobs_by_id.remove(&body.job_id);
        }
        if is_final_job_status(&view.status) {
            self.telemetry
                .runner_job_finalized(body.request_id.as_deref(), &body.job_id);
        }
        Ok(view)
    }
}
