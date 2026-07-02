use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentProjectSummary, ShellAgentShellRequest,
    ShellClientCapabilities, ShellClientJobLogRequest, ShellClientJobLogResponse,
    ShellClientJobStatusRequest, ShellClientJobStatusResponse, ShellClientJobStopRequest,
    ShellClientJobStopResponse, ShellClientJobsListRequest, ShellClientJobsListResponse,
    ShellClientRegisterRequest, ShellClientView, ShellFileOpRequest, ShellFileOpResponse,
    ShellJobInfo, ShellJobOpRequest, ShellJobOpResponse, ShellRunRequest, ShellRunResponse,
};
#[cfg(test)]
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellJobCodexMetadata,
};
use salvo::prelude::*;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, Notify};
use uuid::Uuid;

mod auth;
mod handlers;
mod jobs;
mod polling;
mod state;
mod validation;

use auth::{assert_shell_client_access, shell_client_visible_to_auth, shell_job_visible_to_auth};
pub(crate) use auth::{
    assert_shell_client_owner, effective_register_owner, enforce_agent_transport,
    enforce_register_owner, requested_by_from_auth, require_agent_transport_scope,
    ShellClientAuthGroup,
};
pub use handlers::{
    shell_agent_job_update, shell_agent_poll, shell_agent_register, shell_agent_result,
};
use jobs::{
    append_limited, assert_active_instance_locked, command_preview,
    ensure_dispatch_supported_locked, ensure_queue_capacity_locked, is_final_job_status, job_view,
    offline_last_seen, refresh_job_status_locked, replace_limited, select_lines,
};
use state::{
    NotifierEntry, PendingShellRequest, ShellClientRecord, ShellClientRegistryInner, ShellJobRecord,
};
use validation::{
    normalize_project_summaries, sha256_hex, trim_string, validate_agent_instance_id,
    validate_file_request, validate_id, validate_optional_field, validate_run_request,
};
#[cfg(test)]
use validation::{MAX_COMMAND_LEN, MAX_RUN_STDIN_BYTES};

const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const CLIENT_ONLINE_WINDOW_SECS: i64 = 60;
/// Maximum number of pending requests queued for a single agent client.
/// Bounds memory when an agent is slow or disconnected: once a client's
/// queue reaches this depth, new enqueues are rejected with a structured
/// error instead of growing unboundedly. The WebSocket outbound channel
/// (`OUTGOING_CHANNEL_CAPACITY` in `agent_ws.rs`) is smaller than this, so a
/// slow WebSocket agent fills its outbound channel first and the request
/// pump applies natural backpressure; this cap is the hard ceiling that
/// protects the registry when even that backpressure cannot drain (e.g. a
/// dead socket the OS has not yet reported as closed).
const MAX_QUEUED_REQUESTS_PER_CLIENT: usize = 256;

/// Transport label for polling agents (HTTP `/api/shell/agent/poll`).
pub const TRANSPORT_POLLING: &str = "polling";
/// Transport label for agents connected over the WebSocket endpoint.
pub const TRANSPORT_WEBSOCKET: &str = "websocket";
/// Transport label for agents connected over the custom QUIC stream transport.
/// Reported in `ShellClientView.transport` and surfaced by `runtime_status` /
/// `listAgents`. New deployments should generally use `transport = "auto"`
/// with `[quic]` configured so QUIC is attempted before fallback transports.
pub const TRANSPORT_QUIC: &str = "quic";

#[derive(Debug, Default)]
pub struct ShellClientRegistry {
    inner: Mutex<ShellClientRegistryInner>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

impl ShellClientRegistry {
    #[allow(dead_code)]
    pub async fn register(
        &self,
        body: ShellClientRegisterRequest,
    ) -> Result<ShellClientView, String> {
        self.register_with_auth(body, None).await
    }

    pub(crate) async fn register_with_auth(
        &self,
        body: ShellClientRegisterRequest,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Result<ShellClientView, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        validate_optional_field(&body.display_name, "display_name")?;
        validate_optional_field(&body.owner, "owner")?;
        validate_optional_field(&body.hostname, "hostname")?;

        let client_id = body.client_id.trim().to_string();
        let agent_instance_id = body.agent_instance_id.trim().to_string();
        let record = ShellClientRecord {
            client_id: client_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            display_name: trim_string(body.display_name),
            owner: trim_string(body.owner),
            hostname: trim_string(body.hostname),
            capabilities: body.capabilities.unwrap_or_default(),
            projects: normalize_project_summaries(body.projects),
            last_seen: now_ts(),
            agent_protocol_version: body
                .agent_protocol_version
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            transport: TRANSPORT_POLLING.to_string(),
            policy: body.policy,
            auth_group: auth.and_then(ShellClientAuthGroup::from_auth),
        };
        let mut inner = self.inner.lock().await;

        // Enforce the agent instance lease. `client_id` is the unique active
        // agent identity: at most one agent process may be online for it at a
        // time. Rules:
        //   - no existing client            -> accept (fresh registration)
        //   - existing client is stale/offline -> accept and replace the
        //     active instance (lease hand-off to the new process)
        //   - existing client is online and the same instance id reconnects
        //     -> accept as a refresh/reconnect
        //   - existing client is online and a *different* instance id tries to
        //     register -> reject with a clear error so two processes cannot
        //     steal each other's requests.
        if let Some(existing) = inner.clients.get(&client_id) {
            let online = now_ts().saturating_sub(existing.last_seen) <= CLIENT_ONLINE_WINDOW_SECS;
            let same_instance = existing.agent_instance_id == agent_instance_id;
            if online && !same_instance {
                return Err(format!(
                    "agent client {} is already online with a different instance",
                    client_id
                ));
            }
        }

        // When a different instance takes over the lease (stale replacement),
        // clear any notifier left by the previous instance so the request pump
        // for the dead process is not re-armed against the new one. A
        // same-instance refresh keeps its notifier in place.
        let replaced_instance = inner
            .clients
            .get(&client_id)
            .map(|existing| existing.agent_instance_id != agent_instance_id)
            .unwrap_or(false);
        if replaced_instance {
            inner.notifiers.remove(&client_id);
        }

        inner.clients.insert(client_id.clone(), record);
        Ok(Self::client_view_locked(&inner, &client_id).expect("client just inserted"))
    }

    /// Override the transport label for a registered client. Called by the
    /// WebSocket handler after a successful register so observability and
    /// `list_agents` reflect how the agent is actually connected. Polling
    /// agents keep the default `"polling"` label set during `register`.
    pub async fn set_transport(&self, client_id: &str, transport: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        client.transport = transport.to_string();
        Ok(())
    }

