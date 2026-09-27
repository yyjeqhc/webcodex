use super::activity::{ActivityRecorder, NoopActivityRecorder};
#[cfg(feature = "workspace-checkpoints")]
use super::checkpoint;
use super::observations::RuntimeObservations;
use super::permissions::PermissionEvaluator;
use super::runtime_info::RuntimeInfo;
use super::sessions;
use super::SessionShellRegistry;
use crate::runner_http::RunnerRegistry;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::Semaphore;
use uuid::Uuid;

fn new_git_diff_hunks_continuation_mac_key() -> Arc<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.git-diff-hunks.runtime-mac-key.v1\0");
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(Uuid::new_v4().as_bytes());
    Arc::new(hasher.finalize().into())
}

#[cfg(test)]
pub(crate) struct ValidationTerminalReconciliationTestHook {
    reconciliation_attempted: Semaphore,
    pause_next_after_snapshot: AtomicBool,
    snapshot_acquired: Semaphore,
    resume_snapshot: Semaphore,
    snapshot_acquisition_count: AtomicUsize,
}

#[cfg(test)]
impl Default for ValidationTerminalReconciliationTestHook {
    fn default() -> Self {
        Self {
            pause_next_after_snapshot: AtomicBool::new(false),
            reconciliation_attempted: Semaphore::new(0),
            snapshot_acquired: Semaphore::new(0),
            resume_snapshot: Semaphore::new(0),
            snapshot_acquisition_count: AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
impl ValidationTerminalReconciliationTestHook {
    pub(crate) fn before_reconciliation_lock(&self) {
        self.reconciliation_attempted.add_permits(1);
    }

    pub(crate) async fn wait_for_reconciliation_attempt(&self) {
        let permit = self
            .reconciliation_attempted
            .acquire()
            .await
            .expect("validation terminal reconciliation attempt semaphore closed");
        permit.forget();
    }

    pub(crate) fn pause_next_snapshot(&self) {
        assert!(
            !self.pause_next_after_snapshot.swap(true, Ordering::SeqCst),
            "validation terminal snapshot pause already armed"
        );
    }

    pub(crate) async fn after_snapshot_acquired(&self) {
        self.snapshot_acquisition_count
            .fetch_add(1, Ordering::SeqCst);
        self.snapshot_acquired.add_permits(1);
        if self.pause_next_after_snapshot.swap(false, Ordering::SeqCst) {
            let permit = self
                .resume_snapshot
                .acquire()
                .await
                .expect("validation terminal snapshot resume semaphore closed");
            permit.forget();
        }
    }

    pub(crate) async fn wait_for_snapshot_acquired(&self) {
        let permit = self
            .snapshot_acquired
            .acquire()
            .await
            .expect("validation terminal snapshot semaphore closed");
        permit.forget();
    }

    pub(crate) fn resume_snapshot(&self) {
        self.resume_snapshot.add_permits(1);
    }

    pub(crate) fn snapshot_acquisition_count(&self) -> usize {
        self.snapshot_acquisition_count.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct ToolRuntime {
    pub runner_registry: Arc<RunnerRegistry>,
    pub(crate) mcp_gateway: Arc<crate::mcp_gateway::McpGatewayRuntime>,
    pub(crate) plugin_gateway: Arc<crate::plugin_gateway::PluginGatewayRuntime>,
    pub(crate) ssh_resource_gateway: Arc<crate::ssh_resource_gateway::SshResourceGatewayRuntime>,
    pub(crate) coding_agent_runs: Arc<super::coding_agent::CodingAgentServerState>,
    pub runtime_info: Arc<RuntimeInfo>,
    /// Server-side MCP Host timing policy. This adapts MCP waiting only and is
    /// never forwarded to Runner execution.
    pub(crate) mcp_host_policy: crate::mcp_host::McpHostRuntimePolicy,
    #[cfg(feature = "workspace-checkpoints")]
    pub(crate) checkpoint_store: checkpoint::CheckpointStore,
    pub(crate) sessions: sessions::SessionStore,
    pub(crate) session_shells: SessionShellRegistry,
    pub(crate) semantic_navigation_probe_timeout: Duration,
    pub(crate) repository_overview_probe_timeout: Duration,
    /// Process-local model-facing handles for exact full-file read snapshots.
    /// Clones share the registry; a Server runtime restart creates a new epoch.
    pub(crate) read_revisions: Arc<super::read_revisions::ReadRevisionRegistry>,
    pub(crate) read_cache: Arc<super::read_cache::ReadCache>,
    pub(crate) validation_sources: Arc<super::validation_source::ValidationSourceRegistry>,
    /// Process-local Project mutation serialization used only by orchestration
    /// frontends. Direct first-class mutations deliberately bypass this registry.
    #[cfg(feature = "experimental-code-mode")]
    pub(crate) orchestration_mutation_fences:
        Arc<super::orchestration_host::OrchestrationMutationFenceRegistry>,
    /// One deadline shared by every item in a `read_files` batch.
    pub(crate) read_files_deadline: Duration,
    /// One deadline shared by every query in a `search_project_texts` batch.
    pub(crate) search_project_texts_deadline: Duration,
    /// Runtime cap for the effective synchronous grace before a read-only
    /// structured validation promotes to a Job. Production permits the public
    /// maximum; the validation budget selects the canonical default or explicit
    /// caller preference. Tests shrink this cap to exercise handoff without sleeping.
    pub(crate) validation_sync_wait: Duration,
    /// Orders authoritative terminal-Job snapshot acquisition through Session
    /// marker/evidence materialization. Marker eviction interprets absence from
    /// that snapshot as retention exit, so a later snapshot must never commit
    /// before an earlier snapshot has finished using its eviction authority.
    /// Cloned runtimes share this mutex; restart drops all in-flight snapshots.
    pub(crate) validation_terminal_reconciliation: Arc<Mutex<()>>,
    #[cfg(test)]
    pub(crate) validation_terminal_reconciliation_test_hook:
        Arc<ValidationTerminalReconciliationTestHook>,
    /// Runtime cap for the model-requested synchronous grace before typed
    /// process/script Job handoff. Production permits the public maximum;
    /// each call's StructuredExecutionBudget selects the actual wait. Tests
    /// shrink this cap to exercise handoff without sleeping.
    pub(crate) structured_execution_sync_wait: Duration,
    /// Per-runtime secret used only to authenticate opaque committed-range
    /// git_diff_hunks continuation state. Clones share the same key; a runtime
    /// restart intentionally invalidates old committed continuations fail-closed.
    pub(crate) git_diff_hunks_continuation_mac_key: Arc<[u8; 32]>,
    /// Authoritative permission evaluator for this runtime instance.
    /// Resolved once at construction (`WEBCODEX_AUTHORITY_MODE`); dispatch
    /// evaluates once per tool request before mutation.
    pub(crate) permission_evaluator: PermissionEvaluator,
    /// Sink for the workspace activity ledger (mutating tool executions).
    /// No-op unless the host injects a durable recorder.
    pub(crate) activity: Arc<dyn ActivityRecorder>,
    /// Bounded connection/runtime observations such as endpoint activity and
    /// last successful meaningful tool call. Never stores payloads or secrets.
    pub(crate) observations: Arc<RuntimeObservations>,
    /// Process-local payload-free view of currently in-flight MCP Window
    /// requests. It is observability only and intentionally resets on restart.
    pub(crate) window_activity: Arc<super::window_activity::WindowActivityRegistry>,
    /// Fail-open metrics projection. This consumes canonical runtime/transport
    /// facts and has no authority over execution or persistence.
    pub(crate) metrics: Arc<dyn super::runtime_metrics::RuntimeMetrics>,
    /// Durable ActionAudit-backed Window activity query handle. This shares the
    /// normal Server SQLite database and never becomes an authorization store.
    pub(crate) window_activity_db: Option<Arc<crate::Database>>,
    /// Optional Control-owned durable project Memory store. It is injected by
    /// the server from the existing webcodex.db handle; Runner-native project
    /// filesystems never own Memory v1 persistence.
    pub(crate) memory_db: Option<Arc<crate::Database>>,
    /// Durable Server-owned mapping from authenticated caller + short Project ref
    /// to one exact canonical Project incarnation. It grants no authority.
    pub(crate) project_reference_db: Option<Arc<crate::Database>>,
    /// Optional Control-owned durable user-domain store. Durable Agent, Conversation,
    /// AgentTask, and Goal state share this Server SQLite handle while remaining
    /// independent tables, lifecycles, and authority domains.
    pub(crate) communication_db: Option<Arc<crate::Database>>,
    /// Dedicated generic Job-terminal-attention store. This is intentionally
    /// independent of Durable Agent communication identity.
    pub(crate) job_terminal_db: Option<Arc<crate::Database>>,
    /// Process-local Host delivery seam for already-durable terminal events.
    pub(crate) job_terminal_continuations:
        Option<crate::job_terminal_attention::JobTerminalContinuationController>,
    #[cfg(test)]
    pub(crate) job_terminal_registration_test_hook:
        Option<crate::tool_runtime::job_terminal_wait::JobTerminalRegistrationTestHook>,
    /// Optional process-local Host continuation registry/controller. It is
    /// created only when the durable communication database is injected and is
    /// intentionally empty again after process restart.
    pub(crate) agent_continuations: Option<crate::agent_wake::AgentContinuationController>,
    /// Process-local LRU registry of compact observation refs (e.g. `~j4`).
    /// Each ref pins one exact (job_id, observation_token) pair for a specific
    /// principal. Intentionally empty after server restart — the model falls back
    /// to raw job_id + after_observation_token on unknown refs.
    pub(crate) observation_ref_registry:
        Arc<webcodex_core::job_observation::ObservationRefRegistry>,
    /// Process-local salt for content-free Job ergonomics correlation.
    pub(crate) job_ergonomics_salt: Arc<[u8; 16]>,
    /// Process-local, bounded, non-authoritative passive Job attention cursor.
    pub(crate) job_attention_cursor: Arc<super::job_attention::JobAttentionCursor>,
}

impl ToolRuntime {
    pub fn new(runner_registry: Arc<RunnerRegistry>, runtime_info: Arc<RuntimeInfo>) -> Self {
        Self {
            runner_registry,
            mcp_gateway: Arc::new(crate::mcp_gateway::McpGatewayRuntime::default()),
            plugin_gateway: Arc::new(crate::plugin_gateway::PluginGatewayRuntime::default()),
            ssh_resource_gateway: Arc::new(
                crate::ssh_resource_gateway::SshResourceGatewayRuntime::default(),
            ),
            coding_agent_runs: Arc::new(super::coding_agent::CodingAgentServerState::default()),
            runtime_info,
            mcp_host_policy: crate::mcp_host::McpHostRuntimePolicy::default(),
            #[cfg(feature = "workspace-checkpoints")]
            checkpoint_store: checkpoint::CheckpointStore::default(),
            sessions: sessions::SessionStore::default(),
            session_shells: SessionShellRegistry::default(),
            semantic_navigation_probe_timeout:
                super::semantic_navigation::DEFAULT_SEMANTIC_NAVIGATION_PROBE_TIMEOUT,
            repository_overview_probe_timeout:
                super::coding_task::DEFAULT_REPOSITORY_OVERVIEW_PROBE_TIMEOUT,
            read_revisions: Arc::new(super::read_revisions::ReadRevisionRegistry::new()),
            read_cache: Arc::new(super::read_cache::ReadCache::default()),
            validation_sources: Arc::new(
                super::validation_source::ValidationSourceRegistry::default(),
            ),
            #[cfg(feature = "experimental-code-mode")]
            orchestration_mutation_fences: Arc::new(
                super::orchestration_host::OrchestrationMutationFenceRegistry::default(),
            ),
            read_files_deadline: super::read_files::DEFAULT_READ_FILES_DEADLINE,
            search_project_texts_deadline:
                super::search_project_texts::DEFAULT_SEARCH_PROJECT_TEXTS_DEADLINE,
            validation_sync_wait: Duration::from_secs(
                super::structured_execution::STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS,
            ),
            validation_terminal_reconciliation: Arc::new(Mutex::new(())),
            #[cfg(test)]
            validation_terminal_reconciliation_test_hook: Arc::new(
                ValidationTerminalReconciliationTestHook::default(),
            ),
            structured_execution_sync_wait: Duration::from_secs(
                super::structured_execution::STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS,
            ),
            git_diff_hunks_continuation_mac_key: new_git_diff_hunks_continuation_mac_key(),
            permission_evaluator: PermissionEvaluator::from_env(),
            activity: Arc::new(NoopActivityRecorder),
            observations: Arc::new(RuntimeObservations::default()),
            window_activity: Arc::new(super::window_activity::WindowActivityRegistry::default()),
            metrics: Arc::new(super::runtime_metrics::TracingRuntimeMetrics),
            window_activity_db: None,
            memory_db: None,
            project_reference_db: None,
            communication_db: None,
            job_terminal_db: None,
            job_terminal_continuations: None,
            #[cfg(test)]
            job_terminal_registration_test_hook: None,
            agent_continuations: None,
            observation_ref_registry: Arc::new(
                webcodex_core::job_observation::ObservationRefRegistry::default(),
            ),
            job_ergonomics_salt: Arc::new(*uuid::Uuid::new_v4().as_bytes()),
            job_attention_cursor: Arc::new(super::job_attention::JobAttentionCursor::default()),
        }
    }

    /// Attach a durable workspace-activity recorder (server wiring).
    pub fn with_activity_recorder(mut self, recorder: Arc<dyn ActivityRecorder>) -> Self {
        self.activity = recorder;
        self
    }

    pub(crate) fn with_window_activity_database(mut self, db: Arc<crate::Database>) -> Self {
        self.window_activity_db = Some(db);
        self
    }

    pub(crate) fn with_mcp_host_policy(
        mut self,
        policy: crate::mcp_host::McpHostRuntimePolicy,
    ) -> Self {
        self.mcp_host_policy = policy;
        self
    }

    pub(crate) fn window_activity_registry(
        &self,
    ) -> Arc<super::window_activity::WindowActivityRegistry> {
        self.window_activity.clone()
    }

    pub(crate) fn with_memory_database(mut self, db: Arc<crate::Database>) -> Self {
        self.memory_db = Some(db);
        self
    }

    pub(crate) fn with_project_reference_database(mut self, db: Arc<crate::Database>) -> Self {
        self.project_reference_db = Some(db);
        self
    }

    pub(crate) fn with_communication_database(mut self, db: Arc<crate::Database>) -> Self {
        self.agent_continuations = Some(crate::agent_wake::AgentContinuationController::new(
            db.clone(),
        ));
        self.communication_db = Some(db);
        self
    }

    pub(crate) fn with_job_terminal_attention(
        mut self,
        db: Arc<crate::Database>,
        controller: crate::job_terminal_attention::JobTerminalContinuationController,
    ) -> Self {
        self.job_terminal_db = Some(db);
        self.job_terminal_continuations = Some(controller);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_job_terminal_registration_test_hook(
        mut self,
        hook: crate::tool_runtime::job_terminal_wait::JobTerminalRegistrationTestHook,
    ) -> Self {
        self.job_terminal_registration_test_hook = Some(hook);
        self
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests() -> Self {
        Self::new_for_tests_with_runner_registry(Arc::new(RunnerRegistry::default()))
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests_with_runner_registry(runner_registry: Arc<RunnerRegistry>) -> Self {
        Self::new(runner_registry, Arc::new(RuntimeInfo::default()))
    }

    pub fn with_session_ledger(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions = sessions::SessionStore::with_persistence_limits(
            path,
            sessions::DEFAULT_MAX_SESSIONS,
            sessions::DEFAULT_MAX_RETAINED_CLOSED_SESSIONS,
            sessions::DEFAULT_MAX_EVENTS_PER_SESSION,
        );
        self
    }

    pub(crate) fn workflow_sessions_console_list(
        &self,
        project: &str,
        limit: Option<usize>,
    ) -> sessions::WorkflowSessionConsoleList {
        self.sessions
            .console_list_for_project(project, limit, sessions::console_validation_hooks())
    }

    pub(crate) fn workflow_sessions_console_lists(
        &self,
        projects: &[&str],
        limit: Option<usize>,
    ) -> std::collections::HashMap<String, sessions::WorkflowSessionConsoleList> {
        self.sessions.console_lists_for_projects(
            projects,
            limit,
            sessions::console_validation_hooks(),
        )
    }

    pub(crate) fn workflow_session_console_detail(
        &self,
        project: &str,
        session_id: &str,
        limit: Option<usize>,
    ) -> Option<sessions::WorkflowSessionConsoleDetail> {
        self.sessions.console_detail_for_project(
            project,
            session_id,
            limit,
            sessions::console_validation_hooks(),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_semantic_navigation_probe_timeout(mut self, timeout: Duration) -> Self {
        self.semantic_navigation_probe_timeout = timeout;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_repository_overview_probe_timeout(mut self, timeout: Duration) -> Self {
        self.repository_overview_probe_timeout = timeout;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_read_files_deadline(mut self, deadline: Duration) -> Self {
        self.read_files_deadline = deadline;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_search_project_texts_deadline(mut self, deadline: Duration) -> Self {
        self.search_project_texts_deadline = deadline;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_validation_sync_wait(mut self, wait: Duration) -> Self {
        self.validation_sync_wait = wait;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_structured_execution_sync_wait(mut self, wait: Duration) -> Self {
        self.structured_execution_sync_wait = wait;
        self
    }

    /// Replace the permission evaluator (tests: mode matrix / single-eval counters).
    #[cfg(test)]
    pub(crate) fn with_permission_evaluator(mut self, evaluator: PermissionEvaluator) -> Self {
        self.permission_evaluator = evaluator;
        self
    }
}
