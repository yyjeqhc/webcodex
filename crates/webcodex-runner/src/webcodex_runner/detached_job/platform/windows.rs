//! Native windows operations; durable ownership remains in the shared model/store.
use super::super::*;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt as WindowsOpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::os::windows::process::CommandExt as WindowsCommandExt;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, WaitForSingleObject, CREATE_BREAKAWAY_FROM_JOB,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

pub(in crate::webcodex_runner::detached_job) fn native_process_start_identity(
    pid: u32,
) -> Result<String, String> {
    let handle = windows_open_process_identity(pid)
        .map_err(|error| format!("failed to open detached Windows process identity: {error}"))?;
    let creation = windows_process_creation_time(handle.as_raw_handle() as HANDLE)?;
    Ok(format!("windows_creation_{creation}"))
}

pub(in crate::webcodex_runner::detached_job) fn windows_open_process_identity(
    pid: u32,
) -> Result<OwnedHandle, io::Error> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

pub(in crate::webcodex_runner::detached_job) fn windows_process_creation_time(
    handle: HANDLE,
) -> Result<u64, String> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(format!(
            "failed to read detached Windows process creation time: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

pub(in crate::webcodex_runner::detached_job) fn detached_process_identity_is_live(
    _lock_path: &Path,
    identity: &DetachedProcessIdentity,
) -> Result<bool, String> {
    validate_process_identity("recovery", identity)?;
    let handle = match windows_open_process_identity(identity.pid) {
        Ok(handle) => handle,
        Err(error) if error.raw_os_error() == Some(87) => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to open detached Windows process identity: {error}"
            ))
        }
    };
    let current = windows_process_creation_time(handle.as_raw_handle() as HANDLE)?;
    if identity.native_start_id != format!("windows_creation_{current}") {
        return Ok(false);
    }
    let wait = unsafe { WaitForSingleObject(handle.as_raw_handle() as HANDLE, 0) };
    if wait == WAIT_TIMEOUT {
        Ok(true)
    } else if wait == WAIT_OBJECT_0 {
        Ok(false)
    } else {
        Err(format!(
            "failed to probe detached Windows process liveness: {}",
            io::Error::last_os_error()
        ))
    }
}

pub(in crate::webcodex_runner::detached_job) fn ensure_private_dir(
    path: &Path,
) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create detached Job state root {}: {error}",
            path.display()
        )
    })?;
    reject_symlink_or_non_dir(path, "detached Job state root")
}

pub(in crate::webcodex_runner::detached_job) fn set_private_dir_permissions(
    path: &Path,
) -> Result<(), String> {
    // The Windows state root inherits its ACL from the Runner-owned application
    // state directory. Detached launch secrets are never persisted here.
    reject_symlink_or_non_dir(path, "detached Job directory")
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
            // Keep the exact temp content protected from readers/writers through
            // replacement while still allowing the rename/delete operation.
            .share_mode(FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&temp)
            .map_err(|error| format!("failed to create detached Job temp state: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write detached Job temp state: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync detached Job temp state: {error}"))?;
        let from = windows_wide_path(&temp);
        let to = windows_wide_path(path);
        let retry_deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0
            {
                drop(file);
                break;
            }
            let error = io::Error::last_os_error();
            let retryable = matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33));
            if !retryable || Instant::now() >= retry_deadline {
                drop(file);
                return Err(format!("failed to commit detached Job state: {error}"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(in crate::webcodex_runner::detached_job) fn windows_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(in crate::webcodex_runner::detached_job) fn sync_directory(_path: &Path) -> Result<(), String> {
    // State replacement uses MOVEFILE_WRITE_THROUGH. We make no stronger host
    // power-loss durability claim for parent-directory metadata on Windows.
    Ok(())
}

pub(in crate::webcodex_runner::detached_job) struct FileLock {
    pub(in crate::webcodex_runner::detached_job) file: File,
}

pub(in crate::webcodex_runner::detached_job) fn exclusive_lock(
    path: &Path,
    blocking: bool,
) -> Result<FileLock, String> {
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err("detached Job lock must be a regular non-symlink file".to_string())
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect detached Job lock {}: {error}",
                    path.display()
                ))
            }
        }
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // Deny concurrent read/write openers while permitting delete so an
            // expired terminal directory can remove this exact locked file.
            .share_mode(FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path);
        match opened {
            Ok(file) => {
                let metadata = fs::symlink_metadata(path).map_err(|error| {
                    format!(
                        "failed to revalidate detached Job lock {}: {error}",
                        path.display()
                    )
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(
                        "detached Job lock path changed or is not a regular non-symlink file"
                            .to_string(),
                    );
                }
                return Ok(FileLock { file });
            }
            Err(error) if blocking && error.raw_os_error() == Some(32) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(format!(
                    "failed to acquire detached Job lock {}: {error}",
                    path.display()
                ))
            }
        }
    }
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

