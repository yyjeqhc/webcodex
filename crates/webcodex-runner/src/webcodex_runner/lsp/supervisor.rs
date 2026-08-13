#[cfg(windows)]
use super::super::util::resolve_program_in_path;
use super::super::util::{find_executable_in_path, is_executable_file};
use super::language::{profile_for_kind, LanguageProfile};
use super::protocol::{read_message, write_message, FramingError, MAX_LSP_MESSAGE_BYTES};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use url::Url;

pub(crate) const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const DEFAULT_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const DEFAULT_MAX_SERVERS_PER_PROJECT: usize = 1;
const DEFAULT_MAX_SERVERS_PER_AGENT: usize = 4;
const MAX_STDERR_BYTES: usize = 64 * 1024;
pub(crate) const MAX_DIAGNOSTIC_DOCUMENTS: usize = 256;
pub(crate) const MAX_DIAGNOSTICS_PER_DOCUMENT: usize = 500;

/// Discriminant for one language-server process kind. Language-specific
/// facts (executable, extensions, initialization options, …) live on the
/// matching `LanguageProfile` in the `language` registry; this enum carries
/// no behavior of its own, so adding a language is one variant plus one
/// profile entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LspServerKind {
    RustAnalyzer,
    Pyright,
    TypeScriptLanguageServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

impl PositionEncoding {
    fn from_initialize_result(result: &Value) -> Self {
        match result
            .pointer("/capabilities/positionEncoding")
            .and_then(Value::as_str)
        {
            Some(value) if value.eq_ignore_ascii_case("utf-8") => Self::Utf8,
            Some(value) if value.eq_ignore_ascii_case("utf-32") => Self::Utf32,
            _ => Self::Utf16,
        }
    }

    pub(crate) fn as_public_label(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16 => "utf-16",
            Self::Utf32 => "utf-32",
        }
    }
}

