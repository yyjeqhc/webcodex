use super::edit::validate_edit_path;
use super::get_projects;
use super::shell::sanitize_tail;
use super::shell::{shell_escape, shell_join_paths};
use super::types::{GitOperation, GitRequest, GitResponse};
use super::{run_project_cmd, CHECK_TIMEOUT_SECS, MAX_OUTPUT_LEN};
use crate::action_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::get_db;
use salvo::prelude::*;
use serde_json::json;

pub(super) const MAX_GIT_PATHS: usize = 50;
pub(super) const MAX_GIT_PATH_LEN: usize = 512;

pub(super) fn validate_git_paths(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("paths cannot be empty for this git operation".to_string());
    }
    if paths.len() > MAX_GIT_PATHS {
        return Err(format!("too many paths; maximum is {}", MAX_GIT_PATHS));
    }
    for path in paths {
        if path.chars().count() > MAX_GIT_PATH_LEN {
            return Err(format!(
                "path is too long; maximum is {} characters",
                MAX_GIT_PATH_LEN
            ));
        }
        validate_edit_path(path)?;
    }
    Ok(())
}

fn validate_git_commit_message(message: &str) -> Result<(), String> {
    let len = message.chars().count();
    if len == 0 {
        return Err("commit message cannot be empty".to_string());
    }
    if len > 200 {
        return Err("commit message is too long; maximum is 200 characters".to_string());
    }
    if message
        .chars()
        .any(|ch| ch == '\n' || ch == '\r' || ch == '\0')
    {
        return Err("commit message cannot contain newlines or NUL".to_string());
    }
    Ok(())
}

fn validate_checkpoint_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 80 {
        return Err("checkpoint_id must be 1..=80 characters".to_string());
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(
            "checkpoint_id may only contain ASCII letters, digits, '-', '_', and '.'".to_string(),
        );
    }
    Ok(())
}

fn resolved_checkpoint_id(body: &GitRequest) -> Result<String, String> {
    match body.checkpoint_id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => {
            validate_checkpoint_id(id)?;
            Ok(id.to_string())
        }
        Some(_) => Err("checkpoint_id cannot be empty".to_string()),
        None => Ok(format!("cp-{}", uuid::Uuid::new_v4())),
    }
}

pub(super) fn git_command_for_request(body: &GitRequest) -> Result<String, String> {
    match body.operation {
        GitOperation::Status => Ok("git status --short".to_string()),
        GitOperation::Diff => {
            if body.paths.is_empty() {
                Ok("git diff".to_string())
            } else {
                validate_git_paths(&body.paths)?;
                Ok(format!("git diff -- {}", shell_join_paths(&body.paths)))
            }
        }
        GitOperation::Log => Ok("git log --oneline -n 20".to_string()),
        GitOperation::Add => {
            validate_git_paths(&body.paths)?;
            Ok(format!("git add -- {}", shell_join_paths(&body.paths)))
        }
        GitOperation::Commit => {
            validate_git_paths(&body.paths)?;
            let message = body
                .message
                .as_deref()
                .ok_or_else(|| "message is required for commit".to_string())?;
            validate_git_commit_message(message)?;
            let paths = shell_join_paths(&body.paths);
            let message = shell_escape(message);
            Ok(format!(
                "git add -- {paths} && if git diff --cached --quiet -- {paths}; then echo 'No staged changes to commit' >&2; exit 1; fi; git commit -m {message} --no-verify"
            ))
        }
        GitOperation::CommitAmendNoEdit => {
            validate_git_paths(&body.paths)?;
            let paths = shell_join_paths(&body.paths);
            Ok(format!(
                "git add -- {paths} && if git diff --cached --quiet -- {paths}; then echo 'No staged changes to amend' >&2; exit 1; fi; git commit --amend --no-edit --no-verify"
            ))
        }
        GitOperation::Checkpoint => {
            let checkpoint_id = resolved_checkpoint_id(body)?;
            let patch_path = format!(".codex/checkpoints/{}.patch", checkpoint_id);
            Ok(format!(
                "set -e; mkdir -p .codex/checkpoints; git diff --binary > {patch}; git status --short > {status}; printf 'checkpoint_id=%s\\npatch=%s\\n' {id} {patch}; printf 'status:\\n'; cat {status}",
                id = shell_escape(&checkpoint_id),
                patch = shell_escape(&patch_path),
                status = shell_escape(&format!(".codex/checkpoints/{}.status", checkpoint_id)),
            ))
        }
        GitOperation::RollbackToCheckpoint => {
            let Some(checkpoint_id) = body.checkpoint_id.as_deref().map(str::trim) else {
                return Err("checkpoint_id is required for rollback_to_checkpoint".to_string());
            };
            validate_checkpoint_id(checkpoint_id)?;
            let patch_path = format!(".codex/checkpoints/{}.patch", checkpoint_id);
            Ok(format!(
                "set -e; patch={patch}; test -f \"$patch\" || {{ echo \"checkpoint not found: {id}\" >&2; exit 1; }}; current=$(mktemp); git diff --binary > \"$current\"; if [ -s \"$current\" ]; then git apply -R --whitespace=nowarn \"$current\"; fi; if [ -s \"$patch\" ]; then git apply --whitespace=nowarn \"$patch\"; fi; rm -f \"$current\"; printf 'rolled_back_to_checkpoint=%s\\n' {id}; git status --short",
                id = shell_escape(checkpoint_id),
                patch = shell_escape(&patch_path),
            ))
        }
    }
}

