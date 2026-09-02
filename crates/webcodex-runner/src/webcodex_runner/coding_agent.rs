use super::config::{AcpAgentConfig, AcpConfig};
use super::projects::load_runner_project_summaries_from_dir;
use super::shell::canonicalize_existing;
use super::shutdown::{ActivityTracker, BackgroundThreads};
use agent_client_protocol_schema::v1::{
    NewSessionResponse, PromptResponse, RequestPermissionRequest, SessionConfigKind,
    SessionConfigOption, SessionConfigSelectOptions, SetSessionConfigOptionResponse, StopReason,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;
#[cfg(all(test, unix))]
use webcodex_core::coding_agent::CodingAgentCancelRequest;
use webcodex_core::coding_agent::{
    validate_coding_agent_run_snapshot, validate_request, CodingAgentConfigValue,
    CodingAgentDispatchState, CodingAgentEvent, CodingAgentEventKind, CodingAgentExecutionState,
    CodingAgentObserveResult, CodingAgentProvider, CodingAgentRequest, CodingAgentResponse,
    CodingAgentResponsePayload, CodingAgentRunInventory, CodingAgentRunSnapshot,
    CodingAgentRunState, CodingAgentTerminal, CodingAgentUsage,
    CODING_AGENT_MAX_EVENTS_PER_RESPONSE, CODING_AGENT_MAX_INVENTORY_RUNS,
    CODING_AGENT_MAX_RETAINED_EVENTS, CODING_AGENT_STOP_REASON_CANCELLED,
    CODING_AGENT_STOP_REASON_END_TURN, CODING_AGENT_STOP_REASON_MAX_TOKENS,
    CODING_AGENT_STOP_REASON_MAX_TURN_REQUESTS, CODING_AGENT_STOP_REASON_REFUSAL,
};
use webcodex_process::ManagedChild;
use webcodex_runner_config::paths::paths_equal;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

const STORE_SCHEMA_VERSION: u32 = 1;
const STORE_FILE: &str = "state.json";
const STORE_MAX_BYTES: usize = 64 * 1024;
const STORE_RETENTION_SECS: i64 = 15 * 60;
const ACP_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
const ACP_SETUP_TIMEOUT: Duration = Duration::from_secs(30);
const ACP_CANCEL_GRACE: Duration = Duration::from_secs(5);
const ACP_POLL: Duration = Duration::from_millis(25);
const ACP_IO_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN: &str =
    "coding_agent_terminal_persistence_uncertain";
const TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE: &str = "ACP terminal transition was observed in memory, but its durable commit was not confirmed; reconcile or reobserve instead of treating the original terminal outcome as durable truth";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DurableDispatchPhase {
    BeforePromptBarrier,
    PromptDispatchMayHaveOccurred,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableRunRecord {
    schema_version: u32,
    run_id: String,
    intent_fingerprint: String,
    authority_fingerprint: String,
    runtime_project_id: String,
    provider_id: String,
    provider_instance_id: String,
    state: CodingAgentRunState,
    execution_state: CodingAgentExecutionState,
    dispatch_phase: DurableDispatchPhase,
    created_at: i64,
    updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal: Option<CodingAgentTerminal>,
}

impl DurableRunRecord {
    fn snapshot(&self, observation_revision: u64) -> CodingAgentRunSnapshot {
        CodingAgentRunSnapshot {
            run_id: self.run_id.clone(),
            intent_fingerprint: self.intent_fingerprint.clone(),
            authority_fingerprint: self.authority_fingerprint.clone(),
            runtime_project_id: self.runtime_project_id.clone(),
            provider_id: self.provider_id.clone(),
            provider_instance_id: self.provider_instance_id.clone(),
            state: self.state.clone(),
            execution_state: self.execution_state,
            observation_revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
            terminal: self.terminal.clone(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TerminalWriteGateState {
    reached: bool,
    released: bool,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TerminalWriteGate {
    state: Mutex<TerminalWriteGateState>,
    changed: Condvar,
}

#[cfg(test)]
impl TerminalWriteGate {
    fn block_writer(&self) {
        let mut state = self.state.lock().unwrap();
        state.reached = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).unwrap();
        }
    }

    fn wait_until_reached(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap();
        while !state.reached {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "terminal durable write gate was never reached"
            );
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.released = true;
        self.changed.notify_all();
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct DurableRunStoreTestControl {
    fail_next_terminal_writes: std::sync::atomic::AtomicUsize,
    terminal_write_gate: Mutex<Option<Arc<TerminalWriteGate>>>,
}

#[derive(Debug, Clone)]
struct DurableRunStore {
    root: PathBuf,
    #[cfg(test)]
    test_control: Arc<DurableRunStoreTestControl>,
}

impl DurableRunStore {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            #[cfg(test)]
            test_control: Arc::new(DurableRunStoreTestControl::default()),
        }
    }

    fn default_root(client_id: &str, server_url: &str) -> Result<PathBuf, String> {
        let server_url = server_url.trim().trim_end_matches('/');
        if client_id.trim().is_empty() || server_url.is_empty() {
            return Err("ACP durable store requires non-empty Runner identity".to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex-coding-agent-store-runner-v1\0");
        hasher.update(client_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(server_url.as_bytes());
        let namespace = format!("{:x}", hasher.finalize());
        Ok(
            webcodex_runner_config::paths::default_client_state_base_dir()?
                .join("runner-coding-agent-runs-v1")
                .join(namespace),
        )
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(run_id.as_bytes());
        self.root.join(format!("{:x}", hasher.finalize()))
    }

    fn state_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join(STORE_FILE)
    }

    fn read(&self, run_id: &str) -> Result<Option<DurableRunRecord>, String> {
        let path = self.state_path(run_id);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return match fs::symlink_metadata(self.run_dir(run_id)) {
                    Ok(_) => Err(
                        "ACP Run state is missing from an existing Run state directory".to_string(),
                    ),
                    Err(dir_error) if dir_error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(dir_error) => {
                        Err(format!("failed to inspect ACP Run state dir: {dir_error}"))
                    }
                };
            }
            Err(error) => return Err(format!("failed to open ACP Run state: {error}")),
        };
        let len = file
            .metadata()
            .map_err(|error| format!("failed to inspect ACP Run state: {error}"))?
            .len() as usize;
        if len == 0 || len > STORE_MAX_BYTES {
            return Err("ACP Run state has invalid bounded size".to_string());
        }
        let mut bytes = Vec::with_capacity(len);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read ACP Run state: {error}"))?;
        let record: DurableRunRecord =
            serde_json::from_slice(&bytes).map_err(|_| "ACP Run state is malformed".to_string())?;
        validate_durable_record(&record)?;
        if record.run_id != run_id
            || self.run_dir(&record.run_id) != path.parent().unwrap_or(Path::new(""))
        {
            return Err("ACP Run state identity mismatch".to_string());
        }
        Ok(Some(record))
    }

    fn write(&self, record: &DurableRunRecord) -> Result<(), String> {
        validate_durable_record(record)?;
        let bytes =
            serde_json::to_vec(record).map_err(|_| "failed to encode ACP Run state".to_string())?;
        if bytes.len() > STORE_MAX_BYTES {
            return Err("ACP Run state exceeds durable bound".to_string());
        }
        #[cfg(test)]
        if record.dispatch_phase == DurableDispatchPhase::Terminal {
            if let Some(gate) = self
                .test_control
                .terminal_write_gate
                .lock()
                .unwrap()
                .clone()
            {
                gate.block_writer();
            }
            if self
                .test_control
                .fail_next_terminal_writes
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                    (remaining > 0).then(|| remaining - 1)
                })
                .is_ok()
            {
                return Err("injected ACP terminal durable write failure".to_string());
            }
        }
        let dir = self.run_dir(&record.run_id);
        fs::create_dir_all(&dir)
            .map_err(|error| format!("failed to create ACP Run state dir: {error}"))?;
        let temp = dir.join(format!("state.{}.tmp", Uuid::new_v4().simple()));
        let state_path = self.state_path(&record.run_id);
        let result = (|| {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| format!("failed to create ACP Run temp state: {error}"))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("failed to persist ACP Run state: {error}"))?;
            drop(file);
            publish_state_file(&temp, &state_path)?;
            sync_parent(&dir)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn scan(&self) -> Result<Vec<DurableRunRecord>, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("failed to list ACP Run state: {error}")),
        };
        let mut records = Vec::new();
        let mut entry_count = 0usize;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("failed to inspect ACP Run state: {error}"))?;
            let ty = entry
                .file_type()
                .map_err(|error| format!("failed to inspect ACP Run state: {error}"))?;
            if !ty.is_dir() || ty.is_symlink() {
                return Err("ACP Run state root contains an unexpected entry".to_string());
            }
            entry_count = entry_count.saturating_add(1);
            if entry_count > CODING_AGENT_MAX_INVENTORY_RUNS {
                return Err("ACP Run durable state exceeds bounded record count".to_string());
            }
            let path = entry.path().join(STORE_FILE);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(state_path = %path.display(), error = %error, "ACP Run durable state unavailable during recovery; preserving tombstone");
                    continue;
                }
            };
            if bytes.is_empty() || bytes.len() > STORE_MAX_BYTES {
                tracing::error!(state_path = %path.display(), "ACP Run durable state has invalid bounded size; preserving tombstone");
                continue;
            }
            let record: DurableRunRecord = match serde_json::from_slice(&bytes) {
                Ok(record) => record,
                Err(_) => {
                    tracing::error!(state_path = %path.display(), "ACP Run durable state is malformed; preserving tombstone");
                    continue;
                }
            };
            if let Err(error) = validate_durable_record(&record) {
                tracing::error!(state_path = %path.display(), error = %error, "ACP Run durable state is invalid; preserving tombstone");
                continue;
            }
            if self.run_dir(&record.run_id) != entry.path() {
                tracing::error!(state_path = %path.display(), "ACP Run durable state directory identity mismatch; preserving tombstone");
                continue;
            }
            records.push(record);
        }
        records.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        Ok(records)
    }

    fn remove(&self, run_id: &str) {
        let _ = fs::remove_dir_all(self.run_dir(run_id));
    }

    #[cfg(test)]
    fn fail_next_terminal_writes(&self, count: usize) {
        self.test_control
            .fail_next_terminal_writes
            .store(count, Ordering::SeqCst);
    }

    #[cfg(test)]
    fn set_terminal_write_gate(&self, gate: Option<Arc<TerminalWriteGate>>) {
        *self.test_control.terminal_write_gate.lock().unwrap() = gate;
    }
}

#[cfg(unix)]
fn publish_state_file(temp: &Path, state_path: &Path) -> Result<(), String> {
    fs::rename(temp, state_path)
        .map_err(|error| format!("failed to publish ACP Run state: {error}"))
}

#[cfg(windows)]
fn publish_state_file(temp: &Path, state_path: &Path) -> Result<(), String> {
    let from = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = state_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
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
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        let retryable = matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33));
        if !retryable || Instant::now() >= retry_deadline {
            return Err(format!("failed to publish ACP Run state: {error}"));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(not(any(unix, windows)))]
fn publish_state_file(_temp: &Path, _state_path: &Path) -> Result<(), String> {
    Err("ACP Run durable state is unsupported on this platform".to_string())
}

fn sync_parent(_dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(_dir)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("failed to sync ACP Run state dir: {error}"))?;
    }
    Ok(())
}

fn validate_durable_record(record: &DurableRunRecord) -> Result<(), String> {
    if record.schema_version != STORE_SCHEMA_VERSION {
        return Err("ACP Run durable record schema is invalid".to_string());
    }
    validate_coding_agent_run_snapshot(&record.snapshot(0))
        .map_err(|error| format!("ACP Run durable snapshot is invalid: {error}"))?;
    if (record.dispatch_phase == DurableDispatchPhase::Terminal) != record.state.terminal() {
        return Err("ACP Run durable phase/state terminal truth is inconsistent".to_string());
    }
    Ok(())
}

#[derive(Debug)]
struct ProviderEntry {
    config: AcpAgentConfig,
    instance_id: String,
}

#[derive(Debug)]
struct LiveRunState {
    snapshot: CodingAgentRunSnapshot,
    events: VecDeque<CodingAgentEvent>,
    first_retained_sequence: u64,
    next_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptDispatchGateState {
    PrePrompt,
    PromptDispatchMayHaveOccurred,
}

#[derive(Debug)]
struct RunEntry {
    state: Mutex<LiveRunState>,
    changed: Condvar,
    cancel_requested: AtomicBool,
    prompt_dispatch: Mutex<PromptDispatchGateState>,
    terminal_transition: Mutex<()>,
}

impl RunEntry {
    fn new(snapshot: CodingAgentRunSnapshot) -> Self {
        Self {
            state: Mutex::new(LiveRunState {
                snapshot,
                events: VecDeque::new(),
                first_retained_sequence: 1,
                next_sequence: 1,
            }),
            changed: Condvar::new(),
            cancel_requested: AtomicBool::new(false),
            prompt_dispatch: Mutex::new(PromptDispatchGateState::PrePrompt),
            terminal_transition: Mutex::new(()),
        }
    }

    fn snapshot(&self) -> CodingAgentRunSnapshot {
        self.state.lock().unwrap().snapshot.clone()
    }

    fn begin_terminal_transition(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        let transition = self.terminal_transition.lock().unwrap();
        if self.snapshot().state.terminal() {
            None
        } else {
            Some(transition)
        }
    }

    fn update_snapshot(&self, mut update: impl FnMut(&mut CodingAgentRunSnapshot)) {
        let mut state = self.state.lock().unwrap();
        update(&mut state.snapshot);
        state.snapshot.observation_revision = state.snapshot.observation_revision.saturating_add(1);
        state.snapshot.updated_at = now();
        self.changed.notify_all();
    }

    fn publish_terminal(&self, mut snapshot: CodingAgentRunSnapshot, mut event: CodingAgentEvent) {
        let mut state = self.state.lock().unwrap();
        snapshot.observation_revision = state.snapshot.observation_revision.saturating_add(2);
        state.snapshot = snapshot;
        event.sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.events.push_back(event);
        while state.events.len() > CODING_AGENT_MAX_RETAINED_EVENTS {
            state.events.pop_front();
            state.first_retained_sequence = state.first_retained_sequence.saturating_add(1);
        }
        self.changed.notify_all();
    }

    fn push_event(&self, mut event: CodingAgentEvent) {
        let mut state = self.state.lock().unwrap();
        event.sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.events.push_back(event);
        while state.events.len() > CODING_AGENT_MAX_RETAINED_EVENTS {
            state.events.pop_front();
            state.first_retained_sequence = state.first_retained_sequence.saturating_add(1);
        }
        state.snapshot.observation_revision = state.snapshot.observation_revision.saturating_add(1);
        state.snapshot.updated_at = now();
        self.changed.notify_all();
    }

