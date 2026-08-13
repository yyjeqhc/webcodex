use super::jobs::{
    assert_active_instance_locked, observe_job_terminal, replace_log_limited, request_preview,
    truncate_output, truncate_output_to,
};
use super::requests::{remove_pending_request_locked, take_pending_request_locked};
use super::validation::{
    normalize_project_summaries, validate_agent_instance_id, validate_id,
    validate_project_summary_count,
};
use super::{now_ts, ShellClientRegistry};
use crate::shell_protocol::{
    ShellAgentPersistentShellResultRequest, ShellAgentPollRequest, ShellAgentResultPayload,
    ShellAgentResultRequest, ShellAgentShellRequest, ShellCommandExecutionState, ShellRunResponse,
};

impl ShellClientRegistry {
    /// Polling-transport entry point. Polling registrations do not carry a
    /// server-internal `connection_id`, so this requires only the public
    /// `client_id` / `agent_instance_id` lease. The HTTP `/poll` handler uses
    /// this path; long-lived transports (WebSocket/QUIC) use
    /// [`ShellClientRegistry::poll_for_connection`] instead so an older
    /// same-instance connection cannot steal requests from the current lease.
    pub async fn poll(
        &self,
        body: ShellAgentPollRequest,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        self.poll_checked(body, None).await
    }

    /// Connection-scoped entry point for long-lived transports. The pump for a
    /// concrete WebSocket/QUIC connection passes its captured `connection_id`
    /// so a stale same-instance connection (whose lease was replaced by a
    /// reconnect) is rejected before it can dequeue a request that belongs to
    /// the new connection.
    pub(crate) async fn poll_for_connection(
        &self,
        body: ShellAgentPollRequest,
        connection_id: &str,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        self.poll_checked(body, Some(connection_id)).await
    }

    /// Unified dequeue path. Every check below — `client_id`,
    /// `agent_instance_id`, and (when provided) the current `connection_id`
    /// — runs while holding the same registry mutex as the queue mutation,
    /// `last_seen` update, project projection and `dispatched`/job-state
    /// transitions. A stale connection returns a stable error with no side
    /// effects: no `last_seen` refresh, no project update, no `pop_front()`,
    /// no `dispatched=true`, and no job-state change.
    async fn poll_checked(
        &self,
        body: ShellAgentPollRequest,
        expected_connection_id: Option<&str>,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        validate_project_summary_count(body.projects.as_deref())?;
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
            if let Some(expected) = expected_connection_id {
                if client.connection_id.as_deref() != Some(expected) {
                    return Err(format!(
                        "agent client {} transport connection is no longer active",
                        body.client_id
                    ));
                }
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
            let Some((request, job_id)) = inner.pending_by_id.get_mut(&request_id).map(|pending| {
                pending.dispatched = true;
                (pending.request.clone(), pending.job_id.clone())
            }) else {
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
                        // Dispatch proves only that the Runner accepted the
                        // Job request. A typed structured Job becomes started
                        // only when the Runner reports `running` after a
                        // successful child spawn.
                        if job.structured_execution.is_none() {
                            job.started_at = Some(now_ts());
                        }
                        super::jobs::notify_job_update(job);
                    }
                }
            }
            return Ok(Some(request));
        }
    }

    /// Polling-transport result entry point. Requires the public
    /// `client_id` / `agent_instance_id` lease and refreshes `last_seen` for
    /// the active instance. Used by the HTTP `/result` handler.
    pub async fn complete(
        &self,
        payload: impl Into<ShellAgentResultPayload>,
    ) -> Result<(), String> {
        let payload = payload.into();
        self.complete_checked(payload.result, payload.command_execution_state, None)
            .await
    }

    /// Connection-scoped result entry point for long-lived transports. A
    /// late, legitimately-dispatched result arriving on a stale
    /// same-instance connection (the request was polled before the
    /// transport reconnect) is still accepted — it belongs to the same
    /// agent instance and is gated by request/job ownership — but it must
    /// not refresh the new connection's `last_seen` liveness. Only the
    /// connection that currently holds the lease refreshes liveness.
    pub(crate) async fn complete_for_connection(
        &self,
        payload: ShellAgentResultPayload,
        connection_id: &str,
    ) -> Result<(), String> {
        self.complete_checked(
            payload.result,
            payload.command_execution_state,
            Some(connection_id),
        )
        .await
    }

    async fn complete_checked(
        &self,
        body: ShellAgentResultRequest,
        command_execution_state: Option<ShellCommandExecutionState>,
        expected_connection_id: Option<&str>,
    ) -> Result<(), String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.request_id, "request_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        // Reject results from a stale/replaced instance before refreshing
        // liveness: a dead process must not update the active lease's
        // `last_seen` or resolve its waiters.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
        // Refresh liveness only for the connection that currently holds the
        // transport lease. A late result on a stale same-instance connection
        // is still processed below, but it must not make the new connection
        // appear online.
        if expected_connection_id.is_none()
            || inner
                .clients
                .get(&body.client_id)
                .is_some_and(|client| client.connection_id.as_deref() == expected_connection_id)
        {
            if let Some(client) = inner.clients.get_mut(&body.client_id) {
                client.last_seen = now_ts();
            }
        }
        let Some(mut pending) = take_pending_request_locked(&mut inner, &body.request_id) else {
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
        let stdout = if is_large_native_image_request(&pending.request) {
            truncate_output_to(
                body.stdout,
                crate::artifact_policy::MAX_MCP_IMAGE_RESPONSE_BYTES,
            )
        } else {
            truncate_output(body.stdout)
        };
        let stderr = truncate_output(body.stderr);
        let success = matches!(
            command_execution_state,
            None | Some(ShellCommandExecutionState::Completed)
        ) && error.is_none()
            && body.exit_code == Some(0);
        if let Some(job_id) = pending.job_id.clone() {
            inner.request_to_job.remove(&request_id);
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                let terminal_now = now_ts();
                job.status = if success {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                observe_job_terminal(job, terminal_now);
                job.ended_at = Some(terminal_now);
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                replace_log_limited(&mut job.stdout, stdout.clone());
                replace_log_limited(&mut job.stderr, stderr.clone());
                job.error = error.clone();
                super::jobs::notify_job_update(job);
            }
        }
        let request_preview = request_preview(&pending.request);
        let response = ShellRunResponse {
            success,
            request_id,
            client_id,
            cwd: pending.request.cwd,
            command_preview: request_preview,
            exit_code: body.exit_code,
            stdout,
            stderr,
            duration_ms: body.duration_ms,
            error,
            request_dispatched: Some(pending.dispatched),
            command_execution_state,
        };
        if let Some(waiter) = pending.waiter.take() {
            let _ = waiter.send(response);
        }
        Ok(())
    }

    pub async fn complete_persistent_shell(
        &self,
        body: ShellAgentPersistentShellResultRequest,
    ) -> Result<(), String> {
        self.complete_persistent_shell_checked(body, None).await
    }

    pub(crate) async fn complete_persistent_shell_for_connection(
        &self,
        body: ShellAgentPersistentShellResultRequest,
        connection_id: &str,
    ) -> Result<(), String> {
        self.complete_persistent_shell_checked(body, Some(connection_id))
            .await
    }

    async fn complete_persistent_shell_checked(
        &self,
        mut body: ShellAgentPersistentShellResultRequest,
        expected_connection_id: Option<&str>,
    ) -> Result<(), String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.request_id, "request_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        normalize_persistent_shell_result(&mut body.result)?;
        let mut inner = self.inner.lock().await;
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
        if expected_connection_id.is_none()
            || inner
                .clients
                .get(&body.client_id)
                .is_some_and(|client| client.connection_id.as_deref() == expected_connection_id)
        {
            if let Some(client) = inner.clients.get_mut(&body.client_id) {
                client.last_seen = now_ts();
            }
        }
        let Some(pending) = inner.pending_by_id.get(&body.request_id) else {
            return Err(format!(
                "unknown or expired persistent shell request: {}",
                body.request_id
            ));
        };
        if pending.request.client_id != body.client_id {
            return Err("request_id does not belong to client_id".to_string());
        }
        let expected = pending.request.persistent_shell.as_ref().ok_or_else(|| {
            "request_id does not belong to a persistent shell request".to_string()
        })?;
        if expected.shell_id != body.result.shell_id
            || expected.workflow_session_id != body.result.workflow_session_id
            || expected.runtime_project_id != body.result.runtime_project_id
        {
            remove_pending_request_locked(&mut inner, &body.request_id);
            inner.persistent_waiters.remove(&body.request_id);
            return Err("persistent shell result identity mismatch".to_string());
        }
        take_pending_request_locked(&mut inner, &body.request_id)
            .expect("persistent shell request remained present after identity validation");
        let waiter = inner.persistent_waiters.remove(&body.request_id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(body.result);
        }
        Ok(())
    }
}

