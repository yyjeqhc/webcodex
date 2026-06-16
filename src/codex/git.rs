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
use crate::shell_protocol::ShellJobOpRequest;
use crate::ShellClientRegistry;
use salvo::prelude::*;
use serde_json::json;
use std::sync::Arc;

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

async fn run_agent_git_command(
    depot: &Depot,
    proj: &crate::projects::ProjectConfig,
    cmd: &str,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    let started = std::time::Instant::now();
    let client_id = match proj.agent_client_id() {
        Ok(client_id) => client_id.to_string(),
        Err(e) => return (-1, String::new(), e, 0),
    };
    let registry = match depot.obtain::<Arc<ShellClientRegistry>>() {
        Ok(registry) => registry.clone(),
        Err(_) => {
            return (
                -1,
                String::new(),
                "Shell client registry not configured".to_string(),
                0,
            )
        }
    };
    let job = match registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some(client_id),
                cwd: Some(proj.path.clone()),
                command: Some(cmd.to_string()),
                timeout_secs: Some(timeout_secs),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
            },
            "codex_git_agent_executor".to_string(),
        )
        .await
    {
        Ok(job) => job,
        Err(e) => return (-1, String::new(), e, started.elapsed().as_millis() as u64),
    };
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs.max(1) + 2);
    loop {
        if std::time::Instant::now() >= deadline {
            let _ = registry.stop_job(&job.job_id).await;
            let (_, stdout, stderr, _, _) = registry
                .job_log(&job.job_id, Some(1), Some(1), None)
                .await
                .unwrap_or_else(|_| (job.clone(), Some(String::new()), Some(String::new()), 1, 1));
            return (
                -1,
                stdout.unwrap_or_default(),
                format!(
                    "{}\nagent git command timed out after {} seconds",
                    stderr.unwrap_or_default(),
                    timeout_secs
                ),
                started.elapsed().as_millis() as u64,
            );
        }
        match registry.get_job(&job.job_id).await {
            Ok(info) => {
                if matches!(
                    info.status.as_str(),
                    "completed" | "failed" | "stopped" | "timeout" | "lost"
                ) {
                    let (_, stdout, stderr, _, _) = registry
                        .job_log(&job.job_id, Some(1), Some(1), None)
                        .await
                        .unwrap_or_else(|_| {
                            (info.clone(), Some(String::new()), Some(String::new()), 1, 1)
                        });
                    let code = info.exit_code.unwrap_or_else(|| {
                        if info.status == "completed" {
                            0
                        } else {
                            -1
                        }
                    });
                    let mut stderr = stderr.unwrap_or_default();
                    if let Some(error) = info.error {
                        if !error.trim().is_empty() {
                            if !stderr.trim().is_empty() {
                                stderr.push('\n');
                            }
                            stderr.push_str(&error);
                        }
                    }
                    return (
                        code,
                        stdout.unwrap_or_default(),
                        stderr,
                        info.duration_ms
                            .unwrap_or_else(|| started.elapsed().as_millis() as u64),
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(
                    if info.status == "queued" { 100 } else { 250 },
                ))
                .await;
            }
            Err(e) => return (-1, String::new(), e, started.elapsed().as_millis() as u64),
        }
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
        GitOperation::Add | GitOperation::Commit | GitOperation::CommitAmendNoEdit
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
        run_agent_git_command(depot, proj, &cmd, CHECK_TIMEOUT_SECS).await
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
                    "path_count": body.paths.len(),
                    "paths": body.paths,
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
