//! Child spawn, stdin ownership, deadline waits, and result classification.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_configured_command(
    policy: &RunnerPolicy,
    mut cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    cmd.current_dir(cwd_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        // Never leak the Runner's own stdin into user subprocesses. Desktop
        // deliberately keeps the Runner stdin pipe open as its parent-liveness
        // lease; inheriting that handle lets grandchildren retain the lease and
        // also gives ordinary no-input commands a long-lived parent pipe instead
        // of an explicit EOF source. Structured commands with no stdin contract
        // receive a closed/null input handle instead.
        cmd.stdin(Stdio::null());
    }
    // ManagedChild owns the whole process tree: a private process group on
    // Unix, a kill-on-close Job Object on Windows. `child_mut()` below only
    // accesses pipe handles; every termination still uses the managed tree.
    let spawn = ManagedChild::spawn(&mut cmd);
    let mut child = match spawn {
        Ok(child) => child,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("{spawn_error_prefix}: {error}")),
            });
        }
    };
    // Structured execution must drain both OS pipes for the entire child
    // lifetime. Starting independent bounded readers before any lifecycle wait
    // prevents either stdout or stderr pipe capacity from throttling the child,
    // even when no caller observes Job logs until terminal completion.
    let drains = match ContinuousPipeDrain::start(&mut child, policy.max_output_bytes) {
        Ok(drains) => drains,
        Err(error) => {
            let cleanup = terminate_child_process_tree(&mut child).err();
            return ShellCommandResult::outcome_unknown(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(with_cleanup_error(error, cleanup)),
            });
        }
    };
    if let Some(on_started) = on_started {
        on_started();
    }
    let mut stdin_writer = match stdin {
        Some(input) => match child.child_mut().stdin.take() {
            Some(mut child_stdin) => {
                let input = input.as_bytes().to_vec();
                let (tx, rx) = mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let _ = tx.send(child_stdin.write_all(&input));
                });
                Some(rx)
            }
            None => {
                let cleanup = terminate_and_collect_pipes(child, drains).err();
                return ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(with_cleanup_error("stdin pipe missing", cleanup)),
                });
            }
        },
        None => None,
    };
    loop {
        if let Err(error) = poll_stdin_writer(&mut stdin_writer) {
            let cleanup = terminate_and_collect_pipes(child, drains).err();
            return ShellCommandResult::outcome_unknown(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(with_cleanup_error(
                    format!("failed to write command stdin: {error}"),
                    cleanup,
                )),
            });
        }
        if stop_requested
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            let duration_ms = start.elapsed().as_millis() as u64;
            return match terminate_and_collect_pipes(child, drains) {
                Ok((_status, stdout, stderr)) => {
                    let (stdout, stdout_truncated) =
                        stdout.normalize_with_truncation(policy.max_output_bytes);
                    let (mut stderr, mut stderr_truncated) =
                        stderr.normalize_with_truncation(policy.max_output_bytes);
                    stderr_truncated |= append_bounded_text(
                        &mut stderr,
                        "job stopped by request",
                        policy.max_output_bytes,
                    );
                    ShellCommandResult::completed(CommandResult {
                        exit_code: Some(-1),
                        stdout: Some(stdout),
                        stderr: Some(stderr),
                        duration_ms: Some(duration_ms),
                        error: Some("job stopped".to_string()),
                    })
                    .with_stream_truncation(stdout_truncated, stderr_truncated)
                }
                Err(e) => ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: Some(-1),
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(duration_ms),
                    error: Some(format!("job stopped; failed to collect output: {}", e)),
                }),
            };
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return match terminate_and_collect_pipes(child, drains) {
                        Ok((_status, stdout, stderr)) => {
                            let (stdout, stdout_truncated) =
                                stdout.normalize_with_truncation(policy.max_output_bytes);
                            let (mut stderr, mut stderr_truncated) =
                                stderr.normalize_with_truncation(policy.max_output_bytes);
                            stderr_truncated |= append_bounded_text(
                                &mut stderr,
                                &format!("command timed out after {} seconds", timeout_secs),
                                policy.max_output_bytes,
                            );
                            ShellCommandResult::timed_out(CommandResult {
                                exit_code: Some(-1),
                                stdout: Some(stdout),
                                stderr: Some(stderr),
                                duration_ms: Some(duration_ms),
                                error: Some("command timed out".to_string()),
                            })
                            .with_stream_truncation(stdout_truncated, stderr_truncated)
                        }
                        Err(e) => ShellCommandResult::outcome_unknown(CommandResult {
                            exit_code: Some(-1),
                            stdout: None,
                            stderr: None,
                            duration_ms: Some(duration_ms),
                            error: Some(format!(
                                "command timed out; failed to collect output: {}",
                                e
                            )),
                        }),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let cleanup = terminate_and_collect_pipes(child, drains).err();
                return ShellCommandResult::outcome_unknown(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: Some(with_cleanup_error(
                        format!("failed to wait command: {}", e),
                        cleanup,
                    )),
                });
            }
        }
    }
    if let Err(error) = finish_stdin_writer(stdin_writer) {
        let cleanup = terminate_and_collect_pipes(child, drains).err();
        return ShellCommandResult::outcome_unknown(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(with_cleanup_error(
                format!("failed to write command stdin: {error}"),
                cleanup,
            )),
        });
    }
    match terminate_and_collect_pipes(child, drains) {
        Ok((status, stdout, stderr)) => {
            let (stdout, stdout_truncated) =
                stdout.normalize_with_truncation(policy.max_output_bytes);
            let (stderr, stderr_truncated) =
                stderr.normalize_with_truncation(policy.max_output_bytes);
            ShellCommandResult::completed(CommandResult {
                exit_code: Some(status.code().unwrap_or(-1)),
                stdout: Some(stdout),
                stderr: Some(stderr),
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            })
            .with_stream_truncation(stdout_truncated, stderr_truncated)
        }
        Err(e) => spawned_output_failure(start, e),
    }
}

pub(super) fn poll_stdin_writer(
    receiver: &mut Option<mpsc::Receiver<std::io::Result<()>>>,
) -> Result<(), String> {
    let Some(active) = receiver.as_ref() else {
        return Ok(());
    };
    match active.try_recv() {
        Ok(Ok(())) => {
            *receiver = None;
            Ok(())
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            // The child may deliberately close stdin before exiting. Its
            // terminal status and output remain the source of truth.
            *receiver = None;
            Ok(())
        }
        Ok(Err(error)) => {
            *receiver = None;
            Err(error.to_string())
        }
        Err(mpsc::TryRecvError::Empty) => Ok(()),
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            Err("stdin writer ended without a result".to_string())
        }
    }
}

pub(super) fn finish_stdin_writer(
    receiver: Option<mpsc::Receiver<std::io::Result<()>>>,
) -> Result<(), String> {
    let Some(receiver) = receiver else {
        return Ok(());
    };
    match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err("stdin writer did not finish after process exit".to_string())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("stdin writer ended without a result".to_string())
        }
    }
}

pub(super) fn spawned_output_failure(start: Instant, error: String) -> ShellCommandResult {
    ShellCommandResult::outcome_unknown(CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: Some(error),
    })
}
