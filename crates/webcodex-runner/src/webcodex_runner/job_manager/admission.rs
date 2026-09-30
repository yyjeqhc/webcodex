//! Exact Job context validation, admission fences and local concurrency queues.

use super::lifecycle::{job_prestart_lifecycle, runner_job_is_active};
use super::*;
pub(super) fn validate_runner_job_context_operation(
    context: &ShellJobContext,
    operation: &RunnerJobOperation,
    client_id: &str,
) -> Result<(), String> {
    const MAX_CONTEXT_FIELD_CHARS: usize = 1_024;
    const MAX_COMMAND_PREVIEW_CHARS: usize = 121;
    let bounded =
        |value: &str, max_chars: usize| !value.contains('\0') && value.chars().count() <= max_chars;
    if !operation.is_start() {
        return Err("stop_job is not a Job start operation".to_string());
    }
    if operation.context() != Some(context) {
        return Err("job recovery context does not match the typed Job operation".to_string());
    }
    if !bounded(&context.command_preview, MAX_COMMAND_PREVIEW_CHARS)
        || context.command_preview.contains(['\r', '\n'])
    {
        return Err("job recovery context command_preview is invalid or oversized".to_string());
    }
    for (name, value) in [
        ("ssh_resource", context.ssh_resource.as_deref()),
        ("project_cwd", context.project_cwd.as_deref()),
        ("cwd", context.cwd.as_deref()),
        ("purpose", context.purpose.as_deref()),
        ("shell", context.shell.as_deref()),
    ] {
        if value.is_some_and(|value| !bounded(value, MAX_CONTEXT_FIELD_CHARS)) {
            return Err(format!(
                "job recovery context {name} is invalid or oversized"
            ));
        }
    }
    if context.cwd.as_deref() != operation.cwd() {
        return Err("job recovery context cwd does not match the execution request".to_string());
    }
    if context.ssh_resource.is_some() && context.workflow_session_id.is_none() {
        return Err("job recovery context SSH resource requires a Workflow Session".to_string());
    }
    if context.purpose.as_deref().is_some_and(|purpose| {
        !matches!(
            purpose,
            "validation"
                | "test"
                | "build"
                | "format"
                | "release"
                | "diagnostic"
                | "operation"
                | "other"
        )
    }) {
        return Err("job recovery context purpose is invalid".to_string());
    }
    if context.shell.as_deref().is_some_and(|shell| {
        !matches!(
            shell,
            "sh" | "bash"
                | "bash_login"
                | "powershell"
                | "python"
                | "javascript"
                | "typescript"
                | "configured"
                | "custom"
                | "remote"
                | "direct_argv"
        )
    }) {
        return Err("job recovery context shell is invalid".to_string());
    }

    let validation_steps = match operation {
        RunnerJobOperation::StartValidation(operation) => {
            let names = operation
                .steps
                .iter()
                .map(|step| step.name.clone())
                .collect::<Vec<_>>();
            if !(1..=3).contains(&operation.steps.len())
                || operation.steps.iter().any(|step| !step.is_canonical())
                || operation.steps.iter().enumerate().any(|(index, step)| {
                    operation.steps[..index]
                        .iter()
                        .any(|earlier| earlier.name == step.name)
                })
            {
                return Err("invalid structured validation plan".to_string());
            }
            names
        }
        _ => Vec::new(),
    };
    if context.validation_steps != validation_steps
        || context
            .validation_steps
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != context.validation_steps.len()
        || context
            .validation_steps
            .iter()
            .any(|step| !matches!(step.as_str(), "format" | "check" | "test"))
    {
        return Err("job recovery context validation_steps are invalid".to_string());
    }
    let validation_context = matches!(operation, RunnerJobOperation::StartValidation(_));
    if context.validation.as_ref().is_some_and(|metadata| {
        !validation_context
            || !metadata.is_valid()
            || metadata
                .steps
                .iter()
                .map(|step| step.name.clone())
                .collect::<Vec<_>>()
                != context.validation_steps
    }) {
        return Err("job recovery context validation metadata is invalid".to_string());
    }
    if context
        .structured_execution
        .as_ref()
        .is_some_and(|metadata| !metadata.is_valid())
    {
        return Err("job recovery context structured execution metadata is invalid".to_string());
    }

    match operation {
        RunnerJobOperation::StartShell(operation) => {
            runner_protocol::validate_raw_shell_wire_command(&operation.command)?;
        }
        RunnerJobOperation::StartBuild(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed project build Job request shape is invalid".to_string());
            }
            runner_protocol::validate_process_argv(&operation.process)?;
            validate_runner_structured_common(
                operation.cwd.as_deref(),
                None,
                operation.timeout_secs,
                runner_protocol::PROCESS_TIMEOUT_MAX_SECS,
            )?;
            if !operation.provenance.is_valid() {
                return Err("project build Job provenance is invalid".to_string());
            }
            let canonical = webcodex_core::project_build::canonical_project_build_process(
                &operation.provenance.backend,
                &operation.provenance.request,
            )
            .map_err(str::to_string)?;
            if canonical != operation.process
                || webcodex_core::project_build::project_build_invocation_digest(&operation.process)
                    != operation.provenance.invocation_digest
            {
                return Err("project build Job process does not match provenance".to_string());
            }
        }
        RunnerJobOperation::StartProcess(operation)
        | RunnerJobOperation::StartInteractiveProcess(operation)
        | RunnerJobOperation::StartDetachedProcess(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed process Job request shape is invalid".to_string());
            }
            runner_protocol::validate_process_argv(&operation.process)?;
            validate_runner_structured_common(
                operation.cwd.as_deref(),
                operation.stdin.as_deref(),
                operation.timeout_secs,
                runner_protocol::PROCESS_TIMEOUT_MAX_SECS,
            )?;
        }
        RunnerJobOperation::StartScript(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed script Job request shape is invalid".to_string());
            }
            runner_protocol::validate_script_request(
                &operation.script,
                operation.stdin.as_deref(),
                operation.cwd.as_deref(),
                operation.timeout_secs,
            )?;
        }
        RunnerJobOperation::StartSkillResource(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed Skill resource Job request shape is invalid".to_string());
            }
            operation
                .request
                .validate()
                .map_err(|error| format!("invalid Runner Skill execution request: {error}"))?;
            validate_runner_structured_common(
                operation.cwd.as_deref(),
                None,
                operation.timeout_secs,
                runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
            )?;
        }
        RunnerJobOperation::StartValidation(_) => {}
        RunnerJobOperation::Stop { .. } => unreachable!("stop rejected above"),
    }

    if context.structured_execution != operation.expected_structured_execution() {
        return Err(
            "job recovery context structured execution metadata does not match request".to_string(),
        );
    }
    if let Some(project_id) = context.runtime_project_id.as_deref() {
        let prefix = format!("agent:{client_id}:");
        if !bounded(project_id, MAX_CONTEXT_FIELD_CHARS)
            || project_id
                .strip_prefix(&prefix)
                .is_none_or(|suffix| suffix.is_empty())
        {
            return Err(
                "job recovery context runtime_project_id does not match the runner".to_string(),
            );
        }
    }
    if let Some(session_id) = context.workflow_session_id.as_deref() {
        if context.runtime_project_id.is_none()
            || !webcodex_core::workflow_session_contract::is_valid_session_id(session_id)
        {
            return Err("job recovery context workflow_session_id is invalid".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn validate_runner_job_context(
    context: &ShellJobContext,
    request: &RunnerRequest,
    client_id: &str,
) -> Result<(), String> {
    let mut request = request.clone();
    request.job_context = Some(context.clone());
    match request.decode_operation()? {
        runner_operation::RunnerOperation::Job(operation) => {
            validate_runner_job_context_operation(context, &operation, client_id)
        }
        _ => Err("request is not a Runner Job operation".to_string()),
    }
}

pub(super) fn validate_runner_structured_common(
    cwd: Option<&str>,
    stdin: Option<&str>,
    timeout_secs: u64,
    timeout_max_secs: u64,
) -> Result<(), String> {
    if let Some(stdin) = stdin {
        if stdin.len() > runner_protocol::PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {} bytes",
                runner_protocol::PROCESS_STDIN_MAX_BYTES
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > runner_protocol::PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {} bytes",
                runner_protocol::PROCESS_CWD_MAX_BYTES
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if !(runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=timeout_max_secs)
        .contains(&timeout_secs)
    {
        return Err(format!(
            "timeout_secs must be between {} and {}",
            runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
            timeout_max_secs
        ));
    }
    Ok(())
}
impl JobManager {
    pub(super) fn shutdown_rejection(&self, operation: &RunnerJobOperation) {
        self.fail_job(operation, "runner is shutting down".to_string(), None);
    }

    pub(in crate::webcodex_runner) fn enqueue(&self, sink: RunnerSink, start: PendingJobStart) {
        if !start.operation.is_start() {
            return;
        }
        let job_id = start.operation.job_id().to_string();
        let Some(context) = start.operation.context().cloned() else {
            return;
        };
        if let Err(error) =
            validate_runner_job_context_operation(&context, &start.operation, sink.client_id())
        {
            let command_execution_state = job_prestart_lifecycle(&start.operation);
            let _ = sink.send_job_update(&RunnerJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                runner_instance_id: sink.runner_instance_id().to_string(),
                job_id,
                request_id: Some(start.metadata.request_id.clone()),
                update_seq: Some(1),
                status: "failed".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state,
                validation_progress: None,
                test_count_evidence: None,
                activity: None,
                finished: true,
            });
            return;
        }
        self.install_sink(sink.clone());
        let client_id = sink.client_id().to_string();
        let runner_instance_id = sink.runner_instance_id().to_string();
        let (queue_locally, immediate_failure) = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let shutting_down = self.shutting_down.load(Ordering::SeqCst);
            let mut jobs = lock_unpoison(&self.jobs);
            if jobs.contains_key(&job_id) {
                return;
            }
            let active_count = jobs
                .values()
                .filter(|job| runner_job_is_active(&job.snapshot.status))
                .count();
            let reserved = jobs
                .values()
                .filter(|job| {
                    job.client_id == client_id
                        && job.slot_reserved
                        && runner_job_is_active(&job.snapshot.status)
                })
                .count();
            let inventory_full = active_count >= JOB_INVENTORY_MAX_ACTIVE_JOBS;
            let admission_failure = crate::webcodex_runner::validation::project::fence(
                &start.policy,
                &start.project_registry_dir,
                &start.operation,
            )
            .and_then(|_| {
                crate::webcodex_runner::project_build::fence(
                    &start.policy,
                    &start.project_registry_dir,
                    &start.operation,
                )
            })
            .err();
            let immediate_failure = if let Some(error) = admission_failure {
                Some(error)
            } else if inventory_full {
                Some(format!(
                    "runner active job inventory limit reached ({})",
                    JOB_INVENTORY_MAX_ACTIVE_JOBS
                ))
            } else if shutting_down {
                Some("runner is shutting down".to_string())
            } else {
                None
            };
            let queue_locally = immediate_failure.is_none() && reserved >= self.max_concurrent;
            let slot_reserved = immediate_failure.is_none() && !queue_locally;
            let now = chrono::Utc::now().timestamp();
            let terminal = immediate_failure.is_some();
            jobs.insert(
                job_id.clone(),
                RunningJob {
                    client_id: client_id.clone(),
                    runner_instance_id,
                    snapshot: ShellJobSnapshot {
                        job_id: job_id.clone(),
                        request_id: start.metadata.request_id.clone(),
                        status: if terminal {
                            "failed".to_string()
                        } else {
                            "agent_queued".to_string()
                        },
                        update_seq: u64::from(terminal),
                        created_at: start.metadata.created_at,
                        started_at: None,
                        ended_at: terminal.then_some(now),
                        exit_code: None,
                        duration_ms: terminal.then_some(0),
                        error: immediate_failure.clone(),
                        command_execution_state: terminal
                            .then(|| job_prestart_lifecycle(&start.operation))
                            .flatten(),
                        context,
                        stdout: ShellJobStreamSnapshot::default(),
                        stderr: ShellJobStreamSnapshot::default(),
                        validation_progress: None,
                        test_count_evidence: None,
                        activity: None,
                    },
                    child: None,
                    input: None,
                    stop_requested: Arc::new(AtomicBool::new(false)),
                    slot_reserved,
                },
            );
            drop(jobs);
            if queue_locally {
                lock_unpoison(&self.queued).push_back(start.clone());
            }
            (queue_locally, immediate_failure)
        };
        if let Some(error) = immediate_failure {
            debug_assert!(!error.is_empty());
            self.resend_snapshot(&job_id);
            self.prune_terminal_records();
            return;
        }
        self.update_and_send(
            &job_id,
            RunnerJobDelta {
                status: "agent_queued".to_string(),
                ..Default::default()
            },
        );
        if queue_locally {
            return;
        }
        self.start_now(start);
    }

    pub(super) fn start_now(&self, start: PendingJobStart) {
        if self.shutting_down.load(Ordering::SeqCst) {
            self.shutdown_rejection(&start.operation);
            return;
        }
        match &start.operation {
            RunnerJobOperation::StartInteractiveProcess(_) => self.start_interactive_process(start),
            RunnerJobOperation::StartDetachedProcess(_) => self.start_detached_process_job(start),
            RunnerJobOperation::StartBuild(_)
            | RunnerJobOperation::StartProcess(_)
            | RunnerJobOperation::StartScript(_)
            | RunnerJobOperation::StartSkillResource(_) => self.start_structured_job(start),
            RunnerJobOperation::StartShell(_) | RunnerJobOperation::StartValidation(_) => {
                self.start_shell_job(start)
            }
            RunnerJobOperation::Stop { .. } => {
                unreachable!("stop Job operation cannot enter the start queue")
            }
        }
    }

    pub(super) fn start_available_queued(&self) {
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                lock_unpoison(&self.queued).clear();
                return;
            }
            let next = {
                let _lifecycle = lock_unpoison(&self.lifecycle);
                if self.shutting_down.load(Ordering::SeqCst) {
                    lock_unpoison(&self.queued).clear();
                    return;
                }
                let mut jobs = lock_unpoison(&self.jobs);
                let mut queued = lock_unpoison(&self.queued);
                let mut selected = None;
                for (idx, queued_start) in queued.iter().enumerate() {
                    let reserved = jobs
                        .values()
                        .filter(|job| {
                            job.client_id == queued_start.metadata.client_id
                                && job.slot_reserved
                                && runner_job_is_active(&job.snapshot.status)
                        })
                        .count();
                    if reserved < self.max_concurrent {
                        selected = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = selected {
                    let job_id = queued[idx].operation.job_id();
                    if let Some(job) = jobs.get_mut(job_id) {
                        job.slot_reserved = true;
                    }
                    queued.remove(idx)
                } else {
                    None
                }
            };
            let Some(start) = next else {
                return;
            };
            self.start_now(start);
        }
    }
}