/// Resolution metadata for lsp_status. Never includes absolute executable paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedCommandInfo {
    pub(crate) available: bool,
    pub(crate) source: crate::lsp_bridge::LspCommandSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspServerStatus {
    Initializing,
    Running,
    Crashed,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LspError {
    ServerUnavailable,
    SpawnFailed(String),
    InitializeFailed(String),
    ProtocolError(String),
    MalformedMessage(String),
    RequestTimeout {
        method: String,
        timeout: Duration,
    },
    JsonRpc {
        code: i64,
        message: String,
        // Captured from the server for diagnostic fidelity; the production
        // envelope currently exposes only the bounded message.
        #[cfg_attr(not(test), allow(dead_code))]
        data: Option<Value>,
    },
    WriterFailed(String),
    ServerExited,
    RestartExhausted(String),
    CapacityExceeded {
        limit: usize,
    },
    InvalidProjectRoot(String),
    CallHierarchyUnsupported,
}

impl LspError {
    fn permits_restart(&self) -> bool {
        matches!(
            self,
            Self::SpawnFailed(_)
                | Self::InitializeFailed(_)
                | Self::ProtocolError(_)
                | Self::MalformedMessage(_)
                | Self::WriterFailed(_)
                | Self::ServerExited
        )
    }
}

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServerUnavailable => f.write_str("language server is unavailable"),
            Self::SpawnFailed(message) => write!(f, "failed to spawn language server: {message}"),
            Self::InitializeFailed(message) => {
                write!(f, "language server initialize failed: {message}")
            }
            Self::ProtocolError(message) => write!(f, "LSP protocol error: {message}"),
            Self::MalformedMessage(message) => write!(f, "malformed LSP message: {message}"),
            Self::RequestTimeout { method, timeout } => write!(
                f,
                "LSP request {method} timed out after {}ms",
                timeout.as_millis()
            ),
            Self::JsonRpc { code, message, .. } => {
                write!(f, "LSP server returned JSON-RPC error {code}: {message}")
            }
            Self::WriterFailed(message) => write!(f, "LSP writer failed: {message}"),
            Self::ServerExited => f.write_str("language server exited"),
            Self::RestartExhausted(message) => {
                write!(f, "language server restart exhausted: {message}")
            }
            Self::CapacityExceeded { limit } => {
                write!(f, "language server capacity exceeded (limit {limit})")
            }
            Self::InvalidProjectRoot(message) => write!(f, "invalid project root: {message}"),
            Self::CallHierarchyUnsupported => {
                f.write_str("language server does not support call hierarchy")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LspCommand {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
}

impl LspCommand {
    pub(crate) fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub(crate) fn arg(mut self, value: impl Into<OsString>) -> Self {
        self.args.push(value.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    fn spawn(&self, project_root: &Path) -> Result<webcodex_process::ManagedChild, LspError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .envs(self.env.iter().cloned())
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        webcodex_process::ManagedChild::spawn(&mut command)
            .map_err(|error| LspError::SpawnFailed(error.to_string()))
    }

    fn is_available(&self, kind: LspServerKind) -> bool {
        let program = Path::new(&self.program);
        let resolved = if program.is_absolute() || program.components().count() > 1 {
            if !is_executable_file(program) {
                return false;
            }
            program.to_path_buf()
        } else {
            match find_executable_on_path(&program.to_string_lossy()) {
                Some(path) => path,
                None => return false,
            }
        };
        // A resolved executable can still be unusable (e.g. rustup installs a
        // PATH shim named `rust-analyzer` even when the component is
        // missing). The probe decides without spawning the language server.
        match profile_for_kind(kind).unusable_command_probe {
            Some(probe) => !probe(&resolved),
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LspSupervisorConfig {
    /// Explicitly configured server commands, keyed by server kind. A
    /// configured command wins over the profile env override and `PATH`
    /// lookup for that kind only.
    pub(crate) commands: HashMap<LspServerKind, LspCommand>,
    pub(crate) max_servers_per_project: usize,
    pub(crate) max_servers_per_agent: usize,
    pub(crate) request_timeout: Duration,
    pub(crate) initialize_timeout: Duration,
    pub(crate) shutdown_timeout: Duration,
    pub(crate) idle_ttl: Duration,
    /// Reap idle/unusable servers from a background thread so `idle_ttl` and
    /// capacity recovery work in long-lived agents without explicit
    /// `cleanup_idle` calls. Tests that pin manual `cleanup_idle` semantics
    /// disable this to stay deterministic.
    pub(crate) background_reaper: bool,
}

impl Default for LspSupervisorConfig {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
            max_servers_per_project: DEFAULT_MAX_SERVERS_PER_PROJECT,
            max_servers_per_agent: DEFAULT_MAX_SERVERS_PER_AGENT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            initialize_timeout: DEFAULT_INITIALIZE_TIMEOUT,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
            idle_ttl: DEFAULT_IDLE_TTL,
            background_reaper: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProcessKey {
    project_root: PathBuf,
    kind: LspServerKind,
}

struct ServerSlot {
    state: Mutex<SlotState>,
    ready: Condvar,
}

enum SlotState {
    Starting,
    Running(Arc<ServerInstance>),
    Failed(LspError),
}

struct SupervisorInner {
    config: LspSupervisorConfig,
    servers: Mutex<HashMap<ProcessKey, Arc<ServerSlot>>>,
    shutting_down: Arc<AtomicBool>,
    shutdown_deadline: Arc<Mutex<Option<Instant>>>,
    shutdown_started: AtomicBool,
    shutdown_result: Mutex<Option<LspShutdownOutcome>>,
    shutdown_changed: Condvar,
    reaper_started: AtomicBool,
    reaper_thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LspShutdownOutcome {
    pub(crate) servers: usize,
    pub(crate) timed_out: usize,
    pub(crate) failures: usize,
    pub(crate) reaper_timed_out: bool,
}

#[derive(Clone)]
pub(crate) struct LspSupervisor {
    inner: Arc<SupervisorInner>,
}

impl Default for LspSupervisor {
    fn default() -> Self {
        Self::new(LspSupervisorConfig::default())
    }
}

impl LspSupervisor {
    pub(crate) fn new(config: LspSupervisorConfig) -> Self {
        Self {
            inner: Arc::new(SupervisorInner {
                config,
                servers: Mutex::new(HashMap::new()),
                shutting_down: Arc::new(AtomicBool::new(false)),
                shutdown_deadline: Arc::new(Mutex::new(None)),
                shutdown_started: AtomicBool::new(false),
                shutdown_result: Mutex::new(None),
                shutdown_changed: Condvar::new(),
                reaper_started: AtomicBool::new(false),
                reaper_thread: Mutex::new(None),
            }),
        }
    }

    /// Resolve command availability and source without starting a server and
    /// without returning absolute executable paths.
    pub(crate) fn resolve_command_info(&self, kind: LspServerKind) -> Option<ResolvedCommandInfo> {
        let (command, source) = self.resolve_command_with_source(kind)?;
        Some(ResolvedCommandInfo {
            available: command.is_available(kind),
            source,
        })
    }

    /// Inspect an existing project server slot without starting one.
    pub(crate) fn project_server_status(
        &self,
        project_root: &Path,
        kind: LspServerKind,
    ) -> Option<LspServerStatus> {
        let root = fs::canonicalize(project_root).ok()?;
        let key = ProcessKey {
            project_root: root,
            kind,
        };
        let servers = lock_unpoison(&self.inner.servers);
        let slot = servers.get(&key)?;
        let state = lock_unpoison(&slot.state);
        match &*state {
            SlotState::Starting => Some(LspServerStatus::Initializing),
            SlotState::Running(server) => Some(server.connection.status()),
            SlotState::Failed(_) => Some(LspServerStatus::Crashed),
        }
    }

    pub(crate) fn project_position_encoding(
        &self,
        project_root: &Path,
        kind: LspServerKind,
    ) -> Option<PositionEncoding> {
        let root = fs::canonicalize(project_root).ok()?;
        let key = ProcessKey {
            project_root: root,
            kind,
        };
        let servers = lock_unpoison(&self.inner.servers);
        let slot = servers.get(&key)?;
        let state = lock_unpoison(&slot.state);
        match &*state {
            SlotState::Running(server) => Some(*lock_unpoison(&server.position_encoding)),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn request(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
    ) -> Result<Value, LspError> {
        self.request_with_timeout(
            validated_project_root,
            kind,
            method,
            params,
            self.inner.config.request_timeout,
        )
    }

    pub(crate) fn request_with_document(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        document_uri: &str,
        language_id: &str,
        text: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, LspError> {
        self.request_with_timeout_inner(
            validated_project_root,
            kind,
            method,
            params,
            self.inner.config.request_timeout,
            Some(DocumentOpen {
                uri: document_uri,
                language_id,
                text,
            }),
        )
    }

    pub(crate) fn prepare_document(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        document_uri: &str,
        language_id: &str,
        text: &str,
    ) -> Result<PositionEncoding, LspError> {
        let key = ProcessKey {
            project_root: canonical_project_root(validated_project_root)?,
            kind,
        };
        let document = DocumentOpen {
            uri: document_uri,
            language_id,
            text,
        };
        for attempt in 0..=1 {
            let server = match self.get_or_start(&key, attempt == 1) {
                Ok(server) => server,
                Err(error) if attempt == 0 && error.permits_restart() => continue,
                Err(error) if attempt == 1 && error.permits_restart() => {
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            };
            match server.synchronize_document(document) {
                Ok(_) => return Ok(server.position_encoding()),
                Err(error) if attempt == 0 && error.permits_restart() => continue,
                Err(error) if attempt == 1 && error.permits_restart() => {
                    self.evict_unusable(&key);
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => {
                    if !server.is_usable() {
                        self.evict_unusable(&key);
                    }
                    return Err(error);
                }
            }
        }
        Err(LspError::RestartExhausted(
            "document open restart failed".to_string(),
        ))
    }

    pub(crate) fn document_diagnostics(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        document_uri: &str,
        language_id: &str,
        text: &str,
        deadline: Instant,
    ) -> Result<DiagnosticsSnapshot, LspError> {
        let key = ProcessKey {
            project_root: canonical_project_root(validated_project_root)?,
            kind,
        };
        let document = DocumentOpen {
            uri: document_uri,
            language_id,
            text,
        };
        for attempt in 0..=1 {
            let server = match self.get_or_start(&key, attempt == 1) {
                Ok(server) => server,
                Err(error) if attempt == 0 && error.permits_restart() => continue,
                Err(error) if attempt == 1 && error.permits_restart() => {
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            };
            let baseline_generation = server.diagnostics.generation();
            let document_version = match server.synchronize_document(document) {
                Ok(version) => version,
                Err(error) if attempt == 0 && error.permits_restart() => continue,
                Err(error) if attempt == 1 && error.permits_restart() => {
                    self.evict_unusable(&key);
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            };
            match server.diagnostics.wait_for_publication(
                document_uri,
                document_version,
                baseline_generation,
                deadline,
            ) {
                Ok((publication, timed_out)) => {
                    return Ok(DiagnosticsSnapshot {
                        position_encoding: server.position_encoding(),
                        publication,
                        timed_out,
                    });
                }
                Err(error) if attempt == 0 && error.permits_restart() => continue,
                Err(error) if attempt == 1 && error.permits_restart() => {
                    self.evict_unusable(&key);
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            }
        }
        Err(LspError::RestartExhausted(
            "diagnostics restart failed".to_string(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn request_with_timeout(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        self.request_with_timeout_inner(validated_project_root, kind, method, params, timeout, None)
    }

    pub(crate) fn request_with_position_encoding(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
    ) -> Result<(Value, PositionEncoding), LspError> {
        self.request_with_timeout_and_encoding_inner(
            validated_project_root,
            kind,
            method,
            params,
            self.inner.config.request_timeout,
            None,
            false,
        )
    }

    pub(crate) fn prepare_call_hierarchy(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        document_uri: &str,
        language_id: &str,
        text: &str,
        line: u32,
        character: u32,
    ) -> Result<(Value, PositionEncoding), LspError> {
        self.request_with_timeout_and_encoding_inner(
            validated_project_root,
            kind,
            "textDocument/prepareCallHierarchy",
            json!({
                "textDocument": { "uri": document_uri },
                "position": { "line": line, "character": character }
            }),
            self.inner.config.request_timeout,
            Some(DocumentOpen {
                uri: document_uri,
                language_id,
                text,
            }),
            true,
        )
    }

    pub(crate) fn incoming_call_hierarchy(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        item: Value,
    ) -> Result<(Value, PositionEncoding), LspError> {
        self.request_with_timeout_and_encoding_inner(
            validated_project_root,
            kind,
            "callHierarchy/incomingCalls",
            json!({ "item": item }),
            self.inner.config.request_timeout,
            None,
            true,
        )
    }

    pub(crate) fn outgoing_call_hierarchy(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        item: Value,
    ) -> Result<(Value, PositionEncoding), LspError> {
        self.request_with_timeout_and_encoding_inner(
            validated_project_root,
            kind,
            "callHierarchy/outgoingCalls",
            json!({ "item": item }),
            self.inner.config.request_timeout,
            None,
            true,
        )
    }

    fn request_with_timeout_inner(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
        timeout: Duration,
        document: Option<DocumentOpen<'_>>,
    ) -> Result<Value, LspError> {
        self.request_with_timeout_and_encoding_inner(
            validated_project_root,
            kind,
            method,
            params,
            timeout,
            document,
            false,
        )
        .map(|(value, _)| value)
    }

    fn request_with_timeout_and_encoding_inner(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
        timeout: Duration,
        document: Option<DocumentOpen<'_>>,
        require_call_hierarchy: bool,
    ) -> Result<(Value, PositionEncoding), LspError> {
        let key = ProcessKey {
            project_root: canonical_project_root(validated_project_root)?,
            kind,
        };
        let mut last_error = None;
        for attempt in 0..=1 {
            let server = match self.get_or_start(&key, attempt == 1) {
                Ok(server) => server,
                Err(error) if attempt == 0 && error.permits_restart() => {
                    last_error = Some(error);
                    continue;
                }
                Err(error) if attempt == 1 && error.permits_restart() => {
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            };
            if require_call_hierarchy && !server.call_hierarchy_supported() {
                return Err(LspError::CallHierarchyUnsupported);
            }
            if let Some(document) = document {
                if let Err(error) = server.synchronize_document(document) {
                    if attempt == 0 && error.permits_restart() {
                        last_error = Some(error);
                        continue;
                    }
                    if attempt == 1 && error.permits_restart() {
                        self.evict_unusable(&key);
                        return Err(LspError::RestartExhausted(error.to_string()));
                    }
                    if !server.is_usable() {
                        self.evict_unusable(&key);
                    }
                    return Err(error);
                }
            }
            match server.request(method, params.clone(), timeout) {
                Ok(value) => return Ok((value, server.position_encoding())),
                Err(LspError::JsonRpc { code: -32601, .. }) if require_call_hierarchy => {
                    return Err(LspError::CallHierarchyUnsupported);
                }
                Err(error) if attempt == 0 && error.permits_restart() => {
                    last_error = Some(error);
                }
                Err(error) if attempt == 1 && error.permits_restart() => {
                    // Final attempt failed: do not leave a crashed-but-alive
                    // child occupying capacity forever.
                    self.evict_unusable(&key);
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => {
                    if !server.is_usable() {
                        self.evict_unusable(&key);
                    }
                    return Err(error);
                }
            }
        }
        Err(LspError::RestartExhausted(
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "restart failed".to_string()),
        ))
    }

    /// Reap unusable slots and idle servers past `idle_ttl`. Called
    /// periodically by the background reaper thread and directly by tests.
    pub(crate) fn cleanup_idle(&self) -> usize {
        let now = Instant::now();
        let mut removed = Vec::new();
        {
            let mut servers = lock_unpoison(&self.inner.servers);
            let keys = servers
                .iter()
                .filter_map(|(key, slot)| {
                    let state = lock_unpoison(&slot.state);
                    match &*state {
                        SlotState::Running(server) if !server.is_usable() => Some(key.clone()),
                        SlotState::Running(server)
                            if server.pending_count() == 0
                                && now.saturating_duration_since(server.last_used())
                                    >= self.inner.config.idle_ttl =>
                        {
                            Some(key.clone())
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            for key in keys {
                if let Some(slot) = servers.remove(&key) {
                    removed.push(slot);
                }
            }
        }
        let count = removed.len();
        // Never perform slow shutdown while holding the supervisor map lock.
        shutdown_slots(removed, Instant::now() + self.inner.config.shutdown_timeout);
        count
    }

    #[cfg(test)]
    pub(crate) fn shutdown(&self) {
        let _ = self.shutdown_until(Instant::now() + self.inner.config.shutdown_timeout);
    }

    pub(crate) fn begin_shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        if let Some(reaper) = lock_unpoison(&self.inner.reaper_thread).as_ref() {
            reaper.thread().unpark();
        }
    }

    pub(crate) fn begin_shutdown_until(&self, deadline: Instant) {
        {
            let mut stored = lock_unpoison(&self.inner.shutdown_deadline);
            *stored = Some(stored.map_or(deadline, |current| current.min(deadline)));
        }
        self.begin_shutdown();
    }

    pub(crate) fn shutdown_until(&self, deadline: Instant) -> LspShutdownOutcome {
        self.begin_shutdown_until(deadline);
        if self.inner.shutdown_started.swap(true, Ordering::SeqCst) {
            let mut result = lock_unpoison(&self.inner.shutdown_result);
            while result.is_none() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return LspShutdownOutcome {
                        timed_out: 1,
                        ..LspShutdownOutcome::default()
                    };
                }
                let (next, _) = self
                    .inner
                    .shutdown_changed
                    .wait_timeout(result, remaining.min(Duration::from_millis(25)))
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                result = next;
            }
            return result.unwrap_or_default();
        }
        let slots = {
            let mut servers = lock_unpoison(&self.inner.servers);
            servers.drain().map(|(_, slot)| slot).collect::<Vec<_>>()
        };
        let mut outcome = shutdown_slots(slots, deadline);
        outcome.reaper_timed_out = !self.stop_reaper_until(deadline);
        let mut result = lock_unpoison(&self.inner.shutdown_result);
        *result = Some(outcome);
        self.inner.shutdown_changed.notify_all();
        outcome
    }

    fn stop_reaper_until(&self, deadline: Instant) -> bool {
        let mut handle = lock_unpoison(&self.inner.reaper_thread).take();
        let Some(handle) = handle.take() else {
            return true;
        };
        handle.thread().unpark();
        while !handle.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                *lock_unpoison(&self.inner.reaper_thread) = Some(handle);
                return false;
            }
            thread::sleep(Duration::from_millis(5).min(remaining));
        }
        let _ = handle.join();
        true
    }

    /// Start the background idle reaper once, on the first server demand.
    /// Idle supervisors (status-only probes, tests without servers) never
    /// spawn the thread. The thread holds only a `Weak` reference between
    /// passes, so it exits on drop or shutdown instead of pinning the
    /// supervisor alive.
    fn ensure_reaper_started(&self) {
        if !self.inner.config.background_reaper {
            return;
        }
        if self.inner.reaper_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        let interval = reaper_interval(self.inner.config.idle_ttl);
        let spawned = thread::Builder::new()
            .name("webcodex-lsp-reaper".to_string())
            .spawn(move || loop {
                thread::park_timeout(interval);
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                if inner.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                // Hold the strong reference only for one cleanup pass so a
                // concurrent drop is delayed by at most one pass.
                LspSupervisor { inner }.cleanup_idle();
            });
        match spawned {
            Ok(handle) => {
                let mut stored = lock_unpoison(&self.inner.reaper_thread);
                if self.inner.shutting_down.load(Ordering::SeqCst) {
                    handle.thread().unpark();
                }
                *stored = Some(handle);
            }
            Err(error) => {
                // Degraded mode: capacity recovery falls back to explicit
                // eviction on failed requests, exactly as before the reaper.
                tracing::debug!(error = %error, "LSP idle reaper thread failed to start");
            }
        }
    }

    fn get_or_start(
        &self,
        key: &ProcessKey,
        retry_failed: bool,
    ) -> Result<Arc<ServerInstance>, LspError> {
        if self.inner.shutting_down.load(Ordering::SeqCst) {
            return Err(LspError::ServerUnavailable);
        }
        self.ensure_reaper_started();
        let (slot, owns_start) = {
            let mut servers = lock_unpoison(&self.inner.servers);
            // A completed failed start has no process to reuse and must not
            // consume capacity forever. Keep entries that still have waiters;
            // their callers will observe the same failed start generation.
            servers.retain(|_, slot| {
                Arc::strong_count(slot) > 1
                    || !matches!(&*lock_unpoison(&slot.state), SlotState::Failed(_))
            });
            if let Some(slot) = servers.get(key) {
                (Arc::clone(slot), false)
            } else {
                self.check_capacity(&servers, key)?;
                let slot = Arc::new(ServerSlot {
                    state: Mutex::new(SlotState::Starting),
                    ready: Condvar::new(),
                });
                servers.insert(key.clone(), Arc::clone(&slot));
                (slot, true)
            }
        };

        let mut waited = false;
        let mut should_start = owns_start;
        let mut stale_server = None;
        if !owns_start {
            let mut state = lock_unpoison(&slot.state);
            loop {
                match &*state {
                    SlotState::Starting => {
                        waited = true;
                        state = wait_unpoison(&slot.ready, state);
                    }
                    SlotState::Running(server) if server.is_usable() => {
                        return Ok(Arc::clone(server));
                    }
                    SlotState::Running(server) => {
                        // Crashed/unhealthy connection (including alive child
                        // without a working reader) must not be reused.
                        if !retry_failed {
                            return Err(LspError::ServerExited);
                        }
                        stale_server = Some(Arc::clone(server));
                        *state = SlotState::Starting;
                        should_start = true;
                        break;
                    }
                    SlotState::Failed(error) => {
                        if !retry_failed || waited {
                            return Err(error.clone());
                        }
                        *state = SlotState::Starting;
                        should_start = true;
                        break;
                    }
                }
            }
        }

        if should_start {
            if let Some(server) = stale_server {
                // Reap the stale instance outside the slot lock. Even when the
                // child is still alive after a reader crash, kill/wait it.
                let _ = server.shutdown_until(Instant::now() + self.inner.config.shutdown_timeout);
            }
            let result = self.start_server(key);
            if self.inner.shutting_down.load(Ordering::SeqCst) {
                if let Ok(server) = &result {
                    let _ = server.shutdown_until(
                        lock_unpoison(&self.inner.shutdown_deadline)
                            .unwrap_or_else(|| Instant::now() + self.inner.config.shutdown_timeout),
                    );
                }
                let mut state = lock_unpoison(&slot.state);
                *state = SlotState::Failed(LspError::ServerUnavailable);
                slot.ready.notify_all();
                return Err(LspError::ServerUnavailable);
            }
            let mut state = lock_unpoison(&slot.state);
            match &result {
                Ok(server) => *state = SlotState::Running(Arc::clone(server)),
                Err(error) => *state = SlotState::Failed(error.clone()),
            }
            slot.ready.notify_all();
            return result;
        }
        Err(LspError::ProtocolError(
            "language server start coordination failed".to_string(),
        ))
    }

    fn check_capacity(
        &self,
        servers: &HashMap<ProcessKey, Arc<ServerSlot>>,
        key: &ProcessKey,
    ) -> Result<(), LspError> {
        if servers.len() >= self.inner.config.max_servers_per_agent {
            return Err(LspError::CapacityExceeded {
                limit: self.inner.config.max_servers_per_agent,
            });
        }
        let project_count = servers
            .keys()
            .filter(|existing| existing.project_root == key.project_root)
            .count();
        if project_count >= self.inner.config.max_servers_per_project {
            return Err(LspError::CapacityExceeded {
                limit: self.inner.config.max_servers_per_project,
            });
        }
        Ok(())
    }

    fn start_server(&self, key: &ProcessKey) -> Result<Arc<ServerInstance>, LspError> {
        let command = self
            .resolve_command(key.kind)
            .ok_or(LspError::ServerUnavailable)?;
        ServerInstance::start(
            key.clone(),
            command,
            self.inner.config.initialize_timeout,
            self.inner.config.shutdown_timeout,
            Arc::clone(&self.inner.shutting_down),
            Arc::clone(&self.inner.shutdown_deadline),
        )
    }

    /// Remove and shut down a Running slot that is no longer usable. Does not
    /// hold the supervisor map lock across the potentially slow shutdown.
    fn evict_unusable(&self, key: &ProcessKey) {
        let slot = {
            let mut servers = lock_unpoison(&self.inner.servers);
            let remove = servers.get(key).is_some_and(|slot| {
                matches!(
                    &*lock_unpoison(&slot.state),
                    SlotState::Running(server) if !server.is_usable()
                )
            });
            if remove {
                servers.remove(key)
            } else {
                None
            }
        };
        if let Some(slot) = slot {
            shutdown_slots(
                vec![slot],
                Instant::now() + self.inner.config.shutdown_timeout,
            );
        }
    }

    fn resolve_command(&self, kind: LspServerKind) -> Option<LspCommand> {
        self.resolve_command_with_source(kind)
            .map(|(command, _)| command)
    }

    fn resolve_command_with_source(
        &self,
        kind: LspServerKind,
    ) -> Option<(LspCommand, crate::lsp_bridge::LspCommandSource)> {
        self.resolve_command_from_sources(
            kind,
            env::var_os(profile_for_kind(kind).env_override),
            env::var_os("PATH").as_deref(),
        )
    }

    /// Resolution priority per kind: explicitly configured command, then the
    /// profile's env override, then the profile executable on `PATH`. An
    /// explicitly configured command is used verbatim; env/PATH resolutions
    /// get the profile's `default_args` (e.g. `--stdio`) appended.
    fn resolve_command_from_sources(
        &self,
        kind: LspServerKind,
        env_override: Option<OsString>,
        path: Option<&OsStr>,
    ) -> Option<(LspCommand, crate::lsp_bridge::LspCommandSource)> {
        if let Some(command) = self.inner.config.commands.get(&kind) {
            return Some((
                command.clone(),
                crate::lsp_bridge::LspCommandSource::Configured,
            ));
        }
        let profile = profile_for_kind(kind);
        if let Some(program) = env_override {
            if !program.is_empty() {
                #[cfg(windows)]
                {
                    // Resolve the override through the platform rules: bare
                    // names are searched on PATH with PATHEXT, `.cmd` shims
                    // resolve as batch programs, and extensionless POSIX
                    // shims are never selected. An unresolvable override
                    // means the tool is unavailable (fail closed rather than
                    // spawning something that would error 193 at runtime).
                    let ambient_path = env::var_os("PATH");
                    let search_path = match path {
                        Some(path) => path,
                        None => ambient_path.as_deref().unwrap_or(OsStr::new("")),
                    };
                    let resolved =
                        resolve_program_in_path(&program.to_string_lossy(), search_path)?;
                    let resolved: OsString = resolved.path().as_os_str().to_os_string();
                    return Some((
                        command_with_default_args(resolved, profile),
                        crate::lsp_bridge::LspCommandSource::Environment,
                    ));
                }
                #[cfg(not(windows))]
                {
                    let _ = path;
                    return Some((
                        command_with_default_args(program, profile),
                        crate::lsp_bridge::LspCommandSource::Environment,
                    ));
                }
            }
        }
        path.and_then(|path| find_executable_in_path(profile.executable, path))
            .map(|program| {
                (
                    command_with_default_args(program, profile),
                    crate::lsp_bridge::LspCommandSource::Path,
                )
            })
    }

    #[cfg(test)]
    fn server_for_test(
        &self,
        root: &Path,
        kind: LspServerKind,
    ) -> Result<Arc<ServerInstance>, LspError> {
        self.get_or_start(
            &ProcessKey {
                project_root: canonical_project_root(root)?,
                kind,
            },
            false,
        )
    }

    #[cfg(test)]
    fn server_count_for_test(&self) -> usize {
        lock_unpoison(&self.inner.servers).len()
    }
}

impl Drop for SupervisorInner {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Ok(reaper) = self.reaper_thread.get_mut() {
            if let Some(reaper) = reaper.as_ref() {
                reaper.thread().unpark();
            }
        }
        let slots = match self.servers.get_mut() {
            Ok(servers) => servers.drain().map(|(_, slot)| slot).collect::<Vec<_>>(),
            Err(poisoned) => poisoned
                .into_inner()
                .drain()
                .map(|(_, slot)| slot)
                .collect::<Vec<_>>(),
        };
        let deadline = lock_unpoison(&self.shutdown_deadline)
            .unwrap_or_else(|| Instant::now() + self.config.shutdown_timeout);
        shutdown_slots(slots, deadline);
        let handle = match self.reaper_thread.get_mut() {
            Ok(handle) => handle.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(handle) = handle {
            join_owned_thread_until(handle, deadline);
        }
    }
}

fn shutdown_slots(slots: Vec<Arc<ServerSlot>>, deadline: Instant) -> LspShutdownOutcome {
    let mut outcome = LspShutdownOutcome::default();
    for slot in slots {
        let server = {
            let mut state = lock_unpoison(&slot.state);
            match &*state {
                SlotState::Running(server) => Some(Arc::clone(server)),
                SlotState::Starting => {
                    *state = SlotState::Failed(LspError::ServerUnavailable);
                    slot.ready.notify_all();
                    None
                }
                SlotState::Failed(_) => None,
            }
        };
        if let Some(server) = server {
            outcome.servers += 1;
            let result = server.shutdown_until(deadline);
            if result.timed_out {
                outcome.timed_out += 1;
            }
            if let Some(error) = result.error {
                outcome.failures += 1;
                tracing::debug!(error = %error, "LSP server shutdown was not graceful");
            }
        }
    }
    outcome
}

fn canonical_project_root(root: &Path) -> Result<PathBuf, LspError> {
    let canonical =
        fs::canonicalize(root).map_err(|error| LspError::InvalidProjectRoot(error.to_string()))?;
    if !canonical.is_dir() {
        return Err(LspError::InvalidProjectRoot(
            "project root is not a directory".to_string(),
        ));
    }
    Ok(canonical)
}

/// Build a command from a resolved program plus the profile's default args.
/// Used for env-override and `PATH` resolutions; explicitly configured
/// commands already carry their own arguments.
fn command_with_default_args(
    program: impl Into<OsString>,
    profile: &LanguageProfile,
) -> LspCommand {
    let mut command = LspCommand::new(program);
    for arg in profile.default_args {
        command = command.arg(*arg);
    }
    command
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    find_executable_in_path(name, &path)
}

/// True when `path` is a rustup proxy for `rust-analyzer` whose active
/// toolchain does not install the component binary.
///
/// Detection is filesystem-only: no process spawn, no Cargo, no network.
/// When toolchain resolution is ambiguous, returns `false` so callers keep
/// the previous PATH-executable semantics instead of inventing unavailability.
/// Wired into the rust `LanguageProfile` as its `unusable_command_probe`.
pub(super) fn is_unusable_rustup_proxy(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|name| name.to_str());
    if file_name != Some("rust-analyzer") && file_name != Some("rust-analyzer.exe") {
        return false;
    }
    if !looks_like_rustup_proxy(path) {
        return false;
    }
    let Some(toolchain) = active_rustup_toolchain() else {
        return false;
    };
    let Some(rustup_home) = rustup_home_dir() else {
        return false;
    };
    let component = rustup_home
        .join("toolchains")
        .join(toolchain)
        .join("bin")
        .join(format!("rust-analyzer{}", env::consts::EXE_SUFFIX));
    !is_executable_file(&component)
}

fn looks_like_rustup_proxy(path: &Path) -> bool {
    // rustup installs cargo-bin shims as symlinks (or hardlinks) to `rustup`.
    // Follow one level of symlink; if the target basename is `rustup`, treat
    // this as a proxy. Hardlinks share the same file as a nearby `rustup`.
    if let Ok(target) = fs::read_link(path) {
        let target_name = target.file_name().and_then(|name| name.to_str());
        if target_name == Some("rustup") || target_name == Some("rustup.exe") {
            return true;
        }
        // Absolute/relative multi-component links that still resolve to rustup.
        let resolved = if target.is_absolute() {
            target
        } else if let Some(parent) = path.parent() {
            parent.join(target)
        } else {
            return false;
        };
        let name = resolved.file_name().and_then(|name| name.to_str());
        return name == Some("rustup") || name == Some("rustup.exe");
    }
    // Hardlink / same-file check against sibling `rustup` next to the shim.
    let Some(parent) = path.parent() else {
        return false;
    };
    let sibling = parent.join(format!("rustup{}", env::consts::EXE_SUFFIX));
    if !sibling.exists() {
        return false;
    }
    match (fs::metadata(path), fs::metadata(&sibling)) {
        (Ok(left), Ok(right)) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                left.dev() == right.dev() && left.ino() == right.ino()
            }
            #[cfg(not(unix))]
            {
                let _ = (left, right);
                false
            }
        }
        _ => false,
    }
}

fn rustup_home_dir() -> Option<PathBuf> {
    if let Some(value) = env::var_os("RUSTUP_HOME") {
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    // Shared home policy: `HOME` on Unix, `USERPROFILE` on Windows (HOME is
    // ignored on Windows — it is either absent or a Git Bash POSIX path).
    webcodex_agent_config::paths::home_dir().map(|home| home.join(".rustup"))
}

fn active_rustup_toolchain() -> Option<String> {
    if let Ok(value) = env::var("RUSTUP_TOOLCHAIN") {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !trimmed.contains(['/', '\\', '\n', '\r', '\0']) {
            return Some(trimmed.to_string());
        }
    }
    let settings = rustup_home_dir()?.join("settings.toml");
    let contents = fs::read_to_string(settings).ok()?;
    parse_default_toolchain(&contents)
}

fn parse_default_toolchain(settings_toml: &str) -> Option<String> {
    // Minimal TOML scrape for `default_toolchain = "..."` — avoids a new
    // dependency and ignores overrides (path-specific toolchains).
    for line in settings_toml.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("default_toolchain") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        let value = rest
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| rest.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(rest)
            .trim();
        if !value.is_empty() && !value.contains(['/', '\\', '\n', '\r', '\0']) {
            return Some(value.to_string());
        }
    }
    None
}

/// Collapse control characters to spaces and trim; `None` when the result is
/// empty. Shared by the generic startup summary and per-language stderr
/// classifiers in the `language` registry.
pub(super) fn compact_stderr(raw: &str) -> Option<String> {
    let compact = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let compact = compact.trim().to_string();
    if compact.is_empty() {
        None
    } else {
        Some(compact)
    }
}

/// Generic, language-agnostic fallback: map bounded stderr capture into a
/// short, path-light startup diagnostic (its first non-empty line). Stable
/// per-language classification runs first via the profile's
/// `startup_stderr_classifier`; this holds no per-language knowledge.
fn generic_startup_stderr_summary(raw: &str) -> Option<String> {
    let compact = compact_stderr(raw)?;
    // Absolute paths are redacted by `bound_error_message` at the public
    // bridge boundary.
    let first = compact
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let safe: String = first.chars().take(160).collect();
    if first.chars().count() > 160 {
        Some(format!("{safe}…"))
    } else if safe.is_empty() {
        None
    } else {
        Some(safe)
    }
}

fn combine_initialize_failure(error: &LspError, stderr_summary: Option<String>) -> String {
    let base = match error {
        LspError::InitializeFailed(message) => message.clone(),
        other => other.to_string(),
    };
    match stderr_summary {
        Some(summary) if !base.contains(&summary) => format!("{base}: {summary}"),
        _ => base,
    }
}

fn remaining_until(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// Reaper cadence: a fraction of the idle TTL, bounded so long TTLs still
/// notice shutdown within a minute and tiny test TTLs do not spin.
fn reaper_interval(idle_ttl: Duration) -> Duration {
    (idle_ttl / 4).clamp(Duration::from_millis(10), Duration::from_secs(60))
}

struct ConnectionState {
    pending: Mutex<HashMap<u64, mpsc::Sender<Result<Value, LspError>>>>,
    status: Mutex<LspServerStatus>,
}

impl ConnectionState {
    fn fail_pending(&self, error: LspError) {
        *lock_unpoison(&self.status) = LspServerStatus::Crashed;
        let pending = {
            let mut pending = lock_unpoison(&self.pending);
            pending
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        };
        for sender in pending {
            let _ = sender.send(Err(error.clone()));
        }
    }

    fn status(&self) -> LspServerStatus {
        *lock_unpoison(&self.status)
    }
}

struct BoundedStderr {
    bytes: VecDeque<u8>,
}

impl BoundedStderr {
    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= MAX_STDERR_BYTES {
            self.bytes.clear();
            self.bytes
                .extend(chunk[chunk.len() - MAX_STDERR_BYTES..].iter().copied());
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(MAX_STDERR_BYTES);
        self.bytes.drain(..overflow);
        self.bytes.extend(chunk.iter().copied());
    }
}

#[derive(Clone, Copy)]
struct DocumentOpen<'a> {
    uri: &'a str,
    language_id: &'a str,
    text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenDocumentState {
    version: i32,
    content_fingerprint: [u8; 32],
}

#[derive(Debug, Clone)]
pub(crate) struct DiagnosticsPublication {
    pub(crate) generation: u64,
    pub(crate) version: Option<i32>,
    pub(crate) received_at: Instant,
    pub(crate) diagnostics: Vec<Value>,
    pub(crate) raw_diagnostics_count: usize,
    pub(crate) related_information_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DiagnosticsSnapshot {
    pub(crate) position_encoding: PositionEncoding,
    pub(crate) publication: Option<DiagnosticsPublication>,
    pub(crate) timed_out: bool,
}

#[derive(Default)]
struct DiagnosticsCacheState {
    generation: u64,
    publications: HashMap<String, DiagnosticsPublication>,
    closed: bool,
}

#[derive(Default)]
struct DiagnosticsCache {
    state: Mutex<DiagnosticsCacheState>,
    changed: Condvar,
    malformed_notifications: AtomicU64,
}

impl DiagnosticsCache {
    fn generation(&self) -> u64 {
        lock_unpoison(&self.state).generation
    }

    fn record_publish_diagnostics(&self, params: Option<&Value>) {
        let Some(params) = params.and_then(Value::as_object) else {
            self.record_malformed();
            return;
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            self.record_malformed();
            return;
        };
        if uri.is_empty() || uri.chars().count() > 4096 {
            self.record_malformed();
            return;
        }
        let version = match params.get("version") {
            None | Some(Value::Null) => None,
            Some(value) => match value.as_i64().and_then(|value| i32::try_from(value).ok()) {
                Some(version) => Some(version),
                None => {
                    self.record_malformed();
                    return;
                }
            },
        };
        let Some(raw_diagnostics) = params.get("diagnostics").and_then(Value::as_array) else {
            self.record_malformed();
            return;
        };
        let raw_diagnostics_count = raw_diagnostics.len();
        let related_information_count = raw_diagnostics
            .iter()
            .map(|diagnostic| {
                diagnostic
                    .get("relatedInformation")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)
            })
            .sum();
        let diagnostics = raw_diagnostics
            .iter()
            .take(MAX_DIAGNOSTICS_PER_DOCUMENT)
            .cloned()
            .collect::<Vec<_>>();

        let mut state = lock_unpoison(&self.state);
        state.generation = state.generation.saturating_add(1);
        let generation = state.generation;
        if !state.publications.contains_key(uri)
            && state.publications.len() >= MAX_DIAGNOSTIC_DOCUMENTS
        {
            let oldest = state
                .publications
                .iter()
                .min_by_key(|(_, publication)| (publication.received_at, publication.generation))
                .map(|(uri, _)| uri.clone());
            if let Some(oldest) = oldest {
                state.publications.remove(&oldest);
            }
        }
        state.publications.insert(
            uri.to_string(),
            DiagnosticsPublication {
                generation,
                version,
                received_at: Instant::now(),
                diagnostics,
                raw_diagnostics_count,
                related_information_count,
            },
        );
        drop(state);
        self.changed.notify_all();
    }

    fn wait_for_publication(
        &self,
        uri: &str,
        document_version: i32,
        baseline_generation: u64,
        deadline: Instant,
    ) -> Result<(Option<DiagnosticsPublication>, bool), LspError> {
        let mut state = lock_unpoison(&self.state);
        loop {
            if let Some(publication) = state.publications.get(uri) {
                let fresh = publication.generation > baseline_generation
                    || publication.version == Some(document_version);
                if fresh {
                    return Ok((Some(publication.clone()), false));
                }
            }
            if state.closed {
                return Err(LspError::ServerExited);
            }
            let remaining = remaining_until(deadline);
            if remaining.is_zero() {
                return Ok((state.publications.get(uri).cloned(), true));
            }
            let waited = self.changed.wait_timeout(state, remaining);
            state = match waited {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    fn mark_closed(&self) {
        lock_unpoison(&self.state).closed = true;
        self.changed.notify_all();
    }

    fn record_malformed(&self) {
        self.malformed_notifications.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn malformed_notification_count(&self) -> u64 {
        self.malformed_notifications.load(Ordering::Relaxed)
    }
}

fn document_fingerprint(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn synchronize_document_state(
    documents: &Mutex<HashMap<String, OpenDocumentState>>,
    document: DocumentOpen<'_>,
    mut notify: impl FnMut(&str, Value) -> Result<(), LspError>,
) -> Result<i32, LspError> {
    let fingerprint = document_fingerprint(document.text);
    // Serialize comparison, notification, and commit for a URI. The state
    // changes only after the notification is accepted by the writer.
    let mut documents = lock_unpoison(documents);
    match documents.get(document.uri).copied() {
        Some(state) if state.content_fingerprint == fingerprint => Ok(state.version),
        Some(state) => {
            let version = state.version.checked_add(1).ok_or_else(|| {
                LspError::ProtocolError("LSP document version exhausted".to_string())
            })?;
            notify(
                "textDocument/didChange",
                json!({
                    "textDocument": {
                        "uri": document.uri,
                        "version": version,
                    },
                    "contentChanges": [{
                        "text": document.text,
                    }],
                }),
            )?;
            documents.insert(
                document.uri.to_string(),
                OpenDocumentState {
                    version,
                    content_fingerprint: fingerprint,
                },
            );
            Ok(version)
        }
        None => {
            notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": document.uri,
                        "languageId": document.language_id,
                        "version": 1,
                        "text": document.text,
                    }
                }),
            )?;
            documents.insert(
                document.uri.to_string(),
                OpenDocumentState {
                    version: 1,
                    content_fingerprint: fingerprint,
                },
            );
            Ok(1)
        }
    }
}

struct ServerInstance {
    key: ProcessKey,
    child: Mutex<webcodex_process::ManagedChild>,
    writer: Arc<Mutex<ChildStdin>>,
    connection: Arc<ConnectionState>,
    next_id: AtomicU64,
    position_encoding: Mutex<PositionEncoding>,
    call_hierarchy_supported: AtomicBool,
    open_documents: Mutex<HashMap<String, OpenDocumentState>>,
    diagnostics: Arc<DiagnosticsCache>,
    last_used: Mutex<Instant>,
    stderr: Arc<Mutex<BoundedStderr>>,
    reader_thread: Mutex<Option<JoinHandle<()>>>,
    stderr_thread: Mutex<Option<JoinHandle<()>>>,
    shutdown_started: AtomicBool,
    supervisor_shutdown: Arc<AtomicBool>,
    global_shutdown_deadline: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug)]
struct ServerShutdownResult {
    timed_out: bool,
    error: Option<String>,
}

impl ServerInstance {
    fn start(
        key: ProcessKey,
        command: LspCommand,
        initialize_timeout: Duration,
        shutdown_timeout: Duration,
        supervisor_shutdown: Arc<AtomicBool>,
        global_shutdown_deadline: Arc<Mutex<Option<Instant>>>,
    ) -> Result<Arc<Self>, LspError> {
        let cleanup_deadline = || {
            lock_unpoison(&global_shutdown_deadline)
                .unwrap_or_else(|| Instant::now() + shutdown_timeout)
        };
        let mut managed = command.spawn(&key.project_root)?;
        let Some(stdin) = managed.child_mut().stdin.take() else {
            cleanup_failed_lsp_child(&mut managed, cleanup_deadline());
            return Err(LspError::SpawnFailed(
                "stdin pipe was unavailable".to_string(),
            ));
        };
        let Some(stdout) = managed.child_mut().stdout.take() else {
            cleanup_failed_lsp_child(&mut managed, cleanup_deadline());
            return Err(LspError::SpawnFailed(
                "stdout pipe was unavailable".to_string(),
            ));
        };
        let Some(stderr) = managed.child_mut().stderr.take() else {
            cleanup_failed_lsp_child(&mut managed, cleanup_deadline());
            return Err(LspError::SpawnFailed(
                "stderr pipe was unavailable".to_string(),
            ));
        };
        let writer = Arc::new(Mutex::new(stdin));
        let connection = Arc::new(ConnectionState {
            pending: Mutex::new(HashMap::new()),
            status: Mutex::new(LspServerStatus::Initializing),
        });
        let stderr_buffer = Arc::new(Mutex::new(BoundedStderr {
            bytes: VecDeque::new(),
        }));
        let diagnostics = Arc::new(DiagnosticsCache::default());

        let reader_connection = Arc::clone(&connection);
        let reader_writer = Arc::clone(&writer);
        let reader_diagnostics = Arc::clone(&diagnostics);
        let reader_thread = match thread::Builder::new()
            .name("webcodex-lsp-reader".to_string())
            .spawn(move || {
                reader_loop(stdout, reader_writer, reader_connection, reader_diagnostics)
            }) {
            Ok(thread) => thread,
            Err(error) => {
                cleanup_failed_lsp_child(&mut managed, cleanup_deadline());
                return Err(LspError::SpawnFailed(error.to_string()));
            }
        };

        let drain_buffer = Arc::clone(&stderr_buffer);
        let stderr_thread = match thread::Builder::new()
            .name("webcodex-lsp-stderr".to_string())
            .spawn(move || {
                let mut stderr = stderr;
                let mut chunk = [0_u8; 4096];
                while let Ok(read) = stderr.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    lock_unpoison(&drain_buffer).push(&chunk[..read]);
                }
            }) {
            Ok(thread) => thread,
            Err(error) => {
                cleanup_failed_lsp_child(&mut managed, cleanup_deadline());
                if reader_thread.is_finished() {
                    let _ = reader_thread.join();
                }
                return Err(LspError::SpawnFailed(error.to_string()));
            }
        };

        let server = Arc::new(Self {
            key,
            child: Mutex::new(managed),
            writer,
            connection,
            next_id: AtomicU64::new(1),
            position_encoding: Mutex::new(PositionEncoding::Utf16),
            call_hierarchy_supported: AtomicBool::new(false),
            open_documents: Mutex::new(HashMap::new()),
            diagnostics,
            last_used: Mutex::new(Instant::now()),
            stderr: stderr_buffer,
            reader_thread: Mutex::new(Some(reader_thread)),
            stderr_thread: Mutex::new(Some(stderr_thread)),
            shutdown_started: AtomicBool::new(false),
            supervisor_shutdown,
            global_shutdown_deadline,
        });

        if let Err(error) = server.initialize(initialize_timeout) {
            // Use the configured shutdown budget, never a fixed default.
            // Shutdown also joins the stderr drain thread so the bounded capture
            // is complete before we classify the failure.
            let deadline = lock_unpoison(&server.global_shutdown_deadline)
                .unwrap_or_else(|| Instant::now() + shutdown_timeout);
            let _ = server.shutdown_until(deadline);
            let stderr_summary = server.startup_stderr_summary();
            return Err(LspError::InitializeFailed(combine_initialize_failure(
                &error,
                stderr_summary,
            )));
        }
        Ok(server)
    }

    /// Bounded, path-light summary of captured language-server stderr for
    /// initialize/start failures. Never returns absolute executable paths.
    ///
    /// The language profile's `startup_stderr_classifier` gets first refusal
    /// for stable, known-failure messages; otherwise the generic first-line
    /// summary applies. No per-language stderr knowledge lives here.
    fn startup_stderr_summary(&self) -> Option<String> {
        let bytes: Vec<u8> = lock_unpoison(&self.stderr).bytes.iter().copied().collect();
        if bytes.is_empty() {
            return None;
        }
        let raw = String::from_utf8_lossy(&bytes);
        if let Some(classify) = profile_for_kind(self.key.kind).startup_stderr_classifier {
            if let Some(summary) = classify(&raw) {
                return Some(summary);
            }
        }
        generic_startup_stderr_summary(&raw)
    }

    fn initialize(&self, timeout: Duration) -> Result<(), LspError> {
        let root_uri = Url::from_directory_path(&self.key.project_root).map_err(|_| {
            LspError::InitializeFailed("project root is not a file URI".to_string())
        })?;
        // WebCodex LSP tools are read-only semantic navigation. Starting the
        // language server must not implicitly execute repository build
        // scripts, proc macros, or dependency fetches. Each language profile
        // carries its constrained read-only `initializationOptions`, pinned
        // by security regression tests.
        let initialization_options = (profile_for_kind(self.key.kind).initialization_options)();
        let result = self.request_raw(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": {"name": "WebCodex agent"},
                "rootUri": root_uri.to_string(),
                "initializationOptions": initialization_options,
                "capabilities": {
                    "general": {
                        "positionEncodings": ["utf-8", "utf-16", "utf-32"]
                    },
                    "textDocument": {
                        "callHierarchy": {
                            "dynamicRegistration": false
                        }
                    }
                }
            }),
            timeout,
            true,
        )?;
        *lock_unpoison(&self.position_encoding) = PositionEncoding::from_initialize_result(&result);
        self.call_hierarchy_supported.store(
            result
                .pointer("/capabilities/callHierarchyProvider")
                .is_some_and(|provider| provider == &Value::Bool(true) || provider.is_object()),
            Ordering::SeqCst,
        );
        self.notify("initialized", json!({}))?;
        if !self.is_alive() {
            return Err(LspError::ServerExited);
        }
        let mut status = lock_unpoison(&self.connection.status);
        if *status == LspServerStatus::Crashed {
            return Err(LspError::ServerExited);
        }
        *status = LspServerStatus::Running;
        Ok(())
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, LspError> {
        if self.shutdown_started.load(Ordering::SeqCst) {
            return Err(LspError::ServerExited);
        }
        if !self.is_usable() {
            return Err(LspError::ServerExited);
        }
        match self.request_raw(method, params, timeout, true) {
            Err(LspError::RequestTimeout { method, timeout })
                if !self.is_alive() || self.connection.status() == LspServerStatus::Crashed =>
            {
                // Prefer an explicit exit/crash over a bare timeout when the
                // connection is already known to be dead.
                let _ = method;
                let _ = timeout;
                Err(LspError::ServerExited)
            }
            result => result,
        }
    }

    fn position_encoding(&self) -> PositionEncoding {
        *lock_unpoison(&self.position_encoding)
    }

    fn call_hierarchy_supported(&self) -> bool {
        self.call_hierarchy_supported.load(Ordering::SeqCst)
    }

    fn request_raw(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        observe_supervisor_shutdown: bool,
    ) -> Result<Value, LspError> {
        if observe_supervisor_shutdown && self.supervisor_shutdown.load(Ordering::SeqCst) {
            return Err(LspError::ServerExited);
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        lock_unpoison(&self.connection.pending).insert(id, sender);
        // Reflect request start (pending registration) so idle cleanup does not
        // race a just-started call while pending_count is still catching up.
        self.touch_last_used();
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(error) = self.write(&message) {
            lock_unpoison(&self.connection.pending).remove(&id);
            self.touch_last_used();
            return Err(error);
        }
        let deadline = Instant::now() + timeout;
        let result = loop {
            if observe_supervisor_shutdown && self.supervisor_shutdown.load(Ordering::SeqCst) {
                lock_unpoison(&self.connection.pending).remove(&id);
                break Err(LspError::ServerExited);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                // Remove before cancellation so a late response is ignored and
                // cannot re-wake this wait or panic on a dropped sender.
                lock_unpoison(&self.connection.pending).remove(&id);
                self.send_cancel_request(id);
                break Err(LspError::RequestTimeout {
                    method: method.to_string(),
                    timeout,
                });
            }
            match receiver.recv_timeout(remaining.min(Duration::from_millis(25))) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err(LspError::ServerExited);
                }
            }
        };
        // Reflect request completion, failure, or timeout.
        self.touch_last_used();
        result
    }

    /// Best-effort `$/cancelRequest`. Failures must not replace the original
    /// timeout error; callers still return `RequestTimeout`. Allowed during
    /// shutdown so a hung `shutdown` request can still be cancelled without
    /// re-entering `shutdown()` (this method never calls shutdown).
    fn send_cancel_request(&self, id: u64) {
        if self.connection.status() == LspServerStatus::Crashed {
            return;
        }
        let message = json!({
            "jsonrpc": "2.0",
            "method": "$/cancelRequest",
            "params": { "id": id },
        });
        // Avoid self.write()'s error mapping path for the timeout return value;
        // only mark the connection crashed when the writer is truly broken.
        let write_result = {
            let mut writer = lock_unpoison(&self.writer);
            write_message(&mut *writer, &message)
        };
        if let Err(error) = write_result {
            self.connection
                .fail_pending(LspError::WriterFailed(error.to_string()));
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn synchronize_document(&self, document: DocumentOpen<'_>) -> Result<i32, LspError> {
        let version =
            synchronize_document_state(&self.open_documents, document, |method, params| {
                self.notify(method, params)
            })?;
        self.touch_last_used();
        Ok(version)
    }

    fn write(&self, message: &Value) -> Result<(), LspError> {
        let mut writer = lock_unpoison(&self.writer);
        write_message(&mut *writer, message).map_err(|error| {
            let error = LspError::WriterFailed(error.to_string());
            self.connection.fail_pending(error.clone());
            error
        })
    }

    /// True when the instance may safely serve ordinary requests.
    ///
    /// Requires: child still running, connection status `Running`, shutdown not
    /// started. `Initializing` is never usable for ordinary callers.
    fn is_usable(&self) -> bool {
        if self.shutdown_started.load(Ordering::SeqCst) {
            return false;
        }
        if self.connection.status() != LspServerStatus::Running {
            return false;
        }
        self.is_alive()
    }

    fn is_alive(&self) -> bool {
        let exited = match lock_unpoison(&self.child).try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => true,
        };
        if exited {
            self.connection.fail_pending(LspError::ServerExited);
        }
        !exited
    }

    fn process_running(&self) -> bool {
        match lock_unpoison(&self.child).try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => false,
        }
    }

    fn last_used(&self) -> Instant {
        *lock_unpoison(&self.last_used)
    }

    fn touch_last_used(&self) {
        *lock_unpoison(&self.last_used) = Instant::now();
    }

    fn pending_count(&self) -> usize {
        lock_unpoison(&self.connection.pending).len()
    }

    fn shutdown_until(&self, deadline: Instant) -> ServerShutdownResult {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return ServerShutdownResult {
                timed_out: !self.process_reaped_and_group_gone(),
                error: None,
            };
        }
        const KILL_REAP_RESERVE: Duration = Duration::from_millis(150);
        let mut graceful_error = None;

        let status = self.connection.status();
        let can_attempt_graceful = status == LspServerStatus::Running && self.process_running();

        if can_attempt_graceful {
            // Healthy Running connection: request shutdown, then exit, then wait
            // for natural exit while reserving a bounded kill/reap tail.
            let graceful_deadline = deadline
                .checked_sub(KILL_REAP_RESERVE)
                .unwrap_or_else(Instant::now);
            let remaining = remaining_until(graceful_deadline);
            if !remaining.is_zero() {
                if let Err(error) = self.request_raw("shutdown", Value::Null, remaining, false) {
                    graceful_error = Some(error.to_string());
                } else {
                    let _ = self.notify("exit", Value::Null);
                }
            }
            if !self.wait_child_and_tree(graceful_deadline) {
                if let Err(error) = self.kill_and_reap_child(deadline) {
                    graceful_error = Some(error);
                }
            }
        } else {
            // Crashed, initializing-failure, writer-failure, or other unusable
            // state: never wait the full deadline for a natural exit. Kill
            // immediately and use only the caller's shared deadline to reap.
            if let Err(error) = self.kill_and_reap_child(deadline) {
                graceful_error = Some(error);
            }
        }

        self.connection.fail_pending(LspError::ServerExited);
        // Closing the process pipes should unblock reader/stderr promptly.
        join_thread_until(&self.reader_thread, deadline);
        join_thread_until(&self.stderr_thread, deadline);

        let timed_out = !self.process_reaped_and_group_gone()
            || lock_unpoison(&self.reader_thread).is_some()
            || lock_unpoison(&self.stderr_thread).is_some();
        ServerShutdownResult {
            timed_out,
            error: graceful_error,
        }
    }

    fn kill_and_reap_child(&self, deadline: Instant) -> Result<(), String> {
        let mut errors = Vec::new();
        // Forcefully terminate the whole managed tree. Idempotent when the
        // tree is already gone, so it is safe to call unconditionally.
        if lock_unpoison(&self.child).terminate_tree().is_err() {
            errors.push("tree_terminate_failed".to_string());
        }
        if !self.wait_child_and_tree(deadline) {
            errors.push("tree_reap_timed_out".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(","))
        }
    }

    /// Wait for both the direct child and the complete managed tree to exit,
    /// using only the caller's shared deadline.
    fn wait_child_and_tree(&self, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if self.process_reaped_and_group_gone() {
                return true;
            }
            let remaining = remaining_until(deadline);
            if remaining.is_zero() {
                break;
            }
            thread::sleep(Duration::from_millis(10).min(remaining));
        }
        self.process_reaped_and_group_gone()
    }

    fn process_reaped_and_group_gone(&self) -> bool {
        // The direct child must be reaped AND the complete managed process
        // tree must be empty. A direct-child exit alone is never "gone".
        let mut child = lock_unpoison(&self.child);
        let child_reaped = matches!(child.try_wait(), Ok(Some(_)) | Err(_));
        child_reaped && child.wait_tree_exit(Duration::ZERO).unwrap_or(false)
    }

    #[cfg(test)]
    fn status(&self) -> LspServerStatus {
        self.connection.status()
    }

    #[cfg(test)]
    fn stderr_len(&self) -> usize {
        lock_unpoison(&self.stderr).bytes.len()
    }

    #[cfg(test)]
    fn process_id(&self) -> u32 {
        lock_unpoison(&self.child).id()
    }
}

impl Drop for ServerInstance {
    fn drop(&mut self) {
        // Must never panic. Uses the same single-deadline shutdown path.
        let _ = self.shutdown_until(Instant::now() + DEFAULT_SHUTDOWN_TIMEOUT);
    }
}

fn reader_loop(
    stdout: impl Read,
    writer: Arc<Mutex<ChildStdin>>,
    connection: Arc<ConnectionState>,
    diagnostics: Arc<DiagnosticsCache>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let message = match read_message(&mut reader, MAX_LSP_MESSAGE_BYTES) {
            Ok(message) => message,
            Err(error) => {
                diagnostics.mark_closed();
                connection.fail_pending(framing_to_lsp_error(error));
                return;
            }
        };
        if let Err(error) = handle_incoming_message(&message, &writer, &connection, &diagnostics) {
            diagnostics.mark_closed();
            connection.fail_pending(error);
            return;
        }
    }
}

fn handle_incoming_message(
    message: &Value,
    writer: &Arc<Mutex<ChildStdin>>,
    connection: &Arc<ConnectionState>,
    diagnostics: &DiagnosticsCache,
) -> Result<(), LspError> {
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(LspError::ProtocolError(
            "message does not declare jsonrpc 2.0".to_string(),
        ));
    }
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        if let Some(id) = message.get("id") {
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Method not found: {method}")},
            });
            write_message(&mut *lock_unpoison(writer), &response)
                .map_err(|error| LspError::WriterFailed(error.to_string()))?;
        } else if method == "textDocument/publishDiagnostics" {
            diagnostics.record_publish_diagnostics(message.get("params"));
        }
        return Ok(());
    }
    let id = message
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| LspError::ProtocolError("response has no numeric id".to_string()))?;
    let sender = lock_unpoison(&connection.pending).remove(&id);
    let Some(sender) = sender else {
        // Late response after timeout/cancel: ignore safely.
        return Ok(());
    };
    let result = if let Some(error) = message.get("error") {
        Err(LspError::JsonRpc {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown JSON-RPC error")
                .to_string(),
            data: error.get("data").cloned(),
        })
    } else if let Some(result) = message.get("result") {
        Ok(result.clone())
    } else {
        Err(LspError::ProtocolError(
            "response has neither result nor error".to_string(),
        ))
    };
    let _ = sender.send(result);
    Ok(())
}

fn framing_to_lsp_error(error: FramingError) -> LspError {
    match error {
        FramingError::Io(io_error) if io_error.kind() == std::io::ErrorKind::UnexpectedEof => {
            LspError::ServerExited
        }
        FramingError::Io(io_error) if io_error.kind() == std::io::ErrorKind::InvalidData => {
            LspError::MalformedMessage(io_error.to_string())
        }
        other => LspError::ProtocolError(other.to_string()),
    }
}

/// Join a helper thread using only the remaining budget of a shared deadline.
///
/// If the thread has not finished by the deadline the handle is left in place
/// so a later `Drop` can retry; we never re-arm a full independent timeout.
fn join_thread_until(thread: &Mutex<Option<JoinHandle<()>>>, deadline: Instant) {
    loop {
        let finished = lock_unpoison(thread)
            .as_ref()
            .map(JoinHandle::is_finished)
            .unwrap_or(true);
        if finished {
            break;
        }
        let remaining = remaining_until(deadline);
        if remaining.is_zero() {
            break;
        }
        thread::sleep(Duration::from_millis(5).min(remaining));
    }
    let handle = {
        let mut guard = lock_unpoison(thread);
        if guard.as_ref().is_some_and(JoinHandle::is_finished) {
            guard.take()
        } else {
            None
        }
    };
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn join_owned_thread_until(handle: JoinHandle<()>, deadline: Instant) {
    let handle = handle;
    while !handle.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        thread::sleep(Duration::from_millis(5).min(remaining));
    }
    let _ = handle.join();
}

/// Clean up an LSP child that failed early (missing pipe or thread spawn).
///
/// The server can never be used, so the whole managed tree is forcefully
/// terminated, then the direct child and the whole tree are reaped within the
/// shared deadline. Never re-arms a fresh wait.
fn cleanup_failed_lsp_child(managed: &mut webcodex_process::ManagedChild, deadline: Instant) {
    let _ = managed.terminate_tree();
    while Instant::now() < deadline {
        match managed.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {
                let remaining = remaining_until(deadline);
                if remaining.is_zero() {
                    break;
                }
                thread::sleep(Duration::from_millis(10).min(remaining));
            }
        }
    }
    let remaining = remaining_until(deadline);
    let _ = managed.wait_tree_exit(remaining);
}

fn lock_unpoison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_unpoison<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectUriClassification {
    InsideProject(PathBuf),
    OutsideProject,
    Unsupported,
}

pub(crate) fn classify_uri_against_project_root(
    canonical_project_root: &Path,
    uri: &str,
) -> ProjectUriClassification {
    let Ok(uri) = Url::parse(uri) else {
        return ProjectUriClassification::Unsupported;
    };
    if uri.scheme() != "file" {
        return ProjectUriClassification::Unsupported;
    }
    // A `file:` URI that cannot be mapped to a local filesystem path can
    // never refer to a file inside the project, so it is external rather
    // than unsupported. On Windows this includes POSIX-style absolute URIs
    // (`file:///usr/lib/...`): the url crate's `to_file_path` requires a
    // drive-letter or UNC prefix there, so such URIs fail conversion and
    // must still count as outside the project boundary.
    let Ok(path) = uri.to_file_path() else {
        return ProjectUriClassification::OutsideProject;
    };
    let Ok(path) = fs::canonicalize(path) else {
        return ProjectUriClassification::OutsideProject;
    };
    if path.starts_with(canonical_project_root) {
        ProjectUriClassification::InsideProject(path)
    } else {
        ProjectUriClassification::OutsideProject
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
