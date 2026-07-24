use super::auth::assert_shell_client_access;
use super::validation::validate_id;
use super::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentProjectSummary, ShellClientCapabilities, SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS, SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE, SHELL_CLIENT_CAPABILITY_GIT, SHELL_CLIENT_CAPABILITY_JOBS,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION, SHELL_CLIENT_CAPABILITY_SHELL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_JOBS,
};

fn capability_enabled(caps: &ShellClientCapabilities, capability: &str) -> bool {
    match capability {
        SHELL_CLIENT_CAPABILITY_SHELL => caps.shell,
        SHELL_CLIENT_CAPABILITY_FILE_READ => caps.file_read,
        SHELL_CLIENT_CAPABILITY_FILE_WRITE => caps.file_write,
        SHELL_CLIENT_CAPABILITY_GIT => caps.git,
        SHELL_CLIENT_CAPABILITY_JOBS => caps.jobs,
        SHELL_CLIENT_CAPABILITY_ASYNC_JOBS => caps.async_jobs,
        SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS => caps.async_shell_jobs,
        SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_JOBS => caps.structured_validation_jobs,
        SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION => caps.lsp_read_only_navigation,
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
    /// Errors with a structured `unknown shell client` message when the
    /// client is not registered.
    pub async fn get_client_capabilities(
        &self,
        client_id: &str,
    ) -> Result<ShellClientCapabilities, String> {
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        Ok(client.capabilities.clone())
    }

    /// Check whether a registered agent client supports a named capability.
    /// Recognized capability names: `shell`, `file_read`, `file_write`,
    /// `git`, `jobs`, `async_jobs`, `async_shell_jobs`,
    /// `structured_validation_jobs`,
    /// `lsp_read_only_navigation`. Unknown capability names return `false`.
    pub async fn client_supports(&self, client_id: &str, capability: &str) -> Result<bool, String> {
        let caps = self.get_client_capabilities(client_id).await?;
        Ok(capability_enabled(&caps, capability))
    }

    pub(crate) async fn client_supports_for_auth(
        &self,
        client_id: &str,
        capability: &str,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<bool, String> {
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        assert_shell_client_access(auth, client)?;
        Ok(capability_enabled(&client.capabilities, capability))
    }

    /// List the projects registered for a given shell client. Currently only
    /// exercised by tests; kept as a public accessor of the registry API.
    #[allow(dead_code)]
    pub async fn list_client_projects(
        &self,
        client_id: &str,
    ) -> Result<Vec<ShellAgentProjectSummary>, String> {
        validate_id(client_id, "client_id")?;
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
        upsert_project_summary(&mut client.projects, project);
        Ok(())
    }
}
