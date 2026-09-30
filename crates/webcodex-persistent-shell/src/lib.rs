//! Bounded, command-oriented persistent shell processes.
//!
//! A shell managed here is a real long-lived local shell process (`sh`/`bash`
//! on Unix or configured PowerShell on Windows). Commands are serialized,
//! stdout/stderr are retained in bounded ring buffers, and command completion
//! plus stream-drain boundaries use transport-private control framing. This
//! crate deliberately does not provide a PTY, raw input, resize, or terminal
//! byte-stream API.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
#[cfg(any(unix, windows))]
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::process::{Child, ChildStdin, Command, Stdio};

/// Reserved descriptor numbers used by the persistent-shell protocol on both
/// platforms. They are plain integers (not `RawFd`, which does not exist on
/// Windows) so the shell wrapper templates and any platform code can share them.
pub const STDOUT_SYNC_FD: i32 = 7;
pub const STDERR_SYNC_FD: i32 = 8;
/// Private control-channel descriptor number, used only by the Unix local
/// shell protocol (and the shell wrapper templates it emits).
#[cfg(unix)]
const CONTROL_FD: i32 = 9;
pub const CONTROL_MAGIC: &[u8] = b"WCPS1";
pub const STDOUT_SYNC_MAGIC: &[u8] = b"WCPSO1";
pub const STDERR_SYNC_MAGIC: &[u8] = b"WCPSE1";
#[cfg(any(unix, windows))]
const CONTROL_FIELD_MAX_BYTES: usize = 8 * 1024;
#[cfg(unix)]
const CONTROL_CHANNEL_CAPACITY: usize = 2;
#[cfg(any(unix, windows))]
const OUTPUT_SYNC_CHANNEL_CAPACITY: usize = 2;
#[cfg(any(unix, windows))]
const OUTPUT_READ_SLEEP: Duration = Duration::from_millis(5);
#[cfg(any(unix, windows))]
const PROCESS_SIGNAL_GRACE: Duration = Duration::from_millis(100);
const TIMEOUT_RECOVERY_WINDOW: Duration = Duration::from_millis(750);
const OPEN_INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TERMINAL_RECORDS: usize = 128;
const MIN_OUTPUT_BYTES: usize = 1024;

fn lock_unpoison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

mod execution;
mod framing;
mod lifecycle;
mod opening;
mod output;
mod types;
#[cfg(unix)]
mod unix;

pub use framing::{
    canonical_dialect, find_bytes, longest_suffix_prefix, output_sync_marker,
    remote_command_wrapper, shell_quote,
};
pub use output::BoundedBuffer;
pub use types::{
    CompletionProgress, ControlFrame, ShellCloseResult, ShellError, ShellExecResult, ShellIdentity,
    ShellLaunch, ShellLimits, ShellState, ShellSummary, ShellTransport, TransportMetadata,
    WaitOutcome,
};

use framing::command_token;
#[cfg(unix)]
use framing::command_wrapper;
#[cfg(all(test, windows))]
use framing::set_test_command_token;
#[cfg(any(unix, windows))]
use framing::{drain_sync_receiver, process_output_pending};
#[cfg(unix)]
use unix::spawn_shell_process;

struct ShellEntry {
    identity: ShellIdentity,
    dialect: String,
    profile: Option<String>,
    initial_cwd: Mutex<PathBuf>,
    initial_cwd_frozen: AtomicBool,
    current_cwd: Mutex<PathBuf>,
    created_at: i64,
    last_activity_at: AtomicU64,
    last_activity_instant: Mutex<Instant>,
    state: Mutex<ShellState>,
    busy: AtomicBool,
    exit_code: Mutex<Option<i32>>,
    close_reason: Mutex<Option<String>>,
    metadata: Mutex<Option<TransportMetadata>>,
    process: Box<dyn ShellTransport>,
}

