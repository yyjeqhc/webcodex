//! Runtime observability metadata injected into `ToolRuntime`.

use super::helpers::normalize_local_status;
use super::jobs::local_jobs_visible_to_auth;
use super::local_jobs::ACTIVE_JOB_STATUSES;
use super::registry::registered_tool_specs;
use super::{permissions, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::{ShellClientView, ShellJobInfo};
use serde_json::{json, Value};
use std::path::PathBuf;

const RUNNING_JOB_STATUSES: &[&str] = &["running", "started"];
const AGENT_QUEUED_JOB_STATUSES: &[&str] = &["queued", "agent_queued"];
const LOCAL_QUEUED_JOB_STATUSES: &[&str] = &["queued"];
const LIST_AGENTS_MAX_CLIENT_IDS: usize = 8;
const TARGET_CLIENT_ID_MAX_CHARS: usize = 128;

#[derive(Debug, Default)]
pub(crate) struct ListAgentsOptions {
    pub(crate) client_id: Option<String>,
    pub(crate) client_ids: Option<Vec<String>>,
    pub(crate) include_projects: Option<bool>,
    pub(crate) summary_only: bool,
}

/// Lightweight runtime metadata injected into `ToolRuntime` so observability
/// tools (e.g. `runtime_status`) can report auth/public-url state without the
/// runtime holding a full `Config` (which would couple it to HTTP/fs details).
///
/// `configured_public_url` is `None` when `WEBCODEX_PUBLIC_URL` is unset; the
/// observability output reports this as `null` so a deployer can immediately
/// see that the public URL has not been configured.
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub auth_enabled: bool,
    pub configured_public_url: Option<String>,
    pub quic: Option<std::sync::Arc<std::sync::Mutex<crate::config::QuicRuntimeStatus>>>,
}

impl RuntimeInfo {
    /// Build `RuntimeInfo` from the process environment. Reads
    /// `WEBCODEX_TOKEN` (presence) and `WEBCODEX_PUBLIC_URL`.
    #[cfg(test)]
    pub fn from_env() -> Self {
        Self::from_env_with_quic_config(&crate::config::QuicServerConfig::from_env())
    }

