use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::json_error;
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct ToolCallRequest {
    pub tool: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Deserialize)]
struct CodexRunRequest {
    pub project: String,
    pub prompt: String,
    #[serde(default)]
    pub approval_mode: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct JobStatusRequest {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
struct JobLogRequest {
    pub job_id: String,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct JobStopRequest {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
struct ProjectIdRequest {
    pub project: String,
}

#[derive(Debug, Deserialize)]
struct ReadProjectFileRequest {
    pub project: String,
    pub path: String,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ProjectGitDiffRequest {
    pub project: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ApplyPatchRequest {
    pub project: String,
    pub patch: String,
}

#[derive(Debug, Deserialize)]
struct RunShellRequest {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProjectFilesRequest {
    pub project: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchProjectTextRequest {
    pub project: String,
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ListJobsRequest {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobTailRequest {
    pub job_id: String,
    #[serde(default)]
    pub tail_lines: Option<usize>,
}

fn runtime(depot: &Depot) -> Option<Arc<ToolRuntime>> {
    depot.obtain::<Arc<ToolRuntime>>().ok().cloned()
}

fn render_result(
    res: &mut Response,
    audit: &ActionAudit,
    operation: &str,
    project: Option<String>,
    result: crate::tool_runtime::ToolResult,
) {
    let status = if result.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    res.status_code(status);
    let mut event = ActionAuditRecord::new(operation.to_string(), result.success, status)
        .error(result.error.clone())
        .summary(json!({
            "output": result.output.clone(),
        }));
    event.project = project;
    audit.record(event);
    res.render(Json(result));
}

#[handler]
pub async fn tools_list(depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    res.render(Json(json!({
        "success": true,
        "tools": runtime.tool_specs(),
    })));
}

#[handler]
pub async fn tools_call(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/tools/call", "callTool");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ToolCallRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let call = match ToolCall::from_tool_name(&body.tool, body.params) {
        Ok(call) => call,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let project = tool_project(&call);
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime.dispatch_with_auth(call, auth.as_ref()).await;
    render_result(res, &audit, &body.tool, project, result);
}

#[handler]
pub async fn codex_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/codex/run", "runCodexTask");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: CodexRunRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunCodex {
                project: body.project,
                prompt: body.prompt,
                approval_mode: body.approval_mode,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
                extra_args: body.extra_args,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_codex", project, result);
}

#[handler]
pub async fn job_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/status", "jobStatus");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobStatusRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: body.job_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_status", None, result);
}

#[handler]
pub async fn job_log(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/log", "jobLog");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobLogRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobLog {
                job_id: body.job_id,
                offset: body.offset,
                tail_lines: body.tail_lines,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_log", None, result);
}

/// Stop a local runtime job by terminating its process group and marking it
/// `stopped`. This is a thin wrapper over `ToolRuntime::stop_job`; it is
/// intentionally NOT exposed as a GPT Action (absent from openapi.json) so
/// remote ChatGPT callers cannot drive an explicit kill. Only jobs the
/// runtime created and recorded can be stopped.
#[handler]
pub async fn job_stop(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/stop", "jobStop");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobStopRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let result = runtime.stop_job(body.job_id).await;
    render_result(res, &audit, "job_stop", None, result);
}

#[handler]
pub async fn projects_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/list", "listProjects");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Body is optional; reject non-empty invalid JSON for consistency but
    // tolerate an empty/missing body since this call takes no arguments.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => Value::Null,
    };
    let _ = body;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, auth.as_ref())
        .await;
    render_result(res, &audit, "list_projects", None, result);
}

#[handler]
pub async fn projects_read_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/read_file", "readProjectFile");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ReadProjectFileRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: body.project,
                path: body.path,
                start_line: body.start_line,
                limit: body.limit,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "read_file", project, result);
}

#[handler]
pub async fn projects_git_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/git_status",
        "getProjectGitStatus",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectIdRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitStatus {
                project: body.project,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_status", project, result);
}

/// `POST /api/projects/git_diff` — thin GPT Actions wrapper over
/// `ToolCall::GitDiff`. Read-only inspection routed to the owning agent.
#[handler]
pub async fn projects_git_diff(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/git_diff", "getProjectGitDiff");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectGitDiffRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitDiff {
                project: body.project,
                args: body.args,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_diff", project, result);
}

/// `POST /api/projects/apply_patch` — thin GPT Actions wrapper over
/// `ToolCall::ApplyPatch`. Executable mutation; requires the owning agent to
/// allow patching and the caller to pass Bearer auth.
#[handler]
pub async fn projects_apply_patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/apply_patch", "applyProjectPatch");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ApplyPatchRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ApplyPatch {
                project: body.project,
                patch: body.patch,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "apply_patch", project, result);
}

