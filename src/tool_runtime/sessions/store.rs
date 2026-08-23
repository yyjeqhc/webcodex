//! In-memory SessionStore: create sessions, record events, and trigger persistence.
//!
//! All durable session-map mutations flow through `SessionStoreInner` helpers.
//! Callers outside this module use `SessionStore` methods only.
use super::super::permissions::PermissionDecision;
use super::super::tool_inputs::SessionMode;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use super::console::{
    build_detail as build_console_detail, build_list_item as build_console_list_item,
    normalize_console_activity_limit, normalize_console_session_limit,
    WorkflowSessionConsoleDetail, WorkflowSessionConsoleList,
};
use super::events::{
    actual_failure_kind_for_tool_result, changed_paths_for_tool, classify_failure_expectation,
    context_result_summary_for_tool_result, diff_review_like_for_tool, extract_job_id,
    extract_project, is_valid_session_id, observed_input_paths_for_tool,
    observed_paths_for_successful_result, persistent_shell_event_evidence_for_tool_result,
    sanitize_tool_execution_state, session_input_summary_for_tool,
    validation_output_summary_for_tool_result, SessionToolClassification,
};
use super::model::{
    CodingSessionError, CodingSessionOutcome, CodingSessionRequest, ColdSessionRecord,
    CompleteSessionMessageInput, CompleteSessionMessageOutcome, CurrentSessionKey,
    DurableCurrentBinding, PersistedCurrentBinding, PersistedCurrentBindings,
    PersistedSessionLedger, PersistedSessionRecord, PersistedSessionSnapshot,
    PersistentShellEventEvidence, PostSessionMessageInput, RecordedModelFacingToolCall,
    SessionCloseError, SessionCloseOutcome, SessionContextRevisionAck, SessionCounts,
    SessionCreateOptions, SessionEvent, SessionExecutionContext,
    SessionExecutionContextUpdateError, SessionExecutionContextUpdateOutcome, SessionGuardDenial,
    SessionGuards, SessionLifecycle, SessionLifecycleDenial, SessionMessage, SessionMessageError,
    SessionMessageStatus, SessionRecord, SessionStoreStatus, SessionSummary, SessionTransport,
    StoredSession, ToolCallRecorderMetadata, ToolCallStart, ToolEffectEventEvidence,
    CALL_ID_PREFIX, DEFAULT_MAX_EVENTS_PER_SESSION, DEFAULT_MAX_MESSAGES_PER_SESSION,
    DEFAULT_MAX_SESSIONS, DEFAULT_SUMMARY_LIMIT, DURABLE_CURRENT_BINDINGS_PER_SESSION,
    EVENT_ID_PREFIX, MAX_CODING_INSTRUCTION_CHARS, MAX_MATERIALIZED_VALIDATION_JOB_IDS,
    MAX_SUMMARY_LIMIT, MESSAGE_ID_PREFIX, SESSION_ID_PREFIX, SESSION_LEDGER_VERSION,
};
use super::persistence::{
    cold_session_from_persisted, load_persisted_ledger, materialize_cold_session,
    write_ledger_atomic,
};
use super::query::{
    build_messages_summary, is_valid_completion_id, validate_message_tags, validate_message_text,
    validate_resolution_text,
};
use super::util::{
    bound_event_error_summary, bound_summary_string, now_ts, redact_and_bound_instruction,
    redact_and_bound_value,
};

#[derive(Debug, Clone)]
pub(crate) struct SessionStore {
    /// Shared session map, bindings, and LRU metadata.
    /// `pub(super)` so sibling modules can lock and call `SessionStoreInner`
    /// transition helpers without touching the maps directly.
    pub(super) inner: Arc<Mutex<SessionStoreInner>>,
    persistence_write_mutex: Arc<Mutex<()>>,
    /// Background ledger writer for persistent stores. `None` for in-memory
    /// stores or when the writer thread could not be spawned (mutations then
    /// fall back to the synchronous write path).
    writer: Option<Arc<LedgerWriterGuard>>,
    /// Process-local wake signal only; durable observation truth remains in the
    /// persisted per-Session revision bookkeeping.
    pub(super) message_observation_notify: tokio::sync::watch::Sender<u64>,
    #[cfg(test)]
    fail_next_coding_continuity_precommit: Arc<std::sync::atomic::AtomicBool>,
}

/// Coordinates a dedicated OS thread that owns session-ledger serialize +
/// atomic disk write. Callers only mark dirty (or flush); they never block
/// async Tokio workers on full-store JSON + `fs::write`.
///
/// Why this exists: every `push_event` used to call `persist_after_mutation`
/// synchronously on the request path, holding a global write mutex while
/// cloning/serializing up to max_sessions×max_events and renaming on disk.
/// Under concurrent MCP tools/call traffic that saturates the async runtime
/// and surfaces as intermittent "no reply" hangs.
struct LedgerWriterGuard {
    shared: Arc<LedgerWriterShared>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

struct LedgerWriterShared {
    state: Mutex<LedgerWriterState>,
    cvar: Condvar,
}

struct LedgerWriterState {
    /// Set by mutation paths; cleared when the writer begins a snapshot.
    dirty: bool,
    /// Monotonic counter advanced every time `dirty` is set. Flush waiters
    /// wait until `writes_completed` reaches the generation they observed.
    dirty_generation: u64,
    /// Generation of the last completed write cycle.
    writes_completed: u64,
    shutdown: bool,
}

impl std::fmt::Debug for LedgerWriterGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerWriterGuard").finish_non_exhaustive()
    }
}

impl LedgerWriterGuard {
    fn spawn(
        store_inner: Arc<Mutex<SessionStoreInner>>,
        write_mutex: Arc<Mutex<()>>,
    ) -> Option<Arc<Self>> {
        let shared = Arc::new(LedgerWriterShared {
            state: Mutex::new(LedgerWriterState {
                dirty: false,
                dirty_generation: 0,
                writes_completed: 0,
                shutdown: false,
            }),
            cvar: Condvar::new(),
        });
        let shared_thread = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("session-ledger-writer".to_string())
            .spawn(move || ledger_writer_loop(shared_thread, store_inner, write_mutex))
            .ok()?;
        Some(Arc::new(Self {
            shared,
            join: Mutex::new(Some(join)),
        }))
    }

    fn mark_dirty(&self) -> u64 {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned");
        state.dirty = true;
        state.dirty_generation = state.dirty_generation.saturating_add(1);
        let generation = state.dirty_generation;
        self.shared.cvar.notify_one();
        generation
    }

    /// Block until the exact generation requested by the caller has been
    /// written. Later concurrent dirty marks do not extend this fence.
    fn flush_through(&self, generation: u64) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned");
        while state.writes_completed < generation {
            state = self
                .shared
                .cvar
                .wait(state)
                .expect("session ledger writer state poisoned");
        }
    }

    /// Test/closeout barrier for every dirty mark observed at call time.
    #[cfg(test)]
    fn flush(&self) {
        let generation = self
            .shared
            .state
            .lock()
            .expect("session ledger writer state poisoned")
            .dirty_generation;
        self.flush_through(generation);
    }
}

impl Drop for LedgerWriterGuard {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            // Keep dirty as-is so the loop performs one final write before exit.
            state.shutdown = true;
            self.shared.cvar.notify_one();
        }
        if let Some(join) = self
            .join
            .lock()
            .expect("session ledger writer join mutex poisoned")
            .take()
        {
            let _ = join.join();
        }
    }
}

fn ledger_writer_loop(
    shared: Arc<LedgerWriterShared>,
    store_inner: Arc<Mutex<SessionStoreInner>>,
    write_mutex: Arc<Mutex<()>>,
) {
    loop {
        let generation = {
            let mut state = shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            while !state.dirty && !state.shutdown {
                state = shared
                    .cvar
                    .wait(state)
                    .expect("session ledger writer state poisoned");
            }
            if !state.dirty {
                // shutdown with nothing pending
                break;
            }
            let generation = state.dirty_generation;
            state.dirty = false;
            generation
        };

        // Snapshot + write under the same write mutex used by the synchronous
        // test hook (`persist_after_mutation_with`), so a custom delayed write
        // cannot race an older snapshot past a newer background write without
        // the lock ordering the two.
        let _write_guard = write_mutex
            .lock()
            .expect("session persistence mutex poisoned");
        let snapshot = {
            let inner = store_inner.lock().expect("session store mutex poisoned");
            let path = inner
                .persistence
                .as_ref()
                .map(|persistence| persistence.path.clone());
            path.map(|path| (path, inner.to_persisted_ledger()))
        };
        let result = match snapshot {
            Some((path, ledger)) => write_ledger_atomic(&path, &ledger).map_err(|err| {
                bound_summary_string(&format!("persist_failed: {}: {err}", path.display()))
            }),
            None => Ok(()),
        };
        {
            let mut inner = store_inner.lock().expect("session store mutex poisoned");
            if let Some(persistence) = inner.persistence.as_mut() {
                match &result {
                    Ok(()) => persistence.last_persist_error = None,
                    Err(error) => {
                        tracing::warn!("session ledger persistence failed: {}", error);
                        persistence.last_persist_error = Some(error.clone());
                    }
                }
            }
        }
        {
            let mut state = shared
                .state
                .lock()
                .expect("session ledger writer state poisoned");
            state.writes_completed = generation;
            // Wake flush waiters; if dirty was re-set during the write the
            // loop body runs again without waiting.
            shared.cvar.notify_all();
            if state.shutdown && !state.dirty {
                break;
            }
        }
        // Drop write_guard at end of iteration so a concurrent
        // persist_after_mutation_with can interleave between cycles.
        drop(_write_guard);
    }
}

#[derive(Debug)]
pub(super) struct SessionStoreInner {
    /// Durable workflow sessions. Mutated only via the helpers below.
    sessions: HashMap<String, StoredSession>,
    /// Fast process-local cache of the exact binding tuple.
    current_sessions: HashMap<CurrentSessionKey, String>,
    /// Durable projection keyed only by the domain-separated SHA-256 of the
    /// complete CurrentSessionKey. Raw tuple components never enter the ledger.
    durable_current_bindings: HashMap<String, DurableCurrentBinding>,
    lru: VecDeque<String>,
    max_sessions: usize,
    max_durable_bindings: usize,
    max_events_per_session: usize,
    restored_binding_count: usize,
    discarded_binding_count: usize,
    persistence: Option<SessionPersistence>,
}

#[derive(Debug, Clone)]
struct SessionPersistence {
    path: PathBuf,
    restored_sessions: usize,
    last_persist_error: Option<String>,
}

fn max_durable_bindings(max_sessions: usize) -> usize {
    max_sessions
        .saturating_mul(DURABLE_CURRENT_BINDINGS_PER_SESSION)
        .max(1)
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS, DEFAULT_MAX_EVENTS_PER_SESSION)
    }
}

impl SessionStore {
    pub(crate) fn new(max_sessions: usize, max_events_per_session: usize) -> Self {
        Self::new_in_memory(max_sessions, max_events_per_session)
    }

    pub(crate) fn new_in_memory(max_sessions: usize, max_events_per_session: usize) -> Self {
        let max_durable_bindings = max_durable_bindings(max_sessions);
        let (message_observation_notify, _) = tokio::sync::watch::channel(0_u64);
        Self {
            inner: Arc::new(Mutex::new(SessionStoreInner {
                sessions: HashMap::<String, StoredSession>::new(),
                current_sessions: HashMap::new(),
                durable_current_bindings: HashMap::new(),
                lru: VecDeque::new(),
                max_sessions,
                max_durable_bindings,
                max_events_per_session,
                restored_binding_count: 0,
                discarded_binding_count: 0,
                persistence: None,
            })),
            persistence_write_mutex: Arc::new(Mutex::new(())),
            writer: None,
            message_observation_notify,
            #[cfg(test)]
            fail_next_coding_continuity_precommit: Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
        }
    }

    pub(crate) fn with_persistence(
        path: impl Into<PathBuf>,
        max_sessions: usize,
        max_events_per_session: usize,
    ) -> Self {
        let path = path.into();
        let max_durable_bindings = max_durable_bindings(max_sessions);
        let restored = load_persisted_ledger(
            &path,
            max_sessions,
            max_events_per_session,
            max_durable_bindings,
        );
        let inner = Arc::new(Mutex::new(SessionStoreInner {
            sessions: restored.sessions,
            current_sessions: HashMap::new(),
            durable_current_bindings: restored.durable_current_bindings,
            lru: restored.lru,
            max_sessions,
            max_durable_bindings,
            max_events_per_session,
            restored_binding_count: restored.restored_binding_count,
            discarded_binding_count: restored.discarded_binding_count,
            persistence: Some(SessionPersistence {
                path,
                restored_sessions: restored.restored_sessions,
                last_persist_error: restored.last_persist_error,
            }),
        }));
        let persistence_write_mutex = Arc::new(Mutex::new(()));
        // Prefer the background writer so mutation paths never park a Tokio
        // worker on full-ledger serialize + disk I/O. If the OS thread cannot
        // be spawned, fall back to the synchronous write path.
        let writer =
            LedgerWriterGuard::spawn(Arc::clone(&inner), Arc::clone(&persistence_write_mutex));
        let (message_observation_notify, _) = tokio::sync::watch::channel(0_u64);
        Self {
            inner,
            persistence_write_mutex,
            writer,
            message_observation_notify,
            #[cfg(test)]
            fail_next_coding_continuity_precommit: Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
        }
    }

