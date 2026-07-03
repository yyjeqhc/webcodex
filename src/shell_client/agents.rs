use super::auth::{assert_shell_client_access, shell_client_visible_to_auth, ShellClientAuthGroup};
use super::jobs::{is_final_job_status, offline_last_seen};
use super::state::{NotifierEntry, ShellClientRecord, ShellClientRegistryInner};
use super::validation::{
    normalize_project_summaries, trim_string, validate_agent_instance_id, validate_id,
    validate_optional_field,
};
use super::{now_ts, ShellClientRegistry, CLIENT_ONLINE_WINDOW_SECS, TRANSPORT_POLLING};
use crate::shell_protocol::{ShellClientRegisterRequest, ShellClientView};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Notify;

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
        // time.
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
    /// any business operation. Used by keepalive traffic to keep active
    /// long-lived transports inside the online window.
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
    /// graceful offline notice.
    pub async fn reconcile_disconnect(&self, client_id: &str, agent_instance_id: &str) {
        let mut inner = self.inner.lock().await;
        let is_active = inner
            .clients
            .get(client_id)
            .map(|client| client.agent_instance_id == agent_instance_id)
            .unwrap_or(false);
        if !is_active {
            return;
        }
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
