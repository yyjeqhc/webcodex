use super::jobs::{
    command_preview, ensure_dispatch_supported_locked, ensure_queue_capacity_locked,
};
use super::state::{PendingShellRequest, ShellClientRegistryInner};
use super::validation::{validate_file_request, validate_id, validate_run_request};
use super::{now_ts, ShellClientRegistry};
use crate::lsp_bridge::{AgentLspPayload, AGENT_LSP_REQUEST_KIND};
use crate::shell_protocol::{
    ShellAgentShellRequest, ShellFileOpRequest, ShellRunRequest, ShellRunResponse,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
};
use tokio::sync::oneshot;
use uuid::Uuid;

pub(super) fn next_request_id() -> String {
    Uuid::new_v4().to_string()
}

pub(super) fn notify_client_locked(inner: &ShellClientRegistryInner, client_id: &str) {
    if let Some(entry) = inner.notifiers.get(client_id) {
        entry.notify.notify_one();
    }
}

pub(super) fn enqueue_pending_request_locked(
    inner: &mut ShellClientRegistryInner,
    client_id: &str,
    request_id: String,
    request: ShellAgentShellRequest,
    waiter: Option<oneshot::Sender<ShellRunResponse>>,
    job_id: Option<String>,
) -> Result<(), String> {
    ensure_dispatch_supported_locked(inner, client_id)?;
    ensure_queue_capacity_locked(inner, client_id)?;
    inner
        .queues_by_client
        .entry(client_id.to_string())
        .or_default()
        .push_back(request_id.clone());
    inner.pending_by_id.insert(
        request_id,
        PendingShellRequest {
            request,
            waiter,
            job_id,
        },
    );
    Ok(())
}

pub(super) fn take_pending_request_locked(
    inner: &mut ShellClientRegistryInner,
    request_id: &str,
) -> Option<PendingShellRequest> {
    inner.pending_by_id.remove(request_id)
}

pub(super) fn remove_pending_request_locked(
    inner: &mut ShellClientRegistryInner,
    request_id: &str,
) -> Option<PendingShellRequest> {
    let pending = take_pending_request_locked(inner, request_id);
    remove_request_from_queues_locked(inner, request_id);
    pending
}

fn remove_request_from_queues_locked(inner: &mut ShellClientRegistryInner, request_id: &str) {
    for queue in inner.queues_by_client.values_mut() {
        queue.retain(|id| id != request_id);
    }
}

/// Resolve every in-flight *synchronous* tool request (no `job_id`) owned by
/// `client_id` with a disconnect error, instead of leaving its oneshot waiter
/// parked until the calling tool's own timeout fires.
///
/// Sync requests (`enqueue_run`/`enqueue_file_op`/`enqueue_project_op`/
/// `enqueue_lsp`) carry a live `waiter` but `job_id: None`. The oneshot
/// `Sender` lives in the shared registry, not in the transport handler, so
/// aborting the connection task does not drop it — without this drain the
/// caller (e.g. an MCP `run_shell`/`read_file`) blocks for the full wait
/// timeout (tens of seconds) after the agent goes away. Job-backed requests
/// are handled separately by `reconcile_disconnect` (they transition their job
/// to `lost`) and are intentionally skipped here.
pub(super) fn resolve_disconnected_sync_requests_locked(
    inner: &mut ShellClientRegistryInner,
    client_id: &str,
    error: &str,
) {
    let request_ids: Vec<String> = inner
        .pending_by_id
        .iter()
        .filter(|(_, pending)| pending.job_id.is_none() && pending.request.client_id == client_id)
        .map(|(request_id, _)| request_id.clone())
        .collect();
    for request_id in request_ids {
        let Some(mut pending) = inner.pending_by_id.remove(&request_id) else {
            continue;
        };
        if let Some(queue) = inner.queues_by_client.get_mut(client_id) {
            queue.retain(|id| id != &request_id);
        }
        if let Some(waiter) = pending.waiter.take() {
            let response = ShellRunResponse {
                success: false,
                request_id: request_id.clone(),
                client_id: client_id.to_string(),
                cwd: pending.request.cwd.clone(),
                command_preview: command_preview(&pending.request.command),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(error.to_string()),
            };
            // The receiver may already be gone if the caller timed out first;
            // a failed send is expected and safe to ignore.
            let _ = waiter.send(response);
        }
    }
}

impl ShellClientRegistry {
    pub async fn enqueue_file_op(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        let request_id = next_request_id();
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
            old_text: body.old_text.clone(),
            pattern: body.pattern.clone(),
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            line: body.line,
            create_dirs: body.create_dirs,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
        };
        let mut inner = self.inner.lock().await;
        enqueue_pending_request_locked(
            &mut inner,
            &body.client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )?;
        notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    pub async fn enqueue_run(
        &self,
        body: ShellRunRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_run_request(&body)?;
        let request_id = next_request_id();
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
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: body.command.clone(),
            stdin: body.stdin.clone(),
            timeout_secs: body.timeout_secs,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
        };
        let mut inner = self.inner.lock().await;
        enqueue_pending_request_locked(
            &mut inner,
            &body.client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )?;
        notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    pub async fn cancel_request(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        remove_pending_request_locked(&mut inner, request_id);
    }

    /// Enqueue a project-management agent request (`register_project` or
    /// `create_project`). The JSON payload is carried in `stdin` so the
    /// agent can parse it without shell interpolation. The `command` field is
    /// empty (unused for these kinds); the agent dispatches on `kind` and
    /// reads the payload from `stdin`. Returns a oneshot receiver for the
    /// `ShellRunResponse` (the agent returns structured JSON in `stdout`).
    pub async fn enqueue_project_op(
        &self,
        client_id: String,
        kind: &str,
        payload: String,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_id(&client_id, "client_id")?;
        if kind != "register_project" && kind != "create_project" {
            return Err(format!("unsupported project op kind: {}", kind));
        }
        if payload.contains('\0') {
            return Err("project op payload must not contain NUL".to_string());
        }
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: kind.to_string(),
            job_id: None,
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
            stdin: Some(payload),
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
        };
        let mut inner = self.inner.lock().await;
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )?;
        notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
    }

    /// Enqueue a typed read-only LSP navigation request. Never falls through
    /// to shell execution: the agent dispatches exclusively on `kind = "lsp"`
    /// with a structured `lsp` payload.
    pub async fn enqueue_lsp(
        &self,
        client_id: String,
        payload: AgentLspPayload,
        requested_by: String,
        timeout_secs: u64,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_id(&client_id, "client_id")?;
        // Capability gate before enqueue so old agents never receive unknown
        // LSP kinds that could fall into shell fallback.
        if !self
            .client_supports(&client_id, SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION)
            .await?
        {
            return Err(format!(
                "agent client {} does not support {}",
                client_id, SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION
            ));
        }
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: AGENT_LSP_REQUEST_KIND.to_string(),
            job_id: None,
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
            timeout_secs: timeout_secs.max(1),
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: Some(payload),
        };
        let mut inner = self.inner.lock().await;
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )?;
        notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
    }
}
