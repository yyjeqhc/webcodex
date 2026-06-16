use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentJobUpdateResponse, ShellAgentPollRequest,
    ShellAgentPollResponse, ShellAgentResultRequest, ShellAgentResultResponse,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
    ShellClientRegisterResponse, ShellClientView, ShellClientsResponse, ShellFileOpRequest,
    ShellFileOpResponse, ShellJobInfo, ShellJobOpRequest, ShellJobOpResponse, ShellRunRequest,
    ShellRunResponse,
};
use salvo::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const MAX_CLIENT_ID_LEN: usize = 80;
const MAX_CLIENT_FIELD_LEN: usize = 200;
const MAX_COMMAND_LEN: usize = 8_000;
const MAX_CWD_LEN: usize = 1_024;
const MAX_FILE_PATH_LEN: usize = 2_048;
const MAX_FILE_CONTENT_BYTES: usize = 512 * 1024;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_SYNC_WAIT_SECS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 24 * 60 * 60;
const CLIENT_ONLINE_WINDOW_SECS: i64 = 60;

#[derive(Debug, Clone)]
struct ShellClientRecord {
    client_id: String,
    display_name: Option<String>,
    owner: Option<String>,
    hostname: Option<String>,
    capabilities: ShellClientCapabilities,
    last_seen: i64,
}

