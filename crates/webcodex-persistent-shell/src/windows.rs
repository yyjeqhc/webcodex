use super::*;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use tempfile::TempDir;
use webcodex_process::ManagedChild;

const POWERSHELL_UTF8_PREAMBLE: &str = concat!(
    "try { $OutputEncoding = [Console]::InputEncoding = ",
    "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch { }",
);
const READER_JOIN_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug)]
struct WindowsShellProcess {
    child: Mutex<ManagedChild>,
    stdin: Mutex<Option<ChildStdin>>,
    control_dir: TempDir,
    expected_token: Arc<Mutex<Option<String>>>,
    stdout_sync_rx: Mutex<mpsc::Receiver<String>>,
    stderr_sync_rx: Mutex<mpsc::Receiver<String>>,
    stdout: Arc<Mutex<BoundedBuffer>>,
    stderr: Arc<Mutex<BoundedBuffer>>,
    readers_stop: Arc<AtomicBool>,
    reader_threads: Mutex<Option<Vec<thread::JoinHandle<()>>>>,
    shutdown_started: AtomicBool,
}

impl WindowsShellProcess {
    fn control_path(&self, token: &str) -> PathBuf {
        self.control_dir.path().join(format!("{token}.frame"))
    }

    fn control_temp_path(&self, token: &str) -> PathBuf {
        self.control_dir.path().join(format!("{token}.frame.tmp"))
    }

    fn clear_expected_token(&self, token: &str) {
        let mut expected = lock_unpoison(&self.expected_token);
        if expected.as_deref() == Some(token) {
            *expected = None;
        }
    }

    fn parse_control_file(&self, token: &str) -> Result<Option<ControlFrame>, ()> {
        let path = self.control_path(token);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(()),
        };
        if bytes.len() > CONTROL_FIELD_MAX_BYTES {
            let _ = fs::remove_file(path);
            return Err(());
        }
        if bytes.last() != Some(&0) {
            return Ok(None);
        }
        let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
        if fields.len() != 5
            || fields[0] != CONTROL_MAGIC
            || fields[1] != token.as_bytes()
            || !fields[4].is_empty()
        {
            let _ = fs::remove_file(path);
            return Err(());
        }
        let status = std::str::from_utf8(fields[2])
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .ok_or(())?;
        let cwd = std::str::from_utf8(fields[3]).map_err(|_| ())?;
        if cwd.is_empty() {
            let _ = fs::remove_file(path);
            return Err(());
        }
        let _ = fs::remove_file(path);
        Ok(Some(ControlFrame {
            token: token.to_string(),
            status,
            cwd: PathBuf::from(cwd),
        }))
    }

    fn finish_readers(&self) {
        self.readers_stop.store(true, Ordering::SeqCst);
        let Some(mut handles) = lock_unpoison(&self.reader_threads).take() else {
            return;
        };
        let deadline = Instant::now() + READER_JOIN_GRACE;
        while Instant::now() < deadline && handles.iter().any(|handle| !handle.is_finished()) {
            thread::sleep(Duration::from_millis(5));
        }
        for handle in handles.drain(..) {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }

    fn terminate_tree(&self) {
        let mut child = lock_unpoison(&self.child);
        let _ = child.terminate_tree();
        let _ = child.wait_tree_exit(PROCESS_SIGNAL_GRACE);
    }

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
}

