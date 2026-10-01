//! Durable record types, identity validation and monotone state transitions.
use super::*;

pub(crate) const DETACHED_STATE_SCHEMA_VERSION: u32 = 2;

pub(crate) const DETACHED_STATE_MAX_RECORDS: usize = JOB_INVENTORY_MAX_JOBS;

// JSON escaping can expand the two retained 64 KiB text tails substantially
// (for example NUL becomes six JSON bytes). Keep one explicit 1 MiB record
// ceiling rather than allowing content-dependent persistence failures.
pub(crate) const DETACHED_STATE_MAX_BYTES: usize = 1024 * 1024;

pub(crate) const DETACHED_CONTEXT_MAX_BYTES: usize = 32 * 1024;

pub(crate) const DETACHED_ERROR_MAX_BYTES: usize = 4 * 1024;

pub(crate) const DETACHED_ENV_MAX_ENTRIES: usize = 256;

pub(crate) const DETACHED_ENV_FIELD_MAX_BYTES: usize = 8 * 1024;

pub(crate) const DETACHED_ENV_TOTAL_MAX_BYTES: usize = 64 * 1024;

pub(crate) const DETACHED_LAUNCH_MAX_BYTES: usize = 192 * 1024;

pub(crate) const DETACHED_HANDOFF_TIMEOUT: Duration = Duration::from_secs(5);

// Detached payloads may run for days. Output tails are recovery/presentation data,
// not process-liveness authority, so do not fsync+rename durable state at the live
// Job update cadence. Terminalization still drains and commits the final tails.
pub(crate) const DETACHED_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(5);

pub(super) const DETACHED_CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(super) const DETACHED_OUTPUT_CHANNEL_CAPACITY: usize = 64;

pub(super) const DETACHED_OUTPUT_READ_CHUNK: usize = 8 * 1024;

pub(super) const DETACHED_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) const DETACHED_INTERNAL_SUPERVISOR: &str = "--webcodex-internal-detached-supervisor";

pub(super) const DETACHED_INTERNAL_WATCHDOG: &str = "--webcodex-internal-detached-watchdog";

pub(super) const HANDSHAKE_READY: u8 = b'R';

pub(super) const HANDSHAKE_ACCEPT: u8 = b'C';

pub(super) const HANDSHAKE_ACCEPTED: u8 = b'A';

pub(super) const WATCHDOG_ARMED: &str = "WATCHDOG_ARMED";

pub(super) const SUPERVISOR_LOCK_FILE: &str = "supervisor.lock";

pub(super) const TREE_LOCK_FILE: &str = "tree.lock";

pub(super) const STATE_FILE: &str = "state.json";

pub(super) const STATE_TEMP_FILE: &str = ".state.tmp";

pub(super) const STATE_LOCK_FILE: &str = "state.lock";

pub(super) const ROOT_LOCK_FILE: &str = ".root.lock";

pub(super) const TERMINAL_RETENTION_MS: i64 = JOB_TERMINAL_RETENTION_SECS * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DetachedJobPhase {
    Prepared,
    SupervisorStarted,
    OwnershipAccepted,
    Running,
    Terminal,
}

