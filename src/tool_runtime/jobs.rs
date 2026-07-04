use serde_json::{json, Value};
use std::path::Path;

use super::helpers::{
    command_rejected_message, is_safe_job_id, normalize_local_status, read_json, read_lines_from,
    read_trim, shell_escape_simple,
};
use super::types::{
    LocalJobKiller, LocalJobRecord, TerminateOutcome, ToolResult, ACTIVE_JOB_STATUSES,
    ACTIVE_LOCAL_STATUSES,
};
use super::ToolRuntime;
use crate::auth::AuthContext;
use crate::shell_client::{command_preview, ShellJobStartMetadata};
use crate::shell_protocol::{ShellJobInfo, ShellJobOpRequest};

fn job_id_for_log(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}

/// Build a bounded job summary `Value` for an agent-known job. Never includes
/// stdout/stderr bodies.
pub(crate) fn agent_job_summary_value(job: &ShellJobInfo) -> Value {
    json!({
        "job_id": job.job_id,
        "kind": job.kind,
        "status": job.status,
        "project": job.project_id,
        "executor": "agent",
        "client_id": job.client_id,
        "created_at": job.created_at,
        "started_at": job.started_at,
        "ended_at": job.ended_at,
        "duration_ms": job.duration_ms,
        "elapsed_secs": job.elapsed_secs,
        "exit_code": job.exit_code,
    })
}

/// Build a bounded job summary `Value` for a local on-disk job by reading
/// lightweight metadata/status files. Returns `None` when a status filter is
/// set and the job does not match. Never includes stdout/stderr bodies.
pub(crate) fn local_job_summary_value(
    job_id: &str,
    record: &LocalJobRecord,
    status_filter: &Option<String>,
) -> Option<Value> {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if let Some(filter) = status_filter {
        if &status != filter {
            return None;
        }
    }
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let ended_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
    let kind = meta
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("shell")
        .to_string();
    Some(json!({
        "job_id": job_id,
        "kind": kind,
        "status": status,
        "project": record.project,
        "executor": "local",
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": ended_at,
        "exit_code": exit_code,
    }))
}

pub(crate) fn local_job_status(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
    include_command_preview: bool,
) -> ToolResult {
    // Reclaim overtime jobs before reading status: this persists a terminal
    // `lost` status (and terminates the process group) so callers see a
    // consistent terminal state and we don't leak processes.
    let timeout_note = enforce_local_job_timeout(record, killer);
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    let exit_code = read_trim(record.dir.join("exit_code")).and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let finished_at = read_trim(record.dir.join("finished_at")).and_then(|v| v.parse::<i64>().ok());
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64);
    let elapsed_secs = started_at.map(|started| {
        finished_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .saturating_sub(started) as u64
    });
    let mut output = json!({
        "job_id": job_id,
        "project": record.project,
        "status": status,
        "exit_code": exit_code,
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": finished_at,
        "elapsed_secs": elapsed_secs,
        "max_runtime_secs": max_runtime_secs,
        "executor": "local",
        "kind": meta.get("kind").cloned().unwrap_or_else(|| Value::String("shell".to_string())),
    });
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    if include_command_preview {
        if let Some(command) = meta.get("command").and_then(Value::as_str) {
            output["command_preview"] = Value::String(command_preview(command));
        }
    }
    ToolResult::ok(output)
}

pub(crate) fn local_job_log(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
    offset: Option<usize>,
    tail_lines: Option<usize>,
) -> ToolResult {
    // A log query on an overtime job also reclaims it so the reported status
    // is terminal and the process group is not leaked.
    let timeout_note = enforce_local_job_timeout(record, killer);
    let stdout = read_lines_from(record.dir.join("stdout.log"), offset, tail_lines);
    let stderr = read_lines_from(record.dir.join("stderr.log"), offset, tail_lines);
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    let mut output = json!({
        "job_id": job_id,
        "status": status,
        "stdout": stdout.0,
        "stderr": stderr.0,
        "next_stdout_line": stdout.1,
        "next_stderr_line": stderr.1,
    });
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    ToolResult::ok(output)
}

