use super::doctor::{executor_name, run_project_doctor_for_project};
use super::hooks::run_project_hook;
use super::types::{
    ProjectDoctorRequest, ProjectHookResponse, ProjectWorkflowGitSnapshot, ProjectWorkflowRequest,
    ProjectWorkflowResponse,
};
use super::{get_projects, is_ssh_enabled, run_project_cmd, SSH_DISABLED_MESSAGE};
use crate::projects::{ProjectConfig, ProjectsConfig};
use salvo::prelude::*;

const DEFAULT_WORKFLOW_MODE: &str = "snapshot";
const DEFAULT_WORKFLOW_HOOK: &str = "doctor";
const DEFAULT_PRECOMMIT_HOOK: &str = "precommit";
const MAX_WORKFLOW_RECENT_JOBS: usize = 50;
const MAX_WORKFLOW_TIMEOUT_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectWorkflowMode {
    Snapshot,
    Doctor,
    Hook,
    Precommit,
}

impl ProjectWorkflowMode {
    fn as_str(self) -> &'static str {
        match self {
            ProjectWorkflowMode::Snapshot => "snapshot",
            ProjectWorkflowMode::Doctor => "doctor",
            ProjectWorkflowMode::Hook => "hook",
            ProjectWorkflowMode::Precommit => "precommit",
        }
    }
}

impl ProjectWorkflowRequest {
    fn effective_mode(&self) -> Result<ProjectWorkflowMode, String> {
        let mode = self.mode.trim();
        match mode {
            "" | DEFAULT_WORKFLOW_MODE => Ok(ProjectWorkflowMode::Snapshot),
            "doctor" => Ok(ProjectWorkflowMode::Doctor),
            "hook" => Ok(ProjectWorkflowMode::Hook),
            DEFAULT_PRECOMMIT_HOOK => Ok(ProjectWorkflowMode::Precommit),
            other => Err(format!(
                "invalid workflow mode '{}'; expected snapshot, doctor, hook, or precommit",
                other
            )),
        }
    }

    fn effective_recent_jobs(&self) -> usize {
        self.recent_jobs.min(MAX_WORKFLOW_RECENT_JOBS)
    }

    fn effective_timeout_secs(&self) -> u64 {
        self.timeout_secs.clamp(1, MAX_WORKFLOW_TIMEOUT_SECS)
    }

    fn effective_doctor_hook(&self) -> String {
        let hook = self.doctor_hook.trim();
        if hook.is_empty() {
            DEFAULT_WORKFLOW_HOOK.to_string()
        } else {
            hook.to_string()
        }
    }

    fn effective_workflow_hook(&self, mode: ProjectWorkflowMode) -> Result<Option<String>, String> {
        let default_hook = match mode {
            ProjectWorkflowMode::Hook => Some(DEFAULT_WORKFLOW_HOOK),
            ProjectWorkflowMode::Precommit => Some(DEFAULT_PRECOMMIT_HOOK),
            ProjectWorkflowMode::Snapshot | ProjectWorkflowMode::Doctor => None,
        };
        let Some(default_hook) = default_hook else {
            return Ok(None);
        };
        let hook = self.hook.as_deref().map(str::trim);
        match hook {
            Some("") => Err("hook cannot be empty".to_string()),
            Some(hook) => Ok(Some(hook.to_string())),
            None => Ok(Some(default_hook.to_string())),
        }
    }
}

fn unavailable_git_snapshot(error: String) -> ProjectWorkflowGitSnapshot {
    ProjectWorkflowGitSnapshot {
        available: false,
        branch: None,
        head: None,
        head_subject: None,
        status_short: None,
        dirty: None,
        diff_stat: None,
        changed_files: Vec::new(),
        error: Some(error),
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

fn parse_git_head_output(output: &str) -> (Option<String>, Option<String>) {
    let output = trim_trailing_newlines(output);
    let mut parts = output.splitn(2, '\0');
    let head = parts.next().and_then(trim_optional);
    let subject = parts.next().and_then(trim_optional);
    (head, subject)
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

fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if !warnings.iter().any(|existing| existing == &warning) {
        warnings.push(warning);
    }
}

async fn run_workflow_command(
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
            "codex_project_workflow_agent_executor",
            "agent project workflow command",
        )
        .await;
    }
    run_project_cmd(proj, command, timeout_secs, projects.ssh.as_ref())
}

