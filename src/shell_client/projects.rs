use super::auth::assert_shell_client_access;
use super::project_inventory::reconcile_dynamic_projection;
#[cfg(test)]
use super::validation::validate_id;
use super::validation::validate_project_summary;
use super::{RunnerFeatureSet, ShellClientRegistry};
use crate::shell_protocol::ShellAgentProjectSummary;
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
    /// Return an immutable clone of the canonical feature truth for one
    /// registered Runner. This is an internal semantic query, never a wire
    /// projection.
    pub(crate) async fn get_client_feature_set(
        &self,
        client_id: &str,
    ) -> Result<RunnerFeatureSet, ShellClientLookupError> {
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let client =
            inner
                .clients
                .get(client_id)
                .ok_or_else(|| ShellClientLookupError::UnknownClient {
                    client_id: client_id.to_string(),
                })?;
        Ok(client.runner_features.clone())
    }

    /// Check whether a registered agent client supports a named capability.
    /// Recognized capability names: `shell`, `file_read`, `file_write`,
    /// `structured_file_delete`, `apply_text_edit_occurrence`,
    /// `git`, `jobs`, `async_jobs`, `async_shell_jobs`,
    /// `ssh_shell`, `persistent_shell`, `structured_validation_argv`,
    /// `structured_cargo_test_count_assertion`, `structured_go_test_json`,
    /// `structured_go_test_tool`,
    /// `structured_process_argv`, `structured_script_payload`,
    /// `internal_posix_script`, `structured_execution_jobs`,
    /// `lsp_read_only_navigation`, `lsp_call_hierarchy`, `project_lifecycle`,
    /// `project_path_registration`, `job_state_reconciliation`, `coding_agent_runs`,
    /// `computer_observe`. Unknown capability names return `false`.
    #[cfg(test)]
    pub(crate) async fn client_supports(
        &self,
        client_id: &str,
        capability: &str,
    ) -> Result<bool, ShellClientLookupError> {
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let client =
            inner
                .clients
                .get(client_id)
                .ok_or_else(|| ShellClientLookupError::UnknownClient {
                    client_id: client_id.to_string(),
                })?;
        Ok(client.runner_features.supports_wire_name(capability))
    }

    pub(crate) async fn client_supports_for_auth(
        &self,
        client_id: &str,
        capability: &str,
        auth: Option<&webcodex_runner_registry::RunnerAccess>,
    ) -> Result<bool, String> {
        self.prune_expired_shared_key_clients().await;
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        assert_shell_client_access(auth, client)?;
        Ok(client.runner_features.supports_wire_name(capability))
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
    pub async fn upsert_client_project_for_instance(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        project: ShellAgentProjectSummary,
    ) -> Result<(), String> {
        validate_project_summary(&project).map_err(str::to_string)?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        upsert_project_summary(&mut client.projects, project);
        reconcile_dynamic_projection(client, super::now_ts());
        Ok(())
    }

    pub async fn remove_client_project_for_instance(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        project_id: &str,
    ) -> Result<bool, String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        let before = client.projects.len();
        client.projects.retain(|project| project.id != project_id);
        let changed = client.projects.len() != before;
        reconcile_dynamic_projection(client, super::now_ts());
        Ok(changed)
    }

    #[cfg(test)]
    pub async fn upsert_client_project(
        &self,
        client_id: &str,
        project: ShellAgentProjectSummary,
    ) -> Result<(), String> {
        let instance = self
            .get_client_view(client_id)
            .await
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?
            .agent_instance_id;
        self.upsert_client_project_for_instance(client_id, &instance, project)
            .await
    }
}
