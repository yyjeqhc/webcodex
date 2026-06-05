use crate::action_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::{get_db, Message, MessageKind};

use super::get_projects;
use super::types::{ReportRequest, ReportResponse};
use salvo::prelude::*;
use serde_json::json;

#[handler]
pub async fn codex_report(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let started_at = chrono::Utc::now().timestamp();
    let explicit_session_id = request_action_session_id(req);
    let Some(projects) = get_projects(depot) else {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some("Projects not configured".to_string()),
        }));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some("No database".to_string()),
        }));
        return;
    };
    let body: ReportRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ReportResponse {
                success: false,
                report_id: None,
                message_id: None,
                path: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let _proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ReportResponse {
                success: false,
                report_id: None,
                message_id: None,
                path: None,
                error: Some(e),
            }));
            return;
        }
    };

    let report_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("{}_{}.json", timestamp, &report_id[..8]);
    let report_dir = std::env::var("DROP_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./data"))
        .join("reports");

    if let Err(e) = std::fs::create_dir_all(&report_dir) {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some(format!("Failed to create reports directory: {}", e)),
        }));
        return;
    }

    let report_path = report_dir.join(&filename);
    let report_json = serde_json::json!({
        "id": report_id,
        "project": body.project,
        "status": body.status,
        "title": body.title,
        "summary": body.summary,
        "channel": body.channel,
        "created_at": now.timestamp(),
    });
    if let Err(e) = std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&report_json).unwrap(),
    ) {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some(format!("Failed to write report: {}", e)),
        }));
        return;
    }

    // Write message to channel
    let msg_text = format!("[{}] {}\n\n{}", body.status, body.title, body.summary);
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        channel: body.channel.clone(),
        kind: MessageKind::Text,
        title: Some(format!("[codex] {}", body.title)),
        text: Some(msg_text),
        file_name: None,
        file_path: None,
        file_size: None,
        mime_type: None,
        created_at: now.timestamp(),
        expires_at: None,
    };
    let message_id = message.id.clone();
    if let Err(e) = db.insert_message(&message) {
        // Report was written but message failed
        let response = ReportResponse {
            success: true,
            report_id: Some(report_id),
            message_id: None,
            path: Some(report_path.to_string_lossy().to_string()),
            error: Some(format!("Report written but message insert failed: {}", e)),
        };
        record_action_event(
            &db,
            ActionAuditEventInput {
                explicit_session_id,
                session_title: None,
                endpoint: "/api/codex/report".to_string(),
                action_name: "writeProjectReport".to_string(),
                operation: Some(body.status.clone()),
                project: Some(body.project.clone()),
                status: "success".to_string(),
                http_status: Some(200),
                started_at,
                ended_at: chrono::Utc::now().timestamp(),
                duration_ms: 0,
                error_summary: response.error.clone(),
                warning_summary: response.error.clone(),
                changed_files: vec![report_path.to_string_lossy().to_string()],
                ids: json!({"report_id": response.report_id, "message_id": response.message_id}),
                summary: json!({"channel": body.channel, "title": body.title, "status": body.status}),
                request_bytes: None,
                response_bytes: None,
            },
        );
        res.render(Json(response));
        return;
    }

    let response = ReportResponse {
        success: true,
        report_id: Some(report_id),
        message_id: Some(message_id),
        path: Some(report_path.to_string_lossy().to_string()),
        error: None,
    };
    record_action_event(
        &db,
        ActionAuditEventInput {
            explicit_session_id,
            session_title: None,
            endpoint: "/api/codex/report".to_string(),
            action_name: "writeProjectReport".to_string(),
            operation: Some(body.status.clone()),
            project: Some(body.project.clone()),
            status: "success".to_string(),
            http_status: Some(200),
            started_at,
            ended_at: chrono::Utc::now().timestamp(),
            duration_ms: 0,
            error_summary: None,
            warning_summary: None,
            changed_files: vec![report_path.to_string_lossy().to_string()],
            ids: json!({"report_id": response.report_id, "message_id": response.message_id}),
            summary: json!({"channel": body.channel, "title": body.title, "status": body.status}),
            request_bytes: None,
            response_bytes: None,
        },
    );
    res.render(Json(response));
}
