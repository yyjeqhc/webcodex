//! Runtime observability metadata injected into `ToolRuntime`.

use super::registry::registered_tool_specs;
use super::{permissions, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerView, ShellJobInfo};
use serde_json::{json, Value};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

const LIST_RUNNERS_MAX_CLIENT_IDS: usize = 8;
const TARGET_CLIENT_ID_MAX_CHARS: usize = 128;

fn runner_capability_negotiation(client: &RunnerView) -> Value {
    let summary = webcodex_runner_registry::capability_negotiation_summary(&client.capabilities);
    json!({
        "mode": summary.mode,
        "protocol_generation": client.runner_protocol_generation.get(),
        "supported_feature_count": summary.supported_feature_count,
        "generation_baseline_feature_count": summary.generation_baseline_feature_count,
        "registration_required_supported": summary.registration_required_supported,
        "critical_contracts": {
            "job_state_reconciliation": summary.critical_contracts.job_state_reconciliation,
            "native_tool_plugins": summary.critical_contracts.native_tool_plugins,
            "detached_process_jobs": summary.critical_contracts.detached_process_jobs,
            "runner_config_control": summary.critical_contracts.runner_config_control,
            "managed_worktree": summary.critical_contracts.managed_worktree,
            "skill_management": summary.critical_contracts.skill_management,
        }
    })
}

fn operation_phase_counts(jobs: &[ShellJobInfo]) -> Value {
    const PHASES: [&str; 9] = [
        "accepted",
        "queued",
        "running",
        "waiting_external",
        "recovering",
        "succeeded",
        "failed",
        "rolled_back",
        "outcome_unknown",
    ];
    let mut counts = serde_json::Map::new();
    for phase in PHASES {
        let count = jobs
            .iter()
            .filter(|job| {
                job.operation_phase
                    .is_some_and(|value| value.as_wire() == phase)
            })
            .count();
        counts.insert(phase.to_string(), json!(count));
    }
    Value::Object(counts)
}

fn caller_authority_status(auth: Option<&AuthContext>) -> Value {
    use webcodex_core::authority::{
        SCOPE_DIAGNOSTICS_READ, SCOPE_JOB_DETACH, SCOPE_JOB_RUN, SCOPE_PLUGIN_MANAGE,
        SCOPE_PLUGIN_MUTATE, SCOPE_PROJECT_WRITE, SCOPE_RUNNER_MANAGE, SCOPE_RUNTIME_READ,
        SCOPE_SERVICE_DEPLOY, SCOPE_SERVICE_RESTART,
    };

    let has_scope = |scope: &str| auth.is_none_or(|context| context.has_scope(scope));
    let job_run = has_scope(SCOPE_JOB_RUN);
    let job_detach = has_scope(SCOPE_JOB_DETACH);
    let detached_process = job_run && job_detach;
    let service_restart = has_scope(SCOPE_SERVICE_RESTART);
    let service_deploy = has_scope(SCOPE_SERVICE_DEPLOY);
    let missing_detached_scopes: Vec<&str> = [SCOPE_JOB_RUN, SCOPE_JOB_DETACH]
        .into_iter()
        .filter(|scope| !has_scope(scope))
        .collect();
    let missing_service_restart_scopes: Vec<&str> = [SCOPE_SERVICE_RESTART]
        .into_iter()
        .filter(|scope| !has_scope(scope))
        .collect();
    let missing_service_deploy_scopes: Vec<&str> = [SCOPE_SERVICE_RESTART, SCOPE_SERVICE_DEPLOY]
        .into_iter()
        .filter(|scope| !has_scope(scope))
        .collect();
    let missing_diagnostics_scopes: Vec<&str> = [SCOPE_DIAGNOSTICS_READ]
        .into_iter()
        .filter(|scope| !has_scope(scope))
        .collect();
    let missing_plugin_mutate_scopes: Vec<&str> = [
        webcodex_core::authority::SCOPE_PLUGIN_INVOKE,
        SCOPE_PLUGIN_MUTATE,
    ]
    .into_iter()
    .filter(|scope| !has_scope(scope))
    .collect();

    json!({
        "principal_kind": auth.map(AuthContext::principal_kind).unwrap_or("internal"),
        "credential_scope_gate": if auth.is_some() { "explicit" } else { "not_applicable" },
        "capabilities": {
            "runtime_read": has_scope(SCOPE_RUNTIME_READ),
            "diagnostics_read": has_scope(SCOPE_DIAGNOSTICS_READ),
            "project_write": has_scope(SCOPE_PROJECT_WRITE),
            "job_run": job_run,
            "detached_process": detached_process,
            "runner_manage": has_scope(SCOPE_RUNNER_MANAGE),
            "plugin_manage": has_scope(SCOPE_PLUGIN_MANAGE),
            "plugin_mutate": has_scope(SCOPE_PLUGIN_MUTATE),
            "service_restart": service_restart,
            "service_deploy": service_deploy,
        },
        "requirements": {
            "run_detached_process": {
                "authorized": detached_process,
                "required_scopes": [SCOPE_JOB_RUN, SCOPE_JOB_DETACH],
                "missing_scopes": missing_detached_scopes,
            },
            "runtime_diagnostics": {
                "authorized": has_scope(SCOPE_DIAGNOSTICS_READ),
                "required_scopes": [SCOPE_DIAGNOSTICS_READ],
                "missing_scopes": missing_diagnostics_scopes,
            },
            "plugin_mutating_call": {
                "authorized": has_scope(webcodex_core::authority::SCOPE_PLUGIN_INVOKE)
                    && has_scope(SCOPE_PLUGIN_MUTATE),
                "required_scopes": [webcodex_core::authority::SCOPE_PLUGIN_INVOKE, SCOPE_PLUGIN_MUTATE],
                "missing_scopes": missing_plugin_mutate_scopes,
            },
            "service_restart": {
                "authorized": service_restart,
                "required_scopes": [SCOPE_SERVICE_RESTART],
                "missing_scopes": missing_service_restart_scopes,
            },
            "service_deploy": {
                "authorized": service_deploy && service_restart,
                "required_scopes": [SCOPE_SERVICE_RESTART, SCOPE_SERVICE_DEPLOY],
                "missing_scopes": missing_service_deploy_scopes,
            }
        }
    })
}

fn runtime_diagnostics_summary() -> Value {
    let stats = webcodex_core::runtime_diagnostics::stats();
    json!({
        "buffered_count": stats.buffered_count,
        "dropped_count": stats.dropped_count,
        "info_count": stats.info_count,
        "warn_count": stats.warn_count,
        "error_count": stats.error_count,
        "oldest_sequence": stats.oldest_sequence,
        "newest_sequence": stats.newest_sequence,
        "newest_warn_sequence": stats.newest_warn_sequence,
        "newest_error_sequence": stats.newest_error_sequence,
    })
}

