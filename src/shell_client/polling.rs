use super::jobs::{assert_active_instance_locked, command_preview, truncate_output};
use super::validation::{normalize_project_summaries, validate_agent_instance_id, validate_id};
use super::{now_ts, ShellClientRegistry};
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellAgentShellRequest, ShellRunResponse,
};

impl ShellClientRegistry {
    pub async fn poll(
        &self,
        body: ShellAgentPollRequest,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        {
            let Some(client) = inner.clients.get_mut(&body.client_id) else {
                return Err(format!("unknown shell client: {}", body.client_id));
            };
            if client.agent_instance_id != body.agent_instance_id {
                return Err(format!(
                    "agent client {} is no longer the active instance (stale or replaced)",
                    body.client_id
                ));
            }
            if body.projects.is_some() {
                client.projects = normalize_project_summaries(body.projects);
            }
            client.last_seen = now_ts();
        }
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
                        job.status = "agent_queued".to_string();
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
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        // Reject results from a stale/replaced instance before refreshing
        // liveness: a dead process must not update the active lease's
        // `last_seen` or resolve its waiters.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
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
}
