//! Runner-side project management tools: `register_project`, `unregister_project`,
//! and `create_project`.
//!
//! Registration and creation route to the selected Runner through the project-op
//! path. Unregistration reuses the shared project lifecycle core so canonical
//! runtime execution keeps the same revision CAS, active-Job fence, capability
//! check, uncertain-delivery semantics, and
//! server inventory update. The Runner remains authoritative for its local
//! project registration records in the Runner project registry.
//!
//! The server never writes project config files or creates directories on the
//! Runner host directly. OS permissions and Runner policy
//! (`allow_cwd_anywhere` / `allowed_roots`) remain the real boundary; there is
//! no workspace abstraction.

use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use super::tool_result::{RecoveryKind, ToolResult};
mod maintenance;

use super::{runner_project_runtime_id, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_PROJECT_READ};
use crate::runner_http::{RunnerFeature, RunnerSemanticView};
use crate::runner_protocol::{RunnerProjectSummary, RUNNER_CAPABILITY_PROJECT_PATH_REGISTRATION};
use maintenance::registered_git_summary;

/// Maximum time the runtime waits for a Runner project-op response. Project
/// operations are fast (write a small TOML, maybe create a directory + git
/// init), so 30s is generous while still bounding the caller.
const PROJECT_OP_WAIT_SECS: u64 = 32;
const AUTO_REGISTERED_PROJECT_SOURCE: &str = "auto_registered";

const LIST_PROJECTS_MAX_QUERY_CHARS: usize = 200;
const LIST_PROJECTS_DEFAULT_RESULTS: usize = 30;
const LIST_PROJECTS_MAX_RESULTS: usize = 100;
const RUNTIME_CONSOLE_LIST_PROJECTS_MAX_RESULTS: usize = 2_000;

#[derive(Debug, Default)]
pub(crate) struct ListProjectsOptions {
    pub(crate) client_id: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: usize,
    pub(crate) summary_only: bool,
    pub(crate) include_git_summary: bool,
}

/// Authorized, filtered and bounded facts for model-facing discovery.
/// Console selectors use a separate minimal projection of the same authority.
pub(crate) struct ProjectInventoryPage {
    pub(crate) projects: Vec<ProjectInventoryRow>,
    pub(crate) matched_count: usize,
    offset: usize,
    limit: usize,
}

pub(crate) struct ProjectInventoryRow {
    pub(crate) id: String,
    pub(crate) project_ref: Option<String>,
    pub(crate) client_id: String,
    pub(crate) runner_status: String,
    pub(crate) connected: bool,
    pub(crate) project: RunnerProjectSummary,
    last_seen: i64,
    active_jobs: usize,
    resolved_shell_profile: Option<String>,
    shell_profile_status: &'static str,
    capabilities: ProjectCapabilities,
}

/// Minimal authorized selector facts for the browser. This path deliberately
/// omits model capabilities, active Jobs and shell-policy projections.
pub(crate) struct ConsoleProjectInventoryPage {
    pub(crate) projects: Vec<ConsoleProjectInventoryRow>,
    pub(crate) matched_count: usize,
}

pub(crate) struct ConsoleProjectInventoryRow {
    pub(crate) id: String,
    pub(crate) project_ref: Option<String>,
    pub(crate) client_id: String,
    pub(crate) runner_status: String,
    pub(crate) connected: bool,
    pub(crate) name: Option<String>,
    pub(crate) path: String,
}

#[derive(serde::Serialize)]
struct ProjectCapabilities {
    git_available: Option<bool>,
    safe_smoke_project: bool,
    supports_artifact_smoke: bool,
    supports_cleanup_verification: bool,
    recommended_for_smoke: bool,
}

