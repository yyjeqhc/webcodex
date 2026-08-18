//! Detached Job ownership substrate for Runner-restart survival.
//!
//! This is deliberately separate from `webcodex-process::ManagedChild`'s
//! ordinary ownership path. A detached execution is prepared into bounded
//! Runner-owned durable state, then handed once to a narrow supervisor process.
//! The supervisor becomes the only payload process-tree owner after the durable
//! `OwnershipAccepted` transition. No public/general detach API lives here.

use super::output_text::{OutputTextDecoder, OutputTextSource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(unix, windows))]
use std::fs::{File, OpenOptions};
#[cfg(any(unix, windows))]
use std::io::{Read, Write};
#[cfg(unix)]
use std::io::{Seek, SeekFrom};
#[cfg(any(unix, windows))]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(any(unix, windows))]
use std::sync::mpsc;
#[cfg(any(unix, windows))]
use std::time::Instant;
use uuid::Uuid;
use webcodex_core::shell_protocol::{
    validate_process_argv, ShellCommandExecutionState, ShellJobContext, ShellJobSnapshot,
    ShellJobStreamSnapshot, ShellProcessArgv, JOB_INVENTORY_MAX_JOBS,
    JOB_SNAPSHOT_STREAM_MAX_BYTES, JOB_TERMINAL_RETENTION_SECS, PROCESS_CWD_MAX_BYTES,
    PROCESS_STDIN_MAX_BYTES, STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(any(unix, windows))]