impl std::fmt::Debug for ShellEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellEntry")
            .field("identity", &self.identity)
            .field("dialect", &self.dialect)
            .field("profile", &self.profile)
            .field("initial_cwd", &*lock_unpoison(&self.initial_cwd))
            .field("state", &*lock_unpoison(&self.state))
            .finish_non_exhaustive()
    }
}

impl ShellEntry {
    fn summary(&self) -> ShellSummary {
        let current_cwd = lock_unpoison(&self.current_cwd).clone();
        ShellSummary {
            identity: self.identity.clone(),
            dialect: self.dialect.clone(),
            profile: self.profile.clone(),
            initial_cwd: lock_unpoison(&self.initial_cwd).clone(),
            cwd: current_cwd,
            created_at: self.created_at,
            last_activity_at: self.last_activity_at.load(Ordering::SeqCst) as i64,
            state: *lock_unpoison(&self.state),
            busy: self.busy.load(Ordering::SeqCst),
            exit_code: *lock_unpoison(&self.exit_code),
            close_reason: lock_unpoison(&self.close_reason).clone(),
        }
    }

    fn touch(&self) {
        self.last_activity_at
            .store(now_ts().max(0) as u64, Ordering::SeqCst);
        *lock_unpoison(&self.last_activity_instant) = Instant::now();
    }

    fn validate_identity(
        &self,
        workflow_session_id: &str,
        runtime_project_id: &str,
    ) -> Result<(), ShellError> {
        if self.identity.workflow_session_id != workflow_session_id
            || self.identity.runtime_project_id != runtime_project_id
        {
            return Err(ShellError::new(
                "persistent_shell_not_found",
                "persistent shell does not belong to the requested Workflow Session and project",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ManagerInner {
    entries: Mutex<HashMap<String, Arc<ShellEntry>>>,
    active_by_session: Mutex<HashMap<String, String>>,
    terminal_order: Mutex<VecDeque<String>>,
    max_shells: AtomicUsize,
    idle_timeout_secs: AtomicU64,
    max_terminal_records: AtomicUsize,
    sweeper_started: AtomicBool,
    stop_sweeper: AtomicBool,
}

impl Drop for ManagerInner {
    fn drop(&mut self) {
        self.stop_sweeper.store(true, Ordering::SeqCst);
        let entries = lock_unpoison(&self.entries)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            entry.process.shutdown();
        }
    }
}

#[derive(Debug, Clone)]
pub struct PersistentShellManager {
    inner: Arc<ManagerInner>,
}

impl PersistentShellManager {
    pub fn new(limits: ShellLimits) -> Self {
        let inner = Arc::new(ManagerInner {
            entries: Mutex::new(HashMap::new()),
            active_by_session: Mutex::new(HashMap::new()),
            terminal_order: Mutex::new(VecDeque::new()),
            max_shells: AtomicUsize::new(limits.max_shells.max(1)),
            idle_timeout_secs: AtomicU64::new(limits.idle_timeout.as_secs().max(1)),
            max_terminal_records: AtomicUsize::new(limits.max_terminal_records.max(1)),
            sweeper_started: AtomicBool::new(false),
            stop_sweeper: AtomicBool::new(false),
        });
        Self { inner }
    }

    pub fn update_limits(&self, limits: ShellLimits) {
        self.inner
            .max_shells
            .store(limits.max_shells.max(1), Ordering::SeqCst);
        self.inner
            .idle_timeout_secs
            .store(limits.idle_timeout.as_secs().max(1), Ordering::SeqCst);
        self.inner
            .max_terminal_records
            .store(limits.max_terminal_records.max(1), Ordering::SeqCst);
        self.prune_terminal_records();
    }
}

pub const fn local_shell_supported() -> bool {
    cfg!(any(unix, windows))
}

#[cfg(all(test, unix))]
#[path = "tests/unix.rs"]
mod tests;
#[cfg(all(test, windows))]
#[path = "tests/windows.rs"]
mod windows_tests;
