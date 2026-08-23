//! Agent-side project management tools: `register_project`, `unregister_project`,
//! `create_project`, and the internal Runner-managed temporary-project path used
//! by coding-task startup.
//!
//! Registration and creation route to the selected agent through the project-op
//! path. Unregistration reuses the shared project lifecycle path so the
//! model-facing tool and `POST /api/projects/unregister` have the same revision
//! CAS, active-Job fence, capability check, uncertain-delivery semantics, and
//! server inventory update. The Runner remains authoritative for its local
//! `projects.d` registration state.
//!
//! The server never writes project config files or creates directories on the
//! agent host directly. OS permissions and agent policy
//! (`allow_cwd_anywhere` / `allowed_roots`) remain the real boundary; there is
//! no workspace abstraction.

use serde_json::{json, Value};
use std::time::Duration;

use super::tool_result::{RecoveryKind, ToolResult};
use super::{agent_project_runtime_id, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::{
    ShellAgentProjectSummary, ShellClientView, SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION,
};

/// Maximum time the runtime waits for an agent project-op response. Project
/// operations are fast (write a small TOML, maybe create a directory + git
/// init), so 30s is generous while still bounding the caller.
const PROJECT_OP_WAIT_SECS: u64 = 32;
const MANAGED_TEMPORARY_PROJECT_SOURCE: &str = "managed_temporary";
const AUTO_REGISTERED_PROJECT_SOURCE: &str = "auto_registered";

const LIST_PROJECTS_MAX_QUERY_CHARS: usize = 200;
const LIST_PROJECTS_MAX_RESULTS: usize = 100;

#[derive(Debug, Default)]
pub(crate) struct ListProjectsOptions {
    pub(crate) client_id: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) summary_only: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProjectCandidate {
    pub(super) runtime_id: String,
    pub(super) client_index: usize,
    pub(super) project_index: usize,
}

fn validate_list_projects_options(
    options: &ListProjectsOptions,
) -> Result<(Option<String>, Option<usize>), ToolResult> {
    if options
        .client_id
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.chars().count() > 128)
    {
        return Err(ToolResult::err_with_output(
            "invalid_client_id: client_id must contain 1..=128 characters".to_string(),
            json!({"error_kind": "invalid_client_id"}),
        ));
    }
    if options
        .project
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.chars().count() > 512)
    {
        return Err(ToolResult::err_with_output(
            "invalid_project: project must contain 1..=512 characters".to_string(),
            json!({"error_kind": "invalid_project"}),
        ));
    }
    let query = match options.query.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ToolResult::err_with_output(
                    "invalid_query: query must not be empty after trimming".to_string(),
                    json!({"error_kind": "invalid_query"}),
                ));
            }
            if trimmed.chars().count() > LIST_PROJECTS_MAX_QUERY_CHARS {
                return Err(ToolResult::err_with_output(
                    format!(
                        "invalid_query: query exceeds {LIST_PROJECTS_MAX_QUERY_CHARS} characters"
                    ),
                    json!({"error_kind": "invalid_query"}),
                ));
            }
            Some(trimmed.to_lowercase())
        }
        None => None,
    };
    let filtered =
        options.client_id.is_some() || options.project.is_some() || options.query.is_some();
    let limit = options
        .limit
        .map(|limit| limit.clamp(1, LIST_PROJECTS_MAX_RESULTS))
        .or_else(|| filtered.then_some(LIST_PROJECTS_MAX_RESULTS));
    Ok((query, limit))
}

fn project_candidates(
    clients: &[ShellClientView],
    options: &ListProjectsOptions,
    query: Option<&str>,
) -> Vec<ProjectCandidate> {
    let mut candidates = Vec::new();
    for (client_index, client) in clients.iter().enumerate() {
        if options
            .client_id
            .as_deref()
            .is_some_and(|expected| client.client_id != expected)
        {
            continue;
        }
        for (project_index, project) in client.projects.iter().enumerate() {
            let runtime_id = agent_project_runtime_id(&client.client_id, &project.id);
            if options
                .project
                .as_deref()
                .is_some_and(|expected| runtime_id != expected)
            {
                continue;
            }
            if query.is_some_and(|needle| !project_query_matches(needle, &runtime_id, project)) {
                continue;
            }
            candidates.push(ProjectCandidate {
                runtime_id,
                client_index,
                project_index,
            });
        }
    }
    candidates.sort_by(|a, b| a.runtime_id.cmp(&b.runtime_id));
    candidates
}