    fn observe(
        &self,
        after: Option<u64>,
        limit: usize,
        wait_secs: u64,
    ) -> Result<CodingAgentObserveResult, String> {
        let deadline = Instant::now() + Duration::from_secs(wait_secs);
        let mut state = self.state.lock().unwrap();
        let latest_emitted_sequence = state.next_sequence.saturating_sub(1);
        if after.is_some_and(|sequence| sequence > latest_emitted_sequence) {
            return Err(format!(
                "CodingAgentRun observation cursor {sequence} is ahead of latest emitted sequence {latest_emitted_sequence}",
                sequence = after.unwrap_or_default()
            ));
        }
        loop {
            let cursor = after.unwrap_or_else(|| state.first_retained_sequence.saturating_sub(1));
            let changed =
                state.next_sequence > cursor.saturating_add(1) || state.snapshot.state.terminal();
            if changed || wait_secs == 0 {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        let requested = after.unwrap_or_else(|| state.first_retained_sequence.saturating_sub(1));
        let history_lost =
            after.is_some_and(|value| value.saturating_add(1) < state.first_retained_sequence);
        let effective = requested.max(state.first_retained_sequence.saturating_sub(1));
        let mut events = state
            .events
            .iter()
            .filter(|event| event.sequence > effective)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let last = events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(effective);
        let has_more = state.events.iter().any(|event| event.sequence > last);
        if events.len() > CODING_AGENT_MAX_EVENTS_PER_RESPONSE {
            events.truncate(CODING_AGENT_MAX_EVENTS_PER_RESPONSE);
        }
        Ok(CodingAgentObserveResult {
            run: state.snapshot.clone(),
            events,
            first_retained_sequence: state.first_retained_sequence,
            next_sequence: last,
            has_more,
            history_lost,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CodingAgentWorkerDrain {
    pub(crate) resources: usize,
    pub(crate) timed_out: usize,
    pub(crate) panicked: usize,
}

#[derive(Debug)]
pub(crate) struct CodingAgentManager {
    client_id: String,
    providers: BTreeMap<String, Arc<ProviderEntry>>,
    max_concurrent_runs: usize,
    permission_timeout: Duration,
    store: DurableRunStore,
    admission: Mutex<()>,
    runs: Mutex<HashMap<String, Arc<RunEntry>>>,
    accepting: AtomicBool,
    workers: ActivityTracker,
    worker_threads: BackgroundThreads,
    #[cfg(test)]
    admission_test_barrier: Mutex<Option<Arc<std::sync::Barrier>>>,
    #[cfg(test)]
    admission_after_accepting_test_barrier: Mutex<Option<Arc<std::sync::Barrier>>>,
    #[cfg(test)]
    admission_after_accepting_test_reached: AtomicBool,
    #[cfg(test)]
    prompt_dispatch_test_barrier: Mutex<Option<Arc<std::sync::Barrier>>>,
    #[cfg(test)]
    prompt_after_barrier_test_delay: Mutex<Option<Duration>>,
    #[cfg(test)]
    initial_claim_writes: std::sync::atomic::AtomicUsize,
}

impl CodingAgentManager {
    pub(crate) fn new(
        config: &AcpConfig,
        client_id: &str,
        server_url: &str,
    ) -> Result<Arc<Self>, String> {
        let mut providers = BTreeMap::new();
        for provider in &config.agents {
            providers.insert(
                provider.id.clone(),
                Arc::new(ProviderEntry {
                    config: provider.clone(),
                    instance_id: format!("acp_{}", Uuid::new_v4().simple()),
                }),
            );
        }
        let manager = Arc::new(Self {
            client_id: client_id.to_string(),
            providers,
            max_concurrent_runs: config.max_concurrent_runs,
            permission_timeout: Duration::from_secs(config.permission_timeout_secs),
            store: DurableRunStore::new(DurableRunStore::default_root(client_id, server_url)?),
            admission: Mutex::new(()),
            runs: Mutex::new(HashMap::new()),
            accepting: AtomicBool::new(true),
            workers: ActivityTracker::default(),
            worker_threads: BackgroundThreads::default(),
            #[cfg(test)]
            admission_test_barrier: Mutex::new(None),
            #[cfg(test)]
            admission_after_accepting_test_barrier: Mutex::new(None),
            #[cfg(test)]
            admission_after_accepting_test_reached: AtomicBool::new(false),
            #[cfg(test)]
            prompt_dispatch_test_barrier: Mutex::new(None),
            #[cfg(test)]
            prompt_after_barrier_test_delay: Mutex::new(None),
            #[cfg(test)]
            initial_claim_writes: std::sync::atomic::AtomicUsize::new(0),
        });
        manager.recover()?;
        Ok(manager)
    }

    #[cfg(test)]
    fn with_store(config: &AcpConfig, root: PathBuf) -> Result<Arc<Self>, String> {
        let mut providers = BTreeMap::new();
        for provider in &config.agents {
            providers.insert(
                provider.id.clone(),
                Arc::new(ProviderEntry {
                    config: provider.clone(),
                    instance_id: format!("acp_{}", Uuid::new_v4().simple()),
                }),
            );
        }
        let manager = Arc::new(Self {
            client_id: "test".to_string(),
            providers,
            max_concurrent_runs: config.max_concurrent_runs,
            permission_timeout: Duration::from_secs(config.permission_timeout_secs),
            store: DurableRunStore::new(root),
            admission: Mutex::new(()),
            runs: Mutex::new(HashMap::new()),
            accepting: AtomicBool::new(true),
            workers: ActivityTracker::default(),
            worker_threads: BackgroundThreads::default(),
            #[cfg(test)]
            admission_test_barrier: Mutex::new(None),
            #[cfg(test)]
            admission_after_accepting_test_barrier: Mutex::new(None),
            #[cfg(test)]
            admission_after_accepting_test_reached: AtomicBool::new(false),
            #[cfg(test)]
            prompt_dispatch_test_barrier: Mutex::new(None),
            #[cfg(test)]
            prompt_after_barrier_test_delay: Mutex::new(None),
            #[cfg(test)]
            initial_claim_writes: std::sync::atomic::AtomicUsize::new(0),
        });
        manager.recover()?;
        Ok(manager)
    }

    pub(crate) fn providers(&self) -> Vec<CodingAgentProvider> {
        self.providers
            .values()
            .map(|provider| CodingAgentProvider {
                provider_id: provider.config.id.clone(),
                provider_instance_id: provider.instance_id.clone(),
                name: provider.config.name.clone(),
            })
            .collect()
    }

    pub(crate) fn inventory(&self) -> CodingAgentRunInventory {
        self.cleanup_expired();
        let mut runs = self
            .runs
            .lock()
            .unwrap()
            .values()
            .map(|entry| entry.snapshot())
            .collect::<Vec<_>>();
        runs.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        runs.truncate(CODING_AGENT_MAX_INVENTORY_RUNS);
        CodingAgentRunInventory { runs }
    }

    pub(crate) fn stop_accepting(&self) {
        // Publish shutdown intent before taking the admission fence. A Start that
        // already owns admission may finish publishing its authoritative RunEntry,
        // but it cannot cross the prompt gate after this store becomes visible.
        self.accepting.store(false, Ordering::Release);
        let _admission = self.admission.lock().unwrap();
        let entries = self
            .runs
            .lock()
            .unwrap()
            .iter()
            .map(|(run_id, entry)| (run_id.clone(), Arc::clone(entry)))
            .collect::<Vec<_>>();
        for (run_id, entry) in entries {
            if !entry.snapshot().state.terminal() {
                let _ = self.request_cancel(&run_id, &entry);
            }
        }
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.workers.active().max(self.worker_threads.pending())
    }

    pub(crate) fn drain_workers_until(&self, deadline: Instant) -> CodingAgentWorkerDrain {
        let resources = self.worker_count();
        let workers_done = self.workers.wait_until(deadline);
        let joined = self.worker_threads.join_until(deadline);
        CodingAgentWorkerDrain {
            resources,
            timed_out: joined.timed_out.max(usize::from(!workers_done)),
            panicked: joined.panicked,
        }
    }

    pub(crate) fn handle(
        self: &Arc<Self>,
        request: CodingAgentRequest,
        project_registry_dir: &Path,
    ) -> CodingAgentResponse {
        if let Err(error) = validate_request(&request) {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "invalid_coding_agent_request",
                error,
                "invalid_input",
                "fix_input",
            );
        }
        match request {
            CodingAgentRequest::Start(request) => self.start(request, project_registry_dir),
            CodingAgentRequest::Observe(request) => {
                let entry = self.runs.lock().unwrap().get(&request.run_id).cloned();
                match entry {
                    Some(entry) => match entry.observe(
                        request.after_sequence,
                        request.limit,
                        request.wait_secs,
                    ) {
                        Ok(observation) => {
                            CodingAgentResponse::success(CodingAgentResponsePayload::Observe {
                                observation,
                            })
                        }
                        Err(error) => response_error(
                            CodingAgentDispatchState::NotStarted,
                            "invalid_coding_agent_observation_cursor",
                            error,
                            "invalid_input",
                            "fix_input",
                        ),
                    },
                    None => response_error(
                        CodingAgentDispatchState::NotStarted,
                        "unknown_coding_agent_run",
                        "CodingAgentRun is not retained by this Runner",
                        "not_found",
                        "reobserve",
                    ),
                }
            }
            CodingAgentRequest::Cancel(request) => {
                let entry = self.runs.lock().unwrap().get(&request.run_id).cloned();
                match entry {
                    Some(entry) => {
                        CodingAgentResponse::success(CodingAgentResponsePayload::Cancel {
                            run: self.request_cancel(&request.run_id, &entry),
                        })
                    }
                    None => response_error(
                        CodingAgentDispatchState::NotStarted,
                        "unknown_coding_agent_run",
                        "CodingAgentRun is not retained by this Runner",
                        "not_found",
                        "reobserve",
                    ),
                }
            }
        }
    }

    fn request_cancel(&self, run_id: &str, entry: &Arc<RunEntry>) -> CodingAgentRunSnapshot {
        let prompt_gate = entry.prompt_dispatch.lock().unwrap();
        let current = entry.snapshot();
        if current.state.terminal() {
            return current;
        }
        entry.cancel_requested.store(true, Ordering::Release);
        entry.changed.notify_all();
        if *prompt_gate == PromptDispatchGateState::PrePrompt {
            self.finish_pre_prompt_cancelled(run_id, entry);
        }
        entry.snapshot()
    }

    fn finish_pre_prompt_cancelled(&self, run_id: &str, entry: &Arc<RunEntry>) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: None,
            message: Some("ACP prompt was not dispatched; CodingAgentRun was cancelled before prompt dispatch".to_string()),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: None,
            status: Some("cancelled".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Cancelled,
            CodingAgentExecutionState::NotStarted,
            terminal,
            event,
        );
    }

    fn pre_prompt_interrupted(&self, run_id: &str, entry: &Arc<RunEntry>) -> bool {
        if !self.accepting.load(Ordering::Acquire) && !entry.snapshot().state.terminal() {
            let _ = self.request_cancel(run_id, entry);
        }
        entry.snapshot().state.terminal()
    }

    fn setup_timeout(&self, run_id: &str, entry: &Arc<RunEntry>) {
        self.setup_failure(
            run_id,
            entry,
            "coding_agent_setup_timeout",
            "CodingAgentRun total deadline expired before ACP prompt dispatch",
        );
    }

    fn pre_prompt_should_stop(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        run_deadline: Instant,
    ) -> bool {
        if self.pre_prompt_interrupted(run_id, entry) {
            return true;
        }
        if remaining_run_budget(run_deadline).is_none() {
            self.setup_timeout(run_id, entry);
            return true;
        }
        false
    }

    fn write_pre_prompt_frame(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        child: &mut ManagedChild,
        outbound: &mut AcpOutboundWriter,
        frame: std::io::Result<Vec<u8>>,
        run_deadline: Instant,
        failure_code: &str,
        failure_message: &str,
    ) -> bool {
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                self.setup_failure(
                    run_id,
                    entry,
                    failure_code,
                    &format!("{failure_message}: {error}"),
                );
                self.terminate_run_io(child, outbound);
                return false;
            }
        };
        let pending = match outbound.start_frame(frame) {
            Ok(pending) => pending,
            Err(error) => {
                if !self.pre_prompt_should_stop(run_id, entry, run_deadline) {
                    self.setup_failure(
                        run_id,
                        entry,
                        failure_code,
                        &format!("{failure_message}: {error}"),
                    );
                }
                self.terminate_run_io(child, outbound);
                return false;
            }
        };
        match wait_outbound_write(
            pending,
            run_deadline,
            Some(&entry.cancel_requested),
            Some(&self.accepting),
        ) {
            OutboundWriteOutcome::Written => true,
            OutboundWriteOutcome::Failed(error) => {
                if !self.pre_prompt_should_stop(run_id, entry, run_deadline) {
                    self.setup_failure(
                        run_id,
                        entry,
                        failure_code,
                        &format!("{failure_message}: {error}"),
                    );
                }
                self.terminate_run_io(child, outbound);
                false
            }
            OutboundWriteOutcome::Interrupted(OutboundInterruption::Deadline) => {
                self.setup_timeout(run_id, entry);
                self.terminate_run_io(child, outbound);
                false
            }
            OutboundWriteOutcome::Interrupted(
                OutboundInterruption::Cancelled | OutboundInterruption::Shutdown,
            ) => {
                let _ = self.pre_prompt_interrupted(run_id, entry);
                self.terminate_run_io(child, outbound);
                false
            }
        }
    }

    fn terminate_run_io(&self, child: &mut ManagedChild, outbound: &mut AcpOutboundWriter) {
        let _ = child.terminate_tree();
        outbound.close();
        let deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        let _ = outbound.wait_finished_until(deadline);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            let _ = child.wait_tree_exit(remaining);
        }
        let _ = child.try_wait();
        let _ = self.worker_threads.reap_finished();
    }

    fn cleanup_run_io(&self, child: &mut ManagedChild, outbound: &mut AcpOutboundWriter) {
        outbound.close();
        let graceful_deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        loop {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= graceful_deadline {
                let _ = child.terminate_tree();
                break;
            }
            thread::sleep(ACP_POLL);
        }
        let forced_deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        let _ = outbound.wait_finished_until(forced_deadline);
        let remaining = forced_deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            let _ = child.wait_tree_exit(remaining);
        }
        let _ = child.try_wait();
        let _ = self.worker_threads.reap_finished();
    }

