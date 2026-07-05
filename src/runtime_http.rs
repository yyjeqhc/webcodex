use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::json_error;
use crate::tool_runtime::kernel::{
    ToolCallContext, ToolCallErrorStatus, ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD;
use crate::tool_runtime::{
    ListToolsOptions, ToolCall, ToolRuntime, TOOL_CALL_ARGUMENTS_FIELD, TOOL_CALL_PARAMS_FIELD,
    TOOL_CALL_TOOL_FIELD, TOOL_CALL_WRAPPER_FIELDS,
};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

mod import_http;
mod jobs;
mod project_files;
mod projects;

pub use import_http::import_conversation_files_to_project;
pub use jobs::{
    codex_run, job_log, job_status, job_stop, job_tail, jobs_list, projects_run_job,
    projects_run_shell,
};
pub use project_files::{
    projects_apply_patch, projects_apply_patch_checked, projects_delete_files,
    projects_discard_untracked, projects_git_diff, projects_git_diff_summary,
    projects_git_restore_paths, projects_git_status, projects_list_files, projects_read_file,
    projects_replace_in_file, projects_search_text, projects_validate_patch, projects_write_file,
};
pub use projects::{projects_create, projects_list, projects_register};

/// Generic runtime tool call body. `tool` is required; `params` carries the
/// tool-specific arguments. `arguments` is accepted as a compatibility alias
/// for `params` — when both are present, `params` wins. GPT Actions may also
/// pass tool-specific arguments as flattened top-level fields. Top-level
/// `recording_session_id` is recorder metadata; top-level `session_id` remains
/// an ordinary flattened tool argument so tools like `session_summary` can use
/// it as business input.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ToolCallRequest {
    pub tool: String,
    #[serde(default)]
    pub params: Value,
    /// Compatibility alias for `params`. Ignored when `params` is present.
    #[serde(default)]
    pub arguments: Value,
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
pub async fn tools_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body = match req.payload().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            ));
            return;
        }
    };
    let options = if body.is_empty() || body.iter().all(|b| b.is_ascii_whitespace()) {
        ListToolsOptions::default()
    } else {
        match serde_json::from_slice::<ListToolsOptions>(&body) {
            Ok(options) => options,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid listRuntimeTools request: {}", e),
                ));
                return;
            }
        }
    };
    let mut payload = runtime.list_tools_payload(options);
    payload["success"] = json!(true);
    res.render(Json(payload));
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
    // Parse the body as a raw JSON value so we can apply the params/arguments
    // precedence rule explicitly and emit field-aware errors that include the
    // tool name. We never echo the raw body back, so tokens/headers/env never
    // leak through error messages.
    let body: Value = match req.parse_json().await {
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
    let (tool, params) = match extract_tool_call(&body) {
        Ok(pair) => pair,
        Err(msg) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, msg));
            return;
        }
    };
    let session_id = extract_recording_session_id(&body);
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let outcome = runtime
        .call_tool_with_context(
            KernelToolCallRequest {
                tool_name: tool.clone(),
                arguments: params,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: session_id.as_deref(),
                auth: auth.as_ref(),
                record_oauth_scope_denials: true,
            },
        )
        .await;
    match outcome.error_status {
        Some(ToolCallErrorStatus::InsufficientScope {
            required_scope,
            description,
        }) => {
            crate::auth::render_oauth_insufficient_scope(res, required_scope, description);
        }
        Some(ToolCallErrorStatus::InvalidArguments { message }) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, message));
        }
        None => {
            let result = outcome
                .result
                .expect("tool kernel outcome without error must include result");
            debug_assert_eq!(outcome.success, result.success);
            render_result(res, &audit, &tool, outcome.project, result);
        }
    }
}

/// Extract `(tool, params)` from a raw `callRuntimeTool` request body.
///
/// Accepted shapes (all route to the same tool dispatch):
/// - `{"tool":"list_tools"}`
/// - `{"tool":"list_tools","params":null}`
/// - `{"tool":"git_diff_summary","params":{"project":"agent:c:p"}}`
/// - `{"tool":"git_diff_summary","arguments":{"project":"agent:c:p"}}`
/// - `{"tool":"git_diff_summary","project":"agent:c:p"}`
/// - `{"tool":"git_status","project":"agent:c:p","recording_session_id":"wc_sess_..."}`
///
/// When both `params` and `arguments` are present, `params` wins; `arguments`
/// is only a compatibility alias. When neither is present, every top-level
/// field except `tool` and reserved metadata like `recording_session_id` is
/// collected into the params object for GPT Action compatibility. Top-level
/// `session_id` is not reserved here; it remains a normal flattened tool
/// argument for tools such as `session_summary`. Returns a human-readable error
/// string (never including the raw body) when the body is not a JSON object or
/// `tool` is missing/not a non-empty string.
fn extract_tool_call(body: &Value) -> Result<(String, Value), String> {
    let obj = body
        .as_object()
        .ok_or_else(|| "request body must be a JSON object".to_string())?;
    let tool = match obj.get(TOOL_CALL_TOOL_FIELD) {
        Some(v) => match v.as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return Err(format!(
                    "field '{TOOL_CALL_TOOL_FIELD}' must be a non-empty string"
                ));
            }
        },
        None => {
            return Err(format!("missing required field '{TOOL_CALL_TOOL_FIELD}'"));
        }
    };
    // params takes precedence over the `arguments` alias; flattened GPT Action
    // fields are collected only when neither object wrapper is present.
    let params = if obj.contains_key(TOOL_CALL_PARAMS_FIELD) {
        obj.get(TOOL_CALL_PARAMS_FIELD)
            .cloned()
            .unwrap_or(Value::Null)
    } else if obj.contains_key(TOOL_CALL_ARGUMENTS_FIELD) {
        obj.get(TOOL_CALL_ARGUMENTS_FIELD)
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        let mut flattened = serde_json::Map::new();
        for (key, value) in obj {
            if !TOOL_CALL_WRAPPER_FIELDS.contains(&key.as_str())
                && key != TOOL_CALL_RECORDING_SESSION_ID_FIELD
            {
                flattened.insert(key.clone(), value.clone());
            }
        }
        if flattened.is_empty() {
            Value::Null
        } else {
            Value::Object(flattened)
        }
    };
    Ok((tool, params))
}