    pub fn from_env_with_quic_config(quic_cfg: &crate::config::QuicServerConfig) -> Self {
        let auth_enabled = std::env::var("WEBCODEX_TOKEN")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let configured_public_url = std::env::var("WEBCODEX_PUBLIC_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        Self {
            auth_enabled,
            configured_public_url,
            quic: Some(std::sync::Arc::new(std::sync::Mutex::new(
                quic_cfg.runtime_status(),
            ))),
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn list_agents(&self, auth: Option<&AuthContext>) -> ToolResult {
        self.list_agents_with_options(auth, ListAgentsOptions::default())
            .await
    }

    pub(crate) async fn list_agents_with_options(
        &self,
        auth: Option<&AuthContext>,
        options: ListAgentsOptions,
    ) -> ToolResult {
        if options.client_id.is_some() && options.client_ids.is_some() {
            return ToolResult::err_with_output(
                "invalid_client_filter: client_id and client_ids are mutually exclusive"
                    .to_string(),
                json!({"error_kind": "invalid_client_filter"}),
            );
        }
        if options
            .client_id
            .as_deref()
            .is_some_and(|value| !valid_target_client_id(value))
        {
            return ToolResult::err_with_output(
                "invalid_client_id: client_id must contain 1..=128 characters".to_string(),
                json!({"error_kind": "invalid_client_id"}),
            );
        }
        if let Some(client_ids) = options.client_ids.as_ref() {
            if client_ids.is_empty() {
                return ToolResult::err_with_output(
                    "invalid_client_ids: at least one client id is required".to_string(),
                    json!({"error_kind": "invalid_client_ids"}),
                );
            }
            if client_ids.len() > LIST_AGENTS_MAX_CLIENT_IDS {
                return ToolResult::err_with_output(
                    format!(
                        "invalid_client_ids: at most {LIST_AGENTS_MAX_CLIENT_IDS} client ids are allowed"
                    ),
                    json!({"error_kind": "invalid_client_ids"}),
                );
            }
            let mut seen = std::collections::HashSet::new();
            for client_id in client_ids {
                if !valid_target_client_id(client_id) {
                    return ToolResult::err_with_output(
                        "invalid_client_ids: every client id must contain 1..=128 characters"
                            .to_string(),
                        json!({"error_kind": "invalid_client_ids"}),
                    );
                }
                if !seen.insert(client_id.as_str()) {
                    return ToolResult::err_with_output(
                        "invalid_client_ids: duplicate client ids are not allowed".to_string(),
                        json!({"error_kind": "invalid_client_ids"}),
                    );
                }
            }
        }

        let mut clients = self.shell_clients.list_clients_for_auth(auth).await;
        clients.sort_by(|a, b| a.client_id.cmp(&b.client_id));
        clients.retain(|client| {
            if let Some(expected) = options.client_id.as_deref() {
                return client.client_id == expected;
            }
            if let Some(expected) = options.client_ids.as_ref() {
                return expected
                    .iter()
                    .any(|client_id| client_id == &client.client_id);
            }
            true
        });
        let mut agent_jobs = self.shell_clients.list_all_jobs_for_auth(auth).await;
        if options.client_id.is_some() || options.client_ids.is_some() {
            agent_jobs.retain(|job| {
                clients
                    .iter()
                    .any(|client| client.client_id == job.client_id)
            });
        }
        let now = chrono::Utc::now().timestamp();
        let include_projects = options.include_projects.unwrap_or(true);
        let agents: Vec<Value> = if options.summary_only {
            clients
                .iter()
                .map(|client| {
                    json!({
                        "client_id": client.client_id,
                        "agent_instance_id": client.agent_instance_id,
                        "display_name": client.display_name,
                        "status": client.status,
                        "connected": client.connected,
                        "agent_protocol_version": client.agent_protocol_version,
                        "transport": client.transport,
                        "last_seen_age_secs": last_seen_age_secs(client, now),
                        "pending_requests": client.pending_requests,
                        "projects_count": enabled_projects_count(client),
                        "active_jobs": active_jobs_for_client(&agent_jobs, &client.client_id),
                        "job_concurrency": job_concurrency_for_client(client, &agent_jobs),
                        "build": client.build,
                    })
                })
                .collect()
        } else {
            clients
                .iter()
                .map(|client| {
                    let mut value = json!({
                        "client_id": client.client_id,
                        "agent_instance_id": client.agent_instance_id,
                        "display_name": client.display_name,
                        "owner": client.owner,
                        "hostname": client.hostname,
                        "host_context": host_context_projection(client.host_context.as_ref()),
                        "status": client.status,
                        "connected": client.connected,
                        "agent_protocol_version": client.agent_protocol_version,
                        "transport": client.transport,
                        "last_seen": client.last_seen,
                        "last_seen_age_secs": last_seen_age_secs(client, now),
                        "pending_requests": client.pending_requests,
                        "projects_count": enabled_projects_count(client),
                        "active_jobs": active_jobs_for_client(&agent_jobs, &client.client_id),
                        "job_concurrency": job_concurrency_for_client(client, &agent_jobs),
                        "capabilities": client.capabilities,
                        "policy": sanitized_policy_summary(client.policy.as_ref()),
                        "shell_profiles": sanitized_shell_profiles_summary(
                            client.policy.as_ref().and_then(|policy| policy.shell_profiles.as_ref())
                        ),
                        "tool_providers": client.policy.as_ref().and_then(|policy| policy.tool_providers.as_ref()),
                    });
                    if include_projects {
                        value["projects"] = json!(client.projects);
                    }
                    value
                })
                .collect()
        };
        if options.summary_only {
            let online = clients.iter().filter(|client| client.connected).count();
            let stale = clients
                .iter()
                .filter(|client| client.status == "stale")
                .count();
            return ToolResult::ok(json!({
                "agents": agents,
                "summary": {
                    "count": clients.len(),
                    "online": online,
                    "offline": clients.len().saturating_sub(online),
                    "stale": stale,
                },
                "count": clients.len(),
            }));
        }
        ToolResult::ok(json!({
            "agents": agents,
            "clients": agent_health_clients(&clients, &agent_jobs, now),
            "summary": agent_health_summary(&clients, &agent_jobs, now),
            "count": clients.len(),
        }))
    }

    /// Build the runtime observability summary. Read-only; never exposes
    /// tokens, api keys, full env, complete project path lists, or
    /// stdout/stderr. Returns a structured JSON object with service metadata,
    /// agent-registered project status, agent client summaries, and job counts.
    pub(crate) async fn runtime_status(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self.shell_clients.list_clients_for_auth(auth).await;

        // -- projects summary -------------------------------------------------
        let agent_registered_count: usize = clients
            .iter()
            .map(|client| {
                client
                    .projects
                    .iter()
                    .filter(|project| !project.disabled)
                    .count()
            })
            .sum();
        let agent_registered_online_count: usize = clients
            .iter()
            .filter(|client| client.connected)
            .map(|client| {
                client
                    .projects
                    .iter()
                    .filter(|project| !project.disabled)
                    .count()
            })
            .sum();
        let effective_count = agent_registered_count;
        let effective_status = if effective_count > 0 {
            "ok"
        } else {
            "no_projects"
        };
        let projects = json!({
            "mode": "agent_registered",
            "agent_registered": {
                "count": agent_registered_count,
                "online_count": agent_registered_online_count,
            },
            "effective": {
                "count": effective_count,
                "status": effective_status,
            },
            "count": effective_count,
        });

        let now = chrono::Utc::now().timestamp();
        let agent_jobs = self.shell_clients.list_all_jobs_for_auth(auth).await;

        // -- agents summary ---------------------------------------------------
        // Build a trimmed client list so the summary never leaks per-request
        // state. Only carry fields useful for observability. `last_seen` is a
        // unix timestamp (seconds) of the most recent heartbeat/result; the
        // console uses it to render how stale an agent is and to make a
        // websocket agent flipping `online` -> `stale` visually obvious.
        let agent_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        // `stale_count` = registered agents whose `last_seen` is older than the
        // online window (status == "stale"). Truly offline agents are removed
        // from the registry on disconnect, so they never appear here.
        let stale_count = agent_count.saturating_sub(online_count);
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "host_context": host_context_projection(c.host_context.as_ref()),
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "last_seen_age_secs": last_seen_age_secs(c, now),
                    "pending_requests": c.pending_requests,
                    "active_jobs": active_jobs_for_client(&agent_jobs, &c.client_id),
                    "job_concurrency": job_concurrency_for_client(c, &agent_jobs),
                    "capabilities": c.capabilities,
                    "projects_count": enabled_projects_count(c),
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                    "tool_providers": c.policy.as_ref().and_then(|p| p.tool_providers.as_ref()),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "clients": clients_summary,
            "summary": agent_health_summary(&clients, &agent_jobs, now),
        });
        let connection_layers = connection_layers(
            &clients,
            agent_registered_count,
            agent_registered_online_count,
            self.observations.as_ref(),
            auth,
            now,
        );
        let version_compatibility = version_compatibility(&clients);

        // -- jobs summary -----------------------------------------------------
        // Agent-known jobs come from the registry; local jobs come from the
        // in-memory map. Active includes same-runner `recovering` jobs.
        let agent_known_count = agent_jobs.len();
        let local_job_dirs: Vec<PathBuf> = if local_jobs_visible_to_auth(auth) {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .values()
                .map(|record| record.dir.clone())
                .collect()
        } else {
            Vec::new()
        };
        let local_known_count = local_job_dirs.len();
        // Avoid double-counting: agent jobs are tracked separately from local
        // jobs (local jobs are only in the in-memory map; agent jobs are only
        // in the registry). Count active across both.
        let agent_active = agent_jobs
            .iter()
            .filter(|j| ACTIVE_JOB_STATUSES.contains(&j.status.as_str()))
            .count();
        let recovering_count = agent_jobs
            .iter()
            .filter(|job| job.status == "recovering")
            .count();
        let agent_running = agent_jobs
            .iter()
            .filter(|job| job_status_is_running(&job.status))
            .count();
        let agent_queued = agent_jobs
            .iter()
            .filter(|job| job_status_is_agent_queued(&job.status))
            .count();
        let reconciled_count = agent_jobs
            .iter()
            .filter(|job| job.recovery_state.as_deref() == Some("reconciled"))
            .count();
        let lost_after_reconcile_count = agent_jobs
            .iter()
            .filter(|job| {
                job.status == "lost"
                    && matches!(
                        job.recovery_reason_code.as_deref(),
                        Some(
                            "runner_inventory_missing"
                                | "runner_instance_replaced"
                                | "runner_recovery_deadline_exceeded"
                        )
                    )
            })
            .count();
        let mut local_active = 0usize;
        let mut local_running = 0usize;
        let mut local_queued = 0usize;
        for dir in local_job_dirs {
            if let Some(status) = std::fs::read_to_string(dir.join("status"))
                .ok()
                .map(|s| s.trim().to_string())
            {
                let normalized = normalize_local_status(&status);
                if ACTIVE_JOB_STATUSES.contains(&normalized.as_str()) {
                    local_active += 1;
                }
                local_running += usize::from(job_status_is_running(&normalized));
                local_queued += usize::from(job_status_is_local_queued(&normalized));
            }
        }
        let active_count = agent_active + local_active;
        let running_count = agent_running + local_running;
        let queued_count = agent_queued + local_queued;
        let jobs = json!({
            "agent_known_count": agent_known_count,
            "local_known_count": local_known_count,
            "active_count": active_count,
            "running_count": running_count,
            "queued_count": queued_count,
            "recovering_count": recovering_count,
            "reconciled_count": reconciled_count,
            "lost_after_reconcile_count": lost_after_reconcile_count,
        });

        // -- tools summary ----------------------------------------------------
        let specs = registered_tool_specs();
        let tools_count = specs.len();
        let tools_names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        let tools = json!({
            "count": tools_count,
            "names": tools_names,
        });

        let quic = self.runtime_info.quic.as_ref().map(|status| {
            let status = status.lock().expect("quic runtime status mutex poisoned");
            json!({
                "enabled": status.enabled,
                "listen": status.listen,
                "alpn": status.alpn,
                "listener_started": status.listener_started,
                "last_error": status.last_error,
            })
        });

        let mut output = json!({
            "service": "webcodex",
            "model_surface": self.model_surface().name(),
            "version": env!("CARGO_PKG_VERSION"),
            "build": crate::build_info::runtime_build_info(),
            "server_time": now,
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            "agents": agents,
            "connection_layers": connection_layers,
            "version_compatibility": version_compatibility,
            "jobs": jobs,
            "tools": tools,
            "authority": permissions::authority_profile_payload(),
            "session_store": self.sessions.status(),
        });
        if let Some(quic) = quic {
            output["quic"] = quic;
        }
        ToolResult::ok(output)
    }