impl ProjectInventoryPage {
    fn into_tool_result(self, options: &ListProjectsOptions) -> ToolResult {
        let projects: Vec<_> = self
            .projects
            .iter()
            .map(|row| {
                let project = &row.project;
                let mut value = json!({
                    "id": row.id,
                    "name": project.name,
                    "path": project.path,
                    "client_id": row.client_id,
                    "enabled": !project.disabled,
                    "active_jobs": row.active_jobs,
                    "source": project_source(project),
                    "connected": row.connected,
                });
                if let Some(description) = project
                    .description
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    value["description"] = json!(description);
                }
                if !row.connected || row.runner_status != "online" {
                    value["agent_status"] = json!(row.runner_status);
                }
                if !options.summary_only {
                    value["agent_project_id"] = json!(project.id);
                    value["description"] = json!(project.description);
                    value["executor"] = json!("agent");
                    value["agent_status"] = json!(row.runner_status);
                    value["resolved_shell_profile"] = json!(row.resolved_shell_profile);
                    value["shell_profile_status"] = json!(row.shell_profile_status);
                    value["allow_patch"] = json!(project.allow_patch);
                    value["disabled"] = json!(project.disabled);
                    value["revision"] = json!(project.revision);
                    value["last_seen"] = json!(row.last_seen);
                    value["shell_profile"] = json!(project.shell_profile);
                    value["capabilities"] = json!(row.capabilities);
                }
                if options.include_git_summary {
                    value["git"] = registered_git_summary(project);
                }
                if let Some(project_ref) = &row.project_ref {
                    value["project_ref"] = json!(project_ref);
                }
                value
            })
            .collect();
        let end = self.offset.saturating_add(projects.len());
        let next_offset = (end < self.matched_count).then_some(end);
        let mut output = json!({
            "count": projects.len(),
            "matched_count": self.matched_count,
            "offset": self.offset,
            "limit": self.limit,
            "next_offset": next_offset,
            "truncated": next_offset.is_some(),
            "projects": projects,
        });
        if !options.summary_only {
            output["recommended_for_smoke"] = json!(self
                .projects
                .iter()
                .filter(|row| row.capabilities.recommended_for_smoke)
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>());
        }
        if self.matched_count == 0 {
            output["hint"] = json!("No accessible Projects match this request. Check the filters and the current credential identity and authorization.");
        }
        ToolResult::ok(output)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProjectCandidate {
    pub(super) runtime_id: String,
    pub(super) client_index: usize,
    pub(super) project_index: usize,
}

fn validate_list_projects_options(
    options: &ListProjectsOptions,
    max_results: usize,
) -> Result<(Option<String>, usize), ToolResult> {
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
    let limit = options
        .limit
        .unwrap_or(LIST_PROJECTS_DEFAULT_RESULTS)
        .clamp(1, max_results);
    Ok((query, limit))
}

fn project_candidates(
    clients: &[RunnerSemanticView],
    options: &ListProjectsOptions,
    query: Option<&str>,
) -> Vec<ProjectCandidate> {
    let mut candidates = Vec::new();
    for (client_index, client) in clients.iter().enumerate() {
        let view = &client.view;
        if options
            .client_id
            .as_deref()
            .is_some_and(|expected| view.client_id != expected)
        {
            continue;
        }
        for (project_index, project) in view.projects.iter().enumerate() {
            let runtime_id = runner_project_runtime_id(&view.client_id, &project.id);
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

fn project_registry_scope_denied(operation: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "{operation} is unavailable to project-scoped credentials; use an already-visible Project, or derive an isolated Project with work_on_project(mode=worktree)"
        ),
        json!({
            "error_kind": "project_registry_scope_denied",
            "failure_kind": "authorization_denied",
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::UserAction)
}

fn project_registry_mutation_denied(
    auth: Option<&AuthContext>,
    operation: &str,
) -> Option<ToolResult> {
    auth.is_some_and(AuthContext::is_project_scoped_model_subject)
        .then(|| project_registry_scope_denied(operation))
}

/// Whether an explicitly authorized parent-root grant admits `path`.
///
/// The grant lives on the project-share OAuth subject
/// (`AuthContext::project_root_scopes`), never on Runner filesystem policy, so
/// Runner cwd authority still cannot become Project-registry authority. The
/// request path must already exist and is canonicalized before the
/// component-wise containment check, which rejects `..` climbs, textual-prefix
/// siblings such as `<root>-extra`, and symlinks that resolve outside every
/// granted root. A missing directory stays denied: admission never creates
/// directories, and the Runner-side `allowed_roots` policy remains the real
/// execution boundary.
fn authorized_project_root_scope_admits(auth: Option<&AuthContext>, path: &str) -> bool {
    let Some(auth) = auth else {
        return false;
    };
    if auth.project_root_scopes.is_empty() {
        return false;
    }
    let requested = Path::new(path);
    if !requested.is_absolute() {
        return false;
    }
    let Ok(canonical) = requested.canonicalize() else {
        return false;
    };
    auth.project_root_scopes
        .iter()
        .any(|root| canonical.starts_with(root))
}

/// Canonical target plus forwarded grant roots for one fence-checked path.
/// Root-scope credentials send the canonical target the fence authorized and
/// the grant's canonical roots, so the Runner re-derives the admission
/// decision against its own resolution before its first side effect; a
/// symlink or component flipped between the fence and the Runner's own
/// resolution then fails closed at the Runner instead of committing.
/// Credentials without parent-root grants get the raw path and no roots, the
/// exact previous behavior. An unresolvable or no-longer-admitted target
/// fails closed here: nothing is submitted.
fn grant_bound_project_path(
    path: &str,
    auth: Option<&AuthContext>,
) -> Result<(String, Option<Vec<String>>), ToolResult> {
    let Some(auth) = auth.filter(|auth| !auth.project_root_scopes.is_empty()) else {
        return Ok((path.to_string(), None));
    };
    let canonical = match std::path::Path::new(path).canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => {
            return Err(project_registry_scope_denied("project path resolution"));
        }
    };
    let Some(canonical) = canonical.to_str() else {
        return Err(project_registry_scope_denied("project path resolution"));
    };
    if !authorized_project_root_scope_admits(Some(auth), canonical) {
        return Err(project_registry_scope_denied("project path resolution"));
    }
    Ok((
        canonical.to_string(),
        Some(
            auth.project_root_scopes
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
        ),
    ))
}

/// Build the resolve-or-register project-op payload from the bound path.
fn resolve_or_register_op_payload(
    path: &str,
    auth: Option<&AuthContext>,
) -> Result<serde_json::Value, ToolResult> {
    let (path, granted_roots) = grant_bound_project_path(path, auth)?;
    Ok(match granted_roots {
        None => json!({"path": path}),
        Some(granted_roots) => json!({
            "path": path,
            "project_root_scopes": granted_roots,
        }),
    })
}

/// Assemble one managed-worktree project-op payload. The Runner validates the
/// exact field count (4 | 6 ordinary, 5 | 7 with a forwarded share grant), so
/// this shape is a contract: source identity and grant roots are only ever
/// added together with their validated values, and the optional grant rides
/// along unchanged through the indeterminate-retry re-submission.
fn managed_worktree_op_payload(
    source_path: String,
    base_ref: Option<String>,
    operation_id: String,
    resume_project_id: Option<String>,
    expected_source_identity: Option<(String, String)>,
    granted_roots: Option<Vec<String>>,
) -> serde_json::Value {
    let mut payload = json!({
        "path": source_path,
        "base_ref": base_ref,
        "operation_id": operation_id,
        "resume_project_id": resume_project_id,
    });
    if let Some((project_id, root_fingerprint)) = expected_source_identity {
        payload["expected_source_project_id"] = json!(project_id);
        payload["expected_source_root_fingerprint"] = json!(root_fingerprint);
    }
    if let Some(granted_roots) = granted_roots {
        payload["project_root_scopes"] = json!(granted_roots);
    }
    payload
}

/// Re-verify the Runner's committed canonical path against the grant. The
/// Runner-side grant check is the commit-time authority; this catches view
/// drift between the Runner's resolution and the Server's grant after the
/// fact and reports the honest effect state. When the response does not carry
/// usable effect metadata the existing indeterminate contract applies:
/// the effect may have happened, so it is never reported as no-change.
fn verify_runner_path_within_granted_roots(
    result: ToolResult,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let Some(auth) = auth.filter(|auth| !auth.project_root_scopes.is_empty()) else {
        return result;
    };
    if !result.success {
        return result;
    }
    let registered = result
        .output
        .get("registered")
        .and_then(serde_json::Value::as_bool);
    let Some(returned) = result
        .output
        .get("path")
        .and_then(serde_json::Value::as_str)
    else {
        let message = "Runner returned a project resolution without a path; the granted project roots could not be re-verified";
        return match registered {
            Some(registered) => ToolResult::err_with_output(
                message,
                json!({
                    "error_kind": "project_registry_scope_denied",
                    "failure_kind": "authorization_denied",
                    "state_changed": registered,
                }),
            )
            .with_recovery(RecoveryKind::UserAction),
            None => indeterminate_grant_verification_result(message),
        };
    };
    if authorized_project_root_scope_admits(Some(auth), returned) {
        return result;
    }
    let message = "Runner returned a project path outside the granted project roots";
    match registered {
        Some(registered) => ToolResult::err_with_output(
            message,
            json!({
                "error_kind": "project_registry_scope_denied",
                "failure_kind": "authorization_denied",
                "state_changed": registered,
            }),
        )
        .with_recovery(RecoveryKind::UserAction),
        None => indeterminate_grant_verification_result(message),
    }
}

/// The Runner reported success but not whether its write landed: the shared
/// project-op indeterminate contract, not a claim of no effect.
fn indeterminate_grant_verification_result(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "error_kind": "operation_indeterminate",
            "failure_kind": "operation_indeterminate",
            "state_changed": true,
        }),
    )
    .with_recovery(RecoveryKind::Reobserve)
}

