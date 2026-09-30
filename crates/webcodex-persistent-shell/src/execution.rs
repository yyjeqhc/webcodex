//! Serialized command execution and timeout recovery.

use super::*;

impl PersistentShellManager {
    pub fn exec(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
        command: &str,
        timeout: Duration,
    ) -> Result<ShellExecResult, ShellError> {
        if command.contains('\0') {
            return Err(ShellError::new(
                "persistent_shell_invalid_command",
                "command cannot contain NUL bytes",
            ));
        }
        if timeout.is_zero() {
            return Err(ShellError::new(
                "persistent_shell_invalid_timeout",
                "timeout must be greater than zero",
            ));
        }
        self.sweep_idle();
        let entry = self.lookup(shell_id, workflow_session_id, runtime_project_id)?;
        self.refresh_exit(&entry);
        let state = *lock_unpoison(&entry.state);
        if state == ShellState::Opening {
            return Err(ShellError::new(
                "shell_busy",
                "persistent shell is still opening",
            ));
        }
        if state != ShellState::Running {
            return Err(ShellError::new(
                "persistent_shell_stale",
                format!("persistent shell is {}", state.as_str()),
            ));
        }
        if entry
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ShellError::new(
                "shell_busy",
                "persistent shell is already executing a command",
            ));
        }
        if *lock_unpoison(&entry.state) != ShellState::Running {
            entry.busy.store(false, Ordering::SeqCst);
            return Err(ShellError::new(
                "persistent_shell_stale",
                "persistent shell became unavailable before command dispatch",
            ));
        }
        let _busy = BusyGuard {
            entry: Arc::clone(&entry),
        };
        entry.touch();

        let stdout_start = lock_unpoison(entry.process.stdout()).cursor();
        let stderr_start = lock_unpoison(entry.process.stderr()).cursor();
        let token = command_token();
        entry.process.set_expected_token(&token);
        let mut completion = CompletionProgress::default();
        let started = Instant::now();
        if let Err(error) = entry.process.write_command(command, &token) {
            self.transition_terminal(
                &entry,
                ShellState::Poisoned,
                None,
                Some("command_write_failed".to_string()),
            );
            entry.process.shutdown();
            return Err(error);
        }

        let outcome = entry
            .process
            .wait_for_completion(&token, timeout, &mut completion);
        let mut timed_out = false;
        let resolved = match outcome {
            WaitOutcome::TimedOut => {
                timed_out = true;
                entry.process.interrupt();
                entry
                    .process
                    .wait_for_completion(&token, TIMEOUT_RECOVERY_WINDOW, &mut completion)
            }
            other => other,
        };
        if timed_out && !matches!(resolved, WaitOutcome::Frame(_) | WaitOutcome::Exited(_)) {
            self.transition_terminal(
                &entry,
                ShellState::Poisoned,
                None,
                Some("command_timeout_sync_lost".to_string()),
            );
            entry.process.shutdown();
            let (stdout, stdout_truncated) =
                lock_unpoison(entry.process.stdout()).snapshot_since(stdout_start);
            let (stderr, stderr_truncated) =
                lock_unpoison(entry.process.stderr()).snapshot_since(stderr_start);
            entry.touch();
            return Ok(ShellExecResult {
                shell_id: shell_id.to_string(),
                command_started: true,
                command_completed: false,
                exit_code: None,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
                duration_ms: started.elapsed().as_millis() as u64,
                execution_state: "timed_out".to_string(),
                shell_state: ShellState::Poisoned,
                cwd: lock_unpoison(&entry.current_cwd).clone(),
                error_code: Some("shell_reset_required".to_string()),
                error: Some(
                    "command timed out and persistent shell synchronization could not be recovered"
                        .to_string(),
                ),
            });
        }

        match &resolved {
            WaitOutcome::Exited(_) => entry.process.terminate_remaining_group_after_exit(),
            WaitOutcome::ControlLost | WaitOutcome::TimedOut => entry.process.shutdown(),
            WaitOutcome::Frame(_) => {}
        }
        let (stdout, stdout_truncated) =
            lock_unpoison(entry.process.stdout()).snapshot_since(stdout_start);
        let (stderr, stderr_truncated) =
            lock_unpoison(entry.process.stderr()).snapshot_since(stderr_start);
        let duration_ms = started.elapsed().as_millis() as u64;
        entry.touch();

        match resolved {
            WaitOutcome::Frame(frame) => {
                if !entry.process.reported_cwd_is_absolute(&frame.cwd) {
                    self.transition_terminal(
                        &entry,
                        ShellState::Poisoned,
                        None,
                        Some("command_cwd_unobservable".to_string()),
                    );
                    entry.process.shutdown();
                    return Ok(ShellExecResult {
                        shell_id: shell_id.to_string(),
                        command_started: true,
                        command_completed: true,
                        exit_code: Some(frame.status),
                        stdout,
                        stderr,
                        stdout_truncated,
                        stderr_truncated,
                        duration_ms,
                        execution_state: "lost".to_string(),
                        shell_state: ShellState::Poisoned,
                        cwd: lock_unpoison(&entry.current_cwd).clone(),
                        error_code: Some("shell_reset_required".to_string()),
                        error: Some(
                            "persistent shell cwd could not be observed safely; reopen the shell"
                                .to_string(),
                        ),
                    });
                }
                let shell_state = {
                    let mut state = lock_unpoison(&entry.state);
                    if state.is_active() {
                        *state = ShellState::Running;
                    }
                    *state
                };
                if shell_state == ShellState::Running {
                    *lock_unpoison(&entry.current_cwd) = frame.cwd;
                }
                let interrupted = shell_state != ShellState::Running;
                Ok(ShellExecResult {
                    shell_id: shell_id.to_string(),
                    command_started: true,
                    command_completed: true,
                    exit_code: Some(frame.status),
                    stdout,
                    stderr,
                    stdout_truncated,
                    stderr_truncated,
                    duration_ms,
                    execution_state: if timed_out {
                        "timed_out".to_string()
                    } else if interrupted {
                        "interrupted".to_string()
                    } else {
                        "completed".to_string()
                    },
                    shell_state,
                    cwd: lock_unpoison(&entry.current_cwd).clone(),
                    error_code: if interrupted {
                        Some("shell_reset_required".to_string())
                    } else {
                        timed_out.then(|| "command_timeout".to_string())
                    },
                    error: if interrupted {
                        Some(
                            "persistent shell was closed while the command was completing"
                                .to_string(),
                        )
                    } else {
                        timed_out.then(|| {
                            "command timed out but the persistent shell recovered synchronization"
                                .to_string()
                        })
                    },
                })
            }
            WaitOutcome::Exited(status) => {
                let code = status.code();
                self.transition_terminal(
                    &entry,
                    ShellState::Exited,
                    code,
                    Some(if timed_out {
                        "shell_exited_after_timeout".to_string()
                    } else {
                        "shell_process_exited".to_string()
                    }),
                );
                entry.process.terminate_remaining_group_after_exit();
                Ok(ShellExecResult {
                    shell_id: shell_id.to_string(),
                    command_started: true,
                    // A normal shell exit is an authoritative terminal
                    // conclusion for the only command in flight, even though
                    // the shell cannot emit the control frame afterward.
                    command_completed: !timed_out,
                    exit_code: code,
                    stdout,
                    stderr,
                    stdout_truncated,
                    stderr_truncated,
                    duration_ms,
                    execution_state: if timed_out {
                        "timed_out".to_string()
                    } else {
                        "shell_exited".to_string()
                    },
                    shell_state: ShellState::Exited,
                    cwd: lock_unpoison(&entry.current_cwd).clone(),
                    error_code: timed_out.then(|| "shell_reset_required".to_string()),
                    error: timed_out.then(|| {
                        "command timeout caused the persistent shell to exit; reopen it".to_string()
                    }),
                })
            }
            WaitOutcome::ControlLost | WaitOutcome::TimedOut => {
                self.transition_terminal(
                    &entry,
                    ShellState::Poisoned,
                    None,
                    Some("control_channel_lost".to_string()),
                );
                entry.process.shutdown();
                Ok(ShellExecResult {
                    shell_id: shell_id.to_string(),
                    command_started: true,
                    command_completed: false,
                    exit_code: None,
                    stdout,
                    stderr,
                    stdout_truncated,
                    stderr_truncated,
                    duration_ms,
                    execution_state: "lost".to_string(),
                    shell_state: ShellState::Poisoned,
                    cwd: lock_unpoison(&entry.current_cwd).clone(),
                    error_code: Some("shell_reset_required".to_string()),
                    error: Some(
                        "persistent shell control channel was lost; reopen the shell".to_string(),
                    ),
                })
            }
        }
    }
}

struct BusyGuard {
    entry: Arc<ShellEntry>,
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.entry.busy.store(false, Ordering::SeqCst);
    }
}