    pub(crate) async fn runtime_status_with_options(
        &self,
        auth: Option<&AuthContext>,
        compact: bool,
        summary_only: bool,
        client_id: Option<String>,
    ) -> ToolResult {
        let result = match client_id {
            Some(client_id) => self.runtime_status_for_client(auth, client_id).await,
            None => self.runtime_status(auth).await,
        };
        if result.success && (compact || summary_only) {
            ToolResult {
                output: compact_runtime_status(&result.output),
                ..result
            }
        } else {
            result
        }
    }

    async fn runtime_status_for_client(
        &self,
        auth: Option<&AuthContext>,
        client_id: String,
    ) -> ToolResult {
        if !valid_target_client_id(&client_id) {
            return ToolResult::err_with_output(
                "invalid_client_id: client_id must contain 1..=128 characters".to_string(),
                json!({"error_kind": "invalid_client_id"}),
            );
        }
        let visible_clients = self.shell_clients.list_clients_for_auth(auth).await;
        let Some(client) = visible_clients
            .iter()
            .find(|client| client.client_id == client_id)
            .cloned()
        else {
            return ToolResult::err_with_output(
                "unknown_client_id: no caller-visible Runner matches the exact client_id"
                    .to_string(),
                json!({"error_kind": "unknown_client_id", "client_id": client_id}),
            );
        };
        let visible_jobs = self.shell_clients.list_all_jobs_for_auth(auth).await;
        let selected_jobs: Vec<ShellJobInfo> = visible_jobs
            .iter()
            .filter(|job| job.client_id == client.client_id)
            .cloned()
            .collect();
        let clients = vec![client.clone()];
        let now = chrono::Utc::now().timestamp();
        let project_count = enabled_projects_count(&client);
        let online_project_count = if client.connected { project_count } else { 0 };
        let target_compatibility = version_compatibility(&clients);
        let target_runner = target_compatibility
            .pointer("/runners/0")
            .cloned()
            .unwrap_or(Value::Null);
        let source_alignment = target_runner
            .get("source_alignment")
            .cloned()
            .unwrap_or_else(|| json!({"status": "unknown"}));
        let fleet_compatibility = version_compatibility(&visible_clients);
        let fleet_runners = fleet_compatibility
            .get("runners")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mismatched_agents_count = fleet_runners
            .iter()
            .filter(|runner| runner.get("status").and_then(Value::as_str) != Some("compatible"))
            .count();
        let source_mismatched_agents_count = fleet_runners
            .iter()
            .filter(|runner| {
                runner
                    .pointer("/source_alignment/status")
                    .and_then(Value::as_str)
                    == Some("different")
            })
            .count();
        let agent_active = selected_jobs
            .iter()
            .filter(|job| ACTIVE_JOB_STATUSES.contains(&job.status.as_str()))
            .count();
        let running_count = selected_jobs
            .iter()
            .filter(|job| job_status_is_running(&job.status))
            .count();
        let queued_count = selected_jobs
            .iter()
            .filter(|job| job_status_is_agent_queued(&job.status))
            .count();
        let recovering_count = selected_jobs
            .iter()
            .filter(|job| job.status == "recovering")
            .count();
        let reconciled_count = selected_jobs
            .iter()
            .filter(|job| job.recovery_state.as_deref() == Some("reconciled"))
            .count();
        let lost_after_reconcile_count = selected_jobs
            .iter()
            .filter(|job| {
                job.status == "lost"
                    && matches!(
                        job.recovery_reason_code.as_deref(),
                        Some(
                            "runner_inventory_missing"
                                | "runner_instance_replaced"
                                | "runner_recovery_deadline_exceeded"
                        )
                    )
            })
            .count();
        let projects = json!({
            "mode": "agent_registered",
            "agent_registered": {
                "count": project_count,
                "online_count": online_project_count,
            },
            "effective": {
                "count": project_count,
                "status": if project_count > 0 { "ok" } else { "no_projects" },
            },
            "count": project_count,
        });
        let agents = json!({
            "count": 1,
            "online_count": usize::from(client.connected),
            "stale_count": usize::from(!client.connected),
            "clients": [{
                "client_id": client.client_id,
                "agent_instance_id": client.agent_instance_id,
                "display_name": client.display_name,
                "status": client.status,
                "connected": client.connected,
                "agent_protocol_version": client.agent_protocol_version,
                "transport": client.transport,
                "last_seen": client.last_seen,
                "last_seen_age_secs": last_seen_age_secs(&client, now),
                "pending_requests": client.pending_requests,
                "active_jobs": active_jobs_for_client(&selected_jobs, &client.client_id),
                "job_concurrency": job_concurrency_for_client(&client, &selected_jobs),
                "projects_count": project_count,
            }],
            "summary": agent_health_summary(&clients, &selected_jobs, now),
        });
        let jobs = json!({
            "agent_known_count": selected_jobs.len(),
            "local_known_count": 0,
            "active_count": agent_active,
            "running_count": running_count,
            "queued_count": queued_count,
            "recovering_count": recovering_count,
            "reconciled_count": reconciled_count,
            "lost_after_reconcile_count": lost_after_reconcile_count,
        });
        let specs = registered_tool_specs();
        let tools = json!({
            "count": specs.len(),
            "names": specs.iter().map(|spec| spec.name.clone()).collect::<Vec<_>>(),
        });
        let server_build = crate::build_info::runtime_build_info();
        ToolResult::ok(json!({
            "service": "webcodex",
            "model_surface": self.model_surface().name(),
            "version": env!("CARGO_PKG_VERSION"),
            "build": server_build,
            "server_time": now,
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "focus": {
                "client_id": client.client_id,
                "connected": client.connected,
                "status": client.status,
                "agent_instance_id": client.agent_instance_id,
                "build": client.build,
                "project_count": project_count,
                "active_jobs": agent_active,
                "job_concurrency": job_concurrency_for_client(&client, &selected_jobs),
                "compatibility_status": target_runner.get("status").cloned().unwrap_or(Value::Null),
                "source_alignment": source_alignment,
            },
            "server": {
                "version": env!("CARGO_PKG_VERSION"),
                "build": server_build,
            },
            "fleet_summary": {
                "visible_runner_count": visible_clients.len(),
                "mismatched_agents_count": mismatched_agents_count,
                "source_mismatched_agents_count": source_mismatched_agents_count,
                "mixed_builds_present": mismatched_agents_count > 0 || source_mismatched_agents_count > 0,
            },
            "projects": projects,
            "agents": agents,
            "version_compatibility": target_compatibility,
            "jobs": jobs,
            "tools": tools,
            "authority": permissions::authority_profile_payload(),
        }))
    }
}

