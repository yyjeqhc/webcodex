use crate::admin_project_lifecycle::{
    AdminProjectLifecycleService, CreateProjectRequest, ProjectMutationRequest,
    RegisterProjectRequest, ServiceResponse,
};
use crate::auth::{AuthContext, AuthKind};
use crate::tool_runtime::activity::ActivityVisibility;
use crate::tool_runtime::admin_dashboard::{
    AdminDashboardDevice, AdminDashboardOverview, AdminDashboardProject, AdminDashboardSnapshot,
};
use crate::tool_runtime::ToolRuntime;
use crate::Database;
use salvo::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

const ACTIVITY_LIMIT: usize = 50;
const ADMIN_BODY_MAX_BYTES: usize = 16 * 1024;

pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};
    Router::new()
        .push(Router::with_path(api_path(RouteId::AdminDashboard)).post(dashboard))
        .push(Router::with_path(api_path(RouteId::AdminProjectsRegister)).post(register_project))
        .push(Router::with_path(api_path(RouteId::AdminProjectsCreate)).post(create_project))
        .push(Router::with_path(api_path(RouteId::AdminProjectsEnable)).post(enable_project))
        .push(Router::with_path(api_path(RouteId::AdminProjectsDisable)).post(disable_project))
        .push(
            Router::with_path(api_path(RouteId::AdminProjectsUnregister)).post(unregister_project),
        )
}

fn error(res: &mut Response, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(json!({"error": {"message": message}})));
}

fn section_status(success: bool, safe_error: &str) -> Value {
    if success {
        json!({"status": "ok", "error": null})
    } else {
        json!({"status": "error", "error": safe_error})
    }
}

fn device_row(device: AdminDashboardDevice) -> Value {
    let client_id = if device.client_id.is_empty() {
        Value::Null
    } else {
        json!(device.client_id)
    };
    json!({
        "display_name": device.display_name,
        "client_id": client_id,
        "status": device.status,
        "transport": device.transport,
        "hostname": device.hostname,
        "last_seen": device.last_seen,
        "capabilities": device.capabilities,
        "project_count": device.project_count,
        "active_jobs": device.active_jobs,
        "runner_protocol_generation": device.runner_protocol_generation,
        "compatibility": device.compatibility,
        "protocol_compatibility": device.protocol_compatibility,
        "build_alignment": device.build_alignment,
    })
}

fn project_row(project: AdminDashboardProject, bootstrap: bool) -> Value {
    let client_id = if project.client_id.is_empty() {
        Value::Null
    } else {
        json!(project.client_id)
    };
    let path = if bootstrap {
        json!(project.path)
    } else {
        json!("hidden for non-bootstrap admin")
    };
    json!({
        "id": project.id,
        "name": project.name,
        "description": project.description,
        "client_id": client_id,
        "path": path,
        "readiness": if project.connected {"online"} else {"offline"},
        "git_available": project.git_available,
        "allow_patch": project.allow_patch,
        "enabled": project.enabled,
        "lifecycle_status": if project.enabled {"enabled"} else {"disabled"},
        "revision": project.revision,
        "active_jobs": project.active_jobs,
        "actions": {
            "enable": !project.enabled,
            "disable": project.enabled,
            "unregister": project.active_jobs == 0
        },
        "shell_profile_status": project.shell_profile_status,
        "compatibility": project.compatibility,
        "protocol_compatibility": project.protocol_compatibility,
        "build_alignment": project.build_alignment,
        "console_hint": "Use /runtime with that project's credential; credentials never belong in URLs.",
    })
}

fn overview_row(overview: AdminDashboardOverview) -> Value {
    json!({
        "version": overview.version,
        "build_commit": overview.build_commit,
        "authority_mode": overview.authority_mode,
        "runners_total": overview.runners_total,
        "runners_online": overview.runners_online,
        "projects_total": overview.projects_total,
        "projects_online": overview.projects_online,
        "active_jobs": overview.active_jobs,
        "version_compatibility": overview.version_compatibility,
        "protocol_compatibility": overview.protocol_compatibility,
        "build_alignment": overview.build_alignment,
        "desktop_runtime_contract": overview.desktop_runtime_contract,
    })
}

