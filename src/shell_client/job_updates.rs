use super::auth::shell_job_visible_to_auth;
use super::jobs::{
    append_limited, assert_active_instance_locked, command_preview, is_final_job_status, job_view,
    refresh_job_status_locked, replace_limited, select_lines,
};
use super::requests::{
    enqueue_pending_request_locked, next_request_id, notify_client_locked,
    remove_pending_request_locked,
};
use super::state::ShellJobRecord;
use super::validation::{validate_agent_instance_id, validate_id, validate_run_request};
use super::{now_ts, ShellClientRegistry};
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentShellRequest, ShellJobInfo, ShellJobOpRequest,
    ShellRunRequest,
};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub(crate) struct ShellJobStartMetadata {
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
}

impl ShellClientRegistry {
    pub async fn start_job(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        self.start_job_with_metadata(body, requested_by, ShellJobStartMetadata::default())
            .await
    }

    pub(crate) async fn start_job_with_metadata(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
        metadata: ShellJobStartMetadata,
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
            stdin: None,
            timeout_secs: body.timeout_secs.unwrap_or(120),
            wait_timeout_secs: 0,
        };
        validate_run_request(&run)?;
        let request_id = next_request_id();
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
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command,
            stdin: None,
            timeout_secs: run.timeout_secs,
            requested_by,
            created_at,
            validation: None,
            lsp: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if !(client.capabilities.async_jobs || client.capabilities.async_shell_jobs) {
            return Err(format!(
                "agent client {} does not support async shell jobs",
                client_id
            ));
        }
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            None,
            Some(job_id.clone()),
        )?;
        let job = ShellJobRecord {
            job_id: job_id.clone(),
            request_id: Some(request_id.clone()),
            client_id: client_id.clone(),
            kind: "shell".to_string(),
            project_id: metadata.project_id,
            session_id: metadata.session_id,
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
            codex: body.codex.clone(),
        };
        inner.request_to_job.insert(request_id, job_id.clone());
        inner.jobs_by_id.insert(job_id.clone(), job);
        notify_client_locked(&inner, &client_id);
        Ok(job_view(
            inner.jobs_by_id.get(&job_id).expect("job just inserted"),
        ))
    }

    pub async fn get_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        self.get_job_for_auth(None, job_id).await
    }

    pub(crate) async fn get_job_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        job_id: &str,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        if !shell_job_visible_to_auth(auth, &inner, &job.client_id) {
            return Err(format!("unknown shell job: {}", job_id));
        }
        Ok(job_view(job))
    }

    pub async fn list_jobs(&self, limit: Option<usize>) -> Vec<ShellJobInfo> {
        self.list_jobs_for_auth(None, limit).await
    }

    pub(crate) async fn list_jobs_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        limit: Option<usize>,
    ) -> Vec<ShellJobInfo> {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| shell_job_visible_to_auth(auth, &inner, &job.client_id))
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs.into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect()
    }

    pub async fn list_jobs_for_client(
        &self,
        client_id: &str,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ShellJobInfo>, String> {
        validate_id(client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(client_id) {
            return Err(format!("unknown shell client: {}", client_id));
        }
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| job.client_id == client_id)
            .filter(|job| status.map(|status| status == job.status).unwrap_or(true))
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(jobs
            .into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect())
    }

    pub async fn job_log(
        &self,
        job_id: &str,
        since_stdout_line: Option<usize>,
        since_stderr_line: Option<usize>,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
        self.job_log_for_auth(
            None,
            job_id,
            since_stdout_line,
            since_stderr_line,
            tail_lines,
        )
        .await
    }

    pub(crate) async fn job_log_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
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
        if !shell_job_visible_to_auth(auth, &inner, &job.client_id) {
            return Err(format!("unknown shell job: {}", job_id));
        }
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

    pub async fn stop_job(
        &self,
        job_id: &str,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.get(job_id).cloned() else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        match job.status.as_str() {
            "queued" => {
                if let Some(request_id) = &job.request_id {
                    remove_pending_request_locked(&mut inner, request_id);
                    inner.request_to_job.remove(request_id);
                }
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stopped".to_string();
                job.ended_at = Some(now_ts());
                job.error = Some("job stopped before agent picked it up".to_string());
                Ok(job_view(job))
            }
            "agent_queued" | "running" | "stop_requested" => {
                let stop_request_id = next_request_id();
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
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    command: String::new(),
                    stdin: None,
                    timeout_secs: 1,
                    requested_by,
                    created_at: now_ts(),
                    validation: None,
                    lsp: None,
                };
                enqueue_pending_request_locked(
                    &mut inner,
                    &client_id,
                    stop_request_id,
                    request,
                    None,
                    Some(job_id.to_string()),
                )?;
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stop_requested".to_string();
                job.error = Some("stop requested".to_string());
                let notify_client_id = job.client_id.clone();
                notify_client_locked(&inner, &notify_client_id);
                Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists")))
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
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        // Reject job updates from a stale/replaced instance before refreshing
        // liveness or mutating job state.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
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
            if is_final_job_status(&job.status) {
                return Ok(job_view(job));
            }
            replace_limited(&mut job.stdout, body.stdout_tail);
            replace_limited(&mut job.stderr, body.stderr_tail);
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
                let incoming_status = body.status.trim();
                job.status = if incoming_status == "queued" && job.started_at.is_some() {
                    "agent_queued".to_string()
                } else {
                    incoming_status.to_string()
                };
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
            if body.finished && !is_final_job_status(&job.status) {
                job.status = if job.error.is_none() && job.exit_code == Some(0) {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                job.ended_at = Some(now_ts());
                request_id_to_remove = job.request_id.clone();
            }
            job_view(job)
        };
        if let Some(request_id) = request_id_to_remove {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
        }
        Ok(view)
    }
}
