//! Native unix operations; durable ownership remains in the shared model/store.
use super::super::*;
use std::io::{Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;

pub(in crate::webcodex_runner::detached_job) fn ensure_private_dir(
    path: &Path,
) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create detached Job state root {}: {error}",
            path.display()
        )
    })?;
    reject_symlink_or_non_dir(path, "detached Job state root")?;
    set_private_dir_permissions(path)
}

pub(in crate::webcodex_runner::detached_job) fn set_private_dir_permissions(
    path: &Path,
) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "failed to set private detached Job directory permissions on {}: {error}",
            path.display()
        )
    })
}

pub(in crate::webcodex_runner::detached_job) fn atomic_write_json<T: Serialize>(
    path: &Path,
    value: &T,
    max_bytes: usize,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode detached Job state: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("detached Job state exceeds {max_bytes} bytes"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "detached Job state path has no parent".to_string())?;
    reject_symlink_or_non_dir(parent, "detached Job state parent")?;
    // One fixed temp name per already-bounded Job directory keeps crash debris
    // bounded to at most one additional state-sized file per Job. A stale temp
    // is removed without following symlinks, then recreated exclusively.
    let temp = parent.join(STATE_TEMP_FILE);
    match fs::symlink_metadata(&temp) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "detached Job temp state must be a regular non-symlink file".to_string()
                );
            }
            fs::remove_file(&temp).map_err(|error| {
                format!("failed to remove stale detached Job temp state: {error}")
            })?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect detached Job temp state: {error}"
            ));
        }
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)
            .map_err(|error| format!("failed to create detached Job temp state: {error}"))?;
        validate_open_regular_file(&file, &temp, "detached Job temp state")?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write detached Job temp state: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync detached Job temp state: {error}"))?;
        fs::rename(&temp, path)
            .map_err(|error| format!("failed to commit detached Job state: {error}"))?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(in crate::webcodex_runner::detached_job) fn sync_directory(path: &Path) -> Result<(), String> {
    let directory = File::open(path)
        .map_err(|error| format!("failed to open detached Job state directory: {error}"))?;
    directory
        .sync_all()
        .map_err(|error| format!("failed to sync detached Job state directory: {error}"))
}

pub(in crate::webcodex_runner::detached_job) fn validate_open_regular_file(
    file: &File,
    path: &Path,
    label: &str,
) -> Result<(), String> {
    let open_metadata = file
        .metadata()
        .map_err(|error| format!("failed to inspect open {label}: {error}"))?;
    if !open_metadata.is_file() {
        return Err(format!("{label} must be a regular file"));
    }
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to revalidate {label} {}: {error}", path.display()))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != open_metadata.dev()
        || path_metadata.ino() != open_metadata.ino()
    {
        return Err(format!(
            "{label} path changed or is not a regular non-symlink file"
        ));
    }
    Ok(())
}

pub(in crate::webcodex_runner::detached_job) struct FileLock {
    pub(in crate::webcodex_runner::detached_job) file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // SAFETY: file is a live descriptor owned by this guard.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(in crate::webcodex_runner::detached_job) fn exclusive_lock(
    path: &Path,
    blocking: bool,
) -> Result<FileLock, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(|error| {
        format!(
            "failed to open detached Job lock {}: {error}",
            path.display()
        )
    })?;
    validate_open_regular_file(&file, path, "detached Job lock")?;
    let operation = libc::LOCK_EX | if blocking { 0 } else { libc::LOCK_NB };
    // SAFETY: flock only observes the valid descriptor and integer operation.
    if unsafe { libc::flock(file.as_raw_fd(), operation) } != 0 {
        return Err(format!(
            "failed to acquire detached Job lock {}: {}",
            path.display(),
            io::Error::last_os_error()
        ));
    }
    // Revalidate the pathname after acquiring the lock so a same-user path
    // replacement race cannot make this guard protect a stale inode.
    validate_open_regular_file(&file, path, "detached Job lock")?;
    Ok(FileLock { file })
}

