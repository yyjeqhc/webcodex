use super::get_projects;
use super::types::{ProjectCapabilities, ProjectCapabilityInfo, ProjectsResponse};
use crate::projects::{Executor, ProjectChecks, ProjectConfig};
use salvo::prelude::*;

fn configured_checks(checks: &Option<ProjectChecks>) -> Vec<String> {
    let Some(checks) = checks else {
        return Vec::new();
    };
    let mut names = Vec::new();
    if checks.fmt.is_some() {
        names.push("fmt".to_string());
    }
    if checks.test.is_some() {
        names.push("test".to_string());
    }
    if checks.build.is_some() {
        names.push("build".to_string());
    }
    if checks.e2e.is_some() {
        names.push("e2e".to_string());
    }
    if checks.full.is_some() {
        names.push("full".to_string());
    }
    names
}

fn project_info(name: &str, project: &ProjectConfig) -> ProjectCapabilityInfo {
    let mut commands = project.commands.keys().cloned().collect::<Vec<_>>();
    commands.sort();
    let mut allowed_checks = project.allowed_checks.clone();
    allowed_checks.sort();
    let configured_checks = configured_checks(&project.checks);
    let ssh_target = if project.executor == Executor::Ssh {
        project.ssh_target().ok()
    } else {
        None
    };
    ProjectCapabilityInfo {
        name: name.to_string(),
        executor: match project.executor {
            Executor::Local => "local".to_string(),
            Executor::Ssh => "ssh".to_string(),
        },
        root: project.path.clone(),
        ssh_target,
        allowed_checks,
        configured_checks: configured_checks.clone(),
        commands: commands.clone(),
        capabilities: ProjectCapabilities {
            edit: true,
            patch: project.allow_patch(),
            artifact: true,
            git: true,
            checks: !configured_checks.is_empty() && !project.allowed_checks.is_empty(),
            jobs: true,
            command_requests: project.allow_command_requests,
            raw_command_requests: project.allow_raw_command_requests,
            configured_commands: !commands.is_empty(),
            reports: true,
        },
    }
}

#[handler]
pub async fn codex_projects(depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ProjectsResponse {
            success: false,
            projects: Vec::new(),
            project_names: Vec::new(),
            error: Some("Projects config not loaded".to_string()),
        }));
        return;
    };
    let mut project_names = projects.available_project_names();
    project_names.sort();
    let mut infos = project_names
        .iter()
        .filter_map(|name| projects.projects.get(name).map(|p| project_info(name, p)))
        .collect::<Vec<_>>();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    res.render(Json(ProjectsResponse {
        success: true,
        projects: infos,
        project_names,
        error: None,
    }));
}
