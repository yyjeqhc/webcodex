use crate::admin_project_lifecycle::{
    AdminProjectLifecycleService, CreateProjectRequest, ProjectMutationRequest,
    RegisterProjectRequest, ServiceResponse,
};
use crate::auth::{AuthContext, AuthKind};
use crate::tool_runtime::activity::ActivityVisibility;
use crate::tool_runtime::{ToolResult, ToolRuntime};
use crate::Database;
use salvo::prelude::*;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const ADMIN_ROUTES: &[&str] = &[
    "/api/admin/dashboard",
    "/api/admin/projects/register",
    "/api/admin/projects/create",
    "/api/admin/projects/enable",
    "/api/admin/projects/disable",
    "/api/admin/projects/unregister",
];
const ACTIVITY_LIMIT: usize = 50;
const ADMIN_BODY_MAX_BYTES: usize = 16 * 1024;

pub(crate) fn routes() -> Router {
    Router::with_path("admin")
        .push(Router::with_path("dashboard").post(dashboard))
        .push(
            Router::with_path("projects")
                .push(Router::with_path("register").post(register_project))
                .push(Router::with_path("create").post(create_project))
                .push(Router::with_path("enable").post(enable_project))
                .push(Router::with_path("disable").post(disable_project))
                .push(Router::with_path("unregister").post(unregister_project)),
        )
}

fn error(res: &mut Response, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(json!({"error": {"message": message}})));
}

fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}

fn section_status(success: bool, safe_error: &str) -> Value {
    if success {
        json!({"status": "ok", "error": null})
    } else {
        json!({"status": "error", "error": safe_error})
    }
}

fn compatibility_by_client(status: &Value) -> HashMap<String, String> {
    status
        .pointer("/version_compatibility/runners")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|runner| {
            Some((
                runner.get("client_id")?.as_str()?.to_string(),
                runner
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
            ))
        })
        .collect()
}