pub(in crate::webcodex_runner::detached_job) fn write_lock_identity(
    lock: &mut FileLock,
    creation_id: &str,
) -> Result<(), String> {
    lock.file
        .set_len(0)
        .map_err(|error| format!("failed to reset detached lifetime lock: {error}"))?;
    lock.file
        .write_all(creation_id.as_bytes())
        .map_err(|error| format!("failed to write detached lifetime identity: {error}"))?;
    lock.file
        .sync_all()
        .map_err(|error| format!("failed to sync detached lifetime identity: {error}"))
}

pub(in crate::webcodex_runner::detached_job) fn lifetime_lock_is_held(
    path: &Path,
    expected_creation_id: &str,
) -> Result<bool, String> {
    validate_identity("lifetime creation_id", expected_creation_id, 96)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to open detached lifetime lock {}: {error}",
                path.display()
            )
        })?;
    validate_open_regular_file(&file, path, "detached lifetime lock")?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("failed to inspect detached lifetime lock: {error}"))?;
    if metadata.len() > 96 {
        return Err("detached lifetime lock identity is oversized".to_string());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("failed to seek detached lifetime lock: {error}"))?;
    let mut identity = String::new();
    Read::by_ref(&mut file)
        .take(97)
        .read_to_string(&mut identity)
        .map_err(|error| format!("failed to read detached lifetime lock identity: {error}"))?;
    if identity != expected_creation_id {
        return Err("detached lifetime lock creation identity mismatch".to_string());
    }
    // SAFETY: flock observes the valid descriptor. EWOULDBLOCK/EAGAIN proves an
    // existing exclusive holder of this exact validated lock inode.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_UN);
        }
        return Ok(false);
    }
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
    {
        Ok(true)
    } else {
        Err(format!("failed to probe detached lifetime lock: {error}"))
    }
}

pub(in crate::webcodex_runner::detached_job) fn make_new_session(command: &mut Command) {
    // SAFETY: the closure only invokes async-signal-safe setsid(2) and builds an
    // io::Error from errno on failure. This path is internal and always execs a
    // known Runner image, never arbitrary shell text.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

pub(in crate::webcodex_runner::detached_job) fn run_internal_platform_mode(
    args: &[String],
) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some(DETACHED_INTERNAL_SUPERVISOR) if args.len() == 4 => {
            run_supervisor(Path::new(&args[1]), &args[2], &args[3], &mut io::stdin())
        }
        Some(DETACHED_INTERNAL_WATCHDOG) if args.len() == 4 => {
            run_watchdog(Path::new(&args[1]), &args[2], &args[3])
        }
        _ => Err("malformed detached internal mode arguments".to_string()),
    }
}

