use super::get_projects;
use super::shell::shell_escape;
use super::trusted::{
    check_background_escape, check_denylist, check_secret_read, validate_trusted_script,
};
use super::types::{job_response, JobOpRequest, JobOpResponse};
use crate::action_sessions::{
    record_action_event, request_action_session_id, summarize_command_text, ActionAuditEventInput,
};
use crate::get_db;
use crate::projects::ProjectConfig;
use salvo::prelude::*;
use serde_json::json;

use super::security::is_sensitive_path;
use super::types::{JobInfo, JobMetadata};
use crate::shell_client::requested_by_from_auth;
use crate::shell_protocol::{
    ShellJobCodexMetadata, ShellJobInfo as AgentShellJobInfo, ShellJobOpRequest,
};
use crate::ShellClientRegistry;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const DEFAULT_JOB_MAX_RUNTIME_SECS: i64 = 3600;
const MAX_JOB_MAX_RUNTIME_SECS: i64 = 604800;

fn record_job_action_event(
    db: &std::sync::Arc<crate::Database>,
    explicit_session_id: Option<String>,
    started_at: i64,
    body: &JobOpRequest,
    response: &JobOpResponse,
    http_status: i64,
) {
    let ended_at = chrono::Utc::now().timestamp();
    let jobs = response
        .job
        .clone()
        .into_iter()
        .chain(response.jobs.clone())
        .collect::<Vec<_>>();
    let resolved_project = body
        .project
        .clone()
        .or_else(|| response.job.as_ref().map(|job| job.project.clone()))
        .or_else(|| response.jobs.first().map(|job| job.project.clone()));
    let status = if response.success {
        "success".to_string()
    } else if http_status == StatusCode::BAD_REQUEST.as_u16() as i64
        || http_status == StatusCode::FORBIDDEN.as_u16() as i64
        || http_status == StatusCode::NOT_FOUND.as_u16() as i64
        || http_status == StatusCode::CONFLICT.as_u16() as i64
    {
        "rejected".to_string()
    } else {
        "failed".to_string()
    };
    let command_summary = body
        .command
        .as_deref()
        .map(|text| summarize_command_text("command_text", text));
    let script_summary = body
        .script_text
        .as_deref()
        .map(|text| summarize_command_text("script_text", text));
    let summary_length = response
        .summary_markdown
        .as_ref()
        .map(|s| s.chars().count());
    let stdout_chars = response.stdout_tail.as_ref().map(|s| s.chars().count());
    let stderr_chars = response.stderr_tail.as_ref().map(|s| s.chars().count());
    let log_truncated = matches!(body.op.as_str(), "log" | "status")
        && response
            .log_total_lines
            .map(|total| total > body.tail_lines)
            .unwrap_or(false);
    record_action_event(
        db,
        ActionAuditEventInput {
            explicit_session_id,
            session_title: None,
            endpoint: "/api/codex/job".to_string(),
            action_name: "runJobOp".to_string(),
            operation: Some(body.op.clone()),
            project: resolved_project,
            status,
            http_status: Some(http_status),
            started_at,
            ended_at,
            duration_ms: (ended_at - started_at).max(0) * 1000,
            error_summary: response.error.clone(),
            warning_summary: if response.warnings.is_empty() {
                None
            } else {
                Some(response.warnings.join(" | "))
            },
            changed_files: Vec::new(),
            ids: json!({
                "job_id": response.job_id,
                "job_ids": response.job_ids,
                "client_request_id": body.client_request_id,
                "goal_id": body.goal_id,
            }),
            summary: json!({
                "project": body.project,
                "suite": body.suite,
                "status": response.job.as_ref().map(|job| job.status.clone()),
                "job_statuses": jobs.iter().map(|job| job.status.clone()).collect::<Vec<_>>(),
                "exit_code": response.job.as_ref().and_then(|job| job.exit_code),
                "exit_codes": jobs.iter().filter_map(|job| job.exit_code).collect::<Vec<_>>(),
                "metadata_only": response.metadata_only,
                "logs_included": response.logs_included,
                "log_total_lines": response.log_total_lines,
                "summary_length": summary_length,
                "stdout_chars": stdout_chars,
                "stderr_chars": stderr_chars,
                "log_truncated": log_truncated,
                "tail_lines": body.tail_lines,
                "since_line": body.since_line,
                "command": command_summary,
                "script_text": script_summary,
                "trusted": body.trusted,
                "max_runtime_secs": body.max_runtime_secs,
                "job_count": jobs.len().max(response.job_ids.len()),
            }),
            request_bytes: None,
            response_bytes: None,
        },
    );
}

fn job_recommended_next_action(response: &JobOpResponse) -> String {
    if !response.success {
        return match response.op.as_str() {
            "recover" => {
                "Use job_id or client_request_id from the create response, then retry recover."
                    .to_string()
            }
            "status" | "log" => {
                "Use recover with client_request_id if the job_id is unknown.".to_string()
            }
            _ => "Fix the rejected job request, then retry only that operation.".to_string(),
        };
    }
    match response.op.as_str() {
        "create" | "check" | "create_batch" => {
            "Poll runJobOp status with detail=basic; use detail=logs or op=log only when logs are needed."
                .to_string()
        }
        "status" => {
            if response.logs_included == Some(true) {
                "Use next_cursor with op=log for incremental logs, or proceed when job status is complete."
                    .to_string()
            } else {
                "If still running, poll status later; if details are needed, call status with detail=logs."
                    .to_string()
            }
        }
        "log" => {
            "Use next_cursor as since_line for incremental log polling; avoid rereading old log lines."
                .to_string()
        }
        "recover" => {
            "Use status detail=basic to refresh the recovered job, or detail=logs only if output is needed."
                .to_string()
        }
        "list" | "summarize" => "Choose a job_id, then use status detail=basic before reading logs.".to_string(),
        "stop" => "Call status detail=basic to confirm the stopped job state.".to_string(),
        _ => "Use the returned job_id/client_request_id for follow-up status or log calls.".to_string(),
    }
}

fn apply_job_workflow_hints(response: &mut JobOpResponse) {
    if response.recommended_next_action.is_none() {
        response.recommended_next_action = Some(job_recommended_next_action(response));
    }
    if response.action_budget_hint.is_none() {
        response.action_budget_hint = Some(
            "Prefer status detail=basic; use op=log with since_line/next_cursor for incremental logs."
                .to_string(),
        );
    }
}

pub(super) fn validate_job_id(job_id: &str) -> Result<(), String> {
    if job_id.is_empty()
        || job_id.len() > 80
        || !job_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid job_id".to_string());
    }
    Ok(())
}

pub(super) fn validate_client_request_id(client_request_id: &str) -> Result<(), String> {
    if client_request_id.is_empty()
        || client_request_id.len() > 128
        || !client_request_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("invalid client_request_id".to_string());
    }
    Ok(())
}

pub(super) fn validate_job_command(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if command.len() > 8000 {
        return Err("command is too long; maximum is 8000 bytes".to_string());
    }
    if command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    Ok(())
}

pub(super) fn validate_job_script_path(script_path: &str) -> Result<(), String> {
    let path = script_path.trim();
    if path.is_empty() {
        return Err("script_path cannot be empty".to_string());
    }
    if path.len() > 512 {
        return Err("script_path is too long; maximum is 512 bytes".to_string());
    }
    if path.contains('\0') {
        return Err("script_path cannot contain NUL bytes".to_string());
    }
    if path.starts_with('-') {
        return Err("script_path cannot start with '-'".to_string());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("script_path must be project-relative".to_string());
    }
    for component in p.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(
                    "script_path must not contain traversal or absolute components".to_string(),
                )
            }
        }
    }
    if is_sensitive_path(path) {
        return Err("script_path points to a sensitive or blocked path".to_string());
    }
    Ok(())
}

pub(super) fn validate_job_script_args(args: &[String]) -> Result<(), String> {
    if args.len() > 100 {
        return Err("script_args must contain at most 100 items".to_string());
    }
    for arg in args {
        if arg.len() > 2000 {
            return Err("script_args item is too long; maximum is 2000 bytes".to_string());
        }
        if arg.contains('\0') {
            return Err("script_args cannot contain NUL bytes".to_string());
        }
    }
    Ok(())
}

/// Build the command that executes a trusted script file in a job directory.
/// The script is written to `.codex/jobs/<job_id>/script.sh` and the command
/// simply runs `bash .codex/jobs/<job_id>/script.sh`.
pub(super) fn build_trusted_script_job_command(job_id: &str) -> String {
    let script_rel = format!(".codex/jobs/{}/script.sh", job_id);
    let script_q = shell_escape(&script_rel);
    format!(
        "test -f {script} || {{ echo 'trusted script not found' >&2; exit 127; }}; bash {script}",
        script = script_q
    )
}

/// Build the content for a trusted script.sh file.
/// Includes shebang, `set -euo pipefail`, and the user script.
pub(super) fn build_trusted_script_content(script_text: &str) -> String {
    format!(
        "#!/usr/bin/env bash\nset -euo pipefail\n{}\n",
        script_text.trim()
    )
}

pub(super) fn build_script_job_command(
    script_path: &str,
    script_args: &[String],
) -> Result<String, String> {
    validate_job_script_path(script_path)?;
    validate_job_script_args(script_args)?;
    let script_q = shell_escape(script_path.trim());
    let args_q = script_args
        .iter()
        .map(|arg| shell_escape(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let mut command = format!(
        "test -f {script} || {{ echo 'script_path not found or not a file' >&2; exit 127; }}; bash {script}",
        script = script_q
    );
    if !args_q.is_empty() {
        command.push(' ');
        command.push_str(&args_q);
    }
    Ok(command)
}

pub(super) fn validate_job_runtime(max_runtime_secs: Option<i64>) -> Result<i64, String> {
    let secs = max_runtime_secs.unwrap_or(DEFAULT_JOB_MAX_RUNTIME_SECS);
    if !(1..=MAX_JOB_MAX_RUNTIME_SECS).contains(&secs) {
        return Err(format!(
            "max_runtime_secs must be between 1 and {}",
            MAX_JOB_MAX_RUNTIME_SECS
        ));
    }
    Ok(secs)
}

pub(super) fn job_dir_rel(job_id: &str) -> String {
    format!(".codex/jobs/{}", job_id)
}

pub(super) fn local_job_dir(root: &Path, job_id: &str) -> PathBuf {
    root.join(job_dir_rel(job_id))
}

pub(super) fn read_file_to_string(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub(super) fn write_status_file(path: &Path, status: &str) {
    let _ = std::fs::write(path.join("status"), status);
}

pub(super) fn write_finished_at_file(path: &Path, timestamp: i64) {
    let _ = std::fs::write(path.join("finished_at"), timestamp.to_string());
}

pub(super) fn read_finished_at_file(path: &Path) -> Option<i64> {
    read_file_to_string(&path.join("finished_at")).and_then(|s| s.parse::<i64>().ok())
}

pub(super) fn tail_lines_from_text(text: &str, tail_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(tail_lines.max(1));
    lines[start..].join("\n")
}

/// Read lines starting from since_line (1-based). Returns (text, total_line_count).
pub(super) fn read_lines_from(text: &str, since_line: usize, max_lines: usize) -> (String, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    if since_line == 0 || since_line > total {
        return (String::new(), total);
    }
    let start = since_line - 1; // convert to 0-based
    let end = (start + max_lines).min(total);
    (lines[start..end].join("\n"), total)
}

pub(super) fn tail_file(path: &Path, tail_lines: usize) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| tail_lines_from_text(&s, tail_lines))
}

/// Returns (text, total_line_count).
pub(super) fn tail_file_with_count(path: &Path, tail_lines: usize) -> (Option<String>, usize) {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let total = s.lines().count();
            (Some(tail_lines_from_text(&s, tail_lines)), total)
        }
        Err(_) => (None, 0),
    }
}