    /// Block until every pending ledger mutation has been written to disk.
    /// No-op for in-memory stores. Required before re-opening the ledger file
    /// from another `SessionStore` (background writes are otherwise deferred).
    #[cfg(test)]
    pub(crate) fn flush_persistence(&self) {
        if let Some(writer) = &self.writer {
            writer.flush();
        }
    }

    pub(crate) fn status(&self) -> SessionStoreStatus {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        let (persistence, restored_sessions, last_persist_error) = match &inner.persistence {
            Some(persistence) => (
                "enabled".to_string(),
                persistence.restored_sessions,
                persistence.last_persist_error.clone(),
            ),
            None => ("disabled".to_string(), 0, None),
        };
        SessionStoreStatus {
            persistence,
            restored_sessions,
            durable_binding_count: inner.durable_current_bindings.len(),
            restored_binding_count: inner.restored_binding_count,
            discarded_binding_count: inner.discarded_binding_count,
            max_durable_bindings: inner.max_durable_bindings,
            max_sessions: inner.max_sessions,
            max_events_per_session: inner.max_events_per_session,
            max_messages_per_session: DEFAULT_MAX_MESSAGES_PER_SESSION,
            last_persist_error,
        }
    }

    #[cfg(test)]
    pub(crate) fn active_session_count_for_test(&self, project: Option<&str>) -> usize {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner
            .sessions
            .values()
            .filter(|record| {
                record.lifecycle() == SessionLifecycle::Active
                    && project.is_none_or(|project| record.project() == Some(project))
            })
            .count()
    }

    #[cfg(test)]
    pub(crate) fn process_local_binding_count_for_test(&self) -> usize {
        self.inner
            .lock()
            .expect("session store mutex poisoned")
            .current_sessions
            .len()
    }

    #[cfg(test)]
    pub(crate) fn insert_process_local_binding_only_for_test(
        &self,
        key: CurrentSessionKey,
        session_id: &str,
    ) {
        self.inner
            .lock()
            .expect("session store mutex poisoned")
            .current_sessions
            .insert(key, session_id.to_string());
    }

    #[cfg(test)]
    pub(crate) fn hot_payload_entry_count_for_test(&self, session_id: &str) -> Option<usize> {
        self.inner
            .lock()
            .expect("session store mutex poisoned")
            .sessions
            .get(session_id)?
            .hot()
            .map(|record| record.events.len() + record.messages.len())
    }

