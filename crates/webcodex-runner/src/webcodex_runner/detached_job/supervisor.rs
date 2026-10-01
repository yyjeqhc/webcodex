//! Shared handoff/acceptance lifecycle and bounded supervisor protocol.
use super::*;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB;

pub(crate) fn handoff_detached_job(
    store: &DetachedJobStore,
    request: DetachedStartRequest,
) -> Result<DetachedHandoffOutcome, String> {
    match store.prepare(&request)? {
        PrepareOutcome::Existing(record) => {
            let execution_id = record.execution_id.clone();
            if record.ownership_accepted_at_unix_ms.is_some() {
                Ok(DetachedHandoffOutcome::Accepted {
                    execution_id,
                    reconciled_from_state: true,
                    record,
                })
            } else if record.phase == DetachedJobPhase::Terminal {
                Ok(DetachedHandoffOutcome::PreAcceptFailed {
                    execution_id,
                    record,
                })
            } else {
                Ok(DetachedHandoffOutcome::Existing {
                    execution_id,
                    record,
                })
            }
        }
        PrepareOutcome::First(record) => {
            #[cfg(any(unix, windows))]
            {
                handoff_first_platform(store, request, record)
            }
            #[cfg(not(any(unix, windows)))]
            {
                let _ = request;
                let execution_id = record.execution_id.clone();
                let record = store.update(&record.job_id, &execution_id, |record| {
                    set_terminal(
                        record,
                        "handoff_failed",
                        None,
                        Some("detached Job supervisor is not enabled on this platform"),
                        record.created_at_unix_ms,
                    );
                    Ok(())
                })?;
                Err(format!(
                    "detached Job supervisor is unsupported on this platform (execution_id={})",
                    record.execution_id
                ))
            }
        }
    }
}