pub(in crate::webcodex_runner::detached_job) fn run_accepted_payload(
    store: &DetachedJobStore,
    accepted: &DetachedJobRecord,
    launch: DetachedLaunchSpec,
) -> Result<DetachedJobRecord, String> {
    let job_dir = store.job_dir(&accepted.job_id);
    let tree_birth = format!("birth_{}", Uuid::new_v4().simple());
    let mut watchdog = spawn_watchdog(&job_dir, &tree_birth, &accepted.execution_id)?;
    let tree_pid = watchdog.id();
    let mut watchdog_acks = watchdog
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| "detached watchdog ack pipe is unavailable".to_string())?;
    let armed = read_line_with_timeout(&mut watchdog_acks, DETACHED_HANDOFF_TIMEOUT)?;
    if armed.trim() != WATCHDOG_ARMED {
        return Err("detached watchdog did not arm for the process tree".to_string());
    }

    let mut payload_command = Command::new(&launch.process.executable);
    payload_command.args(&launch.process.args).env_clear();
    for (key, value) in &launch.env {
        payload_command.env(key, value);
    }
    if let Some(cwd) = launch.cwd.as_deref() {
        payload_command.current_dir(cwd);
    }
    payload_command
        .stdin(if launch.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    payload_command.process_group(tree_pid as i32);
    let mut payload = payload_command
        .spawn()
        .map_err(|error| format!("failed to spawn detached payload: {error}"))?;
    let payload_started = unix_ms();
    let tree_identity = DetachedProcessIdentity {
        pid: tree_pid,
        creation_id: tree_birth,
        native_start_id: native_process_start_identity(tree_pid)?,
        started_at_unix_ms: payload_started,
    };
    let running = store.update(&accepted.job_id, &accepted.execution_id, |record| {
        if record.phase != DetachedJobPhase::OwnershipAccepted {
            return Err("detached payload cannot start from current durable state".to_string());
        }
        record.phase = DetachedJobPhase::Running;
        record.payload_started_at_unix_ms = Some(payload_started);
        record.tree_leader = Some(tree_identity.clone());
        Ok(())
    })?;
    // Only acknowledge after both durable ownership acceptance and Running.
    // The ACK is advisory once that boundary is committed: a Runner/owner may
    // disappear before reading it, and that lost response must not stop or
    // respawn the already-owned payload.
    let _ = write_supervisor_handshake(&[HANDSHAKE_ACCEPTED]);

    let stdin_thread = launch.stdin.map(|stdin| {
        let mut child_stdin = payload.stdin.take();
        std::thread::spawn(move || {
            if let Some(mut child_stdin) = child_stdin.take() {
                let _ = child_stdin.write_all(stdin.as_bytes());
            }
        })
    });
    let stdout = payload
        .stdout
        .take()
        .ok_or_else(|| "detached payload stdout pipe is unavailable".to_string())?;
    let stderr = payload
        .stderr
        .take()
        .ok_or_else(|| "detached payload stderr pipe is unavailable".to_string())?;
    let (output_tx, output_rx) = mpsc::sync_channel(DETACHED_OUTPUT_CHANNEL_CAPACITY);
    let stdout_thread = spawn_output_reader(stdout, output_tx.clone(), true);
    let stderr_thread = spawn_output_reader(stderr, output_tx, false);
    let mut stdout_decoder = OutputTextDecoder::new(OutputTextSource::LocalProcess);
    let mut stderr_decoder = OutputTextDecoder::new(OutputTextSource::LocalProcess);
    let started = Instant::now();
    let mut state = running;
    let mut last_checkpoint = Instant::now();
    let mut last_control_poll = Instant::now();
    let mut output_dirty = false;
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut direct_status: Option<ExitStatus> = None;
    let mut forced_status: Option<(&'static str, String)> = None;

    loop {
        match output_rx.recv_timeout(DETACHED_PROCESS_POLL_INTERVAL) {
            Ok(OutputEvent::Stdout(bytes)) => {
                let text = stdout_decoder.push(&bytes, false);
                append_output_tail(&mut state.stdout, bytes.len(), &text);
                output_dirty = true;
            }
            Ok(OutputEvent::Stderr(bytes)) => {
                let text = stderr_decoder.push(&bytes, false);
                append_output_tail(&mut state.stderr, bytes.len(), &text);
                output_dirty = true;
            }
            Ok(OutputEvent::StdoutEof) => {
                let text = stdout_decoder.push(&[], true);
                append_output_tail(&mut state.stdout, 0, &text);
                stdout_eof = true;
                output_dirty |= !text.is_empty();
            }
            Ok(OutputEvent::StderrEof) => {
                let text = stderr_decoder.push(&[], true);
                append_output_tail(&mut state.stderr, 0, &text);
                stderr_eof = true;
                output_dirty |= !text.is_empty();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stdout_eof = true;
                stderr_eof = true;
            }
        }

        if output_dirty && last_checkpoint.elapsed() >= DETACHED_CHECKPOINT_INTERVAL {
            let stdout_snapshot = state.stdout.clone();
            let stderr_snapshot = state.stderr.clone();
            state = store.update(&state.job_id, &state.execution_id, |record| {
                if record.phase != DetachedJobPhase::Running {
                    return Err("detached output checkpoint found non-running state".to_string());
                }
                record.stdout = stdout_snapshot;
                record.stderr = stderr_snapshot;
                Ok(())
            })?;
            output_dirty = false;
            last_checkpoint = Instant::now();
        }

        if direct_status.is_none()
            && forced_status.is_none()
            && last_control_poll.elapsed() >= DETACHED_CONTROL_POLL_INTERVAL
        {
            let control = store.read(&state.job_id)?;
            if control.execution_id != state.execution_id {
                return Err("detached control state execution identity changed".to_string());
            }
            if control.stop_requested {
                forced_status = Some(("stopped", "job stopped by request".to_string()));
            }
            last_control_poll = Instant::now();
        }

        if direct_status.is_none() {
            match payload.try_wait() {
                Ok(Some(status)) => direct_status = Some(status),
                Ok(None) => {}
                Err(error) => {
                    forced_status = Some((
                        "failed",
                        format!("failed to wait for detached payload: {error}"),
                    ));
                }
            }
        }
        if forced_status.is_none()
            && direct_status.is_none()
            && started.elapsed().as_secs() >= launch.timeout_secs
        {
            forced_status = Some((
                "timeout",
                format!(
                    "detached payload timed out after {} seconds",
                    launch.timeout_secs
                ),
            ));
        }
        if forced_status.is_none() {
            if let Ok(Some(_)) = watchdog.try_wait() {
                forced_status = Some((
                    "failed",
                    "detached process-tree watchdog exited unexpectedly".to_string(),
                ));
            }
        }
        if direct_status.is_some() || forced_status.is_some() {
            break;
        }
    }

    let _ = watchdog.terminate_tree();
    let _ = watchdog.wait_tree_exit(DETACHED_HANDOFF_TIMEOUT);
    let _ = watchdog.wait();
    if direct_status.is_none() {
        direct_status = payload.wait().ok();
    }

    let drain_deadline = Instant::now() + DETACHED_HANDOFF_TIMEOUT;
    while !(stdout_eof && stderr_eof) && Instant::now() < drain_deadline {
        match output_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(OutputEvent::Stdout(bytes)) => {
                let text = stdout_decoder.push(&bytes, false);
                append_output_tail(&mut state.stdout, bytes.len(), &text);
            }
            Ok(OutputEvent::Stderr(bytes)) => {
                let text = stderr_decoder.push(&bytes, false);
                append_output_tail(&mut state.stderr, bytes.len(), &text);
            }
            Ok(OutputEvent::StdoutEof) => {
                let text = stdout_decoder.push(&[], true);
                append_output_tail(&mut state.stdout, 0, &text);
                stdout_eof = true;
            }
            Ok(OutputEvent::StderrEof) => {
                let text = stderr_decoder.push(&[], true);
                append_output_tail(&mut state.stderr, 0, &text);
                stderr_eof = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stdout_eof = true;
                stderr_eof = true;
            }
        }
    }
    if !(stdout_eof && stderr_eof) && forced_status.is_none() {
        forced_status = Some((
            "failed",
            "detached payload output did not reach EOF within the bounded drain window".to_string(),
        ));
    }
    // Terminal persistence must never wait on an unbounded reader/writer join.
    // Normal process-group cleanup closes these pipes. If a hostile descendant
    // escaped the group and kept a pipe open, process exit terminates these
    // supervisor-local helper threads after the bounded terminal commit.
    drop(stdout_thread);
    drop(stderr_thread);
    drop(stdin_thread);

    let (terminal_status, terminal_exit, terminal_error) =
        if let Some((status, error)) = forced_status {
            (status.to_string(), Some(-1), Some(error))
        } else if let Some(status) = direct_status {
            let code = status.code().unwrap_or(-1);
            if status.success() {
                ("completed".to_string(), Some(code), None)
            } else {
                ("failed".to_string(), Some(code), None)
            }
        } else {
            (
                "failed".to_string(),
                None,
                Some("detached payload exited without an observable status".to_string()),
            )
        };
    let stdout_final = state.stdout.clone();
    let stderr_final = state.stderr.clone();
    let terminal = store.update(&state.job_id, &state.execution_id, |record| {
        record.stdout = stdout_final;
        record.stderr = stderr_final;
        set_terminal(
            record,
            &terminal_status,
            terminal_exit,
            terminal_error.as_deref(),
            payload_started,
        );
        Ok(())
    })?;

    Ok(terminal)
}

