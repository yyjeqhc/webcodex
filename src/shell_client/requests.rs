use super::auth::assert_shell_client_access;
use super::jobs::{
    command_preview, ensure_dispatch_supported_locked, ensure_queue_capacity_locked,
    request_preview, PendingRequestEnqueueError,
};
use super::projects::{capability_enabled, ShellClientLookupError};
use super::state::{PendingShellRequest, ShellClientRegistryInner};
use super::validation::{
    validate_file_request, validate_id, validate_process_request, validate_run_request,
    validate_script_enqueue_request,
};
use super::{now_ts, ShellClientRegistry, CLIENT_ONLINE_WINDOW_SECS};
use crate::lsp_bridge::{AgentLspPayload, AgentLspRequest, AGENT_LSP_REQUEST_KIND};
use crate::shell_protocol::{
    shell_computer_request_payload_max_bytes, PersistentShellRequest, PersistentShellResult,
    ShellAgentShellRequest, ShellFileOpRequest, ShellJobContext, ShellProcessArgv, ShellRunRequest,
    ShellRunResponse, ShellScriptPayload, RAW_SHELL_COMMAND_MAX_BYTES,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ,
    SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL, SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE, SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION, SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE, SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE, SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION, SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL,
    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS, SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
};
use std::fmt;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnqueueLspError {
    InvalidRequest {
        message: String,
    },
    UnknownClient {
        client_id: String,
    },
    ClientOffline {
        client_id: String,
    },
    UnsupportedCapability {
        client_id: String,
        capability: &'static str,
    },
    QueueFull {
        client_id: String,
        limit: usize,
    },
}

impl fmt::Display for EnqueueLspError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest { message } => formatter.write_str(message),
            Self::UnknownClient { client_id } => {
                write!(formatter, "unknown shell client: {client_id}")
            }
            Self::ClientOffline { client_id } => write!(
                formatter,
                "shell client {client_id} is offline (no keepalive within \
                 {CLIENT_ONLINE_WINDOW_SECS}s); reconnect the agent before retrying"
            ),
            Self::UnsupportedCapability {
                client_id,
                capability,
            } => write!(
                formatter,
                "agent client {client_id} does not support {capability}"
            ),
            Self::QueueFull { client_id, limit } => write!(
                formatter,
                "too many pending requests for shell client {client_id} (limit {limit})"
            ),
        }
    }
}

impl std::error::Error for EnqueueLspError {}

impl From<PendingRequestEnqueueError> for EnqueueLspError {
    fn from(error: PendingRequestEnqueueError) -> Self {
        match error {
            PendingRequestEnqueueError::UnknownClient { client_id } => {
                Self::UnknownClient { client_id }
            }
            PendingRequestEnqueueError::ClientOffline { client_id } => {
                Self::ClientOffline { client_id }
            }
            PendingRequestEnqueueError::QueueFull { client_id, limit } => {
                Self::QueueFull { client_id, limit }
            }
        }
    }
}

impl From<ShellClientLookupError> for EnqueueLspError {
    fn from(error: ShellClientLookupError) -> Self {
        match error {
            ShellClientLookupError::UnknownClient { client_id } => {
                Self::UnknownClient { client_id }
            }
        }
    }
}

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
) -> Result<(), PendingRequestEnqueueError> {
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
            expected_client_owner: None,
            expected_project_id: None,
            expected_project_cwd: None,
            dispatched: false,
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
                command_preview: request_preview(&pending.request),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(error.to_string()),
                request_dispatched: Some(pending.dispatched),
                command_execution_state: None,
            };
            // The receiver may already be gone if the caller timed out first;
            // a failed send is expected and safe to ignore.
            let _ = waiter.send(response);
        }
        inner.persistent_waiters.remove(&request_id);
    }
}