/// Hidden process entrypoint used only by the Runner's internal supervisor and
/// watchdog children. It is intentionally absent from public help and tool
/// surfaces.
pub(crate) fn maybe_run_internal_mode<I, S>(args: I) -> Option<i32>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect::<Vec<_>>();
    let first = args.first()?.as_str();
    if !matches!(
        first,
        DETACHED_INTERNAL_SUPERVISOR | DETACHED_INTERNAL_WATCHDOG
    ) {
        return None;
    }
    #[cfg(any(unix, windows))]
    {
        Some(match run_internal_platform_mode(&args) {
            Ok(()) => 0,
            Err(error) => {
                // Internal failures are persisted by the supervisor when it has
                // enough state to do so. Never print launch payload data here.
                eprintln!("webcodex-runner detached internal mode failed: {error}");
                1
            }
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = args;
        Some(2)
    }
}

#[cfg(any(unix, windows))]
pub(super) fn handoff_first_platform(
    store: &DetachedJobStore,
    request: DetachedStartRequest,
    prepared: DetachedJobRecord,
) -> Result<DetachedHandoffOutcome, String> {
    let job_dir = store.job_dir(&prepared.job_id);
    let supervisor_birth = format!("birth_{}", Uuid::new_v4().simple());
    let supervisor_args = [
        job_dir.to_string_lossy().into_owned(),
        prepared.execution_id.clone(),
        supervisor_birth,
    ];
    let mut command = match detached_supervisor_command(&supervisor_args, true) {
        Ok(command) => command,
        Err(error) => {
            mark_pre_accept_failure(store, &prepared, &error)?;
            return Err(error);
        }
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        #[cfg(windows)]
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            // Some host-owned Job Objects (including GitHub-hosted Windows
            // runners) forbid CREATE_BREAKAWAY_FROM_JOB. Access denied is a
            // pre-start CreateProcess failure, so no child exists to reconcile.
            // Retry exactly once without breakaway; the supervisor remains in
            // the host Job Object while retaining its own durable ownership and
            // nested payload Job Object semantics.
            let mut fallback = match detached_supervisor_command(&supervisor_args, false) {
                Ok(command) => command,
                Err(error) => {
                    mark_pre_accept_failure(store, &prepared, &error)?;
                    return Err(error);
                }
            };
            match fallback.spawn() {
                Ok(child) => child,
                Err(fallback_error) => {
                    let message = format!(
                        "failed to spawn detached Job supervisor after Windows breakaway fallback: {fallback_error}"
                    );
                    mark_pre_accept_failure(store, &prepared, &message)?;
                    return Err(message);
                }
            }
        }
        Err(error) => {
            let message = format!("failed to spawn detached Job supervisor: {error}");
            mark_pre_accept_failure(store, &prepared, &message)?;
            return Err(message);
        }
    };
    let mut child_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let message = "detached supervisor stdin pipe is unavailable".to_string();
            cleanup_pre_accept_supervisor(store, &prepared, child, message.clone())?;
            return Err(message);
        }
    };
    let child_stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let message = "detached supervisor handshake pipe is unavailable".to_string();
            cleanup_pre_accept_supervisor(store, &prepared, child, message.clone())?;
            return Err(message);
        }
    };
    let handshake = spawn_byte_reader(child_stderr);

    if let Err(error) = write_launch_frame(&mut child_stdin, &request.launch) {
        cleanup_pre_accept_supervisor(store, &prepared, child, error.clone())?;
        return Err(error);
    }

    match handshake.recv_timeout(DETACHED_HANDOFF_TIMEOUT) {
        Ok(HANDSHAKE_READY) => {}
        Ok(other) => {
            let error = format!("detached supervisor returned invalid ready byte {other}");
            cleanup_pre_accept_supervisor(store, &prepared, child, error.clone())?;
            return Err(error);
        }
        Err(error) => {
            let message = format!("detached supervisor did not become ready: {error}");
            cleanup_pre_accept_supervisor(store, &prepared, child, message.clone())?;
            return Err(message);
        }
    }

    if let Err(error) = child_stdin.write_all(&[HANDSHAKE_ACCEPT]) {
        let message = format!("failed to commit detached supervisor handoff: {error}");
        cleanup_pre_accept_supervisor(store, &prepared, child, message.clone())?;
        return Err(message);
    }
    if child_stdin.flush().is_err() {
        // A one-byte pipe write may already have delivered the commit. Never
        // kill or retry after this point; reconcile only from durable state.
        let record = wait_for_accepted_state(store, &prepared, DETACHED_HANDOFF_TIMEOUT)?;
        spawn_supervisor_reaper(child);
        if let Some(record) = record {
            return Ok(resolved_handoff_outcome(record, true));
        }
        return Ok(DetachedHandoffOutcome::OutcomeUnknown {
            execution_id: prepared.execution_id.clone(),
            record: prepared.clone(),
        });
    }
    drop(child_stdin);

    let ack = handshake.recv_timeout(DETACHED_HANDOFF_TIMEOUT);
    let durable = wait_for_accepted_state(store, &prepared, DETACHED_HANDOFF_TIMEOUT)?;
    if let Some(record) = durable {
        spawn_supervisor_reaper(child);
        return Ok(resolved_handoff_outcome(
            record,
            ack != Ok(HANDSHAKE_ACCEPTED),
        ));
    }

    // We successfully wrote the one-byte accept commit but cannot prove whether
    // the supervisor crossed the durable boundary. Do not signal or respawn it.
    // Keep the one-shot state claim and return an explicit unknown outcome.
    spawn_supervisor_reaper(child);
    let record = store.read(&prepared.job_id).unwrap_or(prepared.clone());
    Ok(DetachedHandoffOutcome::OutcomeUnknown {
        execution_id: prepared.execution_id,
        record,
    })
}

