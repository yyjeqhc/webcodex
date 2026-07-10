use super::checkpoint;
use super::local_jobs::{LocalJobKiller, LocalJobRecord, SystemJobKiller};
use super::runtime_info::RuntimeInfo;
use super::sessions;
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
    pub(crate) checkpoint_store: checkpoint::CheckpointStore,
    pub(crate) sessions: sessions::SessionStore,
    pub(crate) local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
    pub(crate) job_killer: Arc<dyn LocalJobKiller>,
    pub(crate) semantic_navigation_probe_timeout: Duration,
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
            checkpoint_store: checkpoint::CheckpointStore::default(),
            sessions: sessions::SessionStore::default(),
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
            semantic_navigation_probe_timeout:
                super::semantic_navigation::DEFAULT_SEMANTIC_NAVIGATION_PROBE_TIMEOUT,
        }
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
}