pub(crate) fn compact_runtime_status(status: &Value) -> Value {
    if status.get("focus").is_some() {
        return json!({
            "compact": true,
            "service": status.get("service").cloned().unwrap_or_else(|| json!("webcodex")),
            "model_surface": status
                .get("model_surface")
                .cloned()
                .unwrap_or_else(|| json!(crate::model_surface::MODEL_SURFACE_LOCAL_CODING)),
            "version": status.get("version").cloned().unwrap_or(Value::Null),
            "focus": status.get("focus").cloned().unwrap_or(Value::Null),
            "server": status.get("server").cloned().unwrap_or(Value::Null),
            "fleet_summary": status.get("fleet_summary").cloned().unwrap_or(Value::Null),
            "projects": {
                "effective": status.pointer("/projects/effective").cloned().unwrap_or(Value::Null),
                "agent_registered": status.pointer("/projects/agent_registered").cloned().unwrap_or(Value::Null),
            },
            "jobs": {
                "active_count": status.pointer("/jobs/active_count").cloned().unwrap_or(Value::Null),
                "running_count": status.pointer("/jobs/running_count").cloned().unwrap_or(Value::Null),
                "queued_count": status.pointer("/jobs/queued_count").cloned().unwrap_or(Value::Null),
            },
            "version_compatibility": {
                "status": status.pointer("/version_compatibility/status").cloned().unwrap_or_else(|| json!("unknown")),
                "source_alignment": status.pointer("/version_compatibility/source_alignment").cloned().unwrap_or_else(|| json!({"status": "unknown"})),
            },
        });
    }
    let mut compact = json!({
        "compact": true,
        "service": status.get("service").cloned().unwrap_or_else(|| json!("webcodex")),
        "model_surface": status
            .get("model_surface")
            .cloned()
            .unwrap_or_else(|| json!(crate::model_surface::MODEL_SURFACE_LOCAL_CODING)),
        "version": status.get("version").cloned().unwrap_or(Value::Null),
        "build": {
            "version": status.get("version").cloned().unwrap_or(Value::Null),
            "git_commit": status.pointer("/build/git_commit").cloned().unwrap_or(Value::Null),
            "git_dirty": status.pointer("/build/git_dirty").cloned().unwrap_or(Value::Null),
        },
        "tools": {
            "count": status.pointer("/tools/count").cloned().unwrap_or(Value::Null),
        },
        "jobs": {
            "active_count": status.pointer("/jobs/active_count").cloned().unwrap_or(Value::Null),
            "running_count": status.pointer("/jobs/running_count").cloned().unwrap_or(Value::Null),
            "queued_count": status.pointer("/jobs/queued_count").cloned().unwrap_or(Value::Null),
        },
        "agents": {
            "count": status.pointer("/agents/count").cloned().unwrap_or_else(|| json!(0)),
            "online_count": status.pointer("/agents/online_count").cloned().unwrap_or_else(|| json!(0)),
            "stale_count": status.pointer("/agents/stale_count").cloned().unwrap_or_else(|| json!(0)),
            "clients": compact_runner_clients(status),
            "summary": status.pointer("/agents/summary").cloned().unwrap_or_else(|| json!({
                "count": 0,
                "online": 0,
                "offline": 0,
                "stale": 0,
                "clients": [],
            })),
        },
        "projects": {
            "effective": status.pointer("/projects/effective").cloned().unwrap_or_else(|| json!({
                "count": 0,
                "status": "unknown",
            })),
            "agent_registered": status.pointer("/projects/agent_registered").cloned().unwrap_or_else(|| json!({
                "count": 0,
                "online_count": 0,
            })),
            "mode": status.pointer("/projects/mode").cloned().unwrap_or_else(|| json!("agent_registered")),
        },
        "connection_layers": status.get("connection_layers").cloned().unwrap_or_else(|| json!({
            "runner_process": {"status": "not_observed"},
            "server_transport": {"status": "not_observed"},
            "server_registration": {"status": "not_observed"},
            "project_registry": {"status": "not_observed"},
            "connector_endpoint": {"status": "not_observed"},
            "session_binding": {"status": "not_observed"},
            "last_successful_tool_call": {"status": "not_observed"},
        })),
        "version_compatibility": {
            "status": status.pointer("/version_compatibility/status").cloned().unwrap_or_else(|| json!("unknown")),
            "source_alignment": status.pointer("/version_compatibility/source_alignment").cloned().unwrap_or_else(|| json!({"status": "unknown"})),
        },
        "authority": status.get("authority").cloned().unwrap_or(Value::Null),
    });
    if let Some(object) = compact.as_object_mut() {
        for field in ["focus", "server", "fleet_summary"] {
            if let Some(value) = status.get(field) {
                object.insert(field.to_string(), value.clone());
            }
        }
    }
    compact
}