#[cfg(any(unix, windows))]
pub(super) fn resolved_handoff_outcome(
    record: DetachedJobRecord,
    reconciled_from_state: bool,
) -> DetachedHandoffOutcome {
    let execution_id = record.execution_id.clone();
    if record.ownership_accepted_at_unix_ms.is_some() {
        DetachedHandoffOutcome::Accepted {
            execution_id,
            reconciled_from_state,
            record,
        }
    } else {
        debug_assert_eq!(record.phase, DetachedJobPhase::Terminal);
        DetachedHandoffOutcome::PreAcceptFailed {
            execution_id,
            record,
        }
    }
}

#[cfg(any(unix, windows))]
pub(super) fn wait_for_accepted_state(
    store: &DetachedJobStore,
    prepared: &DetachedJobRecord,
    timeout: Duration,
) -> Result<Option<DetachedJobRecord>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let record = store.read(&prepared.job_id)?;
        if record.execution_id != prepared.execution_id {
            return Err("detached Job execution identity changed during handoff".to_string());
        }
        if record.ownership_accepted_at_unix_ms.is_some()
            || record.phase == DetachedJobPhase::Terminal
        {
            return Ok(Some(record));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(any(unix, windows))]
pub(super) fn mark_pre_accept_failure(
    store: &DetachedJobStore,
    prepared: &DetachedJobRecord,
    error: &str,
) -> Result<DetachedJobRecord, String> {
    let current = store.read(&prepared.job_id)?;
    if current.execution_id != prepared.execution_id
        || current.ownership_accepted_at_unix_ms.is_some()
    {
        return Err(
            "detached Job crossed or changed identity before pre-accept failure persistence"
                .to_string(),
        );
    }
    store.update(&prepared.job_id, &prepared.execution_id, |record| {
        set_terminal(
            record,
            "handoff_failed",
            None,
            Some(error),
            prepared.created_at_unix_ms,
        );
        Ok(())
    })
}

#[cfg(any(unix, windows))]
pub(super) fn cleanup_pre_accept_supervisor(
    store: &DetachedJobStore,
    prepared: &DetachedJobRecord,
    mut child: Child,
    error: String,
) -> Result<(), String> {
    // The direct Child handle is still Runner-owned before acceptance, so there
    // is no PID-reuse ambiguity in signaling this exact process.
    #[cfg(unix)]
    let _ = unsafe { libc::kill(child.id() as i32, libc::SIGKILL) };
    #[cfg(windows)]
    let _ = child.kill();
    let deadline = Instant::now() + DETACHED_HANDOFF_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                return Err(
                    "timed out waiting for pre-accept detached supervisor cleanup".to_string(),
                )
            }
            Err(wait_error) => {
                return Err(format!(
                    "failed to wait for pre-accept detached supervisor cleanup: {wait_error}"
                ))
            }
        }
    }
    let current = store.read(&prepared.job_id)?;
    if current.ownership_accepted_at_unix_ms.is_some() {
        return Err(
            "detached supervisor crossed ownership boundary during pre-accept cleanup; refusing to rewrite state"
                .to_string(),
        );
    }
    let _ = store.update(&prepared.job_id, &prepared.execution_id, |record| {
        set_terminal(
            record,
            "handoff_failed",
            None,
            Some(&error),
            prepared.created_at_unix_ms,
        );
        Ok(())
    })?;
    Ok(())
}

#[cfg(any(unix, windows))]
pub(super) fn spawn_supervisor_reaper(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(any(unix, windows))]
pub(super) fn write_launch_frame(
    writer: &mut impl Write,
    spec: &DetachedLaunchSpec,
) -> Result<(), String> {
    validate_launch_spec(spec)?;
    let bytes = serde_json::to_vec(spec)
        .map_err(|error| format!("failed to encode detached launch payload: {error}"))?;
    if bytes.len() > DETACHED_LAUNCH_MAX_BYTES || bytes.len() > u32::MAX as usize {
        return Err("detached launch payload exceeds its bound".to_string());
    }
    writer
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| writer.write_all(&bytes))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("failed to send detached launch payload: {error}"))
}

