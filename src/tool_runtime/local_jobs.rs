//! Local job records and process-group termination support.

use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
#[cfg(test)]
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::helpers::{DEFAULT_JOB_LOG_TAIL_LINES, MAX_LOCAL_LOG_LINES};

pub(crate) const MAX_LOCAL_LOG_BYTES_PER_STREAM: usize = 1024 * 1024;
const LOCAL_LOG_READ_CHUNK_BYTES: usize = 64 * 1024;
const LOCAL_JOB_OBSERVATION_FILE: &str = "observation.json";

#[cfg(test)]
static BOUNDED_LOG_READ_COUNTS: LazyLock<Mutex<HashMap<PathBuf, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
static BOUNDED_LOG_READ_DELAYS: LazyLock<Mutex<HashMap<PathBuf, Duration>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LocalJobObservation {
    pub(crate) version: u8,
    pub(crate) epoch: String,
    pub(crate) revision: u64,
    pub(crate) status: String,
    pub(crate) stdout_len: u64,
    pub(crate) stderr_len: u64,
    pub(crate) terminal_generation: u64,
}
impl LocalJobObservation {
    pub(crate) fn terminal(&self) -> bool {
        super::jobs::is_terminal_job_status(&super::helpers::normalize_local_status(&self.status))
    }
    pub(crate) fn token(&self, job_id: &str) -> Result<String, String> {
        crate::job_observation::JobObservationToken::new_legacy(
            crate::job_observation::JobObservationExecutor::Local,
            job_id,
            self.epoch.clone(),
            self.revision,
        )
        .map(|token| token.encode())
        .map_err(|error| error.to_string())
    }

    pub(crate) fn token_with_cursors(
        &self,
        job_id: &str,
        stdout_cursor: usize,
        stderr_cursor: usize,
    ) -> Result<String, String> {
        crate::job_observation::JobObservationToken::new(
            crate::job_observation::JobObservationExecutor::Local,
            job_id,
            self.epoch.clone(),
            self.revision,
            stdout_cursor as u64,
            stderr_cursor as u64,
        )
        .map(|token| token.encode())
        .map_err(|error| error.to_string())
    }
}
#[derive(Debug, Clone)]
pub(crate) struct LocalJobRecord {
    pub(crate) project: String,
    pub(crate) dir: PathBuf,
    observation: Arc<Mutex<Option<LocalJobObservation>>>,
    visibility: Arc<AtomicU8>,
    terminal: Arc<AtomicBool>,
}
#[derive(Debug, Clone)]
pub(crate) struct LocalJobLogSnapshot {
    pub(crate) retained_text: String,
    pub(crate) total_lines: usize,
    pub(crate) first_retained_line: usize,
    pub(crate) truncated: bool,
}
impl LocalJobRecord {
    #[cfg(test)]
    pub(crate) fn new(project: String, dir: PathBuf) -> Self {
        let observation = read_observation(&dir).ok();
        let terminal = observation
            .as_ref()
            .is_some_and(LocalJobObservation::terminal);
        Self {
            project,
            dir,
            observation: Arc::new(Mutex::new(observation)),
            visibility: Arc::new(AtomicU8::new(0)),
            terminal: Arc::new(AtomicBool::new(terminal)),
        }
    }

    pub(crate) fn initialize(
        project: String,
        dir: PathBuf,
    ) -> Result<(Self, LocalJobObservation), String> {
        let observation = LocalJobObservation {
            version: 1,
            epoch: crate::job_observation::new_epoch(),
            revision: 0,
            status: "running".into(),
            stdout_len: 0,
            stderr_len: 0,
            terminal_generation: 0,
        };
        persist_observation(&dir, &observation)?;
        let record = Self {
            project,
            dir,
            observation: Arc::new(Mutex::new(Some(observation.clone()))),
            visibility: Arc::new(AtomicU8::new(0)),
            terminal: Arc::new(AtomicBool::new(false)),
        };
        Ok((record, observation))
    }