/// Returns (text, total_line_count).
pub(super) fn read_from_line_with_count(
    path: &Path,
    since_line: usize,
    max_lines: usize,
) -> (Option<String>, usize) {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let (text, total) = read_lines_from(&s, since_line, max_lines);
            (Some(text), total)
        }
        Err(_) => (None, 0),
    }
}

/// Check stderr content for OOM signals.
pub(super) fn detect_oom_hint(stderr: &str) -> Option<String> {
    let lower = stderr.to_ascii_lowercase();
    let oom_patterns = [
        "out of memory",
        "oom",
        "killed",
        "memory error",
        "memoryerror",
        "cuda out of memory",
        "cudaoutofmemoryerror",
        "cannot allocate memory",
        "bus error",
    ];
    if oom_patterns.iter().any(|p| lower.contains(p)) {
        Some("possible_oom".to_string())
    } else {
        None
    }
}

pub(super) fn read_job_metadata_local(root: &Path, job_id: &str) -> Result<JobMetadata, String> {
    validate_job_id(job_id)?;
    let path = local_job_dir(root, job_id).join("metadata.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read job metadata: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse job metadata: {}", e))
}

pub(super) fn pid_running_local(pid: i64) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(super) fn update_job_status_local(root: &Path, meta: &JobMetadata) -> JobInfo {
    let dir = local_job_dir(root, &meta.job_id);
    let now = chrono::Utc::now().timestamp();
    let pid = read_file_to_string(&dir.join("pid")).and_then(|s| s.parse::<i64>().ok());
    let exit_code = read_file_to_string(&dir.join("exit_code")).and_then(|s| s.parse::<i32>().ok());
    let mut status =
        read_file_to_string(&dir.join("status")).unwrap_or_else(|| meta.status.clone());
    if status == "running" {
        if let Some(pid) = pid {
            if meta.started_at.unwrap_or(meta.created_at) + meta.max_runtime_secs < now {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
                status = "timeout".to_string();
                write_status_file(&dir, &status);
                write_finished_at_file(&dir, now);
            } else if !pid_running_local(pid) {
                status = if exit_code.unwrap_or(1) == 0 {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                write_status_file(&dir, &status);
                if read_finished_at_file(&dir).is_none() {
                    write_finished_at_file(&dir, now);
                }
            }
        }
    }
    let finished_at = read_finished_at_file(&dir).or(meta.finished_at);
    // Compute elapsed_secs: wall-clock time from started_at to finished_at (or now if running)
    let elapsed_secs = meta.started_at.map(|s| finished_at.unwrap_or(now) - s);
    // Detect OOM hint from stderr tail
    let oom_hint = tail_file(&dir.join("stderr.log"), 40)
        .as_deref()
        .and_then(|s| detect_oom_hint(s));
    JobInfo {
        job_id: meta.job_id.clone(),
        client_request_id: meta.client_request_id.clone(),
        project: meta.project.clone(),
        goal_id: meta.goal_id.clone(),
        command: meta.command.clone(),
        kind: meta.kind.clone(),
        suite: meta.suite.clone(),
        script_path: meta.script_path.clone(),
        reason: meta.reason.clone(),
        status,
        created_at: meta.created_at,
        started_at: meta.started_at,
        finished_at,
        max_runtime_secs: meta.max_runtime_secs,
        executor: meta.executor.clone(),
        pid,
        exit_code,
        elapsed_secs,
        oom_hint,
    }
}

pub(super) fn local_job_info(root: &Path, job_id: &str) -> Result<JobInfo, String> {
    let meta = read_job_metadata_local(root, job_id)?;
    Ok(update_job_status_local(root, &meta))
}

pub(super) fn create_local_job(
    proj: &ProjectConfig,
    project: &str,
    goal_id: &str,
    command: &str,
    client_request_id: Option<String>,
    kind: Option<String>,
    suite: Option<String>,
    script_path: Option<String>,
    reason: Option<String>,
    max_runtime_secs: i64,
    trusted_script_text: Option<&str>,
) -> Result<JobInfo, String> {
    // For trusted_script_text, the command is a placeholder that will be
    // replaced with the actual script.sh path after generating job_id.
    let is_trusted_script = trusted_script_text.is_some();
    if let Some(script_text) = trusted_script_text {
        validate_trusted_script(script_text)?;
    }
    if !is_trusted_script {
        validate_job_command(command)?;
    }
    let root = proj.root();
    let job_id = uuid::Uuid::new_v4().to_string();
    let dir = local_job_dir(&root, &job_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create job dir: {}", e))?;
    // Determine the actual command to run
    let actual_command = if is_trusted_script {
        // Write script.sh BEFORE spawning the job
        let script_text = trusted_script_text.unwrap();
        let script_content = build_trusted_script_content(script_text);
        std::fs::write(dir.join("script.sh"), &script_content)
            .map_err(|e| format!("Failed to write trusted script.sh: {}", e))?;
        build_trusted_script_job_command(&job_id)
    } else {
        command.to_string()
    };
    let now = chrono::Utc::now().timestamp();
    let meta = JobMetadata {
        job_id: job_id.clone(),
        client_request_id,
        project: project.to_string(),
        goal_id: goal_id.to_string(),
        command: actual_command.clone(),
        kind,
        suite,
        script_path,
        reason,
        status: "running".to_string(),
        created_at: now,
        started_at: Some(now),
        finished_at: None,
        max_runtime_secs,
        executor: "local".to_string(),
        host: None,
        path: proj.path.clone(),
    };
    std::fs::write(
        dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write metadata: {}", e))?;
    std::fs::write(
        dir.join("command.sh"),
        format!("#!/usr/bin/env bash\n{}\n", actual_command),
    )
    .map_err(|e| format!("Failed to write command.sh: {}", e))?;
    std::fs::write(dir.join("status"), "running")
        .map_err(|e| format!("Failed to write status: {}", e))?;
    let dir_s = dir.to_string_lossy().to_string();
    let wrapper = format!(
        "bash {0}/command.sh > {0}/stdout.log 2> {0}/stderr.log; code=$?; echo $code > {0}/exit_code; finished=$(date +%s); echo $finished > {0}/finished_at; if [ $code -eq 0 ]; then echo completed > {0}/status; else echo failed > {0}/status; fi",
        shell_escape(&dir_s)
    );
    let child = std::process::Command::new("setsid")
        .arg("sh")
        .arg("-c")
        .arg(wrapper)
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn job: {}", e))?;
    std::fs::write(dir.join("pid"), child.id().to_string())
        .map_err(|e| format!("Failed to write pid: {}", e))?;
    Ok(update_job_status_local(&root, &meta))
}

pub(super) fn list_local_jobs(
    root: &Path,
    limit: usize,
    status_filter: Option<&str>,
) -> Vec<JobInfo> {
    let jobs_dir = root.join(".codex/jobs");
    let mut jobs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(jobs_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if jobs.len() >= limit {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if validate_job_id(&name).is_ok() {
                if let Ok(info) = local_job_info(root, &name) {
                    if status_filter.map(|s| s == info.status).unwrap_or(true) {
                        jobs.push(info);
                    }
                }
            }
        }
    }
    jobs.sort_by_key(|j| -j.created_at);
    jobs.truncate(limit);
    jobs
}

pub(super) fn filter_jobs_by_client_request_id(
    jobs: &mut Vec<JobInfo>,
    client_request_id: Option<&str>,
) {
    if let Some(client_request_id) = client_request_id {
        jobs.retain(|j| j.client_request_id.as_deref() == Some(client_request_id));
    }
}

pub(super) fn local_job_log(
    root: &Path,
    job_id: &str,
    tail_lines: usize,
    since_line: Option<usize>,
) -> Result<(String, String, usize), String> {
    validate_job_id(job_id)?;
    let dir = local_job_dir(root, job_id);
    let stdout_path = dir.join("stdout.log");
    let stderr_path = dir.join("stderr.log");
    let (stdout_text, total_lines) = if let Some(sl) = since_line.filter(|&n| n > 0) {
        read_from_line_with_count(&stdout_path, sl, tail_lines)
    } else {
        tail_file_with_count(&stdout_path, tail_lines)
    };
    let (stderr_text, _) = tail_file_with_count(&stderr_path, tail_lines);
    Ok((
        stdout_text.unwrap_or_default(),
        stderr_text.unwrap_or_default(),
        total_lines,
    ))
}

fn kill_local_tree(pid: i64, signal: &str) {
    let children = std::process::Command::new("pgrep")
        .arg("-P")
        .arg(pid.to_string())
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();
    for child in children.lines() {
        if let Ok(child_pid) = child.trim().parse::<i64>() {
            kill_local_tree(child_pid, signal);
        }
    }
    let _ = std::process::Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status();
}

pub(super) fn stop_local_job(root: &Path, job_id: &str) -> Result<JobInfo, String> {
    validate_job_id(job_id)?;
    let meta = read_job_metadata_local(root, job_id)?;
    let dir = local_job_dir(root, job_id);
    if let Some(pid) = read_file_to_string(&dir.join("pid")).and_then(|s| s.parse::<i64>().ok()) {
        kill_local_tree(pid, "-TERM");
        std::thread::sleep(std::time::Duration::from_millis(300));
        if pid_running_local(pid) {
            kill_local_tree(pid, "-KILL");
        }
    }
    write_status_file(&dir, "stopped");
    write_finished_at_file(&dir, chrono::Utc::now().timestamp());
    Ok(update_job_status_local(root, &meta))
}

pub(super) fn summarize_jobs_markdown(jobs: &[JobInfo], log_tails: &[(String, String)]) -> String {
    let mut md = String::from("# Codex job summary\n\n| job_id | kind | suite | status | exit_code | duration_secs | command |\n|---|---|---|---:|---:|---:|---|\n");
    for job in jobs {
        let duration = job
            .finished_at
            .or_else(|| Some(chrono::Utc::now().timestamp()))
            .unwrap_or(job.created_at)
            - job.started_at.unwrap_or(job.created_at);
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | `{}` |\n",
            job.job_id,
            job.kind.as_deref().unwrap_or("command"),
            job.suite.as_deref().unwrap_or(""),
            job.status,
            job.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "".to_string()),
            duration.max(0),
            job.command.replace('`', "'")
        ));
    }
    for (idx, job) in jobs.iter().enumerate() {
        let (stdout_tail, stderr_tail) = log_tails.get(idx).cloned().unwrap_or_default();
        md.push_str(&format!("\n## {}\n\n", job.job_id));
        md.push_str("### stdout tail\n\n```text\n");
        md.push_str(&stdout_tail);
        md.push_str("\n```\n\n### stderr tail\n\n```text\n");
        md.push_str(&stderr_tail);
        md.push_str("\n```\n");
    }
    md
}