    /// Refresh `last_seen` for a registered client to "now" without performing
    /// any business operation. Used by the WebSocket reader so that idle
    /// keepalive traffic (`Ping`/`Pong`) keeps a connected agent inside the
    /// `CLIENT_ONLINE_WINDOW_SECS` online window. Without this, a WebSocket
    /// agent that has no pending requests would age out to `"stale"` after 60s
    /// even though its socket is still open.
    ///
    /// Returns an error (and mutates nothing) for an unknown `client_id` so
    /// callers can log a clear diagnostic; it is a no-op for the unknown path.
    /// `register`, `poll`, `complete`, and `update_job` already refresh
    /// `last_seen` on their own, so this is only needed for keepalive frames.
    ///
    /// `agent_instance_id` is required: a stale/replaced instance must not be
    /// able to refresh the active lease's `last_seen` via Ping/Pong. If the id
    /// does not match the currently active instance, the touch is rejected
    /// before mutating any state.
    pub async fn touch_client(
        &self,
        client_id: &str,
        agent_instance_id: &str,
    ) -> Result<(), String> {
        validate_agent_instance_id(agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        client.last_seen = now_ts();
        Ok(())
    }

    /// Test-only hook to force a client's `last_seen` so liveness/stale
    /// behavior can be exercised without sleeping for the full online window.
    #[cfg(test)]
    pub async fn set_last_seen_for_test(&self, client_id: &str, ts: i64) {
        let mut inner = self.inner.lock().await;
        if let Some(client) = inner.clients.get_mut(client_id) {
            client.last_seen = ts;
        }
    }

    /// Register a push notifier for a client. The WebSocket handler calls
    /// this after register; the server's request pump waits on the notifier
    /// between polls. Calling this replaces any previously registered
    /// notifier for the client (e.g. after a reconnect).
    ///
    /// `agent_instance_id` records which agent process owns the notifier. The
    /// caller is the instance that just successfully registered, so it is the
    /// active instance; this always installs/overwrites the notifier entry for
    /// `client_id` tagged with that instance id.
    pub async fn register_notifier(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        notify: Arc<Notify>,
    ) -> Result<(), String> {
        validate_agent_instance_id(agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        // Only the currently active instance may install a notifier. A late
        // notifier registration from a stale instance (e.g. it registered,
        // then was replaced before reaching this call) must not overwrite the
        // active instance's notifier.
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        inner.notifiers.insert(
            client_id.to_string(),
            NotifierEntry {
                notify,
                agent_instance_id: agent_instance_id.to_string(),
            },
        );
        Ok(())
    }

    /// Reconcile state after an agent transport disconnects or sends a
    /// graceful offline notice. Active-instance strategy:
    ///
    /// - remove the push notifier so the request pump is not re-armed;
    /// - mark every non-final, running-like job owned by the client as
    ///   `"lost"` with a descriptive error, and drop its pending request (the
    ///   oneshot waiter resolves to a "dropped" error on the caller side);
    /// - the client record itself is retained so late results/updates can be
    ///   logged and runtime_status/list_agents keep observability history;
    /// - `last_seen` is moved just outside the online window so the active
    ///   lease is released immediately and a restarted agent can register
    ///   without waiting for the normal 60s timeout.
    ///
    /// `agent_instance_id` identifies *which* agent process disconnected. The
    /// cleanup only fires when that id matches the currently active instance
    /// for `client_id`; a stale disconnect (e.g. instance A's socket finally
    /// tearing down after instance B already replaced it) must NOT remove
    /// B's notifier or mark B's jobs lost.
    ///
    /// This is intentionally conservative about jobs: a reconnecting agent that keeps
    /// running the same job will see the server-side job as `"lost"` (final),
    /// so its late `job_update`/`result` is ignored by `update_job`/`complete`.
    /// Operators should treat `"lost"` as "the server no longer tracks this
    /// job; restart it if needed". A future phase may lift `JobManager` to
    /// agent-level so reconnects can resume in-flight jobs.
    pub async fn reconcile_disconnect(&self, client_id: &str, agent_instance_id: &str) {
        let mut inner = self.inner.lock().await;
        // Only reconcile when the disconnect belongs to the currently active
        // instance. A stale disconnect (a previous process whose socket finally
        // tore down after a newer instance already took over the lease) must
        // not touch the active instance's notifier or jobs.
        let is_active = inner
            .clients
            .get(client_id)
            .map(|client| client.agent_instance_id == agent_instance_id)
            .unwrap_or(false);
        if !is_active {
            return;
        }
        // Remove the notifier only if it still belongs to this instance.
        if inner
            .notifiers
            .get(client_id)
            .map(|entry| entry.agent_instance_id == agent_instance_id)
            .unwrap_or(false)
        {
            inner.notifiers.remove(client_id);
        }
        let lost_error = "agent transport disconnected".to_string();
        let now = now_ts();
        if let Some(client) = inner.clients.get_mut(client_id) {
            client.last_seen = offline_last_seen(now);
        }
        let lost_job_ids: Vec<String> = inner
            .jobs_by_id
            .iter()
            .filter_map(|(job_id, job)| {
                if job.client_id != client_id {
                    return None;
                }
                if is_final_job_status(&job.status)
                    || !matches!(
                        job.status.as_str(),
                        "queued" | "agent_queued" | "running" | "stop_requested"
                    )
                {
                    return None;
                }
                Some(job_id.clone())
            })
            .collect();
        for job_id in lost_job_ids {
            let request_id = inner
                .jobs_by_id
                .get(&job_id)
                .and_then(|j| j.request_id.clone());
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                job.status = "lost".to_string();
                job.ended_at = Some(now);
                job.error = Some(lost_error.clone());
            }
            if let Some(request_id) = request_id {
                inner.pending_by_id.remove(&request_id);
                inner.request_to_job.remove(&request_id);
                if let Some(queue) = inner.queues_by_client.get_mut(client_id) {
                    queue.retain(|id| id != &request_id);
                }
            }
        }
    }

    /// Wake the push notifier for a client if one is registered. Called by
    /// the enqueue paths (`enqueue_run`, `enqueue_file_op`, `start_job`,
    /// `stop_job`) so the WebSocket pump can immediately push the new
    /// request to the agent instead of waiting for a poll. Holds no lock of
    /// its own; callers must already hold `inner`.
    fn notify_client_locked(inner: &ShellClientRegistryInner, client_id: &str) {
        if let Some(entry) = inner.notifiers.get(client_id) {
            entry.notify.notify_one();
        }
    }

    pub async fn list_clients(&self) -> Vec<ShellClientView> {
        let inner = self.inner.lock().await;
        let mut ids = inner.clients.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| Self::client_view_locked(&inner, &id))
            .collect()
    }

    pub(crate) async fn list_clients_for_auth(
        &self,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Vec<ShellClientView> {
        let inner = self.inner.lock().await;
        let mut ids = inner.clients.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.into_iter()
            .filter(|id| {
                inner
                    .clients
                    .get(id)
                    .map(|client| shell_client_visible_to_auth(auth, client))
                    .unwrap_or(false)
            })
            .filter_map(|id| Self::client_view_locked(&inner, &id))
            .collect()
    }

    pub async fn get_client_view(&self, client_id: &str) -> Option<ShellClientView> {
        let inner = self.inner.lock().await;
        Self::client_view_locked(&inner, client_id)
    }

    pub(crate) async fn get_client_view_for_auth(
        &self,
        client_id: &str,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Option<ShellClientView> {
        let inner = self.inner.lock().await;
        let client = inner.clients.get(client_id)?;
        if !shell_client_visible_to_auth(auth, client) {
            return None;
        }
        Self::client_view_locked(&inner, client_id)
    }

    pub(crate) async fn assert_client_access(
        &self,
        auth: Option<&crate::auth::AuthContext>,
        client_id: &str,
    ) -> Result<(), String> {
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        assert_shell_client_access(auth, client)
    }

    /// Return the capabilities advertised by a registered agent client.
    /// Errors with a structured `unknown shell client` message when the
    /// client is not registered.
    pub async fn get_client_capabilities(
        &self,
        client_id: &str,
    ) -> Result<ShellClientCapabilities, String> {
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        Ok(client.capabilities.clone())
    }

    /// Check whether a registered agent client supports a named capability.
    /// Recognized capability names: `shell`, `file_read`, `file_write`,
    /// `git`, `jobs`, `async_jobs`, `async_shell_jobs`. Unknown capability
    /// names return `false`.
    pub async fn client_supports(&self, client_id: &str, capability: &str) -> Result<bool, String> {
        let caps = self.get_client_capabilities(client_id).await?;
        Ok(match capability {
            "shell" => caps.shell,
            "file_read" => caps.file_read,
            "file_write" => caps.file_write,
            "git" => caps.git,
            "jobs" => caps.jobs,
            "async_jobs" => caps.async_jobs,
            "async_shell_jobs" => caps.async_shell_jobs,
            _ => false,
        })
    }

    /// List the projects registered for a given shell client. Currently only
    /// exercised by tests; kept as a public accessor of the registry API.
    #[allow(dead_code)]
    pub async fn list_client_projects(
        &self,
        client_id: &str,
    ) -> Result<Vec<ShellAgentProjectSummary>, String> {
        validate_id(client_id, "client_id")?;
        let inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        Ok(client.projects.clone())
    }

    /// Insert or replace a single project summary in the cached project list
    /// for `client_id`. Called by the runtime after a successful
    /// `register_project` / `create_project` agent operation so that
    /// `listProjects` sees the new project immediately, without waiting for
    /// the agent's next register/poll cycle. If a project with the same id
    /// already exists it is replaced; otherwise the new summary is appended
    /// and the list is re-sorted by id (matching `normalize_project_summaries`).
    pub async fn upsert_client_project(
        &self,
        client_id: &str,
        project: ShellAgentProjectSummary,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if let Some(existing) = client.projects.iter_mut().find(|p| p.id == project.id) {
            *existing = project;
        } else {
            client.projects.push(project);
            client.projects.sort_by(|a, b| a.id.cmp(&b.id));
            client.projects.dedup_by(|a, b| a.id == b.id);
        }
        Ok(())
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
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &body.client_id)?;
        ensure_queue_capacity_locked(&inner, &body.client_id)?;
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
        Self::notify_client_locked(&inner, &body.client_id);
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
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &body.client_id)?;
        ensure_queue_capacity_locked(&inner, &body.client_id)?;
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
        Self::notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    pub async fn cancel_request(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending_by_id.remove(request_id);
        for queue in inner.queues_by_client.values_mut() {
            queue.retain(|id| id != request_id);
        }
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
        let request_id = Uuid::new_v4().to_string();
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
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &client_id)?;
        ensure_queue_capacity_locked(&inner, &client_id)?;
        inner
            .queues_by_client
            .entry(client_id.clone())
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
        Self::notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
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
            stdin: None,
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
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        ensure_dispatch_supported_locked(&inner, &client_id)?;
        if !(client.capabilities.async_jobs || client.capabilities.async_shell_jobs) {
            return Err(format!(
                "agent client {} does not support async shell jobs",
                client_id
            ));
        }
        ensure_queue_capacity_locked(&inner, &client_id)?;
        inner
            .queues_by_client
            .entry(client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        let job = ShellJobRecord {
            job_id: job_id.clone(),
            request_id: Some(request_id.clone()),
            client_id: client_id.clone(),
            kind: "shell".to_string(),
            project_id: None,
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
        Self::notify_client_locked(&inner, &client_id);
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
            "agent_queued" | "running" | "stop_requested" => {
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
                };
                ensure_dispatch_supported_locked(&inner, &client_id)?;
                ensure_queue_capacity_locked(&inner, &client_id)?;
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
                let notify_client_id = job.client_id.clone();
                Self::notify_client_locked(&inner, &notify_client_id);
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
            agent_instance_id: client.agent_instance_id.clone(),
            display_name: client.display_name.clone(),
            owner: client.owner.clone(),
            hostname: client.hostname.clone(),
            status: if connected { "online" } else { "stale" }.to_string(),
            connected,
            last_seen: client.last_seen,
            capabilities: client.capabilities.clone(),
            pending_requests,
            projects: client.projects.clone(),
            agent_protocol_version: client.agent_protocol_version.clone(),
            transport: client.transport.clone(),
            policy: client.policy.clone(),
        })
    }
}

fn get_registry(depot: &Depot) -> Option<Arc<ShellClientRegistry>> {
    depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned()
}

async fn assert_registry_client_owner(
    registry: &ShellClientRegistry,
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
) -> Result<(), (StatusCode, String)> {
    if registry.get_client_view(client_id).await.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown shell client: {}", client_id),
        ));
    }
    registry
        .assert_client_access(auth, client_id)
        .await
        .map_err(|e| {
            let status = if e.contains("unknown shell client") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::FORBIDDEN
            };
            (status, e)
        })
}

