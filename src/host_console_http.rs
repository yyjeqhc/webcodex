use crate::auth::AuthContext;
use crate::connector_runtime::http::{render, runtime};
use crate::connector_runtime::workspace::LocalResultDecision;
use crate::connector_runtime::{
    result_projection, store_error_outcome, validate_opaque_id, ConnectorCallOutcome,
    ConnectorRuntime, TaskCancelInput, TaskReviewInput,
};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) const CONSOLE_ROUTES: &[&str] = &[
    "/api/console/readiness",
    "/api/console/tasks",
    "/api/console/task/review",
    "/api/console/task/cancel",
    "/api/console/result/accept",
    "/api/console/result/reject",
];

pub(crate) fn routes() -> Router {
    Router::with_path("console")
        .push(Router::with_path("readiness").post(readiness))
        .push(Router::with_path("tasks").post(tasks))
        .push(Router::with_path("task/review").post(task_review))
        .push(Router::with_path("task/cancel").post(task_cancel))
        .push(Router::with_path("result/accept").post(result_accept))
        .push(Router::with_path("result/reject").post(result_reject))
}

fn failure(status: u16, code: &str, message: impl Into<String>) -> ConnectorCallOutcome {
    ConnectorCallOutcome::error(
        status,
        code,
        message,
        false,
        true,
        Some("Correct the request or refresh the current task review."),
        None,
        false,
    )
}

fn prepared(
    req: &Request,
    depot: &Depot,
) -> Result<(Arc<ConnectorRuntime>, AuthContext), ConnectorCallOutcome> {
    crate::auth::require_json_same_origin(req)
        .map_err(|(status, code, message)| failure(status, code, message))?;
    let runtime = runtime(depot).ok_or_else(|| {
        failure(
            404,
            "connector_surface_disabled",
            "this project has not been configured",
        )
    })?;
    let auth = depot
        .obtain::<AuthContext>()
        .cloned()
        .map_err(|_| failure(401, "unauthorized", "authentication required"))?;
    Ok((runtime, auth))
}

macro_rules! prepare {
    ($req:expr, $depot:expr, $res:expr) => {
        match prepared($req, $depot) {
            Ok(context) => context,
            Err(outcome) => return render($res, outcome),
        }
    };
}

fn invalid(res: &mut Response, message: impl Into<String>) {
    render(res, failure(400, "invalid_arguments", message));
}

macro_rules! parse {
    ($ty:ty, $req:expr, $res:expr) => {
        match $req.parse_json::<$ty>().await {
            Ok(input) => input,
            Err(error) => return invalid($res, format!("invalid request: {error}")),
        }
    };
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListInput {
    #[serde(default)]
    include_completed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideInput {
    task_id: String,
    result_id: Option<String>,
}

#[handler]
async fn readiness(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = prepare!(req, depot, res);
    match runtime.readiness(&auth).await {
        Some(report) => res.render(Json(report)),
        None => render(res, failure(401, "unauthorized", "authentication required")),
    }
}

#[handler]
async fn tasks(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, _) = prepare!(req, depot, res);
    let input = parse!(ListInput, req, res);
    match runtime.db.local_reviewable_tasks(
        &runtime.context().project_id,
        input.include_completed,
        20,
    ) {
        Ok(rows) => res.render(Json(json!({ "tasks": rows }))),
        Err(error) => render(res, store_error_outcome(error, None)),
    }
}

#[handler]
async fn task_review(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = prepare!(req, depot, res);
    let input = parse!(TaskReviewInput, req, res);
    if validate_opaque_id(&input.task_id, "wc_task_", "task_id").is_err()
        || input.max_events.is_some()
    {
        return invalid(res, "invalid review input");
    }
    render(res, runtime.host_review(&auth, input).await);
}

#[handler]
async fn task_cancel(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = prepare!(req, depot, res);
    let input = parse!(TaskCancelInput, req, res);
    if validate_opaque_id(&input.task_id, "wc_task_", "task_id").is_err() {
        return invalid(res, "invalid cancel input");
    }
    render(res, runtime.host_cancel(&auth, input).await);
}

async fn decide(req: &mut Request, depot: &Depot, res: &mut Response, accept: bool) {
    let (runtime, _) = prepare!(req, depot, res);
    let input = parse!(DecideInput, req, res);
    let result_valid = input
        .result_id
        .as_deref()
        .is_none_or(|id| validate_opaque_id(id, "wc_result_", "result_id").is_ok());
    if validate_opaque_id(&input.task_id, "wc_task_", "task_id").is_err()
        || !result_valid
        || (accept && input.result_id.is_none())
    {
        return invalid(res, "invalid decision input");
    }
    let decision = if accept {
        LocalResultDecision::Accept
    } else {
        LocalResultDecision::Reject
    };
    let result = runtime.host_decide(
        &input.task_id,
        input.result_id.as_deref(),
        decision,
        chrono::Utc::now().timestamp(),
    );
    match result {
        Ok(result) => res.render(Json(json!({
            "decision": result.decision_status,
            "result": result_projection(&result)
        }))),
        Err(error) => render(res, store_error_outcome(error, None)),
    }
}

#[handler]
async fn result_accept(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    decide(req, depot, res, true).await;
}

#[handler]
async fn result_reject(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    decide(req, depot, res, false).await;
}