fn extract_recording_session_id(body: &Value) -> Option<String> {
    body.as_object()
        .and_then(|obj| obj.get(TOOL_CALL_RECORDING_SESSION_ID_FIELD))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
    use std::time::Duration;

    mod import_http_tests;
    mod jobs_tests;
    mod project_files_tests;
    mod projects_tests;

    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        })
    }

    fn test_config_oauth2(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config {
                enabled: true,
                access_token_ttl_secs: 3600,
                refresh_token_ttl_secs: 2_592_000,
                ..crate::OAuth2Config::default()
            },
        })
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn seed_oauth_client(
        db: &crate::Database,
        user: &crate::models::UserRecord,
    ) -> crate::models::OAuthClientRecord {
        let now = chrono::Utc::now().timestamp();
        let secret = crate::auth::generate_oauth_client_secret();
        let record = crate::models::OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: crate::auth::hash_token(&secret),
            name: "Test App".to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read project:write job:run account:manage"
                .to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        record
    }

    fn seed_oauth_access_token_with_shared_key_hash(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        user: &crate::models::UserRecord,
        scopes: &str,
        shared_key_hash: Option<&str>,
    ) -> String {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_access_token();
        let (subject_kind, subject_id, user_id, shared_key_hash) = match shared_key_hash {
            Some(hash) => (
                "shared_key".to_string(),
                hash.to_string(),
                None,
                Some(hash.to_string()),
            ),
            None => (
                "managed_user".to_string(),
                user.id.clone(),
                Some(user.id.clone()),
                None,
            ),
        };
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext),
            client_id: client.client_id.clone(),
            subject_kind,
            subject_id,
            user_id,
            scopes: scopes.to_string(),
            resource: None,
            shared_key_hash,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        plaintext
    }

    fn phase2_oauth_service(scopes: &str) -> (tempfile::TempDir, salvo::Service, String) {
        phase2_oauth_service_with_shared_key_hash(scopes, None)
    }

    fn phase2_oauth_service_with_shared_key_hash(
        scopes: &str,
        shared_key_hash: Option<&str>,
    ) -> (tempfile::TempDir, salvo::Service, String) {
        let config = test_config_oauth2(Some("secret"));
        let (tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_oauth_client(&db, &user);
        let token = seed_oauth_access_token_with_shared_key_hash(
            &db,
            &client,
            &user,
            scopes,
            shared_key_hash,
        );
        let project_dir = tmp.path().join("project");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("README.md"), "hello\n").unwrap();
        let runtime = Arc::new(runtime_with_local_project(&project_dir, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));
        (tmp, service, token)
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
                    .push(Router::with_path("tools/list").post(tools_list))
                    .push(Router::with_path("tools/call").post(tools_call))
                    .push(
                        Router::with_path("artifacts/import")
                            .post(import_conversation_files_to_project),
                    )
                    .push(Router::with_path("projects/list").post(projects_list))
                    .push(Router::with_path("projects/register").post(projects_register))
                    .push(Router::with_path("projects/create").post(projects_create))
                    .push(Router::with_path("projects/read_file").post(projects_read_file))
                    .push(Router::with_path("projects/git_status").post(projects_git_status))
                    .push(Router::with_path("projects/git_diff").post(projects_git_diff))
                    .push(Router::with_path("projects/apply_patch").post(projects_apply_patch))
                    .push(
                        Router::with_path("projects/validate_patch").post(projects_validate_patch),
                    )
                    .push(Router::with_path("projects/run_shell").post(projects_run_shell))
                    .push(
                        Router::with_path("projects/apply_patch_checked")
                            .post(projects_apply_patch_checked),
                    )
                    .push(Router::with_path("projects/delete_files").post(projects_delete_files))
                    .push(
                        Router::with_path("projects/git_restore_paths")
                            .post(projects_git_restore_paths),
                    )
                    .push(
                        Router::with_path("projects/discard_untracked")
                            .post(projects_discard_untracked),
                    )
                    .push(
                        Router::with_path("projects/replace_in_file")
                            .post(projects_replace_in_file),
                    )
                    .push(Router::with_path("projects/write_file").post(projects_write_file))
                    .push(Router::with_path("projects/run_job").post(projects_run_job))
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

    async fn register_import_agent_with_capabilities(
        root: &std::path::Path,
        capabilities: Option<crate::shell_protocol::ShellClientCapabilities>,
    ) -> (Arc<ToolRuntime>, Arc<ShellClientRegistry>) {
        use crate::shell_protocol::{ShellAgentProjectSummary, ShellClientRegisterRequest};
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "importer".to_string(),
                agent_instance_id: "inst-import".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities,
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "demo".to_string(),
                    name: Some("Demo".to_string()),
                    path: root.to_string_lossy().to_string(),
                    allow_patch: true,
                    kind: None,
                    description: None,
                    hooks: vec![],
                    disabled: false,
                    git_branch: None,
                    git_head: None,
                    git_dirty: None,
                    updated_at: 0,
                    shell_profile: None,
                }]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let runtime = Arc::new(ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "server projects disabled in import tests".to_string(),
                "test".to_string(),
            )),
            registry.clone(),
            Arc::new(CodexConfig::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        ));
        (runtime, registry)
    }

    async fn register_import_agent(
        root: &std::path::Path,
    ) -> (Arc<ToolRuntime>, Arc<ShellClientRegistry>) {
        register_import_agent_with_capabilities(root, None).await
    }

    async fn complete_one_agent_request(
        registry: Arc<ShellClientRegistry>,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        exit_code: i32,
    ) {
        use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
        let request = loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        registry
            .complete(ShellAgentResultRequest {
                client_id: "importer".to_string(),
                agent_instance_id: "inst-import".to_string(),
                request_id: request.request_id,
                exit_code: Some(exit_code),
                stdout: Some(stdout.into()),
                stderr: Some(stderr.into()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    }

    // =========================================================================
    // listProjects
    // =========================================================================

    #[tokio::test]
    async fn all_project_endpoints_require_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let endpoints: Vec<(&str, Value)> = vec![
            ("/api/projects/list", json!({})),
            (
                "/api/projects/read_file",
                json!({"project": "demo", "path": "README.md"}),
            ),
            ("/api/projects/git_status", json!({"project": "demo"})),
            ("/api/projects/git_diff", json!({"project": "demo"})),
            (
                "/api/projects/apply_patch",
                json!({"project": "demo", "patch": "diff"}),
            ),
            (
                "/api/projects/validate_patch",
                json!({"project": "demo", "patch": "diff"}),
            ),
            ("/api/tools/list", json!({})),
            ("/api/tools/call", json!({"tool": "list_tools"})),
            ("/api/runtime/status", json!({})),
            (
                "/api/projects/register",
                json!({"client_id": "oe", "id": "my-project", "name": "My Project", "path": "/root/git/my-project"}),
            ),
            (
                "/api/projects/create",
                json!({"client_id": "oe", "id": "hello", "name": "Hello", "path": "/root/git/hello"}),
            ),
        ];
        for (path, body) in &endpoints {
            let resp = TestClient::post(&format!("http://localhost{path}"))
                .json(body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{path} should require bearer auth"
            );
        }
    }

    // =========================================================================
    // getRuntimeStatus / /api/runtime/status
    // =========================================================================

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
        assert_eq!(out["service"], "webcodex");
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
    // Phase 2: callRuntimeTool / /api/tools/call generic entry point
    // =========================================================================

    fn phase2_service() -> (tempfile::TempDir, salvo::Service) {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));
        (_tmp, service)
    }

    #[test]
    fn extract_tool_call_params_precede_flattened_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "wrong",
            "params": {"project": "right"},
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(params, json!({"project": "right"}));
    }

    #[test]
    fn extract_tool_call_arguments_precede_flattened_fields_without_params() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "wrong",
            "arguments": {"project": "right"},
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(params, json!({"project": "right"}));
    }

    #[test]
    fn extract_tool_call_collects_flattened_top_level_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "agent:oe:webcodex",
            "session_id": "wc_sess_tool_arg",
            "recording_session_id": "wc_sess_recorder",
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(
            params,
            json!({"project": "agent:oe:webcodex", "session_id": "wc_sess_tool_arg"})
        );
        assert_eq!(
            extract_recording_session_id(&json!({"recording_session_id": "wc_sess_recorder"})),
            Some("wc_sess_recorder".to_string())
        );
    }

    #[test]
    fn extract_tool_call_collects_flattened_session_handoff_flags() {
        let body = json!({
            "tool": "session_handoff_summary",
            "project": "agent:special:test-mcp",
            "session_id": "wc_sess_test",
            "include_validation": true,
            "include_workspace": true,
            "include_checkpoints": true,
            "limit": 20,
            "recording_session_id": "wc_sess_recorder"
        });
        let (tool, params) = extract_tool_call(&body).unwrap();

        assert_eq!(tool, "session_handoff_summary");
        assert_eq!(
            params,
            json!({
                "project": "agent:special:test-mcp",
                "session_id": "wc_sess_test",
                "include_validation": true,
                "include_workspace": true,
                "include_checkpoints": true,
                "limit": 20
            })
        );
        assert!(
            params
                .as_object()
                .is_some_and(|m| !m.contains_key("recording_session_id")),
            "recording_session_id must not leak into concrete params"
        );
        assert_eq!(
            extract_recording_session_id(&body),
            Some("wc_sess_recorder".to_string())
        );
    }

    #[test]
    fn extract_tool_call_collects_flattened_line_edit_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "replace_line_range",
            "project": "agent:oe:webcodex",
            "path": "x.tmp",
            "start_line": 2,
            "end_line": 3,
            "new_text": "BETA\nGAMMA\n",
            "expected_old_prefix": "beta\n",
        }))
        .unwrap();

        assert_eq!(tool, "replace_line_range");
        assert_eq!(params["project"], "agent:oe:webcodex");
        assert_eq!(params["path"], "x.tmp");
        assert_eq!(params["start_line"], 2);
        assert_eq!(params["end_line"], 3);
        assert_eq!(params["new_text"], "BETA\nGAMMA\n");
        assert_eq!(params["expected_old_prefix"], "beta\n");
    }

    #[test]
    fn extract_tool_call_collects_flattened_anchor_edit_fields() {
        let old_key = concat!("old_", "text");
        let guard_key = concat!("expected_old_", "sha256");
        let mut body = serde_json::Map::new();
        body.insert("tool".to_string(), json!("replace_exact_block"));
        body.insert("project".to_string(), json!("demo"));
        body.insert("path".to_string(), json!("x.tmp"));
        body.insert(old_key.to_string(), json!("old\n"));
        body.insert("new_text".to_string(), json!("new\n"));
        body.insert(guard_key.to_string(), json!("whole-file-sha"));
        let (tool, params) = extract_tool_call(&Value::Object(body)).unwrap();

        assert_eq!(tool, "replace_exact_block");
        assert_eq!(params["project"], "demo");
        assert_eq!(params["path"], "x.tmp");
        assert_eq!(params[old_key], "old\n");
        assert_eq!(params["new_text"], "new\n");
        assert_eq!(params[guard_key], "whole-file-sha");
    }

    #[test]
    fn extract_tool_call_collects_flattened_checkpoint_restore_fields() {
        // GPT Action flattened call for workspace_checkpoint_restore: the
        // recorder metadata (recording_session_id) must be stripped from
        // params while the business fields (project/checkpoint_id/confirm)
        // are collected into params for concrete dispatch.
        let body = json!({
            "tool": "workspace_checkpoint_restore",
            "project": "agent:special:test",
            "checkpoint_id": "wc_ckpt_abc",
            "confirm": true,
            "recording_session_id": "wc_sess_record"
        });
        let (tool, params) = extract_tool_call(&body).unwrap();

        assert_eq!(tool, "workspace_checkpoint_restore");
        assert_eq!(params["project"], "agent:special:test");
        assert_eq!(params["checkpoint_id"], "wc_ckpt_abc");
        assert_eq!(params["confirm"], true);
        assert!(
            params
                .as_object()
                .is_some_and(|m| !m.contains_key("recording_session_id")),
            "recording_session_id must not leak into concrete params"
        );
        assert_eq!(
            extract_recording_session_id(&body),
            Some("wc_sess_record".to_string()),
            "recording_session_id must remain available as wrapper recorder metadata"
        );
    }

    #[test]
    fn extract_tool_call_collects_flattened_apply_text_edits_fields() {
        // GPT Action flattened call for apply_text_edits: nested `edits`
        // array and scalar flattened fields must be collected into params.
        let (tool, params) = extract_tool_call(&json!({
            "tool": "apply_text_edits",
            "project": "agent:special:test",
            "path": "a.txt",
            "dry_run": true,
            "edits": [
                {"kind": "replace_exact", "old_text": "a", "new_text": "b"}
            ]
        }))
        .unwrap();

        assert_eq!(tool, "apply_text_edits");
        assert_eq!(params["project"], "agent:special:test");
        assert_eq!(params["path"], "a.txt");
        assert_eq!(params["dry_run"], true);
        let edits = params["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["kind"], "replace_exact");
        assert_eq!(edits[0]["old_text"], "a");
        assert_eq!(edits[0]["new_text"], "b");
    }

    #[test]
    fn extract_tool_call_no_argument_tool_keeps_null_params() {
        let (tool, params) = extract_tool_call(&json!({"tool": "list_tools"})).unwrap();

        assert_eq!(tool, "list_tools");
        assert!(params.is_null() || params.as_object().is_some_and(|m| m.is_empty()));
    }

    #[tokio::test]
    async fn http_tools_list_returns_names_and_count() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(
            body["tools"].is_array(),
            "tools array must remain for back-compat"
        );
        assert!(body["names"].is_array(), "names array must be present");
        let names = body["names"].as_array().unwrap();
        assert!(!names.is_empty(), "names must not be empty");
        assert!(names.iter().any(|n| n == "list_tools"));
        assert!(names.iter().any(|n| n == "git_diff_summary"));
        assert!(names.iter().any(|n| n == "git_log"));
        assert!(names.iter().any(|n| n == "show_changes"));
        assert!(
            !names.iter().any(|n| n == "run_codex"),
            "model-facing tools/list names must not include run_codex: {:?}",
            names
        );
        assert_eq!(body["count"], names.len());
        for tool in body["tools"].as_array().unwrap() {
            assert!(tool["inputSchema"].is_object());
            assert!(tool["outputSchema"].is_object());
        }
        // Optional enrichment fields.
        assert!(body["categories"].is_object(), "categories must be present");
        assert!(
            body["recommended_flows"].is_array(),
            "recommended_flows must be present"
        );
        // names and tools must stay in sync.
        let tools_count = body["tools"].as_array().unwrap().len();
        assert_eq!(tools_count, names.len());
    }

    #[tokio::test]
    async fn http_tools_list_supports_bounded_summary_request() {
        let (_tmp, service) = phase2_service();
        let mut full_resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&full_resp), StatusCode::OK);
        let full_body: Value = full_resp.take_json().await.unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({
                "category": "artifact",
                "features": "artifact_upload",
                "summary_only": true,
                "limit": 10
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["category"], "artifact");
        assert_eq!(body["features"], "artifact_upload");
        assert_eq!(body["truncated"], false);
        assert_eq!(body["total_count"], full_body["total_count"]);
        let names = body["names"].as_array().unwrap();
        for tool in [
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
        ] {
            assert!(names.iter().any(|name| name == tool), "missing {tool}");
        }
        for tool in body["tools"].as_array().unwrap() {
            assert!(tool.get("inputSchema").is_none(), "{tool:?}");
            assert!(tool.get("outputSchema").is_none(), "{tool:?}");
        }
        assert!(
            body.to_string().len() < full_body.to_string().len() / 2,
            "bounded response should be substantially smaller than full list"
        );
    }

    #[tokio::test]
    async fn http_tools_call_run_codex_returns_disabled_without_creating_job() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "run_codex",
                "params": {
                    "project": "demo",
                    "prompt": "summarize"
                }
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert_eq!(body["output"]["code"], "run_codex_disabled");
        let err = body["error"].as_str().unwrap();
        assert!(err.contains("currently disabled"), "{err}");
        assert!(!err.contains("/"), "error must not leak local paths: {err}");

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_jobs", "params": {}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["output"]["jobs"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn http_tools_call_params_omitted_works_for_list_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["tools"].is_array());
    }

    #[tokio::test]
    async fn http_tools_call_params_null_works_for_list_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools", "params": null}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["tools"].is_array());
    }

    #[tokio::test]
    async fn http_tools_call_arguments_alias_works() {
        // `arguments` is accepted as a compatibility alias for `params`.
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools", "arguments": null}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
    }

    #[tokio::test]
    async fn start_session_returns_session_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let (runtime, _registry) = register_import_agent(tmp_proj.path()).await;
        let service = Service::new(build_projects_router(config, db, runtime));
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "start_session",
                "project": "demo",
                "title": "implement show_changes follow-up"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["output"]["success"], true);
        assert!(body["output"]["session_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("wc_sess_")));
        assert_eq!(body["output"]["project"], "agent:importer:demo");
        assert_eq!(body["output"]["project_input"], "demo");
        assert_eq!(body["output"]["resolved_project"], "agent:importer:demo");
        assert_eq!(body["output"]["title"], "implement show_changes follow-up");
        assert!(body["output"]["created_at"].is_i64());
    }

    #[tokio::test]
    async fn session_summary_empty_session() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "params": {"title": "empty"}}))
            .send(&service)
            .await;
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "session_summary",
                "session_id": session_id,
                "limit": 50
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["session_id"], session_id);
        assert_eq!(body["output"]["counts"]["tool_calls"], 0);
        assert_eq!(body["output"]["counts"]["succeeded"], 0);
        assert_eq!(body["output"]["counts"]["failed"], 0);
        assert!(body["output"]["events"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn start_session_read_only_summary_returns_guard_config() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "start_session",
                "params": {"title": "readonly", "mode": "read_only"}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();
        assert_eq!(start_body["output"]["mode"], "read_only");
        assert_eq!(start_body["output"]["guards"]["deny_write_tools"], true);
        assert_eq!(start_body["output"]["guards"]["deny_shell_tools"], true);

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "session_summary", "params": {"session_id": session_id}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["session_id"], session_id);
        assert_eq!(body["output"]["mode"], "read_only");
        assert_eq!(body["output"]["guards"]["deny_write_tools"], true);
        assert_eq!(body["output"]["guards"]["deny_shell_tools"], true);
    }

    #[tokio::test]
    async fn api_tools_call_records_success_event_with_session_id() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "params": {"title": "tracking"}}))
            .send(&service)
            .await;
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "list_projects",
                "recording_session_id": session_id,
                "params": {}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let _: Value = resp.take_json().await.unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "session_summary", "params": {"session_id": session_id}}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["counts"]["tool_calls"], 1);
        assert_eq!(body["output"]["counts"]["succeeded"], 1);
        assert_eq!(body["output"]["counts"]["failed"], 0);
        let events = body["output"]["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["kind"], "tool_call_started");
        assert_eq!(events[1]["kind"], "tool_call_finished");
        assert_eq!(events[1]["transport"], "api");
        assert_eq!(events[1]["tool_name"], "list_projects");
        assert_eq!(events[1]["risk_class"], "read_only");
        assert_eq!(events[1]["status"], "succeeded");
        assert!(events[1]["duration_ms"].is_u64());
    }

    #[tokio::test]
    async fn api_tools_call_records_failure_event_with_session_id() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "params": {"title": "tracking"}}))
            .send(&service)
            .await;
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "read_file",
                "recording_session_id": session_id,
                "params": {"project": "demo", "path": "missing.txt"}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let _: Value = resp.take_json().await.unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "session_summary", "params": {"session_id": session_id}}))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["counts"]["tool_calls"], 1);
        assert_eq!(body["output"]["counts"]["failed"], 1);
        let event = &body["output"]["events"].as_array().unwrap()[1];
        assert_eq!(event["tool_name"], "read_file");
        assert_eq!(event["status"], "failed");
        assert_eq!(event["error_kind"], "runtime_error");
        assert!(event["error_message_summary"].as_str().unwrap().len() <= 243);
    }

    #[tokio::test]
    async fn api_tools_call_uses_recording_session_id_for_recorder_metadata() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "title": "tracking"}))
            .send(&service)
            .await;
        let tracking_body: Value = resp.take_json().await.unwrap();
        let tracking_session_id = tracking_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "title": "business"}))
            .send(&service)
            .await;
        let business_body: Value = resp.take_json().await.unwrap();
        let business_session_id = business_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "session_summary",
                "session_id": business_session_id,
                "recording_session_id": tracking_session_id
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["session_id"], business_session_id);
        assert_eq!(body["output"]["title"], "business");
        assert_eq!(body["output"]["session_recorded"], true);

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "session_summary",
                "session_id": tracking_session_id
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let tracking_summary: Value = resp.take_json().await.unwrap();
        assert_eq!(
            tracking_summary["output"]["events"][0]["tool_name"],
            "session_summary"
        );
        assert_eq!(
            tracking_summary["output"]["events"][0]["input_summary"]["session_id"],
            business_session_id
        );
    }

    #[tokio::test]
    async fn api_tools_call_message_tool_keeps_business_session_id_with_recording_session_id() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "title": "tracking"}))
            .send(&service)
            .await;
        let tracking_body: Value = resp.take_json().await.unwrap();
        let tracking_session_id = tracking_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session", "title": "business"}))
            .send(&service)
            .await;
        let business_body: Value = resp.take_json().await.unwrap();
        let business_session_id = business_body["output"]["session_id"].as_str().unwrap();

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "post_session_message",
                "session_id": business_session_id,
                "recording_session_id": tracking_session_id,
                "kind": "guidance",
                "message": "Keep this behind callRuntimeTool.",
                "tags": ["openapi", "constraint"],
                "priority": "normal"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["session_id"], business_session_id);
        assert!(body["output"]["message_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("wc_msg_")));
        assert_eq!(body["output"]["session_recorded"], true);

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "list_session_messages",
                "session_id": business_session_id,
                "kind": "guidance"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let business_messages: Value = resp.take_json().await.unwrap();
        assert_eq!(
            business_messages["output"]["session_id"],
            business_session_id
        );
        assert_eq!(
            business_messages["output"]["messages"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "session_summary",
                "session_id": tracking_session_id
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let tracking_summary: Value = resp.take_json().await.unwrap();
        assert_eq!(
            tracking_summary["output"]["events"][0]["tool_name"],
            "post_session_message"
        );
        assert_eq!(
            tracking_summary["output"]["events"][0]["input_summary"]["session_id"],
            business_session_id
        );
    }

    #[tokio::test]
    async fn read_only_session_allows_post_session_message_metadata() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "start_session",
                "title": "readonly message board",
                "mode": "read_only"
            }))
            .send(&service)
            .await;
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();
        assert_eq!(start_body["output"]["mode"], "read_only");

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "post_session_message",
                "session_id": session_id,
                "kind": "progress",
                "message": "Read-only sessions may still record collaboration metadata."
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["message"]["kind"], "progress");
        assert!(body["output"].get("changed_paths").is_none());

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "session_summary", "session_id": session_id}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let summary: Value = resp.take_json().await.unwrap();
        assert_eq!(summary["output"]["messages"]["total"], 1);
        assert_eq!(summary["output"]["counts"]["tool_calls"], 0);
    }

    #[tokio::test]
    async fn session_summary_bounds_event_limit() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "start_session"}))
            .send(&service)
            .await;
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();

        for _ in 0..3 {
            let mut resp = TestClient::post("http://localhost/api/tools/call")
                .bearer_auth("secret")
                .json(&json!({
                    "tool": "list_projects",
                    "recording_session_id": session_id,
                    "params": {}
                }))
                .send(&service)
                .await;
            let _: Value = resp.take_json().await.unwrap();
        }

        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "session_summary",
                "params": {"session_id": session_id, "limit": 1}
            }))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["output"]["counts"]["tool_calls"], 3);
        let events = body["output"]["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "tool_call_finished");
    }

    #[tokio::test]
    async fn http_tools_call_params_wins_over_arguments() {
        // When both params and arguments are present, params wins. Use a tool
        // whose params shape we can distinguish: git_diff_summary takes a
        // `project`. The runtime returns a structured error for an unknown
        // project, but the project string from `params` is what gets routed,
        // so we assert the error names the params project (not the arguments
        // one).
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "git_diff_summary",
                "params": {"project": "agent:params-wins:p"},
                "arguments": {"project": "agent:arguments-loses:p"},
            }))
            .send(&service)
            .await;
        // Authenticated + dispatched to ToolRuntime (structured error, not 401).
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("params-wins"),
            "params must win over arguments; error was: {}",
            err
        );
        assert!(
            !err.contains("arguments-loses"),
            "arguments must not be used when params present; error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_unknown_tool_returns_useful_error() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "definitely_not_a_tool"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("definitely_not_a_tool"),
            "error must name the tool"
        );
        // Must point the caller at discovery and list available tools.
        assert!(
            err.contains("listRuntimeTools") || err.contains("list_tools"),
            "error should hint at discovery: {}",
            err
        );
        assert!(
            err.contains("git_diff_summary"),
            "error should list available tools: {}",
            err
        );
        // Must not leak secrets / config artifacts.
        let lower = err.to_lowercase();
        for forbidden in [
            "token",
            "authorization",
            "agent.toml",
            "webcodex.env",
            "secret",
        ] {
            assert!(
                !lower.contains(&forbidden),
                "unknown-tool error must not leak '{}': {}",
                forbidden,
                err
            );
        }
    }

    #[tokio::test]
    async fn http_tools_call_missing_required_field_names_tool_and_field() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "run_shell", "params": {"command": "echo"}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("run_shell"),
            "error must name the tool: {}",
            err
        );
        assert!(
            err.contains("project"),
            "error must name the missing field: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_wrong_field_type_names_tool() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "run_shell", "params": {"project": 123, "command": "echo"}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("run_shell"),
            "wrong-type error must name the tool: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_missing_tool_field_returns_field_error() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"params": {}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("tool"),
            "error must mention the missing 'tool' field: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_git_diff_summary_dispatches() {
        // callRuntimeTool routes git_diff_summary to the runtime. With an
        // unknown agent project the runtime returns a structured error (not a
        // 401/404), proving the generic path deserializes + dispatches.
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "git_diff_summary",
                "params": {"project": "agent:nope:nope"}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "git_diff_summary should return a structured runtime error"
        );
    }

    #[tokio::test]
    async fn http_tools_call_git_log_dispatches() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "git_log",
                "params": {"project": "agent:nope:nope", "limit": 5, "skip": 1}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "git_log should return a structured runtime error"
        );
    }

    #[tokio::test]
    async fn http_tools_call_show_changes_dispatches() {
        // callRuntimeTool routes show_changes to the runtime. With an unknown
        // agent project the runtime returns a structured error, proving the
        // generic path deserializes + dispatches.
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "show_changes",
                "params": {"project": "agent:nope:nope", "include_diff": false}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "show_changes should return a structured runtime error"
        );
    }

    #[tokio::test]
    async fn api_show_changes_with_session_id() {
        use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let (runtime, registry) = register_import_agent(tmp_proj.path()).await;
        let service = Service::new(build_projects_router(config, db, runtime));
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "start_session",
                "params": {"project": "agent:importer:demo", "title": "api show changes"}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let start_body: Value = resp.take_json().await.unwrap();
        let session_id = start_body["output"]["session_id"].as_str().unwrap();

        let request = async {
            TestClient::post("http://localhost/api/tools/call")
                .bearer_auth("secret")
                .json(&json!({
                    "tool": "show_changes",
                    "params": {
                        "project": "agent:importer:demo",
                        "session_id": session_id,
                        "include_diff": false,
                        "session_event_limit": 10
                    }
                }))
                .send(&service)
                .await
        };
        let complete = async {
            let mut req = None;
            for _ in 0..20 {
                req = registry
                    .poll(ShellAgentPollRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        projects: None,
                    })
                    .await
                    .unwrap();
                if req.is_some() {
                    break;
                }
                tokio::task::yield_now().await;
            }
            let req = req.expect("show_changes should enqueue an agent shell request");
            let stdout = "## main\n?? README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
            registry
                .complete(ShellAgentResultRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    request_id: req.request_id,
                    exit_code: Some(0),
                    stdout: Some(stdout.to_string()),
                    stderr: Some(String::new()),
                    duration_ms: Some(1),
                    error: None,
                })
                .await
                .unwrap();
        };
        let (mut resp, _) = tokio::join!(request, complete);
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["output"]["project"], "agent:importer:demo");
        assert_eq!(body["output"]["session"]["found"], true);
        assert_eq!(body["output"]["session"]["session_id"], session_id);
        assert_eq!(body["output"]["session"]["title"], "api show changes");
    }

    async fn oauth_tools_call(
        service: &Service,
        token: &str,
        tool: &str,
        params: Value,
    ) -> (StatusCode, Value, Option<String>) {
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth(token)
            .json(&json!({"tool": tool, "params": params}))
            .send(service)
            .await;
        let status = effective_status(&resp);
        let challenge = resp
            .headers()
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.take_json::<Value>().await.unwrap();
        (status, body, challenge)
    }

    fn assert_oauth_scope_rejected(
        status: StatusCode,
        body: &Value,
        challenge: Option<&str>,
        scope: Option<&str>,
    ) {
        assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
        assert_eq!(body["error"], "insufficient_scope");
        let challenge = challenge.unwrap_or("");
        assert!(
            challenge.contains("error=\"insufficient_scope\""),
            "challenge: {}",
            challenge
        );
        if let Some(scope) = scope {
            assert!(
                body["error_description"]
                    .as_str()
                    .unwrap_or("")
                    .contains(scope),
                "body: {:?}",
                body
            );
            assert!(challenge.contains(scope), "challenge: {}", challenge);
        }
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_runtime_read_for_list_tools_or_runtime_status() {
        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, _) = oauth_tools_call(&service, &token, "list_tools", Value::Null).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) =
            oauth_tools_call(&service, &token, "runtime_status", Value::Null).await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_RUNTIME_READ),
        );
    }

    #[tokio::test]
    async fn session_tools_oauth_scope_policy() {
        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, _) =
            oauth_tools_call(&service, &token, "start_session", json!({"title": "oauth"})).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);
        let session_id = body["output"]["session_id"].as_str().unwrap();
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "session_summary",
            json!({"session_id": session_id}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) =
            oauth_tools_call(&service, &token, "start_session", json!({})).await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_RUNTIME_READ),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_project_read_for_read_file() {
        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "read_file",
            json!({"project": "demo", "path": "README.md"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "read_file",
            json!({"project": "demo", "path": "README.md"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_PROJECT_READ),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_show_changes_tool_scope_is_project_read() {
        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "show_changes",
            json!({"project": "agent:nope:nope", "session_id": "wc_sess_missing"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {:?}", body);
        assert_eq!(body["success"], false);

        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "show_changes",
            json!({"project": "agent:nope:nope", "session_id": "wc_sess_missing"}),
        )
        .await;
        assert_oauth_scope_rejected(status, &body, challenge.as_deref(), Some("project:read"));
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_project_write_for_anchor_edit_tools() {
        let (_tmp, service, token) = phase2_oauth_service("project:write");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "replace_exact_block",
            json!({"project": "demo", "path": "README.md", "old_text": "old", "new_text": "new"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "insert_before_pattern",
            json!({
                "project": "demo",
                "path": "README.md",
                "pattern": "anchor",
                "text": "inserted\n"
            }),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_PROJECT_WRITE),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_job_run_for_run_shell_or_run_job() {
        let (_tmp, service, token) = phase2_oauth_service("job:run");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "run_shell",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "run_job",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_JOB_RUN),
        );
    }

    #[tokio::test]
    async fn bridge_oauth2_tools_call_still_requires_project_read_and_job_run_scopes() {
        let (_tmp, service, token) =
            phase2_oauth_service_with_shared_key_hash("runtime:read", Some("hash-a"));
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "read_file",
            json!({"project": "demo", "path": "README.md"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_PROJECT_READ),
        );

        let (_tmp, service, token) =
            phase2_oauth_service_with_shared_key_hash("project:read", Some("hash-a"));
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "run_job",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_JOB_RUN),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_unknown_tool_fails_closed() {
        let (_tmp, service, token) = phase2_oauth_service("runtime:read project:read");
        let (status, body, challenge) =
            oauth_tools_call(&service, &token, "definitely_not_a_tool", Value::Null).await;
        assert_oauth_scope_rejected(status, &body, challenge.as_deref(), None);
    }

    #[tokio::test]
    async fn api_token_tools_call_behavior_unchanged() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "definitely_not_a_tool"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or("")
            .contains("definitely_not_a_tool"));
    }

    #[tokio::test]
    async fn http_tools_list_includes_phase4_edit_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let names = body["names"].as_array().unwrap();
        assert!(names.iter().any(|n| n == "replace_in_file"));
        assert!(names.iter().any(|n| n == "write_project_file"));
        assert_eq!(body["count"], names.len());
        let tools = body["tools"].as_array().unwrap();
        let start_session = tools
            .iter()
            .find(|tool| tool["name"] == "start_session")
            .expect("missing start_session");
        assert_eq!(
            start_session["inputSchema"]["properties"]["mode"]["enum"],
            json!(["normal", "read_only"])
        );
        assert!(start_session["inputSchema"]["properties"]
            .get("deny_write_tools")
            .is_some());
        assert!(start_session["inputSchema"]["properties"]
            .get("deny_shell_tools")
            .is_some());
        assert!(
            start_session["outputSchema"]["properties"]["output"]["properties"]
                .get("guards")
                .is_some()
        );
        for name in ["read_file", "run_shell", "write_project_file"] {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("missing tool {name}"));
            assert!(
                tool["inputSchema"]["properties"]
                    .get("session_id")
                    .is_some(),
                "tools/list schema missing session_id for {name}"
            );
            assert!(
                !tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|field| field == "session_id"),
                "session_id must be optional for {name}"
            );
        }
    }

    #[tokio::test]
    async fn http_tools_call_dispatches_phase4_edit_tools() {
        // callRuntimeTool routes replace_in_file / write_project_file to the
        // runtime. With a non-agent project the runtime returns a structured
        // error (not a 401/404), proving the generic path dispatches them.
        let (_tmp, service) = phase2_service();
        for (tool, params) in [
            (
                "replace_in_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "old": "a", "new": "b"}),
            ),
            (
                "write_project_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
            ),
        ] {
            let mut resp = TestClient::post("http://localhost/api/tools/call")
                .bearer_auth("secret")
                .json(&json!({"tool": tool, "params": params}))
                .send(&service)
                .await;
            assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
            let body: Value = resp.take_json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(
                body["error"].as_str().is_some_and(|e| !e.is_empty()),
                "{} should return a structured runtime error",
                tool
            );
        }
    }
}