async fn collect_git_snapshot(
    depot: &Depot,
    projects: &ProjectsConfig,
    proj: &ProjectConfig,
    timeout_secs: u64,
    ssh_enabled: bool,
) -> (ProjectWorkflowGitSnapshot, Vec<String>) {
    let mut warnings = Vec::new();
    if proj.is_ssh() && !ssh_enabled {
        let warning = SSH_DISABLED_MESSAGE.to_string();
        warnings.push(warning.clone());
        return (unavailable_git_snapshot(warning), warnings);
    }

    let (status_code, status_stdout, status_stderr, _) =
        run_workflow_command(depot, projects, proj, "git status --short", timeout_secs).await;
    if status_code != 0 {
        let error = command_failure("git status --short", &status_stdout, &status_stderr);
        warnings.push(error.clone());
        return (unavailable_git_snapshot(error), warnings);
    }

    let status_short = trim_trailing_newlines(&status_stdout);
    let dirty = !status_short.is_empty();

    let (branch_code, branch_stdout, branch_stderr, _) = run_workflow_command(
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
        warnings.push(command_failure(
            "git rev-parse --abbrev-ref HEAD",
            &branch_stdout,
            &branch_stderr,
        ));
        None
    };

    let (head_code, head_stdout, head_stderr, _) = run_workflow_command(
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
        warnings.push(command_failure(
            "git log -1 --pretty=format:%h%x00%s",
            &head_stdout,
            &head_stderr,
        ));
        (None, None)
    };

    let (diff_code, diff_stdout, diff_stderr, _) =
        run_workflow_command(depot, projects, proj, "git diff --stat", timeout_secs).await;
    let diff_stat = if diff_code == 0 {
        Some(trim_trailing_newlines(&diff_stdout))
    } else {
        warnings.push(command_failure(
            "git diff --stat",
            &diff_stdout,
            &diff_stderr,
        ));
        None
    };

    let (changed_code, changed_stdout, changed_stderr, _) = run_workflow_command(
        depot,
        projects,
        proj,
        "git diff --name-status",
        timeout_secs,
    )
    .await;
    let changed_files = if changed_code == 0 {
        trim_trailing_newlines(&changed_stdout)
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect()
    } else {
        warnings.push(command_failure(
            "git diff --name-status",
            &changed_stdout,
            &changed_stderr,
        ));
        Vec::new()
    };

    let error = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    };
    (
        ProjectWorkflowGitSnapshot {
            available: true,
            branch,
            head,
            head_subject,
            status_short: Some(status_short),
            dirty: Some(dirty),
            diff_stat,
            changed_files,
            error,
        },
        warnings,
    )
}

fn workflow_error(
    project: String,
    mode: String,
    executor: String,
    root: String,
    ssh_enabled: bool,
    error: String,
) -> ProjectWorkflowResponse {
    ProjectWorkflowResponse {
        success: false,
        project,
        mode,
        executor,
        root,
        ssh_enabled,
        git_before: unavailable_git_snapshot(error.clone()),
        doctor: None,
        hook_result: None,
        git_after: unavailable_git_snapshot(error.clone()),
        warnings: vec![error.clone()],
        recommended_next_action: "Review workflow warnings".to_string(),
        error: Some(error),
    }
}

fn recommended_next_action(
    git_after: &ProjectWorkflowGitSnapshot,
    hook_result: Option<&ProjectHookResponse>,
    warnings: &[String],
    proj: &ProjectConfig,
) -> String {
    if hook_result.is_some_and(|result| !result.success) {
        return "Fix failing hook step".to_string();
    }
    if warnings
        .iter()
        .any(|warning| warning.contains(SSH_DISABLED_MESSAGE))
    {
        return "Use agent executor or enable SSH explicitly".to_string();
    }
    if warnings.iter().any(|warning| {
        warning.contains("agent client")
            && (warning.contains("not found") || warning.contains("not connected"))
    }) {
        return "Check agent connection".to_string();
    }
    if git_after.dirty == Some(true) && proj.hooks.contains_key(DEFAULT_PRECOMMIT_HOOK) {
        return "Run precommit hook before committing".to_string();
    }
    if git_after.dirty == Some(true) {
        return "Review git diff before committing".to_string();
    }
    if git_after.dirty == Some(false) {
        return "No changes detected".to_string();
    }
    "Review workflow warnings".to_string()
}

