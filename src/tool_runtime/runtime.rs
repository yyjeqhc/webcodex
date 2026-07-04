use super::checkpoint;
use super::local_jobs::{LocalJobKiller, LocalJobRecord, SystemJobKiller};
use super::runtime_info::RuntimeInfo;
use super::sessions;
use crate::config::CodexConfig;
use crate::projects::ProjectsState;
use crate::shell_client::ShellClientRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ToolRuntime {
    pub projects: Arc<ProjectsState>,
    pub shell_clients: Arc<ShellClientRegistry>,
    #[allow(dead_code)]
    pub codex: Arc<CodexConfig>,
    pub runtime_info: Arc<RuntimeInfo>,
    pub(crate) checkpoint_store: checkpoint::CheckpointStore,
    pub(crate) sessions: sessions::SessionStore,
    pub(crate) local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
    pub(crate) job_killer: Arc<dyn LocalJobKiller>,
}

impl ToolRuntime {
    pub fn new(
        projects: Arc<ProjectsState>,
        shell_clients: Arc<ShellClientRegistry>,
        codex: Arc<CodexConfig>,
        runtime_info: Arc<RuntimeInfo>,
    ) -> Self {
        Self {
            projects,
            shell_clients,
            codex,
            runtime_info,
            checkpoint_store: checkpoint::CheckpointStore::default(),
            sessions: sessions::SessionStore::default(),
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
        }
    }

    pub fn with_session_ledger(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions = sessions::SessionStore::with_persistence(
            path,
            sessions::DEFAULT_MAX_SESSIONS,
            sessions::DEFAULT_MAX_EVENTS_PER_SESSION,
        );
        self
    }
}