fn record_shell_run_action(
    audit: &ActionAudit,
    response: &ShellRunResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("run", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"request_id": response.request_id}))
            .summary(json!({
                "client_id": response.client_id,
                "cwd": response.cwd,
                "command_preview": response.command_preview,
                "exit_code": response.exit_code,
                "duration_ms": response.duration_ms,
            })),
    );
}

fn record_shell_file_action(
    audit: &ActionAudit,
    response: &ShellFileOpResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new(response.op.clone(), response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"request_id": response.request_id}))
            .summary(json!({
                "client_id": response.client_id,
                "path": response.path,
                "cwd": response.cwd,
                "bytes": response.bytes,
                "sha256": response.sha256,
                "entries_count": response.entries.len(),
            })),
    );
}

fn record_shell_job_action(
    audit: &ActionAudit,
    response: &ShellJobOpResponse,
    http_status: StatusCode,
) {
    let job_id = response.job.as_ref().map(|job| job.job_id.clone());
    let job_ids = if response.jobs.is_empty() {
        Vec::<String>::new()
    } else {
        response.jobs.iter().map(|job| job.job_id.clone()).collect()
    };
    audit.record(
        ActionAuditRecord::new(response.op.clone(), response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"job_id": job_id, "job_ids": job_ids}))
            .summary(json!({
                "job_status": response.job.as_ref().map(|job| job.status.clone()),
                "client_id": response.job.as_ref().map(|job| job.client_id.clone()),
                "jobs_count": response.jobs.len(),
                "stdout_included": response.stdout.is_some(),
                "stderr_included": response.stderr.is_some(),
            })),
    );
}

fn record_shell_job_status_action(
    audit: &ActionAudit,
    response: &ShellClientJobStatusResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_status", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({
                "job_id": response.job_id,
                "client_id": response.client_id,
            }))
            .summary(json!({
                "kind": response.kind,
                "status": response.status,
                "exit_code": response.exit_code,
                "elapsed_secs": response.elapsed_secs,
            })),
    );
}

fn record_shell_job_log_action(
    audit: &ActionAudit,
    response: &ShellClientJobLogResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_log", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({
                "job_id": response.job_id,
                "client_id": response.client_id,
            }))
            .summary(json!({
                "stdout_included": response.stdout_tail.is_some(),
                "stderr_included": response.stderr_tail.is_some(),
                "next_stdout_line": response.next_stdout_line,
                "next_stderr_line": response.next_stderr_line,
            })),
    );
}

fn record_shell_job_stop_action(
    audit: &ActionAudit,
    response: &ShellClientJobStopResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_stop", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"job_id": response.job_id}))
            .summary(json!({"status": response.status})),
    );
}

fn record_shell_jobs_list_action(
    audit: &ActionAudit,
    response: &ShellClientJobsListResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_list", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"client_id": response.client_id}))
            .summary(json!({"jobs_count": response.jobs.len()})),
    );
}

fn render_shell_run(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellRunResponse,
) {
    res.status_code(status);
    record_shell_run_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_status(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobStatusResponse,
) {
    res.status_code(status);
    record_shell_job_status_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_log(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobLogResponse,
) {
    res.status_code(status);
    record_shell_job_log_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_stop_response(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobStopResponse,
) {
    res.status_code(status);
    record_shell_job_stop_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_jobs_list(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobsListResponse,
) {
    res.status_code(status);
    record_shell_jobs_list_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_file(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellFileOpResponse,
) {
    res.status_code(status);
    record_shell_file_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellJobOpResponse,
) {
    res.status_code(status);
    record_shell_job_action(audit, &response, status);
    res.render(Json(response));
}

#[handler]
pub async fn shell_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/run", "runShell");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_run(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            ShellRunResponse {
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
            },
        );
        return;
    };
    let body: ShellRunRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_run(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                ShellRunResponse {
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
                },
            );
            return;
        }
    };
    let wait_timeout_secs = body.wait_timeout_secs;
    let client_id = body.client_id.clone();
    let cwd = body.cwd.clone();
    let preview = command_preview(&body.command);
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        render_shell_run(
            res,
            &audit,
            status,
            ShellRunResponse {
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
            },
        );
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    let (request_id, rx) = match registry.enqueue_run(body, requested_by).await {
        Ok(result) => result,
        Err(e) => {
            render_shell_run(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                ShellRunResponse {
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
                },
            );
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => render_shell_run(res, &audit, StatusCode::OK, response),
        Ok(Err(_closed)) => render_shell_run(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            ShellRunResponse {
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
            },
        ),
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            render_shell_run(
                res,
                &audit,
                StatusCode::REQUEST_TIMEOUT,
                ShellRunResponse {
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
                },
            );
        }
    }
}

fn shell_file_response_from_run(
    op: String,
    path: String,
    cwd: Option<String>,
    request_content: Option<String>,
    response: ShellRunResponse,
) -> ShellFileOpResponse {
    let success = response.error.is_none() && response.exit_code == Some(0);
    let stdout = response.stdout.unwrap_or_default();
    let entries = if op == "list" && success {
        stdout.lines().map(|line| line.to_string()).collect()
    } else {
        Vec::new()
    };
    let content = if op == "read" && success {
        Some(stdout.clone())
    } else {
        None
    };
    let bytes = match op.as_str() {
        "read" => content.as_ref().map(|s| s.len()),
        "write" if success => Some(stdout.trim().parse::<usize>().unwrap_or(0)),
        _ => None,
    };
    let sha256 = match op.as_str() {
        "read" if success => content.as_ref().map(|s| sha256_hex(s)),
        "write" if success => request_content.as_ref().map(|s| sha256_hex(s)),
        _ => None,
    };
    ShellFileOpResponse {
        success,
        op,
        request_id: response.request_id,
        client_id: response.client_id,
        path,
        cwd,
        content,
        entries,
        bytes,
        sha256,
        stderr: response.stderr,
        error: response.error,
    }
}

#[handler]
pub async fn shell_file_op(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/file", "shellFileOp");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        let response = shell_file_error_response(
            "unknown".to_string(),
            String::new(),
            String::new(),
            None,
            "Shell client registry not configured".to_string(),
        );
        render_shell_file(res, &audit, StatusCode::INTERNAL_SERVER_ERROR, response);
        return;
    };
    let body: ShellFileOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            let response = shell_file_error_response(
                "unknown".to_string(),
                String::new(),
                String::new(),
                None,
                format!("Invalid JSON: {}", e),
            );
            render_shell_file(res, &audit, StatusCode::BAD_REQUEST, response);
            return;
        }
    };
    let op = body.op.clone();
    let client_id = body.client_id.clone();
    let path = body.path.clone();
    let cwd = body.cwd.clone();
    let request_content = body.content.clone();
    let wait_timeout_secs = body.wait_timeout_secs;
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        let response = shell_file_error_response(op, client_id, path, cwd, e);
        render_shell_file(res, &audit, status, response);
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    let (request_id, rx) = match registry.enqueue_file_op(body, requested_by).await {
        Ok(result) => result,
        Err(e) => {
            let response = shell_file_error_response(op, client_id, path, cwd, e);
            render_shell_file(res, &audit, StatusCode::BAD_REQUEST, response);
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => render_shell_file(
            res,
            &audit,
            StatusCode::OK,
            shell_file_response_from_run(op, path, cwd, request_content, response),
        ),
        Ok(Err(_closed)) => {
            let response = shell_file_error_response(
                op,
                client_id,
                path,
                cwd,
                "shell file request waiter was dropped".to_string(),
            );
            render_shell_file(res, &audit, StatusCode::INTERNAL_SERVER_ERROR, response);
        }
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            let response = shell_file_error_response(
                op,
                client_id,
                path,
                cwd,
                format!(
                    "timed out waiting {} seconds for shell file result",
                    wait_timeout_secs
                ),
            );
            render_shell_file(res, &audit, StatusCode::REQUEST_TIMEOUT, response);
        }
    }
}