/// Resolve the process-group id to signal for a local job. Prefers an explicit
/// `process_group_id` in metadata (written by current spawn code); falls back
/// to the `pid` file, which under `setsid` is equal to the pgid. Returns
/// `None` when neither is recorded (e.g. very old metadata predating pid
/// tracking) — in that case we never guess at a pid to kill.
pub(crate) fn resolve_job_pgid(meta: &Value, record: &LocalJobRecord) -> Option<i64> {
    meta.get("process_group_id")
        .and_then(Value::as_i64)
        .or_else(|| read_trim(record.dir.join("pid")).and_then(|s| s.parse::<i64>().ok()))
}

/// If a local job is still `running` but has exceeded `max_runtime_secs`,
/// terminate its process group and persist a terminal `lost` status. Returns a
/// short human-readable note when a timeout was enforced, or `None` if the job
/// is not running or not over time.
///
/// Safety: the pid/pgid come only from this job's own on-disk files (written by
/// us at spawn time via `setsid`). We never kill based on caller-supplied pids.
/// If no pid/pgid is recorded, we only mark the job `lost` — never guess. Kill
/// failures never panic; a conservative `lost` status is persisted regardless.
pub(crate) fn enforce_local_job_timeout(
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> Option<String> {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    if normalize_local_status(&raw_status) != "running" {
        return None;
    }
    let started_at = meta.get("started_at").and_then(Value::as_i64)?;
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64)?;
    // The wrapper writes `finished_at` before `status`. If it exists, the job
    // just finished (or was already reclaimed) — do not double-reclaim.
    if read_trim(record.dir.join("finished_at")).is_some() {
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    if now.saturating_sub(started_at) <= max_runtime_secs {
        return None;
    }
    // Over time. Reclaim the process group if we recorded one.
    let pgid = resolve_job_pgid(&meta, record);
    let note = match pgid {
        Some(pgid) => {
            let pid = read_trim(record.dir.join("pid"))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!(
                        "timed out after {}s; process group {} terminated ({})",
                        max_runtime_secs, pgid, sig
                    )
                }
                TerminateOutcome::AlreadyGone => format!(
                    "timed out after {}s; process group {} already exited; marked lost",
                    max_runtime_secs, pgid
                ),
            }
        }
        None => format!(
            "timed out after {}s; no pid/process_group_id on record; marked lost",
            max_runtime_secs
        ),
    };
    // Persist terminal state so subsequent reads are consistent and we don't
    // repeatedly attempt to kill. The wrapper shell was part of the group and
    // is now gone, so it will not write its own status/finished_at.
    if let Err(e) = std::fs::write(record.dir.join("status"), "lost") {
        tracing::warn!(
            job_id = %job_id_for_log(&record.dir),
            error = %e,
            "failed to write timed-out local job status"
        );
    }
    if let Err(e) = std::fs::write(record.dir.join("finished_at"), now.to_string()) {
        tracing::warn!(
            job_id = %job_id_for_log(&record.dir),
            error = %e,
            "failed to write timed-out local job finished_at"
        );
    }
    Some(note)
}

/// Stop a local job by terminating its process group and persisting a
/// `stopped` status. Only acts on active jobs; terminal jobs are left alone.
/// Like `enforce_local_job_timeout`, the pid/pgid come only from the job's own
/// on-disk files, and missing pid/pgid yields a conservative `stopped` marker
/// without guessing. Kill failures never panic.
pub(crate) fn stop_local_job(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> ToolResult {
    let meta = read_json(record.dir.join("metadata.json"));
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if !ACTIVE_LOCAL_STATUSES.contains(&status.as_str()) {
        return ToolResult::ok(json!({
            "job_id": job_id,
            "project": record.project,
            "status": status,
            "note": "job already terminal; not stopped again",
        }));
    }
    let now = chrono::Utc::now().timestamp();
    let note = match resolve_job_pgid(&meta, record) {
        Some(pgid) => {
            let pid = read_trim(record.dir.join("pid"))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!("stopped; process group {} terminated ({})", pgid, sig)
                }
                TerminateOutcome::AlreadyGone => {
                    format!("stopped; process group {} already exited", pgid)
                }
            }
        }
        None => "stopped; no pid/process_group_id on record; marked stopped".to_string(),
    };
    if let Err(e) = std::fs::write(record.dir.join("status"), "stopped") {
        tracing::warn!(
            job_id,
            error = %e,
            "failed to write stopped local job status"
        );
    }
    if let Err(e) = std::fs::write(record.dir.join("finished_at"), now.to_string()) {
        tracing::warn!(
            job_id,
            error = %e,
            "failed to write stopped local job finished_at"
        );
    }
    ToolResult::ok(json!({
        "job_id": job_id,
        "project": record.project,
        "status": "stopped",
        "note": note,
    }))
}

