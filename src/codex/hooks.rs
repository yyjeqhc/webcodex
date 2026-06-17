use super::shell::sanitize_tail;
use super::types::{ProjectHookRequest, ProjectHookResponse, ProjectHookStep};
use super::{ensure_ssh_enabled, get_projects, run_project_cmd, MAX_OUTPUT_LEN};
use crate::projects::{ProjectConfig, ProjectsConfig};
use salvo::prelude::*;

const DEFAULT_HOOK_TIMEOUT_SECS: u64 = 120;
const MAX_HOOK_TIMEOUT_SECS: u64 = 24 * 60 * 60;
const MAX_HOOK_COMMAND_LEN: usize = 2_000;

fn hook_error(project: String, hook: String, error: String) -> ProjectHookResponse {
    ProjectHookResponse {
        success: false,
        project,
        hook,
        steps: Vec::new(),
        git_status_short: String::new(),
        error: Some(error),
    }
}

fn validate_hook_command(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("hook command cannot be empty".to_string());
    }
    if command.chars().count() > MAX_HOOK_COMMAND_LEN {
        return Err(format!(
            "hook command is too long; maximum is {} characters",
            MAX_HOOK_COMMAND_LEN
        ));
    }
    Ok(())
}

fn hook_timeout_secs(timeout_secs: Option<u64>) -> u64 {
    timeout_secs
        .unwrap_or(DEFAULT_HOOK_TIMEOUT_SECS)
        .clamp(1, MAX_HOOK_TIMEOUT_SECS)
}

fn get_hook_commands<'a>(proj: &'a ProjectConfig, hook: &str) -> Result<&'a [String], String> {
    let hook_name = hook.trim();
    if hook_name.is_empty() {
        return Err("hook cannot be empty".to_string());
    }
    let Some(commands) = proj.hooks.get(hook_name) else {
        return Err(format!(
            "hook '{}' is not configured for this project",
            hook_name
        ));
    };
    if commands.is_empty() {
        return Err(format!("hook '{}' has no commands", hook_name));
    }
    Ok(commands)
}

async fn run_hook_command(
    depot: &Depot,
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    command: &str,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    if proj.is_agent() {
        return super::agent_exec::run_agent_project_command(
            depot,
            proj,
            command,
            timeout_secs,
            "codex_project_hook_agent_executor",
            "agent project hook command",
        )
        .await;
    }
    run_project_cmd(proj, command, timeout_secs, projects.ssh.as_ref())
}

async fn git_status_short(
    depot: &Depot,
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
) -> String {
    let (_code, stdout, _stderr, _duration_ms) =
        run_hook_command(depot, projects, proj, "git status --short", 10).await;
    stdout
}

pub(super) async fn run_project_hook(
    depot: &Depot,
    projects: &ProjectsConfig,
    project: &str,
    proj: &ProjectConfig,
    hook: &str,
    commands: &[String],
    timeout_secs: Option<u64>,
) -> ProjectHookResponse {
    let timeout_secs = hook_timeout_secs(timeout_secs);
    let mut steps = Vec::new();
    let mut success = true;
    let mut error = None;

    for command in commands {
        if let Err(e) = validate_hook_command(command) {
            success = false;
            error = Some(e);
            break;
        }
        let (exit_code, stdout, stderr, duration_ms) =
            run_hook_command(depot, projects, proj, command, timeout_secs).await;
        let (stdout_tail, _) = sanitize_tail(&stdout, MAX_OUTPUT_LEN);
        let (stderr_tail, _) = sanitize_tail(&stderr, MAX_OUTPUT_LEN);
        steps.push(ProjectHookStep {
            command: command.clone(),
            exit_code,
            stdout_tail,
            stderr_tail,
            duration_ms,
        });
        if exit_code != 0 {
            success = false;
            error = Some("hook command failed".to_string());
            break;
        }
    }

    let git_status_short = git_status_short(depot, projects, proj).await;
    ProjectHookResponse {
        success,
        project: project.to_string(),
        hook: hook.to_string(),
        steps,
        git_status_short,
        error,
    }
}

#[handler]
pub async fn codex_project_hook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.render(Json(hook_error(
            String::new(),
            String::new(),
            "Projects not configured".to_string(),
        )));
        return;
    };
    let body: ProjectHookRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(hook_error(
                String::new(),
                String::new(),
                format!("Invalid JSON: {}", e),
            )));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(proj) => proj,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(hook_error(body.project, body.hook, e)));
            return;
        }
    };
    if let Err(e) = ensure_ssh_enabled(depot, proj) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(hook_error(body.project, body.hook, e)));
        return;
    }
    let hook_name_owned = body.hook.trim().to_string();
    let hook_name = hook_name_owned.as_str();
    let commands = match get_hook_commands(proj, hook_name) {
        Ok(commands) => commands,
        Err(e) if e == "hook cannot be empty" => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(hook_error(body.project, body.hook, e)));
            return;
        }
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(hook_error(body.project, body.hook, e)));
            return;
        }
    };
    let response = run_project_hook(
        depot,
        &projects,
        &body.project,
        proj,
        hook_name,
        commands,
        body.timeout_secs,
    )
    .await;
    res.render(Json(response));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Executor, ProjectsConfig};
    use std::collections::HashMap;

    fn local_project(path: &str, hooks: HashMap<String, Vec<String>>) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Local,
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
            hooks,
        }
    }

    fn projects_with(project: ProjectConfig) -> ProjectsConfig {
        let mut projects = HashMap::new();
        projects.insert("demo".to_string(), project);
        ProjectsConfig {
            ssh: None,
            projects,
        }
    }

    #[tokio::test]
    async fn project_hook_runs_commands_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let mut hooks = HashMap::new();
        hooks.insert(
            "doctor".to_string(),
            vec!["printf hello".to_string(), "printf world".to_string()],
        );
        let project = local_project(tmp.path().to_str().unwrap(), hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();

        let response = run_project_hook(
            &depot,
            &projects,
            "demo",
            &project,
            "doctor",
            project.hooks.get("doctor").unwrap(),
            Some(5),
        )
        .await;

        assert!(response.success);
        assert_eq!(response.steps.len(), 2);
        assert_eq!(response.steps[0].stdout_tail, "hello");
        assert_eq!(response.steps[1].stdout_tail, "world");
    }

    #[tokio::test]
    async fn project_hook_stops_after_first_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let mut hooks = HashMap::new();
        hooks.insert(
            "precommit".to_string(),
            vec!["false".to_string(), "printf should-not-run".to_string()],
        );
        let project = local_project(tmp.path().to_str().unwrap(), hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();

        let response = run_project_hook(
            &depot,
            &projects,
            "demo",
            &project,
            "precommit",
            project.hooks.get("precommit").unwrap(),
            Some(5),
        )
        .await;

        assert!(!response.success);
        assert_eq!(response.steps.len(), 1);
        assert_eq!(response.steps[0].exit_code, 1);
        assert_eq!(response.error.as_deref(), Some("hook command failed"));
    }

    #[test]
    fn missing_project_hook_has_clear_error() {
        let project = local_project("/tmp/demo", HashMap::new());
        let err = get_hook_commands(&project, "missing").unwrap_err();
        assert_eq!(err, "hook 'missing' is not configured for this project");
    }

    #[test]
    fn ssh_project_hook_is_rejected_when_ssh_disabled() {
        let mut project = local_project("/tmp/demo", HashMap::new());
        project.executor = Executor::Ssh;
        let depot = Depot::new();
        let err = ensure_ssh_enabled(&depot, &project).unwrap_err();
        assert_eq!(err, super::super::ssh_disabled_error());
    }
}
