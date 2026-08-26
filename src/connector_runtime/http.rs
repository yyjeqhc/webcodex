//! REST adapter for the canonical connector capabilities.

use super::{ConnectorCallOutcome, ConnectorRuntime, ConnectorRuntimeSlot, ConnectorTransport};
use crate::auth::AuthContext;
use salvo::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};
    Router::new()
        .push(Router::with_path(api_path(RouteId::ConnectorReadiness)).post(readiness))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskStart)).post(task_start))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskList)).post(task_list))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskResume)).post(task_resume))
        .push(Router::with_path(api_path(RouteId::ConnectorFilesList)).post(files_list))
        .push(Router::with_path(api_path(RouteId::ConnectorFilesRead)).post(files_read))
        .push(Router::with_path(api_path(RouteId::ConnectorFilesSearch)).post(files_search))
        .push(Router::with_path(api_path(RouteId::ConnectorCodeNavigate)).post(code_navigate))
        .push(Router::with_path(api_path(RouteId::ConnectorCodeImpact)).post(code_impact))
        .push(Router::with_path(api_path(RouteId::ConnectorEditsApply)).post(edits_apply))
        .push(Router::with_path(api_path(RouteId::ConnectorChecksRun)).post(checks_run))
        .push(Router::with_path(api_path(RouteId::ConnectorCommandsRun)).post(commands_run))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskReview)).post(task_review))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskCancel)).post(task_cancel))
        .push(Router::with_path(api_path(RouteId::ConnectorTaskFinish)).post(task_finish))
}

#[handler]
async fn readiness(depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(crate::project_entry::runtime_readiness(
            None,
            crate::project_entry::RemoteProbe::ProjectMissing,
        )));
        return;
    };
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(json!({"error": "Unauthorized"})));
        return;
    };
    let Some(project_readiness) = runtime.readiness(auth).await else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(json!({"error": "Unauthorized"})));
        return;
    };
    res.render(Json(project_readiness));
}

pub(crate) fn runtime(depot: &Depot) -> Option<Arc<ConnectorRuntime>> {
    depot
        .obtain::<ConnectorRuntimeSlot>()
        .ok()
        .and_then(|slot| slot.0.clone())
}

async fn dispatch(
    capability: &'static str,
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(json!({
            "ok": false,
            "task_id": null,
            "run_id": null,
            "event_cursor": null,
            "data": null,
            "warnings": [],
            "blocking": true,
            "error": {
                "code": "connector_surface_disabled",
                "message": "this project has not been configured",
                "retryable": false,
                "user_action_required": true,
                "suggested_action": "Run webcodex setup, then webcodex run."
            }
        })));
        return;
    };
    let arguments: Value = match req.parse_json().await {
        Ok(arguments) => arguments,
        Err(error) => {
            render(
                res,
                ConnectorCallOutcome::error(
                    400,
                    "invalid_arguments",
                    format!("{capability}: invalid JSON: {error}"),
                    false,
                    false,
                    Some("Send a JSON object matching the operation schema."),
                    None,
                    true,
                ),
            );
            return;
        }
    };
    let auth = depot.obtain::<AuthContext>().ok().cloned();
    let window = crate::client_window::api_window(req, res);
    let outcome = runtime
        .call_for_window(
            capability,
            arguments,
            auth.as_ref(),
            ConnectorTransport::Api,
            Some(&window),
        )
        .await;
    render(res, outcome);
}

pub(crate) fn render(res: &mut Response, outcome: ConnectorCallOutcome) {
    let status =
        StatusCode::from_u16(outcome.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    res.status_code(status);
    if let Some(scope) = outcome.required_scope {
        let challenge = crate::auth::oauth_insufficient_scope_challenge(Some(scope));
        if let Ok(value) = salvo::http::HeaderValue::from_str(&challenge) {
            res.headers_mut().insert("www-authenticate", value);
        }
    }
    res.render(Json(outcome.body));
}

/// Each connector capability gets an identical `#[handler]` that forwards its
/// own name to `dispatch`. The macro keeps the handlers from being copies.
macro_rules! connector_handlers {
    ($($name:ident),+ $(,)?) => {
        $(
            #[handler]
            async fn $name(req: &mut Request, depot: &mut Depot, res: &mut Response) {
                dispatch(stringify!($name), req, depot, res).await;
            }
        )+
    };
}

connector_handlers! {
    task_start,
    task_list,
    task_resume,
    files_list,
    files_read,
    files_search,
    code_navigate,
    code_impact,
    edits_apply,
    checks_run,
    commands_run,
    task_review,
    task_cancel,
    task_finish,
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::{Router, Service};

    /// The Actions schema announces one route per capability, but nothing else
    /// ties the announcement to a registered handler: files_list shipped in
    /// the schema while its route 404ed. Every announced capability must
    /// answer — the unconfigured-project body proves the route exists.
    #[tokio::test]
    async fn every_announced_capability_route_is_served() {
        let service = Service::new(Router::with_path("api").push(super::routes()));
        for name in crate::connector_runtime::surface::CAPABILITY_NAMES {
            let route = crate::connector_runtime::surface::route_for(name)
                .expect("registered connector capability has a route");
            let mut response = TestClient::post(format!("http://127.0.0.1{route}"))
                .json(&serde_json::json!({}))
                .send(&service)
                .await;
            let body = response.take_string().await.unwrap_or_default();
            assert!(
                body.contains("connector_surface_disabled"),
                "capability {name} is announced at {route} but not served: {body}"
            );
        }
    }
}