    #[cfg(test)]
    pub(crate) fn cold_payload_bytes_for_test(&self, session_id: &str) -> Option<usize> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        match inner.sessions.get(session_id)? {
            StoredSession::Hot(_) => None,
            StoredSession::Cold(record) => Some(record.raw.get().len()),
        }
    }

    /// Thin convenience wrapper — creation always goes through
    /// [`Self::start_session_with_options`].
    #[cfg(test)]
    pub(crate) fn start_session(
        &self,
        project: Option<String>,
        title: Option<String>,
    ) -> SessionSummary {
        self.start_session_with_guards(
            project,
            title,
            SessionMode::Normal,
            SessionGuards::default(),
        )
    }

    /// Thin convenience wrapper — creation always goes through
    /// [`Self::start_session_with_options`].
    #[cfg(test)]
    pub(crate) fn start_session_with_guards(
        &self,
        project: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        guards: SessionGuards,
    ) -> SessionSummary {
        self.start_session_with_options(SessionCreateOptions::new(project, title, mode, guards))
            .expect("default Session execution context must be valid")
    }

    /// Sole create entry point for workflow sessions.
    ///
    /// Stores session-creation inputs (including project instructions) on the
    /// `SessionRecord`. Convenience wrappers above all delegate here.
    pub(crate) fn start_session_with_options(
        &self,
        mut opts: SessionCreateOptions,
    ) -> Result<SessionSummary, String> {
        opts.execution_context = opts.execution_context.validated()?;
        #[cfg(test)]
        if opts.project.is_some() && opts.owner_authority_fingerprint.is_none() {
            // cfg(test)-only callers often build a synthetic project Session
            // directly, without an AuthContext. Mark those fixtures explicitly;
            // real runtime creation supplies a canonical fingerprint before this
            // point, so production missing-fingerprint records still fail closed.
            opts.owner_authority_fingerprint =
                Some(super::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT.to_string());
        }
        if opts.project.is_none() && !opts.execution_context.is_empty() {
            return Err(
                "execution_context requires a Workflow Session bound to a registered project"
                    .to_string(),
            );
        }
        let session_id = format!("{SESSION_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let now = now_ts();
        let guards = SessionGuards::effective(opts.mode, opts.guards);
        let owner_authority_fingerprint = opts.owner_authority_fingerprint;
        let record = SessionRecord {
            session_id: session_id.clone(),
            project: opts.project,
            owner_authority_fingerprint,
            title: opts.title,
            mode: opts.mode,
            guards,
            execution_context: opts.execution_context,
            // Create always yields Active; only explicit close transitions later.
            lifecycle: SessionLifecycle::Active,
            created_at: now,
            updated_at: now,
            messages: VecDeque::new(),
            events: VecDeque::new(),
            events_observed: 0,
            context_revision: 0,
            materialized_validation_job_ids: VecDeque::new(),
            message_observation_revision: 0,
            message_observation_floor: 0,
            message_observation_revisions: Default::default(),
            project_instructions: opts.project_instructions,
        };
        let summary = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.insert_session(record)
        };
        self.persist_after_mutation();
        Ok(summary)
    }

    /// Commit the coding context, accepted instruction, and binding together
    /// under the in-memory store lock. Persistent stores then queue the updated
    /// JSON ledger to the background writer; success does not imply disk flush.
    ///
    /// Every fallible check happens before the in-memory commit. Once mutation
    /// begins, session creation or capability update, the instruction event,
    /// process-local cache, and durable exact binding are applied under the
    /// same store lock and enter one persistence generation.
    pub(crate) fn ensure_coding_session(
        &self,
        request: CodingSessionRequest,
    ) -> Result<CodingSessionOutcome, CodingSessionError> {
        if request.resume_session_id.is_some() && request.new_session {
            return Err(CodingSessionError::ResumeNewSessionConflict);
        }
        let explicit_resume_session_id = match request.resume_session_id.as_deref() {
            Some(session_id)
                if session_id != session_id.trim() || !is_valid_session_id(session_id) =>
            {
                return Err(CodingSessionError::InvalidResumeSessionId);
            }
            Some(session_id) => Some(session_id.to_string()),
            None => None,
        };
        let explicit_resume = explicit_resume_session_id.is_some();
        let now = now_ts();
        let new_session_id = format!("{SESSION_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let new_event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let requested_guards = SessionGuards::effective(request.mode, request.guards);
        let requested_execution_context = request
            .execution_context
            .clone()
            .map(SessionExecutionContext::validated)
            .transpose()
            .map_err(CodingSessionError::InvalidExecutionContext)?;
        let instruction = request
            .instruction
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| redact_and_bound_instruction(value, MAX_CODING_INSTRUCTION_CHARS));

        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let reusable_session_id = if let Some(session_id) = explicit_resume_session_id {
                let Some(stored) = inner.sessions.get(&session_id) else {
                    return Err(CodingSessionError::UnknownResumeSession { session_id });
                };
                let lifecycle = stored.lifecycle();
                if !lifecycle.allows_mutation() {
                    return Err(CodingSessionError::ResumeSessionNotActive {
                        session_id,
                        lifecycle,
                    });
                }
                if stored.project() != Some(request.project.as_str()) {
                    return Err(CodingSessionError::ResumeProjectMismatch {
                        session_id,
                        session_project: stored.project().map(str::to_string),
                        request_project: request.project.clone(),
                    });
                }
                Some(session_id)
            } else if request.bind_current && !request.new_session {
                request.key.as_ref().and_then(|key| {
                    inner.reusable_current_session_id(key, request.project.as_str())
                })
            } else {
                None
            };

            let legacy_authority_upgrade = if let (Some(session_id), Some(authority_fingerprint)) = (
                reusable_session_id.as_deref(),
                request.authority_fingerprint.as_deref(),
            ) {
                let stored_fingerprint = inner
                    .sessions
                    .get(session_id)
                    .and_then(StoredSession::owner_authority_fingerprint);
                #[cfg(test)]
                let synthetic_test_fixture = stored_fingerprint
                    == Some(super::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT);
                #[cfg(not(test))]
                let synthetic_test_fixture = false;
                if synthetic_test_fixture {
                    false
                } else if let Some(stored_fingerprint) = stored_fingerprint {
                    if stored_fingerprint != authority_fingerprint {
                        return Err(CodingSessionError::ResumeAuthorityMismatch {
                            session_id: session_id.to_string(),
                        });
                    }
                    false
                } else {
                    let Some(key) = request.key.as_ref() else {
                        return Err(CodingSessionError::LegacySessionAuthorityUnverifiable {
                            session_id: session_id.to_string(),
                        });
                    };
                    if !inner.legacy_project_session_authority_upgrade_proof(
                        session_id,
                        key,
                        &request.project,
                    ) {
                        return Err(CodingSessionError::LegacySessionAuthorityUnverifiable {
                            session_id: session_id.to_string(),
                        });
                    }
                    true
                }
            } else {
                false
            };

            if let Some(session_id) = reusable_session_id {
                let (previous_mode, previous_guards, previous_execution_context) = {
                    let record = inner
                        .sessions
                        .get(&session_id)
                        .and_then(StoredSession::hot)
                        .expect("active reusable session must stay hot");
                    (record.mode, record.guards, record.execution_context.clone())
                };
                // Repeating a mode preserves any stricter explicit guard from
                // the root task. A real mode transition applies the newly
                // requested effective guard profile.
                let next_guards = if previous_mode == request.mode {
                    SessionGuards {
                        deny_write_tools: previous_guards.deny_write_tools
                            || requested_guards.deny_write_tools,
                        deny_shell_tools: previous_guards.deny_shell_tools
                            || requested_guards.deny_shell_tools,
                    }
                } else {
                    requested_guards
                };
                let capability_changed =
                    previous_mode != request.mode || previous_guards != next_guards;
                let next_execution_context = requested_execution_context
                    .clone()
                    .unwrap_or_else(|| previous_execution_context.clone());
                let execution_context_changed =
                    next_execution_context != previous_execution_context;
                if previous_guards.deny_write_tools
                    && !next_guards.deny_write_tools
                    && !request.write_scope_verified
                {
                    return Err(CodingSessionError::WriteScopeRequired);
                }
                if self.take_coding_continuity_fault() {
                    return Err(CodingSessionError::CommitFailed);
                }

                // Snapshot the previous attempt *before* appending the new
                // task_instruction. Continuation feedback projects over this so
                // it describes what the previous attempt did, not the empty new
                // attempt. Use the per-session evidence cap so a long previous
                // attempt's work is retained for the projection.
                let pre_instruction_summary = inner
                    .summary(&session_id, Some(DEFAULT_MAX_EVENTS_PER_SESSION))
                    .expect("reusable session must summarize before instruction append");

                let event = coding_instruction_event(
                    &new_event_id,
                    &session_id,
                    &request.project,
                    instruction.clone(),
                    request.transport,
                    request.mode,
                    Some(previous_mode),
                    requested_guards,
                    Some(previous_guards),
                    capability_changed,
                    request.context_refreshed,
                    requested_execution_context.clone(),
                    Some(previous_execution_context.clone()),
                    execution_context_changed,
                    true,
                    explicit_resume,
                    request.bind_current && request.key.is_some(),
                    now,
                );
                {
                    let max_events = inner.max_events_per_session;
                    let record = inner
                        .sessions
                        .get_mut(&session_id)
                        .and_then(StoredSession::hot_mut)
                        .expect("active reusable session must stay hot before commit");
                    if legacy_authority_upgrade {
                        // The exact durable binding proof and every other fallible
                        // continuation check completed above. Commit the canonical
                        // fence with the same Session mutation/persistence generation.
                        record.owner_authority_fingerprint = request.authority_fingerprint.clone();
                    }
                    record.mode = request.mode;
                    record.guards = next_guards;
                    record.execution_context = next_execution_context;
                    record.updated_at = now;
                    if let Some(project_instructions) = request.project_instructions {
                        // A transient runner/read failure must not erase the
                        // last complete in-memory rules snapshot. Fresh
                        // sessions may still retain a partial/unavailable
                        // snapshot so startup can report it conservatively.
                        if project_instructions.scan_complete
                            || record.project_instructions.is_none()
                        {
                            record.project_instructions = Some(project_instructions);
                        }
                    }
                    record.events.push_back(Arc::new(event));
                    record.events_observed = record.events_observed.saturating_add(1);
                    while record.events.len() > max_events {
                        record.events.pop_front();
                    }
                }
                inner.touch(&session_id);
                if request.bind_current {
                    if let Some(key) = request.key {
                        inner.replace_current_binding(key, &session_id, now);
                    }
                }
                let summary = inner
                    .summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT))
                    .expect("continued session must summarize");
                CodingSessionOutcome {
                    summary,
                    pre_instruction_summary: Some(pre_instruction_summary),
                    reused: true,
                    previous_mode: Some(previous_mode),
                    previous_guards: Some(previous_guards),
                    capability_changed,
                    execution_context_changed,
                }
            } else {
                if self.take_coding_continuity_fault() {
                    return Err(CodingSessionError::CommitFailed);
                }
                let execution_context = requested_execution_context.clone().unwrap_or_default();
                let execution_context_changed = !execution_context.is_empty();
                let event = coding_instruction_event(
                    &new_event_id,
                    &new_session_id,
                    &request.project,
                    instruction.clone(),
                    request.transport,
                    request.mode,
                    None,
                    requested_guards,
                    None,
                    false,
                    request.context_refreshed,
                    requested_execution_context,
                    None,
                    execution_context_changed,
                    false,
                    false,
                    request.bind_current && request.key.is_some(),
                    now,
                );
                let record = SessionRecord {
                    session_id: new_session_id.clone(),
                    project: Some(request.project.clone()),
                    owner_authority_fingerprint: request.authority_fingerprint.clone(),
                    // The first accepted instruction remains the root title.
                    // Follow-up instructions never overwrite it.
                    title: instruction,
                    mode: request.mode,
                    guards: requested_guards,
                    execution_context,
                    lifecycle: SessionLifecycle::Active,
                    created_at: now,
                    updated_at: now,
                    messages: VecDeque::new(),
                    events: VecDeque::from([Arc::new(event)]),
                    events_observed: 1,
                    context_revision: 0,
                    materialized_validation_job_ids: VecDeque::new(),
                    message_observation_revision: 0,
                    message_observation_floor: 0,
                    message_observation_revisions: Default::default(),
                    project_instructions: request.project_instructions,
                };
                let summary = inner.insert_session(record);
                if request.bind_current {
                    if let Some(key) = request.key {
                        inner.replace_current_binding(key, &summary.session_id, now);
                    }
                }
                CodingSessionOutcome {
                    summary,
                    pre_instruction_summary: None,
                    reused: false,
                    previous_mode: None,
                    previous_guards: None,
                    capability_changed: false,
                    execution_context_changed,
                }
            }
        };
        self.persist_after_mutation();
        Ok(outcome)
    }

    #[cfg(test)]
    /// Inject a failure before any in-memory continuity mutation. This does
    /// not model background ledger persistence failure or rollback.
    pub(crate) fn fail_next_coding_continuity_precommit_for_test(&self) {
        self.fail_next_coding_continuity_precommit
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn take_coding_continuity_fault(&self) -> bool {
        self.fail_next_coding_continuity_precommit
            .swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(not(test))]
    fn take_coding_continuity_fault(&self) -> bool {
        false
    }

    pub(crate) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        self.with_record_for_query(session_id, |record, cold| {
            summarize_record(record, limit, cold)
        })
    }

    /// Bounded, read-only Workflow Session rows for one exact runtime project.
    /// The project is authoritative caller context, never request-controlled UI state.
    pub(crate) fn console_list_for_project(
        &self,
        project: &str,
        limit: Option<usize>,
    ) -> WorkflowSessionConsoleList {
        let limit = normalize_console_session_limit(limit);
        let (candidates, total) = {
            let inner = self.inner.lock().expect("session store mutex poisoned");
            let mut candidates = inner
                .sessions
                .values()
                .filter(|session| session.project() == Some(project))
                .map(|session| (session.session_id().to_string(), session.updated_at()))
                .collect::<Vec<_>>();
            candidates
                .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));
            let total = candidates.len();
            candidates.truncate(limit);
            (candidates, total)
        };
        let sessions = candidates
            .into_iter()
            .filter_map(|(session_id, _)| {
                self.with_record_for_query(&session_id, |record, _| {
                    (record.project.as_deref() == Some(project))
                        .then(|| build_console_list_item(record, project))
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        WorkflowSessionConsoleList {
            returned: sessions.len(),
            truncated: total > limit,
            total,
            sessions,
        }
    }

    /// Bounded, read-only human timeline for one exact project-scoped Session.
    /// Unknown and wrong-project ids intentionally collapse to the same `None`.
    pub(crate) fn console_detail_for_project(
        &self,
        project: &str,
        session_id: &str,
        limit: Option<usize>,
    ) -> Option<WorkflowSessionConsoleDetail> {
        let allowed = {
            let inner = self.inner.lock().expect("session store mutex poisoned");
            inner
                .sessions
                .get(session_id)
                .is_some_and(|session| session.project() == Some(project))
        };
        if !allowed {
            return None;
        }
        let limit = normalize_console_activity_limit(limit);
        self.with_record_for_query(session_id, |record, _| {
            (record.project.as_deref() == Some(project))
                .then(|| build_console_detail(record, project, limit))
        })
        .flatten()
    }

    pub(super) fn with_record_for_query<T>(
        &self,
        session_id: &str,
        query: impl FnOnce(&SessionRecord, Option<&ColdSessionRecord>) -> T,
    ) -> Option<T> {
        let (cold, max_events_per_session) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.touch(session_id);
            let max_events_per_session = inner.max_events_per_session;
            let stored = inner.sessions.get(session_id)?;
            match stored {
                StoredSession::Hot(record) => return Some(query(record, None)),
                StoredSession::Cold(record) => (record.clone(), max_events_per_session),
            }
        };
        let record = materialize_cold_session(&cold, max_events_per_session)?;
        Some(query(&record, Some(&cold)))
    }

    fn coldify_closed_session(&self, session_id: &str) {
        let (snapshot, project_instructions) = {
            let inner = self.inner.lock().expect("session store mutex poisoned");
            let Some(StoredSession::Hot(record)) = inner.sessions.get(session_id) else {
                return;
            };
            if record.lifecycle.allows_mutation() {
                return;
            }
            (
                PersistedSessionRecord::from_record(record, inner.max_events_per_session),
                record
                    .project_instructions
                    .as_ref()
                    .map(|instructions| instructions.to_summary()),
            )
        };
        let cold = match cold_session_from_persisted(&snapshot, project_instructions) {
            Ok(cold) => cold,
            Err(err) => {
                tracing::warn!("session cold serialization failed: {err}");
                return;
            }
        };
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let Some(stored) = inner.sessions.get_mut(session_id) else {
            return;
        };
        let Some(record) = stored.hot() else {
            return;
        };
        if !snapshot.still_matches_record(record) {
            return;
        }
        debug_assert!(!cold.lifecycle.allows_mutation());
        *stored = StoredSession::Cold(cold);
    }

    pub(crate) fn contains_session(&self, session_id: &str) -> bool {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.contains_session(session_id)
    }

    pub(crate) fn context_revision(&self, session_id: &str) -> Option<u64> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner
            .sessions
            .get(session_id)
            .map(StoredSession::context_revision)
    }

    pub(crate) fn session_mode(&self, session_id: &str) -> Option<SessionMode> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.guard_state(session_id).map(|(mode, _)| mode)
    }

    pub(crate) fn session_project(&self, session_id: &str) -> Option<Option<String>> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.session_project(session_id)
    }

    pub(crate) fn session_target_authority(
        &self,
        session_id: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.session_target_authority(session_id)
    }

    /// Return inherited defaults only for an active Session whose registered
    /// project exactly matches the already-resolved request project.
    pub(crate) fn execution_context_for_project(
        &self,
        session_id: &str,
        resolved_project: &str,
    ) -> Option<SessionExecutionContext> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        let record = inner.sessions.get(session_id)?.hot()?;
        (record.lifecycle.allows_mutation() && record.project.as_deref() == Some(resolved_project))
            .then(|| record.execution_context.clone())
    }

    pub(crate) fn guard_state(&self, session_id: &str) -> Option<(SessionMode, SessionGuards)> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.guard_state(session_id)
    }

    pub(crate) fn lifecycle_state(&self, session_id: &str) -> Option<SessionLifecycle> {
        let inner = self.inner.lock().expect("session store mutex poisoned");
        inner.lifecycle_state(session_id)
    }

    /// Explicit close: `Active → Closed`. Idempotent for already-closed sessions.
    ///
    /// Never creates a session for an unknown id. Removes every process-local
    /// and durable current binding that points at the session. Emits a single
    /// `session_closed` ledger event only on a real `Active → Closed`
    /// transition.
    pub(crate) fn close_session(
        &self,
        session_id: &str,
    ) -> Result<SessionCloseOutcome, SessionCloseError> {
        let session_id = session_id.trim();
        if session_id.is_empty() || !is_valid_session_id(session_id) {
            return Err(SessionCloseError::UnknownSession);
        }
        let committed = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            inner.close_session(session_id)?
        };
        let outcome = committed.unwrap_or_else(|| SessionCloseOutcome {
            summary: self
                .summary(session_id, Some(DEFAULT_SUMMARY_LIMIT))
                .expect("known cold closed session must materialize for summary"),
            already_closed: true,
        });
        self.coldify_closed_session(session_id);
        self.persist_after_mutation();
        Ok(outcome)
    }

    /// Replace the complete execution context and append one safe metadata
    /// event together under the in-memory store lock. Persistent stores then
    /// queue the JSON ledger to the background writer. `{}` clears all execution defaults.
    pub(crate) fn update_execution_context(
        &self,
        session_id: &str,
        execution_context: SessionExecutionContext,
        transport: SessionTransport,
    ) -> Result<SessionExecutionContextUpdateOutcome, SessionExecutionContextUpdateError> {
        let session_id = session_id.trim();
        if session_id.is_empty() || !is_valid_session_id(session_id) {
            return Err(SessionExecutionContextUpdateError::UnknownSession);
        }
        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let stored = inner
                .sessions
                .get(session_id)
                .ok_or(SessionExecutionContextUpdateError::UnknownSession)?;
            let lifecycle = stored.lifecycle();
            if !lifecycle.allows_mutation() {
                return Err(SessionExecutionContextUpdateError::SessionNotActive { lifecycle });
            }
            let record = stored
                .hot()
                .expect("active execution-context session must stay hot");
            let project = record
                .project
                .clone()
                .ok_or(SessionExecutionContextUpdateError::SessionHasNoProject)?;
            let previous_execution_context = record.execution_context.clone();
            let execution_context = execution_context
                .validated()
                .map_err(SessionExecutionContextUpdateError::InvalidExecutionContext)?;
            let changed = execution_context != previous_execution_context;
            let now = now_ts();
            let event = session_execution_context_updated_event(
                session_id,
                &project,
                transport,
                execution_context.clone(),
                previous_execution_context.clone(),
                changed,
                now,
            );
            let max_events = inner.max_events_per_session;
            let record = inner
                .sessions
                .get_mut(session_id)
                .and_then(StoredSession::hot_mut)
                .expect("validated active Session must stay hot under store lock");
            record.execution_context = execution_context;
            record.updated_at = now;
            record.events.push_back(Arc::new(event));
            record.events_observed = record.events_observed.saturating_add(1);
            while record.events.len() > max_events {
                record.events.pop_front();
            }
            inner.touch(session_id);
            let summary = inner
                .summary(session_id, Some(DEFAULT_SUMMARY_LIMIT))
                .expect("updated Session must summarize");
            SessionExecutionContextUpdateOutcome {
                summary,
                previous_execution_context,
                changed,
            }
        };
        self.persist_after_mutation();
        Ok(outcome)
    }

    /// Authoritative lifecycle check used by dispatch/kernel before mutation.
    ///
    /// Closed/Archived sessions deny write-like, shell-like, and a small set of
    /// session-local mutations (messages, checkpoint create/restore/delete).
    /// Query tools and pure reads remain allowed. `close_session` itself is
    /// never denied so repeated close stays idempotent.
    pub(crate) fn lifecycle_denial(
        &self,
        session_id: &str,
        tool_name: &str,
    ) -> Option<SessionLifecycleDenial> {
        let lifecycle = self.lifecycle_state(session_id)?;
        if lifecycle.allows_mutation() {
            return None;
        }
        if tool_name == "close_session" {
            return None;
        }
        if lifecycle_blocks_tool(tool_name) {
            return Some(SessionLifecycleDenial { lifecycle });
        }
        None
    }

    /// Authoritative guard check used by dispatch/kernel before mutation.
    pub(crate) fn guard_denial(
        &self,
        session_id: &str,
        tool_name: &str,
    ) -> Option<SessionGuardDenial> {
        let (mode, guards) = self.guard_state(session_id)?;
        let classification = SessionToolClassification::for_tool(tool_name);
        if mode == SessionMode::Inspect
            && matches!(tool_name, "open_session_shell" | "session_shell_exec")
        {
            return Some(SessionGuardDenial {
                mode,
                guard: "persistent_shell_mode_unsupported",
            });
        }
        if guards.deny_write_tools && classification.write_like {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_write_tools",
            });
        }
        if guards.deny_shell_tools && classification.shell_like {
            return Some(SessionGuardDenial {
                mode,
                guard: "deny_shell_tools",
            });
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn record_tool_call_started(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
    ) -> Option<ToolCallStart> {
        self.record_tool_call_started_with_options(
            session_id, transport, tool_name, arguments, None,
        )
    }

    pub(crate) fn record_tool_call_started_with_options(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
        resolved_project: Option<String>,
    ) -> Option<ToolCallStart> {
        self.record_tool_call_started_with_metadata(
            session_id,
            transport,
            tool_name,
            arguments,
            resolved_project,
            ToolCallRecorderMetadata::from_arguments(arguments),
        )
    }

    /// Sole entry for appending a `tool_call_started` ledger event.
    pub(crate) fn record_tool_call_started_with_metadata(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
        resolved_project: Option<String>,
        metadata: ToolCallRecorderMetadata,
    ) -> Option<ToolCallStart> {
        let session_id = session_id?.trim();
        if !is_valid_session_id(session_id) {
            return None;
        }
        let pre_call_context_revision = self.context_revision(session_id)?;
        let now = now_ts();
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let project = extract_project(arguments);
        let call_id = format!("{CALL_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let classification = SessionToolClassification::for_tool(tool_name);
        let risk_class = classification.risk_class.to_string();
        let changed_paths = changed_paths_for_tool(tool_name, arguments);
        let observed_paths = observed_input_paths_for_tool(tool_name, arguments);
        let diff_review_like = diff_review_like_for_tool(tool_name, arguments);
        let input_summary = Some(session_input_summary_for_tool(tool_name, arguments));
        let expectation = metadata.expectation;
        let ack_session_context_revision = metadata.ack_session_context_revision;
        let start = ToolCallStart {
            event_id: event_id.clone(),
            call_id: call_id.clone(),
            session_id: session_id.to_string(),
            transport,
            tool_name: tool_name.to_string(),
            project: project.clone(),
            resolved_project: resolved_project.clone(),
            risk_class: risk_class.clone(),
            read_like: classification.read_like,
            write_like: classification.write_like,
            shell_like: classification.shell_like,
            git_like: classification.git_like,
            change_summary_like: classification.change_summary_like,
            diff_review_like,
            changed_paths: changed_paths.clone(),
            observed_paths: observed_paths.clone(),
            started_at: now,
            started_instant: Instant::now(),
            permission: None,
            expectation: expectation.clone(),
            pre_call_context_revision,
            ack_session_context_revision,
        };
        self.push_event(SessionEvent {
            event_id,
            session_id: session_id.to_string(),
            kind: "tool_call_started".to_string(),
            context_revision: None,
            context_result_summary: None,
            call_id: Some(call_id),
            timestamp: now,
            transport: transport.as_str().to_string(),
            tool_name: tool_name.to_string(),
            project,
            resolved_project,
            risk_class,
            read_like: classification.read_like,
            write_like: classification.write_like,
            shell_like: classification.shell_like,
            git_like: classification.git_like,
            change_summary_like: classification.change_summary_like,
            diff_review_like,
            started_at: Some(now),
            finished_at: None,
            duration_ms: None,
            status: None,
            exit_code: None,
            failure_kind: None,
            error_kind: None,
            expected_failure: expectation.expected_failure.then_some(true),
            expected_failure_kind: expectation.expected_failure_kind.clone(),
            assertion_name: expectation.assertion_name.clone(),
            actual_failure_kind: None,
            failure_expectation_result: None,
            warning_kind: None,
            session_project: None,
            request_project: None,
            allow_cross_project_session_required: None,
            allow_cross_project_session: None,
            error_message_summary: None,
            changed_paths,
            observed_paths,
            job_id: None,
            persistent_shell: None,
            effect_evidence: None,
            input_summary,
            validation_output_summary: None,
            permission: None,
            instruction: None,
            requested_mode: None,
            previous_mode: None,
            requested_guards: None,
            previous_guards: None,
            capability_changed: None,
            context_refreshed: None,
            execution_context: None,
            previous_execution_context: None,
            execution_context_changed: None,
        });
        Some(start)
    }

    pub(crate) fn record_permission_decision(
        &self,
        start: &mut ToolCallStart,
        permission: PermissionDecision,
    ) {
        start.permission = Some(permission.clone());
        let persisted = self.set_event_permission(&start.session_id, &start.event_id, permission);
        if persisted {
            self.persist_after_mutation();
        }
    }

    /// Attach the bounded outcome of automatic persistent-shell cleanup to the
    /// single `session_closed` system event. Phase one permits at most one
    /// active shell per Session, so this remains a scalar evidence record.
    pub(crate) fn record_session_close_persistent_shell_evidence(
        &self,
        session_id: &str,
        shell_id: &str,
        shell_state: &str,
        execution_state: &str,
        error_code: Option<&str>,
        already_closed: bool,
    ) {
        let evidence = persistent_shell_event_evidence_for_tool_result(
            "close_session",
            &serde_json::json!({
                "shell_id": shell_id,
                "shell_state": shell_state,
                "execution_state": execution_state,
                "error_code": error_code,
                "command_started": false,
                "command_completed": false,
                "already_closed": already_closed,
            }),
        );
        let Some(evidence) = evidence else {
            return;
        };
        let persisted = self.set_session_close_persistent_shell_evidence(session_id, evidence);
        if persisted {
            self.persist_after_mutation();
        }
    }

    fn tool_effect_event_evidence_for_result(output: &Value) -> Option<ToolEffectEventEvidence> {
        let state_changed = output.get("state_changed").and_then(Value::as_bool);
        let command_started = output.get("command_started").and_then(Value::as_bool);
        let command_completed = output.get("command_completed").and_then(Value::as_bool);
        let execution_state = output
            .get("execution_state")
            .and_then(Value::as_str)
            .and_then(sanitize_tool_execution_state);
        if state_changed.is_none()
            && command_started.is_none()
            && command_completed.is_none()
            && execution_state.is_none()
        {
            return None;
        }
        Some(ToolEffectEventEvidence {
            state_changed,
            command_started,
            command_completed,
            execution_state,
        })
    }

    fn push_model_facing_event(
        &self,
        mut event: SessionEvent,
        pre_call_context_revision: u64,
        ack_session_context_revision: SessionContextRevisionAck,
    ) -> Option<RecordedModelFacingToolCall> {
        let session_id = event.session_id.clone();
        let outcome = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let max_events = inner.max_events_per_session;
            let stored = inner.sessions.get_mut(&session_id)?;
            let project_instructions = match stored {
                StoredSession::Cold(cold) => cold.project_instructions.clone(),
                StoredSession::Hot(_) => None,
            };
            let mut materialized = match stored {
                StoredSession::Hot(_) => None,
                StoredSession::Cold(cold) => materialize_cold_session(cold, max_events),
            };
            let record = match stored {
                StoredSession::Hot(record) => record,
                StoredSession::Cold(_) => materialized.as_mut()?,
            };
            if record.context_revision < pre_call_context_revision {
                return None;
            }
            let next_revision = record.context_revision.checked_add(1)?;
            let recovery_start = match ack_session_context_revision {
                SessionContextRevisionAck::Revision(revision)
                    if revision <= pre_call_context_revision =>
                {
                    revision
                }
                _ => 0,
            };
            let recovery_events = record
                .events
                .iter()
                .filter_map(|candidate| {
                    let candidate_revision = candidate.context_revision?;
                    (candidate_revision > recovery_start
                        && candidate_revision <= pre_call_context_revision)
                        .then(|| candidate.as_ref().clone())
                })
                .collect::<Vec<_>>();
            let expected_recovery_count = match ack_session_context_revision {
                SessionContextRevisionAck::Revision(revision)
                    if revision <= pre_call_context_revision =>
                {
                    pre_call_context_revision.saturating_sub(revision)
                }
                _ => pre_call_context_revision,
            };
            let history_lost = expected_recovery_count > recovery_events.len() as u64;
            event.context_revision = Some(next_revision);
            let event_id = event.event_id.clone();
            record.context_revision = next_revision;
            record.updated_at = record.updated_at.max(event.timestamp);
            record.events.push_back(Arc::new(event));
            record.events_observed = record.events_observed.saturating_add(1);
            while record.events.len() > max_events {
                record.events.pop_front();
            }
            let outcome = RecordedModelFacingToolCall {
                event_id,
                session_id: session_id.clone(),
                context_revision: next_revision,
                pre_call_context_revision,
                ack_session_context_revision,
                recovery_events,
                history_lost,
            };
            if let Some(record) = materialized.as_ref() {
                let persisted = PersistedSessionRecord::from_record(record, max_events);
                let cold = cold_session_from_persisted(&persisted, project_instructions).ok()?;
                *stored = StoredSession::Cold(cold);
            }
            inner.touch(&session_id);
            outcome
        };
        self.persist_after_mutation();
        Some(outcome)
    }

    /// Append a finished ledger event that is not itself returned as a model-facing
    /// ToolResult (for example a pre-kernel parsing/scope failure or an internal
    /// nested operation). It deliberately does not advance model context continuity.
    pub(crate) fn record_tool_call_finished(
        &self,
        start: Option<ToolCallStart>,
        success: bool,
        output: &Value,
        error: Option<&str>,
        error_kind: Option<&str>,
    ) -> Option<String> {
        let (event, _, _) =
            Self::tool_call_finished_event(start, success, output, error, error_kind)?;
        let event_id = event.event_id.clone();
        self.push_event(event);
        Some(event_id)
    }

    /// Append a finished model-facing ToolResult and atomically allocate its next
    /// Session-local context revision with the corresponding event annotation.
    pub(crate) fn record_model_facing_tool_call_finished(
        &self,
        start: Option<ToolCallStart>,
        success: bool,
        output: &Value,
        error: Option<&str>,
        error_kind: Option<&str>,
    ) -> Option<RecordedModelFacingToolCall> {
        let (event, pre_call_context_revision, ack_session_context_revision) =
            Self::tool_call_finished_event(start, success, output, error, error_kind)?;
        self.push_model_facing_event(
            event,
            pre_call_context_revision,
            ack_session_context_revision,
        )
    }

    fn tool_call_finished_event(
        start: Option<ToolCallStart>,
        success: bool,
        output: &Value,
        error: Option<&str>,
        error_kind: Option<&str>,
    ) -> Option<(SessionEvent, u64, SessionContextRevisionAck)> {
        let start = start?;
        let pre_call_context_revision = start.pre_call_context_revision;
        let ack_session_context_revision = start.ack_session_context_revision;
        let finished_at = now_ts();
        let duration_ms = start
            .started_instant
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let failure_kind = output
            .get("failure_kind")
            .and_then(Value::as_str)
            .map(str::to_string);
        let error_kind = error_kind
            .or_else(|| error.and_then(|_| output.get("failure_kind").and_then(Value::as_str)))
            .or_else(|| error.map(|_| "runtime_error"));
        let actual_failure_kind = actual_failure_kind_for_tool_result(output, error, error_kind);
        let failure_expectation_result = classify_failure_expectation(
            success,
            &start.expectation,
            actual_failure_kind.as_deref(),
        );
        let warning_kind = output
            .get("warning_kind")
            .and_then(Value::as_str)
            .map(str::to_string);
        let session_project = output
            .get("session_project")
            .and_then(Value::as_str)
            .map(str::to_string);
        let request_project = output
            .get("request_project")
            .and_then(Value::as_str)
            .map(str::to_string);
        let allow_cross_project_session_required = output
            .get("allow_cross_project_session_required")
            .and_then(Value::as_bool);
        let allow_cross_project_session = output
            .get("allow_cross_project_session")
            .and_then(Value::as_bool);
        let error_message_summary =
            error.map(|message| bound_event_error_summary(message, start.shell_like));
        let validation_output_summary =
            validation_output_summary_for_tool_result(&start.tool_name, output);
        let observed_paths = if success {
            observed_paths_for_successful_result(
                &start.tool_name,
                start.observed_paths.clone(),
                output,
            )
        } else {
            Vec::new()
        };
        let persistent_shell =
            persistent_shell_event_evidence_for_tool_result(&start.tool_name, output);
        let effect_evidence = Self::tool_effect_event_evidence_for_result(output);
        let event = SessionEvent {
            event_id,
            session_id: start.session_id,
            kind: "tool_call_finished".to_string(),
            context_revision: None,
            context_result_summary: context_result_summary_for_tool_result(
                &start.tool_name,
                output,
            ),
            call_id: Some(start.call_id),
            timestamp: finished_at,
            transport: start.transport.as_str().to_string(),
            tool_name: start.tool_name,
            project: start.project,
            resolved_project: start.resolved_project,
            risk_class: start.risk_class,
            read_like: start.read_like,
            write_like: start.write_like,
            shell_like: start.shell_like,
            git_like: start.git_like,
            change_summary_like: start.change_summary_like,
            diff_review_like: start.diff_review_like,
            started_at: Some(start.started_at),
            finished_at: Some(finished_at),
            duration_ms: Some(duration_ms),
            status: Some(if success { "succeeded" } else { "failed" }.to_string()),
            exit_code: output.get("exit_code").and_then(Value::as_i64),
            failure_kind,
            error_kind: error.map(|_| error_kind.unwrap_or("runtime_error").to_string()),
            expected_failure: start.expectation.expected_failure.then_some(true),
            expected_failure_kind: start.expectation.expected_failure_kind,
            assertion_name: start.expectation.assertion_name,
            actual_failure_kind,
            failure_expectation_result: Some(failure_expectation_result.to_string()),
            warning_kind,
            session_project,
            request_project,
            allow_cross_project_session_required,
            allow_cross_project_session,
            error_message_summary,
            changed_paths: start.changed_paths,
            observed_paths,
            job_id: extract_job_id(output),
            persistent_shell,
            effect_evidence,
            input_summary: None,
            validation_output_summary,
            permission: start.permission,
            instruction: None,
            requested_mode: None,
            previous_mode: None,
            requested_guards: None,
            previous_guards: None,
            capability_changed: None,
            context_refreshed: None,
            execution_context: None,
            previous_execution_context: None,
            execution_context_changed: None,
        };
        Some((
            event,
            pre_call_context_revision,
            ack_session_context_revision,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_validation_job_terminal(
        &self,
        session_id: &str,
        job_id: &str,
        retained_terminal_job_ids: &[&str],
        tool_name: &str,
        project: Option<String>,
        validation_target_id: &str,
        job_status: &str,
        exit_code: Option<i64>,
        validation_passed: Option<bool>,
        started_at: Option<i64>,
        finished_at: Option<i64>,
        duration_ms: Option<u64>,
        validation_output_summary: Option<Value>,
    ) -> bool {
        let session_id = session_id.trim();
        let job_id = job_id.trim();
        let valid_target = validation_target_id
            .strip_prefix("target:")
            .is_some_and(|suffix| {
                suffix.len() == 24 && suffix.as_bytes().iter().all(u8::is_ascii_hexdigit)
            });
        let Some(timestamp) = finished_at else {
            // Reconciliation must never substitute wall-clock read time for
            // authoritative execution activity.
            return false;
        };
        if !is_valid_session_id(session_id)
            || !super::super::helpers::is_safe_job_id(job_id)
            || retained_terminal_job_ids.len() > MAX_MATERIALIZED_VALIDATION_JOB_IDS
            || retained_terminal_job_ids.iter().any(|candidate| {
                *candidate != candidate.trim() || !super::super::helpers::is_safe_job_id(candidate)
            })
            || !retained_terminal_job_ids
                .iter()
                .any(|candidate| *candidate == job_id)
            || !matches!(
                tool_name,
                "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test"
            )
            || !valid_target
            || !matches!(
                job_status,
                "completed" | "failed" | "timeout" | "timed_out" | "stopped" | "cancelled" | "lost"
            )
        {
            return false;
        }
        let process_succeeded = job_status == "completed" && exit_code == Some(0);
        let succeeded = process_succeeded && validation_passed.unwrap_or(true);
        let failure_kind = (!succeeded).then(|| match job_status {
            "timeout" | "timed_out" => "timeout".to_string(),
            "stopped" | "cancelled" => "cancelled".to_string(),
            "lost" => "execution_lost".to_string(),
            _ if process_succeeded => "validation_failed".to_string(),
            _ => "command_exit_nonzero".to_string(),
        });
        let classification = SessionToolClassification::for_tool(tool_name);
        let event = SessionEvent {
            event_id: format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
            call_id: None,
            session_id: session_id.to_string(),
            kind: "validation_job_terminal".to_string(),
            context_revision: None,
            context_result_summary: None,
            timestamp,
            transport: "job_terminal".to_string(),
            tool_name: tool_name.to_string(),
            project: project.clone(),
            resolved_project: project.clone(),
            risk_class: classification.risk_class.to_string(),
            read_like: classification.read_like,
            write_like: classification.write_like,
            shell_like: classification.shell_like,
            git_like: classification.git_like,
            change_summary_like: classification.change_summary_like,
            diff_review_like: false,
            started_at,
            finished_at: Some(timestamp),
            duration_ms,
            status: Some(if succeeded { "succeeded" } else { "failed" }.to_string()),
            exit_code,
            failure_kind,
            error_kind: None,
            expected_failure: None,
            expected_failure_kind: None,
            assertion_name: None,
            actual_failure_kind: None,
            failure_expectation_result: None,
            warning_kind: None,
            session_project: None,
            request_project: None,
            allow_cross_project_session_required: None,
            allow_cross_project_session: None,
            error_message_summary: None,
            changed_paths: Vec::new(),
            observed_paths: Vec::new(),
            job_id: Some(job_id.to_string()),
            persistent_shell: None,
            effect_evidence: None,
            input_summary: Some(serde_json::json!({
                "validation_target_id": validation_target_id,
            })),
            validation_output_summary,
            permission: None,
            instruction: None,
            requested_mode: None,
            previous_mode: None,
            requested_guards: None,
            previous_guards: None,
            capability_changed: None,
            context_refreshed: None,
            execution_context: None,
            previous_execution_context: None,
            execution_context_changed: None,
        };

        let (cold, hot_closed, recorded) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let max_events = inner.max_events_per_session;
            let Some(stored) = inner.sessions.get_mut(session_id) else {
                return false;
            };
            match stored {
                StoredSession::Hot(record) => {
                    let recorded = Self::append_validation_job_terminal_to_record(
                        record,
                        job_id,
                        retained_terminal_job_ids,
                        project.as_deref(),
                        &event,
                        timestamp,
                        max_events,
                    );
                    let hot_closed = recorded && !record.lifecycle.allows_mutation();
                    (None, hot_closed, recorded)
                }
                StoredSession::Cold(record) => (Some(record.clone()), false, false),
            }
        };
        let recorded = match cold {
            Some(cold) => {
                self.rewrite_cold_record(session_id, cold, false, |record, max_events| {
                    Self::append_validation_job_terminal_to_record(
                        record,
                        job_id,
                        retained_terminal_job_ids,
                        project.as_deref(),
                        &event,
                        timestamp,
                        max_events,
                    )
                })
            }
            None => recorded,
        };
        if !recorded {
            return false;
        }
        if hot_closed {
            self.coldify_closed_session(session_id);
        }
        self.persist_after_mutation();
        true
    }

    fn append_validation_job_terminal_to_record(
        record: &mut SessionRecord,
        job_id: &str,
        retained_terminal_job_ids: &[&str],
        project: Option<&str>,
        event: &SessionEvent,
        timestamp: i64,
        max_events: usize,
    ) -> bool {
        if record.project.as_deref() != project
            || record
                .materialized_validation_job_ids
                .iter()
                .any(|materialized| materialized == job_id)
        {
            return false;
        }

        // The Runner can retain at most 64 authoritative terminal Jobs. Keep an
        // exact marker for every Job still present in this reconciliation snapshot;
        // if stale markers fill the bound, discard one that the authoritative
        // snapshot can no longer name before inserting the new identity.
        while record.materialized_validation_job_ids.len() >= MAX_MATERIALIZED_VALIDATION_JOB_IDS {
            let Some(stale_index) =
                record
                    .materialized_validation_job_ids
                    .iter()
                    .position(|materialized| {
                        !retained_terminal_job_ids
                            .iter()
                            .any(|candidate| *candidate == materialized.as_str())
                    })
            else {
                // A complete valid terminal snapshot cannot name more than the
                // bound. Fail closed rather than evicting a still-retained Job.
                return false;
            };
            record.materialized_validation_job_ids.remove(stale_index);
        }
        record
            .materialized_validation_job_ids
            .push_back(job_id.to_string());
        record.updated_at = record.updated_at.max(timestamp);
        record.events.push_back(Arc::new(event.clone()));
        record.events_observed = record.events_observed.saturating_add(1);
        while record.events.len() > max_events {
            record.events.pop_front();
        }
        true
    }

    /// Append bounded recorder-only CodingAgentRun lifecycle evidence. The
    /// explicit Workflow Session is provenance only: this path grants no Run
    /// authority and intentionally stores no prompt, ACP session id, event body,
    /// reasoning, tool payload, credential, or idempotency key.
    pub(crate) fn record_coding_agent_lifecycle_evidence(
        &self,
        session_id: &str,
        project: &str,
        run_id: &str,
        provider_id: &str,
        kind: &str,
        state: &str,
        execution_state: &str,
        terminal_stop_reason: Option<&str>,
        terminal_error_code: Option<&str>,
    ) -> bool {
        let project_matches = self
            .with_record_for_query(session_id, |record, _| {
                record.project.as_deref() == Some(project)
            })
            .unwrap_or(false);
        if !project_matches {
            return false;
        }
        let now = now_ts();
        self.push_event(coding_agent_lifecycle_event(
            session_id,
            project,
            run_id,
            provider_id,
            kind,
            state,
            execution_state,
            terminal_stop_reason,
            terminal_error_code,
            now,
        ));
        true
    }

    /// Sole entry for appending a session ledger event.
    fn push_event(&self, event: SessionEvent) {
        let session_id = event.session_id.clone();
        let mut event = Some(event);
        let (cold, hot_closed) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let max_events_per_session = inner.max_events_per_session;
            let Some(stored) = inner.sessions.get_mut(&session_id) else {
                return;
            };
            match stored {
                StoredSession::Hot(record) => {
                    record.updated_at = now_ts();
                    record.events.push_back(Arc::new(event.take().unwrap()));
                    record.events_observed = record.events_observed.saturating_add(1);
                    while record.events.len() > max_events_per_session {
                        record.events.pop_front();
                    }
                    let hot_closed = !record.lifecycle.allows_mutation();
                    inner.touch(&session_id);
                    (None, hot_closed)
                }
                StoredSession::Cold(record) => (Some(record.clone()), false),
            }
        };
        let persisted = match cold {
            Some(cold) => {
                let event = event.as_ref().expect("cold append keeps source event");
                self.rewrite_cold_record(&session_id, cold, true, |record, max_events| {
                    record.updated_at = now_ts();
                    record.events.push_back(Arc::new(event.clone()));
                    record.events_observed = record.events_observed.saturating_add(1);
                    while record.events.len() > max_events {
                        record.events.pop_front();
                    }
                    true
                })
            }
            None => true,
        };
        if persisted {
            if hot_closed {
                self.coldify_closed_session(&session_id);
            }
            self.persist_after_mutation();
        }
    }

    fn max_events_per_session(&self) -> usize {
        self.inner
            .lock()
            .expect("session store mutex poisoned")
            .max_events_per_session
    }

    fn rewrite_cold_record(
        &self,
        session_id: &str,
        mut cold: ColdSessionRecord,
        touch: bool,
        mutate: impl Fn(&mut SessionRecord, usize) -> bool,
    ) -> bool {
        let max_events_per_session = self.max_events_per_session();
        loop {
            let Some(mut record) = materialize_cold_session(&cold, max_events_per_session) else {
                tracing::warn!(session_id, "cold session materialization failed");
                return false;
            };
            if !mutate(&mut record, max_events_per_session) {
                return false;
            }
            let persisted = PersistedSessionRecord::from_record(&record, max_events_per_session);
            let next =
                match cold_session_from_persisted(&persisted, cold.project_instructions.clone()) {
                    Ok(next) => next,
                    Err(err) => {
                        tracing::warn!(
                            session_id,
                            "cold session rewrite serialization failed: {err}"
                        );
                        return false;
                    }
                };
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let Some(stored) = inner.sessions.get_mut(session_id) else {
                return false;
            };
            match stored {
                StoredSession::Cold(current) if Arc::ptr_eq(&current.raw, &cold.raw) => {
                    *stored = StoredSession::Cold(next);
                    if touch {
                        inner.touch(session_id);
                    }
                    return true;
                }
                StoredSession::Cold(current) => {
                    cold = current.clone();
                }
                StoredSession::Hot(_) => return false,
            }
        }
    }

    fn set_event_permission(
        &self,
        session_id: &str,
        event_id: &str,
        permission: PermissionDecision,
    ) -> bool {
        let mut permission = Some(permission);
        let (cold, hot_closed, found) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let Some(stored) = inner.sessions.get_mut(session_id) else {
                return false;
            };
            match stored {
                StoredSession::Hot(record) => {
                    let found = record
                        .events
                        .iter_mut()
                        .rev()
                        .find(|event| event.event_id == event_id)
                        .map(|event| {
                            Arc::make_mut(event).permission = Some(permission.take().unwrap());
                        })
                        .is_some();
                    (None, !record.lifecycle.allows_mutation(), found)
                }
                StoredSession::Cold(record) => (Some(record.clone()), false, true),
            }
        };
        let found = match cold {
            Some(cold) => self.rewrite_cold_record(session_id, cold, false, |record, _| {
                let Some(event) = record
                    .events
                    .iter_mut()
                    .rev()
                    .find(|event| event.event_id == event_id)
                else {
                    return false;
                };
                Arc::make_mut(event).permission = Some(permission.as_ref().unwrap().clone());
                true
            }),
            None => found,
        };
        if found && hot_closed {
            self.coldify_closed_session(session_id);
        }
        found
    }

    fn set_session_close_persistent_shell_evidence(
        &self,
        session_id: &str,
        evidence: PersistentShellEventEvidence,
    ) -> bool {
        let mut evidence = Some(evidence);
        let (cold, hot_closed, found) = {
            let mut inner = self.inner.lock().expect("session store mutex poisoned");
            let Some(stored) = inner.sessions.get_mut(session_id) else {
                return false;
            };
            match stored {
                StoredSession::Hot(record) => {
                    let found = if let Some(event) = record.events.iter_mut().rev().find(|event| {
                        event.kind == "session_closed" && event.tool_name == "close_session"
                    }) {
                        Arc::make_mut(event).persistent_shell = Some(evidence.take().unwrap());
                        true
                    } else {
                        false
                    };
                    if found {
                        record.updated_at = now_ts();
                    }
                    (None, !record.lifecycle.allows_mutation(), found)
                }
                StoredSession::Cold(record) => (Some(record.clone()), false, true),
            }
        };
        let found = match cold {
            Some(cold) => self.rewrite_cold_record(session_id, cold, false, |record, _| {
                let Some(event) = record.events.iter_mut().rev().find(|event| {
                    event.kind == "session_closed" && event.tool_name == "close_session"
                }) else {
                    return false;
                };
                Arc::make_mut(event).persistent_shell = Some(evidence.as_ref().unwrap().clone());
                record.updated_at = now_ts();
                true
            }),
            None => found,
        };
        if found && hot_closed {
            self.coldify_closed_session(session_id);
        }
        found
    }

    pub(super) fn persist_after_mutation(&self) {
        if let Some(writer) = &self.writer {
            // Fire-and-forget: the dedicated writer thread serializes and
            // writes. Narrow restart-safe operations use
            // `persist_after_mutation_durable` instead.
            let _ = writer.mark_dirty();
            return;
        }
        self.persist_after_mutation_with(write_ledger_atomic);
    }

    /// Persist this exact mutation generation before returning success. Used
    /// only by low-frequency operations whose success response promises
    /// restart-safe idempotency; ordinary Session events remain asynchronous.
    pub(super) fn persist_after_mutation_durable(&self) -> Result<(), ()> {
        if let Some(writer) = &self.writer {
            let generation = writer.mark_dirty();
            writer.flush_through(generation);
        } else {
            self.persist_after_mutation_with(write_ledger_atomic);
        }
        let inner = self.inner.lock().expect("session store mutex poisoned");
        if inner
            .persistence
            .as_ref()
            .and_then(|persistence| persistence.last_persist_error.as_ref())
            .is_some()
        {
            Err(())
        } else {
            Ok(())
        }
    }

    pub(super) fn persist_after_mutation_with(
        &self,
        write_ledger: impl FnOnce(&PathBuf, &PersistedSessionLedger) -> io::Result<()>,
    ) {
        let _write_guard = self
            .persistence_write_mutex
            .lock()
            .expect("session persistence mutex poisoned");
        let Some((path, ledger)) = ({
            let inner = self.inner.lock().expect("session store mutex poisoned");
            let path = inner
                .persistence
                .as_ref()
                .map(|persistence| persistence.path.clone());
            path.map(|path| (path, inner.to_persisted_ledger()))
        }) else {
            return;
        };
        let result = write_ledger(&path, &ledger).map_err(|err| {
            bound_summary_string(&format!("persist_failed: {}: {err}", path.display()))
        });
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let Some(persistence) = inner.persistence.as_mut() else {
            return;
        };
        match result {
            Ok(()) => persistence.last_persist_error = None,
            Err(error) => {
                tracing::warn!("session ledger persistence failed: {}", error);
                persistence.last_persist_error = Some(error);
            }
        }
    }
}

