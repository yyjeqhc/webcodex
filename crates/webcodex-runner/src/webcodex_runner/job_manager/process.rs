//! Structured native process, build, script and Skill Job execution.

use super::lifecycle::process_running_activity;
use super::*;
impl JobManager {
    pub(super) fn start_structured_job(&self, start: PendingJobStart) {
        let PendingJobStart {
            generation,
            policy,
            shell,
            skills,
            client_id,
            server_url,
            project_registry_dir,
            operation,
            ..
        } = start;
        let job_id = operation.job_id().to_string();
        if !matches!(
            operation,
            RunnerJobOperation::StartBuild(_)
                | RunnerJobOperation::StartProcess(_)
                | RunnerJobOperation::StartScript(_)
                | RunnerJobOperation::StartSkillResource(_)
        ) {
            unreachable!("structured Job starter received non structured operation");
        }
        let stop_requested = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                None
            } else {
                let mut jobs = lock_unpoison(&self.jobs);
                let Some(job) = jobs.get_mut(&job_id) else {
                    return;
                };
                job.slot_reserved = true;
                Some(Arc::clone(&job.stop_requested))
            }
        };
        let Some(stop_requested) = stop_requested else {
            self.shutdown_rejection(&operation);
            return;
        };
        let manager = self.clone_for_worker();
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            // An admitted build can wait in the local queue while its recipe,
            // lockfile or registered root changes. Recheck on the worker before
            // entering the native process path, not only when enqueueing it.
            if let Err(error) = crate::webcodex_runner::project_build::fence(
                &policy,
                &project_registry_dir,
                &operation,
            ) {
                manager.fail_job(&operation, error, None);
                return;
            }
            let started_manager = manager.clone_for_worker();
            let started_job_id = job_id.clone();
            let on_started = || {
                started_manager.update_and_send(
                    &started_job_id,
                    RunnerJobDelta {
                        status: "running".to_string(),
                        activity: Some(process_running_activity()),
                        ..Default::default()
                    },
                );
            };
            let result = match &operation {
                RunnerJobOperation::StartBuild(request) => {
                    run_process_with_profiles_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.process.executable,
                        &request.process.args,
                        None,
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                RunnerJobOperation::StartProcess(request) => {
                    run_process_with_profiles_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.process.executable,
                        &request.process.args,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                RunnerJobOperation::StartScript(request) => {
                    run_script_with_profiles_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.script,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                RunnerJobOperation::StartSkillResource(request) => {
                    run_skill_resource_with_profiles_and_execution_state(
                        generation,
                        &skills,
                        &client_id,
                        &server_url,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.request,
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                _ => unreachable!("structured Job starter received non structured operation"),
            };
            let execution_state = result.execution_state;
            let stopped = stop_requested.load(Ordering::SeqCst)
                && execution_state == ShellCommandExecutionState::Completed;
            let status = match execution_state {
                ShellCommandExecutionState::NotStarted => "failed",
                ShellCommandExecutionState::OutcomeUnknown => "lost",
                ShellCommandExecutionState::TimedOut => "timeout",
                ShellCommandExecutionState::Completed if stopped => "stopped",
                ShellCommandExecutionState::Completed
                    if result.result.exit_code == Some(0) && result.result.error.is_none() =>
                {
                    "completed"
                }
                ShellCommandExecutionState::Completed => "failed",
            };
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: status.to_string(),
                    stdout_chunk: result.result.stdout,
                    stderr_chunk: result.result.stderr,
                    exit_code: result.result.exit_code,
                    duration_ms: result.result.duration_ms,
                    error: result.result.error,
                    command_execution_state: Some(execution_state),
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }
}