// --- recover / status detail=basic helpers ---

/// Determine the effective detail level for op=status.
/// Only explicit "logs" triggers log reading; everything else is "basic".
/// This function is extracted for testability.
fn effective_status_detail(detail: Option<&str>) -> &'static str {
    match detail {
        Some("logs") => "logs",
        _ => "basic",
    }
}

/// Lightweight metadata-only job info for local jobs.
/// Reads metadata.json + status/exit_code/finished_at files only.
/// No kill -0, no OOM detection, no log reading.
fn recover_local_job_info(root: &Path, job_id: &str) -> Result<JobInfo, String> {
    let meta = read_job_metadata_local(root, job_id)?;
    let dir = local_job_dir(root, job_id);
    let status = read_file_to_string(&dir.join("status")).unwrap_or_else(|| meta.status.clone());
    let exit_code = read_file_to_string(&dir.join("exit_code")).and_then(|s| s.parse::<i32>().ok());
    let pid = read_file_to_string(&dir.join("pid")).and_then(|s| s.parse::<i64>().ok());
    let finished_at = read_finished_at_file(&dir).or(meta.finished_at);
    let now = chrono::Utc::now().timestamp();
    let elapsed_secs = meta.started_at.map(|s| finished_at.unwrap_or(now) - s);
    Ok(JobInfo {
        job_id: meta.job_id,
        client_request_id: meta.client_request_id,
        project: meta.project,
        goal_id: meta.goal_id,
        command: meta.command,
        kind: meta.kind,
        suite: meta.suite,
        script_path: meta.script_path,
        reason: meta.reason,
        status,
        created_at: meta.created_at,
        started_at: meta.started_at,
        finished_at,
        max_runtime_secs: meta.max_runtime_secs,
        executor: meta.executor,
        pid,
        exit_code,
        elapsed_secs,
        oom_hint: None, // No OOM detection in recover
    })
}

