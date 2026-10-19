use super::helpers::{bounded_tail, command_rejected_message, COMMAND_STDIO_TAIL_CHARS};
use super::process::{
    add_structured_continuation_facts, classify_process_failure, command_failure_result,
    outcome_unknown_result, process_tool_failure_result, success_output,
    terminal_structured_job_result,
};
use super::shell::{dispatch_uncertainty_lifecycle, runner_command_lifecycle};
use super::{ExecutionPurpose, ToolResult};
use crate::auth::AuthContext;
use crate::runner_http::RunnerRegistry;
use crate::runner_protocol::{
    ShellCommandExecutionState, ShellJobInfo, ShellRunResponse, PROCESS_TIMEOUT_MAX_SECS,
    SCRIPT_TIMEOUT_MAX_SECS, STRUCTURED_EXECUTION_TIMEOUT_DEFAULT_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS, STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_runner_registry::RunnerAccess;

/// Canonical model-facing synchronous grace before an already-started
/// structured execution is handed off as the same durable Job.
pub(crate) const STRUCTURED_EXECUTION_SYNC_WAIT_SECS: u64 = 10;
pub(crate) const INITIAL_JOB_HANDOFF_TAIL_LINES: usize = 40;
pub(crate) use webcodex_core::runtime_contract::STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StructuredExecutionBudget {
    pub(crate) effective_timeout_secs: u64,
    pub(crate) sync_wait_secs: u64,
}

impl StructuredExecutionBudget {
    pub(crate) fn resolve_process(timeout_secs: Option<u64>) -> Result<Self, String> {
        Self::resolve_with_timeout_max(timeout_secs, None, PROCESS_TIMEOUT_MAX_SECS)
    }

    pub(crate) fn resolve_process_with_sync_wait(
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
    ) -> Result<Self, String> {
        Self::resolve_with_timeout_max(timeout_secs, sync_wait_secs, PROCESS_TIMEOUT_MAX_SECS)
    }

    pub(crate) fn resolve_script_with_sync_wait(
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
    ) -> Result<Self, String> {
        Self::resolve_with_timeout_max(timeout_secs, sync_wait_secs, SCRIPT_TIMEOUT_MAX_SECS)
    }

    pub(crate) fn resolve_with_sync_wait(
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
    ) -> Result<Self, String> {
        Self::resolve_with_timeout_max(
            timeout_secs,
            sync_wait_secs,
            STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
        )
    }

    fn resolve_with_timeout_max(
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
        timeout_max_secs: u64,
    ) -> Result<Self, String> {
        let requested_timeout_secs =
            timeout_secs.unwrap_or(STRUCTURED_EXECUTION_TIMEOUT_DEFAULT_SECS);
        if requested_timeout_secs < STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS {
            return Err(format!(
                "timeout_secs must be at least {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS}"
            ));
        }
        let effective_timeout_secs = requested_timeout_secs.min(timeout_max_secs);
        let sync_wait_secs = match sync_wait_secs {
            Some(0) => return Err("sync_wait_secs must be at least 1".to_string()),
            Some(sync_wait_secs) => sync_wait_secs
                .min(STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS)
                .min(effective_timeout_secs),
            None => STRUCTURED_EXECUTION_SYNC_WAIT_SECS.min(effective_timeout_secs),
        };
        Ok(Self {
            effective_timeout_secs,
            sync_wait_secs,
        })
    }
}

/// Wait for an admitted durable execution and project its existing Job. Admission
/// stays with the caller; this path cannot enqueue a replacement execution.
pub(crate) async fn await_structured_job_result(
    clients: Arc<RunnerRegistry>,
    job_id: String,
    sync_wait_limit: Duration,
    auth: Option<&AuthContext>,
    budget: StructuredExecutionBudget,
    summary: &str,
    purpose: ExecutionPurpose,
) -> Result<ToolResult, StructuredJobHandoffFailure> {
    let wait = sync_wait_limit.min(Duration::from_secs(budget.sync_wait_secs));
    match await_hidden_structured_job(clients.clone(), job_id, wait, auth.cloned()).await? {
        HiddenStructuredJobWait::Terminal {
            job,
            stdout,
            stderr,
        } => {
            let mut result =
                terminal_structured_job_result(&job, stdout, stderr, budget.effective_timeout_secs);
            add_structured_continuation_facts(
                &mut result,
                budget.effective_timeout_secs,
                budget.sync_wait_secs,
                true,
            );
            finalize_hidden_terminal_projection(&clients, auth, &job, &mut result, budget).await;
            Ok(result)
        }
        HiddenStructuredJobWait::Continued {
            observation,
            execution_state,
            command_started,
        } => {
            let detected_summary = super::jobs::detected_job_summary_with_activity(
                Some(summary),
                Some(purpose.as_str()),
                &observation.job.status,
                observation.job.exit_code.map(i64::from),
                &observation.stdout_tail,
                &observation.stderr_tail,
                observation.stdout_truncated || observation.stderr_truncated,
                observation.job.activity.as_ref(),
            );
            let continuation = super::jobs::observe_job_continuation(
                &observation.job.job_id,
                observation.job.observation_token.as_deref(),
            );
            Ok(ToolResult::ok(json!({
                "execution_state": execution_state,
                "command_started": command_started,
                "command_completed": false,
                "command_ok": false,
                "exit_code": null,
                "failure_kind": null,
                "tool_failure": false,
                "promoted_to_job": true,
                "terminal": false,
                "job_id": observation.job.job_id,
                "job_status": observation.job.status,
                "observation_token": observation.job.observation_token,
                "continuation_semantics": super::jobs::job_observation_continuation_semantics(),
                "activity": observation.job.activity,
                "effective_timeout_secs": budget.effective_timeout_secs,
                "sync_wait_secs": budget.sync_wait_secs,
                "async_handoff_available": true,
                "stdout_tail": observation.stdout_tail,
                "stderr_tail": observation.stderr_tail,
                "stdout_lines": observation.stdout_lines,
                "stderr_lines": observation.stderr_lines,
                "stdout_truncated": observation.stdout_truncated,
                "stderr_truncated": observation.stderr_truncated,
                "detected_summary": detected_summary,
                "continuation": continuation,
            })))
        }
    }
}

/// Entry-specific diagnostic wording; it never selects lifecycle or retry state.
#[derive(Clone, Copy)]
pub(crate) enum StructuredRequestKind {
    Process,
    Script,
}

impl StructuredRequestKind {
    fn name(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Script => "script",
        }
    }

    fn rejection_hint(self) -> &'static str {
        match self {
            Self::Process => "inspect the rejection, correct executable/argv/cwd, then retry.",
            Self::Script => "inspect the rejection, correct language/script/args/cwd, then retry.",
        }
    }
}

