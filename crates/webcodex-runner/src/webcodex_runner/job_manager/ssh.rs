//! Named SSH Job preparation and bounded uncertain-delivery execution.

use super::lifecycle::{
    post_spawn_interruption_delta, post_spawn_interruption_reason, raw_shell_job_terminal_lifecycle,
};
use super::retained::append_bounded_tail;
use super::*;
impl JobManager {
    /// Start one remote SSH command as a normal runner job. Resource/session
    /// validation and local command preparation happen before spawn, so those
    /// failures are unambiguously command-not-started. Once `ssh` is running,
    /// never retry because remote delivery may already have happened.
    pub(super) fn start_ssh_shell_job(
        &self,
        generation: u64,
        policy: RunnerPolicy,
        ssh: SshConfig,
        operation: RunnerJobOperation,
    ) {
        let request = match &operation {
            RunnerJobOperation::StartShell(request) => request,
            RunnerJobOperation::StartValidation(_) => {
                self.fail_job(
                    &operation,
                    "ssh_resource_unsupported_for_request: SSH resources do not support structured validation jobs; command was not started".to_string(),
                    None,
                );
                return;
            }
            _ => unreachable!("SSH Job starter received non shell/validation operation"),
        };
        let job_id = request.job_id.clone();
        let Some(resource_name) = request.context.ssh_resource.as_deref() else {
            return;
        };
        let Some(session_id) = request.context.workflow_session_id.as_deref() else {
            self.fail_job(
                &operation,
                "ssh_session_required: an SSH resource requires a Workflow Session id; command was not started".to_string(),
                None,
            );
            return;
        };
        let prepared = match self.ssh_pool.prepare_job_command(
            generation,
            &ssh,
            resource_name,
            session_id,
            request.cwd.as_deref(),
            &request.command,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.fail_job(&operation, error, None);
                return;
            }
        };
        let transport = prepared.transport.clone();
        let program_delivery = prepared.program_delivery;
        let mut command = prepared.command;
        if program_delivery.requires_stdin() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

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
        let start = Instant::now();
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(error) => {
                self.fail_job(
                    &operation,
                    format!(
                        "ssh_command_spawn_failed: could not start local ssh client: {error}; command was not started"
                    ),
                    None,
                );
                return;
            }
        };
        let mut child_stdin = child.child_mut().stdin.take();
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let child = Arc::new(Mutex::new(child));
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
                    job.child = Some(Arc::clone(&child));
                }
            }
            rejection
        };
        if let Some(error) = post_spawn_rejection {
            let _ = terminate_managed_tree(&child);
            // ManagedChild::spawn has already resumed ssh.exe. A successful
            // local tree termination cannot prove that the remote command was
            // never dispatched, so do not recycle pre-start NotStarted here.
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
                ..Default::default()
            },
        );
        let manager = self.clone_for_worker();
        let ssh_pool = self.ssh_pool.clone();
        let output_limit_bytes = policy.max_output_bytes;
        let worker_guard = self.workers.enter();
        let timeout_secs = request.timeout_secs.min(policy.max_timeout_secs).max(1);
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            const OUTPUT_CHANNEL_CAPACITY: usize = 64;
            let (tx, rx) = mpsc::sync_channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
            let mut readers = Vec::new();
            if let Some(stdout) = stdout.take() {
                readers.push(spawn_reader(
                    stdout,
                    tx.clone(),
                    true,
                    OutputTextSource::RemoteSsh,
                ));
            }
            if let Some(stderr) = stderr.take() {
                readers.push(spawn_reader(
                    stderr,
                    tx.clone(),
                    false,
                    OutputTextSource::RemoteSsh,
                ));
            }
            drop(tx);
            // Readers must already be draining before program/caller stdin can
            // block. The writer is tracked and polled by this same Job worker.
            let mut writer_start_error = None;
            let mut stdin_writer = match program_delivery.spawn_writer(child_stdin.take(), None) {
                Ok(writer) => writer,
                Err(error) => {
                    writer_start_error = Some(error);
                    let _ = terminate_managed_tree(&child);
                    None
                }
            };
            let mut transport_stderr = String::new();
            let (mut status, mut exit_code, mut error, interrupted_after_dispatch) = loop {
                let mut out = String::new();
                let mut err = String::new();
                while let Ok(chunk) = rx.try_recv() {
                    match chunk {
                        OutputChunk::Stdout(text) => out.push_str(&text),
                        OutputChunk::Stderr(text) => err.push_str(&text),
                    }
                }
                if !err.is_empty() {
                    append_bounded_tail(&mut transport_stderr, &err, 16 * 1024);
                }
                if !out.is_empty() || !err.is_empty() {
                    manager.update_and_send(
                        &job_id,
                        RunnerJobDelta {
                            status: "running".to_string(),
                            stdout_chunk: (!out.is_empty()).then_some(out),
                            stderr_chunk: (!err.is_empty()).then_some(err),
                            stream_limit_bytes: Some(output_limit_bytes),
                            ..Default::default()
                        },
                    );
                }
                if let Some(writer_error) = writer_start_error.take().or_else(|| {
                    stdin_writer
                        .as_mut()
                        .and_then(|writer| writer.poll_failure())
                }) {
                    let _ = terminate_managed_tree(&child);
                    break ("failed".to_string(), None, Some(writer_error), true);
                }
                let wait_result = {
                    let mut child = lock_unpoison(&child);
                    child.try_wait()
                };
                match wait_result {
                    Ok(Some(status)) => {
                        if stop_requested.load(Ordering::SeqCst) {
                            break (
                                "failed".to_string(),
                                None,
                                Some(
                                    "ssh_command_stopped_after_dispatch: local SSH tree was terminated, remote command outcome is unknown; do not blindly retry"
                                        .to_string(),
                                ),
                                true,
                            );
                        }
                        if status.success() {
                            break ("completed".to_string(), Some(0), None, false);
                        }
                        break ("failed".to_string(), status.code(), None, false);
                    }
                    Ok(None) => {
                        if stop_requested.load(Ordering::SeqCst) {
                            let _ = terminate_managed_tree(&child);
                            break (
                                "failed".to_string(),
                                None,
                                Some(
                                    "ssh_command_stopped_after_dispatch: local SSH tree was terminated, remote command outcome is unknown; do not blindly retry"
                                        .to_string(),
                                ),
                                true,
                            );
                        }
                        if start.elapsed() >= Duration::from_secs(timeout_secs) {
                            stop_requested.store(true, Ordering::SeqCst);
                            let _ = terminate_managed_tree(&child);
                            break (
                                "timeout".to_string(),
                                Some(-1),
                                Some(format!("job timed out after {timeout_secs} seconds")),
                                false,
                            );
                        }
                    }
                    Err(wait_error) => {
                        break (
                            "failed".to_string(),
                            None,
                            Some(format!(
                                "ssh_command_wait_failed: command may have started and was not retried: {wait_error}"
                            )),
                            false,
                        );
                    }
                }
                std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
            };
            // The SSH client is the root of a private process tree. Ensure a
            // background child holding either pipe cannot delay the terminal
            // update indefinitely.
            cleanup_managed_tree(&child);
            let tree_cleanup_uncertain = managed_tree_running(&child);
            let writer_finish_error = stdin_writer.as_mut().and_then(|writer| {
                let interrupted =
                    interrupted_after_dispatch || status == "timeout" || tree_cleanup_uncertain;
                let result = if interrupted {
                    writer.finish_after_tree_cleanup()
                } else {
                    writer.finish_bounded()
                };
                result.err()
            });
            let mut final_out = String::new();
            let mut final_err = String::new();
            drain_and_join_reader_threads_until(
                readers,
                &rx,
                &mut final_out,
                &mut final_err,
                Instant::now() + Duration::from_secs(1),
            );
            if !final_err.is_empty() {
                append_bounded_tail(&mut transport_stderr, &final_err, 16 * 1024);
            }
            let mut command_execution_state = if interrupted_after_dispatch {
                ShellCommandExecutionState::OutcomeUnknown
            } else {
                raw_shell_job_terminal_lifecycle(&status, exit_code)
            };
            if let Some(writer_error) = writer_finish_error {
                status = "failed".to_string();
                exit_code = None;
                error = Some(writer_error);
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
            }
            if tree_cleanup_uncertain {
                status = "failed".to_string();
                error = Some(
                    "ssh_command_cleanup_failed: local SSH process tree exit could not be proven; command may have started and was not retried"
                        .to_string(),
                );
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
            }
            if matches!(status.as_str(), "completed" | "failed")
                && matches!(
                    command_execution_state,
                    ShellCommandExecutionState::Completed
                )
                && is_transport_failure(&transport, exit_code, Some(&transport_stderr))
            {
                ssh_pool.invalidate_after_transport_failure(&transport);
                status = "failed".to_string();
                error = Some(
                    "ssh_transport_failed: command may have started and was not retried"
                        .to_string(),
                );
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
                if !final_err.is_empty() && !final_err.ends_with('\n') {
                    final_err.push('\n');
                }
                final_err.push_str(
                    "webcodex: SSH transport ended after dispatch; the command may have started and was not retried\n",
                );
            }
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status,
                    stdout_chunk: (!final_out.is_empty()).then_some(final_out),
                    stderr_chunk: (!final_err.is_empty()).then_some(final_err),
                    exit_code,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error,
                    command_execution_state: Some(command_execution_state),
                    stream_limit_bytes: Some(output_limit_bytes),
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }
}
