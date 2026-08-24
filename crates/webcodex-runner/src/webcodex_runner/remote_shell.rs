//! Remote persistent-shell transport: one long-lived `sh`/`bash` running on a
//! Workflow Session's SSH resource.
//!
//! On Unix the transport spawns `ssh -o ControlMaster=no -S <pool path> <host>
//! <shell>` and reuses the existing [`SshConnectionPool`]'s authenticated mux
//! transport. On Windows it spawns one direct long-lived `ssh.exe <host> <shell>`
//! because OpenSSH-for-Windows has no usable Unix control socket in this runtime.
//! Both paths delegate authentication and Host-alias resolution to the Runner's
//! local OpenSSH configuration; no second credential or model-facing SSH input is
//! introduced.
//!
//! Because an SSH exec channel exposes only stdout and stderr (no extra control
//! FD), the remote shell reserves FD 7 (a dup of the channel's original stdout)
//! and FD 8 (a dup of the original stderr) at startup. The shared command
//! wrapper (`webcodex_persistent_shell::remote_command_wrapper`) then writes the
//! stdout-sync marker to FD 7, the stderr-sync marker to FD 8, and the control
//! frame (token + exit status + `pwd -P` cwd) inline on FD 8. User redirects
//! (`exec 2>&1`, etc.) cannot move those reserved targets. The manager's state
//! machine, buffers, timeout recovery, and lifecycle semantics are shared via
//! the [`ShellTransport`] trait — nothing here duplicates that.