enum StructuredRequestWaitFailure {
    WaiterDropped,
    TimedOut,
}

/// Complete one already-enqueued synchronous request. A lost response is not
/// proof that the request was unstarted: only cancellation's dispatch evidence
/// can establish that, and an absent record remains uncertain.
pub(crate) async fn await_structured_request_result(
    clients: &RunnerRegistry,
    request_id: &str,
    receiver: tokio::sync::oneshot::Receiver<ShellRunResponse>,
    timeout_secs: u64,
    kind: StructuredRequestKind,
) -> ToolResult {
    let failure = match tokio::time::timeout(Duration::from_secs(timeout_secs + 2), receiver).await
    {
        Ok(Ok(response)) => return structured_response_result(response, timeout_secs, kind),
        Ok(Err(_)) => StructuredRequestWaitFailure::WaiterDropped,
        Err(_) => StructuredRequestWaitFailure::TimedOut,
    };
    let dispatch = clients.cancel_request_dispatch_state(request_id).await;
    let state = dispatch_uncertainty_lifecycle(dispatch);
    let name = kind.name();
    match (failure, state) {
        (StructuredRequestWaitFailure::WaiterDropped, ShellCommandExecutionState::NotStarted) => {
            process_tool_failure_result(
                command_rejected_message(
                    format!("{name} request waiter was dropped before Runner dispatch"),
                    "check Runner connectivity, then retry.",
                ),
                "runtime_error",
                state,
            )
        }
        (StructuredRequestWaitFailure::TimedOut, ShellCommandExecutionState::NotStarted) => {
            process_tool_failure_result(
                command_rejected_message(
                    format!("timed out waiting {timeout_secs} seconds before Runner dispatch"),
                    "check Runner connectivity and availability, then retry.",
                ),
                "timeout",
                state,
            )
        }
        (StructuredRequestWaitFailure::WaiterDropped, _) => outcome_unknown_result(format!(
            "{name} request waiter was dropped after dispatch may have occurred"
        )),
        (StructuredRequestWaitFailure::TimedOut, _) => outcome_unknown_result(format!(
            "timed out waiting {timeout_secs} seconds for the Runner {name} result"
        )),
    }
}