    pub(crate) fn initialize_hidden(
        project: String,
        dir: PathBuf,
    ) -> Result<(Self, LocalJobObservation), String> {
        let (record, observation) = Self::initialize(project, dir)?;
        record.visibility.store(1, Ordering::Release);
        Ok((record, observation))
    }

    pub(crate) fn is_public(&self) -> bool {
        self.visibility.load(Ordering::Acquire) == 0
    }

    pub(crate) fn promote_if_active(&self) -> Result<Option<LocalJobObservation>, String> {
        if self.terminal.load(Ordering::Acquire) {
            return Ok(None);
        }
        if self
            .visibility
            .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(None);
        }
        let observation = match self.observe() {
            Ok(observation) => observation,
            Err(error) => {
                let _ = self
                    .visibility
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
                return Err(error);
            }
        };
        if observation.terminal() || self.terminal.load(Ordering::Acquire) {
            let _ = self
                .visibility
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
            return Ok(None);
        }
        Ok(Some(observation))
    }

    pub(crate) fn mark_terminal(&self) {
        self.terminal.store(true, Ordering::Release);
    }

    pub(crate) fn is_terminal(&self) -> bool {
        self.terminal.load(Ordering::Acquire)
    }

    pub(crate) fn mark_cleanup_pending(&self) {
        self.visibility.store(2, Ordering::Release);
    }

    pub(crate) fn cleanup_pending(&self) -> bool {
        self.visibility.load(Ordering::Acquire) == 2
    }

    pub(crate) fn observe(&self) -> Result<LocalJobObservation, String> {
        let status = match std::fs::read_to_string(self.dir.join("status")) {
            Ok(status) => status,
            Err(error) => {
                return Err(format!(
                    "local_job_observation_read_failed: status: {error}"
                ));
            }
        };
        let stdout_len = match std::fs::metadata(self.dir.join("stdout.log")) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return Err(format!(
                    "local_job_observation_read_failed: stdout.log: {error}"
                ));
            }
        };
        let stderr_len = match std::fs::metadata(self.dir.join("stderr.log")) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return Err(format!(
                    "local_job_observation_read_failed: stderr.log: {error}"
                ));
            }
        };

        let mut guard = self.observation.lock().unwrap();
        if guard.is_none() {
            match read_observation(&self.dir) {
                Ok(observation) => *guard = Some(observation),
                Err(error) => {
                    return Err(error);
                }
            }
        }
        let current = guard.as_ref().unwrap();
        if current.status == status
            && current.stdout_len == stdout_len
            && current.stderr_len == stderr_len
            && current.terminal_generation == 0
        {
            return Ok(current.clone());
        }
        let next = LocalJobObservation {
            version: 1,
            epoch: current.epoch.clone(),
            revision: current.revision.saturating_add(1),
            status,
            stdout_len,
            stderr_len,
            terminal_generation: 0,
        };
        if let Err(error) = persist_observation(&self.dir, &next) {
            return Err(error);
        }
        *guard = Some(next.clone());
        Ok(next)
    }
    pub(crate) fn read_text(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(name)).ok()
    }
    pub(crate) fn read_log_lines(
        &self,
        name: &str,
        offset: Option<usize>,
        tail_lines: Option<usize>,
    ) -> (String, usize, usize, bool) {
        let snapshot_len = std::fs::metadata(self.dir.join(name))
            .map(|meta| meta.len())
            .unwrap_or(0);
        self.read_log_lines_at(name, offset, tail_lines, snapshot_len)
    }
    pub(crate) fn read_log_lines_at(
        &self,
        name: &str,
        offset: Option<usize>,
        tail_lines: Option<usize>,
        snapshot_len: u64,
    ) -> (String, usize, usize, bool) {
        self.read_log_snapshot_at(name, offset, snapshot_len)
            .map(|log| log.read_lines(offset, tail_lines))
            .unwrap_or_else(|| (String::new(), 1, 0, false))
    }

    pub(crate) fn read_log_snapshot_at(
        &self,
        name: &str,
        offset: Option<usize>,
        snapshot_len: u64,
    ) -> Option<LocalJobLogSnapshot> {
        read_bounded_log(&self.dir.join(name), offset, Some(snapshot_len)).ok()
    }
    pub(crate) fn read_json(&self, name: &str) -> Value {
        self.read_text(name)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    }
}
fn read_observation(dir: &Path) -> Result<LocalJobObservation, String> {
    let text = std::fs::read_to_string(dir.join(LOCAL_JOB_OBSERVATION_FILE))
        .map_err(|e| format!("local_job_observation_read_failed: {e}"))?;
    let observation: LocalJobObservation =
        serde_json::from_str(&text).map_err(|e| format!("local_job_observation_invalid: {e}"))?;
    if observation.version != 1 || observation.epoch.is_empty() {
        return Err("local_job_observation_invalid: unsupported version or empty epoch".into());
    }
    Ok(observation)
}
fn persist_observation(dir: &Path, observation: &LocalJobObservation) -> Result<(), String> {
    use std::io::Write;
    let encoded = serde_json::to_vec(observation)
        .map_err(|e| format!("local_job_observation_encode_failed: {e}"))?;
    let temporary = dir.join(format!(
        "observation.json.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        std::fs::rename(&temporary, dir.join(LOCAL_JOB_OBSERVATION_FILE))?;
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("local_job_observation_persist_failed: {error}"));
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn reset_bounded_log_read_count(path: &Path) {
    BOUNDED_LOG_READ_COUNTS.lock().unwrap().remove(path);
}

#[cfg(test)]
pub(crate) fn bounded_log_read_count(path: &Path) -> usize {
    BOUNDED_LOG_READ_COUNTS
        .lock()
        .unwrap()
        .get(path)
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn set_bounded_log_read_delay(path: &Path, delay: Option<Duration>) {
    let mut delays = BOUNDED_LOG_READ_DELAYS.lock().unwrap();
    if let Some(delay) = delay {
        delays.insert(path.to_path_buf(), delay);
    } else {
        delays.remove(path);
    }
}

impl LocalJobLogSnapshot {
    pub(crate) fn read_lines(
        &self,
        offset: Option<usize>,
        tail_lines: Option<usize>,
    ) -> (String, usize, usize, bool) {
        let lines: Vec<&str> = self.retained_text.lines().collect();
        if self.total_lines == 0 {
            return (String::new(), 1, 0, false);
        }

        let (start_idx, limit) = if let Some(requested) = offset {
            let first_available = self.first_retained_line;
            let effective = requested.max(1).max(first_available);
            let start = if effective > self.total_lines {
                lines.len()
            } else {
                effective.saturating_sub(first_available).min(lines.len())
            };
            (start, MAX_LOCAL_LOG_LINES)
        } else {
            let tail = tail_lines
                .filter(|lines| *lines > 0)
                .map(|lines| lines.min(MAX_LOCAL_LOG_LINES))
                .unwrap_or(DEFAULT_JOB_LOG_TAIL_LINES);
            (lines.len().saturating_sub(tail), tail)
        };
        let end_idx = start_idx.saturating_add(limit).min(lines.len());
        let selected = lines[start_idx..end_idx].join("\n");
        let next_line = if start_idx >= lines.len() {
            self.total_lines.saturating_add(1)
        } else {
            self.first_retained_line.saturating_add(end_idx)
        };
        (
            selected,
            next_line,
            self.total_lines,
            self.truncated || start_idx > 0 || end_idx < lines.len(),
        )
    }
}

/// Capture an inspect job's terminal files in memory, then release its only
/// scratch owner so the private directory is removed. Active jobs keep the
/// scratch alive; terminal status is published only after `finished_at`.
fn read_bounded_log(
    path: &Path,
    offset: Option<usize>,
    snapshot_len: Option<u64>,
) -> std::io::Result<LocalJobLogSnapshot> {
    let file = File::open(path)?;
    // Freeze this call at the length observed immediately after opening. A job
    // may continue appending, but this reader must not chase a growing file.
    let snapshot_len = snapshot_len.unwrap_or(file.metadata()?.len());
    #[cfg(test)]
    {
        let mut counts = BOUNDED_LOG_READ_COUNTS.lock().unwrap();
        *counts.entry(path.to_path_buf()).or_default() += 1;
        drop(counts);
        if let Some(delay) = BOUNDED_LOG_READ_DELAYS.lock().unwrap().get(path).copied() {
            std::thread::sleep(delay);
        }
    }
    let mut file = file.take(snapshot_len);
    let mut retained = VecDeque::with_capacity(MAX_LOCAL_LOG_BYTES_PER_STREAM);
    let mut buffer = [0_u8; LOCAL_LOG_READ_CHUNK_BYTES];
    let mut total_newlines = 0_usize;
    let mut last_byte = None;
    let requested_line = offset.map(|line| line.max(1));
    let mut current_line = 1_usize;
    let mut retained_complete_lines = 0_usize;
    let mut first_retained_line = requested_line.unwrap_or(1);
    let mut truncated = false;

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        for &byte in &buffer[..read] {
            total_newlines = total_newlines.saturating_add(usize::from(byte == b'\n'));
            last_byte = Some(byte);

            let retain_byte = match requested_line {
                Some(requested) => {
                    current_line >= requested && retained_complete_lines < MAX_LOCAL_LOG_LINES
                }
                None => true,
            };
            if retain_byte {
                if retained.len() == MAX_LOCAL_LOG_BYTES_PER_STREAM {
                    if retained.pop_front() == Some(b'\n') {
                        first_retained_line = first_retained_line.saturating_add(1);
                    }
                    truncated = true;
                }
                retained.push_back(byte);
                if byte == b'\n' {
                    retained_complete_lines = retained_complete_lines.saturating_add(1);
                }
            }
            if byte == b'\n' {
                current_line = current_line.saturating_add(1);
            }
        }
    }

    let total_lines =
        total_newlines.saturating_add(usize::from(last_byte.is_some_and(|byte| byte != b'\n')));
    let mut retained: Vec<u8> = retained.into_iter().collect();
    if requested_line.is_none() {
        let retained_lines = byte_line_count(&retained);
        if retained_lines > MAX_LOCAL_LOG_LINES {
            let lines_to_drop = retained_lines - MAX_LOCAL_LOG_LINES;
            if let Some(end) = nth_newline(&retained, lines_to_drop) {
                retained.drain(..=end);
                first_retained_line = first_retained_line.saturating_add(lines_to_drop);
                truncated = true;
            }
        }
    }

    let mut retained_text = String::from_utf8_lossy(&retained).into_owned();
    if retained_text.len() > MAX_LOCAL_LOG_BYTES_PER_STREAM {
        let mut start = retained_text.len() - MAX_LOCAL_LOG_BYTES_PER_STREAM;
        while !retained_text.is_char_boundary(start) {
            start += 1;
        }
        // Prefer a complete first line after lossy UTF-8 expansion. If the
        // byte limit lands inside a line and another line follows, discard
        // the partial line through its newline. A single remaining long line
        // has no later boundary, so keep its suffix and its original line
        // number instead of returning an empty log.
        if retained_text.as_bytes()[start - 1] != b'\n' {
            if let Some(next_newline) = retained_text.as_bytes()[start..]
                .iter()
                .position(|byte| *byte == b'\n')
            {
                let next_line_start = start + next_newline + 1;
                if next_line_start < retained_text.len() {
                    start = next_line_start;
                }
            }
        }
        let removed_newlines = retained_text.as_bytes()[..start]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        first_retained_line = first_retained_line.saturating_add(removed_newlines);
        retained_text = retained_text[start..].to_string();
        truncated = true;
    }
    let retained_lines = retained_text.lines().count();
    if retained_lines == 0 {
        first_retained_line = total_lines.saturating_add(1);
    }
    let retained_end = first_retained_line.saturating_add(retained_lines.saturating_sub(1));
    truncated |= first_retained_line > 1 || retained_end < total_lines;

    Ok(LocalJobLogSnapshot {
        retained_text,
        total_lines,
        first_retained_line,
        truncated,
    })
}

fn byte_line_count(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        .saturating_add(usize::from(bytes.last().is_some_and(|byte| *byte != b'\n')))
}

fn nth_newline(bytes: &[u8], count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let mut seen = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            seen += 1;
            if seen == count {
                return Some(index);
            }
        }
    }
    None
}

