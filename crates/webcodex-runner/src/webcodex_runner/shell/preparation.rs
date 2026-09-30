//! Bounded subprocess probes and profile-preparation I/O.

use super::*;

pub(super) fn stderr_tail(bytes: &[u8]) -> String {
    const MAX_ERR: usize = 4096;
    normalize_output_text(bytes, false, MAX_ERR, OutputTextSource::LocalProcess)
}

pub(super) struct ProfilePreparePipeReader {
    pub(super) stream_name: &'static str,
    pub(super) result_rx: mpsc::Receiver<Result<Vec<u8>, String>>,
    pub(super) handle: std::thread::JoinHandle<()>,
}

impl ProfilePreparePipeReader {
    pub(super) fn finish_until(self, deadline: Instant) -> Result<Vec<u8>, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self.result_rx.recv_timeout(remaining) {
            Ok(result) => {
                join_profile_prepare_reader_until(self.handle, deadline, self.stream_name)?;
                result
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "profile prepare {} reader did not finish before the cleanup deadline",
                self.stream_name
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                join_profile_prepare_reader_until(self.handle, deadline, self.stream_name)?;
                Err(format!(
                    "profile prepare {} reader exited without a result",
                    self.stream_name
                ))
            }
        }
    }
}

pub(super) fn join_profile_prepare_reader_until(
    handle: std::thread::JoinHandle<()>,
    deadline: Instant,
    stream_name: &'static str,
) -> Result<(), String> {
    while !handle.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "profile prepare {stream_name} reader did not join before the cleanup deadline"
            ));
        }
        std::thread::sleep(Duration::from_millis(5).min(remaining));
    }
    handle
        .join()
        .map_err(|_| format!("profile prepare {stream_name} reader panicked"))
}

pub(super) fn spawn_profile_prepare_pipe_reader(
    stream_name: &'static str,
    mut pipe: impl Read + Send + 'static,
) -> ProfilePreparePipeReader {
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = pipe
            .read_to_end(&mut buf)
            .map(|_| buf)
            .map_err(|e| format!("failed to read profile prepare {stream_name}: {e}"));
        let _ = result_tx.send(result);
    });
    ProfilePreparePipeReader {
        stream_name,
        result_rx,
        handle,
    }
}

pub(super) fn collect_profile_prepare_output(
    stdout: ProfilePreparePipeReader,
    stderr: ProfilePreparePipeReader,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let deadline = Instant::now() + PROFILE_PREPARE_PIPE_DRAIN_TIMEOUT;
    let stdout = stdout.finish_until(deadline)?;
    let stderr = stderr.finish_until(deadline)?;
    Ok((stdout, stderr))
}

pub(super) fn run_prepare_command(
    mut cmd: Command,
    timeout: Duration,
    stop_requested: Option<&AtomicBool>,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    if stop_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("profile prepare stopped during runner shutdown".to_string());
    }
    // ManagedChild owns the whole profile-prepare process tree: a private
    // process group on Unix, a kill-on-close Job Object on Windows.
    let mut child = ManagedChild::spawn(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))
        .map_err(|e| format!("failed to spawn profile prepare command: {}", e))?;
    let stdout = match child.child_mut().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let cleanup = terminate_child_without_output(child).err();
            return Err(with_cleanup_error(
                "profile prepare stdout pipe missing",
                cleanup,
            ));
        }
    };
    let stderr = match child.child_mut().stderr.take() {
        Some(stderr) => stderr,
        None => {
            drop(stdout);
            let cleanup = terminate_child_without_output(child).err();
            return Err(with_cleanup_error(
                "profile prepare stderr pipe missing",
                cleanup,
            ));
        }
    };
    let stdout_reader = spawn_profile_prepare_pipe_reader("stdout", stdout);
    let stderr_reader = spawn_profile_prepare_pipe_reader("stderr", stderr);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if stop_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    let cleanup = terminate_child_process_tree(&mut child).err();
                    let output = collect_profile_prepare_output(stdout_reader, stderr_reader).err();
                    return Err(with_cleanup_error(
                        output.map_or_else(
                            || "profile prepare stopped during runner shutdown".to_string(),
                            |error| {
                                format!(
                                    "profile prepare stopped during runner shutdown; failed to collect output: {error}"
                                )
                            },
                        ),
                        cleanup,
                    ));
                }
                if start.elapsed() >= timeout {
                    let cleanup = terminate_child_process_tree(&mut child).err();
                    return match collect_profile_prepare_output(stdout_reader, stderr_reader) {
                        Ok((_stdout, stderr)) => Err(format!(
                            "profile prepare timed out after {} seconds; stderr tail: {}{}",
                            timeout.as_secs(),
                            stderr_tail(&stderr),
                            cleanup
                                .as_deref()
                                .map(|error| format!("; cleanup failed: {error}"))
                                .unwrap_or_default(),
                        )),
                        Err(error) => Err(with_cleanup_error(
                            format!(
                                "profile prepare timed out after {} seconds; failed to collect output: {}",
                                timeout.as_secs(),
                                error
                            ),
                            cleanup,
                        )),
                    };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let cleanup = terminate_child_process_tree(&mut child).err();
                let output = collect_profile_prepare_output(stdout_reader, stderr_reader).err();
                let base = match output {
                    Some(error) => {
                        format!("failed to wait profile prepare command: {}; failed to collect output: {}", e, error)
                    }
                    None => format!("failed to wait profile prepare command: {}", e),
                };
                return Err(with_cleanup_error(base, cleanup));
            }
        }
    };
    // The direct child has already exited, but its managed process tree can
    // still contain background descendants that inherited these pipe handles.
    // Terminate the whole tree before waiting on the readers so they see EOF
    // promptly.
    let cleanup = terminate_child_process_tree(&mut child).err();
    let output = collect_profile_prepare_output(stdout_reader, stderr_reader);
    match (cleanup, output) {
        (None, Ok((stdout, stderr))) => Ok((status, stdout, stderr)),
        (Some(cleanup), Ok(_)) => Err(format!(
            "failed to clean up profile prepare command process group: {cleanup}"
        )),
        (None, Err(error)) => Err(format!("failed to collect profile prepare output: {error}")),
        (Some(cleanup), Err(error)) => Err(format!(
            "failed to clean up profile prepare command process group: {cleanup}; failed to collect output: {error}"
        )),
    }
}