#[derive(Debug)]
struct PendingShellRequest {
    request: ShellAgentShellRequest,
    waiter: Option<oneshot::Sender<ShellRunResponse>>,
    job_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ShellJobRecord {
    job_id: String,
    request_id: Option<String>,
    client_id: String,
    cwd: Option<String>,
    command_preview: String,
    status: String,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct ShellClientRegistryInner {
    clients: HashMap<String, ShellClientRecord>,
    pending_by_id: HashMap<String, PendingShellRequest>,
    queues_by_client: HashMap<String, VecDeque<String>>,
    jobs_by_id: HashMap<String, ShellJobRecord>,
    request_to_job: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct ShellClientRegistry {
    inner: Mutex<ShellClientRegistryInner>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn validate_id(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_CLIENT_ID_LEN {
        return Err(format!(
            "{} must be 1..={} characters",
            field, MAX_CLIENT_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '-', '_', and '.'",
            field
        ));
    }
    Ok(())
}

fn validate_optional_field(value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(value) = value {
        if value.chars().count() > MAX_CLIENT_FIELD_LEN {
            return Err(format!(
                "{} is too long; maximum is {} characters",
                field, MAX_CLIENT_FIELD_LEN
            ));
        }
        if value.contains('\0') {
            return Err(format!("{} cannot contain NUL bytes", field));
        }
    }
    Ok(())
}

fn validate_file_request(body: &ShellFileOpRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    match body.op.as_str() {
        "read" | "write" | "list" => {}
        _ => return Err("op must be one of read, write, list".to_string()),
    }
    let path = body.path.trim();
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if body.path.len() > MAX_FILE_PATH_LEN {
        return Err(format!(
            "path is too long; maximum is {} bytes",
            MAX_FILE_PATH_LEN
        ));
    }
    if body.path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if let Some(content) = &body.content {
        if content.len() > MAX_FILE_CONTENT_BYTES {
            return Err(format!(
                "content is too large; maximum is {} bytes",
                MAX_FILE_CONTENT_BYTES
            ));
        }
        if body.op != "write" {
            return Err("content is only allowed for op=write".to_string());
        }
    }
    if body.op == "write" && body.content.is_none() {
        return Err("content is required for op=write".to_string());
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for shellFileOp",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

fn validate_run_request(body: &ShellRunRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    let command = body.command.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if body.command.len() > MAX_COMMAND_LEN {
        return Err(format!(
            "command is too long; maximum is {} bytes",
            MAX_COMMAND_LEN
        ));
    }
    if body.command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if body.timeout_secs == 0 || body.timeout_secs > MAX_COMMAND_TIMEOUT_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {}",
            MAX_COMMAND_TIMEOUT_SECS
        ));
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for synchronous runShell",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

fn trim_string(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn command_preview(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    const MAX_PREVIEW: usize = 120;
    if first_line.chars().count() <= MAX_PREVIEW {
        first_line.to_string()
    } else {
        let preview = first_line.chars().take(MAX_PREVIEW).collect::<String>();
        format!("{}…", preview)
    }
}

fn truncate_output(value: Option<String>) -> Option<String> {
    value.map(|s| {
        if s.len() <= MAX_OUTPUT_BYTES {
            s
        } else {
            let mut start = s.len() - MAX_OUTPUT_BYTES;
            while start < s.len() && !s.is_char_boundary(start) {
                start += 1;
            }
            format!(
                "[output truncated to last {} bytes]\n{}",
                MAX_OUTPUT_BYTES,
                &s[start..]
            )
        }
    })
}

fn job_view(job: &ShellJobRecord) -> ShellJobInfo {
    ShellJobInfo {
        job_id: job.job_id.clone(),
        request_id: job.request_id.clone(),
        client_id: job.client_id.clone(),
        cwd: job.cwd.clone(),
        command_preview: job.command_preview.clone(),
        status: job.status.clone(),
        created_at: job.created_at,
        started_at: job.started_at,
        ended_at: job.ended_at,
        exit_code: job.exit_code,
        duration_ms: job.duration_ms,
        error: job.error.clone(),
    }
}

fn select_lines(
    value: Option<&String>,
    since_line: Option<usize>,
    tail_lines: Option<usize>,
) -> (Option<String>, usize) {
    let Some(value) = value else {
        return (Some(String::new()), since_line.unwrap_or(1));
    };
    let lines = value.lines().collect::<Vec<_>>();
    if let Some(tail) = tail_lines.filter(|n| *n > 0) {
        let start = lines.len().saturating_sub(tail);
        let selected = lines[start..].join("\n");
        let text = if selected.is_empty() {
            selected
        } else {
            format!("{}\n", selected)
        };
        return (Some(text), lines.len() + 1);
    }
    let start_line = since_line.unwrap_or(1).max(1);
    let start_idx = start_line.saturating_sub(1).min(lines.len());
    let selected = lines[start_idx..].join("\n");
    let text = if selected.is_empty() {
        selected
    } else {
        format!("{}\n", selected)
    };
    (Some(text), lines.len() + 1)
}

fn append_limited(target: &mut Option<String>, chunk: Option<String>) {
    let Some(chunk) = chunk else {
        return;
    };
    let target_value = target.get_or_insert_with(String::new);
    target_value.push_str(&chunk);
    if target_value.len() > MAX_OUTPUT_BYTES {
        let mut start = target_value.len() - MAX_OUTPUT_BYTES;
        while start < target_value.len() && !target_value.is_char_boundary(start) {
            start += 1;
        }
        *target_value = format!(
            "[output truncated to last {} bytes]\n{}",
            MAX_OUTPUT_BYTES,
            &target_value[start..]
        );
    }
}

fn is_final_job_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "stopped" | "timeout" | "lost"
    )
}

fn client_is_connected_locked(inner: &ShellClientRegistryInner, client_id: &str) -> bool {
    inner
        .clients
        .get(client_id)
        .map(|client| now_ts().saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS)
        .unwrap_or(false)
}

fn refresh_job_status_locked(inner: &mut ShellClientRegistryInner, job_id: &str) {
    let Some(job) = inner.jobs_by_id.get(job_id) else {
        return;
    };
    if is_final_job_status(&job.status)
        || !matches!(job.status.as_str(), "running" | "stop_requested")
    {
        return;
    }
    let client_id = job.client_id.clone();
    if client_is_connected_locked(inner, &client_id) {
        return;
    }
    if let Some(job) = inner.jobs_by_id.get_mut(job_id) {
        job.status = "lost".to_string();
        job.ended_at = Some(now_ts());
        job.error = Some("shell client went stale while job was running".to_string());
    }
}

impl ShellClientRegistry {
    pub async fn register(
        &self,
        body: ShellClientRegisterRequest,
    ) -> Result<ShellClientView, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_optional_field(&body.display_name, "display_name")?;
        validate_optional_field(&body.owner, "owner")?;
        validate_optional_field(&body.hostname, "hostname")?;

        let client_id = body.client_id.trim().to_string();
        let record = ShellClientRecord {
            client_id: client_id.clone(),
            display_name: trim_string(body.display_name),
            owner: trim_string(body.owner),
            hostname: trim_string(body.hostname),
            capabilities: body.capabilities.unwrap_or_default(),
            last_seen: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        inner.clients.insert(client_id.clone(), record);
        Ok(Self::client_view_locked(&inner, &client_id).expect("client just inserted"))
    }

    pub async fn list_clients(&self) -> Vec<ShellClientView> {
        let inner = self.inner.lock().await;
        let mut ids = inner.clients.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| Self::client_view_locked(&inner, &id))
            .collect()
    }

    pub async fn enqueue_file_op(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let kind = format!("file_{}", body.op);
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind,
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: Some(body.path.trim().to_string()),
            content: body.content.clone(),
            max_bytes: body.max_bytes,
            command: String::new(),
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(&body.client_id) {
            return Err(format!("unknown shell client: {}", body.client_id));
        }
        inner
            .queues_by_client
            .entry(body.client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
                job_id: None,
            },
        );
        Ok((request_id, rx))
    }

    pub async fn enqueue_run(
        &self,
        body: ShellRunRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_run_request(&body)?;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            command: body.command.clone(),
            timeout_secs: body.timeout_secs,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(&body.client_id) {
            return Err(format!("unknown shell client: {}", body.client_id));
        }
        inner
            .queues_by_client
            .entry(body.client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
                job_id: None,
            },
        );
        Ok((request_id, rx))
    }

    pub async fn cancel_request(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending_by_id.remove(request_id);
        for queue in inner.queues_by_client.values_mut() {
            queue.retain(|id| id != request_id);
        }
    }

    pub async fn poll(
        &self,
        body: ShellAgentPollRequest,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        validate_id(&body.client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(&body.client_id) else {
            return Err(format!("unknown shell client: {}", body.client_id));
        };
        client.last_seen = now_ts();
        loop {
            let request_id = {
                let Some(queue) = inner.queues_by_client.get_mut(&body.client_id) else {
                    return Ok(None);
                };
                queue.pop_front()
            };
            let Some(request_id) = request_id else {
                return Ok(None);
            };
            let Some((request, job_id)) = inner
                .pending_by_id
                .get(&request_id)
                .map(|pending| (pending.request.clone(), pending.job_id.clone()))
            else {
                continue;
            };
            if request.kind == "stop_job" {
                inner.pending_by_id.remove(&request_id);
                return Ok(Some(request));
            }
            if let Some(job_id) = job_id {
                if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                    if job.status == "queued" {
                        job.status = "running".to_string();
                        job.started_at = Some(now_ts());
                    }
                }
            }
            return Ok(Some(request));
        }
    }

    pub async fn complete(&self, body: ShellAgentResultRequest) -> Result<(), String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.request_id, "request_id")?;
        let mut inner = self.inner.lock().await;
        if let Some(client) = inner.clients.get_mut(&body.client_id) {
            client.last_seen = now_ts();
        }
        let Some(mut pending) = inner.pending_by_id.remove(&body.request_id) else {
            return Err(format!(
                "unknown or expired shell request: {}",
                body.request_id
            ));
        };
        if pending.request.client_id != body.client_id {
            return Err("request_id does not belong to client_id".to_string());
        }
        let request_id = body.request_id.clone();
        let client_id = body.client_id.clone();
        let error = body.error.clone();
        let stdout = truncate_output(body.stdout);
        let stderr = truncate_output(body.stderr);
        if let Some(job_id) = pending.job_id.clone() {
            inner.request_to_job.remove(&request_id);
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                job.status = if error.is_none() && body.exit_code == Some(0) {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                job.ended_at = Some(now_ts());
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.stdout = stdout.clone();
                job.stderr = stderr.clone();
                job.error = error.clone();
            }
        }
        let response = ShellRunResponse {
            success: error.is_none() && body.exit_code == Some(0),
            request_id,
            client_id,
            cwd: pending.request.cwd,
            command_preview: command_preview(&pending.request.command),
            exit_code: body.exit_code,
            stdout,
            stderr,
            duration_ms: body.duration_ms,
            error,
        };
        if let Some(waiter) = pending.waiter.take() {
            let _ = waiter.send(response);
        }
        Ok(())
    }

    pub async fn start_job(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        let client_id = body
            .client_id
            .clone()
            .ok_or_else(|| "client_id is required for op=start".to_string())?;
        let command = body
            .command
            .clone()
            .ok_or_else(|| "command is required for op=start".to_string())?;
        let run = ShellRunRequest {
            client_id: client_id.clone(),
            cwd: body.cwd.clone(),
            command: command.clone(),
            timeout_secs: body.timeout_secs.unwrap_or(120),
            wait_timeout_secs: 0,
        };
        validate_run_request(&run)?;
        let request_id = Uuid::new_v4().to_string();
        let job_id = Uuid::new_v4().to_string();
        let created_at = now_ts();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: "start_job".to_string(),
            job_id: Some(job_id.clone()),
            cwd: run.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            command,
            timeout_secs: run.timeout_secs,
            requested_by,
            created_at,
        };
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(&client_id) {
            return Err(format!("unknown shell client: {}", client_id));
        }
        inner
            .queues_by_client
            .entry(client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        let job = ShellJobRecord {
            job_id: job_id.clone(),
            request_id: Some(request_id.clone()),
            client_id: client_id.clone(),
            cwd: run.cwd.clone(),
            command_preview: command_preview(&run.command),
            status: "queued".to_string(),
            created_at,
            started_at: None,
            ended_at: None,
            exit_code: None,
            duration_ms: None,
            stdout: None,
            stderr: None,
            error: None,
        };
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: None,
                job_id: Some(job_id.clone()),
            },
        );
        inner.request_to_job.insert(request_id, job_id.clone());
        inner.jobs_by_id.insert(job_id.clone(), job);
        Ok(job_view(
            inner.jobs_by_id.get(&job_id).expect("job just inserted"),
        ))
    }

    pub async fn get_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        Ok(job_view(job))
    }

    pub async fn list_jobs(&self, limit: Option<usize>) -> Vec<ShellJobInfo> {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner.jobs_by_id.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs.into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect()
    }

    pub async fn job_log(
        &self,
        job_id: &str,
        since_stdout_line: Option<usize>,
        since_stderr_line: Option<usize>,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        let (stdout, next_stdout_line) =
            select_lines(job.stdout.as_ref(), since_stdout_line, tail_lines);
        let (stderr, next_stderr_line) =
            select_lines(job.stderr.as_ref(), since_stderr_line, tail_lines);
        Ok((
            job_view(job),
            stdout,
            stderr,
            next_stdout_line,
            next_stderr_line,
        ))
    }

    pub async fn stop_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.get(job_id).cloned() else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        match job.status.as_str() {
            "queued" => {
                if let Some(request_id) = &job.request_id {
                    inner.pending_by_id.remove(request_id);
                    inner.request_to_job.remove(request_id);
                    for queue in inner.queues_by_client.values_mut() {
                        queue.retain(|id| id != request_id);
                    }
                }
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stopped".to_string();
                job.ended_at = Some(now_ts());
                job.error = Some("job stopped before agent picked it up".to_string());
                Ok(job_view(job))
            }
            "running" | "stop_requested" => {
                let stop_request_id = Uuid::new_v4().to_string();
                let client_id = job.client_id.clone();
                let request = ShellAgentShellRequest {
                    request_id: stop_request_id.clone(),
                    client_id: client_id.clone(),
                    kind: "stop_job".to_string(),
                    job_id: Some(job_id.to_string()),
                    cwd: None,
                    path: None,
                    content: None,
                    max_bytes: None,
                    command: String::new(),
                    timeout_secs: 1,
                    requested_by: "gpt_action_or_web".to_string(),
                    created_at: now_ts(),
                };
                inner
                    .queues_by_client
                    .entry(client_id)
                    .or_default()
                    .push_back(stop_request_id.clone());
                inner.pending_by_id.insert(
                    stop_request_id,
                    PendingShellRequest {
                        request,
                        waiter: None,
                        job_id: Some(job_id.to_string()),
                    },
                );
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stop_requested".to_string();
                job.error = Some("stop requested".to_string());
                Ok(job_view(job))
            }
            _ => Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists"))),
        }
    }

    pub async fn update_job(
        &self,
        body: ShellAgentJobUpdateRequest,
    ) -> Result<ShellJobInfo, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        if let Some(client) = inner.clients.get_mut(&body.client_id) {
            client.last_seen = now_ts();
        }
        let mut request_id_to_remove = None;
        let view = {
            let Some(job) = inner.jobs_by_id.get_mut(&body.job_id) else {
                return Err(format!("unknown shell job: {}", body.job_id));
            };
            if job.client_id != body.client_id {
                return Err("job_id does not belong to client_id".to_string());
            }
            append_limited(&mut job.stdout, body.stdout_chunk);
            append_limited(&mut job.stderr, body.stderr_chunk);
            if job.started_at.is_none()
                && matches!(
                    body.status.as_str(),
                    "running" | "completed" | "failed" | "stopped" | "timeout"
                )
            {
                job.started_at = Some(now_ts());
            }
            if !body.status.trim().is_empty() && !is_final_job_status(&job.status) {
                job.status = body.status.trim().to_string();
            }
            if is_final_job_status(&body.status) {
                job.status = body.status;
                job.ended_at = Some(now_ts());
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.error = body.error;
                request_id_to_remove = job.request_id.clone();
            } else if body.error.is_some() {
                job.error = body.error;
            }
            job_view(job)
        };
        if let Some(request_id) = request_id_to_remove {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
        }
        Ok(view)
    }

    fn client_view_locked(
        inner: &ShellClientRegistryInner,
        client_id: &str,
    ) -> Option<ShellClientView> {
        let client = inner.clients.get(client_id)?;
        let pending_requests = inner
            .queues_by_client
            .get(client_id)
            .map(VecDeque::len)
            .unwrap_or(0);
        let age = now_ts().saturating_sub(client.last_seen);
        let connected = age <= CLIENT_ONLINE_WINDOW_SECS;
        Some(ShellClientView {
            client_id: client.client_id.clone(),
            display_name: client.display_name.clone(),
            owner: client.owner.clone(),
            hostname: client.hostname.clone(),
            status: if connected { "online" } else { "stale" }.to_string(),
            connected,
            last_seen: client.last_seen,
            capabilities: client.capabilities.clone(),
            pending_requests,
        })
    }
}