impl ToolRuntime {
    pub(crate) async fn list_projects(&self, auth: Option<&AuthContext>) -> ToolResult {
        self.list_projects_with_options(auth, ListProjectsOptions::default())
            .await
    }

    pub(crate) async fn list_projects_with_options(
        &self,
        auth: Option<&AuthContext>,
        options: ListProjectsOptions,
    ) -> ToolResult {
        let (query, limit) = match validate_list_projects_options(&options) {
            Ok(validated) => validated,
            Err(result) => return result,
        };
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
        self.list_projects_from_visible_clients(auth, &options, query.as_deref(), limit, &clients)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn list_projects_with_visible_clients_for_test(
        &self,
        auth: Option<&AuthContext>,
        options: ListProjectsOptions,
        clients: &[ShellClientView],
    ) -> ToolResult {
        let (query, limit) = match validate_list_projects_options(&options) {
            Ok(validated) => validated,
            Err(result) => return result,
        };
        self.list_projects_from_visible_clients(auth, &options, query.as_deref(), limit, clients)
            .await
    }

    async fn list_projects_from_visible_clients(
        &self,
        auth: Option<&AuthContext>,
        options: &ListProjectsOptions,
        query: Option<&str>,
        limit: Option<usize>,
        clients: &[ShellClientView],
    ) -> ToolResult {
        let mut candidates = project_candidates(clients, options, query);
        let matched_count = candidates.len();
        if let Some(limit) = limit {
            candidates.truncate(limit);
        }
        let truncated = candidates.len() < matched_count;

        let mut list = Vec::with_capacity(candidates.len());
        for ProjectCandidate {
            runtime_id,
            client_index,
            project_index,
        } in candidates
        {
            // Extract only one selected Project and the small Runner fields used by
            // its projection before awaiting Job state. Candidate staging above
            // never owns or clones a ShellClientView (and therefore never clones
            // the Runner's complete projects Vec per match).
            let (
                client_id,
                agent_status,
                connected,
                last_seen,
                project,
                resolved_shell_profile,
                shell_profile_status,
                capabilities,
            ) = {
                let client = &clients[client_index];
                let project = &client.projects[project_index];
                let shell_profiles = client
                    .policy
                    .as_ref()
                    .and_then(|policy| policy.shell_profiles.as_ref());
                let (resolved_shell_profile, shell_profile_status) =
                    resolve_project_shell_profile(project.shell_profile.as_deref(), shell_profiles);
                (
                    client.client_id.clone(),
                    client.status.clone(),
                    client.connected,
                    client.last_seen,
                    project.clone(),
                    resolved_shell_profile,
                    shell_profile_status,
                    smoke_project_capabilities(client, project),
                )
            };
            let active_jobs = self
                .shell_clients
                .count_active_jobs_for_project(auth, &runtime_id)
                .await;
            let value = if options.summary_only {
                json!({
                    "id": runtime_id,
                    "agent_project_id": project.id,
                    "name": project.name,
                    "description": project.description,
                    "executor": "agent",
                    "client_id": client_id,
                    "enabled": !project.disabled,
                    "active_jobs": active_jobs,
                    "source": project_source(&project),
                    "agent_status": agent_status,
                    "connected": connected,
                    "resolved_shell_profile": resolved_shell_profile,
                    "shell_profile_status": shell_profile_status,
                    "capabilities": {
                        "git_available": capabilities["git_available"],
                        "recommended_for_smoke": capabilities["recommended_for_smoke"],
                    },
                })
            } else {
                json!({
                    "id": runtime_id,
                    "agent_project_id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "executor": "agent",
                    "client_id": client_id,
                    "allow_patch": project.allow_patch,
                    "description": project.description,
                    "enabled": !project.disabled,
                    "disabled": project.disabled,
                    "revision": project.revision,
                    "active_jobs": active_jobs,
                    "source": project_source(&project),
                    "agent_status": agent_status,
                    "connected": connected,
                    "last_seen": last_seen,
                    "shell_profile": project.shell_profile,
                    "resolved_shell_profile": resolved_shell_profile,
                    "shell_profile_status": shell_profile_status,
                    "capabilities": capabilities,
                })
            };
            list.push(value);
        }
        let recommended_for_smoke: Vec<Value> = list
            .iter()
            .filter(|project| {
                project["capabilities"]["recommended_for_smoke"]
                    .as_bool()
                    .unwrap_or(false)
            })
            .filter_map(|project| project["id"].as_str().map(|id| json!(id)))
            .collect();
        ToolResult::ok(json!({
            "count": list.len(),
            "matched_count": matched_count,
            "truncated": truncated,
            "projects": list,
            "recommended_for_smoke": recommended_for_smoke,
        }))
    }

    /// Register an existing directory as a WebCodex project on the selected
    /// agent. See the `ToolCall::RegisterProject` doc comment for the full
    /// contract. The server validates the owner boundary, builds a JSON
    /// payload, routes it to the agent, parses the JSON response, and
    /// refreshes the server-side project cache.
    pub(crate) async fn register_project(
        &self,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.project_op(
            "register_project",
            client_id,
            id,
            name,
            path,
            description,
            allow_patch,
            None,
            false,
            false,
            overwrite,
            auth,
        )
        .await
    }

    /// Remove only one exact Runner project registration. The caller supplies
    /// the revision observed from `list_projects`; the shared lifecycle core
    /// keeps CAS, active-Job fencing, owner filtering, and uncertain-delivery
    /// semantics identical to `POST /api/projects/unregister`.
    pub(crate) async fn unregister_project(
        &self,
        project: String,
        expected_revision: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let response = crate::admin_project_lifecycle::unregister_project_runtime(
            self,
            auth,
            &project,
            &expected_revision,
        )
        .await;
        if (200..300).contains(&response.status) {
            return ToolResult::ok(response.body);
        }
        let error_code = response
            .body
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("operation_failed")
            .to_string();
        let message = if error_code == "operation_indeterminate" {
            "operation_indeterminate: unregister may have completed; call list_projects and reconcile the exact project before retrying".to_string()
        } else {
            error_code.clone()
        };
        ToolResult::err_with_output(message, response.body)
    }

    /// Create a new directory on the selected agent and register it as a
    /// WebCodex project. See the `ToolCall::CreateProject` doc comment for the
    /// full contract.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_project(
        &self,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        template: Option<String>,
        git_init: bool,
        allow_existing_empty: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.project_op(
            "create_project",
            client_id,
            id,
            name,
            path,
            description,
            allow_patch,
            template,
            git_init,
            allow_existing_empty,
            overwrite,
            auth,
        )
        .await
    }