fn local_job_status_string(record: &LocalJobRecord) -> String {
    let raw_status = read_trim(record.dir.join("status")).unwrap_or_default();
    normalize_local_status(&raw_status)
}

fn local_job_session_id(record: &LocalJobRecord) -> Option<String> {
    read_json(record.dir.join("metadata.json"))
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn confirmation_required_result(project: &str, job_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        "confirmation_required: stop_job requires confirm=true".to_string(),
        json!({
            "error_kind": "confirmation_required",
            "failure_kind": "confirmation_required",
            "project": project,
            "job_id": job_id,
            "stopped": false,
            "already_finished": false,
            "command_started": false,
        }),
    )
}

fn job_not_found_result(project: &str, job_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("job_not_found: {}", job_id),
        json!({
            "error_kind": "job_not_found",
            "failure_kind": "job_not_found",
            "project": project,
            "job_id": job_id,
            "stopped": false,
            "already_finished": false,
            "command_started": false,
        }),
    )
}

fn job_project_mismatch_result(
    request_project: &str,
    job_project: &str,
    job_id: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "job_project_mismatch: job {} belongs to project {} but request used {}",
            job_id, job_project, request_project
        ),
        json!({
            "error_kind": "job_project_mismatch",
            "failure_kind": "job_project_mismatch",
            "project": request_project,
            "job_project": job_project,
            "job_id": job_id,
            "stopped": false,
            "already_finished": false,
            "command_started": false,
        }),
    )
}

fn job_stop_forbidden_result(
    request_project: &str,
    job_id: &str,
    request_session_id: Option<&str>,
    job_session_id: Option<&str>,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "job_stop_forbidden: job {} is bound to a different session",
            job_id
        ),
        json!({
            "error_kind": "job_stop_forbidden",
            "failure_kind": "job_stop_forbidden",
            "project": request_project,
            "job_id": job_id,
            "request_session_id": request_session_id,
            "job_session_id": job_session_id,
            "stopped": false,
            "already_finished": false,
            "command_started": false,
        }),
    )
}

fn job_session_unknown_warning() -> Value {
    json!({
        "kind": "job_session_unknown",
        "warning_kind": "job_session_unknown",
        "message": "job has no recorded session_id; stop authorized by project boundary only",
    })
}

fn ownership_basis_for_stop(
    request_project: &str,
    job_id: &str,
    request_session_id: Option<&str>,
    job_session_id: Option<&str>,
) -> Result<(&'static str, Vec<Value>), ToolResult> {
    match job_session_id {
        Some(job_session_id) if Some(job_session_id) == request_session_id => {
            Ok(("project_and_session", Vec::new()))
        }
        Some(job_session_id) => Err(job_stop_forbidden_result(
            request_project,
            job_id,
            request_session_id,
            Some(job_session_id),
        )),
        None => Ok((
            "unknown_session_project_only",
            vec![job_session_unknown_warning()],
        )),
    }
}