use super::config::SshConfig;
use super::shutdown::lock_unpoison;
use super::ssh::{PreparedPersistentShellCommand, SshConnectionPool};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Child;
use std::process::{ChildStdin, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use webcodex_persistent_shell::{
    find_bytes, longest_suffix_prefix, output_sync_marker, remote_command_wrapper, BoundedBuffer,
    CompletionProgress, ControlFrame, ShellError, ShellTransport, TransportMetadata, WaitOutcome,
    CONTROL_MAGIC, STDERR_SYNC_MAGIC, STDOUT_SYNC_MAGIC,
};
#[cfg(windows)]
use webcodex_process::ManagedChild;

/// How long to wait for the ssh child to exit after signalling it during
/// shutdown/interrupt before forcing a kill. Mirrors the local shell's grace.
const REMOTE_SIGNAL_GRACE: Duration = Duration::from_millis(100);
#[cfg(windows)]
const REMOTE_READER_JOIN_GRACE: Duration = Duration::from_millis(250);
/// Poll interval for non-blocking reads and completion waits.
const REMOTE_READ_SLEEP: Duration = Duration::from_millis(5);
/// Bound on a single control-frame field, matching the local control reader.
const CONTROL_FIELD_MAX_BYTES: usize = 8 * 1024;
const OUTPUT_SYNC_CHANNEL_CAPACITY: usize = 2;
const CONTROL_CHANNEL_CAPACITY: usize = 2;

/// A remote persistent shell ready to be driven by the shared manager.
pub(crate) struct RemoteShellTransport {
    #[cfg(unix)]
    child: Mutex<Child>,
    #[cfg(windows)]
    child: Mutex<ManagedChild>,
    stdin: Mutex<Option<ChildStdin>>,
    #[cfg(unix)]
    process_group_id: u32,
    /// Named SSH resource this shell is bound to, captured at open so the
    /// shared manager can validate it against the current Runner config.
    resource_name: String,
    /// SSH config generation this shell was opened against, captured at open.
    generation: u64,
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

impl RemoteShellTransport {
    /// Spawn the long-lived remote shell. `shell_program` is `sh` or `bash`.
    /// The bootstrap written to stdin reserves FD 7/8 before any user command.
    pub(crate) fn spawn(
        pool: &SshConnectionPool,
        generation: u64,
        config: &SshConfig,
        resource_name: &str,
        session_id: &str,
        shell_program: &str,
        max_output_bytes: usize,
    ) -> Result<(Self, Option<String>), ShellError> {
        let prepared = pool
            .prepare_persistent_shell_command(
                generation,
                config,
                resource_name,
                session_id,
                shell_program,
            )
            .map_err(|message| ShellError::new("ssh_persistent_shell_spawn_failed", message))?;
        let PreparedPersistentShellCommand {
            mut command,
            default_cwd,
        } = prepared;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        let mut child = command.spawn().map_err(|error| {
            ShellError::new(
                "ssh_persistent_shell_spawn_failed",
                format!("failed to spawn remote persistent shell: {error}"),
            )
        })?;
        #[cfg(windows)]
        let mut child = ManagedChild::spawn(&mut command).map_err(|error| {
            ShellError::new(
                "ssh_persistent_shell_spawn_failed",
                format!("failed to spawn managed remote persistent shell: {error}"),
            )
        })?;
        #[cfg(unix)]
        let process_group_id = child.id();
        #[cfg(unix)]
        let child_stdio = &mut child;
        #[cfg(windows)]
        let child_stdio = child.child_mut();
        let stdin = child_stdio.stdin.take().ok_or_else(|| {
            ShellError::new(
                "ssh_persistent_shell_spawn_failed",
                "remote persistent shell stdin pipe was not created",
            )
        })?;
        let stdout = child_stdio.stdout.take().ok_or_else(|| {
            ShellError::new(
                "ssh_persistent_shell_spawn_failed",
                "remote persistent shell stdout pipe was not created",
            )
        })?;
        let stderr = child_stdio.stderr.take().ok_or_else(|| {
            ShellError::new(
                "ssh_persistent_shell_spawn_failed",
                "remote persistent shell stderr pipe was not created",
            )
        })?;

        let readers_stop = Arc::new(AtomicBool::new(false));
        let stdout_buffer = Arc::new(Mutex::new(BoundedBuffer::new(max_output_bytes)));
        let stderr_buffer = Arc::new(Mutex::new(BoundedBuffer::new(max_output_bytes)));
        let expected_token = Arc::new(Mutex::new(None));
        let (control_tx, control_rx) = mpsc::sync_channel(CONTROL_CHANNEL_CAPACITY);
        let (stdout_sync_tx, stdout_sync_rx) = mpsc::sync_channel(OUTPUT_SYNC_CHANNEL_CAPACITY);
        let (stderr_sync_tx, stderr_sync_rx) = mpsc::sync_channel(OUTPUT_SYNC_CHANNEL_CAPACITY);

        let handles = vec![
            spawn_stdout_reader(
                stdout,
                Arc::clone(&stdout_buffer),
                Arc::clone(&expected_token),
                stdout_sync_tx,
                Arc::clone(&readers_stop),
            )?,
            spawn_stderr_reader(
                stderr,
                Arc::clone(&stderr_buffer),
                Arc::clone(&expected_token),
                stderr_sync_tx,
                control_tx,
                Arc::clone(&readers_stop),
            )?,
        ];

        Ok((
            Self {
                child: Mutex::new(child),
                stdin: Mutex::new(Some(stdin)),
                #[cfg(unix)]
                process_group_id,
                resource_name: resource_name.to_string(),
                generation,
                expected_token,
                control_rx: Mutex::new(control_rx),
                stdout_sync_rx: Mutex::new(stdout_sync_rx),
                stderr_sync_rx: Mutex::new(stderr_sync_rx),
                stdout: stdout_buffer,
                stderr: stderr_buffer,
                readers_stop,
                reader_threads: Mutex::new(Some(handles)),
                shutdown_started: AtomicBool::new(false),
            },
            default_cwd,
        ))
    }

    fn set_expected_token(&self, token: &str) {
        *lock_unpoison(&self.expected_token) = Some(token.to_string());
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        let wrapper = remote_command_wrapper(command, token);
        let mut stdin = lock_unpoison(&self.stdin);
        let Some(stdin) = stdin.as_mut() else {
            return Err(ShellError::new(
                "persistent_shell_stale",
                "remote persistent shell stdin is closed",
            ));
        };
        stdin.write_all(wrapper.as_bytes()).map_err(|error| {
            ShellError::new(
                "persistent_shell_write_failed",
                format!("failed to write command to remote persistent shell: {error}"),
            )
        })?;
        stdin.flush().map_err(|error| {
            ShellError::new(
                "persistent_shell_write_failed",
                format!("failed to flush remote persistent shell command: {error}"),
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
                #[cfg(windows)]
                {
                    // Windows pipe EOF can become visible just before the direct
                    // ssh.exe exit status. Give only the remaining command budget,
                    // capped by the normal ownership grace, to classify a real
                    // process exit instead of spuriously reporting framing loss.
                    let grace = deadline
                        .saturating_duration_since(Instant::now())
                        .min(REMOTE_SIGNAL_GRACE);
                    if !grace.is_zero() {
                        if let Some(status) = self.wait_for_direct_exit(grace) {
                            return WaitOutcome::Exited(status);
                        }
                    }
                }
                // The ssh child died before reaching sync. Report it as a lost
                // control channel so the manager poisons the shell.
                return WaitOutcome::ControlLost;
            }
            if Instant::now() >= deadline {
                return WaitOutcome::TimedOut;
            }
            thread::sleep(REMOTE_READ_SLEEP);
        }
    }

    fn try_wait(&self) -> Option<ExitStatus> {
        lock_unpoison(&self.child).try_wait().ok().flatten()
    }

    #[cfg(windows)]
    fn wait_for_direct_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now().checked_add(timeout)?;
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

    #[cfg(windows)]
    fn terminate_managed_tree(&self) {
        let mut child = lock_unpoison(&self.child);
        let _ = child.terminate_tree();
        let _ = child.wait_tree_exit(REMOTE_SIGNAL_GRACE);
    }

    fn interrupt(&self) {
        #[cfg(unix)]
        {
            // SIGINT the ssh child's private process group so the remote shell and
            // any foreground command receive the interrupt. The manager decides
            // whether sync is recovered afterward.
            let _ = signal_process_group(self.process_group_id, libc::SIGINT);
        }
        #[cfg(windows)]
        {
            // Redirected Windows OpenSSH has no safe command-scoped Ctrl-C
            // primitive. Force the owned Job Object tree down; the manager's
            // recovery wait then observes exit and never reuses an uncertain stream.
            self.terminate_managed_tree();
        }
    }

    fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return;
        }
        lock_unpoison(&self.stdin).take();
        if self.try_wait().is_some() {
            self.finish_readers();
            return;
        }
        #[cfg(unix)]
        {
            let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
            let deadline = Instant::now() + REMOTE_SIGNAL_GRACE;
            while Instant::now() < deadline {
                if self.try_wait().is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            if self.try_wait().is_none() {
                let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
                let kill_deadline = Instant::now() + REMOTE_SIGNAL_GRACE;
                while Instant::now() < kill_deadline {
                    if self.try_wait().is_some() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
        #[cfg(windows)]
        self.terminate_managed_tree();
        self.finish_readers();
    }

    fn terminate_remaining_group_after_exit(&self) {
        // Once the direct ssh child exits, any locally owned descendants or pipe
        // holders must still be reclaimed before this transport is considered done.
        if !self.reader_threads_running() {
            self.finish_readers();
            return;
        }
        #[cfg(unix)]
        {
            let _ = signal_process_group(self.process_group_id, libc::SIGTERM);
            let deadline = Instant::now() + REMOTE_SIGNAL_GRACE;
            while Instant::now() < deadline && self.reader_threads_running() {
                thread::sleep(Duration::from_millis(5));
            }
            if self.reader_threads_running() {
                let _ = signal_process_group(self.process_group_id, libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        self.terminate_managed_tree();
        self.finish_readers();
    }

    fn reader_threads_running(&self) -> bool {
        lock_unpoison(&self.reader_threads)
            .as_ref()
            .is_some_and(|handles| handles.iter().any(|handle| !handle.is_finished()))
    }

    fn finish_readers(&self) {
        self.readers_stop.store(true, Ordering::SeqCst);
        let Some(mut handles) = lock_unpoison(&self.reader_threads).take() else {
            return;
        };
        #[cfg(unix)]
        for handle in handles {
            let _ = handle.join();
        }
        #[cfg(windows)]
        {
            let deadline = Instant::now() + REMOTE_READER_JOIN_GRACE;
            while Instant::now() < deadline && handles.iter().any(|handle| !handle.is_finished()) {
                thread::sleep(Duration::from_millis(5));
            }
            for handle in handles.drain(..) {
                if handle.is_finished() {
                    let _ = handle.join();
                }
            }
        }
    }
}

impl Drop for RemoteShellTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ShellTransport for RemoteShellTransport {
    fn set_expected_token(&self, token: &str) {
        RemoteShellTransport::set_expected_token(self, token);
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        RemoteShellTransport::write_command(self, command, token)
    }

    fn wait_for_completion(
        &self,
        token: &str,
        timeout: Duration,
        progress: &mut CompletionProgress,
    ) -> WaitOutcome {
        RemoteShellTransport::wait_for_completion(self, token, timeout, progress)
    }

    fn try_wait(&self) -> Option<ExitStatus> {
        RemoteShellTransport::try_wait(self)
    }

    fn interrupt(&self) {
        RemoteShellTransport::interrupt(self);
    }

    fn shutdown(&self) {
        RemoteShellTransport::shutdown(self);
    }

    fn terminate_remaining_group_after_exit(&self) {
        RemoteShellTransport::terminate_remaining_group_after_exit(self);
    }

    fn stdout(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stdout
    }

    fn stderr(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stderr
    }

    fn reported_cwd_is_absolute(&self, cwd: &Path) -> bool {
        // The remote shell is always POSIX sh/bash, even when its local ssh.exe
        // transport runs on Windows. Do not apply Windows Path::is_absolute to
        // the remote namespace (`/tmp` is not absolute under Windows rules).
        cwd.to_string_lossy().starts_with('/')
    }

    fn metadata(&self) -> Option<TransportMetadata> {
        Some(TransportMetadata {
            resource: Some(self.resource_name.clone()),
            generation: Some(self.generation),
        })
    }
}

/// The bootstrap written to the remote shell's stdin at open time, before the
/// open initialization command. It reserves FD 7/8 as dups of the channel's
/// original stdout/stderr so the shared command wrapper's markers always reach
/// the Runner regardless of later user redirects, then applies the effective
/// remote cwd when one was requested.
///
/// The `cd` runs only after FD 7/8 are reserved, and its failure is observable
/// through the initialization control frame: a non-zero `cd` makes open fail
/// instead of silently falling back to the SSH login directory. `cd --` guards
/// a leading `-`; the path is single-quoted with the shared shell-quoting
/// helper so control characters cannot break out of the quoted argument.
pub(crate) fn remote_shell_bootstrap(effective_cwd: Option<&str>) -> String {
    // `exec 7>&1` dups current stdout to FD 7; `exec 8>&2` dups current stderr
    // to FD 8. Both persist for the shell's lifetime. A user `exec 2>&1` later
    // only moves FD 2, not the reserved FD 8.
    let mut bootstrap = "exec 7>&1 8>&2\n".to_string();
    if let Some(cwd) = effective_cwd {
        bootstrap.push_str("cd -- ");
        bootstrap.push_str(&super::shell::shell_quote(cwd));
        bootstrap.push('\n');
    }
    bootstrap
}

fn drain_sync_receiver(
    receiver: &Mutex<mpsc::Receiver<String>>,
    token: &str,
    synced: &mut bool,
) -> bool {
    let receiver = lock_unpoison(receiver);
    loop {
        match receiver.try_recv() {
            Ok(received) if received == token => *synced = true,
            Ok(_) => {}
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => return true,
        }
    }
}

/// stdout reader: strip the per-command `WCPSO1` sync marker, signal sync, and
/// keep everything else as user stdout. Identical marker logic to the local
/// shell's stdout reader.
fn spawn_stdout_reader(
    mut pipe: impl Read + Send + 'static,
    buffer: Arc<Mutex<BoundedBuffer>>,
    expected_token: Arc<Mutex<Option<String>>>,
    sync_sender: mpsc::SyncSender<String>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, ShellError> {
    thread::Builder::new()
        .name("wc-remote-shell-stdout".to_string())
        .spawn(move || {
            let mut chunk = [0_u8; 8192];
            let mut pending = Vec::new();
            let mut last_synced_token: Option<String> = None;
            while !stop.load(Ordering::SeqCst) {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        pending.extend_from_slice(&chunk[..read]);
                        let expected = lock_unpoison(&expected_token).clone();
                        process_sync_pending(
                            &mut pending,
                            &buffer,
                            &expected,
                            &sync_sender,
                            STDOUT_SYNC_MAGIC,
                            &mut last_synced_token,
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(REMOTE_READ_SLEEP);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            lock_unpoison(&buffer).append(&pending);
        })
        .map_err(|error| {
            ShellError::new(
                "ssh_persistent_shell_reader_failed",
                format!("failed to start remote persistent shell stdout reader: {error}"),
            )
        })
}

/// stderr reader: the FD-8 stream carries user stderr followed by the
/// `WCPSE1` sync marker and then the inline `WCPS1` control frame. The reader
/// strips the sync marker, forwards sync, parses+strips the control frame, and
/// keeps only genuine user stderr in the buffer.
fn spawn_stderr_reader(
    mut pipe: impl Read + Send + 'static,
    buffer: Arc<Mutex<BoundedBuffer>>,
    expected_token: Arc<Mutex<Option<String>>>,
    sync_sender: mpsc::SyncSender<String>,
    control_sender: mpsc::SyncSender<ControlFrame>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, ShellError> {
    thread::Builder::new()
        .name("wc-remote-shell-stderr".to_string())
        .spawn(move || {
            let mut chunk = [0_u8; 8192];
            let mut pending = Vec::new();
            let mut last_synced_token: Option<String> = None;
            // Inline control-frame parser state. The control frame only appears
            // after the stderr sync marker for the current token, so parsing is
            // gated on having synced this token.
            let mut control_stage: u8 = 0;
            let mut control_field = Vec::new();
            let mut control_token = String::new();
            let mut control_status: i32 = 0;
            while !stop.load(Ordering::SeqCst) {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        pending.extend_from_slice(&chunk[..read]);
                        let expected = lock_unpoison(&expected_token).clone();
                        // First strip the sync marker; everything before it is
                        // user stderr, everything after is the control frame.
                        process_sync_pending(
                            &mut pending,
                            &buffer,
                            &expected,
                            &sync_sender,
                            STDERR_SYNC_MAGIC,
                            &mut last_synced_token,
                        );
                        // Then parse+strip the control frame that follows.
                        if last_synced_token.is_some() {
                            process_control_pending(
                                &mut pending,
                                &expected,
                                &mut control_stage,
                                &mut control_field,
                                &mut control_token,
                                &mut control_status,
                                &control_sender,
                            );
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(REMOTE_READ_SLEEP);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            lock_unpoison(&buffer).append(&pending);
        })
        .map_err(|error| {
            ShellError::new(
                "ssh_persistent_shell_reader_failed",
                format!("failed to start remote persistent shell stderr reader: {error}"),
            )
        })
}

/// Strip the per-command output-sync marker from the stream and emit user bytes
/// to the bounded buffer. Mirrors the local shell's `process_output_pending`:
/// the marker is removed, sync is signalled once, and a split marker (suffix
/// that could be a marker prefix) is retained rather than emitted. Bytes after
/// the marker are left in `pending` for the caller (the stderr reader parses
/// the control frame from them).
fn process_sync_pending(
    pending: &mut Vec<u8>,
    buffer: &Arc<Mutex<BoundedBuffer>>,
    expected_token: &Option<String>,
    sync_sender: &mpsc::SyncSender<String>,
    sync_magic: &[u8],
    last_synced_token: &mut Option<String>,
) {
    let Some(token) = expected_token else {
        lock_unpoison(buffer).append(pending);
        pending.clear();
        return;
    };
    if last_synced_token.as_deref() == Some(token.as_str()) {
        // Already synced this token: there is no second sync marker for this
        // command, so remaining bytes belong to the control frame (stderr) or
        // the next command's user output (stdout). Leave them for the caller.
        return;
    }
    let marker = output_sync_marker(sync_magic, token);
    if let Some(position) = find_bytes(pending, &marker) {
        lock_unpoison(buffer).append(&pending[..position]);
        pending.drain(..position + marker.len());
        let _ = sync_sender.send(token.clone());
        *last_synced_token = Some(token.clone());
        // Do NOT flush the remainder to the buffer: on stderr it is the control
        // frame; on stdout there is none after the marker. The caller handles
        // any leftover.
        return;
    }
    let retained = longest_suffix_prefix(pending, &marker);
    let emit = pending.len().saturating_sub(retained);
    if emit > 0 {
        lock_unpoison(buffer).append(&pending[..emit]);
        pending.drain(..emit);
    }
}

/// Parse the inline control frame `WCPS1\0{token}\0{status}\0{cwd}\0` from the
/// stderr stream, forwarding a `ControlFrame` and stripping every byte of the
/// frame from user stderr. Only frames whose token matches the currently
/// expected token are accepted. Bytes that are not part of a complete frame are
/// kept in `pending` for the next read (they are control-frame bytes, never
/// user stderr, because the control frame immediately follows the sync marker).
fn process_control_pending(
    pending: &mut Vec<u8>,
    expected_token: &Option<String>,
    stage: &mut u8,
    field: &mut Vec<u8>,
    control_token: &mut String,
    control_status: &mut i32,
    sender: &mpsc::SyncSender<ControlFrame>,
) {
    let expected = expected_token.clone();
    let bytes = std::mem::take(pending);
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        if byte != 0 {
            if field.len() < CONTROL_FIELD_MAX_BYTES {
                field.push(byte);
            } else {
                field.clear();
                *stage = 0;
            }
            continue;
        }
        match *stage {
            0 if field.as_slice() == CONTROL_MAGIC => {
                *stage = 1;
            }
            0 => {}
            1 => {
                let candidate = String::from_utf8_lossy(field).into_owned();
                if expected.as_deref() == Some(candidate.as_str()) {
                    *control_token = candidate;
                    *stage = 2;
                } else {
                    *stage = u8::from(field.as_slice() == CONTROL_MAGIC);
                }
            }
            2 => match String::from_utf8_lossy(field).parse::<i32>() {
                Ok(value) => {
                    *control_status = value;
                    *stage = 3;
                }
                Err(_) => *stage = 0,
            },
            3 => {
                if field.last() == Some(&b'\n') {
                    field.pop();
                }
                let cwd = PathBuf::from(String::from_utf8_lossy(field).into_owned());
                let _ = sender.try_send(ControlFrame {
                    token: std::mem::take(control_token),
                    status: *control_status,
                    cwd,
                });
                *stage = 0;
            }
            _ => *stage = 0,
        }
        field.clear();
    }
    // Keep any unprocessed tail (a partial control frame) for the next read.
    if *stage != 0 || i < bytes.len() {
        pending.extend_from_slice(&bytes[i..]);
    }
}

#[cfg(unix)]
fn signal_process_group(process_group_id: u32, signal: i32) -> Result<(), ShellError> {
    let process_group_id = i32::try_from(process_group_id).map_err(|_| {
        ShellError::new(
            "ssh_persistent_shell_signal_failed",
            "remote persistent shell process group id is invalid",
        )
    })?;
    // SAFETY: the ssh child is placed in a private session/process group before
    // exec; negative pid targets that group only.
    if unsafe { libc::kill(-process_group_id, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(ShellError::new(
        "ssh_persistent_shell_signal_failed",
        format!("failed to signal remote persistent shell process group: {error}"),
    ))
}
