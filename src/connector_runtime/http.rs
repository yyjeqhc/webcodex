//! REST adapter for the canonical connector capabilities.

use super::{ConnectorCallOutcome, ConnectorRuntime, ConnectorRuntimeSlot, ConnectorTransport};
use crate::auth::AuthContext;
use salvo::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) fn routes() -> Router {
    Router::with_path("connector")
        .push(Router::with_path("task/start").post(task_start))
        .push(Router::with_path("files/read").post(files_read))
        .push(Router::with_path("files/search").post(files_search))
        .push(Router::with_path("edits/apply").post(edits_apply))
        .push(Router::with_path("checks/run").post(checks_run))
        .push(Router::with_path("commands/run").post(commands_run))
        .push(Router::with_path("task/review").post(task_review))
        .push(Router::with_path("task/finish").post(task_finish))
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
                "message": "this server was not started by a project-bound connect profile",
                "retryable": false,
                "user_action_required": true,
                "suggested_action": "Start the project with webcodex connect."
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
    let outcome = runtime
        .call(
            capability,
            arguments,
            auth.as_ref(),
            ConnectorTransport::Api,
        )
        .await;
    render(res, outcome);
}

fn render(res: &mut Response, outcome: ConnectorCallOutcome) {
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

#[handler]
async fn task_start(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("task_start", req, depot, res).await;
}

#[handler]
async fn files_read(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("files_read", req, depot, res).await;
}

#[handler]
async fn files_search(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("files_search", req, depot, res).await;
}

#[handler]
async fn edits_apply(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("edits_apply", req, depot, res).await;
}

#[handler]
async fn checks_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("checks_run", req, depot, res).await;
}

#[handler]
async fn commands_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("commands_run", req, depot, res).await;
}

#[handler]
async fn task_review(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("task_review", req, depot, res).await;
}

#[handler]
async fn task_finish(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    dispatch("task_finish", req, depot, res).await;
}
