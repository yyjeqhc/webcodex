use super::access_control::assert_runner_access;
use super::project_inventory::reconcile_dynamic_projection;
#[cfg(any(test, feature = "root-test-support"))]
use super::validation::validate_id;
use super::validation::validate_project_summary;
use super::{RunnerFeatureSet, RunnerRegistry};
#[cfg(test)]
use std::fmt;
use webcodex_core::runner_protocol::RunnerProjectSummary;

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerLookupError {
    UnknownRunner { client_id: String },
}

#[cfg(test)]
impl fmt::Display for RunnerLookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRunner { client_id } => {
                write!(formatter, "unknown shell client: {client_id}")
            }
        }
    }
}

#[cfg(test)]
impl std::error::Error for RunnerLookupError {}

fn upsert_project_summary(projects: &mut Vec<RunnerProjectSummary>, project: RunnerProjectSummary) {
    if let Some(existing) = projects.iter_mut().find(|p| p.id == project.id) {
        *existing = project;
    } else {
        projects.push(project);
        projects.sort_by(|a, b| a.id.cmp(&b.id));
        projects.dedup_by(|a, b| a.id == b.id);
    }
}

impl RunnerRegistry {
    /// Return an immutable clone of the canonical feature truth for one
    /// registered Runner. This is an internal semantic query, never a wire
    /// projection.
    pub async fn get_runner_feature_set(
        &self,
        client_id: &str,
    ) -> Result<RunnerFeatureSet, String> {
        self.prune_expired_shared_key_runners().await;
        let inner = self.inner.lock().await;
        let runner = inner
            .runners
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {client_id}"))?;
        Ok(runner.runner_features.clone())
    }

    /// Check whether a registered Runner supports a named capability.
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
    pub(crate) async fn runner_supports(
        &self,
        client_id: &str,
        capability: &str,
    ) -> Result<bool, RunnerLookupError> {
        self.prune_expired_shared_key_runners().await;
        let inner = self.inner.lock().await;
        let runner =
            inner
                .runners
                .get(client_id)
                .ok_or_else(|| RunnerLookupError::UnknownRunner {
                    client_id: client_id.to_string(),
                })?;
        Ok(runner.runner_features.supports_wire_name(capability))
    }

    pub async fn runner_supports_for_auth(
        &self,
        client_id: &str,
        capability: &str,
        auth: Option<&crate::RunnerAccess>,
    ) -> Result<bool, String> {
        self.prune_expired_shared_key_runners().await;
        let inner = self.inner.lock().await;
        let runner = inner
            .runners
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        assert_runner_access(auth, runner)?;
        Ok(runner.runner_features.supports_wire_name(capability))
    }

    /// Test-only accessor for projects registered to a runner.
    #[cfg(any(test, feature = "root-test-support"))]
    pub async fn list_runner_projects(
        &self,
        client_id: &str,
    ) -> Result<Vec<RunnerProjectSummary>, String> {
        validate_id(client_id, "client_id")?;
        self.prune_expired_shared_key_runners().await;
        let inner = self.inner.lock().await;
        let Some(runner) = inner.runners.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        Ok(runner.projects.clone())
    }

    /// Insert or replace a single project summary in the cached project list
    /// for `client_id`. Called by the runtime after a successful
    /// `register_project` / `create_project` Runner operation so that
    /// `listProjects` sees the new project immediately, without waiting for
    /// the Runner's next register/poll cycle. If a project with the same id
    /// already exists it is replaced; otherwise the new summary is appended
    /// and the list is re-sorted by id (matching `normalize_project_summaries`).
    pub async fn upsert_runner_project_for_instance(
        &self,
        client_id: &str,
        runner_instance_id: &str,
        project: RunnerProjectSummary,
    ) -> Result<(), String> {
        validate_project_summary(&project).map_err(str::to_string)?;
        let mut inner = self.inner.lock().await;
        let Some(runner) = inner.runners.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if runner.runner_instance_id != runner_instance_id {
            return Err(format!(
                "runner {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        upsert_project_summary(&mut runner.projects, project);
        reconcile_dynamic_projection(runner, crate::registry::now_ts());
        Ok(())
    }

    pub async fn remove_runner_project_for_instance(
        &self,
        client_id: &str,
        runner_instance_id: &str,
        project_id: &str,
    ) -> Result<bool, String> {
        let mut inner = self.inner.lock().await;
        let Some(runner) = inner.runners.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if runner.runner_instance_id != runner_instance_id {
            return Err(format!(
                "runner {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        let before = runner.projects.len();
        runner.projects.retain(|project| project.id != project_id);
        let changed = runner.projects.len() != before;
        reconcile_dynamic_projection(runner, crate::registry::now_ts());
        Ok(changed)
    }

    #[cfg(test)]
    pub async fn upsert_runner_project(
        &self,
        client_id: &str,
        project: RunnerProjectSummary,
    ) -> Result<(), String> {
        let instance = self
            .get_runner_view(client_id)
            .await
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?
            .runner_instance_id;
        self.upsert_runner_project_for_instance(client_id, &instance, project)
            .await
    }
}
