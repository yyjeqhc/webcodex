//! Runtime dispatch adapters for project management tool calls.

use super::projects::ListProjectsOptions;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_project_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ListProjects {
                client_id,
                project,
                query,
                limit,
                summary_only,
            } => {
                self.list_projects_with_options(
                    auth,
                    ListProjectsOptions {
                        client_id,
                        project,
                        query,
                        limit,
                        summary_only,
                    },
                )
                .await
            }
            ToolCall::RegisterProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                overwrite,
            } => {
                self.register_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    overwrite,
                    auth,
                )
                .await
            }
            ToolCall::UnregisterProject {
                project,
                expected_revision,
            } => {
                self.unregister_project(project, expected_revision, auth)
                    .await
            }
            ToolCall::CreateProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                template,
                git_init,
                allow_existing_empty,
                overwrite,
            } => {
                self.create_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    template,
                    git_init,
                    allow_existing_empty,
                    overwrite,
                    auth,
                )
                .await
            }
            _ => unreachable!("non-project tool routed to project dispatcher"),
        }
    }
}
