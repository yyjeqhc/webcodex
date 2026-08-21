use super::auth::{assert_shell_client_access, shell_job_visible_to_auth};
use super::jobs::{
    append_log_limited, assert_active_instance_locked, command_preview, is_final_job_status,
    job_view, notify_job_update, observe_job_terminal, process_preview, refresh_job_status_locked,
    replace_log_limited, script_preview, select_log_lines,
};
use super::reconciliation::validate_stream_snapshot;
use super::requests::{
    enqueue_pending_request_locked, next_request_id, notify_client_locked,
    remove_pending_request_locked,
};
use super::state::{DetachedIdempotencyIntent, ShellJobRecord, ShellJobVisibility};
use super::validation::{validate_agent_instance_id, validate_id, validate_run_request};
use super::{
    now_ts, ShellClientRegistry, DETACHED_IDEMPOTENCY_CONFLICT,
    DETACHED_IDEMPOTENCY_RECOVERY_PREFIX,
};
use crate::shell_protocol::{
    validate_process_argv, validate_script_request, validation_infrastructure_failure_code,
    ShellAgentJobUpdateRequest, ShellAgentShellRequest, ShellCommandExecutionState,
    ShellJobContext, ShellJobInfo, ShellJobOpRequest, ShellJobStructuredExecutionMetadata,
    ShellJobValidationMetadata, ShellJobValidationStep, ShellProcessArgv, ShellRunRequest,
    ShellScriptPayload, DETACHED_IDEMPOTENCY_KEY_MAX_BYTES, PROCESS_CWD_MAX_BYTES,
    PROCESS_STDIN_MAX_BYTES, STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct ValidationProtocolError(&'static str);

/// Outcome of a bounded `job_log`/`job_tail` wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum JobLogWaitOutcome {
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
    pub(crate) fn as_str(&self) -> &'static str {
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
pub(crate) struct JobLogWait {
    pub(crate) wait_outcome: JobLogWaitOutcome,
    pub(crate) waited_ms: u64,
    /// Whether the job changed relative to the supplied `after_observation_token`.
    /// Always false when no `after_observation_token` was provided.
    pub(crate) changed: bool,
    /// Whether the job is terminal per the canonical job terminal definition.
    pub(crate) terminal: bool,
}

impl Default for JobLogWait {
    fn default() -> Self {
        Self {
            wait_outcome: JobLogWaitOutcome::Immediate,
            waited_ms: 0,
            changed: false,
            terminal: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JobPublicMutationSignature {
    status: String,
    recovery_state: Option<String>,
    recovery_reason_code: Option<String>,
    last_update_seq: u64,
    stdout: super::state::ShellJobLogState,
    stderr: super::state::ShellJobLogState,
    error: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    command_execution_state: Option<ShellCommandExecutionState>,
    validation_progress: Option<crate::shell_protocol::ShellJobValidationProgress>,
    recovered_after_server_restart: bool,
    reconciled_at: Option<i64>,
}

fn public_mutation_signature(job: &ShellJobRecord) -> JobPublicMutationSignature {
    JobPublicMutationSignature {
        status: job.status.clone(),
        recovery_state: job.recovery_state.clone(),
        recovery_reason_code: job.recovery_reason_code.clone(),
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
        recovered_after_server_restart: job.recovered_after_server_restart,
        reconciled_at: job.reconciled_at,
    }
}

fn invalid_progress(code: &'static str) -> Result<(), ValidationProtocolError> {
    Err(ValidationProtocolError(code))
}

fn validate_validation_progress(
    job: &ShellJobRecord,
    update: &ShellAgentJobUpdateRequest,
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
    let status = update.status.trim();
    if update.finished && !is_final_job_status(status) {
        return invalid_progress("validation_progress_invalid");
    }
    let cancelling = matches!(
        status,
        "stopped" | "cancelled" | "timeout" | "timed_out" | "lost"
    );
    let Some(progress) = update.validation_progress.as_ref() else {
        if matches!(status, "queued" | "agent_queued") || cancelling {
            return Ok(());
        }
        return invalid_progress("validation_progress_missing");
    };
    if matches!(status, "queued" | "agent_queued")
        || progress.completed > job.validation_steps.len()
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
    let infrastructure_failure = status == "failed"
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
    } else if !is_final_job_status(status) {
        let expected = job.validation_steps.get(progress.completed);
        expected.map(String::as_str) == progress.current_step.as_deref()
            && progress.failed_step.is_none()
    } else if status == "completed" && update.exit_code == Some(0) {
        progress.completed == job.validation_steps.len() && no_active_step
    } else if status == "failed" {
        let expected = job.validation_steps.get(progress.completed);
        expected.map(String::as_str) == progress.failed_step.as_deref()
            && progress.current_step.is_none()
    } else {
        false
    };
    if valid {
        Ok(())
    } else if status == "completed" && update.exit_code == Some(0) {
        invalid_progress("validation_progress_incomplete")
    } else {
        invalid_progress("validation_progress_invalid")
    }
}

fn validate_command_execution_state(
    job: &ShellJobRecord,
    update: &ShellAgentJobUpdateRequest,
) -> Result<(), ValidationProtocolError> {
    let status = update.status.trim();
    let terminal = is_final_job_status(status);
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
            matches!(status, "failed" | "stopped" | "cancelled" | "lost")
                && job.started_at.is_none()
        }
        ShellCommandExecutionState::OutcomeUnknown => matches!(status, "failed" | "lost"),
        ShellCommandExecutionState::TimedOut => matches!(status, "timeout" | "timed_out"),
        ShellCommandExecutionState::Completed => {
            matches!(status, "completed" | "failed" | "stopped" | "cancelled")
        }
    };
    if valid {
        Ok(())
    } else {
        invalid_progress("structured_job_lifecycle_invalid")
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ShellJobStartMetadata {
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) ssh_resource: Option<String>,
    pub(crate) project_cwd: Option<String>,
    pub(crate) purpose: Option<String>,
    pub(crate) shell: Option<String>,
    pub(crate) validation_steps: Vec<ShellJobValidationStep>,
    pub(crate) validation: Option<ShellJobValidationMetadata>,
    pub(crate) visibility: ShellJobVisibility,
    pub(crate) sandbox: Option<String>,
    pub(crate) structured_execution: Option<StructuredJobExecution>,
    pub(crate) stdin: Option<String>,
    /// Detached-only caller replay key. Consumed to derive the logical Job
    /// identity; never copied into Runner protocol, durable state, or audit.
    pub(crate) detached_idempotency_key: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum StructuredJobExecution {
    Process(ShellProcessArgv),
    DetachedProcess(ShellProcessArgv),
    Script(ShellScriptPayload),
}

fn validate_structured_job_common(
    cwd: Option<&str>,
    stdin: Option<&str>,
    timeout_secs: u64,
) -> Result<(), String> {
    if let Some(stdin) = stdin {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {PROCESS_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if !(STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS)
        .contains(&timeout_secs)
    {
        return Err(format!(
            "timeout_secs must be between {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS} and {STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS}"
        ));
    }
    Ok(())
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

fn detached_idempotency_principal(
    auth: Option<&crate::auth::AuthContext>,
) -> Result<String, String> {
    let Some(auth) = auth else {
        return Ok("internal".to_string());
    };
    if auth.is_bootstrap() {
        return Ok("bootstrap".to_string());
    }
    if auth.is_oauth_token() && !auth.is_oauth_shared_key_subject() {
        let user_id = auth.user_id.as_deref().ok_or_else(|| {
            "detached idempotency requires a stable OAuth user identity".to_string()
        })?;
        let client_id = auth
            .allowed_client_id
            .as_deref()
            .unwrap_or("unknown-client");
        return Ok(format!("oauth2:{user_id}:{client_id}"));
    }
    if let Some(api_key_id) = auth.api_key_id.as_deref() {
        return Ok(format!("{}:{api_key_id}", auth.principal_kind()));
    }
    Err("detached idempotency requires a stable authenticated caller identity".to_string())
}

fn detached_job_id_for_key(
    auth: Option<&crate::auth::AuthContext>,
    key: &str,
) -> Result<String, String> {
    validate_detached_idempotency_key(key)?;
    let principal = detached_idempotency_principal(auth)?;
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-detached-initiation-v1\0");
    hasher.update(principal.as_bytes());
    hasher.update(b"\0");
    hasher.update(key.as_bytes());
    Ok(format!("detached_{:x}", hasher.finalize()))
}

impl ShellClientRegistry {
    pub async fn start_job(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        self.start_job_with_metadata(body, requested_by, ShellJobStartMetadata::default())
            .await
    }

    pub(crate) async fn start_job_with_metadata(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
        metadata: ShellJobStartMetadata,
    ) -> Result<ShellJobInfo, String> {
        self.start_job_with_metadata_for_auth(body, requested_by, metadata, None)
            .await
    }

    pub(crate) async fn start_job_with_metadata_for_auth(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
        metadata: ShellJobStartMetadata,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<ShellJobInfo, String> {
        let client_id = body
            .client_id
            .clone()
            .ok_or_else(|| "client_id is required for op=start".to_string())?;
        let command = body
            .command
            .clone()
            .ok_or_else(|| "command is required for op=start".to_string())?;
        let timeout_secs = body.timeout_secs.unwrap_or(120);
        let normalized_job_cwd = body.cwd.clone().map(|cwd| cwd.trim().to_string());
        let request_id = next_request_id();
        let created_at = now_ts();
        // Ordinary Session-scoped jobs retain their Session id even without an
        // SSH resource. Only the inverse is invalid: remote execution without
        // the Workflow Session that owns the SSH context.
        if metadata.ssh_resource.is_some() && metadata.session_id.is_none() {
            return Err(
                "ssh_session_required: an SSH resource requires a Workflow Session id".to_string(),
            );
        }
        if metadata.ssh_resource.as_deref().is_some_and(|resource| {
            resource.is_empty()
                || resource.len() > 80
                || resource.contains("..")
                || !resource.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        }) {
            return Err("ssh_resource_invalid: resource name is invalid".to_string());
        }
        let sandbox = metadata.sandbox;
        let validation_steps = metadata.validation_steps;
        let validation = metadata.validation;
        let structured_execution = metadata.structured_execution;
        let structured_stdin = metadata.stdin;
        if validation_steps.len() > 3
            || validation_steps.iter().any(|step| !step.is_canonical())
            || validation_steps
                .iter()
                .map(|step| step.name.as_str())
                .collect::<HashSet<_>>()
                .len()
                != validation_steps.len()
        {
            return Err("invalid structured validation plan".to_string());
        }
        if validation
            .as_ref()
            .is_some_and(|metadata| !metadata.is_valid() || metadata.steps != validation_steps)
        {
            return Err("invalid structured validation metadata".to_string());
        }
        if structured_execution.is_some()
            && (!validation_steps.is_empty() || validation.is_some() || !command.is_empty())
        {
            return Err(
                "typed structured Job starts require command=\"\" and no validation plan"
                    .to_string(),
            );
        }
        let (
            request_kind,
            request_command,
            request_process,
            request_script,
            request_stdin,
            safe_command_preview,
            structured_metadata,
            job_kind,
        ) = match structured_execution {
            Some(StructuredJobExecution::Process(process)) => {
                validate_process_argv(&process)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    structured_stdin.as_deref(),
                    timeout_secs,
                )?;
                let preview =
                    process_preview(&process.executable, process.args.iter().map(String::as_str));
                let safe = ShellJobStructuredExecutionMetadata {
                    execution_source: "run_process".to_string(),
                    language: None,
                    script_bytes: None,
                    arg_count: process.args.len(),
                    stdin_present: structured_stdin.is_some(),
                };
                (
                    "start_process_job",
                    String::new(),
                    Some(process),
                    None,
                    structured_stdin,
                    preview,
                    Some(safe),
                    "run_process",
                )
            }
            Some(StructuredJobExecution::DetachedProcess(process)) => {
                validate_process_argv(&process)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    structured_stdin.as_deref(),
                    timeout_secs,
                )?;
                let preview = format!("detached process ({} args)", process.args.len());
                let safe = ShellJobStructuredExecutionMetadata {
                    execution_source: "run_detached_process".to_string(),
                    language: None,
                    script_bytes: None,
                    arg_count: process.args.len(),
                    stdin_present: structured_stdin.is_some(),
                };
                (
                    "start_detached_process_job",
                    String::new(),
                    Some(process),
                    None,
                    structured_stdin,
                    preview,
                    Some(safe),
                    "run_detached_process",
                )
            }
            Some(StructuredJobExecution::Script(script)) => {
                validate_script_request(
                    &script,
                    structured_stdin.as_deref(),
                    normalized_job_cwd.as_deref(),
                    timeout_secs,
                )?;
                let preview = script_preview(
                    script.language.as_str(),
                    script.script.len(),
                    script.args.len(),
                );
                let safe = ShellJobStructuredExecutionMetadata {
                    execution_source: "run_script".to_string(),
                    language: Some(script.language),
                    script_bytes: Some(script.script.len()),
                    arg_count: script.args.len(),
                    stdin_present: structured_stdin.is_some(),
                };
                (
                    "start_script_job",
                    String::new(),
                    None,
                    Some(script),
                    structured_stdin,
                    preview,
                    Some(safe),
                    "run_script",
                )
            }
            None => {
                let run = ShellRunRequest {
                    client_id: client_id.clone(),
                    cwd: normalized_job_cwd.clone(),
                    command: command.clone(),
                    stdin: None,
                    timeout_secs,
                    wait_timeout_secs: 0,
                };
                validate_run_request(&run)?;
                let request_kind = if validation_steps.is_empty() {
                    "start_job"
                } else {
                    "start_validation_job"
                };
                let request_command = if validation_steps.is_empty() {
                    command.clone()
                } else {
                    serde_json::to_string(&validation_steps)
                        .map_err(|error| format!("could not serialize validation plan: {error}"))?
                };
                let preview = if validation_steps.is_empty() {
                    command_preview(&run.command)
                } else {
                    format!(
                        "validation: {}",
                        validation_steps
                            .iter()
                            .map(|step| step.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                (
                    request_kind,
                    request_command,
                    None,
                    None,
                    None,
                    preview,
                    None,
                    "shell",
                )
            }
        };
        let detached_idempotency_key = metadata.detached_idempotency_key.as_deref();
        let detached_request = request_kind == "start_detached_process_job";
        if detached_request != detached_idempotency_key.is_some() {
            return Err(
                "detached idempotency_key must be present exactly for detached process Job starts"
                    .to_string(),
            );
        }
        let detached_intent = if detached_request {
            Some(DetachedIdempotencyIntent {
                project_id: metadata.project_id.clone(),
                session_id: metadata.session_id.clone(),
                project_cwd: metadata.project_cwd.clone(),
                cwd: normalized_job_cwd.clone(),
                purpose: metadata.purpose.clone(),
                shell: metadata.shell.clone(),
                process: request_process
                    .as_ref()
                    .expect("detached request has typed process")
                    .clone(),
                stdin: request_stdin.clone(),
                timeout_secs,
            })
        } else {
            None
        };
        let job_id = match detached_idempotency_key {
            Some(key) => detached_job_id_for_key(auth, key)?,
            None => Uuid::new_v4().to_string(),
        };
        let validation_step_names = validation_steps
            .iter()
            .map(|step| step.name.clone())
            .collect::<Vec<_>>();
        let job_context = ShellJobContext {
            runtime_project_id: metadata.project_id.clone(),
            workflow_session_id: metadata.session_id.clone(),
            ssh_resource: metadata.ssh_resource.clone(),
            project_cwd: metadata.project_cwd.clone(),
            cwd: normalized_job_cwd.clone(),
            purpose: metadata.purpose.clone(),
            shell: metadata.shell.clone(),
            command_preview: safe_command_preview.clone(),
            validation_steps: validation_step_names.clone(),
            validation: validation.clone(),
            structured_execution: structured_metadata.clone(),
        };
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: request_kind.to_string(),
            job_id: Some(job_id.clone()),
            cwd: normalized_job_cwd.clone(),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: request_command,
            process: request_process,
            script: request_script,
            stdin: request_stdin,
            timeout_secs,
            requested_by,
            created_at,
            validation: None,
            lsp: None,
            sandbox,
            job_context: Some(job_context),
            mcp_gateway: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if auth.is_some() {
            assert_shell_client_access(auth, client)?;
        }
        if !(client.capabilities.async_jobs || client.capabilities.async_shell_jobs) {
            return Err(format!(
                "agent client {} does not support async shell jobs",
                client_id
            ));
        }
        if structured_metadata.is_some() && !client.capabilities.structured_execution_jobs {
            return Err(format!(
                "capability_unavailable: agent client {client_id} does not support structured_execution_jobs"
            ));
        }
        if request.kind == "start_detached_process_job"
            && !client.capabilities.detached_process_jobs
        {
            return Err(format!(
                "capability_unavailable: agent client {client_id} does not support detached_process_jobs"
            ));
        }
        if structured_metadata.is_some() && metadata.ssh_resource.is_some() {
            return Err(
                "ssh_resource_unsupported_for_request: typed structured Jobs do not support SSH resources"
                    .to_string(),
            );
        }
        if metadata.ssh_resource.is_some() && !client.capabilities.ssh_shell {
            return Err(format!(
                "agent_capability_unavailable: agent client {} does not support ssh_shell",
                client_id
            ));
        }
        if let Some(mode) = request.sandbox.as_deref() {
            if mode != crate::command_sandbox::INSPECT_SANDBOX_MODE {
                return Err(format!("unknown sandbox mode '{mode}'"));
            }
            if !client.capabilities.sandbox_inspect_commands {
                return Err(format!(
                    "{}: agent client {} cannot enforce the inspect sandbox",
                    crate::shell_protocol::SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS,
                    client_id
                ));
            }
        }
        if !validation_steps.is_empty() && !client.capabilities.structured_validation_argv {
            return Err(format!(
                "structured_validation_unavailable: agent client {} does not support structured argv validation jobs",
                client_id
            ));
        }
        if validation_steps
            .iter()
            .any(crate::shell_protocol::ShellJobValidationStep::is_structured_go_test_json)
            && !client.capabilities.structured_go_test_json
        {
            return Err(format!(
                "structured_go_test_json_unavailable: agent client {} does not support machine-readable Go test validation",
                client_id
            ));
        }
        if validation_steps.iter().any(|step| {
            step.is_structured_go_test_json() && step.args.as_slice() != ["test", "-json", "./..."]
        }) && !client.capabilities.structured_go_test_packages
        {
            return Err(format!(
                "structured_go_test_packages_unavailable: agent client {} does not support focused Go package validation argv",
                client_id
            ));
        }
        if validation
            .as_ref()
            .is_some_and(|metadata| metadata.tool == "go_test")
            && !client.capabilities.structured_go_test_tool
        {
            return Err(format!(
                "structured_go_test_tool_unavailable: agent client {} does not support the first-class go_test validation contract",
                client_id
            ));
        }
        if metadata
            .project_id
            .as_deref()
            .is_some_and(|project| inner.unregistering_projects.contains_key(project))
        {
            return Err("project_unregister_in_progress".to_string());
        }
        let agent_instance_id = client.agent_instance_id.clone();
        let auth_group = client.auth_group.clone();
        if let Some(intent) = detached_intent.as_ref() {
            if let Some(existing) = inner.jobs_by_id.get(&job_id) {
                if existing.kind != "run_detached_process" {
                    return Err(format!(
                        "{DETACHED_IDEMPOTENCY_CONFLICT}: idempotency key resolves to a non-detached Job"
                    ));
                }
                match existing.detached_idempotency_intent.as_ref() {
                    Some(existing_intent) if existing_intent == intent => {
                        return Ok(job_view(existing));
                    }
                    Some(_) => {
                        return Err(format!(
                            "{DETACHED_IDEMPOTENCY_CONFLICT}: idempotency key was already used for different detached process intent"
                        ));
                    }
                    None => {
                        return Err(format!("{DETACHED_IDEMPOTENCY_RECOVERY_PREFIX}{job_id}"));
                    }
                }
            }
        }
        enqueue_pending_request_locked(
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
            agent_instance_id,
            kind: job_kind.to_string(),
            project_id: metadata.project_id,
            session_id: metadata.session_id,
            ssh_resource: metadata.ssh_resource,
            cwd: normalized_job_cwd,
            project_cwd: metadata.project_cwd,
            purpose: metadata.purpose,
            shell: metadata.shell,
            command_preview: safe_command_preview,
            detached_idempotency_intent: detached_intent,
            status: "queued".to_string(),
            created_at,
            started_at: None,
            ended_at: None,
            terminal_observed_at: None,
            exit_code: None,
            duration_ms: None,
            stdout: Default::default(),
            stderr: Default::default(),
            error: None,
            command_execution_state: None,
            structured_execution: structured_metadata,
            codex: body.codex.clone(),
            validation_steps: validation_step_names,
            validation,
            validation_progress: None,
            last_update_seq: 0,
            visibility: metadata.visibility,

            recovery_state: None,
            recovered_after_server_restart: false,
            reconciled_at: None,
            recovery_reason_code: None,
            recovering_since: None,
            recovery_original_status: None,
            observation_epoch: self.observation_epoch.clone(),
            public_revision: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            update_notify: std::sync::Arc::new(Notify::new()),
        };
        inner.request_to_job.insert(request_id, job_id.clone());
        inner.jobs_by_id.insert(job_id.clone(), job);
        notify_client_locked(&inner, &client_id);
        Ok(job_view(
            inner.jobs_by_id.get(&job_id).expect("job just inserted"),
        ))
    }

    #[cfg(test)]
    pub(crate) async fn hidden_job_ids_for_test(&self) -> Vec<String> {
        let inner = self.inner.lock().await;
        let mut ids = inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility != ShellJobVisibility::Public)
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub(crate) async fn promote_hidden_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let job = inner
            .jobs_by_id
            .get_mut(job_id)
            .ok_or_else(|| format!("unknown shell job: {job_id}"))?;
        if job.visibility == ShellJobVisibility::CleanupPending {
            return Err(format!("structured job cleanup is pending: {job_id}"));
        }
        // A terminal update may race the sync-wait deadline. Keep terminal
        // records hidden so the initiating structured tool call returns its
        // terminal result instead of handing off an already-finished Job.
        if !is_final_job_status(&job.status) {
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

    pub(crate) async fn get_hidden_job_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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

    pub(crate) async fn hidden_job_log_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        job_id: &str,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
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
    pub(crate) fn record_hidden_cleanup_intent(
        &self,
        job_id: String,
        auth: Option<crate::auth::AuthContext>,
    ) {
        self.cleanup_intents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(job_id, auth);
    }

    pub(crate) async fn process_hidden_cleanup_intents(&self) {
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

    #[cfg(test)]
    pub(crate) fn has_hidden_cleanup_intent_for_test(&self, job_id: &str) -> bool {
        self.cleanup_intents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(job_id)
    }

    pub(crate) async fn cancel_hidden_job_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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
        if job.status == "queued" {
            if let Some(request_id) = job.request_id.as_deref() {
                remove_pending_request_locked(&mut inner, request_id);
                inner.request_to_job.remove(request_id);
            }
            inner.jobs_by_id.remove(job_id);
            return Ok(true);
        }
        if is_final_job_status(&job.status) {
            inner.jobs_by_id.remove(job_id);
            return Ok(true);
        }
        inner
            .jobs_by_id
            .get_mut(job_id)
            .expect("job exists")
            .visibility = ShellJobVisibility::CleanupPending;
        if matches!(
            job.status.as_str(),
            "agent_queued" | "running" | "stop_requested"
        ) && job.status != "stop_requested"
        {
            let stop_request_id = next_request_id();
            let request = ShellAgentShellRequest {
                request_id: stop_request_id.clone(),
                client_id: job.client_id.clone(),
                kind: "stop_job".to_string(),
                job_id: Some(job_id.to_string()),
                cwd: None,
                path: None,
                content: None,
                max_bytes: None,
                expected_sha256: None,
                expected_prefix: None,
                start_line: None,
                end_line: None,
                create_dirs: false,
                command: String::new(),
                process: None,
                script: None,
                stdin: None,
                timeout_secs: 1,
                requested_by: "tool_runtime_cleanup".to_string(),
                created_at: now_ts(),
                validation: None,
                lsp: None,
                sandbox: None,
                job_context: None,
                mcp_gateway: None,
                persistent_shell: None,
            };
            enqueue_pending_request_locked(
                &mut inner,
                &job.client_id,
                stop_request_id,
                request,
                None,
                Some(job_id.to_string()),
            )?;
            let record = inner.jobs_by_id.get_mut(job_id).expect("job exists");
            record.status = "stop_requested".to_string();
            record.error = Some("internal structured execution cleanup requested".to_string());
            notify_client_locked(&inner, &job.client_id);
        }
        Ok(false)
    }

    /// Remove a job record entirely (agent-backed).
    /// The caller must ensure the job is terminal or stopped first; removing a
    /// still-active record would orphan the runner process (its later updates
    /// then fail harmlessly as "unknown shell job"). Also drops any still
    /// pending start request so a queued-but-never-run job leaves no request
    /// behind.
    #[cfg(test)]
    pub(crate) async fn remove_job_record(&self, job_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.remove(job_id) else {
            return false;
        };
        if let Some(request_id) = job.request_id {
            inner.request_to_job.remove(&request_id);
            inner.pending_by_id.remove(&request_id);
            if let Some(queue) = inner.queues_by_client.get_mut(&job.client_id) {
                queue.retain(|id| id != &request_id);
            }
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
    pub(crate) async fn remove_projected_hidden_terminal_job_record(&self, job_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return false;
        };
        let Some(request_id) = job.request_id.clone() else {
            return false;
        };
        if job.visibility != ShellJobVisibility::HiddenUntilHandoff
            || !is_final_job_status(&job.status)
        {
            return false;
        }
        let client_id = job.client_id.clone();
        let agent_instance_id = job.agent_instance_id.clone();
        // Preserve same-instance reconnect suppression whenever the exact
        // Runner lease is still current. If that lease disappeared or was
        // replaced after the terminal snapshot was already projected, do not
        // strand the hidden terminal record forever: a replacement instance
        // cannot consume the retired lease, while a later registration after
        // the Server forgot the lease is conservatively equivalent to fresh
        // recovery and may reconstruct retained Runner terminal evidence.
        if let Some(client) = inner
            .clients
            .get_mut(&client_id)
            .filter(|client| client.agent_instance_id == agent_instance_id)
        {
            // The suppression store predates raw-shell/validation hidden
            // handoff, but its proof is only the exact
            // client/instance/job/request tuple.
            client.remember_projected_structured_terminal(
                job_id.to_string(),
                request_id.clone(),
                now_ts(),
            );
        }

        let removed = inner
            .jobs_by_id
            .remove(job_id)
            .expect("projected hidden Job was checked under the registry lock");
        inner.request_to_job.remove(&request_id);
        inner.pending_by_id.remove(&request_id);
        if let Some(queue) = inner.queues_by_client.get_mut(&removed.client_id) {
            queue.retain(|id| id != &request_id);
        }
        true
    }

    pub(crate) async fn remove_projected_hidden_structured_job_record(&self, job_id: &str) -> bool {
        self.remove_projected_hidden_terminal_job_record(job_id)
            .await
    }

    pub async fn get_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        self.get_job_for_auth(None, job_id).await
    }

    pub(crate) async fn get_job_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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

    pub async fn list_jobs(&self, limit: Option<usize>) -> Vec<ShellJobInfo> {
        self.list_jobs_for_auth(None, limit).await
    }

    pub(crate) async fn list_jobs_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        limit: Option<usize>,
    ) -> Vec<ShellJobInfo> {
        self.visible_job_records_for_auth(auth)
            .await
            .into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect()
    }

    /// Complete caller-visible Agent Job set for aggregate observability.
    /// Unlike the public list projection, this has no display pagination:
    /// runtime counts must not silently drop an older active Job behind newer
    /// records. Authorization and public-visibility filtering are identical
    /// to `list_jobs_for_auth`.
    pub(crate) async fn list_all_jobs_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Vec<ShellJobInfo> {
        self.visible_job_records_for_auth(auth)
            .await
            .into_iter()
            .map(|job| job_view(&job))
            .collect()
    }

    async fn visible_job_records_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Vec<ShellJobRecord> {
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

    /// Count active jobs for one exact runtime project without applying the
    /// display-list pagination limit. Local-only jobs do not carry an agent
    /// runtime project id and are intentionally excluded.
    pub(crate) async fn count_active_jobs_for_project(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        runtime_project_id: &str,
    ) -> usize {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        inner
            .jobs_by_id
            .values()
            .filter(|job| job.visibility == ShellJobVisibility::Public)
            .filter(|job| shell_job_visible_to_auth(auth, &inner, job))
            .filter(|job| job.project_id.as_deref() == Some(runtime_project_id))
            .filter(|job| crate::tool_runtime::ACTIVE_JOB_STATUSES.contains(&job.status.as_str()))
            .count()
    }

    /// Atomically fence new job starts and count all currently active jobs for
    /// a runtime project. The fence remains until `end_project_unregister`.
    pub(crate) async fn begin_project_unregister(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        runtime_project_id: &str,
    ) -> Result<usize, String> {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let active = inner
            .jobs_by_id
            .values()
            .filter(|job| shell_job_visible_to_auth(auth, &inner, job))
            .filter(|job| job.project_id.as_deref() == Some(runtime_project_id))
            .filter(|job| crate::tool_runtime::ACTIVE_JOB_STATUSES.contains(&job.status.as_str()))
            .count();
        if active == 0 {
            *inner
                .unregistering_projects
                .entry(runtime_project_id.to_string())
                .or_insert(0) += 1;
        }
        Ok(active)
    }

    pub(crate) async fn end_project_unregister(&self, runtime_project_id: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(count) = inner.unregistering_projects.get_mut(runtime_project_id) {
            *count -= 1;
            if *count == 0 {
                inner.unregistering_projects.remove(runtime_project_id);
            }
        }
    }

    pub async fn list_jobs_for_client(
        &self,
        client_id: &str,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ShellJobInfo>, String> {
        validate_id(client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(client_id) {
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
            .filter(|job| status.map(|status| status == job.status).unwrap_or(true))
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
    pub(crate) async fn job_log_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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
            JobLogWait,
        ),
        String,
    > {
        validate_id(job_id, "job_id")?;
        let after = after_observation_token
            .map(|value| {
                crate::job_observation::JobObservationToken::parse_bound(
                    value,
                    crate::job_observation::JobObservationExecutor::Agent,
                    job_id,
                )
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
            let revision = job.public_revision.load(Ordering::Relaxed);
            let changed = after.as_ref().is_some_and(|token| {
                token.epoch != job.observation_epoch.as_ref() || token.revision != revision
            });
            let terminal = is_final_job_status(&job.status);
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
                    terminal,
                };
                let (stdout, next_stdout_line, _, _) =
                    select_log_lines(&job.stdout, since_stdout_line, tail_lines);
                let (stderr, next_stderr_line, _, _) =
                    select_log_lines(&job.stderr, since_stderr_line, tail_lines);
                return Ok((
                    job_view(job),
                    stdout,
                    stderr,
                    next_stdout_line,
                    next_stderr_line,
                    wait,
                ));
            }

            let update_notify = job.update_notify.clone();
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
            let revision = job.public_revision.load(Ordering::Relaxed);
            let changed = after.as_ref().is_some_and(|token| {
                token.epoch != job.observation_epoch.as_ref() || token.revision != revision
            });
            let terminal = is_final_job_status(&job.status);
            if changed || terminal {
                let wait = JobLogWait {
                    wait_outcome: if terminal {
                        JobLogWaitOutcome::Terminal
                    } else {
                        JobLogWaitOutcome::Updated
                    },
                    waited_ms,
                    changed,
                    terminal,
                };
                let (stdout, next_stdout_line, _, _) =
                    select_log_lines(&job.stdout, since_stdout_line, tail_lines);
                let (stderr, next_stderr_line, _, _) =
                    select_log_lines(&job.stderr, since_stderr_line, tail_lines);
                return Ok((
                    job_view(job),
                    stdout,
                    stderr,
                    next_stdout_line,
                    next_stderr_line,
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
                let revision = job.public_revision.load(Ordering::Relaxed);
                let changed = after.as_ref().is_some_and(|token| {
                    token.epoch != job.observation_epoch.as_ref() || token.revision != revision
                });
                let terminal = is_final_job_status(&job.status);
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
                    terminal,
                };
                let (stdout, next_stdout_line, _, _) =
                    select_log_lines(&job.stdout, since_stdout_line, tail_lines);
                let (stderr, next_stderr_line, _, _) =
                    select_log_lines(&job.stderr, since_stderr_line, tail_lines);
                return Ok((
                    job_view(job),
                    stdout,
                    stderr,
                    next_stdout_line,
                    next_stderr_line,
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

    pub(crate) async fn stop_job_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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
        match job.status.as_str() {
            "queued" => {
                if let Some(request_id) = &job.request_id {
                    remove_pending_request_locked(&mut inner, request_id);
                    inner.request_to_job.remove(request_id);
                }
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                let terminal_now = now_ts();
                job.status = "stopped".to_string();
                observe_job_terminal(job, terminal_now);
                job.ended_at = Some(terminal_now);
                job.error = Some("job stopped before agent picked it up".to_string());
                if job.structured_execution.is_some() {
                    job.command_execution_state = Some(ShellCommandExecutionState::NotStarted);
                }
                notify_job_update(job);
                Ok(job_view(job))
            }
            "agent_queued" | "running" | "stop_requested" => {
                let stop_request_id = next_request_id();
                let client_id = job.client_id.clone();
                let request = ShellAgentShellRequest {
                    request_id: stop_request_id.clone(),
                    client_id: client_id.clone(),
                    kind: "stop_job".to_string(),
                    job_id: Some(job_id.to_string()),
                    cwd: None,
                    path: None,
                    content: None,
                    max_bytes: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    create_dirs: false,
                    command: String::new(),
                    process: None,
                    script: None,
                    stdin: None,
                    timeout_secs: 1,
                    requested_by,
                    created_at: now_ts(),
                    validation: None,
                    lsp: None,
                    sandbox: None,
                    job_context: None,
                    mcp_gateway: None,
                    persistent_shell: None,
                };
                enqueue_pending_request_locked(
                    &mut inner,
                    &client_id,
                    stop_request_id,
                    request,
                    None,
                    Some(job_id.to_string()),
                )?;
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stop_requested".to_string();
                job.error = Some("stop requested".to_string());
                notify_job_update(job);
                let notify_client_id = job.client_id.clone();
                notify_client_locked(&inner, &notify_client_id);
                Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists")))
            }
            "recovering" => Err(
                "runner_unavailable_recovering: wait for same-instance job reconciliation before retrying stop_job"
                    .to_string(),
            ),
            _ => Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists"))),
        }
    }

    /// Polling-transport job update entry point. Job ownership and update
    /// sequence rules decide acceptance; this path refreshes `last_seen` for
    /// the active instance. Used by the HTTP `/job_update` handler.
    pub async fn update_job(
        &self,
        body: ShellAgentJobUpdateRequest,
    ) -> Result<ShellJobInfo, String> {
        self.update_job_checked(body, None).await
    }

    /// Connection-scoped job update entry point for long-lived transports.
    /// Acceptance still follows the existing `agent_instance_id`, job
    /// ownership and `update_seq` rules so a legitimately-dispatched job's
    /// late update is not dropped just because the transport connection was
    /// replaced. A late update arriving on a stale same-instance connection,
    /// however, must not refresh the new connection's `last_seen` liveness.
    pub(crate) async fn update_job_for_connection(
        &self,
        body: ShellAgentJobUpdateRequest,
        connection_id: &str,
    ) -> Result<ShellJobInfo, String> {
        self.update_job_checked(body, Some(connection_id)).await
    }

    async fn update_job_checked(
        &self,
        body: ShellAgentJobUpdateRequest,
        expected_connection_id: Option<&str>,
    ) -> Result<ShellJobInfo, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.job_id, "job_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        if let Some(snapshot) = body.log_snapshot.as_ref() {
            validate_stream_snapshot(&snapshot.stdout, "job update stdout snapshot")?;
            validate_stream_snapshot(&snapshot.stderr, "job update stderr snapshot")?;
            if body.stdout_chunk.is_some()
                || body.stderr_chunk.is_some()
                || body.stdout_tail.is_some()
                || body.stderr_tail.is_some()
            {
                return Err(
                    "job update log_snapshot cannot be combined with chunk or legacy tail fields"
                        .to_string(),
                );
            }
        }
        let mut inner = self.inner.lock().await;
        // Reject job updates from a stale/replaced instance before refreshing
        // liveness or mutating job state.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
        let sequenced = inner
            .clients
            .get(&body.client_id)
            .is_some_and(|client| client.capabilities.job_state_reconciliation);
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
        if sequenced
            && !matches!(
                incoming_status,
                "agent_queued"
                    | "running"
                    | "stop_requested"
                    | "completed"
                    | "failed"
                    | "stopped"
                    | "timeout"
                    | "timed_out"
                    | "cancelled"
                    | "lost"
            )
        {
            return Err(format!(
                "job_state_reconciliation update status '{}' is invalid",
                incoming_status
            ));
        }
        if sequenced && body.finished != is_final_job_status(incoming_status) {
            return Err(
                "job_state_reconciliation update has inconsistent finished/status".to_string(),
            );
        }
        if sequenced
            && !is_final_job_status(incoming_status)
            && (body.exit_code.is_some() || body.duration_ms.is_some())
        {
            return Err(
                "active job_state_reconciliation update contains terminal result fields"
                    .to_string(),
            );
        }
        if sequenced && incoming_status == "completed" && body.exit_code != Some(0) {
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
                .clients
                .get(&body.client_id)
                .is_some_and(|client| client.connection_id.as_deref() == expected_connection_id)
        {
            if let Some(client) = inner.clients.get_mut(&body.client_id) {
                client.last_seen = now_ts();
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
            if job.agent_instance_id != body.agent_instance_id {
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
            if job.status == "recovering" && sequenced && body.log_snapshot.is_none() {
                return Err(
                    "recovering job update requires an authoritative log_snapshot or register inventory"
                        .to_string(),
                );
            }
            let before = public_mutation_signature(job);
            if sequenced && incoming_seq.is_some_and(|sequence| sequence <= job.last_update_seq) {
                return Ok(job_view(job));
            }
            if is_final_job_status(&job.status) {
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
            if let Err(error) = validate_validation_progress(job, &body)
                .and_then(|_| validate_command_execution_state(job, &body))
            {
                let terminal_now = now_ts();
                job.status = "failed".to_string();
                observe_job_terminal(job, terminal_now);
                job.ended_at = Some(terminal_now);
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.error = Some(format!("executor protocol violation: {}", error.0));
                if job.structured_execution.is_some() {
                    job.command_execution_state = Some(if job.started_at.is_some() {
                        ShellCommandExecutionState::OutcomeUnknown
                    } else {
                        ShellCommandExecutionState::NotStarted
                    });
                }
                request_id_to_remove = job.request_id.clone();
            } else {
                let was_recovering = job.status == "recovering";
                if let Some(snapshot) = body.log_snapshot {
                    super::jobs::replace_log_from_snapshot(&mut job.stdout, &snapshot.stdout);
                    super::jobs::replace_log_from_snapshot(&mut job.stderr, &snapshot.stderr);
                } else {
                    replace_log_limited(&mut job.stdout, body.stdout_tail);
                    replace_log_limited(&mut job.stderr, body.stderr_tail);
                    append_log_limited(&mut job.stdout, body.stdout_chunk);
                    append_log_limited(&mut job.stderr, body.stderr_chunk);
                }
                if body.validation_progress.is_some() {
                    job.validation_progress = body.validation_progress.clone();
                }
                if job.started_at.is_none()
                    && body.command_execution_state != Some(ShellCommandExecutionState::NotStarted)
                    && matches!(
                        incoming_status,
                        "running" | "completed" | "failed" | "stopped" | "timeout"
                    )
                {
                    job.started_at = Some(now_ts());
                }
                if !incoming_status.is_empty() && !is_final_job_status(&job.status) {
                    job.status = if incoming_status == "queued" && job.started_at.is_some() {
                        "agent_queued".to_string()
                    } else {
                        incoming_status.to_string()
                    };
                }
                if is_final_job_status(incoming_status) {
                    let terminal_now = now_ts();
                    job.status = incoming_status.to_string();
                    observe_job_terminal(job, terminal_now);
                    job.ended_at = Some(terminal_now);
                    job.exit_code = body.exit_code;
                    job.duration_ms = body.duration_ms;
                    job.error = body.error;
                    job.command_execution_state = body.command_execution_state;
                    job.recovery_state = job.reconciled_at.map(|_| "reconciled".to_string());
                    job.recovering_since = None;
                    job.recovery_original_status = None;
                    request_id_to_remove = job.request_id.clone();
                } else if body.error.is_some() {
                    job.error = body.error;
                }
                if body.finished && !is_final_job_status(&job.status) {
                    let terminal_now = now_ts();
                    job.status = if job.error.is_none() && job.exit_code == Some(0) {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    };
                    observe_job_terminal(job, terminal_now);
                    job.ended_at = Some(terminal_now);
                    request_id_to_remove = job.request_id.clone();
                }
                if was_recovering {
                    job.recovery_state = Some("reconciled".to_string());
                    job.reconciled_at = Some(now_ts());
                    job.recovery_reason_code =
                        Some("same_instance_update_reconciliation".to_string());
                    job.recovering_since = None;
                    job.recovery_original_status = None;
                }
            }
            if let Some(sequence) = incoming_seq {
                job.last_update_seq = sequence;
            }
            if public_mutation_signature(job) != before {
                notify_job_update(job);
            }
            remove_cleanup_terminal = job.visibility == ShellJobVisibility::CleanupPending
                && is_final_job_status(&job.status);
            job_view(job)
        };
        if let Some(request_id) = request_id_to_remove {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
        }
        if remove_cleanup_terminal {
            inner.jobs_by_id.remove(&body.job_id);
        }
        Ok(view)
    }
}