#[cfg(any(unix, windows))]
pub(super) fn read_launch_frame(reader: &mut impl Read) -> Result<DetachedLaunchSpec, String> {
    let mut length = [0u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| format!("failed to read detached launch length: {error}"))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > DETACHED_LAUNCH_MAX_BYTES {
        return Err("detached launch payload length is invalid".to_string());
    }
    let mut bytes = vec![0u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read detached launch payload: {error}"))?;
    let spec: DetachedLaunchSpec = serde_json::from_slice(&bytes)
        .map_err(|error| format!("malformed detached launch payload: {error}"))?;
    validate_launch_spec(&spec)?;
    Ok(spec)
}

#[cfg(any(unix, windows))]
pub(super) fn spawn_byte_reader(mut reader: impl Read + Send + 'static) -> mpsc::Receiver<u8> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        while reader.read_exact(&mut byte).is_ok() {
            if tx.send(byte[0]).is_err() {
                break;
            }
        }
    });
    rx
}

#[cfg(any(unix, windows))]
pub(super) fn run_supervisor(
    job_dir: &Path,
    execution_id: &str,
    supervisor_birth: &str,
    launch_reader: &mut impl Read,
) -> Result<(), String> {
    reject_symlink_or_non_dir(job_dir, "detached Job directory")?;
    let state_path = job_dir.join(STATE_FILE);
    let initial: DetachedJobRecord =
        read_json_bounded(&state_path, DETACHED_STATE_MAX_BYTES, "detached Job state")?;
    validate_record(&initial)?;
    if initial.execution_id != execution_id || initial.phase != DetachedJobPhase::Prepared {
        return Err("stale detached supervisor invocation".to_string());
    }
    validate_identity("supervisor creation_id", supervisor_birth, 96)?;
    if !supervisor_birth.starts_with("birth_") {
        return Err("invalid detached supervisor creation identity".to_string());
    }
    let store = DetachedJobStore::new(
        job_dir
            .parent()
            .ok_or_else(|| "detached Job directory has no state root".to_string())?
            .to_path_buf(),
    );
    let mut supervisor_lock = exclusive_lock(&job_dir.join(SUPERVISOR_LOCK_FILE), false)?;
    write_lock_identity(&mut supervisor_lock, supervisor_birth)?;
    let supervisor_pid = std::process::id();
    let supervisor_identity = DetachedProcessIdentity {
        pid: supervisor_pid,
        creation_id: supervisor_birth.to_string(),
        native_start_id: native_process_start_identity(supervisor_pid)?,
        started_at_unix_ms: unix_ms(),
    };
    let supervisor_started = store.update(&initial.job_id, execution_id, |record| {
        if record.phase != DetachedJobPhase::Prepared {
            return Err("detached supervisor state is no longer prepared".to_string());
        }
        record.phase = DetachedJobPhase::SupervisorStarted;
        record.supervisor_started_at_unix_ms = Some(unix_ms());
        record.supervisor = Some(supervisor_identity.clone());
        Ok(())
    })?;

    let launch = match read_launch_frame(launch_reader) {
        Ok(launch) => launch,
        Err(error) => {
            let _ = store.update(&initial.job_id, execution_id, |record| {
                set_terminal(
                    record,
                    "handoff_failed",
                    None,
                    Some(&error),
                    supervisor_started.created_at_unix_ms,
                );
                Ok(())
            });
            return Err(error);
        }
    };

    let execution_result = (|| -> Result<(), String> {
        // No process-tree helper or payload exists before this Ready/Accept
        // boundary. Before acceptance the Runner still owns the direct
        // supervisor child and can clean it without leaving an orphan.
        write_supervisor_handshake(&[HANDSHAKE_READY])?;
        let mut accept = [0u8; 1];
        launch_reader
            .read_exact(&mut accept)
            .map_err(|error| format!("detached handoff ended before acceptance: {error}"))?;
        if accept[0] != HANDSHAKE_ACCEPT {
            return Err("detached handoff accept byte is invalid".to_string());
        }

        let accepted_at = unix_ms();
        let accepted = store.update(&initial.job_id, execution_id, |record| {
            if record.phase != DetachedJobPhase::SupervisorStarted {
                return Err("detached supervisor cannot accept from current state".to_string());
            }
            record.phase = DetachedJobPhase::OwnershipAccepted;
            record.ownership_accepted_at_unix_ms = Some(accepted_at);
            Ok(())
        })?;

        let _ = run_accepted_payload(&store, &accepted, launch)?;
        Ok(())
    })();

    if let Err(error) = execution_result {
        let current = store.read(&initial.job_id)?;
        let accepted = current.ownership_accepted_at_unix_ms.is_some();
        let started_at = current
            .ownership_accepted_at_unix_ms
            .unwrap_or(supervisor_started.created_at_unix_ms);
        let status = if accepted { "failed" } else { "handoff_failed" };
        let terminal = store.update(&initial.job_id, execution_id, |record| {
            if record.phase != DetachedJobPhase::Terminal {
                set_terminal(record, status, None, Some(&error), started_at);
            }
            Ok(())
        })?;
        if terminal.ownership_accepted_at_unix_ms.is_some() {
            // A committed execution must be reconciled by the caller even when
            // post-accept payload setup fails; it must never be respawned.
            let _ = write_supervisor_handshake(&[HANDSHAKE_ACCEPTED]);
        }
        return Err(error);
    }

    drop(supervisor_lock);
    Ok(())
}

