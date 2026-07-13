use super::state::{ShellClientRegistryInner, ShellJobRecord};
use super::{now_ts, CLIENT_ONLINE_WINDOW_SECS, MAX_OUTPUT_BYTES, MAX_QUEUED_REQUESTS_PER_CLIENT};
use crate::shell_protocol::{ShellAgentJobResult, ShellAgentShellJobResult, ShellJobInfo};
use std::collections::VecDeque;

pub(crate) const COMMAND_PREVIEW_MAX_CHARS: usize = 120;

pub(crate) fn command_preview(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    if first_line.chars().count() <= COMMAND_PREVIEW_MAX_CHARS {
        first_line.to_string()
    } else {
        let preview = first_line
            .chars()
            .take(COMMAND_PREVIEW_MAX_CHARS)
            .collect::<String>();
        format!("{}…", preview)
    }
}

pub(super) fn truncate_output(value: Option<String>) -> Option<String> {
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

pub(super) fn job_view(job: &ShellJobRecord) -> ShellJobInfo {
    let now = now_ts();
    let elapsed_secs = if let Some(duration_ms) = job.duration_ms {
        Some(duration_ms / 1000)
    } else {
        job.started_at
            .map(|started_at| job.ended_at.unwrap_or(now).saturating_sub(started_at) as u64)
    };
    let result = if is_final_job_status(&job.status) {
        Some(ShellAgentJobResult {
            shell: Some(ShellAgentShellJobResult {
                cwd: job.cwd.clone(),
                command_preview: job.command_preview.clone(),
                exit_code: job.exit_code,
                duration_ms: job.duration_ms,
                error: job.error.clone(),
            }),
        })
    } else {
        None
    };
    ShellJobInfo {
        job_id: job.job_id.clone(),
        request_id: job.request_id.clone(),
        client_id: job.client_id.clone(),
        kind: job.kind.clone(),
        project_id: job.project_id.clone(),
        session_id: job.session_id.clone(),
        cwd: job.cwd.clone(),
        command_preview: job.command_preview.clone(),
        status: job.status.clone(),
        created_at: job.created_at,
        started_at: job.started_at,
        ended_at: job.ended_at,
        exit_code: job.exit_code,
        duration_ms: job.duration_ms,
        elapsed_secs,
        error: job.error.clone(),
        codex: job.codex.clone(),
        result,
    }
}

pub(super) fn select_lines(
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

pub(super) fn append_limited(target: &mut Option<String>, chunk: Option<String>) {
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

pub(super) fn replace_limited(target: &mut Option<String>, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    *target = truncate_output(Some(value));
}

pub(super) fn is_final_job_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "stopped" | "timeout" | "timed_out" | "lost" | "cancelled"
    )
}

fn client_is_connected_locked(inner: &ShellClientRegistryInner, client_id: &str) -> bool {
    inner
        .clients
        .get(client_id)
        .map(|client| now_ts().saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS)
        .unwrap_or(false)
}

pub(super) fn offline_last_seen(now: i64) -> i64 {
    now.saturating_sub(CLIENT_ONLINE_WINDOW_SECS.saturating_add(1))
}

/// Verify that `client_id` exists and that `agent_instance_id` matches the
/// instance that currently holds the lease for it. A stale/replaced instance
/// (e.g. a second process that was rejected, or the previous process after a
/// stale replacement) is rejected so it can no longer poll or submit results.
/// Callers must already hold `inner`.
pub(super) fn assert_active_instance_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
    agent_instance_id: &str,
) -> Result<(), String> {
    let Some(client) = inner.clients.get(client_id) else {
        return Err(format!("unknown shell client: {}", client_id));
    };
    if client.agent_instance_id != agent_instance_id {
        return Err(format!(
            "agent client {} is no longer the active instance (stale or replaced)",
            client_id
        ));
    }
    Ok(())
}

/// Reject enqueue when a client's pending queue has reached
/// `MAX_QUEUED_REQUESTS_PER_CLIENT`. Callers must already hold `inner`.
pub(super) fn ensure_queue_capacity_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
) -> Result<(), String> {
    let len = inner
        .queues_by_client
        .get(client_id)
        .map(VecDeque::len)
        .unwrap_or(0);
    if len >= MAX_QUEUED_REQUESTS_PER_CLIENT {
        return Err(format!(
            "too many pending requests for shell client {} (limit {})",
            client_id, MAX_QUEUED_REQUESTS_PER_CLIENT
        ));
    }
    Ok(())
}

/// Ensure a request target exists and is currently online before enqueueing
/// work for the agent pump. Callers must already hold `inner`.
///
/// Online is defined by `CLIENT_ONLINE_WINDOW_SECS` against `last_seen`. Without
/// this gate, a registered-but-disconnected agent still accepts enqueues that
/// can only fail after the caller's wait timeout (or pile up until
/// `MAX_QUEUED_REQUESTS_PER_CLIENT` and then permanently reject new work for
/// that client until process restart) — a major amplifier of MCP "no reply".
pub(super) fn ensure_dispatch_supported_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
) -> Result<(), String> {
    if !inner.clients.contains_key(client_id) {
        return Err(format!("unknown shell client: {}", client_id));
    }
    if !client_is_connected_locked(inner, client_id) {
        return Err(format!(
            "shell client {} is offline (no keepalive within {}s); reconnect the agent before retrying",
            client_id, CLIENT_ONLINE_WINDOW_SECS
        ));
    }
    Ok(())
}

pub(super) fn refresh_job_status_locked(inner: &mut ShellClientRegistryInner, job_id: &str) {
    let Some(job) = inner.jobs_by_id.get(job_id) else {
        return;
    };
    if is_final_job_status(&job.status)
        || !matches!(
            job.status.as_str(),
            "agent_queued" | "running" | "stop_requested"
        )
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
