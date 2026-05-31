use crate::{
    get_db, json_error, CreateDesktopTaskRequest, DesktopTask, DesktopTaskClaimRequest,
    DesktopTaskEventRequest,
};
use salvo::prelude::*;
use uuid::Uuid;

#[derive(Debug, serde::Serialize)]
struct DesktopTaskResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<DesktopTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DesktopTaskListResponse {
    success: bool,
    tasks: Vec<DesktopTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn valid_status(status: &str) -> bool {
    matches!(
        status,
        "pending" | "running" | "completed" | "failed" | "needs_input" | "cancelled"
    )
}

#[handler]
pub async fn create_desktop_task(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let body: CreateDesktopTaskRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let title = body.title.trim();
    let instructions = body.instructions.trim();
    if title.is_empty() || instructions.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "title and instructions are required",
        ));
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let task = DesktopTask {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        instructions: instructions.to_string(),
        status: "pending".to_string(),
        priority: body.priority.unwrap_or(0).clamp(-100, 100),
        claimed_by: None,
        last_event: Some("created".to_string()),
        screenshot_url: None,
        created_at: now,
        updated_at: now,
    };
    match db.insert_desktop_task(&task) {
        Ok(_) => res.render(Json(DesktopTaskResponse {
            success: true,
            task: Some(task),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(DesktopTaskResponse {
                success: false,
                task: None,
                error: Some(e.to_string()),
            }));
        }
    }
}

#[handler]
pub async fn list_desktop_tasks(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let status = req.query::<String>("status");
    if let Some(status) = status.as_deref() {
        if !valid_status(status) {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(DesktopTaskListResponse {
                success: false,
                tasks: Vec::new(),
                error: Some("invalid status".to_string()),
            }));
            return;
        }
    }
    let limit = req.query::<usize>("limit").unwrap_or(20).clamp(1, 100);
    match db.list_desktop_tasks(status.as_deref(), limit) {
        Ok(tasks) => res.render(Json(DesktopTaskListResponse {
            success: true,
            tasks,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(DesktopTaskListResponse {
                success: false,
                tasks: Vec::new(),
                error: Some(e.to_string()),
            }));
        }
    }
}

#[handler]
pub async fn claim_desktop_task(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("id").unwrap_or_default();
    let body: DesktopTaskClaimRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let worker = body.worker.trim();
    if worker.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(StatusCode::BAD_REQUEST, "worker is required"));
        return;
    }
    match db.claim_desktop_task(&id, worker, chrono::Utc::now().timestamp()) {
        Ok(Some(task)) => res.render(Json(DesktopTaskResponse {
            success: true,
            task: Some(task),
            error: None,
        })),
        Ok(None) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(DesktopTaskResponse {
                success: false,
                task: None,
                error: Some("task is missing or not pending".to_string()),
            }));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(DesktopTaskResponse {
                success: false,
                task: None,
                error: Some(e.to_string()),
            }));
        }
    }
}

#[handler]
pub async fn append_desktop_task_event(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("id").unwrap_or_default();
    let body: DesktopTaskEventRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    if let Some(status) = body.status.as_deref() {
        if !valid_status(status) {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, "invalid status"));
            return;
        }
    }
    match db.update_desktop_task_event(
        &id,
        body.status.as_deref(),
        body.worker.as_deref(),
        body.message.as_deref(),
        body.screenshot_url.as_deref(),
        chrono::Utc::now().timestamp(),
    ) {
        Ok(Some(task)) => res.render(Json(DesktopTaskResponse {
            success: true,
            task: Some(task),
            error: None,
        })),
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(DesktopTaskResponse {
                success: false,
                task: None,
                error: Some("task not found".to_string()),
            }));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(DesktopTaskResponse {
                success: false,
                task: None,
                error: Some(e.to_string()),
            }));
        }
    }
}
