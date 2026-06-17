use super::types::{InstanceInfo, ProjectCapabilities, ProjectCapabilityInfo, ProjectsResponse};
use super::{get_projects, get_projects_config_path, get_projects_load_error};
use crate::action_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::auth::get_config;
use crate::get_db;
use crate::projects::{Executor, ProjectConfig};
use crate::shell_protocol::ShellClientView;
use crate::ShellClientRegistry;
use salvo::prelude::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn instance_info(depot: &Depot) -> InstanceInfo {
    let data_dir = get_config(depot)
        .map(|cfg| cfg.data_dir.display().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    InstanceInfo {
        service: "private-drop".to_string(),
        api: "codex".to_string(),
        schema: "codex-openapi-compact".to_string(),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        server_time: chrono::Utc::now().timestamp(),
        pid: std::process::id(),
        hostname: hostname(),
        data_dir,
        projects_config_path: get_projects_config_path(depot).unwrap_or_else(|| {
            std::env::var("PROJECTS_CONFIG").unwrap_or_else(|_| "./projects.toml".to_string())
        }),
        public_url: std::env::var("PUBLIC_URL")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}

fn project_info(
    name: &str,
    project: &ProjectConfig,
    shell_clients: &HashMap<String, ShellClientView>,
    ssh_enabled: bool,
) -> ProjectCapabilityInfo {
    let mut commands = project.commands.keys().cloned().collect::<Vec<_>>();
    commands.sort();
    let mut hooks = project.hooks.keys().cloned().collect::<Vec<_>>();
    hooks.sort();
    let allowed_checks = project.effective_allowed_checks();
    let mut configured_checks = project.configured_check_names();
    configured_checks.sort();
    let ssh_endpoints = if project.executor == Executor::Ssh {
        project.ssh_targets()
    } else {
        Vec::new()
    };
    let ssh_target = ssh_endpoints.first().cloned();
    let agent_client_id = if project.executor == Executor::Agent {
        project.client_id.clone()
    } else {
        None
    };
    let agent = agent_client_id
        .as_deref()
        .and_then(|client_id| shell_clients.get(client_id));
    ProjectCapabilityInfo {
        name: name.to_string(),
        executor: match project.executor {
            Executor::Local => "local".to_string(),
            Executor::Ssh => "ssh".to_string(),
            Executor::Agent => "agent".to_string(),
        },
        root: project.path.clone(),
        ssh_enabled,
        ssh_target,
        ssh_endpoints,
        agent_client_id,
        agent_status: agent.map(|client| client.status.clone()).or_else(|| {
            if project.executor == Executor::Agent {
                Some("missing".to_string())
            } else {
                None
            }
        }),
        agent_connected: agent
            .map(|client| client.connected)
            .or_else(|| (project.executor == Executor::Agent).then_some(false)),
        allowed_checks,
        configured_checks: configured_checks.clone(),
        commands: commands.clone(),
        hooks: hooks.clone(),
        default_apply_patch_backend: project
            .default_apply_patch_backend
            .clone()
            .unwrap_or_else(|| "builtin".to_string()),
        capabilities: ProjectCapabilities {
            edit: true,
            patch: project.allow_patch(),
            artifact: true,
            git: true,
            project_doctor: true,
            checks: !configured_checks.is_empty() && project.checks_enabled(),
            jobs: true,
            command_requests: project.allow_command_requests,
            raw_command_requests: project.allow_raw_command_requests,
            configured_commands: !commands.is_empty(),
            configured_hooks: !hooks.is_empty(),
            reports: true,
        },
    }
}

#[handler]
pub async fn codex_projects(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let started_at = chrono::Utc::now().timestamp();
    let audit_db = get_db(depot);
    let explicit_session_id = request_action_session_id(req);
    let Some(projects) = get_projects(depot) else {
        let load_error = get_projects_load_error(depot)
            .unwrap_or_else(|| "Projects config not loaded".to_string());
        let response = ProjectsResponse {
            success: false,
            projects: Vec::new(),
            project_names: Vec::new(),
            instance: Some(instance_info(depot)),
            error: Some(load_error),
            recommended_next_action: Some(
                "Fix projects.toml or PROJECTS_CONFIG, restart or reload the service, then call getCodexProjects."
                    .to_string(),
            ),
            action_budget_hint: Some(
                "Use getCodexProjects once per session, then batch context reads.".to_string(),
            ),
        };
        let error_summary = response.error.clone();
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(response));
        if let Some(db) = audit_db.as_ref() {
            let ended_at = chrono::Utc::now().timestamp();
            record_action_event(
                db,
                ActionAuditEventInput {
                    explicit_session_id,
                    session_title: None,
                    endpoint: "/api/codex/projects".to_string(),
                    action_name: "getCodexProjects".to_string(),
                    operation: Some("list".to_string()),
                    project: None,
                    status: "failed".to_string(),
                    http_status: Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16() as i64),
                    started_at,
                    ended_at,
                    duration_ms: (ended_at - started_at).max(0) * 1000,
                    error_summary,
                    warning_summary: None,
                    changed_files: Vec::new(),
                    ids: json!({}),
                    summary: json!({
                        "project_count": 0,
                        "project_names_count": 0,
                    }),
                    request_bytes: None,
                    response_bytes: None,
                },
            );
        }
        return;
    };
    let mut project_names = projects.available_project_names();
    project_names.sort();
    let shell_clients = match depot.obtain::<Arc<ShellClientRegistry>>() {
        Ok(registry) => registry
            .list_clients()
            .await
            .into_iter()
            .map(|client| (client.client_id.clone(), client))
            .collect::<HashMap<_, _>>(),
        Err(_) => HashMap::new(),
    };
    let ssh_enabled = super::is_ssh_enabled(depot);
    let mut infos = project_names
        .iter()
        .filter_map(|name| {
            projects
                .projects
                .get(name)
                .map(|p| project_info(name, p, &shell_clients, ssh_enabled))
        })
        .collect::<Vec<_>>();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    let response = ProjectsResponse {
        success: true,
        projects: infos,
        project_names,
        instance: Some(instance_info(depot)),
        error: None,
        recommended_next_action: Some(
            "Pick one project, then call getProjectContextBatch for overview, git_status, tree, and targeted reads."
                .to_string(),
        ),
        action_budget_hint: Some(
            "Batch related reads; prefer context_batch, edit, job, and action_sessions over many small calls."
                .to_string(),
        ),
    };
    let project_count = response.projects.len();
    let project_names_count = response.project_names.len();
    let ssh_project_count = response
        .projects
        .iter()
        .filter(|project| project.executor == "ssh")
        .count();
    let agent_project_count = response
        .projects
        .iter()
        .filter(|project| project.executor == "agent")
        .count();
    let connected_agent_project_count = response
        .projects
        .iter()
        .filter(|project| project.executor == "agent" && project.agent_connected == Some(true))
        .count();
    res.render(Json(response));
    if let Some(db) = audit_db.as_ref() {
        let ended_at = chrono::Utc::now().timestamp();
        record_action_event(
            db,
            ActionAuditEventInput {
                explicit_session_id,
                session_title: None,
                endpoint: "/api/codex/projects".to_string(),
                action_name: "getCodexProjects".to_string(),
                operation: Some("list".to_string()),
                project: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at,
                ended_at,
                duration_ms: (ended_at - started_at).max(0) * 1000,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({
                    "project_count": project_count,
                    "project_names_count": project_names_count,
                    "ssh_project_count": ssh_project_count,
                    "agent_project_count": agent_project_count,
                    "connected_agent_project_count": connected_agent_project_count,
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::action_sessions::{
        compute_stats, decode_event, record_action_event, ActionAuditEventInput,
    };
    use crate::Database;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn projects_event_is_lightweight_and_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&tmp.path().join("drop.db")).unwrap());
        record_action_event(
            &db,
            ActionAuditEventInput {
                explicit_session_id: Some("session-projects".to_string()),
                session_title: None,
                endpoint: "/api/codex/projects".to_string(),
                action_name: "getCodexProjects".to_string(),
                operation: Some("list".to_string()),
                project: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: 1,
                ended_at: 2,
                duration_ms: 10,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({
                    "project_count": 2,
                    "project_names_count": 2,
                    "instance": {"public_url": "http://localhost:8080"}
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
        let event = decode_event(
            db.list_action_events("session-projects", 10)
                .unwrap()
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(event.endpoint, "/api/codex/projects");
        assert_eq!(event.summary["project_count"], 2);
        let stats = compute_stats(&[event]);
        assert_eq!(stats.by_endpoint["/api/codex/projects"], 1);
    }
}
