use super::shutdown::ShutdownReport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH,
};

const DIAGNOSTICS_DIR: &str = "runner-exit-diagnostics-v1";
const STATE_SCHEMA_VERSION: u32 = 1;
const RECORD_MAX_BYTES: usize = 16 * 1024;
const MAX_LIFECYCLE_RECORDS: usize = 8;
const ATOMIC_REPLACE_RETRY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LifecycleState {
    Starting,
    Running,
    ShutdownRequested,
    TransportReturned,
    Exited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportOutcome {
    Ok,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalOutcome {
    Clean,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BuildEvidence {
    version: Option<String>,
    git_commit: Option<String>,
    git_dirty: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TransportReturnEvidence {
    at_unix_ms: i64,
    outcome: TransportOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ShutdownEvidence {
    elapsed_ms: u64,
    timed_out_phases: Vec<String>,
    failed_phases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TerminalEvidence {
    at_unix_ms: i64,
    outcome: TerminalOutcome,
    reason_code: String,
    shutdown: Option<ShutdownEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LifecycleRecord {
    schema_version: u32,
    diagnostic_id: String,
    client_id: String,
    process_id: u32,
    process_started_at_unix_s: i64,
    record_started_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    transport: String,
    build: BuildEvidence,
    state: LifecycleState,
    shutdown_signal_received_at_unix_ms: Option<i64>,
    transport_return: Option<TransportReturnEvidence>,
    terminal: Option<TerminalEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PanicEvidence {
    schema_version: u32,
    diagnostic_id: String,
    process_id: u32,
    at_unix_ms: i64,
    thread_name: Option<String>,
    source_file: Option<String>,
    source_line: Option<u32>,
    source_column: Option<u32>,
}

pub(crate) struct RunnerExitDiagnostics {
    record_path: PathBuf,
    panic_path: PathBuf,
    record: Mutex<LifecycleRecord>,
    panic_write: Mutex<()>,
}

impl RunnerExitDiagnostics {
    pub(crate) fn start(
        client_id: &str,
        server_url: &str,
        transport: &str,
        process_started_at_unix_s: i64,
        build_version: Option<&str>,
        build_git_commit: Option<&str>,
        build_git_dirty: Option<bool>,
    ) -> Result<Arc<Self>, String> {
        let root = default_root_for_runner(client_id, server_url)?;
        Self::start_in_root(
            root,
            client_id,
            transport,
            process_started_at_unix_s,
            build_version,
            build_git_commit,
            build_git_dirty,
            chrono::Utc::now().timestamp_millis(),
            std::process::id(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_in_root(
        root: PathBuf,
        client_id: &str,
        transport: &str,
        process_started_at_unix_s: i64,
        build_version: Option<&str>,
        build_git_commit: Option<&str>,
        build_git_dirty: Option<bool>,
        now_ms: i64,
        process_id: u32,
    ) -> Result<Arc<Self>, String> {
        fs::create_dir_all(&root).map_err(|error| {
            format!("failed to create Runner exit diagnostic directory: {error}")
        })?;
        ensure_directory_not_symlink(&root)?;

        let diagnostic_id = uuid::Uuid::new_v4().simple().to_string();
        let prefix = format!("{now_ms:020}-{process_id}-{diagnostic_id}");
        let record_path = root.join(format!("{prefix}.json"));
        let panic_path = root.join(format!("{prefix}.panic.json"));
        let record = LifecycleRecord {
            schema_version: STATE_SCHEMA_VERSION,
            diagnostic_id,
            client_id: bounded_single_line(client_id, 128),
            process_id,
            process_started_at_unix_s,
            record_started_at_unix_ms: now_ms,
            updated_at_unix_ms: now_ms,
            transport: bounded_single_line(transport, 32),
            build: BuildEvidence {
                version: build_version.map(|value| bounded_single_line(value, 128)),
                git_commit: build_git_commit.map(|value| bounded_single_line(value, 128)),
                git_dirty: build_git_dirty,
            },
            state: LifecycleState::Starting,
            shutdown_signal_received_at_unix_ms: None,
            transport_return: None,
            terminal: None,
        };
        atomic_write_json(&record_path, &record)?;
        prune_old_records(&root, &record_path);
        Ok(Arc::new(Self {
            record_path,
            panic_path,
            record: Mutex::new(record),
            panic_write: Mutex::new(()),
        }))
    }

    pub(crate) fn install_panic_hook(self: &Arc<Self>) {
        let previous = std::panic::take_hook();
        let weak = Arc::downgrade(self);
        std::panic::set_hook(Box::new(move |info| {
            if let Some(diagnostics) = weak.upgrade() {
                diagnostics.record_panic(info);
            }
            previous(info);
        }));
    }

    pub(crate) fn mark_running(&self) {
        self.update(|record, now_ms| {
            record.state = LifecycleState::Running;
            record.updated_at_unix_ms = now_ms;
        });
    }

    pub(crate) fn mark_shutdown_signal_received(&self) {
        self.update(|record, now_ms| {
            record.state = LifecycleState::ShutdownRequested;
            record
                .shutdown_signal_received_at_unix_ms
                .get_or_insert(now_ms);
            record.updated_at_unix_ms = now_ms;
        });
    }

    pub(crate) fn mark_transport_returned(&self, ok: bool) {
        self.update(|record, now_ms| {
            record.state = LifecycleState::TransportReturned;
            record.transport_return = Some(TransportReturnEvidence {
                at_unix_ms: now_ms,
                outcome: if ok {
                    TransportOutcome::Ok
                } else {
                    TransportOutcome::Error
                },
            });
            record.updated_at_unix_ms = now_ms;
        });
    }

    pub(crate) fn mark_terminal(
        &self,
        clean: bool,
        reason_code: &'static str,
        shutdown: Option<&ShutdownReport>,
    ) {
        self.update(|record, now_ms| {
            record.state = LifecycleState::Exited;
            record.terminal = Some(TerminalEvidence {
                at_unix_ms: now_ms,
                outcome: if clean {
                    TerminalOutcome::Clean
                } else {
                    TerminalOutcome::Fatal
                },
                reason_code: reason_code.to_string(),
                shutdown: shutdown.map(|report| ShutdownEvidence {
                    elapsed_ms: report.elapsed_ms,
                    timed_out_phases: report
                        .timed_out_phases
                        .iter()
                        .map(|phase| (*phase).to_string())
                        .collect(),
                    failed_phases: report
                        .failed_phases
                        .iter()
                        .map(|phase| (*phase).to_string())
                        .collect(),
                }),
            });
            record.updated_at_unix_ms = now_ms;
        });
    }

    #[cfg(test)]
    fn record_path(&self) -> &Path {
        &self.record_path
    }

    fn update(&self, update: impl FnOnce(&mut LifecycleRecord, i64)) {
        let mut record = self
            .record
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut record, chrono::Utc::now().timestamp_millis());
        if let Err(error) = atomic_write_json(&self.record_path, &*record) {
            tracing::warn!(error = %error, "Windows Runner exit diagnostic update failed; Runner continues");
        }
    }

    fn record_panic(&self, info: &std::panic::PanicHookInfo<'_>) {
        let _guard = match self.panic_write.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        let location = info.location();
        let current = std::thread::current();
        let evidence = PanicEvidence {
            schema_version: STATE_SCHEMA_VERSION,
            diagnostic_id: self
                .record
                .try_lock()
                .map(|record| record.diagnostic_id.clone())
                .unwrap_or_else(|_| "unavailable".to_string()),
            process_id: std::process::id(),
            at_unix_ms: chrono::Utc::now().timestamp_millis(),
            thread_name: current.name().map(|name| bounded_single_line(name, 128)),
            source_file: location.map(|location| bounded_single_line(location.file(), 512)),
            source_line: location.map(|location| location.line()),
            source_column: location.map(|location| location.column()),
        };
        let _ = atomic_write_json(&self.panic_path, &evidence);
    }
}

fn default_root_for_runner(client_id: &str, server_url: &str) -> Result<PathBuf, String> {
    let base = webcodex_runner_config::paths::default_client_state_base_dir()?;
    Ok(default_root_for_runner_with_base(
        &base, client_id, server_url,
    ))
}

fn default_root_for_runner_with_base(base: &Path, client_id: &str, server_url: &str) -> PathBuf {
    let server_url = server_url.trim().trim_end_matches('/');
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-runner-exit-diagnostics-v1\0");
    hasher.update(client_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(server_url.as_bytes());
    base.join(DIAGNOSTICS_DIR)
        .join(format!("{:x}", hasher.finalize()))
}

fn bounded_single_line(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|ch| *ch != '\r' && *ch != '\n')
        .take(max_chars)
        .collect()
}

fn ensure_directory_not_symlink(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect Runner exit diagnostic directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Runner exit diagnostic path must be a regular directory".to_string());
    }
    Ok(())
}

fn prune_old_records(root: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut lifecycle = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let file_type = entry.file_type().ok()?;
            (file_type.is_file() && name.ends_with(".json") && !name.ends_with(".panic.json"))
                .then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    lifecycle.sort_by(|a, b| a.0.cmp(&b.0));
    let remove_count = lifecycle.len().saturating_sub(MAX_LIFECYCLE_RECORDS);
    for (_, path) in lifecycle
        .into_iter()
        .filter(|(_, path)| path != current)
        .take(remove_count)
    {
        let panic = panic_path_for(&path);
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(panic);
    }
}

fn panic_path_for(record_path: &Path) -> PathBuf {
    let stem = record_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("runner");
    record_path.with_file_name(format!("{stem}.panic.json"))
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode Runner exit diagnostic: {error}"))?;
    if bytes.len() > RECORD_MAX_BYTES {
        return Err(format!(
            "Runner exit diagnostic exceeds {RECORD_MAX_BYTES} bytes"
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Runner exit diagnostic path has no parent".to_string())?;
    ensure_directory_not_symlink(parent)?;
    reject_symlink_or_non_file(path)?;
    let temp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runner-exit-diagnostic")
    ));
    match fs::symlink_metadata(&temp) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("Runner exit diagnostic temp path is not a regular file".to_string());
            }
            fs::remove_file(&temp)
                .map_err(|error| format!("failed to remove stale diagnostic temp file: {error}"))?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect Runner exit diagnostic temp file: {error}"
            ));
        }
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&temp)
            .map_err(|error| {
                format!("failed to create Runner exit diagnostic temp file: {error}")
            })?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write Runner exit diagnostic: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync Runner exit diagnostic: {error}"))?;
        publish_state_file(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn reject_symlink_or_non_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("Runner exit diagnostic destination is not a regular file".to_string())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to inspect Runner exit diagnostic destination: {error}"
        )),
    }
}

fn publish_state_file(temp: &Path, state_path: &Path) -> Result<(), String> {
    let from = windows_wide_path(temp);
    let to = windows_wide_path(state_path);
    let retry_deadline = Instant::now() + ATOMIC_REPLACE_RETRY;
    loop {
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } != 0
        {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        let retryable = matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33));
        if !retryable || Instant::now() >= retry_deadline {
            return Err(format!("failed to publish Runner exit diagnostic: {error}"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn windows_wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_record(path: &Path) -> LifecycleRecord {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn lifecycle_record_preserves_exit_classification_without_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let diagnostics = RunnerExitDiagnostics::start_in_root(
            temp.path().to_path_buf(),
            "msi",
            "websocket",
            123,
            Some("0.3.8"),
            Some("deadbeef"),
            Some(false),
            456_000,
            42,
        )
        .unwrap();

        diagnostics.mark_running();
        diagnostics.mark_shutdown_signal_received();
        diagnostics.mark_transport_returned(false);
        diagnostics.mark_terminal(false, "transport_returned_error", None);

        let record = read_record(diagnostics.record_path());
        assert_eq!(record.client_id, "msi");
        assert_eq!(record.process_id, 42);
        assert_eq!(record.state, LifecycleState::Exited);
        assert!(record.shutdown_signal_received_at_unix_ms.is_some());
        assert_eq!(
            record.transport_return.as_ref().map(|item| item.outcome),
            Some(TransportOutcome::Error)
        );
        let terminal = record.terminal.unwrap();
        assert_eq!(terminal.outcome, TerminalOutcome::Fatal);
        assert_eq!(terminal.reason_code, "transport_returned_error");
        let encoded = fs::read_to_string(diagnostics.record_path()).unwrap();
        for forbidden in ["Authorization", "Bearer ", "token=", "command", "payload"] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn profile_namespace_hashes_server_url_and_retention_is_bounded() {
        let base = tempfile::tempdir().unwrap();
        let root = default_root_for_runner_with_base(base.path(), "msi", "https://sg4.yyjeqhc.cn/");
        assert!(!root.to_string_lossy().contains("sg4.yyjeqhc.cn"));
        fs::create_dir_all(&root).unwrap();
        for index in 0..(MAX_LIFECYCLE_RECORDS + 3) {
            let diagnostics = RunnerExitDiagnostics::start_in_root(
                root.clone(),
                "msi",
                "websocket",
                index as i64,
                None,
                None,
                None,
                1_000 + index as i64,
                10 + index as u32,
            )
            .unwrap();
            diagnostics.mark_running();
        }
        // Retention must remain bounded even if the wall clock moves backward
        // and the current process record sorts before older records.
        let skewed = RunnerExitDiagnostics::start_in_root(
            root.clone(),
            "msi",
            "websocket",
            99,
            None,
            None,
            None,
            1,
            999,
        )
        .unwrap();
        skewed.mark_running();
        let lifecycle_count = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                entry.file_type().is_ok_and(|kind| kind.is_file())
                    && name.ends_with(".json")
                    && !name.ends_with(".panic.json")
            })
            .count();
        assert_eq!(lifecycle_count, MAX_LIFECYCLE_RECORDS);
    }
}