/// Authoritative in-memory transitions for workflow session state.
///
/// Map fields stay private; sibling modules must use these helpers so create,
/// bind, message, and event mutations cannot bypass the store.
/// Tools blocked on Closed/Archived workflow sessions.
///
/// Query and pure-read tools remain allowed. Message-board mutations and
/// session-scoped checkpoint mutations are blocked even when metadata marks
/// them read-only (they still change durable session/project evidence).
fn lifecycle_blocks_tool(tool_name: &str) -> bool {
    let classification = SessionToolClassification::for_tool(tool_name);
    if classification.write_like || classification.shell_like {
        return true;
    }
    matches!(
        tool_name,
        "post_session_message"
            | "resolve_session_message"
            | "complete_session_message"
            | "update_session_context"
            | "workspace_checkpoint_create"
            | "workspace_checkpoint_restore"
            | "workspace_checkpoint_delete"
    )
}

#[allow(clippy::too_many_arguments)]
fn coding_instruction_event(
    event_id: &str,
    session_id: &str,
    project: &str,
    instruction: Option<String>,
    transport: SessionTransport,
    requested_mode: SessionMode,
    previous_mode: Option<SessionMode>,
    requested_guards: SessionGuards,
    previous_guards: Option<SessionGuards>,
    capability_changed: bool,
    context_refreshed: bool,
    execution_context: Option<SessionExecutionContext>,
    previous_execution_context: Option<SessionExecutionContext>,
    execution_context_changed: bool,
    reused: bool,
    explicit_resume: bool,
    current_binding_established: bool,
    now: i64,
) -> SessionEvent {
    SessionEvent {
        event_id: event_id.to_string(),
        session_id: session_id.to_string(),
        kind: "task_instruction".to_string(),
        context_revision: None,
        context_result_summary: None,
        call_id: None,
        timestamp: now,
        transport: transport.as_str().to_string(),
        tool_name: "start_coding_task".to_string(),
        project: Some(project.to_string()),
        resolved_project: Some(project.to_string()),
        risk_class: "read_only".to_string(),
        read_like: true,
        write_like: false,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        diff_review_like: false,
        started_at: Some(now),
        finished_at: Some(now),
        duration_ms: Some(0),
        status: Some("succeeded".to_string()),
        exit_code: None,
        failure_kind: None,
        error_kind: None,
        expected_failure: None,
        expected_failure_kind: None,
        assertion_name: None,
        actual_failure_kind: None,
        failure_expectation_result: None,
        warning_kind: None,
        session_project: None,
        request_project: None,
        allow_cross_project_session_required: None,
        allow_cross_project_session: None,
        error_message_summary: None,
        changed_paths: Vec::new(),
        observed_paths: Vec::new(),
        job_id: None,
        persistent_shell: None,
        effect_evidence: None,
        input_summary: Some(redact_and_bound_value(&serde_json::json!({
            "requested_mode": requested_mode.as_str(),
            "requested_guards": requested_guards,
            "capability_changed": capability_changed,
            "context_refreshed": context_refreshed,
            "execution_context_provided": execution_context.is_some(),
            "execution_context_changed": execution_context_changed,
            "execution_context": execution_context,
            "previous_execution_context": previous_execution_context,
            "session_reused": reused,
            "explicit_resume": explicit_resume,
            "current_binding_established": current_binding_established,
        }))),
        validation_output_summary: None,
        permission: None,
        instruction,
        requested_mode: Some(requested_mode.as_str().to_string()),
        previous_mode: previous_mode.map(|mode| mode.as_str().to_string()),
        requested_guards: Some(requested_guards),
        previous_guards,
        capability_changed: Some(capability_changed),
        context_refreshed: Some(context_refreshed),
        execution_context,
        previous_execution_context,
        execution_context_changed: Some(execution_context_changed),
    }
}

