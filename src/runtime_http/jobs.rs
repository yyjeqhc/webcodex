use super::{render_result, runtime};
use crate::action_audit::ActionAudit;
use crate::json_error;
use crate::tool_runtime::ToolCall;
use salvo::prelude::*;
use serde::Deserialize;

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
struct RunShellRequest {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// `POST /api/projects/run_job` - thin REST wrapper over
/// `ToolCall::RunJob`. Starts an async background shell job in an
/// agent-registered project and returns a `job_id`. Execution with side
/// effects; requires Bearer auth and the agent async shell job capability.
/// Dedicated GPT Action (`startProjectShellJob`); also reachable via
/// callRuntimeTool / MCP tools/call. Poll with `getRuntimeJobStatus` and read
/// output with `getRuntimeJobTail` / `getRuntimeJobLog`.
#[derive(Debug, Deserialize)]
struct StartProjectShellJobRequest {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub cwd: Option<String>,
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
                session_id: None,
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

/// `POST /api/projects/run_job` handler. Thin wrapper: parse request, auth,
/// audit, and dispatch to `ToolRuntime` via `ToolCall::RunJob`. All business
/// logic (capability checks, owner boundary, job creation) stays in
/// `ToolRuntime`.
#[handler]
pub async fn projects_run_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/run_job", "startProjectShellJob");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: StartProjectShellJobRequest = match req.parse_json().await {
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
            ToolCall::RunJob {
                project: body.project,
                command: body.command,
                session_id: body.session_id,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_job", project, result);
}

/// `POST /api/projects/run_shell` - thin GPT Actions wrapper over
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
                session_id: body.session_id,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_shell", project, result);
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