    /// Ask a Runner to create a directory under its configured managed
    /// temporary-project root and register it through the ordinary projects.d
    /// lifecycle. The Runner, rather than this server, owns all directory-name
    /// generation, path validation, and filesystem mutation.
    pub(crate) async fn create_managed_temporary_project(
        &self,
        client_id: String,
        name: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Some(name) = name.as_deref() {
            if let Err(error) = validate_project_op_name(name) {
                return ToolResult::err(error);
            }
        }
        self.submit_project_op(
            "create_project",
            client_id.clone(),
            json!({
                "kind": "create_project",
                "client_id": client_id,
                "managed_temporary_project": true,
                "name": name,
            }),
            auth,
        )
        .await
    }

    /// Ask the selected Runner to resolve an existing registration by
    /// canonical path or persist a new projects.d entry under its registry
    /// write lock. This internal operation is intentionally absent from the
    /// model-visible tool registry.
    pub(crate) async fn resolve_or_register_project(
        &self,
        client_id: String,
        path: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_project_op_path(&path) {
            return ToolResult::err_with_output(
                error,
                json!({
                    "error_kind": "invalid_project_path",
                    "failure_kind": "invalid_arguments",
                    "field": "path",
                    "state_changed": false,
                }),
            );
        }
        if let Some(client) = self
            .shell_clients
            .get_client_view_for_auth(&client_id, auth)
            .await
        {
            if let Err(error) = self
                .shell_clients
                .assert_client_access(auth, &client_id)
                .await
            {
                return ToolResult::err(error);
            }
            if !client.capabilities.project_path_registration {
                return ToolResult::err_with_output(
                    "agent_capability_unavailable: the selected Runner does not support project path registration; upgrade the Runner or use an existing registered project id",
                    json!({
                        "error_kind": "agent_capability_unavailable",
                        "failure_kind": "capability_unavailable",
                        "reason_code": "project_path_registration_requires_newer_runner",
                        "capability": SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION,
                        "state_changed": false,
                    }),
                )
                .with_recovery(RecoveryKind::NoAction, None);
            }
        }
        self.submit_project_op(
            "resolve_or_register_project",
            client_id,
            json!({"path": path}),
            auth,
        )
        .await
    }