fn structured_response_result(
    response: ShellRunResponse,
    timeout_secs: u64,
    kind: StructuredRequestKind,
) -> ToolResult {
    let state = runner_command_lifecycle(&response, timeout_secs);
    let exit_code = response.exit_code;
    let stdout = response.stdout.unwrap_or_default();
    let stderr = response.stderr.unwrap_or_default();
    match state {
        ShellCommandExecutionState::NotStarted => {
            let fallback = format!("Runner rejected the {} before spawn", kind.name());
            let reason = response.error.as_deref().unwrap_or(&fallback);
            process_tool_failure_result(
                command_rejected_message(reason, kind.rejection_hint()),
                classify_process_failure(reason),
                state,
            )
        }
        ShellCommandExecutionState::OutcomeUnknown => outcome_unknown_result(
            response
                .error
                .as_deref()
                .unwrap_or("the Runner did not return a trustworthy terminal result"),
        ),
        ShellCommandExecutionState::Completed
            if response.error.is_none() && exit_code == Some(0) =>
        {
            ToolResult::ok(success_output(0, stdout, stderr, response.duration_ms))
        }
        ShellCommandExecutionState::TimedOut | ShellCommandExecutionState::Completed => {
            command_failure_result(
                exit_code,
                stdout,
                stderr,
                response.duration_ms,
                timeout_secs,
                state,
            )
        }
    }
}