fn project_dashboard(
    snapshot: AdminDashboardSnapshot,
    activity_result: Result<Vec<Value>, ()>,
    bootstrap: bool,
) -> Value {
    let AdminDashboardSnapshot {
        overview,
        devices,
        projects,
    } = snapshot;
    let overview_ok = overview.is_ok();
    let devices_ok = devices.is_ok();
    let projects_ok = projects.is_ok();
    let activity_ok = activity_result.is_ok();

    let diagnostics = overview
        .as_ref()
        .map(|overview| {
            json!({
                "runner_process": overview.diagnostics.runner_process.clone(),
                "server_transport": overview.diagnostics.server_transport.clone(),
                "server_registration": overview.diagnostics.server_registration.clone(),
                "project_registry": overview.diagnostics.project_registry.clone(),
                "version_compatibility": overview.diagnostics.version_compatibility.clone(),
            })
        })
        .unwrap_or_else(|_| json!({}));
    let overview = overview.map(overview_row).unwrap_or(Value::Null);

    let mut device_rows = devices
        .unwrap_or_default()
        .into_iter()
        .map(device_row)
        .collect::<Vec<_>>();
    device_rows.sort_by(|left, right| {
        left["client_id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["client_id"].as_str().unwrap_or_default())
    });
    let mut project_rows = projects
        .unwrap_or_default()
        .into_iter()
        .map(|project| project_row(project, bootstrap))
        .collect::<Vec<_>>();
    project_rows.sort_by(|left, right| {
        left["id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["id"].as_str().unwrap_or_default())
    });

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
        "activity": activity_result.unwrap_or_default(),
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

    let snapshot = runtime.admin_dashboard_snapshot(&auth).await;
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
        snapshot,
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
            Arc::new(crate::RunnerRegistry::default()),
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

    fn populated_snapshot() -> AdminDashboardSnapshot {
        AdminDashboardSnapshot {
            overview: Ok(AdminDashboardOverview {
                version: "1.2.3".to_string(),
                build_commit: Some("abcdef0".to_string()),
                authority_mode: "trusted_agent".to_string(),
                runners_total: 2,
                runners_online: 1,
                projects_total: 3,
                projects_online: 1,
                active_jobs: 0,
                version_compatibility: "compatible".to_string(),
                protocol_compatibility: "compatible".to_string(),
                build_alignment: "different_version".to_string(),
                desktop_runtime_contract:
                    webcodex_core::desktop_runtime_contract::DESKTOP_RUNTIME_CONTRACT,
                diagnostics: crate::tool_runtime::admin_dashboard::AdminDashboardDiagnostics {
                    runner_process: json!({"status":"online"}),
                    server_transport: json!({"status":"online"}),
                    server_registration: json!({"status":"online"}),
                    project_registry: json!({"status":"online"}),
                    version_compatibility: json!({"status":"compatible"}),
                },
            }),
            devices: Ok(vec![
                AdminDashboardDevice {
                    display_name: Some("B".to_string()),
                    client_id: "runner-b".to_string(),
                    status: "stale".to_string(),
                    transport: "quic".to_string(),
                    hostname: None,
                    last_seen: 2,
                    capabilities: vec!["git".to_string(), "shell".to_string()],
                    project_count: 1,
                    active_jobs: 0,
                    runner_protocol_generation: 2,
                    compatibility: "compatible".to_string(),
                    protocol_compatibility: "compatible".to_string(),
                    build_alignment: "different_version".to_string(),
                },
                AdminDashboardDevice {
                    display_name: Some("A".to_string()),
                    client_id: "runner-a".to_string(),
                    status: "online".to_string(),
                    transport: "websocket".to_string(),
                    hostname: None,
                    last_seen: 1,
                    capabilities: vec!["git".to_string(), "shell".to_string()],
                    project_count: 1,
                    active_jobs: 0,
                    runner_protocol_generation: 2,
                    compatibility: "compatible".to_string(),
                    protocol_compatibility: "compatible".to_string(),
                    build_alignment: "exact".to_string(),
                },
            ]),
            projects: Ok(vec![
                AdminDashboardProject {
                    id: "agent:runner-b:zeta".to_string(),
                    name: Some("Zeta".to_string()),
                    description: None,
                    client_id: "runner-b".to_string(),
                    path: "/secret/zeta".to_string(),
                    connected: false,
                    git_available: Some(false),
                    allow_patch: true,
                    enabled: true,
                    revision: None,
                    active_jobs: 0,
                    shell_profile_status: "default".to_string(),
                    compatibility: "compatible".to_string(),
                    protocol_compatibility: "compatible".to_string(),
                    build_alignment: "different_version".to_string(),
                },
                AdminDashboardProject {
                    id: "agent:runner-a:alpha".to_string(),
                    name: Some("Alpha".to_string()),
                    description: None,
                    client_id: "runner-a".to_string(),
                    path: "/safe/alpha".to_string(),
                    connected: true,
                    git_available: Some(true),
                    allow_patch: true,
                    enabled: true,
                    revision: None,
                    active_jobs: 0,
                    shell_profile_status: "default".to_string(),
                    compatibility: "compatible".to_string(),
                    protocol_compatibility: "compatible".to_string(),
                    build_alignment: "exact".to_string(),
                },
                AdminDashboardProject {
                    id: "agent:unknown:orphan".to_string(),
                    name: Some("Orphan".to_string()),
                    description: None,
                    client_id: "unknown".to_string(),
                    path: "/hidden/orphan".to_string(),
                    connected: false,
                    git_available: None,
                    allow_patch: false,
                    enabled: true,
                    revision: None,
                    active_jobs: 0,
                    shell_profile_status: "unknown".to_string(),
                    compatibility: "unknown".to_string(),
                    protocol_compatibility: "unknown".to_string(),
                    build_alignment: "unknown".to_string(),
                },
            ]),
        }
    }

    fn populated_projection(bootstrap: bool) -> Value {
        project_dashboard(populated_snapshot(), Ok(vec![]), bootstrap)
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
        assert_eq!(body["devices"][0]["runner_protocol_generation"], 2);
        assert_eq!(body["devices"][0]["transport"], "websocket");
        assert_eq!(body["devices"][1]["status"], "stale");
        assert_eq!(body["devices"][1]["capabilities"], json!(["git", "shell"]));
        assert_eq!(body["devices"][1]["compatibility"], "compatible");
        assert_eq!(body["devices"][1]["build_alignment"], "different_version");
        assert_eq!(body["devices"][1]["runner_protocol_generation"], 2);
        assert_eq!(body["devices"][1]["transport"], "quic");
        assert_eq!(body["projects"][0]["id"], "agent:runner-a:alpha");
        assert_eq!(body["projects"][0]["compatibility"], "compatible");
        assert_eq!(body["projects"][1]["compatibility"], "compatible");
        assert_eq!(body["projects"][1]["build_alignment"], "different_version");
        assert_eq!(body["projects"][2]["compatibility"], "unknown");
        assert_eq!(body["projects"][0]["path"], "/safe/alpha");
        assert_eq!(body["overview"]["version_compatibility"], "compatible");
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
        let snapshot = populated_snapshot();
        let cases = [
            project_dashboard(
                AdminDashboardSnapshot {
                    overview: Err(()),
                    ..snapshot.clone()
                },
                Ok(vec![]),
                false,
            ),
            project_dashboard(
                AdminDashboardSnapshot {
                    devices: Err(()),
                    ..snapshot.clone()
                },
                Ok(vec![]),
                false,
            ),
            project_dashboard(
                AdminDashboardSnapshot {
                    projects: Err(()),
                    ..snapshot.clone()
                },
                Ok(vec![]),
                false,
            ),
            project_dashboard(snapshot, Err(()), false),
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
        let admin = crate::route_metadata::iter_routes()
            .filter(|spec| spec.surface == crate::route_metadata::RouteSurface::Admin);
        assert!(admin
            .clone()
            .all(|spec| spec.path.starts_with("/api/admin/")));
    }
}