fn compact_runner_clients(status: &Value) -> Vec<Value> {
    let agents = status
        .pointer("/agents/clients")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let compatibility = status
        .pointer("/version_compatibility/runners")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    agents
        .iter()
        .filter_map(|agent| {
            let client_id = agent.get("client_id")?.as_str()?;
            let compat = compatibility
                .iter()
                .find(|candidate| candidate.get("client_id").and_then(Value::as_str) == Some(client_id));
            let mut compact = json!({
                "client_id": client_id,
                "agent_instance_id": agent.get("agent_instance_id").cloned().unwrap_or(Value::Null),
                "status": agent.get("status").cloned().unwrap_or(Value::Null),
                "transport": agent.get("transport").cloned().unwrap_or(Value::Null),
                "build_git_commit": compat.and_then(|value| value.get("build_git_commit")).cloned().unwrap_or(Value::Null),
                "build_git_dirty": compat.and_then(|value| value.get("build_git_dirty")).cloned().unwrap_or(Value::Null),
                "version_matches_server": compat.and_then(|value| value.get("version_matches_server")).cloned().unwrap_or(Value::Null),
                "source_alignment": compat.and_then(|value| value.get("source_alignment")).cloned().unwrap_or_else(|| json!({"status": "unknown"})),
            });
            if status.get("focus").is_none() {
                compact["host_context"] = agent.get("host_context").cloned().unwrap_or(Value::Null);
            }
            Some(compact)
        })
        .collect()
}

fn host_context_projection(context: Option<&crate::shell_protocol::AgentHostContext>) -> Value {
    match context {
        Some(context) => json!({
            "source": "runner_config",
            "role": context.role,
            "runtime": context.runtime,
            "service": context.service,
            "network": context.network,
            "architecture": context.architecture,
        }),
        None => Value::Null,
    }
}

/// Stale threshold for runner-derived layers (heartbeat window).
const RUNNER_STALE_AFTER_SECS: i64 = crate::shell_client::CLIENT_ONLINE_WINDOW_SECS;
/// Stale threshold for connector/tool-call activity observations.
const ACTIVITY_STALE_AFTER_SECS: i64 = 600;

/// Supported runner protocol versions for the current server build.
const SUPPORTED_AGENT_PROTOCOL_VERSIONS: &[&str] = &["polling-v1", "websocket-v1", "quic-v1"];

/// One connection-layer observation with the canonical contract fields:
/// `status`, `observed_at`, `source`, `age_secs`, `stale_after_secs`,
/// `reason_code`. Extra layer-specific facts are merged on top.
fn layer_observation(
    status: &str,
    observed_at: Option<i64>,
    source: &str,
    stale_after_secs: Option<i64>,
    reason_code: Option<&str>,
    now: i64,
    extra: Value,
) -> Value {
    let mut layer = json!({
        "status": status,
        "observed_at": observed_at,
        "source": source,
        "age_secs": observed_at.map(|at| now.saturating_sub(at)),
        "stale_after_secs": stale_after_secs,
        "reason_code": reason_code,
    });
    if let (Some(object), Some(extra)) = (layer.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            object.insert(key.clone(), value.clone());
        }
    }
    layer
}