    /// Shared implementation for both `register_project` and `create_project`.
    /// `kind` is `"register_project"` or `"create_project"`. Fields not
    /// applicable to `register_project` (template, git_init,
    /// allow_existing_empty) are ignored by the agent for that kind.
    #[allow(clippy::too_many_arguments)]
    async fn project_op(
        &self,
        kind: &str,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        template: Option<String>,
        git_init: bool,
        allow_existing_empty: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        // -- basic server-side request shape validation ----------------------
        // The agent does the authoritative path/policy validation, but the
        // server rejects obviously malformed requests early so the agent is
        // never bothered with them.
        if let Err(e) = validate_project_op_id(&id) {
            return ToolResult::err(e);
        }
        if let Err(e) = validate_project_op_name(&name) {
            return ToolResult::err(e);
        }
        if let Some(ref desc) = description {
            if let Err(e) = validate_project_op_description(desc) {
                return ToolResult::err(e);
            }
        }
        if let Err(e) = validate_project_op_path(&path) {
            return ToolResult::err(e);
        }

        let payload = json!({
            "kind": kind,
            "client_id": client_id,
            "id": id,
            "name": name,
            "path": path,
            "description": description,
            "allow_patch": allow_patch,
            "template": template,
            "git_init": git_init,
            "allow_existing_empty": allow_existing_empty,
            "overwrite": overwrite,
        });
        self.submit_project_op(kind, client_id, payload, auth).await
    }

    /// Shared transport, response parsing, cache-upsert, and owner-boundary
    /// path for public project operations and internal managed temporary
    /// project creation.
    async fn submit_project_op(
        &self,
        kind: &str,
        client_id: String,
        payload: Value,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        // -- owner boundary + client existence --------------------------------
        let Some(client_view) = self
            .shell_clients
            .get_client_view_for_auth(&client_id, auth)
            .await
        else {
            return ToolResult::err(format!(
                "unknown agent client '{}'. Call listAgents to discover registered client_ids.",
                client_id
            ));
        };
        let expected_agent_instance_id = client_view.agent_instance_id.clone();
        if let Err(e) = self
            .shell_clients
            .assert_client_access(auth, &client_id)
            .await
        {
            return ToolResult::err(e);
        }

        // -- route the already validated payload to the agent -----------------
        let payload_str = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => {
                return ToolResult::err(format!("failed to serialize project op payload: {}", e))
            }
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_project_op(
                client_id.clone(),
                kind,
                payload_str,
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(result) => result,
            Err(_) => {
                return ToolResult::err_with_output(
                    "agent_unavailable",
                    json!({"error_code":"agent_unavailable"}),
                )
            }
        };
        let response =
            match tokio::time::timeout(Duration::from_secs(PROJECT_OP_WAIT_SECS), rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) | Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    return ToolResult::err_with_output(
                        "operation_indeterminate",
                        json!({"error_code":"operation_indeterminate"}),
                    );
                }
            };

        // -- parse the agent response -----------------------------------------
        // The agent returns structured JSON in stdout. On error, stdout may be
        // empty and the error is in the `error` field.
        if let Some(err) = response.error.as_ref() {
            return ToolResult::err(err.clone());
        }
        let stdout = response.stdout.as_deref().unwrap_or("");
        if stdout.is_empty() {
            return ToolResult::err("agent returned empty project op result");
        }
        let result: Value = match serde_json::from_str::<Value>(stdout) {
            Ok(value) => value,
            Err(error) => {
                return ToolResult::err(format!(
                    "failed to parse agent project op response: {} (stdout: {})",
                    error,
                    truncate_for_error(stdout)
                ))
            }
        };
        if response.exit_code != Some(0) {
            let code = result
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or("operation_failed")
                .to_string();
            return ToolResult::err_with_output(code, result);
        }

        // -- commit authoritative server routing projection -------------------
        // Runner persistence already happened exactly once. The caller may see
        // ordinary success only after the authoritative summary is fenced to the
        // same Runner instance and committed into the Server routing projection.
        let Some(project) = parse_project_summary_from_result(&result, &client_id) else {
            return project_projection_reconcile_required(
                &client_id,
                &expected_agent_instance_id,
                &result,
                "authoritative_project_summary_missing",
            );
        };
        if let Err(error) = self
            .shell_clients
            .upsert_client_project_for_instance(&client_id, &expected_agent_instance_id, project)
            .await
        {
            return project_projection_reconcile_required(
                &client_id,
                &expected_agent_instance_id,
                &result,
                if error.contains("stale or replaced") {
                    "runner_instance_changed_before_projection"
                } else {
                    "server_project_projection_failed"
                },
            );
        }

        ToolResult::ok(result)
    }
}

