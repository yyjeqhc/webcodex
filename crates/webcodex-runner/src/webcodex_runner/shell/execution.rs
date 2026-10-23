//! Child spawn, stdin ownership, deadline waits, and result classification.

use super::*;

/// Keep the short-lived bounded-search hot path responsive without increasing
/// polling work for ordinary commands or long-running searches.
pub(super) fn child_poll_interval(bounded_search: bool, elapsed: Duration) -> Duration {
    if bounded_search && elapsed < Duration::from_millis(100) {
        Duration::from_millis(10)
    } else {
        Duration::from_millis(50)
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_configured_command(
    policy: &RunnerPolicy,
    cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    execute_configured_command_impl(
        policy,
        cmd,
        cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        spawn_error_prefix,
        on_started,
        None,
        false,
        None,
    )
}

#[cfg(any(windows, all(test, feature = "runner-real-process-tests")))]
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_configured_command_with_prefix(
    policy: &RunnerPolicy,
    cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
    prefix: Option<super::drains::StdoutPrefixLimit>,
) -> ShellCommandResult {
    execute_configured_command_impl(
        policy,
        cmd,
        cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        spawn_error_prefix,
        on_started,
        prefix,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_configured_command_with_output(
    policy: &RunnerPolicy,
    cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
    exact_utf8: bool,
    deadline: Option<Instant>,
) -> ShellCommandResult {
    execute_configured_command_impl(
        policy,
        cmd,
        cwd_path,
        stdin,
        timeout_secs,
        stop_requested,
        start,
        spawn_error_prefix,
        on_started,
        None,
        exact_utf8,
        deadline,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_configured_command_impl(
    policy: &RunnerPolicy,
    mut cmd: Command,
    cwd_path: &Path,
    stdin: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    start: Instant,
    spawn_error_prefix: &str,
    on_started: Option<&dyn Fn()>,
    prefix: Option<super::drains::StdoutPrefixLimit>,
    exact_utf8: bool,
    deadline: Option<Instant>,
) -> ShellCommandResult {
    // Preserve fractional phase budgets and count preparation against both the
    // local policy limit and the original multi-phase operation deadline.
    let policy_deadline = start + Duration::from_secs(timeout_secs);
    let deadline = deadline.map_or(policy_deadline, |deadline| deadline.min(policy_deadline));
    if Instant::now() >= deadline {
        return ShellCommandResult::timed_out(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("execution deadline expired during preparation".into()),
        });
    }
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
    let drains =
        match ContinuousPipeDrain::start_with_prefix(&mut child, policy.max_output_bytes, prefix) {
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
    let mut stdin_broken_pipe: Option<std::io::Error> = None;
    let search_poll_start = Instant::now();
    let mut output_bounded = false;
    loop {
        match poll_stdin_writer(&mut stdin_writer, exact_utf8) {
            Ok(Some(error)) => {
                if stdin_broken_pipe.is_none() {
                    stdin_broken_pipe = Some(error);
                }
            }
            Ok(None) => {}
            Err(error) => {
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
        if prefix.is_some() && drains.stdout_limit_reached() {
            output_bounded = true;
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return match terminate_and_collect_pipes(child, drains) {
                        Ok((_status, stdout, stderr)) => {
                            let (stdout, stdout_truncated) =
                                stdout.normalize_with_truncation(policy.max_output_bytes);
                            let (mut stderr, mut stderr_truncated) =
                                stderr.normalize_with_truncation(policy.max_output_bytes);
                            stderr_truncated |= append_bounded_text(
                                &mut stderr,
                                &format!(
                                    "command timed out after {} seconds",
                                    deadline.saturating_duration_since(start).as_secs_f64()
                                ),
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
                std::thread::sleep(child_poll_interval(
                    prefix.is_some(),
                    search_poll_start.elapsed(),
                ));
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
    match finish_stdin_writer(stdin_writer, exact_utf8) {
        Ok(Some(error)) => {
            if stdin_broken_pipe.is_none() {
                stdin_broken_pipe = Some(error);
            }
        }
        Ok(None) => {}
        Err(error) => {
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
    }
    let limit_reached = Arc::clone(&drains.stdout_limit_reached);
    match terminate_and_collect_pipes(child, drains) {
        Ok((status, stdout, stderr)) => {
            if exact_utf8 {
                if let Some(error) = stdin_broken_pipe {
                    if status.code() == Some(0) {
                        return spawned_output_failure(
                            start,
                            format!("failed to write command stdin: {error}"),
                        );
                    }
                }
                let code = if stdout.raw_truncated
                    || stderr.raw_truncated
                    || stdout.bytes.len() > policy.max_output_bytes
                    || stderr.bytes.len() > policy.max_output_bytes
                {
                    Some("format_output_truncated")
                } else if std::str::from_utf8(&stdout.bytes).is_err()
                    || std::str::from_utf8(&stderr.bytes).is_err()
                {
                    Some("format_output_invalid_utf8")
                } else if status.code() == Some(0) && !stderr.bytes.is_empty() {
                    Some("format_stderr_unexpected")
                } else {
                    None
                };
                if let Some(code) = code {
                    return spawned_output_failure(start, code.into());
                }
                return ShellCommandResult::completed(CommandResult {
                    exit_code: Some(status.code().unwrap_or(-1)),
                    stdout: Some(String::from_utf8(stdout.bytes).expect("checked UTF-8")),
                    stderr: if stderr.bytes.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8(stderr.bytes).expect("checked UTF-8"))
                    },
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: None,
                });
            }
            let (stdout, stdout_truncated) =
                stdout.normalize_with_truncation(policy.max_output_bytes);
            let (stderr, stderr_truncated) =
                stderr.normalize_with_truncation(policy.max_output_bytes);
            // A search prefix limit is the native equivalent of successful
            // head/SIGPIPE termination. Cleanup failure still fails closed.
            let output_bounded =
                output_bounded || prefix.is_some() && limit_reached.load(Ordering::SeqCst);
            // rg can complete with a genuine search error while the drain is
            // reaching its cap. Do not turn that known failure into success.
            let output_bounded = output_bounded && status.code() != Some(2);
            ShellCommandResult::completed(CommandResult {
                exit_code: Some(if output_bounded {
                    0
                } else {
                    status.code().unwrap_or(-1)
                }),
                stdout: Some(stdout),
                stderr: Some(if output_bounded {
                    String::new()
                } else {
                    stderr
                }),
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
    exact_utf8: bool,
) -> Result<Option<std::io::Error>, String> {
    let Some(active) = receiver.as_ref() else {
        return Ok(None);
    };
    match active.try_recv() {
        Ok(Ok(())) => {
            *receiver = None;
            Ok(None)
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            *receiver = None;
            if exact_utf8 {
                Ok(Some(error))
            } else {
                // The child may deliberately close stdin before exiting. Its
                // terminal status and output remain the source of truth.
                Ok(None)
            }
        }
        Ok(Err(error)) => {
            *receiver = None;
            Err(error.to_string())
        }
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            Err("stdin writer ended without a result".to_string())
        }
    }
}

pub(super) fn finish_stdin_writer(
    receiver: Option<mpsc::Receiver<std::io::Result<()>>>,
    exact_utf8: bool,
) -> Result<Option<std::io::Error>, String> {
    let Some(receiver) = receiver else {
        return Ok(None);
    };
    match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(Ok(())) => Ok(None),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            if exact_utf8 {
                Ok(Some(error))
            } else {
                Ok(None)
            }
        }
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
