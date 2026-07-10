use super::protocol::{read_message, write_message, FramingError, MAX_LSP_MESSAGE_BYTES};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LspServerKind {
    RustAnalyzer,
}

impl LspServerKind {
    fn executable_name(self) -> &'static str {
        match self {
            Self::RustAnalyzer => "rust-analyzer",
        }
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspServerStatus {
    Available,
    Unavailable,
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
        data: Option<Value>,
    },
    WriterFailed(String),
    ServerExited,
    RestartExhausted(String),
    CapacityExceeded {
        limit: usize,
    },
    InvalidProjectRoot(String),
    ShutdownFailed(String),
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
            Self::ServerUnavailable => f.write_str("rust-analyzer is unavailable"),
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
            Self::ShutdownFailed(message) => {
                write!(f, "language server shutdown failed: {message}")
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

    pub(crate) fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    fn spawn(&self, project_root: &Path) -> Result<Child, LspError> {
        Command::new(&self.program)
            .args(&self.args)
            .envs(self.env.iter().cloned())
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| LspError::SpawnFailed(error.to_string()))
    }

    fn is_available(&self) -> bool {
        let program = Path::new(&self.program);
        if program.is_absolute() || program.components().count() > 1 {
            is_executable_file(program)
        } else {
            find_executable_on_path(&program.to_string_lossy()).is_some()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LspSupervisorConfig {
    pub(crate) rust_analyzer: Option<LspCommand>,
    pub(crate) max_servers_per_project: usize,
    pub(crate) max_servers_per_agent: usize,
    pub(crate) request_timeout: Duration,
    pub(crate) initialize_timeout: Duration,
    pub(crate) shutdown_timeout: Duration,
    pub(crate) idle_ttl: Duration,
}

impl Default for LspSupervisorConfig {
    fn default() -> Self {
        Self {
            rust_analyzer: None,
            max_servers_per_project: DEFAULT_MAX_SERVERS_PER_PROJECT,
            max_servers_per_agent: DEFAULT_MAX_SERVERS_PER_AGENT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            initialize_timeout: DEFAULT_INITIALIZE_TIMEOUT,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
            idle_ttl: DEFAULT_IDLE_TTL,
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
    shutting_down: AtomicBool,
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
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn availability(&self, kind: LspServerKind) -> LspServerStatus {
        if self
            .resolve_command(kind)
            .is_some_and(|command| command.is_available())
        {
            LspServerStatus::Available
        } else {
            LspServerStatus::Unavailable
        }
    }

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

    pub(crate) fn request_with_timeout(
        &self,
        validated_project_root: &Path,
        kind: LspServerKind,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
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
            match server.request(method, params.clone(), timeout) {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 0 && error.permits_restart() => {
                    last_error = Some(error);
                }
                Err(error) if attempt == 1 && error.permits_restart() => {
                    return Err(LspError::RestartExhausted(error.to_string()));
                }
                Err(error) => return Err(error),
            }
        }
        Err(LspError::RestartExhausted(
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "restart failed".to_string()),
        ))
    }

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
                        SlotState::Running(server)
                            if now.saturating_duration_since(server.last_used())
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
        shutdown_slots(removed, self.inner.config.shutdown_timeout);
        count
    }

    pub(crate) fn shutdown(&self) {
        if self.inner.shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let slots = {
            let mut servers = lock_unpoison(&self.inner.servers);
            servers.drain().map(|(_, slot)| slot).collect::<Vec<_>>()
        };
        shutdown_slots(slots, self.inner.config.shutdown_timeout);
    }

    fn get_or_start(
        &self,
        key: &ProcessKey,
        retry_failed: bool,
    ) -> Result<Arc<ServerInstance>, LspError> {
        if self.inner.shutting_down.load(Ordering::SeqCst) {
            return Err(LspError::ServerUnavailable);
        }
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
                    SlotState::Running(server) if server.is_alive() => {
                        return Ok(Arc::clone(server));
                    }
                    SlotState::Running(server) => {
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
                let _ = server.shutdown(self.inner.config.shutdown_timeout);
            }
            let result = self.start_server(key);
            if self.inner.shutting_down.load(Ordering::SeqCst) {
                if let Ok(server) = &result {
                    let _ = server.shutdown(self.inner.config.shutdown_timeout);
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
        ServerInstance::start(key.clone(), command, self.inner.config.initialize_timeout)
    }

    fn resolve_command(&self, kind: LspServerKind) -> Option<LspCommand> {
        self.resolve_command_from_sources(
            kind,
            env::var_os("WEBCODEX_RUST_ANALYZER"),
            env::var_os("PATH").as_deref(),
        )
    }

    fn resolve_command_from_sources(
        &self,
        kind: LspServerKind,
        rust_analyzer_override: Option<OsString>,
        path: Option<&OsStr>,
    ) -> Option<LspCommand> {
        if let Some(command) = &self.inner.config.rust_analyzer {
            return Some(command.clone());
        }
        if kind == LspServerKind::RustAnalyzer {
            if let Some(program) = rust_analyzer_override {
                if !program.is_empty() {
                    return Some(LspCommand::new(program));
                }
            }
        }
        path.and_then(|path| find_executable_in_path(kind.executable_name(), path))
            .map(LspCommand::new)
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
}

impl Drop for SupervisorInner {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let slots = match self.servers.get_mut() {
            Ok(servers) => servers.drain().map(|(_, slot)| slot).collect::<Vec<_>>(),
            Err(poisoned) => poisoned
                .into_inner()
                .drain()
                .map(|(_, slot)| slot)
                .collect::<Vec<_>>(),
        };
        shutdown_slots(slots, self.config.shutdown_timeout);
    }
}

fn shutdown_slots(slots: Vec<Arc<ServerSlot>>, timeout: Duration) {
    for slot in slots {
        let server = {
            let state = lock_unpoison(&slot.state);
            match &*state {
                SlotState::Running(server) => Some(Arc::clone(server)),
                SlotState::Starting | SlotState::Failed(_) => None,
            }
        };
        if let Some(server) = server {
            if let Err(error) = server.shutdown(timeout) {
                tracing::debug!(error = %error, "LSP server shutdown was not graceful");
            }
        }
    }
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

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    find_executable_in_path(name, &path)
}

fn find_executable_in_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
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

struct ServerInstance {
    key: ProcessKey,
    child: Mutex<Child>,
    writer: Arc<Mutex<ChildStdin>>,
    connection: Arc<ConnectionState>,
    next_id: AtomicU64,
    position_encoding: Mutex<PositionEncoding>,
    last_used: Mutex<Instant>,
    stderr: Arc<Mutex<BoundedStderr>>,
    reader_thread: Mutex<Option<JoinHandle<()>>>,
    stderr_thread: Mutex<Option<JoinHandle<()>>>,
    shutdown_started: AtomicBool,
}

impl ServerInstance {
    fn start(
        key: ProcessKey,
        command: LspCommand,
        initialize_timeout: Duration,
    ) -> Result<Arc<Self>, LspError> {
        let mut child = command.spawn(&key.project_root)?;
        let Some(stdin) = child.stdin.take() else {
            terminate_child(&mut child);
            return Err(LspError::SpawnFailed(
                "stdin pipe was unavailable".to_string(),
            ));
        };
        let Some(stdout) = child.stdout.take() else {
            terminate_child(&mut child);
            return Err(LspError::SpawnFailed(
                "stdout pipe was unavailable".to_string(),
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            terminate_child(&mut child);
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

        let reader_connection = Arc::clone(&connection);
        let reader_writer = Arc::clone(&writer);
        let reader_thread = match thread::Builder::new()
            .name("webcodex-lsp-reader".to_string())
            .spawn(move || reader_loop(stdout, reader_writer, reader_connection))
        {
            Ok(thread) => thread,
            Err(error) => {
                terminate_child(&mut child);
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
                terminate_child(&mut child);
                if reader_thread.is_finished() {
                    let _ = reader_thread.join();
                }
                return Err(LspError::SpawnFailed(error.to_string()));
            }
        };

        let server = Arc::new(Self {
            key,
            child: Mutex::new(child),
            writer,
            connection,
            next_id: AtomicU64::new(1),
            position_encoding: Mutex::new(PositionEncoding::Utf16),
            last_used: Mutex::new(Instant::now()),
            stderr: stderr_buffer,
            reader_thread: Mutex::new(Some(reader_thread)),
            stderr_thread: Mutex::new(Some(stderr_thread)),
            shutdown_started: AtomicBool::new(false),
        });

        if let Err(error) = server.initialize(initialize_timeout) {
            let _ = server.shutdown(DEFAULT_SHUTDOWN_TIMEOUT);
            return Err(LspError::InitializeFailed(error.to_string()));
        }
        Ok(server)
    }

    fn initialize(&self, timeout: Duration) -> Result<(), LspError> {
        let root_uri = Url::from_directory_path(&self.key.project_root).map_err(|_| {
            LspError::InitializeFailed("project root is not a file URI".to_string())
        })?;
        let result = self.request_raw(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": {"name": "WebCodex agent"},
                "rootUri": root_uri.to_string(),
                "capabilities": {
                    "general": {
                        "positionEncodings": ["utf-8", "utf-16", "utf-32"]
                    }
                }
            }),
            timeout,
        )?;
        *lock_unpoison(&self.position_encoding) = PositionEncoding::from_initialize_result(&result);
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
        if !self.is_alive() {
            return Err(LspError::ServerExited);
        }
        match self.request_raw(method, params, timeout) {
            Err(LspError::RequestTimeout { .. })
                if !self.is_alive()
                    || *lock_unpoison(&self.connection.status) == LspServerStatus::Crashed =>
            {
                Err(LspError::ServerExited)
            }
            result => result,
        }
    }

    fn request_raw(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        *lock_unpoison(&self.last_used) = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        lock_unpoison(&self.connection.pending).insert(id, sender);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(error) = self.write(&message) {
            lock_unpoison(&self.connection.pending).remove(&id);
            return Err(error);
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lock_unpoison(&self.connection.pending).remove(&id);
                Err(LspError::RequestTimeout {
                    method: method.to_string(),
                    timeout,
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(LspError::ServerExited),
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn write(&self, message: &Value) -> Result<(), LspError> {
        let mut writer = lock_unpoison(&self.writer);
        write_message(&mut *writer, message).map_err(|error| {
            let error = LspError::WriterFailed(error.to_string());
            self.connection.fail_pending(error.clone());
            error
        })
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

    fn last_used(&self) -> Instant {
        *lock_unpoison(&self.last_used)
    }

    fn shutdown(&self, timeout: Duration) -> Result<(), LspError> {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let mut graceful_error = None;
        if self.is_alive() {
            if let Err(error) = self.request_raw("shutdown", Value::Null, timeout) {
                graceful_error = Some(error.to_string());
            }
            let _ = self.notify("exit", Value::Null);
        }

        let deadline = Instant::now() + timeout;
        let mut exited = false;
        while Instant::now() < deadline {
            match lock_unpoison(&self.child).try_wait() {
                Ok(Some(_)) => {
                    exited = true;
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    graceful_error = Some(error.to_string());
                    break;
                }
            }
        }
        if !exited {
            let mut child = lock_unpoison(&self.child);
            let _ = child.kill();
            child
                .wait()
                .map_err(|error| LspError::ShutdownFailed(error.to_string()))?;
        }
        self.connection.fail_pending(LspError::ServerExited);
        join_if_finished(&self.reader_thread, timeout);
        join_if_finished(&self.stderr_thread, timeout);
        if let Some(error) = graceful_error {
            return Err(LspError::ShutdownFailed(error));
        }
        Ok(())
    }

    #[cfg(test)]
    fn position_encoding(&self) -> PositionEncoding {
        *lock_unpoison(&self.position_encoding)
    }

    #[cfg(test)]
    fn status(&self) -> LspServerStatus {
        *lock_unpoison(&self.connection.status)
    }

    #[cfg(test)]
    fn pending_count(&self) -> usize {
        lock_unpoison(&self.connection.pending).len()
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
        let _ = self.shutdown(DEFAULT_SHUTDOWN_TIMEOUT);
    }
}

fn reader_loop(
    stdout: impl Read,
    writer: Arc<Mutex<ChildStdin>>,
    connection: Arc<ConnectionState>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let message = match read_message(&mut reader, MAX_LSP_MESSAGE_BYTES) {
            Ok(message) => message,
            Err(error) => {
                connection.fail_pending(framing_to_lsp_error(error));
                return;
            }
        };
        if let Err(error) = handle_incoming_message(&message, &writer, &connection) {
            connection.fail_pending(error);
            return;
        }
    }
}

fn handle_incoming_message(
    message: &Value,
    writer: &Arc<Mutex<ChildStdin>>,
    connection: &Arc<ConnectionState>,
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
        }
        return Ok(());
    }
    let id = message
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| LspError::ProtocolError("response has no numeric id".to_string()))?;
    let sender = lock_unpoison(&connection.pending).remove(&id);
    let Some(sender) = sender else {
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

fn join_if_finished(thread: &Mutex<Option<JoinHandle<()>>>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let finished = lock_unpoison(thread)
            .as_ref()
            .map(JoinHandle::is_finished)
            .unwrap_or(true);
        if finished || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let handle = {
        let mut handle = lock_unpoison(thread);
        if handle.as_ref().is_some_and(JoinHandle::is_finished) {
            handle.take()
        } else {
            None
        }
    };
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
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
    let Ok(path) = uri.to_file_path() else {
        return ProjectUriClassification::Unsupported;
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
