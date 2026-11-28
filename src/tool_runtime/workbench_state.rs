//! Read-only Workbench projection assembled from canonical runtime observations.

use super::kernel::{
    check_runtime_tool_scope, HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
};
use super::permissions::{add_permission_to_result, permission_execution_denied_result};
use super::{ToolResult, ToolRuntime};
use crate::auth::{AuthContext, AuthKind};
use serde_json::{json, Map, Value};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;

fn error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(message, json!({"error_kind":kind,"state_changed":false}))
}

fn settings_value(instance_name: String, display_name_override: Option<String>) -> Value {
    let display_name = display_name_override
        .clone()
        .unwrap_or_else(|| instance_name.clone());
    json!({
        "instance_name": instance_name,
        "display_name_override": display_name_override,
        "display_name": display_name,
    })
}

impl ToolRuntime {
    /// Read the configured instance name and this authenticated principal's override.
    pub(crate) fn workbench_settings(&self, auth: Option<&AuthContext>) -> ToolResult {
        let config = match crate::config::WorkbenchConfig::from_env() {
            Ok(config) => config,
            Err(message) => return error("invalid_instance_name", &message),
        };
        let principal = match super::communication::communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return error(
                "workbench_preferences_unavailable",
                "Workbench preferences are unavailable",
            );
        };
        match db.read_workbench_display_name(&principal) {
            Ok(override_name) => {
                ToolResult::ok(settings_value(config.instance_name, override_name))
            }
            Err(store_error) => error(store_error.code(), store_error.message()),
        }
    }

    /// Update or clear this authenticated principal's Workbench display-name override.
    pub(crate) fn update_workbench_settings(
        &self,
        display_name: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if auth.is_some_and(|auth| auth.kind == AuthKind::OpenAnonymous) {
            return error(
                "workbench_settings_access_denied",
                "Anonymous callers cannot change Workbench settings",
            );
        }
        if check_runtime_tool_scope(auth, "update_goal").is_err() {
            return error(
                "insufficient_scope",
                "The caller lacks the required Workbench settings scope",
            );
        }
        let principal = match super::communication::communication_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return error(
                "workbench_preferences_unavailable",
                "Workbench preferences are unavailable",
            );
        };
        let config = match crate::config::WorkbenchConfig::from_env() {
            Ok(config) => config,
            Err(message) => return error("invalid_instance_name", &message),
        };
        // This app-only adapter does not enter the canonical ToolCall dispatcher.
        // Reuse its authority evaluator before persisting either a name or a clear.
        let permission = self.permission_evaluator.evaluate_resolved_required(
            "update_workbench_settings",
            None,
            "write",
        );
        if matches!(permission.status.as_str(), "denied" | "pending") {
            let mut result = permission_execution_denied_result(&permission);
            add_permission_to_result(&mut result, &permission);
            result.output["state_changed"] = json!(false);
            return result;
        }
        let mut result = match db.set_workbench_display_name(&principal, display_name.as_deref()) {
            Ok(name) => ToolResult::ok(settings_value(config.instance_name, name)),
            Err(store_error) => error(store_error.code(), store_error.message()),
        };
        add_permission_to_result(&mut result, &permission);
        result
    }

    /// Aggregate a bounded Workbench view from canonical, scope-checked reads.
    pub(crate) async fn read_workbench_state(
        &self,
        project: Option<String>,
        session_id: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        if limit == 0 || limit > MAX_LIMIT {
            return error(
                "invalid_workbench_page",
                "Workbench limit must be between 1 and 50",
            );
        }

        let mut errors = Map::new();
        let settings_result = self.workbench_settings(auth);
        let default_settings = crate::config::WorkbenchConfig::from_env()
            .map(|config| settings_value(config.instance_name, None))
            .unwrap_or(Value::Null);
        let settings = section_value("settings", settings_result, default_settings, &mut errors);

        let project_requested = project.is_some();
        let resolved_project = match project {
            Some(input) => {
                if check_runtime_tool_scope(auth, "list_projects").is_err() {
                    section_error(
                        "project",
                        "insufficient_scope",
                        "Project access is unavailable",
                        &mut errors,
                    );
                    None
                } else {
                    match self.resolve_project_metadata_for_auth(&input, auth).await {
                        Ok(resolved) => Some(resolved.resolved_id),
                        Err(_) => {
                            section_error(
                                "project",
                                "project_unavailable",
                                "Project is unavailable",
                                &mut errors,
                            );
                            None
                        }
                    }
                }
            }
            None => None,
        };

        let mut tasks = if project_requested && resolved_project.is_none() {
            ToolResult::err_with_output(
                "Project is unavailable",
                json!({"error_kind":"project_unavailable","state_changed":false}),
            )
        } else {
            self.canonical_observation(
                "list_agent_tasks",
                json!({"offset":offset,"limit":limit}),
                auth,
                window,
            )
            .await
        };
        if tasks.success {
            project_task_summaries(&mut tasks.output);
            if let Some(project_id) = resolved_project.as_deref() {
                let source_total = tasks
                    .output
                    .get("total_count")
                    .cloned()
                    .unwrap_or(Value::Null);
                let source_offset = tasks.output.get("offset").cloned().unwrap_or(json!(offset));
                let source_next = tasks
                    .output
                    .get("next_offset")
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(array) = tasks.output.get_mut("tasks").and_then(Value::as_array_mut) {
                    array.retain(|task| {
                        task.get("referenced_project_id").and_then(Value::as_str)
                            == Some(project_id)
                    });
                }
                tasks.output["source_total_count"] = source_total;
                tasks.output["source_offset"] = source_offset;
                tasks.output["source_next_offset"] = source_next;
                tasks.output["project_filtered_after_page"] = json!(true);
            }
        }
        let task_fallback = json!({
            "total_count":0,"offset":offset,"next_offset":null,"truncated":false,
            "tasks":[],"source_total_count":0,"source_offset":offset,
            "source_next_offset":null,"project_filtered_after_page":project_requested,
        });
        let tasks = section_value("tasks", tasks, task_fallback, &mut errors);

        let goals = self
            .canonical_observation(
                "list_goals",
                json!({"offset":offset,"limit":limit}),
                auth,
                window,
            )
            .await;
        let goals = section_value(
            "goals",
            goals,
            json!({"total_count":0,"offset":offset,"next_offset":null,"truncated":false,"goals":[]}),
            &mut errors,
        );

        let agents = self
            .canonical_observation(
                "list_agent_identities",
                json!({"offset":0,"limit":MAX_LIMIT}),
                auth,
                window,
            )
            .await;
        let mut agents = section_value(
            "agents",
            agents,
            json!({"total_count":0,"offset":0,"next_offset":null,"truncated":false,"agents":[]}),
            &mut errors,
        );

        if let Some(items) = agents.get_mut("agents").and_then(Value::as_array_mut) {
            for agent in items {
                if let Some(fields) = agent.as_object_mut() {
                    fields.retain(|key, _| {
                        matches!(
                            key.as_str(),
                            "agent_id"
                                | "display_name"
                                | "profile_revision"
                                | "production_auto_resume_available"
                        )
                    });
                }
            }
        }

        let providers = if let Some(project_id) = resolved_project.as_deref() {
            let runner_id = project_runner_id(project_id);
            match runner_id {
                Some(client_id) => {
                    let result = self
                        .canonical_observation(
                            "list_runners",
                            json!({"client_id":client_id,"include_projects":false,"summary_only":true,"limit":1}),
                            auth,
                            window,
                        )
                        .await;
                    if result.success {
                        let runner = result
                            .output
                            .get("runners")
                            .and_then(Value::as_array)
                            .and_then(|runners| runners.first());
                        if let Some(runner) = runner.filter(|runner| {
                            runner.get("connected").and_then(Value::as_bool) == Some(true)
                        }) {
                            runner
                                .get("coding_agent_providers")
                                .cloned()
                                .unwrap_or_else(|| json!([]))
                        } else {
                            section_error(
                                "providers",
                                "runner_unavailable",
                                "The selected Project's Runner is offline or unavailable",
                                &mut errors,
                            );
                            json!([])
                        }
                    } else {
                        let _ = section_value("providers", result, json!([]), &mut errors);
                        json!([])
                    }
                }
                None => {
                    section_error(
                        "providers",
                        "project_unavailable",
                        "Project Runner is unavailable",
                        &mut errors,
                    );
                    json!([])
                }
            }
        } else if project_requested {
            section_error(
                "providers",
                "project_unavailable",
                "Project providers are unavailable",
                &mut errors,
            );
            json!([])
        } else {
            json!([])
        };

        let todos = if let Some(raw_session) = session_id {
            if check_runtime_tool_scope(auth, "list_session_messages").is_err() {
                section_error(
                    "todos",
                    "insufficient_scope",
                    "Session messages are unavailable",
                    &mut errors,
                );
                json!([])
            } else {
                if resolved_project.is_none() {
                    section_error(
                        "todos",
                        "project_required",
                        "Select an authorized Project before reading Session todos",
                        &mut errors,
                    );
                    json!([])
                } else {
                    match self.canonicalize_explicit_session_selector(&raw_session, auth) {
                        Ok(session)
                            if self
                                .observe_bound_session_target(
                                    &session,
                                    "list_session_messages",
                                    auth,
                                )
                                .await
                                .is_ok() =>
                        {
                            let session_project = self
                                .sessions
                                .summary(&session, Some(1))
                                .and_then(|summary| summary.project);
                            if session_project.as_ref() != resolved_project.as_ref() {
                                section_error(
                                    "todos",
                                    "session_project_mismatch",
                                    "Session does not belong to the selected Project",
                                    &mut errors,
                                );
                                json!([])
                            } else {
                                let result = self.canonical_observation(
                            "list_session_messages",
                            json!({"session_id":session,"kind":"todo","status":"open","limit":limit}),
                            auth,
                            window,
                        ).await;
                                if result.success {
                                    let mut messages = result
                                        .output
                                        .get("messages")
                                        .cloned()
                                        .unwrap_or_else(|| json!([]));
                                    if let Some(messages) = messages.as_array_mut() {
                                        for message in messages {
                                            if let Some(object) = message.as_object_mut() {
                                                object.insert(
                                                    "session_id".to_owned(),
                                                    json!(session),
                                                );
                                            }
                                        }
                                    }
                                    messages
                                } else {
                                    let _ = section_value("todos", result, json!([]), &mut errors);
                                    json!([])
                                }
                            }
                        }
                        _ => {
                            section_error(
                                "todos",
                                "session_unavailable",
                                "Session messages are unavailable",
                                &mut errors,
                            );
                            json!([])
                        }
                    }
                }
            }
        } else {
            json!([])
        };

        let capabilities = json!({
            "manage_goals": check_runtime_tool_scope(auth, "update_goal").is_ok(),
            "submit_work": check_runtime_tool_scope(auth, "create_agent_task").is_ok(),
            "settings": auth.is_some_and(|auth| auth.kind != AuthKind::OpenAnonymous)
                && check_runtime_tool_scope(auth, "update_goal").is_ok(),
        });
        ToolResult::ok(json!({
            "settings": settings,
            "tasks": tasks,
            "goals": goals,
            "agents": agents,
            "providers": providers,
            "todos": todos,
            "capabilities": capabilities,
            "execution_options": [],
            "errors": errors,
            "project": resolved_project,
            "offset": offset,
            "limit": limit,
        }))
    }

    async fn canonical_observation(
        &self,
        tool: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        self.call_tool_with_context(
            ToolCallRequest {
                tool_name: tool.to_owned(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth,
                window,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await
        .result
        .unwrap_or_else(|| {
            error(
                "canonical_read_unavailable",
                "Canonical read is unavailable",
            )
        })
    }
}

// Aggregate pages carry compact summaries. The exact detail read owns full
// terminal output; never repeat up to fifty result bodies in a homepage poll.
fn project_task_summaries(output: &mut Value) {
    if let Some(tasks) = output.get_mut("tasks").and_then(Value::as_array_mut) {
        for task in tasks {
            if let Some(attempt) = task
                .get_mut("latest_attempt")
                .and_then(Value::as_object_mut)
            {
                attempt.remove("terminal_result");
                if let Some(reason) = attempt.get("terminal_reason").and_then(Value::as_str) {
                    if reason.len() > 512 {
                        let mut end = 512;
                        while !reason.is_char_boundary(end) {
                            end -= 1;
                        }
                        let excerpt = reason[..end].to_owned();
                        attempt.insert("terminal_reason".into(), json!(excerpt));
                        attempt.insert("terminal_reason_truncated".into(), json!(true));
                    }
                }
            }
        }
    }
}

fn project_runner_id(project_id: &str) -> Option<&str> {
    let rest = project_id.strip_prefix("agent:")?;
    let (client_id, project) = rest.split_once(':')?;
    (!client_id.is_empty() && !project.is_empty()).then_some(client_id)
}

fn section_value(
    section: &str,
    result: ToolResult,
    fallback: Value,
    errors: &mut Map<String, Value>,
) -> Value {
    if result.success {
        result.output
    } else {
        let kind = result
            .output
            .get("error_kind")
            .and_then(Value::as_str)
            .unwrap_or("canonical_read_failed");
        let message = result.error.as_deref().unwrap_or("Canonical read failed");
        errors.insert(
            section.to_owned(),
            json!({"error_kind":kind,"message":message}),
        );
        fallback
    }
}

fn section_error(section: &str, kind: &str, message: &str, errors: &mut Map<String, Value>) {
    errors.insert(
        section.to_owned(),
        json!({"error_kind":kind,"message":message}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{SCOPE_COMMUNICATION_MANAGE, SCOPE_COMMUNICATION_READ};
    use std::sync::Arc;

    fn auth(user_id: &str, username: &str) -> AuthContext {
        let mut auth = AuthContext::new(AuthKind::ApiToken);
        auth.user_id = Some(user_id.to_owned());
        auth.username = Some(username.to_owned());
        auth.scopes = vec![
            SCOPE_COMMUNICATION_READ.to_owned(),
            SCOPE_COMMUNICATION_MANAGE.to_owned(),
        ];
        auth
    }

    #[test]
    fn aggregate_omits_terminal_bodies_and_marks_utf8_reason_excerpts() {
        let mut page = json!({"tasks":[{"task_id":"unchanged","latest_attempt":{"terminal_result":"sensitive result body".repeat(1000),"terminal_reason":"界".repeat(1000)}}],"next_offset":50});
        project_task_summaries(&mut page);
        assert_eq!(page["next_offset"], 50);
        assert_eq!(page["tasks"][0]["task_id"], "unchanged");
        let attempt = &page["tasks"][0]["latest_attempt"];
        assert!(attempt.get("terminal_result").is_none());
        assert!(attempt["terminal_reason"].as_str().unwrap().len() <= 512);
        assert_eq!(attempt["terminal_reason_truncated"], true);
        assert!(serde_json::to_vec(&page).unwrap().len() < 1024);
    }

    #[tokio::test]
    async fn state_keeps_partial_sections_when_persistent_store_is_missing() {
        let runtime = ToolRuntime::new_for_tests();
        let result = runtime
            .read_workbench_state(None, None, None, None, None, None)
            .await;
        assert!(result.success);
        assert_eq!(
            result.output["settings"]["display_name_override"],
            Value::Null
        );
        assert!(result.output["settings"]["instance_name"].is_string());
        assert_eq!(
            result.output["errors"]["settings"]["error_kind"],
            "workbench_preferences_unavailable"
        );
        assert_eq!(
            result.output["errors"]["tasks"]["error_kind"],
            "agent_task_store_unavailable"
        );
        assert_eq!(
            result.output["errors"]["goals"]["error_kind"],
            "goal_store_unavailable"
        );
        assert_eq!(
            result.output["errors"]["agents"]["error_kind"],
            "communication_store_unavailable"
        );
        assert_eq!(result.output["tasks"]["tasks"], json!([]));
        assert_eq!(result.output["goals"]["goals"], json!([]));
        assert_eq!(result.output["agents"]["agents"], json!([]));
    }

    #[test]
    fn settings_follow_stable_owner_identity_across_username_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::Database::open(&temp.path().join("workbench.db")).unwrap());
        let runtime = ToolRuntime::new_for_tests().with_communication_database(db);
        let owner = auth("stable-user", "old-alias");
        let alias = auth("stable-user", "new-alias");
        let other = auth("other-user", "old-alias");

        assert!(
            runtime
                .update_workbench_settings(Some("Personal Bench".to_owned()), Some(&owner))
                .success
        );
        assert_eq!(
            runtime.workbench_settings(Some(&alias)).output["display_name_override"],
            "Personal Bench"
        );
        assert_eq!(
            runtime.workbench_settings(Some(&other)).output["display_name_override"],
            Value::Null
        );
    }
}
