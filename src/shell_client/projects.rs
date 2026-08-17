use super::auth::assert_shell_client_access;
#[cfg(test)]
use super::validation::validate_id;
use super::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentProjectSummary, ShellClientCapabilities,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA, SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL, SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT, SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION, SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE, SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE, SHELL_CLIENT_CAPABILITY_GIT,
    SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT, SHELL_CLIENT_CAPABILITY_JOBS,
    SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION, SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION, SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL,
    SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE, SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION,
    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS, SHELL_CLIENT_CAPABILITY_SHELL,
    SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL, SHELL_CLIENT_CAPABILITY_SSH_SHELL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV,
};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellClientLookupError {
    UnknownClient { client_id: String },
}

impl fmt::Display for ShellClientLookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownClient { client_id } => {
                write!(formatter, "unknown shell client: {client_id}")
            }
        }
    }
}

impl std::error::Error for ShellClientLookupError {}

pub(super) fn capability_enabled(caps: &ShellClientCapabilities, capability: &str) -> bool {
    match capability {
        SHELL_CLIENT_CAPABILITY_SHELL => caps.shell,
        SHELL_CLIENT_CAPABILITY_FILE_READ => caps.file_read,
        SHELL_CLIENT_CAPABILITY_FILE_WRITE => caps.file_write,
        SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ => caps.artifact_export_chunk_read,
        SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA => {
            caps.artifact_export_streaming_metadata
        }
        SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE => caps.structured_file_delete,
        SHELL_CLIENT_CAPABILITY_GIT => caps.git,
        SHELL_CLIENT_CAPABILITY_JOBS => caps.jobs,
        SHELL_CLIENT_CAPABILITY_ASYNC_JOBS => caps.async_jobs,
        SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS => caps.async_shell_jobs,
        SHELL_CLIENT_CAPABILITY_SSH_SHELL => caps.ssh_shell,
        SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL => caps.persistent_shell,
        SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL => caps.ssh_persistent_shell,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV => caps.structured_validation_argv,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON => caps.structured_go_test_json,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL => caps.structured_go_test_tool,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES => caps.structured_go_test_packages,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV => caps.structured_process_argv,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD => caps.structured_script_payload,
        SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT => caps.internal_posix_script,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS => caps.structured_execution_jobs,
        SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION => caps.lsp_read_only_navigation,
        SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY => caps.lsp_call_hierarchy,
        SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS => caps.sandbox_inspect_commands,
        SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE => caps.project_lifecycle,
        SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION => caps.project_path_registration,
        SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE => caps.computer_observe,
        SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION => caps.computer_snapshot_region,
        SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE => {
            caps.computer_accessibility_observe
        }
        SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE => caps.computer_element_state,
        SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL => caps.computer_control,
        SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT => caps.computer_scroll_to_element,
        SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT => caps.computer_key_input,
        SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE => caps.computer_window_activate,
        SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT => caps.computer_text_input,
        SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION => caps.job_state_reconciliation,
        _ => false,
    }
}

fn upsert_project_summary(
    projects: &mut Vec<ShellAgentProjectSummary>,
    project: ShellAgentProjectSummary,
) {
    if let Some(existing) = projects.iter_mut().find(|p| p.id == project.id) {
        *existing = project;
    } else {
        projects.push(project);
        projects.sort_by(|a, b| a.id.cmp(&b.id));
        projects.dedup_by(|a, b| a.id == b.id);
    }
}

impl ShellClientRegistry {
    /// Return the capabilities advertised by a registered agent client.
    /// Returns a typed lookup error when the client is not registered.
    pub(crate) async fn get_client_capabilities(
        &self,
        client_id: &str,
    ) -> Result<ShellClientCapabilities, ShellClientLookupError> {
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let client =
            inner
                .clients
                .get(client_id)
                .ok_or_else(|| ShellClientLookupError::UnknownClient {
                    client_id: client_id.to_string(),
                })?;
        Ok(client.capabilities.clone())
    }

    /// Check whether a registered agent client supports a named capability.
    /// Recognized capability names: `shell`, `file_read`, `file_write`,
    /// `structured_file_delete`,
    /// `git`, `jobs`, `async_jobs`, `async_shell_jobs`,
    /// `ssh_shell`, `persistent_shell`, `structured_validation_argv`,
    /// `structured_go_test_json`, `structured_go_test_tool`,
    /// `structured_process_argv`, `structured_script_payload`,
    /// `internal_posix_script`, `structured_execution_jobs`,
    /// `lsp_read_only_navigation`, `lsp_call_hierarchy`,
    /// `sandbox_inspect_commands`, `project_lifecycle`,
    /// `project_path_registration`, `job_state_reconciliation`, `computer_observe`. Unknown capability
    /// names return `false`.
    #[cfg(test)]
    pub(crate) async fn client_supports(
        &self,
        client_id: &str,
        capability: &str,
    ) -> Result<bool, ShellClientLookupError> {
        let caps = self.get_client_capabilities(client_id).await?;
        Ok(capability_enabled(&caps, capability))
    }

    pub(crate) async fn client_supports_for_auth(
        &self,
        client_id: &str,
        capability: &str,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<bool, String> {
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        assert_shell_client_access(auth, client)?;
        Ok(capability_enabled(&client.capabilities, capability))
    }

    /// Test-only accessor for projects registered to a shell client.
    #[cfg(test)]
    pub async fn list_client_projects(
        &self,
        client_id: &str,
    ) -> Result<Vec<ShellAgentProjectSummary>, String> {
        validate_id(client_id, "client_id")?;
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        Ok(client.projects.clone())
    }

    /// Insert or replace a single project summary in the cached project list
    /// for `client_id`. Called by the runtime after a successful
    /// `register_project` / `create_project` agent operation so that
    /// `listProjects` sees the new project immediately, without waiting for
    /// the agent's next register/poll cycle. If a project with the same id
    /// already exists it is replaced; otherwise the new summary is appended
    /// and the list is re-sorted by id (matching `normalize_project_summaries`).
    pub async fn upsert_client_project(
        &self,
        client_id: &str,
        project: ShellAgentProjectSummary,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if client.projects.len() >= super::MAX_RUNNER_PROJECT_SUMMARIES
            && !client
                .projects
                .iter()
                .any(|existing| existing.id == project.id)
        {
            return Err(format!(
                "runner project summary limit reached (maximum {} projects)",
                super::MAX_RUNNER_PROJECT_SUMMARIES
            ));
        }
        upsert_project_summary(&mut client.projects, project);
        Ok(())
    }

    pub async fn remove_client_project(
        &self,
        client_id: &str,
        project_id: &str,
    ) -> Result<bool, String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        let before = client.projects.len();
        client.projects.retain(|project| project.id != project_id);
        Ok(client.projects.len() != before)
    }
}