fn normalize_persistent_shell_result(
    result: &mut crate::shell_protocol::PersistentShellResult,
) -> Result<(), String> {
    validate_id(&result.shell_id, "shell_id")?;
    validate_id(&result.workflow_session_id, "workflow_session_id")?;
    if result.runtime_project_id.is_empty()
        || result.runtime_project_id.len() > 1024
        || result.runtime_project_id.chars().any(char::is_control)
    {
        return Err("invalid runtime_project_id in persistent shell result".to_string());
    }
    if !matches!(
        result.shell_state.as_str(),
        "opening" | "running" | "exited" | "closed" | "poisoned" | "lost" | "unknown"
    ) {
        return Err("invalid shell_state in persistent shell result".to_string());
    }
    const MAX_METADATA_BYTES: usize = 8 * 1024;
    for (name, value) in [
        ("execution_state", Some(&result.execution_state)),
        ("cwd", result.cwd.as_ref()),
        ("initial_cwd", result.initial_cwd.as_ref()),
        ("shell", result.shell.as_ref()),
        ("profile", result.profile.as_ref()),
        ("close_reason", result.close_reason.as_ref()),
        ("error_code", result.error_code.as_ref()),
        ("error", result.error.as_ref()),
    ] {
        if value.is_some_and(|value| value.len() > MAX_METADATA_BYTES) {
            return Err(format!(
                "{name} exceeds persistent shell result metadata limit"
            ));
        }
    }
    if truncate_persistent_shell_stream(&mut result.stdout) {
        result.stdout_truncated = true;
    }
    if truncate_persistent_shell_stream(&mut result.stderr) {
        result.stderr_truncated = true;
    }
    Ok(())
}

fn truncate_persistent_shell_stream(value: &mut String) -> bool {
    if value.len() <= super::MAX_OUTPUT_BYTES {
        return false;
    }
    let mut start = value.len() - super::MAX_OUTPUT_BYTES;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    *value = value[start..].to_string();
    true
}

fn is_large_native_image_request(request: &ShellAgentShellRequest) -> bool {
    if request.kind == "computer_snapshot" {
        return true;
    }
    request.kind == "file_read_project_artifact"
        && request
            .content
            .as_deref()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(content).ok())
            .and_then(|payload| payload.get("mcp_image").and_then(|value| value.as_bool()))
            .unwrap_or(false)
}