impl DetachedJobPhase {
    pub(super) fn rank(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::SupervisorStarted => 1,
            Self::OwnershipAccepted => 2,
            Self::Running => 3,
            Self::Terminal => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedProcessIdentity {
    pub(crate) pid: u32,
    /// One random identifier generated before this exact native process birth.
    /// A live lifetime channel/lock is used with the PID; this identifier is
    /// never treated as a standalone liveness proof.
    pub(crate) creation_id: String,
    /// OS-native process birth identity used to fence PID reuse during restart
    /// reconciliation. Linux records `/proc/<pid>/stat` starttime, macOS uses
    /// `proc_pidinfo(PROC_PIDTBSDINFO)` birth time, and Windows uses process
    /// creation time. A backend must provide an equivalent before advertising handoff.
    pub(crate) native_start_id: String,
    pub(crate) started_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedOutputState {
    pub(crate) total_bytes: u64,
    pub(crate) retained_bytes: usize,
    /// Absolute line cursor of the first retained line. These cursors mirror
    /// `ShellJobStreamSnapshot` so Phase 2 can reconstruct the same Job log
    /// range after a Runner restart even when the bounded tail already dropped
    /// older lines.
    pub(crate) first_retained_line: usize,
    pub(crate) next_line: usize,
    pub(crate) truncated: bool,
    #[serde(default)]
    pub(crate) tail: String,
}

impl Default for DetachedOutputState {
    fn default() -> Self {
        Self {
            total_bytes: 0,
            retained_bytes: 0,
            first_retained_line: 1,
            next_line: 1,
            truncated: false,
            tail: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedTerminalResult {
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    pub(crate) completed_at_unix_ms: i64,
    pub(crate) duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedJobRecord {
    pub(crate) schema_version: u32,
    pub(crate) job_id: String,
    pub(crate) execution_id: String,
    pub(crate) request_id: String,
    pub(crate) client_id: String,
    #[serde(rename = "agent_instance_id")]
    pub(crate) runner_instance_id: String,
    pub(crate) context: ShellJobContext,
    pub(crate) phase: DetachedJobPhase,
    pub(crate) update_seq: u64,
    pub(crate) stop_requested: bool,
    pub(crate) created_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supervisor_started_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ownership_accepted_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) payload_started_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supervisor: Option<DetachedProcessIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tree_leader: Option<DetachedProcessIdentity>,
    #[serde(default)]
    pub(crate) stdout: DetachedOutputState,
    #[serde(default)]
    pub(crate) stderr: DetachedOutputState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) terminal: Option<DetachedTerminalResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedLaunchSpec {
    pub(crate) process: ShellProcessArgv,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stdin: Option<String>,
    #[serde(default)]
    pub(crate) env: Vec<(String, String)>,
    pub(crate) timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DetachedStartRequest {
    pub(crate) job_id: String,
    pub(crate) request_id: String,
    pub(crate) client_id: String,
    #[serde(rename = "agent_instance_id")]
    pub(crate) runner_instance_id: String,
    pub(crate) context: ShellJobContext,
    pub(crate) launch: DetachedLaunchSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DetachedHandoffOutcome {
    Accepted {
        execution_id: String,
        reconciled_from_state: bool,
        record: DetachedJobRecord,
    },
    Existing {
        execution_id: String,
        record: DetachedJobRecord,
    },
    PreAcceptFailed {
        execution_id: String,
        record: DetachedJobRecord,
    },
    OutcomeUnknown {
        execution_id: String,
        record: DetachedJobRecord,
    },
}

pub(super) fn execution_id_for(request: &DetachedStartRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-detached-job-v1\0");
    hasher.update(request.job_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.client_id.as_bytes());
    format!("exec_{:x}", hasher.finalize())
}

pub(super) fn validate_start_request(request: &DetachedStartRequest) -> Result<(), String> {
    validate_identity("job_id", &request.job_id, 256)?;
    validate_identity("request_id", &request.request_id, 256)?;
    validate_identity("client_id", &request.client_id, 128)?;
    validate_identity("agent_instance_id", &request.runner_instance_id, 256)?;
    let context = serde_json::to_vec(&request.context)
        .map_err(|error| format!("failed to encode detached Job context: {error}"))?;
    if context.len() > DETACHED_CONTEXT_MAX_BYTES {
        return Err(format!(
            "detached Job context exceeds {DETACHED_CONTEXT_MAX_BYTES} bytes"
        ));
    }
    validate_launch_spec(&request.launch)
}

pub(super) fn validate_launch_spec(spec: &DetachedLaunchSpec) -> Result<(), String> {
    validate_process_argv(&spec.process)?;
    if let Some(cwd) = spec.cwd.as_deref() {
        if cwd.is_empty() || cwd.len() > PROCESS_CWD_MAX_BYTES || cwd.contains('\0') {
            return Err(format!(
                "detached process cwd must be 1..={PROCESS_CWD_MAX_BYTES} bytes and contain no NUL"
            ));
        }
    }
    if let Some(stdin) = spec.stdin.as_deref() {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "detached process stdin exceeds {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("detached process stdin cannot contain NUL bytes".to_string());
        }
    }
    if !(STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=PROCESS_TIMEOUT_MAX_SECS)
        .contains(&spec.timeout_secs)
    {
        return Err(format!(
            "detached process timeout must be {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS}..={PROCESS_TIMEOUT_MAX_SECS} seconds"
        ));
    }
    if spec.env.len() > DETACHED_ENV_MAX_ENTRIES {
        return Err(format!(
            "detached process environment may contain at most {DETACHED_ENV_MAX_ENTRIES} entries"
        ));
    }
    let mut total = 0usize;
    for (index, (key, value)) in spec.env.iter().enumerate() {
        if key.is_empty()
            || key.contains('=')
            || key.contains('\0')
            || value.contains('\0')
            || key.len() > DETACHED_ENV_FIELD_MAX_BYTES
            || value.len() > DETACHED_ENV_FIELD_MAX_BYTES
        {
            return Err(format!(
                "detached process env[{index}] has an invalid or oversized key/value"
            ));
        }
        total = total
            .saturating_add(key.len())
            .saturating_add(value.len())
            .saturating_add(2);
    }
    if total > DETACHED_ENV_TOTAL_MAX_BYTES {
        return Err(format!(
            "detached process environment exceeds {DETACHED_ENV_TOTAL_MAX_BYTES} bytes"
        ));
    }
    let encoded = serde_json::to_vec(spec)
        .map_err(|error| format!("failed to encode detached launch payload: {error}"))?;
    if encoded.len() > DETACHED_LAUNCH_MAX_BYTES {
        return Err(format!(
            "detached launch payload exceeds {DETACHED_LAUNCH_MAX_BYTES} bytes"
        ));
    }
    Ok(())
}

pub(super) fn validate_existing_request(
    record: &DetachedJobRecord,
    request: &DetachedStartRequest,
) -> Result<(), String> {
    if record.job_id != request.job_id
        || record.request_id != request.request_id
        || record.client_id != request.client_id
        || record.runner_instance_id != request.runner_instance_id
        || record.context != request.context
        || record.execution_id != execution_id_for(request)
    {
        return Err(
            "detached Job already has a different durable execution/ownership identity".to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_identity(name: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(format!(
            "detached Job {name} must be non-empty, at most {max} bytes, and contain no NUL"
        ));
    }
    Ok(())
}

pub(super) fn validate_record(record: &DetachedJobRecord) -> Result<(), String> {
    if record.schema_version != DETACHED_STATE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported detached Job state schema version {}",
            record.schema_version
        ));
    }
    validate_identity("job_id", &record.job_id, 256)?;
    validate_identity("execution_id", &record.execution_id, 96)?;
    validate_identity("request_id", &record.request_id, 256)?;
    validate_identity("client_id", &record.client_id, 128)?;
    validate_identity("agent_instance_id", &record.runner_instance_id, 256)?;
    let context = serde_json::to_vec(&record.context)
        .map_err(|error| format!("failed to encode detached Job context: {error}"))?;
    if context.len() > DETACHED_CONTEXT_MAX_BYTES {
        return Err("detached Job durable context is oversized".to_string());
    }
    validate_output_state("stdout", &record.stdout)?;
    validate_output_state("stderr", &record.stderr)?;
    if let Some(identity) = record.supervisor.as_ref() {
        validate_process_identity("supervisor", identity)?;
    }
    if let Some(identity) = record.tree_leader.as_ref() {
        validate_process_identity("tree_leader", identity)?;
    }
    if record.phase.rank() >= DetachedJobPhase::SupervisorStarted.rank()
        && record.phase != DetachedJobPhase::Terminal
        && record.supervisor.is_none()
    {
        return Err("detached Job state is missing supervisor identity".to_string());
    }
    if matches!(
        record.phase,
        DetachedJobPhase::OwnershipAccepted | DetachedJobPhase::Running
    ) && record.ownership_accepted_at_unix_ms.is_none()
    {
        return Err("detached Job accepted state is missing acceptance timestamp".to_string());
    }
    if record.phase == DetachedJobPhase::Running
        && (record.payload_started_at_unix_ms.is_none() || record.tree_leader.is_none())
    {
        return Err("detached Job running state is missing process-tree identity".to_string());
    }
    match (&record.phase, &record.terminal) {
        (DetachedJobPhase::Terminal, Some(terminal)) => {
            validate_terminal(terminal)?;
        }
        (DetachedJobPhase::Terminal, None) => {
            return Err("detached Job terminal state is missing terminal result".to_string())
        }
        (_, Some(_)) => return Err("non-terminal detached Job has a terminal result".to_string()),
        _ => {}
    }
    if record.stop_requested && record.ownership_accepted_at_unix_ms.is_none() {
        return Err("detached Job stop request predates ownership acceptance".to_string());
    }
    let encoded = serde_json::to_vec(record)
        .map_err(|error| format!("failed to encode detached Job state: {error}"))?;
    if encoded.len() > DETACHED_STATE_MAX_BYTES {
        return Err("detached Job state exceeds its durable size bound".to_string());
    }
    Ok(())
}

pub(super) fn validate_process_identity(
    name: &str,
    identity: &DetachedProcessIdentity,
) -> Result<(), String> {
    if identity.pid == 0
        || identity.creation_id.len() > 96
        || !identity.creation_id.starts_with("birth_")
        || identity.native_start_id.is_empty()
        || identity.native_start_id.len() > 128
        || identity.native_start_id.contains('\0')
    {
        return Err(format!("invalid detached {name} process identity"));
    }
    #[cfg(target_os = "linux")]
    if !identity.native_start_id.starts_with("linux_start_") {
        return Err(format!(
            "invalid detached {name} Linux process start identity"
        ));
    }
    #[cfg(target_os = "macos")]
    if !identity.native_start_id.starts_with("macos_start_") {
        return Err(format!(
            "invalid detached {name} macOS process start identity"
        ));
    }
    #[cfg(windows)]
    if !identity.native_start_id.starts_with("windows_creation_") {
        return Err(format!(
            "invalid detached {name} Windows process start identity"
        ));
    }
    Ok(())
}

pub(super) fn validate_output_state(
    name: &str,
    output: &DetachedOutputState,
) -> Result<(), String> {
    if output.tail.len() > JOB_SNAPSHOT_STREAM_MAX_BYTES
        || output.retained_bytes != output.tail.len()
    {
        return Err(format!("detached {name} output state exceeds its bound"));
    }
    let expected_next = output
        .first_retained_line
        .checked_add(detached_retained_line_count(&output.tail))
        .ok_or_else(|| format!("detached {name} output line cursor overflow"))?;
    if output.first_retained_line == 0 || output.next_line != expected_next {
        return Err(format!(
            "detached {name} output line cursors are inconsistent"
        ));
    }
    Ok(())
}

pub(super) fn validate_terminal(terminal: &DetachedTerminalResult) -> Result<(), String> {
    if terminal.status.is_empty() || terminal.status.len() > 64 {
        return Err("detached Job terminal status is invalid".to_string());
    }
    if !matches!(
        terminal.status.as_str(),
        "completed" | "failed" | "stopped" | "timeout" | "handoff_failed" | "supervisor_lost"
    ) {
        return Err("detached Job terminal status is unsupported".to_string());
    }
    if terminal
        .error
        .as_deref()
        .is_some_and(|error| error.len() > DETACHED_ERROR_MAX_BYTES)
    {
        return Err("detached Job terminal error is oversized".to_string());
    }
    Ok(())
}

pub(super) fn validate_transition(
    previous: &DetachedJobRecord,
    next: &DetachedJobRecord,
) -> Result<(), String> {
    if previous.schema_version != next.schema_version
        || previous.job_id != next.job_id
        || previous.execution_id != next.execution_id
        || previous.request_id != next.request_id
        || previous.client_id != next.client_id
        || previous.runner_instance_id != next.runner_instance_id
        || previous.context != next.context
        || previous.created_at_unix_ms != next.created_at_unix_ms
    {
        return Err("detached Job immutable durable identity changed".to_string());
    }
    if next.update_seq != previous.update_seq.saturating_add(1) {
        return Err("detached Job update sequence must advance exactly once".to_string());
    }
    if previous.stop_requested && !next.stop_requested {
        return Err("detached Job stop request cannot be cleared".to_string());
    }
    if next.phase.rank() < previous.phase.rank() {
        return Err("detached Job phase cannot regress".to_string());
    }
    if previous.ownership_accepted_at_unix_ms.is_some()
        && previous.ownership_accepted_at_unix_ms != next.ownership_accepted_at_unix_ms
    {
        return Err("detached Job acceptance timestamp is immutable".to_string());
    }
    Ok(())
}

pub(super) fn set_terminal(
    record: &mut DetachedJobRecord,
    status: &str,
    exit_code: Option<i32>,
    error: Option<&str>,
    started_at_unix_ms: i64,
) {
    let completed = unix_ms();
    record.phase = DetachedJobPhase::Terminal;
    record.terminal = Some(DetachedTerminalResult {
        status: status.to_string(),
        exit_code,
        error: error.map(bound_error),
        completed_at_unix_ms: completed,
        duration_ms: completed.saturating_sub(started_at_unix_ms).max(0) as u64,
    });
}

pub(super) fn bound_error(error: &str) -> String {
    if error.len() <= DETACHED_ERROR_MAX_BYTES {
        return error.to_string();
    }
    let mut end = DETACHED_ERROR_MAX_BYTES;
    while end > 0 && !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_string()
}

pub(super) fn detached_retained_line_count(value: &str) -> usize {
    value.lines().count()
}

pub(super) fn append_output_tail(output: &mut DetachedOutputState, raw_bytes: usize, text: &str) {
    output.total_bytes = output.total_bytes.saturating_add(raw_bytes as u64);
    if !text.is_empty() {
        output.tail.push_str(text);
    }
    if output.tail.len() > JOB_SNAPSHOT_STREAM_MAX_BYTES {
        // Detached output is a byte tail: always preserve the newest bounded
        // bytes, even when one logical line is larger than the retention cap.
        // Advancing by newline bytes actually discarded is sufficient to keep
        // the absolute line range reconstructable without sacrificing a recent
        // suffix merely to align the retained tail to a line boundary.
        let mut start = output.tail.len() - JOB_SNAPSHOT_STREAM_MAX_BYTES;
        while start < output.tail.len() && !output.tail.is_char_boundary(start) {
            start += 1;
        }
        let dropped_lines = output.tail[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        output.tail.drain(..start);
        output.first_retained_line = output.first_retained_line.saturating_add(dropped_lines);
        output.truncated = true;
    }
    output.retained_bytes = output.tail.len();
    output.next_line = output
        .first_retained_line
        .saturating_add(detached_retained_line_count(&output.tail));
}

pub(crate) fn snapshot_from_detached_record(
    record: &DetachedJobRecord,
) -> Result<ShellJobSnapshot, String> {
    validate_record(record)?;
    if record.update_seq == 0 {
        return Err("detached Job snapshot update_seq must be greater than zero".to_string());
    }
    let terminal = record.terminal.as_ref();
    let status = match terminal.map(|value| value.status.as_str()) {
        Some("handoff_failed") => RunnerJobLifecycle::Failed,
        Some("supervisor_lost") => RunnerJobLifecycle::Lost,
        Some("timeout") => RunnerJobLifecycle::Timeout,
        Some("completed") => RunnerJobLifecycle::Completed,
        Some("failed") => RunnerJobLifecycle::Failed,
        Some("stopped") => RunnerJobLifecycle::Stopped,
        Some(other) => return Err(format!("unsupported detached terminal status {other}")),
        None if record.stop_requested => RunnerJobLifecycle::StopRequested,
        None if record.ownership_accepted_at_unix_ms.is_some() => RunnerJobLifecycle::Running,
        None => RunnerJobLifecycle::RunnerQueued,
    };
    let command_execution_state = match terminal.map(|value| value.status.as_str()) {
        Some("handoff_failed") => Some(ShellCommandExecutionState::NotStarted),
        Some("supervisor_lost") => Some(ShellCommandExecutionState::OutcomeUnknown),
        Some("timeout") => Some(ShellCommandExecutionState::TimedOut),
        Some("completed" | "failed" | "stopped") => Some(ShellCommandExecutionState::Completed),
        Some(_) => None,
        None => None,
    };
    let activity = matches!(
        status,
        RunnerJobLifecycle::Running | RunnerJobLifecycle::StopRequested
    )
    .then_some(ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase: ShellJobActivityPhase::ProcessRunning,
        source: ShellJobActivitySource::RunnerExecution,
    });
    let stream = |output: &DetachedOutputState| ShellJobStreamSnapshot {
        tail: output.tail.clone(),
        first_retained_line: output.first_retained_line,
        next_line: output.next_line,
        truncated: output.truncated,
    };
    Ok(ShellJobSnapshot {
        job_id: record.job_id.clone(),
        request_id: record.request_id.clone(),
        status: status.as_wire().to_string(),
        update_seq: record.update_seq,
        created_at: record.created_at_unix_ms.div_euclid(1000),
        started_at: record
            .payload_started_at_unix_ms
            .or(record.ownership_accepted_at_unix_ms)
            .map(|value| value.div_euclid(1000)),
        ended_at: terminal.map(|value| value.completed_at_unix_ms.div_euclid(1000)),
        exit_code: terminal.and_then(|value| value.exit_code),
        duration_ms: terminal.map(|value| value.duration_ms),
        error: terminal.and_then(|value| value.error.clone()),
        command_execution_state,
        context: record.context.clone(),
        stdout: stream(&record.stdout),
        stderr: stream(&record.stderr),
        validation_progress: None,
        test_count_evidence: None,
        activity,
    })
}

pub(super) fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
