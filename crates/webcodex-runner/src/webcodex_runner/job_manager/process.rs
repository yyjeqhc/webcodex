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
            let build_started = Instant::now();
            let mut build_artifacts = None;
            let mut result = match &operation {
                RunnerJobOperation::StartBuild(request)
                    if request.provenance.backend == "rust"
                        && request.provenance.request.has_rust_options() =>
                {
                    let (result, artifacts) = crate::webcodex_runner::project_build::run_rust_build(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        &operation,
                        &stop_requested,
                        &on_started,
                    );
                    build_artifacts = artifacts;
                    result
                }
                RunnerJobOperation::StartBuild(request)
                    if request.provenance.backend == "python" =>
                {
                    crate::webcodex_runner::shell::run_python_build_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.process.args,
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                        Some(&|| {
                            crate::webcodex_runner::project_build::fence(
                                &policy,
                                &project_registry_dir,
                                &operation,
                            )
                        }),
                    )
                }
                RunnerJobOperation::StartBuild(request) => {
                    let env_overrides = if request.provenance.backend == "go" {
                        &GO_PROJECT_SINGLE_MODULE_ENV[..]
                    } else if request.provenance.backend == "node" {
                        &[
                            ("COREPACK_ENABLE_NETWORK", "0"),
                            ("COREPACK_ENABLE_DOWNLOAD_PROMPT", "0"),
                            ("COREPACK_ENV_FILE", "0"),
                            ("COREPACK_ENABLE_AUTO_PIN", "0"),
                        ][..]
                    } else {
                        &[]
                    };
                    run_process_with_profiles_and_execution_state_with_internal_env_and_start_hook(
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
                        env_overrides,
                        Some(&on_started),
                        Some(&|| {
                            crate::webcodex_runner::project_build::fence(
                                &policy,
                                &project_registry_dir,
                                &operation,
                            )
                        }),
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
            if let RunnerJobOperation::StartBuild(request) = &operation {
                if request.provenance.backend == "python"
                    && result.execution_state == ShellCommandExecutionState::Completed
                    && result.result.exit_code == Some(0)
                    && result.result.error.is_none()
                    && !stop_requested.load(Ordering::SeqCst)
                {
                    let deadline = build_started
                        + Duration::from_secs(request.timeout_secs.min(policy.max_timeout_secs));
                    match crate::webcodex_runner::project_build::observe_python_artifacts(
                        &policy,
                        &project_registry_dir,
                        &request.provenance.request,
                        Path::new(request.cwd.as_deref().expect("build cwd")),
                        deadline,
                        &stop_requested,
                    ) {
                        Ok(artifacts) => build_artifacts = Some(artifacts),
                        Err(error) => {
                            if error == "build_timeout" {
                                result.execution_state = ShellCommandExecutionState::TimedOut;
                            }
                            result.result.error = Some(error);
                        }
                    }
                }
            }
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
            // A stop can arrive after artifact observation succeeds. The wire
            // contract allows metadata only on a completed terminal report.
            if status != "completed" {
                build_artifacts = None;
            }
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: status.to_string(),
                    build_artifacts,
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

impl JobManager {
    pub(super) fn start_format_job(&self, start: PendingJobStart) {
        use webcodex_core::project_format::ProjectFormatMutationReport;
        let job_id = start.operation.job_id().to_string();
        let stop = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                drop(_lifecycle);
                self.shutdown_rejection(&start.operation);
                return;
            }
            let mut jobs = lock_unpoison(&self.jobs);
            let Some(job) = jobs.get_mut(&job_id) else {
                return;
            };
            job.slot_reserved = true;
            Arc::clone(&job.stop_requested)
        };
        let manager = self.clone_for_worker();
        let guard = self.workers.enter();
        std::thread::spawn(move || {
            let _guard = guard;
            let RunnerJobOperation::StartFormat(operation) = &start.operation else {
                unreachable!()
            };
            let started = Instant::now();
            let deadline = started
                + Duration::from_secs(operation.timeout_secs.min(start.policy.max_timeout_secs));
            let outcome = (|| {
                if Instant::now() >= deadline {
                    return Err("format_timeout");
                }
                crate::webcodex_runner::project_format::fence(
                    &start.policy,
                    &start.project_registry_dir,
                    &start.operation,
                )
                .map_err(|_| "format_plan_stale")?;
                if Instant::now() >= deadline {
                    return Err("format_timeout");
                }
                let planned = crate::webcodex_runner::project_format::replan(
                    &start.policy,
                    &start.project_registry_dir,
                    &operation.plan,
                )?;
                if Instant::now() >= deadline {
                    return Err("format_timeout");
                }
                // This is a single admitted Job, not one execution per file.
                manager.update_and_send(
                    &job_id,
                    RunnerJobDelta {
                        status: "running".into(),
                        ..Default::default()
                    },
                );
                if Instant::now() >= deadline {
                    return Err("format_timeout");
                }
                let candidates =
                    crate::webcodex_runner::project_format::format_candidates_with_deadline(
                        &start.policy,
                        &start.shell,
                        &start.project_registry_dir,
                        &planned,
                        &manager.prepared_profiles,
                        start.generation,
                        deadline,
                        Some(&stop),
                    )?;
                crate::webcodex_runner::project_format::commit_candidates(
                    &start.policy,
                    &start.project_registry_dir,
                    &planned,
                    &candidates,
                    deadline,
                    &stop,
                )
            })();
            let (status, execution, report, error) = match outcome {
                Ok(report) => (
                    "completed",
                    ShellCommandExecutionState::Completed,
                    report,
                    None,
                ),
                Err(code) => (
                    if code == "format_timeout" {
                        "timeout"
                    } else {
                        "failed"
                    },
                    if code == "format_timeout" {
                        ShellCommandExecutionState::TimedOut
                    } else {
                        ShellCommandExecutionState::OutcomeUnknown
                    },
                    ProjectFormatMutationReport::Unknown,
                    Some(code.to_string()),
                ),
            };
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: status.into(),
                    command_execution_state: Some(execution),
                    exit_code: error.is_none().then_some(0),
                    error,
                    format_mutation: Some(report),
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }
}
