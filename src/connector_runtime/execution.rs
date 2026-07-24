//! Application boundary between durable Task executions and existing jobs.

mod monitor;

use super::workspace;
use crate::auth::AuthContext;
use crate::db::{
    ConnectorExecution, ConnectorExecutionFailure, ConnectorExecutionObservation,
    ConnectorExecutionReservation, ConnectorTaskSnapshot, ConnectorTaskStoreError,
};
use crate::shell_protocol::ShellJobValidationStep;
use crate::tool_runtime::ToolRuntime;
use crate::Database;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::time::Instant;

const DEFAULT_YIELD_MS: u64 = 8_000;
const CANCEL_YIELD_MS: u64 = 5_000;
const REVIEW_POLL_MS: u64 = 100;

#[derive(Clone)]
struct MonitorTiming {
    grace: Duration,
    fast_poll: Duration,
    running_poll: Duration,
    silent_poll: Duration,
    failure_poll_max: Duration,
}

impl Default for MonitorTiming {
    fn default() -> Self {
        Self {
            grace: Duration::from_secs(30),
            fast_poll: Duration::from_millis(100),
            running_poll: Duration::from_millis(500),
            silent_poll: Duration::from_secs(1),
            failure_poll_max: Duration::from_secs(2),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CancelDispatch {
    ReferencePending,
    Sent,
    Failed,
}

#[cfg(test)]
pub(crate) struct ExecutionAttachGate {
    created: tokio::sync::Barrier,
    resume: tokio::sync::Barrier,
}

#[cfg(test)]
impl ExecutionAttachGate {
    pub(crate) fn new() -> Self {
        Self {
            created: tokio::sync::Barrier::new(2),
            resume: tokio::sync::Barrier::new(2),
        }
    }

    pub(crate) async fn wait_until_job_created(&self) {
        self.created.wait().await;
    }

    pub(crate) async fn release_attach(&self) {
        self.resume.wait().await;
    }
}

#[derive(Clone)]
pub(crate) struct ExecutionService {
    tools: Arc<ToolRuntime>,
    db: Arc<Database>,
    workspace: workspace::WorkspaceManager,
    yield_ms: u64,
    monitor_timing: MonitorTiming,
    monitors: Arc<Mutex<HashSet<String>>>,
    release_locks: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
    #[cfg(test)]
    monitor_starts: Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    attach_gate: Option<Arc<ExecutionAttachGate>>,
}

pub(crate) struct ReviewState {
    pub task: ConnectorTaskSnapshot,
    pub execution: Option<ConnectorExecution>,
    pub heartbeat: bool,
}

impl ExecutionService {
    pub(crate) fn new(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
        workspace: workspace::WorkspaceManager,
    ) -> Self {
        Self {
            tools,
            db,
            workspace,
            yield_ms: DEFAULT_YIELD_MS,
            monitor_timing: MonitorTiming::default(),
            monitors: Arc::new(Mutex::new(HashSet::new())),
            release_locks: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            monitor_starts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            attach_gate: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_yield_ms(mut self, yield_ms: u64) -> Self {
        self.yield_ms = yield_ms.max(1);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_monitor_timing(mut self, grace_ms: u64, poll_ms: u64) -> Self {
        self.monitor_timing = MonitorTiming {
            grace: Duration::from_millis(grace_ms.max(1)),
            fast_poll: Duration::from_millis(poll_ms.max(1)),
            running_poll: Duration::from_millis(poll_ms.max(1)),
            silent_poll: Duration::from_millis(poll_ms.max(1)),
            failure_poll_max: Duration::from_millis(poll_ms.max(1)),
        };
        self
    }

    #[cfg(test)]
    pub(crate) fn with_attach_gate(mut self, gate: Arc<ExecutionAttachGate>) -> Self {
        self.attach_gate = Some(gate);
        self
    }

    #[cfg(test)]
    pub(crate) fn monitor_start_count(&self) -> usize {
        self.monitor_starts
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn active_monitor_count(&self) -> usize {
        self.monitors.lock().unwrap().len()
    }

    pub(crate) fn reconcile_startup(
        &self,
        project_id: &str,
        now: i64,
    ) -> Result<(usize, usize), ConnectorTaskStoreError> {
        self.db.reconcile_connector_executions(project_id, now)
    }

    pub(crate) fn reserve(
        &self,
        task: &ConnectorTaskSnapshot,
        kind: &str,
        operation_id: &str,
        request_sha256: &str,
        check_plan: &[String],
        check_recipe: Option<&Value>,
        check_workspace_sha256: Option<&str>,
        timeout_secs: u64,
        now: i64,
    ) -> Result<ConnectorExecutionReservation, ConnectorTaskStoreError> {
        self.db.reserve_connector_execution(
            task,
            kind,
            operation_id,
            request_sha256,
            check_plan,
            check_recipe,
            check_workspace_sha256,
            now.saturating_add(timeout_secs as i64),
            now,
        )
    }

    pub(crate) async fn execute(
        &self,
        reservation: ConnectorExecutionReservation,
        task: ConnectorTaskSnapshot,
        command: String,
        cwd: Option<String>,
        timeout_secs: u64,
        auth: AuthContext,
        validation_steps: Vec<ShellJobValidationStep>,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let execution = match reservation {
            ConnectorExecutionReservation::Existing(execution) => {
                if execution.is_active() && execution.executor_reference.is_some() {
                    self.spawn_monitor(task.clone(), execution.execution_id.clone(), auth.clone());
                }
                return self
                    .wait_for_terminal(&execution.execution_id, self.yield_ms)
                    .await;
            }
            ConnectorExecutionReservation::Created(execution) => {
                self.db.start_connector_execution(
                    &execution.execution_id,
                    chrono::Utc::now().timestamp(),
                )?
            }
        };
        if execution.state != "starting" {
            return Ok(execution);
        }
        let result = self
            .tools
            .run_job_for_auth(
                task.execution_executor_ref.clone(),
                command,
                None,
                Some(timeout_secs as i64),
                cwd,
                validation_steps,
                Some(&auth),
            )
            .await;
        if !result.success {
            return self.db.finish_connector_execution(
                &execution.execution_id,
                ConnectorExecutionFailure::Submission("executor_rejected"),
                chrono::Utc::now().timestamp(),
            );
        }
        let Some(job_id) = result.output["job_id"].as_str() else {
            return self.db.finish_connector_execution(
                &execution.execution_id,
                ConnectorExecutionFailure::Submission("execution_adapter_error"),
                chrono::Utc::now().timestamp(),
            );
        };
        let status = result.output["status"].as_str().unwrap_or("queued");
        #[cfg(test)]
        if let Some(gate) = &self.attach_gate {
            gate.created.wait().await;
            gate.resume.wait().await;
        }
        let attached = self.db.attach_connector_executor(
            &execution.execution_id,
            job_id,
            status,
            chrono::Utc::now().timestamp(),
        )?;
        if attached.state == "cancel_requested" {
            if self.dispatch_cancel(&task, &attached, &auth).await == CancelDispatch::Failed {
                return self.db.finish_connector_execution(
                    &execution.execution_id,
                    ConnectorExecutionFailure::Unknown("cancel_transport_unknown"),
                    chrono::Utc::now().timestamp(),
                );
            }
        }
        if attached.is_terminal() {
            return Ok(attached);
        }
        self.spawn_monitor(task, execution.execution_id.clone(), auth);
        self.wait_for_terminal(&execution.execution_id, self.yield_ms)
            .await
    }

    pub(crate) async fn cancel_task(
        &self,
        task: ConnectorTaskSnapshot,
        reason: Option<&str>,
        auth: AuthContext,
    ) -> Result<Option<ConnectorExecution>, ConnectorTaskStoreError> {
        let requested = self.db.request_connector_execution_cancel(
            &task,
            reason,
            chrono::Utc::now().timestamp(),
        )?;
        let Some(mut execution) = requested else {
            self.release_cancelled_workspace(task).await;
            return Ok(None);
        };
        if execution.is_terminal() {
            self.release_cancelled_workspace(task).await;
            return Ok(Some(execution));
        }
        match self.dispatch_cancel(&task, &execution, &auth).await {
            CancelDispatch::ReferencePending => return Ok(Some(execution)),
            CancelDispatch::Failed => {
                execution = self.db.finish_connector_execution(
                    &execution.execution_id,
                    ConnectorExecutionFailure::Unknown("cancel_transport_unknown"),
                    chrono::Utc::now().timestamp(),
                )?;
            }
            CancelDispatch::Sent => {
                self.spawn_monitor(task.clone(), execution.execution_id.clone(), auth);
                execution = self
                    .wait_for_terminal(&execution.execution_id, CANCEL_YIELD_MS)
                    .await?;
            }
        }
        if execution.state == "cancelled" {
            self.release_cancelled_workspace(task).await;
        }
        Ok(Some(execution))
    }

    pub(crate) async fn wait_for_review(
        &self,
        initial_task: ConnectorTaskSnapshot,
        after_cursor: Option<i64>,
        wait_ms: u64,
    ) -> Result<ReviewState, ConnectorTaskStoreError> {
        let initial = self.review_state(initial_task)?;
        if wait_ms == 0
            || after_cursor.is_some_and(|cursor| initial.task.event_cursor > cursor)
            || initial
                .execution
                .as_ref()
                .is_some_and(ConnectorExecution::is_terminal)
        {
            return Ok(initial);
        }
        let signature = execution_signature(initial.execution.as_ref());
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                tokio::time::sleep(remaining.min(Duration::from_millis(REVIEW_POLL_MS))).await;
            }
            let mut current = self.review_state(initial.task.clone())?;
            let timed_out = Instant::now() >= deadline;
            if timed_out
                || current.task.event_cursor > after_cursor.unwrap_or(initial.task.event_cursor)
                || execution_signature(current.execution.as_ref()) != signature
                || current
                    .execution
                    .as_ref()
                    .is_some_and(ConnectorExecution::is_terminal)
            {
                current.heartbeat = timed_out;
                return Ok(current);
            }
        }
    }

    fn review_state(
        &self,
        task: ConnectorTaskSnapshot,
    ) -> Result<ReviewState, ConnectorTaskStoreError> {
        let task =
            self.db
                .connector_task(&task.task_id, &task.project_id, &task.owner_subject_id)?;
        let execution = self.db.latest_connector_execution(
            &task.task_id,
            &task.project_id,
            &task.owner_subject_id,
            None,
        )?;
        Ok(ReviewState {
            task,
            execution,
            heartbeat: false,
        })
    }

    pub(crate) async fn wait_for_terminal(
        &self,
        execution_id: &str,
        wait_ms: u64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            let execution = self.db.connector_execution(execution_id)?;
            if execution.is_terminal() || Instant::now() >= deadline {
                return Ok(execution);
            }
            tokio::time::sleep(Duration::from_millis(REVIEW_POLL_MS.min(wait_ms.max(1)))).await;
        }
    }

    pub(super) async fn release_cancelled_workspace(&self, task: ConnectorTaskSnapshot) {
        let release_lock = {
            let mut locks = self.release_locks.lock().unwrap();
            locks
                .get(&task.task_id)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| {
                    let lock = Arc::new(tokio::sync::Mutex::new(()));
                    locks.insert(task.task_id.clone(), Arc::downgrade(&lock));
                    lock
                })
        };
        let guard = release_lock.lock().await;
        let warning = if task.isolated {
            let manager = self.workspace.clone();
            let release_task = task.clone();
            match tokio::task::spawn_blocking(move || manager.release_task_workspace(&release_task))
                .await
            {
                Ok(warning) => warning,
                Err(error) => Some(format!("workspace release task failed: {error}")),
            }
        } else {
            None
        };
        if let Some(ref warning) = warning {
            tracing::warn!(task_id = %task.task_id, warning, "cancelled workspace release was incomplete");
        }
        drop(guard);
        let mut locks = self.release_locks.lock().unwrap();
        if Arc::strong_count(&release_lock) == 1
            && locks
                .get(&task.task_id)
                .is_some_and(|lock| lock.ptr_eq(&Arc::downgrade(&release_lock)))
        {
            locks.remove(&task.task_id);
        }
    }

    pub(crate) async fn projection(
        &self,
        execution: &ConnectorExecution,
        auth: &AuthContext,
        include_output_tail: bool,
    ) -> Value {
        let output_tail = match (include_output_tail, execution.executor_reference.as_deref()) {
            (true, Some(job_id)) => self
                .tools
                .shell_clients
                .job_log_for_auth(Some(auth), job_id, None, None, Some(200))
                .await
                .ok()
                .map(|(_, stdout, stderr, _, _)| {
                    json!({
                        "stdout": stdout.unwrap_or_default(),
                        "stderr": stderr.unwrap_or_default(),
                        "bounded": true
                    })
                }),
            _ => None,
        };
        execution_projection(execution, chrono::Utc::now().timestamp(), output_tail)
    }
}

pub(crate) fn execution_projection(
    execution: &ConnectorExecution,
    now: i64,
    output_tail: Option<Value>,
) -> Value {
    let last_progress_at = execution
        .last_output_at
        .or(execution.started_at)
        .or(execution.queued_at)
        .unwrap_or(execution.submitted_at);
    let capability_outcome = match execution.state.as_str() {
        "succeeded" => "completed",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "interrupted" | "unknown" => "needs_attention",
        _ => "in_progress",
    };
    let queue_reason = if execution.failure_source.as_deref() == Some("queue") {
        Some("queue_deadline")
    } else if execution.state == "queued" {
        Some("executor_queue")
    } else {
        None
    };
    json!({
        "execution_id": execution.execution_id,
        "operation_id": execution.operation_id,
        "kind": execution.kind,
        "submission_status": if execution.failure_source.as_deref() == Some("submission") {
            "rejected"
        } else {
            "accepted"
        },
        "execution_status": execution.state,
        "exit_code": execution.exit_code,
        "terminal_reason": execution.terminal_reason,
        "failure_source": execution.failure_source,
        "failure_code": execution.failure_code,
        "observation_status": if execution.first_status_failure_at.is_some() && execution.is_active() {
            "degraded"
        } else {
            "available"
        },
        "first_status_failure_at": execution.first_status_failure_at,
        "last_successful_observation_at": execution.last_successful_observation_at,
        "status_failure_code": execution.status_failure_code,
        "assertion_status": assertion_status(execution),
        "assertion_evidence": execution.assertion_evidence,
        "checks": check_results(execution),
        "recipe": recipe_projection(execution),
        "capability_outcome": capability_outcome,
        "queued_at": execution.queued_at,
        "queue_age_ms": execution.queued_at.map(|queued| now.saturating_sub(queued) * 1000),
        "queue_reason": queue_reason,
        "blocker_execution_id": execution.blocks_finish().then_some(&execution.execution_id),
        "started_at": execution.started_at,
        "finished_at": execution.finished_at,
        "last_progress_at": last_progress_at,
        "silent_for_ms": now.saturating_sub(last_progress_at) * 1000,
        "stdout_cursor": execution.stdout_cursor,
        "stderr_cursor": execution.stderr_cursor,
        "output_tail": output_tail,
        "blocking": execution.blocks_finish(),
        "next_action": execution_next_action(execution)
    })
}

fn recipe_projection(execution: &ConnectorExecution) -> Value {
    let Some(identity) = execution.check_recipe.as_ref() else {
        return Value::Null;
    };
    json!({
        "id": identity.get("recipe_id"),
        "version": identity.get("recipe_version"),
        "root": identity.get("recipe_root_relative"),
        "checks": identity.get("semantic_checks")
    })
}

pub(super) fn durable_assertion_evidence(
    check: &str,
    recipe_identity: Option<&Value>,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Value {
    use crate::tool_runtime::validation_parser::{PARSER_KIND, PARSER_VERSION};
    use crate::tool_runtime::validation_profile::{
        validation_adapter_for_tool, ValidationFailureEvidence,
    };

    let tool = recipe_identity.and_then(|identity| {
        let checks = identity.get("semantic_checks")?.as_array()?;
        let index = checks.iter().position(|candidate| candidate == check)?;
        identity
            .get("tool_identities")?
            .as_array()?
            .get(index)?
            .as_str()
    });
    let (failure_kind, diagnostics) = tool
        .and_then(validation_adapter_for_tool)
        .map(|adapter| {
            let diagnostics = adapter.parse(stdout, stderr, true);
            let failure_kind = adapter.map_failure_kind(ValidationFailureEvidence {
                success: false,
                reported_failure_kind: Some("command_exit_nonzero"),
                exit_code: exit_code.map(i64::from),
                diagnostics: Some(&diagnostics),
                stdout_excerpt: stdout,
                stderr_excerpt: stderr,
            });
            (failure_kind, Some(diagnostics))
        })
        .unwrap_or(("process_exit", None));
    let parser = diagnostics.as_ref().map(|_| PARSER_KIND);
    let parser_version = diagnostics.as_ref().map(|_| PARSER_VERSION);
    let mut evidence = json!({
        "failed_check": check,
        "failure_kind": failure_kind,
        "exit_code": exit_code,
        "parser": parser,
        "parser_version": parser_version,
        "diagnostics": diagnostics
    });
    sanitize_evidence(&mut evidence);
    if serde_json::to_vec(&evidence)
        .is_ok_and(|bytes| bytes.len() <= crate::db::MAX_ASSERTION_EVIDENCE_BYTES)
    {
        evidence
    } else {
        json!({
            "failed_check": check,
            "failure_kind": failure_kind,
            "exit_code": exit_code,
            "parser": parser,
            "parser_version": parser_version,
            "diagnostics": null
        })
    }
}

fn sanitize_evidence(value: &mut Value) {
    match value {
        Value::String(text) => *text = crate::validation_bridge::sanitize_bridge_text(text),
        Value::Array(items) => items.iter_mut().for_each(sanitize_evidence),
        Value::Object(fields) => fields.values_mut().for_each(sanitize_evidence),
        _ => {}
    }
}

fn assertion_status(execution: &ConnectorExecution) -> &'static str {
    if execution.kind != "check" {
        return "not_run";
    }
    match execution.state.as_str() {
        "succeeded" => "passed",
        "failed" if execution.failure_source.as_deref() == Some("check") => "failed",
        "accepted" | "queued" | "starting" | "running" | "cancel_requested" => "in_progress",
        _ => "not_run",
    }
}

fn check_results(execution: &ConnectorExecution) -> Value {
    if execution.kind != "check" {
        return Value::Null;
    }
    let assertion = assertion_status(execution);
    Value::Array(
        execution
            .check_plan
            .iter()
            .enumerate()
            .map(|(index, check)| {
                let status = if index < execution.check_completed {
                    "passed"
                } else if assertion == "failed"
                    && execution.failed_check.as_deref() == Some(check.as_str())
                {
                    "failed"
                } else if index == execution.check_completed && assertion == "in_progress" {
                    "in_progress"
                } else {
                    "not_run"
                };
                json!({ "check": check, "status": status })
            })
            .collect(),
    )
}

fn execution_next_action(execution: &ConnectorExecution) -> &'static str {
    if execution.failure_source.as_deref() == Some("executor")
        && execution
            .failure_code
            .as_deref()
            .is_some_and(|code| code.starts_with("validation_"))
    {
        return "upgrade_agent_and_rerun_checks";
    }
    match execution.state.as_str() {
        "accepted" | "queued" | "starting" | "running" => "review_or_cancel",
        "cancel_requested" => "wait_for_cancellation",
        "succeeded" | "failed" => "continue_or_finish",
        "cancelled" => "start_a_new_task",
        "interrupted" => "resume_or_reject_on_the_host",
        "unknown" => "inspect_executor_state_before_continuing",
        _ => "review_task",
    }
}

fn execution_signature(
    execution: Option<&ConnectorExecution>,
) -> Option<(&str, usize, usize, Option<i64>)> {
    execution.map(|execution| {
        (
            execution.state.as_str(),
            execution.stdout_cursor,
            execution.stderr_cursor,
            execution.first_status_failure_at,
        )
    })
}