impl ShellClientRegistry {
    pub async fn enqueue_file_op(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        if body.op == "read_project_artifact_export_chunk" {
            return Err(
                "read_project_artifact_export_chunk is internal-only; generic file-op enqueue is forbidden"
                    .to_string(),
            );
        }
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
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            create_dirs: body.create_dirs,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
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

    /// Enqueue the create-only artifact write used by computer_save_snapshot.
    /// Target ownership and file_write are rechecked under the same registry
    /// lock as pending admission so a concurrent Runner replacement cannot
    /// receive a write it no longer advertises.
    pub(crate) async fn enqueue_computer_snapshot_artifact(
        &self,
        body: ShellFileOpRequest,
        expected_project_id: &str,
        expected_project_cwd: &str,
        requested_by: String,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        if body.op != "save_project_artifact" {
            return Err(format!(
                "computer snapshot artifact enqueue only accepts op=save_project_artifact (got {})",
                body.op
            ));
        }
        if expected_project_id.is_empty()
            || expected_project_cwd.is_empty()
            || body.cwd.as_deref().map(str::trim) != Some(expected_project_cwd)
        {
            return Err(
                "computer snapshot artifact target project identity is invalid".to_string(),
            );
        }
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind: "file_save_project_artifact".to_string(),
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: Some(body.path.trim().to_string()),
            content: body.content.clone(),
            max_bytes: body.max_bytes,
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            create_dirs: body.create_dirs,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
        let current = inner
            .clients
            .get(&body.client_id)
            .ok_or_else(|| format!("unknown shell client: {}", body.client_id))?;
        assert_shell_client_access(auth, current)?;
        if !current.capabilities.file_write {
            return Err(format!(
                "capability_unavailable: agent client {} does not support {SHELL_CLIENT_CAPABILITY_FILE_WRITE}",
                body.client_id
            ));
        }
        if !current.projects.iter().any(|project| {
            !project.disabled
                && project.id == expected_project_id
                && project.path == expected_project_cwd
        }) {
            return Err(format!(
                "stale_project: target project {expected_project_id} is no longer registered at the resolved path"
            ));
        }
        let expected_client_owner = current.owner.clone();
        enqueue_pending_request_locked(
            &mut inner,
            &body.client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )?;
        let pending = inner
            .pending_by_id
            .get_mut(&request_id)
            .expect("computer snapshot artifact request was just enqueued");
        pending.expected_client_owner = expected_client_owner;
        pending.expected_project_id = Some(expected_project_id.to_string());
        pending.expected_project_cwd = Some(expected_project_cwd.to_string());
        notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    /// Enqueue the internal artifact-export segment read. Capability checks and
    /// pending admission share the registry lock so a mixed-version replacement
    /// can never receive an unsupported request. Only an explicit capability
    /// miss is eligible for the Control-side legacy read fallback.
    pub(crate) async fn enqueue_artifact_export_chunk(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        if body.op != "read_project_artifact_export_chunk" {
            return Err(format!(
                "artifact export chunk enqueue only accepts op=read_project_artifact_export_chunk (got {})",
                body.op
            ));
        }
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind: "file_read_project_artifact_export_chunk".to_string(),
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: Some(body.path.trim().to_string()),
            content: body.content.clone(),
            max_bytes: body.max_bytes,
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            create_dirs: body.create_dirs,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&body.client_id) else {
            return Err(format!("unknown shell client: {}", body.client_id));
        };
        assert_shell_client_access(auth, client)?;
        if !client.capabilities.file_read {
            return Err(format!(
                "capability_unavailable: agent client {} does not support {SHELL_CLIENT_CAPABILITY_FILE_READ}",
                body.client_id
            ));
        }
        if !client.capabilities.artifact_export_chunk_read {
            return Err(format!(
                "capability_unavailable: agent client {} does not support {SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ}",
                body.client_id
            ));
        }
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

    /// Enqueue a structured `delete_project_files` agent file op. This is the
    /// authoritative TOCTOU fence for mixed-version rolling upgrades: the
    /// `structured_file_delete` capability check and the pending-request
    /// admission happen under the same registry lock, so an agent that
    /// re-registers without the capability between a caller's pre-check and
    /// this call never receives a structured delete request it cannot
    /// understand.
    ///
    /// When the current client no longer advertises the capability, nothing is
    /// queued, no waiter or request is created, and an error carrying the
    /// `capability_unavailable:` prefix is returned so the caller can take the
    /// legacy shell fallback (supported by old and new Runners).
    pub(crate) async fn enqueue_structured_file_delete(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        if body.op != "delete_project_files" {
            return Err(format!(
                "structured file delete only accepts op=delete_project_files (got {})",
                body.op
            ));
        }
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
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            create_dirs: body.create_dirs,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&body.client_id) else {
            return Err(format!("unknown shell client: {}", body.client_id));
        };
        if !client.capabilities.structured_file_delete {
            return Err(format!(
                "capability_unavailable: agent client {} does not support {SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE}",
                body.client_id
            ));
        }
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
        self.enqueue_run_with_sandbox(body, requested_by, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn enqueue_process_with_sandbox(
        &self,
        client_id: String,
        cwd: Option<String>,
        process: ShellProcessArgv,
        stdin: Option<String>,
        timeout_secs: u64,
        wait_timeout_secs: u64,
        requested_by: String,
        sandbox: Option<String>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_process_request(
            &client_id,
            cwd.as_deref(),
            &process,
            stdin.as_deref(),
            timeout_secs,
            wait_timeout_secs,
        )?;
        let normalized_cwd = cwd.map(|cwd| cwd.trim().to_string());
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: "run_process".to_string(),
            job_id: None,
            cwd: normalized_cwd,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: Some(process),
            script: None,
            stdin,
            timeout_secs,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: sandbox.clone(),
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {client_id}"));
        };
        if !client.capabilities.structured_process_argv {
            return Err(format!(
                "capability_unavailable: agent client {client_id} does not support {SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV}"
            ));
        }
        if let Some(mode) = sandbox.as_deref() {
            if mode != crate::command_sandbox::INSPECT_SANDBOX_MODE {
                return Err(format!("unknown sandbox mode '{mode}'"));
            }
            if !client.capabilities.sandbox_inspect_commands {
                return Err(format!(
                    "{}: agent client {} cannot enforce the inspect sandbox",
                    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS, client_id
                ));
            }
        }
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn enqueue_script_with_sandbox(
        &self,
        client_id: String,
        cwd: Option<String>,
        script: ShellScriptPayload,
        stdin: Option<String>,
        timeout_secs: u64,
        wait_timeout_secs: u64,
        requested_by: String,
        sandbox: Option<String>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_script_enqueue_request(
            &client_id,
            cwd.as_deref(),
            &script,
            stdin.as_deref(),
            timeout_secs,
            wait_timeout_secs,
        )?;
        let normalized_cwd = cwd.map(|cwd| cwd.trim().to_string());
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: "run_script".to_string(),
            job_id: None,
            cwd: normalized_cwd,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: Some(script),
            stdin,
            timeout_secs,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: sandbox.clone(),
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {client_id}"));
        };
        if !client.capabilities.structured_script_payload {
            return Err(format!(
                "capability_unavailable: agent client {client_id} does not support {SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD}"
            ));
        }
        if let Some(mode) = sandbox.as_deref() {
            if mode != crate::command_sandbox::INSPECT_SANDBOX_MODE {
                return Err(format!("unknown sandbox mode '{mode}'"));
            }
            if !client.capabilities.sandbox_inspect_commands {
                return Err(format!(
                    "{}: agent client {} cannot enforce the inspect sandbox",
                    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS, client_id
                ));
            }
        }
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

    pub(crate) async fn enqueue_run_with_sandbox(
        &self,
        body: ShellRunRequest,
        requested_by: String,
        sandbox: Option<String>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        self.enqueue_run_with_sandbox_and_ssh(body, requested_by, sandbox, None, None)
            .await
    }

    /// Internal Session execution context for a named Runner-local SSH
    /// resource. Public shell endpoints do not accept this field directly.
    pub(crate) async fn enqueue_run_with_sandbox_and_ssh(
        &self,
        body: ShellRunRequest,
        requested_by: String,
        sandbox: Option<String>,
        ssh_resource: Option<String>,
        ssh_session_id: Option<String>,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_run_request(&body)?;
        // A Workflow Session may execute locally without an SSH resource.
        // Only remote execution without the Session that owns the resource is
        // invalid.
        if ssh_resource.is_some() && ssh_session_id.is_none() {
            return Err(
                "ssh_session_required: an SSH resource requires a Workflow Session id".to_string(),
            );
        }
        let normalized_cwd = body.cwd.clone().map(|cwd| cwd.trim().to_string());
        let ssh_context = ssh_resource
            .zip(ssh_session_id)
            .map(|(resource, session_id)| ShellJobContext {
                runtime_project_id: None,
                workflow_session_id: Some(session_id),
                ssh_resource: Some(resource),
                project_cwd: None,
                cwd: normalized_cwd.clone(),
                purpose: None,
                shell: None,
                command_preview: command_preview(&body.command),
                validation_steps: Vec::new(),
                validation: None,
                structured_execution: None,
            });
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: normalized_cwd,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: body.command.clone(),
            process: None,
            script: None,
            stdin: body.stdin.clone(),
            timeout_secs: body.timeout_secs,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: sandbox.clone(),
            job_context: ssh_context,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        if request
            .job_context
            .as_ref()
            .is_some_and(|context| context.ssh_resource.is_some())
        {
            let Some(client) = inner.clients.get(&body.client_id) else {
                return Err(format!("unknown shell client: {}", body.client_id));
            };
            if !client.capabilities.ssh_shell {
                return Err(format!(
                    "agent_capability_unavailable: agent client {} does not support ssh_shell",
                    body.client_id
                ));
            }
        }
        if let Some(mode) = sandbox.as_deref() {
            if mode != crate::command_sandbox::INSPECT_SANDBOX_MODE {
                return Err(format!("unknown sandbox mode '{mode}'"));
            }
            let Some(client) = inner.clients.get(&body.client_id) else {
                return Err(format!("unknown shell client: {}", body.client_id));
            };
            if !client.capabilities.sandbox_inspect_commands {
                return Err(format!(
                    "{}: agent client {} cannot enforce the inspect sandbox",
                    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS, body.client_id
                ));
            }
        }
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

    /// Cancel a pending synchronous request and report whether an Agent had
    /// already polled it. This lets timeout callers distinguish queue timeout
    /// from an actually started command without retaining expired requests.
    pub async fn cancel_request(&self, request_id: &str) -> bool {
        self.cancel_request_dispatch_state(request_id)
            .await
            .unwrap_or(false)
    }

    /// Remove synchronous pending requests whose response receiver has already
    /// been dropped by the caller. A closed waiter proves there is no remaining
    /// observer for the response, so retaining the queue/registry entry can only
    /// consume bounded pending-request capacity until a late result or disconnect.
    pub(crate) async fn cancel_abandoned_sync_requests(&self) -> usize {
        let mut inner = self.inner.lock().await;
        let abandoned = inner
            .pending_by_id
            .iter()
            .filter(|(_, pending)| {
                pending.job_id.is_none()
                    && pending
                        .waiter
                        .as_ref()
                        .is_some_and(tokio::sync::oneshot::Sender::is_closed)
            })
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();
        for request_id in &abandoned {
            inner.persistent_waiters.remove(request_id);
            remove_pending_request_locked(&mut inner, request_id);
        }
        abandoned.len()
    }

    /// Cancel a pending synchronous request while preserving the distinction
    /// between an undispatched request and one whose registry record was
    /// already consumed. A missing record cannot prove that execution did not
    /// start, so lifecycle-sensitive callers must treat `None` conservatively.
    pub(crate) async fn cancel_request_dispatch_state(&self, request_id: &str) -> Option<bool> {
        let mut inner = self.inner.lock().await;
        inner.persistent_waiters.remove(request_id);
        remove_pending_request_locked(&mut inner, request_id).map(|pending| pending.dispatched)
    }

    /// Enqueue one explicit persistent-shell lifecycle operation. Capability
    /// absence is a hard failure; there is no fallback to `run_shell`.
    ///
    /// `job_context` carries safe Session/resource metadata so the Runner can
    /// route an SSH persistent shell to its bound resource; it is `None` for
    /// local persistent shells. SSH persistent shells additionally require the
    /// `ssh_persistent_shell` capability (plus `ssh_shell` and
    /// `persistent_shell`); absence fails closed before enqueue.
    pub(crate) async fn enqueue_persistent_shell(
        &self,
        client_id: String,
        request: PersistentShellRequest,
        job_context: Option<ShellJobContext>,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<PersistentShellResult>), String> {
        validate_id(&client_id, "client_id")?;
        validate_id(&request.shell_id, "shell_id")?;
        validate_id(&request.workflow_session_id, "workflow_session_id")?;
        if request.runtime_project_id.trim().is_empty() {
            return Err("runtime_project_id is required".to_string());
        }
        if !matches!(
            request.action.as_str(),
            "open" | "exec" | "status" | "close"
        ) {
            return Err(format!(
                "unsupported persistent shell action: {}",
                request.action
            ));
        }
        if request
            .command
            .as_deref()
            .is_some_and(|command| command.contains('\0'))
        {
            return Err("persistent shell command cannot contain NUL".to_string());
        }
        if request
            .command
            .as_deref()
            .is_some_and(|command| command.len() > RAW_SHELL_COMMAND_MAX_BYTES)
        {
            return Err(format!(
                "persistent shell command too long (max {RAW_SHELL_COMMAND_MAX_BYTES} bytes)"
            ));
        }
        let request_id = next_request_id();
        let (tx, rx) = oneshot::channel();
        let wire_request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: "persistent_shell".to_string(),
            job_id: None,
            cwd: request.cwd.clone(),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: request.command.clone().unwrap_or_default(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: request.timeout_secs.unwrap_or(30),
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: job_context.clone(),
            persistent_shell: Some(request),
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {client_id}"));
        };
        if !client.capabilities.persistent_shell {
            return Err(format!(
                "agent_capability_unavailable: agent client {client_id} does not support {SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL}"
            ));
        }
        // An SSH persistent shell requires all three capabilities; a legacy
        // runner that predates ssh_persistent_shell must fail closed here rather
        // than silently opening a local shell on the Runner host.
        if job_context
            .as_ref()
            .is_some_and(|ctx| ctx.ssh_resource.is_some())
            && (!client.capabilities.ssh_shell || !client.capabilities.ssh_persistent_shell)
        {
            return Err(format!(
                "agent_capability_unavailable: agent client {client_id} does not support {SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL}"
            ));
        }
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            wire_request,
            None,
            None,
        )?;
        inner.persistent_waiters.insert(request_id.clone(), tx);
        notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
    }

    /// Enqueue a project-management agent request (`register_project`,
    /// `create_project`, or the internal path resolver). The JSON payload is
    /// carried in `stdin` so the
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
        if !matches!(
            kind,
            "register_project"
                | "create_project"
                | "resolve_or_register_project"
                | "project_lifecycle_enable"
                | "project_lifecycle_disable"
                | "project_lifecycle_unregister"
        ) {
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
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: Some(payload),
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
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

    /// Enqueue one typed bounded computer request. The payload is bounded JSON
    /// in stdin and command is always empty. Owner/auth and the exact per-kind
    /// computer capability requirements are rechecked under the registry lock so a
    /// concurrent re-registration cannot create a TOCTOU escape.
    pub async fn enqueue_computer(
        &self,
        client_id: String,
        kind: &'static str,
        payload: String,
        requested_by: String,
        auth: Option<&crate::auth::AuthContext>,
        timeout_secs: u64,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_id(&client_id, "client_id")?;
        let required_capabilities: &[&str] = match kind {
            "computer_list_windows" | "computer_snapshot" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE]
            }
            "computer_snapshot_region" => &[
                SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
                SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION,
            ],
            "computer_accessibility_status" | "computer_accessibility_tree" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE]
            }
            "computer_element_state" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE],
            "computer_control" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL],
            "computer_scroll_to_element" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT],
            "computer_activate_window" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE],
            "computer_input_text" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT],
            _ => return Err("invalid computer request kind".to_string()),
        };
        if payload.len() > shell_computer_request_payload_max_bytes(kind) || payload.contains('\0')
        {
            return Err("computer request payload is invalid or too large".to_string());
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
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: Some(payload),
            timeout_secs: timeout_secs.max(1),
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
        let current = inner
            .clients
            .get(&client_id)
            .ok_or_else(|| format!("unknown shell client: {client_id}"))?;
        assert_shell_client_access(auth, current)?;
        if let Some(required_capability) = required_capabilities
            .iter()
            .copied()
            .find(|capability| !capability_enabled(&current.capabilities, capability))
        {
            return Err(format!(
                "agent client {client_id} does not support {required_capability}"
            ));
        }
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )
        .map_err(|error| error.to_string())?;
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
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), EnqueueLspError> {
        validate_id(&client_id, "client_id")
            .map_err(|message| EnqueueLspError::InvalidRequest { message })?;
        // Capability gate before enqueue so old agents never receive unknown
        // LSP kinds that could fall into shell fallback.
        let required_capability =
            if matches!(&payload.request, AgentLspRequest::CallHierarchy { .. }) {
                SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY
            } else {
                SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION
            };
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
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: timeout_secs.max(1),
            requested_by,
            created_at: now_ts(),
            validation: None,
            lsp: Some(payload),
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
        // This check is the authoritative TOCTOU fence: capability validation
        // and pending-request admission happen under the same registry lock.
        let current =
            inner
                .clients
                .get(&client_id)
                .ok_or_else(|| EnqueueLspError::UnknownClient {
                    client_id: client_id.clone(),
                })?;
        if !capability_enabled(&current.capabilities, required_capability) {
            return Err(EnqueueLspError::UnsupportedCapability {
                client_id,
                capability: required_capability,
            });
        }
        enqueue_pending_request_locked(
            &mut inner,
            &client_id,
            request_id.clone(),
            request,
            Some(tx),
            None,
        )
        .map_err(EnqueueLspError::from)?;
        notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
    }
}