fn shell_file_error_response(
    op: String,
    client_id: String,
    path: String,
    cwd: Option<String>,
    error: String,
) -> ShellFileOpResponse {
    ShellFileOpResponse {
        success: false,
        op,
        request_id: String::new(),
        client_id,
        path,
        cwd,
        content: None,
        entries: Vec::new(),
        bytes: None,
        sha256: None,
        stderr: None,
        error: Some(error),
    }
}

fn shell_job_error_response(op: String, error: String) -> ShellJobOpResponse {
    ShellJobOpResponse {
        success: false,
        op,
        job: None,
        jobs: Vec::new(),
        stdout: None,
        stderr: None,
        next_stdout_line: None,
        next_stderr_line: None,
        error: Some(error),
    }
}

fn shell_job_status_response_from_job(job: ShellJobInfo) -> ShellClientJobStatusResponse {
    ShellClientJobStatusResponse {
        success: true,
        job_id: Some(job.job_id.clone()),
        client_id: Some(job.client_id.clone()),
        kind: Some(job.kind.clone()),
        status: Some(job.status.clone()),
        elapsed_secs: job.elapsed_secs,
        exit_code: job.exit_code,
        result: job.result.clone(),
        job: Some(job),
        error: None,
    }
}

fn shell_job_status_error_response(error: String) -> ShellClientJobStatusResponse {
    ShellClientJobStatusResponse {
        success: false,
        job_id: None,
        client_id: None,
        kind: None,
        status: None,
        elapsed_secs: None,
        exit_code: None,
        result: None,
        job: None,
        error: Some(error),
    }
}

fn shell_job_log_error_response(error: String) -> ShellClientJobLogResponse {
    ShellClientJobLogResponse {
        success: false,
        job_id: None,
        client_id: None,
        stdout_tail: None,
        stderr_tail: None,
        next_stdout_line: None,
        next_stderr_line: None,
        job: None,
        error: Some(error),
    }
}

fn shell_job_stop_error_response(error: String) -> ShellClientJobStopResponse {
    ShellClientJobStopResponse {
        success: false,
        job_id: None,
        status: None,
        job: None,
        error: Some(error),
    }
}

fn shell_jobs_list_error_response(client_id: String, error: String) -> ShellClientJobsListResponse {
    ShellClientJobsListResponse {
        success: false,
        client_id,
        jobs: Vec::new(),
        error: Some(error),
    }
}

async fn authorize_job_access(
    registry: &ShellClientRegistry,
    auth: Option<&crate::auth::AuthContext>,
    job_id: &str,
    requested_client_id: Option<&str>,
) -> Result<ShellJobInfo, (StatusCode, String)> {
    let job = registry
        .get_job(job_id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Some(requested_client_id) = requested_client_id {
        if requested_client_id != job.client_id {
            return Err((
                StatusCode::FORBIDDEN,
                format!(
                    "job_id {} belongs to client {}, not {}",
                    job_id, job.client_id, requested_client_id
                ),
            ));
        }
    }
    assert_registry_client_owner(registry, auth, &job.client_id).await?;
    Ok(job)
}

#[handler]
pub async fn shell_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/job", "runShellJob");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_error_response(
                "unknown".to_string(),
                "Shell client registry not configured".to_string(),
            ),
        );
        return;
    };
    let body: ShellJobOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_error_response("unknown".to_string(), format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let op = body.op.clone();
    match op.as_str() {
        "start" => {
            let Some(client_id) = body.client_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "client_id is required for op=start".to_string()),
                );
                return;
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            let requested_by = requested_by_from_auth(auth.as_ref());
            match registry.start_job(body, requested_by).await {
                Ok(job) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout: None,
                        stderr: None,
                        next_stdout_line: None,
                        next_stderr_line: None,
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "status" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=status".to_string()),
                );
                return;
            };
            match registry.get_job(job_id).await {
                Ok(job) => {
                    if let Err((status, e)) =
                        assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
                    {
                        render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                        return;
                    }
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::OK,
                        ShellJobOpResponse {
                            success: true,
                            op,
                            job: Some(job),
                            jobs: Vec::new(),
                            stdout: None,
                            stderr: None,
                            next_stdout_line: None,
                            next_stderr_line: None,
                            error: None,
                        },
                    )
                }
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "list" => {
            let limit = body.limit.unwrap_or(20).clamp(1, 100);
            let mut jobs = Vec::new();
            for job in registry.list_jobs(Some(100)).await {
                if auth.as_ref().map(|auth| auth.is_admin()).unwrap_or(false) {
                    jobs.push(job);
                    continue;
                }
                if registry
                    .assert_client_access(auth.as_ref(), &job.client_id)
                    .await
                    .is_ok()
                {
                    jobs.push(job);
                }
            }
            jobs.truncate(limit);
            render_shell_job(
                res,
                &audit,
                StatusCode::OK,
                ShellJobOpResponse {
                    success: true,
                    op,
                    job: None,
                    jobs,
                    stdout: None,
                    stderr: None,
                    next_stdout_line: None,
                    next_stderr_line: None,
                    error: None,
                },
            );
        }
        "log" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=log".to_string()),
                );
                return;
            };
            let job = match registry.get_job(job_id).await {
                Ok(job) => job,
                Err(e) => {
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::BAD_REQUEST,
                        shell_job_error_response(op, e),
                    );
                    return;
                }
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            match registry
                .job_log(
                    job_id,
                    body.since_stdout_line,
                    body.since_stderr_line,
                    body.tail_lines,
                )
                .await
            {
                Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout,
                        stderr,
                        next_stdout_line: Some(next_stdout_line),
                        next_stderr_line: Some(next_stderr_line),
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "stop" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=stop".to_string()),
                );
                return;
            };
            let job = match registry.get_job(job_id).await {
                Ok(job) => job,
                Err(e) => {
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::BAD_REQUEST,
                        shell_job_error_response(op, e),
                    );
                    return;
                }
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            let requested_by = requested_by_from_auth(auth.as_ref());
            match registry.stop_job(job_id, requested_by).await {
                Ok(job) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout: None,
                        stderr: None,
                        next_stdout_line: None,
                        next_stderr_line: None,
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        _ => render_shell_job(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_error_response(
                op,
                "op must be one of start, status, log, stop, list".to_string(),
            ),
        ),
    }
}

#[handler]
pub async fn shell_job_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/shell/jobs/status",
        "getShellClientJobStatus",
    );
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_status(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_status_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobStatusRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_status(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_status_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    match authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        Ok(job) => render_shell_job_status(
            res,
            &audit,
            StatusCode::OK,
            shell_job_status_response_from_job(job),
        ),
        Err((status, e)) => {
            render_shell_job_status(res, &audit, status, shell_job_status_error_response(e))
        }
    }
}

#[handler]
pub async fn shell_job_log(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/log", "getShellClientJobLog");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_log(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_log_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobLogRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_log(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_log_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let job = match authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        Ok(job) => job,
        Err((status, e)) => {
            render_shell_job_log(res, &audit, status, shell_job_log_error_response(e));
            return;
        }
    };
    match registry
        .job_log(
            &body.job_id,
            body.since_stdout_line,
            body.since_stderr_line,
            body.tail_lines,
        )
        .await
    {
        Ok((job, stdout_tail, stderr_tail, next_stdout_line, next_stderr_line)) => {
            render_shell_job_log(
                res,
                &audit,
                StatusCode::OK,
                ShellClientJobLogResponse {
                    success: true,
                    job_id: Some(job.job_id.clone()),
                    client_id: Some(job.client_id.clone()),
                    stdout_tail,
                    stderr_tail,
                    next_stdout_line: Some(next_stdout_line),
                    next_stderr_line: Some(next_stderr_line),
                    job: Some(job),
                    error: None,
                },
            );
        }
        Err(e) => render_shell_job_log(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_log_error_response(e),
        ),
    }
    let _ = job;
}

#[handler]
pub async fn shell_job_stop(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/stop", "stopShellClientJob");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_stop_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobStopRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_stop_response(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_stop_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    if let Err((status, e)) = authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        render_shell_job_stop_response(res, &audit, status, shell_job_stop_error_response(e));
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    match registry.stop_job(&body.job_id, requested_by).await {
        Ok(job) => render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::OK,
            ShellClientJobStopResponse {
                success: true,
                job_id: Some(job.job_id.clone()),
                status: Some(job.status.clone()),
                job: Some(job),
                error: None,
            },
        ),
        Err(e) => render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_stop_error_response(e),
        ),
    }
}

