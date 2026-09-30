//! Public shell identity, results, limits, and transport contract.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellIdentity {
    pub shell_id: String,
    pub workflow_session_id: String,
    pub runtime_project_id: String,
    pub executor: String,
    pub client_id: Option<String>,
}

/// Opaque transport-binding metadata captured at open time and preserved for
/// the life of one shell entry.
///
/// A transport reports whatever immutable facts its bindings depend on without
/// exposing transport details to the shared state machine. The Runner uses it
/// to remember the named SSH resource and its configuration generation at open,
/// so a later config change invalidates the shell instead of letting a stale
/// transport keep accepting commands. Nothing host-, credential-, or
/// ControlPath-shaped is ever stored here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportMetadata {
    /// Named resource the transport is bound to, if any.
    pub resource: Option<String>,
    /// Configuration generation the transport was opened against, if any.
    pub generation: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ShellLaunch {
    pub identity: ShellIdentity,
    pub dialect: String,
    pub profile: Option<String>,
    pub program: String,
    /// Startup arguments owned by the selected shell/profile before any
    /// transport-specific payload mode. Unix command-mode `-c` is forbidden;
    /// Windows receives the configured PowerShell prefix arguments and the
    /// transport appends its private `-File` bootstrap.
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
    pub env: HashMap<String, String>,
    /// Initialization text evaluated once in the new shell before `open`
    /// succeeds. Its output is drained but never returned to later commands.
    pub initialization: Option<String>,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellState {
    Opening,
    Running,
    Exited,
    Closed,
    Poisoned,
    Lost,
}

impl ShellState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Closed => "closed",
            Self::Poisoned => "poisoned",
            Self::Lost => "lost",
        }
    }

    pub(super) fn is_active(self) -> bool {
        matches!(self, Self::Opening | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSummary {
    pub identity: ShellIdentity,
    pub dialect: String,
    pub profile: Option<String>,
    pub initial_cwd: PathBuf,
    pub cwd: PathBuf,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub state: ShellState,
    pub busy: bool,
    pub exit_code: Option<i32>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecResult {
    pub shell_id: String,
    pub command_started: bool,
    pub command_completed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
    pub execution_state: String,
    pub shell_state: ShellState,
    pub cwd: PathBuf,
    pub error_code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCloseResult {
    pub summary: ShellSummary,
    pub already_closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellError {
    pub code: &'static str,
    pub message: String,
}

impl ShellError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ShellError {}

#[derive(Debug, Clone, Copy)]
pub struct ShellLimits {
    pub max_shells: usize,
    pub idle_timeout: Duration,
    pub max_terminal_records: usize,
}

impl Default for ShellLimits {
    fn default() -> Self {
        Self {
            max_shells: 8,
            idle_timeout: Duration::from_secs(30 * 60),
            max_terminal_records: DEFAULT_TERMINAL_RECORDS,
        }
    }
}

/// One parsed completion control frame emitted by a shell transport.
#[derive(Debug)]
pub struct ControlFrame {
    pub token: String,
    pub status: i32,
    pub cwd: PathBuf,
}

/// Accumulated synchronization evidence for one in-flight command. A transport
/// fills `control`, `stdout_synced`, and `stderr_synced` as it observes the
/// per-command markers; the manager treats a frame as complete only once both
/// output streams are synced and the control frame has arrived.
#[derive(Default)]
pub struct CompletionProgress {
    pub control: Option<ControlFrame>,
    pub stdout_synced: bool,
    pub stderr_synced: bool,
}

/// Outcome of waiting for a command's completion frame.
pub enum WaitOutcome {
    Frame(ControlFrame),
    Exited(ExitStatus),
    TimedOut,
    ControlLost,
}

/// Minimal transport boundary for one long-lived shell process.
///
/// Only the mechanics that differ between a local spawned child and a remote
/// shell driven over an SSH channel are abstracted here. The
/// [`PersistentShellManager`] owns all shared semantics on top of this trait:
/// identity binding, the `ShellState` machine, the busy guard, output limits,
/// timeout recovery, poisoned/lost transitions, idle reclamation, and lifecycle
/// (`close_session` / `close_project` / `close_all`).
///
/// `write_command` feeds a command plus its per-command high-entropy `token`
/// to the shell; the transport's readers surface the matching sync markers and
/// control frame through `wait_for_completion`. `interrupt` delivers a timeout
/// signal to the shell's process group so the manager can attempt sync
/// recovery. `stdout`/`stderr` expose the bounded buffers the manager snapshots
/// per command.
pub trait ShellTransport: Send + Sync {
    fn set_expected_token(&self, token: &str);
    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError>;
    fn wait_for_completion(
        &self,
        token: &str,
        timeout: Duration,
        progress: &mut CompletionProgress,
    ) -> WaitOutcome;
    fn try_wait(&self) -> Option<ExitStatus>;
    /// Best-effort timeout interruption. The transport chooses the narrowest
    /// process-lifecycle primitive its host supports; the manager decides whether
    /// to keep or poison the shell only from subsequent synchronization evidence.
    fn interrupt(&self);
    fn shutdown(&self);
    fn terminate_remaining_group_after_exit(&self);
    fn stdout(&self) -> &Arc<Mutex<BoundedBuffer>>;
    fn stderr(&self) -> &Arc<Mutex<BoundedBuffer>>;
    /// Validate the shell-reported cwd in the transport's path namespace.
    /// Local transports use the host platform's path rules. Remote transports
    /// can override this when their shell path syntax differs from the Runner.
    fn reported_cwd_is_absolute(&self, cwd: &Path) -> bool {
        cwd.is_absolute()
    }
    /// Opaque binding metadata captured at open. The shared manager stores it
    /// on the entry so callers can validate bindings that a transport depends
    /// on (e.g. an SSH resource + config generation). The default is no
    /// metadata; only transports with a real binding override this.
    fn metadata(&self) -> Option<TransportMetadata> {
        None
    }
}