    fn write_post_prompt_frame(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        child: &mut ManagedChild,
        outbound: &mut AcpOutboundWriter,
        frame: std::io::Result<Vec<u8>>,
        deadline: Instant,
        observe_cancel: bool,
        uncertainty_code: &str,
    ) -> bool {
        let frame = match frame {
            Ok(frame) => frame,
            Err(_) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                return false;
            }
        };
        let pending = match outbound.start_frame(frame) {
            Ok(pending) => pending,
            Err(_) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                return false;
            }
        };
        let cancelled = observe_cancel.then_some(&entry.cancel_requested);
        match wait_outbound_write(pending, deadline, cancelled, Some(&self.accepting)) {
            OutboundWriteOutcome::Written => true,
            OutboundWriteOutcome::Failed(_)
            | OutboundWriteOutcome::Interrupted(
                OutboundInterruption::Cancelled
                | OutboundInterruption::Shutdown
                | OutboundInterruption::Deadline,
            ) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                false
            }
        }
    }

    fn start(
        self: &Arc<Self>,
        request: webcodex_core::coding_agent::CodingAgentStartRequest,
        project_registry_dir: &Path,
    ) -> CodingAgentResponse {
        #[cfg(test)]
        {
            let barrier = self.admission_test_barrier.lock().unwrap().clone();
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
        // Admission is the authoritative process-local fence for both idempotent
        // run identity and max_concurrent_runs. It intentionally ends once the
        // durable BeforePromptBarrier claim and in-memory RunEntry both exist;
        // provider execution is never serialized by this lock.
        let admission = self.admission.lock().unwrap();
        if !self.accepting.load(Ordering::Acquire) {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "coding_agent_stopping",
                "Runner is stopping",
                "unavailable",
                "wait",
            );
        }
        #[cfg(test)]
        {
            self.admission_after_accepting_test_reached
                .store(true, Ordering::SeqCst);
            let barrier = self
                .admission_after_accepting_test_barrier
                .lock()
                .unwrap()
                .clone();
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
        self.cleanup_expired();
        if let Some(existing) = self.runs.lock().unwrap().get(&request.run_id).cloned() {
            let snapshot = existing.snapshot();
            if snapshot.intent_fingerprint != request.intent_fingerprint {
                return response_error(
                    CodingAgentDispatchState::NotStarted,
                    "idempotency_conflict",
                    "run_id already belongs to a different CodingAgentRun intent",
                    "invalid_input",
                    "fix_input",
                );
            }
            return CodingAgentResponse::success(CodingAgentResponsePayload::Start {
                run: snapshot,
            });
        }
        let Some(provider) = self.providers.get(&request.provider_id).cloned() else {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "coding_agent_provider_unavailable",
                "configured ACP provider is unavailable",
                "unavailable",
                "reobserve",
            );
        };
        if provider.instance_id != request.provider_instance_id {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "stale_coding_agent_provider",
                "ACP provider instance was replaced",
                "stale_state",
                "reobserve",
            );
        }
        if !project_binding_matches(
            project_registry_dir,
            &self.client_id,
            &request.runtime_project_id,
            &request.project_root,
        ) {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "stale_coding_agent_project",
                "registered writable Project binding no longer matches start intent",
                "stale_state",
                "reobserve",
            );
        }
        let active = self
            .runs
            .lock()
            .unwrap()
            .values()
            .filter(|entry| !entry.snapshot().state.terminal())
            .count();
        if active >= self.max_concurrent_runs {
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "coding_agent_capacity_full",
                "Runner ACP concurrency is full",
                "capacity",
                "wait",
            );
        }
        let environment = match resolve_environment(&provider.config) {
            Ok(environment) => environment,
            Err(error) => {
                return response_error(
                    CodingAgentDispatchState::NotStarted,
                    "coding_agent_environment_unavailable",
                    error,
                    "configuration",
                    "retry_same",
                )
            }
        };
        match self.store.read(&request.run_id) {
            Ok(Some(record)) => {
                let snapshot = record.snapshot(0);
                if snapshot.intent_fingerprint != request.intent_fingerprint {
                    return response_error(
                        CodingAgentDispatchState::NotStarted,
                        "idempotency_conflict",
                        "durable run_id belongs to a different CodingAgentRun intent",
                        "invalid_input",
                        "fix_input",
                    );
                }
                let entry = Arc::new(RunEntry::new(snapshot.clone()));
                self.runs
                    .lock()
                    .unwrap()
                    .insert(request.run_id.clone(), entry);
                return CodingAgentResponse::success(CodingAgentResponsePayload::Start {
                    run: snapshot,
                });
            }
            Ok(None) => {}
            Err(error) => {
                return response_error(
                    CodingAgentDispatchState::OutcomeUnknown,
                    "coding_agent_durable_state_unavailable",
                    error,
                    "durable_state_unavailable",
                    "reconcile",
                );
            }
        }

        let timestamp = now();
        let record = DurableRunRecord {
            schema_version: STORE_SCHEMA_VERSION,
            run_id: request.run_id.clone(),
            intent_fingerprint: request.intent_fingerprint.clone(),
            authority_fingerprint: request.authority_fingerprint.clone(),
            runtime_project_id: request.runtime_project_id.clone(),
            provider_id: request.provider_id.clone(),
            provider_instance_id: request.provider_instance_id.clone(),
            state: CodingAgentRunState::Starting,
            execution_state: CodingAgentExecutionState::NotStarted,
            dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
            created_at: timestamp,
            updated_at: timestamp,
            terminal: None,
        };
        #[cfg(test)]
        self.initial_claim_writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Err(error) = self.store.write(&record) {
            // Admission has not launched the ACP turn yet. Remove any partial
            // directory/state residue so the documented retry_same response
            // cannot later be mistaken for a previously dispatched Run.
            self.store.remove(&request.run_id);
            return response_error(
                CodingAgentDispatchState::NotStarted,
                "coding_agent_admission_persist_failed",
                error,
                "io",
                "retry_same",
            );
        }
        let entry = Arc::new(RunEntry::new(record.snapshot(0)));
        self.runs
            .lock()
            .unwrap()
            .insert(request.run_id.clone(), Arc::clone(&entry));
        let worker_guard = self.workers.enter();
        drop(admission);
        let _ = self.worker_threads.reap_finished();
        let run_id = request.run_id.clone();
        let thread_entry = Arc::clone(&entry);
        let thread_manager = Arc::clone(self);
        let (start_tx, start_rx) = mpsc::sync_channel(0);
        let spawn_result = thread::Builder::new()
            .name(format!(
                "wc-acp-{}",
                run_id.chars().take(24).collect::<String>()
            ))
            .spawn(move || {
                let _worker_guard = worker_guard;
                if start_rx.recv().is_ok() {
                    thread_manager.run_turn(request, provider, environment, thread_entry);
                }
            });
        match spawn_result {
            Ok(handle) => {
                self.worker_threads.register(handle);
                let _ = start_tx.send(());
            }
            Err(error) => {
                manager_finish_setup_failure(
                    self,
                    &run_id,
                    &entry,
                    "coding_agent_thread_spawn_failed",
                    &error.to_string(),
                );
            }
        }
        CodingAgentResponse::success(CodingAgentResponsePayload::Start {
            run: entry.snapshot(),
        })
    }

    fn run_turn(
        self: Arc<Self>,
        request: webcodex_core::coding_agent::CodingAgentStartRequest,
        provider: Arc<ProviderEntry>,
        environment: Vec<(String, std::ffi::OsString)>,
        entry: Arc<RunEntry>,
    ) {
        let run_deadline = Instant::now() + Duration::from_secs(request.timeout_secs);
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            return;
        }
        let mut command = Command::new(&provider.config.executable);
        command.args(&provider.config.args).env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
        command.current_dir(&request.project_root);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match ManagedChild::spawn(&mut command) {
            Ok(child) => child,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_spawn_failed",
                    &error.to_string(),
                );
                return;
            }
        };
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            let _ = child.terminate_tree();
            let _ = child.wait();
            return;
        }
        let stdin = match child.child_mut().stdin.take() {
            Some(stdin) => stdin,
            None => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_stdio_unavailable",
                    "ACP stdin unavailable",
                );
                let _ = child.terminate_tree();
                return;
            }
        };
        let stdout = match child.child_mut().stdout.take() {
            Some(stdout) => stdout,
            None => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_stdio_unavailable",
                    "ACP stdout unavailable",
                );
                let _ = child.terminate_tree();
                return;
            }
        };
        let stderr = child.child_mut().stderr.take();
        if let Some(stderr) = stderr {
            let _ = thread::Builder::new()
                .name("wc-acp-stderr".to_string())
                .spawn(move || {
                    let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
                });
        }
        let mut outbound = match AcpOutboundWriter::spawn(stdin, &self.worker_threads) {
            Ok(outbound) => outbound,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_writer_unavailable",
                    &error.to_string(),
                );
                let _ = child.terminate_tree();
                let _ = child.wait_tree_exit(ACP_IO_CLEANUP_TIMEOUT);
                let _ = child.try_wait();
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(32);
        let _reader = match thread::Builder::new()
            .name("wc-acp-stdout".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(ReaderEvent::Eof);
                            break;
                        }
                        Ok(_) if line.len() <= ACP_MESSAGE_MAX_BYTES => {
                            let value = serde_json::from_str::<Value>(&line)
                                .map(ReaderEvent::Message)
                                .unwrap_or(ReaderEvent::Malformed);
                            if tx.send(value).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            let _ = tx.send(ReaderEvent::TooLarge);
                            break;
                        }
                        Err(_) => {
                            let _ = tx.send(ReaderEvent::Io);
                            break;
                        }
                    }
                }
            }) {
            Ok(handle) => handle,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_reader_unavailable",
                    &error.to_string(),
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };

        let mut next_id = 1u64;
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        let initialize_id = next_id;
        next_id += 1;
        if !self.write_pre_prompt_frame(
            &request.run_id,
            &entry,
            &mut child,
            &mut outbound,
            request_frame(
                initialize_id,
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {},
                    "clientInfo": {"name":"webcodex-runner","version":env!("CARGO_PKG_VERSION")}
                }),
            ),
            run_deadline,
            "coding_agent_initialize_write_failed",
            "failed to write initialize",
        ) {
            return;
        }
        let Some(initialize_wait) = bounded_setup_wait(run_deadline) else {
            self.setup_timeout(&request.run_id, &entry);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        };
        let initialize = match wait_response(
            &rx,
            initialize_id,
            initialize_wait,
            Some(&entry.cancel_requested),
        ) {
            Ok(value) => value,
            Err(error) => {
                if !self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
                    self.setup_failure(
                        &request.run_id,
                        &entry,
                        "coding_agent_initialize_failed",
                        &error,
                    );
                }
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        if initialize.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
            self.setup_failure(
                &request.run_id,
                &entry,
                "coding_agent_protocol_version_unsupported",
                "ACP v1 was not negotiated",
            );
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        let new_id = next_id;
        next_id += 1;
        if !self.write_pre_prompt_frame(
            &request.run_id,
            &entry,
            &mut child,
            &mut outbound,
            request_frame(
                new_id,
                "session/new",
                json!({
                    "cwd": request.project_root,
                    "mcpServers": []
                }),
            ),
            run_deadline,
            "coding_agent_session_new_write_failed",
            "failed to write session/new",
        ) {
            return;
        }
        let Some(session_new_wait) = bounded_setup_wait(run_deadline) else {
            self.setup_timeout(&request.run_id, &entry);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        };
        let new_value =
            match wait_response(&rx, new_id, session_new_wait, Some(&entry.cancel_requested)) {
                Ok(value) => value,
                Err(error) => {
                    if !self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
                        self.setup_failure(
                            &request.run_id,
                            &entry,
                            "coding_agent_session_new_failed",
                            &error,
                        );
                    }
                    self.terminate_run_io(&mut child, &mut outbound);
                    return;
                }
            };
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        let new_session: NewSessionResponse = match serde_json::from_value(new_value) {
            Ok(response) => response,
            Err(_) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_session_new_invalid",
                    "invalid ACP session/new result",
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        let session_id = new_session.session_id.to_string();
        let mut advertised = new_session.config_options.unwrap_or_default();

        for (key, value) in &request.config {
            if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
            if !provider
                .config
                .allowed_config_options
                .iter()
                .any(|allowed| allowed == key)
            {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_config_not_allowed",
                    "ACP config override is not operator-allowed",
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
            if !config_override_is_valid(&advertised, key, value) {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_config_invalid",
                    "ACP config override is not currently advertised/legal",
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
            let config_id = next_id;
            next_id += 1;
            let params = match config_params(&session_id, key, value) {
                Some(params) => params,
                None => {
                    self.setup_failure(
                        &request.run_id,
                        &entry,
                        "coding_agent_config_invalid",
                        "ACP config value type is unsupported by stable v1",
                    );
                    self.terminate_run_io(&mut child, &mut outbound);
                    return;
                }
            };
            if !self.write_pre_prompt_frame(
                &request.run_id,
                &entry,
                &mut child,
                &mut outbound,
                request_frame(config_id, "session/set_config_option", params),
                run_deadline,
                "coding_agent_config_write_failed",
                "failed to write session/set_config_option",
            ) {
                return;
            }
            let Some(config_wait) = bounded_setup_wait(run_deadline) else {
                self.setup_timeout(&request.run_id, &entry);
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            };
            let result =
                match wait_response(&rx, config_id, config_wait, Some(&entry.cancel_requested)) {
                    Ok(result) => result,
                    Err(error) => {
                        if !self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
                            self.setup_failure(
                                &request.run_id,
                                &entry,
                                "coding_agent_config_failed",
                                &error,
                            );
                        }
                        self.terminate_run_io(&mut child, &mut outbound);
                        return;
                    }
                };
            if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
            let refreshed: SetSessionConfigOptionResponse = match serde_json::from_value(result) {
                Ok(result) => result,
                Err(_) => {
                    self.setup_failure(
                        &request.run_id,
                        &entry,
                        "coding_agent_config_invalid_response",
                        "invalid refreshed ACP config options",
                    );
                    self.terminate_run_io(&mut child, &mut outbound);
                    return;
                }
            };
            advertised = refreshed.config_options;
            if !config_override_is_current(&advertised, key, value) {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_config_not_applied",
                    "ACP config override was not reflected by provider",
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        }

        #[cfg(test)]
        {
            let barrier = self.prompt_dispatch_test_barrier.lock().unwrap().clone();
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
        let prompt_id = next_id;
        let prompt_frame = match request_frame(
            prompt_id,
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{"type":"text","text":request.instruction}]
            }),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_failed",
                    &error.to_string(),
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        let mut prompt_gate = entry.prompt_dispatch.lock().unwrap();
        if entry.snapshot().state.terminal()
            || entry.cancel_requested.load(Ordering::Acquire)
            || !self.accepting.load(Ordering::Acquire)
        {
            if !entry.snapshot().state.terminal() {
                entry.cancel_requested.store(true, Ordering::Release);
                self.finish_pre_prompt_cancelled(&request.run_id, &entry);
            }
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        if remaining_run_budget(run_deadline).is_none() {
            self.setup_timeout(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        // Irreversible uncertainty barrier: durable state is committed before the
        // first byte of session/prompt can be handed to the sole stdin writer.
        if let Err(error) = self.persist_phase(
            &request.run_id,
            &entry,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred,
            CodingAgentRunState::Running,
            CodingAgentExecutionState::OutcomeUnknown,
            None,
        ) {
            self.setup_failure(
                &request.run_id,
                &entry,
                "coding_agent_dispatch_barrier_failed",
                &error,
            );
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        #[cfg(test)]
        {
            let delay = *self.prompt_after_barrier_test_delay.lock().unwrap();
            if let Some(delay) = delay {
                thread::sleep(delay);
            }
        }
        if remaining_run_budget(run_deadline).is_none() {
            self.setup_timeout(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        // Shutdown intent may become visible while the durable uncertainty barrier
        // is being written. The gate is still pre-prompt, so overwrite the durable
        // barrier with truthful cancelled/not_started state before writer handoff.
        if !self.accepting.load(Ordering::Acquire) {
            entry.cancel_requested.store(true, Ordering::Release);
            self.finish_pre_prompt_cancelled(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        // Mark possible dispatch before the writer can consume any prompt byte.
        // If the bounded queue handoff itself fails, no prompt byte was writable,
        // so restore the in-memory gate while it is still exclusively held.
        *prompt_gate = PromptDispatchGateState::PromptDispatchMayHaveOccurred;
        let prompt_pending = match outbound.start_frame(prompt_frame) {
            Ok(pending) => pending,
            Err(error) => {
                *prompt_gate = PromptDispatchGateState::PrePrompt;
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_failed",
                    &error,
                );
                drop(prompt_gate);
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        // The authoritative possible-dispatch boundary is the successful writer
        // handoff. Never retain this gate while waiting on ChildStdin backpressure.
        drop(prompt_gate);
        match wait_outbound_write(
            prompt_pending,
            run_deadline,
            Some(&entry.cancel_requested),
            Some(&self.accepting),
        ) {
            OutboundWriteOutcome::Written => {
                entry.update_snapshot(|snapshot| {
                    snapshot.execution_state = CodingAgentExecutionState::Started
                });
                let _ = self.persist_from_entry(
                    &request.run_id,
                    &entry,
                    DurableDispatchPhase::PromptDispatchMayHaveOccurred,
                );
            }
            OutboundWriteOutcome::Failed(_)
            | OutboundWriteOutcome::Interrupted(
                OutboundInterruption::Cancelled
                | OutboundInterruption::Shutdown
                | OutboundInterruption::Deadline,
            ) => {
                self.terminate_run_io(&mut child, &mut outbound);
                self.mark_lost(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_uncertain",
                );
                return;
            }
        }

        let mut cancel_sent = false;
        let mut cancel_deadline = None;
        loop {
            if entry.cancel_requested.load(Ordering::Acquire)
                || Instant::now() >= run_deadline
                || !self.accepting.load(Ordering::Acquire)
            {
                if !cancel_sent {
                    let deadline =
                        *cancel_deadline.get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                    if !self.write_post_prompt_frame(
                        &request.run_id,
                        &entry,
                        &mut child,
                        &mut outbound,
                        notification_frame("session/cancel", json!({"sessionId":session_id})),
                        deadline,
                        false,
                        "coding_agent_cancel_write_uncertain",
                    ) {
                        return;
                    }
                    cancel_sent = true;
                }
            }
            if cancel_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                self.terminate_run_io(&mut child, &mut outbound);
                self.mark_lost(
                    &request.run_id,
                    &entry,
                    "coding_agent_cancel_terminal_missing",
                );
                return;
            }
            match rx.recv_timeout(ACP_POLL) {
                Ok(ReaderEvent::Message(message)) => {
                    if message.get("method").and_then(Value::as_str) == Some("session/update") {
                        if let Some(event) = normalize_update(&message) {
                            entry.push_event(event);
                        }
                        continue;
                    }
                    if message.get("method").and_then(Value::as_str)
                        == Some("session/request_permission")
                    {
                        let Some(id) = message.get("id").and_then(Value::as_u64) else {
                            self.terminate_run_io(&mut child, &mut outbound);
                            self.mark_lost(
                                &request.run_id,
                                &entry,
                                "coding_agent_permission_id_invalid",
                            );
                            return;
                        };
                        let params = message.get("params").cloned().unwrap_or(Value::Null);
                        if serde_json::from_value::<RequestPermissionRequest>(params.clone())
                            .is_err()
                        {
                            self.terminate_run_io(&mut child, &mut outbound);
                            self.mark_lost(
                                &request.run_id,
                                &entry,
                                "coding_agent_permission_invalid",
                            );
                            return;
                        }
                        entry.push_event(permission_event(&params));
                        entry.update_snapshot(|snapshot| {
                            snapshot.state = CodingAgentRunState::WaitingPermission
                        });
                        let permission_deadline =
                            (Instant::now() + self.permission_timeout).min(run_deadline);
                        while Instant::now() < permission_deadline
                            && !entry.cancel_requested.load(Ordering::Acquire)
                            && self.accepting.load(Ordering::Acquire)
                        {
                            thread::sleep(ACP_POLL);
                        }
                        let lifecycle_interrupted = entry.cancel_requested.load(Ordering::Acquire)
                            || Instant::now() >= run_deadline
                            || !self.accepting.load(Ordering::Acquire);
                        let response_deadline = if lifecycle_interrupted {
                            *cancel_deadline
                                .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE)
                        } else {
                            (Instant::now() + ACP_CANCEL_GRACE).min(run_deadline)
                        };
                        // P1 never selects an allow/reject option. Cancelled is
                        // the only fail-closed ACP outcome emitted by WebCodex.
                        if !self.write_post_prompt_frame(
                            &request.run_id,
                            &entry,
                            &mut child,
                            &mut outbound,
                            result_frame(id, json!({"outcome":{"outcome":"cancelled"}})),
                            response_deadline,
                            !lifecycle_interrupted,
                            "coding_agent_permission_response_uncertain",
                        ) {
                            return;
                        }
                        entry.update_snapshot(|snapshot| {
                            snapshot.state = CodingAgentRunState::Running
                        });
                        if (entry.cancel_requested.load(Ordering::Acquire)
                            || Instant::now() >= run_deadline
                            || !self.accepting.load(Ordering::Acquire))
                            && !cancel_sent
                        {
                            let deadline = *cancel_deadline
                                .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                notification_frame(
                                    "session/cancel",
                                    json!({"sessionId":session_id}),
                                ),
                                deadline,
                                false,
                                "coding_agent_cancel_write_uncertain",
                            ) {
                                return;
                            }
                            cancel_sent = true;
                        }
                        continue;
                    }
                    if message.get("method").is_some() {
                        let deadline = *cancel_deadline
                            .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                        if let Some(id) = message.get("id").and_then(Value::as_u64) {
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                error_frame(id, -32601, "unsupported ACP client request"),
                                deadline,
                                true,
                                "coding_agent_transport_lost",
                            ) {
                                return;
                            }
                        }
                        if !cancel_sent {
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                notification_frame(
                                    "session/cancel",
                                    json!({"sessionId":session_id}),
                                ),
                                deadline,
                                false,
                                "coding_agent_cancel_write_uncertain",
                            ) {
                                return;
                            }
                            cancel_sent = true;
                        }
                        continue;
                    }
                    if message.get("id").and_then(Value::as_u64) == Some(prompt_id) {
                        if let Some(error) = message.get("error") {
                            self.finish_failed(
                                &request.run_id,
                                &entry,
                                "prompt_error",
                                bounded_json_summary(error),
                            );
                            self.cleanup_run_io(&mut child, &mut outbound);
                            return;
                        }
                        let Some(result) = message.get("result").cloned() else {
                            self.finish_failed(
                                &request.run_id,
                                &entry,
                                "invalid_prompt_response",
                                "missing prompt result".to_string(),
                            );
                            self.cleanup_run_io(&mut child, &mut outbound);
                            return;
                        };
                        let response: PromptResponse = match serde_json::from_value(result) {
                            Ok(response) => response,
                            Err(_) => {
                                self.finish_failed(
                                    &request.run_id,
                                    &entry,
                                    "unknown_stop_reason",
                                    "invalid or unknown ACP stopReason".to_string(),
                                );
                                self.cleanup_run_io(&mut child, &mut outbound);
                                return;
                            }
                        };
                        match response.stop_reason {
                            StopReason::EndTurn => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Completed,
                                CODING_AGENT_STOP_REASON_END_TURN,
                                None,
                            ),
                            StopReason::Cancelled => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Cancelled,
                                CODING_AGENT_STOP_REASON_CANCELLED,
                                None,
                            ),
                            StopReason::MaxTokens => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_MAX_TOKENS,
                                Some("ACP turn reached max tokens"),
                            ),
                            StopReason::MaxTurnRequests => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_MAX_TURN_REQUESTS,
                                Some("ACP turn reached max requests"),
                            ),
                            StopReason::Refusal => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_REFUSAL,
                                Some("ACP agent refused the turn"),
                            ),
                            _ => self.finish_failed(
                                &request.run_id,
                                &entry,
                                "unknown_stop_reason",
                                "unknown ACP stop reason".to_string(),
                            ),
                        }
                        self.cleanup_run_io(&mut child, &mut outbound);
                        return;
                    }
                }
                Ok(
                    ReaderEvent::Eof
                    | ReaderEvent::Malformed
                    | ReaderEvent::TooLarge
                    | ReaderEvent::Io,
                ) => {
                    self.terminate_run_io(&mut child, &mut outbound);
                    self.mark_lost(&request.run_id, &entry, "coding_agent_transport_lost");
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if child.try_wait().ok().flatten().is_some() {
                        outbound.close();
                        let _ =
                            outbound.wait_finished_until(Instant::now() + ACP_IO_CLEANUP_TIMEOUT);
                        let _ = self.worker_threads.reap_finished();
                        self.mark_lost(&request.run_id, &entry, "coding_agent_process_exited");
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate_run_io(&mut child, &mut outbound);
                    self.mark_lost(&request.run_id, &entry, "coding_agent_transport_lost");
                    return;
                }
            }
        }
    }

    fn recover(&self) -> Result<(), String> {
        let mut map = self.runs.lock().unwrap();
        for mut record in self.store.scan()? {
            if record.dispatch_phase != DurableDispatchPhase::Terminal {
                let message = if record.dispatch_phase
                    == DurableDispatchPhase::PromptDispatchMayHaveOccurred
                {
                    "Runner restarted after prompt dispatch uncertainty barrier"
                } else {
                    "Runner restarted before prompt dispatch barrier"
                };
                let state = if record.dispatch_phase
                    == DurableDispatchPhase::PromptDispatchMayHaveOccurred
                {
                    CodingAgentRunState::Lost
                } else {
                    CodingAgentRunState::Failed
                };
                let execution = if state == CodingAgentRunState::Lost {
                    CodingAgentExecutionState::OutcomeUnknown
                } else {
                    CodingAgentExecutionState::NotStarted
                };
                record.state = state.clone();
                record.execution_state = execution;
                record.dispatch_phase = DurableDispatchPhase::Terminal;
                record.updated_at = now();
                record.terminal = Some(CodingAgentTerminal {
                    stop_reason: None,
                    error_code: Some(
                        if state == CodingAgentRunState::Lost {
                            "runner_restart_uncertain"
                        } else {
                            "runner_restart_not_started"
                        }
                        .to_string(),
                    ),
                    message: Some(message.to_string()),
                    completed_at: now(),
                });
                self.store.write(&record)?;
            }
            map.insert(
                record.run_id.clone(),
                Arc::new(RunEntry::new(record.snapshot(0))),
            );
        }
        Ok(())
    }

    fn cleanup_expired(&self) {
        let cutoff = now().saturating_sub(STORE_RETENTION_SECS);
        let expired = {
            let map = self.runs.lock().unwrap();
            map.iter()
                .filter(|(_, entry)| {
                    let snapshot = entry.snapshot();
                    snapshot.state.terminal() && snapshot.updated_at < cutoff
                })
                .map(|(run_id, _)| run_id.clone())
                .collect::<Vec<_>>()
        };
        if expired.is_empty() {
            return;
        }
        let mut map = self.runs.lock().unwrap();
        for run_id in expired {
            map.remove(&run_id);
            self.store.remove(&run_id);
        }
    }

    fn setup_failure(&self, run_id: &str, entry: &Arc<RunEntry>, code: &str, message: &str) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(bounded_text(message)),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("failed".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::NotStarted,
            terminal,
            event,
        );
    }

    fn finish_failed(&self, run_id: &str, entry: &Arc<RunEntry>, code: &str, message: String) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(bounded_text(&message)),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("failed".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::Completed,
            terminal,
            event,
        );
    }

    fn finish_terminal(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        state: CodingAgentRunState,
        stop_reason: &str,
        message: Option<&str>,
    ) {
        let terminal = CodingAgentTerminal {
            stop_reason: Some(stop_reason.to_string()),
            error_code: if state == CodingAgentRunState::Failed {
                Some(stop_reason.to_string())
            } else {
                None
            },
            message: message.map(bounded_text),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: message.map(bounded_text),
            label: Some(stop_reason.to_string()),
            status: Some(format!("{state:?}").to_ascii_lowercase()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            state,
            CodingAgentExecutionState::Completed,
            terminal,
            event,
        );
    }

    fn mark_lost(&self, run_id: &str, entry: &Arc<RunEntry>, code: &str) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(
                "ACP prompt outcome is unknown; prompt must not be redispatched".to_string(),
            ),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("lost".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Lost,
            CodingAgentExecutionState::OutcomeUnknown,
            terminal,
            event,
        );
    }

    fn commit_terminal_transition(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        desired_state: CodingAgentRunState,
        desired_execution_state: CodingAgentExecutionState,
        desired_terminal: CodingAgentTerminal,
        desired_event: CodingAgentEvent,
    ) {
        let Some(_terminal_transition) = entry.begin_terminal_transition() else {
            return;
        };
        let current = entry.snapshot();
        let mut candidate = current.clone();
        candidate.state = desired_state.clone();
        candidate.execution_state = desired_execution_state;
        candidate.updated_at = desired_terminal.completed_at;
        candidate.terminal = Some(desired_terminal);
        let durable =
            durable_record_from_snapshot(run_id, &candidate, DurableDispatchPhase::Terminal)
                .and_then(|record| self.store.write(&record));
        match durable {
            Ok(()) => entry.publish_terminal(candidate, desired_event),
            Err(error) => {
                let pre_prompt = desired_execution_state == CodingAgentExecutionState::NotStarted;
                let completed_at = now();
                let terminal = CodingAgentTerminal {
                    stop_reason: None,
                    error_code: Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN.to_string()),
                    message: Some(TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE.to_string()),
                    completed_at,
                };
                let mut uncertain = current;
                uncertain.state = if pre_prompt {
                    CodingAgentRunState::Failed
                } else {
                    CodingAgentRunState::Lost
                };
                uncertain.execution_state = if pre_prompt {
                    CodingAgentExecutionState::NotStarted
                } else {
                    CodingAgentExecutionState::OutcomeUnknown
                };
                uncertain.updated_at = completed_at;
                uncertain.terminal = Some(terminal.clone());
                let status = if pre_prompt { "failed" } else { "lost" };
                tracing::error!(
                    run_id = %run_id,
                    desired_state = ?desired_state,
                    desired_execution_state = ?desired_execution_state,
                    error = %error,
                    "ACP terminal durable commit failed; publishing conservative persistence uncertainty"
                );
                entry.publish_terminal(
                    uncertain,
                    CodingAgentEvent {
                        sequence: 0,
                        kind: CodingAgentEventKind::Terminal,
                        text: terminal.message.clone(),
                        label: terminal.error_code.clone(),
                        status: Some(status.to_string()),
                        usage: None,
                    },
                );
            }
        }
    }

    fn persist_phase(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        phase: DurableDispatchPhase,
        state: CodingAgentRunState,
        execution_state: CodingAgentExecutionState,
        terminal: Option<CodingAgentTerminal>,
    ) -> Result<(), String> {
        entry.update_snapshot(|snapshot| {
            snapshot.state = state.clone();
            snapshot.execution_state = execution_state;
            snapshot.terminal = terminal.clone();
        });
        self.persist_from_entry(run_id, entry, phase)
    }

    fn persist_from_entry(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        phase: DurableDispatchPhase,
    ) -> Result<(), String> {
        let snapshot = entry.snapshot();
        let record = durable_record_from_snapshot(run_id, &snapshot, phase)?;
        self.store.write(&record)
    }
}