#[handler]
pub async fn shell_jobs_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/list", "listShellClientJobs");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_jobs_list(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_jobs_list_error_response(
                String::new(),
                "Shell client registry not configured".to_string(),
            ),
        );
        return;
    };
    let body: ShellClientJobsListRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_jobs_list(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_jobs_list_error_response(String::new(), format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let client_id = body.client_id.clone();
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        render_shell_jobs_list(
            res,
            &audit,
            status,
            shell_jobs_list_error_response(client_id, e),
        );
        return;
    }
    match registry
        .list_jobs_for_client(
            &client_id,
            body.status.as_deref(),
            Some(body.limit.unwrap_or(20).clamp(1, 100)),
        )
        .await
    {
        Ok(jobs) => render_shell_jobs_list(
            res,
            &audit,
            StatusCode::OK,
            ShellClientJobsListResponse {
                success: true,
                client_id,
                jobs,
                error: None,
            },
        ),
        Err(e) => render_shell_jobs_list(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_jobs_list_error_response(client_id, e),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_protocol::AGENT_PROTOCOL_VERSION_QUIC_V1;

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
            shared_key_hash: None,
        }
    }

    /// Phase 3 test helper: build an agent-token AuthContext bound to
    /// `username` and `allowed_client_id`, carrying the given agent scopes.
    fn agent_auth_context(
        username: &str,
        allowed_client_id: &str,
        scopes: Vec<&str>,
    ) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::AgentToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-agent".to_string()),
            api_key_name: Some("agent key".to_string()),
            role: Some("user".to_string()),
            scopes: scopes.into_iter().map(str::to_string).collect(),
            is_bootstrap: false,
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(allowed_client_id.to_string()),
            shared_key_hash: None,
        }
    }

    fn shared_key_auth_context(hash: &str) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::SharedKey,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: Some("shared-key".to_string()),
            scopes: vec![
                crate::auth::SCOPE_AGENT_REGISTER.to_string(),
                crate::auth::SCOPE_AGENT_POLL.to_string(),
                crate::auth::SCOPE_AGENT_RESULT.to_string(),
                crate::auth::SCOPE_AGENT_JOB_UPDATE.to_string(),
            ],
            is_bootstrap: false,
            token_kind: Some("shared-key".to_string()),
            allowed_client_id: None,
            shared_key_hash: Some(hash.to_string()),
        }
    }

    fn open_auth_context() -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::OpenAnonymous,
            user_id: None,
            username: None,
            api_key_id: None,
            api_key_name: None,
            role: Some("open".to_string()),
            scopes: vec![
                crate::auth::SCOPE_AGENT_REGISTER.to_string(),
                crate::auth::SCOPE_AGENT_POLL.to_string(),
                crate::auth::SCOPE_AGENT_RESULT.to_string(),
                crate::auth::SCOPE_AGENT_JOB_UPDATE.to_string(),
            ],
            is_bootstrap: false,
            token_kind: Some("open".to_string()),
            allowed_client_id: None,
            shared_key_hash: None,
        }
    }

    fn oauth_bridge_auth_context(hash: &str, scopes: Vec<&str>) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::OAuth2Token,
            user_id: None,
            username: None,
            api_key_id: Some("oauth-access-token".to_string()),
            api_key_name: None,
            role: Some("shared-key".to_string()),
            scopes: scopes.into_iter().map(str::to_string).collect(),
            is_bootstrap: false,
            token_kind: Some("oauth2_shared_key".to_string()),
            allowed_client_id: Some("oauth-client".to_string()),
            shared_key_hash: Some(hash.to_string()),
        }
    }

    fn managed_oauth_auth_context(
        username: &str,
        shared_key_hash: Option<&str>,
    ) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::OAuth2Token,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("oauth-access-token".to_string()),
            api_key_name: None,
            role: Some("user".to_string()),
            scopes: Vec::new(),
            is_bootstrap: false,
            token_kind: Some("oauth2".to_string()),
            allowed_client_id: Some("oauth-client".to_string()),
            shared_key_hash: shared_key_hash.map(str::to_string),
        }
    }

    fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(id.to_string()),
            path: path.to_string(),
            allow_patch: true,
            kind: Some("rust".to_string()),
            description: Some("test project".to_string()),
            hooks: vec!["doctor".to_string(), "precommit".to_string()],
            disabled: false,
            git_branch: Some("codex".to_string()),
            git_head: Some("9a7d3ce".to_string()),
            git_dirty: Some(false),
            updated_at: 123456,
            shell_profile: None,
        }
    }

    fn async_job_capabilities() -> ShellClientCapabilities {
        let mut capabilities = ShellClientCapabilities::default();
        capabilities.async_jobs = true;
        capabilities.async_shell_jobs = true;
        capabilities.jobs = true;
        capabilities
    }

    fn file_request(op: &str) -> ShellFileOpRequest {
        ShellFileOpRequest {
            op: op.to_string(),
            client_id: "oe".to_string(),
            path: "src/auth/scopes.rs".to_string(),
            cwd: Some("/root/git/webcodex".to_string()),
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
            wait_timeout_secs: 0,
        }
    }

    #[test]
    fn validate_file_request_allows_read_with_start_and_end_line() {
        let mut req = file_request("read");
        req.start_line = Some(10);
        req.end_line = Some(20);

        validate_file_request(&req).unwrap();
    }

    #[test]
    fn validate_file_request_rejects_read_with_only_start_line() {
        let mut req = file_request("read");
        req.start_line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(
            err,
            "end_line is required when start_line is set for op=read"
        );
    }

    #[test]
    fn validate_file_request_rejects_read_with_only_end_line() {
        let mut req = file_request("read");
        req.end_line = Some(20);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(
            err,
            "start_line is required when end_line is set for op=read"
        );
    }

    #[test]
    fn validate_file_request_rejects_read_with_invalid_range() {
        let mut req = file_request("read");
        req.start_line = Some(20);
        req.end_line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "invalid line range");

        req.start_line = Some(0);
        req.end_line = Some(10);
        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "invalid line range");
    }

    #[tokio::test]
    async fn registry_filters_lightweight_clients_by_auth_group() {
        let registry = ShellClientRegistry::default();
        let shared_a = shared_key_auth_context("hash-a");
        let shared_b = shared_key_auth_context("hash-b");
        let bridge_a = oauth_bridge_auth_context("hash-a", vec![]);
        let managed_oauth = managed_oauth_auth_context("alice", Some("hash-a"));
        let open = open_auth_context();
        let bootstrap = auth_context(None, true);

        for (client_id, auth) in [
            ("shared-a", &shared_a),
            ("shared-b", &shared_b),
            ("open", &open),
        ] {
            registry
                .register_with_auth(
                    ShellClientRegisterRequest {
                        client_id: client_id.to_string(),
                        agent_instance_id: format!("inst-{}", client_id),
                        display_name: None,
                        owner: None,
                        hostname: None,
                        capabilities: Some(async_job_capabilities()),
                        projects: Some(vec![project_summary(client_id, "/tmp/project")]),
                        agent_protocol_version: None,
                        policy: None,
                    },
                    Some(auth),
                )
                .await
                .unwrap();
        }
        registry
            .register(ShellClientRegisterRequest {
                client_id: "managed".to_string(),
                agent_instance_id: "inst-managed".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: Some(vec![project_summary("managed", "/tmp/managed")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        let visible_to_a: Vec<String> = registry
            .list_clients_for_auth(Some(&shared_a))
            .await
            .into_iter()
            .map(|c| c.client_id)
            .collect();
        assert_eq!(visible_to_a, vec!["shared-a"]);
        let visible_to_bridge_a: Vec<String> = registry
            .list_clients_for_auth(Some(&bridge_a))
            .await
            .into_iter()
            .map(|c| c.client_id)
            .collect();
        assert_eq!(visible_to_bridge_a, vec!["shared-a"]);
        assert!(registry
            .assert_client_access(Some(&shared_a), "shared-a")
            .await
            .is_ok());
        assert!(registry
            .assert_client_access(Some(&bridge_a), "shared-a")
            .await
            .is_ok());
        assert!(registry
            .assert_client_access(Some(&shared_a), "shared-b")
            .await
            .unwrap_err()
            .contains("unknown shell client"));
        assert!(registry
            .assert_client_access(Some(&shared_a), "open")
            .await
            .unwrap_err()
            .contains("unknown shell client"));
        assert!(registry
            .assert_client_access(Some(&bridge_a), "shared-b")
            .await
            .unwrap_err()
            .contains("unknown shell client"));
        assert!(registry
            .assert_client_access(Some(&bridge_a), "open")
            .await
            .unwrap_err()
            .contains("unknown shell client"));

        let visible_to_open: Vec<String> = registry
            .list_clients_for_auth(Some(&open))
            .await
            .into_iter()
            .map(|c| c.client_id)
            .collect();
        assert_eq!(visible_to_open, vec!["open"]);
        assert_eq!(
            ShellClientAuthGroup::from_auth(&open),
            Some(ShellClientAuthGroup::OpenAnonymous)
        );
        assert_eq!(
            ShellClientAuthGroup::from_auth(&bridge_a),
            Some(ShellClientAuthGroup::SharedKey("hash-a".to_string()))
        );
        assert!(bridge_a.is_oauth_shared_key_subject());
        assert_eq!(ShellClientAuthGroup::from_auth(&managed_oauth), None);
        assert!(!managed_oauth.is_oauth_shared_key_subject());
        let visible_to_managed_oauth: Vec<String> = registry
            .list_clients_for_auth(Some(&managed_oauth))
            .await
            .into_iter()
            .map(|c| c.client_id)
            .collect();
        assert_eq!(visible_to_managed_oauth, vec!["managed"]);
        assert!(registry
            .assert_client_access(Some(&managed_oauth), "managed")
            .await
            .is_ok());
        assert!(registry
            .assert_client_access(Some(&managed_oauth), "shared-a")
            .await
            .unwrap_err()
            .contains("unknown shell client"));

        let visible_to_bootstrap: Vec<String> = registry
            .list_clients_for_auth(Some(&bootstrap))
            .await
            .into_iter()
            .map(|c| c.client_id)
            .collect();
        assert_eq!(
            visible_to_bootstrap,
            vec!["managed", "open", "shared-a", "shared-b"]
        );
    }

    #[test]
    fn validate_file_request_rejects_read_with_line_field() {
        let mut req = file_request("read");
        req.line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "line is only allowed for op=insert_at_line");
    }

    #[test]
    fn validate_file_request_rejects_read_with_expected_prefix() {
        let mut req = file_request("read");
        req.expected_prefix = Some("pub fn".to_string());

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "expected_prefix is only allowed for line edit ops");
    }

    #[test]
    fn requested_by_from_auth_uses_bootstrap_username_or_anonymous() {
        let bootstrap = auth_context(None, true);
        assert_eq!(requested_by_from_auth(Some(&bootstrap)), "bootstrap");

        let alice = auth_context(Some("alice"), false);
        assert_eq!(requested_by_from_auth(Some(&alice)), "alice");

        assert_eq!(requested_by_from_auth(None), "anonymous");
    }

    #[test]
    fn assert_shell_client_owner_enforces_owner_boundary() {
        let bootstrap = auth_context(None, true);
        assert!(assert_shell_client_owner(Some(&bootstrap), "client-1", None).is_ok());

        let alice = auth_context(Some("alice"), false);
        assert!(assert_shell_client_owner(Some(&alice), "client-1", Some("alice")).is_ok());

        let mismatch =
            assert_shell_client_owner(Some(&alice), "client-1", Some("bob")).unwrap_err();
        assert!(mismatch.contains("owned by bob"));
        assert!(mismatch.contains("belongs to alice"));

        let missing = assert_shell_client_owner(Some(&alice), "client-1", None).unwrap_err();
        assert_eq!(missing, "agent client client-1 has no owner");

        let anonymous = assert_shell_client_owner(None, "client-1", Some("anonymous")).unwrap_err();
        assert!(anonymous.contains("belongs to anonymous"));
    }

    #[tokio::test]
    async fn registry_registers_and_lists_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: Some("XRH".to_string()),
                owner: Some("yyjeqhc".to_string()),
                hostname: Some("fineserver".to_string()),
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
    async fn registry_register_saves_projects() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].projects.len(), 1);
        assert_eq!(clients[0].projects[0].id, "webcodex");

        let projects = registry.list_client_projects("oe").await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/root/git/webcodex");
    }

    #[tokio::test]
    async fn registry_poll_updates_projects() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("one", "/tmp/one")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: Some(vec![
                    project_summary("one", "/tmp/one"),
                    project_summary("two", "/tmp/two"),
                ]),
            })
            .await
            .unwrap();
        assert!(polled.is_none());

        let projects = registry.list_client_projects("oe").await.unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].id, "one");
        assert_eq!(projects[1].id, "two");
    }

    #[tokio::test]
    async fn registry_project_owner_check_enforces_boundary() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "alice-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "bob-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("bob".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("secret", "/tmp/secret")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        let alice = auth_context(Some("alice"), false);
        assert!(
            assert_registry_client_owner(&registry, Some(&alice), "alice-client")
                .await
                .is_ok()
        );
        let projects = registry.list_client_projects("alice-client").await.unwrap();
        assert_eq!(projects.len(), 1);

        let mismatch = assert_registry_client_owner(&registry, Some(&alice), "bob-client")
            .await
            .unwrap_err();
        assert_eq!(mismatch.0, StatusCode::FORBIDDEN);
        assert!(mismatch.1.contains("owned by bob"));
    }

    #[test]
    fn protocol_async_capability_defaults_false() {
        let capabilities = ShellClientCapabilities::default();
        assert!(!capabilities.async_jobs);
        assert!(!capabilities.async_shell_jobs);

        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "capabilities": {"shell": true}
            }"#,
        )
        .unwrap();
        let capabilities = request.capabilities.unwrap();
        assert!(!capabilities.async_jobs);
        assert!(!capabilities.async_shell_jobs);
    }

    #[test]
    fn protocol_serde_keeps_old_register_compatible() {
        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "capabilities": {"shell": true, "file_read": true}
            }"#,
        )
        .unwrap();
        assert_eq!(request.client_id, "oe");
        assert!(request.projects.is_none());
        // Old agents omit agent_protocol_version; the field deserializes as None.
        assert!(request.agent_protocol_version.is_none());
    }

    #[test]
    fn protocol_serde_parses_agent_protocol_version() {
        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "agent_protocol_version": "polling-v1"
            }"#,
        )
        .unwrap();
        assert_eq!(
            request.agent_protocol_version.as_deref(),
            Some("polling-v1")
        );
    }

    #[tokio::test]
    async fn register_without_protocol_version_defaults_to_unknown() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_protocol_version, "unknown");
    }

    #[tokio::test]
    async fn register_with_protocol_version_is_exposed_in_view() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_id, "xrh");
        assert_eq!(clients[0].agent_protocol_version, "polling-v1");
        let view = registry.get_client_view("xrh").await.unwrap();
        assert_eq!(view.agent_protocol_version, "polling-v1");
    }

    #[tokio::test]
    async fn register_blank_protocol_version_falls_back_to_unknown() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("   ".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_protocol_version, "unknown");
    }

    #[tokio::test]
    async fn client_supports_reflects_registered_capabilities() {
        let registry = ShellClientRegistry::default();
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        caps.file_read = true;
        caps.async_shell_jobs = true;
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(caps),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        assert!(registry.client_supports("oe", "shell").await.unwrap());
        assert!(registry.client_supports("oe", "file_read").await.unwrap());
        assert!(registry
            .client_supports("oe", "async_shell_jobs")
            .await
            .unwrap());
        assert!(!registry.client_supports("oe", "git").await.unwrap());
        // Unknown capability name is false, not an error.
        assert!(!registry.client_supports("oe", "teleport").await.unwrap());
        // Unknown client is a structured error.
        let err = registry
            .client_supports("ghost", "shell")
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
        let err = registry.get_client_capabilities("ghost").await.unwrap_err();
        assert!(err.contains("unknown shell client"));
    }

    #[tokio::test]
    async fn registry_enqueues_polls_and_completes_shell_request() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let (request_id, rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "xrh".to_string(),
                    cwd: Some("/tmp".to_string()),
                    command: "echo hello".to_string(),
                    stdin: Some("hello stdin".to_string()),
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
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.command, "echo hello");
        assert_eq!(polled.stdin.as_deref(), Some("hello stdin"));
        registry
            .complete(ShellAgentResultRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
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
                    stdin: None,
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
    }

    async fn register_quic_v1_client(registry: &ShellClientRegistry, client_id: &str) {
        registry
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: Some(vec![project_summary("webcodex", "/tmp/webcodex")]),
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            })
            .await
            .unwrap();
        registry
            .set_transport(client_id, TRANSPORT_QUIC)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_run_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-run").await;

        let (_request_id, _rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "quic-run".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let view = registry.get_client_view("quic-run").await.unwrap();
        assert_eq!(view.transport, TRANSPORT_QUIC);
        assert_eq!(view.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
        assert_eq!(view.pending_requests, 1);
        assert!(view.capabilities.shell);
        assert!(view.capabilities.async_shell_jobs);
    }

    #[tokio::test]
    async fn enqueue_file_op_allows_read_with_line_range() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "oe").await;

        let mut req = file_request("read");
        req.start_line = Some(7);
        req.end_line = Some(12);
        let (request_id, _rx) = registry
            .enqueue_file_op(req, "tester".to_string())
            .await
            .unwrap();

        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.kind, "file_read");
        assert_eq!(polled.path.as_deref(), Some("src/auth/scopes.rs"));
        assert_eq!(polled.start_line, Some(7));
        assert_eq!(polled.end_line, Some(12));
        assert_eq!(polled.line, None);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_file_and_project_ops_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-ops").await;

        let (_file_request_id, _file_rx) = registry
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "read".to_string(),
                    client_id: "quic-ops".to_string(),
                    path: "README.md".to_string(),
                    cwd: None,
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
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        let (_project_request_id, _project_rx) = registry
            .enqueue_project_op(
                "quic-ops".to_string(),
                "register_project",
                "{}".to_string(),
                "tester".to_string(),
            )
            .await
            .unwrap();

        let view = registry.get_client_view("quic-ops").await.unwrap();
        assert_eq!(view.pending_requests, 2);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_start_job_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-job").await;

        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("quic-job".to_string()),
                    cwd: None,
                    command: Some("sleep 1".to_string()),
                    timeout_secs: Some(5),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        let view = registry.get_client_view("quic-job").await.unwrap();
        assert_eq!(view.pending_requests, 1);
        assert_eq!(job.status, "queued");
        assert_eq!(registry.list_jobs(Some(10)).await.len(), 1);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_stop_job_delivery_queueing() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "quic-stop".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("quic-stop".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "quic-stop".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        registry
            .set_transport("quic-stop", TRANSPORT_QUIC)
            .await
            .unwrap();

        let stopped = registry
            .stop_job(&job.job_id, "tester".to_string())
            .await
            .unwrap();
        let view = registry.get_client_view("quic-stop").await.unwrap();
        assert_eq!(view.pending_requests, 1);
        assert_eq!(stopped.status, "stop_requested");
    }

    #[test]
    fn validate_run_request_allows_bounded_stdin_beyond_command_limit() {
        let body = ShellRunRequest {
            client_id: "client-1".to_string(),
            cwd: None,
            command: "cat >/dev/null".to_string(),
            stdin: Some("x".repeat(MAX_COMMAND_LEN + 1024)),
            timeout_secs: 10,
            wait_timeout_secs: 1,
        };
        validate_run_request(&body).expect("stdin has its own larger bound");
    }

    #[test]
    fn validate_run_request_rejects_oversized_stdin() {
        let body = ShellRunRequest {
            client_id: "client-1".to_string(),
            cwd: None,
            command: "cat >/dev/null".to_string(),
            stdin: Some("x".repeat(MAX_RUN_STDIN_BYTES + 1)),
            timeout_secs: 10,
            wait_timeout_secs: 1,
        };
        let err = validate_run_request(&body).unwrap_err();
        assert!(err.contains("stdin is too large"), "got: {}", err);
    }

    #[tokio::test]
    async fn registry_shell_job_start_poll_complete_and_log() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
                    codex: Some(ShellJobCodexMetadata {
                        project: Some("demo".to_string()),
                        goal_id: Some("goal-1".to_string()),
                        client_request_id: Some("crid-1".to_string()),
                        command: Some("printf hello".to_string()),
                        kind: Some("command".to_string()),
                        suite: None,
                        script_path: None,
                        reason: Some("test job".to_string()),
                        max_runtime_secs: Some(10),
                    }),
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(job.status, "queued");
        assert_eq!(
            job.codex
                .as_ref()
                .and_then(|codex| codex.client_request_id.as_deref()),
            Some("crid-1")
        );
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.command, "printf hello");
        let running = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(running.status, "agent_queued");
        registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
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
        assert_eq!(
            done.codex
                .as_ref()
                .and_then(|codex| codex.project.as_deref()),
            Some("demo")
        );
        let listed = registry.list_jobs(Some(10)).await;
        assert_eq!(
            listed
                .iter()
                .find(|listed| listed.job_id == job.job_id)
                .and_then(|listed| listed.codex.as_ref())
                .and_then(|codex| codex.goal_id.as_deref()),
            Some("goal-1")
        );
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
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let stopped = registry
            .stop_job(&job.job_id, "test".to_string())
            .await
            .unwrap();
        assert_eq!(stopped.status, "stopped");
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        assert!(polled.is_none());
    }

    #[tokio::test]
    async fn registry_shell_job_stop_running_delivers_stop_to_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let started = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(started.kind, "start_job");

        let stop_requested = registry
            .stop_job(&job.job_id, "test".to_string())
            .await
            .unwrap();
        assert_eq!(stop_requested.status, "stop_requested");
        let stop = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stop.kind, "stop_job");
        assert_eq!(stop.job_id.as_deref(), Some(job.job_id.as_str()));
    }

    #[tokio::test]
    async fn registry_marks_running_job_lost_when_client_stale() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
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

    #[tokio::test]
    async fn touch_client_refreshes_stale_client_back_to_online() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        // Age the client past the online window so it reads as stale.
        registry
            .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        let stale = registry.get_client_view("oe").await.unwrap();
        assert!(!stale.connected);
        assert_eq!(stale.status, "stale");

        // A keepalive touch must bring it back online.
        registry.touch_client("oe", "inst").await.unwrap();
        let fresh = registry.get_client_view("oe").await.unwrap();
        assert!(fresh.connected);
        assert_eq!(fresh.status, "online");

        // Unknown client_id is a clear error and does not mutate state.
        assert!(registry.touch_client("nope", "inst").await.is_err());
    }

    #[tokio::test]
    async fn touch_client_refreshes_websocket_transport_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        registry
            .set_transport("ws-1", TRANSPORT_WEBSOCKET)
            .await
            .unwrap();

        registry
            .set_last_seen_for_test("ws-1", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        let stale = registry.get_client_view("ws-1").await.unwrap();
        assert_eq!(stale.transport, "websocket");
        assert!(!stale.connected);

        registry.touch_client("ws-1", "inst").await.unwrap();
        let fresh = registry.get_client_view("ws-1").await.unwrap();
        assert_eq!(fresh.transport, "websocket");
        assert!(fresh.connected);
        assert_eq!(fresh.status, "online");
    }

    #[tokio::test]
    async fn touch_client_rejects_stale_instance_and_accepts_active() {
        // Regression: a stale/replaced instance must not refresh the active
        // lease's `last_seen` via Ping/Pong keepalive.
        let registry = ShellClientRegistry::default();
        // Instance A registers and is online.
        let view_a = register_with_instance(&registry, "oe", "inst-a").await;
        assert!(view_a.connected);

        // Age A out so a newer instance may take over the lease.
        registry
            .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        // Instance B replaces A.
        let view_b = register_with_instance(&registry, "oe", "inst-b").await;
        assert_eq!(view_b.agent_instance_id, "inst-b");
        assert!(view_b.connected);

        // Capture B's last_seen right after registration.
        let before = registry.get_client_view("oe").await.unwrap().last_seen;
        // Sleep a moment so a successful touch would observably advance
        // last_seen.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        // Stale instance A's keepalive must be rejected and must NOT advance
        // last_seen for B.
        let err = registry.touch_client("oe", "inst-a").await.unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );
        let after_a = registry.get_client_view("oe").await.unwrap().last_seen;
        assert_eq!(
            after_a, before,
            "stale instance touch must not refresh active last_seen"
        );
        // A stale instance must not resurrect the client to online either.
        let view_after_a = registry.get_client_view("oe").await.unwrap();
        assert!(view_after_a.connected);

        // Active instance B's keepalive succeeds and refreshes last_seen.
        registry.touch_client("oe", "inst-b").await.unwrap();
        let after_b = registry.get_client_view("oe").await.unwrap().last_seen;
        assert!(
            after_b > before,
            "active instance touch must refresh last_seen"
        );
        assert!(registry.get_client_view("oe").await.unwrap().connected);

        // An empty agent_instance_id is rejected by validation.
        assert!(registry.touch_client("oe", "").await.is_err());
    }

    #[test]
    fn enforce_register_owner_skips_when_no_auth() {
        // No AuthMiddleware (unit tests): defer to the middleware, which in
        // production rejects anonymous requests before the handler runs.
        assert!(enforce_register_owner(None, "client-1", Some("anyone")).is_ok());
        assert!(enforce_register_owner(None, "client-1", None).is_ok());
    }

    #[test]
    fn enforce_register_owner_bootstrap_allows_any_owner() {
        let bootstrap = auth_context(None, true);
        assert!(enforce_register_owner(Some(&bootstrap), "client-1", None).is_ok());
        assert!(enforce_register_owner(Some(&bootstrap), "client-1", Some("bob")).is_ok());
    }

    #[test]
    fn enforce_register_owner_user_token_is_rejected() {
        // Phase 3: user tokens (Phase 2 personal API tokens) are no longer
        // allowed on agent transport endpoints. Only bootstrap or agent tokens
        // may register.
        let alice = auth_context(Some("alice"), false);
        let err = enforce_register_owner(Some(&alice), "client-1", Some("alice")).unwrap_err();
        assert!(err.contains("user tokens are not allowed"), "got: {}", err);
    }

    #[test]
    fn enforce_register_owner_agent_token_matching_client_id_succeeds() {
        let alice = agent_auth_context(
            "alice",
            "alice-laptop",
            vec![
                "agent:register",
                "agent:poll",
                "agent:result",
                "agent:job_update",
            ],
        );
        // Matching client_id + matching owner -> Ok.
        assert!(enforce_register_owner(Some(&alice), "alice-laptop", Some("alice")).is_ok());
        // Matching client_id + missing owner -> Ok (owner filled in by the
        // caller via effective_register_owner).
        assert!(enforce_register_owner(Some(&alice), "alice-laptop", None).is_ok());
    }

    #[test]
    fn enforce_register_owner_agent_token_wrong_client_id_rejected() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        let err = enforce_register_owner(Some(&alice), "other-laptop", None).unwrap_err();
        assert!(err.contains("not bound to client_id"), "got: {}", err);
    }

    #[test]
    fn enforce_register_owner_agent_token_owner_mismatch_rejected() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        let err = enforce_register_owner(Some(&alice), "alice-laptop", Some("bob")).unwrap_err();
        assert!(err.contains("agent token owner is 'alice'"), "got: {}", err);
        assert!(err.contains("bob"), "got: {}", err);
    }

    #[test]
    fn effective_register_owner_agent_token_fills_username() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        // Missing owner -> filled with the token's username.
        assert_eq!(
            effective_register_owner(Some(&alice), None),
            Some("alice".to_string())
        );
        // Matching owner preserved.
        assert_eq!(
            effective_register_owner(Some(&alice), Some("alice")),
            Some("alice".to_string())
        );
        // Bootstrap keeps the request owner.
        let bootstrap = auth_context(None, true);
        assert_eq!(
            effective_register_owner(Some(&bootstrap), Some("bob")),
            Some("bob".to_string())
        );
    }

    #[test]
    fn enforce_agent_transport_rejects_user_token() {
        let alice = auth_context(Some("alice"), false);
        let err = enforce_agent_transport(Some(&alice), "client-1").unwrap_err();
        assert!(err.contains("user tokens are not allowed"), "got: {}", err);
    }

    #[test]
    fn enforce_agent_transport_agent_token_matching_client_succeeds() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
        assert!(enforce_agent_transport(Some(&alice), "alice-laptop").is_ok());
        let err = enforce_agent_transport(Some(&alice), "other").unwrap_err();
        assert!(err.contains("not bound"), "got: {}", err);
    }

    #[test]
    fn enforce_agent_transport_bootstrap_succeeds() {
        let bootstrap = auth_context(None, true);
        assert!(enforce_agent_transport(Some(&bootstrap), "any-client").is_ok());
    }

    #[test]
    fn require_agent_transport_scope_agent_token_with_scope_succeeds() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
        assert!(require_agent_transport_scope(Some(&alice), "agent:poll").is_ok());
        assert!(require_agent_transport_scope(Some(&alice), "agent:register").is_err());
    }

    #[test]
    fn require_agent_transport_scope_bootstrap_always_succeeds() {
        let bootstrap = auth_context(None, true);
        assert!(require_agent_transport_scope(Some(&bootstrap), "agent:register").is_ok());
    }

    #[test]
    fn require_agent_transport_scope_user_token_rejected() {
        let alice = auth_context(Some("alice"), false);
        let err = require_agent_transport_scope(Some(&alice), "agent:register").unwrap_err();
        assert!(err.contains("missing required scope"), "got: {}", err);
    }

    #[test]
    fn oauth_bridge_token_remains_blocked_from_agent_transport() {
        let bridge = oauth_bridge_auth_context(
            "hash-a",
            vec![
                "agent:register",
                "agent:poll",
                "agent:result",
                "agent:job_update",
            ],
        );
        assert!(!bridge.is_lightweight());
        assert!(enforce_agent_transport(Some(&bridge), "client-a")
            .unwrap_err()
            .contains("user tokens are not allowed"));
        assert!(
            require_agent_transport_scope(Some(&bridge), "agent:register")
                .unwrap_err()
                .contains("missing required scope")
        );
    }

    #[tokio::test]
    async fn registry_rejects_enqueue_when_queue_full() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "full".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        // Fill the queue to the limit without any consumer draining it.
        for _ in 0..MAX_QUEUED_REQUESTS_PER_CLIENT {
            registry
                .enqueue_run(
                    ShellRunRequest {
                        client_id: "full".to_string(),
                        cwd: None,
                        command: "echo hi".to_string(),
                        stdin: None,
                        timeout_secs: 5,
                        wait_timeout_secs: 0,
                    },
                    "tester".to_string(),
                )
                .await
                .unwrap();
        }
        // The next enqueue must be rejected with a structured error instead
        // of growing the queue unboundedly.
        let err = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "full".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("too many pending requests"));
        assert!(err.contains("full"));
        // The queue is exactly at the cap; memory is bounded.
        let view = registry.get_client_view("full").await.unwrap();
        assert_eq!(view.pending_requests, MAX_QUEUED_REQUESTS_PER_CLIENT);
    }

    #[tokio::test]
    async fn reconcile_disconnect_marks_running_jobs_lost() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
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
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        // Job is "queued" with its request sitting in the client's queue.
        let before = registry.get_client_view("oe").await.unwrap();
        assert_eq!(before.pending_requests, 1);
        // Transport disconnects (e.g. WebSocket dropped).
        registry.reconcile_disconnect("oe", "inst").await;
        let lost = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(lost.status, "lost");
        assert!(lost.error.unwrap().contains("disconnected"));
        // Pending request was dropped: no dangling waiter / queue entry.
        let after = registry.get_client_view("oe").await.unwrap();
        assert_eq!(after.pending_requests, 0);
    }

    #[tokio::test]
    async fn reconcile_disconnect_releases_active_lease_immediately() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;

        registry.reconcile_disconnect("oe", "inst-a").await;

        let offline = registry.get_client_view("oe").await.unwrap();
        assert!(
            !offline.connected,
            "active disconnect must immediately leave online window"
        );
        assert!(now_ts().saturating_sub(offline.last_seen) > CLIENT_ONLINE_WINDOW_SECS);

        let new_view = register_with_instance(&registry, "oe", "inst-b").await;
        assert_eq!(new_view.agent_instance_id, "inst-b");
        assert!(
            new_view.connected,
            "new instance should register without waiting 60 seconds"
        );
    }

    // ------------------------------------------------------------------------
    // Agent instance identity / lease model (Phase 1)
    // ------------------------------------------------------------------------

    /// Helper: register a client with an explicit `agent_instance_id`.
    async fn register_with_instance(
        registry: &ShellClientRegistry,
        client_id: &str,
        instance: &str,
    ) -> ShellClientView {
        registry
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: instance.to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn lease_first_register_accepts_instance() {
        let registry = ShellClientRegistry::default();
        let view = register_with_instance(&registry, "oe", "inst-a").await;
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);
        // The view/list path exposes the instance id.
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_same_instance_reregister_accepts() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Same client_id + same instance id is a reconnect/refresh: accepted.
        let _ = register_with_instance(&registry, "oe", "inst-a").await;
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn lease_different_online_instance_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // A second process with the same client_id but a different instance
        // must be rejected while the first is online.
        let err = registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap_err();
        assert!(err.contains("already online"), "error was: {err}");
        assert!(err.contains("different instance"), "error was: {err}");
        // The active instance is unchanged.
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_stale_replaced_by_different_instance_accepts() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Age the first instance past the online window so it reads as stale.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        // A different instance may now take over the lease.
        let _ = register_with_instance(&registry, "oe", "inst-b").await;
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-b");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn lease_stale_instance_poll_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Replace with a newer instance after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A can no longer poll.
        let err = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                projects: None,
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can still poll.
        registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                projects: None,
            })
            .await
            .expect("active instance must poll");
    }

    #[tokio::test]
    async fn lease_stale_instance_result_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Enqueue a request and let instance A poll it.
        let (request_id, _rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "oe".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();

        // Replace instance A with B after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A cannot submit the result.
        let err = registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can submit the result.
        registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .expect("active instance must submit result");
    }

    #[tokio::test]
    async fn lease_stale_instance_job_update_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
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
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Replace instance A with B after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A cannot update the job.
        let err = registry
            .update_job(ShellAgentJobUpdateRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                job_id: job.job_id.clone(),
                request_id: None,
                status: "running".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                finished: false,
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can update the job.
        registry
            .update_job(ShellAgentJobUpdateRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                job_id: job.job_id.clone(),
                request_id: None,
                status: "running".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                finished: false,
            })
            .await
            .expect("active instance must update job");
    }

    #[tokio::test]
    async fn lease_list_clients_exposes_instance_id() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].agent_instance_id, "inst-a");
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_reconcile_disconnect_stale_instance_is_noop() {
        // A stale instance disconnecting after a newer instance has taken over
        // must NOT clear the active notifier or mark the active instance's
        // jobs lost.
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Install a notifier for instance A.
        let notify_a = Arc::new(Notify::new());
        registry
            .register_notifier("oe", "inst-a", notify_a.clone())
            .await
            .unwrap();
        // Start a job under instance A.
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
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Age out A and let B take over.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;
        // B installs its own notifier.
        let notify_b = Arc::new(Notify::new());
        registry
            .register_notifier("oe", "inst-b", notify_b.clone())
            .await
            .unwrap();

        // A's transport finally disconnects. This must be a no-op: B's notifier
        // stays and B's job is not marked lost.
        registry.reconcile_disconnect("oe", "inst-a").await;
        let job_view = registry.get_job(&job.job_id).await.unwrap();
        assert_ne!(
            job_view.status, "lost",
            "stale disconnect must not mark active instance job lost"
        );
        // B's disconnect, however, does reconcile.
        registry.reconcile_disconnect("oe", "inst-b").await;
        let job_view = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(job_view.status, "lost");
    }

    #[tokio::test]
    async fn lease_register_notifier_rejects_stale_instance() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Replace A with B.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;
        // A's late notifier registration must be rejected so it cannot
        // overwrite B's notifier.
        let err = registry
            .register_notifier("oe", "inst-a", Arc::new(Notify::new()))
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );
        // B can still install its notifier.
        registry
            .register_notifier("oe", "inst-b", Arc::new(Notify::new()))
            .await
            .expect("active instance must install notifier");
    }

    #[tokio::test]
    async fn lease_register_rejects_empty_instance_id() {
        let registry = ShellClientRegistry::default();
        let err = registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap_err();
        assert!(err.contains("agent_instance_id"), "error was: {err}");
    }
}