pub(crate) enum HiddenStructuredJobWait {
    Terminal {
        job: ShellJobInfo,
        stdout: String,
        stderr: String,
    },
    Continued {
        observation: StructuredJobObservation,
        execution_state: &'static str,
        command_started: bool,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct StructuredJobObservation {
    pub(crate) job: ShellJobInfo,
    pub(crate) stdout_tail: String,
    pub(crate) stderr_tail: String,
    pub(crate) stdout_lines: usize,
    pub(crate) stderr_lines: usize,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

pub(crate) async fn structured_job_observation(
    clients: &RunnerRegistry,
    access: Option<&RunnerAccess>,
    job_id: &str,
) -> Result<StructuredJobObservation, String> {
    let (job, stdout, stderr, next_stdout_line, next_stderr_line) = clients
        .hidden_job_log_for_auth(access, job_id, Some(INITIAL_JOB_HANDOFF_TAIL_LINES))
        .await?;
    let stdout = stdout.unwrap_or_default();
    let stderr = stderr.unwrap_or_default();
    let stdout_lines = next_stdout_line.saturating_sub(1);
    let stderr_lines = next_stderr_line.saturating_sub(1);
    let (stdout_tail, stdout_char_truncated) = bounded_tail(&stdout, COMMAND_STDIO_TAIL_CHARS);
    let (stderr_tail, stderr_char_truncated) = bounded_tail(&stderr, COMMAND_STDIO_TAIL_CHARS);
    let stdout_truncated = job.stdout_log_truncated
        || job.stdout_retained_from_line.is_some_and(|line| line > 1)
        || stdout.lines().count() < stdout_lines
        || stdout_char_truncated;
    let stderr_truncated = job.stderr_log_truncated
        || job.stderr_retained_from_line.is_some_and(|line| line > 1)
        || stderr.lines().count() < stderr_lines
        || stderr_char_truncated;
    Ok(StructuredJobObservation {
        job,
        stdout_tail,
        stderr_tail,
        stdout_lines,
        stderr_lines,
        stdout_truncated,
        stderr_truncated,
    })
}

/// An execution has already been admitted. Only an independently authorized
/// Public record may carry identity into a model-facing failure; error prose is
/// deliberately not retained as recovery authority (or leaked as payload).
pub(crate) struct StructuredJobHandoffFailure {
    public_job: Option<ShellJobInfo>,
}

impl StructuredJobHandoffFailure {
    pub(crate) async fn unresolved(
        clients: &RunnerRegistry,
        access: Option<&RunnerAccess>,
        job_id: &str,
    ) -> Self {
        Self {
            public_job: clients.get_job_for_auth(access, job_id).await.ok(),
        }
    }

    pub(crate) fn has_public_continuation(&self) -> bool {
        self.public_job.is_some()
    }

    pub(crate) fn into_tool_result(
        self,
        project: &str,
        budget: StructuredExecutionBudget,
    ) -> super::ToolResult {
        use serde_json::json;
        let mut result = super::process::outcome_unknown_result(
            "durable execution was admitted but handoff observation failed; recover the original execution before considering any retry",
        );
        super::process::add_structured_continuation_facts(
            &mut result,
            budget.effective_timeout_secs,
            budget.sync_wait_secs,
            true,
        );
        if let Some(job) = self.public_job {
            result.output["promoted_to_job"] = json!(true);
            result.output["job_id"] = json!(job.job_id);
            result.output["job_status"] = json!(job.status);
            result.output["activity"] = serde_json::Value::Null;
            result.output["continuation"] = super::jobs::observe_job_continuation(
                &job.job_id,
                job.observation_token.as_deref(),
            );
        } else {
            result.output["suggested_call"] = super::SuggestedToolCall::fallback_recovery(
                "list_jobs",
                json!({"project": project}),
            )
            .to_value();
        }
        result
    }
}

/// Finalize a hidden terminal projection without destroying recovery evidence.
///
/// Conclusive terminal executions keep the existing sync-first behavior and are
/// removed after their result is projected. `outcome_unknown` is intentionally
/// different: effects may have occurred, so publish the same durable Job and
/// attach its exact observation continuation. If publication itself cannot be
/// proven, retain the hidden terminal record rather than deleting evidence.
pub(crate) async fn finalize_hidden_terminal_projection(
    clients: &RunnerRegistry,
    auth: Option<&AuthContext>,
    job: &ShellJobInfo,
    result: &mut super::ToolResult,
    budget: StructuredExecutionBudget,
) {
    if job.command_execution_state != Some(ShellCommandExecutionState::OutcomeUnknown) {
        clients
            .remove_projected_hidden_terminal_job_record(&job.job_id)
            .await;
        return;
    }

    let access = crate::runner_http::runner_access_from_auth(auth);
    let Ok(public_job) = clients
        .promote_hidden_job(access.as_ref(), &job.job_id)
        .await
    else {
        return;
    };

    super::process::add_structured_continuation_facts(
        result,
        budget.effective_timeout_secs,
        budget.sync_wait_secs,
        true,
    );
    result.output["promoted_to_job"] = serde_json::json!(true);
    result.output["job_id"] = serde_json::json!(public_job.job_id);
    result.output["job_status"] = serde_json::json!(public_job.status);
    result.output["activity"] =
        serde_json::to_value(public_job.activity).unwrap_or(serde_json::Value::Null);
    if let Some(observation_token) = public_job.observation_token.as_deref() {
        result.output["observation_token"] = serde_json::json!(observation_token);
    }
    result.output["continuation"] =
        super::jobs::observe_job_continuation(&job.job_id, public_job.observation_token.as_deref());
}

/// Re-observe/promote the same durable record, never a replacement execution.
/// A terminal race belongs to the initiating call; an unprojectable hidden or
/// cleanup-pending record stays private and keeps its cleanup guard armed.
pub(crate) async fn recover_hidden_structured_job(
    clients: &RunnerRegistry,
    access: Option<&RunnerAccess>,
    job_id: &str,
) -> Result<HiddenStructuredJobWait, StructuredJobHandoffFailure> {
    if let Ok(job) = clients.promote_hidden_job(access, job_id).await {
        if !super::jobs::is_terminal_job_status(&job.status) {
            return Err(StructuredJobHandoffFailure {
                public_job: Some(job),
            });
        }
        if let Ok(terminal) = hidden_terminal_snapshot(clients, access, job_id).await {
            return Ok(terminal);
        }
    }
    // Public may already have been established before observation failed. This
    // lookup rechecks visibility and caller authority; it cannot reveal hidden
    // terminal or CleanupPending records and does not infer a token from prose.
    Err(StructuredJobHandoffFailure::unresolved(clients, access, job_id).await)
}

pub(crate) async fn await_hidden_structured_job(
    clients: Arc<RunnerRegistry>,
    job_id: String,
    sync_wait: Duration,
    auth: Option<AuthContext>,
) -> Result<HiddenStructuredJobWait, StructuredJobHandoffFailure> {
    let access = crate::runner_http::runner_access_from_auth(auth.as_ref());
    let mut guard = HiddenJobCleanupGuard::new(clients.clone(), job_id.clone(), access.clone());
    let attempt: Result<HiddenStructuredJobWait, String> = async {
        let deadline = std::time::Instant::now() + sync_wait;
        loop {
            if let Ok(job) = clients
                .get_hidden_job_for_auth(access.as_ref(), &job_id)
                .await
            {
                if crate::tool_runtime::jobs::is_terminal_job_status(&job.status) {
                    let terminal =
                        hidden_terminal_snapshot(&clients, access.as_ref(), &job_id).await?;
                    guard.disarm();
                    return Ok(terminal);
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let promoted = clients.promote_hidden_job(access.as_ref(), &job_id).await?;
        if crate::tool_runtime::jobs::is_terminal_job_status(&promoted.status) {
            let terminal = hidden_terminal_snapshot(&clients, access.as_ref(), &job_id).await?;
            guard.disarm();
            return Ok(terminal);
        }
        let observation = structured_job_observation(&clients, access.as_ref(), &job_id).await?;
        if crate::tool_runtime::jobs::is_terminal_job_status(&observation.job.status) {
            let terminal = hidden_terminal_snapshot(&clients, access.as_ref(), &job_id).await?;
            guard.disarm();
            return Ok(terminal);
        }
        let (execution_state, command_started) = continued_execution_state(
            observation.job.status.as_str(),
            observation.job.started_at.is_some(),
        );
        guard.disarm();
        Ok(HiddenStructuredJobWait::Continued {
            observation,
            execution_state,
            command_started,
        })
    }
    .await;
    let result = match attempt {
        Ok(result) => Ok(result),
        Err(_) => {
            Box::pin(recover_hidden_structured_job(
                &clients,
                access.as_ref(),
                &job_id,
            ))
            .await
        }
    };
    if result.is_ok()
        || result
            .as_ref()
            .is_err_and(|failure| failure.has_public_continuation())
    {
        guard.disarm();
    }
    result
}

fn continued_execution_state(status: &str, started: bool) -> (&'static str, bool) {
    // Recovery is a Server observation overlay, not Runner lifecycle truth, and
    // does not prove that execution is presently running.
    if status == "recovering" {
        return ("outcome_unknown", true);
    }
    match RunnerJobLifecycle::from_wire(status).ok() {
        Some(
            RunnerJobLifecycle::Queued
            | RunnerJobLifecycle::RunnerQueued
            | RunnerJobLifecycle::StartedLegacy,
        ) if started => ("running", true),
        Some(
            RunnerJobLifecycle::Queued
            | RunnerJobLifecycle::RunnerQueued
            | RunnerJobLifecycle::StartedLegacy,
        ) => ("queued", false),
        Some(RunnerJobLifecycle::Running) => ("running", true),
        Some(RunnerJobLifecycle::StopRequested) if started => ("running", true),
        Some(RunnerJobLifecycle::StopRequested) => ("queued", false),
        _ => ("outcome_unknown", true),
    }
}

async fn hidden_terminal_snapshot(
    clients: &RunnerRegistry,
    access: Option<&RunnerAccess>,
    job_id: &str,
) -> Result<HiddenStructuredJobWait, String> {
    let (job, stdout, stderr, _, _) = clients
        .hidden_job_log_for_auth(access, job_id, None)
        .await?;
    Ok(HiddenStructuredJobWait::Terminal {
        job,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

/// Cancels an initially hidden Job if the initiating future is dropped before
/// it can return either a terminal projection or a public continuation.
struct HiddenJobCleanupGuard {
    clients: Arc<RunnerRegistry>,
    job_id: String,
    access: Option<RunnerAccess>,
    armed: bool,
}

impl HiddenJobCleanupGuard {
    fn new(clients: Arc<RunnerRegistry>, job_id: String, access: Option<RunnerAccess>) -> Self {
        Self {
            clients,
            job_id,
            access,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for HiddenJobCleanupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let clients = self.clients.clone();
        clients.record_hidden_cleanup_intent(self.job_id.clone(), self.access.clone());
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                clients.process_hidden_cleanup_intents().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continued_execution_state_uses_the_fresh_observed_started_state() {
        assert_eq!(
            continued_execution_state("stop_requested", false),
            ("queued", false)
        );
        assert_eq!(
            continued_execution_state("stop_requested", true),
            ("running", true)
        );
        assert_eq!(
            continued_execution_state("agent_queued", false),
            ("queued", false)
        );
        assert_eq!(
            continued_execution_state("agent_queued", true),
            ("running", true)
        );
        assert_eq!(
            continued_execution_state("started", true),
            ("running", true)
        );
        assert_eq!(
            continued_execution_state("running", true),
            ("running", true)
        );
        assert_eq!(
            continued_execution_state("recovering", true),
            ("outcome_unknown", true)
        );
    }

    #[test]
    fn structured_execution_budget_clamps_oversized_preferences_and_rejects_zero() {
        let default = StructuredExecutionBudget::resolve_with_sync_wait(None, None).unwrap();
        assert_eq!(default.effective_timeout_secs, 60);
        assert_eq!(default.sync_wait_secs, 10);

        let short = StructuredExecutionBudget::resolve_with_sync_wait(Some(5), None).unwrap();
        assert_eq!(short.effective_timeout_secs, 5);
        assert_eq!(short.sync_wait_secs, 5);

        for wait in [1, 45, STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS] {
            let budget =
                StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(wait)).unwrap();
            assert_eq!(budget.effective_timeout_secs, 600);
            assert_eq!(budget.sync_wait_secs, wait);
        }

        let host_boundary =
            StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(60)).unwrap();
        assert_eq!(host_boundary.effective_timeout_secs, 600);
        assert_eq!(
            host_boundary.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS
        );

        let oversized =
            StructuredExecutionBudget::resolve_with_sync_wait(Some(4_000), Some(600)).unwrap();
        assert_eq!(
            oversized.effective_timeout_secs,
            STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS
        );
        assert_eq!(
            oversized.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS
        );

        let process_default = StructuredExecutionBudget::resolve_process(None).unwrap();
        assert_eq!(process_default.effective_timeout_secs, 60);
        let process_six_hours =
            StructuredExecutionBudget::resolve_process_with_sync_wait(Some(21_600), None).unwrap();
        assert_eq!(process_six_hours.effective_timeout_secs, 21_600);
        assert_eq!(
            process_six_hours.sync_wait_secs,
            STRUCTURED_EXECUTION_SYNC_WAIT_SECS
        );
        let process_oversized =
            StructuredExecutionBudget::resolve_process(Some(PROCESS_TIMEOUT_MAX_SECS + 1)).unwrap();
        assert_eq!(
            process_oversized.effective_timeout_secs,
            PROCESS_TIMEOUT_MAX_SECS
        );

        let script_default =
            StructuredExecutionBudget::resolve_script_with_sync_wait(None, None).unwrap();
        assert_eq!(script_default.effective_timeout_secs, 60);
        let script_six_hours =
            StructuredExecutionBudget::resolve_script_with_sync_wait(Some(21_600), None).unwrap();
        assert_eq!(script_six_hours.effective_timeout_secs, 21_600);
        let script_oversized = StructuredExecutionBudget::resolve_script_with_sync_wait(
            Some(SCRIPT_TIMEOUT_MAX_SECS + 1),
            None,
        )
        .unwrap();
        assert_eq!(
            script_oversized.effective_timeout_secs,
            SCRIPT_TIMEOUT_MAX_SECS
        );

        let over_total =
            StructuredExecutionBudget::resolve_with_sync_wait(Some(5), Some(60)).unwrap();
        assert_eq!(over_total.effective_timeout_secs, 5);
        assert_eq!(over_total.sync_wait_secs, 5);

        assert!(StructuredExecutionBudget::resolve_with_sync_wait(Some(0), None).is_err());
        assert!(StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(0)).is_err());
    }
}
