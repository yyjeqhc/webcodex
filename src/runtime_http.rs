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
    let result = runtime.dispatch(call).await;
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
    let result = runtime
        .dispatch(ToolCall::RunCodex {
            project: body.project,
            prompt: body.prompt,
            approval_mode: body.approval_mode,
            timeout_secs: body.timeout_secs,
            cwd: body.cwd,
            extra_args: body.extra_args,
        })
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
    let result = runtime
        .dispatch(ToolCall::JobStatus {
            job_id: body.job_id,
        })
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
    let result = runtime
        .dispatch(ToolCall::JobLog {
            job_id: body.job_id,
            offset: body.offset,
            tail_lines: body.tail_lines,
        })
        .await;
    render_result(res, &audit, "job_log", None, result);
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
    let result = runtime.dispatch(ToolCall::ListProjects).await;
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
    let result = runtime
        .dispatch(ToolCall::ReadFile {
            project: body.project,
            path: body.path,
            start_line: body.start_line,
            limit: body.limit,
        })
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
    let result = runtime
        .dispatch(ToolCall::GitStatus {
            project: body.project,
        })
        .await;
    render_result(res, &audit, "git_status", project, result);
}

fn tool_project(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::RunShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::GitStatus { project }
        | ToolCall::GitDiff { project, .. }
        | ToolCall::ReadFile { project, .. }
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
                    .push(Router::with_path("projects/git_status").post(projects_git_status)),
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
    async fn http_projects_list_happy_path_returns_configured_project() {
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
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], "demo");
        assert_eq!(list[0]["executor"], "local");
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
    async fn http_projects_read_file_happy_path_returns_content() {
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
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["output"]["content"], "line1\nline2");
        assert_eq!(body["output"]["total_lines"], 2);
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
    async fn http_projects_git_status_happy_path_runs_git_status() {
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
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        // git status --porcelain lists untracked files with "?? " prefix.
        let stdout = body["output"]["stdout"].as_str().unwrap_or("");
        assert!(stdout.contains("tracked.txt"), "stdout was: {}", stdout);
    }
}