fn existing_project_path_result(client_id: &str, project: &RunnerProjectSummary) -> ToolResult {
    ToolResult::ok(json!({
        "id": runner_project_runtime_id(client_id, &project.id),
        "agent_project_id": project.id,
        "client_id": client_id,
        "name": project.name,
        "path": project.path,
        "kind": project.kind,
        "description": project.description,
        "allow_patch": project.allow_patch,
        "disabled": project.disabled,
        "revision": project.revision,
        "source": "path",
        "outcome": "reused_existing_registration",
        "registered": false,
        "created_config": false,
        "changed": false,
        "recovered": true,
    }))
}

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn list_projects(&self, auth: Option<&AuthContext>) -> ToolResult {
        self.list_projects_with_options(auth, ListProjectsOptions::default())
            .await
    }

    /// For project-scoped model/API credentials, path-based coding may only use
    /// an exact path already present in that ProjectGrant's caller-visible
    /// Runner inventory, or — when the share was explicitly started with
    /// `--project-roots` — an existing directory inside one of those granted
    /// canonical parent roots, which the caller then admits on demand through
    /// the ordinary idempotent project-op path. This prevents broad Runner
    /// filesystem authority (needed for canonical managed-worktree siblings)
    /// from becoming Project-registry authority without an explicit share-level
    /// grant. Other credential classes keep the ordinary path-registration
    /// behavior.
    pub(crate) async fn project_scoped_visible_project_for_exact_path(
        &self,
        client_id: &str,
        path: &str,
        auth: Option<&AuthContext>,
    ) -> Result<Option<RunnerProjectSummary>, ToolResult> {
        if !auth.is_some_and(AuthContext::is_project_scoped_model_subject) {
            return Ok(None);
        }
        let access = crate::runner_http::runner_access_from_auth(auth);
        let Some(client) = self
            .runner_registry
            .get_runner_semantic_view_for_auth(client_id, access.as_ref())
            .await
        else {
            return Err(project_registry_scope_denied("project path resolution"));
        };
        if let Some(project) = client
            .view
            .projects
            .iter()
            .find(|project| !project.disabled && project.path == path)
            .cloned()
        {
            return Ok(Some(project));
        }
        if authorized_project_root_scope_admits(auth, path) {
            // No exact registration yet: let the caller's project-op resolve or
            // register it under the Runner registry write lock. Repeated calls
            // keep the same stable project identity.
            return Ok(None);
        }
        Err(project_registry_scope_denied("project path resolution"))
    }

    pub(crate) async fn list_projects_with_options(
        &self,
        auth: Option<&AuthContext>,
        options: ListProjectsOptions,
    ) -> ToolResult {
        self.list_projects_with_options_cap(auth, options, LIST_PROJECTS_MAX_RESULTS)
            .await
    }

    pub(crate) async fn query_projects_for_runtime_console(
        &self,
        auth: Option<&AuthContext>,
        client_id: Option<String>,
        project: Option<String>,
        query: Option<String>,
        limit: usize,
    ) -> Result<ConsoleProjectInventoryPage, ToolResult> {
        let options = ListProjectsOptions {
            client_id,
            project,
            query,
            limit: Some(limit),
            ..Default::default()
        };
        let (query, limit) =
            validate_list_projects_options(&options, RUNTIME_CONSOLE_LIST_PROJECTS_MAX_RESULTS)?;
        let access = crate::runner_http::runner_access_from_auth(auth);
        let selection = self
            .runner_registry
            .select_projects_for_auth(
                access.as_ref(),
                options.client_id.as_deref(),
                options.project.as_deref(),
                limit,
                |id, project| {
                    query
                        .as_deref()
                        .is_none_or(|needle| project_query_matches(needle, id, project))
                },
            )
            .await;
        let identities = selection
            .projects
            .iter()
            .map(|row| (row.id.clone(), row.root_fingerprint.clone()))
            .collect();
        let references = self.project_references_for_console(identities, auth).await;
        let projects = selection
            .projects
            .into_iter()
            .zip(references)
            .map(|(row, project_ref)| ConsoleProjectInventoryRow {
                id: row.id,
                project_ref,
                client_id: row.client_id,
                runner_status: row.runner_status,
                connected: row.connected,
                name: row.name,
                path: row.path,
            })
            .collect();
        Ok(ConsoleProjectInventoryPage {
            projects,
            matched_count: selection.matched_count,
        })
    }

    pub(super) async fn list_projects_with_options_cap(
        &self,
        auth: Option<&AuthContext>,
        options: ListProjectsOptions,
        max_results: usize,
    ) -> ToolResult {
        match self
            .query_project_inventory(auth, &options, max_results)
            .await
        {
            Ok(page) => page.into_tool_result(&options),
            Err(result) => result,
        }
    }

    async fn query_project_inventory(
        &self,
        auth: Option<&AuthContext>,
        options: &ListProjectsOptions,
        max_results: usize,
    ) -> Result<ProjectInventoryPage, ToolResult> {
        let (query, limit) = validate_list_projects_options(options, max_results)?;
        let access = crate::runner_http::runner_access_from_auth(auth);
        let clients = self
            .runner_registry
            .list_project_semantic_views_for_auth(
                access.as_ref(),
                options.client_id.as_deref(),
                options.project.as_deref(),
            )
            .await;
        Ok(self
            .project_inventory_from_semantic_clients(
                auth,
                options,
                query.as_deref(),
                limit,
                &clients,
            )
            .await)
    }

    /// Reuse the canonical Runner/project visibility projection for an exact
    /// Project id without dispatching a model-visible tool. This is an
    /// observability fence only: it grants no Project authority and callers
    /// must still hold `project:read` explicitly.
    pub(crate) async fn exact_project_visible_to_auth(
        &self,
        auth: &AuthContext,
        project: &str,
    ) -> bool {
        if !auth.has_scope(SCOPE_PROJECT_READ) {
            return false;
        }
        let access = crate::runner_http::runner_access_from_auth(Some(auth));
        self.runner_registry
            .exact_project_visible_for_auth_snapshot(access.as_ref(), project)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn list_projects_with_visible_clients_for_test(
        &self,
        auth: Option<&AuthContext>,
        options: ListProjectsOptions,
        clients: &[crate::runner_protocol::RunnerView],
    ) -> ToolResult {
        let (query, limit) =
            match validate_list_projects_options(&options, LIST_PROJECTS_MAX_RESULTS) {
                Ok(validated) => validated,
                Err(result) => return result,
            };
        let semantic_clients = clients
            .iter()
            .cloned()
            .map(RunnerSemanticView::from_public_view_for_test)
            .collect::<Vec<_>>();
        self.project_inventory_from_semantic_clients(
            auth,
            &options,
            query.as_deref(),
            limit,
            &semantic_clients,
        )
        .await
        .into_tool_result(&options)
    }

    async fn project_inventory_from_semantic_clients(
        &self,
        auth: Option<&AuthContext>,
        options: &ListProjectsOptions,
        query: Option<&str>,
        limit: usize,
        clients: &[RunnerSemanticView],
    ) -> ProjectInventoryPage {
        let access = crate::runner_http::runner_access_from_auth(auth);
        let candidates = project_candidates(clients, options, query);
        let matched_count = candidates.len();
        // Offsets address this request's authorized, filtered, ID-sorted snapshot.
        // Registration or authorization changes between calls may shift rows;
        // callers needing a fresh traversal restart at zero and deduplicate IDs.
        let candidates = candidates
            .into_iter()
            .skip(options.offset)
            .take(limit)
            .collect::<Vec<_>>();
        let project_ids: Vec<&str> = candidates
            .iter()
            .map(|candidate| candidate.runtime_id.as_str())
            .collect();
        let active_jobs_by_project = self
            .runner_registry
            .count_active_jobs_for_projects(access.as_ref(), &project_ids)
            .await;

        let projects = candidates
            .into_iter()
            .map(|candidate| {
                // Clone only the selected Project after filtering and limiting.
                let client = &clients[candidate.client_index];
                let view = &client.view;
                let project = &view.projects[candidate.project_index];
                let (resolved_shell_profile, shell_profile_status) = resolve_project_shell_profile(
                    project.shell_profile.as_deref(),
                    view.policy
                        .as_ref()
                        .and_then(|policy| policy.shell_profiles.as_ref()),
                );
                ProjectInventoryRow {
                    project_ref: self.project_reference_for_identity(
                        &candidate.runtime_id,
                        project.root_fingerprint.as_deref(),
                        auth,
                    ),
                    active_jobs: active_jobs_by_project
                        .get(&candidate.runtime_id)
                        .copied()
                        .unwrap_or(0),
                    id: candidate.runtime_id,
                    client_id: view.client_id.clone(),
                    runner_status: view.status.clone(),
                    connected: view.connected,
                    last_seen: view.last_seen,
                    project: project.clone(),
                    resolved_shell_profile,
                    shell_profile_status,
                    capabilities: smoke_project_capabilities(client, project),
                }
            })
            .collect();
        ProjectInventoryPage {
            projects,
            matched_count,
            offset: options.offset,
            limit,
        }
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
    /// semantics in one canonical runtime path.
    pub(crate) async fn unregister_project(
        &self,
        project: String,
        expected_revision: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Some(result) = project_registry_mutation_denied(auth, "unregister_project") {
            return result;
        }
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

    /// Create a new directory on the selected Runner, or explicitly adopt an
    /// already-existing empty directory, and register it as a WebCodex project.
    /// See the `ToolCall::CreateProject` doc comment for the full contract.
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
        adopt_existing_empty: bool,
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
            adopt_existing_empty,
            overwrite,
            auth,
        )
        .await
    }

    /// Ask the selected Runner to resolve an existing registration by
    /// canonical path or persist a new project registration record under its registry
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
        match self
            .project_scoped_visible_project_for_exact_path(&client_id, &path, auth)
            .await
        {
            Ok(Some(project)) => return existing_project_path_result(&client_id, &project),
            Ok(None) => {}
            Err(result) => return result,
        }
        let access = crate::runner_http::runner_access_from_auth(auth);
        if let Some(client) = self
            .runner_registry
            .get_runner_semantic_view_for_auth(&client_id, access.as_ref())
            .await
        {
            if let Err(error) = self
                .runner_registry
                .assert_runner_access(access.as_ref(), &client_id)
                .await
            {
                return ToolResult::err(error);
            }
            if !client.supports(RunnerFeature::ProjectPathRegistration) {
                return ToolResult::err_with_output(
                    "agent_capability_unavailable: accepted Runner violated the generation-2 project_path_registration baseline",
                    json!({
                        "error_kind": "agent_capability_unavailable",
                        "failure_kind": "capability_unavailable",
                        "reason_code": "runner_generation_baseline_invariant",
                        "capability": RUNNER_CAPABILITY_PROJECT_PATH_REGISTRATION,
                        "state_changed": false,
                    }),
                )
                .with_recovery(RecoveryKind::NoAction);
            }
        }
        let payload = match resolve_or_register_op_payload(&path, auth) {
            Ok(payload) => payload,
            Err(denied) => return denied,
        };
        let result = self
            .submit_project_op("resolve_or_register_project", client_id, payload, auth)
            .await;
        verify_runner_path_within_granted_roots(result, auth)
    }

    /// Ask the exact Runner to prepare or re-observe one managed detached
    /// worktree and register it as an ordinary runtime Project. `operation_id`
    /// is transport/idempotency identity only and grants no Project authority.
    pub(crate) async fn prepare_managed_worktree(
        &self,
        client_id: String,
        path: String,
        base_ref: Option<String>,
        operation_id: String,
        resume_project_id: Option<String>,
        expected_source_project_id: Option<String>,
        expected_source_root_fingerprint: Option<String>,
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
        let exact_path_project = match self
            .project_scoped_visible_project_for_exact_path(&client_id, &path, auth)
            .await
        {
            Ok(exact) => exact,
            Err(result) => return result,
        };
        // The exact-path fence hit is the authorization fact itself; absence of
        // grant roots is NOT (ordinary credentials also carry none).
        let exact_path_authorized = exact_path_project.is_some();
        // Two independent, separately valid authorization sources bind the source
        // the Runner will actually derive from: an exact-path registered Project
        // (admitted above by registration, not by roots) or the credential's
        // parent-root share grant. Exact-path sources keep their registered
        // project identity and root fingerprint as the replay/retarget anchor and
        // carry no grant roots; parent-root sources carry the grant. The Runner
        // re-checks either against its own canonical resolution before any side
        // effect. Only the source is grant-scoped — the derived sibling worktree
        // destination is a Runner-owned namespace whose boundary remains
        // allowed_roots, and the Runner result reports the destination path, so
        // there is nothing to re-verify here.
        let (source_path, granted_roots, fence_identity) = match exact_path_project {
            Some(project) => (
                path.clone(),
                None,
                project
                    .root_fingerprint
                    .map(|root_fingerprint| (project.id.clone(), root_fingerprint)),
            ),
            None => {
                let (source_path, granted_roots) = match grant_bound_project_path(&path, auth) {
                    Ok(bound) => bound,
                    Err(denied) => return denied,
                };
                (source_path, granted_roots, None)
            }
        };
        let access = crate::runner_http::runner_access_from_auth(auth);
        if let Some(client) = self
            .runner_registry
            .get_runner_semantic_view_for_auth(&client_id, access.as_ref())
            .await
        {
            if let Err(error) = self
                .runner_registry
                .assert_runner_access(access.as_ref(), &client_id)
                .await
            {
                return ToolResult::err(error);
            }
            if !client.supports(RunnerFeature::ManagedWorktree) {
                return ToolResult::err_with_output(
                    "agent_capability_unavailable: Runner does not support managed worktree bootstrap",
                    json!({
                        "error_kind": "agent_capability_unavailable",
                        "failure_kind": "capability_unavailable",
                        "capability": crate::runner_protocol::RUNNER_CAPABILITY_MANAGED_WORKTREE,
                        "state_changed": false,
                    }),
                )
                .with_recovery(RecoveryKind::NoAction);
            }
        }
        let fresh_managed_bootstrap = resume_project_id.is_none();
        // Caller-supplied identity anchors replay/retarget and wins; the
        // exact-path fence fills it in when the model did not supply one.
        let expected_source_identity =
            match (expected_source_project_id, expected_source_root_fingerprint) {
                (Some(project_id), Some(root_fingerprint)) => Some((project_id, root_fingerprint)),
                (None, None) => fence_identity,
                _ => {
                    return ToolResult::err_with_output(
                        "managed worktree source identity must be complete",
                        json!({
                            "error_kind": "managed_worktree_source_identity_unavailable",
                            "failure_kind": "operation_failed",
                            "state_changed": false,
                        }),
                    )
                    .with_recovery(RecoveryKind::Reobserve);
                }
            };
        // Only sources actually admitted through the exact-path fence must have
        // a complete, usable expected source identity to anchor the operation.
        // When the Runner's inventory temporarily lacks a root_fingerprint and
        // the caller did not supply one, the request must fail closed on the
        // existing Reobserve contract instead of going out unanchored.
        // Parent-root and ordinary credentials keep the pre-existing behavior:
        // their sources are bounded by the forwarded grant or by ordinary
        // admission, not by a fence identity. Resume requests (resume_project_id
        // set) are exempt from THIS server-side completeness check only — the
        // Runner still anchors them to its stored source path and lineage.
        if exact_path_authorized
            && expected_source_identity.is_none()
            && resume_project_id.is_none()
        {
            return ToolResult::err_with_output(
                "managed worktree source identity must be complete",
                json!({
                    "error_kind": "managed_worktree_source_identity_unavailable",
                    "failure_kind": "operation_failed",
                    "state_changed": false,
                }),
            )
            .with_recovery(RecoveryKind::Reobserve);
        }
        let payload = managed_worktree_op_payload(
            source_path,
            base_ref,
            operation_id,
            resume_project_id,
            expected_source_identity,
            granted_roots,
        );
        let first = self
            .submit_project_op(
                "prepare_managed_worktree",
                client_id.clone(),
                payload.clone(),
                auth,
            )
            .await;
        let indeterminate = !first.success
            && ["error_code", "error_kind"].into_iter().any(|field| {
                first.output.get(field).and_then(Value::as_str) == Some("operation_indeterminate")
            });
        if !indeterminate {
            return first;
        }
        // A lost response may have followed either worktree creation or registry
        // publication. Re-issue the *same* operation identity exactly once: the
        // Runner only re-observes/converges that deterministic target and never
        // creates a second worktree for this tool invocation.
        let mut recovered = self
            .submit_project_op("prepare_managed_worktree", client_id, payload, auth)
            .await;
        if recovered.success
            && fresh_managed_bootstrap
            && recovered.output.get("outcome").and_then(Value::as_str)
                == Some("managed_worktree_recovered")
            && recovered.output.get("registered").and_then(Value::as_bool) == Some(false)
        {
            // This operation_id is freshly generated and never model-supplied.
            // Seeing its already-registered Project after the first response was
            // indeterminate proves the mutation happened earlier in this same
            // work_on_project call, even though the re-observation attempt itself
            // did not write the registry. Normalize to call-level mutation truth.
            recovered.output["registered"] = json!(true);
            recovered.output["created_config"] = json!(true);
            recovered.output["changed"] = json!(true);
        }
        recovered
    }

    /// Shared implementation for both `register_project` and `create_project`.
    /// `kind` is `"register_project"` or `"create_project"`. Fields not
    /// applicable to `register_project` (template, git_init,
    /// adopt_existing_empty) are ignored by the Runner for that kind.
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
        adopt_existing_empty: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Some(result) = project_registry_mutation_denied(auth, kind) {
            return result;
        }
        // -- basic server-side request shape validation ----------------------
        // The Runner does the authoritative path/policy validation, but the
        // server rejects obviously malformed requests early so the Runner is
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
            "adopt_existing_empty": adopt_existing_empty,
            "overwrite": overwrite,
        });
        self.submit_project_op(kind, client_id, payload, auth).await
    }

    /// Shared transport, response parsing, cache-upsert, and owner-boundary
    /// path for public project operations.
    async fn submit_project_op(
        &self,
        kind: &str,
        client_id: String,
        payload: Value,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let access = crate::runner_http::runner_access_from_auth(auth);
        // -- owner boundary + client existence --------------------------------
        let Some(client_view) = self
            .runner_registry
            .get_runner_view_for_auth(&client_id, access.as_ref())
            .await
        else {
            return ToolResult::err(format!(
                "unknown Runner '{}'. Call list_runners to discover registered client_ids.",
                client_id
            ));
        };
        let expected_runner_instance_id = client_view.runner_instance_id.clone();
        if let Err(e) = self
            .runner_registry
            .assert_runner_access(access.as_ref(), &client_id)
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
            .runner_registry
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
                    self.runner_registry.cancel_request(&request_id).await;
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
            return ToolResult::err("Runner returned empty project op result");
        }
        let mut result: Value = match serde_json::from_str::<Value>(stdout) {
            Ok(value) => value,
            Err(error) => {
                return ToolResult::err(format!(
                    "failed to parse Runner project op response: {} (stdout: {})",
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
                &expected_runner_instance_id,
                &result,
                "authoritative_project_summary_missing",
            );
        };
        let resolved_project = runner_project_runtime_id(&client_id, &project.id);
        let root_fingerprint = project.root_fingerprint.clone();
        if let Err(error) = self
            .runner_registry
            .upsert_runner_project_for_instance(&client_id, &expected_runner_instance_id, project)
            .await
        {
            return project_projection_reconcile_required(
                &client_id,
                &expected_runner_instance_id,
                &result,
                if error.contains("stale or replaced") {
                    "runner_instance_changed_before_projection"
                } else {
                    "server_project_projection_failed"
                },
            );
        }

        if let Some(project_ref) = self.project_reference_for_identity(
            &resolved_project,
            root_fingerprint.as_deref(),
            auth,
        ) {
            result["project_ref"] = json!(project_ref);
        }
        ToolResult::ok(result)
    }
}