fn durable_record_from_snapshot(
    run_id: &str,
    snapshot: &CodingAgentRunSnapshot,
    phase: DurableDispatchPhase,
) -> Result<DurableRunRecord, String> {
    if snapshot.run_id != run_id {
        return Err("ACP Run identity changed unexpectedly".to_string());
    }
    Ok(DurableRunRecord {
        schema_version: STORE_SCHEMA_VERSION,
        run_id: snapshot.run_id.clone(),
        intent_fingerprint: snapshot.intent_fingerprint.clone(),
        authority_fingerprint: snapshot.authority_fingerprint.clone(),
        runtime_project_id: snapshot.runtime_project_id.clone(),
        provider_id: snapshot.provider_id.clone(),
        provider_instance_id: snapshot.provider_instance_id.clone(),
        state: snapshot.state.clone(),
        execution_state: snapshot.execution_state,
        dispatch_phase: phase,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        terminal: snapshot.terminal.clone(),
    })
}

fn manager_finish_setup_failure(
    manager: &Arc<CodingAgentManager>,
    run_id: &str,
    entry: &Arc<RunEntry>,
    code: &str,
    message: &str,
) {
    manager.setup_failure(run_id, entry, code, message);
}

struct OutboundWriteRequest {
    frame: Vec<u8>,
    completion: mpsc::SyncSender<Result<(), String>>,
}

struct PendingOutboundWrite {
    completion: Receiver<Result<(), String>>,
}

struct AcpOutboundWriter {
    requests: Option<mpsc::SyncSender<OutboundWriteRequest>>,
    finished: Arc<(Mutex<bool>, Condvar)>,
}

struct OutboundWriterFinished(Arc<(Mutex<bool>, Condvar)>);

impl Drop for OutboundWriterFinished {
    fn drop(&mut self) {
        let (finished, changed) = &*self.0;
        let mut finished = finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *finished = true;
        changed.notify_all();
    }
}

impl AcpOutboundWriter {
    fn spawn<W: Write + Send + 'static>(
        mut sink: W,
        threads: &BackgroundThreads,
    ) -> std::io::Result<Self> {
        let (requests, receiver) = mpsc::sync_channel::<OutboundWriteRequest>(1);
        let finished = Arc::new((Mutex::new(false), Condvar::new()));
        let thread_finished = Arc::clone(&finished);
        let handle = thread::Builder::new()
            .name("wc-acp-stdin".to_string())
            .spawn(move || {
                let _finished = OutboundWriterFinished(thread_finished);
                while let Ok(request) = receiver.recv() {
                    let result = sink
                        .write_all(&request.frame)
                        .and_then(|_| sink.flush())
                        .map_err(|error| error.to_string());
                    let failed = result.is_err();
                    let _ = request.completion.send(result);
                    if failed {
                        break;
                    }
                }
            })?;
        threads.register(handle);
        Ok(Self {
            requests: Some(requests),
            finished,
        })
    }

    fn start_frame(&self, frame: Vec<u8>) -> Result<PendingOutboundWrite, String> {
        let requests = self
            .requests
            .as_ref()
            .ok_or_else(|| "ACP outbound writer is closed".to_string())?;
        let (completion, receiver) = mpsc::sync_channel(1);
        requests
            .try_send(OutboundWriteRequest { frame, completion })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "ACP outbound writer is busy".to_string(),
                mpsc::TrySendError::Disconnected(_) => {
                    "ACP outbound writer is unavailable".to_string()
                }
            })?;
        Ok(PendingOutboundWrite {
            completion: receiver,
        })
    }

    fn close(&mut self) {
        self.requests.take();
    }

    fn wait_finished_until(&self, deadline: Instant) -> bool {
        let (finished, changed) = &*self.finished;
        let mut finished = finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*finished {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timed_out) = changed
                .wait_timeout(finished, remaining.min(ACP_POLL))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            finished = next;
            if timed_out.timed_out() && Instant::now() >= deadline {
                return false;
            }
        }
        true
    }
}

impl Drop for AcpOutboundWriter {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutboundInterruption {
    Cancelled,
    Shutdown,
    Deadline,
}

enum OutboundWriteOutcome {
    Written,
    Failed(String),
    Interrupted(OutboundInterruption),
}

#[derive(Debug)]
enum ReaderEvent {
    Message(Value),
    Eof,
    Malformed,
    TooLarge,
    Io,
}

fn remaining_run_budget(run_deadline: Instant) -> Option<Duration> {
    let remaining = run_deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then_some(remaining)
}

fn bounded_setup_wait(run_deadline: Instant) -> Option<Duration> {
    remaining_run_budget(run_deadline).map(|remaining| remaining.min(ACP_SETUP_TIMEOUT))
}

fn wait_outbound_write(
    pending: PendingOutboundWrite,
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
    accepting: Option<&AtomicBool>,
) -> OutboundWriteOutcome {
    loop {
        match pending.completion.try_recv() {
            Ok(Ok(())) => return OutboundWriteOutcome::Written,
            Ok(Err(error)) => return OutboundWriteOutcome::Failed(error),
            Err(mpsc::TryRecvError::Disconnected) => {
                return OutboundWriteOutcome::Failed(
                    "ACP outbound writer disconnected before acknowledgement".to_string(),
                );
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Cancelled);
        }
        if accepting.is_some_and(|flag| !flag.load(Ordering::Acquire)) {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Shutdown);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Deadline);
        }
        match pending.completion.recv_timeout(remaining.min(ACP_POLL)) {
            Ok(Ok(())) => return OutboundWriteOutcome::Written,
            Ok(Err(error)) => return OutboundWriteOutcome::Failed(error),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return OutboundWriteOutcome::Failed(
                    "ACP outbound writer disconnected before acknowledgement".to_string(),
                );
            }
        }
    }
}

fn wait_response(
    rx: &Receiver<ReaderEvent>,
    id: u64,
    timeout: Duration,
    interrupted: Option<&AtomicBool>,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if interrupted.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err("ACP setup interrupted".to_string());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("ACP request timed out".to_string());
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(ReaderEvent::Message(message)) => {
                if message.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(error) = message.get("error") {
                    return Err(format!(
                        "ACP request failed: {}",
                        bounded_json_summary(error)
                    ));
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "ACP response missing result".to_string());
            }
            Ok(
                ReaderEvent::Eof | ReaderEvent::Malformed | ReaderEvent::TooLarge | ReaderEvent::Io,
            ) => return Err("ACP transport failed".to_string()),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("ACP transport disconnected".to_string())
            }
        }
    }
}

fn request_frame(id: u64, method: &str, params: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
}

fn notification_frame(method: &str, params: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","method":method,"params":params}))
}

fn result_frame(id: u64, result: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn error_frame(id: u64, code: i64, message: &str) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
}

fn frame_message(value: &Value) -> std::io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    if bytes.len() > ACP_MESSAGE_MAX_BYTES {
        return Err(std::io::Error::other("ACP message too large"));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

fn project_binding_matches(
    project_registry_dir: &Path,
    client_id: &str,
    runtime_project_id: &str,
    root: &str,
) -> bool {
    let Ok(requested_root) = canonicalize_existing(Path::new(root)) else {
        return false;
    };
    if !requested_root.is_dir() {
        return false;
    }
    load_runner_project_summaries_from_dir(project_registry_dir)
        .into_iter()
        .any(|project| {
            if format!("agent:{client_id}:{}", project.id) != runtime_project_id
                || !project.allow_patch
                || project.disabled
            {
                return false;
            }
            canonicalize_existing(Path::new(&project.path))
                .ok()
                .filter(|registered_root| registered_root.is_dir())
                .is_some_and(|registered_root| paths_equal(&registered_root, &requested_root))
        })
}

fn resolve_environment(
    provider: &AcpAgentConfig,
) -> Result<Vec<(String, std::ffi::OsString)>, String> {
    let mut result = Vec::with_capacity(provider.env_from_env.len());
    for (destination, source) in &provider.env_from_env {
        let Some(value) = std::env::var_os(source) else {
            return Err(format!(
                "required ACP environment source '{source}' is missing"
            ));
        };
        result.push((destination.clone(), value));
    }
    Ok(result)
}

fn config_override_is_valid(
    options: &[SessionConfigOption],
    key: &str,
    value: &CodingAgentConfigValue,
) -> bool {
    let Some(option) = options.iter().find(|option| option.id.to_string() == key) else {
        return false;
    };
    match (&option.kind, value) {
        (SessionConfigKind::Boolean(_), CodingAgentConfigValue::Bool(_)) => true,
        (SessionConfigKind::Select(select), CodingAgentConfigValue::String(requested)) => {
            match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => options
                    .iter()
                    .any(|option| option.value.to_string() == *requested),
                SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .flat_map(|group| group.options.iter())
                    .any(|option| option.value.to_string() == *requested),
                _ => false,
            }
        }
        _ => false,
    }
}

fn config_override_is_current(
    options: &[SessionConfigOption],
    key: &str,
    value: &CodingAgentConfigValue,
) -> bool {
    let Some(option) = options.iter().find(|option| option.id.to_string() == key) else {
        return false;
    };
    match (&option.kind, value) {
        (SessionConfigKind::Boolean(current), CodingAgentConfigValue::Bool(requested)) => {
            current.current_value == *requested
        }
        (SessionConfigKind::Select(current), CodingAgentConfigValue::String(requested)) => {
            current.current_value.to_string() == *requested
        }
        _ => false,
    }
}

fn config_params(session_id: &str, key: &str, value: &CodingAgentConfigValue) -> Option<Value> {
    match value {
        CodingAgentConfigValue::String(value) => {
            Some(json!({"sessionId":session_id,"configId":key,"value":value}))
        }
        CodingAgentConfigValue::Bool(value) => {
            Some(json!({"sessionId":session_id,"configId":key,"type":"boolean","value":value}))
        }
        CodingAgentConfigValue::Integer(_) => None,
    }
}

fn normalize_update(message: &Value) -> Option<CodingAgentEvent> {
    let update = message.get("params")?.get("update")?;
    let kind = update.get("sessionUpdate")?.as_str()?;
    let text = update
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .map(bounded_text);
    match kind {
        "agent_message_chunk" => Some(event(CodingAgentEventKind::AgentMessage, text, None, None)),
        "agent_thought_chunk" => Some(event(CodingAgentEventKind::Reasoning, text, None, None)),
        "plan" => Some(event(
            CodingAgentEventKind::Plan,
            None,
            Some("plan".to_string()),
            Some("updated".to_string()),
        )),
        "tool_call" | "tool_call_update" => {
            let label = update
                .get("title")
                .and_then(Value::as_str)
                .map(bounded_text)
                .or_else(|| update.get("kind").and_then(Value::as_str).map(bounded_text));
            let status = update
                .get("status")
                .and_then(Value::as_str)
                .map(bounded_text);
            let event_kind = match update.get("kind").and_then(Value::as_str) {
                Some("edit") | Some("delete") | Some("move") => CodingAgentEventKind::FileChange,
                Some("execute") => CodingAgentEventKind::TerminalActivity,
                _ => CodingAgentEventKind::ToolActivity,
            };
            Some(event(event_kind, None, label, status))
        }
        "usage_update" => {
            let usage = CodingAgentUsage {
                used_tokens: update.get("used").and_then(Value::as_u64),
                context_window_tokens: update.get("size").and_then(Value::as_u64),
                cost_amount: update
                    .pointer("/cost/amount")
                    .and_then(Value::as_f64)
                    .map(|amount| amount.to_string()),
                cost_currency: update
                    .pointer("/cost/currency")
                    .and_then(Value::as_str)
                    .map(bounded_text),
            };
            Some(CodingAgentEvent {
                sequence: 0,
                kind: CodingAgentEventKind::Usage,
                text: None,
                label: None,
                status: None,
                usage: Some(usage),
            })
        }
        _ => None,
    }
}