pub(in crate::webcodex_runner::detached_job) fn spawn_watchdog(
    job_dir: &Path,
    creation_id: &str,
    execution_id: &str,
) -> Result<ManagedChild, String> {
    let mut command = internal_mode_command(
        DETACHED_INTERNAL_WATCHDOG,
        &[
            job_dir.to_string_lossy().into_owned(),
            creation_id.to_string(),
            execution_id.to_string(),
        ],
    )?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    ManagedChild::spawn(&mut command)
        .map_err(|error| format!("failed to spawn detached process-tree watchdog: {error}"))
}

pub(in crate::webcodex_runner::detached_job) fn run_watchdog(
    job_dir: &Path,
    creation_id: &str,
    execution_id: &str,
) -> Result<(), String> {
    validate_identity("watchdog creation_id", creation_id, 96)?;
    validate_identity("watchdog execution_id", execution_id, 96)?;
    if !creation_id.starts_with("birth_") {
        return Err("invalid detached watchdog creation identity".to_string());
    }
    reject_symlink_or_non_dir(job_dir, "detached Job directory")?;
    let record: DetachedJobRecord = read_json_bounded(
        &job_dir.join(STATE_FILE),
        DETACHED_STATE_MAX_BYTES,
        "detached Job state",
    )?;
    validate_record(&record)?;
    if record.execution_id != execution_id
        || !matches!(
            record.phase,
            DetachedJobPhase::OwnershipAccepted | DetachedJobPhase::Running
        )
    {
        return Err("stale detached watchdog invocation".to_string());
    }
    let mut lock = exclusive_lock(&job_dir.join(TREE_LOCK_FILE), false)?;
    write_lock_identity(&mut lock, creation_id)?;
    let pid = std::process::id();
    // ManagedChild creates this watchdog as the private process-group leader.
    // The watchdog itself remains a live member while it performs death cleanup,
    // so its group identity cannot be a stale/reused numeric PGID.
    let pgrp = unsafe { libc::getpgrp() };
    if pgrp <= 0 || pgrp as u32 != pid {
        return Err("detached watchdog is not its private process-group leader".to_string());
    }
    write_control_fd(2, format!("{WATCHDOG_ARMED}\n").as_bytes())?;

    let mut control = [0u8; 1];
    match io::stdin().read_exact(&mut control) {
        Ok(()) => Err("detached watchdog received unexpected control data".to_string()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            // The supervisor lifetime pipe closed. Because this exact watchdog
            // process is still a member/leader of the private group, kill(0)
            // targets only the group it currently belongs to; no PID lookup or
            // reuse-prone numeric identity is involved. Success SIGKILLs this
            // watchdog too, so the call does not return in the normal case.
            let rc = unsafe { libc::kill(0, libc::SIGKILL) };
            if rc == 0 {
                Err("detached watchdog survived its own process-group SIGKILL".to_string())
            } else {
                Err(format!(
                    "detached watchdog failed to terminate its process group: {}",
                    io::Error::last_os_error()
                ))
            }
        }
        Err(error) => Err(format!("detached watchdog control failed: {error}")),
    }
}