impl ShellTransport for WindowsShellProcess {
    fn set_expected_token(&self, token: &str) {
        *lock_unpoison(&self.expected_token) = Some(token.to_string());
        let _ = fs::remove_file(self.control_path(token));
        let _ = fs::remove_file(self.control_temp_path(token));
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        let frame = powershell_command_frame(command, token, &self.control_path(token));
        let mut stdin = lock_unpoison(&self.stdin);
        let Some(stdin) = stdin.as_mut() else {
            return Err(ShellError::new(
                "persistent_shell_stale",
                "persistent shell stdin is closed",
            ));
        };
        stdin.write_all(frame.as_bytes()).map_err(|error| {
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
            let stdout_disconnected =
                drain_sync_receiver(&self.stdout_sync_rx, token, &mut progress.stdout_synced);
            let stderr_disconnected =
                drain_sync_receiver(&self.stderr_sync_rx, token, &mut progress.stderr_synced);
            if progress.control.is_none() {
                match self.parse_control_file(token) {
                    Ok(Some(frame)) => progress.control = Some(frame),
                    Ok(None) => {}
                    Err(()) => return WaitOutcome::ControlLost,
                }
            }
            if progress.stdout_synced && progress.stderr_synced {
                if let Some(frame) = progress.control.take() {
                    self.clear_expected_token(token);
                    return WaitOutcome::Frame(frame);
                }
            }
            if let Some(status) = self.try_wait() {
                return WaitOutcome::Exited(status);
            }
            if (stdout_disconnected && !progress.stdout_synced)
                || (stderr_disconnected && !progress.stderr_synced)
            {
                // On Windows the reader can observe pipe EOF a few scheduler
                // ticks before `Child::try_wait` reports the direct PowerShell
                // exit. Give only the remaining command budget (capped at the
                // normal process-signal grace) to distinguish an authoritative
                // shell exit from a live process whose framing was actually lost.
                let remaining = deadline.saturating_duration_since(Instant::now());
                let grace = remaining.min(PROCESS_SIGNAL_GRACE);
                if let Some(status) = self.wait_for_direct_exit(grace) {
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

    fn try_wait(&self) -> Option<ExitStatus> {
        lock_unpoison(&self.child).try_wait().ok().flatten()
    }

    fn interrupt(&self) {
        // A redirected, non-ConPTY PowerShell has no safe command-scoped Ctrl-C
        // primitive. The shared manager waits a bounded recovery window; if the
        // frame does not arrive, it poisons the shell and `shutdown` kills the
        // complete Job Object tree instead of reusing an uncertain stream.
    }

    fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return;
        }
        lock_unpoison(&self.stdin).take();
        self.terminate_tree();
        self.finish_readers();
        if let Some(token) = lock_unpoison(&self.expected_token).take() {
            let _ = fs::remove_file(self.control_path(&token));
            let _ = fs::remove_file(self.control_temp_path(&token));
        }
    }

    fn terminate_remaining_group_after_exit(&self) {
        self.terminate_tree();
        self.finish_readers();
    }

    fn stdout(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stdout
    }

    fn stderr(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stderr
    }
}

impl Drop for WindowsShellProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(super) fn spawn_shell_process(
    launch: &ShellLaunch,
) -> Result<Box<dyn ShellTransport>, ShellError> {
    let control_dir = tempfile::Builder::new()
        .prefix("webcodex-persistent-shell-")
        .tempdir()
        .map_err(|error| {
            ShellError::new(
                "persistent_shell_spawn_failed",
                format!("failed to create persistent shell control directory: {error}"),
            )
        })?;
    let bootstrap_suffix = Uuid::new_v4().simple().to_string();
    let bootstrap = powershell_bootstrap_script(&bootstrap_suffix);
    let bootstrap_path = control_dir.path().join("bootstrap.ps1");
    fs::write(&bootstrap_path, bootstrap.as_bytes()).map_err(|error| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            format!("failed to write persistent shell bootstrap: {error}"),
        )
    })?;
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .arg("-File")
        .arg(&bootstrap_path)
        .current_dir(&launch.initial_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(&launch.env);
    let mut child = ManagedChild::spawn(&mut command).map_err(|error| {
        ShellError::new(
            "persistent_shell_spawn_failed",
            format!("failed to spawn managed PowerShell process: {error}"),
        )
    })?;
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
    ];
    Ok(Box::new(WindowsShellProcess {
        child: Mutex::new(child),
        stdin: Mutex::new(Some(stdin)),
        control_dir,
        expected_token,
        stdout_sync_rx: Mutex::new(stdout_sync_rx),
        stderr_sync_rx: Mutex::new(stderr_sync_rx),
        stdout: stdout_buffer,
        stderr: stderr_buffer,
        readers_stop,
        reader_threads: Mutex::new(Some(handles)),
        shutdown_started: AtomicBool::new(false),
    }))
}

fn spawn_output_reader(
    name: &'static str,
    mut pipe: impl Read + Send + 'static,
    buffer: Arc<Mutex<BoundedBuffer>>,
    expected_token: Arc<Mutex<Option<String>>>,
    sync_sender: mpsc::SyncSender<String>,
    sync_magic: &'static [u8],
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, ShellError> {
    thread::Builder::new()
        .name(format!("wc-persistent-shell-win-{name}"))
        .spawn(move || {
            let mut chunk = [0_u8; 8192];
            let mut pending = Vec::new();
            let mut last_synced_token: Option<String> = None;
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        pending.extend_from_slice(&chunk[..read]);
                        process_output_pending(
                            &mut pending,
                            &buffer,
                            &expected_token,
                            &sync_sender,
                            sync_magic,
                            &mut last_synced_token,
                        );
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            lock_unpoison(&buffer).append(&pending);
        })
        .map_err(|error| {
            ShellError::new(
                "persistent_shell_reader_failed",
                format!("failed to start persistent shell {name} reader: {error}"),
            )
        })
}

fn powershell_command_frame(command: &str, token: &str, control_path: &Path) -> String {
    let command_b64 = BASE64.encode(command.as_bytes());
    let control_path_b64 = BASE64.encode(control_path.to_string_lossy().as_bytes());
    format!("{token}\t{command_b64}\t{control_path_b64}\r\n")
}

fn powershell_bootstrap_script(suffix: &str) -> String {
    // The bootstrap is trusted transport code evaluated once from a private
    // temporary `-File`. This avoids Windows PowerShell 5.1's `-EncodedCommand`
    // CLIXML stderr serialization, whose buffered tail can otherwise overtake
    // our explicit stderr drain marker. User command text still never enters
    // the PowerShell parser through stdin: each command arrives as one ASCII
    // Base64 frame read by an explicit UTF-8 StreamReader and is dot-sourced
    // only after decoding. The loop itself runs in one script scope, so
    // cwd/env/variables/functions persist across frames.
    format!(
        "{POWERSHELL_UTF8_PREAMBLE}\n\
         $__wc_input_{suffix} = [IO.StreamReader]::new([Console]::OpenStandardInput(), [Text.UTF8Encoding]::new($false), $false, 4096, $true)\n\
         while (($__wc_line_{suffix} = $__wc_input_{suffix}.ReadLine()) -ne $null) {{\n\
         $__wc_parts_{suffix} = $__wc_line_{suffix}.Split([char]9)\n\
         if ($__wc_parts_{suffix}.Length -ne 3) {{ [Environment]::Exit(125) }}\n\
         $__wc_token_{suffix} = $__wc_parts_{suffix}[0]\n\
         if ($__wc_token_{suffix}.Length -ne 32 -or $__wc_token_{suffix} -notmatch '^[0-9a-f]+$') {{ [Environment]::Exit(125) }}\n\
         $__wc_source_{suffix} = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($__wc_parts_{suffix}[1]))\n\
         $__wc_script_{suffix} = [ScriptBlock]::Create($__wc_source_{suffix} + [Environment]::NewLine + '$__wc_ok_{suffix} = $?; $__wc_native_{suffix} = $LASTEXITCODE')\n\
         $__wc_control_path_{suffix} = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($__wc_parts_{suffix}[2]))\n\
         $__wc_status_{suffix} = 0\n\
         $__wc_ok_{suffix} = $true\n\
         $__wc_native_{suffix} = 0\n\
         $LASTEXITCODE = 0\n\
         try {{\n\
         . $__wc_script_{suffix}\n\
         if (-not $__wc_ok_{suffix}) {{ if ($__wc_native_{suffix}) {{ $__wc_status_{suffix} = [int]$__wc_native_{suffix} }} else {{ $__wc_status_{suffix} = 1 }} }}\n\
         }} catch {{\n\
         $__wc_status_{suffix} = 1\n\
         [Console]::Error.WriteLine($_.ToString())\n\
         }}\n\
         $__wc_stdout_{suffix} = [Console]::OpenStandardOutput()\n\
         $__wc_stdout_bytes_{suffix} = [Text.Encoding]::ASCII.GetBytes('WCPSO1' + [char]0 + $__wc_token_{suffix} + [char]0)\n\
         $__wc_stdout_{suffix}.Write($__wc_stdout_bytes_{suffix}, 0, $__wc_stdout_bytes_{suffix}.Length); $__wc_stdout_{suffix}.Flush()\n\
         $__wc_stderr_{suffix} = [Console]::OpenStandardError()\n\
         $__wc_stderr_bytes_{suffix} = [Text.Encoding]::ASCII.GetBytes('WCPSE1' + [char]0 + $__wc_token_{suffix} + [char]0)\n\
         $__wc_stderr_{suffix}.Write($__wc_stderr_bytes_{suffix}, 0, $__wc_stderr_bytes_{suffix}.Length); $__wc_stderr_{suffix}.Flush()\n\
         $__wc_cwd_{suffix} = $ExecutionContext.SessionState.Path.CurrentLocation.ProviderPath\n\
         $__wc_control_text_{suffix} = 'WCPS1' + [char]0 + $__wc_token_{suffix} + [char]0 + [string]$__wc_status_{suffix} + [char]0 + $__wc_cwd_{suffix} + [char]0\n\
         $__wc_control_tmp_{suffix} = $__wc_control_path_{suffix} + '.tmp'\n\
         [IO.File]::WriteAllBytes($__wc_control_tmp_{suffix}, [Text.Encoding]::UTF8.GetBytes($__wc_control_text_{suffix}))\n\
         [IO.File]::Move($__wc_control_tmp_{suffix}, $__wc_control_path_{suffix})\n\
         }}"
    )
}