/// Lightweight metadata-only job info for SSH jobs.
/// Single SSH call: reads metadata.json + status/pid/exit_code/finished_at.
/// Find a local job ID by client_request_id. Only reads metadata.json (no status update).
fn find_local_job_id_by_client_request_id(
    root: &Path,
    client_request_id: &str,
    goal_id: Option<&str>,
) -> Option<String> {
    let jobs_dir = root.join(".codex/jobs");
    let mut candidates: Vec<(i64, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&jobs_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if validate_job_id(&name).is_ok() {
                if let Ok(meta) = read_job_metadata_local(root, &name) {
                    if meta.client_request_id.as_deref() == Some(client_request_id) {
                        if goal_id.map(|g| g == meta.goal_id).unwrap_or(true) {
                            candidates.push((meta.created_at, name));
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.first().map(|(_, id)| id.clone())
}

/// Lightweight status update for local jobs (skip OOM detection).
fn update_job_status_local_basic(root: &Path, meta: &JobMetadata) -> JobInfo {
    let dir = local_job_dir(root, &meta.job_id);
    let now = chrono::Utc::now().timestamp();
    let pid = read_file_to_string(&dir.join("pid")).and_then(|s| s.parse::<i64>().ok());
    let exit_code = read_file_to_string(&dir.join("exit_code")).and_then(|s| s.parse::<i32>().ok());
    let mut status =
        read_file_to_string(&dir.join("status")).unwrap_or_else(|| meta.status.clone());
    if status == "running" {
        if let Some(pid) = pid {
            if meta.started_at.unwrap_or(meta.created_at) + meta.max_runtime_secs < now {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
                status = "timeout".to_string();
                write_status_file(&dir, &status);
                write_finished_at_file(&dir, now);
            } else if !pid_running_local(pid) {
                status = if exit_code.unwrap_or(1) == 0 {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                write_status_file(&dir, &status);
                if read_finished_at_file(&dir).is_none() {
                    write_finished_at_file(&dir, now);
                }
            }
        }
    }
    let finished_at = read_finished_at_file(&dir).or(meta.finished_at);
    let elapsed_secs = meta.started_at.map(|s| finished_at.unwrap_or(now) - s);
    // Skip OOM detection (no stderr read) for basic detail
    JobInfo {
        job_id: meta.job_id.clone(),
        client_request_id: meta.client_request_id.clone(),
        project: meta.project.clone(),
        goal_id: meta.goal_id.clone(),
        command: meta.command.clone(),
        kind: meta.kind.clone(),
        suite: meta.suite.clone(),
        script_path: meta.script_path.clone(),
        reason: meta.reason.clone(),
        status,
        created_at: meta.created_at,
        started_at: meta.started_at,
        finished_at,
        max_runtime_secs: meta.max_runtime_secs,
        executor: meta.executor.clone(),
        pid,
        exit_code,
        elapsed_secs,
        oom_hint: None,
    }
}

/// Lightweight local job info (basic detail, no OOM detection).
fn local_job_info_basic(root: &Path, job_id: &str) -> Result<JobInfo, String> {
    let meta = read_job_metadata_local(root, job_id)?;
    Ok(update_job_status_local_basic(root, &meta))
}

/// Lightweight status update for SSH jobs (skip kill -0, kill_tree, OOM detection).
/// Note: basic is metadata/status-file based and may be stale for SSH jobs;
fn agent_shell_job_to_job_info(
    info: AgentShellJobInfo,
    expected_client_id: Option<&str>,
    expected_project: Option<&str>,
    stderr_tail: Option<&str>,
) -> Option<JobInfo> {
    if let Some(expected_client_id) = expected_client_id {
        if info.client_id != expected_client_id {
            return None;
        }
    }
    let codex = info.codex?;
    let project = codex.project?;
    if let Some(expected_project) = expected_project {
        if project != expected_project {
            return None;
        }
    }
    let goal_id = codex.goal_id?;
    let now = chrono::Utc::now().timestamp();
    let finished_at = info.ended_at;
    let elapsed_secs = if let Some(duration_ms) = info.duration_ms {
        Some((duration_ms / 1000) as i64)
    } else {
        info.started_at
            .map(|started_at| finished_at.unwrap_or(now).saturating_sub(started_at))
    };
    Some(JobInfo {
        job_id: info.job_id,
        client_request_id: codex.client_request_id,
        project,
        goal_id,
        command: codex.command.unwrap_or(info.command_preview),
        kind: codex.kind,
        suite: codex.suite,
        script_path: codex.script_path,
        reason: codex.reason,
        status: info.status,
        created_at: info.created_at,
        started_at: info.started_at,
        finished_at,
        max_runtime_secs: codex
            .max_runtime_secs
            .unwrap_or(DEFAULT_JOB_MAX_RUNTIME_SECS),
        executor: "agent".to_string(),
        pid: None,
        exit_code: info.exit_code,
        elapsed_secs,
        oom_hint: stderr_tail.and_then(detect_oom_hint),
    })
}

async fn resolve_agent_registry(
    depot: &Depot,
    proj: &ProjectConfig,
) -> Result<(String, Arc<ShellClientRegistry>), String> {
    super::agent_exec::resolve_agent_project_client(depot, proj).await
}

async fn create_agent_job(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    goal_id: &str,
    command: &str,
    client_request_id: Option<String>,
    kind: Option<String>,
    suite: Option<String>,
    script_path: Option<String>,
    reason: Option<String>,
    max_runtime_secs: i64,
) -> Result<JobInfo, String> {
    validate_job_command(command)?;
    let timeout_secs = max_runtime_secs
        .try_into()
        .map_err(|_| "max_runtime_secs is invalid".to_string())?;
    let (client_id, registry) = resolve_agent_registry(depot, proj).await?;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok();
    let requested_by = requested_by_from_auth(auth);
    let shell_job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some(client_id.clone()),
                cwd: Some(proj.path.clone()),
                command: Some(command.to_string()),
                timeout_secs: Some(timeout_secs),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: Some(ShellJobCodexMetadata {
                    project: Some(project.to_string()),
                    goal_id: Some(goal_id.to_string()),
                    client_request_id,
                    command: Some(command.to_string()),
                    kind,
                    suite,
                    script_path,
                    reason,
                    max_runtime_secs: Some(max_runtime_secs),
                }),
            },
            requested_by,
        )
        .await?;
    agent_shell_job_to_job_info(shell_job, Some(&client_id), Some(project), None)
        .ok_or_else(|| "agent job metadata was not recorded".to_string())
}

async fn list_agent_jobs(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    limit: usize,
    status_filter: Option<&str>,
) -> Result<Vec<JobInfo>, String> {
    let (client_id, registry) = resolve_agent_registry(depot, proj).await?;
    let mut jobs = registry
        .list_jobs(Some(limit.max(100).clamp(1, 100)))
        .await
        .into_iter()
        .filter_map(|info| agent_shell_job_to_job_info(info, Some(&client_id), Some(project), None))
        .filter(|info| status_filter.map(|s| s == info.status).unwrap_or(true))
        .collect::<Vec<_>>();
    jobs.sort_by_key(|j| -j.created_at);
    jobs.truncate(limit);
    Ok(jobs)
}

async fn agent_job_info_basic(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    job_id: &str,
) -> Result<JobInfo, String> {
    validate_job_id(job_id)?;
    let (client_id, registry) = resolve_agent_registry(depot, proj).await?;
    let info = registry.get_job(job_id).await?;
    if info.client_id != client_id {
        return Err("agent shell job is not a Codex job for this project".to_string());
    }
    agent_shell_job_to_job_info(info, Some(&client_id), Some(project), None)
        .ok_or_else(|| "agent shell job is not a Codex job for this project".to_string())
}

async fn agent_job_log_with_count(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    job_id: &str,
    tail_lines: usize,
    since_line: Option<usize>,
) -> Result<(String, String, usize), String> {
    validate_job_id(job_id)?;
    let (client_id, registry) = resolve_agent_registry(depot, proj).await?;
    let info = registry.get_job(job_id).await?;
    if agent_shell_job_to_job_info(info, Some(&client_id), Some(project), None).is_none() {
        return Err("agent shell job is not a Codex job for this project".to_string());
    }
    let (info, stdout, stderr, next_stdout_line, _next_stderr_line) = registry
        .job_log(
            job_id,
            since_line.filter(|line| *line > 0),
            None,
            if since_line.filter(|line| *line > 0).is_some() {
                None
            } else {
                Some(tail_lines.clamp(1, 1000))
            },
        )
        .await?;
    if info.client_id != client_id {
        return Err("agent shell job is not a Codex job for this project".to_string());
    }
    let stderr_tail = stderr.unwrap_or_default();
    let total_lines = next_stdout_line.saturating_sub(1);
    Ok((stdout.unwrap_or_default(), stderr_tail, total_lines))
}

async fn stop_agent_job(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    job_id: &str,
) -> Result<JobInfo, String> {
    validate_job_id(job_id)?;
    let (client_id, registry) = resolve_agent_registry(depot, proj).await?;
    let info = registry.get_job(job_id).await?;
    if agent_shell_job_to_job_info(info, Some(&client_id), Some(project), None).is_none() {
        return Err("agent shell job is not a Codex job for this project".to_string());
    }
    let auth = depot.obtain::<crate::auth::AuthContext>().ok();
    let requested_by = requested_by_from_auth(auth);
    let info = registry.stop_job(job_id, requested_by).await?;
    agent_shell_job_to_job_info(info, Some(&client_id), Some(project), None)
        .ok_or_else(|| "agent shell job is not a Codex job for this project".to_string())
}

async fn find_agent_job_id_by_client_request_id(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    client_request_id: &str,
    goal_id: Option<&str>,
) -> Result<Option<String>, String> {
    let mut jobs = list_agent_jobs(depot, proj, project, 100, None).await?;
    jobs.retain(|job| job.client_request_id.as_deref() == Some(client_request_id));
    if let Some(goal_id) = goal_id {
        jobs.retain(|job| job.goal_id == goal_id);
    }
    jobs.sort_by_key(|j| -j.created_at);
    Ok(jobs.first().map(|job| job.job_id.clone()))
}

async fn recover_agent_job_info(
    depot: &Depot,
    proj: &ProjectConfig,
    project: &str,
    job_id: &str,
) -> Result<JobInfo, String> {
    agent_job_info_basic(depot, proj, project, job_id).await
}

#[handler]
pub async fn codex_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let started_at = chrono::Utc::now().timestamp();
    let audit_db = get_db(depot);
    let explicit_session_id = request_action_session_id(req);
    let pending_job_body: Option<JobOpRequest>;
    macro_rules! render_job {
        (Json($response:expr)) => {{
            let mut response = $response;
            apply_job_workflow_hints(&mut response);
            if let (Some(db), Some(body)) = (audit_db.as_ref(), pending_job_body.as_ref()) {
                record_job_action_event(
                    db,
                    explicit_session_id.clone(),
                    started_at,
                    body,
                    &response,
                    res.status_code.unwrap_or(StatusCode::OK).as_u16() as i64,
                );
            }
            res.render(Json(response));
        }};
    }
    let Some(projects) = get_projects(depot) else {
        let mut response = job_response(
            "unknown",
            false,
            Some("Projects not configured".to_string()),
        );
        apply_job_workflow_hints(&mut response);
        res.render(Json(response));
        return;
    };
    let Some(_db) = get_db(depot) else {
        let mut response = job_response(
            "unknown",
            false,
            Some("Database not configured".to_string()),
        );
        apply_job_workflow_hints(&mut response);
        res.render(Json(response));
        return;
    };
    let body: JobOpRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            let mut response = job_response("unknown", false, Some(format!("Invalid JSON: {}", e)));
            apply_job_workflow_hints(&mut response);
            res.render(Json(response));
            return;
        }
    };
    let op = body.op.clone();
    pending_job_body = Some(body.clone());
    let project = match body.project.clone() {
        Some(p) => p,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            render_job!(Json(job_response(
                &op,
                false,
                Some("project is required".to_string()),
            )));
            return;
        }
    };
    let proj = match projects.get_project(&project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            render_job!(Json(job_response(&op, false, Some(e))));
            return;
        }
    };
    let client_request_id = body.client_request_id.as_deref();
    if let Some(client_request_id) = client_request_id {
        if let Err(e) = validate_client_request_id(client_request_id) {
            res.status_code(StatusCode::BAD_REQUEST);
            render_job!(Json(job_response(&op, false, Some(e))));
            return;
        }
    }
    match op.as_str() {
        "check" => {
            let Some(goal_id) = body.goal_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("goal_id is required".to_string()),
                )));
                return;
            };
            let Some(suite) = body.suite.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("suite is required".to_string()),
                )));
                return;
            };
            if !proj.is_check_allowed(suite) {
                res.status_code(StatusCode::FORBIDDEN);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some(format!(
                        "Check '{}' is not allowed. Allowed: {}",
                        suite,
                        proj.effective_allowed_checks().join(", ")
                    )),
                )));
                return;
            }
            let command = match proj.get_check_command(suite) {
                Ok(c) => c,
                Err(e) => {
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            };
            let max_runtime_secs = match validate_job_runtime(body.max_runtime_secs) {
                Ok(v) => v,
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            };
            if let Some(client_request_id) = client_request_id {
                let mut existing = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                existing.retain(|j| j.goal_id == goal_id);
                filter_jobs_by_client_request_id(&mut existing, Some(client_request_id));
                existing.sort_by_key(|j| -j.created_at);
                if let Some(job) = existing.first().cloned() {
                    render_job!(Json(JobOpResponse {
                        success: true,
                        op,
                        job_id: Some(job.job_id.clone()),
                        job_ids: vec![job.job_id.clone()],
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout_tail: None,
                        stderr_tail: None,
                        summary_markdown: None,
                        error: None,
                        log_total_lines: None,
                        next_cursor: None,
                        metadata_only: None,
                        logs_included: None,
                        warnings: Vec::new(),
                        recommended_next_action: None,
                        action_budget_hint: None,
                    }));
                    return;
                }
            }
            let reason = body
                .reason
                .clone()
                .or_else(|| Some(format!("run check {}", suite)));
            let result = if proj.is_agent() {
                create_agent_job(
                    depot,
                    proj,
                    &project,
                    goal_id,
                    &command,
                    body.client_request_id.clone(),
                    Some("check".to_string()),
                    Some(suite.to_string()),
                    None,
                    reason,
                    max_runtime_secs,
                )
                .await
            } else {
                create_local_job(
                    proj,
                    &project,
                    goal_id,
                    &command,
                    body.client_request_id.clone(),
                    Some("check".to_string()),
                    Some(suite.to_string()),
                    None,
                    reason,
                    max_runtime_secs,
                    None,
                )
            };
            match result {
                Ok(job) => render_job!(Json(JobOpResponse {
                    success: true,
                    op,
                    job_id: Some(job.job_id.clone()),
                    job_ids: vec![job.job_id.clone()],
                    job: Some(job),
                    jobs: Vec::new(),
                    stdout_tail: None,
                    stderr_tail: None,
                    summary_markdown: None,
                    error: None,
                    log_total_lines: None,
                    next_cursor: None,
                    metadata_only: None,
                    logs_included: None,
                    warnings: Vec::new(),
                    recommended_next_action: None,
                    action_budget_hint: None,
                })),
                Err(e) => render_job!(Json(job_response(&op, false, Some(e)))),
            }
        }
        "create" => {
            let Some(goal_id) = body.goal_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("goal_id is required".to_string()),
                )));
                return;
            };
            // Determine the command source: trusted script_text, command, or script_path
            let (command, job_kind, job_script_path, trusted_script_text) = match (
                body.script_text.as_deref(),
                body.trusted.unwrap_or(false),
                body.command.as_deref(),
                body.script_path.as_deref(),
            ) {
                // Trusted script_text mode
                (Some(script_text), true, None, None) => {
                    // Validate the trusted script
                    if let Err(e) = validate_trusted_script(script_text) {
                        res.status_code(StatusCode::BAD_REQUEST);
                        render_job!(Json(job_response(&op, false, Some(e))));
                        return;
                    }
                    // Safety checks
                    if let Some(err) = check_denylist(script_text) {
                        res.status_code(StatusCode::BAD_REQUEST);
                        render_job!(Json(job_response(&op, false, Some(err))));
                        return;
                    }
                    if let Some(err) = check_secret_read(script_text) {
                        res.status_code(StatusCode::BAD_REQUEST);
                        render_job!(Json(job_response(&op, false, Some(err))));
                        return;
                    }
                    if let Some(err) = check_background_escape(script_text) {
                        res.status_code(StatusCode::BAD_REQUEST);
                        render_job!(Json(job_response(&op, false, Some(err))));
                        return;
                    }
                    let command = if proj.is_agent() {
                        format!(
                            "bash -lc {}",
                            shell_escape(&build_trusted_script_content(script_text))
                        )
                    } else {
                        // We need a job_id to build the script path command, but we don't
                        // have it yet. create_local_job writes script.sh before spawning.
                        String::new()
                    };
                    (
                        command,
                        "trusted_script".to_string(),
                        None,
                        Some(script_text.to_string()),
                    )
                }
                // script_text without trusted=true
                (Some(_), false, _, _) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some("script_text requires trusted=true".to_string()),
                    )));
                    return;
                }
                // script_text conflicts with command/script_path
                (Some(_), true, Some(_), _) | (Some(_), true, _, Some(_)) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some(
                            "provide script_text (trusted) or command/script_path, not both"
                                .to_string(),
                        ),
                    )));
                    return;
                }
                // Original command or script_path mode (not trusted)
                (None, _, Some(_), Some(_)) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some("provide either command or script_path, not both".to_string()),
                    )));
                    return;
                }
                (None, _, Some(command), None) => {
                    (command.to_string(), "command".to_string(), None, None)
                }
                (None, _, None, Some(script_path)) => {
                    match build_script_job_command(script_path, &body.script_args) {
                        Ok(command) => (
                            command,
                            "script".to_string(),
                            Some(script_path.to_string()),
                            None,
                        ),
                        Err(e) => {
                            res.status_code(StatusCode::BAD_REQUEST);
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                }
                (None, _, None, None) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some(
                            "command, script_path, or script_text (with trusted=true) is required"
                                .to_string(),
                        ),
                    )));
                    return;
                }
            };
            let max_runtime_secs = match validate_job_runtime(body.max_runtime_secs) {
                Ok(v) => v,
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            };
            if let Some(client_request_id) = client_request_id {
                let mut existing = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                existing.retain(|j| j.goal_id == goal_id);
                filter_jobs_by_client_request_id(&mut existing, Some(client_request_id));
                existing.sort_by_key(|j| -j.created_at);
                if let Some(job) = existing.first().cloned() {
                    render_job!(Json(JobOpResponse {
                        success: true,
                        op,
                        job_id: Some(job.job_id.clone()),
                        job_ids: vec![job.job_id.clone()],
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout_tail: None,
                        stderr_tail: None,
                        summary_markdown: None,
                        error: None,
                        log_total_lines: None,
                        next_cursor: None,
                        metadata_only: None,
                        logs_included: None,
                        warnings: Vec::new(),
                        recommended_next_action: None,
                        action_budget_hint: None,
                    }));
                    return;
                }
            }
            let result = if proj.is_agent() {
                create_agent_job(
                    depot,
                    proj,
                    &project,
                    goal_id,
                    &command,
                    body.client_request_id.clone(),
                    Some(job_kind.clone()),
                    None,
                    job_script_path.clone(),
                    body.reason.clone(),
                    max_runtime_secs,
                )
                .await
            } else {
                create_local_job(
                    proj,
                    &project,
                    goal_id,
                    &command,
                    body.client_request_id.clone(),
                    Some(job_kind.clone()),
                    None,
                    job_script_path.clone(),
                    body.reason.clone(),
                    max_runtime_secs,
                    trusted_script_text.as_deref(),
                )
            };
            let job = match result {
                Ok(job) => job,
                Err(e) => {
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            };
            render_job!(Json(JobOpResponse {
                success: true,
                op,
                job_id: Some(job.job_id.clone()),
                job_ids: vec![job.job_id.clone()],
                job: Some(job),
                jobs: Vec::new(),
                stdout_tail: None,
                stderr_tail: None,
                summary_markdown: None,
                error: None,
                log_total_lines: None,
                next_cursor: None,
                metadata_only: None,
                logs_included: None,
                warnings: Vec::new(),
                recommended_next_action: None,
                action_budget_hint: None,
            }));
        }
        "create_batch" => {
            let Some(goal_id) = body.goal_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("goal_id is required".to_string()),
                )));
                return;
            };
            if body.commands.is_empty() || body.commands.len() > 20 {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("commands must contain 1..20 items".to_string()),
                )));
                return;
            }
            let max_runtime_secs = match validate_job_runtime(body.max_runtime_secs) {
                Ok(v) => v,
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            };
            for command in &body.commands {
                if let Err(e) = validate_job_command(command) {
                    res.status_code(StatusCode::BAD_REQUEST);
                    render_job!(Json(job_response(&op, false, Some(e))));
                    return;
                }
            }
            if let Some(client_request_id) = client_request_id {
                let mut existing = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                existing.retain(|j| j.goal_id == goal_id);
                existing.retain(|j| {
                    j.client_request_id
                        .as_deref()
                        .map(|id| {
                            id == client_request_id
                                || id.starts_with(&format!("{}.", client_request_id))
                        })
                        .unwrap_or(false)
                });
                existing.sort_by_key(|j| -j.created_at);
                if !existing.is_empty() {
                    let job_ids = existing
                        .iter()
                        .map(|j| j.job_id.clone())
                        .collect::<Vec<_>>();
                    render_job!(Json(JobOpResponse {
                        success: true,
                        op,
                        job_id: job_ids.first().cloned(),
                        job_ids,
                        job: existing.first().cloned(),
                        jobs: existing,
                        stdout_tail: None,
                        stderr_tail: None,
                        summary_markdown: None,
                        error: None,
                        log_total_lines: None,
                        next_cursor: None,
                        metadata_only: None,
                        logs_included: None,
                        warnings: Vec::new(),
                        recommended_next_action: None,
                        action_budget_hint: None,
                    }));
                    return;
                }
            }
            let mut jobs = Vec::new();
            for (idx, command) in body.commands.iter().enumerate() {
                let batch_client_request_id = body
                    .client_request_id
                    .as_ref()
                    .map(|id| format!("{}.{}", id, idx + 1));
                let result = if proj.is_agent() {
                    create_agent_job(
                        depot,
                        proj,
                        &project,
                        goal_id,
                        command,
                        batch_client_request_id,
                        Some("command".to_string()),
                        None,
                        None,
                        body.reason.clone(),
                        max_runtime_secs,
                    )
                    .await
                } else {
                    create_local_job(
                        proj,
                        &project,
                        goal_id,
                        command,
                        batch_client_request_id,
                        Some("command".to_string()),
                        None,
                        None,
                        body.reason.clone(),
                        max_runtime_secs,
                        None,
                    )
                };
                match result {
                    Ok(job) => jobs.push(job),
                    Err(e) => {
                        render_job!(Json(job_response(&op, false, Some(e))));
                        return;
                    }
                }
            }
            let job_ids = jobs.iter().map(|j| j.job_id.clone()).collect::<Vec<_>>();
            render_job!(Json(JobOpResponse {
                success: true,
                op,
                job_id: job_ids.first().cloned(),
                job_ids,
                job: jobs.first().cloned(),
                jobs,
                stdout_tail: None,
                stderr_tail: None,
                summary_markdown: None,
                error: None,
                log_total_lines: None,
                next_cursor: None,
                metadata_only: None,
                logs_included: None,
                warnings: Vec::new(),
                recommended_next_action: None,
                action_budget_hint: None,
            }));
        }
        "list" => {
            let limit = body.limit.clamp(1, 100);
            let status_filter = body.status.as_deref();
            let mut jobs = if proj.is_agent() {
                match list_agent_jobs(depot, proj, &project, limit, status_filter).await {
                    Ok(jobs) => jobs,
                    Err(e) => {
                        render_job!(Json(job_response(&op, false, Some(e))));
                        return;
                    }
                }
            } else {
                list_local_jobs(&proj.root(), limit, status_filter)
            };
            if let Some(goal_id) = body.goal_id.as_deref() {
                jobs.retain(|j| j.goal_id == goal_id);
            }
            if let Some(client_request_id) = client_request_id {
                jobs.retain(|j| {
                    j.client_request_id
                        .as_deref()
                        .map(|id| {
                            id == client_request_id
                                || id.starts_with(&format!("{}.", client_request_id))
                        })
                        .unwrap_or(false)
                });
            }
            let job_ids = jobs.iter().map(|j| j.job_id.clone()).collect::<Vec<_>>();
            render_job!(Json(JobOpResponse {
                success: true,
                op,
                job_id: job_ids.first().cloned(),
                job_ids,
                job: jobs.first().cloned(),
                jobs,
                stdout_tail: None,
                stderr_tail: None,
                summary_markdown: None,
                error: None,
                log_total_lines: None,
                next_cursor: None,
                metadata_only: None,
                logs_included: None,
                warnings: Vec::new(),
                recommended_next_action: None,
                action_budget_hint: None,
            }));
        }
        "status" => {
            let job_id_owned;
            let job_id = if let Some(job_id) = body.job_id.as_deref() {
                job_id
            } else if let Some(client_request_id) = client_request_id {
                let mut jobs = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                if let Some(goal_id) = body.goal_id.as_deref() {
                    jobs.retain(|j| j.goal_id == goal_id);
                }
                jobs.retain(|j| j.client_request_id.as_deref() == Some(client_request_id));
                jobs.sort_by_key(|j| -j.created_at);
                let Some(job) = jobs.first() else {
                    res.status_code(StatusCode::NOT_FOUND);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some("job not found for client_request_id".to_string()),
                    )));
                    return;
                };
                job_id_owned = job.job_id.clone();
                &job_id_owned
            } else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("job_id or client_request_id is required".to_string()),
                )));
                return;
            };

            // Determine detail level:
            // - Default (None) → "basic" (lightweight, no OOM, no logs)
            // - Explicit "basic" → same as default
            // - Explicit "logs" → basic + include log tails in response
            // tail_lines does NOT implicitly trigger logs; only explicit detail=logs reads logs.
            let effective_detail = effective_status_detail(body.detail.as_deref());

            let job_result = if proj.is_agent() {
                agent_job_info_basic(depot, proj, &project, job_id).await
            } else if effective_detail == "basic" {
                local_job_info_basic(&proj.root(), job_id)
            } else {
                // detail=logs: use basic status + read log tails
                local_job_info_basic(&proj.root(), job_id)
            };

            match job_result {
                Ok(job) => {
                    if effective_detail == "logs" {
                        // Read log tails
                        let tail_lines = body.tail_lines.clamp(1, 1000);
                        let (stdout_tail, stderr_tail, log_total_lines) = if proj.is_agent() {
                            match agent_job_log_with_count(
                                depot,
                                proj,
                                &project,
                                job_id,
                                tail_lines,
                                body.since_line,
                            )
                            .await
                            {
                                Ok((out, err, total)) => (Some(out), Some(err), Some(total)),
                                Err(_) => (None, None, None),
                            }
                        } else {
                            match local_job_log(&proj.root(), job_id, tail_lines, body.since_line) {
                                Ok((out, err, total)) => (Some(out), Some(err), Some(total)),
                                Err(_) => (None, None, None),
                            }
                        };
                        let next_cursor =
                            log_total_lines.and_then(|t| if t > 0 { Some(t + 1) } else { None });
                        render_job!(Json(JobOpResponse {
                            success: true,
                            op,
                            job_id: Some(job.job_id.clone()),
                            job_ids: vec![job.job_id.clone()],
                            job: Some(job),
                            jobs: Vec::new(),
                            stdout_tail,
                            stderr_tail,
                            summary_markdown: None,
                            error: None,
                            log_total_lines,
                            next_cursor,
                            metadata_only: Some(false),
                            logs_included: Some(true),
                            warnings: Vec::new(),
                            recommended_next_action: None,
                            action_budget_hint: None,
                        }))
                    } else {
                        // basic: no logs
                        render_job!(Json(JobOpResponse {
                            success: true,
                            op,
                            job_id: Some(job.job_id.clone()),
                            job_ids: vec![job.job_id.clone()],
                            job: Some(job),
                            jobs: Vec::new(),
                            stdout_tail: None,
                            stderr_tail: None,
                            summary_markdown: None,
                            error: None,
                            log_total_lines: None,
                            next_cursor: None,
                            metadata_only: Some(false),
                            logs_included: Some(false),
                            warnings: Vec::new(),
                            recommended_next_action: None,
                            action_budget_hint: None,
                        }))
                    }
                }
                Err(e) => render_job!(Json(job_response(&op, false, Some(e)))),
            }
        }
        "log" => {
            let job_id_owned;
            let job_id = if let Some(job_id) = body.job_id.as_deref() {
                job_id
            } else if let Some(client_request_id) = client_request_id {
                let mut jobs = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                if let Some(goal_id) = body.goal_id.as_deref() {
                    jobs.retain(|j| j.goal_id == goal_id);
                }
                jobs.retain(|j| j.client_request_id.as_deref() == Some(client_request_id));
                jobs.sort_by_key(|j| -j.created_at);
                let Some(job) = jobs.first() else {
                    res.status_code(StatusCode::NOT_FOUND);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some("job not found for client_request_id".to_string()),
                    )));
                    return;
                };
                job_id_owned = job.job_id.clone();
                &job_id_owned
            } else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("job_id or client_request_id is required".to_string()),
                )));
                return;
            };
            let tail_lines = body.tail_lines.clamp(1, 1000);
            let since_line = body.since_line;
            let result: Result<(String, String, usize), String> = if proj.is_agent() {
                agent_job_log_with_count(depot, proj, &project, job_id, tail_lines, since_line)
                    .await
            } else {
                local_job_log(&proj.root(), job_id, tail_lines, since_line)
            };
            match result {
                Ok((stdout_tail, stderr_tail, total_lines)) => {
                    // next_cursor points to the line after the last returned line
                    let next_cursor = if total_lines > 0 {
                        Some(total_lines + 1)
                    } else {
                        None
                    };
                    render_job!(Json(JobOpResponse {
                        success: true,
                        op,
                        job_id: Some(job_id.to_string()),
                        job_ids: vec![job_id.to_string()],
                        job: None,
                        jobs: Vec::new(),
                        stdout_tail: Some(stdout_tail),
                        stderr_tail: Some(stderr_tail),
                        summary_markdown: None,
                        error: None,
                        log_total_lines: Some(total_lines),
                        next_cursor,
                        metadata_only: None,
                        logs_included: None,
                        warnings: Vec::new(),
                        recommended_next_action: None,
                        action_budget_hint: None,
                    }))
                }
                Err(e) => render_job!(Json(job_response(&op, false, Some(e)))),
            }
        }
        "stop" => {
            let job_id_owned;
            let job_id = if let Some(job_id) = body.job_id.as_deref() {
                job_id
            } else if let Some(client_request_id) = client_request_id {
                let mut jobs = if proj.is_agent() {
                    match list_agent_jobs(depot, proj, &project, 100, None).await {
                        Ok(jobs) => jobs,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    list_local_jobs(&proj.root(), 100, None)
                };
                if let Some(goal_id) = body.goal_id.as_deref() {
                    jobs.retain(|j| j.goal_id == goal_id);
                }
                jobs.retain(|j| j.client_request_id.as_deref() == Some(client_request_id));
                jobs.sort_by_key(|j| -j.created_at);
                let Some(job) = jobs.first() else {
                    res.status_code(StatusCode::NOT_FOUND);
                    render_job!(Json(job_response(
                        &op,
                        false,
                        Some("job not found for client_request_id".to_string()),
                    )));
                    return;
                };
                job_id_owned = job.job_id.clone();
                &job_id_owned
            } else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(job_response(
                    &op,
                    false,
                    Some("job_id or client_request_id is required".to_string()),
                )));
                return;
            };
            let result = if proj.is_agent() {
                stop_agent_job(depot, proj, &project, job_id).await
            } else {
                stop_local_job(&proj.root(), job_id)
            };
            match result {
                Ok(job) => render_job!(Json(JobOpResponse {
                    success: true,
                    op,
                    job_id: Some(job.job_id.clone()),
                    job_ids: vec![job.job_id.clone()],
                    job: Some(job),
                    jobs: Vec::new(),
                    stdout_tail: None,
                    stderr_tail: None,
                    summary_markdown: None,
                    error: None,
                    log_total_lines: None,
                    next_cursor: None,
                    metadata_only: None,
                    logs_included: None,
                    warnings: Vec::new(),
                    recommended_next_action: None,
                    action_budget_hint: None,
                })),
                Err(e) => render_job!(Json(job_response(&op, false, Some(e)))),
            }
        }
        "summarize" => {
            let limit = body.limit.clamp(1, 100);
            let mut jobs = if proj.is_agent() {
                match list_agent_jobs(depot, proj, &project, limit, body.status.as_deref()).await {
                    Ok(jobs) => jobs,
                    Err(e) => {
                        render_job!(Json(job_response(&op, false, Some(e))));
                        return;
                    }
                }
            } else {
                list_local_jobs(&proj.root(), limit, body.status.as_deref())
            };
            if let Some(goal_id) = body.goal_id.as_deref() {
                jobs.retain(|j| j.goal_id == goal_id);
            }
            if let Some(client_request_id) = client_request_id {
                jobs.retain(|j| {
                    j.client_request_id
                        .as_deref()
                        .map(|id| {
                            id == client_request_id
                                || id.starts_with(&format!("{}.", client_request_id))
                        })
                        .unwrap_or(false)
                });
            }
            let mut tails = Vec::new();
            for job in &jobs {
                let pair: (String, String) = if proj.is_agent() {
                    agent_job_log_with_count(
                        depot,
                        proj,
                        &project,
                        &job.job_id,
                        body.tail_lines.clamp(1, 1000),
                        None,
                    )
                    .await
                    .map(|(out, err, _)| (out, err))
                    .unwrap_or_default()
                } else {
                    local_job_log(
                        &proj.root(),
                        &job.job_id,
                        body.tail_lines.clamp(1, 1000),
                        None,
                    )
                    .map(|(out, err, _)| (out, err))
                    .unwrap_or_default()
                };
                tails.push(pair);
            }
            let summary = summarize_jobs_markdown(&jobs, &tails);
            let job_ids = jobs.iter().map(|j| j.job_id.clone()).collect::<Vec<_>>();
            render_job!(Json(JobOpResponse {
                success: true,
                op,
                job_id: job_ids.first().cloned(),
                job_ids,
                job: jobs.first().cloned(),
                jobs,
                stdout_tail: None,
                stderr_tail: None,
                summary_markdown: Some(summary),
                error: None,
                log_total_lines: None,
                next_cursor: None,
                metadata_only: None,
                logs_included: None,
                warnings: Vec::new(),
                recommended_next_action: None,
                action_budget_hint: None,
            }));
        }
        "recover" => {
            // Metadata-only recovery: no log reading, no process checks, no OOM detection.
            // Priority: job_id takes precedence over client_request_id when both provided.
            let job_id_owned;
            let resolved_job_id = if let Some(jid) = body.job_id.as_deref() {
                jid
            } else if let Some(crid) = client_request_id {
                let found = if proj.is_agent() {
                    match find_agent_job_id_by_client_request_id(
                        depot,
                        proj,
                        &project,
                        crid,
                        body.goal_id.as_deref(),
                    )
                    .await
                    {
                        Ok(found) => found,
                        Err(e) => {
                            render_job!(Json(job_response(&op, false, Some(e))));
                            return;
                        }
                    }
                } else {
                    find_local_job_id_by_client_request_id(
                        &proj.root(),
                        crid,
                        body.goal_id.as_deref(),
                    )
                };
                match found {
                    Some(id) => {
                        job_id_owned = id;
                        &job_id_owned
                    }
                    None => {
                        res.status_code(StatusCode::NOT_FOUND);
                        render_job!(Json(JobOpResponse {
                            success: false,
                            op: op.clone(),
                            job_id: None,
                            job_ids: Vec::new(),
                            job: None,
                            jobs: Vec::new(),
                            stdout_tail: None,
                            stderr_tail: None,
                            summary_markdown: None,
                            error: Some("job not found for client_request_id".to_string()),
                            log_total_lines: None,
                            next_cursor: None,
                            metadata_only: Some(true),
                            logs_included: None,
                            warnings: Vec::new(),
                            recommended_next_action: None,
                            action_budget_hint: None,
                        }));
                        return;
                    }
                }
            } else {
                res.status_code(StatusCode::BAD_REQUEST);
                render_job!(Json(JobOpResponse {
                    success: false,
                    op: op.clone(),
                    job_id: None,
                    job_ids: Vec::new(),
                    job: None,
                    jobs: Vec::new(),
                    stdout_tail: None,
                    stderr_tail: None,
                    summary_markdown: None,
                    error: Some("job_id or client_request_id is required for recover".to_string()),
                    log_total_lines: None,
                    next_cursor: None,
                    metadata_only: None,
                    logs_included: None,
                    warnings: Vec::new(),
                    recommended_next_action: None,
                    action_budget_hint: None,
                }));
                return;
            };
            let result = if proj.is_agent() {
                recover_agent_job_info(depot, proj, &project, resolved_job_id).await
            } else {
                recover_local_job_info(&proj.root(), resolved_job_id)
            };
            match result {
                Ok(job) => render_job!(Json(JobOpResponse {
                    success: true,
                    op,
                    job_id: Some(job.job_id.clone()),
                    job_ids: vec![job.job_id.clone()],
                    job: Some(job),
                    jobs: Vec::new(),
                    stdout_tail: None,
                    stderr_tail: None,
                    summary_markdown: None,
                    error: None,
                    log_total_lines: None,
                    next_cursor: None,
                    metadata_only: Some(true),
                    logs_included: Some(false),
                    warnings: Vec::new(),
                    recommended_next_action: None,
                    action_budget_hint: None,
                })),
                Err(e) => render_job!(Json(JobOpResponse {
                    success: false,
                    op,
                    job_id: None,
                    job_ids: Vec::new(),
                    job: None,
                    jobs: Vec::new(),
                    stdout_tail: None,
                    stderr_tail: None,
                    summary_markdown: None,
                    error: Some(e),
                    log_total_lines: None,
                    next_cursor: None,
                    metadata_only: Some(true),
                    logs_included: None,
                    warnings: Vec::new(),
                    recommended_next_action: None,
                    action_budget_hint: None,
                })),
            }
        }
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            render_job!(Json(job_response(
                &op,
                false,
                Some("unsupported job op".to_string()),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_sessions::{compute_stats, decode_event};
    use crate::projects::ProjectConfig;
    use crate::Database;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;

    /// Create a minimal local job directory for testing.
    fn create_test_job_dir(root: &Path, job_id: &str, client_request_id: Option<&str>) -> PathBuf {
        let dir = local_job_dir(root, job_id);
        fs::create_dir_all(&dir).unwrap();
        let now = chrono::Utc::now().timestamp();
        let meta = JobMetadata {
            job_id: job_id.to_string(),
            client_request_id: client_request_id.map(|s| s.to_string()),
            project: "testproj".to_string(),
            goal_id: "goal-1".to_string(),
            command: "echo hello".to_string(),
            kind: Some("command".to_string()),
            suite: None,
            script_path: None,
            reason: None,
            status: "completed".to_string(),
            created_at: now - 100,
            started_at: Some(now - 100),
            finished_at: Some(now - 50),
            max_runtime_secs: 3600,
            executor: "local".to_string(),
            host: None,
            path: root.to_string_lossy().to_string(),
        };
        fs::write(
            dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("status"), "completed").unwrap();
        fs::write(dir.join("exit_code"), "0").unwrap();
        fs::write(dir.join("pid"), "12345").unwrap();
        fs::write(dir.join("finished_at"), (now - 50).to_string()).unwrap();
        fs::write(dir.join("stdout.log"), "line1\nline2\nline3\n").unwrap();
        fs::write(dir.join("stderr.log"), "some error output\n").unwrap();
        dir
    }

    fn test_db() -> (tempfile::TempDir, Arc<Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&tmp.path().join("webcodex.db")).unwrap());
        (tmp, db)
    }

    fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
        let (role, scopes) = if is_bootstrap {
            ("admin".to_string(), vec!["admin".to_string()])
        } else {
            ("user".to_string(), Vec::new())
        };
        crate::auth::AuthContext {
            kind: if is_bootstrap {
                crate::auth::AuthKind::Bootstrap
            } else {
                crate::auth::AuthKind::ApiToken
            },
            user_id: username.map(|username| format!("user-{}", username)),
            username: username.map(str::to_string),
            api_key_id: username.map(|username| format!("key-{}", username)),
            api_key_name: username.map(|username| format!("{} key", username)),
            role: Some(role),
            scopes,
            is_bootstrap,
            token_kind: if is_bootstrap {
                None
            } else {
                Some("user".to_string())
            },
            allowed_client_id: None,
        }
    }

    fn test_agent_project(root: &str) -> ProjectConfig {
        ProjectConfig {
            path: root.to_string(),
            executor: crate::projects::Executor::Agent,
            client_id: Some("oe".to_string()),
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn async_shell_job_capabilities() -> crate::shell_protocol::ShellClientCapabilities {
        let mut capabilities = crate::shell_protocol::ShellClientCapabilities::default();
        capabilities.jobs = true;
        capabilities.async_jobs = true;
        capabilities.async_shell_jobs = true;
        capabilities
    }

    #[tokio::test]
    async fn agent_job_helpers_create_complete_log_and_recover() {
        let registry = Arc::new(crate::ShellClientRegistry::default());
        registry
            .register(crate::shell_protocol::ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_shell_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let mut depot = Depot::new();
        depot.inject(registry.clone());
        depot.inject(auth_context(None, true));
        let proj = test_agent_project("/tmp/webcodex-agent-job");

        let job = create_agent_job(
            &depot,
            &proj,
            "agent-project",
            "goal-agent",
            "printf hello",
            Some("crid-agent".to_string()),
            Some("command".to_string()),
            None,
            None,
            Some("run agent job".to_string()),
            60,
        )
        .await
        .unwrap();
        assert_eq!(job.executor, "agent");
        assert_eq!(job.status, "queued");
        assert_eq!(job.project, "agent-project");
        assert_eq!(job.goal_id, "goal-agent");
        assert_eq!(job.client_request_id.as_deref(), Some("crid-agent"));

        let request = registry
            .poll(crate::shell_protocol::ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.kind, "start_job");
        assert_eq!(request.cwd.as_deref(), Some("/tmp/webcodex-agent-job"));
        assert_eq!(request.command, "printf hello");

        registry
            .complete(crate::shell_protocol::ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some("warn\n".to_string()),
                duration_ms: Some(1500),
                error: None,
            })
            .await
            .unwrap();

        let done = agent_job_info_basic(&depot, &proj, "agent-project", &job.job_id)
            .await
            .unwrap();
        assert_eq!(done.status, "completed");
        assert_eq!(done.exit_code, Some(0));
        assert_eq!(done.elapsed_secs, Some(1));

        let listed = list_agent_jobs(&depot, &proj, "agent-project", 10, Some("completed"))
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].job_id, job.job_id);

        let (stdout, stderr, total_lines) =
            agent_job_log_with_count(&depot, &proj, "agent-project", &job.job_id, 10, Some(1))
                .await
                .unwrap();
        assert_eq!(stdout, "hello\n");
        assert_eq!(stderr, "warn\n");
        assert_eq!(total_lines, 1);

        let found = find_agent_job_id_by_client_request_id(
            &depot,
            &proj,
            "agent-project",
            "crid-agent",
            Some("goal-agent"),
        )
        .await
        .unwrap();
        assert_eq!(found.as_deref(), Some(job.job_id.as_str()));

        let recovered = recover_agent_job_info(&depot, &proj, "agent-project", &job.job_id)
            .await
            .unwrap();
        assert_eq!(recovered.status, "completed");
    }

    #[tokio::test]
    async fn agent_job_helpers_reject_job_id_from_other_client() {
        let registry = Arc::new(crate::ShellClientRegistry::default());
        registry
            .register(crate::shell_protocol::ShellClientRegisterRequest {
                client_id: "alice-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_shell_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        registry
            .register(crate::shell_protocol::ShellClientRegisterRequest {
                client_id: "bob-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("bob".to_string()),
                hostname: None,
                capabilities: Some(async_shell_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        let bob_job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("bob-client".to_string()),
                    cwd: Some("/tmp/webcodex-bob".to_string()),
                    command: Some("printf secret".to_string()),
                    timeout_secs: Some(60),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: Some(ShellJobCodexMetadata {
                        project: Some("agent-project".to_string()),
                        goal_id: Some("goal-bob".to_string()),
                        client_request_id: Some("crid-bob".to_string()),
                        command: Some("printf secret".to_string()),
                        kind: Some("command".to_string()),
                        suite: None,
                        script_path: None,
                        reason: Some("bob job".to_string()),
                        max_runtime_secs: Some(60),
                    }),
                },
                "bob".to_string(),
            )
            .await
            .unwrap();

        let mut depot = Depot::new();
        depot.inject(registry.clone());
        depot.inject(auth_context(Some("alice"), false));
        let mut proj = test_agent_project("/tmp/webcodex-alice");
        proj.client_id = Some("alice-client".to_string());

        let err = agent_job_info_basic(&depot, &proj, "agent-project", &bob_job.job_id)
            .await
            .unwrap_err();
        assert_eq!(err, "agent shell job is not a Codex job for this project");

        let err =
            agent_job_log_with_count(&depot, &proj, "agent-project", &bob_job.job_id, 10, None)
                .await
                .unwrap_err();
        assert_eq!(err, "agent shell job is not a Codex job for this project");

        let err = stop_agent_job(&depot, &proj, "agent-project", &bob_job.job_id)
            .await
            .unwrap_err();
        assert_eq!(err, "agent shell job is not a Codex job for this project");

        let listed = list_agent_jobs(&depot, &proj, "agent-project", 10, None)
            .await
            .unwrap();
        assert!(listed.is_empty());

        let still_queued = registry.get_job(&bob_job.job_id).await.unwrap();
        assert_eq!(still_queued.status, "queued");
    }

    #[test]
    fn recover_local_job_reads_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job_id = "test-recover-job-1";
        create_test_job_dir(root, job_id, Some("crid-001"));

        let info = recover_local_job_info(root, job_id).unwrap();
        assert_eq!(info.job_id, job_id);
        assert_eq!(info.client_request_id, Some("crid-001".to_string()));
        assert_eq!(info.status, "completed");
        assert_eq!(info.exit_code, Some(0));
        // recover does NOT read logs, so oom_hint is None
        assert!(info.oom_hint.is_none());
    }

    #[test]
    fn recover_local_job_does_not_read_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job_id = "test-recover-no-logs";
        let dir = create_test_job_dir(root, job_id, None);

        // Write OOM-like content to stderr - recover should NOT detect it
        fs::write(dir.join("stderr.log"), "Out of memory\nKilled\n").unwrap();

        let info = recover_local_job_info(root, job_id).unwrap();
        // recover returns None for oom_hint (no OOM detection)
        assert!(info.oom_hint.is_none());
    }

    #[test]
    fn recover_missing_job_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let result = recover_local_job_info(root, "nonexistent-job");
        assert!(result.is_err());
    }

    #[test]
    fn find_local_job_by_client_request_id() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".codex/jobs")).unwrap();

        create_test_job_dir(root, "job-a", Some("crid-match"));
        create_test_job_dir(root, "job-b", Some("crid-other"));
        create_test_job_dir(root, "job-c", None);

        let found = find_local_job_id_by_client_request_id(root, "crid-match", None);
        assert_eq!(found, Some("job-a".to_string()));

        let found = find_local_job_id_by_client_request_id(root, "crid-other", None);
        assert_eq!(found, Some("job-b".to_string()));

        let not_found = find_local_job_id_by_client_request_id(root, "crid-missing", None);
        assert!(not_found.is_none());
    }

    #[test]
    fn find_local_job_by_client_request_id_with_goal_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".codex/jobs")).unwrap();

        create_test_job_dir(root, "job-x", Some("crid-goal"));

        let found = find_local_job_id_by_client_request_id(root, "crid-goal", Some("goal-1"));
        assert_eq!(found, Some("job-x".to_string()));

        let not_found =
            find_local_job_id_by_client_request_id(root, "crid-goal", Some("wrong-goal"));
        assert!(not_found.is_none());
    }

    #[test]
    fn local_job_info_basic_skips_oom_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job_id = "test-basic-no-oom";
        let dir = create_test_job_dir(root, job_id, None);

        // Write OOM-like content to stderr - basic should NOT detect it
        fs::write(dir.join("stderr.log"), "CUDA out of memory\n").unwrap();
        // Make sure status shows the job is no longer running
        fs::write(dir.join("status"), "completed").unwrap();
        fs::write(dir.join("exit_code"), "1").unwrap();

        let info = local_job_info_basic(root, job_id).unwrap();
        // basic detail returns None for oom_hint (no OOM detection)
        assert!(info.oom_hint.is_none());
    }

    #[test]
    fn local_job_info_basic_returns_status_and_exit_code() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job_id = "test-basic-status";
        create_test_job_dir(root, job_id, None);

        let info = local_job_info_basic(root, job_id).unwrap();
        assert_eq!(info.status, "completed");
        assert_eq!(info.exit_code, Some(0));
    }

    #[test]
    fn local_job_log_reads_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let job_id = "test-log-read";
        let dir = create_test_job_dir(root, job_id, None);

        fs::write(dir.join("stdout.log"), "line1\nline2\nline3\n").unwrap();
        fs::write(dir.join("stderr.log"), "err1\nerr2\n").unwrap();

        let (stdout, stderr, total) = local_job_log(root, job_id, 10, None).unwrap();
        assert!(stdout.contains("line1"));
        assert!(stderr.contains("err1"));
        assert_eq!(total, 3);
    }

    #[test]
    fn generated_openapi_schema_excludes_legacy_codex_job_surface() {
        let spec = crate::openapi::build_openapi_spec();

        assert!(
            spec["paths"]["/api/codex/job"].is_null(),
            "legacy /api/codex/job must not appear in generated GPT Actions schema"
        );
        assert!(
            spec["components"]["schemas"]["JobOpRequest"].is_null(),
            "legacy JobOpRequest must not appear in generated GPT Actions schema"
        );
        assert!(
            spec["components"]["schemas"]["JobOpResponse"].is_null(),
            "legacy JobOpResponse must not appear in generated GPT Actions schema"
        );

        let run_codex_desc = spec["paths"]["/api/codex/run"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            run_codex_desc.contains("job_id"),
            "current runCodexTask description should mention job_id, got: {}",
            run_codex_desc
        );
    }

    #[test]
    fn generated_openapi_schema_has_no_auto_upgrade_text() {
        let spec = crate::openapi::build_openapi_spec();
        let schema_text = serde_json::to_string(&spec).unwrap();

        assert!(
            !schema_text.contains("auto-upgrades"),
            "generated GPT Actions schema should not contain stale auto-upgrade wording"
        );
        assert!(
            schema_text.contains("getRuntimeJobStatus"),
            "generated schema should include the current job status action"
        );
        assert!(
            schema_text.contains("getRuntimeJobLog"),
            "generated schema should include the current job log action"
        );
    }

    #[test]
    fn old_status_request_without_detail_deserializes() {
        // Simulate an old client sending a request without the detail field
        let request: JobOpRequest = serde_json::from_str(
            r#"{
                "op": "status",
                "project": "myproj",
                "job_id": "abc-123",
                "tail_lines": 80
            }"#,
        )
        .unwrap();
        assert_eq!(request.op, "status");
        assert!(request.detail.is_none());
        assert_eq!(request.tail_lines, 80);
    }

    #[test]
    fn effective_status_detail_default_is_basic() {
        assert_eq!(effective_status_detail(None), "basic");
    }

    #[test]
    fn effective_status_detail_basic_is_basic() {
        assert_eq!(effective_status_detail(Some("basic")), "basic");
    }

    #[test]
    fn effective_status_detail_logs_is_logs() {
        assert_eq!(effective_status_detail(Some("logs")), "logs");
    }

    #[test]
    fn effective_status_detail_unknown_is_basic() {
        assert_eq!(effective_status_detail(Some("anything_else")), "basic");
    }

    #[test]
    fn tail_lines_does_not_trigger_logs() {
        // Even with tail_lines > 0, detail=None must resolve to "basic"
        let request: JobOpRequest = serde_json::from_str(
            r#"{
                "op": "status",
                "project": "myproj",
                "job_id": "abc-123",
                "tail_lines": 80
            }"#,
        )
        .unwrap();
        assert_eq!(request.tail_lines, 80);
        assert!(request.detail.is_none());
        // effective_status_detail must return "basic" even though tail_lines > 0
        assert_eq!(effective_status_detail(request.detail.as_deref()), "basic");
    }

    #[test]
    fn build_trusted_script_job_command_points_to_script() {
        let cmd = build_trusted_script_job_command("abc-123");
        assert!(
            cmd.contains(".codex/jobs/abc-123/script.sh"),
            "command should point to script.sh in job dir, got: {}",
            cmd
        );
        assert!(
            cmd.contains("bash"),
            "command should use bash, got: {}",
            cmd
        );
        // Must NOT contain the old broken pattern
        assert!(
            !cmd.contains("set -euo pipefail; '"),
            "command should NOT use the old single-quote-wrapped pattern, got: {}",
            cmd
        );
    }

    #[test]
    fn build_trusted_script_content_includes_all_parts() {
        let content = build_trusted_script_content("echo hello\necho world");
        assert!(content.starts_with("#!/usr/bin/env bash\n"));
        assert!(content.contains("set -euo pipefail"));
        assert!(content.contains("echo hello"));
        assert!(content.contains("echo world"));
    }

    #[test]
    fn create_local_job_with_trusted_script_text_works() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = ProjectConfig {
            path: tmp.path().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        std::fs::create_dir_all(tmp.path().join(".codex/jobs")).unwrap();

        let script_text = "echo trusted_job_output";
        let result = create_local_job(
            &proj,
            "test-project",
            "goal-test",
            "", // placeholder for trusted_script_text mode
            None,
            Some("trusted_script".to_string()),
            None,
            None,
            Some("test reason".to_string()),
            60,
            Some(script_text),
        );
        assert!(
            result.is_ok(),
            "trusted script job creation should succeed: {:?}",
            result
        );
        let job = result.unwrap();

        // Verify script.sh was written before the job spawned
        let dir = tmp.path().join(".codex/jobs").join(&job.job_id);
        let script_content = std::fs::read_to_string(dir.join("script.sh")).unwrap();
        assert!(script_content.contains("#!/usr/bin/env bash"));
        assert!(script_content.contains("set -euo pipefail"));
        assert!(script_content.contains("echo trusted_job_output"));

        // Verify command points to script.sh
        assert!(job.command.contains("script.sh"));

        // Wait for job to complete
        let mut attempts = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let status =
                std::fs::read_to_string(dir.join("status")).unwrap_or_else(|_| "running".into());
            if status != "running" || attempts > 50 {
                break;
            }
            attempts += 1;
        }

        // Verify output
        let stdout = std::fs::read_to_string(dir.join("stdout.log")).unwrap_or_default();
        assert!(
            stdout.contains("trusted_job_output"),
            "stdout should contain script output, got: {}",
            stdout
        );
    }

    #[test]
    fn create_local_job_with_empty_trusted_script_text_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = ProjectConfig {
            path: tmp.path().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        std::fs::create_dir_all(tmp.path().join(".codex/jobs")).unwrap();

        let result = create_local_job(
            &proj,
            "test-project",
            "goal-test",
            "",
            None,
            Some("trusted_script".to_string()),
            None,
            None,
            Some("test reason".to_string()),
            60,
            Some(""),
        );
        assert!(result.is_err(), "empty trusted script should fail");
    }

    #[test]
    fn create_local_job_with_nul_trusted_script_text_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = ProjectConfig {
            path: tmp.path().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        std::fs::create_dir_all(tmp.path().join(".codex/jobs")).unwrap();

        let result = create_local_job(
            &proj,
            "test-project",
            "goal-test",
            "",
            None,
            Some("trusted_script".to_string()),
            None,
            None,
            Some("test reason".to_string()),
            60,
            Some("echo\0bad"),
        );
        assert!(result.is_err(), "NUL trusted script should fail");
    }

    #[test]
    fn create_local_job_without_trusted_script_works_normally() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = ProjectConfig {
            path: tmp.path().to_string_lossy().to_string(),
            executor: crate::projects::Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: true,
            allow_raw_command_requests: true,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        };
        std::fs::create_dir_all(tmp.path().join(".codex/jobs")).unwrap();

        let result = create_local_job(
            &proj,
            "test-project",
            "goal-test",
            "echo normal_job_output",
            None,
            Some("command".to_string()),
            None,
            None,
            Some("test reason".to_string()),
            60,
            None,
        );
        assert!(
            result.is_ok(),
            "normal job creation should succeed: {:?}",
            result
        );
        let job = result.unwrap();
        assert_eq!(job.command, "echo normal_job_output");

        // No script.sh should exist for non-trusted jobs
        let dir = tmp.path().join(".codex/jobs").join(&job.job_id);
        assert!(
            !dir.join("script.sh").exists(),
            "script.sh should NOT exist for non-trusted jobs"
        );
    }

    #[test]
    fn record_job_action_event_sanitizes_log_and_summary_payloads() {
        let (_tmp, db) = test_db();
        let body = JobOpRequest {
            op: "log".to_string(),
            project: Some("demo".to_string()),
            goal_id: Some("goal-1".to_string()),
            job_id: Some("job-1".to_string()),
            client_request_id: Some("client-1".to_string()),
            suite: Some("test".to_string()),
            command: Some("echo visible".to_string()),
            script_path: None,
            script_args: Vec::new(),
            script_text: Some("printf visible\ncat .env".to_string()),
            trusted: Some(true),
            commands: Vec::new(),
            reason: Some("please stream the logs".to_string()),
            status: None,
            limit: 20,
            tail_lines: 2,
            max_runtime_secs: Some(30),
            detail: Some("logs".to_string()),
            since_line: Some(4),
            response_mode: Some("summary".to_string()),
        };
        let response = JobOpResponse {
            success: true,
            op: "log".to_string(),
            job_id: Some("job-1".to_string()),
            job_ids: vec!["job-1".to_string()],
            job: Some(JobInfo {
                job_id: "job-1".to_string(),
                client_request_id: Some("client-1".to_string()),
                project: "demo".to_string(),
                goal_id: "goal-1".to_string(),
                command: "echo visible".to_string(),
                kind: Some("command".to_string()),
                suite: Some("test".to_string()),
                script_path: None,
                reason: None,
                status: "completed".to_string(),
                created_at: 1,
                started_at: Some(2),
                finished_at: Some(3),
                max_runtime_secs: 30,
                executor: "local".to_string(),
                pid: None,
                exit_code: Some(0),
                elapsed_secs: Some(1),
                oom_hint: None,
            }),
            jobs: Vec::new(),
            stdout_tail: Some("line1\nline2".to_string()),
            stderr_tail: Some("err1".to_string()),
            summary_markdown: Some("# summary\nreal body".to_string()),
            error: None,
            log_total_lines: Some(10),
            next_cursor: Some(11),
            metadata_only: Some(false),
            logs_included: Some(true),
            warnings: Vec::new(),
            recommended_next_action: None,
            action_budget_hint: None,
        };
        record_job_action_event(
            &db,
            Some("session-job".to_string()),
            100,
            &body,
            &response,
            200,
        );
        let events = db.list_action_events("session-job", 10).unwrap();
        let event = decode_event(events.into_iter().next().unwrap());
        assert_eq!(event.endpoint, "/api/codex/job");
        assert_eq!(event.operation.as_deref(), Some("log"));
        assert_eq!(event.ids["job_id"], "job-1");
        assert_eq!(event.summary["tail_lines"], 2);
        assert_eq!(event.summary["since_line"], 4);
        assert_eq!(event.summary["log_total_lines"], 10);
        assert_eq!(event.summary["stdout_chars"], 11);
        assert_eq!(event.summary["stderr_chars"], 4);
        assert_eq!(event.summary["log_truncated"], true);
        assert_eq!(event.summary["summary_length"], 19);
        assert!(event.summary["command"]["sha256"].is_string());
        assert!(event.summary["script_text"]["sha256"].is_string());
        let summary_text = event.summary.to_string();
        assert!(!summary_text.contains("line1"));
        assert!(!summary_text.contains("err1"));
        assert!(!summary_text.contains("real body"));
        let stats = compute_stats(&[event]);
        assert_eq!(stats.job_count, 1);
    }

    #[test]
    fn record_job_action_event_marks_rejected_failures() {
        let (_tmp, db) = test_db();
        let body = JobOpRequest {
            op: "recover".to_string(),
            project: Some("demo".to_string()),
            goal_id: Some("goal-1".to_string()),
            job_id: None,
            client_request_id: Some("missing".to_string()),
            suite: None,
            command: None,
            script_path: None,
            script_args: Vec::new(),
            script_text: None,
            trusted: None,
            commands: Vec::new(),
            reason: None,
            status: None,
            limit: 20,
            tail_lines: 80,
            max_runtime_secs: None,
            detail: None,
            since_line: None,
            response_mode: None,
        };
        let response = JobOpResponse {
            success: false,
            op: "recover".to_string(),
            job_id: None,
            job_ids: Vec::new(),
            job: None,
            jobs: Vec::new(),
            stdout_tail: None,
            stderr_tail: None,
            summary_markdown: None,
            error: Some("job not found for client_request_id".to_string()),
            log_total_lines: None,
            next_cursor: None,
            metadata_only: Some(true),
            logs_included: None,
            warnings: Vec::new(),
            recommended_next_action: None,
            action_budget_hint: None,
        };
        record_job_action_event(
            &db,
            Some("session-job-fail".to_string()),
            100,
            &body,
            &response,
            404,
        );
        let event = decode_event(
            db.list_action_events("session-job-fail", 10)
                .unwrap()
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(event.status, "rejected");
        assert_eq!(event.summary["metadata_only"], true);
    }
}