fn permission_event(params: &Value) -> CodingAgentEvent {
    let label = params
        .pointer("/toolCall/title")
        .and_then(Value::as_str)
        .map(bounded_text);
    let count = params
        .get("options")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    CodingAgentEvent {
        sequence: 0,
        kind: CodingAgentEventKind::PermissionRequest,
        text: None,
        label,
        status: Some(format!("pending:{count}_options")),
        usage: None,
    }
}

fn event(
    kind: CodingAgentEventKind,
    text: Option<String>,
    label: Option<String>,
    status: Option<String>,
) -> CodingAgentEvent {
    CodingAgentEvent {
        sequence: 0,
        kind,
        text,
        label,
        status,
        usage: None,
    }
}

fn bounded_text(value: &str) -> String {
    const MAX: usize = webcodex_core::coding_agent::CODING_AGENT_MAX_EVENT_TEXT_BYTES;
    const SUFFIX: &str = "…";
    if value.len() <= MAX {
        return value.to_string();
    }
    let mut end = MAX.saturating_sub(SUFFIX.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = String::with_capacity(MAX);
    bounded.push_str(&value[..end]);
    bounded.push_str(SUFFIX);
    debug_assert!(bounded.len() <= MAX);
    bounded
}
fn bounded_json_summary(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "invalid_json".to_string());
    bounded_text(&text)
}
fn now() -> i64 {
    Utc::now().timestamp()
}

fn response_error(
    dispatch: CodingAgentDispatchState,
    code: &str,
    message: impl Into<String>,
    failure: &str,
    recovery: &str,
) -> CodingAgentResponse {
    CodingAgentResponse::error(
        dispatch,
        code,
        message.into(),
        Some(failure),
        Some(recovery),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[derive(Default)]
    struct BlockingWriteState {
        entered: bool,
        released: bool,
    }

    #[derive(Clone)]
    struct BlockingWrite {
        state: Arc<(Mutex<BlockingWriteState>, Condvar)>,
    }

    impl Write for BlockingWrite {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let (state, changed) = &*self.state;
            let mut state = state.lock().unwrap();
            state.entered = true;
            changed.notify_all();
            while !state.released {
                state = changed.wait(state).unwrap();
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn wait_for_blocking_write(state: &Arc<(Mutex<BlockingWriteState>, Condvar)>) {
        let (state, changed) = &**state;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut state = state.lock().unwrap();
        while !state.entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "writer never entered blocking sink");
            let (next, _) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
    }

    fn release_blocking_write(state: &Arc<(Mutex<BlockingWriteState>, Condvar)>) {
        let (state, changed) = &**state;
        let mut state = state.lock().unwrap();
        state.released = true;
        changed.notify_all();
    }

    fn fake_config(executable: String, args: Vec<String>) -> AcpConfig {
        AcpConfig {
            max_concurrent_runs: 1,
            permission_timeout_secs: 1,
            agents: vec![AcpAgentConfig {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                executable,
                args,
                env_from_env: BTreeMap::new(),
                allowed_config_options: vec!["mode".to_string()],
            }],
        }
    }

    #[cfg(unix)]
    fn fake_agent(temp: &TempDir, scenario: &str) -> (String, Vec<String>) {
        let path = temp.path().join("fake-acp.py");
        let script = r#"#!/usr/bin/env python3
import json,os,sys,time,subprocess
scenario=sys.argv[1]
config_values={'one':'a','two':'a','three':'a','four':'a'}
log_path=os.path.join(os.path.dirname(__file__),'fake-acp.log')
def log(x):
 with open(log_path,'a',encoding='utf-8') as f: f.write(json.dumps(x,separators=(',',':'))+'\n')
def send(x):
 log({'send':x}); print(json.dumps(x),flush=True)
log({'startup_pid':os.getpid(),'env_keys':sorted(k for k in os.environ if k.startswith('WEBCODEX_TEST_ACP_') or k=='ACP_VISIBLE')})
for line in sys.stdin:
 m=json.loads(line); log({'recv':m}); method=m.get('method'); rid=m.get('id')
 if method=='initialize':
  if scenario=='crash_before_prompt': sys.exit(9)
  if scenario in ('block_initialize','block_initialize_tree'):
   if scenario=='block_initialize_tree':
    child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   ready=os.path.join(os.path.dirname(__file__),'initialize.ready')
   release=os.path.join(os.path.dirname(__file__),'initialize.release')
   open(ready,'w').close()
   while not os.path.exists(release): time.sleep(0.01)
  send({'jsonrpc':'2.0','id':rid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
 elif method=='session/new':
  if scenario=='slow_configs':
   opts=[{'id':k,'name':k.title(),'type':'select','currentValue':config_values[k],'options':[{'value':'a','name':'A'},{'value':'b','name':'B'}]} for k in config_values]
  else:
   opts=[{'id':'mode','name':'Mode','type':'select','currentValue':'agent','options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]}]
  session_id='s'*70000 if scenario=='block_cancel_write' else 's1'
  send({'jsonrpc':'2.0','id':rid,'result':{'sessionId':session_id,'configOptions':opts}})
  if scenario in ('block_after_session_new','block_after_session_new_tree'):
   if scenario=='block_after_session_new_tree':
    child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   open(os.path.join(os.path.dirname(__file__),'stdin_stopped.ready'),'w').close()
   while True: time.sleep(1)
 elif method=='session/set_config_option':
  if scenario=='slow_configs':
   time.sleep(0.6)
   k=m['params']['configId']; v=m['params']['value']; config_values[k]=v
   opts=[{'id':key,'name':key.title(),'type':'select','currentValue':config_values[key],'options':[{'value':'a','name':'A'},{'value':'b','name':'B'}]} for key in config_values]
  else:
   v=m['params']['value']; opts=[{'id':'mode','name':'Mode','type':'select','currentValue':v,'options':[{'value':'agent','name':'Agent'},{'value':'read-only','name':'Read Only'}]}]
  send({'jsonrpc':'2.0','id':rid,'result':{'configOptions':opts}})
 elif method=='session/prompt':
  if scenario=='crash_after_prompt': sys.exit(7)
  if scenario=='spawn_descendant':
   child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
  if scenario=='block_cancel_write':
   child=subprocess.Popen(['/bin/sh','-c','sleep 60']); log({'descendant_pid':child.pid})
   open(os.path.join(os.path.dirname(__file__),'prompt_read.ready'),'w').close()
   while True: time.sleep(1)
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'hello'}}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_thought_chunk','content':{'type':'text','text':'thinking'}}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'plan','entries':[]}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'tool_call','toolCallId':'t1','title':'inspect','kind':'execute','status':'in_progress'}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'tool_call','toolCallId':'t2','title':'edit','kind':'edit','status':'in_progress'}}})
  send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'usage_update','used':53,'size':200,'cost':{'amount':0.045,'currency':'USD'}}}})
  if scenario=='many_events':
   for i in range(300): send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s1','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'m'+str(i)}}}})
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'end_turn'}})
  elif scenario=='permission':
   send({'jsonrpc':'2.0','id':99,'method':'session/request_permission','params':{'sessionId':'s1','toolCall':{'toolCallId':'t1','title':'permission','status':'pending'},'options':[{'optionId':'allow','name':'Allow','kind':'allow_once'}]}})
   response=json.loads(sys.stdin.readline()); log({'recv':response})
   assert response['result']['outcome']['outcome']=='cancelled'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='permission_hold':
   send({'jsonrpc':'2.0','id':99,'method':'session/request_permission','params':{'sessionId':'s1','toolCall':{'toolCallId':'t1','title':'permission','status':'pending'},'options':[{'optionId':'allow','name':'Allow','kind':'allow_once'}]}})
   response=json.loads(sys.stdin.readline()); log({'recv':response})
   assert response['result']['outcome']['outcome']=='cancelled'
   cancel=json.loads(sys.stdin.readline()); log({'recv':cancel}); assert cancel.get('method')=='session/cancel'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='unsupported_callback':
   send({'jsonrpc':'2.0','id':98,'method':'fs/read_text_file','params':{'path':'private'}})
   response=json.loads(sys.stdin.readline()); log({'recv':response}); assert response['error']['code']==-32601
   cancel=json.loads(sys.stdin.readline()); log({'recv':cancel}); assert cancel.get('method')=='session/cancel'
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}})
  elif scenario=='wait_cancel':
   while True:
    x=json.loads(sys.stdin.readline()); log({'recv':x})
    if x.get('method')=='session/cancel': send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':'cancelled'}}); break
  else:
   stop={'end':'end_turn','spawn_descendant':'end_turn','cancelled':'cancelled','max_tokens':'max_tokens','max_turn_requests':'max_turn_requests','refusal':'refusal','unknown':'future_reason'}.get(scenario,'end_turn')
   send({'jsonrpc':'2.0','id':rid,'result':{'stopReason':stop}})