/// `POST /api/projects/run_shell` — thin GPT Actions wrapper over
/// `ToolCall::RunShell`. Executable with side effects; requires the owning
/// agent's shell capability and Bearer auth.
#[handler]
pub async fn projects_run_shell(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/run_shell",
        "runProjectShellCommand",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: RunShellRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: body.project,
                command: body.command,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_shell", project, result);
}

#[handler]
pub async fn runtime_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/runtime/status", "getRuntimeStatus");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Body is optional; tolerate an empty/missing body since this call takes
    // no arguments.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => Value::Null,
    };
    let _ = body;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(ToolCall::RuntimeStatus, auth.as_ref())
        .await;
    render_result(res, &audit, "runtime_status", None, result);
}

/// `ToolCall::ListProjectFiles`. Read-only, agent-backed file listing.
#[handler]
pub async fn projects_list_files(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/list_files", "listProjectFiles");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ListProjectFilesRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjectFiles {
                project: body.project,
                path: body.path,
                limit: body.limit,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "list_project_files", Some(project), result);
}

/// `ToolCall::SearchProjectText`. Read-only, agent-backed bounded text search.
#[handler]
pub async fn projects_search_text(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/search_text", "searchProjectText");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: SearchProjectTextRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectText {
                project: body.project,
                pattern: body.pattern,
                path: body.path,
                limit: body.limit,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "search_project_text", Some(project), result);
}

/// `ToolCall::GitDiffSummary`. Read-only git inspection.
#[handler]
pub async fn projects_git_diff_summary(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/git_diff_summary",
        "getProjectGitDiffSummary",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectIdRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitDiffSummary {
                project: body.project,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_diff_summary", Some(project), result);
}

/// `ToolCall::ListJobs`. Bounded job summaries (no stdout/stderr bodies).
#[handler]
pub async fn jobs_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/list", "listRuntimeJobs");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ListJobsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: body.limit,
                status: body.status,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "list_jobs", None, result);
}

/// `ToolCall::JobTail`. Bounded stdout/stderr tails for a job.
#[handler]
pub async fn job_tail(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/tail", "getRuntimeJobTail");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobTailRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobTail {
                job_id: body.job_id,
                tail_lines: body.tail_lines,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_tail", None, result);
}