pub(in crate::webcodex_runner::detached_job) fn run_internal_platform_mode(
    args: &[String],
) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some(DETACHED_INTERNAL_SUPERVISOR) if args.len() == 4 => {
            run_supervisor(Path::new(&args[1]), &args[2], &args[3], &mut io::stdin())
        }
        Some(DETACHED_INTERNAL_WATCHDOG) => {
            Err("detached watchdog internal mode is not used on Windows".to_string())
        }
        _ => Err("malformed detached internal mode arguments".to_string()),
    }
}

pub(in crate::webcodex_runner::detached_job) fn run_accepted_payload(
    store: &DetachedJobStore,
    accepted: &DetachedJobRecord,
    launch: DetachedLaunchSpec,
) -> Result<DetachedJobRecord, String> {
    let tree_birth = format!("birth_{}", Uuid::new_v4().simple());
    let mut payload_command = super::super::super::shell::structured_process_command(
        std::ffi::OsStr::new(&launch.process.executable),
        &launch.process.args,
        launch.cwd.as_deref().map(Path::new),
    )?;
    payload_command.env_clear();
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

    // ManagedChild is created inside the detached supervisor. Its private
    // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE Job Object therefore belongs to the
    // supervisor rather than the Runner. If the supervisor dies, Windows closes
    // its last Job handle and kills the complete payload tree.
    let mut payload = ManagedChild::spawn(&mut payload_command)
        .map_err(|error| format!("failed to spawn detached Windows payload: {error}"))?;
    let payload_started = unix_ms();
    let tree_pid = payload.id();
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
    let _ = write_supervisor_handshake(&[HANDSHAKE_ACCEPTED]);

    let stdin_thread = launch.stdin.map(|stdin| {
        let mut child_stdin = payload.child_mut().stdin.take();
        std::thread::spawn(move || {
            if let Some(mut child_stdin) = child_stdin.take() {
                let _ = child_stdin.write_all(stdin.as_bytes());
            }
        })
    });
    let stdout = payload
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| "detached payload stdout pipe is unavailable".to_string())?;
    let stderr = payload
        .child_mut()
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

        if forced_status.is_none() && last_control_poll.elapsed() >= DETACHED_CONTROL_POLL_INTERVAL
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
        if forced_status.is_none() && started.elapsed().as_secs() >= launch.timeout_secs {
            forced_status = Some((
                "timeout",
                format!(
                    "detached payload timed out after {} seconds",
                    launch.timeout_secs
                ),
            ));
        }
        if forced_status.is_some() {
            break;
        }
        match payload.try_tree_exit() {
            Ok(true) if direct_status.is_some() => break,
            Ok(_) => {}
            Err(error) => {
                forced_status = Some((
                    "failed",
                    format!("failed to observe detached Windows Job Object: {error}"),
                ));
                break;
            }
        }
    }

    if forced_status.is_some() {
        let _ = payload.terminate_tree();
        let _ = payload.wait_tree_exit(DETACHED_HANDOFF_TIMEOUT);
    }
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
    store.update(&state.job_id, &state.execution_id, |record| {
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
    })
}

pub(in crate::webcodex_runner::detached_job) fn write_supervisor_handshake(
    bytes: &[u8],
) -> Result<(), String> {
    let mut stderr = io::stderr().lock();
    stderr
        .write_all(bytes)
        .and_then(|_| stderr.flush())
        .map_err(|error| format!("detached supervisor handshake write failed: {error}"))
}