fn coding_agent_lifecycle_event(
    session_id: &str,
    project: &str,
    run_id: &str,
    provider_id: &str,
    kind: &str,
    state: &str,
    execution_state: &str,
    terminal_stop_reason: Option<&str>,
    terminal_error_code: Option<&str>,
    now: i64,
) -> SessionEvent {
    let input_summary = serde_json::json!({
        "run_id": bound_summary_string(run_id),
        "provider_id": bound_summary_string(provider_id),
        "state": bound_summary_string(state),
        "execution_state": bound_summary_string(execution_state),
        "terminal_stop_reason": terminal_stop_reason.map(bound_summary_string),
        "terminal_error_code": terminal_error_code.map(bound_summary_string),
    });
    SessionEvent {
        event_id: format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.to_string(),
        kind: bound_summary_string(kind),
        call_id: None,
        timestamp: now,
        transport: "system".to_string(),
        tool_name: "coding_agent_start".to_string(),
        project: Some(project.to_string()),
        resolved_project: Some(project.to_string()),
        risk_class: "job_run".to_string(),
        read_like: false,
        write_like: false,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        diff_review_like: false,
        started_at: Some(now),
        finished_at: Some(now),
        duration_ms: Some(0),
        status: Some(bound_summary_string(state)),
        exit_code: None,
        failure_kind: None,
        error_kind: terminal_error_code.map(bound_summary_string),
        expected_failure: None,
        expected_failure_kind: None,
        assertion_name: None,
        actual_failure_kind: None,
        failure_expectation_result: None,
        warning_kind: None,
        session_project: None,
        request_project: None,
        allow_cross_project_session_required: None,
        allow_cross_project_session: None,
        error_message_summary: None,
        changed_paths: Vec::new(),
        observed_paths: Vec::new(),
        job_id: None,
        persistent_shell: None,
        effect_evidence: None,
        input_summary: Some(input_summary),
        validation_output_summary: None,
        permission: None,
        instruction: None,
        requested_mode: None,
        previous_mode: None,
        requested_guards: None,
        previous_guards: None,
        capability_changed: None,
        context_refreshed: None,
        execution_context: None,
        previous_execution_context: None,
        execution_context_changed: None,
    }
}