fn enabled_capabilities(value: Option<&Value>) -> Vec<String> {
    let mut names = value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, enabled)| {
            enabled
                .as_bool()
                .filter(|enabled| *enabled)
                .map(|_| name.clone())
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn project_dashboard(
    status_result: ToolResult,
    agents_result: ToolResult,
    projects_result: ToolResult,
    activity_result: Result<Vec<Value>, ()>,
    bootstrap: bool,
) -> Value {
    let overview_ok = status_result.success;
    let devices_ok = agents_result.success;
    let projects_ok = projects_result.success;
    let activity_ok = activity_result.is_ok();
    let status = if overview_ok {
        status_result.output
    } else {
        Value::Null
    };
    let compatibility = compatibility_by_client(&status);
    let global_compat = status
        .pointer("/version_compatibility/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    let mut device_rows = if devices_ok {
        agents_result
            .output
            .get("agents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|agent| {
                let client_id = agent
                    .get("client_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                json!({
                    "display_name": text(agent.get("display_name")),
                    "client_id": if client_id.is_empty() { Value::Null } else { json!(client_id) },
                    "status": text(agent.get("status")).unwrap_or_else(|| if agent.get("connected").and_then(Value::as_bool).unwrap_or(false) {"online".into()} else {"offline".into()}),
                    "transport": text(agent.get("transport")),
                    "hostname": text(agent.get("hostname")),
                    "last_seen": agent.get("last_seen").cloned().unwrap_or(Value::Null),
                    "capabilities": enabled_capabilities(agent.get("capabilities")),
                    "project_count": agent.get("projects_count").cloned().unwrap_or_else(|| json!(0)),
                    "active_jobs": agent.get("active_jobs").cloned().unwrap_or_else(|| json!(0)),
                    "runner_protocol": text(agent.get("agent_protocol_version")),
                    "compatibility": compatibility.get(client_id).map(String::as_str).unwrap_or("unknown"),
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    device_rows.sort_by(|a, b| {
        a["client_id"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["client_id"].as_str().unwrap_or_default())
    });

    let mut project_rows = if projects_ok {
        projects_result
            .output
            .get("projects")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|project| {
                let connected = project
                    .get("connected")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let client_id = project
                    .get("client_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let capabilities = project
                    .get("capabilities")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let enabled = project.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                let active_jobs = project.get("active_jobs").and_then(Value::as_u64).unwrap_or(0);
                json!({
                    "id": text(project.get("id")),
                    "name": text(project.get("name")),
                    "description": project.get("description").cloned().unwrap_or(Value::Null),
                    "client_id": if client_id.is_empty() { Value::Null } else { json!(client_id) },
                    "path": if bootstrap { project.get("path").cloned().unwrap_or(Value::Null) } else { json!("hidden for non-bootstrap admin") },
                    "readiness": if connected {"online"} else {"offline"},
                    "git_available": capabilities.get("git_available").cloned().unwrap_or(Value::Null),
                    "allow_patch": project.get("allow_patch").cloned().unwrap_or(Value::Null),
                    "enabled": enabled,
                    "lifecycle_status": if enabled {"enabled"} else {"disabled"},
                    "revision": project.get("revision").cloned().unwrap_or(Value::Null),
                    "active_jobs": active_jobs,
                    "actions": {
                        "enable": !enabled,
                        "disable": enabled,
                        "unregister": active_jobs == 0
                    },
                    "shell_profile_status": project.get("shell_profile_status").cloned().unwrap_or(Value::Null),
                    "compatibility": compatibility.get(client_id).map(String::as_str).unwrap_or("unknown"),
                    "console_hint": "Use /console with that project's credential; credentials never belong in URLs.",
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    project_rows.sort_by(|a, b| {
        a["id"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["id"].as_str().unwrap_or_default())
    });

    let activity = activity_result.unwrap_or_default();
    let overview = if overview_ok {
        json!({
            "version": status.get("version").cloned().unwrap_or(Value::Null),
            "build_commit": status.pointer("/build/git_commit").cloned().unwrap_or(Value::Null),
            "authority_mode": status.pointer("/authority/mode").cloned().unwrap_or(Value::Null),
            "agents_total": status.pointer("/agents/count").cloned().unwrap_or_else(|| json!(0)),
            "agents_online": status.pointer("/agents/online_count").cloned().unwrap_or_else(|| json!(0)),
            "projects_total": status.pointer("/projects/agent_registered/count").cloned().unwrap_or_else(|| json!(0)),
            "projects_online": status.pointer("/projects/agent_registered/online_count").cloned().unwrap_or_else(|| json!(0)),
            "active_jobs": status.pointer("/jobs/active_count").cloned().unwrap_or_else(|| json!(0)),
            "version_compatibility": global_compat,
        })
    } else {
        Value::Null
    };
    let diagnostics = if overview_ok {
        json!({
            "runner_process": status.pointer("/connection_layers/runner_process").cloned().unwrap_or(Value::Null),
            "server_transport": status.pointer("/connection_layers/server_transport").cloned().unwrap_or(Value::Null),
            "server_registration": status.pointer("/connection_layers/server_registration").cloned().unwrap_or(Value::Null),
            "project_registry": status.pointer("/connection_layers/project_registry").cloned().unwrap_or(Value::Null),
            "connector_endpoint": status.pointer("/connection_layers/connector_endpoint").cloned().unwrap_or(Value::Null),
            "version_compatibility": status.get("version_compatibility").cloned().unwrap_or(Value::Null),
        })
    } else {
        Value::Object(Map::new())
    };

    json!({
        "section_status": {
            "overview": section_status(overview_ok, "overview unavailable"),
            "devices": section_status(devices_ok, "devices unavailable"),
            "projects": section_status(projects_ok, "projects unavailable"),
            "activity": section_status(activity_ok, "activity unavailable"),
        },
        "overview": overview,
        "devices": device_rows,
        "projects": project_rows,
        "diagnostics": diagnostics,
        "activity": activity,
        "limits": {"activity": ACTIVITY_LIMIT}
    })
}

fn require_admin(depot: &Depot) -> Result<AuthContext, (StatusCode, &'static str)> {
    let auth = depot
        .obtain::<AuthContext>()
        .cloned()
        .map_err(|_| (StatusCode::UNAUTHORIZED, "authentication required"))?;
    if !auth.is_admin()
        || matches!(
            auth.kind,
            AuthKind::AgentToken
                | AuthKind::ProjectCredential
                | AuthKind::SharedKey
                | AuthKind::OpenAnonymous
                | AuthKind::AccountCredential
        )
    {
        return Err((
            StatusCode::FORBIDDEN,
            "bootstrap or admin-scoped token required",
        ));
    }
    Ok(auth)
}

async fn parse_admin_json<T: serde::de::DeserializeOwned>(
    req: &mut Request,
) -> Result<T, ServiceResponse> {
    let bytes = req
        .payload_with_max_size(ADMIN_BODY_MAX_BYTES)
        .await
        .map_err(|_| ServiceResponse {
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            body: json!({"error":{"code":"invalid_request"}}),
        })?;
    serde_json::from_slice(bytes).map_err(|_| ServiceResponse {
        status: StatusCode::BAD_REQUEST.as_u16(),
        body: json!({"error":{"code":"invalid_request"}}),
    })
}

fn render_service_response(res: &mut Response, response: ServiceResponse) {
    res.status_code(
        StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
    );
    res.render(Json(response.body));
}

async fn lifecycle_context(
    req: &mut Request,
    depot: &Depot,
) -> Result<(AuthContext, AdminProjectLifecycleService), ServiceResponse> {
    crate::auth::require_json_same_origin(req).map_err(|(status, _, _)| ServiceResponse {
        status,
        body: json!({"error":{"code":"invalid_request"}}),
    })?;
    let auth = require_admin(depot).map_err(|(status, _)| ServiceResponse {
        status: status.as_u16(),
        body: json!({"error":{"code": if status == StatusCode::UNAUTHORIZED {"unauthorized"} else {"forbidden"}}}),
    })?;
    let runtime = depot
        .obtain::<Arc<ToolRuntime>>()
        .cloned()
        .map_err(|_| ServiceResponse {
            status: 500,
            body: json!({"error":{"code":"operation_failed"}}),
        })?;
    let db = depot
        .obtain::<Arc<Database>>()
        .cloned()
        .map_err(|_| ServiceResponse {
            status: 500,
            body: json!({"error":{"code":"operation_failed"}}),
        })?;
    Ok((auth, AdminProjectLifecycleService::new(runtime, db)))
}

#[handler]
async fn register_project(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (auth, service) = match lifecycle_context(req, depot).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    let body = match parse_admin_json::<RegisterProjectRequest>(req).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    render_service_response(res, service.register(&auth, body).await);
}

#[handler]
async fn create_project(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (auth, service) = match lifecycle_context(req, depot).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    let body = match parse_admin_json::<CreateProjectRequest>(req).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    render_service_response(res, service.create(&auth, body).await);
}

async fn mutate_project(
    action: &'static str,
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let (auth, service) = match lifecycle_context(req, depot).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    let body = match parse_admin_json::<ProjectMutationRequest>(req).await {
        Ok(value) => value,
        Err(response) => return render_service_response(res, response),
    };
    render_service_response(res, service.mutate(&auth, action, body).await);
}

#[handler]
async fn enable_project(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    mutate_project("enable", req, depot, res).await;
}
#[handler]
async fn disable_project(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    mutate_project("disable", req, depot, res).await;
}
#[handler]
async fn unregister_project(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    mutate_project("unregister", req, depot, res).await;
}

#[handler]
async fn dashboard(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if let Err((status, _, message)) = crate::auth::require_json_same_origin(req) {
        return error(
            res,
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
            message,
        );
    }
    let Ok(auth) = depot.obtain::<AuthContext>().cloned() else {
        return error(res, StatusCode::UNAUTHORIZED, "authentication required");
    };
    if !auth.is_admin()
        || matches!(
            auth.kind,
            AuthKind::AgentToken
                | AuthKind::ProjectCredential
                | AuthKind::SharedKey
                | AuthKind::OpenAnonymous
                | AuthKind::AccountCredential
        )
    {
        return error(
            res,
            StatusCode::FORBIDDEN,
            "bootstrap or admin-scoped token required",
        );
    }
    let Ok(runtime) = depot.obtain::<Arc<ToolRuntime>>().cloned() else {
        return error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime unavailable",
        );
    };
    let Ok(db) = depot.obtain::<Arc<Database>>().cloned() else {
        return error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "database unavailable",
        );
    };

    let status = runtime.runtime_status(Some(&auth)).await;
    let agents = runtime.list_agents(Some(&auth)).await;
    let projects = runtime.list_projects(Some(&auth)).await;
    let activity = db
        .list_workspace_activity_for_clients(ACTIVITY_LIMIT, None, ActivityVisibility::Global, &[])
        .map(|rows| {
            rows.into_iter()
                .map(|row| {
                    json!({
                        "created_at": row.created_at,
                        "kind": row.tool,
                        "project_id": row.project,
                        "status": if row.success {"ok"} else {"failed"}
                    })
                })
                .collect::<Vec<_>>()
        })
        .map_err(|_| ());

    res.render(Json(project_dashboard(
        status,
        agents,
        projects,
        activity,
        auth.is_bootstrap(),
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::scopes::SCOPE_ADMIN;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    fn service(auth: Option<AuthContext>) -> Service {
        let (_tmp, db) = crate::test_support::test_db();
        let runtime = Arc::new(ToolRuntime::new(
            Arc::new(crate::ShellClientRegistry::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        ));
        let mut router = Router::new()
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime));
        if let Some(auth) = auth {
            router = router.hoop(affix_state::inject(auth));
        }
        Service::new(router.push(routes()))
    }

    async fn call(auth: Option<AuthContext>) -> (StatusCode, Value) {
        let mut response = TestClient::post("http://127.0.0.1/admin/dashboard")
            .add_header("host", "127.0.0.1", true)
            .add_header("origin", "http://127.0.0.1", true)
            .add_header("content-type", "application/json", true)
            .body("{}")
            .send(&service(auth))
            .await;
        let status = response.status_code.unwrap();
        let body = response.take_json::<Value>().await.unwrap();
        (status, body)
    }

    fn populated_projection(bootstrap: bool) -> Value {
        let status = ToolResult::ok(json!({
            "version": "1.2.3",
            "version_compatibility": {
                "status": "version_mismatch",
                "runners": [
                    {"client_id":"runner-b","status":"version_mismatch"},
                    {"client_id":"runner-a","status":"compatible"}
                ]
            }
        }));
        let agents = ToolResult::ok(json!({"agents":[
            {"client_id":"runner-b","display_name":"B","status":"stale","capabilities":{"shell":true,"patch":false,"git":true}},
            {"client_id":"runner-a","display_name":"A","status":"online","capabilities":{"shell":true,"git":true}}
        ]}));
        let projects = ToolResult::ok(json!({"projects":[
            {"id":"agent:runner-b:zeta","client_id":"runner-b","name":"Zeta","path":"/secret/zeta","connected":false,"capabilities":{"git_available":false}},
            {"id":"agent:runner-a:alpha","client_id":"runner-a","name":"Alpha","path":"/safe/alpha","connected":true,"capabilities":{"git_available":true}},
            {"id":"agent:unknown:orphan","client_id":"unknown","name":"Orphan","path":"/hidden/orphan","connected":false}
        ]}));
        project_dashboard(status, agents, projects, Ok(vec![]), bootstrap)
    }

    #[tokio::test]
    async fn admin_dashboard_rejects_missing_and_project_scoped_credentials() {
        assert_eq!(call(None).await.0, StatusCode::UNAUTHORIZED);
        for kind in [AuthKind::ProjectCredential, AuthKind::AgentToken] {
            assert_eq!(
                call(Some(AuthContext::new(kind))).await.0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            call(Some(AuthContext::new(AuthKind::ApiToken))).await.0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn admin_dashboard_accepts_bootstrap_and_admin_pat_without_secrets() {
        let mut bootstrap = AuthContext::new(AuthKind::Bootstrap);
        bootstrap.is_bootstrap = true;
        let (status, body) = call(Some(bootstrap)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let serialized = serde_json::to_string(&body).unwrap().to_ascii_lowercase();
        for forbidden in [
            "bootstrap_token",
            "project_credential",
            "agent_token",
            "secret_env",
            "authorization",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        assert_eq!(body["limits"]["activity"], ACTIVITY_LIMIT);

        let mut admin = AuthContext::new(AuthKind::ApiToken);
        admin.scopes.push(SCOPE_ADMIN.to_string());
        assert_eq!(call(Some(admin)).await.0, StatusCode::OK);
    }

    #[test]
    fn populated_projection_is_sorted_and_uses_per_runner_compatibility() {
        let body = populated_projection(true);
        assert_eq!(body["devices"][0]["client_id"], "runner-a");
        assert_eq!(body["devices"][0]["status"], "online");
        assert_eq!(body["devices"][0]["capabilities"], json!(["git", "shell"]));
        assert_eq!(body["devices"][0]["compatibility"], "compatible");
        assert_eq!(body["devices"][1]["status"], "stale");
        assert_eq!(body["devices"][1]["capabilities"], json!(["git", "shell"]));
        assert_eq!(body["devices"][1]["compatibility"], "version_mismatch");
        assert_eq!(body["projects"][0]["id"], "agent:runner-a:alpha");
        assert_eq!(body["projects"][0]["compatibility"], "compatible");
        assert_eq!(body["projects"][1]["compatibility"], "version_mismatch");
        assert_eq!(body["projects"][2]["compatibility"], "unknown");
        assert_eq!(body["projects"][0]["path"], "/safe/alpha");
        assert_eq!(
            body["overview"]["version_compatibility"],
            "version_mismatch"
        );
    }

    #[test]
    fn admin_pat_projection_hides_paths() {
        let body = populated_projection(false);
        assert!(body["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|project| { project["path"] == "hidden for non-bootstrap admin" }));
    }

    #[test]
    fn data_source_failures_are_independent_and_safe() {
        let cases = [
            project_dashboard(
                ToolResult::err("secret /path"),
                ToolResult::ok(json!({"agents":[]})),
                ToolResult::ok(json!({"projects":[]})),
                Ok(vec![]),
                true,
            ),
            project_dashboard(
                ToolResult::ok(json!({})),
                ToolResult::err("secret token"),
                ToolResult::ok(json!({"projects":[]})),
                Ok(vec![]),
                true,
            ),
            project_dashboard(
                ToolResult::ok(json!({})),
                ToolResult::ok(json!({"agents":[]})),
                ToolResult::err("secret env"),
                Ok(vec![]),
                true,
            ),
            project_dashboard(
                ToolResult::ok(json!({})),
                ToolResult::ok(json!({"agents":[]})),
                ToolResult::ok(json!({"projects":[]})),
                Err(()),
                true,
            ),
        ];
        let sections = ["overview", "devices", "projects", "activity"];
        for (body, failed) in cases.into_iter().zip(sections) {
            assert_eq!(body["section_status"][failed]["status"], "error");
            let serialized = serde_json::to_string(&body).unwrap();
            assert!(!serialized.contains("secret"));
            for section in sections.into_iter().filter(|section| *section != failed) {
                assert_eq!(body["section_status"][section]["status"], "ok");
            }
        }
    }

    #[test]
    fn activity_is_bounded_and_projection_has_no_secret_fields() {
        let body = populated_projection(true);
        assert_eq!(body["limits"]["activity"], ACTIVITY_LIMIT);
        let serialized = serde_json::to_string(&body).unwrap().to_ascii_lowercase();
        for forbidden in [
            "project_credential",
            "agent_token",
            "secret_env",
            "authorization",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    async fn lifecycle_call(
        auth: Option<AuthContext>,
        body: &str,
        origin: bool,
    ) -> (StatusCode, Value) {
        let mut request = TestClient::post("http://127.0.0.1/admin/projects/register")
            .add_header("host", "127.0.0.1", true)
            .add_header("content-type", "application/json", true)
            .body(body.to_string());
        request = request.add_header(
            "origin",
            if origin {
                "http://127.0.0.1"
            } else {
                "http://evil.invalid"
            },
            true,
        );
        let mut response = request.send(&service(auth)).await;
        let status = response.status_code.unwrap();
        let body = response.take_json::<Value>().await.unwrap();
        (status, body)
    }

    #[tokio::test]
    async fn lifecycle_routes_enforce_admin_same_origin_and_strict_json() {
        let valid = r#"{
            "client_id":"oe","project_id":"demo","name":"Demo",
            "path":"/tmp/demo","allow_patch":true,"idempotency_key":"req-1"
        }"#;
        assert_eq!(
            lifecycle_call(None, valid, true).await.0,
            StatusCode::UNAUTHORIZED
        );
        for kind in [AuthKind::ProjectCredential, AuthKind::AgentToken] {
            assert_eq!(
                lifecycle_call(Some(AuthContext::new(kind)), valid, true)
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            lifecycle_call(Some(AuthContext::new(AuthKind::ApiToken)), valid, true)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let mut admin = AuthContext::new(AuthKind::ApiToken);
        admin.scopes.push(SCOPE_ADMIN.to_string());
        assert_eq!(
            lifecycle_call(Some(admin.clone()), valid, false).await.0,
            StatusCode::FORBIDDEN
        );
        let unknown = valid.replace("\n        }", ",\"unknown\":true\n        }");
        assert_eq!(
            lifecycle_call(Some(admin), &unknown, true).await.0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn dashboard_projection_exposes_lifecycle_actions() {
        let body = populated_projection(true);
        let project = &body["projects"][0];
        assert_eq!(project["enabled"], true);
        assert_eq!(project["actions"]["disable"], true);
        assert_eq!(project["actions"]["enable"], false);
        assert_eq!(project["actions"]["unregister"], true);
    }

    #[test]
    fn admin_routes_are_separate_from_console_routes() {
        assert!(ADMIN_ROUTES
            .iter()
            .all(|route| route.starts_with("/api/admin/")));
        assert!(ADMIN_ROUTES
            .iter()
            .all(|route| !crate::host_console_http::CONSOLE_ROUTES.contains(route)));
    }
}