impl Default for LocalJobLogSnapshot {
    fn default() -> Self {
        Self {
            retained_text: String::new(),
            total_lines: 0,
            first_retained_line: 1,
            truncated: false,
        }
    }
}

/// Local job statuses that are still active (not yet terminal). A stop/timeout
/// only acts on these; terminal jobs (`completed`/`failed`/`stopped`/`lost`)
/// are left untouched.
pub(crate) const ACTIVE_LOCAL_STATUSES: &[&str] = &["running", "queued", "started"];

/// Statuses counted as broadly "active" by runtime observability and bounded
/// summaries. `stop_requested` remains active for compatibility, but
/// lifecycle summaries classify it as nonblocking terminal-pending state.
pub(crate) const ACTIVE_JOB_STATUSES: &[&str] = &[
    "running",
    "queued",
    "started",
    "agent_queued",
    "stop_requested",
    "recovering",
];

/// Outcome of attempting to terminate a local job's process group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminateOutcome {
    /// The process group was alive and was signalled. `escalated_to_kill` is
    /// true when SIGTERM did not suffice within the grace window and SIGKILL
    /// was sent to the whole group.
    Terminated { pgid: i64, escalated_to_kill: bool },
    /// No live process was found for the recorded pid (already exited).
    AlreadyGone,
}