fn runtime_diagnostics_health(summary: &Value) -> Value {
    let info_count = summary
        .get("info_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let warn_count = summary
        .get("warn_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let error_count = summary
        .get("error_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let dropped_count = summary
        .get("dropped_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "status": if warn_count > 0 || error_count > 0 || dropped_count > 0 {
            "attention_required"
        } else {
            "ready"
        },
        "info_count": info_count,
        "warn_count": warn_count,
        "error_count": error_count,
        "dropped_count": dropped_count,
        "newest_warn_sequence": summary.get("newest_warn_sequence").cloned().unwrap_or(Value::Null),
        "newest_error_sequence": summary.get("newest_error_sequence").cloned().unwrap_or(Value::Null),
    })
}

fn runner_source_alignment_effective_ready(runner: &Value, server_git_dirty: Option<bool>) -> bool {
    let source = runner.get("source_alignment").unwrap_or(&Value::Null);
    let status = source
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if matches!(status, "aligned" | "current") {
        return true;
    }
    source.get("reason_code").and_then(Value::as_str)
        == Some("dirty_build_prevents_exact_source_alignment")
        && runner
            .get("version_matches_server")
            .and_then(Value::as_bool)
            == Some(true)
        && source
            .get("git_commit_matches_server")
            .and_then(Value::as_bool)
            == Some(true)
        && runner.get("build_git_dirty").and_then(Value::as_bool) == Some(true)
        && server_git_dirty == Some(true)
}

fn source_alignment_effective_ready(compatibility: &Value) -> bool {
    let overall = compatibility
        .pointer("/source_alignment/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if overall == "no_runners" {
        return true;
    }
    let server_git_dirty = compatibility
        .pointer("/server/build/git_dirty")
        .and_then(Value::as_bool);
    let Some(runners) = compatibility.get("runners").and_then(Value::as_array) else {
        return false;
    };
    !runners.is_empty()
        && runners
            .iter()
            .all(|runner| runner_source_alignment_effective_ready(runner, server_git_dirty))
}

fn structured_runtime_health(
    runner_count: usize,
    online_count: usize,
    stale_count: usize,
    recovering_count: usize,
    outcome_unknown_count: usize,
    compatibility_status: &str,
    source_alignment_status: &str,
    source_alignment_ready: bool,
    service_draining: bool,
    supervisor_available: bool,
    deployment_store_available: bool,
    public_url_configured: bool,
    mcp_gateway: crate::gateway_circuit_breaker::GatewayCircuitBreakerStats,
    plugin_gateway: crate::gateway_circuit_breaker::GatewayCircuitBreakerStats,
) -> Value {
    let mut degraded_reasons = Vec::new();
    if runner_count == 0 {
        degraded_reasons.push(json!({
            "code": "no_runners_registered",
            "component": "runner_registry",
        }));
    } else if online_count < runner_count || stale_count > 0 {
        degraded_reasons.push(json!({
            "code": "runner_offline_or_stale",
            "component": "runner_registry",
            "runner_count": runner_count,
            "online_count": online_count,
            "stale_count": stale_count,
        }));
    }
    if recovering_count > 0 {
        degraded_reasons.push(json!({
            "code": "jobs_recovering",
            "component": "jobs",
            "count": recovering_count,
        }));
    }
    if compatibility_status == "version_mismatch" {
        degraded_reasons.push(json!({
            "code": "runner_version_mismatch",
            "component": "version_alignment",
        }));
    }
    if !source_alignment_ready {
        degraded_reasons.push(json!({
            "code": "source_alignment_not_aligned",
            "component": "version_alignment",
            "status": source_alignment_status,
        }));
    }

    let readiness_status = if !deployment_store_available {
        "not_ready"
    } else if !degraded_reasons.is_empty() {
        "degraded"
    } else if service_draining {
        "draining"
    } else {
        "ready"
    };

    json!({
        "liveness": {
            "status": "live",
        },
        "readiness": {
            "status": readiness_status,
            "ready": deployment_store_available && degraded_reasons.is_empty() && !service_draining,
            "accepting_consequential_work": deployment_store_available && degraded_reasons.is_empty() && !service_draining,
        },
        "degraded_reasons": degraded_reasons,
        "components": {
            "deployment_store": {
                "status": if deployment_store_available { "ready" } else { "unavailable" },
            },
            "runner_registry": {
                "status": if runner_count == 0 {
                    "empty"
                } else if online_count < runner_count || stale_count > 0 {
                    "degraded"
                } else {
                    "ready"
                },
                "count": runner_count,
                "online_count": online_count,
                "stale_count": stale_count,
            },
            "jobs": {
                "status": if recovering_count > 0 {
                    "recovering"
                } else if outcome_unknown_count > 0 {
                    "attention_required"
                } else {
                    "ready"
                },
                "recovering_count": recovering_count,
                "outcome_unknown_count": outcome_unknown_count,
            },
            "version_alignment": {
                "compatibility_status": compatibility_status,
                "source_alignment_status": source_alignment_status,
                "source_alignment_effective_ready": source_alignment_ready,
            },
            "mcp_gateway": {
                "status": if mcp_gateway.open_circuits > 0 || mcp_gateway.saturated_keys > 0 { "degraded" } else { "ready" },
                "tracked_keys": mcp_gateway.tracked_keys,
                "open_circuits": mcp_gateway.open_circuits,
                "half_open_probes": mcp_gateway.half_open_probes,
                "total_in_flight": mcp_gateway.total_in_flight,
                "saturated_keys": mcp_gateway.saturated_keys,
            },
            "plugin_gateway": {
                "status": if plugin_gateway.open_circuits > 0 || plugin_gateway.saturated_keys > 0 { "degraded" } else { "ready" },
                "tracked_keys": plugin_gateway.tracked_keys,
                "open_circuits": plugin_gateway.open_circuits,
                "half_open_probes": plugin_gateway.half_open_probes,
                "total_in_flight": plugin_gateway.total_in_flight,
                "saturated_keys": plugin_gateway.saturated_keys,
            },
            "service_lifecycle": {
                "status": if service_draining { "draining" } else { "serving" },
            },
            "service_supervisor": {
                "status": if supervisor_available { "available" } else { "unavailable" },
            },
            "public_tunnel": {
                "status": if public_url_configured { "configured_unverified" } else { "not_configured" },
            },
        },
    })
}

fn deployment_preflight_payload(
    runtime: &Value,
    caller: Value,
    client_id: &str,
    operation: &str,
) -> Value {
    let server_service_control = runtime
        .pointer("/authority/service_control")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let connected = runtime
        .pointer("/focus/connected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let alignment = runtime
        .pointer("/focus/source_alignment/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let active_count = runtime
        .pointer("/jobs/active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let running_count = runtime
        .pointer("/jobs/running_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let queued_count = runtime
        .pointer("/jobs/queued_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let recovering_count = runtime
        .pointer("/jobs/recovering_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let lifecycle = runtime
        .get("service_lifecycle")
        .cloned()
        .unwrap_or_else(|| json!({"draining": false, "generation": 1, "changed_at": 0}));
    let lifecycle_draining = lifecycle
        .get("draining")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let restart_authorized = caller
        .pointer("/capabilities/service_restart")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let deploy_authorized = caller
        .pointer("/capabilities/service_deploy")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut blockers = Vec::new();
    if !server_service_control {
        blockers.push(json!({
            "code": "service_control_disabled",
            "message": "Server authority policy does not permit service lifecycle operations."
        }));
    }
    if !restart_authorized {
        blockers.push(json!({
            "code": "missing_service_restart_scope",
            "message": "Current credential is missing service:restart."
        }));
    }
    let deploy_scope_required = matches!(operation, "deploy" | "rollback");
    if deploy_scope_required && !deploy_authorized {
        blockers.push(json!({
            "code": "missing_service_deploy_scope",
            "message": "Current credential is missing service:deploy for this operation."
        }));
    }
    if !connected {
        blockers.push(json!({
            "code": "target_runner_offline",
            "message": "Target Runner is not currently connected."
        }));
    }
    if matches!(operation, "restart" | "deploy" | "rollback") {
        if runtime
            .pointer("/service_supervisor/available")
            .and_then(Value::as_bool)
            != Some(true)
        {
            blockers.push(json!({
                "code": "service_supervisor_unavailable",
                "message": "This Server is not running under a compatible standalone supervisor."
            }));
        } else if runtime
            .pointer("/service_supervisor/managed_client_id")
            .and_then(Value::as_str)
            != Some(client_id)
        {
            blockers.push(json!({
                "code": "service_supervisor_target_mismatch",
                "message": "The standalone supervisor does not manage the requested Runner client_id."
            }));
        }
    }
    if recovering_count > 0 {
        blockers.push(json!({
            "code": "jobs_recovering",
            "message": "At least one Job is still recovering; resolve recovery before deployment."
        }));
    }

    let mut warnings = Vec::new();
    if alignment != "aligned" {
        warnings.push(json!({
            "code": "source_alignment_not_aligned",
            "message": "Target Runner source alignment is not currently aligned with the Server.",
            "status": alignment,
        }));
    }
    if runtime
        .pointer("/fleet_summary/mixed_builds_present")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        warnings.push(json!({
            "code": "mixed_builds_present",
            "message": "The visible fleet currently contains mixed builds."
        }));
    }
    match runtime
        .pointer("/health/components/public_tunnel/status")
        .and_then(Value::as_str)
    {
        Some("configured_unverified") => warnings.push(json!({
            "code": "public_tunnel_unverified",
            "message": "A public URL is configured but deployment preflight did not obtain recent verified public-origin evidence."
        })),
        Some("degraded") => warnings.push(json!({
            "code": "public_tunnel_degraded",
            "message": "The most recent preflight-managed public-origin probe did not verify the configured public tunnel."
        })),
        Some("stale") => warnings.push(json!({
            "code": "public_tunnel_probe_stale",
            "message": "Cached public-origin evidence is stale; deployment preflight refreshes it before readiness evaluation."
        })),
        _ => {}
    }

    let drain_required = !lifecycle_draining || active_count > 0;
    let ready_to_begin = blockers.is_empty();
    let ready_for_cutover = ready_to_begin && lifecycle_draining && active_count == 0;
    let readiness = if !ready_to_begin {
        "blocked"
    } else if drain_required {
        "drain_required"
    } else {
        "ready"
    };

    json!({
        "readiness": readiness,
        "ready_to_begin": ready_to_begin,
        "ready_for_cutover": ready_for_cutover,
        "drain_required": drain_required,
        "service_lifecycle": lifecycle,
        "state_changed": false,
        "operation": operation,
        "target": {
            "client_id": client_id,
            "connected": connected,
            "source_alignment": alignment,
        },
        "jobs": {
            "active_count": active_count,
            "running_count": running_count,
            "queued_count": queued_count,
            "recovering_count": recovering_count,
        },
        "authority": {
            "server_service_control": server_service_control,
            "caller": caller,
        },
        "blockers": blockers,
        "warnings": warnings,
    })
}

impl ToolRuntime {
    pub(crate) async fn deployment_preflight(
        &self,
        auth: Option<&AuthContext>,
        client_id: String,
        operation: String,
    ) -> ToolResult {
        let client_id = client_id.trim().to_string();
        let operation = operation.trim().to_ascii_lowercase();
        if !matches!(operation.as_str(), "deploy" | "restart" | "rollback") {
            return ToolResult::err_with_output(
                "operation must be one of deploy, restart, rollback",
                json!({
                    "error_kind": "invalid_deployment_operation",
                    "state_changed": false,
                }),
            );
        }
        if client_id.is_empty() || client_id.chars().count() > TARGET_CLIENT_ID_MAX_CHARS {
            return ToolResult::err_with_output(
                "client_id must be 1..=128 characters",
                json!({
                    "error_kind": "invalid_client_id",
                    "state_changed": false,
                }),
            );
        }

        // Refresh bounded public-ingress evidence inside preflight so release
        // workflows do not need a separate model-visible probe turn. Fresh
        // cached evidence is reused by PublicTunnelProbeState::probe.
        let _ = self
            .runtime_info
            .public_tunnel_probe
            .probe(self.runtime_info.configured_public_url.as_deref())
            .await;

        let status = self
            .runtime_status_with_options(auth, false, false, Some(client_id.clone()))
            .await;
        if !status.success {
            return status;
        }

        ToolResult::ok(deployment_preflight_payload(
            &status.output,
            caller_authority_status(auth),
            &client_id,
            &operation,
        ))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ListRunnersOptions {
    pub(crate) client_id: Option<String>,
    pub(crate) client_ids: Option<Vec<String>>,
    pub(crate) include_projects: Option<bool>,
    pub(crate) summary_only: bool,
}

/// Lightweight runtime metadata injected into `ToolRuntime` so observability
/// tools (e.g. `runtime_status`) can report bounded auth/OAuth/public-url state
/// without the runtime holding a full `Config` (which would couple it to HTTP/fs details).
///
/// `configured_public_url` is `None` when `WEBPI_PUBLIC_URL` is unset; the
/// observability output reports this as `null` so a deployer can immediately
/// see that the public URL has not been configured.
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub auth_enabled: bool,
    pub configured_public_url: Option<String>,
    pub(crate) public_tunnel_probe:
        std::sync::Arc<super::public_tunnel_probe::PublicTunnelProbeState>,
    pub oauth2_enabled: bool,
    pub oauth2_shared_key_bridge_enabled: bool,
    pub quic: Option<std::sync::Arc<std::sync::Mutex<crate::config::QuicRuntimeStatus>>>,
}

impl RuntimeInfo {
    /// Build test runtime metadata from the same parsed `Config` used by
    /// production startup, plus the public URL and QUIC runtime configuration.
    #[cfg(test)]
    pub fn from_env() -> Self {
        let config = crate::config::Config::from_env();
        Self::from_config_with_quic_config(&config, &crate::config::QuicServerConfig::from_env())
    }

    pub fn from_config_with_quic_config(
        config: &crate::config::Config,
        quic_cfg: &crate::config::QuicServerConfig,
    ) -> Self {
        let auth_enabled = config.is_auth_enabled();
        let configured_public_url = std::env::var("WEBPI_PUBLIC_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        Self {
            auth_enabled,
            configured_public_url,
            public_tunnel_probe: std::sync::Arc::new(
                super::public_tunnel_probe::PublicTunnelProbeState::default(),
            ),
            oauth2_enabled: config.oauth2.enabled,
            oauth2_shared_key_bridge_enabled: config.oauth2.enabled
                && config.oauth2.shared_key_bridge_enabled,
            quic: Some(std::sync::Arc::new(std::sync::Mutex::new(
                quic_cfg.runtime_status(),
            ))),
        }
    }
}

impl ToolRuntime {
    fn effective_config_status(&self) -> Value {
        json!({
            "auth": {
                "shared_key_enabled": self.runtime_info.auth_enabled
                    && crate::auth::shared_key_enabled(),
                "anonymous_enabled": self.runtime_info.auth_enabled
                    && crate::auth::allow_anonymous_enabled(),
                "oauth2_enabled": self.runtime_info.oauth2_enabled,
                "oauth2_shared_key_bridge_enabled": self.runtime_info.oauth2_shared_key_bridge_enabled,
            },
            "tool_request_trace_mode": crate::config::tool_request_trace_mode().as_str(),
        })
    }

    pub(crate) async fn list_runners(&self, auth: Option<&AuthContext>) -> ToolResult {
        self.list_runners_with_options(auth, ListRunnersOptions::default())
            .await
    }

    pub(crate) async fn list_runners_with_options(
        &self,
        auth: Option<&AuthContext>,
        options: ListRunnersOptions,
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
            if client_ids.len() > LIST_RUNNERS_MAX_CLIENT_IDS {
                return ToolResult::err_with_output(
                    format!(
                        "invalid_client_ids: at most {LIST_RUNNERS_MAX_CLIENT_IDS} client ids are allowed"
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

        let access = crate::runner_http::runner_access_from_auth(auth);
        let mut clients = self
            .runner_registry
            .list_runners_for_auth(access.as_ref())
            .await;
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
        let mut runner_jobs = self
            .runner_registry
            .list_all_jobs_for_auth(access.as_ref())
            .await;
        if options.client_id.is_some() || options.client_ids.is_some() {
            runner_jobs.retain(|job| {
                clients
                    .iter()
                    .any(|client| client.client_id == job.client_id)
            });
        }
        let now = chrono::Utc::now().timestamp();
        let include_projects = options.include_projects.unwrap_or(true);
        let runners: Vec<Value> = if options.summary_only {
            clients
                .iter()
                .map(|client| {
                    json!({
                        "client_id": client.client_id,
                        "agent_instance_id": client.runner_instance_id,
                        "display_name": client.display_name,
                        "status": client.status,
                        "connected": client.connected,
                        "agent_protocol_generation": client.runner_protocol_generation.get(),
                        "capability_negotiation": runner_capability_negotiation(client),
                        "transport": client.transport,
                        "last_seen_age_secs": last_seen_age_secs(client, now),
                        "pending_requests": client.pending_requests,
                        "projects_count": enabled_projects_count(client),
                        "project_inventory": client.project_inventory,
                        "active_jobs": active_jobs_for_client(&runner_jobs, &client.client_id),
                        "job_concurrency": job_concurrency_for_client(client, &runner_jobs),
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
                        "agent_instance_id": client.runner_instance_id,
                        "display_name": client.display_name,
                        "owner": client.owner,
                        "hostname": client.hostname,
                        "host_context": host_context_projection(client.host_context.as_ref()),
                        "status": client.status,
                        "connected": client.connected,
                        "agent_protocol_generation": client.runner_protocol_generation.get(),
                        "capability_negotiation": runner_capability_negotiation(client),
                        "transport": client.transport,
                        "last_seen": client.last_seen,
                        "last_seen_age_secs": last_seen_age_secs(client, now),
                        "pending_requests": client.pending_requests,
                        "projects_count": enabled_projects_count(client),
                        "project_inventory": client.project_inventory,
                        "active_jobs": active_jobs_for_client(&runner_jobs, &client.client_id),
                        "job_concurrency": job_concurrency_for_client(client, &runner_jobs),
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
                // Runtime Console, admin/ops, and status projections consume this established key.
                "agents": runners,
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
            // Runtime Console, admin/ops, and status projections consume this established key.
            "agents": runners,
            "clients": runner_health_clients(&clients, &runner_jobs, now),
            "summary": runner_health_summary(&clients, &runner_jobs, now),
            "count": clients.len(),
        }))
    }

    /// Build the runtime observability summary. Read-only; never exposes
    /// tokens, api keys, full env, complete project path lists, or
    /// stdout/stderr. Returns a structured JSON object with service metadata,
    /// Runner-registered Project status, Runner summaries, and Job counts.
    pub(crate) async fn runtime_status(&self, auth: Option<&AuthContext>) -> ToolResult {
        let access = crate::runner_http::runner_access_from_auth(auth);
        let clients = self
            .runner_registry
            .list_runners_for_auth(access.as_ref())
            .await;

        // -- Projects summary -------------------------------------------------
        let runner_registered_count: usize = clients
            .iter()
            .map(|client| {
                client
                    .projects
                    .iter()
                    .filter(|project| !project.disabled)
                    .count()
            })
            .sum();
        let runner_registered_online_count: usize = clients
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
        let effective_count = runner_registered_count;
        let effective_status = if effective_count > 0 {
            "ok"
        } else {
            "no_projects"
        };
        let projects = json!({
            // Stable pre-0.4 runtime_status compatibility identities.
            "mode": "agent_registered",
            "agent_registered": {
                "count": runner_registered_count,
                "online_count": runner_registered_online_count,
            },
            "effective": {
                "count": effective_count,
                "status": effective_status,
            },
            "count": effective_count,
        });

        let now = chrono::Utc::now().timestamp();
        let runner_jobs = self
            .runner_registry
            .list_all_jobs_for_auth(access.as_ref())
            .await;

        // -- Runners summary --------------------------------------------------
        // Build a trimmed client list so the summary never leaks per-request
        // state. Only carry fields useful for observability. `last_seen` is a
        // unix timestamp (seconds) of the most recent heartbeat/result; the
        // console uses it to render how stale a Runner is and to make a
        // websocket Runner flipping `online` -> `stale` visually obvious.
        let runner_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        // `stale_count` = registered Runners whose `last_seen` is older than the
        // online window (status == "stale"). Truly offline Runners are removed
        // from the registry on disconnect, so they never appear here.
        let stale_count = runner_count.saturating_sub(online_count);
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.runner_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "host_context": host_context_projection(c.host_context.as_ref()),
                    "connected": c.connected,
                    "agent_protocol_generation": c.runner_protocol_generation.get(),
                    "capability_negotiation": runner_capability_negotiation(c),
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "last_seen_age_secs": last_seen_age_secs(c, now),
                    "pending_requests": c.pending_requests,
                    "active_jobs": active_jobs_for_client(&runner_jobs, &c.client_id),
                    "job_concurrency": job_concurrency_for_client(c, &runner_jobs),
                    "capabilities": c.capabilities,
                    "projects_count": enabled_projects_count(c),
                    "project_inventory": c.project_inventory,
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                    "tool_providers": c.policy.as_ref().and_then(|p| p.tool_providers.as_ref()),
                })
            })
            .collect();
        let runners = json!({
            "count": runner_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "clients": clients_summary,
            "summary": runner_health_summary(&clients, &runner_jobs, now),
        });
        let connection_layers = connection_layers(
            &clients,
            runner_registered_count,
            runner_registered_online_count,
            self.observations.as_ref(),
            auth,
            now,
        );
        let version_compatibility = version_compatibility(&clients);

        // -- jobs summary -----------------------------------------------------
        // Registered Project jobs are Runner-owned. Active includes same-runner
        // `recovering` jobs during restart/reconnect reconciliation.
        let runner_known_count = runner_jobs.len();
        let active_count = runner_jobs
            .iter()
            .filter(|j| webcodex_runner_registry::job_status_is_active(&j.status))
            .count();
        let recovering_count = runner_jobs
            .iter()
            .filter(|job| job.status == "recovering")
            .count();
        let running_count = runner_jobs
            .iter()
            .filter(|job| job_status_is_running(&job.status))
            .count();
        let queued_count = runner_jobs
            .iter()
            .filter(|job| job_status_is_runner_queued(&job.status))
            .count();
        let reconciled_count = runner_jobs
            .iter()
            .filter(|job| job.recovery_state.as_deref() == Some("reconciled"))
            .count();
        let lost_after_reconcile_count = runner_jobs
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
        let phase_counts = operation_phase_counts(&runner_jobs);
        let outcome_unknown_count = phase_counts
            .get("outcome_unknown")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let jobs = json!({
            "count": runner_known_count,
            "active_count": active_count,
            "running_count": running_count,
            "queued_count": queued_count,
            "recovering_count": recovering_count,
            "reconciled_count": reconciled_count,
            "lost_after_reconcile_count": lost_after_reconcile_count,
            "operation_phase_counts": phase_counts,
        });
        let service_lifecycle = self.service_lifecycle.snapshot();
        let supervisor_available = super::service_supervisor::available();
        let diagnostics = runtime_diagnostics_summary();
        let mut health = structured_runtime_health(
            runner_count,
            online_count,
            stale_count,
            recovering_count,
            outcome_unknown_count,
            version_compatibility
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            version_compatibility
                .pointer("/source_alignment/status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            source_alignment_effective_ready(&version_compatibility),
            service_lifecycle.draining,
            supervisor_available,
            self.deployment_db.is_some(),
            self.runtime_info.configured_public_url.is_some(),
            self.mcp_gateway.breaker_stats(),
            self.plugin_gateway.breaker_stats(),
        );
        health["components"]["diagnostics"] = runtime_diagnostics_health(&diagnostics);
        health["components"]["public_tunnel"] = self
            .runtime_info
            .public_tunnel_probe
            .health_projection(self.runtime_info.configured_public_url.is_some());

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
            "service": "webpi",
            "mcp_compact_schemas": crate::model_surface::effective_mcp_compact_schemas(
                crate::config::mcp_compact_schemas_override(),
            ),
            "effective_config": self.effective_config_status(),
            "version": env!("CARGO_PKG_VERSION"),
            "build": crate::build_info::runtime_build_info(),
            "server_time": now,
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            // Runtime Console, admin HTTP, and CLI ops consume this established key.
            "agents": runners,
            "connection_layers": connection_layers,
            "version_compatibility": version_compatibility,
            "jobs": jobs,
            "health": health,
            "diagnostics": diagnostics,
            "tools": tools,
            "authority": permissions::authority_profile_payload(),
            "caller_authority": caller_authority_status(auth),
            "service_lifecycle": service_lifecycle.as_json(),
            "service_supervisor": {
                "available": supervisor_available,
                "managed_client_id": super::service_supervisor::managed_client_id(),
            },
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
        let access = crate::runner_http::runner_access_from_auth(auth);
        let visible_clients = self
            .runner_registry
            .list_runners_for_auth(access.as_ref())
            .await;
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
        let visible_jobs = self
            .runner_registry
            .list_all_jobs_for_auth(access.as_ref())
            .await;
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
        let fleet_server_git_dirty = fleet_compatibility
            .pointer("/server/build/git_dirty")
            .and_then(Value::as_bool);
        let source_mismatched_agents_count = fleet_runners
            .iter()
            .filter(|runner| {
                !runner_source_alignment_effective_ready(runner, fleet_server_git_dirty)
            })
            .count();
        let runner_active = selected_jobs
            .iter()
            .filter(|job| webcodex_runner_registry::job_status_is_active(&job.status))
            .count();
        let running_count = selected_jobs
            .iter()
            .filter(|job| job_status_is_running(&job.status))
            .count();
        let queued_count = selected_jobs
            .iter()
            .filter(|job| job_status_is_runner_queued(&job.status))
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
        let runners = json!({
            "count": 1,
            "online_count": usize::from(client.connected),
            "stale_count": usize::from(!client.connected),
            "clients": [{
                "client_id": client.client_id,
                "agent_instance_id": client.runner_instance_id,
                "display_name": client.display_name,
                "status": client.status,
                "connected": client.connected,
                "agent_protocol_generation": client.runner_protocol_generation.get(),
                "capability_negotiation": runner_capability_negotiation(&client),
                "transport": client.transport,
                "last_seen": client.last_seen,
                "last_seen_age_secs": last_seen_age_secs(&client, now),
                "pending_requests": client.pending_requests,
                "active_jobs": active_jobs_for_client(&selected_jobs, &client.client_id),
                "job_concurrency": job_concurrency_for_client(&client, &selected_jobs),
                "projects_count": project_count,
            }],
            "summary": runner_health_summary(&clients, &selected_jobs, now),
        });
        let phase_counts = operation_phase_counts(&selected_jobs);
        let outcome_unknown_count = phase_counts
            .get("outcome_unknown")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let jobs = json!({
            "count": selected_jobs.len(),
            "active_count": runner_active,
            "running_count": running_count,
            "queued_count": queued_count,
            "recovering_count": recovering_count,
            "reconciled_count": reconciled_count,
            "lost_after_reconcile_count": lost_after_reconcile_count,
            "operation_phase_counts": phase_counts,
        });
        let service_lifecycle = self.service_lifecycle.snapshot();
        let supervisor_available = super::service_supervisor::available();
        let diagnostics = runtime_diagnostics_summary();
        let mut health = structured_runtime_health(
            1,
            usize::from(client.connected),
            usize::from(client.status == "stale"),
            recovering_count,
            outcome_unknown_count,
            target_runner
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            source_alignment
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            source_alignment_effective_ready(&target_compatibility),
            service_lifecycle.draining,
            supervisor_available,
            self.deployment_db.is_some(),
            self.runtime_info.configured_public_url.is_some(),
            self.mcp_gateway.breaker_stats(),
            self.plugin_gateway.breaker_stats(),
        );
        health["components"]["diagnostics"] = runtime_diagnostics_health(&diagnostics);
        health["components"]["public_tunnel"] = self
            .runtime_info
            .public_tunnel_probe
            .health_projection(self.runtime_info.configured_public_url.is_some());
        let specs = registered_tool_specs();
        let tools = json!({
            "count": specs.len(),
            "names": specs.iter().map(|spec| spec.name.clone()).collect::<Vec<_>>(),
        });
        let server_build = crate::build_info::runtime_build_info();
        ToolResult::ok(json!({
            "service": "webpi",
            "mcp_compact_schemas": crate::model_surface::effective_mcp_compact_schemas(
                crate::config::mcp_compact_schemas_override(),
            ),
            "effective_config": self.effective_config_status(),
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
                "agent_instance_id": client.runner_instance_id,
                "build": client.build,
                "project_count": project_count,
                "active_jobs": runner_active,
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
            "agents": runners,
            "version_compatibility": target_compatibility,
            "jobs": jobs,
            "health": health,
            "diagnostics": diagnostics,
            "tools": tools,
            "authority": permissions::authority_profile_payload(),
            "caller_authority": caller_authority_status(auth),
            "service_lifecycle": service_lifecycle.as_json(),
            "service_supervisor": {
                "available": supervisor_available,
                "managed_client_id": super::service_supervisor::managed_client_id(),
            },
        }))
    }
}

pub(crate) fn compact_runtime_status(status: &Value) -> Value {
    if status.get("focus").is_some() {
        return json!({
            "compact": true,
            "service": status.get("service").cloned().unwrap_or_else(|| json!("webpi")),
            "health": status.get("health").cloned().unwrap_or(Value::Null),
            "diagnostics": status.get("diagnostics").cloned().unwrap_or(Value::Null),
            "mcp_compact_schemas": status.get("mcp_compact_schemas").cloned().unwrap_or_else(|| json!(false)),
            "effective_config": status.get("effective_config").cloned().unwrap_or(Value::Null),
            "auth_enabled": status.get("auth_enabled").cloned().unwrap_or_else(|| json!(false)),
            "configured_public_url": status.get("configured_public_url").cloned().unwrap_or(Value::Null),
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
                "operation_phase_counts": status.pointer("/jobs/operation_phase_counts").cloned().unwrap_or_else(|| json!({
                    "accepted": 0,
                    "queued": 0,
                    "running": 0,
                    "waiting_external": 0,
                    "recovering": 0,
                    "succeeded": 0,
                    "failed": 0,
                    "rolled_back": 0,
                    "outcome_unknown": 0,
                })),
            },
            "version_compatibility": {
                "status": status.pointer("/version_compatibility/status").cloned().unwrap_or_else(|| json!("unknown")),
                "source_alignment": status.pointer("/version_compatibility/source_alignment").cloned().unwrap_or_else(|| json!({"status": "unknown"})),
            },
            "caller_authority": status.get("caller_authority").cloned().unwrap_or(Value::Null),
            "service_lifecycle": status.get("service_lifecycle").cloned().unwrap_or(Value::Null),
            "service_supervisor": status.get("service_supervisor").cloned().unwrap_or(Value::Null),
        });
    }
    let mut compact = json!({
        "compact": true,
        "service": status.get("service").cloned().unwrap_or_else(|| json!("webpi")),
        "health": status.get("health").cloned().unwrap_or(Value::Null),
        "diagnostics": status.get("diagnostics").cloned().unwrap_or(Value::Null),
        "mcp_compact_schemas": status.get("mcp_compact_schemas").cloned().unwrap_or_else(|| json!(false)),
        "effective_config": status.get("effective_config").cloned().unwrap_or(Value::Null),
        "auth_enabled": status.get("auth_enabled").cloned().unwrap_or_else(|| json!(false)),
        "configured_public_url": status.get("configured_public_url").cloned().unwrap_or(Value::Null),
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
            "operation_phase_counts": status.pointer("/jobs/operation_phase_counts").cloned().unwrap_or_else(|| json!({
                "accepted": 0,
                "queued": 0,
                "running": 0,
                "waiting_external": 0,
                "recovering": 0,
                "succeeded": 0,
                "failed": 0,
                "rolled_back": 0,
                "outcome_unknown": 0,
            })),
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
            "last_successful_tool_call": {"status": "not_observed"},
        })),
        "version_compatibility": {
            "status": status.pointer("/version_compatibility/status").cloned().unwrap_or_else(|| json!("unknown")),
            "source_alignment": status.pointer("/version_compatibility/source_alignment").cloned().unwrap_or_else(|| json!({"status": "unknown"})),
        },
        "authority": status.get("authority").cloned().unwrap_or(Value::Null),
        "caller_authority": status.get("caller_authority").cloned().unwrap_or(Value::Null),
        "service_lifecycle": status.get("service_lifecycle").cloned().unwrap_or(Value::Null),
        "service_supervisor": status.get("service_supervisor").cloned().unwrap_or(Value::Null),
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
    let runners = status
        .pointer("/agents/clients")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let compatibility = status
        .pointer("/version_compatibility/runners")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    runners
        .iter()
        .filter_map(|runner| {
            let client_id = runner.get("client_id")?.as_str()?;
            let compat = compatibility
                .iter()
                .find(|candidate| candidate.get("client_id").and_then(Value::as_str) == Some(client_id));
            let mut compact = json!({
                "client_id": client_id,
                "agent_instance_id": runner.get("agent_instance_id").cloned().unwrap_or(Value::Null),
                "status": runner.get("status").cloned().unwrap_or(Value::Null),
                "transport": runner.get("transport").cloned().unwrap_or(Value::Null),
                "build_git_commit": compat.and_then(|value| value.get("build_git_commit")).cloned().unwrap_or(Value::Null),
                "build_git_dirty": compat.and_then(|value| value.get("build_git_dirty")).cloned().unwrap_or(Value::Null),
                "version_matches_server": compat.and_then(|value| value.get("version_matches_server")).cloned().unwrap_or(Value::Null),
                "source_alignment": compat.and_then(|value| value.get("source_alignment")).cloned().unwrap_or_else(|| json!({"status": "unknown"})),
            });
            if status.get("focus").is_none() {
                compact["host_context"] = runner.get("host_context").cloned().unwrap_or(Value::Null);
            }
            Some(compact)
        })
        .collect()
}

fn host_context_projection(context: Option<&crate::runner_protocol::RunnerHostContext>) -> Value {
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
const RUNNER_STALE_AFTER_SECS: i64 = crate::runner_http::RUNNER_ONLINE_WINDOW_SECS;
/// Stale threshold for connector/tool-call activity observations.
const ACTIVITY_STALE_AFTER_SECS: i64 = 600;

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
    clients: &[RunnerView],
    registered_projects: usize,
    online_projects: usize,
    observations: &super::observations::RuntimeObservations,
    auth: Option<&AuthContext>,
    now: i64,
) -> Value {
    // Freshest client drives single-value observations; counts stay explicit.
    let freshest = clients.iter().max_by_key(|c| c.last_seen);
    let online: Vec<&RunnerView> = clients.iter().filter(|c| c.connected).collect();
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
                    "agent_instance_id": client.runner_instance_id,
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
                "agent_instance_id": client.runner_instance_id,
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
                "connection_instance": client.runner_instance_id,
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
                "connection_instance": client.runner_instance_id,
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
                "runner_instance": client.runner_instance_id,
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
                "runner_instance": client.runner_instance_id,
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
                "providing_instance": freshest_online.map(|c| c.runner_instance_id.clone()),
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

    // -- last_successful_tool_call: scoped meaningful activity ----------------
    let principal = super::session_context::runtime_observation_principal(auth).ok();
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
        "last_successful_tool_call": last_successful_tool_call,
    })
}

/// Mixed-version diagnostics: a connected runner is not automatically
/// capability-compatible. Reports facts about which side to upgrade without
/// exposing paths or environment.
fn version_compatibility(clients: &[RunnerView]) -> Value {
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
    clients: &[RunnerView],
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

            let (status, reason_code, action) = if version_matches_server == Some(false) {
                (
                    "version_mismatch",
                    Some("runner_version_differs_from_server"),
                    Some("align server and runner package versions (redeploy the older side)"),
                )
            } else {
                ("compatible", None, None)
            };
            if status == "version_mismatch" {
                overall = "version_mismatch";
            }
            json!({
                "client_id": client.client_id,
                "agent_protocol_generation": client.runner_protocol_generation.get(),
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
    clients: &[RunnerView],
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

fn enabled_projects_count(client: &RunnerView) -> usize {
    client
        .projects
        .iter()
        .filter(|project| !project.disabled)
        .count()
}

fn last_seen_age_secs(client: &RunnerView, now: i64) -> i64 {
    now.saturating_sub(client.last_seen)
}

fn active_jobs_for_client(runner_jobs: &[ShellJobInfo], client_id: &str) -> usize {
    runner_jobs
        .iter()
        .filter(|job| {
            job.client_id == client_id
                && webcodex_runner_registry::job_status_is_active(&job.status)
        })
        .count()
}

fn job_status_is_running(status: &str) -> bool {
    matches!(
        RunnerJobLifecycle::from_wire(status),
        Ok(RunnerJobLifecycle::Running | RunnerJobLifecycle::StartedLegacy)
    )
}

fn job_status_is_runner_queued(status: &str) -> bool {
    matches!(
        RunnerJobLifecycle::from_wire(status),
        Ok(RunnerJobLifecycle::Queued | RunnerJobLifecycle::RunnerQueued)
    )
}

fn job_concurrency_for_client(client: &RunnerView, runner_jobs: &[ShellJobInfo]) -> Value {
    let mut running = 0usize;
    let mut queued = 0usize;
    for job in runner_jobs
        .iter()
        .filter(|job| job.client_id == client.client_id)
    {
        running += usize::from(job_status_is_running(&job.status));
        queued += usize::from(job_status_is_runner_queued(&job.status));
    }
    json!({
        "limit": client.job_concurrency_limit,
        "running": running,
        "queued": queued,
    })
}

fn runner_health_clients(
    clients: &[RunnerView],
    runner_jobs: &[ShellJobInfo],
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
                "project_inventory": client.project_inventory,
                "pending_requests": client.pending_requests,
                "active_jobs": active_jobs_for_client(runner_jobs, &client.client_id),
                "job_concurrency": job_concurrency_for_client(client, runner_jobs),
            })
        })
        .collect()
}

fn runner_health_summary(clients: &[RunnerView], runner_jobs: &[ShellJobInfo], now: i64) -> Value {
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
        "clients": runner_health_clients(clients, runner_jobs, now),
    })
}

/// Build the sanitized policy summary JSON exposed in `runtime_status` and
/// `list_runners`. Only the safe fields are carried: `allow_raw_shell`,
/// `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`,
/// `max_output_bytes`. The agent token, shell env values, init_script
/// contents, and full Runner config contents are NEVER included. Older agents
/// that registered without a policy produce `Value::Null` so the field is
/// present-but-null for clients that expect it.
fn sanitized_policy_summary(policy: Option<&crate::runner_protocol::RunnerPolicySummary>) -> Value {
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
/// `runtime_status`, `list_runners`, and `list_projects`. Only safe metadata is
/// carried: default profile name, configured count, prepared-cache count, and
/// per-profile name / has_init_script (boolean) / env_keys_count / program /
/// args_count. NEVER includes init_script bodies, env values, tokens, or the
/// full env snapshot. Older agents that did not report a summary produce
/// `Value::Null`.
fn sanitized_shell_profiles_summary(
    summary: Option<&crate::runner_protocol::ShellProfilesSummary>,
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
            public_tunnel_probe: std::sync::Arc::new(
                super::public_tunnel_probe::PublicTunnelProbeState::default(),
            ),
            oauth2_enabled: false,
            oauth2_shared_key_bridge_enabled: false,
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
            assert!(!job_status_is_runner_queued(status), "{status}");
        }
        for status in ["queued", "agent_queued"] {
            assert!(job_status_is_runner_queued(status), "{status}");
            assert!(!job_status_is_running(status), "{status}");
        }
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
            assert!(!job_status_is_runner_queued(status), "{status}");
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
                operation_phase: None,
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
                test_count_evidence: None,
                activity: None,
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

        let client = RunnerView {
            client_id: "shared-runner".to_string(),
            runner_instance_id: "shared-instance".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            status: "online".to_string(),
            connected: true,
            last_seen: 0,
            capabilities: Default::default(),
            coding_agent_providers: None,
            pending_requests: 0,
            projects: Vec::new(),
            project_inventory: None,
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
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

    fn healthy_breaker_stats() -> crate::gateway_circuit_breaker::GatewayCircuitBreakerStats {
        crate::gateway_circuit_breaker::GatewayCircuitBreakerStats::default()
    }

    #[test]
    fn dirty_same_build_is_effectively_ready_without_rewriting_raw_alignment() {
        let compatibility = json!({
            "status": "compatible",
            "source_alignment": {"status": "different"},
            "server": {
                "version": "0.4.1",
                "build": {"git_commit": "abc", "git_dirty": true}
            },
            "runners": [{
                "client_id": "runner-a",
                "build_version": "0.4.1",
                "build_git_commit": "abc",
                "build_git_dirty": true,
                "version_matches_server": true,
                "status": "compatible",
                "source_alignment": {
                    "status": "different",
                    "git_commit_matches_server": true,
                    "source_matches_server": false,
                    "reason_code": "dirty_build_prevents_exact_source_alignment"
                }
            }]
        });
        assert!(source_alignment_effective_ready(&compatibility));

        let health = structured_runtime_health(
            1,
            1,
            0,
            0,
            0,
            "compatible",
            "different",
            true,
            false,
            true,
            true,
            false,
            healthy_breaker_stats(),
            healthy_breaker_stats(),
        );
        assert_eq!(health["readiness"]["status"], "ready");
        assert_eq!(health["readiness"]["accepting_consequential_work"], true);
        assert_eq!(
            health["components"]["version_alignment"]["source_alignment_status"],
            "different"
        );
        assert_eq!(
            health["components"]["version_alignment"]["source_alignment_effective_ready"],
            true
        );
        assert_eq!(health["degraded_reasons"], json!([]));
    }

    #[test]
    fn dirty_build_identity_mismatch_remains_degraded() {
        let compatibility = json!({
            "status": "compatible",
            "source_alignment": {"status": "different"},
            "server": {"build": {"git_dirty": true}},
            "runners": [{
                "version_matches_server": true,
                "build_git_dirty": true,
                "source_alignment": {
                    "status": "different",
                    "git_commit_matches_server": false,
                    "reason_code": "runner_git_commit_differs_from_server"
                }
            }]
        });
        assert!(!source_alignment_effective_ready(&compatibility));
    }

    #[test]
    fn diagnostic_attention_is_component_scoped_not_global_readiness() {
        let summary = json!({
            "buffered_count": 4,
            "dropped_count": 1,
            "info_count": 1,
            "warn_count": 2,
            "error_count": 1,
            "oldest_sequence": 2,
            "newest_sequence": 5,
            "newest_warn_sequence": 4,
            "newest_error_sequence": 5,
        });
        let component = runtime_diagnostics_health(&summary);
        assert_eq!(component["status"], "attention_required");
        assert_eq!(component["warn_count"], 2);
        assert_eq!(component["error_count"], 1);
        assert_eq!(component["dropped_count"], 1);
        assert_eq!(component["newest_warn_sequence"], 4);
        assert_eq!(component["newest_error_sequence"], 5);

        let health = structured_runtime_health(
            1,
            1,
            0,
            0,
            0,
            "compatible",
            "aligned",
            true,
            false,
            true,
            true,
            false,
            healthy_breaker_stats(),
            healthy_breaker_stats(),
        );
        assert_eq!(health["readiness"]["status"], "ready");
        assert_eq!(health["degraded_reasons"], json!([]));
    }

    #[test]
    fn outcome_unknown_requires_attention_without_degrading_runtime_readiness() {
        let health = structured_runtime_health(
            1,
            1,
            0,
            0,
            2,
            "compatible",
            "aligned",
            true,
            false,
            true,
            true,
            false,
            healthy_breaker_stats(),
            healthy_breaker_stats(),
        );
        assert_eq!(health["readiness"]["status"], "ready");
        assert_eq!(health["readiness"]["ready"], true);
        assert_eq!(health["components"]["jobs"]["status"], "attention_required");
        assert_eq!(health["components"]["jobs"]["outcome_unknown_count"], 2);
        assert_eq!(health["degraded_reasons"], json!([]));
    }

    #[test]
    fn recovering_jobs_take_priority_over_unknown_outcome_attention() {
        let health = structured_runtime_health(
            1,
            1,
            0,
            1,
            2,
            "compatible",
            "aligned",
            true,
            false,
            true,
            true,
            false,
            healthy_breaker_stats(),
            healthy_breaker_stats(),
        );
        assert_eq!(health["readiness"]["status"], "degraded");
        assert_eq!(health["components"]["jobs"]["status"], "recovering");
        assert_eq!(health["components"]["jobs"]["recovering_count"], 1);
        assert_eq!(health["components"]["jobs"]["outcome_unknown_count"], 2);
    }

    #[test]
    fn gateway_breaker_degradation_is_component_scoped_not_global_readiness() {
        let health = structured_runtime_health(
            1,
            1,
            0,
            0,
            0,
            "compatible",
            "aligned",
            true,
            false,
            true,
            true,
            false,
            crate::gateway_circuit_breaker::GatewayCircuitBreakerStats {
                tracked_keys: 2,
                open_circuits: 1,
                half_open_probes: 0,
                total_in_flight: 3,
                saturated_keys: 0,
            },
            crate::gateway_circuit_breaker::GatewayCircuitBreakerStats::default(),
        );
        assert_eq!(health["readiness"]["status"], "ready");
        assert_eq!(health["readiness"]["ready"], true);
        assert_eq!(health["components"]["mcp_gateway"]["status"], "degraded");
        assert_eq!(health["components"]["mcp_gateway"]["open_circuits"], 1);
        assert_eq!(health["components"]["plugin_gateway"]["status"], "ready");
        assert_eq!(health["degraded_reasons"], json!([]));
    }

    #[test]
    fn caller_authority_reports_missing_detached_scope_without_widening_authority() {
        let mut auth = AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.scopes = vec![webcodex_core::authority::SCOPE_JOB_RUN.to_string()];
        let status = caller_authority_status(Some(&auth));
        assert_eq!(status["principal_kind"], "oauth2");
        assert_eq!(status["capabilities"]["job_run"], true);
        assert_eq!(status["capabilities"]["detached_process"], false);
        assert_eq!(
            status["requirements"]["run_detached_process"]["missing_scopes"],
            json!([webcodex_core::authority::SCOPE_JOB_DETACH])
        );
    }

    #[test]
    fn caller_authority_accepts_exact_detached_scope_pair() {
        let mut auth = AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.scopes = vec![
            webcodex_core::authority::SCOPE_JOB_RUN.to_string(),
            webcodex_core::authority::SCOPE_JOB_DETACH.to_string(),
        ];
        let status = caller_authority_status(Some(&auth));
        assert_eq!(status["capabilities"]["detached_process"], true);
        assert_eq!(
            status["requirements"]["run_detached_process"]["missing_scopes"],
            json!([])
        );
    }

    #[test]
    fn caller_authority_keeps_service_deploy_narrower_than_restart() {
        let mut auth = AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.scopes = vec![webcodex_core::authority::SCOPE_SERVICE_RESTART.to_string()];
        let status = caller_authority_status(Some(&auth));
        assert_eq!(status["capabilities"]["service_restart"], true);
        assert_eq!(status["capabilities"]["service_deploy"], false);
        assert_eq!(
            status["requirements"]["service_deploy"]["missing_scopes"],
            json!([webcodex_core::authority::SCOPE_SERVICE_DEPLOY])
        );
    }

    #[test]
    fn restart_preflight_does_not_require_service_deploy_scope() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {"connected": true, "source_alignment": {"status": "aligned"}},
            "service_lifecycle": {"draining": false, "generation": 1, "changed_at": 1},
            "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
            "jobs": {"active_count": 0, "running_count": 0, "queued_count": 0, "recovering_count": 0},
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({"capabilities": {"service_restart": true, "service_deploy": false}});
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-a", "restart");
        let codes: Vec<&str> = preflight["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .filter_map(|item| item["code"].as_str())
            .collect();
        assert!(!codes.contains(&"missing_service_deploy_scope"));
        assert_eq!(preflight["ready_to_begin"], true);
        assert_eq!(preflight["operation"], "restart");
    }

    #[test]
    fn restart_preflight_blocks_supervisor_target_mismatch() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {"connected": true, "source_alignment": {"status": "aligned"}},
            "service_lifecycle": {"draining": true, "generation": 2, "changed_at": 1},
            "service_supervisor": {"available": true, "managed_client_id": "runner-local"},
            "jobs": {"active_count": 0, "running_count": 0, "queued_count": 0, "recovering_count": 0},
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({"capabilities": {"service_restart": true, "service_deploy": false}});
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-remote", "restart");
        let codes: Vec<&str> = preflight["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .filter_map(|item| item["code"].as_str())
            .collect();
        assert!(codes.contains(&"service_supervisor_target_mismatch"));
        assert_eq!(preflight["ready_to_begin"], false);
        assert_eq!(preflight["ready_for_cutover"], false);
    }

    #[test]
    fn deploy_preflight_blocks_supervisor_target_mismatch() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {"connected": true, "source_alignment": {"status": "aligned"}},
            "service_lifecycle": {"draining": true, "generation": 2, "changed_at": 1},
            "service_supervisor": {"available": true, "managed_client_id": "runner-local"},
            "jobs": {"active_count": 0, "running_count": 0, "queued_count": 0, "recovering_count": 0},
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({"capabilities": {"service_restart": true, "service_deploy": true}});
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-remote", "deploy");
        let codes: Vec<&str> = preflight["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .filter_map(|item| item["code"].as_str())
            .collect();
        assert!(codes.contains(&"service_supervisor_target_mismatch"));
        assert_eq!(preflight["ready_for_cutover"], false);
    }

    #[test]
    fn deployment_preflight_requires_drain_for_active_jobs() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {
                "connected": true,
                "source_alignment": {"status": "aligned"}
            },
            "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
            "jobs": {
                "active_count": 2,
                "running_count": 1,
                "queued_count": 1,
                "recovering_count": 0
            },
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({
            "capabilities": {
                "service_restart": true,
                "service_deploy": true
            }
        });
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-a", "deploy");
        assert_eq!(preflight["readiness"], "drain_required");
        assert_eq!(preflight["ready_to_begin"], true);
        assert_eq!(preflight["ready_for_cutover"], false);
        assert_eq!(preflight["drain_required"], true);
        assert_eq!(preflight["blockers"], json!([]));
    }

    #[test]
    fn deployment_preflight_is_ready_only_after_drain_and_zero_active_jobs() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {
                "connected": true,
                "source_alignment": {"status": "aligned"}
            },
            "service_lifecycle": {
                "draining": true,
                "generation": 2,
                "changed_at": 123
            },
            "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
            "jobs": {
                "active_count": 0,
                "running_count": 0,
                "queued_count": 0,
                "recovering_count": 0
            },
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({
            "capabilities": {
                "service_restart": true,
                "service_deploy": true
            }
        });
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-a", "deploy");
        assert_eq!(preflight["readiness"], "ready");
        assert_eq!(preflight["ready_to_begin"], true);
        assert_eq!(preflight["ready_for_cutover"], true);
        assert_eq!(preflight["drain_required"], false);
        assert_eq!(preflight["service_lifecycle"]["generation"], 2);
    }

    #[test]
    fn deployment_preflight_public_tunnel_evidence_is_warning_only() {
        let caller = json!({
            "capabilities": {
                "service_restart": true,
                "service_deploy": true
            }
        });
        for (status, expected_code) in [
            ("configured_unverified", "public_tunnel_unverified"),
            ("degraded", "public_tunnel_degraded"),
            ("stale", "public_tunnel_probe_stale"),
        ] {
            let runtime = json!({
                "authority": {"service_control": true},
                "focus": {
                    "connected": true,
                    "source_alignment": {"status": "aligned"}
                },
                "service_lifecycle": {
                    "draining": true,
                    "generation": 2,
                    "changed_at": 123
                },
                "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
                "jobs": {
                    "active_count": 0,
                    "running_count": 0,
                    "queued_count": 0,
                    "recovering_count": 0
                },
                "fleet_summary": {"mixed_builds_present": false},
                "health": {"components": {"public_tunnel": {"status": status}}}
            });
            let preflight =
                deployment_preflight_payload(&runtime, caller.clone(), "runner-a", "deploy");
            assert_eq!(preflight["readiness"], "ready", "{status}");
            assert_eq!(preflight["ready_to_begin"], true, "{status}");
            assert_eq!(preflight["ready_for_cutover"], true, "{status}");
            assert_eq!(preflight["blockers"], json!([]), "{status}");
            let codes: Vec<&str> = preflight["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .filter_map(|item| item["code"].as_str())
                .collect();
            assert!(codes.contains(&expected_code), "{status}: {codes:?}");
        }

        let verified = json!({
            "authority": {"service_control": true},
            "focus": {"connected": true, "source_alignment": {"status": "aligned"}},
            "service_lifecycle": {"draining": true, "generation": 2, "changed_at": 123},
            "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
            "jobs": {"active_count": 0, "running_count": 0, "queued_count": 0, "recovering_count": 0},
            "fleet_summary": {"mixed_builds_present": false},
            "health": {"components": {"public_tunnel": {"status": "verified"}}}
        });
        let preflight = deployment_preflight_payload(&verified, caller, "runner-a", "deploy");
        let codes: Vec<&str> = preflight["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .filter_map(|item| item["code"].as_str())
            .collect();
        assert!(!codes.iter().any(|code| code.starts_with("public_tunnel_")));
    }

    #[test]
    fn deployment_preflight_blocks_missing_service_scope_and_recovery() {
        let runtime = json!({
            "authority": {"service_control": true},
            "focus": {
                "connected": true,
                "source_alignment": {"status": "aligned"}
            },
            "service_supervisor": {"available": true, "managed_client_id": "runner-a"},
            "jobs": {
                "active_count": 1,
                "running_count": 0,
                "queued_count": 0,
                "recovering_count": 1
            },
            "fleet_summary": {"mixed_builds_present": false}
        });
        let caller = json!({
            "capabilities": {
                "service_restart": true,
                "service_deploy": false
            }
        });
        let preflight = deployment_preflight_payload(&runtime, caller, "runner-a", "deploy");
        assert_eq!(preflight["readiness"], "blocked");
        let codes: Vec<&str> = preflight["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .filter_map(|item| item["code"].as_str())
            .collect();
        assert!(codes.contains(&"missing_service_deploy_scope"));
        assert!(codes.contains(&"jobs_recovering"));
    }

    #[test]
    fn compact_runtime_status_keeps_minimum_job_state_counts() {
        let compact = compact_runtime_status(&json!({
            "diagnostics": {
                "buffered_count": 4,
                "dropped_count": 1,
                "info_count": 1,
                "warn_count": 2,
                "error_count": 1,
                "oldest_sequence": 2,
                "newest_sequence": 5,
                "newest_warn_sequence": 4,
                "newest_error_sequence": 5
            },
            "jobs": {
                "active_count": 5,
                "running_count": 2,
                "queued_count": 1,
                "operation_phase_counts": {
                    "accepted": 0,
                    "queued": 1,
                    "running": 2,
                    "waiting_external": 0,
                    "recovering": 1,
                    "succeeded": 1,
                    "failed": 0,
                    "rolled_back": 0,
                    "outcome_unknown": 0,
                }
            }
        }));
        assert_eq!(
            compact["jobs"],
            json!({
                "active_count": 5,
                "running_count": 2,
                "queued_count": 1,
                "operation_phase_counts": {
                    "accepted": 0,
                    "queued": 1,
                    "running": 2,
                    "waiting_external": 0,
                    "recovering": 1,
                    "succeeded": 1,
                    "failed": 0,
                    "rolled_back": 0,
                    "outcome_unknown": 0,
                }
            })
        );
        assert_eq!(
            compact["diagnostics"],
            json!({
                "buffered_count": 4,
                "dropped_count": 1,
                "info_count": 1,
                "warn_count": 2,
                "error_count": 1,
                "oldest_sequence": 2,
                "newest_sequence": 5,
                "newest_warn_sequence": 4,
                "newest_error_sequence": 5
            })
        );
    }
}
