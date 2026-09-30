//! Unix local transport, command delivery, and process-group ownership.

use super::*;

pub(super) mod launch;
mod readers;

use launch::{create_control_pipe, resolve_control_program, SpawnedChildGuard};
use readers::{spawn_control_reader, spawn_output_reader};

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct ShellProcess {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    process_group_id: u32,
    control_printf: String,
    control_pwd: String,
    expected_token: Arc<Mutex<Option<String>>>,
    control_rx: Mutex<mpsc::Receiver<ControlFrame>>,
    stdout_sync_rx: Mutex<mpsc::Receiver<String>>,
    stderr_sync_rx: Mutex<mpsc::Receiver<String>>,
    stdout: Arc<Mutex<BoundedBuffer>>,
    stderr: Arc<Mutex<BoundedBuffer>>,
    readers_stop: Arc<AtomicBool>,
    reader_threads: Mutex<Option<Vec<thread::JoinHandle<()>>>>,
    shutdown_started: AtomicBool,
}

#[cfg(unix)]
impl ShellProcess {
    fn set_expected_token(&self, token: &str) {
        *lock_unpoison(&self.expected_token) = Some(token.to_string());
    }

    fn interrupt(&self) {
        let _ = signal_process_group(self.process_group_id, libc::SIGINT);
    }

    fn stdout_buffer(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stdout
    }

    fn stderr_buffer(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stderr
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        let wrapper = command_wrapper(command, token, &self.control_printf, &self.control_pwd);
        let mut stdin = lock_unpoison(&self.stdin);
        let Some(stdin) = stdin.as_mut() else {
            return Err(ShellError::new(
                "persistent_shell_stale",
                "persistent shell stdin is closed",
            ));
        };
        stdin.write_all(wrapper.as_bytes()).map_err(|error| {
            ShellError::new(
                "persistent_shell_write_failed",
                format!("failed to write command to persistent shell: {error}"),
            )
        })?;
        stdin.flush().map_err(|error| {
            ShellError::new(
                "persistent_shell_write_failed",
                format!("failed to flush persistent shell command: {error}"),
            )
        })
    }

