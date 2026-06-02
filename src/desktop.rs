use crate::{
    get_db, json_error, CreateDesktopTaskRequest, DesktopTask, DesktopTaskClaimRequest,
    DesktopTaskEvent, DesktopTaskEventRequest, DesktopTaskOpRequest,
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

#[derive(Debug, serde::Serialize)]
struct DesktopTaskDetailResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<DesktopTask>,
    events: Vec<DesktopTaskEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DesktopTaskOpResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<DesktopTask>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tasks: Vec<DesktopTask>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    events: Vec<DesktopTaskEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn desktop_op_response(task: Option<DesktopTask>) -> Json<DesktopTaskOpResponse> {
    Json(DesktopTaskOpResponse {
        success: true,
        task,
        tasks: Vec::new(),
        events: Vec::new(),
        error: None,
    })
}

fn valid_status(status: &str) -> bool {
    matches!(
        status,
        "pending" | "running" | "completed" | "failed" | "needs_input" | "cancelled"
    )
}

fn create_task_from_fields(title: &str, instructions: &str, priority: Option<i64>) -> DesktopTask {
    let now = chrono::Utc::now().timestamp();
    DesktopTask {
        id: Uuid::new_v4().to_string(),
        title: title.trim().to_string(),
        instructions: instructions.trim().to_string(),
        status: "pending".to_string(),
        priority: priority.unwrap_or(0).clamp(-100, 100),
        claimed_by: None,
        last_event: Some("created".to_string()),
        screenshot_url: None,
        created_at: now,
        updated_at: now,
    }
}

#[handler]
pub async fn desktop_task_op(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let body: DesktopTaskOpRequest = match req.parse_json().await {
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
    match body.op.as_str() {
        "create" => {
            let title = body.title.unwrap_or_default();
            let instructions = body.instructions.unwrap_or_default();
            if title.trim().is_empty() || instructions.trim().is_empty() {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(
                    StatusCode::BAD_REQUEST,
                    "title and instructions are required",
                ));
                return;
            }
            let task = create_task_from_fields(&title, &instructions, body.priority);
            match db.insert_desktop_task(&task) {
                Ok(_) => res.render(desktop_op_response(Some(task))),
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        "list" => {
            if let Some(status) = body.status.as_deref() {
                if !valid_status(status) {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(json_error(StatusCode::BAD_REQUEST, "invalid status"));
                    return;
                }
            }
            match db.list_desktop_tasks(
                body.status.as_deref(),
                body.limit.unwrap_or(20).clamp(1, 100),
            ) {
                Ok(tasks) => res.render(Json(DesktopTaskOpResponse {
                    success: true,
                    task: None,
                    tasks,
                    events: Vec::new(),
                    error: None,
                })),
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        "get" => {
            let id = body.id.unwrap_or_default();
            match db.get_desktop_task(&id) {
                Ok(Some(task)) => match db.list_desktop_task_events(&id) {
                    Ok(events) => res.render(Json(DesktopTaskOpResponse {
                        success: true,
                        task: Some(task),
                        tasks: Vec::new(),
                        events,
                        error: None,
                    })),
                    Err(e) => {
                        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                        res.render(Json(DesktopTaskOpResponse {
                            success: false,
                            task: None,
                            tasks: Vec::new(),
                            events: Vec::new(),
                            error: Some(e.to_string()),
                        }));
                    }
                },
                Ok(None) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some("task not found".to_string()),
                    }));
                }
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        "claim_next" => {
            let worker = body.worker.unwrap_or_default();
            if worker.trim().is_empty() {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, "worker is required"));
                return;
            }
            match db.claim_next_desktop_task(worker.trim(), chrono::Utc::now().timestamp()) {
                Ok(task) => res.render(desktop_op_response(task)),
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        "claim" => {
            let id = body.id.unwrap_or_default();
            let worker = body.worker.unwrap_or_default();
            if id.is_empty() || worker.trim().is_empty() {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(
                    StatusCode::BAD_REQUEST,
                    "id and worker are required",
                ));
                return;
            }
            match db.claim_desktop_task(&id, worker.trim(), chrono::Utc::now().timestamp()) {
                Ok(Some(task)) => res.render(desktop_op_response(Some(task))),
                Ok(None) => {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some("task is missing or not pending".to_string()),
                    }));
                }
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        "event" => {
            let id = body.id.unwrap_or_default();
            if id.is_empty() {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, "id is required"));
                return;
            }
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
                Ok(Some(task)) => res.render(desktop_op_response(Some(task))),
                Ok(None) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some("task not found".to_string()),
                    }));
                }
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(DesktopTaskOpResponse {
                        success: false,
                        task: None,
                        tasks: Vec::new(),
                        events: Vec::new(),
                        error: Some(e.to_string()),
                    }));
                }
            }
        }
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                "unknown desktop task op",
            ));
        }
    }
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
pub async fn get_desktop_task_detail(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
    let id = req.param::<String>("id").unwrap_or_default();
    match db.get_desktop_task(&id) {
        Ok(Some(task)) => match db.list_desktop_task_events(&id) {
            Ok(events) => res.render(Json(DesktopTaskDetailResponse {
                success: true,
                task: Some(task),
                events,
                error: None,
            })),
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(DesktopTaskDetailResponse {
                    success: false,
                    task: None,
                    events: Vec::new(),
                    error: Some(e.to_string()),
                }));
            }
        },
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(DesktopTaskDetailResponse {
                success: false,
                task: None,
                events: Vec::new(),
                error: Some("task not found".to_string()),
            }));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(DesktopTaskDetailResponse {
                success: false,
                task: None,
                events: Vec::new(),
                error: Some(e.to_string()),
            }));
        }
    }
}

#[handler]
pub async fn claim_next_desktop_task(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No database"));
        return;
    };
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
    match db.claim_next_desktop_task(worker, chrono::Utc::now().timestamp()) {
        Ok(Some(task)) => res.render(Json(DesktopTaskResponse {
            success: true,
            task: Some(task),
            error: None,
        })),
        Ok(None) => res.render(Json(DesktopTaskResponse {
            success: true,
            task: None,
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