pub(in crate::webcodex_runner::detached_job) fn write_supervisor_handshake(
    bytes: &[u8],
) -> Result<(), String> {
    write_control_fd(2, bytes)
}

pub(in crate::webcodex_runner::detached_job) fn write_control_fd(
    fd: i32,
    mut bytes: &[u8],
) -> Result<(), String> {
    while !bytes.is_empty() {
        // SAFETY: bytes points at a live slice and fd is a process-owned stdio
        // descriptor configured for this internal control channel.
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("detached control write failed: {error}"));
        }
        if written == 0 {
            return Err("detached control write made no progress".to_string());
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

pub(in crate::webcodex_runner::detached_job) fn read_line_with_timeout(
    reader: &mut std::process::ChildStderr,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut line = Vec::with_capacity(64);
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("timed out waiting for detached child ack".to_string());
        }
        let remaining_ms = deadline
            .saturating_duration_since(now)
            .as_millis()
            .min(i32::MAX as u128) as i32;
        let mut pollfd = libc::pollfd {
            fd: reader.as_raw_fd(),
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        // SAFETY: pollfd points to one valid descriptor owned by reader.
        let ready = unsafe { libc::poll(&mut pollfd, 1, remaining_ms) };
        if ready == 0 {
            return Err("timed out waiting for detached child ack".to_string());
        }
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("failed to poll detached child ack: {error}"));
        }
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => return Err("detached child ack channel reached EOF".to_string()),
            Ok(1) => {
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    return String::from_utf8(line)
                        .map_err(|_| "detached child ack was not UTF-8".to_string());
                }
                if line.len() > 256 {
                    return Err("detached child ack exceeded 256 bytes".to_string());
                }
            }
            Ok(_) => unreachable!("single-byte detached ack read"),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("failed to read detached child ack: {error}")),
        }
    }
}
