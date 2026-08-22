//! Browser-only hosted Runtime Console API.
//!
//! This surface is intentionally not a model capability. It reuses the normal
//! runtime project authorization and the existing Workflow Session console
//! projection without creating a second store, parser, or observation authority.

use crate::auth::AuthContext;
use crate::tool_runtime::sessions::is_valid_session_id;
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

const DEFAULT_PROJECT_LIMIT: usize = 50;
const MAX_PROJECT_LIMIT: usize = 100;
const MAX_PROJECT_ID_CHARS: usize = 512;
const MAX_PROJECT_NAME_CHARS: usize = 160;
const MAX_CLIENT_ID_CHARS: usize = 160;
const MAX_STATUS_CHARS: usize = 64;

pub(crate) fn routes() -> Router {
    Router::with_path("runtime-console")
        .push(Router::with_path("projects").post(projects))
        .push(Router::with_path("workflow-sessions").post(workflow_sessions))
        .push(Router::with_path("workflow-session").post(workflow_session))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectsInput {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionsInput {
    project: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionInput {
    project: String,
    session_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleProjects {
    projects: Vec<RuntimeConsoleProject>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleProject {
    id: String,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeConsoleError {
    Invalid,
    NotFound,
    Internal,
    Request { status: u16, message: &'static str },
}

impl RuntimeConsoleError {
    fn status(self) -> StatusCode {
        match self {
            Self::Invalid => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Request { status, .. } => {
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST)
            }
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Invalid request",
            Self::NotFound => "Not found",
            Self::Internal => "Runtime Console unavailable",
            Self::Request { message, .. } => message,
        }
    }
}

fn render_error(res: &mut Response, error: RuntimeConsoleError) {
    let status = error.status();
    res.status_code(status);
    res.render(crate::json_error(status, error.message()));
}

fn bounded_text(value: &Value, max_chars: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(max_chars).collect())
}

fn valid_project_id(project: &str) -> bool {
    !project.is_empty()
        && project.len() <= MAX_PROJECT_ID_CHARS
        && !project.chars().any(char::is_control)
}

fn bounded_client_id(value: &Value) -> Option<String> {
    let client_id = value.as_str()?;
    if client_id.is_empty()
        || client_id.chars().count() > MAX_CLIENT_ID_CHARS
        || client_id.chars().any(char::is_control)
    {
        return None;
    }
    Some(client_id.to_string())
}

async fn prepared(
    req: &Request,
    depot: &Depot,
) -> Result<(Arc<ToolRuntime>, AuthContext), RuntimeConsoleError> {
    crate::auth::require_json_same_origin(req)
        .map_err(|(status, _code, message)| RuntimeConsoleError::Request { status, message })?;
    let runtime = depot
        .obtain::<Arc<ToolRuntime>>()
        .cloned()
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let auth = depot
        .obtain::<AuthContext>()
        .cloned()
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok((runtime, auth))
}

async fn visible_project_values(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<Vec<Value>, RuntimeConsoleError> {
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjects {
                client_id: None,
                project: None,
                query: None,
                limit: None,
                summary_only: false,
            },
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(RuntimeConsoleError::Internal);
    }
    result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(RuntimeConsoleError::Internal)
}

fn project_selector_row(value: &Value) -> Option<RuntimeConsoleProject> {
    let id = bounded_text(value.get("id")?, MAX_PROJECT_ID_CHARS)?;
    if !valid_project_id(&id) {
        return None;
    }
    let client_id = bounded_client_id(value.get("client_id")?)?;
    Some(RuntimeConsoleProject {
        id,
        client_id,
        name: value
            .get("name")
            .and_then(|value| bounded_text(value, MAX_PROJECT_NAME_CHARS)),
        connected: value
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        agent_status: value
            .get("agent_status")
            .and_then(|value| bounded_text(value, MAX_STATUS_CHARS)),
    })
}

async fn projects_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    let visible = visible_project_values(runtime, auth).await?;
    let limit = limit
        .unwrap_or(DEFAULT_PROJECT_LIMIT)
        .clamp(1, MAX_PROJECT_LIMIT);
    let project_rows = visible
        .iter()
        .filter_map(project_selector_row)
        .take(limit)
        .collect::<Vec<_>>();
    let truncated = visible.len() > project_rows.len();
    Ok(RuntimeConsoleProjects {
        projects: project_rows,
        truncated,
    })
}

async fn authorize_exact_project(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
) -> Result<(), RuntimeConsoleError> {
    if !valid_project_id(project) {
        return Err(RuntimeConsoleError::Invalid);
    }
    let visible = visible_project_values(runtime, auth).await?;
    if visible
        .iter()
        .any(|value| value.get("id").and_then(Value::as_str) == Some(project))
    {
        Ok(())
    } else {
        Err(RuntimeConsoleError::NotFound)
    }
}

async fn workflow_sessions_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    limit: Option<usize>,
) -> Result<crate::tool_runtime::sessions::WorkflowSessionConsoleList, RuntimeConsoleError> {
    authorize_exact_project(runtime, auth, project).await?;
    Ok(runtime.workflow_sessions_console_list(project, limit))
}

async fn workflow_session_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<crate::tool_runtime::sessions::WorkflowSessionConsoleDetail, RuntimeConsoleError> {
    if !is_valid_session_id(session_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    authorize_exact_project(runtime, auth, project).await?;
    runtime
        .workflow_session_console_detail(project, session_id, limit)
        .ok_or(RuntimeConsoleError::NotFound)
}

#[handler]
async fn projects(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<ProjectsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match projects_for_auth(&runtime, &auth, input.limit).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_sessions(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match workflow_sessions_for_auth(&runtime, &auth, &input.project, input.limit).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match workflow_session_for_auth(
        &runtime,
        &auth,
        &input.project,
        &input.session_id,
        input.limit,
    )
    .await
    {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
    };
    use crate::tool_runtime::sessions::{
        PostSessionMessageInput, SessionMessageKind, SessionMessagePriority,
    };
    use crate::tool_runtime::RuntimeInfo;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    fn project(id: &str, private_path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(format!("Project {id}")),
            path: private_path.to_string(),
            allow_patch: true,
            kind: None,
            description: Some("private description".to_string()),
            hooks: vec!["private-hook".to_string()],
            disabled: false,
            revision: Some(format!("sha256:{}", "1".repeat(64))),
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: Some("private-shell-profile".to_string()),
        }
    }

    async fn register_project(
        runtime: &ToolRuntime,
        client_id: &str,
        project_id: &str,
        private_path: &str,
        auth: Option<&AuthContext>,
    ) {
        runtime
            .shell_clients
            .register_with_auth(
                ShellClientRegisterRequest {
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    client_id: client_id.to_string(),
                    agent_instance_id: format!("inst-{client_id}"),
                    display_name: Some(format!("Device {client_id}")),
                    owner: None,
                    hostname: Some(format!("private-host-{client_id}")),
                    host_context: None,
                    capabilities: Some(ShellClientCapabilities::default()),
                    projects: Some(vec![project(project_id, private_path)]),
                    agent_protocol_version: Some("polling-v1".to_string()),
                    policy: None,
                },
                auth,
            )
            .await
            .unwrap();
    }

    fn test_runtime() -> Arc<ToolRuntime> {
        Arc::new(ToolRuntime::new(
            Arc::new(crate::ShellClientRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        ))
    }

    fn hosted_service(runtime: Arc<ToolRuntime>) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(None);
        let (tmp, db) = crate::test_support::test_db();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(
                crate::connector_runtime::ConnectorRuntimeSlot::default(),
            ))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    #[test]
    fn selector_uses_bounded_authoritative_client_id_without_parsing_project_id() {
        let projected = project_selector_row(&serde_json::json!({
            "id": "agent:not-the-device:project",
            "client_id": "device-real",
            "name": "Demo",
            "connected": true,
            "agent_status": "online"
        }))
        .unwrap();
        assert_eq!(projected.client_id, "device-real");
        assert_ne!(projected.client_id, "not-the-device");

        let overlong = "x".repeat(MAX_CLIENT_ID_CHARS + 1);
        assert!(project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": overlong,
            "connected": true
        }))
        .is_none());
        assert!(project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": "bad\nclient",
            "connected": true
        }))
        .is_none());
    }

    #[tokio::test]
    async fn hosted_runtime_console_works_without_connector_runtime_and_projects_are_safe() {
        let runtime = test_runtime();
        register_project(
            &runtime,
            "special",
            "webcodex",
            "/root/private/webcodex",
            None,
        )
        .await;
        let (_tmp, service) = hosted_service(runtime);
        let mut response = TestClient::post("http://localhost/api/runtime-console/projects")
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let body: Value = response.take_json().await.unwrap();
        assert_eq!(body["projects"][0]["id"], "agent:special:webcodex");
        assert_eq!(body["projects"][0]["client_id"], "special");
        let selector = body["projects"][0].as_object().unwrap();
        assert!(selector.keys().all(|key| matches!(
            key.as_str(),
            "id" | "client_id" | "name" | "connected" | "agent_status"
        )));
        let serialized = serde_json::to_string(&body).unwrap();
        for private in [
            "/root/private/webcodex",
            "private-host-special",
            "private-shell-profile",
            "private-hook",
            &format!("sha256:{}", "1".repeat(64)),
            "private description",
        ] {
            assert!(
                !serialized.contains(private),
                "leaked {private}: {serialized}"
            );
        }
    }

    #[tokio::test]
    async fn runtime_console_preserves_browser_same_origin_and_json_errors() {
        let (_tmp, service) = hosted_service(test_runtime());
        let cross_origin = TestClient::post("http://localhost/api/runtime-console/projects")
            .add_header("host", "localhost", true)
            .add_header("origin", "http://attacker.example", true)
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));

        let unsupported = TestClient::post("http://localhost/api/runtime-console/projects")
            .add_header("host", "localhost", true)
            .body("{}")
            .send(&service)
            .await;
        assert_eq!(
            unsupported.status_code,
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
    }

    #[tokio::test]
    async fn selector_and_session_access_follow_authoritative_project_visibility() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("group-a");
        let auth_b = crate::auth::shared_key_context("group-b");
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        register_project(&runtime, "client-b", "proj-b", "/private/b", Some(&auth_b)).await;

        let direct = runtime
            .dispatch_with_auth(
                ToolCall::ListProjects {
                    client_id: None,
                    project: None,
                    query: None,
                    limit: None,
                    summary_only: false,
                },
                Some(&auth_a),
            )
            .await;
        let projected = projects_for_auth(&runtime, &auth_a, Some(100))
            .await
            .unwrap();
        let direct_ids = direct.output["projects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| value["id"].as_str())
            .collect::<Vec<_>>();
        let projected_ids = projected
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(projected_ids, direct_ids);
        assert_eq!(projected_ids, vec!["agent:client-a:proj-a"]);
        assert_eq!(projected.projects[0].client_id, "client-a");
        assert_eq!(
            projected.projects[0].client_id,
            direct.output["projects"][0]["client_id"].as_str().unwrap()
        );

        let foreign = runtime.sessions.start_session(
            Some("agent:client-b:proj-b".to_string()),
            Some("foreign".to_string()),
        );
        assert_eq!(
            workflow_session_for_auth(
                &runtime,
                &auth_a,
                "agent:client-b:proj-b",
                &foreign.session_id,
                Some(20),
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            workflow_session_for_auth(
                &runtime,
                &auth_a,
                "agent:client-a:proj-a",
                &foreign.session_id,
                Some(20),
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
    }

    #[tokio::test]
    async fn runtime_console_reuses_workflow_session_projection_and_sanitizer() {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("group-a");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = runtime
            .sessions
            .start_session(Some(project_id.to_string()), Some("observe".to_string()));
        runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Progress,
                message: "working in /root/private/source.rs".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();

        let hosted_list = workflow_sessions_for_auth(&runtime, &auth, project_id, Some(20))
            .await
            .unwrap();
        let direct_list = runtime.workflow_sessions_console_list(project_id, Some(20));
        assert_eq!(
            serde_json::to_value(hosted_list).unwrap(),
            serde_json::to_value(direct_list).unwrap()
        );

        let hosted_detail =
            workflow_session_for_auth(&runtime, &auth, project_id, &session.session_id, Some(20))
                .await
                .unwrap();
        let direct_detail = runtime
            .workflow_session_console_detail(project_id, &session.session_id, Some(20))
            .unwrap();
        assert_eq!(
            serde_json::to_value(&hosted_detail).unwrap(),
            serde_json::to_value(&direct_detail).unwrap()
        );
        let serialized = serde_json::to_string(&hosted_detail).unwrap();
        assert!(!serialized.contains("/root/private/source.rs"));
        assert!(serialized.contains("[private path]"));
    }
}