fn project_projection_reconcile_required(
    client_id: &str,
    agent_instance_id: &str,
    result: &Value,
    reason_code: &str,
) -> ToolResult {
    let project_id = result
        .get("agent_project_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let revision = result
        .get("revision")
        .and_then(Value::as_str)
        .map(str::to_string);
    let state_changed = result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ToolResult::err_with_output(
        "project_projection_reconcile_required",
        json!({
            "error_code": "project_projection_reconcile_required",
            "failure_kind": "reconcile_required",
            "reason_code": reason_code,
            "state_changed": state_changed,
            "client_id": client_id,
            "agent_instance_id": agent_instance_id,
            "agent_project_id": project_id.clone(),
            "revision": revision.clone(),
            "authoritative_outcome": result.get("outcome").cloned().unwrap_or(Value::Null),
            "reconcile": {
                "action": "observe_exact_project_revision_before_retry",
                "project": project_id.map(|project_id| format!("agent:{client_id}:{project_id}")),
                "expected_revision": revision,
            }
        }),
    )
}

fn agent_protocol_reports_project_git(protocol: &str) -> bool {
    matches!(
        protocol,
        crate::shell_protocol::AGENT_PROTOCOL_VERSION_POLLING_V1
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_POLLING_V2
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_WEBSOCKET_V1
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_WEBSOCKET_V2
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_QUIC_V1
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_QUIC_V2
    )
}

fn project_query_matches(
    needle: &str,
    runtime_id: &str,
    project: &ShellAgentProjectSummary,
) -> bool {
    [
        Some(runtime_id),
        Some(project.id.as_str()),
        project.name.as_deref(),
        project.description.as_deref(),
        Some(project.path.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(needle))
}

fn project_source(project: &ShellAgentProjectSummary) -> &'static str {
    match project.kind.as_deref() {
        Some(MANAGED_TEMPORARY_PROJECT_SOURCE) => MANAGED_TEMPORARY_PROJECT_SOURCE,
        Some(AUTO_REGISTERED_PROJECT_SOURCE) => AUTO_REGISTERED_PROJECT_SOURCE,
        _ => "agent_registered",
    }
}

fn project_git_available(
    client: &crate::shell_protocol::ShellClientView,
    project: &crate::shell_protocol::ShellAgentProjectSummary,
) -> Option<bool> {
    if project.git_branch.is_some() || project.git_head.is_some() || project.git_dirty.is_some() {
        Some(true)
    } else if agent_protocol_reports_project_git(&client.agent_protocol_version) {
        Some(false)
    } else {
        None
    }
}

fn smoke_marker_present(project: &crate::shell_protocol::ShellAgentProjectSummary) -> bool {
    let name = project.name.as_deref().unwrap_or_default();
    [project.id.as_str(), name, project.path.as_str()]
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .any(|value| value.contains("smoke") || value.contains("test") || value.contains("sandbox"))
}

fn smoke_project_capabilities(
    client: &crate::shell_protocol::ShellClientView,
    project: &crate::shell_protocol::ShellAgentProjectSummary,
) -> Value {
    let git_available = project_git_available(client, project);
    let safe_smoke_project =
        project.allow_patch && client.connected && smoke_marker_present(project);
    let supports_artifact_smoke = client.capabilities.file_read && client.capabilities.file_write;
    let supports_cleanup_verification =
        supports_artifact_smoke || git_available.is_some_and(|available| available);
    let recommended_for_smoke = safe_smoke_project
        && git_available.is_some_and(|available| available)
        && supports_cleanup_verification;

    json!({
        "git_available": git_available,
        "safe_smoke_project": safe_smoke_project,
        "supports_artifact_smoke": supports_artifact_smoke,
        "supports_cleanup_verification": supports_cleanup_verification,
        "recommended_for_smoke": recommended_for_smoke,
    })
}

/// Resolve which shell profile a project uses and whether it is configured.
/// Returns `(resolved_name, status)` where:
/// - `resolved_name` = `project_shell_profile` (if set) else the agent's
///   `default_profile` (if any) else `None`.
/// - `status` = `"configured"` if the resolved name exists in the agent's
///   configured profiles; `"missing"` if a name resolved but is not
///   configured; `"not_configured"` if no profile resolves at all; and
///   `"unknown"` if the agent did not report a shell-profiles summary so the
///   configured set cannot be checked.
fn resolve_project_shell_profile(
    project_shell_profile: Option<&str>,
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> (Option<String>, &'static str) {
    let resolved = project_shell_profile
        .map(str::to_string)
        .or_else(|| summary.and_then(|s| s.default_profile.clone()));
    match resolved {
        None => (None, "not_configured"),
        Some(name) => match summary {
            None => (Some(name), "unknown"),
            Some(s) => {
                if s.profiles.iter().any(|p| p.name == name) {
                    (Some(name), "configured")
                } else {
                    (Some(name), "missing")
                }
            }
        },
    }
}

// =============================================================================
// Server-side request-shape validation helpers
// =============================================================================

/// Validate the project `id` field server-side. The agent does the
/// authoritative validation, but this rejects obviously malformed ids early.
/// Rules: non-empty, <= 64 chars, ASCII letters/digits/dash/underscore only,
/// no slash, no backslash, no dot-dot, no NUL.
fn validate_project_op_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id.contains('\0') {
        return Err("id must not contain NUL".to_string());
    }
    if id.len() > 64 {
        return Err("id must be at most 64 characters".to_string());
    }
    if id.contains('/') || id.contains('\\') {
        return Err("id must not contain slash or backslash".to_string());
    }
    if id == ".." || id == "." {
        return Err("id cannot be '.' or '..'".to_string());
    }
    if id.contains("..") {
        return Err("id must not contain dot-dot traversal".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(())
}

/// Validate the project `name` field server-side: non-empty after trim, <= 120
/// chars, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 characters".to_string());
    }
    Ok(())
}

/// Validate the optional `description` field: <= 500 chars, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 characters".to_string());
    }
    Ok(())
}