/// Abstraction over terminating a local job's process group.
///
/// The production implementation shells out to `kill -TERM/-KILL -<pgid>`
/// (negative pid => whole process group). Local jobs are spawned with
/// `setsid`, which makes the wrapper shell a session and process-group
/// leader, so `-<pgid>` reaches the wrapper and every descendant it spawned
/// in a single signal, reliably reclaiming the whole subtree.
///
/// Tests inject a fake to assert the runtime targets the correct pgid without
/// spawning real processes. The runtime only ever passes pids/pgids read from
/// its own on-disk job files, never caller-supplied pids.
pub(crate) trait LocalJobKiller: Send + Sync {
    /// Terminate the process group led by `pid`/`pgid`. Sends SIGTERM, waits
    /// briefly, and escalates to SIGKILL if the leader is still alive. Never
    /// panics; a failure to signal is reflected as a `Terminated` outcome
    /// without escalation.
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome;
}

/// Production `LocalJobKiller` backed by the `kill` shell command.
pub(crate) struct SystemJobKiller;

impl SystemJobKiller {
    /// True if a process with `pid` is currently alive (`kill -0`).
    fn is_alive(pid: i64) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// True while any process remains in the owned process group. This is
    /// stronger than checking only the leader: the leader may exit after TERM
    /// while a descendant still needs escalation or reaping.
    fn group_is_alive(pgid: i64) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg("--")
            .arg(format!("-{pgid}"))
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// Send `signal` (e.g. `-TERM`/`-KILL`) to the whole process group `pgid`
    /// (negative pid). Failures are swallowed: a non-existent group yields a
    /// non-zero exit which we treat as nothing left to signal.
    fn signal_group(pgid: i64, signal: &str) {
        match std::process::Command::new("kill")
            .arg(signal)
            .arg("--")
            .arg(format!("-{}", pgid))
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => {
                tracing::debug!(
                    pgid,
                    signal,
                    status = %status,
                    "local job process-group signal did not report success"
                );
            }
            Err(e) => {
                tracing::warn!(
                    pgid,
                    signal,
                    error = %e,
                    "failed to signal local job process group"
                );
            }
        }
    }
}