use webcodex_process::ManagedChild;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as WindowsOpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
#[cfg(windows)]
use std::os::windows::process::CommandExt as WindowsCommandExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, WaitForSingleObject, CREATE_BREAKAWAY_FROM_JOB,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

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
pub(crate) const DETACHED_CHECKPOINT_INTERVAL: Duration = Duration::from_millis(250);
const DETACHED_CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DETACHED_OUTPUT_CHANNEL_CAPACITY: usize = 64;
const DETACHED_OUTPUT_READ_CHUNK: usize = 8 * 1024;
const DETACHED_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const DETACHED_INTERNAL_SUPERVISOR: &str = "--webcodex-internal-detached-supervisor";
const DETACHED_INTERNAL_WATCHDOG: &str = "--webcodex-internal-detached-watchdog";
const HANDSHAKE_READY: u8 = b'R';
const HANDSHAKE_ACCEPT: u8 = b'C';
const HANDSHAKE_ACCEPTED: u8 = b'A';
const WATCHDOG_ARMED: &str = "WATCHDOG_ARMED";
const SUPERVISOR_LOCK_FILE: &str = "supervisor.lock";
const TREE_LOCK_FILE: &str = "tree.lock";
const STATE_FILE: &str = "state.json";
const STATE_TEMP_FILE: &str = ".state.tmp";
const STATE_LOCK_FILE: &str = "state.lock";
const ROOT_LOCK_FILE: &str = ".root.lock";
const TERMINAL_RETENTION_MS: i64 = JOB_TERMINAL_RETENTION_SECS * 1_000;

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
    fn rank(self) -> u8 {
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
    /// reconciliation. Linux records `/proc/<pid>/stat` starttime; later
    /// platform backends must provide their equivalent before enabling handoff.
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
    pub(crate) agent_instance_id: String,
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
    pub(crate) agent_instance_id: String,
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

#[derive(Debug, Clone)]
pub(crate) struct DetachedJobStore {
    root: PathBuf,
}

#[derive(Debug)]
enum PrepareOutcome {
    First(DetachedJobRecord),
    Existing(DetachedJobRecord),
}

impl DetachedJobStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // Phase 1 establishes the durable substrate without wiring ordinary
    // JobManager execution into it; Phase 2 restart reconstruction will use the
    // canonical state root.
    #[allow(dead_code)]
    pub(crate) fn default_root() -> Result<PathBuf, String> {
        Ok(
            webcodex_agent_config::paths::default_client_state_base_dir()?
                .join("runner-detached-jobs-v1"),
        )
    }

    fn job_dir(&self, job_id: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(job_id.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        self.root.join(digest)
    }

    fn state_path_for_job(&self, job_id: &str) -> PathBuf {
        self.job_dir(job_id).join(STATE_FILE)
    }

    pub(crate) fn read(&self, job_id: &str) -> Result<DetachedJobRecord, String> {
        let record: DetachedJobRecord = read_json_bounded(
            &self.state_path_for_job(job_id),
            DETACHED_STATE_MAX_BYTES,
            "detached Job state",
        )?;
        validate_record(&record)?;
        if record.job_id != job_id {
            return Err("detached Job state job_id does not match its lookup identity".to_string());
        }
        Ok(record)
    }

    pub(crate) fn scan_for_client(
        &self,
        client_id: &str,
    ) -> Result<Vec<DetachedJobRecord>, String> {
        validate_identity("client_id", client_id, 128)?;
        match fs::symlink_metadata(&self.root) {
            Ok(_) => reject_symlink_or_non_dir(&self.root, "detached Job state root")?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "failed to inspect detached Job state root {}: {error}",
                    self.root.display()
                ))
            }
        }
        let _root_lock = exclusive_lock(&self.root.join(ROOT_LOCK_FILE), true)?;
        let mut records = Vec::new();
        let mut job_dirs = 0usize;
        for entry in fs::read_dir(&self.root)
            .map_err(|error| format!("failed to list detached Job state root: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("failed to inspect detached Job state: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("failed to inspect detached Job state entry: {error}"))?;
            if file_type.is_symlink() {
                return Err("detached Job state root contains a symlink entry".to_string());
            }
            if !file_type.is_dir() {
                if entry.file_name() == ROOT_LOCK_FILE {
                    continue;
                }
                return Err(
                    "detached Job state root contains an unexpected non-directory entry"
                        .to_string(),
                );
            }
            job_dirs = job_dirs.saturating_add(1);
            if job_dirs > DETACHED_STATE_MAX_RECORDS {
                return Err(format!(
                    "detached Job state root exceeds {DETACHED_STATE_MAX_RECORDS} records"
                ));
            }
            let record: DetachedJobRecord = read_json_bounded(
                &entry.path().join(STATE_FILE),
                DETACHED_STATE_MAX_BYTES,
                "detached Job state",
            )?;
            validate_record(&record)?;
            if self.job_dir(&record.job_id) != entry.path() {
                return Err(
                    "detached Job state directory does not match its job identity".to_string(),
                );
            }
            if record.client_id == client_id {
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        Ok(records)
    }

    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn reconcile_after_runner_restart(
        &self,
        record: DetachedJobRecord,
    ) -> Result<Option<DetachedJobRecord>, String> {
        validate_record(&record)?;
        if record.phase == DetachedJobPhase::Terminal {
            return Ok(Some(record));
        }
        if record.ownership_accepted_at_unix_ms.is_none() {
            // A replacement Runner must never turn a pre-accept residue into a
            // respawn. Prepared has no supervisor and is therefore definitely
            // not_started. SupervisorStarted may still be draining the old
            // Runner's one-shot handoff pipe; while that exact supervisor is live
            // we retain and observe the same record. Only after its exact native
            // identity/lifetime lock proves dead may recovery persist
            // handoff_failed. This closes the narrow accept-byte race without
            // making the residue invisible.
            if record.phase == DetachedJobPhase::Prepared {
                let execution_id = record.execution_id.clone();
                return self
                    .update(&record.job_id, &execution_id, |current| {
                        set_terminal(
                            current,
                            "handoff_failed",
                            None,
                            Some("Runner restarted before detached ownership acceptance"),
                            current.created_at_unix_ms,
                        );
                        Ok(())
                    })
                    .map(Some);
            }
            let supervisor = record.supervisor.as_ref().ok_or_else(|| {
                "pre-accept detached Job is missing supervisor identity".to_string()
            })?;
            if detached_process_identity_is_live(
                &self.job_dir(&record.job_id).join(SUPERVISOR_LOCK_FILE),
                supervisor,
            )? {
                return Ok(Some(record));
            }
            let refreshed = self.read(&record.job_id)?;
            if refreshed.phase == DetachedJobPhase::Terminal
                || refreshed.ownership_accepted_at_unix_ms.is_some()
            {
                return Ok(Some(refreshed));
            }
            let execution_id = refreshed.execution_id.clone();
            return self
                .update(&refreshed.job_id, &execution_id, |current| {
                    set_terminal(
                        current,
                        "handoff_failed",
                        None,
                        Some("detached supervisor exited before ownership acceptance"),
                        current.created_at_unix_ms,
                    );
                    Ok(())
                })
                .map(Some);
        }
        let supervisor = record
            .supervisor
            .as_ref()
            .ok_or_else(|| "accepted detached Job is missing supervisor identity".to_string())?;
        if detached_process_identity_is_live(
            &self.job_dir(&record.job_id).join(SUPERVISOR_LOCK_FILE),
            supervisor,
        )? {
            return Ok(Some(record));
        }
        let refreshed = self.read(&record.job_id)?;
        if refreshed.phase == DetachedJobPhase::Terminal {
            return Ok(Some(refreshed));
        }
        let started_at = refreshed
            .payload_started_at_unix_ms
            .or(refreshed.ownership_accepted_at_unix_ms)
            .unwrap_or(refreshed.created_at_unix_ms);
        let execution_id = refreshed.execution_id.clone();
        self.update(&refreshed.job_id, &execution_id, |current| {
            if current.phase != DetachedJobPhase::Terminal {
                set_terminal(
                    current,
                    "supervisor_lost",
                    None,
                    Some(
                        "detached supervisor is no longer live after Runner restart reconciliation",
                    ),
                    started_at,
                );
            }
            Ok(())
        })
        .map(Some)
    }

    pub(crate) fn request_stop(
        &self,
        job_id: &str,
        execution_id: &str,
    ) -> Result<DetachedJobRecord, String> {
        self.update(job_id, execution_id, |record| {
            if record.ownership_accepted_at_unix_ms.is_none() {
                return Err(
                    "detached Job cannot be stopped before ownership acceptance".to_string()
                );
            }
            record.stop_requested = true;
            Ok(())
        })
    }

    fn reclaim_expired_terminal_records_locked(&self, now_unix_ms: i64) -> Result<usize, String> {
        let mut retained = 0usize;
        for entry in fs::read_dir(&self.root).map_err(|error| {
            format!("failed to list detached Job state root for reclamation: {error}")
        })? {
            let entry = entry.map_err(|error| {
                format!("failed to inspect detached Job state during reclamation: {error}")
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!("failed to inspect detached Job state entry during reclamation: {error}")
            })?;
            if file_type.is_symlink() {
                return Err("detached Job reclamation found a symlink state entry".to_string());
            }
            if !file_type.is_dir() {
                if entry.file_name() == ROOT_LOCK_FILE {
                    continue;
                }
                return Err(
                    "detached Job reclamation found an unexpected non-directory state entry"
                        .to_string(),
                );
            }
            let job_dir = entry.path();
            let initial: DetachedJobRecord = read_json_bounded(
                &job_dir.join(STATE_FILE),
                DETACHED_STATE_MAX_BYTES,
                "detached Job state",
            )?;
            validate_record(&initial)?;
            if self.job_dir(&initial.job_id) != job_dir {
                return Err(
                    "detached Job reclamation state directory does not match durable job identity"
                        .to_string(),
                );
            }
            let terminal_expired = initial.terminal.as_ref().is_some_and(|terminal| {
                now_unix_ms.saturating_sub(terminal.completed_at_unix_ms) >= TERMINAL_RETENTION_MS
            });
            if !terminal_expired {
                retained = retained.saturating_add(1);
                continue;
            }
            self.reclaim_terminal_job_dir_locked(&job_dir, &initial, now_unix_ms)?;
        }
        Ok(retained)
    }

    fn reclaim_terminal_job_dir_locked(
        &self,
        job_dir: &Path,
        expected: &DetachedJobRecord,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        reject_symlink_or_non_dir(job_dir, "detached Job reclamation directory")?;
        let state_lock_path = job_dir.join(STATE_LOCK_FILE);
        let state_lock = exclusive_lock(&state_lock_path, true)?;
        let current: DetachedJobRecord = read_json_bounded(
            &job_dir.join(STATE_FILE),
            DETACHED_STATE_MAX_BYTES,
            "detached Job state",
        )?;
        validate_record(&current)?;
        if current.job_id != expected.job_id || current.execution_id != expected.execution_id {
            return Err("detached Job reclamation identity changed under lock".to_string());
        }
        let terminal = current
            .terminal
            .as_ref()
            .ok_or_else(|| "detached Job reclamation refuses an active state record".to_string())?;
        if now_unix_ms.saturating_sub(terminal.completed_at_unix_ms) < TERMINAL_RETENTION_MS {
            return Err("detached Job reclamation retention window changed under lock".to_string());
        }
        let mut removable = Vec::new();
        for child in fs::read_dir(job_dir).map_err(|error| {
            format!("failed to list detached Job directory for reclamation: {error}")
        })? {
            let child = child.map_err(|error| {
                format!("failed to inspect detached Job reclamation child: {error}")
            })?;
            let file_type = child.file_type().map_err(|error| {
                format!("failed to inspect detached Job reclamation child type: {error}")
            })?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(
                    "detached Job reclamation found a symlink or non-file child; refusing deletion"
                        .to_string(),
                );
            }
            let name = child.file_name();
            let known = name == STATE_FILE
                || name == STATE_LOCK_FILE
                || name == STATE_TEMP_FILE
                || name == SUPERVISOR_LOCK_FILE
                || name == TREE_LOCK_FILE;
            if !known {
                return Err(
                    "detached Job reclamation found an unexpected state child; refusing deletion"
                        .to_string(),
                );
            }
            removable.push(child.path());
        }
        for path in removable {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "failed to remove expired detached Job state {}: {error}",
                    path.display()
                )
            })?;
        }
        // The exact state lock remains open through remove_dir. A racing updater
        // can only fail closed against the disappearing path; it cannot turn a
        // terminal record back into an active execution.
        fs::remove_dir(job_dir).map_err(|error| {
            format!(
                "failed to remove expired detached Job directory {}: {error}",
                job_dir.display()
            )
        })?;
        drop(state_lock);
        sync_directory(&self.root)?;
        Ok(())
    }

    fn prepare(&self, request: &DetachedStartRequest) -> Result<PrepareOutcome, String> {
        validate_start_request(request)?;
        ensure_private_dir(&self.root)?;
        let _root_lock = exclusive_lock(&self.root.join(ROOT_LOCK_FILE), true)?;
        let job_dir = self.job_dir(&request.job_id);
        match fs::symlink_metadata(&job_dir) {
            Ok(_) => {
                reject_symlink_or_non_dir(&job_dir, "detached Job directory")?;
                let existing = self.read(&request.job_id)?;
                validate_existing_request(&existing, request)?;
                return Ok(PrepareOutcome::Existing(existing));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect detached Job state directory before prepare: {error}"
                ))
            }
        }
        let retained = self.reclaim_expired_terminal_records_locked(unix_ms())?;
        if retained >= DETACHED_STATE_MAX_RECORDS {
            return Err(format!(
                "detached Job state root is full; maximum is {DETACHED_STATE_MAX_RECORDS} records"
            ));
        }
        fs::create_dir(&job_dir)
            .map_err(|error| format!("failed to create detached Job state directory: {error}"))?;
        set_private_dir_permissions(&job_dir)?;

        let record = DetachedJobRecord {
            schema_version: DETACHED_STATE_SCHEMA_VERSION,
            job_id: request.job_id.clone(),
            execution_id: execution_id_for(request),
            request_id: request.request_id.clone(),
            client_id: request.client_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            context: request.context.clone(),
            phase: DetachedJobPhase::Prepared,
            // JobManager has already projected agent_queued at sequence 1 before
            // detached durable handoff starts. Reserve that sequence so every
            // durable transition, including a pre-accept failure, is strictly
            // newer than the public queued snapshot.
            update_seq: 1,
            stop_requested: false,
            created_at_unix_ms: unix_ms(),
            supervisor_started_at_unix_ms: None,
            ownership_accepted_at_unix_ms: None,
            payload_started_at_unix_ms: None,
            supervisor: None,
            tree_leader: None,
            stdout: DetachedOutputState::default(),
            stderr: DetachedOutputState::default(),
            terminal: None,
        };
        validate_record(&record)?;
        if let Err(error) =
            atomic_write_json(&job_dir.join(STATE_FILE), &record, DETACHED_STATE_MAX_BYTES)
        {
            let _ = fs::remove_dir(&job_dir);
            return Err(error);
        }
        Ok(PrepareOutcome::First(record))
    }

    fn update<F>(
        &self,
        job_id: &str,
        execution_id: &str,
        update: F,
    ) -> Result<DetachedJobRecord, String>
    where
        F: FnOnce(&mut DetachedJobRecord) -> Result<(), String>,
    {
        let job_dir = self.job_dir(job_id);
        reject_symlink_or_non_dir(&job_dir, "detached Job directory")?;
        let _guard = exclusive_lock(&job_dir.join(STATE_LOCK_FILE), true)?;
        let mut record: DetachedJobRecord = read_json_bounded(
            &job_dir.join(STATE_FILE),
            DETACHED_STATE_MAX_BYTES,
            "detached Job state",
        )?;
        validate_record(&record)?;
        if record.job_id != job_id || record.execution_id != execution_id {
            return Err("detached Job state identity mismatch".to_string());
        }
        let previous = record.clone();
        if previous.phase == DetachedJobPhase::Terminal {
            return Ok(previous);
        }
        update(&mut record)?;
        if record == previous {
            return Ok(previous);
        }
        record.update_seq = previous
            .update_seq
            .checked_add(1)
            .ok_or_else(|| "detached Job update sequence overflow".to_string())?;
        validate_transition(&previous, &record)?;
        validate_record(&record)?;
        atomic_write_json(&job_dir.join(STATE_FILE), &record, DETACHED_STATE_MAX_BYTES)?;
        Ok(record)
    }
}

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