async fn run_workflow_hook(
    depot: &Depot,
    projects: &ProjectsConfig,
    project: &str,
    proj: &ProjectConfig,
    hook_name: &str,
    timeout_secs: u64,
) -> Result<ProjectHookResponse, String> {
    let commands = proj
        .hooks
        .get(hook_name)
        .ok_or_else(|| format!("project hook '{}' is not configured", hook_name))?;
    if commands.is_empty() {
        return Err(format!("project hook '{}' has no commands", hook_name));
    }
    Ok(run_project_hook(
        depot,
        projects,
        project,
        proj,
        hook_name,
        commands,
        Some(timeout_secs),
    )
    .await)
}

pub(super) async fn run_project_workflow_for_project(
    depot: &Depot,
    projects: &ProjectsConfig,
    project: &str,
    proj: &ProjectConfig,
    body: ProjectWorkflowRequest,
) -> ProjectWorkflowResponse {
    let mode = match body.effective_mode() {
        Ok(mode) => mode,
        Err(e) => {
            return workflow_error(
                project.to_string(),
                body.mode,
                executor_name(proj).to_string(),
                proj.path.clone(),
                is_ssh_enabled(depot),
                e,
            );
        }
    };
    let mode_name = mode.as_str().to_string();
    let timeout_secs = body.effective_timeout_secs();
    let ssh_enabled = is_ssh_enabled(depot);
    let mut warnings = Vec::new();
    let mut success = true;
    let mut error = None;

    let workflow_hook = match body.effective_workflow_hook(mode) {
        Ok(hook) => hook,
        Err(e) => {
            success = false;
            error = Some(e.clone());
            push_warning(&mut warnings, e);
            None
        }
    };

    let (git_before, git_before_warnings) =
        collect_git_snapshot(depot, projects, proj, timeout_secs, ssh_enabled).await;
    for warning in git_before_warnings {
        push_warning(&mut warnings, warning);
    }

    let should_run_doctor = mode == ProjectWorkflowMode::Doctor || body.run_doctor;
    let mut doctor = None;
    if should_run_doctor {
        let doctor_request = ProjectDoctorRequest {
            project: project.to_string(),
            run_hook: mode == ProjectWorkflowMode::Doctor && body.run_doctor_hook,
            hook: body.effective_doctor_hook(),
            recent_jobs: body.effective_recent_jobs(),
            timeout_secs,
        };
        let doctor_response =
            run_project_doctor_for_project(depot, projects, project, proj, doctor_request).await;
        for warning in &doctor_response.warnings {
            push_warning(&mut warnings, warning.clone());
        }
        if !doctor_response.success {
            success = false;
            error = doctor_response
                .error
                .clone()
                .or_else(|| Some("project doctor failed".to_string()));
        }
        doctor = Some(doctor_response);
    }

    let mut hook_result = None;
    if let Some(hook_name) = workflow_hook.as_deref() {
        if proj.is_ssh() && !ssh_enabled {
            success = false;
            let warning = SSH_DISABLED_MESSAGE.to_string();
            push_warning(&mut warnings, warning.clone());
            error = Some(warning);
        } else if error.is_none() || mode != ProjectWorkflowMode::Doctor {
            match run_workflow_hook(depot, projects, project, proj, hook_name, timeout_secs).await {
                Ok(result) => {
                    if !result.success {
                        success = false;
                        error = result
                            .error
                            .clone()
                            .or_else(|| Some(format!("project hook '{}' failed", hook_name)));
                    }
                    hook_result = Some(result);
                }
                Err(e) => {
                    success = false;
                    error = Some(e.clone());
                    push_warning(&mut warnings, e);
                }
            }
        }
    }

    let (git_after, git_after_warnings) =
        collect_git_snapshot(depot, projects, proj, timeout_secs, ssh_enabled).await;
    for warning in git_after_warnings {
        push_warning(&mut warnings, warning);
    }

    if matches!(
        mode,
        ProjectWorkflowMode::Snapshot | ProjectWorkflowMode::Doctor
    ) && error.is_some()
        && hook_result.is_none()
        && mode == ProjectWorkflowMode::Snapshot
    {
        success = true;
    }

    let recommended_next_action =
        recommended_next_action(&git_after, hook_result.as_ref(), &warnings, proj);

    ProjectWorkflowResponse {
        success,
        project: project.to_string(),
        mode: mode_name,
        executor: executor_name(proj).to_string(),
        root: proj.path.clone(),
        ssh_enabled,
        git_before,
        doctor,
        hook_result,
        git_after,
        warnings,
        recommended_next_action,
        error,
    }
}