fn project_projection_reconcile_required(
    client_id: &str,
    runner_instance_id: &str,
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
            "runner_instance_id": runner_instance_id,
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

fn project_query_matches(needle: &str, runtime_id: &str, project: &RunnerProjectSummary) -> bool {
    [
        Some(runtime_id),
        Some(project.id.as_str()),
        project.name.as_deref(),
        project.description.as_deref(),
        Some(project.path.as_str()),
        project.registration_source.as_deref(),
        project.kind.as_deref(),
        Some(project_source(project)),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(needle))
}

fn project_source(project: &RunnerProjectSummary) -> &'static str {
    match project.registration_source.as_deref() {
        Some(AUTO_REGISTERED_PROJECT_SOURCE) => AUTO_REGISTERED_PROJECT_SOURCE,
        // A present additive provenance field is authoritative. Known explicit
        // and unknown future wire values both fail closed to ordinary registered
        // provenance so a future string cannot break the project inventory.
        Some(_) => "agent_registered",
        None if project.kind.as_deref() == Some(AUTO_REGISTERED_PROJECT_SOURCE) => {
            AUTO_REGISTERED_PROJECT_SOURCE
        }
        None => "agent_registered",
    }
}

pub(super) fn project_git_available(
    project: &crate::runner_protocol::RunnerProjectSummary,
) -> Option<bool> {
    if project.git_branch.is_some() || project.git_head.is_some() || project.git_dirty.is_some() {
        Some(true)
    } else {
        Some(false)
    }
}

