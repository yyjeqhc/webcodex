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
    // Kept for the server binary and tests; the agent-only binary builds this
    // module without wiring runtime HTTP metadata, so it is intentionally idle
    // in that compile unit.
    #[allow(dead_code)]
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
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
        let agent_jobs = self.shell_clients.list_jobs_for_auth(auth, None).await;
        let now = chrono::Utc::now().timestamp();
        let agents: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "hostname": c.hostname,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "last_seen_age_secs": last_seen_age_secs(c, now),
                    "pending_requests": c.pending_requests,
                    "projects_count": enabled_projects_count(c),
                    "active_jobs": active_jobs_for_client(&agent_jobs, &c.client_id),
                    "capabilities": c.capabilities,
                    "projects": c.projects,
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
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
        let agent_jobs = self.shell_clients.list_jobs_for_auth(auth, None).await;

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
        // from the registry on disconnect, so they never appear here; the
        // legacy `offline_count` field is retained (it mirrors `stale_count`
        // for the registered set) for backward compatibility with existing
        // callers/tests.
        let stale_count = agent_count.saturating_sub(online_count);
        let offline_count = stale_count;
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "last_seen_age_secs": last_seen_age_secs(c, now),
                    "pending_requests": c.pending_requests,
                    "active_jobs": active_jobs_for_client(&agent_jobs, &c.client_id),
                    "capabilities": c.capabilities,
                    "projects_count": enabled_projects_count(c),
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "offline_count": offline_count,
            "clients": clients_summary,
            "summary": agent_health_summary(&clients, &agent_jobs, now),
        });

        // -- jobs summary -----------------------------------------------------
        // Agent-known jobs come from the registry; local jobs come from the
        // in-memory map. Active = running/queued/agent_queued/stop_requested.
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
        let mut local_active = 0usize;
        for dir in local_job_dirs {
            if let Some(status) = std::fs::read_to_string(dir.join("status"))
                .ok()
                .map(|s| s.trim().to_string())
            {
                let normalized = normalize_local_status(&status);
                if ACTIVE_JOB_STATUSES.contains(&normalized.as_str()) {
                    local_active += 1;
                }
            }
        }
        let active_count = agent_active + local_active;
        let jobs = json!({
            "agent_known_count": agent_known_count,
            "local_known_count": local_known_count,
            "active_count": active_count,
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
            "version": env!("CARGO_PKG_VERSION"),
            "build": crate::build_info::runtime_build_info(),
            "server_time": now,
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            "agents": agents,
            "jobs": jobs,
            "tools": tools,
            "permissions": permissions::permission_profile_payload(),
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
    ) -> ToolResult {
        let result = self.runtime_status(auth).await;
        if compact || summary_only {
            ToolResult {
                output: compact_runtime_status(&result.output),
                ..result
            }
        } else {
            result
        }
    }
}

pub(crate) fn compact_runtime_status(status: &Value) -> Value {
    json!({
        "compact": true,
        "service": status.get("service").cloned().unwrap_or_else(|| json!("webcodex")),
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
        },
        "agents": {
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
    })
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
                    })
                })
                .collect();
            json!({
                "default_profile": s.default_profile,
                "configured_count": s.configured_count,
                "prepared_cache_count": s.prepared_cache_count,
                "profiles": profiles,
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