/// Validate the project `path` field server-side: non-empty, absolute, no NUL.
/// The Server may route to an agent on a different OS, so this check must accept
/// both POSIX and Windows absolute-path shapes without applying host-local path
/// semantics. The agent remains authoritative for existence, policy (including
/// current UNC support), and canonicalization.
pub(super) fn validate_project_op_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path must not contain NUL".to_string());
    }
    let bytes = path.as_bytes();
    let posix_absolute = path.starts_with('/');
    let windows_drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let windows_unc_or_verbatim_absolute = path.starts_with("\\\\");
    if !(posix_absolute || windows_drive_absolute || windows_unc_or_verbatim_absolute) {
        return Err("path must be an absolute path".to_string());
    }
    Ok(())
}

/// Truncate a string for inclusion in an error message (bounded).
fn truncate_for_error(s: &str) -> String {
    const MAX: usize = 200;
    if s.len() <= MAX {
        s.to_string()
    } else {
        format!("{}…", &s[..MAX])
    }
}

/// Parse a `ShellAgentProjectSummary` from the agent's project-op JSON
/// response so the server can upsert it into the cached project list. The
/// response includes `agent_project_id`, `client_id`, `name`, `path`, and
/// `allow_patch` — enough to build a summary that `listProjects` can show
/// immediately.
fn parse_project_summary_from_result(
    result: &Value,
    _client_id: &str,
) -> Option<ShellAgentProjectSummary> {
    let agent_project_id = result.get("agent_project_id")?.as_str()?;
    let name = result
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path = result.get("path")?.as_str()?;
    let allow_patch = result
        .get("allow_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Some(ShellAgentProjectSummary {
        id: agent_project_id.to_string(),
        name: name.or_else(|| Some(agent_project_id.to_string())),
        path: path.to_string(),
        allow_patch,
        kind: result
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: result
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        hooks: Vec::new(),
        disabled: result
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        revision: result
            .get("revision")
            .and_then(Value::as_str)
            .map(str::to_string),
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: chrono::Utc::now().timestamp(),
        shell_profile: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_empty() {
        assert!(validate_project_op_id("").is_err());
    }

    #[test]
    fn validate_id_rejects_nul() {
        assert!(validate_project_op_id("a\0b").is_err());
    }

    #[test]
    fn validate_id_rejects_slash() {
        assert!(validate_project_op_id("a/b").is_err());
    }

    #[test]
    fn validate_id_rejects_backslash() {
        assert!(validate_project_op_id("a\\b").is_err());
    }

    #[test]
    fn validate_id_rejects_dot_dot() {
        assert!(validate_project_op_id("..").is_err());
        assert!(validate_project_op_id("a..b").is_err());
    }

    #[test]
    fn validate_id_rejects_long() {
        let id = "a".repeat(65);
        assert!(validate_project_op_id(&id).is_err());
    }

    #[test]
    fn validate_id_accepts_valid() {
        assert!(validate_project_op_id("my-project").is_ok());
        assert!(validate_project_op_id("hello_123").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty_after_trim() {
        assert!(validate_project_op_name("   ").is_err());
    }

    #[test]
    fn validate_name_rejects_nul() {
        assert!(validate_project_op_name("a\0b").is_err());
    }

    #[test]
    fn validate_path_rejects_relative() {
        assert!(validate_project_op_path("relative/path").is_err());
    }

    #[test]
    fn validate_path_rejects_empty() {
        assert!(validate_project_op_path("").is_err());
    }

    #[test]
    fn validate_path_rejects_nul() {
        assert!(validate_project_op_path("/root/\0").is_err());
    }

    #[test]
    fn validate_path_accepts_cross_platform_absolute_shapes() {
        assert!(validate_project_op_path("/root/git/my-project").is_ok());
        assert!(validate_project_op_path(r"C:\repo").is_ok());
        assert!(validate_project_op_path("c:/repo").is_ok());
        assert!(validate_project_op_path(r"\\?\C:\repo").is_ok());
        assert!(validate_project_op_path(r"\\server\share\repo").is_ok());
        assert!(validate_project_op_path(r"C:repo").is_err());
        assert!(validate_project_op_path(r"\repo").is_err());
        assert!(validate_project_op_path(r"relative\repo").is_err());
    }

    #[test]
    fn parse_summary_extracts_fields() {
        let result = json!({
            "agent_project_id": "my-project",
            "client_id": "oe",
            "name": "My Project",
            "path": "/root/git/my-project",
            "allow_patch": true,
            "description": "desc",
        });
        let summary = parse_project_summary_from_result(&result, "oe").unwrap();
        assert_eq!(summary.id, "my-project");
        assert_eq!(summary.name.as_deref(), Some("My Project"));
        assert_eq!(summary.path, "/root/git/my-project");
        assert!(summary.allow_patch);
        assert!(!summary.disabled);
    }

    #[test]
    fn parse_summary_defaults_name_to_id() {
        let result = json!({
            "agent_project_id": "hello",
            "client_id": "oe",
            "path": "/root/git/hello",
        });
        let summary = parse_project_summary_from_result(&result, "oe").unwrap();
        assert_eq!(summary.name.as_deref(), Some("hello"));
    }

    #[test]
    fn validate_id_rejects_single_dot() {
        assert!(validate_project_op_id(".").is_err());
    }

    #[test]
    fn validate_id_rejects_non_alphanumeric() {
        assert!(validate_project_op_id("a!b").is_err());
        assert!(validate_project_op_id("a b").is_err());
        assert!(validate_project_op_id("a.b").is_err());
    }

    #[test]
    fn validate_description_rejects_nul() {
        assert!(validate_project_op_description("a\0b").is_err());
    }

    #[test]
    fn validate_description_rejects_long() {
        let desc = "a".repeat(501);
        assert!(validate_project_op_description(&desc).is_err());
    }

    #[test]
    fn validate_description_accepts_none() {
        // None/empty description is valid.
        assert!(validate_project_op_description("").is_ok());
    }

    #[test]
    fn validate_name_rejects_long() {
        let name = "a".repeat(121);
        assert!(validate_project_op_name(&name).is_err());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(validate_project_op_name("My Project").is_ok());
        assert!(validate_project_op_name("A").is_ok());
    }
}