fn session_closed_system_event(session_id: &str, now: i64) -> SessionEvent {
    SessionEvent {
        event_id: format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.to_string(),
        kind: "session_closed".to_string(),
        context_revision: None,
        context_result_summary: None,
        call_id: None,
        timestamp: now,
        transport: "system".to_string(),
        tool_name: "close_session".to_string(),
        project: None,
        resolved_project: None,
        risk_class: "read_only".to_string(),
        read_like: true,
        write_like: false,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        diff_review_like: false,
        started_at: Some(now),
        finished_at: Some(now),
        duration_ms: Some(0),
        status: Some("succeeded".to_string()),
        exit_code: None,
        failure_kind: None,
        error_kind: None,
        expected_failure: None,
        expected_failure_kind: None,
        assertion_name: None,
        actual_failure_kind: None,
        failure_expectation_result: None,
        warning_kind: None,
        session_project: None,
        request_project: None,
        allow_cross_project_session_required: None,
        allow_cross_project_session: None,
        error_message_summary: None,
        changed_paths: Vec::new(),
        observed_paths: Vec::new(),
        job_id: None,
        persistent_shell: None,
        effect_evidence: None,
        input_summary: None,
        validation_output_summary: None,
        permission: None,
        instruction: None,
        requested_mode: None,
        previous_mode: None,
        requested_guards: None,
        previous_guards: None,
        capability_changed: None,
        context_refreshed: None,
        execution_context: None,
        previous_execution_context: None,
        execution_context_changed: None,
    }
}