fn execution_id_for(request: &DetachedStartRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-detached-job-v1\0");
    hasher.update(request.job_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.client_id.as_bytes());
    format!("exec_{:x}", hasher.finalize())
}

fn validate_start_request(request: &DetachedStartRequest) -> Result<(), String> {
    validate_identity("job_id", &request.job_id, 256)?;
    validate_identity("request_id", &request.request_id, 256)?;
    validate_identity("client_id", &request.client_id, 128)?;
    validate_identity("agent_instance_id", &request.agent_instance_id, 256)?;
    let context = serde_json::to_vec(&request.context)
        .map_err(|error| format!("failed to encode detached Job context: {error}"))?;
    if context.len() > DETACHED_CONTEXT_MAX_BYTES {
        return Err(format!(
            "detached Job context exceeds {DETACHED_CONTEXT_MAX_BYTES} bytes"
        ));
    }
    validate_launch_spec(&request.launch)
}

fn validate_launch_spec(spec: &DetachedLaunchSpec) -> Result<(), String> {
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
    if !(STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS)
        .contains(&spec.timeout_secs)
    {
        return Err(format!(
            "detached process timeout must be {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS}..={STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS} seconds"
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

fn validate_existing_request(
    record: &DetachedJobRecord,
    request: &DetachedStartRequest,
) -> Result<(), String> {
    if record.job_id != request.job_id
        || record.request_id != request.request_id
        || record.client_id != request.client_id
        || record.agent_instance_id != request.agent_instance_id
        || record.context != request.context
        || record.execution_id != execution_id_for(request)
    {
        return Err(
            "detached Job already has a different durable execution/ownership identity".to_string(),
        );
    }
    Ok(())
}

fn validate_identity(name: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(format!(
            "detached Job {name} must be non-empty, at most {max} bytes, and contain no NUL"
        ));
    }
    Ok(())
}

fn validate_record(record: &DetachedJobRecord) -> Result<(), String> {
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
    validate_identity("agent_instance_id", &record.agent_instance_id, 256)?;
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

fn validate_process_identity(name: &str, identity: &DetachedProcessIdentity) -> Result<(), String> {
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
    #[cfg(windows)]
    if !identity.native_start_id.starts_with("windows_creation_") {
        return Err(format!(
            "invalid detached {name} Windows process start identity"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn native_process_start_identity(pid: u32) -> Result<String, String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
        .map_err(|error| format!("failed to read detached process start identity: {error}"))?;
    let close = stat
        .rfind(')')
        .ok_or_else(|| "malformed /proc process stat for detached identity".to_string())?;
    let remaining = stat
        .get(close + 2..)
        .ok_or_else(|| "malformed /proc process stat suffix for detached identity".to_string())?;
    // `/proc/<pid>/stat` field 22 is starttime. `remaining` begins at field 3
    // (`state`), so starttime is zero-based index 19 after the closing comm.
    let start = remaining
        .split_whitespace()
        .nth(19)
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| "missing Linux process starttime for detached identity".to_string())?;
    Ok(format!("linux_start_{start}"))
}

#[cfg(windows)]
fn native_process_start_identity(pid: u32) -> Result<String, String> {
    let handle = windows_open_process_identity(pid)
        .map_err(|error| format!("failed to open detached Windows process identity: {error}"))?;
    let creation = windows_process_creation_time(handle.as_raw_handle() as HANDLE)?;
    Ok(format!("windows_creation_{creation}"))
}

#[cfg(windows)]
fn windows_open_process_identity(pid: u32) -> Result<OwnedHandle, io::Error> {
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

#[cfg(windows)]
fn windows_process_creation_time(handle: HANDLE) -> Result<u64, String> {
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

#[cfg(windows)]
fn detached_process_identity_is_live(
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

#[cfg(all(unix, not(target_os = "linux")))]
fn native_process_start_identity(_pid: u32) -> Result<String, String> {
    Err(
        "detached native process start identity is not implemented on this Unix platform"
            .to_string(),
    )
}

#[cfg(target_os = "linux")]
fn detached_process_identity_is_live(
    lock_path: &Path,
    identity: &DetachedProcessIdentity,
) -> Result<bool, String> {
    validate_process_identity("recovery", identity)?;
    let current_start = match native_process_start_identity(identity.pid) {
        Ok(value) => value,
        Err(error) => {
            let proc_stat = PathBuf::from(format!("/proc/{}/stat", identity.pid));
            if !proc_stat.exists() {
                return Ok(false);
            }
            return Err(error);
        }
    };
    if current_start != identity.native_start_id {
        return Ok(false);
    }
    lifetime_lock_is_held(lock_path, &identity.creation_id)
}

fn validate_output_state(name: &str, output: &DetachedOutputState) -> Result<(), String> {
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

fn validate_terminal(terminal: &DetachedTerminalResult) -> Result<(), String> {
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

fn validate_transition(
    previous: &DetachedJobRecord,
    next: &DetachedJobRecord,
) -> Result<(), String> {
    if previous.schema_version != next.schema_version
        || previous.job_id != next.job_id
        || previous.execution_id != next.execution_id
        || previous.request_id != next.request_id
        || previous.client_id != next.client_id
        || previous.agent_instance_id != next.agent_instance_id
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

fn set_terminal(
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

fn bound_error(error: &str) -> String {
    if error.len() <= DETACHED_ERROR_MAX_BYTES {
        return error.to_string();
    }
    let mut end = DETACHED_ERROR_MAX_BYTES;
    while end > 0 && !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_string()
}

fn detached_retained_line_count(value: &str) -> usize {
    value.lines().count()
}

fn append_output_tail(output: &mut DetachedOutputState, raw_bytes: usize, text: &str) {
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
        Some("handoff_failed") => "failed",
        Some("supervisor_lost") => "lost",
        Some("timeout") => "timeout",
        Some("completed") => "completed",
        Some("failed") => "failed",
        Some("stopped") => "stopped",
        Some(other) => return Err(format!("unsupported detached terminal status {other}")),
        None if record.stop_requested => "stop_requested",
        None if record.ownership_accepted_at_unix_ms.is_some() => "running",
        None => "agent_queued",
    };
    let command_execution_state = match terminal.map(|value| value.status.as_str()) {
        Some("handoff_failed") => Some(ShellCommandExecutionState::NotStarted),
        Some("supervisor_lost") => Some(ShellCommandExecutionState::OutcomeUnknown),
        Some("timeout") => Some(ShellCommandExecutionState::TimedOut),
        Some("completed" | "failed" | "stopped") => Some(ShellCommandExecutionState::Completed),
        Some(_) => None,
        None => None,
    };
    let stream = |output: &DetachedOutputState| ShellJobStreamSnapshot {
        tail: output.tail.clone(),
        first_retained_line: output.first_retained_line,
        next_line: output.next_line,
        truncated: output.truncated,
    };
    Ok(ShellJobSnapshot {
        job_id: record.job_id.clone(),
        request_id: record.request_id.clone(),
        status: status.to_string(),
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
    })
}

fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(unix)]
fn ensure_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create detached Job state root {}: {error}",
            path.display()
        )
    })?;
    reject_symlink_or_non_dir(path, "detached Job state root")?;
    set_private_dir_permissions(path)
}

#[cfg(windows)]
fn ensure_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create detached Job state root {}: {error}",
            path.display()
        )
    })?;
    reject_symlink_or_non_dir(path, "detached Job state root")
}

#[cfg(not(any(unix, windows)))]
fn ensure_private_dir(_path: &Path) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "failed to set private detached Job directory permissions on {}: {error}",
            path.display()
        )
    })
}

#[cfg(windows)]
fn set_private_dir_permissions(path: &Path) -> Result<(), String> {
    // The Windows state root inherits its ACL from the Runner-owned application
    // state directory. Detached launch secrets are never persisted here.
    reject_symlink_or_non_dir(path, "detached Job directory")
}

#[cfg(not(any(unix, windows)))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

fn reject_symlink_or_non_dir(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} must be a real directory, not a symlink"));
    }
    Ok(())
}

