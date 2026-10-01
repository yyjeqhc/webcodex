//! Authorized skills routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_skills_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
    ) -> ToolResult {
        match call {
            ToolCall::SkillLoad { name, .. } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("load_skill requires a resolved Project"),
                };
                self.skill_load(&project, name, auth).await
            }

            ToolCall::SkillList {
                query,
                offset,
                limit,
                expected_catalog_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("list_skills requires a resolved Project"),
                };
                self.skill_list(
                    &project,
                    query,
                    offset,
                    limit,
                    expected_catalog_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillReadFile {
                skill_id,
                path,
                start_line,
                limit,
                expected_definition_revision,
                expected_package_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("read_skill_file requires a resolved Project"),
                };
                self.skill_read_file(
                    &project,
                    skill_id,
                    path,
                    start_line,
                    limit,
                    expected_definition_revision,
                    expected_package_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillVersions {
                skill_key,
                offset,
                limit,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err("list_skill_versions requires a resolved Project")
                    }
                };
                self.skill_versions(&project, skill_key, offset, limit, auth)
                    .await
            }

            ToolCall::SkillInstall {
                skill_key,
                artifact_path,
                expected_artifact_sha256,
                idempotency_key,
                activate,
                expected_state_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("install_skill requires a resolved Project"),
                };
                self.skill_install(
                    &project,
                    skill_key,
                    artifact_path,
                    expected_artifact_sha256,
                    idempotency_key,
                    activate.unwrap_or(false),
                    expected_state_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillActivate {
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("activate_skill requires a resolved Project"),
                };
                self.skill_activate(
                    &project,
                    skill_key,
                    package_revision,
                    expected_state_revision,
                    idempotency_key,
                    auth,
                )
                .await
            }

            ToolCall::SkillRemoveRevision {
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err("remove_skill_revision requires a resolved Project")
                    }
                };
                self.skill_remove_revision(
                    &project,
                    skill_key,
                    package_revision,
                    expected_state_revision,
                    idempotency_key,
                    auth,
                )
                .await
            }
            _ => ToolResult::err("tool does not belong to the skills dispatch family"),
        }
    }
}
