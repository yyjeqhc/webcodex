//! Authorized memory routing; outer governance owns admission.

use super::*;

impl ToolRuntime {
    pub(super) async fn dispatch_memory_authorized(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
    ) -> ToolResult {
        match call {
            ToolCall::MemorySearch {
                query,
                tags,
                offset,
                limit,
                expected_catalog_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_search requires a resolved Project"),
                };
                self.memory_search(
                    &project,
                    query,
                    tags,
                    offset,
                    limit,
                    expected_catalog_revision,
                )
            }

            ToolCall::MemoryRead {
                memory_key,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_read requires a resolved Project"),
                };
                self.memory_read(&project, memory_key, expected_revision)
            }

            ToolCall::MemorySet {
                memory_key,
                summary,
                body,
                priority,
                bootstrap,
                tags,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_set requires a resolved Project"),
                };
                self.memory_set(
                    &project,
                    memory_key,
                    summary,
                    body,
                    priority,
                    bootstrap,
                    tags,
                    expected_revision,
                    auth,
                )
            }

            ToolCall::MemoryDelete {
                memory_key,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_delete requires a resolved Project"),
                };
                self.memory_delete(&project, memory_key, expected_revision)
            }

            ToolCall::MemoryScopeList { offset, limit } => {
                self.memory_scope_list(auth, offset, limit).await
            }

            ToolCall::MemoryScopePurge {
                memory_scope_id,
                expected_catalog_revision,
                confirm,
            } => {
                self.memory_scope_purge(auth, memory_scope_id, expected_catalog_revision, confirm)
                    .await
            }
            _ => ToolResult::err("tool does not belong to the memory dispatch family"),
        }
    }
}