    fn wait_for_completion(
        &self,
        token: &str,
        timeout: Duration,
        progress: &mut CompletionProgress,
    ) -> WaitOutcome {
        let deadline = Instant::now() + timeout;
        loop {
            let control_disconnected = {
                let receiver = lock_unpoison(&self.control_rx);
                let mut disconnected = false;
                loop {
                    match receiver.try_recv() {
                        Ok(frame) if frame.token == token => progress.control = Some(frame),
                        Ok(_) => {}
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
                disconnected
            };
            let stdout_disconnected =
                drain_sync_receiver(&self.stdout_sync_rx, token, &mut progress.stdout_synced);
            let stderr_disconnected =
                drain_sync_receiver(&self.stderr_sync_rx, token, &mut progress.stderr_synced);
            if progress.stdout_synced && progress.stderr_synced {
                if let Some(frame) = progress.control.take() {
                    self.clear_expected_token(token);
                    return WaitOutcome::Frame(frame);
                }
            }
            if let Some(status) = self.try_wait() {
                return WaitOutcome::Exited(status);
            }
            if control_disconnected
                || (stdout_disconnected && !progress.stdout_synced)
                || (stderr_disconnected && !progress.stderr_synced)
            {
                if let Some(status) = self.wait_for_exit(PROCESS_SIGNAL_GRACE) {
                    return WaitOutcome::Exited(status);
                }
                return WaitOutcome::ControlLost;
            }
            if Instant::now() >= deadline {
                return WaitOutcome::TimedOut;
            }
            thread::sleep(OUTPUT_READ_SLEEP);
        }
    }

    fn clear_expected_token(&self, token: &str) {
        let mut expected = lock_unpoison(&self.expected_token);
        if expected.as_deref() == Some(token) {
            *expected = None;
        }
    }

    fn try_wait(&self) -> Option<ExitStatus> {
        lock_unpoison(&self.child).try_wait().ok().flatten()
    }

    fn wait_for_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_wait() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return;
        }
        lock_unpoison(&self.stdin).take();
        if self.try_wait().is_some() {
            self.terminate_remaining_group_after_exit();
            return;
        }
        let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
        let deadline = Instant::now() + PROCESS_SIGNAL_GRACE;
        while Instant::now() < deadline {
            if self.try_wait().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        if self.try_wait().is_none() {
            let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
            let kill_deadline = Instant::now() + PROCESS_SIGNAL_GRACE;
            while Instant::now() < kill_deadline {
                if self.try_wait().is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        self.finish_readers();
    }

    fn terminate_remaining_group_after_exit(&self) {
        if !self.reader_threads_running() {
            self.finish_readers();
            return;
        }
        let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
        let deadline = Instant::now() + PROCESS_SIGNAL_GRACE;
        while Instant::now() < deadline && self.reader_threads_running() {
            thread::sleep(Duration::from_millis(5));
        }
        // A reader still blocked after the grace period means an original
        // process-group member still owns stdout/stderr/control. The private
        // group cannot be reused while that member exists.
        if self.reader_threads_running() {
            let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
        }
        self.finish_readers();
    }

    fn reader_threads_running(&self) -> bool {
        lock_unpoison(&self.reader_threads)
            .as_ref()
            .is_some_and(|handles| handles.iter().any(|handle| !handle.is_finished()))
    }

    fn finish_readers(&self) {
        self.readers_stop.store(true, Ordering::SeqCst);
        if let Some(handles) = lock_unpoison(&self.reader_threads).take() {
            for handle in handles {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(unix)]
impl Drop for ShellProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(unix)]
impl ShellTransport for ShellProcess {
    fn set_expected_token(&self, token: &str) {
        ShellProcess::set_expected_token(self, token);
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        ShellProcess::write_command(self, command, token)
    }

    fn wait_for_completion(
        &self,
        token: &str,
        timeout: Duration,
        progress: &mut CompletionProgress,
    ) -> WaitOutcome {
        ShellProcess::wait_for_completion(self, token, timeout, progress)
    }

    fn try_wait(&self) -> Option<ExitStatus> {
        ShellProcess::try_wait(self)
    }

    fn interrupt(&self) {
        ShellProcess::interrupt(self);
    }

    fn shutdown(&self) {
        ShellProcess::shutdown(self);
    }

    fn terminate_remaining_group_after_exit(&self) {
        ShellProcess::terminate_remaining_group_after_exit(self);
    }

    fn stdout(&self) -> &Arc<Mutex<BoundedBuffer>> {
        ShellProcess::stdout_buffer(self)
    }

    fn stderr(&self) -> &Arc<Mutex<BoundedBuffer>> {
        ShellProcess::stderr_buffer(self)
    }
}

#[cfg(unix)]
pub(super) fn spawn_shell_process(launch: &ShellLaunch) -> Result<ShellProcess, ShellError> {
    let control_printf = resolve_control_program("printf")?;
    let control_pwd = resolve_control_program("pwd")?;
    let (control_reader, control_writer) = create_control_pipe()?;
    let control_write_fd = control_writer.as_raw_fd();
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&launch.initial_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(&launch.env);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid`, `dup2`, and `fcntl` are async-signal-safe. The raw
        // writer fd remains open in the parent until `spawn` returns.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                // Duplicate the control writer first because its inherited fd
                // can itself be 7 or 8 in the parent. Output sync fds share the
                // exact stdout/stderr pipes, providing a deterministic drain
                // boundary for each command.
                if libc::dup2(control_write_fd, CONTROL_FD) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::dup2(libc::STDOUT_FILENO, STDOUT_SYNC_FD) == -1
                    || libc::dup2(libc::STDERR_FILENO, STDERR_SYNC_FD) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) == -1
                    || libc::fcntl(STDOUT_SYNC_FD, libc::F_SETFD, 0) == -1
                    || libc::fcntl(STDERR_SYNC_FD, libc::F_SETFD, 0) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let child = command.spawn().map_err(|error| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            format!("failed to spawn persistent shell: {error}"),
        )
    })?;
    drop(control_writer);
    let mut child = SpawnedChildGuard::new(child);
    let process_group_id = child.process_group_id;
    let stdin = child.child_mut().stdin.take().ok_or_else(|| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            "persistent shell stdin pipe was not created",
        )
    })?;
    let stdout = child.child_mut().stdout.take().ok_or_else(|| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            "persistent shell stdout pipe was not created",
        )
    })?;
    let stderr = child.child_mut().stderr.take().ok_or_else(|| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            "persistent shell stderr pipe was not created",
        )
    })?;

    let readers_stop = Arc::new(AtomicBool::new(false));
    let stdout_buffer = Arc::new(Mutex::new(BoundedBuffer::new(launch.max_output_bytes)));
    let stderr_buffer = Arc::new(Mutex::new(BoundedBuffer::new(launch.max_output_bytes)));
    let expected_token = Arc::new(Mutex::new(None));
    let (control_tx, control_rx) = mpsc::sync_channel(CONTROL_CHANNEL_CAPACITY);
    let (stdout_sync_tx, stdout_sync_rx) = mpsc::sync_channel(OUTPUT_SYNC_CHANNEL_CAPACITY);
    let (stderr_sync_tx, stderr_sync_rx) = mpsc::sync_channel(OUTPUT_SYNC_CHANNEL_CAPACITY);
    let handles = vec![
        spawn_output_reader(
            "stdout",
            stdout,
            Arc::clone(&stdout_buffer),
            Arc::clone(&expected_token),
            stdout_sync_tx,
            STDOUT_SYNC_MAGIC,
            Arc::clone(&readers_stop),
        )?,
        spawn_output_reader(
            "stderr",
            stderr,
            Arc::clone(&stderr_buffer),
            Arc::clone(&expected_token),
            stderr_sync_tx,
            STDERR_SYNC_MAGIC,
            Arc::clone(&readers_stop),
        )?,
        spawn_control_reader(
            control_reader,
            Arc::clone(&expected_token),
            control_tx,
            Arc::clone(&readers_stop),
        )?,
    ];
    Ok(ShellProcess {
        child: Mutex::new(child.disarm()),
        stdin: Mutex::new(Some(stdin)),
        process_group_id,
        control_printf,
        control_pwd,
        expected_token,
        control_rx: Mutex::new(control_rx),
        stdout_sync_rx: Mutex::new(stdout_sync_rx),
        stderr_sync_rx: Mutex::new(stderr_sync_rx),
        stdout: stdout_buffer,
        stderr: stderr_buffer,
        readers_stop,
        reader_threads: Mutex::new(Some(handles)),
        shutdown_started: AtomicBool::new(false),
    })
}

#[cfg(unix)]
fn signal_process_group(process_group_id: u32, signal: i32) -> Result<(), ShellError> {
    let process_group_id = i32::try_from(process_group_id).map_err(|_| {
        ShellError::new(
            "persistent_shell_signal_failed",
            "persistent shell process group id is invalid",
        )
    })?;
    // SAFETY: the shell is placed in a private session/process group before
    // exec; negative pid targets that group only.
    if unsafe { libc::kill(-process_group_id, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(ShellError::new(
        "persistent_shell_signal_failed",
        format!("failed to signal persistent shell process group: {error}"),
    ))
}
