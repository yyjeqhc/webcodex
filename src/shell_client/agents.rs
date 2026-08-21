use super::auth::{assert_shell_client_access, shell_client_visible_to_auth, ShellClientAuthGroup};
use super::jobs::{begin_job_recovery, is_final_job_status, mark_job_lost, offline_last_seen};
use super::reconciliation::{
    preflight_inventory_locked, reconcile_inventory_locked, terminate_instance_jobs_locked,
    validate_job_inventory,
};
use super::requests::resolve_disconnected_sync_requests_locked;
use super::state::{NotifierEntry, ShellClientRecord, ShellClientRegistryInner};
use super::validation::{
    normalize_project_summaries, normalize_tool_providers, trim_string, validate_agent_instance_id,
    validate_id, validate_optional_field, validate_project_summary_count,
};
use super::{
    now_ts, ShellClientRegistry, CLIENT_ONLINE_WINDOW_SECS, MAX_RETIRED_INSTANCES_PER_CLIENT,
    TRANSPORT_POLLING,
};
use crate::shell_protocol::{
    ShellClientCapabilities, ShellClientRegisterRequest, ShellClientView,
    JOB_INVENTORY_MAX_ACTIVE_JOBS,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::Notify;

impl ShellClientRegistry {
    #[cfg(test)]
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
        self.register_with_auth_connection(body, auth, None).await
    }

    pub(crate) async fn register_with_auth_connection(
        &self,
        body: ShellClientRegisterRequest,
        auth: Option<&crate::auth::AuthContext>,
        connection_id: Option<&str>,
    ) -> Result<ShellClientView, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        validate_optional_field(&body.display_name, "display_name")?;
        validate_optional_field(&body.owner, "owner")?;
        validate_optional_field(&body.hostname, "hostname")?;
        validate_project_summary_count(body.projects.as_deref())?;
        if body
            .job_concurrency_limit
            .is_some_and(|limit| !(1..=JOB_INVENTORY_MAX_ACTIVE_JOBS).contains(&limit))
        {
            return Err(format!(
                "job_concurrency_limit must be between 1 and {JOB_INVENTORY_MAX_ACTIVE_JOBS}"
            ));
        }

        let client_id = body.client_id.trim().to_string();
        let agent_instance_id = body.agent_instance_id.trim().to_string();
        let capabilities = body.capabilities.clone().unwrap_or_default();
        let job_inventory = body.job_inventory.clone();
        let host_context = body
            .host_context
            .clone()
            .map(crate::shell_protocol::AgentHostContext::normalized)
            .transpose()?;
        let mut policy = body.policy;
        if let Some(policy) = policy.as_mut() {
            policy.tool_providers = normalize_tool_providers(policy.tool_providers.take());
        }
        let now = now_ts();
        let record = ShellClientRecord {
            client_id: client_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            display_name: trim_string(body.display_name),
            owner: trim_string(body.owner),
            hostname: trim_string(body.hostname),
            host_context,
            capabilities: capabilities.clone(),
            projects: normalize_project_summaries(body.projects),
            last_seen: now,
            agent_protocol_version: body
                .agent_protocol_version
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            transport: TRANSPORT_POLLING.to_string(),
            policy,
            auth_group: auth.and_then(ShellClientAuthGroup::from_auth),
            registered_at: now,
            connected_at: now,
            connection_id: connection_id.map(str::to_string),
            disconnected_at: None,
            process_started_at: body.process_started_at,
            build: body.build,
            job_concurrency_limit: body.job_concurrency_limit,
            projected_structured_terminal_suppressions: VecDeque::new(),
        };
        match (
            capabilities.job_state_reconciliation,
            job_inventory.as_ref(),
        ) {
            (true, Some(inventory)) => {
                validate_job_inventory(&client_id, &record.projects, inventory)?;
            }
            (true, None) => {
                return Err(
                    "job_state_reconciliation capability requires job_inventory".to_string()
                );
            }
            (false, Some(_)) => {
                return Err(
                    "job_inventory requires job_state_reconciliation capability".to_string()
                );
            }
            (false, None) => {}
        }
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now);

        if inner
            .clients
            .get(&client_id)
            .is_some_and(|existing| existing.auth_group != record.auth_group)
        {
            return Err("agent client identity is unavailable".to_string());
        }
        if let Some(ShellClientAuthGroup::SharedKey(group)) = record.auth_group.as_ref() {
            let is_new_client = !inner.clients.contains_key(&client_id);
            if is_new_client {
                let group_count = inner
                    .clients
                    .values()
                    .filter(|client| {
                        matches!(
                            client.auth_group.as_ref(),
                            Some(ShellClientAuthGroup::SharedKey(existing_group))
                                if existing_group == group
                        )
                    })
                    .count();
                if group_count >= self.shared_key_limits.per_group {
                    return Err(format!(
                        "shared-key runner group limit reached (maximum {} runners)",
                        self.shared_key_limits.per_group
                    ));
                }
                let global_count = inner
                    .clients
                    .values()
                    .filter(|client| {
                        matches!(client.auth_group, Some(ShellClientAuthGroup::SharedKey(_)))
                    })
                    .count();
                if global_count >= self.shared_key_limits.global {
                    return Err(format!(
                        "shared-key runner global limit reached (maximum {} runners)",
                        self.shared_key_limits.global
                    ));
                }
            }
        }
        if inner
            .retired_instances
            .get(&client_id)
            .is_some_and(|retired| retired.iter().any(|id| id == &agent_instance_id))
        {
            return Err(format!(
                "agent client {} instance was replaced and cannot reclaim the lease",
                client_id
            ));
        }
        if inner.clients.get(&client_id).is_some_and(|existing| {
            existing.agent_instance_id == agent_instance_id
                && existing.capabilities.job_state_reconciliation
                && !capabilities.job_state_reconciliation
        }) {
            return Err(
                "same runner instance cannot downgrade job_state_reconciliation capability"
                    .to_string(),
            );
        }
        // `agent_instance_id` is the Runner process identity, so a same-process
        // reconnect that stops advertising structured_file_delete is a
        // downgrade of process-lifetime capability: reject it before replacing
        // the current record so a queued structured delete is never handed to a
        // registration that cannot understand it. false -> false, false -> true
        // and true -> true reconnects remain allowed.
        if inner.clients.get(&client_id).is_some_and(|existing| {
            existing.agent_instance_id == agent_instance_id
                && existing.capabilities.structured_file_delete
                && !capabilities.structured_file_delete
        }) {
            return Err(
                "same runner instance cannot downgrade structured_file_delete capability"
                    .to_string(),
            );
        }
        // The dedicated internal-POSIX request kind is also binary support.
        // A same-process reconnect cannot withdraw it while queued requests may
        // still target that exact instance.
        if inner.clients.get(&client_id).is_some_and(|existing| {
            existing.agent_instance_id == agent_instance_id
                && existing.capabilities.internal_posix_script
                && !capabilities.internal_posix_script
        }) {
            return Err(
                "same runner instance cannot downgrade internal_posix_script capability"
                    .to_string(),
            );
        }
        // This optimized export request kind is process-lifetime binary support.
        // Reject same-instance downgrade so a request already admitted for this
        // process is never handed to a registration claiming it cannot decode it.
        if inner.clients.get(&client_id).is_some_and(|existing| {
            existing.agent_instance_id == agent_instance_id
                && existing.capabilities.artifact_export_chunk_read
                && !capabilities.artifact_export_chunk_read
        }) {
            return Err(
                "same runner instance cannot downgrade artifact_export_chunk_read capability"
                    .to_string(),
            );
        }
        if inner.clients.get(&client_id).is_some_and(|existing| {
            existing.agent_instance_id == agent_instance_id
                && existing.capabilities.artifact_export_streaming_metadata
                && !capabilities.artifact_export_streaming_metadata
        }) {
            return Err(
                "same runner instance cannot downgrade artifact_export_streaming_metadata capability"
                    .to_string(),
            );
        }
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
        let replaced_instance_id = replaced_instance.then(|| {
            inner
                .clients
                .get(&client_id)
                .expect("replacement requires existing client")
                .agent_instance_id
                .clone()
        });
        if let Some(inventory) = job_inventory.as_ref() {
            preflight_inventory_locked(&inner, &client_id, &agent_instance_id, inventory)?;
        }
        if replaced_instance {
            let replaced_instance_id = replaced_instance_id
                .as_deref()
                .expect("replacement id captured");
            terminate_instance_jobs_locked(
                &mut inner,
                &client_id,
                replaced_instance_id,
                job_inventory.as_ref(),
                now,
            );
            // A different `agent_instance_id` is a new Runner process; pending
            // synchronous requests were admitted for the process that existed
            // when they were queued, so they must not be inherited by or
            // dispatched to the replacement. Fail and drain them here, under
            // the same lock and before the new lease is installed, preserving
            // `request_dispatched` truth. Job-backed requests are governed by
            // `terminate_instance_jobs_locked` above and are never drained as
            // synchronous requests.
            resolve_disconnected_sync_requests_locked(
                &mut inner,
                &client_id,
                "runner instance was replaced before the request completed; the request was not handed to the replacement instance",
            );
            let retired = inner
                .retired_instances
                .entry(client_id.clone())
                .or_default();
            if !retired.iter().any(|id| id == replaced_instance_id) {
                retired.push_back(replaced_instance_id.to_string());
                while retired.len() > MAX_RETIRED_INSTANCES_PER_CLIENT {
                    retired.pop_front();
                }
            }
        }

        // Same-instance re-register is a transport reconnect: keep the
        // original registration time; `connected_at` reflects the new
        // connection. A new instance starts a fresh lifecycle.
        let mut record = record;
        if let Some(existing) = inner.clients.get(&client_id) {
            if existing.agent_instance_id == agent_instance_id {
                record.registered_at = existing.registered_at;
                record.projected_structured_terminal_suppressions =
                    existing.projected_structured_terminal_suppressions.clone();
                record.prune_projected_structured_terminal_suppressions(now);
            }
        }

        // Every successful registration replaces the concrete transport
        // lease, including a same-instance reconnect. Removing the previous
        // notifier here ensures an older connection cannot keep receiving new
        // requests while its eventual disconnect races the new connection.
        inner.notifiers.remove(&client_id);
        inner.clients.insert(client_id.clone(), record);
        if let Some(inventory) = job_inventory.as_ref() {
            let auth_group = inner
                .clients
                .get(&client_id)
                .and_then(|client| client.auth_group.clone());
            let reconciliation = reconcile_inventory_locked(
                &mut inner,
                &client_id,
                &agent_instance_id,
                auth_group,
                self.observation_epoch.clone(),
                inventory,
                now,
            );
            let process_started_at = inner
                .clients
                .get(&client_id)
                .and_then(|client| client.process_started_at);
            tracing::info!(
                target: "webcodex::job_reconciliation",
                client_id = %client_id,
                agent_instance_id = %agent_instance_id,
                process_started_at = ?process_started_at,
                inventory_active = reconciliation.inventory_active,
                inventory_terminal = reconciliation.inventory_terminal,
                reconstructed = reconciliation.reconstructed,
                updated = reconciliation.updated,
                missing = reconciliation.missing,
                suppressed_terminal = reconciliation.suppressed_terminal,
                "runner job inventory reconciled"
            );
        }
        Ok(Self::client_view_locked(&inner, &client_id).expect("client just inserted"))
    }

    /// Override the transport label for a registered client. Called by the
    /// WebSocket handler after a successful register so observability and
    /// `list_agents` reflect how the agent is actually connected. Polling
    /// agents keep the default `"polling"` label set during `register`.
    #[cfg(test)]
    pub async fn set_transport(&self, client_id: &str, transport: &str) -> Result<(), String> {
        self.set_transport_checked(client_id, None, None, transport)
            .await
    }

    pub(crate) async fn set_transport_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
        transport: &str,
    ) -> Result<(), String> {
        self.set_transport_checked(
            client_id,
            Some(agent_instance_id),
            Some(connection_id),
            transport,
        )
        .await
    }

    async fn set_transport_checked(
        &self,
        client_id: &str,
        agent_instance_id: Option<&str>,
        connection_id: Option<&str>,
        transport: &str,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if agent_instance_id.is_some_and(|id| client.agent_instance_id != id)
            || connection_id.is_some_and(|id| client.connection_id.as_deref() != Some(id))
        {
            return Err(format!(
                "agent client {} transport connection is no longer active",
                client_id
            ));
        }
        client.transport = transport.to_string();
        Ok(())
    }

    /// Refresh `last_seen` for a registered client to "now" without performing
    /// any business operation. Used by keepalive traffic to keep active
    /// long-lived transports inside the online window. Polling agents have no
    /// server-internal connection lease and use this path directly; long-lived
    /// transports use [`Self::touch_client_for_connection`] instead so a stale
    /// same-instance connection cannot revive the new lease.
    #[cfg(test)]
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

    /// Connection-scoped keepalive refresh for long-lived transports
    /// (WebSocket/QUIC Ping/Pong). Validates `client_id`,
    /// `agent_instance_id`, and the current `connection_id` under the same
    /// registry mutex that owns `last_seen`. A delayed Ping/Pong from a stale
    /// same-instance connection (replaced by a reconnect) returns a stable
    /// error and must not refresh the new connection's `last_seen` or revive
    /// an already-disconnected client. Polling keepalive keeps using
    /// [`ShellClientRegistry::touch_client`].
    pub(crate) async fn touch_client_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
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
        if client.connection_id.as_deref() != Some(connection_id) {
            return Err(format!(
                "agent client {} transport connection is no longer active",
                client_id
            ));
        }
        client.last_seen = now_ts();
        Ok(())
    }

    /// Apply sanitized provider metadata to the active agent record. Optional
    /// metadata is best-effort: malformed/unknown state is ignored by the
    /// normalizer and never changes transport or tool completion semantics.
    /// Polling-transport `RuntimeMetadata` uses this path.
    pub async fn update_tool_providers(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        status: Option<crate::shell_protocol::ToolProvidersStatus>,
    ) -> Result<(), String> {
        self.update_tool_providers_checked(client_id, agent_instance_id, None, status)
            .await
    }

    /// Connection-scoped `RuntimeMetadata` for long-lived transports. A stale
    /// same-instance connection must not overwrite the current connection's
    /// provider metadata or refresh its liveness: when the connection no
    /// longer holds the lease the update is rejected with a stable error.
    pub(crate) async fn update_tool_providers_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
        status: Option<crate::shell_protocol::ToolProvidersStatus>,
    ) -> Result<(), String> {
        self.update_tool_providers_checked(
            client_id,
            agent_instance_id,
            Some(connection_id),
            status,
        )
        .await
    }

    async fn update_tool_providers_checked(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        expected_connection_id: Option<&str>,
        status: Option<crate::shell_protocol::ToolProvidersStatus>,
    ) -> Result<(), String> {
        let Some(status) = normalize_tool_providers(status) else {
            return Ok(());
        };
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
        if let Some(expected) = expected_connection_id {
            if client.connection_id.as_deref() != Some(expected) {
                return Err(format!(
                    "agent client {} transport connection is no longer active",
                    client_id
                ));
            }
        }
        if let Some(policy) = client.policy.as_mut() {
            policy.tool_providers = Some(status);
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

    pub(super) fn prune_expired_shared_key_clients_locked(
        &self,
        inner: &mut ShellClientRegistryInner,
        now: i64,
    ) {
        let expired = inner
            .clients
            .iter()
            .filter_map(|(client_id, client)| {
                if !matches!(client.auth_group, Some(ShellClientAuthGroup::SharedKey(_))) {
                    return None;
                }
                let transport_connected = inner.notifiers.contains_key(client_id);
                let recently_seen =
                    now.saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS;
                if transport_connected || recently_seen {
                    return None;
                }
                let offline_since = client.disconnected_at.unwrap_or(client.last_seen);
                (now.saturating_sub(offline_since) > self.shared_key_limits.offline_ttl_secs)
                    .then(|| client_id.clone())
            })
            .collect::<Vec<_>>();
        if expired.is_empty() {
            return;
        }

        let expired_set = expired.iter().cloned().collect::<HashSet<_>>();
        let request_ids = inner
            .pending_by_id
            .iter()
            .filter(|(_, pending)| expired_set.contains(&pending.request.client_id))
            .map(|(request_id, _)| request_id.clone())
            .collect::<HashSet<_>>();
        let job_ids = inner
            .jobs_by_id
            .iter()
            .filter(|(_, job)| expired_set.contains(&job.client_id))
            .map(|(job_id, _)| job_id.clone())
            .collect::<HashSet<_>>();

        for client_id in &expired {
            resolve_disconnected_sync_requests_locked(
                inner,
                client_id,
                "agent went offline: shared-key runner registration expired",
            );
        }
        for job_id in &job_ids {
            if let Some(job) = inner.jobs_by_id.get_mut(job_id) {
                mark_job_lost(
                    job,
                    now,
                    "shared_key_runner_expired",
                    "shared-key runner registration expired after being offline",
                );
            }
        }
        for request_id in &request_ids {
            inner.pending_by_id.remove(request_id);
            inner.persistent_waiters.remove(request_id);
        }
        inner.request_to_job.retain(|request_id, job_id| {
            !request_ids.contains(request_id) && !job_ids.contains(job_id)
        });
        for client_id in &expired {
            inner.clients.remove(client_id);
            inner.queues_by_client.remove(client_id);
            inner.notifiers.remove(client_id);
            inner.retired_instances.remove(client_id);
            let project_prefix = format!("agent:{client_id}:");
            inner
                .unregistering_projects
                .retain(|project_id, _| !project_id.starts_with(&project_prefix));
        }
        self.cleanup_intents
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|job_id, _| !job_ids.contains(job_id));
    }

    pub(super) async fn prune_expired_shared_key_clients(&self) {
        let now = now_ts();
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now);
    }

    /// Register a push notifier for a client. The WebSocket handler calls
    /// this after register; the server's request pump waits on the notifier
    /// between polls. Calling this replaces any previously registered
    /// notifier for the client (e.g. after a reconnect).
    #[cfg(test)]
    pub async fn register_notifier(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        notify: Arc<Notify>,
    ) -> Result<(), String> {
        self.register_notifier_checked(client_id, agent_instance_id, None, notify)
            .await
    }

    pub(crate) async fn register_notifier_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
        notify: Arc<Notify>,
    ) -> Result<(), String> {
        self.register_notifier_checked(client_id, agent_instance_id, Some(connection_id), notify)
            .await
    }

    async fn register_notifier_checked(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: Option<&str>,
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
        if connection_id.is_some_and(|id| client.connection_id.as_deref() != Some(id)) {
            return Err(format!(
                "agent client {} transport connection is no longer active",
                client_id
            ));
        }
        inner.notifiers.insert(
            client_id.to_string(),
            NotifierEntry {
                notify,
                agent_instance_id: agent_instance_id.to_string(),
                connection_id: connection_id.map(str::to_string),
            },
        );
        Ok(())
    }

    /// Reconcile state after an agent transport disconnects or sends a
    /// graceful offline notice.
    #[cfg(test)]
    pub async fn reconcile_disconnect(&self, client_id: &str, agent_instance_id: &str) {
        self.reconcile_disconnect_checked(client_id, agent_instance_id, None)
            .await;
    }

    pub(crate) async fn reconcile_disconnect_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
    ) {
        self.reconcile_disconnect_checked(client_id, agent_instance_id, Some(connection_id))
            .await;
    }

    async fn reconcile_disconnect_checked(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: Option<&str>,
    ) {
        let mut inner = self.inner.lock().await;
        let is_active = inner
            .clients
            .get(client_id)
            .map(|client| {
                client.agent_instance_id == agent_instance_id
                    && connection_id
                        .map(|id| client.connection_id.as_deref() == Some(id))
                        .unwrap_or(true)
            })
            .unwrap_or(false);
        if !is_active {
            return;
        }
        if inner
            .notifiers
            .get(client_id)
            .map(|entry| {
                entry.agent_instance_id == agent_instance_id
                    && connection_id
                        .map(|id| entry.connection_id.as_deref() == Some(id))
                        .unwrap_or(true)
            })
            .unwrap_or(false)
        {
            inner.notifiers.remove(client_id);
        }
        let now = now_ts();
        let recoverable = inner
            .clients
            .get(client_id)
            .is_some_and(|client| client.capabilities.job_state_reconciliation);
        if let Some(client) = inner.clients.get_mut(client_id) {
            client.last_seen = offline_last_seen(now);
            client.disconnected_at = Some(now);
        }
        let affected_job_ids: Vec<String> = inner
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
        for job_id in affected_job_ids {
            let request_id = inner
                .jobs_by_id
                .get(&job_id)
                .and_then(|j| j.request_id.clone());
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                if recoverable && job.status != "queued" {
                    begin_job_recovery(job, now, "runner_transport_disconnected");
                } else {
                    let (reason, message) = if job.status == "queued" {
                        (
                            "runner_request_not_dispatched",
                            "agent transport disconnected before the queued request was dispatched",
                        )
                    } else {
                        ("legacy_runner_disconnected", "agent transport disconnected")
                    };
                    mark_job_lost(job, now, reason, message);
                }
            }
            if let Some(request_id) = request_id {
                inner.pending_by_id.remove(&request_id);
                inner.request_to_job.remove(&request_id);
                if let Some(queue) = inner.queues_by_client.get_mut(client_id) {
                    queue.retain(|id| id != &request_id);
                }
            }
        }
        // Drop any additional job-backed control request (for example an
        // undispatched stop request). Reconciliation restores executor truth;
        // it never replays a stale server queue after reconnect.
        let extra_job_requests = inner
            .pending_by_id
            .iter()
            .filter(|(_, pending)| {
                pending.request.client_id == client_id && pending.job_id.is_some()
            })
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();
        for request_id in extra_job_requests {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
            if let Some(queue) = inner.queues_by_client.get_mut(client_id) {
                queue.retain(|id| id != &request_id);
            }
        }
        // Synchronous tool requests (run_shell/read_file/write/lsp/project ops)
        // carry a live oneshot waiter but no job_id, so the job loop above skips
        // them. Fail them fast here; otherwise the calling tool blocks until its
        // own wait timeout (tens of seconds) after the agent has already gone,
        // which surfaces as an unresponsive MCP `tools/call`.
        resolve_disconnected_sync_requests_locked(
            &mut inner,
            client_id,
            "agent went offline: transport disconnected before returning a result",
        );
    }

    #[cfg(test)]
    pub async fn list_clients(&self) -> Vec<ShellClientView> {
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
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
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
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

    pub(crate) async fn has_connected_shared_key_group(&self, shared_key_hash: &str) -> bool {
        let now = now_ts();
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now);
        inner.clients.values().any(|client| {
            matches!(
                client.auth_group.as_ref(),
                Some(ShellClientAuthGroup::SharedKey(group)) if group == shared_key_hash
            ) && now.saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS
        })
    }

    /// Return capability snapshots for the currently-online Runners in one exact
    /// shared-key authorization group. Callers must evaluate each snapshot as a
    /// whole; combining capabilities across different Runners would overstate
    /// executable authority.
    pub(crate) async fn connected_shared_key_group_capabilities(
        &self,
        shared_key_hash: &str,
    ) -> Vec<ShellClientCapabilities> {
        let now = now_ts();
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now);
        inner
            .clients
            .values()
            .filter(|client| {
                matches!(
                    client.auth_group.as_ref(),
                    Some(ShellClientAuthGroup::SharedKey(group)) if group == shared_key_hash
                ) && now.saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS
            })
            .map(|client| client.capabilities.clone())
            .collect()
    }

    pub async fn get_client_view(&self, client_id: &str) -> Option<ShellClientView> {
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
        Self::client_view_locked(&inner, client_id)
    }

    pub(crate) async fn get_client_view_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
    ) -> Option<ShellClientView> {
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
        let client = inner.clients.get(client_id)?;
        if client.agent_instance_id != agent_instance_id
            || client.connection_id.as_deref() != Some(connection_id)
        {
            return None;
        }
        Self::client_view_locked(&inner, client_id)
    }

    pub(crate) async fn get_client_view_for_auth(
        &self,
        client_id: &str,
        auth: Option<&crate::auth::AuthContext>,
    ) -> Option<ShellClientView> {
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
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
        let mut inner = self.inner.lock().await;
        self.prune_expired_shared_key_clients_locked(&mut inner, now_ts());
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
            host_context: client.host_context.clone(),
            status: if connected { "online" } else { "stale" }.to_string(),
            connected,
            last_seen: client.last_seen,
            capabilities: client.capabilities.clone(),
            pending_requests,
            projects: client.projects.clone(),
            agent_protocol_version: client.agent_protocol_version.clone(),
            transport: client.transport.clone(),
            policy: client.policy.clone(),
            registered_at: client.registered_at,
            connected_at: client.connected_at,
            disconnected_at: client.disconnected_at,
            process_started_at: client.process_started_at,
            build: client.build.clone(),
            job_concurrency_limit: client.job_concurrency_limit,
        })
    }
}
