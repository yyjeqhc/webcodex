//! Read-only target discovery and explicit bounded registration maintenance.
use super::*;
use std::collections::HashSet;
use webcodex_tool_contracts::tool_call::ProjectUnregisterInput;

pub(super) fn registered_git_summary(project: &RunnerProjectSummary) -> Value {
    json!({"branch":project.git_branch,"head":project.git_head,"dirty":project.git_dirty,
        "source":"runner_inventory","freshness":"unverified"})
}

impl ToolRuntime {
    pub(crate) async fn resolve_workspace(
        &self,
        client_id: String,
        path: Option<String>,
        query: Option<String>,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if client_id.is_empty()
            || client_id.chars().count() > 128
            || client_id.chars().any(char::is_control)
            || query.as_ref().is_some_and(|q| {
                q.trim().is_empty() || q.chars().count() > 200 || q.chars().any(char::is_control)
            })
        {
            return ToolResult::err("invalid_workspace_selector");
        }
        let query = query.map(|q| q.trim().to_lowercase());
        if path.is_some() == query.is_some() {
            return ToolResult::err("invalid_workspace_selector");
        }
        if let Some(path) = &path {
            if validate_project_op_path(path).is_err() {
                return ToolResult::err("invalid_project_path");
            }
        }
        let access = crate::runner_http::runner_access_from_auth(auth);
        let clients = self
            .runner_registry
            .list_project_semantic_views_for_auth(access.as_ref(), Some(&client_id), None)
            .await;
        let Some(client) = clients.first() else {
            return ToolResult::err_with_output(
                "runner_unavailable",
                json!({"reason_code":"runner_unavailable"}),
            );
        };
        let mut matches = client
            .view
            .projects
            .iter()
            .filter(|project| {
                let id = runner_project_runtime_id(&client_id, &project.id);
                path.as_ref().is_none_or(|path| project.path == *path)
                    && query
                        .as_deref()
                        .is_none_or(|query| super::project_query_matches(query, &id, project))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| a.id.cmp(&b.id));
        let count = matches.len();
        let limit = limit.unwrap_or(20).clamp(1, 100);
        let mut rows = Vec::new();
        let mut bytes = 512;
        for project in matches.into_iter().take(limit) {
            let id = runner_project_runtime_id(&client_id, &project.id);
            let reference =
                self.project_reference_for_identity(&id, project.root_fingerprint.as_deref(), auth);
            let mut row = json!({"project":id,"client_id":client_id,"name":project.name,
                "path":project.path,"enabled":!project.disabled,"revision":project.revision,
                "connected":client.view.connected,"git":registered_git_summary(project)});
            if let Some(reference) = reference {
                row["project_ref"] = json!(reference);
            }
            bytes += serde_json::to_vec(&row)
                .expect("workspace row serializes")
                .len();
            if bytes > 48 * 1024 {
                break;
            }
            rows.push(row);
        }
        // A positive match identifies existing registration only. No claim is
        // made about unregistered paths or the current state of Runner files.
        if count > 0 && rows.is_empty() {
            return ToolResult::err("workspace_candidate_exceeds_output_bound");
        }
        // The previous published snapshot can remain visible while a new
        // inventory is incomplete. It cannot prove uniqueness or absence.
        let complete = client
            .view
            .project_inventory
            .as_ref()
            .is_some_and(|inventory| inventory.sync_state == "complete");
        let resolution = match (complete, count) {
            (false, _) => "incomplete",
            (true, 0) => "not_found",
            (true, 1) => "resolved",
            (true, _) => "ambiguous",
        };
        let mut output = json!({"resolution":resolution,"source":"registered_projects",
            "client_id":client_id,"connected":client.view.connected,"matched_count":count,
            "truncated":count > rows.len()});
        if complete && count == 1 {
            output["workspace"] = rows.remove(0);
        } else {
            output["candidates"] = json!(rows);
        }
        ToolResult::ok(output)
    }

    pub(crate) async fn unregister_projects(
        &self,
        items: Vec<ProjectUnregisterInput>,
        dry_run: bool,
        confirm: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Some(result) = project_registry_mutation_denied(auth, "unregister_projects") {
            return result;
        }
        if items.is_empty() || items.len() > 16 {
            return ToolResult::err("invalid_batch_size");
        }
        let mut seen = HashSet::new();
        // Reject malformed or duplicate identities before the first effect.
        for item in &items {
            if item.project.len() > 512
                || crate::admin_project_lifecycle::parse_runtime_project(&item.project).is_err()
                || crate::admin_project_lifecycle::validate_revision(&item.expected_revision)
                    .is_err()
                || !seen.insert(&item.project)
            {
                return ToolResult::err("invalid_or_duplicate_project");
            }
        }
        if !dry_run && !confirm {
            return ToolResult::err("confirmation_required");
        }
        let mut results = Vec::with_capacity(items.len());
        let mut stop = false;
        let mut changed = false;
        let mut all_success = true;
        for (index, item) in items.iter().enumerate() {
            if stop {
                all_success = false;
                results.push(json!({"index":index,"project":item.project,"success":false,
                    "error":"not_attempted_after_uncertain_outcome"}));
                continue;
            }
            let result = if dry_run {
                self.preview_unregister_project(item, auth).await
            } else {
                self.unregister_project(item.project.clone(), item.expected_revision.clone(), auth)
                    .await
            };
            let code = result.output.pointer("/error/code").and_then(Value::as_str);
            // A malformed native response may follow an effect, and a failed
            // Server projection is not proof that the registration survived.
            stop = !dry_run
                && !result.success
                && !matches!(
                    code,
                    Some(
                        "revision_conflict"
                            | "active_jobs_conflict"
                            | "project_not_found"
                            | "agent_unavailable"
                            | "unsupported_runner_version"
                            | "invalid_request"
                            | "path_outside_allowed_roots"
                    )
                );
            changed |= result.output["changed"].as_bool() == Some(true)
                || result.output["state_changed"].as_bool() == Some(true);
            all_success &= result.success;
            results.push(
                json!({"index":index,"project":item.project,"success":result.success,
                "output":result.output,"error":result.error}),
            );
        }
        ToolResult {
            success: all_success,
            output: json!({"dry_run":dry_run,"changed":changed,
            "items":results,"outcome_unknown":stop}),
            error: (!all_success).then(|| {
                "Inspect per-item outcomes; do not replay successful or uncertain removals".into()
            }),
        }
    }

    async fn preview_unregister_project(
        &self,
        item: &ProjectUnregisterInput,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let (client_id, project_id) =
            crate::admin_project_lifecycle::parse_runtime_project(&item.project)
                .expect("batch identifiers were validated");
        let access = crate::runner_http::runner_access_from_auth(auth);
        if auth.is_some()
            && self
                .runner_registry
                .assert_runner_access(access.as_ref(), &client_id)
                .await
                .is_err()
        {
            return ToolResult::err("project_unavailable");
        }
        let Some(client) = self
            .runner_registry
            .get_runner_semantic_view_for_auth(&client_id, access.as_ref())
            .await
        else {
            return ToolResult::err("project_unavailable");
        };
        let Some(project) = client.view.projects.iter().find(|p| p.id == project_id) else {
            return ToolResult::err("project_unavailable");
        };
        if project.revision.as_deref() != Some(item.expected_revision.as_str()) {
            return ToolResult::err("revision_conflict");
        }
        ToolResult::ok(
            json!({"outcome":"previewed","path":project.path,"revision":project.revision,
            "connected":client.view.connected,"changed":false,"execution_checks_pending":true}),
        )
    }
}
