use super::hooks::run_project_hook;
use super::jobs::{list_local_jobs, list_ssh_jobs};
use super::types::{
    JobInfo, ProjectDoctorAgentCapabilities, ProjectDoctorAgentInfo, ProjectDoctorGitInfo,
    ProjectDoctorHooksInfo, ProjectDoctorRecentJob, ProjectDoctorRequest, ProjectDoctorResponse,
};
use super::{get_projects, is_ssh_enabled, run_project_cmd, SSH_DISABLED_MESSAGE};
use crate::projects::{ProjectConfig, ProjectsConfig};
use crate::shell_client::assert_shell_client_owner;
use crate::shell_protocol::{ShellClientView, ShellJobInfo};
use crate::ShellClientRegistry;
use salvo::prelude::*;
use std::sync::Arc;

const DEFAULT_DOCTOR_HOOK: &str = "doctor";
const MAX_DOCTOR_RECENT_JOBS: usize = 50;
#[cfg(test)]
const DEFAULT_DOCTOR_TIMEOUT_SECS: u64 = 120;
const MAX_DOCTOR_TIMEOUT_SECS: u64 = 24 * 60 * 60;

impl ProjectDoctorRequest {
    fn effective_hook_name(&self) -> String {
        let hook = self.hook.trim();
        if hook.is_empty() {
            DEFAULT_DOCTOR_HOOK.to_string()
        } else {
            hook.to_string()
        }
    }

    fn effective_recent_jobs(&self) -> usize {
        self.recent_jobs.min(MAX_DOCTOR_RECENT_JOBS)
    }

    fn effective_timeout_secs(&self) -> u64 {
        self.timeout_secs.clamp(1, MAX_DOCTOR_TIMEOUT_SECS)
    }
}

#[derive(Default)]
struct AgentInspection {
    info: Option<ProjectDoctorAgentInfo>,
    registry: Option<Arc<ShellClientRegistry>>,
    client_id: Option<String>,
    can_execute: bool,
    can_list_jobs: bool,
    blocker: Option<String>,
    security_error: Option<String>,
}

pub(super) fn executor_name(proj: &ProjectConfig) -> &'static str {
    if proj.is_agent() {
        "agent"
    } else if proj.is_ssh() {
        "ssh"
    } else {
        "local"
    }
}

fn unavailable_git(error: String) -> ProjectDoctorGitInfo {
    ProjectDoctorGitInfo {
        available: false,
        branch: None,
        head: None,
        head_subject: None,
        status_short: None,
        dirty: None,
        error: Some(error),
    }
}

fn empty_hooks_info(hook_name: String) -> ProjectDoctorHooksInfo {
    ProjectDoctorHooksInfo {
        configured: Vec::new(),
        doctor_hook: hook_name,
        doctor_hook_configured: false,
        recommended_next: None,
    }
}

fn doctor_error(project: String, hook: String, error: String) -> ProjectDoctorResponse {
    let hook_name = if hook.trim().is_empty() {
        DEFAULT_DOCTOR_HOOK.to_string()
    } else {
        hook.trim().to_string()
    };
    ProjectDoctorResponse {
        success: false,
        project,
        executor: "local".to_string(),
        root: String::new(),
        ssh_enabled: false,
        agent: None,
        git: unavailable_git(error.clone()),
        hooks: empty_hooks_info(hook_name),
        hook_result: None,
        recent_jobs: Vec::new(),
        warnings: Vec::new(),
        error: Some(error),
    }
}

fn build_hooks_info(proj: &ProjectConfig, doctor_hook: &str) -> ProjectDoctorHooksInfo {
    let mut configured = proj.hooks.keys().cloned().collect::<Vec<_>>();
    configured.sort();
    let doctor_hook_configured = proj.hooks.contains_key(doctor_hook);
    let recommended_next = if proj.hooks.contains_key("precommit") {
        Some("precommit".to_string())
    } else if proj.hooks.contains_key(DEFAULT_DOCTOR_HOOK) {
        Some(DEFAULT_DOCTOR_HOOK.to_string())
    } else {
        None
    };
    ProjectDoctorHooksInfo {
        configured,
        doctor_hook: doctor_hook.to_string(),
        doctor_hook_configured,
        recommended_next,
    }
}