#[handler]
pub async fn codex_project_workflow(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(workflow_error(
            String::new(),
            DEFAULT_WORKFLOW_MODE.to_string(),
            "local".to_string(),
            String::new(),
            false,
            "Projects not configured".to_string(),
        )));
        return;
    };
    let body: ProjectWorkflowRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(workflow_error(
                String::new(),
                DEFAULT_WORKFLOW_MODE.to_string(),
                "local".to_string(),
                String::new(),
                is_ssh_enabled(depot),
                format!("Invalid JSON: {}", e),
            )));
            return;
        }
    };
    let mode_for_error = body.mode.clone();
    let project_name = body.project.clone();
    let proj = match projects.get_project(&project_name) {
        Ok(proj) => proj,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(workflow_error(
                project_name,
                mode_for_error,
                "local".to_string(),
                String::new(),
                is_ssh_enabled(depot),
                e,
            )));
            return;
        }
    };
    if body.effective_mode().is_err() {
        res.status_code(StatusCode::BAD_REQUEST);
    }
    let response =
        run_project_workflow_for_project(depot, &projects, &project_name, proj, body).await;
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
        let mut project = local_project(path, hooks);
        project.executor = Executor::Ssh;
        project.host = Some("example.invalid".to_string());
        project
    }

    fn projects_with(project: ProjectConfig) -> ProjectsConfig {
        let mut projects = HashMap::new();
        projects.insert("demo".to_string(), project);
        ProjectsConfig {
            ssh: None,
            projects,
        }
    }

    fn workflow_request(mode: &str) -> ProjectWorkflowRequest {
        ProjectWorkflowRequest {
            project: "demo".to_string(),
            mode: mode.to_string(),
            hook: None,
            run_doctor: false,
            doctor_hook: "doctor".to_string(),
            run_doctor_hook: false,
            recent_jobs: 0,
            timeout_secs: 5,
        }
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
    fn project_workflow_request_defaults_and_clamps() {
        let request: ProjectWorkflowRequest =
            serde_json::from_str(r#"{"project":"demo"}"#).unwrap();
        assert_eq!(
            request.effective_mode().unwrap(),
            ProjectWorkflowMode::Snapshot
        );
        assert!(request.run_doctor);
        assert!(!request.run_doctor_hook);
        assert_eq!(request.recent_jobs, 10);
        assert_eq!(request.effective_recent_jobs(), 10);
        assert_eq!(request.timeout_secs, 120);
        assert_eq!(request.effective_timeout_secs(), 120);

        let request: ProjectWorkflowRequest =
            serde_json::from_str(r#"{"project":"demo","recent_jobs":80}"#).unwrap();
        assert_eq!(request.effective_recent_jobs(), 50);
    }

    #[test]
    fn project_workflow_mode_validation() {
        for mode in ["snapshot", "doctor", "hook", "precommit"] {
            let request = workflow_request(mode);
            assert!(request.effective_mode().is_ok(), "mode {mode} should parse");
        }
        let request = workflow_request("deploy");
        let err = request.effective_mode().unwrap_err();
        assert!(err.contains("invalid workflow mode 'deploy'"));
    }

    #[test]
    fn project_workflow_recommendations_are_stable() {
        let project = local_project("/tmp/demo", HashMap::new());
        let clean = ProjectWorkflowGitSnapshot {
            dirty: Some(false),
            ..ProjectWorkflowGitSnapshot::default()
        };
        assert_eq!(
            recommended_next_action(&clean, None, &[], &project),
            "No changes detected"
        );

        let failed_hook = ProjectHookResponse {
            success: false,
            project: "demo".to_string(),
            hook: "doctor".to_string(),
            steps: Vec::new(),
            git_status_short: String::new(),
            error: Some("hook command failed".to_string()),
        };
        assert_eq!(
            recommended_next_action(&clean, Some(&failed_hook), &[], &project),
            "Fix failing hook step"
        );

        assert_eq!(
            recommended_next_action(&clean, None, &[SSH_DISABLED_MESSAGE.to_string()], &project),
            "Use agent executor or enable SSH explicitly"
        );
    }

    #[tokio::test]
    async fn project_workflow_missing_precommit_is_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let project = local_project(tmp.path().to_str().unwrap(), HashMap::new());
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let mut request = workflow_request("precommit");
        request.run_doctor = false;

        let response =
            run_project_workflow_for_project(&depot, &projects, "demo", &project, request).await;

        assert!(!response.success);
        assert_eq!(
            response.error.as_deref(),
            Some("project hook 'precommit' is not configured")
        );
        assert!(response.hook_result.is_none());
    }

    #[tokio::test]
    async fn project_workflow_hook_failure_keeps_git_snapshots() {
        let tmp = tempfile::tempdir().unwrap();
        let mut hooks = HashMap::new();
        hooks.insert(
            "doctor".to_string(),
            vec!["false".to_string(), "printf should-not-run".to_string()],
        );
        let project = local_project(tmp.path().to_str().unwrap(), hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let mut request = workflow_request("hook");
        request.run_doctor = false;

        let response =
            run_project_workflow_for_project(&depot, &projects, "demo", &project, request).await;

        assert!(!response.success);
        let hook_result = response.hook_result.unwrap();
        assert!(!hook_result.success);
        assert_eq!(hook_result.steps.len(), 1);
        assert_eq!(hook_result.steps[0].command, "false");
        assert!(response.git_before.error.is_some() || response.git_before.available);
        assert!(response.git_after.error.is_some() || response.git_after.available);
    }

    #[tokio::test]
    async fn project_workflow_local_git_snapshot_collects_evidence() {
        if !git_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        run_git(tmp.path(), &["init"]);
        run_git(
            tmp.path(),
            &["config", "user.email", "workflow@example.test"],
        );
        run_git(tmp.path(), &["config", "user.name", "Workflow Test"]);
        std::fs::write(tmp.path().join("file.txt"), "hello\n").unwrap();
        run_git(tmp.path(), &["add", "file.txt"]);
        run_git(tmp.path(), &["commit", "-m", "initial commit"]);
        std::fs::write(tmp.path().join("file.txt"), "hello again\n").unwrap();

        let project = local_project(tmp.path().to_str().unwrap(), HashMap::new());
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let (snapshot, warnings) =
            collect_git_snapshot(&depot, &projects, &project, 5, false).await;

        assert!(warnings.is_empty());
        assert!(snapshot.available);
        assert!(snapshot.branch.is_some());
        assert!(snapshot
            .head
            .as_deref()
            .is_some_and(|head| !head.is_empty()));
        assert_eq!(snapshot.head_subject.as_deref(), Some("initial commit"));
        assert!(snapshot
            .status_short
            .as_deref()
            .is_some_and(|status| status.contains("file.txt")));
        assert_eq!(snapshot.dirty, Some(true));
        assert!(snapshot
            .diff_stat
            .as_deref()
            .is_some_and(|stat| stat.contains("file.txt")));
        assert!(snapshot
            .changed_files
            .iter()
            .any(|file| file.contains("file.txt")));
    }

    #[tokio::test]
    async fn project_workflow_ssh_disabled_does_not_run_hook() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "precommit".to_string(),
            vec!["printf should-not-run".to_string()],
        );
        let project = ssh_project("/tmp/private-drop-ssh-workflow", hooks);
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let mut request = workflow_request("precommit");
        request.run_doctor = false;

        let response =
            run_project_workflow_for_project(&depot, &projects, "demo", &project, request).await;

        assert!(!response.success);
        assert!(response.hook_result.is_none());
        assert!(response
            .warnings
            .iter()
            .any(|warning| warning.contains(SSH_DISABLED_MESSAGE)));
        assert_eq!(
            response.recommended_next_action,
            "Use agent executor or enable SSH explicitly"
        );
    }

    #[tokio::test]
    async fn project_workflow_doctor_integration_returns_doctor() {
        let tmp = tempfile::tempdir().unwrap();
        let project = local_project(tmp.path().to_str().unwrap(), HashMap::new());
        let projects = projects_with(project.clone());
        let depot = Depot::new();
        let request = workflow_request("doctor");

        let response =
            run_project_workflow_for_project(&depot, &projects, "demo", &project, request).await;

        assert!(response.doctor.is_some());
        assert_eq!(response.mode, "doctor");
    }
}