fn connection_layers(
    clients: &[ShellClientView],
    registered_projects: usize,
    online_projects: usize,
    observations: &super::observations::RuntimeObservations,
    auth: Option<&AuthContext>,
    now: i64,
) -> Value {
    // Freshest client drives single-value observations; counts stay explicit.
    let freshest = clients.iter().max_by_key(|c| c.last_seen);
    let online: Vec<&ShellClientView> = clients.iter().filter(|c| c.connected).collect();
    let freshest_online = online.iter().max_by_key(|c| c.last_seen).copied();

    // -- runner_process: process observation, distinct from transport --------
    let runner_process = match (freshest_online, freshest) {
        (Some(client), _) => {
            let (source, reason) = if client.process_started_at.is_some() {
                ("runner_process_report", None)
            } else {
                // Live transport still proves a running process; the runner
                // just did not report its start identity.
                ("transport_liveness", Some("process_start_not_reported"))
            };
            layer_observation(
                "ready",
                Some(client.last_seen),
                source,
                Some(RUNNER_STALE_AFTER_SECS),
                reason,
                now,
                json!({
                    "client_id": client.client_id,
                    "agent_instance_id": client.agent_instance_id,
                    "process_started_at": client.process_started_at,
                }),
            )
        }
        (None, Some(client)) => layer_observation(
            "stale",
            Some(client.last_seen),
            "server_heartbeat_window",
            Some(RUNNER_STALE_AFTER_SECS),
            Some("heartbeat_expired"),
            now,
            json!({
                "client_id": client.client_id,
                "agent_instance_id": client.agent_instance_id,
                "process_started_at": client.process_started_at,
            }),
        ),
        (None, None) => layer_observation(
            "not_observed",
            None,
            "server_registry",
            None,
            Some("no_runner_registered"),
            now,
            json!({}),
        ),
    };

    // -- server_transport: real connection lifecycle --------------------------
    let server_transport = match (freshest_online, freshest) {
        (Some(client), _) => layer_observation(
            "connected",
            Some(client.last_seen),
            "server_transport_lifecycle",
            Some(RUNNER_STALE_AFTER_SECS),
            None,
            now,
            json!({
                "connected_clients": online.len(),
                "transport": client.transport,
                "connection_instance": client.agent_instance_id,
                "connected_at": client.connected_at,
                "last_heartbeat_at": client.last_seen,
            }),
        ),
        (None, Some(client)) => layer_observation(
            "disconnected",
            client.disconnected_at.or(Some(client.last_seen)),
            "server_transport_lifecycle",
            None,
            Some("transport_closed_or_heartbeat_expired"),
            now,
            json!({
                "connected_clients": 0,
                "transport": client.transport,
                "connection_instance": client.agent_instance_id,
                "connected_at": client.connected_at,
                "disconnected_at": client.disconnected_at,
            }),
        ),
        (None, None) => layer_observation(
            "not_observed",
            None,
            "server_transport_lifecycle",
            None,
            Some("no_transport_ever_connected"),
            now,
            json!({"connected_clients": 0}),
        ),
    };

    // -- server_registration: which instance registered, and is it current ---
    let server_registration = match (freshest_online, freshest) {
        (Some(client), _) => layer_observation(
            "registered",
            Some(client.registered_at),
            "runner_registration",
            None,
            None,
            now,
            json!({
                "registered_clients": clients.len(),
                "runner_instance": client.agent_instance_id,
                "registered_at": client.registered_at,
                "last_refreshed_at": client.last_seen,
            }),
        ),
        (None, Some(client)) => layer_observation(
            "stale",
            Some(client.registered_at),
            "runner_registration",
            Some(RUNNER_STALE_AFTER_SECS),
            Some("registration_instance_disconnected"),
            now,
            json!({
                "registered_clients": clients.len(),
                "runner_instance": client.agent_instance_id,
                "registered_at": client.registered_at,
                "last_refreshed_at": client.last_seen,
            }),
        ),
        (None, None) => layer_observation(
            "not_observed",
            None,
            "runner_registration",
            None,
            Some("no_registration"),
            now,
            json!({"registered_clients": 0}),
        ),
    };

    // -- project_registry ------------------------------------------------------
    let project_registry = if registered_projects == 0 {
        layer_observation(
            "not_configured",
            None,
            "runner_project_report",
            None,
            Some("no_projects_registered"),
            now,
            json!({"registered_projects": 0, "online_projects": 0}),
        )
    } else if online_projects > 0 {
        layer_observation(
            "registered",
            freshest_online.map(|c| c.last_seen),
            "runner_project_report",
            Some(RUNNER_STALE_AFTER_SECS),
            None,
            now,
            json!({
                "registered_projects": registered_projects,
                "online_projects": online_projects,
                "providing_instance": freshest_online.map(|c| c.agent_instance_id.clone()),
            }),
        )
    } else {
        // Projects are known but their providing runner connection is gone:
        // a stale registration must not pretend to be callable.
        layer_observation(
            "stale",
            freshest.map(|c| c.last_seen),
            "runner_project_report",
            Some(RUNNER_STALE_AFTER_SECS),
            Some("providing_runner_disconnected"),
            now,
            json!({
                "registered_projects": registered_projects,
                "online_projects": 0,
            }),
        )
    };

    // -- connector_endpoint: observed activity, never config inference --------
    let connector_endpoint = if !observations.connector_configured() {
        layer_observation(
            "not_configured",
            None,
            "connector_runtime",
            None,
            Some("connector_runtime_disabled"),
            now,
            json!({}),
        )
    } else {
        match observations.latest_connector_observation() {
            Some(observation) => {
                let status = match observation.status.as_str() {
                    "ready" | "request_succeeded" => "ready",
                    _ => "unknown",
                };
                let reason = (status == "unknown").then_some("last_probe_not_ready");
                layer_observation(
                    status,
                    Some(observation.observed_at),
                    &observation.source,
                    Some(ACTIVITY_STALE_AFTER_SECS),
                    reason,
                    now,
                    json!({"last_observation": observation.status}),
                )
            }
            None => layer_observation(
                "not_observed",
                None,
                "connector_runtime",
                None,
                Some("no_connector_requests_observed"),
                now,
                json!({}),
            ),
        }
    };

    // -- session_binding: exact durable identity + process-local cache ----------
    let session_binding = layer_observation(
        "not_observed",
        None,
        "session_store",
        None,
        Some("exact_binding_requires_window_and_project_observation"),
        now,
        json!({
            "process_local_cache": true,
            "durable_exact_binding": true,
            "restored_after_restart": true,
            "requires_stable_window_identity": true,
            "missing_identity_fallback": false,
        }),
    );

    // -- last_successful_tool_call: scoped meaningful activity ----------------
    let principal = super::session_context::current_session_principal(auth).ok();
    let observation = principal
        .as_ref()
        .and_then(|(kind, id)| observations.latest_tool_call_for_principal(kind, id))
        .map(|obs| (obs, "principal"))
        .or_else(|| {
            observations
                .latest_tool_call()
                .map(|obs| (obs, "any_principal"))
        });
    let last_successful_tool_call = match observation {
        Some((obs, scope)) => layer_observation(
            "observed",
            Some(obs.observed_at),
            "runtime_observations",
            Some(ACTIVITY_STALE_AFTER_SECS),
            None,
            now,
            json!({
                "scope": scope,
                "principal_kind": obs.principal_kind,
                "project": obs.project,
                "surface": obs.surface,
                "session_id": obs.session_id,
                "tool": obs.tool,
            }),
        ),
        None => layer_observation(
            "not_observed",
            None,
            "runtime_observations",
            None,
            Some("no_meaningful_tool_calls_recorded"),
            now,
            json!({}),
        ),
    };

    json!({
        "runner_process": runner_process,
        "server_transport": server_transport,
        "server_registration": server_registration,
        "project_registry": project_registry,
        "connector_endpoint": connector_endpoint,
        "session_binding": session_binding,
        "last_successful_tool_call": last_successful_tool_call,
    })
}

/// Mixed-version diagnostics: a connected runner is not automatically
/// capability-compatible. Reports facts about which side to upgrade without
/// exposing paths or environment.
fn version_compatibility(clients: &[ShellClientView]) -> Value {
    let build = crate::build_info::runtime_build_info();
    version_compatibility_against(
        clients,
        env!("CARGO_PKG_VERSION"),
        build.git_commit,
        build.git_dirty,
        json!(build),
    )
}