impl LocalJobKiller for SystemJobKiller {
    fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
        if !Self::is_alive(pid) && !Self::group_is_alive(pgid) {
            return TerminateOutcome::AlreadyGone;
        }
        Self::signal_group(pgid, "-TERM");
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if !Self::group_is_alive(pgid) {
                return TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill: false,
                };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let escalated = Self::group_is_alive(pgid);
        if escalated {
            Self::signal_group(pgid, "-KILL");
            let reap_deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < reap_deadline && Self::group_is_alive(pgid) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        TerminateOutcome::Terminated {
            pgid,
            escalated_to_kill: escalated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOSSY_TEST_INVALID_BYTES_PER_LINE: usize = 3 * 1024;

    fn numbered_lossy_log(total_lines: usize) -> Vec<u8> {
        let mut log = Vec::with_capacity(total_lines * (LOSSY_TEST_INVALID_BYTES_PER_LINE + 16));
        for line in 1..=total_lines {
            log.extend_from_slice(format!("line-{line:04}:").as_bytes());
            log.resize(log.len() + LOSSY_TEST_INVALID_BYTES_PER_LINE, 0xff);
            log.push(b'\n');
        }
        log
    }

    fn numbered_line_ids(text: &str) -> Vec<usize> {
        text.lines()
            .map(|line| {
                let (number, _) = line
                    .strip_prefix("line-")
                    .and_then(|line| line.split_once(':'))
                    .expect("retained line did not start at a numbered line boundary");
                number.parse().unwrap()
            })
            .collect()
    }

    #[test]
    fn active_job_logs_are_bounded_and_keep_global_line_cursors() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let total_lines = 20_000;
        let stdout = (1..=total_lines)
            .map(|line| format!("{line:05}:{}\n", "x".repeat(120)))
            .collect::<String>();
        let stderr = (1..=total_lines)
            .map(|line| format!("err-{line:05}:{}\n", "y".repeat(120)))
            .collect::<String>();
        assert!(stdout.len() > 2 * MAX_LOCAL_LOG_BYTES_PER_STREAM);
        assert!(stderr.len() > 2 * MAX_LOCAL_LOG_BYTES_PER_STREAM);
        std::fs::write(dir.join("stdout.log"), stdout).unwrap();
        std::fs::write(dir.join("stderr.log"), stderr).unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);

        for name in ["stdout.log", "stderr.log"] {
            let (tail, cursor, observed_lines, truncated) =
                record.read_log_lines(name, None, Some(MAX_LOCAL_LOG_LINES));
            assert!(tail.len() <= MAX_LOCAL_LOG_BYTES_PER_STREAM);
            assert_eq!(tail.lines().count(), MAX_LOCAL_LOG_LINES);
            assert_eq!(observed_lines, total_lines);
            assert_eq!(cursor, total_lines + 1);
            assert!(truncated);
            assert!(tail.contains(&format!("{total_lines:05}:")));
            assert!(!tail.contains("00001:"));

            let (first_page, cursor, observed_lines, truncated) =
                record.read_log_lines(name, Some(1), None);
            assert_eq!(first_page.lines().count(), MAX_LOCAL_LOG_LINES);
            assert_eq!(cursor, MAX_LOCAL_LOG_LINES + 1);
            assert_eq!(observed_lines, total_lines);
            assert!(truncated);
            assert!(first_page.contains("00001:"));
            assert!(!first_page.contains("00501:"));

            let (page, cursor, observed_lines, truncated) =
                record.read_log_lines(name, Some(total_lines - 9), None);
            assert_eq!(page.lines().count(), 10);
            assert_eq!(cursor, total_lines + 1);
            assert_eq!(observed_lines, total_lines);
            assert!(truncated);

            let (past_eof, cursor, observed_lines, truncated) =
                record.read_log_lines(name, Some(total_lines + 10), None);
            assert!(past_eof.is_empty());
            assert_eq!(cursor, total_lines + 1);
            assert_eq!(observed_lines, total_lines);
            assert!(truncated);
        }
    }

