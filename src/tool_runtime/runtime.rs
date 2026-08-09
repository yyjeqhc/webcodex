use super::activity::{ActivityRecorder, NoopActivityRecorder};
use super::checkpoint;
use super::local_jobs::{LocalJobKiller, LocalJobRecord, SystemJobKiller};
use super::observations::RuntimeObservations;
use super::permissions::PermissionEvaluator;
use super::runtime_info::RuntimeInfo;
use super::sessions;
use super::SessionShellRegistry;
use crate::config::CodexConfig;
use crate::shell_client::ShellClientRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ToolRuntime {
    pub shell_clients: Arc<ShellClientRegistry>,
    #[allow(dead_code)]
    pub codex: Arc<CodexConfig>,
    pub runtime_info: Arc<RuntimeInfo>,
    model_surface: crate::model_surface::ModelSurface,
    pub(crate) checkpoint_store: checkpoint::CheckpointStore,
    pub(crate) sessions: sessions::SessionStore,
    pub(crate) session_shells: SessionShellRegistry,
    pub(crate) local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
    pub(crate) job_killer: Arc<dyn LocalJobKiller>,
    pub(crate) semantic_navigation_probe_timeout: Duration,
    pub(crate) repository_overview_probe_timeout: Duration,
    /// One deadline shared by every item in a `read_files` batch.
    pub(crate) read_files_deadline: Duration,
    /// One deadline shared by every query in a `search_project_texts` batch.
    pub(crate) search_project_texts_deadline: Duration,
    /// Internal synchronous wait window for a read-only structured validation
    /// before it promotes to a Job. Defaults to `SYNC_VALIDATION_WAIT_SECS`;
    /// tests shrink it so the handoff path can be exercised without sleeping.
    pub(crate) validation_sync_wait: Duration,
    /// Internal synchronous grace for typed process/script Jobs. It controls
    /// only when the existing execution is exposed, never its total timeout.
    pub(crate) structured_execution_sync_wait: Duration,
    /// Authoritative permission evaluator for this runtime instance.
    /// Resolved once at construction (`WEBCODEX_AUTHORITY_MODE`); dispatch
    /// evaluates once per tool request before mutation.
    pub(crate) permission_evaluator: PermissionEvaluator,
    /// Sink for the workspace activity ledger (mutating tool executions).
    /// No-op unless the host injects a durable recorder.
    pub(crate) activity: Arc<dyn ActivityRecorder>,
    /// Cross-surface connection observations (connector endpoint activity,
    /// last successful meaningful tool call). Shared with the connector
    /// runtime; never stores payloads or secrets.
    pub(crate) observations: Arc<RuntimeObservations>,
}

impl ToolRuntime {
    pub fn new(
        shell_clients: Arc<ShellClientRegistry>,
        codex: Arc<CodexConfig>,
        runtime_info: Arc<RuntimeInfo>,
    ) -> Self {
        Self {
            shell_clients,
            codex,
            runtime_info,
            model_surface: crate::model_surface::ModelSurface::LocalCoding,
            checkpoint_store: checkpoint::CheckpointStore::default(),
            sessions: sessions::SessionStore::default(),
            session_shells: SessionShellRegistry::default(),
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
            semantic_navigation_probe_timeout:
                super::semantic_navigation::DEFAULT_SEMANTIC_NAVIGATION_PROBE_TIMEOUT,
            repository_overview_probe_timeout:
                super::coding_task::DEFAULT_REPOSITORY_OVERVIEW_PROBE_TIMEOUT,
            read_files_deadline: super::read_files::DEFAULT_READ_FILES_DEADLINE,
            search_project_texts_deadline:
                super::search_project_texts::DEFAULT_SEARCH_PROJECT_TEXTS_DEADLINE,
            validation_sync_wait: Duration::from_secs(super::helpers::SYNC_VALIDATION_WAIT_SECS),
            structured_execution_sync_wait: Duration::from_secs(
                super::structured_execution::STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
            ),
            permission_evaluator: PermissionEvaluator::from_env(),
            activity: Arc::new(NoopActivityRecorder),
            observations: Arc::new(RuntimeObservations::default()),
        }
    }

    pub(crate) fn with_model_surface(
        mut self,
        model_surface: crate::model_surface::ModelSurface,
    ) -> Self {
        self.model_surface = model_surface;
        self
    }

    pub(crate) fn model_surface(&self) -> crate::model_surface::ModelSurface {
        self.model_surface
    }

    /// Attach a durable workspace-activity recorder (server wiring).
    pub fn with_activity_recorder(mut self, recorder: Arc<dyn ActivityRecorder>) -> Self {
        self.activity = recorder;
        self
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests() -> Self {
        Self::new_for_tests_with_shell_clients(Arc::new(ShellClientRegistry::default()))
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests_with_shell_clients(
        shell_clients: Arc<ShellClientRegistry>,
    ) -> Self {
        Self::new(
            shell_clients,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    pub fn with_session_ledger(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions = sessions::SessionStore::with_persistence(
            path,
            sessions::DEFAULT_MAX_SESSIONS,
            sessions::DEFAULT_MAX_EVENTS_PER_SESSION,
        );
        self
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

    /// Replace the in-memory session store with one capped at `max_events_per_session`
    /// events, so tests can exercise ledger truncation without recording hundreds of
    /// events. Sessions are still durable-exact within the in-memory store.
    #[cfg(test)]
    pub(crate) fn with_session_event_cap(mut self, max_events_per_session: usize) -> Self {
        self.sessions = sessions::SessionStore::new_in_memory(
            sessions::DEFAULT_MAX_SESSIONS,
            max_events_per_session,
        );
        self
    }

    /// Replace the permission evaluator (tests: mode matrix / single-eval counters).
    #[cfg(test)]
    pub(crate) fn with_permission_evaluator(mut self, evaluator: PermissionEvaluator) -> Self {
        self.permission_evaluator = evaluator;
        self
    }
}