"#;
        fs::write(&path, script).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        (
            path.to_string_lossy().to_string(),
            vec![scenario.to_string()],
        )
    }

    #[cfg(unix)]
    fn project_fixture(temp: &TempDir) -> PathBuf {
        let root = temp.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        fs::write(
            projects.join("p.toml"),
            format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
        )
        .unwrap();
        projects
    }

    #[cfg(unix)]
    fn start_request(
        manager: &CodingAgentManager,
        root: &Path,
        run: &str,
        config: BTreeMap<String, CodingAgentConfigValue>,
    ) -> CodingAgentRequest {
        let provider = manager.providers().remove(0);
        CodingAgentRequest::Start(webcodex_core::coding_agent::CodingAgentStartRequest {
            run_id: run.to_string(),
            intent_fingerprint: "fingerprint".to_string(),
            authority_fingerprint: "auth_test".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            project_root: root.to_string_lossy().to_string(),
            provider_id: "codex".to_string(),
            provider_instance_id: provider.provider_instance_id,
            instruction: "inspect".to_string(),
            config,
            timeout_secs: 10,
        })
    }

    #[cfg(unix)]
    fn start_request_with_timeout(
        manager: &CodingAgentManager,
        root: &Path,
        run: &str,
        config: BTreeMap<String, CodingAgentConfigValue>,
        timeout_secs: u64,
    ) -> CodingAgentRequest {
        let mut request = start_request(manager, root, run, config);
        let CodingAgentRequest::Start(start) = &mut request else {
            unreachable!();
        };
        start.timeout_secs = timeout_secs;
        request
    }

    #[cfg(unix)]
    fn max_instruction_request(
        manager: &CodingAgentManager,
        root: &Path,
        run: &str,
        timeout_secs: u64,
    ) -> CodingAgentRequest {
        let mut request =
            start_request_with_timeout(manager, root, run, BTreeMap::new(), timeout_secs);
        let CodingAgentRequest::Start(start) = &mut request else {
            unreachable!();
        };
        start.instruction =
            "x".repeat(webcodex_core::coding_agent::CODING_AGENT_MAX_INSTRUCTION_BYTES);
        let frame = request_frame(
            3,
            "session/prompt",
            json!({
                "sessionId":"s1",
                "prompt":[{"type":"text","text":start.instruction.clone()}]
            }),
        )
        .unwrap();
        assert!(
            frame.len() > 64 * 1024,
            "max legal prompt frame must exceed the measured special Linux pipe capacity"
        );
        request
    }

    #[cfg(unix)]
    fn wait_for_prompt_handoff(manager: &CodingAgentManager, run: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
            if *entry.prompt_dispatch.lock().unwrap()
                == PromptDispatchGateState::PromptDispatchMayHaveOccurred
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "prompt was never handed to writer"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(unix)]
    fn wait_for_terminal_observation(
        manager: &CodingAgentManager,
        run: &str,
    ) -> CodingAgentObserveResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
            let observation = entry
                .observe(None, 64, 0)
                .expect("retained fake Run cursor must be valid");
            if observation.run.state.terminal()
                && observation
                    .events
                    .iter()
                    .any(|event| event.kind == CodingAgentEventKind::Terminal)
            {
                return observation;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for terminal ACP observation: {observation:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn run_scenario(
        scenario: &str,
        config: BTreeMap<String, CodingAgentConfigValue>,
    ) -> (Arc<CodingAgentManager>, String, CodingAgentObserveResult) {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, scenario);
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let store = temp.path().join("store");
        let manager = CodingAgentManager::with_store(&cfg, store).unwrap();
        let run = "wc_agent_run_0123456789abcdef".to_string();
        let response = manager.handle(start_request(&manager, &root, &run, config), &projects);
        assert!(response.error.is_none(), "{:?}", response.error);
        let observation = wait_for_terminal_observation(&manager, &run);
        std::mem::forget(temp);
        (manager, run, observation)
    }

    #[cfg(unix)]
    fn wait_for_snapshot_until(
        manager: &CodingAgentManager,
        run: &str,
        deadline: Instant,
        predicate: impl Fn(&CodingAgentRunSnapshot) -> bool,
    ) -> CodingAgentRunSnapshot {
        loop {
            let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for CodingAgentRun state: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn wait_for_snapshot(
        manager: &CodingAgentManager,
        run: &str,
        predicate: impl Fn(&CodingAgentRunSnapshot) -> bool,
    ) -> CodingAgentRunSnapshot {
        wait_for_snapshot_until(
            manager,
            run,
            Instant::now() + Duration::from_secs(5),
            predicate,
        )
    }

    #[cfg(unix)]
    fn wire_log(temp: &TempDir) -> Vec<Value> {
        let path = temp.path().join("fake-acp.log");
        if !path.exists() {
            return Vec::new();
        }
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[cfg(unix)]
    fn received_methods(log: &[Value]) -> Vec<String> {
        log.iter()
            .filter_map(|entry| entry.pointer("/recv/method").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }

    #[cfg(unix)]
    fn wait_for_received_method_count(temp: &TempDir, method: &str, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let count = received_methods(&wire_log(temp))
                .iter()
                .filter(|candidate| candidate.as_str() == method)
                .count();
            if count >= expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {expected} {method} calls; observed {count}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn wait_for_path(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {}",
                path.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    fn wait_for_proc_exit(pid: u64) {
        let proc_path = PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_secs(3);
        while proc_path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !proc_path.exists(),
            "process {pid} survived CodingAgent worker drain"
        );
    }

    fn successful_start_run_id(response: &CodingAgentResponse) -> Option<String> {
        match response.payload.as_ref() {
            Some(CodingAgentResponsePayload::Start { run }) => Some(run.run_id.clone()),
            _ => None,
        }
    }

    fn seed_terminal_test_run(
        manager: &CodingAgentManager,
        run: &str,
        phase: DurableDispatchPhase,
        state: CodingAgentRunState,
        execution_state: CodingAgentExecutionState,
    ) -> Arc<RunEntry> {
        let provider = manager.providers().remove(0);
        let timestamp = now();
        let record = DurableRunRecord {
            schema_version: STORE_SCHEMA_VERSION,
            run_id: run.to_string(),
            intent_fingerprint: "fingerprint".to_string(),
            authority_fingerprint: "auth_test".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            provider_id: "codex".to_string(),
            provider_instance_id: provider.provider_instance_id,
            state,
            execution_state,
            dispatch_phase: phase,
            created_at: timestamp,
            updated_at: timestamp,
            terminal: None,
        };
        manager.store.write(&record).unwrap();
        let entry = Arc::new(RunEntry::new(record.snapshot(0)));
        manager
            .runs
            .lock()
            .unwrap()
            .insert(run.to_string(), Arc::clone(&entry));
        entry
    }

    fn assert_terminal_persistence_uncertain(
        entry: &RunEntry,
        expected_state: CodingAgentRunState,
        expected_execution_state: CodingAgentExecutionState,
    ) {
        let snapshot = entry.snapshot();
        assert_eq!(snapshot.state, expected_state);
        assert_eq!(snapshot.execution_state, expected_execution_state);
        let terminal = snapshot.terminal.as_ref().expect("uncertain terminal");
        assert_eq!(terminal.stop_reason, None);
        assert_eq!(
            terminal.error_code.as_deref(),
            Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN)
        );
        assert_eq!(
            terminal.message.as_deref(),
            Some(TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE)
        );
        let observation = entry.observe(None, 64, 0).unwrap();
        let terminal_events = observation
            .events
            .iter()
            .filter(|event| event.kind == CodingAgentEventKind::Terminal)
            .collect::<Vec<_>>();
        assert_eq!(terminal_events.len(), 1);
        assert_eq!(
            terminal_events[0].label.as_deref(),
            Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN)
        );
        let expected_status = if expected_state == CodingAgentRunState::Failed {
            "failed"
        } else {
            "lost"
        };
        assert_eq!(terminal_events[0].status.as_deref(), Some(expected_status));
    }

    #[cfg(unix)]
    fn prompt_count(temp: &TempDir) -> usize {
        received_methods(&wire_log(temp))
            .iter()
            .filter(|method| method.as_str() == "session/prompt")
            .count()
    }

    #[test]
    fn post_prompt_failed_persistence_failure_is_lost_without_original_error_truth() {
        let temp = TempDir::new().unwrap();
        let cfg = fake_config("unused-provider".to_string(), Vec::new());
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_persistfailed01";
        let entry = seed_terminal_test_run(
            &manager,
            run,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred,
            CodingAgentRunState::Running,
            CodingAgentExecutionState::Started,
        );
        manager.store.fail_next_terminal_writes(1);

        manager.finish_failed(
            run,
            &entry,
            "prompt_error",
            "provider prompt failed".to_string(),
        );

        assert_terminal_persistence_uncertain(
            &entry,
            CodingAgentRunState::Lost,
            CodingAgentExecutionState::OutcomeUnknown,
        );
        assert_ne!(
            entry
                .snapshot()
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.error_code.as_deref()),
            Some("prompt_error")
        );
        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(
            durable.dispatch_phase,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred
        );
        assert_eq!(durable.terminal, None);
    }

    #[test]
    fn setup_timeout_persistence_failure_is_failed_not_started() {
        let temp = TempDir::new().unwrap();
        let cfg = fake_config("unused-provider".to_string(), Vec::new());
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_persisttimeout01";
        let entry = seed_terminal_test_run(
            &manager,
            run,
            DurableDispatchPhase::BeforePromptBarrier,
            CodingAgentRunState::Starting,
            CodingAgentExecutionState::NotStarted,
        );
        manager.store.fail_next_terminal_writes(1);

        manager.setup_timeout(run, &entry);

        assert_terminal_persistence_uncertain(
            &entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::NotStarted,
        );
        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(
            durable.dispatch_phase,
            DurableDispatchPhase::BeforePromptBarrier
        );
        assert_eq!(durable.terminal, None);
    }

    #[test]
    fn mark_lost_persistence_failure_replaces_specific_lost_reason() {
        let temp = TempDir::new().unwrap();
        let cfg = fake_config("unused-provider".to_string(), Vec::new());
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_persistlost001";
        let entry = seed_terminal_test_run(
            &manager,
            run,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred,
            CodingAgentRunState::Running,
            CodingAgentExecutionState::Started,
        );
        manager.store.fail_next_terminal_writes(1);

        manager.mark_lost(run, &entry, "coding_agent_transport_lost");

        assert_terminal_persistence_uncertain(
            &entry,
            CodingAgentRunState::Lost,
            CodingAgentExecutionState::OutcomeUnknown,
        );
        assert_ne!(
            entry
                .snapshot()
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.error_code.as_deref()),
            Some("coding_agent_transport_lost")
        );
    }

    #[test]
    #[cfg(unix)]
    fn completed_terminal_is_persisted_before_live_publication_and_restart() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let store_root = temp.path().join("store");
        let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
        let run = "wc_agent_run_persistorder001";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        let gate = Arc::new(TerminalWriteGate::default());
        manager
            .store
            .set_terminal_write_gate(Some(Arc::clone(&gate)));

        assert!(manager.handle(request.clone(), &projects).error.is_none());
        gate.wait_until_reached();
        let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
        let before = entry.observe(None, 64, 0).unwrap();
        assert!(!before.run.state.terminal());
        assert_ne!(before.run.state, CodingAgentRunState::Completed);
        assert!(before
            .events
            .iter()
            .all(|event| event.kind != CodingAgentEventKind::Terminal));
        let durable_before = manager.store.read(run).unwrap().unwrap();
        assert_eq!(
            durable_before.dispatch_phase,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred
        );
        assert_ne!(durable_before.state, CodingAgentRunState::Completed);

        gate.release();
        manager.store.set_terminal_write_gate(None);
        let observation = wait_for_terminal_observation(&manager, run);
        assert_eq!(observation.run.state, CodingAgentRunState::Completed);
        assert_eq!(
            observation.run.execution_state,
            CodingAgentExecutionState::Completed
        );
        assert_eq!(
            observation
                .run
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.stop_reason.as_deref()),
            Some(CODING_AGENT_STOP_REASON_END_TURN)
        );
        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
        assert_eq!(durable.state, CodingAgentRunState::Completed);
        assert_eq!(
            durable.execution_state,
            CodingAgentExecutionState::Completed
        );
        assert_eq!(durable.terminal, observation.run.terminal);
        assert_eq!(durable.updated_at, observation.run.updated_at);
        assert_eq!(prompt_count(&temp), 1);
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);

        let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
        let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(restored.state, CodingAgentRunState::Completed);
        assert_eq!(
            restored.execution_state,
            CodingAgentExecutionState::Completed
        );
        assert!(restarted.handle(request, &projects).error.is_none());
        assert_eq!(prompt_count(&temp), 1);
    }

    #[test]
    #[cfg(unix)]
    fn completion_persistence_failure_publishes_only_lost_and_never_redispatches() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let store_root = temp.path().join("store");
        let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
        let run = "wc_agent_run_persistendfail1";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        let gate = Arc::new(TerminalWriteGate::default());
        manager.store.fail_next_terminal_writes(1);
        manager
            .store
            .set_terminal_write_gate(Some(Arc::clone(&gate)));

        assert!(manager.handle(request.clone(), &projects).error.is_none());
        gate.wait_until_reached();
        let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
        let before = entry.observe(None, 64, 0).unwrap();
        assert!(!before.run.state.terminal());
        assert_ne!(before.run.state, CodingAgentRunState::Completed);
        assert!(before
            .events
            .iter()
            .all(|event| event.kind != CodingAgentEventKind::Terminal));
        gate.release();
        manager.store.set_terminal_write_gate(None);

        let observation = wait_for_terminal_observation(&manager, run);
        assert_terminal_persistence_uncertain(
            &entry,
            CodingAgentRunState::Lost,
            CodingAgentExecutionState::OutcomeUnknown,
        );
        assert!(observation.events.iter().all(|event| {
            event.kind != CodingAgentEventKind::Terminal
                || event.label.as_deref() != Some(CODING_AGENT_STOP_REASON_END_TURN)
        }));
        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(
            durable.dispatch_phase,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred
        );
        assert_ne!(durable.state, CodingAgentRunState::Completed);
        assert_eq!(prompt_count(&temp), 1);
        assert!(manager.handle(request.clone(), &projects).error.is_none());
        assert_eq!(prompt_count(&temp), 1);
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);

        let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
        let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(restored.state, CodingAgentRunState::Lost);
        assert_eq!(
            restored.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
        assert!(restarted.handle(request, &projects).error.is_none());
        assert_eq!(prompt_count(&temp), 1);
    }

    #[test]
    #[cfg(unix)]
    fn pre_prompt_cancel_persistence_failure_is_failed_and_restart_never_prompts() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_initialize");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let store_root = temp.path().join("store");
        let manager = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
        let run = "wc_agent_run_persistcancel01";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        manager.store.fail_next_terminal_writes(1);

        assert!(manager.handle(request.clone(), &projects).error.is_none());
        wait_for_path(&temp.path().join("initialize.ready"));
        let cancelled = manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        assert!(cancelled.error.is_none());
        let entry = manager.runs.lock().unwrap().get(run).cloned().unwrap();
        assert_terminal_persistence_uncertain(
            &entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::NotStarted,
        );
        assert_eq!(prompt_count(&temp), 0);
        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(
            durable.dispatch_phase,
            DurableDispatchPhase::BeforePromptBarrier
        );
        assert_eq!(durable.terminal, None);
        assert!(manager.handle(request.clone(), &projects).error.is_none());
        assert_eq!(prompt_count(&temp), 0);

        fs::write(temp.path().join("initialize.release"), b"release").unwrap();
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
        let restored = restarted.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(restored.state, CodingAgentRunState::Failed);
        assert_eq!(
            restored.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert!(restarted.handle(request, &projects).error.is_none());
        assert_eq!(prompt_count(&temp), 0);
    }

    #[test]
    fn concurrent_pre_prompt_terminal_transitions_commit_one_terminal_truth() {
        let temp = TempDir::new().unwrap();
        let cfg = fake_config("unused-provider".to_string(), Vec::new());
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let provider = manager.providers().remove(0);
        let run = "wc_agent_run_terminalrace01";
        let timestamp = now();
        let record = DurableRunRecord {
            schema_version: STORE_SCHEMA_VERSION,
            run_id: run.to_string(),
            intent_fingerprint: "fingerprint".to_string(),
            authority_fingerprint: "auth_test".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            provider_id: "codex".to_string(),
            provider_instance_id: provider.provider_instance_id,
            state: CodingAgentRunState::Starting,
            execution_state: CodingAgentExecutionState::NotStarted,
            dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
            created_at: timestamp,
            updated_at: timestamp,
            terminal: None,
        };
        manager.store.write(&record).unwrap();
        let entry = Arc::new(RunEntry::new(record.snapshot(0)));
        manager
            .runs
            .lock()
            .unwrap()
            .insert(run.to_string(), Arc::clone(&entry));

        let race = Arc::new(std::sync::Barrier::new(3));
        let failure_manager = Arc::clone(&manager);
        let failure_entry = Arc::clone(&entry);
        let failure_race = Arc::clone(&race);
        let failure = thread::spawn(move || {
            failure_race.wait();
            failure_manager.setup_failure(run, &failure_entry, "setup_failed", "setup failed");
        });
        let cancel_manager = Arc::clone(&manager);
        let cancel_entry = Arc::clone(&entry);
        let cancel_race = Arc::clone(&race);
        let cancel = thread::spawn(move || {
            cancel_race.wait();
            cancel_manager.finish_pre_prompt_cancelled(run, &cancel_entry);
        });
        race.wait();
        failure.join().unwrap();
        cancel.join().unwrap();

        let snapshot = entry.snapshot();
        assert!(matches!(
            snapshot.state,
            CodingAgentRunState::Cancelled | CodingAgentRunState::Failed
        ));
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        validate_coding_agent_run_snapshot(&snapshot).unwrap();

        let durable = manager.store.read(run).unwrap().unwrap();
        assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
        assert_eq!(durable.state, snapshot.state);
        assert_eq!(durable.execution_state, snapshot.execution_state);
        assert_eq!(durable.terminal, snapshot.terminal);

        let live = entry.state.lock().unwrap();
        assert_eq!(
            live.events
                .iter()
                .filter(|event| event.kind == CodingAgentEventKind::Terminal)
                .count(),
            1,
            "a terminal race emitted more than one terminal event"
        );
    }

    #[test]
    fn outbound_writer_blocking_sink_does_not_block_lifecycle_owner() {
        let state = Arc::new((Mutex::new(BlockingWriteState::default()), Condvar::new()));
        let threads = BackgroundThreads::default();
        let mut writer = AcpOutboundWriter::spawn(
            BlockingWrite {
                state: Arc::clone(&state),
            },
            &threads,
        )
        .unwrap();
        let pending = writer.start_frame(vec![b'x'; 1024]).unwrap();
        wait_for_blocking_write(&state);

        let cancelled = AtomicBool::new(true);
        let outcome = wait_outbound_write(
            pending,
            Instant::now() + Duration::from_secs(1),
            Some(&cancelled),
            None,
        );
        assert!(matches!(
            outcome,
            OutboundWriteOutcome::Interrupted(OutboundInterruption::Cancelled)
        ));
        assert_eq!(threads.pending(), 1);

        // The production owner uses ManagedChild::terminate_tree to make a blocked
        // pipe write return. Releasing this deterministic sink models that exact
        // post-interruption effect without relying on pipe capacity or sleeps.
        release_blocking_write(&state);
        writer.close();
        assert!(writer.wait_finished_until(Instant::now() + Duration::from_secs(1)));
        let joined = threads.join_until(Instant::now() + Duration::from_secs(1));
        assert_eq!(joined.timed_out, 0);
        assert_eq!(joined.panicked, 0);
        assert_eq!(threads.pending(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn acp_v1_sequence_cwd_config_and_normalized_updates_are_exact() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_sequence0001";
        let response = manager.handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        );
        assert!(response.error.is_none(), "{:?}", response.error);
        let observation = wait_for_terminal_observation(&manager, run);
        assert_eq!(observation.run.state, CodingAgentRunState::Completed);

        let log = wire_log(&temp);
        assert_eq!(
            received_methods(&log),
            vec!["initialize", "session/new", "session/prompt"]
        );
        let initialize = log
            .iter()
            .find_map(|entry| {
                (entry.pointer("/recv/method").and_then(Value::as_str) == Some("initialize"))
                    .then(|| entry.pointer("/recv").unwrap())
            })
            .unwrap();
        assert_eq!(
            initialize.pointer("/params/clientCapabilities"),
            Some(&json!({}))
        );
        let session_new = log
            .iter()
            .find_map(|entry| {
                (entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/new"))
                    .then(|| entry.pointer("/recv").unwrap())
            })
            .unwrap();
        assert_eq!(
            session_new.pointer("/params/cwd").and_then(Value::as_str),
            Some(root.to_string_lossy().as_ref())
        );
        assert_eq!(session_new.pointer("/params/mcpServers"), Some(&json!([])));

        for kind in [
            CodingAgentEventKind::AgentMessage,
            CodingAgentEventKind::Reasoning,
            CodingAgentEventKind::Plan,
            CodingAgentEventKind::TerminalActivity,
            CodingAgentEventKind::FileChange,
            CodingAgentEventKind::Usage,
            CodingAgentEventKind::Terminal,
        ] {
            assert!(
                observation.events.iter().any(|event| event.kind == kind),
                "missing {kind:?}"
            );
        }
        let usage = observation
            .events
            .iter()
            .find(|event| event.kind == CodingAgentEventKind::Usage)
            .and_then(|event| event.usage.as_ref())
            .unwrap();
        assert_eq!(usage.used_tokens, Some(53));
        assert_eq!(usage.context_window_tokens, Some(200));
        assert_eq!(usage.cost_amount.as_deref(), Some("0.045"));
        assert_eq!(usage.cost_currency.as_deref(), Some("USD"));
    }

    #[test]
    #[cfg(unix)]
    fn explicit_config_is_ordered_and_invalid_config_never_prompts() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_config000001";
        let response = manager.handle(
            start_request(
                &manager,
                &root,
                run,
                BTreeMap::from([(
                    "mode".to_string(),
                    CodingAgentConfigValue::String("read-only".to_string()),
                )]),
            ),
            &projects,
        );
        assert!(response.error.is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(
            received_methods(&wire_log(&temp)),
            vec![
                "initialize",
                "session/new",
                "session/set_config_option",
                "session/prompt"
            ]
        );

        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_badconfig001";
        let response = manager.handle(
            start_request(
                &manager,
                &root,
                run,
                BTreeMap::from([(
                    "not-advertised".to_string(),
                    CodingAgentConfigValue::String("x".to_string()),
                )]),
            ),
            &projects,
        );
        assert!(response.error.is_none());
        let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(terminal.state, CodingAgentRunState::Failed);
        assert_eq!(
            terminal.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert!(!received_methods(&wire_log(&temp))
            .iter()
            .any(|method| method == "session/prompt"));
    }

    #[test]
    #[cfg(unix)]
    fn cancel_permission_and_unsupported_requests_are_fail_closed() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "permission_hold");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_permcancel01";
        assert!(manager
            .handle(
                start_request(&manager, &root, run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        wait_for_snapshot(&manager, run, |snapshot| {
            snapshot.state == CodingAgentRunState::WaitingPermission
        });
        let cancel = manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        assert!(cancel.error.is_none());
        let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(terminal.state, CodingAgentRunState::Cancelled);
        let log = wire_log(&temp);
        let permission_cancel = log
            .iter()
            .position(|entry| {
                entry.pointer("/recv/id").and_then(Value::as_u64) == Some(99)
                    && entry
                        .pointer("/recv/result/outcome/outcome")
                        .and_then(Value::as_str)
                        == Some("cancelled")
            })
            .unwrap();
        let prompt_cancel = log
            .iter()
            .position(|entry| {
                entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/cancel")
            })
            .unwrap();
        assert!(
            permission_cancel < prompt_cancel,
            "pending permission must be completed before prompt cancel"
        );

        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "unsupported_callback");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_unsupported1";
        assert!(manager
            .handle(
                start_request(&manager, &root, run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        let terminal = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(terminal.state, CodingAgentRunState::Cancelled);
        let log = wire_log(&temp);
        assert!(log.iter().any(
            |entry| entry.pointer("/recv/error/code").and_then(Value::as_i64) == Some(-32601)
        ));
        assert!(log.iter().any(
            |entry| entry.pointer("/recv/method").and_then(Value::as_str) == Some("session/cancel")
        ));
    }

    #[test]
    fn bounded_text_never_exceeds_utf8_byte_budget() {
        let max = webcodex_core::coding_agent::CODING_AGENT_MAX_EVENT_TEXT_BYTES;
        let cases = [
            "a".repeat(max),
            "a".repeat(max + 1),
            "é".repeat(max / "é".len() + 2),
            "€".repeat(max / "€".len() + 2),
            "🦀".repeat(max / "🦀".len() + 2),
            "z".repeat(max * 4),
        ];
        for input in cases {
            let output = bounded_text(&input);
            assert!(output.len() <= max, "{} > {max}", output.len());
            assert!(std::str::from_utf8(output.as_bytes()).is_ok());
            if input.len() <= max {
                assert_eq!(output, input);
            } else {
                assert!(output.ends_with('…'));
            }
        }

        let json = json!({"body": "界".repeat(max * 2)});
        let summary = bounded_json_summary(&json);
        assert!(summary.len() <= max);
        assert!(summary.ends_with('…'));
    }

    #[test]
    #[cfg(unix)]
    fn event_ring_capacity_and_continuation_are_bounded() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "many_events");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_manyevents01";
        assert!(manager
            .handle(
                start_request(&manager, &root, run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        let entry = manager.runs.lock().unwrap().get(run).unwrap().clone();
        let first = entry.observe(Some(0), 32, 0).unwrap();
        assert!(first.run.state.terminal());
        assert!(first.history_lost);
        assert!(first.has_more);
        assert_eq!(first.events.len(), 32);
        assert!(first.first_retained_sequence > 1);
        let second = entry.observe(Some(first.next_sequence), 32, 0).unwrap();
        assert!(second.run.state.terminal());
        assert!(!second.events.is_empty());
        assert!(second.events.first().unwrap().sequence > first.events.last().unwrap().sequence);

        let latest = entry.state.lock().unwrap().next_sequence.saturating_sub(1);
        let error = entry
            .observe(Some(latest.saturating_add(1)), 32, 0)
            .unwrap_err();
        assert!(
            error.contains("ahead of latest emitted sequence"),
            "{error}"
        );
        let response = manager.handle(
            CodingAgentRequest::Observe(webcodex_core::coding_agent::CodingAgentObserveRequest {
                run_id: run.to_string(),
                after_sequence: Some(latest.saturating_add(1)),
                limit: 32,
                wait_secs: 0,
            }),
            &projects,
        );
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("invalid_coding_agent_observation_cursor")
        );
    }

    #[test]
    #[cfg(unix)]
    fn concurrent_duplicate_start_admission_creates_exactly_one_prompt() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "wait_cancel");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_concurrentdup01";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        *manager.admission_test_barrier.lock().unwrap() =
            Some(Arc::new(std::sync::Barrier::new(2)));

        let first_manager = Arc::clone(&manager);
        let first_projects = projects.clone();
        let first_request = request.clone();
        let first = thread::spawn(move || first_manager.handle(first_request, &first_projects));
        let second_manager = Arc::clone(&manager);
        let second_projects = projects.clone();
        let second = thread::spawn(move || second_manager.handle(request, &second_projects));

        let first = first.join().unwrap();
        let second = second.join().unwrap();
        *manager.admission_test_barrier.lock().unwrap() = None;
        assert!(first.error.is_none(), "{:?}", first.error);
        assert!(second.error.is_none(), "{:?}", second.error);
        assert_eq!(successful_start_run_id(&first).as_deref(), Some(run));
        assert_eq!(successful_start_run_id(&second).as_deref(), Some(run));
        assert_eq!(
            manager
                .initial_claim_writes
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "duplicate concurrent admission must publish only one initial durable claim"
        );
        assert_eq!(manager.runs.lock().unwrap().len(), 1);
        wait_for_received_method_count(&temp, "session/prompt", 1);
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1,
            "duplicate concurrent admission dispatched more than one ACP prompt"
        );

        manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
    }

    #[test]
    #[cfg(unix)]
    fn concurrent_capacity_admission_never_exceeds_configured_limit() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "wait_cancel");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        *manager.admission_test_barrier.lock().unwrap() =
            Some(Arc::new(std::sync::Barrier::new(2)));
        let first_request = start_request(
            &manager,
            &root,
            "wc_agent_run_concurrentcap01",
            BTreeMap::new(),
        );
        let second_request = start_request(
            &manager,
            &root,
            "wc_agent_run_concurrentcap02",
            BTreeMap::new(),
        );

        let first_manager = Arc::clone(&manager);
        let first_projects = projects.clone();
        let first = thread::spawn(move || first_manager.handle(first_request, &first_projects));
        let second_manager = Arc::clone(&manager);
        let second_projects = projects.clone();
        let second = thread::spawn(move || second_manager.handle(second_request, &second_projects));

        let responses = [first.join().unwrap(), second.join().unwrap()];
        *manager.admission_test_barrier.lock().unwrap() = None;
        let successes = responses
            .iter()
            .filter_map(successful_start_run_id)
            .collect::<Vec<_>>();
        let capacity_failures = responses
            .iter()
            .filter(|response| {
                response.error.as_ref().map(|error| error.code.as_str())
                    == Some("coding_agent_capacity_full")
            })
            .count();
        assert_eq!(successes.len(), 1, "exactly one Run may acquire the slot");
        assert_eq!(
            capacity_failures, 1,
            "the competing Run must fail capacity admission"
        );
        assert_eq!(
            manager
                .initial_claim_writes
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "capacity admission must publish only the winning durable claim"
        );
        assert_eq!(
            manager
                .runs
                .lock()
                .unwrap()
                .values()
                .filter(|entry| !entry.snapshot().state.terminal())
                .count(),
            1,
            "active Run count exceeded max_concurrent_runs=1"
        );
        wait_for_received_method_count(&temp, "session/prompt", 1);
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1
        );

        let winner = &successes[0];
        manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: winner.clone(),
            }),
            &projects,
        );
        wait_for_snapshot(&manager, winner, |snapshot| snapshot.state.terminal());
    }

    #[test]
    #[cfg(unix)]
    fn initialize_wait_consumes_total_run_deadline() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_initialize");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_totalinitialize01";
        let started_at = Instant::now();
        let started = manager.handle(
            start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 1),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert!(started_at.elapsed() < Duration::from_secs(3));
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(snapshot.state, CodingAgentRunState::Failed);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            snapshot
                .terminal
                .as_ref()
                .and_then(|t| t.error_code.as_deref()),
            Some("coding_agent_setup_timeout")
        );
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
    }

    #[test]
    #[cfg(unix)]
    fn config_setup_cumulatively_consumes_total_run_deadline() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "slow_configs");
        let mut cfg = fake_config(exe, args);
        cfg.agents[0].allowed_config_options = ["one", "two", "three", "four"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_totalconfigs001";
        let config = ["one", "two", "three", "four"]
            .into_iter()
            .map(|key| {
                (
                    key.to_string(),
                    CodingAgentConfigValue::String("b".to_string()),
                )
            })
            .collect();
        let started = manager.handle(
            start_request_with_timeout(&manager, &root, run, config, 2),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(snapshot.state, CodingAgentRunState::Failed);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            snapshot
                .terminal
                .as_ref()
                .and_then(|t| t.error_code.as_deref()),
            Some("coding_agent_setup_timeout")
        );
        let methods = received_methods(&wire_log(&temp));
        assert!(
            methods
                .iter()
                .filter(|m| m.as_str() == "session/set_config_option")
                .count()
                >= 3,
            "expected cumulative config setup before total deadline: {methods:?}"
        );
        assert_eq!(
            methods
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
    }

    #[test]
    #[cfg(unix)]
    fn deadline_after_durable_prompt_barrier_still_prevents_prompt_write() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_deadlinebarrier01";
        *manager.prompt_after_barrier_test_delay.lock().unwrap() =
            Some(Duration::from_millis(1100));
        let started = manager.handle(
            start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 1),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        *manager.prompt_after_barrier_test_delay.lock().unwrap() = None;
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(snapshot.state, CodingAgentRunState::Failed);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            snapshot
                .terminal
                .as_ref()
                .and_then(|t| t.error_code.as_deref()),
            Some("coding_agent_setup_timeout")
        );
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
    }

    #[test]
    #[cfg(unix)]
    fn blocked_max_prompt_write_respects_total_deadline_and_reaps_tree() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_promptbackpressure01";
        let started_at = Instant::now();
        // Process startup and ACP negotiation are setup for this assertion, not
        // the behavior under test. Hosted macOS VMs have already demonstrated
        // that a three-second total budget can expire before the fake provider
        // reaches the intended blocked ChildStdin state. Use the same bounded
        // ten-second Run budget as the adjacent blocked-write lifecycle tests;
        // the assertions below still require the permanently blocked write to
        // terminate at the total Run deadline and preserve uncertainty/reaping.
        let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("stdin_stopped.ready"));
        wait_for_prompt_handoff(&manager, run);
        let observation_deadline = started_at + Duration::from_secs(13);
        let snapshot = wait_for_snapshot_until(&manager, run, observation_deadline, |snapshot| {
            snapshot.state.terminal()
        });
        assert!(
            Instant::now() < observation_deadline,
            "blocked prompt write escaped the total Run deadline"
        );
        assert_eq!(snapshot.state, CodingAgentRunState::Lost);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
        assert_eq!(
            *manager
                .runs
                .lock()
                .unwrap()
                .get(run)
                .unwrap()
                .prompt_dispatch
                .lock()
                .unwrap(),
            PromptDispatchGateState::PromptDispatchMayHaveOccurred
        );
        let log = wire_log(&temp);
        let startup_pid = log
            .iter()
            .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
            .unwrap();
        let descendant_pid = log
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(drain.panicked, 0);
        assert_eq!(manager.worker_count(), 0);
        #[cfg(target_os = "linux")]
        {
            wait_for_proc_exit(startup_pid);
            wait_for_proc_exit(descendant_pid);
        }
    }

    #[test]
    #[cfg(unix)]
    fn cancel_returns_while_max_prompt_write_is_blocked() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_cancelblockedprompt1";
        let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("stdin_stopped.ready"));
        wait_for_prompt_handoff(&manager, run);

        let cancel_started = Instant::now();
        let cancelled = manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        assert!(
            cancel_started.elapsed() < Duration::from_secs(1),
            "Cancel waited for blocked ChildStdin write"
        );
        let cancel_snapshot = match cancelled.payload.unwrap() {
            CodingAgentResponsePayload::Cancel { run } => run,
            other => panic!("unexpected cancel payload: {other:?}"),
        };
        assert_ne!(
            cancel_snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        let snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(snapshot.state, CodingAgentRunState::Lost);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
        let log = wire_log(&temp);
        let startup_pid = log
            .iter()
            .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
            .unwrap();
        let descendant_pid = log
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(manager.worker_count(), 0);
        #[cfg(target_os = "linux")]
        {
            wait_for_proc_exit(startup_pid);
            wait_for_proc_exit(descendant_pid);
        }
    }

    #[test]
    #[cfg(unix)]
    fn shutdown_remains_bounded_while_max_prompt_write_is_blocked() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_after_session_new_tree");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_shutdownblockedprompt";
        let started = manager.handle(max_instruction_request(&manager, &root, run, 10), &projects);
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("stdin_stopped.ready"));
        wait_for_prompt_handoff(&manager, run);

        let stop_started = Instant::now();
        manager.stop_accepting();
        assert!(
            stop_started.elapsed() < Duration::from_secs(1),
            "stop_accepting waited for blocked ChildStdin write"
        );
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(4));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(drain.panicked, 0);
        assert_eq!(manager.worker_count(), 0);
        let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(snapshot.state, CodingAgentRunState::Lost);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
        let log = wire_log(&temp);
        let startup_pid = log
            .iter()
            .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
            .unwrap();
        let descendant_pid = log
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();
        #[cfg(target_os = "linux")]
        {
            wait_for_proc_exit(startup_pid);
            wait_for_proc_exit(descendant_pid);
        }
    }

    #[test]
    #[cfg(unix)]
    fn blocked_cancel_notification_is_bounded_by_cancel_grace() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_cancel_write");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_blockedcancelwrite1";
        let started = manager.handle(
            start_request_with_timeout(&manager, &root, run, BTreeMap::new(), 30),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("prompt_read.ready"));
        wait_for_snapshot(&manager, run, |snapshot| {
            snapshot.execution_state == CodingAgentExecutionState::Started
        });
        assert!(
            notification_frame("session/cancel", json!({"sessionId":"s".repeat(70_000)}),)
                .unwrap()
                .len()
                > 64 * 1024,
            "cancel backpressure fixture must exceed the measured special Linux pipe capacity"
        );

        let cancel_started = Instant::now();
        let cancelled = manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        assert!(
            cancel_started.elapsed() < Duration::from_secs(1),
            "Cancel waited on the later session/cancel write"
        );
        let cancel_snapshot = match cancelled.payload.unwrap() {
            CodingAgentResponsePayload::Cancel { run } => run,
            other => panic!("unexpected cancel payload: {other:?}"),
        };
        assert_ne!(
            cancel_snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        let terminal_started = Instant::now();
        let drain = manager.drain_workers_until(
            Instant::now() + ACP_CANCEL_GRACE + ACP_IO_CLEANUP_TIMEOUT + Duration::from_secs(1),
        );
        assert_eq!(drain.timed_out, 0);
        assert_eq!(drain.panicked, 0);
        let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert!(
            terminal_started.elapsed()
                < ACP_CANCEL_GRACE + ACP_IO_CLEANUP_TIMEOUT + Duration::from_secs(1),
            "blocked session/cancel escaped ACP_CANCEL_GRACE plus cleanup bound"
        );
        assert_eq!(snapshot.state, CodingAgentRunState::Lost);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
        assert_eq!(
            snapshot
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.error_code.as_deref()),
            Some("coding_agent_cancel_write_uncertain")
        );
        let log = wire_log(&temp);
        let startup_pid = log
            .iter()
            .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
            .unwrap();
        let descendant_pid = log
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();
        assert_eq!(manager.worker_count(), 0);
        #[cfg(target_os = "linux")]
        {
            wait_for_proc_exit(startup_pid);
            wait_for_proc_exit(descendant_pid);
        }
    }

    #[test]
    #[cfg(unix)]
    fn cancel_during_initialize_never_dispatches_prompt() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_initialize");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_cancelinitialize01";
        let started = manager.handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("initialize.ready"));

        let cancelled = manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: run.to_string(),
            }),
            &projects,
        );
        let snapshot = match cancelled.payload.unwrap() {
            CodingAgentResponsePayload::Cancel { run } => run,
            other => panic!("unexpected cancel payload: {other:?}"),
        };
        assert_eq!(snapshot.state, CodingAgentRunState::Cancelled);
        assert_eq!(
            snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            snapshot
                .terminal
                .as_ref()
                .and_then(|t| t.stop_reason.as_deref()),
            None
        );
        fs::write(temp.path().join("initialize.release"), b"release").unwrap();
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        assert_eq!(drain.panicked, 0);
        let methods = received_methods(&wire_log(&temp));
        assert_eq!(
            methods
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
        assert_eq!(
            methods
                .iter()
                .filter(|m| m.as_str() == "session/cancel")
                .count(),
            0
        );
        let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
        assert_eq!(
            final_snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
    }

    #[test]
    #[cfg(unix)]
    fn cancel_and_prompt_gate_race_has_only_linearized_outcomes() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "wait_cancel");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_promptgaterace01";
        let race = Arc::new(std::sync::Barrier::new(2));
        *manager.prompt_dispatch_test_barrier.lock().unwrap() = Some(Arc::clone(&race));
        let started = manager.handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);

        let cancel_manager = Arc::clone(&manager);
        let cancel_projects = projects.clone();
        let cancel_race = Arc::clone(&race);
        let cancel = thread::spawn(move || {
            cancel_race.wait();
            cancel_manager.handle(
                CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                    run_id: run.to_string(),
                }),
                &cancel_projects,
            )
        });
        let cancelled = cancel.join().unwrap();
        assert!(cancelled.error.is_none(), "{:?}", cancelled.error);
        *manager.prompt_dispatch_test_barrier.lock().unwrap() = None;
        let final_snapshot = wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        let methods = received_methods(&wire_log(&temp));
        let prompts = methods
            .iter()
            .filter(|m| m.as_str() == "session/prompt")
            .count();
        let cancels = methods
            .iter()
            .filter(|m| m.as_str() == "session/cancel")
            .count();
        assert!(
            prompts <= 1,
            "prompt dispatched more than once: {methods:?}"
        );
        match prompts {
            0 => {
                assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
                assert_eq!(
                    final_snapshot.execution_state,
                    CodingAgentExecutionState::NotStarted
                );
                assert_eq!(cancels, 0);
            }
            1 => {
                assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
                assert_eq!(
                    final_snapshot.execution_state,
                    CodingAgentExecutionState::Completed
                );
                assert_eq!(cancels, 1);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    #[cfg(unix)]
    fn shutdown_during_admission_catches_published_run_before_prompt() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "wait_cancel");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_shutdownadmission01";
        let publish_barrier = Arc::new(std::sync::Barrier::new(2));
        *manager
            .admission_after_accepting_test_barrier
            .lock()
            .unwrap() = Some(Arc::clone(&publish_barrier));
        manager
            .admission_after_accepting_test_reached
            .store(false, Ordering::SeqCst);

        let start_manager = Arc::clone(&manager);
        let start_projects = projects.clone();
        let request = start_request(&manager, &root, run, BTreeMap::new());
        let start = thread::spawn(move || start_manager.handle(request, &start_projects));
        let reached_deadline = Instant::now() + Duration::from_secs(5);
        while !manager
            .admission_after_accepting_test_reached
            .load(Ordering::SeqCst)
        {
            assert!(Instant::now() < reached_deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let shutdown_manager = Arc::clone(&manager);
        let shutdown = thread::spawn(move || shutdown_manager.stop_accepting());
        let stopping_deadline = Instant::now() + Duration::from_secs(5);
        while manager.accepting.load(Ordering::Acquire) {
            assert!(Instant::now() < stopping_deadline);
            thread::sleep(Duration::from_millis(5));
        }
        publish_barrier.wait();
        let started = start.join().unwrap();
        assert!(started.error.is_none(), "{:?}", started.error);
        shutdown.join().unwrap();
        *manager
            .admission_after_accepting_test_barrier
            .lock()
            .unwrap() = None;
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert_eq!(drain.timed_out, 0);
        let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
        assert_eq!(
            final_snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
    }

    #[test]
    #[cfg(unix)]
    fn shutdown_drains_setup_worker_and_reaps_provider_tree() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "block_initialize_tree");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_shutdowndrain01";
        let started = manager.handle(
            start_request(&manager, &root, run, BTreeMap::new()),
            &projects,
        );
        assert!(started.error.is_none(), "{:?}", started.error);
        wait_for_path(&temp.path().join("initialize.ready"));
        assert!(manager.worker_count() > 0);
        let log = wire_log(&temp);
        let startup_pid = log
            .iter()
            .find_map(|entry| entry.get("startup_pid").and_then(Value::as_u64))
            .unwrap();
        let descendant_pid = log
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();

        manager.stop_accepting();
        fs::write(temp.path().join("initialize.release"), b"release").unwrap();
        let drain = manager.drain_workers_until(Instant::now() + Duration::from_secs(3));
        assert!(drain.resources > 0);
        assert_eq!(drain.timed_out, 0);
        assert_eq!(drain.panicked, 0);
        assert_eq!(manager.worker_count(), 0);
        let final_snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
        assert_eq!(final_snapshot.state, CodingAgentRunState::Cancelled);
        assert_eq!(
            final_snapshot.execution_state,
            CodingAgentExecutionState::NotStarted
        );
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|m| m.as_str() == "session/prompt")
                .count(),
            0
        );
        #[cfg(target_os = "linux")]
        {
            wait_for_proc_exit(startup_pid);
            wait_for_proc_exit(descendant_pid);
        }
    }

    #[test]
    #[cfg(unix)]
    fn capacity_stale_provider_and_replay_are_fenced_before_duplicate_prompt() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "wait_cancel");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let first_run = "wc_agent_run_capacity0001";
        assert!(manager
            .handle(
                start_request(&manager, &root, first_run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        wait_for_snapshot(&manager, first_run, |snapshot| {
            snapshot.state == CodingAgentRunState::Running
        });
        let second = manager.handle(
            start_request(
                &manager,
                &root,
                "wc_agent_run_capacity0002",
                BTreeMap::new(),
            ),
            &projects,
        );
        assert_eq!(
            second.error.as_ref().map(|error| error.code.as_str()),
            Some("coding_agent_capacity_full")
        );
        let mut stale = start_request(&manager, &root, "wc_agent_run_stale000001", BTreeMap::new());
        if let CodingAgentRequest::Start(request) = &mut stale {
            request.provider_instance_id = "replaced-provider".to_string();
        }
        let stale = manager.handle(stale, &projects);
        assert_eq!(
            stale.error.as_ref().map(|error| error.code.as_str()),
            Some("stale_coding_agent_provider")
        );
        manager.handle(
            CodingAgentRequest::Cancel(CodingAgentCancelRequest {
                run_id: first_run.to_string(),
            }),
            &projects,
        );
        wait_for_snapshot(&manager, first_run, |snapshot| snapshot.state.terminal());

        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_replay000001";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        assert!(manager.handle(request.clone(), &projects).error.is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert!(manager.handle(request, &projects).error.is_none());
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1
        );
        let mut conflict = start_request(&manager, &root, run, BTreeMap::new());
        if let CodingAgentRequest::Start(request) = &mut conflict {
            request.intent_fingerprint = "different-fingerprint".to_string();
        }
        assert_eq!(
            manager
                .handle(conflict, &projects)
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("idempotency_conflict")
        );
    }

    #[test]
    #[cfg(unix)]
    fn corrupt_durable_record_after_possible_dispatch_fails_closed_without_redispatch() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_corruptdurable01";
        let request = start_request(&manager, &root, run, BTreeMap::new());
        assert!(manager.handle(request.clone(), &projects).error.is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1
        );

        manager.runs.lock().unwrap().remove(run);
        fs::write(manager.store.state_path(run), b"{corrupt-json").unwrap();
        let corrupt = manager.handle(request.clone(), &projects);
        assert_eq!(
            corrupt.dispatch_state,
            CodingAgentDispatchState::OutcomeUnknown
        );
        assert_eq!(
            corrupt.error.as_ref().map(|error| error.code.as_str()),
            Some("coding_agent_durable_state_unavailable")
        );

        fs::remove_file(manager.store.state_path(run)).unwrap();
        let missing = manager.handle(request.clone(), &projects);
        assert_eq!(
            missing.dispatch_state,
            CodingAgentDispatchState::OutcomeUnknown
        );
        assert_eq!(
            missing.error.as_ref().map(|error| error.code.as_str()),
            Some("coding_agent_durable_state_unavailable")
        );
        let store_root = manager.store.root.clone();
        drop(manager);
        let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
        let after_restart = restarted.handle(
            start_request(&restarted, &root, run, BTreeMap::new()),
            &projects,
        );
        assert_eq!(
            after_restart.dispatch_state,
            CodingAgentDispatchState::OutcomeUnknown
        );
        assert_eq!(
            after_restart
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("coding_agent_durable_state_unavailable")
        );
        assert_eq!(
            received_methods(&wire_log(&temp))
                .iter()
                .filter(|method| method.as_str() == "session/prompt")
                .count(),
            1,
            "missing or corrupt durable state after a possible prompt must never become retry authority"
        );
    }

    #[test]
    fn durable_store_replaces_existing_state_cross_platform() {
        let temp = TempDir::new().unwrap();
        let store = DurableRunStore::new(temp.path().join("store"));
        let timestamp = now();
        let mut record = DurableRunRecord {
            schema_version: STORE_SCHEMA_VERSION,
            run_id: "wc_agent_run_replace_state01".to_string(),
            intent_fingerprint: "fingerprint".to_string(),
            authority_fingerprint: "auth_replace".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            provider_id: "codex".to_string(),
            provider_instance_id: "acp_replace".to_string(),
            state: CodingAgentRunState::Starting,
            execution_state: CodingAgentExecutionState::NotStarted,
            dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
            created_at: timestamp,
            updated_at: timestamp,
            terminal: None,
        };
        store.write(&record).unwrap();
        record.state = CodingAgentRunState::Running;
        record.execution_state = CodingAgentExecutionState::OutcomeUnknown;
        record.dispatch_phase = DurableDispatchPhase::PromptDispatchMayHaveOccurred;
        record.updated_at = timestamp.saturating_add(1);
        store.write(&record).unwrap();
        let restored = store.read(&record.run_id).unwrap().unwrap();
        assert_eq!(
            restored.dispatch_phase,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred
        );
        assert_eq!(
            restored.execution_state,
            CodingAgentExecutionState::OutcomeUnknown
        );
    }

    #[test]
    fn project_binding_requires_current_writable_registration() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        let other = temp.path().join("other");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&other).unwrap();
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        let config_path = projects.join("p.toml");
        let write_registration = |allow_patch: bool, disabled: bool, path: &Path| {
            fs::write(
                &config_path,
                format!(
                    "id = \"demo\"\npath = {:?}\nallow_patch = {allow_patch}\ndisabled = {disabled}\n",
                    path.to_string_lossy()
                ),
            )
            .unwrap();
        };

        write_registration(false, false, &root);
        assert!(!project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            root.to_string_lossy().as_ref()
        ));

        write_registration(true, true, &root);
        assert!(!project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            root.to_string_lossy().as_ref()
        ));

        write_registration(true, false, &root);
        assert!(project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            root.to_string_lossy().as_ref()
        ));
        assert!(!project_binding_matches(
            &projects,
            "test",
            "agent:test:wrong",
            root.to_string_lossy().as_ref()
        ));
        assert!(!project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            other.to_string_lossy().as_ref()
        ));
    }

    #[cfg(unix)]
    #[test]
    fn project_binding_accepts_canonical_alias_and_rejects_symlink_retarget() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let alias = temp.path().join("repo-alias");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        symlink(&first, &alias).unwrap();
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        fs::write(
            projects.join("p.toml"),
            format!(
                "id = \"demo\"\npath = {:?}\nallow_patch = true\n",
                alias.to_string_lossy()
            ),
        )
        .unwrap();
        let canonical_first = canonicalize_existing(&first).unwrap();
        assert!(project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            canonical_first.to_string_lossy().as_ref()
        ));

        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();
        assert!(!project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            canonical_first.to_string_lossy().as_ref()
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn project_binding_accepts_var_private_var_alias() {
        assert_eq!(
            canonicalize_existing(Path::new("/var")).unwrap(),
            canonicalize_existing(Path::new("/private/var")).unwrap()
        );
        let temp = TempDir::new().unwrap();
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        fs::write(
            projects.join("p.toml"),
            "id = \"demo\"\npath = \"/private/var\"\nallow_patch = true\n",
        )
        .unwrap();
        assert!(project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            "/var"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn project_binding_accepts_windows_case_and_verbatim_disk_identity() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("RepoCase");
        fs::create_dir_all(&root).unwrap();
        let canonical = canonicalize_existing(&root).unwrap();
        let canonical_text = canonical.to_string_lossy().to_string();
        let plain = canonical_text
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical_text)
            .to_string();
        let case_variant = plain.to_ascii_uppercase();
        let verbatim = format!(r"\\?\{plain}");
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        fs::write(
            projects.join("p.toml"),
            format!("id = \"demo\"\npath = {:?}\nallow_patch = true\n", plain),
        )
        .unwrap();
        assert!(project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            &case_variant
        ));
        assert!(project_binding_matches(
            &projects,
            "test",
            "agent:test:demo",
            &verbatim
        ));
    }

    #[test]
    #[cfg(unix)]
    fn child_environment_is_cleared_and_missing_mapping_never_spawns() {
        let _guard = crate::tests::test_env_lock();
        let _env = crate::tests::EnvGuard::new()
            .set("WEBCODEX_TEST_ACP_VISIBLE", "visible-value")
            .set("WEBCODEX_TEST_ACP_HIDDEN", "must-not-reach-child");
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let mut cfg = fake_config(exe, args);
        cfg.agents[0].env_from_env = BTreeMap::from([(
            "ACP_VISIBLE".to_string(),
            "WEBCODEX_TEST_ACP_VISIBLE".to_string(),
        )]);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_envclear0001";
        assert!(manager
            .handle(
                start_request(&manager, &root, run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        let startup = wire_log(&temp)
            .into_iter()
            .find(|entry| entry.get("env_keys").is_some())
            .unwrap();
        assert_eq!(startup["env_keys"], json!(["ACP_VISIBLE"]));

        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let mut cfg = fake_config(exe, args);
        cfg.agents[0].env_from_env = BTreeMap::from([(
            "ACP_VISIBLE".to_string(),
            "WEBCODEX_TEST_ACP_MISSING".to_string(),
        )]);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let response = manager.handle(
            start_request(
                &manager,
                &root,
                "wc_agent_run_missingenv01",
                BTreeMap::new(),
            ),
            &projects,
        );
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("coding_agent_environment_unavailable")
        );
        assert!(
            wire_log(&temp).is_empty(),
            "provider child must not start when an env source is missing"
        );
    }

    #[test]
    #[cfg(unix)]
    fn pre_barrier_restart_is_not_started_and_child_tree_is_reaped() {
        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "end");
        let cfg = fake_config(exe, args);
        let store_root = temp.path().join("store");
        fs::create_dir_all(&store_root).unwrap();
        let initial = CodingAgentManager::with_store(&cfg, store_root.clone()).unwrap();
        let provider = initial.providers().remove(0);
        drop(initial);
        let timestamp = now();
        DurableRunStore::new(store_root.clone())
            .write(&DurableRunRecord {
                schema_version: STORE_SCHEMA_VERSION,
                run_id: "wc_agent_run_prebarrier01".to_string(),
                intent_fingerprint: "fingerprint".to_string(),
                authority_fingerprint: "auth_test".to_string(),
                runtime_project_id: "agent:test:demo".to_string(),
                provider_id: "codex".to_string(),
                provider_instance_id: provider.provider_instance_id,
                state: CodingAgentRunState::Starting,
                execution_state: CodingAgentExecutionState::NotStarted,
                dispatch_phase: DurableDispatchPhase::BeforePromptBarrier,
                created_at: timestamp,
                updated_at: timestamp,
                terminal: None,
            })
            .unwrap();
        let restarted = CodingAgentManager::with_store(&cfg, store_root).unwrap();
        let recovered = restarted
            .runs
            .lock()
            .unwrap()
            .get("wc_agent_run_prebarrier01")
            .unwrap()
            .snapshot();
        assert_eq!(recovered.state, CodingAgentRunState::Failed);
        assert_eq!(
            recovered.execution_state,
            CodingAgentExecutionState::NotStarted
        );

        let temp = TempDir::new().unwrap();
        let (exe, args) = fake_agent(&temp, "spawn_descendant");
        let cfg = fake_config(exe, args);
        let projects = project_fixture(&temp);
        let root = temp.path().join("repo");
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_reaptree0001";
        assert!(manager
            .handle(
                start_request(&manager, &root, run, BTreeMap::new()),
                &projects
            )
            .error
            .is_none());
        wait_for_snapshot(&manager, run, |snapshot| snapshot.state.terminal());
        let descendant = wire_log(&temp)
            .iter()
            .find_map(|entry| entry.get("descendant_pid").and_then(Value::as_u64))
            .unwrap();
        let proc_path = PathBuf::from(format!("/proc/{descendant}"));
        let deadline = Instant::now() + Duration::from_secs(3);
        while proc_path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !proc_path.exists(),
            "ACP descendant process survived ManagedChild cleanup"
        );
    }

    #[test]
    #[ignore = "opt-in real Codex ACP dogfood; requires local Codex auth and network"]
    #[cfg(unix)]
    fn real_codex_acp_opt_in_dogfood() {
        let temp = TempDir::new().unwrap();
        let root = std::env::current_dir().unwrap();
        let projects = temp.path().join("project-registry");
        fs::create_dir_all(&projects).unwrap();
        fs::write(
            projects.join("dogfood.toml"),
            format!("id = \"demo\"\npath = {:?}\n", root.to_string_lossy()),
        )
        .unwrap();

        let mut env_from_env = BTreeMap::new();
        // This list is dogfood/test-owned and intentionally explicit. Production
        // providers inherit nothing unless the operator declares each mapping.
        for name in [
            "HOME",
            "PATH",
            "USER",
            "SHELL",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
        ] {
            if std::env::var_os(name).is_some() {
                env_from_env.insert(name.to_string(), name.to_string());
            }
        }
        let cfg = AcpConfig {
            max_concurrent_runs: 1,
            permission_timeout_secs: 3,
            agents: vec![AcpAgentConfig {
                id: "codex".to_string(),
                name: "Codex ACP dogfood".to_string(),
                executable: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@agentclientprotocol/codex-acp".to_string(),
                ],
                env_from_env,
                allowed_config_options: Vec::new(),
            }],
        };
        let manager = CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
        let run = "wc_agent_run_realcodexdogfood01";
        let provider = manager.providers().remove(0);
        let request = CodingAgentRequest::Start(webcodex_core::coding_agent::CodingAgentStartRequest {
            run_id: run.to_string(),
            intent_fingerprint: "real-codex-dogfood-v1".to_string(),
            authority_fingerprint: "auth_real_codex_dogfood".to_string(),
            runtime_project_id: "agent:test:demo".to_string(),
            project_root: root.to_string_lossy().into_owned(),
            provider_id: "codex".to_string(),
            provider_instance_id: provider.provider_instance_id,
            instruction: "Read Cargo.toml only and reply with the WebCodex package version in one short sentence. Do not modify files, run builds, install dependencies, or request elevated permissions.".to_string(),
            config: BTreeMap::new(),
            timeout_secs: 180,
        });
        let admitted = manager.handle(request, &projects);
        assert!(
            admitted.error.is_none(),
            "admission failed: {:?}",
            admitted.error
        );
        let deadline = Instant::now() + Duration::from_secs(150);
        let terminal = loop {
            let snapshot = manager.runs.lock().unwrap().get(run).unwrap().snapshot();
            if snapshot.state.terminal() {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "real Codex ACP dogfood timed out: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(100));
        };
        let observation = manager
            .runs
            .lock()
            .unwrap()
            .get(run)
            .unwrap()
            .observe(None, 64, 0)
            .expect("retained real Run cursor must be valid");
        assert_eq!(
            terminal.state,
            CodingAgentRunState::Completed,
            "terminal={terminal:?}; events={:?}",
            observation.events
        );
        assert_eq!(
            terminal
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.stop_reason.as_deref()),
            Some("end_turn")
        );
        assert!(
            observation
                .events
                .iter()
                .any(|event| event.kind == CodingAgentEventKind::AgentMessage),
            "real Codex ACP produced no normalized agent message: {:?}",
            observation.events
        );
        assert!(
            observation.events.iter().any(|event| matches!(
                event.kind,
                CodingAgentEventKind::ToolActivity
                    | CodingAgentEventKind::TerminalActivity
                    | CodingAgentEventKind::Usage
            )),
            "real Codex ACP produced no normalized activity: {:?}",
            observation.events
        );
    }

    #[test]
    #[cfg(unix)]
    fn fake_acp_normalizes_activity_and_terminal_matrix() {
        let (manager, run, obs) = run_scenario("end", BTreeMap::new());
        assert_eq!(obs.run.state, CodingAgentRunState::Completed);
        let durable = manager.store.read(&run).unwrap().unwrap();
        assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
        assert_eq!(durable.state, CodingAgentRunState::Completed);
        assert_eq!(
            durable.execution_state,
            CodingAgentExecutionState::Completed
        );
        assert_eq!(durable.terminal, obs.run.terminal);
        assert!(obs
            .events
            .iter()
            .any(|e| e.kind == CodingAgentEventKind::AgentMessage));
        assert!(obs
            .events
            .iter()
            .any(|e| e.kind == CodingAgentEventKind::Reasoning));
        assert!(obs
            .events
            .iter()
            .any(|e| e.kind == CodingAgentEventKind::TerminalActivity));
        for scenario in [
            "cancelled",
            "max_tokens",
            "max_turn_requests",
            "refusal",
            "unknown",
        ] {
            let (manager, run, obs) = run_scenario(scenario, BTreeMap::new());
            let expected = if scenario == "cancelled" {
                CodingAgentRunState::Cancelled
            } else {
                CodingAgentRunState::Failed
            };
            assert_eq!(obs.run.state, expected);
            assert_eq!(
                obs.run.execution_state,
                CodingAgentExecutionState::Completed
            );
            let durable = manager.store.read(&run).unwrap().unwrap();
            assert_eq!(durable.dispatch_phase, DurableDispatchPhase::Terminal);
            assert_eq!(durable.state, expected);
            assert_eq!(
                durable.execution_state,
                CodingAgentExecutionState::Completed
            );
            assert_eq!(durable.terminal, obs.run.terminal);
        }
    }

    #[test]
    #[cfg(unix)]
    fn config_and_permission_paths_are_fail_closed() {
        let (_, _, obs) = run_scenario(
            "end",
            BTreeMap::from([(
                "mode".to_string(),
                CodingAgentConfigValue::String("read-only".to_string()),
            )]),
        );
        assert_eq!(obs.run.state, CodingAgentRunState::Completed);
        let (_, _, permission) = run_scenario("permission", BTreeMap::new());
        assert_eq!(permission.run.state, CodingAgentRunState::Cancelled);
        assert!(permission
            .events
            .iter()
            .any(|e| e.kind == CodingAgentEventKind::PermissionRequest));
    }

    #[test]
    #[cfg(unix)]
    fn post_barrier_crash_is_lost_and_restart_never_redispatches() {
        let (manager, run, obs) = run_scenario("crash_after_prompt", BTreeMap::new());
        assert_eq!(obs.run.state, CodingAgentRunState::Lost);
        let cfg = AcpConfig {
            max_concurrent_runs: manager.max_concurrent_runs,
            permission_timeout_secs: 1,
            agents: manager
                .providers
                .values()
                .map(|p| p.config.clone())
                .collect(),
        };
        let restarted = CodingAgentManager::with_store(&cfg, manager.store.root.clone()).unwrap();
        assert_eq!(
            restarted
                .runs
                .lock()
                .unwrap()
                .get(&run)
                .unwrap()
                .snapshot()
                .state,
            CodingAgentRunState::Lost
        );
    }

    #[test]
    #[cfg(unix)]
    fn durable_record_contains_no_prompt_or_event_bodies() {
        let (manager, run, _) = run_scenario("end", BTreeMap::new());
        let bytes = fs::read(manager.store.state_path(&run)).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("inspect"));
        assert!(!text.contains("hello"));
        assert!(!text.contains("thinking"));
    }
}