fn smoke_marker_present(project: &crate::runner_protocol::RunnerProjectSummary) -> bool {
    let name = project.name.as_deref().unwrap_or_default();
    [project.id.as_str(), name, project.path.as_str()]
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .any(|value| value.contains("smoke") || value.contains("test") || value.contains("sandbox"))
}

fn smoke_project_capabilities(
    client: &RunnerSemanticView,
    project: &crate::runner_protocol::RunnerProjectSummary,
) -> ProjectCapabilities {
    let git_available = project_git_available(project);
    let safe_smoke_project =
        project.allow_patch && client.view.connected && smoke_marker_present(project);
    let supports_artifact_smoke =
        client.supports(RunnerFeature::FileRead) && client.supports(RunnerFeature::FileWrite);
    let supports_cleanup_verification =
        supports_artifact_smoke || git_available.is_some_and(|available| available);
    let recommended_for_smoke = safe_smoke_project
        && git_available.is_some_and(|available| available)
        && supports_cleanup_verification;

    ProjectCapabilities {
        git_available,
        safe_smoke_project,
        supports_artifact_smoke,
        supports_cleanup_verification,
        recommended_for_smoke,
    }
}

/// Resolve which shell profile a project uses and whether it is configured.
/// Returns `(resolved_name, status)` where:
/// - `resolved_name` = `project_shell_profile` (if set) else the Runner's
///   `default_profile` (if any) else `None`.
/// - `status` = `"configured"` if the resolved name exists in the Runner's
///   configured profiles; `"missing"` if a name resolved but is not
///   configured; `"not_configured"` if no profile resolves at all; and
///   `"unknown"` if the Runner did not report a shell-profiles summary so the
///   configured set cannot be checked.
pub(super) fn resolve_project_shell_profile(
    project_shell_profile: Option<&str>,
    summary: Option<&crate::runner_protocol::ShellProfilesSummary>,
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

/// Validate the project `id` field server-side. The Runner does the
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
/// UTF-8 bytes, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 UTF-8 bytes".to_string());
    }
    Ok(())
}