fn version_compatibility_against(
    clients: &[ShellClientView],
    server_version: &str,
    server_git_commit: Option<&str>,
    server_git_dirty: Option<bool>,
    server_build: Value,
) -> Value {
    let mut overall = if clients.is_empty() {
        "no_runners"
    } else {
        "compatible"
    };
    let mut source_overall = if clients.is_empty() {
        "no_runners"
    } else {
        "aligned"
    };
    let runners: Vec<Value> = clients
        .iter()
        .map(|client| {
            let protocol_supported =
                SUPPORTED_AGENT_PROTOCOL_VERSIONS.contains(&client.agent_protocol_version.as_str());
            let build_version = client.build.as_ref().and_then(|b| b.version.clone());
            let build_git_commit = client.build.as_ref().and_then(|b| b.git_commit.clone());
            let build_git_dirty = client.build.as_ref().and_then(|b| b.git_dirty);
            let version_matches_server = build_version
                .as_deref()
                .map(|version| version == server_version);
            let git_commit_matches_server = match (build_git_commit.as_deref(), server_git_commit) {
                (Some(runner), Some(server)) => Some(runner == server),
                _ => None,
            };
            let source_matches_server = match (
                git_commit_matches_server,
                build_git_dirty,
                server_git_dirty,
            ) {
                (Some(false), _, _) => Some(false),
                (Some(true), Some(false), Some(false)) => Some(true),
                (Some(true), Some(true), _) | (Some(true), _, Some(true)) => Some(false),
                _ => None,
            };
            let (source_status, source_reason_code, source_action) = match source_matches_server {
                Some(true) => ("aligned", None, None),
                Some(false) if git_commit_matches_server == Some(false) => (
                    "different",
                    Some("runner_git_commit_differs_from_server"),
                    Some("redeploy the older side when exact dogfood source alignment is required"),
                ),
                Some(false) => (
                    "different",
                    Some("dirty_build_prevents_exact_source_alignment"),
                    Some("rebuild clean artifacts before relying on exact source alignment"),
                ),
                None => (
                    "unknown",
                    Some("build_source_identity_incomplete"),
                    Some("use builds that report git commit and dirty state for exact source alignment"),
                ),
            };
            match (source_status, source_overall) {
                ("different", _) => source_overall = "different",
                ("unknown", "aligned") => source_overall = "unknown",
                _ => {}
            }

            let (status, reason_code, action) = if !protocol_supported {
                (
                    "capability_mismatch",
                    Some("agent_protocol_version_unsupported"),
                    Some("upgrade the runner to a build announcing a supported protocol version"),
                )
            } else if version_matches_server == Some(false) {
                (
                    "version_mismatch",
                    Some("runner_version_differs_from_server"),
                    Some("align server and runner package versions (redeploy the older side)"),
                )
            } else {
                ("compatible", None, None)
            };
            match (status, overall) {
                ("capability_mismatch", _) => overall = "capability_mismatch",
                ("version_mismatch", o) if o != "capability_mismatch" => {
                    overall = "version_mismatch"
                }
                _ => {}
            }
            json!({
                "client_id": client.client_id,
                "agent_protocol_version": client.agent_protocol_version,
                "protocol_supported": protocol_supported,
                "build_version": build_version,
                "build_git_commit": build_git_commit,
                "build_git_dirty": build_git_dirty,
                "version_matches_server": version_matches_server,
                "status": status,
                "reason_code": reason_code,
                "action": action,
                "source_alignment": {
                    "status": source_status,
                    "git_commit_matches_server": git_commit_matches_server,
                    "source_matches_server": source_matches_server,
                    "reason_code": source_reason_code,
                    "action": source_action,
                },
            })
        })
        .collect();
    json!({
        "status": overall,
        "source_alignment": {
            "status": source_overall,
        },
        "server": {
            "version": server_version,
            "build": server_build,
        },
        "runners": runners,
    })
}

fn valid_target_client_id(client_id: &str) -> bool {
    !client_id.is_empty() && client_id.chars().count() <= TARGET_CLIENT_ID_MAX_CHARS
}

#[cfg(test)]
pub(crate) fn version_compatibility_for_test(
    clients: &[ShellClientView],
    server_version: &str,
    server_git_commit: Option<&str>,
    server_git_dirty: Option<bool>,
) -> Value {
    version_compatibility_against(
        clients,
        server_version,
        server_git_commit,
        server_git_dirty,
        json!({
            "git_commit": server_git_commit,
            "git_dirty": server_git_dirty,
            "built_at": null,
        }),
    )
}

fn enabled_projects_count(client: &ShellClientView) -> usize {
    client
        .projects
        .iter()
        .filter(|project| !project.disabled)
        .count()
}

fn last_seen_age_secs(client: &ShellClientView, now: i64) -> i64 {
    now.saturating_sub(client.last_seen)
}

fn active_jobs_for_client(agent_jobs: &[ShellJobInfo], client_id: &str) -> usize {
    agent_jobs
        .iter()
        .filter(|job| {
            job.client_id == client_id && ACTIVE_JOB_STATUSES.contains(&job.status.as_str())
        })
        .count()
}

fn job_status_is_running(status: &str) -> bool {
    RUNNING_JOB_STATUSES.contains(&status)
}

fn job_status_is_agent_queued(status: &str) -> bool {
    AGENT_QUEUED_JOB_STATUSES.contains(&status)
}

fn job_status_is_local_queued(status: &str) -> bool {
    LOCAL_QUEUED_JOB_STATUSES.contains(&status)
}

fn job_concurrency_for_client(client: &ShellClientView, agent_jobs: &[ShellJobInfo]) -> Value {
    let mut running = 0usize;
    let mut queued = 0usize;
    for job in agent_jobs
        .iter()
        .filter(|job| job.client_id == client.client_id)
    {
        running += usize::from(job_status_is_running(&job.status));
        queued += usize::from(job_status_is_agent_queued(&job.status));
    }
    json!({
        "limit": client.job_concurrency_limit,
        "running": running,
        "queued": queued,
    })
}

fn agent_health_clients(
    clients: &[ShellClientView],
    agent_jobs: &[ShellJobInfo],
    now: i64,
) -> Vec<Value> {
    clients
        .iter()
        .map(|client| {
            json!({
                "client_id": client.client_id,
                "status": client.status,
                "transport": client.transport,
                "last_seen_age_secs": last_seen_age_secs(client, now),
                "projects_count": enabled_projects_count(client),
                "pending_requests": client.pending_requests,
                "active_jobs": active_jobs_for_client(agent_jobs, &client.client_id),
                "job_concurrency": job_concurrency_for_client(client, agent_jobs),
            })
        })
        .collect()
}