fn agent_capabilities_from_view(view: &ShellClientView) -> ProjectDoctorAgentCapabilities {
    ProjectDoctorAgentCapabilities {
        shell: view.capabilities.shell,
        file_read: view.capabilities.file_read,
        file_write: view.capabilities.file_write,
        git: view.capabilities.git,
        jobs: view.capabilities.jobs,
    }
}

fn agent_info_from_view(view: &ShellClientView) -> ProjectDoctorAgentInfo {
    ProjectDoctorAgentInfo {
        client_id: view.client_id.clone(),
        connected: view.connected,
        owner: view.owner.clone(),
        hostname: view.hostname.clone(),
        capabilities: agent_capabilities_from_view(view),
    }
}

fn missing_agent_info(client_id: String) -> ProjectDoctorAgentInfo {
    ProjectDoctorAgentInfo {
        client_id,
        connected: false,
        owner: None,
        hostname: None,
        capabilities: ProjectDoctorAgentCapabilities::default(),
    }
}

async fn inspect_agent(
    depot: &Depot,
    proj: &ProjectConfig,
    warnings: &mut Vec<String>,
) -> AgentInspection {
    let client_id = match proj.agent_client_id() {
        Ok(client_id) => client_id.to_string(),
        Err(e) => {
            warnings.push(e.clone());
            return AgentInspection {
                blocker: Some(e),
                ..AgentInspection::default()
            };
        }
    };
    let registry = match depot.obtain::<Arc<ShellClientRegistry>>() {
        Ok(registry) => registry.clone(),
        Err(_) => {
            let error = "Shell client registry not configured".to_string();
            warnings.push(error.clone());
            return AgentInspection {
                info: Some(missing_agent_info(client_id.clone())),
                client_id: Some(client_id),
                blocker: Some(error),
                ..AgentInspection::default()
            };
        }
    };
    let clients = registry.list_clients().await;
    let Some(view) = clients
        .into_iter()
        .find(|client| client.client_id == client_id)
    else {
        let error = format!("agent client {} not found", client_id);
        warnings.push(error.clone());
        return AgentInspection {
            info: Some(missing_agent_info(client_id.clone())),
            registry: Some(registry),
            client_id: Some(client_id),
            blocker: Some(error),
            ..AgentInspection::default()
        };
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok();
    if let Err(e) = assert_shell_client_owner(auth, &client_id, view.owner.as_deref()) {
        warnings.push(e.clone());
        return AgentInspection {
            info: Some(agent_info_from_view(&view)),
            registry: Some(registry),
            client_id: Some(client_id),
            blocker: Some(e.clone()),
            security_error: Some(e),
            ..AgentInspection::default()
        };
    }
    let info = agent_info_from_view(&view);
    if !view.connected {
        let error = format!("agent client {} is not connected", client_id);
        warnings.push(error.clone());
        return AgentInspection {
            info: Some(info),
            registry: Some(registry),
            client_id: Some(client_id),
            can_list_jobs: true,
            blocker: Some(error),
            ..AgentInspection::default()
        };
    }
    AgentInspection {
        info: Some(info),
        registry: Some(registry),
        client_id: Some(client_id),
        can_execute: true,
        can_list_jobs: true,
        blocker: None,
        security_error: None,
    }
}

fn trim_trailing_newlines(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_string()
}

fn trim_optional(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn command_failure(command: &str, stdout: &str, stderr: &str) -> String {
    let detail = stderr.trim();
    if !detail.is_empty() {
        return format!("{} failed: {}", command, detail);
    }
    let detail = stdout.trim();
    if !detail.is_empty() {
        return format!("{} failed: {}", command, detail);
    }
    format!("{} failed", command)
}

fn parse_git_head_output(output: &str) -> (Option<String>, Option<String>) {
    let output = trim_trailing_newlines(output);
    let mut parts = output.splitn(2, '\0');
    let head = parts.next().and_then(trim_optional);
    let subject = parts.next().and_then(trim_optional);
    (head, subject)
}

async fn run_doctor_command(
    depot: &Depot,
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    command: &str,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    if proj.is_agent() {
        super::agent_exec::run_agent_project_command(
            depot,
            proj,
            command,
            timeout_secs,
            "codex_project_doctor_agent_executor",
            "agent project doctor command",
        )
        .await
    } else {
        run_project_cmd(proj, command, timeout_secs, projects.ssh.as_ref())
    }
}

async fn collect_git_info(
    depot: &Depot,
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    timeout_secs: u64,
) -> (ProjectDoctorGitInfo, Vec<String>) {
    let mut warnings = Vec::new();
    let (status_code, status_stdout, status_stderr, _) =
        run_doctor_command(depot, projects, proj, "git status --short", timeout_secs).await;
    if status_code != 0 {
        let error = command_failure("git status --short", &status_stdout, &status_stderr);
        warnings.push(error.clone());
        return (unavailable_git(error), warnings);
    }
    let status_short = trim_trailing_newlines(&status_stdout);
    let dirty = !status_short.is_empty();

    let (branch_code, branch_stdout, branch_stderr, _) = run_doctor_command(
        depot,
        projects,
        proj,
        "git rev-parse --abbrev-ref HEAD",
        timeout_secs,
    )
    .await;
    let branch = if branch_code == 0 {
        trim_optional(&branch_stdout)
    } else {
        let error = command_failure(
            "git rev-parse --abbrev-ref HEAD",
            &branch_stdout,
            &branch_stderr,
        );
        warnings.push(error);
        None
    };

    let (head_code, head_stdout, head_stderr, _) = run_doctor_command(
        depot,
        projects,
        proj,
        "git log -1 --pretty=format:%h%x00%s",
        timeout_secs,
    )
    .await;
    let (head, head_subject) = if head_code == 0 {
        parse_git_head_output(&head_stdout)
    } else {
        let error = command_failure(
            "git log -1 --pretty=format:%h%x00%s",
            &head_stdout,
            &head_stderr,
        );
        warnings.push(error);
        (None, None)
    };
    let error = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    };
    (
        ProjectDoctorGitInfo {
            available: true,
            branch,
            head,
            head_subject,
            status_short: Some(status_short),
            dirty: Some(dirty),
            error,
        },
        warnings,
    )
}

fn preview_command(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    const MAX_PREVIEW: usize = 120;
    if first_line.chars().count() <= MAX_PREVIEW {
        first_line.to_string()
    } else {
        let preview = first_line.chars().take(MAX_PREVIEW).collect::<String>();
        format!("{}...", preview)
    }
}

fn recent_job_from_job_info(job: JobInfo) -> ProjectDoctorRecentJob {
    ProjectDoctorRecentJob {
        job_id: job.job_id,
        status: job.status,
        created_at: job.created_at,
        command_preview: preview_command(&job.command),
        exit_code: job.exit_code,
        error: None,
        executor: Some(job.executor),
        client_id: None,
        project: Some(job.project),
        goal_id: Some(job.goal_id),
        client_request_id: job.client_request_id,
    }
}

fn recent_job_from_agent_info(info: ShellJobInfo, project: &str) -> Option<ProjectDoctorRecentJob> {
    let codex = info.codex?;
    let crate::shell_protocol::ShellJobCodexMetadata {
        project: job_project,
        goal_id,
        client_request_id,
        command,
        ..
    } = codex;
    let job_project = job_project?;
    if job_project != project {
        return None;
    }
    let command_preview = command
        .as_deref()
        .map(preview_command)
        .unwrap_or(info.command_preview);
    Some(ProjectDoctorRecentJob {
        job_id: info.job_id,
        status: info.status,
        created_at: info.created_at,
        command_preview,
        exit_code: info.exit_code,
        error: info.error,
        executor: Some("agent".to_string()),
        client_id: Some(info.client_id),
        project: Some(job_project),
        goal_id,
        client_request_id,
    })
}

async fn collect_recent_jobs(
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    project: &str,
    limit: usize,
    agent: &AgentInspection,
    ssh_enabled: bool,
) -> Vec<ProjectDoctorRecentJob> {
    if limit == 0 {
        return Vec::new();
    }
    let mut jobs = if proj.is_agent() {
        let (Some(registry), Some(client_id)) = (&agent.registry, &agent.client_id) else {
            return Vec::new();
        };
        if !agent.can_list_jobs {
            return Vec::new();
        }
        registry
            .list_jobs(Some(limit.max(100).clamp(1, 100)))
            .await
            .into_iter()
            .filter(|job| job.client_id == *client_id)
            .filter_map(|job| recent_job_from_agent_info(job, project))
            .collect::<Vec<_>>()
    } else if proj.is_ssh() {
        if !ssh_enabled {
            Vec::new()
        } else {
            list_ssh_jobs(
                proj,
                limit.max(100).clamp(1, 100),
                None,
                projects.ssh.as_ref(),
            )
            .into_iter()
            .filter(|job| job.project == project)
            .map(recent_job_from_job_info)
            .collect::<Vec<_>>()
        }
    } else {
        list_local_jobs(&proj.root(), limit.max(100).clamp(1, 100), None)
            .into_iter()
            .filter(|job| job.project == project)
            .map(recent_job_from_job_info)
            .collect::<Vec<_>>()
    };
    jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    jobs.truncate(limit);
    jobs
}

pub(super) async fn run_project_doctor_for_project(
    depot: &Depot,
    projects: &ProjectsConfig,
    project: &str,
    proj: &ProjectConfig,
    body: ProjectDoctorRequest,
) -> ProjectDoctorResponse {
    let hook_name = body.effective_hook_name();
    let timeout_secs = body.effective_timeout_secs();
    let recent_jobs_limit = body.effective_recent_jobs();
    let ssh_enabled = is_ssh_enabled(depot);
    let hooks = build_hooks_info(proj, &hook_name);
    let mut warnings = Vec::new();
    let mut error = None;
    let mut success = true;

    let agent = if proj.is_agent() {
        inspect_agent(depot, proj, &mut warnings).await
    } else {
        AgentInspection::default()
    };
    if let Some(security_error) = agent.security_error.clone() {
        success = false;
        error = Some(security_error);
    }

    let mut can_execute = true;
    let mut execution_blocker = None;
    if proj.is_ssh() && !ssh_enabled {
        let warning = SSH_DISABLED_MESSAGE.to_string();
        warnings.push(warning.clone());
        can_execute = false;
        execution_blocker = Some(warning);
    }
    if proj.is_agent() {
        can_execute = agent.can_execute;
        execution_blocker = agent.blocker.clone();
    }

    let git = if can_execute {
        let (git, git_warnings) = collect_git_info(depot, projects, proj, timeout_secs).await;
        warnings.extend(git_warnings);
        git
    } else {
        unavailable_git(
            execution_blocker
                .clone()
                .unwrap_or_else(|| "project executor is not available".to_string()),
        )
    };

    let recent_jobs = collect_recent_jobs(
        projects,
        proj,
        project,
        recent_jobs_limit,
        &agent,
        ssh_enabled,
    )
    .await;

    let mut hook_result = None;
    if body.run_hook {
        match proj.hooks.get(&hook_name) {
            None => warnings.push(format!("project hook '{}' is not configured", hook_name)),
            Some(commands) if commands.is_empty() => {
                warnings.push(format!("project hook '{}' has no commands", hook_name))
            }
            Some(_) if !can_execute => {
                let reason = execution_blocker
                    .clone()
                    .unwrap_or_else(|| "project executor is not available".to_string());
                warnings.push(format!(
                    "project hook '{}' was not run: {}",
                    hook_name, reason
                ));
            }
            Some(commands) => {
                let result = run_project_hook(
                    depot,
                    projects,
                    project,
                    proj,
                    &hook_name,
                    commands,
                    Some(timeout_secs),
                )
                .await;
                if !result.success {
                    success = false;
                    error = result
                        .error
                        .clone()
                        .or_else(|| Some(format!("project hook '{}' failed", hook_name)));
                }
                hook_result = Some(result);
            }
        }
    }

    ProjectDoctorResponse {
        success,
        project: project.to_string(),
        executor: executor_name(proj).to_string(),
        root: proj.path.clone(),
        ssh_enabled,
        agent: agent.info,
        git,
        hooks,
        hook_result,
        recent_jobs,
        warnings,
        error,
    }
}

#[handler]
pub async fn codex_project_doctor(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(doctor_error(
            String::new(),
            String::new(),
            "Projects not configured".to_string(),
        )));
        return;
    };
    let body: ProjectDoctorRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(doctor_error(
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
            res.render(Json(doctor_error(body.project, body.hook, e)));
            return;
        }
    };
    let project_name = body.project.clone();
    let response =
        run_project_doctor_for_project(depot, &projects, &project_name, proj, body).await;
    res.render(Json(response));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Executor, ProjectsConfig};
    use std::collections::HashMap;
    use std::process::Command;

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

    fn ssh_project(path: &str, hooks: HashMap<String, Vec<String>>) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Ssh,
            host: Some("example.invalid".to_string()),
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

    fn agent_project(path: &str, client_id: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Agent,
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            client_id: Some(client_id.to_string()),
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
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

    fn doctor_request(run_hook: bool, hook: &str, recent_jobs: usize) -> ProjectDoctorRequest {
        ProjectDoctorRequest {
            project: "demo".to_string(),
            run_hook,
            hook: hook.to_string(),
            recent_jobs,
            timeout_secs: 5,
        }
    }

    fn warning_contains(response: &ProjectDoctorResponse, needle: &str) -> bool {
        response
            .warnings
            .iter()
            .any(|warning| warning.contains(needle))
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn run_git(dir: &std::path::Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("failed to run git {:?}: {}", args, e));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn project_doctor_request_defaults_and_clamps() {
        let request: ProjectDoctorRequest = serde_json::from_str(r#"{"project":"demo"}"#).unwrap();
        assert!(request.run_hook);
        assert_eq!(request.hook, DEFAULT_DOCTOR_HOOK);
        assert_eq!(request.recent_jobs, 10);
        assert_eq!(request.timeout_secs, DEFAULT_DOCTOR_TIMEOUT_SECS);
        assert_eq!(request.effective_recent_jobs(), 10);

        let request: ProjectDoctorRequest =
            serde_json::from_str(r#"{"project":"demo","recent_jobs":80}"#).unwrap();
        assert_eq!(request.effective_recent_jobs(), MAX_DOCTOR_RECENT_JOBS);

        let request: ProjectDoctorRequest =
            serde_json::from_str(r#"{"project":"demo","recent_jobs":0}"#).unwrap();
        assert_eq!(request.effective_recent_jobs(), 0);
    }

    #[test]
    fn project_doctor_hooks_info_sorts_and_recommends_precommit() {
        let mut hooks = HashMap::new();
        hooks.insert("zeta".to_string(), vec!["true".to_string()]);
        hooks.insert("precommit".to_string(), vec!["true".to_string()]);
        hooks.insert("doctor".to_string(), vec!["true".to_string()]);
        let project = local_project("/tmp/private-drop-hooks", hooks);

        let info = build_hooks_info(&project, "doctor");
        assert_eq!(info.configured, vec!["doctor", "precommit", "zeta"]);
        assert_eq!(info.recommended_next.as_deref(), Some("precommit"));
        assert!(info.doctor_hook_configured);

        let info = build_hooks_info(&project, "lint");
        assert_eq!(info.doctor_hook, "lint");
        assert!(!info.doctor_hook_configured);
    }

    #[tokio::test]
    async fn local_git_doctor_collects_branch_head_status_and_dirty() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        run_git(tmp.path(), &["init"]);
        run_git(tmp.path(), &["config", "user.email", "doctor@example.test"]);
        run_git(tmp.path(), &["config", "user.name", "Doctor Test"]);
        std::fs::write(tmp.path().join("file.txt"), "hello\n").unwrap();
        run_git(tmp.path(), &["add", "file.txt"]);
        run_git(tmp.path(), &["commit", "-m", "initial commit"]);

        let project = local_project(tmp.path().to_str().unwrap(), HashMap::new());
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(false, "doctor", 0),
        )
        .await;

        assert!(response.git.available);
        assert!(response.git.branch.is_some());
        assert!(response
            .git
            .head
            .as_deref()
            .is_some_and(|head| !head.is_empty()));
        assert_eq!(response.git.head_subject.as_deref(), Some("initial commit"));
        assert_eq!(response.git.status_short.as_deref(), Some(""));
        assert_eq!(response.git.dirty, Some(false));

        std::fs::write(tmp.path().join("file.txt"), "hello again\n").unwrap();
        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(false, "doctor", 0),
        )
        .await;

        assert_eq!(response.git.dirty, Some(true));
        assert!(response
            .git
            .status_short
            .as_deref()
            .is_some_and(|status| status.contains("file.txt")));
    }

    #[tokio::test]
    async fn missing_doctor_hook_warns_without_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let project = local_project(tmp.path().to_str().unwrap(), HashMap::new());
        let projects = projects_with(project.clone());
        let depot = Depot::new();

        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(true, "doctor", 0),
        )
        .await;

        assert!(response.success);
        assert!(response.hook_result.is_none());
        assert!(warning_contains(
            &response,
            "project hook 'doctor' is not configured"
        ));
    }

    #[tokio::test]
    async fn hook_failure_keeps_doctor_context() {
        let tmp = tempfile::tempdir().unwrap();
        let mut hooks = HashMap::new();
        hooks.insert(
            "doctor".to_string(),
            vec!["false".to_string(), "printf should-not-run".to_string()],
        );
        let project = local_project(tmp.path().to_str().unwrap(), hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();

        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(true, "doctor", 0),
        )
        .await;

        assert!(!response.success);
        assert_eq!(response.hooks.doctor_hook, "doctor");
        assert_eq!(response.hooks.configured, vec!["doctor"]);
        assert!(response.git.error.is_some() || response.git.available);
        let hook_result = response.hook_result.unwrap();
        assert!(!hook_result.success);
        assert_eq!(hook_result.steps.len(), 1);
        assert_eq!(hook_result.steps[0].command, "false");
    }

    #[tokio::test]
    async fn ssh_disabled_warns_and_does_not_run_hook() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "doctor".to_string(),
            vec!["printf should-not-run".to_string()],
        );
        let project = ssh_project("/tmp/private-drop-ssh", hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();

        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(true, "doctor", 0),
        )
        .await;

        assert!(warning_contains(&response, SSH_DISABLED_MESSAGE));
        assert!(response.hook_result.is_none());
        assert!(!response.git.available);
    }

    #[tokio::test]
    async fn agent_offline_warns_without_panic() {
        let registry = Arc::new(crate::ShellClientRegistry::default());
        let mut depot = Depot::new();
        depot.inject(registry);
        let project = agent_project("/tmp/private-drop-agent", "oe");
        let projects = projects_with(project.clone());

        let response = run_project_doctor_for_project(
            &depot,
            &projects,
            "demo",
            &project,
            doctor_request(false, "doctor", 0),
        )
        .await;

        assert!(response
            .agent
            .as_ref()
            .is_some_and(|agent| { agent.client_id == "oe" && !agent.connected }));
        assert!(warning_contains(&response, "agent client oe not found"));
    }
}