    #[test]
    fn active_job_log_lossy_truncation_keeps_offset_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let total_lines = 540;
        std::fs::write(dir.join("stdout.log"), numbered_lossy_log(total_lines)).unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);

        let (page, next_line, observed_lines, truncated) =
            record.read_log_lines("stdout.log", Some(1), None);
        let returned_lines = numbered_line_ids(&page);

        assert!(page.len() <= MAX_LOCAL_LOG_BYTES_PER_STREAM);
        assert!(page.contains('\u{fffd}'));
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);
        assert!(!returned_lines.is_empty());
        assert!(returned_lines[0] > 1);
        assert_eq!(returned_lines.last().copied(), Some(MAX_LOCAL_LOG_LINES));
        assert_eq!(next_line, MAX_LOCAL_LOG_LINES + 1);
        assert_eq!(next_line, returned_lines.last().unwrap() + 1);
        assert_eq!(
            next_line,
            returned_lines[0] + returned_lines.len(),
            "cursor must be based on the final retained prefix"
        );

        let (later_page, later_cursor, observed_lines, truncated) =
            record.read_log_lines("stdout.log", Some(21), None);
        let later_lines = numbered_line_ids(&later_page);
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);
        assert!(later_lines[0] > 21);
        assert_eq!(later_lines.last().copied(), Some(520));
        assert_eq!(later_cursor, 521);
        assert_eq!(later_cursor, later_lines[0] + later_lines.len());

        let (tail, tail_cursor, observed_lines, truncated) =
            record.read_log_lines("stdout.log", None, Some(20));
        assert_eq!(
            numbered_line_ids(&tail),
            (total_lines - 19..=total_lines).collect::<Vec<_>>()
        );
        assert_eq!(tail_cursor, total_lines + 1);
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);
    }

    #[test]
    fn active_job_log_lossy_cursor_fetches_next_page_without_repeat() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let total_lines = 540;
        std::fs::write(dir.join("stdout.log"), numbered_lossy_log(total_lines)).unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);

        let (first_page, next_line, observed_lines, truncated) =
            record.read_log_lines("stdout.log", Some(1), None);
        let first_page_lines = numbered_line_ids(&first_page);
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);

        let (second_page, final_cursor, observed_lines, truncated) =
            record.read_log_lines("stdout.log", Some(next_line), None);
        let second_page_lines = numbered_line_ids(&second_page);
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);
        assert_eq!(second_page_lines.first().copied(), Some(next_line));
        assert!(
            second_page_lines[0] > *first_page_lines.last().unwrap(),
            "the follow-up page repeated the prior page"
        );
        assert_eq!(second_page_lines.last().copied(), Some(total_lines));
        assert_eq!(final_cursor, total_lines + 1);
        assert!(final_cursor > next_line);

        let (past_eof, eof_cursor, observed_lines, truncated) =
            record.read_log_lines("stdout.log", Some(final_cursor), None);
        assert!(past_eof.is_empty());
        assert_eq!(eof_cursor, total_lines + 1);
        assert_eq!(observed_lines, total_lines);
        assert!(truncated);
    }

    #[test]
    fn active_job_log_lossy_truncation_discards_partial_first_line() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let mut stdout = vec![0xff; MAX_LOCAL_LOG_BYTES_PER_STREAM + 64 * 1024];
        stdout.extend_from_slice(
            b"\nline-0002:two\nline-0003:three\nline-0004:four\nline-0005:THE-END",
        );
        std::fs::write(dir.join("stdout.log"), stdout).unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);

        let (page, next_line, total_lines, truncated) =
            record.read_log_lines("stdout.log", Some(1), None);
        assert_eq!(
            page,
            "line-0002:two\nline-0003:three\nline-0004:four\nline-0005:THE-END"
        );
        assert_eq!(numbered_line_ids(&page), vec![2, 3, 4, 5]);
        assert_eq!(next_line, 6);
        assert_eq!(total_lines, 5);
        assert!(truncated);

        let (past_eof, eof_cursor, total_lines, truncated) =
            record.read_log_lines("stdout.log", Some(next_line), None);
        assert!(past_eof.is_empty());
        assert_eq!(eof_cursor, 6);
        assert_eq!(total_lines, 5);
        assert!(truncated);
    }

    #[test]
    fn active_job_log_bounds_long_lossy_line_and_preserves_small_logs() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let mut long_line = vec![0xff; 2 * MAX_LOCAL_LOG_BYTES_PER_STREAM];
        long_line.extend_from_slice(b"THE-END");
        std::fs::write(dir.join("stdout.log"), long_line).unwrap();
        std::fs::write(dir.join("stderr.log"), b"one\ntwo\nthree\n").unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);

        let (stdout, cursor, total_lines, truncated) =
            record.read_log_lines("stdout.log", None, None);
        assert!(stdout.len() <= MAX_LOCAL_LOG_BYTES_PER_STREAM);
        assert!(stdout.ends_with("THE-END"));
        assert!(stdout.contains('\u{fffd}'));
        assert_eq!(cursor, 2);
        assert_eq!(total_lines, 1);
        assert!(truncated);

        let (stderr, cursor, total_lines, truncated) =
            record.read_log_lines("stderr.log", None, None);
        assert_eq!(stderr, "one\ntwo\nthree");
        assert_eq!(cursor, 4);
        assert_eq!(total_lines, 3);
        assert!(!truncated);

        let (stderr_tail, cursor, total_lines, truncated) =
            record.read_log_lines("stderr.log", None, Some(2));
        assert_eq!(stderr_tail, "two\nthree");
        assert_eq!(cursor, 4);
        assert_eq!(total_lines, 3);
        assert!(truncated);
    }

    #[test]
    fn active_job_log_read_does_not_chase_appends() {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;

        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("job");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("stdout.log");
        std::fs::write(&path, "start\n").unwrap();
        let record = LocalJobRecord::new("project".to_string(), dir);
        let keep_writing = Arc::new(AtomicBool::new(true));
        let writer_flag = keep_writing.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let writer = std::thread::spawn(move || {
            let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            let chunk = vec![b'z'; LOCAL_LOG_READ_CHUNK_BYTES];
            let mut bytes_written = 0_usize;
            let mut ready_sent = false;
            while writer_flag.load(Ordering::Relaxed) {
                file.write_all(&chunk).unwrap();
                bytes_written = bytes_written.saturating_add(chunk.len());
                if !ready_sent && bytes_written > MAX_LOCAL_LOG_BYTES_PER_STREAM {
                    ready_tx.send(()).unwrap();
                    ready_sent = true;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });

        if let Err(error) = ready_rx.recv_timeout(Duration::from_secs(2)) {
            keep_writing.store(false, Ordering::Relaxed);
            writer.join().unwrap();
            panic!("writer did not exceed the log bound: {error}");
        }
        let started = Instant::now();
        let (text, cursor, total_lines, truncated) =
            record.read_log_lines("stdout.log", None, Some(10));
        let elapsed = started.elapsed();
        keep_writing.store(false, Ordering::Relaxed);
        writer.join().unwrap();

        assert!(elapsed < Duration::from_secs(2), "read took {elapsed:?}");
        assert!(text.len() <= MAX_LOCAL_LOG_BYTES_PER_STREAM);
        assert!(cursor <= total_lines.saturating_add(1));
        assert!(truncated);
    }
}