fn session_execution_context_updated_event(
    session_id: &str,
    project: &str,
    transport: SessionTransport,
    execution_context: SessionExecutionContext,
    previous_execution_context: SessionExecutionContext,
    changed: bool,
    now: i64,
) -> SessionEvent {
    SessionEvent {
        event_id: format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.to_string(),
        kind: "session_execution_context_updated".to_string(),
        context_revision: None,
        context_result_summary: None,
        call_id: None,
        timestamp: now,
        transport: transport.as_str().to_string(),
        tool_name: "update_session_context".to_string(),
        project: Some(project.to_string()),
        resolved_project: Some(project.to_string()),
        risk_class: "read_only".to_string(),
        read_like: true,
        write_like: false,
        shell_like: false,
        git_like: false,
        change_summary_like: false,
        diff_review_like: false,
        started_at: Some(now),
        finished_at: Some(now),
        duration_ms: Some(0),
        status: Some("succeeded".to_string()),
        exit_code: None,
        failure_kind: None,
        error_kind: None,
        expected_failure: None,
        expected_failure_kind: None,
        assertion_name: None,
        actual_failure_kind: None,
        failure_expectation_result: None,
        warning_kind: None,
        session_project: None,
        request_project: None,
        allow_cross_project_session_required: None,
        allow_cross_project_session: None,
        error_message_summary: None,
        changed_paths: Vec::new(),
        observed_paths: Vec::new(),
        job_id: None,
        persistent_shell: None,
        effect_evidence: None,
        input_summary: Some(redact_and_bound_value(&serde_json::json!({
            "execution_context": execution_context,
            "previous_execution_context": previous_execution_context,
            "execution_context_changed": changed,
        }))),
        validation_output_summary: None,
        permission: None,
        instruction: None,
        requested_mode: None,
        previous_mode: None,
        requested_guards: None,
        previous_guards: None,
        capability_changed: None,
        context_refreshed: None,
        execution_context: Some(execution_context),
        previous_execution_context: Some(previous_execution_context),
        execution_context_changed: Some(changed),
    }
}

fn summarize_record(
    record: &SessionRecord,
    limit: Option<usize>,
    cold: Option<&ColdSessionRecord>,
) -> SessionSummary {
    let limit = limit
        .unwrap_or(DEFAULT_SUMMARY_LIMIT)
        .clamp(0, MAX_SUMMARY_LIMIT);
    let finished_events: Vec<&SessionEvent> = record
        .events
        .iter()
        .map(Arc::as_ref)
        .filter(|event| event.kind == "tool_call_finished")
        .collect();
    let counts = SessionCounts {
        tool_calls: finished_events.len(),
        succeeded: finished_events
            .iter()
            .filter(|event| event.status.as_deref() == Some("succeeded"))
            .count(),
        failed: finished_events
            .iter()
            .filter(|event| event.status.as_deref() == Some("failed"))
            .count(),
        read_like: finished_events
            .iter()
            .filter(|event| event.read_like)
            .count(),
        write_like: finished_events
            .iter()
            .filter(|event| event.write_like)
            .count(),
        shell_like: finished_events
            .iter()
            .filter(|event| event.shell_like)
            .count(),
        git_like: finished_events
            .iter()
            .filter(|event| event.git_like)
            .count(),
        change_summary_like: finished_events
            .iter()
            .filter(|event| event.change_summary_like)
            .count(),
    };
    let retained_total = record.events.len();
    let observed_total = record.events_observed.max(retained_total as u64) as usize;
    let skip = retained_total.saturating_sub(limit);
    let events: Vec<SessionEvent> = record
        .events
        .iter()
        .skip(skip)
        .map(|event| event.as_ref().clone())
        .collect();
    let events_returned = events.len();
    let project_instructions = match cold {
        Some(cold) => cold.project_instructions.clone(),
        None => record
            .project_instructions
            .as_ref()
            .map(|snapshot| snapshot.to_summary()),
    };
    SessionSummary {
        session_id: record.session_id.clone(),
        project: record.project.clone(),
        title: record.title.clone(),
        mode: record.mode,
        guards: record.guards,
        execution_context: record.execution_context.clone(),
        lifecycle: record.lifecycle,
        created_at: record.created_at,
        updated_at: record.updated_at,
        counts,
        events,
        events_total: observed_total,
        events_returned,
        events_truncated: observed_total > events_returned,
        first_retained_sequence: observed_total.saturating_sub(events_returned),
        project_instructions,
        messages: build_messages_summary(record),
    }
}

impl SessionStoreInner {
    // --- create / lifecycle ---