fn stop_job_output(
    project: &str,
    job_id: &str,
    status_before: &str,
    status_after: &str,
    stopped: bool,
    already_finished: bool,
    ownership_basis: &str,
    warnings: Vec<Value>,
) -> Value {
    let mut output = json!({
        "stopped": stopped,
        "already_finished": already_finished,
        "job_id": job_id,
        "project": project,
        "status_before": status_before,
        "status_after": status_after,
        "command_started": false,
        "ownership_basis": ownership_basis,
    });
    if !warnings.is_empty() {
        output["warning_kind"] = warnings
            .first()
            .and_then(|warning| warning.get("warning_kind"))
            .cloned()
            .unwrap_or(Value::Null);
        output["warnings"] = Value::Array(warnings);
    }
    output
}

fn active_job_brief(summary: &Value) -> Value {
    json!({
        "job_id": summary.get("job_id").cloned().unwrap_or(Value::Null),
        "kind": summary.get("kind").cloned().unwrap_or_else(|| json!("shell")),
        "status": summary.get("status").cloned().unwrap_or(Value::Null),
        "project": summary.get("project").cloned().unwrap_or(Value::Null),
        "started_at": summary.get("started_at").cloned().unwrap_or(Value::Null),
        "created_at": summary.get("created_at").cloned().unwrap_or(Value::Null),
        "executor": summary.get("executor").cloned().unwrap_or(Value::Null),
    })
}

impl ToolRuntime {
    pub(crate) fn local_jobs_visible_to_auth(auth: Option<&AuthContext>) -> bool {
        !auth
            .map(|auth| auth.is_lightweight() || auth.is_oauth_shared_key_subject())
            .unwrap_or(false)
    }

