use super::{render_result, runtime};
use crate::action_audit::ActionAudit;
use crate::json_error;
use crate::tool_runtime::ToolCall;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::Value;

/// `POST /api/projects/register` — thin REST wrapper over
/// `ToolCall::RegisterProject`. Mutation with side effects; registers an
/// existing directory as a WebCodex project on the selected agent. Dedicated
/// GPT Action (`registerProject`); also reachable via callRuntimeTool / MCP
/// tools/call.
#[derive(Debug, Deserialize)]
struct RegisterProjectRequest {
    pub client_id: String,
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "crate::tool_runtime::default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub overwrite: bool,
}

/// `POST /api/projects/create` — thin REST wrapper over
/// `ToolCall::CreateProject`. Mutation with side effects; creates a new
/// directory on the selected agent and registers it as a WebCodex project.
/// Dedicated GPT Action (`createProject`); also reachable via callRuntimeTool
/// / MCP tools/call.
#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    pub client_id: String,
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "crate::tool_runtime::default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub git_init: bool,
    #[serde(default)]
    pub allow_existing_empty: bool,
    #[serde(default)]
    pub overwrite: bool,
}

#[handler]
pub async fn projects_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/list", "listProjects");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Body is optional; reject non-empty invalid JSON for consistency but
    // tolerate an empty/missing body since this call takes no arguments.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => Value::Null,
    };
    let _ = body;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, auth.as_ref())
        .await;
    render_result(res, &audit, "list_projects", None, result);
}

/// `ToolCall::RegisterProject`. Registers an existing directory as a
/// WebCodex project on the selected agent. Mutation with side effects; executes
/// on the selected agent and is constrained by agent policy.
#[handler]
pub async fn projects_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/register", "registerProject");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: RegisterProjectRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RegisterProject {
                client_id: body.client_id,
                id: body.id,
                name: body.name,
                path: body.path,
                description: body.description,
                allow_patch: body.allow_patch,
                overwrite: body.overwrite,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "register_project", None, result);
}

/// `ToolCall::CreateProject`. Creates a new directory on the selected agent
/// and registers it as a WebCodex project. Mutation with side effects; executes
/// on the selected agent and is constrained by agent policy.
#[handler]
pub async fn projects_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/create", "createProject");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: CreateProjectRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::CreateProject {
                client_id: body.client_id,
                id: body.id,
                name: body.name,
                path: body.path,
                description: body.description,
                allow_patch: body.allow_patch,
                template: body.template,
                git_init: body.git_init,
                allow_existing_empty: body.allow_existing_empty,
                overwrite: body.overwrite,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "create_project", None, result);
}
