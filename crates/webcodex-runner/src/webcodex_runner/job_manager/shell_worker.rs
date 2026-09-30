//! Local Shell spawn fences, process I/O and validation-step execution.

use super::lifecycle::{
    cargo_activity_from_stderr, post_spawn_interruption_delta, post_spawn_interruption_reason,
    process_running_activity, raw_shell_job_terminal_lifecycle, validation_step_activity,
};
use super::*;

/// A fully prepared local Shell/validation plan, before any child is spawned.
pub(super) struct PreparedShellJob {
    pub(super) job_id: String,
    pub(super) operation: RunnerJobOperation,
    pub(super) policy: RunnerPolicy,
    pub(super) steps: Vec<ShellJobValidationStep>,
    pub(super) timeout_secs: u64,
    pub(super) validation: bool,
    pub(super) capture_cargo_test_count: bool,
    pub(super) step_count: usize,
    pub(super) commands: VecDeque<std::process::Command>,
    pub(super) prepared_profile: Option<Arc<crate::webcodex_runner::shell::PreparedShellProfile>>,
}

impl JobManager {
    pub(super) fn start_prepared_shell_job(&self, prepared: PreparedShellJob) {
        let PreparedShellJob {
            job_id,
            operation,
            policy,
            steps,
            timeout_secs,
            validation,
            capture_cargo_test_count,
            step_count,
            mut commands,
            prepared_profile,
        } = prepared;
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
        // Preserve the pre-start proof boundary explicitly. A stop/shutdown
        // observed here is still known to precede ManagedChild::spawn; the
        // fence below is intentionally repeated after spawn because that later
        // race can no longer claim NotStarted.
        if stop_requested.load(Ordering::SeqCst) {
            self.fail_job(&operation, "job stopped before start".to_string(), None);
            return;
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            self.shutdown_rejection(&operation);
            return;
        }
        let start = Instant::now();
        let mut command = commands.pop_front().expect("validated non-empty plan");
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(e) => {
                if validation {
                    self.fail_job(
                        &operation,
                        VALIDATION_STEP_SPAWN_FAILED_CODE.to_string(),
                        Some(ShellJobValidationProgress {
                            completed: 0,
                            current_step: None,
                            failed_step: None,
                        }),
                    );
                } else {
                    let error = prepared_profile
                        .as_ref()
                        .map(|profile_name| {
                            format!(
                                "failed to spawn shell profile '{}': {}",
                                profile_name.profile_name, e
                            )
                        })
                        .unwrap_or_else(|| format!("failed to spawn command: {}", e));
                    self.fail_job(&operation, error, None);
                }
                return;
            }
        };
        let input = if matches!(&operation, RunnerJobOperation::StartInteractiveProcess(_)) {
            let attached = child
                .child_mut()
                .stdin
                .take()
                .ok_or_else(|| std::io::Error::other("stdin pipe missing"))
                .and_then(input::InputChannel::attach);
            match attached {
                Ok(input) => Some(input),
                Err(_) => {
                    drop(child);
                    self.update_and_send(
                        &job_id,
                        post_spawn_interruption_delta(
                            &operation,
                            start.elapsed().as_millis() as u64,
                            "interactive input initialization failed",
                        ),
                    );
                    self.start_available_queued();
                    return;
                }
            }
        } else {
            None
        };
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let mut child = Arc::new(Mutex::new(child));
        let post_spawn_rejection = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut jobs = lock_unpoison(&self.jobs);
            let mut job = jobs.get_mut(&job_id);
            let rejection = post_spawn_interruption_reason(
                self.shutting_down.load(Ordering::SeqCst),
                stop_requested.load(Ordering::SeqCst),
                job.is_some(),
            );
            if rejection.is_none() {
                if let Some(job) = job.as_mut() {
                    job.child = Some(child.clone());
                    job.input = input.clone();
                }
            }
            rejection
        };
        if let Some(error) = post_spawn_rejection {
            let _ = terminate_managed_tree(&child);
            // ManagedChild::spawn succeeded before this fence. Even if the
            // termination request succeeds, the user command may already have
            // executed and we do not wait here for a trustworthy exit status.
            // Raw shell must therefore remain conservatively started/unknown;
            // never recycle the pre-start NotStarted evidence across this
            // spawn boundary.
            self.update_and_send(
                &job_id,
                post_spawn_interruption_delta(
                    &operation,
                    start.elapsed().as_millis() as u64,
                    error,
                ),
            );
            self.start_available_queued();
            return;
        }
        self.update_and_send(
            &job_id,
            RunnerJobDelta {
                status: "running".to_string(),
                validation_progress: validation.then(|| ShellJobValidationProgress {
                    completed: 0,
                    current_step: Some(steps[0].name.clone()),
                    failed_step: None,
                }),
                activity: Some(if validation {
                    validation_step_activity(&steps[0])
                } else {
                    process_running_activity()
                }),
                ..Default::default()
            },
        );
        let jobs = self.jobs.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        let shutting_down = Arc::clone(&self.shutting_down);
        let manager = self.clone_for_worker();
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
            let mut step_index = 0;
            let mut test_count_accumulator = capture_cargo_test_count
                .then(webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator::default);
            let (final_status, out, err, final_progress) = loop {
                const OUTPUT_CHANNEL_CAPACITY: usize = 64;
                let (tx, rx) = mpsc::sync_channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
                let mut readers = Vec::new();
                if let Some(stdout) = stdout {
                    readers.push(spawn_reader(
                        stdout,
                        tx.clone(),
                        true,
                        OutputTextSource::LocalProcess,
                    ));
                }
                if let Some(stderr) = stderr {
                    readers.push(spawn_reader(
                        stderr,
                        tx.clone(),
                        false,
                        OutputTextSource::LocalProcess,
                    ));
                }
                drop(tx);
                let step_status = loop {
                    let mut out = String::new();
                    let mut err = String::new();
                    while let Ok(chunk) = rx.try_recv() {
                        match chunk {
                            OutputChunk::Stdout(text) => out.push_str(&text),
                            OutputChunk::Stderr(text) => err.push_str(&text),
                        }
                    }
                    if !out.is_empty() || !err.is_empty() {
                        observe_cargo_test_count_chunks(&mut test_count_accumulator, &out, &err);
                        let activity = validation
                            .then(|| cargo_activity_from_stderr(&steps[step_index], &err))
                            .flatten();
                        manager.update_and_send(
                            &job_id,
                            RunnerJobDelta {
                                status: "running".to_string(),
                                stdout_chunk: (!out.is_empty()).then_some(out),
                                stderr_chunk: (!err.is_empty()).then_some(err),
                                validation_progress: validation.then(|| {
                                    ShellJobValidationProgress {
                                        completed: step_index,
                                        current_step: Some(steps[step_index].name.clone()),
                                        failed_step: None,
                                    }
                                }),
                                activity,
                                ..Default::default()
                            },
                        );
                    }
                    let wait_result = {
                        let mut child = lock_unpoison(&child);
                        child.try_wait()
                    };
                    match wait_result {
                        Ok(Some(status)) => {
                            let stopped = stop_requested.load(Ordering::SeqCst);
                            break (
                                if stopped {
                                    "stopped"
                                } else if status.success() {
                                    "completed"
                                } else {
                                    "failed"
                                }
                                .to_string(),
                                Some(status.code().unwrap_or(-1)),
                                if stopped {
                                    Some("job stopped by request".to_string())
                                } else {
                                    None
                                },
                            );
                        }
                        Ok(None) => {
                            if stop_requested.load(Ordering::SeqCst) {
                                let _ = terminate_managed_tree(&child);
                                break (
                                    "stopped".to_string(),
                                    Some(-1),
                                    Some("job stopped by request".to_string()),
                                );
                            }
                            if start.elapsed() >= Duration::from_secs(timeout_secs) {
                                stop_requested.store(true, Ordering::SeqCst);
                                let _ = terminate_managed_tree(&child);
                                break (
                                    "timeout".to_string(),
                                    Some(-1),
                                    Some(format!("job timed out after {} seconds", timeout_secs)),
                                );
                            }
                        }
                        Err(e) => {
                            // The host lost track of a process it started.
                            // For a validation job that must arrive as a
                            // machine-readable infrastructure code: the step
                            // did not fail, its outcome is simply unknown,
                            // and saying "check failed" would blame the
                            // project for the executor's problem.
                            eprintln!("webcodex-runner failed to wait job {job_id}: {e}");
                            break (
                                "failed".to_string(),
                                None,
                                Some(wait_failure_error(validation, &e)),
                            );
                        }
                    }
                    std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
                };
                // A direct child can exit while a background descendant keeps
                // stdout/stderr open. Give the tree a short window to exit on
                // its own, then force-terminate whatever remains before the
                // bounded reader join, so cleanup cannot wait forever on EOF.
                cleanup_managed_tree(&child);
                let mut out = String::new();
                let mut err = String::new();
                drain_and_join_reader_threads_until(
                    readers,
                    &rx,
                    &mut out,
                    &mut err,
                    Instant::now() + Duration::from_secs(1),
                );
                observe_cargo_test_count_chunks(&mut test_count_accumulator, &out, &err);
                if step_status.0 == "completed" && step_index + 1 < step_count {
                    step_index += 1;
                    if stop_requested.load(Ordering::SeqCst) {
                        break (
                            (
                                "stopped".to_string(),
                                Some(-1),
                                Some("job stopped by request".to_string()),
                            ),
                            out,
                            err,
                            validation.then_some(ShellJobValidationProgress {
                                completed: step_index,
                                current_step: None,
                                failed_step: None,
                            }),
                        );
                    }
                    {
                        let _lifecycle_guard = lock_unpoison(&lifecycle);
                        if shutting_down.load(Ordering::SeqCst)
                            || stop_requested.load(Ordering::SeqCst)
                        {
                            break (
                                (
                                    "stopped".to_string(),
                                    Some(-1),
                                    Some("job stopped by request".to_string()),
                                ),
                                out,
                                err,
                                validation.then_some(ShellJobValidationProgress {
                                    completed: step_index,
                                    current_step: None,
                                    failed_step: None,
                                }),
                            );
                        }
                    }
                    let mut next_command = commands
                        .pop_front()
                        .expect("one command per validation step");
                    let spawn = ManagedChild::spawn(&mut next_command);
                    let mut next = match spawn {
                        Ok(child) => child,
                        Err(_error) => {
                            break (
                                (
                                    "failed".to_string(),
                                    None,
                                    Some(VALIDATION_STEP_SPAWN_FAILED_CODE.to_string()),
                                ),
                                out,
                                err,
                                validation.then_some(ShellJobValidationProgress {
                                    completed: step_index,
                                    current_step: None,
                                    failed_step: None,
                                }),
                            )
                        }
                    };
                    let next_stdout = next.child_mut().stdout.take();
                    let next_stderr = next.child_mut().stderr.take();
                    let next = Arc::new(Mutex::new(next));
                    let reject_for_shutdown = {
                        let _lifecycle_guard = lock_unpoison(&lifecycle);
                        if shutting_down.load(Ordering::SeqCst)
                            || stop_requested.load(Ordering::SeqCst)
                        {
                            true
                        } else if let Some(job) = lock_unpoison(&jobs).get_mut(&job_id) {
                            job.child = Some(Arc::clone(&next));
                            false
                        } else {
                            true
                        }
                    };
                    if reject_for_shutdown {
                        let _ = terminate_managed_tree(&next);
                        break (
                            (
                                "stopped".to_string(),
                                Some(-1),
                                Some("job stopped by request".to_string()),
                            ),
                            out,
                            err,
                            validation.then_some(ShellJobValidationProgress {
                                completed: step_index,
                                current_step: None,
                                failed_step: None,
                            }),
                        );
                    }
                    child = next;
                    manager.update_and_send(
                        &job_id,
                        RunnerJobDelta {
                            status: "running".to_string(),
                            stdout_chunk: (!out.is_empty()).then_some(out),
                            stderr_chunk: (!err.is_empty()).then_some(err),
                            validation_progress: validation.then(|| ShellJobValidationProgress {
                                completed: step_index,
                                current_step: Some(steps[step_index].name.clone()),
                                failed_step: None,
                            }),
                            activity: validation
                                .then(|| validation_step_activity(&steps[step_index])),
                            ..Default::default()
                        },
                    );
                    stdout = next_stdout;
                    stderr = next_stderr;
                    continue;
                }
                let progress = validation.then(|| ShellJobValidationProgress {
                    completed: if step_status.0 == "completed" {
                        steps.len()
                    } else {
                        step_index
                    },
                    current_step: None,
                    // An infrastructure code names no failed step: the
                    // connector reads `failed_step` as "this check rejected
                    // the work", which is exactly what did not happen.
                    failed_step: validation_failed_step(
                        &step_status.0,
                        step_status.2.as_deref(),
                        &steps[step_index].name,
                    ),
                });
                break (step_status, out, err, progress);
            };
            let command_execution_state = (!validation)
                .then(|| raw_shell_job_terminal_lifecycle(&final_status.0, final_status.1));
            let test_count_evidence = finish_cargo_test_count_evidence(test_count_accumulator);
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: final_status.0,
                    stdout_chunk: (!out.is_empty()).then_some(out),
                    stderr_chunk: (!err.is_empty()).then_some(err),
                    exit_code: final_status.1,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: final_status.2,
                    command_execution_state,
                    stream_limit_bytes: None,
                    validation_progress: final_progress,
                    test_count_evidence,
                    activity: None,
                    finished: true,
                },
            );
            manager.start_available_queued();
        });
    }
}