fn tool_project(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::RunShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::GitStatus { project }
        | ToolCall::GitDiff { project, .. }
        | ToolCall::GitDiffSummary { project }
        | ToolCall::ReadFile { project, .. }
        | ToolCall::ListProjectFiles { project, .. }
        | ToolCall::SearchProjectText { project, .. }
        | ToolCall::RunJob { project, .. }
        | ToolCall::RunCodex { project, .. } => Some(project.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
    use crate::shell_client::ShellClientRegistry;
    use crate::CodexConfig;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
        })
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn local_project_config(path: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    /// Build a ToolRuntime backed by a single local project rooted at `root`.
    fn runtime_with_local_project(root: &std::path::Path, project_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            project_id.to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        )
    }

    /// Build a router that mirrors the production /api wiring for the new
    /// dedicated project actions: Config, Database, and ToolRuntime are
    /// injected so AuthMiddleware and the handlers resolve state exactly as
    /// in `main.rs`.
    fn build_projects_router(
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        runtime: Arc<ToolRuntime>,
    ) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("projects/list").post(projects_list))
                    .push(Router::with_path("projects/read_file").post(projects_read_file))
                    .push(Router::with_path("projects/git_status").post(projects_git_status))
                    .push(Router::with_path("projects/git_diff").post(projects_git_diff))
                    .push(Router::with_path("projects/apply_patch").post(projects_apply_patch))
                    .push(Router::with_path("projects/run_shell").post(projects_run_shell))
                    .push(Router::with_path("projects/list_files").post(projects_list_files))
                    .push(Router::with_path("projects/search_text").post(projects_search_text))
                    .push(
                        Router::with_path("projects/git_diff_summary")
                            .post(projects_git_diff_summary),
                    )
                    .push(Router::with_path("jobs/list").post(jobs_list))
                    .push(Router::with_path("jobs/tail").post(job_tail))
                    .push(Router::with_path("runtime/status").post(runtime_status)),
            )
    }

    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    // =========================================================================
    // listProjects
    // =========================================================================

    #[tokio::test]
    async fn http_projects_list_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/list")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_list_rejects_wrong_bearer() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/list")
            .bearer_auth("wrong")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_list_ignores_server_configured_projects() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let list = body["output"]
            .as_array()
            .expect("output is a project array");
        assert!(
            list.is_empty(),
            "runtime project discovery is agent-registered only"
        );
    }

    // =========================================================================
    // readProjectFile
    // =========================================================================

    #[tokio::test]
    async fn http_projects_read_file_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        std::fs::write(tmp_proj.path().join("README.md"), "hello").unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/read_file")
            .json(&json!({"project": "demo", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_read_file_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        std::fs::write(tmp_proj.path().join("README.md"), "line1\nline2\n").unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    #[tokio::test]
    async fn http_projects_read_file_rejects_unknown_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({"project": "nope", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("nope"));
    }

    // =========================================================================
    // getProjectGitStatus
    // =========================================================================

    #[tokio::test]
    async fn http_projects_git_status_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/git_status")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_git_status_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        // Initialize a real git repo so `git status --porcelain` succeeds.
        let root = tmp_proj.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init");
        std::fs::write(root.join("tracked.txt"), "a").unwrap();
        let runtime = Arc::new(runtime_with_local_project(root, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/git_status")
            .bearer_auth("secret")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // getProjectGitDiff
    // =========================================================================

    #[tokio::test]
    async fn http_projects_git_diff_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/git_diff")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_git_diff_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let root = tmp_proj.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init");
        let runtime = Arc::new(runtime_with_local_project(root, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/git_diff")
            .bearer_auth("secret")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // applyProjectPatch
    // =========================================================================

    #[tokio::test]
    async fn http_projects_apply_patch_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/apply_patch")
            .json(&json!({"project": "demo", "patch": "diff"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_apply_patch_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/apply_patch")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "patch": "diff"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // runProjectShellCommand
    // =========================================================================

    #[tokio::test]
    async fn http_projects_run_shell_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/run_shell")
            .json(&json!({"project": "demo", "command": "echo hi"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_run_shell_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/run_shell")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "command": "echo hi"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // getRuntimeStatus / /api/runtime/status
    // =========================================================================

    #[tokio::test]
    async fn http_runtime_status_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/runtime/status")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_runtime_status_rejects_wrong_bearer() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth("wrong")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_runtime_status_correct_bearer_returns_summary() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let out = &body["output"];
        assert_eq!(out["service"], "private-drop");
        assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(out["projects"]["configured"], true);
        assert_eq!(out["projects"]["count"], 1);
        assert!(out["agents"]["count"].is_i64());
        assert!(out["jobs"]["active_count"].is_i64());
        assert!(out["tools"]["count"].is_i64());
        // No secrets in the HTTP response either.
        let serialized = serde_json::to_string(&body).unwrap();
        for forbidden in ["token", "api_key", "secret", "password"] {
            assert!(
                !serialized
                    .to_lowercase()
                    .contains(&forbidden.to_lowercase()),
                "runtime_status HTTP response must not contain '{}'",
                forbidden
            );
        }
    }

    // =========================================================================
    // Phase A read-only console REST wrappers (wiring + auth gate)
    // =========================================================================

    #[tokio::test]
    async fn http_console_routes_require_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        for (path, body) in [
            ("/api/projects/list_files", json!({"project": "demo"})),
            (
                "/api/projects/search_text",
                json!({"project": "demo", "pattern": "fn"}),
            ),
            ("/api/projects/git_diff_summary", json!({"project": "demo"})),
            ("/api/jobs/list", json!({})),
            ("/api/jobs/tail", json!({"job_id": "abc"})),
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{} should require auth",
                path
            );
        }
    }

    #[tokio::test]
    async fn http_console_routes_accept_correct_bearer_and_route_to_runtime() {
        // With a correct bearer token the routes reach the runtime. The
        // project id below is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/list_files")
            .bearer_auth("secret")
            .json(&json!({"project": "agent:nope:nope"}))
            .send(&service)
            .await;
        // Authenticated and dispatched to ToolRuntime: a structured error
        // (BAD_REQUEST + success=false), not a 401/404.
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "list_files should return a structured runtime error"
        );

        // list_jobs reaches the runtime and returns a bounded summary list
        // even with no jobs present.
        let mut resp = TestClient::post("http://localhost/api/jobs/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["jobs"].is_array());
    }
}