fn get_registry(depot: &Depot) -> Option<Arc<ShellClientRegistry>> {
    depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned()
}

fn registry_error() -> Json<ShellClientsResponse> {
    Json(ShellClientsResponse {
        success: false,
        clients: Vec::new(),
        error: Some("Shell client registry not configured".to_string()),
    })
}

#[handler]
pub async fn shell_clients(depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(registry_error());
        return;
    };
    res.render(Json(ShellClientsResponse {
        success: true,
        clients: registry.list_clients().await,
        error: None,
    }));
}

#[handler]
pub async fn shell_agent_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellClientRegisterRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.register(body).await {
        Ok(client) => res.render(Json(ShellClientRegisterResponse {
            success: true,
            client: Some(client),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_poll(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentPollRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.poll(body).await {
        Ok(request) => res.render(Json(ShellAgentPollResponse {
            success: true,
            request,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_result(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentResultRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.complete(body).await {
        Ok(()) => res.render(Json(ShellAgentResultResponse {
            success: true,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellRunResponse {
            success: false,
            request_id: String::new(),
            client_id: String::new(),
            cwd: None,
            command_preview: String::new(),
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellRunRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id: String::new(),
                cwd: None,
                command_preview: String::new(),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let wait_timeout_secs = body.wait_timeout_secs;
    let client_id = body.client_id.clone();
    let cwd = body.cwd.clone();
    let preview = command_preview(&body.command);
    let (request_id, rx) = match registry
        .enqueue_run(body, "gpt_action_or_web".to_string())
        .await
    {
        Ok(result) => result,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(e),
            }));
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => res.render(Json(response)),
        Ok(Err(_closed)) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id,
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some("shell request waiter was dropped".to_string()),
            }));
        }
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            res.status_code(StatusCode::REQUEST_TIMEOUT);
            res.render(Json(ShellRunResponse {
                success: false,
                request_id,
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(format!(
                    "timed out waiting {} seconds for shell client result",
                    wait_timeout_secs
                )),
            }));
        }
    }
}

fn shell_file_response_from_run(
    op: String,
    path: String,
    cwd: Option<String>,
    response: ShellRunResponse,
) -> ShellFileOpResponse {
    let stdout = response.stdout.unwrap_or_default();
    let entries = if op == "list" && response.error.is_none() {
        stdout.lines().map(|line| line.to_string()).collect()
    } else {
        Vec::new()
    };
    let content = if op == "read" && response.error.is_none() {
        Some(stdout.clone())
    } else {
        None
    };
    let bytes = match op.as_str() {
        "read" => content.as_ref().map(|s| s.len()),
        "write" => Some(stdout.trim().parse::<usize>().unwrap_or(0)),
        _ => None,
    };
    ShellFileOpResponse {
        success: response.error.is_none() && response.exit_code == Some(0),
        op,
        request_id: response.request_id,
        client_id: response.client_id,
        path,
        cwd,
        content,
        entries,
        bytes,
        stderr: response.stderr,
        error: response.error,
    }
}

fn shell_file_error(
    op: String,
    client_id: String,
    path: String,
    cwd: Option<String>,
    error: String,
) -> Json<ShellFileOpResponse> {
    Json(ShellFileOpResponse {
        success: false,
        op,
        request_id: String::new(),
        client_id,
        path,
        cwd,
        content: None,
        entries: Vec::new(),
        bytes: None,
        stderr: None,
        error: Some(error),
    })
}

#[handler]
pub async fn shell_file_op(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(shell_file_error(
            "unknown".to_string(),
            String::new(),
            String::new(),
            None,
            "Shell client registry not configured".to_string(),
        ));
        return;
    };
    let body: ShellFileOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(shell_file_error(
                "unknown".to_string(),
                String::new(),
                String::new(),
                None,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let op = body.op.clone();
    let client_id = body.client_id.clone();
    let path = body.path.clone();
    let cwd = body.cwd.clone();
    let wait_timeout_secs = body.wait_timeout_secs;
    let (request_id, rx) = match registry
        .enqueue_file_op(body, "gpt_action_or_web".to_string())
        .await
    {
        Ok(result) => result,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(shell_file_error(op, client_id, path, cwd, e));
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => res.render(Json(shell_file_response_from_run(op, path, cwd, response))),
        Ok(Err(_closed)) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(shell_file_error(
                op,
                client_id,
                path,
                cwd,
                "shell file request waiter was dropped".to_string(),
            ));
        }
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            res.status_code(StatusCode::REQUEST_TIMEOUT);
            res.render(shell_file_error(
                op,
                client_id,
                path,
                cwd,
                format!(
                    "timed out waiting {} seconds for shell file result",
                    wait_timeout_secs
                ),
            ));
        }
    }
}

fn shell_job_error(op: String, error: String) -> Json<ShellJobOpResponse> {
    Json(ShellJobOpResponse {
        success: false,
        op,
        job: None,
        jobs: Vec::new(),
        stdout: None,
        stderr: None,
        next_stdout_line: None,
        next_stderr_line: None,
        error: Some(error),
    })
}

#[handler]
pub async fn shell_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(shell_job_error(
            "unknown".to_string(),
            "Shell client registry not configured".to_string(),
        ));
        return;
    };
    let body: ShellJobOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(shell_job_error(
                "unknown".to_string(),
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let op = body.op.clone();
    match op.as_str() {
        "start" => match registry
            .start_job(body, "gpt_action_or_web".to_string())
            .await
        {
            Ok(job) => res.render(Json(ShellJobOpResponse {
                success: true,
                op,
                job: Some(job),
                jobs: Vec::new(),
                stdout: None,
                stderr: None,
                next_stdout_line: None,
                next_stderr_line: None,
                error: None,
            })),
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(shell_job_error(op, e));
            }
        },
        "status" => {
            let Some(job_id) = body.job_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(shell_job_error(
                    op,
                    "job_id is required for op=status".to_string(),
                ));
                return;
            };
            match registry.get_job(job_id).await {
                Ok(job) => res.render(Json(ShellJobOpResponse {
                    success: true,
                    op,
                    job: Some(job),
                    jobs: Vec::new(),
                    stdout: None,
                    stderr: None,
                    next_stdout_line: None,
                    next_stderr_line: None,
                    error: None,
                })),
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(shell_job_error(op, e));
                }
            }
        }
        "list" => {
            let jobs = registry.list_jobs(body.limit).await;
            res.render(Json(ShellJobOpResponse {
                success: true,
                op,
                job: None,
                jobs,
                stdout: None,
                stderr: None,
                next_stdout_line: None,
                next_stderr_line: None,
                error: None,
            }));
        }
        "log" => {
            let Some(job_id) = body.job_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(shell_job_error(
                    op,
                    "job_id is required for op=log".to_string(),
                ));
                return;
            };
            match registry
                .job_log(
                    job_id,
                    body.since_stdout_line,
                    body.since_stderr_line,
                    body.tail_lines,
                )
                .await
            {
                Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => {
                    res.render(Json(ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout,
                        stderr,
                        next_stdout_line: Some(next_stdout_line),
                        next_stderr_line: Some(next_stderr_line),
                        error: None,
                    }))
                }
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(shell_job_error(op, e));
                }
            }
        }
        "stop" => {
            let Some(job_id) = body.job_id.as_deref() else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(shell_job_error(
                    op,
                    "job_id is required for op=stop".to_string(),
                ));
                return;
            };
            match registry.stop_job(job_id).await {
                Ok(job) => res.render(Json(ShellJobOpResponse {
                    success: true,
                    op,
                    job: Some(job),
                    jobs: Vec::new(),
                    stdout: None,
                    stderr: None,
                    next_stdout_line: None,
                    next_stderr_line: None,
                    error: None,
                })),
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(shell_job_error(op, e));
                }
            }
        }
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(shell_job_error(
                op,
                "op must be one of start, status, log, stop, list".to_string(),
            ));
        }
    }
}

