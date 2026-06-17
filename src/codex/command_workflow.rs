use super::command_request::{validate_command_request_reason, COMMAND_REQUEST_TTL_SECS};
use super::shell::sanitize_tail;
use super::types::CommandRequestResponse;
use super::{run_project_cmd, ssh_disabled_error, CHECK_TIMEOUT_SECS, MAX_OUTPUT_LEN};
use crate::projects::ProjectsConfig;
use crate::{CodexGoalRecord, Database};

pub(super) fn require_active_goal(
    db: &Database,
    goal_id: &str,
    project: &str,
) -> Result<CodexGoalRecord, String> {
    let goal = db
        .get_goal(goal_id)
        .map_err(|e| format!("Failed to load goal: {}", e))?
        .ok_or_else(|| "Goal not found".to_string())?;
    if goal.project != project {
        return Err("Goal project does not match request project".to_string());
    }
    if goal.status != "active" {
        return Err("Goal is not active".to_string());
    }
    let now = chrono::Utc::now().timestamp();
    if goal.expires_at < now {
        let _ = db.update_goal_status(&goal.id, "expired", now, Some("Goal expired"));
        return Err("Goal expired".to_string());
    }
    Ok(goal)
}

pub(super) fn approve_command_request_inner(
    projects: &ProjectsConfig,
    db: &Database,
    request_id: String,
    ssh_enabled: bool,
) -> CommandRequestResponse {
    let approved_at = chrono::Utc::now().timestamp();
    let min_created_at = approved_at - COMMAND_REQUEST_TTL_SECS;
    let mut record =
        match db.claim_command_request_for_execution(&request_id, approved_at, min_created_at) {
            Ok(Some(record)) => record,
            Ok(None) => match db.get_command_request(&request_id) {
                Ok(Some(record)) => {
                    if record.status == "pending" && record.created_at < min_created_at {
                        let error = "Command request expired".to_string();
                        let expired = db
                            .expire_command_request(&record.id, approved_at, &error)
                            .ok()
                            .flatten()
                            .unwrap_or(record);
                        return CommandRequestResponse {
                            success: false,
                            request_id: Some(expired.id.clone()),
                            record: Some(expired),
                            error: Some(error),
                        };
                    }
                    return CommandRequestResponse {
                        success: false,
                        request_id: Some(record.id.clone()),
                        record: Some(record),
                        error: Some("Command request is not pending".to_string()),
                    };
                }
                Ok(None) => {
                    return CommandRequestResponse {
                        success: false,
                        request_id: Some(request_id),
                        record: None,
                        error: Some("Command request not found".to_string()),
                    };
                }
                Err(e) => {
                    return CommandRequestResponse {
                        success: false,
                        request_id: Some(request_id),
                        record: None,
                        error: Some(format!("Failed to load command request: {}", e)),
                    };
                }
            },
            Err(e) => {
                return CommandRequestResponse {
                    success: false,
                    request_id: Some(request_id),
                    record: None,
                    error: Some(format!("Failed to claim command request: {}", e)),
                };
            }
        };
    let proj = match projects.get_project(&record.project) {
        Ok(p) => p,
        Err(e) => {
            record.status = "failed".to_string();
            record.executed_at = Some(chrono::Utc::now().timestamp());
            record.error = Some(e.clone());
            let _ = db.update_command_request_result(&record);
            return CommandRequestResponse {
                success: false,
                request_id: Some(record.id.clone()),
                record: Some(record),
                error: Some(e),
            };
        }
    };
    if !proj.allow_command_requests {
        let error = "Command requests are not enabled for this project".to_string();
        record.status = "failed".to_string();
        record.executed_at = Some(chrono::Utc::now().timestamp());
        record.error = Some(error.clone());
        let _ = db.update_command_request_result(&record);
        return CommandRequestResponse {
            success: false,
            request_id: Some(record.id.clone()),
            record: Some(record),
            error: Some(error),
        };
    }
    if proj.is_ssh() && !ssh_enabled {
        let error = ssh_disabled_error();
        record.status = "failed".to_string();
        record.executed_at = Some(chrono::Utc::now().timestamp());
        record.error = Some(error.clone());
        let _ = db.update_command_request_result(&record);
        return CommandRequestResponse {
            success: false,
            request_id: Some(record.id.clone()),
            record: Some(record),
            error: Some(error),
        };
    }
    let cmd = match record.command_text.clone() {
        Some(cmd) if !cmd.is_empty() => cmd,
        _ => {
            let error = "Command request is missing command_text snapshot".to_string();
            record.status = "failed".to_string();
            record.executed_at = Some(chrono::Utc::now().timestamp());
            record.error = Some(error.clone());
            let _ = db.update_command_request_result(&record);
            return CommandRequestResponse {
                success: false,
                request_id: Some(record.id.clone()),
                record: Some(record),
                error: Some(error),
            };
        }
    };
    let (code, stdout, stderr, _) =
        run_project_cmd(proj, &cmd, CHECK_TIMEOUT_SECS, projects.ssh.as_ref());
    let (stdout_tail, _) = sanitize_tail(&stdout, MAX_OUTPUT_LEN);
    let (stderr_tail, _) = sanitize_tail(&stderr, MAX_OUTPUT_LEN);
    record.status = if code == 0 { "completed" } else { "failed" }.to_string();
    record.approved_at = Some(approved_at);
    record.executed_at = Some(chrono::Utc::now().timestamp());
    record.exit_code = Some(code);
    record.stdout_tail = Some(stdout_tail);
    record.stderr_tail = Some(stderr_tail);
    record.error = if code == 0 {
        None
    } else {
        Some("command failed".to_string())
    };
    if let Err(e) = db.update_command_request_result(&record) {
        return CommandRequestResponse {
            success: false,
            request_id: Some(record.id.clone()),
            record: Some(record),
            error: Some(format!("Failed to update command request: {}", e)),
        };
    }
    CommandRequestResponse {
        success: code == 0,
        request_id: Some(record.id.clone()),
        record: Some(record),
        error: if code == 0 {
            None
        } else {
            Some("command failed".to_string())
        },
    }
}

pub(super) fn reject_command_request_inner(
    db: &Database,
    request_id: String,
    reason: Option<String>,
) -> CommandRequestResponse {
    if let Err(e) = validate_command_request_reason(&reason) {
        return CommandRequestResponse {
            success: false,
            request_id: Some(request_id),
            record: None,
            error: Some(e),
        };
    }
    let error = reason.unwrap_or_else(|| "Rejected by user".to_string());
    match db.reject_command_request(&request_id, chrono::Utc::now().timestamp(), &error) {
        Ok(Some(record)) => CommandRequestResponse {
            success: true,
            request_id: Some(record.id.clone()),
            record: Some(record),
            error: None,
        },
        Ok(None) => match db.get_command_request(&request_id) {
            Ok(Some(record)) => CommandRequestResponse {
                success: false,
                request_id: Some(record.id.clone()),
                record: Some(record),
                error: Some("Command request is not pending".to_string()),
            },
            Ok(None) => CommandRequestResponse {
                success: false,
                request_id: Some(request_id),
                record: None,
                error: Some("Command request not found".to_string()),
            },
            Err(e) => CommandRequestResponse {
                success: false,
                request_id: Some(request_id),
                record: None,
                error: Some(format!("Failed to load command request: {}", e)),
            },
        },
        Err(e) => CommandRequestResponse {
            success: false,
            request_id: Some(request_id),
            record: None,
            error: Some(format!("Failed to reject command request: {}", e)),
        },
    }
}