    pub(crate) async fn run_job(
        &self,
        project: String,
        command: String,
        session_id: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(e) => return ToolResult::err(command_rejected_message(
                e.to_message(),
                "verify the project id with list_projects, then retry with a registered project.",
            )),
        };
        let project_id = resolved.resolved_id.clone();
        let proj = resolved.config;
        let max_runtime = timeout_secs.unwrap_or(3600).clamp(1, 604800);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "refresh the agent project registry with list_projects, then retry.",
                    ))
                }
            };
            match self
                .shell_clients
                .start_job_with_metadata(
                    ShellJobOpRequest {
                        op: "start".to_string(),
                        client_id: Some(client_id),
                        cwd: cwd.or_else(|| Some(proj.path.clone())),
                        command: Some(command),
                        timeout_secs: Some(max_runtime as u64),
                        job_id: None,
                        since_stdout_line: None,
                        since_stderr_line: None,
                        tail_lines: None,
                        limit: None,
                        codex: None,
                    },
                    "tool_runtime".to_string(),
                    ShellJobStartMetadata {
                        project_id: Some(project_id.clone()),
                        session_id: session_id.clone(),
                    },
                )
                .await
            {
                Ok(job) => ToolResult::ok(json!({
                    "job_id": job.job_id,
                    "kind": job.kind,
                    "status": job.status,
                    "project": project_id,
                })),
                Err(e) => ToolResult::err(command_rejected_message(
                    e,
                    "confirm the agent is connected and async jobs are allowed, then retry or use run_shell for short commands.",
                )),
            }
        } else {
            let root = proj.root();
            let job_id = uuid::Uuid::new_v4().to_string();
            let dir = root.join(format!(".codex/jobs/{}", job_id));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return ToolResult::err(format!("Failed to create job dir: {}", e));
            }
            let now = chrono::Utc::now().timestamp();
            let mut meta = json!({
                "job_id": job_id,
                "project": project_id.clone(),
                "command": command,
                "status": "running",
                "created_at": now,
                "started_at": now,
                "max_runtime_secs": max_runtime,
                "executor": "local",
                "path": proj.path.clone(),
                "kind": "shell",
            });
            if let Some(session_id) = session_id.as_ref() {
                meta["session_id"] = json!(session_id);
            }
            if let Err(e) = std::fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap_or_default(),
            ) {
                return ToolResult::err(format!("Failed to write metadata: {}", e));
            }
            let cmd_content = format!("#!/usr/bin/env bash\n{}\n", command);
            if let Err(e) = std::fs::write(dir.join("command.sh"), &cmd_content) {
                return ToolResult::err(format!("Failed to write command.sh: {}", e));
            }
            if let Err(e) = std::fs::write(dir.join("status"), "running") {
                tracing::warn!(
                    job_id = %job_id,
                    error = %e,
                    "failed to write initial local job status"
                );
            }
            let dir_s = dir.to_string_lossy().to_string();
            let wrapper = format!(
                "bash {0}/command.sh > {0}/stdout.log 2> {0}/stderr.log; code=$?; echo $code > {0}/exit_code; finished=$(date +%s); echo $finished > {0}/finished_at; if [ $code -eq 0 ]; then echo completed > {0}/status; else echo failed > {0}/status; fi",
                shell_escape_simple(&dir_s)
            );
            match std::process::Command::new("setsid")
                .arg("sh")
                .arg("-c")
                .arg(wrapper)
                .current_dir(&root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    // `setsid` makes the child a session + process-group
                    // leader, so child.id() is both the leader pid and the
                    // process-group id. Record the pgid so timeout/stop can
                    // signal the whole subtree (`kill -<pgid>`).
                    let pgid = child.id() as i64;
                    if let Err(e) = std::fs::write(dir.join("pid"), child.id().to_string()) {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to write local job pid"
                        );
                    }
                    meta["process_group_id"] = json!(pgid);
                    if let Err(e) = std::fs::write(
                        dir.join("metadata.json"),
                        serde_json::to_string_pretty(&meta).unwrap_or_default(),
                    ) {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to update local job metadata with process group"
                        );
                    }
                    self.local_jobs.lock().await.insert(
                        job_id.clone(),
                        LocalJobRecord {
                            project: project_id.clone(),
                            dir,
                        },
                    );
                    ToolResult::ok(json!({
                        "job_id": job_id,
                        "kind": "shell",
                        "status": "running",
                        "project": project_id,
                    }))
                }
                Err(e) => ToolResult::err(format!("Failed to spawn job: {}", e)),
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn job_status(&self, job_id: String) -> ToolResult {
        self.job_status_for_auth(job_id, false, None).await
    }

    pub(crate) async fn job_status_for_auth(
        &self,
        job_id: String,
        include_command_preview: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            if !Self::local_jobs_visible_to_auth(auth) {
                return ToolResult::err(format!("unknown job: {}", job_id));
            }
            return local_job_status(&job_id, &record, killer, include_command_preview);
        }
        // Fall through to agent-backed jobs. If the agent registry does not
        // know this job either, attempt local recovery from on-disk metadata
        // so jobs started before a server restart remain queryable.
        if self
            .shell_clients
            .get_job_for_auth(auth, &job_id)
            .await
            .is_err()
        {
            if let Some(record) = self.recover_local_job(&job_id).await {
                if !Self::local_jobs_visible_to_auth(auth) {
                    return ToolResult::err(format!("unknown job: {}", job_id));
                }
                return local_job_status(&job_id, &record, killer, include_command_preview);
            }
            return ToolResult::err(format!("unknown job: {}", job_id));
        }
        match self.shell_clients.get_job_for_auth(auth, &job_id).await {
            Ok(job) => {
                let mut output = json!({
                    "job_id": job.job_id,
                    "status": job.status,
                    "exit_code": job.exit_code,
                    "started_at": job.started_at,
                    "ended_at": job.ended_at,
                    "duration_ms": job.duration_ms,
                    "elapsed_secs": job.elapsed_secs,
                    "client_id": job.client_id,
                    "error": job.error,
                });
                if include_command_preview {
                    output["command_preview"] = Value::String(job.command_preview);
                }
                ToolResult::ok(output)
            }
            Err(_) => ToolResult::err(format!("unknown job: {}", job_id)),
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn job_log(
        &self,
        job_id: String,
        offset: Option<usize>,
        tail_lines: Option<usize>,
    ) -> ToolResult {
        self.job_log_for_auth(job_id, offset, tail_lines, None)
            .await
    }

    pub(crate) async fn job_log_for_auth(
        &self,
        job_id: String,
        offset: Option<usize>,
        tail_lines: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            if !Self::local_jobs_visible_to_auth(auth) {
                return ToolResult::err(format!("unknown job: {}", job_id));
            }
            return local_job_log(&job_id, &record, killer, offset, tail_lines);
        }
        if self
            .shell_clients
            .get_job_for_auth(auth, &job_id)
            .await
            .is_err()
        {
            if let Some(record) = self.recover_local_job(&job_id).await {
                if !Self::local_jobs_visible_to_auth(auth) {
                    return ToolResult::err(format!("unknown job: {}", job_id));
                }
                return local_job_log(&job_id, &record, killer, offset, tail_lines);
            }
            return ToolResult::err(format!("unknown job: {}", job_id));
        }
        match self
            .shell_clients
            .job_log_for_auth(auth, &job_id, offset, None, tail_lines.or(Some(500)))
            .await
        {
            Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => {
                ToolResult::ok(json!({
                    "job_id": job.job_id,
                    "status": job.status,
                    "stdout": stdout,
                    "stderr": stderr,
                    "next_stdout_line": next_stdout_line,
                    "next_stderr_line": next_stderr_line,
                }))
            }
            Err(_) => ToolResult::err(format!("unknown job: {}", job_id)),
        }
    }

    /// `list_jobs`: bounded job summaries across agent and local executors.
    /// Never returns stdout/stderr bodies — only metadata.
    #[allow(dead_code)]
    pub(crate) async fn list_jobs(
        &self,
        limit: Option<usize>,
        status: Option<String>,
    ) -> ToolResult {
        self.list_jobs_for_auth(limit, status, None).await
    }

    pub(crate) async fn list_jobs_for_auth(
        &self,
        limit: Option<usize>,
        status: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let max = limit.unwrap_or(20).clamp(1, 100);
        let status_filter = status
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // Agent jobs come pre-bounded to `max` by the registry. Local jobs are
        // collected fully (the in-memory map is small) so truncation can be
        // detected accurately for the common local-only case.
        let agent_jobs = self.shell_clients.list_jobs_for_auth(auth, Some(max)).await;
        let mut summaries: Vec<Value> = agent_jobs
            .iter()
            .filter(|j| {
                status_filter
                    .as_ref()
                    .map(|s| s == &j.status)
                    .unwrap_or(true)
            })
            .map(agent_job_summary_value)
            .collect();
        let local_records: Vec<(String, LocalJobRecord)> = if Self::local_jobs_visible_to_auth(auth)
        {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .iter()
                .map(|(job_id, record)| (job_id.clone(), record.clone()))
                .collect()
        } else {
            Vec::new()
        };
        for (job_id, record) in &local_records {
            if let Some(summary) = local_job_summary_value(job_id, record, &status_filter) {
                summaries.push(summary);
            }
        }
        summaries.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["created_at"].as_i64().unwrap_or(0))
        });
        let truncated = summaries.len() > max;
        summaries.truncate(max);
        ToolResult::ok(json!({
            "jobs": summaries,
            "count": summaries.len(),
            "truncated": truncated,
        }))
    }

    /// `job_tail`: bounded stdout/stderr tails for a job. Reuses the bounded
    /// `job_log` path with a tail-focused default so the console never reads
    /// full logs by default.
    #[allow(dead_code)]
    pub(crate) async fn job_tail(&self, job_id: String, tail_lines: Option<usize>) -> ToolResult {
        self.job_tail_for_auth(job_id, tail_lines, None).await
    }

    pub(crate) async fn job_tail_for_auth(
        &self,
        job_id: String,
        tail_lines: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let tail = tail_lines.unwrap_or(200).clamp(1, 500);
        self.job_log_for_auth(job_id, None, Some(tail), auth).await
    }

    /// Model-facing `stop_job`: requires confirm=true, verifies project/session
    /// ownership, and never exposes stdout/stderr.
    pub(crate) async fn stop_job_model_facing(
        &self,
        project: String,
        job_id: String,
        session_id: Option<String>,
        confirm: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !confirm {
            return confirmation_required_result(&project, &job_id);
        }
        if !is_safe_job_id(&job_id) {
            return job_not_found_result(&project, &job_id);
        }

        let cached = {
            let jobs = self.local_jobs.lock().await;
            jobs.get(&job_id).cloned()
        };
        if let Some(record) = match cached {
            Some(record) => Some(record),
            None => self.recover_local_job(&job_id).await,
        } {
            if !Self::local_jobs_visible_to_auth(auth) {
                return job_not_found_result(&project, &job_id);
            }
            let request_project = self
                .resolve_project_input_for_auth(&project, auth)
                .await
                .map(|resolved| resolved.resolved_id)
                .unwrap_or_else(|_| project.trim().to_string());
            if record.project != request_project {
                return job_project_mismatch_result(&request_project, &record.project, &job_id);
            }
            let job_session_id = local_job_session_id(&record);
            let (ownership_basis, warnings) = match ownership_basis_for_stop(
                &request_project,
                &job_id,
                session_id.as_deref(),
                job_session_id.as_deref(),
            ) {
                Ok(value) => value,
                Err(result) => return result,
            };
            let status_before = local_job_status_string(&record);
            if !ACTIVE_LOCAL_STATUSES.contains(&status_before.as_str()) {
                return ToolResult::ok(stop_job_output(
                    &request_project,
                    &job_id,
                    &status_before,
                    &status_before,
                    false,
                    true,
                    ownership_basis,
                    warnings,
                ));
            }
            let stop_result = stop_local_job(&job_id, &record, self.job_killer.as_ref());
            if !stop_result.success {
                return stop_result;
            }
            let status_after = stop_result
                .output
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("stopped")
                .to_string();
            return ToolResult::ok(stop_job_output(
                &request_project,
                &job_id,
                &status_before,
                &status_after,
                true,
                false,
                ownership_basis,
                warnings,
            ));
        }

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let request_project = resolved.resolved_id;
        let job = match self.shell_clients.get_job_for_auth(auth, &job_id).await {
            Ok(job) => job,
            Err(_) => return job_not_found_result(&request_project, &job_id),
        };
        let Some(job_project) = job
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
        else {
            return job_stop_forbidden_result(
                &request_project,
                &job_id,
                session_id.as_deref(),
                job.session_id.as_deref(),
            );
        };
        if job_project != request_project {
            return job_project_mismatch_result(&request_project, job_project, &job_id);
        }
        let (ownership_basis, warnings) = match ownership_basis_for_stop(
            &request_project,
            &job_id,
            session_id.as_deref(),
            job.session_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let status_before = job.status.clone();
        if !ACTIVE_JOB_STATUSES.contains(&status_before.as_str()) {
            return ToolResult::ok(stop_job_output(
                &request_project,
                &job_id,
                &status_before,
                &status_before,
                false,
                true,
                ownership_basis,
                warnings,
            ));
        }
        let stopped = match self
            .shell_clients
            .stop_job(&job_id, "tool_runtime".to_string())
            .await
        {
            Ok(job) => job,
            Err(_) => return job_not_found_result(&request_project, &job_id),
        };
        ToolResult::ok(stop_job_output(
            &request_project,
            &job_id,
            &status_before,
            &stopped.status,
            true,
            false,
            ownership_basis,
            warnings,
        ))
    }

    /// Bounded active job summary for finish/handoff. Never returns stdout,
    /// stderr, tails, command text, or command previews.
    pub(crate) async fn active_jobs_summary(
        &self,
        project: Option<&str>,
        auth: Option<&AuthContext>,
        limit: usize,
    ) -> Value {
        let max = limit.clamp(1, 20);
        let mut active = Vec::new();
        for job in self.shell_clients.list_jobs_for_auth(auth, Some(100)).await {
            if !ACTIVE_JOB_STATUSES.contains(&job.status.as_str()) {
                continue;
            }
            if let Some(project) = project {
                if job.project_id.as_deref() != Some(project) {
                    continue;
                }
            }
            active.push(agent_job_summary_value(&job));
        }

        if Self::local_jobs_visible_to_auth(auth) {
            let local_records: Vec<(String, LocalJobRecord)> = {
                let local_jobs_map = self.local_jobs.lock().await;
                local_jobs_map
                    .iter()
                    .map(|(job_id, record)| (job_id.clone(), record.clone()))
                    .collect()
            };
            for (job_id, record) in local_records {
                if let Some(project) = project {
                    if record.project != project {
                        continue;
                    }
                }
                let status = local_job_status_string(&record);
                if !ACTIVE_JOB_STATUSES.contains(&status.as_str()) {
                    continue;
                }
                if let Some(summary) = local_job_summary_value(&job_id, &record, &None) {
                    active.push(summary);
                }
            }
        }

        active.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["created_at"].as_i64().unwrap_or(0))
        });
        let active_count = active.len();
        let recent: Vec<Value> = active.iter().take(max).map(active_job_brief).collect();
        let mut warnings = Vec::new();
        if active_count > 0 {
            warnings.push(json!({
                "kind": "active_jobs_present",
                "message": format!(
                    "{} active job{} still running",
                    active_count,
                    if active_count == 1 { "" } else { "s" }
                ),
            }));
        }
        json!({
            "active_count": active_count,
            "recent": recent,
            "recent_limit": max,
            "truncated": active_count > max,
            "warnings": warnings,
        })
    }

    /// Stop a local job by terminating its process group and marking it
    /// `stopped`.
    ///
    /// This is an internal lifecycle method intended as the implementation
    /// backing a future explicit stop API; it is deliberately **not** exposed
    /// as a GPT Actions / MCP write tool, to avoid surfacing an arbitrary kill
    /// surface to remote callers. Only jobs we created and recorded (in-memory
    /// or recoverable on disk) can be stopped, and the pid/pgid come
    /// exclusively from the job's own on-disk files — never from caller input.
    pub async fn stop_job(&self, job_id: String) -> ToolResult {
        if !is_safe_job_id(&job_id) {
            return ToolResult::err("invalid job id");
        }
        let cached = {
            let jobs = self.local_jobs.lock().await;
            jobs.get(&job_id).cloned()
        };
        let record = match cached {
            Some(r) => r,
            None => match self.recover_local_job(&job_id).await {
                Some(r) => r,
                None => return ToolResult::err(format!("unknown job: {}", job_id)),
            },
        };
        stop_local_job(&job_id, &record, self.job_killer.as_ref())
    }

    /// Recover a local job from on-disk `.codex/jobs/<job_id>/metadata.json`
    /// under any configured project root. Rejects job ids that could escape
    /// the project directory and verifies the metadata matches the configured
    /// project before caching the record in memory.
    pub(crate) async fn recover_local_job(&self, job_id: &str) -> Option<LocalJobRecord> {
        if !is_safe_job_id(job_id) {
            return None;
        }
        let projects = self.projects.config.as_ref()?;
        for (id, proj) in &projects.projects {
            let root = proj.root();
            let job_dir = root.join(format!(".codex/jobs/{}", job_id));
            let meta_path = job_dir.join("metadata.json");
            if !meta_path.exists() {
                continue;
            }
            // Path safety: canonicalize both and verify the job dir is under
            // the configured project root.
            let canonical_root = match root.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let canonical_job_dir = match job_dir.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !canonical_job_dir.starts_with(&canonical_root) {
                continue;
            }
            // Verify metadata belongs to this configured project. This stops a
            // recovered job from one project being mistaken for another.
            let meta = read_json(meta_path);
            let meta_project = meta.get("project").and_then(Value::as_str).unwrap_or("");
            let meta_path_str = meta.get("path").and_then(Value::as_str).unwrap_or("");
            if meta_project != id || meta_path_str != proj.path {
                continue;
            }
            let record = LocalJobRecord {
                project: id.clone(),
                dir: job_dir.clone(),
            };
            self.local_jobs
                .lock()
                .await
                .insert(job_id.to_string(), record.clone());
            return Some(record);
        }
        None
    }
}