#[cfg(any(unix, windows))]
#[derive(Debug)]
pub(super) enum OutputEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    StdoutEof,
    StderrEof,
}

#[cfg(any(unix, windows))]
pub(super) fn spawn_output_reader(
    mut reader: impl Read + Send + 'static,
    tx: mpsc::SyncSender<OutputEvent>,
    stdout: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; DETACHED_OUTPUT_READ_CHUNK];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(if stdout {
                        OutputEvent::StdoutEof
                    } else {
                        OutputEvent::StderrEof
                    });
                    break;
                }
                Ok(count) => {
                    let event = if stdout {
                        OutputEvent::Stdout(buffer[..count].to_vec())
                    } else {
                        OutputEvent::Stderr(buffer[..count].to_vec())
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = tx.send(if stdout {
                        OutputEvent::StdoutEof
                    } else {
                        OutputEvent::StderrEof
                    });
                    break;
                }
            }
        }
    })
}

#[cfg(any(unix, windows))]
pub(super) fn detached_supervisor_command(
    args: &[String],
    _windows_breakaway: bool,
) -> Result<Command, String> {
    let mut command = internal_mode_command(DETACHED_INTERNAL_SUPERVISOR, args)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    make_new_session(&mut command);
    #[cfg(windows)]
    if _windows_breakaway {
        command.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    Ok(command)
}

#[cfg(any(unix, windows))]
pub(super) fn internal_mode_command(mode: &str, args: &[String]) -> Result<Command, String> {
    #[cfg(test)]
    {
        let mut command = Command::new(
            std::env::current_exe()
                .map_err(|error| format!("failed to locate detached test executable: {error}"))?,
        );
        command
            .arg("--exact")
            .arg("webcodex_runner::detached_job::tests::internal_mode_subprocess_entrypoint")
            .arg("--nocapture")
            .env_clear()
            .env("WEBCODEX_DETACHED_TEST_INTERNAL_MODE", mode)
            .env(
                "WEBCODEX_DETACHED_TEST_INTERNAL_ARGS",
                serde_json::to_string(args)
                    .map_err(|error| format!("failed to encode detached test args: {error}"))?,
            );
        return Ok(command);
    }
    #[cfg(not(test))]
    {
        let mut command =
            Command::new(std::env::current_exe().map_err(|error| {
                format!("failed to locate webcodex-runner executable: {error}")
            })?);
        command.arg(mode).args(args).env_clear();
        Ok(command)
    }
}
