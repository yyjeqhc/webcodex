use super::access_control::assert_runner_access;
use super::project_inventory::reconcile_dynamic_projection;
#[cfg(any(test, feature = "root-test-support"))]
use super::validation::validate_id;
use super::validation::validate_project_summary;
use super::{RunnerFeature, RunnerFeatureSet, RunnerRegistry};
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

// A late Runner response must never overwrite a newer Server Project view.
fn update_cached_allow_patch(
    project: &mut RunnerProjectSummary,
    expected_revision: &str,
    revision: &str,
    root_fingerprint: &str,
    allow_patch: bool,
) -> Result<(), String> {
    if project.revision.as_deref() != Some(expected_revision)
        || project.root_fingerprint.as_deref() != Some(root_fingerprint)
    {
        return Err("Project revision or root identity changed".into());
    }
    project.allow_patch = allow_patch;
    project.revision = Some(revision.to_string());
    project.updated_at = crate::registry::now_ts();
    Ok(())
}

#[cfg(test)]
mod allow_patch_projection_tests {
    use super::*;

    #[test]
    fn update_only_one_projection_field_and_reject_stale_or_retargeted() {
        let mut project: RunnerProjectSummary = serde_json::from_value(serde_json::json!({
            "id": "demo", "name": "Keep", "path": "/source",
            "allow_patch": true, "revision": "sha256:old",
            "root_fingerprint": "sha256:root", "hooks": ["before_edit"],
            "git_branch": "main", "updated_at": 1, "shell_profile": "private-profile"
        }))
        .unwrap();
        update_cached_allow_patch(
            &mut project,
            "sha256:old",
            "sha256:new",
            "sha256:root",
            false,
        )
        .unwrap();
        assert!(!project.allow_patch);
        assert_eq!(project.revision.as_deref(), Some("sha256:new"));
        assert_eq!(project.name.as_deref(), Some("Keep"));
        assert_eq!(project.hooks, vec!["before_edit".to_string()]);
        assert_eq!(project.shell_profile.as_deref(), Some("private-profile"));
        assert_eq!(project.git_branch.as_deref(), Some("main"));
        let snapshot = serde_json::to_value(&project).unwrap();
        assert!(update_cached_allow_patch(
            &mut project,
            "sha256:old",
            "sha256:bad",
            "sha256:root",
            true
        )
        .is_err());
        assert!(update_cached_allow_patch(
            &mut project,
            "sha256:new",
            "sha256:bad",
            "sha256:retarget",
            true
        )
        .is_err());
        assert_eq!(serde_json::to_value(&project).unwrap(), snapshot);
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
        Ok(RunnerFeature::from_wire_name(capability)
            .is_some_and(|feature| runner.supports(feature)))
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
        Ok(RunnerFeature::from_wire_name(capability)
            .is_some_and(|feature| runner.supports(feature)))
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
    /// `list_projects` sees the new project immediately, without waiting for
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

    /// CAS only the requested permission on the live Server projection.
    pub async fn update_runner_project_allow_patch_for_instance(
        &self,
        client_id: &str,
        runner_instance_id: &str,
        project_id: &str,
        expected_revision: &str,
        revision: &str,
        root_fingerprint: &str,
        allow_patch: bool,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let runner = inner
            .runners
            .get_mut(client_id)
            .ok_or_else(|| format!("unknown shell client: {client_id}"))?;
        if runner.runner_instance_id != runner_instance_id {
            return Err("runner instance changed during Project update".into());
        }
        let project = runner
            .projects
            .iter_mut()
            .find(|p| p.id == project_id)
            .ok_or_else(|| "Project removed during update".to_string())?;
        update_cached_allow_patch(
            project,
            expected_revision,
            revision,
            root_fingerprint,
            allow_patch,
        )?;
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