fn read_json_bounded<T: for<'de> Deserialize<'de>>(
    path: &Path,
    max_bytes: usize,
    label: &str,
) -> Result<T, String> {
    #[cfg(unix)]
    let bytes = {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| format!("failed to open {label} {}: {error}", path.display()))?;
        validate_open_regular_file(&file, path, label)?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("failed to inspect open {label}: {error}"))?;
        if metadata.len() > max_bytes as u64 {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take((max_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
        if bytes.len() > max_bytes {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        bytes
    };
    #[cfg(not(unix))]
    let bytes = {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("{label} must be a regular non-symlink file"));
        }
        if metadata.len() > max_bytes as u64 {
            return Err(format!("{label} exceeds {max_bytes} bytes"));
        }
        fs::read(path)
            .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?
    };
    serde_json::from_slice(&bytes).map_err(|error| format!("corrupt {label}: {error}"))
}

#[cfg(unix)]
fn atomic_write_json<T: Serialize>(path: &Path, value: &T, max_bytes: usize) -> Result<(), String> {
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

#[cfg(windows)]
fn atomic_write_json<T: Serialize>(path: &Path, value: &T, max_bytes: usize) -> Result<(), String> {
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

#[cfg(not(any(unix, windows)))]
fn atomic_write_json<T: Serialize>(
    _path: &Path,
    _value: &T,
    _max_bytes: usize,
) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

#[cfg(windows)]
fn windows_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    let directory = File::open(path)
        .map_err(|error| format!("failed to open detached Job state directory: {error}"))?;
    directory
        .sync_all()
        .map_err(|error| format!("failed to sync detached Job state directory: {error}"))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), String> {
    // State replacement uses MOVEFILE_WRITE_THROUGH. We make no stronger host
    // power-loss durability claim for parent-directory metadata on Windows.
    Ok(())
}

#[cfg(unix)]
fn validate_open_regular_file(file: &File, path: &Path, label: &str) -> Result<(), String> {
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

#[cfg(unix)]
struct FileLock {
    file: File,
}

#[cfg(unix)]
impl Drop for FileLock {
    fn drop(&mut self) {
        // SAFETY: file is a live descriptor owned by this guard.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[cfg(unix)]
fn exclusive_lock(path: &Path, blocking: bool) -> Result<FileLock, String> {
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

#[cfg(windows)]
struct FileLock {
    file: File,
}

#[cfg(windows)]
fn exclusive_lock(path: &Path, blocking: bool) -> Result<FileLock, String> {
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

#[cfg(not(any(unix, windows)))]
fn exclusive_lock(_path: &Path, _blocking: bool) -> Result<(), String> {
    Err("detached Job locking is unsupported on this platform".to_string())
}

#[cfg(unix)]
fn write_lock_identity(lock: &mut FileLock, creation_id: &str) -> Result<(), String> {
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

#[cfg(windows)]
fn write_lock_identity(lock: &mut FileLock, creation_id: &str) -> Result<(), String> {
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

#[cfg(unix)]
fn lifetime_lock_is_held(path: &Path, expected_creation_id: &str) -> Result<bool, String> {
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

#[cfg(any(unix, windows))]
fn handoff_first_platform(
    store: &DetachedJobStore,
    request: DetachedStartRequest,
    prepared: DetachedJobRecord,
) -> Result<DetachedHandoffOutcome, String> {
    let job_dir = store.job_dir(&prepared.job_id);
    let supervisor_birth = format!("birth_{}", Uuid::new_v4().simple());
    let mut command = match internal_mode_command(
        DETACHED_INTERNAL_SUPERVISOR,
        &[
            job_dir.to_string_lossy().into_owned(),
            prepared.execution_id.clone(),
            supervisor_birth,
        ],
    ) {
        Ok(command) => command,
        Err(error) => {
            mark_pre_accept_failure(store, &prepared, &error)?;
            return Err(error);
        }
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    make_new_session(&mut command);
    #[cfg(windows)]
    command.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    let mut child = match command.spawn() {
        Ok(child) => child,
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
fn resolved_handoff_outcome(
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
fn wait_for_accepted_state(
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
fn mark_pre_accept_failure(
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
fn cleanup_pre_accept_supervisor(
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
fn spawn_supervisor_reaper(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(any(unix, windows))]
fn write_launch_frame(writer: &mut impl Write, spec: &DetachedLaunchSpec) -> Result<(), String> {
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
fn read_launch_frame(reader: &mut impl Read) -> Result<DetachedLaunchSpec, String> {
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
fn spawn_byte_reader(mut reader: impl Read + Send + 'static) -> mpsc::Receiver<u8> {
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

#[cfg(unix)]
fn make_new_session(command: &mut Command) {
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

#[cfg(unix)]
fn run_internal_platform_mode(args: &[String]) -> Result<(), String> {
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

#[cfg(windows)]
fn run_internal_platform_mode(args: &[String]) -> Result<(), String> {
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

#[cfg(any(unix, windows))]
fn run_supervisor(
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

#[cfg(unix)]
fn run_accepted_payload(
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

#[cfg(windows)]
fn run_accepted_payload(
    store: &DetachedJobStore,
    accepted: &DetachedJobRecord,
    launch: DetachedLaunchSpec,
) -> Result<DetachedJobRecord, String> {
    let tree_birth = format!("birth_{}", Uuid::new_v4().simple());
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

#[cfg(any(unix, windows))]
#[derive(Debug)]
enum OutputEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    StdoutEof,
    StderrEof,
}

#[cfg(any(unix, windows))]
fn spawn_output_reader(
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

#[cfg(unix)]
fn spawn_watchdog(
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

#[cfg(unix)]
fn run_watchdog(job_dir: &Path, creation_id: &str, execution_id: &str) -> Result<(), String> {
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

#[cfg(unix)]
fn write_supervisor_handshake(bytes: &[u8]) -> Result<(), String> {
    write_control_fd(2, bytes)
}

#[cfg(windows)]
fn write_supervisor_handshake(bytes: &[u8]) -> Result<(), String> {
    let mut stderr = io::stderr().lock();
    stderr
        .write_all(bytes)
        .and_then(|_| stderr.flush())
        .map_err(|error| format!("detached supervisor handshake write failed: {error}"))
}

#[cfg(unix)]
fn write_control_fd(fd: i32, mut bytes: &[u8]) -> Result<(), String> {
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

#[cfg(unix)]
fn read_line_with_timeout(
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

#[cfg(any(unix, windows))]
fn internal_mode_command(mode: &str, args: &[String]) -> Result<Command, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::sync::{Mutex, OnceLock};

    static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    #[cfg(windows)]
    #[test]
    fn windows_atomic_state_replace_retries_transient_destination_sharing() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state.json");
        fs::write(&state, br#"{"value":1}"#).unwrap();
        let blocker = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&state)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(blocker);
        });

        atomic_write_json(&state, &serde_json::json!({"value": 2}), 1024).unwrap();
        release.join().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
        assert_eq!(value["value"], 2);
    }

    #[test]
    fn internal_mode_subprocess_entrypoint() {
        let Some(mode) = std::env::var_os("WEBCODEX_DETACHED_TEST_INTERNAL_MODE") else {
            return;
        };
        let args: Vec<String> = serde_json::from_str(
            &std::env::var("WEBCODEX_DETACHED_TEST_INTERNAL_ARGS").expect("internal args"),
        )
        .expect("decode internal args");
        let mut full = vec![mode.to_string_lossy().into_owned()];
        full.extend(args);
        let code = match run_internal_platform_mode(&full) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("detached test internal mode failed: {error}");
                1
            }
        };
        std::process::exit(code);
    }

    fn safe_context() -> ShellJobContext {
        ShellJobContext {
            runtime_project_id: Some("agent:test:project".to_string()),
            workflow_session_id: Some("wc_sess_test".to_string()),
            ssh_resource: None,
            project_cwd: Some("/tmp/project".to_string()),
            cwd: Some("/tmp/project".to_string()),
            purpose: Some("test".to_string()),
            shell: None,
            command_preview: "native test process".to_string(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: None,
        }
    }

    fn test_request(executable: String, args: Vec<String>) -> DetachedStartRequest {
        DetachedStartRequest {
            job_id: format!("job_{}", Uuid::new_v4().simple()),
            request_id: format!("req_{}", Uuid::new_v4().simple()),
            client_id: "test-runner".to_string(),
            agent_instance_id: "test-instance".to_string(),
            context: safe_context(),
            launch: DetachedLaunchSpec {
                process: ShellProcessArgv { executable, args },
                cwd: None,
                stdin: None,
                env: Vec::new(),
                timeout_secs: 10,
            },
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn pre_accept_failure_advances_beyond_public_agent_queued_sequence() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = test_request("never-started".to_string(), Vec::new());
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            PrepareOutcome::Existing(_) => panic!("fresh detached prepare unexpectedly existed"),
        };
        assert_eq!(prepared.phase, DetachedJobPhase::Prepared);
        assert_eq!(prepared.update_seq, 1);

        let terminal = mark_pre_accept_failure(&store, &prepared, "preaccept blocked").unwrap();
        assert_eq!(terminal.phase, DetachedJobPhase::Terminal);
        assert_eq!(terminal.update_seq, 2);
        assert!(terminal.ownership_accepted_at_unix_ms.is_none());
        assert!(terminal.supervisor.is_none());
        assert!(terminal.tree_leader.is_none());
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "handoff_failed");

        let snapshot = snapshot_from_detached_record(&terminal).unwrap();
        assert_eq!(snapshot.update_seq, 2);
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.command_execution_state,
            Some(ShellCommandExecutionState::NotStarted)
        );
    }

    #[test]
    fn durable_record_rejects_mixed_or_oversized_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = test_request("/bin/true".to_string(), Vec::new());
        let record = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            PrepareOutcome::Existing(_) => panic!("unexpected existing state"),
        };
        let state_path = store.state_path_for_job(&request.job_id);
        fs::write(&state_path, b"not-json").unwrap();
        assert!(store.read(&request.job_id).unwrap_err().contains("corrupt"));

        atomic_write_json(&state_path, &record, DETACHED_STATE_MAX_BYTES).unwrap();
        let mut stale = record.clone();
        stale.schema_version += 1;
        atomic_write_json(&state_path, &stale, DETACHED_STATE_MAX_BYTES).unwrap();
        assert!(store
            .read(&request.job_id)
            .unwrap_err()
            .contains("unsupported detached Job state schema"));
    }

    #[test]
    fn durable_record_never_contains_ephemeral_launch_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let sentinel = "wc-detached-private-sentinel-9f3c0a";
        let digest = format!("{:x}", Sha256::digest(sentinel.as_bytes()));
        let mut request = test_request(format!("/tmp/{sentinel}"), vec![sentinel.to_string()]);
        request.launch.stdin = Some(sentinel.to_string());
        request
            .launch
            .env
            .push(("PRIVATE_TOKEN".to_string(), sentinel.to_string()));
        let _ = store.prepare(&request).unwrap();
        let bytes = fs::read(store.state_path_for_job(&request.job_id)).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains(sentinel),
            "durable state leaked launch body: {text}"
        );
        assert!(
            !text.contains(&digest),
            "durable state leaked launch-body digest: {text}"
        );
        assert!(!text.contains("PRIVATE_TOKEN"));
    }

    #[test]
    fn durable_state_root_has_a_hard_record_count_bound() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        for _ in 0..DETACHED_STATE_MAX_RECORDS {
            let request = test_request("/bin/true".to_string(), Vec::new());
            assert!(matches!(
                store.prepare(&request).unwrap(),
                PrepareOutcome::First(_)
            ));
        }
        let request = test_request("/bin/true".to_string(), Vec::new());
        let error = store.prepare(&request).unwrap_err();
        assert!(error.contains("state root is full"), "{error}");
        assert_eq!(
            store.scan_for_client("test-runner").unwrap().len(),
            DETACHED_STATE_MAX_RECORDS
        );
    }

    fn terminal_record(store: &DetachedJobStore) -> DetachedStartRequest {
        let request = test_request("/bin/true".to_string(), Vec::new());
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            PrepareOutcome::Existing(_) => panic!("fresh terminal fixture unexpectedly existed"),
        };
        store
            .update(&request.job_id, &prepared.execution_id, |record| {
                set_terminal(
                    record,
                    "completed",
                    Some(0),
                    None,
                    record.created_at_unix_ms,
                );
                Ok(())
            })
            .unwrap();
        request
    }

    #[test]
    fn terminal_reclamation_respects_retention_window_and_then_deletes() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = terminal_record(&store);
        let completed = store
            .read(&request.job_id)
            .unwrap()
            .terminal
            .unwrap()
            .completed_at_unix_ms;

        assert_eq!(
            store
                .reclaim_expired_terminal_records_locked(completed + TERMINAL_RETENTION_MS - 1)
                .unwrap(),
            1
        );
        assert!(store.read(&request.job_id).is_ok());

        assert_eq!(
            store
                .reclaim_expired_terminal_records_locked(completed + TERMINAL_RETENTION_MS)
                .unwrap(),
            0
        );
        assert!(store.read(&request.job_id).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn accepted_active_record_is_never_reclaimed() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("count_once", Vec::new());
        let outcome = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(outcome, DetachedHandoffOutcome::Accepted { .. }));
        let running = store.read(&request.job_id).unwrap();
        assert!(running.ownership_accepted_at_unix_ms.is_some());
        assert_ne!(running.phase, DetachedJobPhase::Terminal);

        assert_eq!(
            store
                .reclaim_expired_terminal_records_locked(i64::MAX)
                .unwrap(),
            1
        );
        assert_eq!(
            store.read(&request.job_id).unwrap().execution_id,
            running.execution_id
        );
        store
            .request_stop(&request.job_id, &running.execution_id)
            .unwrap();
        let _ = wait_for_terminal(&store, &request.job_id);
    }

    #[cfg(unix)]
    #[test]
    fn reclamation_fails_closed_on_corrupt_or_symlink_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = terminal_record(&store);
        let job_dir = store.job_dir(&request.job_id);
        let completed = store
            .read(&request.job_id)
            .unwrap()
            .terminal
            .unwrap()
            .completed_at_unix_ms;
        let unexpected_target = temp.path().join("do-not-delete");
        fs::write(&unexpected_target, b"sentinel").unwrap();
        std::os::unix::fs::symlink(&unexpected_target, job_dir.join("unexpected-link")).unwrap();

        let error = store
            .reclaim_expired_terminal_records_locked(completed + TERMINAL_RETENTION_MS)
            .unwrap_err();
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(fs::read(&unexpected_target).unwrap(), b"sentinel");
        assert!(job_dir.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prepared_restart_residue_converges_to_not_started_without_payload() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let marker = temp.path().join("must-not-run");
        let request = make_payload_request(
            "count_once",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            PrepareOutcome::Existing(_) => panic!("fresh pre-accept fixture unexpectedly existed"),
        };
        let reconciled = store
            .reconcile_after_runner_restart(prepared)
            .unwrap()
            .unwrap();
        assert_eq!(reconciled.phase, DetachedJobPhase::Terminal);
        assert_eq!(
            reconciled.terminal.as_ref().unwrap().status,
            "handoff_failed"
        );
        assert_eq!(
            snapshot_from_detached_record(&reconciled)
                .unwrap()
                .command_execution_state,
            Some(ShellCommandExecutionState::NotStarted)
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !marker.exists(),
            "pre-accept recovery must never spawn the payload"
        );
    }

    #[test]
    fn expired_terminal_record_releases_capacity_for_new_prepare() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let expired = terminal_record(&store);
        let mut expired_record = store.read(&expired.job_id).unwrap();
        expired_record
            .terminal
            .as_mut()
            .unwrap()
            .completed_at_unix_ms = unix_ms().saturating_sub(TERMINAL_RETENTION_MS + 1);
        atomic_write_json(
            &store.state_path_for_job(&expired.job_id),
            &expired_record,
            DETACHED_STATE_MAX_BYTES,
        )
        .unwrap();

        for _ in 1..DETACHED_STATE_MAX_RECORDS {
            let request = test_request("/bin/true".to_string(), Vec::new());
            assert!(matches!(
                store.prepare(&request).unwrap(),
                PrepareOutcome::First(_)
            ));
        }
        let replacement = test_request("/bin/true".to_string(), Vec::new());
        assert!(matches!(
            store.prepare(&replacement).unwrap(),
            PrepareOutcome::First(_)
        ));
        assert!(store.read(&expired.job_id).is_err());
        assert!(store.read(&replacement.job_id).is_ok());
        assert_eq!(
            store.scan_for_client("test-runner").unwrap().len(),
            DETACHED_STATE_MAX_RECORDS
        );
    }

    #[test]
    fn duplicate_prepare_keeps_one_execution_identity() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = test_request("/bin/true".to_string(), Vec::new());
        let first = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("first prepare must claim"),
        };
        let second = match store.prepare(&request).unwrap() {
            PrepareOutcome::Existing(record) => record,
            _ => panic!("second prepare must reuse durable claim"),
        };
        assert_eq!(first.execution_id, second.execution_id);
        assert_eq!(first, second);
    }

    #[test]
    fn output_tail_is_bounded_while_total_bytes_and_line_cursors_continue() {
        let mut output = DetachedOutputState::default();
        let text = "line\n".repeat(JOB_SNAPSHOT_STREAM_MAX_BYTES / 5 + 1024);
        append_output_tail(&mut output, text.len(), &text);
        assert_eq!(output.total_bytes, text.len() as u64);
        assert!(output.retained_bytes <= JOB_SNAPSHOT_STREAM_MAX_BYTES);
        assert_eq!(output.retained_bytes, output.tail.len());
        assert!(output.first_retained_line > 1);
        assert_eq!(output.next_line, 1 + text.lines().count());
        assert!(output.truncated);
        validate_output_state("stdout", &output).unwrap();

        let previous_next = output.next_line;
        append_output_tail(&mut output, 5, "tail\n");
        assert_eq!(output.next_line, previous_next + 1);
        validate_output_state("stdout", &output).unwrap();
    }

    #[test]
    fn durable_record_bound_covers_worst_case_escaped_output_tails() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = test_request("/bin/true".to_string(), Vec::new());
        let mut record = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("first prepare must claim"),
        };
        let escaped_tail = "\0".repeat(JOB_SNAPSHOT_STREAM_MAX_BYTES);
        record.stdout.tail = escaped_tail.clone();
        record.stdout.retained_bytes = record.stdout.tail.len();
        record.stdout.total_bytes = record.stdout.tail.len() as u64;
        record.stdout.first_retained_line = 17;
        record.stdout.next_line = 18;
        record.stdout.truncated = true;
        record.stderr.tail = escaped_tail;
        record.stderr.retained_bytes = record.stderr.tail.len();
        record.stderr.total_bytes = record.stderr.tail.len() as u64;
        record.stderr.first_retained_line = 23;
        record.stderr.next_line = 24;
        record.stderr.truncated = true;
        validate_record(&record).unwrap();
        let encoded = serde_json::to_vec(&record).unwrap();
        assert!(encoded.len() < DETACHED_STATE_MAX_BYTES);
        atomic_write_json(
            &store.state_path_for_job(&request.job_id),
            &record,
            DETACHED_STATE_MAX_BYTES,
        )
        .unwrap();
        assert_eq!(store.read(&request.job_id).unwrap(), record);
    }

    #[cfg(unix)]
    #[test]
    fn durable_state_and_lock_paths_reject_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let real_root = temp.path().join("real-root");
        fs::create_dir(&real_root).unwrap();
        let state_link = temp.path().join("state-link");
        std::os::unix::fs::symlink(&real_root, &state_link).unwrap();
        let error = ensure_private_dir(&state_link).unwrap_err();
        assert!(error.contains("not a symlink"), "{error}");

        let target = temp.path().join("lock-target");
        fs::write(&target, b"do-not-touch").unwrap();
        let lock_link = temp.path().join("lock-link");
        std::os::unix::fs::symlink(&target, &lock_link).unwrap();
        assert!(exclusive_lock(&lock_link, false).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"do-not-touch");
    }

    #[cfg(unix)]
    #[test]
    fn stale_watchdog_invocation_fails_before_tree_lock_creation() {
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = test_request("/bin/true".to_string(), Vec::new());
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("first prepare must claim"),
        };
        let job_dir = store.job_dir(&request.job_id);
        let error = run_watchdog(
            &job_dir,
            &format!("birth_{}", Uuid::new_v4().simple()),
            &prepared.execution_id,
        )
        .unwrap_err();
        assert!(
            error.contains("stale detached watchdog invocation"),
            "{error}"
        );
        assert!(!job_dir.join(TREE_LOCK_FILE).exists());
    }

    #[cfg(unix)]
    fn process_alive(pid: u32) -> bool {
        // SAFETY: kill(pid, 0) performs a liveness/permission probe only.
        let rc = unsafe { libc::kill(pid as i32, 0) };
        rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(unix)]
    fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        predicate()
    }

    #[cfg(unix)]
    fn wait_for_terminal(store: &DetachedJobStore, job_id: &str) -> DetachedJobRecord {
        assert!(wait_until(Duration::from_secs(15), || {
            store
                .read(job_id)
                .is_ok_and(|record| record.phase == DetachedJobPhase::Terminal)
        }));
        store.read(job_id).unwrap()
    }

    #[cfg(unix)]
    fn payload_command(
        scenario: &str,
        env: Vec<(String, String)>,
    ) -> (String, Vec<String>, Vec<(String, String)>) {
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let args = vec![
            "--exact".to_string(),
            "webcodex_runner::detached_job::tests::payload_subprocess_entrypoint".to_string(),
            "--nocapture".to_string(),
        ];
        let mut all_env = env;
        all_env.push((
            "WEBCODEX_DETACHED_PAYLOAD_SCENARIO".to_string(),
            scenario.to_string(),
        ));
        (executable, args, all_env)
    }

    #[cfg(unix)]
    #[test]
    fn payload_subprocess_entrypoint() {
        let Some(scenario) = std::env::var_os("WEBCODEX_DETACHED_PAYLOAD_SCENARIO") else {
            return;
        };
        match scenario.to_string_lossy().as_ref() {
            "delayed_marker" => {
                let marker = PathBuf::from(std::env::var_os("PAYLOAD_MARKER").unwrap());
                std::thread::sleep(Duration::from_millis(400));
                fs::write(marker, b"done").unwrap();
            }
            "count_once" => {
                let marker = PathBuf::from(std::env::var_os("PAYLOAD_MARKER").unwrap());
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(marker)
                    .unwrap();
                writeln!(file, "run").unwrap();
                file.flush().unwrap();
                std::thread::sleep(Duration::from_secs(2));
            }
            "output_flood" => {
                let stdout = vec![b'o'; JOB_SNAPSHOT_STREAM_MAX_BYTES * 5];
                let stderr = vec![b'e'; JOB_SNAPSHOT_STREAM_MAX_BYTES * 4];
                io::stdout().write_all(&stdout).unwrap();
                io::stdout().write_all(b"STDOUT_END\n").unwrap();
                io::stdout().flush().unwrap();
                io::stderr().write_all(&stderr).unwrap();
                io::stderr().write_all(b"STDERR_END\n").unwrap();
                io::stderr().flush().unwrap();
            }
            "tree" => {
                let parent_marker = PathBuf::from(std::env::var_os("PARENT_PID_MARKER").unwrap());
                let child_marker = PathBuf::from(std::env::var_os("CHILD_PID_MARKER").unwrap());
                fs::write(&parent_marker, std::process::id().to_string()).unwrap();
                let mut child = Command::new(std::env::current_exe().unwrap());
                child
                    .arg("--exact")
                    .arg("webcodex_runner::detached_job::tests::payload_descendant_subprocess_entrypoint")
                    .arg("--nocapture")
                    .env_clear()
                    .env("WEBCODEX_DETACHED_DESCENDANT_MARKER", child_marker)
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());
                #[allow(clippy::zombie_processes)]
                let _child = child.spawn().unwrap();
                std::thread::sleep(Duration::from_secs(60));
            }
            other => panic!("unknown detached payload scenario: {other}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn payload_descendant_subprocess_entrypoint() {
        let Some(marker) = std::env::var_os("WEBCODEX_DETACHED_DESCENDANT_MARKER") else {
            return;
        };
        fs::write(marker, std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(60));
    }

    #[cfg(unix)]
    fn make_payload_request(scenario: &str, env: Vec<(String, String)>) -> DetachedStartRequest {
        let (executable, args, env) = payload_command(scenario, env);
        let mut request = test_request(executable, args);
        request.launch.env = env;
        request
    }

    #[cfg(unix)]
    #[test]
    fn accepted_handoff_keeps_payload_alive_after_owner_process_exits() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let state_root = temp.path().join("state");
        let marker = temp.path().join("marker");
        let request = make_payload_request(
            "delayed_marker",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let instruction = temp.path().join("owner.json");
        let result_path = temp.path().join("owner-result.json");
        fs::write(
            &instruction,
            serde_json::to_vec(&(state_root.clone(), request.clone(), result_path.clone()))
                .unwrap(),
        )
        .unwrap();
        let mut owner = Command::new(std::env::current_exe().unwrap());
        owner
            .arg("--exact")
            .arg("webcodex_runner::detached_job::tests::owner_subprocess_entrypoint")
            .arg("--nocapture")
            .env_clear()
            .env("WEBCODEX_DETACHED_OWNER_INSTRUCTION", &instruction)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = owner.spawn().unwrap().wait().unwrap();
        assert!(status.success());
        assert!(result_path.exists());
        assert!(wait_until(Duration::from_secs(5), || marker.exists()));
        let store = DetachedJobStore::new(state_root);
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "completed");
    }

    #[cfg(unix)]
    #[test]
    fn owner_subprocess_entrypoint() {
        let Some(path) = std::env::var_os("WEBCODEX_DETACHED_OWNER_INSTRUCTION") else {
            return;
        };
        let (root, request, result_path): (PathBuf, DetachedStartRequest, PathBuf) =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let store = DetachedJobStore::new(root);
        let outcome = handoff_detached_job(&store, request).unwrap();
        fs::write(result_path, format!("{outcome:?}")).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn accepted_handoff_survives_owner_exit_before_ack() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let state_root = temp.path().join("state");
        let marker = temp.path().join("count");
        let request = make_payload_request(
            "count_once",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let instruction = temp.path().join("accept-exit-owner.json");
        fs::write(
            &instruction,
            serde_json::to_vec(&(state_root.clone(), request.clone())).unwrap(),
        )
        .unwrap();

        let mut owner = Command::new(std::env::current_exe().unwrap());
        owner
            .arg("--exact")
            .arg("webcodex_runner::detached_job::tests::accept_then_exit_owner_subprocess_entrypoint")
            .arg("--nocapture")
            .env_clear()
            .env("WEBCODEX_DETACHED_ACCEPT_EXIT_INSTRUCTION", &instruction)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(owner.spawn().unwrap().wait().unwrap().success());

        let store = DetachedJobStore::new(state_root);
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert!(terminal.ownership_accepted_at_unix_ms.is_some());
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "completed");
        let replay = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(
            replay,
            DetachedHandoffOutcome::Accepted {
                reconciled_from_state: true,
                ..
            }
        ));
        let runs = fs::read_to_string(marker).unwrap();
        assert_eq!(runs.lines().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn accept_then_exit_owner_subprocess_entrypoint() {
        let Some(path) = std::env::var_os("WEBCODEX_DETACHED_ACCEPT_EXIT_INSTRUCTION") else {
            return;
        };
        let (root, request): (PathBuf, DetachedStartRequest) =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let store = DetachedJobStore::new(root);
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("accept-exit owner expected a fresh durable claim"),
        };
        let job_dir = store.job_dir(&request.job_id);
        let supervisor_birth = format!("birth_{}", Uuid::new_v4().simple());
        let mut command = internal_mode_command(
            DETACHED_INTERNAL_SUPERVISOR,
            &[
                job_dir.to_string_lossy().into_owned(),
                prepared.execution_id.clone(),
                supervisor_birth,
            ],
        )
        .unwrap();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        make_new_session(&mut command);
        let mut child = command.spawn().unwrap();
        let mut child_stdin = child.stdin.take().unwrap();
        let child_stderr = child.stderr.take().unwrap();
        let handshake = spawn_byte_reader(child_stderr);
        write_launch_frame(&mut child_stdin, &request.launch).unwrap();
        assert_eq!(
            handshake.recv_timeout(DETACHED_HANDOFF_TIMEOUT).unwrap(),
            HANDSHAKE_READY
        );
        child_stdin.write_all(&[HANDSHAKE_ACCEPT]).unwrap();
        child_stdin.flush().unwrap();
        // Exit without reading HANDSHAKE_ACCEPTED or reaping the direct
        // supervisor. This simulates lost Runner response/ownership after the
        // commit byte has been sent.
        std::process::exit(0);
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_handoff_never_spawns_a_second_payload() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let marker = temp.path().join("count");
        let request = make_payload_request(
            "count_once",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let first = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(first, DetachedHandoffOutcome::Accepted { .. }));
        let second = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(
            second,
            DetachedHandoffOutcome::Accepted {
                reconciled_from_state: true,
                ..
            }
        ));
        let _ = wait_for_terminal(&store, &request.job_id);
        let runs = fs::read_to_string(marker).unwrap();
        assert_eq!(runs.lines().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn durable_update_sequence_advances_and_duplicate_handoff_does_not() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("count_once", Vec::new());
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        let running = store.read(&request.job_id).unwrap();
        assert!(running.update_seq >= 3);
        let sequence = running.update_seq;
        let replay = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(replay, DetachedHandoffOutcome::Accepted { .. }));
        assert_eq!(store.read(&request.job_id).unwrap().update_seq, sequence);
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert!(terminal.update_seq > sequence);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn restart_scan_reconciles_live_detached_execution_without_respawn() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let state_root = temp.path().join("state");
        let marker = temp.path().join("count");
        let request = make_payload_request(
            "count_once",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let instruction = temp.path().join("restart-owner.json");
        fs::write(
            &instruction,
            serde_json::to_vec(&(state_root.clone(), request.clone())).unwrap(),
        )
        .unwrap();
        let mut owner = Command::new(std::env::current_exe().unwrap());
        owner
            .arg("--exact")
            .arg("webcodex_runner::detached_job::tests::accept_then_exit_owner_subprocess_entrypoint")
            .arg("--nocapture")
            .env_clear()
            .env("WEBCODEX_DETACHED_ACCEPT_EXIT_INSTRUCTION", &instruction)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(owner.spawn().unwrap().wait().unwrap().success());

        let store = DetachedJobStore::new(state_root);
        assert!(wait_until(Duration::from_secs(5), || store
            .read(&request.job_id)
            .is_ok_and(|record| record.phase == DetachedJobPhase::Running)));
        let records = store.scan_for_client(&request.client_id).unwrap();
        assert_eq!(records.len(), 1);
        let recovered = store
            .reconcile_after_runner_restart(records.into_iter().next().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(recovered.job_id, request.job_id);
        assert_eq!(recovered.phase, DetachedJobPhase::Running);
        let snapshot = snapshot_from_detached_record(&recovered).unwrap();
        assert_eq!(snapshot.status, "running");
        assert_eq!(snapshot.update_seq, recovered.update_seq);
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "completed");
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn durable_stop_request_terminates_exact_supervisor_owned_tree() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let parent_marker = temp.path().join("parent.pid");
        let child_marker = temp.path().join("child.pid");
        let request = make_payload_request(
            "tree",
            vec![
                (
                    "PARENT_PID_MARKER".to_string(),
                    parent_marker.to_string_lossy().into_owned(),
                ),
                (
                    "CHILD_PID_MARKER".to_string(),
                    child_marker.to_string_lossy().into_owned(),
                ),
            ],
        );
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(wait_until(Duration::from_secs(5), || {
            parent_marker.exists() && child_marker.exists()
        }));
        let parent_pid: u32 = fs::read_to_string(&parent_marker).unwrap().parse().unwrap();
        let child_pid: u32 = fs::read_to_string(&child_marker).unwrap().parse().unwrap();
        let running = store.read(&request.job_id).unwrap();
        let stopped = store
            .request_stop(&request.job_id, &running.execution_id)
            .unwrap();
        assert!(stopped.stop_requested);
        assert_eq!(stopped.update_seq, running.update_seq + 1);
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "stopped");
        assert!(terminal.update_seq > stopped.update_seq);
        assert!(wait_until(Duration::from_secs(5), || !process_alive(
            parent_pid
        )));
        assert!(wait_until(Duration::from_secs(5), || !process_alive(
            child_pid
        )));
        let snapshot = snapshot_from_detached_record(&terminal).unwrap();
        assert_eq!(snapshot.status, "stopped");
        assert_eq!(
            snapshot.command_execution_state,
            Some(ShellCommandExecutionState::Completed)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stale_native_supervisor_identity_reconciles_to_lost_without_respawn() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("delayed_marker", Vec::new());
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(wait_until(Duration::from_secs(5), || store
            .read(&request.job_id)
            .is_ok_and(|record| record.phase == DetachedJobPhase::Running)));
        let mut running = store.read(&request.job_id).unwrap();
        let real_supervisor_pid = running.supervisor.as_ref().unwrap().pid;
        running.supervisor.as_mut().unwrap().native_start_id = "linux_start_0".to_string();
        atomic_write_json(
            &store.state_path_for_job(&request.job_id),
            &running,
            DETACHED_STATE_MAX_BYTES,
        )
        .unwrap();
        let lost = store
            .reconcile_after_runner_restart(running)
            .unwrap()
            .unwrap();
        assert_eq!(lost.phase, DetachedJobPhase::Terminal);
        assert_eq!(lost.terminal.as_ref().unwrap().status, "supervisor_lost");
        let snapshot = snapshot_from_detached_record(&lost).unwrap();
        assert_eq!(snapshot.status, "lost");
        assert_eq!(
            snapshot.command_execution_state,
            Some(ShellCommandExecutionState::OutcomeUnknown)
        );
        // The replacement reconciler never signals by numeric PID. Clean up the
        // deliberately still-live test supervisor only after the assertion.
        unsafe {
            libc::kill(real_supervisor_pid as i32, libc::SIGKILL);
        }
        assert!(wait_until(Duration::from_secs(5), || !process_alive(
            real_supervisor_pid
        )));
    }

    #[cfg(unix)]
    #[test]
    fn supervisor_continuously_drains_and_bounds_both_output_streams() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("output_flood", Vec::new());
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert!(terminal.stdout.total_bytes > (JOB_SNAPSHOT_STREAM_MAX_BYTES * 4) as u64);
        assert!(terminal.stderr.total_bytes > (JOB_SNAPSHOT_STREAM_MAX_BYTES * 3) as u64);
        assert!(terminal.stdout.truncated);
        assert!(terminal.stderr.truncated);
        assert!(terminal.stdout.retained_bytes <= JOB_SNAPSHOT_STREAM_MAX_BYTES);
        assert!(terminal.stderr.retained_bytes <= JOB_SNAPSHOT_STREAM_MAX_BYTES);
        assert!(terminal.stdout.tail.contains("STDOUT_END"));
        assert!(terminal.stderr.tail.contains("STDERR_END"));
    }

    #[cfg(unix)]
    #[test]
    fn terminal_state_is_atomically_rereadable() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("output_flood", Vec::new());
        let job_id = request.job_id.clone();
        let reader_store = store.clone();
        let reader = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut observed_state = false;
            loop {
                match reader_store.read(&job_id) {
                    Ok(record) => {
                        observed_state = true;
                        if record.phase == DetachedJobPhase::Terminal {
                            return record;
                        }
                    }
                    Err(error) if !observed_state && error.contains("No such file") => {}
                    Err(error) => panic!("atomic state read after first commit failed: {error}"),
                }
                assert!(Instant::now() < deadline, "terminal state timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        let observed = reader.join().unwrap();
        let reread = store.read(&request.job_id).unwrap();
        assert_eq!(observed, reread);
        assert_eq!(reread.phase, DetachedJobPhase::Terminal);
    }

    #[cfg(target_os = "linux")]
    fn linux_child_pids(pid: u32) -> Vec<u32> {
        let mut children = Vec::new();
        let tasks = match fs::read_dir(format!("/proc/{pid}/task")) {
            Ok(tasks) => tasks,
            Err(_) => return children,
        };
        for task in tasks.flatten() {
            let path = task.path().join("children");
            children.extend(
                fs::read_to_string(path)
                    .unwrap_or_default()
                    .split_whitespace()
                    .filter_map(|value| value.parse::<u32>().ok()),
            );
        }
        children.sort_unstable();
        children.dedup();
        children
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pre_accept_owner_disconnect_is_terminal_and_replay_never_spawns() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let marker = temp.path().join("must-not-run");
        let request = make_payload_request(
            "delayed_marker",
            vec![(
                "PAYLOAD_MARKER".to_string(),
                marker.to_string_lossy().into_owned(),
            )],
        );
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("first prepare"),
        };
        let job_dir = store.job_dir(&request.job_id);
        let supervisor_birth = format!("birth_{}", Uuid::new_v4().simple());
        let mut command = internal_mode_command(
            DETACHED_INTERNAL_SUPERVISOR,
            &[
                job_dir.to_string_lossy().into_owned(),
                prepared.execution_id.clone(),
                supervisor_birth,
            ],
        )
        .unwrap();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        make_new_session(&mut command);
        let mut child = command.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let handshake = spawn_byte_reader(stderr);
        write_launch_frame(&mut stdin, &request.launch).unwrap();
        assert_eq!(
            handshake.recv_timeout(DETACHED_HANDOFF_TIMEOUT).unwrap(),
            HANDSHAKE_READY
        );
        let children = linux_child_pids(child.id());
        assert!(
            children.is_empty(),
            "pre-accept supervisor must not start a tree helper or payload"
        );
        drop(stdin);
        assert!(wait_until(Duration::from_secs(5), || child
            .try_wait()
            .unwrap()
            .is_some()));
        let terminal = wait_for_terminal(&store, &request.job_id);
        assert!(terminal.ownership_accepted_at_unix_ms.is_none());
        assert!(terminal.tree_leader.is_none());
        assert_eq!(terminal.terminal.as_ref().unwrap().status, "handoff_failed");
        assert!(!marker.exists());

        let replay = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(matches!(
            replay,
            DetachedHandoffOutcome::PreAcceptFailed { .. }
        ));
        std::thread::sleep(Duration::from_millis(500));
        assert!(!marker.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pre_accept_supervisor_death_leaves_no_internal_or_payload_orphan() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let request = make_payload_request("delayed_marker", Vec::new());
        let prepared = match store.prepare(&request).unwrap() {
            PrepareOutcome::First(record) => record,
            _ => panic!("first prepare"),
        };
        let job_dir = store.job_dir(&request.job_id);
        let supervisor_birth = format!("birth_{}", Uuid::new_v4().simple());
        let mut command = internal_mode_command(
            DETACHED_INTERNAL_SUPERVISOR,
            &[
                job_dir.to_string_lossy().into_owned(),
                prepared.execution_id.clone(),
                supervisor_birth,
            ],
        )
        .unwrap();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        make_new_session(&mut command);
        let mut child = command.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let handshake = spawn_byte_reader(stderr);
        write_launch_frame(&mut stdin, &request.launch).unwrap();
        assert_eq!(
            handshake.recv_timeout(DETACHED_HANDOFF_TIMEOUT).unwrap(),
            HANDSHAKE_READY
        );
        let supervisor_pid = child.id();
        let children = linux_child_pids(supervisor_pid);
        assert!(
            children.is_empty(),
            "pre-accept supervisor must not start a tree helper or payload"
        );
        unsafe {
            libc::kill(supervisor_pid as i32, libc::SIGKILL);
        }
        assert!(wait_until(Duration::from_secs(5), || child
            .try_wait()
            .unwrap()
            .is_some()));
        let record = store.read(&request.job_id).unwrap();
        assert!(record.ownership_accepted_at_unix_ms.is_none());
        assert!(record.tree_leader.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn supervisor_death_terminates_payload_process_tree() {
        let _guard = test_env_lock();
        let temp = tempfile::tempdir().unwrap();
        let store = DetachedJobStore::new(temp.path().join("state"));
        let parent_marker = temp.path().join("parent.pid");
        let child_marker = temp.path().join("child.pid");
        let request = make_payload_request(
            "tree",
            vec![
                (
                    "PARENT_PID_MARKER".to_string(),
                    parent_marker.to_string_lossy().into_owned(),
                ),
                (
                    "CHILD_PID_MARKER".to_string(),
                    child_marker.to_string_lossy().into_owned(),
                ),
            ],
        );
        let _ = handoff_detached_job(&store, request.clone()).unwrap();
        assert!(wait_until(Duration::from_secs(5), || {
            parent_marker.exists() && child_marker.exists()
        }));
        let parent_pid: u32 = fs::read_to_string(&parent_marker).unwrap().parse().unwrap();
        let child_pid: u32 = fs::read_to_string(&child_marker).unwrap().parse().unwrap();
        let running = store.read(&request.job_id).unwrap();
        assert_eq!(running.phase, DetachedJobPhase::Running);
        let supervisor_pid = running.supervisor.as_ref().unwrap().pid;
        assert!(process_alive(supervisor_pid));
        assert!(process_alive(parent_pid));
        assert!(process_alive(child_pid));
        let job_dir = store.job_dir(&request.job_id);
        let supervisor_identity = running.supervisor.as_ref().unwrap();
        let tree_identity = running.tree_leader.as_ref().unwrap();
        assert_eq!(
            supervisor_identity.native_start_id,
            native_process_start_identity(supervisor_identity.pid).unwrap()
        );
        assert_eq!(
            tree_identity.native_start_id,
            native_process_start_identity(tree_identity.pid).unwrap()
        );
        assert!(lifetime_lock_is_held(
            &job_dir.join(SUPERVISOR_LOCK_FILE),
            &supervisor_identity.creation_id,
        )
        .unwrap());
        assert!(
            lifetime_lock_is_held(&job_dir.join(TREE_LOCK_FILE), &tree_identity.creation_id,)
                .unwrap()
        );
        unsafe {
            libc::kill(supervisor_pid as i32, libc::SIGKILL);
        }
        assert!(wait_until(Duration::from_secs(10), || !process_alive(
            parent_pid
        )));
        assert!(wait_until(Duration::from_secs(10), || !process_alive(
            child_pid
        )));
        assert!(wait_until(Duration::from_secs(10), || {
            !process_alive(running.tree_leader.as_ref().unwrap().pid)
        }));
        assert!(wait_until(Duration::from_secs(10), || {
            lifetime_lock_is_held(
                &job_dir.join(SUPERVISOR_LOCK_FILE),
                &supervisor_identity.creation_id,
            ) == Ok(false)
        }));
        assert!(wait_until(Duration::from_secs(10), || {
            lifetime_lock_is_held(&job_dir.join(TREE_LOCK_FILE), &tree_identity.creation_id)
                == Ok(false)
        }));
    }
}