#[handler]
pub async fn shell_agent_job_update(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentJobUpdateRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    match registry.update_job(body).await {
        Ok(job) => res.render(Json(ShellAgentJobUpdateResponse {
            success: true,
            job: Some(job),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(e),
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_registers_and_lists_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                display_name: Some("XRH".to_string()),
                owner: Some("yyjeqhc".to_string()),
                hostname: Some("fineserver".to_string()),
                capabilities: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_id, "xrh");
        assert!(clients[0].connected);
        assert_eq!(clients[0].pending_requests, 0);
    }

    #[tokio::test]
    async fn registry_enqueues_polls_and_completes_shell_request() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
            })
            .await
            .unwrap();
        let (request_id, rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "xrh".to_string(),
                    cwd: Some("/tmp".to_string()),
                    command: "echo hello".to_string(),
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "xrh".to_string(),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.command, "echo hello");
        registry
            .complete(ShellAgentResultRequest {
                client_id: "xrh".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(12),
                error: None,
            })
            .await
            .unwrap();
        let response = rx.await.unwrap();
        assert!(response.success);
        assert_eq!(response.stdout.as_deref(), Some("hello\n"));
    }

    #[tokio::test]
    async fn registry_rejects_unknown_client_run() {
        let registry = ShellClientRegistry::default();
        let err = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "missing".to_string(),
                    cwd: None,
                    command: "pwd".to_string(),
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
    }

    #[tokio::test]
    async fn registry_shell_job_start_poll_complete_and_log() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: Some("/tmp".to_string()),
                    command: Some("printf hello".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(job.status, "queued");
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.command, "printf hello");
        let running = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(running.status, "running");
        registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                request_id: polled.request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(20),
                error: None,
            })
            .await
            .unwrap();
        let done = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(done.status, "completed");
        assert_eq!(done.exit_code, Some(0));
        let (_info, stdout, stderr, next_stdout, next_stderr) = registry
            .job_log(&job.job_id, Some(1), Some(1), None)
            .await
            .unwrap();
        assert_eq!(stdout.as_deref(), Some("hello\n"));
        assert_eq!(stderr.as_deref(), Some(""));
        assert_eq!(next_stdout, 2);
        assert_eq!(next_stderr, 1);
    }

    #[tokio::test]
    async fn registry_shell_job_stop_cancels_queued_job() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let stopped = registry.stop_job(&job.job_id).await.unwrap();
        assert_eq!(stopped.status, "stopped");
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
            })
            .await
            .unwrap();
        assert!(polled.is_none());
    }

    #[tokio::test]
    async fn registry_marks_running_job_lost_when_client_stale() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
            })
            .await
            .unwrap()
            .unwrap();
        {
            let mut inner = registry.inner.lock().await;
            let client = inner.clients.get_mut("oe").unwrap();
            client.last_seen = now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1;
        }
        let lost = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(lost.status, "lost");
        assert!(lost.error.unwrap().contains("stale"));
    }
}