fn agent_health_summary(
    clients: &[ShellClientView],
    agent_jobs: &[ShellJobInfo],
    now: i64,
) -> Value {
    let online = clients.iter().filter(|client| client.connected).count();
    let stale = clients
        .iter()
        .filter(|client| client.status == "stale")
        .count();
    let offline = clients.len().saturating_sub(online);
    json!({
        "count": clients.len(),
        "online": online,
        "offline": offline,
        "stale": stale,
        "clients": agent_health_clients(clients, agent_jobs, now),
    })
}

/// Build the sanitized policy summary JSON exposed in `runtime_status` and
/// `listAgents`. Only the safe fields are carried: `allow_raw_shell`,
/// `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`,
/// `max_output_bytes`. The agent token, shell env values, init_script
/// contents, and full agent.toml contents are NEVER included. Older agents
/// that registered without a policy produce `Value::Null` so the field is
/// present-but-null for clients that expect it.
fn sanitized_policy_summary(policy: Option<&crate::shell_protocol::AgentPolicySummary>) -> Value {
    match policy {
        Some(p) => json!({
            "allow_raw_shell": p.allow_raw_shell,
            "allow_cwd_anywhere": p.allow_cwd_anywhere,
            "allowed_roots": p.allowed_roots,
            "max_timeout_secs": p.max_timeout_secs,
            "max_output_bytes": p.max_output_bytes,
        }),
        None => Value::Null,
    }
}

/// Build the sanitized shell-profiles summary JSON exposed in
/// `runtime_status`, `listAgents`, and `listProjects`. Only safe metadata is
/// carried: default profile name, configured count, prepared-cache count, and
/// per-profile name / has_init_script (boolean) / env_keys_count / program /
/// args_count. NEVER includes init_script bodies, env values, tokens, or the
/// full env snapshot. Older agents that did not report a summary produce
/// `Value::Null`.
fn sanitized_shell_profiles_summary(
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> Value {
    match summary {
        Some(s) => {
            let profiles: Vec<Value> = s
                .profiles
                .iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "has_init_script": p.has_init_script,
                        "env_keys_count": p.env_keys_count,
                        "program": p.program,
                        "args_count": p.args_count,
                        "dialect": p.dialect,
                    })
                })
                .collect();
            json!({
                "default_profile": s.default_profile,
                "configured_count": s.configured_count,
                "prepared_cache_count": s.prepared_cache_count,
                "profiles": profiles,
                "default_dialect": s.default_dialect,
                "available_dialects": s.available_dialects,
            })
        }
        None => Value::Null,
    }
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            auth_enabled: false,
            configured_public_url: None,
            quic: Some(std::sync::Arc::new(std::sync::Mutex::new(
                crate::config::QuicServerConfig::default().runtime_status(),
            ))),
        }
    }
}

#[cfg(test)]
mod phase_e2_status_tests {
    use super::*;

    #[test]
    fn concurrency_counts_use_only_canonical_running_and_queued_statuses() {
        for status in ["running", "started"] {
            assert!(job_status_is_running(status), "{status}");
            assert!(!job_status_is_agent_queued(status), "{status}");
        }
        for status in ["queued", "agent_queued"] {
            assert!(job_status_is_agent_queued(status), "{status}");
            assert!(!job_status_is_running(status), "{status}");
        }
        assert!(job_status_is_local_queued("queued"));
        assert!(!job_status_is_local_queued("agent_queued"));
        for status in [
            "stop_requested",
            "recovering",
            "completed",
            "failed",
            "stopped",
            "lost",
            "timeout",
            "timed_out",
            "cancelled",
        ] {
            assert!(!job_status_is_running(status), "{status}");
            assert!(!job_status_is_agent_queued(status), "{status}");
            assert!(!job_status_is_local_queued(status), "{status}");
        }
    }

    #[test]
    fn runner_concurrency_counts_jobs_across_projects_for_one_client() {
        fn job(job_id: &str, client_id: &str, project_id: &str, status: &str) -> ShellJobInfo {
            ShellJobInfo {
                job_id: job_id.to_string(),
                request_id: None,
                client_id: client_id.to_string(),
                kind: "shell".to_string(),
                project_id: Some(project_id.to_string()),
                session_id: None,
                ssh_resource: None,
                cwd: None,
                project_cwd: None,
                purpose: None,
                shell: None,
                command_preview: "test".to_string(),
                status: status.to_string(),
                created_at: 0,
                started_at: None,
                ended_at: None,
                exit_code: None,
                duration_ms: None,
                elapsed_secs: None,
                error: None,
                command_execution_state: None,
                structured_execution: None,
                codex: None,
                result: None,
                validation_progress: None,
                validation: None,
                recovery_state: None,
                recovered_after_server_restart: false,
                reconciled_at: None,
                recovery_reason_code: None,
                observation_token: None,
                last_update_seq: None,
                stdout_retained_from_line: None,
                stderr_retained_from_line: None,
                stdout_log_truncated: false,
                stderr_log_truncated: false,
            }
        }

        let client = ShellClientView {
            client_id: "shared-runner".to_string(),
            agent_instance_id: "shared-instance".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            status: "online".to_string(),
            connected: true,
            last_seen: 0,
            capabilities: Default::default(),
            pending_requests: 0,
            projects: Vec::new(),
            agent_protocol_version: "websocket-v1".to_string(),
            transport: "websocket".to_string(),
            policy: None,
            registered_at: 0,
            connected_at: 0,
            disconnected_at: None,
            process_started_at: None,
            build: None,
            job_concurrency_limit: Some(2),
        };
        let jobs = vec![
            job(
                "project-a-running",
                "shared-runner",
                "agent:shared:a",
                "running",
            ),
            job(
                "project-b-queued",
                "shared-runner",
                "agent:shared:b",
                "agent_queued",
            ),
            job(
                "foreign-running",
                "other-runner",
                "agent:other:c",
                "running",
            ),
        ];

        assert_eq!(active_jobs_for_client(&jobs, "shared-runner"), 2);
        assert_eq!(
            job_concurrency_for_client(&client, &jobs),
            json!({"limit": 2, "running": 1, "queued": 1})
        );
    }

    #[test]
    fn compact_runtime_status_keeps_minimum_job_state_counts() {
        let compact = compact_runtime_status(&json!({
            "jobs": {
                "active_count": 5,
                "running_count": 2,
                "queued_count": 1,
            }
        }));
        assert_eq!(
            compact["jobs"],
            json!({
                "active_count": 5,
                "running_count": 2,
                "queued_count": 1,
            })
        );
    }
}