/// Validate the optional `description` field: <= 500 UTF-8 bytes, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 UTF-8 bytes".to_string());
    }
    Ok(())
}

pub(super) use webcodex_core::runtime_contract::validate_project_op_path;

/// Truncate a string for inclusion in an error message (bounded).
fn truncate_for_error(s: &str) -> String {
    const MAX_BYTES: usize = 200;
    if s.len() <= MAX_BYTES {
        s.to_string()
    } else {
        let mut end = MAX_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

/// Parse a `RunnerProjectSummary` from the Runner's project-op JSON
/// response so the server can upsert it into the cached project list. The
/// response includes `agent_project_id`, `client_id`, `name`, `path`, and
/// `allow_patch` — enough to build a summary that `list_projects` can show
/// immediately.
fn parse_project_summary_from_result(
    result: &Value,
    _client_id: &str,
) -> Option<RunnerProjectSummary> {
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
    let lineage = match result.get("lineage") {
        None | Some(Value::Null) => None,
        Some(value) => Some(serde_json::from_value(value.clone()).ok()?),
    };
    Some(RunnerProjectSummary {
        registration_incarnation: result
            .get("registration_incarnation")
            .and_then(Value::as_str)
            .map(str::to_string),
        id: agent_project_id.to_string(),
        name: name.or_else(|| Some(agent_project_id.to_string())),
        path: path.to_string(),
        allow_patch,
        kind: result
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        registration_source: result
            .get("registration_source")
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
        root_fingerprint: result
            .get("root_fingerprint")
            .and_then(Value::as_str)
            .map(str::to_string),
        lineage,
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
    fn legacy_managed_temporary_kind_is_projected_as_ordinary_registration() {
        let project = RunnerProjectSummary {
            registration_incarnation: None,
            id: "legacy".to_string(),
            name: Some("Legacy".to_string()),
            path: "/tmp/legacy".to_string(),
            allow_patch: true,
            kind: Some("managed_temporary".to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 0,
            shell_profile: None,
        };
        assert_eq!(project_source(&project), "agent_registered");
    }

    #[test]
    fn old_runner_auto_registered_kind_is_compatibility_fallback() {
        let mut project = RunnerProjectSummary {
            registration_incarnation: None,
            id: "legacy-auto".to_string(),
            name: Some("Legacy Auto".to_string()),
            path: "/tmp/legacy-auto".to_string(),
            allow_patch: true,
            kind: Some(AUTO_REGISTERED_PROJECT_SOURCE.to_string()),
            registration_source: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            root_fingerprint: None,
            lineage: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 0,
            shell_profile: None,
        };
        assert_eq!(project_source(&project), AUTO_REGISTERED_PROJECT_SOURCE);
        project.registration_source = Some("explicit".to_string());
        assert_eq!(
            project_source(&project),
            "agent_registered",
            "new provenance must override the legacy kind sentinel"
        );
        project.registration_source = Some("future_registration_source".to_string());
        assert_eq!(project_source(&project), "agent_registered");
    }

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
    fn validation_bounds_are_utf8_bytes() {
        let name_error = validate_project_op_name(&"界".repeat(41)).unwrap_err();
        assert!(name_error.contains("120 UTF-8 bytes"));

        let description_error = validate_project_op_description(&"界".repeat(167)).unwrap_err();
        assert!(description_error.contains("500 UTF-8 bytes"));
    }

    #[test]
    fn truncate_for_error_never_splits_utf8() {
        let input = "界".repeat(67);
        let truncated = truncate_for_error(&input);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncated.trim_end_matches('…').len(), 198);
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

    fn root_scoped_auth(roots: &[&std::path::Path]) -> AuthContext {
        let mut auth = AuthContext::new(crate::auth::AuthKind::OAuth2Token);
        auth.token_kind = Some(crate::auth::PROJECT_SHARE_OAUTH_TOKEN_KIND.to_string());
        auth.project_grant_id = Some("wc_pgrant_test".to_string());
        auth.project_root_scopes = roots.iter().map(|root| root.to_path_buf()).collect();
        auth
    }

    #[test]
    fn project_root_scope_admits_canonical_children_and_root_itself() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        let inner = base.join("magpie");
        std::fs::create_dir_all(&inner).expect("fixture directories");
        // An intermediate directory that really exists, so the `..` spelling
        // below resolves through it.
        std::fs::create_dir_all(base.join("nested")).expect("fixture intermediate");
        let root_canonical = base.canonicalize().expect("canonical fixture root");
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        assert!(authorized_project_root_scope_admits(
            Some(&auth),
            &inner.to_string_lossy()
        ));
        assert!(authorized_project_root_scope_admits(
            Some(&auth),
            &root_canonical.to_string_lossy()
        ));
        // A `..`-laden spelling resolves to the same canonical directory.
        assert!(authorized_project_root_scope_admits(
            Some(&auth),
            &format!("{}/nested/../magpie", root_canonical.display())
        ));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn project_root_scope_rejects_outside_and_prefix_siblings() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        let inner = base.join("magpie");
        std::fs::create_dir_all(&inner).expect("fixture directories");
        let sibling = format!("{}-extra", base.display());
        std::fs::create_dir_all(&sibling).expect("sibling fixture directory");
        let root_canonical = base.canonicalize().expect("canonical fixture root");
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        // Textual-prefix sibling directory is not inside the root.
        assert!(!authorized_project_root_scope_admits(Some(&auth), &sibling));
        // Parent of the root is outside.
        let parent = root_canonical.parent().unwrap().to_path_buf();
        assert!(!authorized_project_root_scope_admits(
            Some(&auth),
            &parent.to_string_lossy()
        ));
        // Missing directory stays denied (admission never creates directories).
        assert!(!authorized_project_root_scope_admits(
            Some(&auth),
            &base.join("not-created-yet").to_string_lossy()
        ));
        // Relative paths are rejected outright.
        assert!(!authorized_project_root_scope_admits(Some(&auth), "magpie"));
        // A subject without granted roots keeps the fence closed.
        assert!(!authorized_project_root_scope_admits(
            Some(&root_scoped_auth(&[] as &[&std::path::Path])),
            &inner.to_string_lossy()
        ));
        // No auth context at all is rejected.
        assert!(!authorized_project_root_scope_admits(
            None,
            &inner.to_string_lossy()
        ));

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&sibling).ok();
    }

    #[test]
    fn project_root_scope_rejects_symlink_escape() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        let outside = std::env::temp_dir().join(format!(
            "wc-root-scope-outside-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(base.join("real")).expect("fixture root");
        std::fs::create_dir_all(outside.join("secret")).expect("outside fixture");
        let root_canonical = base.canonicalize().expect("canonical fixture root");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, base.join("escape-link")).expect("symlink");
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        // The link name is inside the root, but it resolves outside: denied.
        assert!(!authorized_project_root_scope_admits(
            Some(&auth),
            &base.join("escape-link").join("secret").to_string_lossy()
        ));

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn managed_worktree_op_payload_field_counts_match_the_runner_contract() {
        let canonical = "/tmp/granted/repo".to_string();
        // Ordinary credentials: the exact previous four-field payload.
        let plain =
            managed_worktree_op_payload(canonical.clone(), None, "op".into(), None, None, None);
        assert_eq!(plain.as_object().unwrap().len(), 4);
        assert_eq!(plain["path"], canonical);
        assert!(plain.get("project_root_scopes").is_none());

        // Source identity retains its two-field addition.
        let identities = managed_worktree_op_payload(
            canonical.clone(),
            Some("HEAD".into()),
            "op".into(),
            Some("wt".into()),
            Some(("source".into(), "wc_projroot_1".into())),
            None,
        );
        assert_eq!(identities.as_object().unwrap().len(), 6);
        assert_eq!(identities["expected_source_project_id"], "source");

        // A forwarded share grant adds exactly one field, alone or with the
        // source identity, so the Runner's 5 | 7 acceptance applies.
        let granted = managed_worktree_op_payload(
            canonical.clone(),
            None,
            "op".into(),
            None,
            None,
            Some(vec!["/tmp/granted".into()]),
        );
        assert_eq!(granted.as_object().unwrap().len(), 5);
        assert_eq!(
            granted["project_root_scopes"],
            serde_json::json!(["/tmp/granted"])
        );
        let both = managed_worktree_op_payload(
            canonical,
            Some("HEAD".into()),
            "op".into(),
            None,
            Some(("source".into(), "wc_projroot_1".into())),
            Some(vec!["/tmp/granted".into()]),
        );
        assert_eq!(both.as_object().unwrap().len(), 7);
    }

    #[test]
    fn resolve_or_register_project_op_payload_binds_canonical_target_and_roots() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(base.join("granted/inner")).expect("fixture");
        let root_canonical = base.join("granted").canonicalize().unwrap();
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        // A non-canonical spelling of an admitted target: the payload carries
        // the canonical form the fence authorized plus the grant's roots.
        let payload = resolve_or_register_op_payload(
            &base.join("granted/inner/.").to_string_lossy(),
            Some(&auth),
        )
        .unwrap();
        assert_eq!(
            payload["path"],
            base.join("granted/inner")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(
            payload["project_root_scopes"],
            serde_json::json!([root_canonical.to_string_lossy()])
        );

        // Credentials without parent-root grants keep the exact previous
        // payload shape.
        let plain =
            resolve_or_register_op_payload(&base.join("granted/inner/.").to_string_lossy(), None)
                .unwrap();
        assert_eq!(
            plain,
            serde_json::json!({"path": base.join("granted/inner/.").to_string_lossy()})
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    #[cfg(unix)]
    fn resolve_or_register_project_op_payload_fails_closed_when_target_stops_resolving() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(base.join("granted/inner")).expect("fixture");
        let root_canonical = base.join("granted").canonicalize().unwrap();
        std::os::unix::fs::symlink(base.join("granted/inner"), base.join("granted/link"))
            .expect("symlink");
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        // The fence admits the link spelling; then the target disappears, so
        // nothing resolvable can be submitted: deny with no state change.
        std::fs::remove_dir_all(base.join("granted/inner")).expect("remove target");
        let denied = resolve_or_register_op_payload(
            &base.join("granted/link").to_string_lossy(),
            Some(&auth),
        )
        .unwrap_err();
        assert!(!denied.success);
        assert_eq!(denied.output["state_changed"], false);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn verify_runner_path_rejects_returned_paths_outside_the_grant() {
        let base =
            std::env::temp_dir().join(format!("wc-root-scope-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(base.join("granted/inner")).expect("fixture");
        std::fs::create_dir_all(base.join("outside")).expect("fixture");
        let root_canonical = base.join("granted").canonicalize().unwrap();
        let auth = root_scoped_auth(&[root_canonical.as_path()]);

        // The Runner committed a fresh registration outside the grant: deny,
        // and report the effect honestly (state_changed follows `registered`).
        let rejected = verify_runner_path_within_granted_roots(
            ToolResult::ok(json!({
                "path": base.join("outside").canonicalize().unwrap().to_string_lossy(),
                "registered": true,
            })),
            Some(&auth),
        );
        assert!(!rejected.success);
        assert_eq!(rejected.output["state_changed"], true);

        // An inside target passes through unchanged.
        let inside_path = base.join("granted/inner").canonicalize().unwrap();
        let passed = verify_runner_path_within_granted_roots(
            ToolResult::ok(json!({
                "path": inside_path.to_string_lossy(),
                "registered": true,
            })),
            Some(&auth),
        );
        assert!(passed.success);

        // A success without a resolvable path cannot be verified: deny, but
        // keep whatever effect state the Runner did report.
        let denied = verify_runner_path_within_granted_roots(
            ToolResult::ok(json!({"registered": false})),
            Some(&auth),
        );
        assert!(!denied.success);
        assert_eq!(denied.output["state_changed"], false);
        let denied = verify_runner_path_within_granted_roots(
            ToolResult::ok(json!({"registered": true})),
            Some(&auth),
        );
        assert!(!denied.success);
        assert_eq!(denied.output["state_changed"], true);

        // Effect metadata missing or malformed uses the shared indeterminate
        // contract: the write may have landed, so it is never claimed as
        // no-change.
        for node in [json!({}), json!({"registered": "yes"})] {
            let denied = verify_runner_path_within_granted_roots(ToolResult::ok(node), Some(&auth));
            assert!(!denied.success);
            assert_eq!(denied.output["error_kind"], "operation_indeterminate");
            assert_eq!(denied.output["state_changed"], true);
        }

        // Failing results and credentials without grants pass through.
        let failed = ToolResult::err("runner unavailable");
        assert!(!verify_runner_path_within_granted_roots(failed, Some(&auth)).success);
        let unscoped = verify_runner_path_within_granted_roots(
            ToolResult::ok(json!({"path": "anything"})),
            None,
        );
        assert!(unscoped.success);

        std::fs::remove_dir_all(&base).ok();
    }
}