#[handler]
pub async fn codex_git(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let started_at = chrono::Utc::now().timestamp();
    let audit_db = get_db(depot);
    let explicit_session_id = request_action_session_id(req);
    let Some(projects) = get_projects(depot) else {
        res.render(Json(GitResponse {
            success: false,
            project: String::new(),
            operation: String::new(),
            exit_code: None,
            duration_ms: 0,
            stdout_tail: None,
            stderr_tail: None,
            truncated: false,
            error: Some("Projects not configured".to_string()),
        }));
        return;
    };
    let body: GitRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(GitResponse {
                success: false,
                project: String::new(),
                operation: String::new(),
                exit_code: None,
                duration_ms: 0,
                stdout_tail: None,
                stderr_tail: None,
                truncated: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(git_error(&body.project, &body.operation, e)));
            return;
        }
    };
    if matches!(
        body.operation,
        GitOperation::Add
            | GitOperation::Commit
            | GitOperation::CommitAmendNoEdit
            | GitOperation::RollbackToCheckpoint
    ) && !proj.allow_patch()
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(git_error(
            &body.project,
            &body.operation,
            "Git mutation is not allowed for this project".to_string(),
        )));
        return;
    }
    let cmd = match git_command_for_request(&body) {
        Ok(cmd) => cmd,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(git_error(&body.project, &body.operation, e)));
            return;
        }
    };
    let (code, stdout, stderr, duration_ms) = if proj.is_agent() {
        super::agent_exec::run_agent_project_command(
            depot,
            proj,
            &cmd,
            CHECK_TIMEOUT_SECS,
            "codex_git_agent_executor",
            "agent git command",
        )
        .await
    } else {
        run_project_cmd(proj, &cmd, CHECK_TIMEOUT_SECS, projects.ssh.as_ref())
    };
    let (stdout_tail, stdout_trunc) = sanitize_tail(&stdout, MAX_OUTPUT_LEN);
    let (stderr_tail, stderr_trunc) = sanitize_tail(&stderr, MAX_OUTPUT_LEN);
    let truncated = stdout_trunc || stderr_trunc;
    let success = code == 0;
    tracing::info!(
        target: "codex.metrics",
        operation = "runProjectGit",
        project = %body.project,
        git_operation = git_operation_name(&body.operation),
        executor = if proj.is_agent() { "agent" } else if proj.is_ssh() { "ssh" } else { "local" },
        success = success,
        exit_code = code,
        duration_ms = duration_ms,
        ssh_calls = if proj.is_ssh() { 1 } else { 0 },
        control_master = projects.ssh.as_ref().map(|s| s.control_master).unwrap_or(false),
        "codex_git_completed"
    );
    let response = GitResponse {
        success,
        project: body.project.clone(),
        operation: git_operation_name(&body.operation).to_string(),
        exit_code: Some(code),
        duration_ms,
        stdout_tail: Some(stdout_tail),
        stderr_tail: Some(stderr_tail),
        truncated,
        error: if success {
            None
        } else {
            Some("git operation failed".to_string())
        },
    };
    res.render(Json(response));
    if let Some(db) = audit_db.as_ref() {
        let ended_at = chrono::Utc::now().timestamp();
        record_action_event(
            db,
            ActionAuditEventInput {
                explicit_session_id,
                session_title: None,
                endpoint: "/api/codex/git".to_string(),
                action_name: "runProjectGit".to_string(),
                operation: Some(git_operation_name(&body.operation).to_string()),
                project: Some(body.project.clone()),
                status: if success {
                    "success".to_string()
                } else {
                    "failed".to_string()
                },
                http_status: Some(200),
                started_at,
                ended_at,
                duration_ms: duration_ms as i64,
                error_summary: if success {
                    None
                } else {
                    Some("git operation failed".to_string())
                },
                warning_summary: None,
                changed_files: body.paths.clone(),
                ids: json!({}),
                summary: json!({
                    "operation": git_operation_name(&body.operation),
                    "executor": if proj.is_agent() { "agent" } else if proj.is_ssh() { "ssh" } else { "local" },
                    "path_count": body.paths.len(),
                    "paths": body.paths,
                    "checkpoint_id": body.checkpoint_id,
                    "truncated": truncated,
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
    }
}

pub(super) fn git_operation_name(operation: &GitOperation) -> &'static str {
    match operation {
        GitOperation::Status => "status",
        GitOperation::Diff => "diff",
        GitOperation::Log => "log",
        GitOperation::Add => "add",
        GitOperation::Commit => "commit",
        GitOperation::CommitAmendNoEdit => "commit_amend_no_edit",
        GitOperation::Checkpoint => "checkpoint",
        GitOperation::RollbackToCheckpoint => "rollback_to_checkpoint",
    }
}

pub(super) fn git_error(project: &str, operation: &GitOperation, error: String) -> GitResponse {
    GitResponse {
        success: false,
        project: project.to_string(),
        operation: git_operation_name(operation).to_string(),
        exit_code: None,
        duration_ms: 0,
        stdout_tail: None,
        stderr_tail: None,
        truncated: false,
        error: Some(error),
    }
}