    /// Sole map-insert path for a newly created session.
    pub(super) fn insert_session(&mut self, record: SessionRecord) -> SessionSummary {
        let session_id = record.session_id.clone();
        self.sessions
            .insert(session_id.clone(), StoredSession::Hot(record));
        self.touch(&session_id);
        self.enforce_session_bound();
        self.summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT))
            .expect("newly inserted session must summarize")
    }

    /// Explicit lifecycle close. Unknown ids fail without create.
    pub(super) fn close_session(
        &mut self,
        session_id: &str,
    ) -> Result<Option<SessionCloseOutcome>, SessionCloseError> {
        self.touch(session_id);
        let lifecycle = self
            .sessions
            .get(session_id)
            .map(StoredSession::lifecycle)
            .ok_or(SessionCloseError::UnknownSession)?;
        match lifecycle {
            SessionLifecycle::Closed | SessionLifecycle::Archived => {
                self.remove_bindings_for_session(session_id);
                let Some(record) = self.sessions.get(session_id).and_then(StoredSession::hot)
                else {
                    return Ok(None);
                };
                Ok(Some(SessionCloseOutcome {
                    summary: summarize_record(record, Some(DEFAULT_SUMMARY_LIMIT), None),
                    already_closed: true,
                }))
            }
            SessionLifecycle::Active => {
                let now = now_ts();
                let event = session_closed_system_event(session_id, now);
                let max_events = self.max_events_per_session;
                {
                    let record = self
                        .sessions
                        .get_mut(session_id)
                        .and_then(StoredSession::hot_mut)
                        .expect("active session must stay hot through close commit");
                    record.lifecycle = SessionLifecycle::Closed;
                    record.updated_at = now;
                    record.events.push_back(Arc::new(event));
                    record.events_observed = record.events_observed.saturating_add(1);
                    while record.events.len() > max_events {
                        record.events.pop_front();
                    }
                }
                self.remove_bindings_for_session(session_id);
                let record = self
                    .sessions
                    .get(session_id)
                    .and_then(StoredSession::hot)
                    .expect("just-closed session remains hot until outer coldification");
                Ok(Some(SessionCloseOutcome {
                    summary: summarize_record(record, Some(DEFAULT_SUMMARY_LIMIT), None),
                    already_closed: false,
                }))
            }
        }
    }

    // --- exact current-session bindings ---

    pub(super) fn bind_current(
        &mut self,
        key: CurrentSessionKey,
        session_id: &str,
    ) -> Option<SessionSummary> {
        let session_id = session_id.trim();
        let record = self.sessions.get(session_id)?;
        if !record.lifecycle().allows_mutation()
            || record.project() != Some(key.resolved_project.as_str())
        {
            return None;
        }
        self.touch(session_id);
        let summary = self.summary(session_id, Some(DEFAULT_SUMMARY_LIMIT))?;
        self.replace_current_binding(key, session_id, now_ts());
        Some(summary)
    }

    pub(super) fn current_session(
        &mut self,
        key: &CurrentSessionKey,
    ) -> (Option<SessionSummary>, bool) {
        let binding_key = key.durable_binding_key();
        if let Some(session_id) = self.current_sessions.get(key).cloned() {
            if self.binding_session_is_reusable(&session_id, &key.resolved_project) {
                self.touch(&session_id);
                let durable_matches = self
                    .durable_current_bindings
                    .get(&binding_key)
                    .is_some_and(|binding| binding.session_id == session_id);
                if !durable_matches {
                    self.durable_current_bindings.insert(
                        binding_key,
                        DurableCurrentBinding {
                            session_id: session_id.clone(),
                            updated_at: now_ts(),
                        },
                    );
                    self.enforce_durable_binding_bound();
                }
                return (
                    self.summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT)),
                    !durable_matches,
                );
            }
            self.current_sessions.remove(key);
        }

        let Some(binding) = self.durable_current_bindings.get(&binding_key).cloned() else {
            return (None, false);
        };
        if self.binding_session_is_reusable(&binding.session_id, &key.resolved_project) {
            self.current_sessions
                .insert(key.clone(), binding.session_id.clone());
            self.touch(&binding.session_id);
            return (
                self.summary(&binding.session_id, Some(DEFAULT_SUMMARY_LIMIT)),
                false,
            );
        }

        self.durable_current_bindings.remove(&binding_key);
        self.discarded_binding_count = self.discarded_binding_count.saturating_add(1);
        (None, true)
    }

    pub(super) fn unbind_current(&mut self, key: &CurrentSessionKey) -> bool {
        let process_local_removed = self.current_sessions.remove(key).is_some();
        let durable_removed = self
            .durable_current_bindings
            .remove(&key.durable_binding_key())
            .is_some();
        process_local_removed || durable_removed
    }

    fn legacy_project_session_authority_upgrade_proof(
        &self,
        session_id: &str,
        key: &CurrentSessionKey,
        expected_project: &str,
    ) -> bool {
        if key.resolved_project != expected_project {
            return false;
        }
        let legacy_session_matches = self.sessions.get(session_id).is_some_and(|record| {
            record.lifecycle().allows_mutation()
                && record.project() == Some(expected_project)
                && record.owner_authority_fingerprint().is_none()
        });
        legacy_session_matches
            && self
                .durable_current_bindings
                .get(&key.durable_binding_key())
                .is_some_and(|binding| binding.session_id == session_id)
    }

    fn reusable_current_session_id(
        &self,
        key: &CurrentSessionKey,
        project: &str,
    ) -> Option<String> {
        self.current_sessions
            .get(key)
            .filter(|session_id| self.binding_session_is_reusable(session_id, project))
            .cloned()
            .or_else(|| {
                self.durable_current_bindings
                    .get(&key.durable_binding_key())
                    .filter(|binding| {
                        self.binding_session_is_reusable(&binding.session_id, project)
                    })
                    .map(|binding| binding.session_id.clone())
            })
    }

    fn binding_session_is_reusable(&self, session_id: &str, project: &str) -> bool {
        self.sessions.get(session_id).is_some_and(|record| {
            record.lifecycle().allows_mutation() && record.project() == Some(project)
        })
    }

    fn replace_current_binding(
        &mut self,
        key: CurrentSessionKey,
        session_id: &str,
        updated_at: i64,
    ) {
        let binding_key = key.durable_binding_key();
        self.current_sessions
            .insert(key, session_id.trim().to_string());
        self.durable_current_bindings.insert(
            binding_key,
            DurableCurrentBinding {
                session_id: session_id.trim().to_string(),
                updated_at,
            },
        );
        self.enforce_durable_binding_bound();
    }

    fn remove_bindings_for_session(&mut self, session_id: &str) {
        self.current_sessions
            .retain(|_, bound_session_id| bound_session_id != session_id);
        self.durable_current_bindings
            .retain(|_, binding| binding.session_id != session_id);
    }

    // --- messages ---

    pub(super) fn post_message(
        &mut self,
        input: PostSessionMessageInput,
        requires_ack: bool,
    ) -> Result<(SessionMessage, bool), SessionMessageError> {
        self.touch(&input.session_id);
        let Some(stored) = self.sessions.get_mut(&input.session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let lifecycle = stored.lifecycle();
        if !lifecycle.allows_mutation() {
            return Err(SessionMessageError::SessionClosed { lifecycle });
        }
        let record = stored
            .hot_mut()
            .expect("active session message mutation must stay hot");
        if requires_ack
            && (input.kind != super::model::SessionMessageKind::Guidance
                || input.priority != super::model::SessionMessagePriority::High)
        {
            return Err(SessionMessageError::InvalidInput(
                "requires_ack is only valid for high-priority guidance".to_string(),
            ));
        }
        let message = validate_message_text(input.message)?;
        let tags = validate_message_tags(input.tags)?;
        if let Some(reply_to) = input.reply_to.as_deref() {
            let found = record
                .messages
                .iter()
                .any(|message| message.message_id == reply_to);
            if !found {
                return Err(SessionMessageError::UnknownMessage);
            }
        }
        let now = now_ts();
        let message = SessionMessage {
            message_id: format!("{MESSAGE_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
            session_id: input.session_id.clone(),
            created_at: now,
            kind: input.kind,
            status: SessionMessageStatus::Open,
            priority: input.priority,
            message,
            tags,
            reply_to: input.reply_to,
            requires_ack,
            first_ack_observed_at: None,
            author_session_id: None,
            resolved_at: None,
            resolution: None,
            resolved_by_message_id: None,
            completion_id: None,
        };
        let revision = Self::next_message_observation_revision(record)?;
        record.updated_at = now;
        record.messages.push_back(Arc::new(message.clone()));
        record
            .message_observation_revisions
            .insert(message.message_id.clone(), revision);
        while record.messages.len() > DEFAULT_MAX_MESSAGES_PER_SESSION {
            if let Some(evicted) = record.messages.pop_front() {
                Self::note_evicted_message_observation(record, &evicted.message_id);
            }
        }
        Ok((message, true))
    }

    pub(super) fn observe_message_acks(
        &mut self,
        session_id: &str,
        message_ids: &[String],
    ) -> super::model::SessionAckObservation {
        let Some(stored) = self.sessions.get_mut(session_id) else {
            return super::model::SessionAckObservation {
                ignored_count: message_ids.len(),
                ..Default::default()
            };
        };
        let Some(record) = stored.hot_mut() else {
            return super::model::SessionAckObservation {
                ignored_count: message_ids.len(),
                ..Default::default()
            };
        };
        let mut outcome = super::model::SessionAckObservation::default();
        let mut seen = std::collections::HashSet::new();
        let now = now_ts();
        for message_id in message_ids {
            if !seen.insert(message_id.as_str()) {
                continue;
            }
            let Some(index) = record.messages.iter().position(|message| {
                message.message_id == *message_id
                    && message.status == SessionMessageStatus::Open
                    && message.kind == super::model::SessionMessageKind::Guidance
                    && message.priority == super::model::SessionMessagePriority::High
                    && message.requires_ack
            }) else {
                outcome.ignored_count += 1;
                continue;
            };
            outcome.accepted_count += 1;
            outcome.accepted_ids.push(message_id.clone());
            if record.messages[index].first_ack_observed_at.is_none() {
                let Ok(revision) = Self::next_message_observation_revision(record) else {
                    outcome.accepted_ids.pop();
                    outcome.accepted_count = outcome.accepted_count.saturating_sub(1);
                    outcome.ignored_count += 1;
                    continue;
                };
                let message = Arc::make_mut(&mut record.messages[index]);
                message.first_ack_observed_at = Some(now);
                record
                    .message_observation_revisions
                    .insert(message.message_id.clone(), revision);
                record.updated_at = now;
                outcome.first_observed_count += 1;
            }
        }
        outcome
    }

    /// Resolve an open message. Already-resolved messages stay resolved
    /// (status is not reopened); an optional new resolution text may update.
    pub(super) fn resolve_message(
        &mut self,
        session_id: &str,
        message_id: &str,
        resolution: Option<String>,
    ) -> Result<(SessionMessage, bool), SessionMessageError> {
        self.touch(session_id);
        let Some(stored) = self.sessions.get_mut(session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let lifecycle = stored.lifecycle();
        if !lifecycle.allows_mutation() {
            return Err(SessionMessageError::SessionClosed { lifecycle });
        }
        let record = stored
            .hot_mut()
            .expect("active session message mutation must stay hot");
        let Some(message_index) = record
            .messages
            .iter()
            .position(|message| message.message_id == message_id)
        else {
            return Err(SessionMessageError::UnknownMessage);
        };
        let resolution = match resolution {
            Some(value) => Some(validate_resolution_text(value)?),
            None => None,
        };
        let changed = record.messages[message_index].status == SessionMessageStatus::Open
            || resolution.as_ref().is_some_and(|resolution| {
                record.messages[message_index].resolution.as_ref() != Some(resolution)
            });
        let revision = if changed {
            Some(Self::next_message_observation_revision(record)?)
        } else {
            None
        };
        let message = Arc::make_mut(&mut record.messages[message_index]);
        if message.status == SessionMessageStatus::Open {
            message.status = SessionMessageStatus::Resolved;
            message.resolved_at = Some(now_ts());
        }
        if resolution.is_some() {
            message.resolution = resolution;
        }
        record.updated_at = now_ts();
        if let Some(revision) = revision {
            record
                .message_observation_revisions
                .insert(message.message_id.clone(), revision);
        }
        Ok((message.clone(), changed))
    }

    pub(super) fn complete_message(
        &mut self,
        input: CompleteSessionMessageInput,
    ) -> Result<CompleteSessionMessageOutcome, SessionMessageError> {
        self.touch(&input.session_id);
        let Some(stored) = self.sessions.get_mut(&input.session_id) else {
            return Err(SessionMessageError::UnknownSession);
        };
        let lifecycle = stored.lifecycle();
        if !lifecycle.allows_mutation() {
            return Err(SessionMessageError::SessionClosed { lifecycle });
        }
        if !is_valid_completion_id(&input.completion_id) {
            return Err(SessionMessageError::InvalidInput(
                "completion identity is invalid".to_string(),
            ));
        }
        if input
            .author_session_id
            .as_deref()
            .is_some_and(|author_session_id| !super::events::is_valid_session_id(author_session_id))
        {
            return Err(SessionMessageError::InvalidInput(
                "author session identity is invalid".to_string(),
            ));
        }
        let answer_text = validate_message_text(input.answer)?;
        let tags = validate_message_tags(input.tags)?;
        let record = stored
            .hot_mut()
            .expect("active session message mutation must stay hot");
        let Some(todo_index) = record
            .messages
            .iter()
            .position(|message| message.message_id == input.message_id)
        else {
            return Err(SessionMessageError::UnknownMessage);
        };
        if record.messages[todo_index].kind != super::model::SessionMessageKind::Todo {
            return Err(SessionMessageError::NotTodo);
        }

        let todo_snapshot = record.messages[todo_index].as_ref().clone();
        if todo_snapshot.status == SessionMessageStatus::Resolved {
            match (
                todo_snapshot.completion_id.as_deref(),
                todo_snapshot.resolved_by_message_id.as_deref(),
            ) {
                (None, None) => {
                    return Err(SessionMessageError::AlreadyCompleted {
                        answer_message_id: None,
                        completion_id: None,
                    });
                }
                (Some(completion_id), Some(answer_message_id)) => {
                    let Some(answer) = record.messages.iter().find(|message| {
                        message.message_id == answer_message_id
                            && message.kind == super::model::SessionMessageKind::Answer
                            && message.reply_to.as_deref() == Some(input.message_id.as_str())
                    }) else {
                        return Err(SessionMessageError::InvalidCompletionState);
                    };
                    if completion_id != input.completion_id {
                        return Err(SessionMessageError::AlreadyCompleted {
                            answer_message_id: Some(answer.message_id.clone()),
                            completion_id: Some(completion_id.to_string()),
                        });
                    }
                    if answer.message != answer_text
                        || answer.tags != tags
                        || answer.priority != input.priority
                    {
                        return Err(SessionMessageError::IdempotencyConflict);
                    }
                    return Ok(CompleteSessionMessageOutcome {
                        todo: todo_snapshot,
                        answer: answer.as_ref().clone(),
                        replayed: true,
                    });
                }
                _ => return Err(SessionMessageError::InvalidCompletionState),
            }
        }
        if todo_snapshot.completion_id.is_some() || todo_snapshot.resolved_by_message_id.is_some() {
            return Err(SessionMessageError::InvalidCompletionState);
        }

        let todo_revision = record
            .message_observation_revision
            .checked_add(1)
            .ok_or(SessionMessageError::InvalidObservationState)?;
        let answer_revision = todo_revision
            .checked_add(1)
            .ok_or(SessionMessageError::InvalidObservationState)?;
        record.message_observation_revision = answer_revision;

        let now = now_ts();
        let answer = SessionMessage {
            message_id: format!("{MESSAGE_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
            session_id: input.session_id.clone(),
            created_at: now,
            kind: super::model::SessionMessageKind::Answer,
            status: SessionMessageStatus::Open,
            priority: input.priority,
            message: answer_text,
            tags,
            reply_to: Some(input.message_id.clone()),
            requires_ack: false,
            first_ack_observed_at: None,
            author_session_id: input.author_session_id,
            resolved_at: None,
            resolution: None,
            resolved_by_message_id: None,
            completion_id: None,
        };
        {
            let todo = Arc::make_mut(&mut record.messages[todo_index]);
            todo.status = SessionMessageStatus::Resolved;
            todo.resolved_at = Some(now);
            todo.resolved_by_message_id = Some(answer.message_id.clone());
            todo.completion_id = Some(input.completion_id);
        }
        record.messages.push_back(Arc::new(answer.clone()));
        record
            .message_observation_revisions
            .insert(input.message_id.clone(), todo_revision);
        record
            .message_observation_revisions
            .insert(answer.message_id.clone(), answer_revision);
        while record.messages.len() > DEFAULT_MAX_MESSAGES_PER_SESSION {
            let protected_answer_id = answer.message_id.as_str();
            let protected_todo_id = input.message_id.as_str();
            let Some(remove_index) = record.messages.iter().position(|message| {
                message.message_id != protected_todo_id && message.message_id != protected_answer_id
            }) else {
                return Err(SessionMessageError::InvalidCompletionState);
            };
            if let Some(evicted) = record.messages.remove(remove_index) {
                Self::note_evicted_message_observation(record, &evicted.message_id);
            }
        }
        record.updated_at = now;
        let todo = record
            .messages
            .iter()
            .find(|message| message.message_id == input.message_id)
            .expect("completed todo retained")
            .as_ref()
            .clone();
        Ok(CompleteSessionMessageOutcome {
            todo,
            answer,
            replayed: false,
        })
    }

    fn next_message_observation_revision(
        record: &mut SessionRecord,
    ) -> Result<u64, SessionMessageError> {
        let revision = record
            .message_observation_revision
            .checked_add(1)
            .ok_or(SessionMessageError::InvalidObservationState)?;
        record.message_observation_revision = revision;
        Ok(revision)
    }

    fn note_evicted_message_observation(record: &mut SessionRecord, message_id: &str) {
        if let Some(revision) = record.message_observation_revisions.remove(message_id) {
            record.message_observation_floor = record.message_observation_floor.max(revision);
        }
    }

    // --- reads / housekeeping ---

    pub(super) fn contains_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub(super) fn session_project(&self, session_id: &str) -> Option<Option<String>> {
        self.sessions
            .get(session_id)
            .map(|record| record.project().map(str::to_string))
    }

    pub(super) fn session_target_authority(
        &self,
        session_id: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        self.sessions.get(session_id).map(|record| {
            (
                record.project().map(str::to_string),
                record.owner_authority_fingerprint().map(str::to_string),
            )
        })
    }

    pub(super) fn guard_state(&self, session_id: &str) -> Option<(SessionMode, SessionGuards)> {
        self.sessions
            .get(session_id)
            .map(StoredSession::mode_guards)
    }

    pub(super) fn lifecycle_state(&self, session_id: &str) -> Option<SessionLifecycle> {
        self.sessions.get(session_id).map(StoredSession::lifecycle)
    }

    fn to_persisted_ledger(&self) -> PersistedSessionLedger {
        let sessions = self
            .lru
            .iter()
            .filter_map(|session_id| self.sessions.get(session_id))
            .map(|record| match record {
                StoredSession::Hot(record) => PersistedSessionSnapshot::Hot(
                    PersistedSessionRecord::from_record(record, self.max_events_per_session),
                ),
                StoredSession::Cold(record) => {
                    PersistedSessionSnapshot::Cold(Arc::clone(&record.raw))
                }
            })
            .collect();
        let mut durable_current_bindings: Vec<PersistedCurrentBinding> = self
            .durable_current_bindings
            .iter()
            .filter(|(_, binding)| {
                self.sessions
                    .get(&binding.session_id)
                    .is_some_and(|session| session.lifecycle().allows_mutation())
            })
            .map(|(binding_key_sha256, binding)| PersistedCurrentBinding {
                binding_key_sha256: binding_key_sha256.clone(),
                session_id: binding.session_id.clone(),
                updated_at: binding.updated_at,
            })
            .collect();
        durable_current_bindings.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.binding_key_sha256.cmp(&right.binding_key_sha256))
        });
        PersistedSessionLedger {
            version: SESSION_LEDGER_VERSION,
            sessions,
            durable_current_bindings: PersistedCurrentBindings {
                records: durable_current_bindings,
                malformed_count: 0,
            },
        }
    }

    pub(super) fn touch(&mut self, session_id: &str) {
        self.lru.retain(|id| id != session_id);
        if self.sessions.contains_key(session_id) {
            self.lru.push_back(session_id.to_string());
        }
    }

    fn enforce_session_bound(&mut self) {
        while self.sessions.len() > self.max_sessions {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.sessions.remove(&oldest);
            self.remove_bindings_for_session(&oldest);
        }
    }

    fn enforce_durable_binding_bound(&mut self) {
        while self.durable_current_bindings.len() > self.max_durable_bindings {
            let Some(oldest_binding_key) = self
                .durable_current_bindings
                .iter()
                .min_by(|(left_key, left), (right_key, right)| {
                    left.updated_at
                        .cmp(&right.updated_at)
                        .then_with(|| left_key.cmp(right_key))
                })
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.durable_current_bindings.remove(&oldest_binding_key);
            self.current_sessions
                .retain(|key, _| key.durable_binding_key() != oldest_binding_key);
        }
    }

    pub(super) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let record = self.sessions.get(session_id)?.hot()?;
        Some(summarize_record(record, limit, None))
    }
}
