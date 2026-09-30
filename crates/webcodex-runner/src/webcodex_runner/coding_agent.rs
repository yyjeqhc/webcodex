//! Runner Coding Agent admission and retained in-memory Run observations.
//! Protocol, setup, process I/O, and durable transitions have private owners.

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
const ACP_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
const ACP_SETUP_TIMEOUT: Duration = Duration::from_secs(30);
const ACP_CANCEL_GRACE: Duration = Duration::from_secs(5);
const ACP_POLL: Duration = Duration::from_millis(25);
const ACP_IO_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN: &str =
    "coding_agent_terminal_persistence_uncertain";
const TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE: &str = "ACP terminal transition was observed in memory, but its durable commit was not confirmed; reconcile or reobserve instead of treating the original terminal outcome as durable truth";

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
    forced_config: BTreeMap<String, CodingAgentConfigValue>,
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

fn manager_finish_setup_failure(
    manager: &Arc<CodingAgentManager>,
    run_id: &str,
    entry: &Arc<RunEntry>,
    code: &str,
    message: &str,
) {
    manager.setup_failure(run_id, entry, code, message);
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
            forced_config: config.forced_config.clone(),
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
            forced_config: config.forced_config.clone(),
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
}

mod lifecycle;
mod protocol;
mod run;
mod setup;
mod store;

use store::{DurableDispatchPhase, DurableRunRecord, DurableRunStore, STORE_SCHEMA_VERSION};

#[cfg(test)]
mod tests;
