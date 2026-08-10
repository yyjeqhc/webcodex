use super::*;
use crate::shell_client::ShellClientRegistry;
use crate::test_support::{seed_oauth_client, seed_user, test_config, test_config_oauth2, test_db};
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use std::time::Duration;

#[path = "runtime_http/tests/import_http_tests.rs"]
mod import_http_tests;
#[path = "runtime_http/tests/jobs_tests.rs"]
mod jobs_tests;
#[path = "runtime_http/tests/project_files_tests.rs"]
mod project_files_tests;
#[path = "runtime_http/tests/projects_tests.rs"]
mod projects_tests;

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
    let token =
        seed_oauth_access_token_with_shared_key_hash(&db, &client, &user, scopes, shared_key_hash);
    let project_dir = tmp.path().join("project");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::write(project_dir.join("README.md"), "hello\n").unwrap();
    let runtime = Arc::new(runtime_with_local_project(&project_dir, "demo"));
    let service = Service::new(build_projects_router(config, db, runtime));
    (tmp, service, token)
}

/// Build a ToolRuntime without any server-side project configuration.
fn runtime_with_local_project(root: &std::path::Path, project_id: &str) -> ToolRuntime {
    let _ = (root, project_id);
    ToolRuntime::new(
        Arc::new(ShellClientRegistry::default()),
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
                .push(Router::with_path("projects/validate_patch").post(projects_validate_patch))
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
                .push(Router::with_path("projects/run_job").post(projects_run_job))
                .push(Router::with_path("projects/list_files").post(projects_list_files))
                .push(Router::with_path("projects/search_text").post(projects_search_text))
                .push(
                    Router::with_path("projects/git_diff_summary").post(projects_git_diff_summary),
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
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
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
                revision: None,
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
    let runtime = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
        registry.clone(),
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

fn spawn_startup_agent_executor(registry: Arc<ShellClientRegistry>) -> tokio::task::JoinHandle<()> {
    use crate::shell_protocol::{
        ShellAgentPollRequest, ShellAgentResultRequest, ShellAgentShellRequest,
    };
    use std::path::Path;

    fn execute(request: &ShellAgentShellRequest) -> (i32, String, String) {
        if request.kind == "file_read" {
            return (1, String::new(), "No such file or directory".to_string());
        }
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&request.command)
            .current_dir(Path::new(request.cwd.as_deref().unwrap_or(".")))
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    tokio::spawn(async move {
        loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                let (exit_code, stdout, stderr) = execute(&request);
                registry
                    .complete(ShellAgentResultRequest {
                        client_id: "importer".to_string(),
                        agent_instance_id: "inst-import".to_string(),
                        request_id: request.request_id,
                        exit_code: Some(exit_code),
                        stdout: Some(stdout),
                        stderr: Some(stderr),
                        duration_ms: Some(1),
                        error: None,
                    })
                    .await
                    .unwrap();
            } else {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    })
}

// =========================================================================
// listProjects
// =========================================================================

#[tokio::test]
async fn all_project_endpoints_require_bearer_auth() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
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
    let _env = crate::auth::AuthEnvGuard::auth_required();
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
    let (runtime, _registry) = register_import_agent(tmp_proj.path()).await;
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
    assert_eq!(out["projects"]["mode"], "agent_registered");
    assert!(out["projects"].get("configured").is_none());
    assert!(out["projects"].get("server_static").is_none());
    assert_eq!(out["projects"]["count"], 1);
    assert_eq!(out["projects"]["agent_registered"]["count"], 1);
    assert_eq!(out["projects"]["agent_registered"]["online_count"], 1);
    assert_eq!(out["projects"]["effective"]["count"], 1);
    assert_eq!(out["projects"]["effective"]["status"], "ok");
    assert!(out["agents"]["count"].is_i64());
    assert!(out["jobs"]["active_count"].is_i64());
    assert!(out["jobs"]["running_count"].is_i64());
    assert!(out["jobs"]["queued_count"].is_i64());
    assert_eq!(
        out["agents"]["clients"][0]["job_concurrency"],
        json!({"limit": null, "running": 0, "queued": 0})
    );
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

#[tokio::test]
async fn flattened_tool_manifest_audit_intent_survives_null_params_wrapper() {
    let (tool, params) = extract_tool_call(&json!({
        "tool": "tool_manifest",
        "params": null,
        "intent": "audit",
    }))
    .unwrap();
    let call = ToolCall::from_tool_name(&tool, params).unwrap();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = runtime_with_local_project(tmp_proj.path(), "demo");

    let result = runtime.dispatch(call).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "audit");
    assert_eq!(result.output["filtered"], true);
    assert!(
        result.output["returned_count"].as_u64().unwrap()
            < result.output["total_count"].as_u64().unwrap(),
        "audit intent must not silently degrade to the full manifest: {:?}",
        result.output
    );
}

#[tokio::test]
async fn http_start_coding_task_rejects_removed_flattened_startup_params() {
    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let (runtime, _registry) = register_import_agent(tmp_proj.path()).await;
    let service = Service::new(build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({
            "tool": "start_coding_task",
            "params": null,
            "project": "agent:importer:demo",
            "include_runtime_status": false,
            "include_git": false,
            "include_recent_commits": false,
            "include_rules": false,
            "include_tool_manifest": true,
            "tool_manifest_intent": "audit",
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["status"], 400);
    let error = body["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("invalid arguments for tool 'start_coding_task'")
            && error.contains("unknown field(s)"),
        "removed flattened startup params must fail strict validation: {body}"
    );
    assert!(
        error.contains("tool_manifest_intent"),
        "rejection should name the removed field: {error}"
    );
}

// =========================================================================
// GPT Action compact response experiment (WEBCODEX_ACTION_COMPACT_RESPONSES)
// =========================================================================

/// Serialize access to `WEBCODEX_ACTION_COMPACT_RESPONSES` and restore the
/// previous process value when dropped.
struct ActionCompactEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<String>,
}

impl ActionCompactEnvGuard {
    fn set(enabled: bool) -> Self {
        let lock = crate::admin_cli::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var("WEBCODEX_ACTION_COMPACT_RESPONSES").ok();
        if enabled {
            std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "true");
        } else {
            std::env::remove_var("WEBCODEX_ACTION_COMPACT_RESPONSES");
        }
        Self {
            _lock: lock,
            previous,
        }
    }

    fn enabled() -> Self {
        Self::set(true)
    }

    fn disabled() -> Self {
        Self::set(false)
    }
}

impl Drop for ActionCompactEnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", value),
            None => std::env::remove_var("WEBCODEX_ACTION_COMPACT_RESPONSES"),
        }
    }
}

fn standard_start_coding_task_call_body(project: &str) -> Value {
    json!({
        "tool": "start_coding_task",
        "project": project,
        "detail": "standard",
    })
}

#[tokio::test]
async fn http_start_coding_task_default_response_is_standard() {
    let _compact = ActionCompactEnvGuard::disabled();

    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let (runtime, registry) = register_import_agent_with_capabilities(
        tmp_proj.path(),
        Some(crate::shell_protocol::ShellClientCapabilities {
            shell: true,
            file_read: true,
            file_write: true,
            git: true,
            ..Default::default()
        }),
    )
    .await;
    let executor = spawn_startup_agent_executor(registry);
    let service = Service::new(build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({
            "tool": "start_coding_task",
            "project": "agent:importer:demo"
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["output"].get("compact").is_none());
    let session_id = body["output"]["session"]["session_id"]
        .as_str()
        .expect("standard response keeps nested session.session_id");
    assert!(session_id.starts_with("wc_sess_"));
    assert_eq!(body["output"]["detail"], "standard");
    assert!(body["output"].get("tool_manifest").is_none());
    assert!(body["output"].get("recommended_flow").is_none());
    assert!(body["output"].get("rules").is_none());
    assert!(body["output"].get("runtime_status").is_none());
    assert!(body["output"]["instructions"].is_object());
    assert!(body["output"]["continuation"].is_object());
    assert!(body["output"]["workspace"].is_object());
    assert!(body["output"]["startup_verdict"].is_object());
    executor.abort();
}

#[tokio::test]
async fn http_start_coding_task_action_compact_wraps_the_shared_core_brief() {
    let _compact = ActionCompactEnvGuard::enabled();

    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let (runtime, registry) = register_import_agent_with_capabilities(
        tmp_proj.path(),
        Some(crate::shell_protocol::ShellClientCapabilities {
            shell: true,
            file_read: true,
            file_write: true,
            git: true,
            ..Default::default()
        }),
    )
    .await;
    let executor = spawn_startup_agent_executor(registry);
    let service = Service::new(build_projects_router(config, db, runtime));

    // REST without the Actions wrapper returns the shared core directly.
    std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "false");
    let mut standard_resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&standard_start_coding_task_call_body("agent:importer:demo"))
        .send(&service)
        .await;
    let standard_status = effective_status(&standard_resp);
    let standard_body: Value = standard_resp.take_json().await.unwrap();
    assert_eq!(standard_status, StatusCode::OK, "{standard_body}");
    let standard_session = standard_body["output"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(standard_body["output"]["detail"], "standard");
    assert!(standard_body["output"].get("runtime_status").is_none());

    std::env::set_var("WEBCODEX_ACTION_COMPACT_RESPONSES", "true");
    let mut compact_resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&standard_start_coding_task_call_body("agent:importer:demo"))
        .send(&service)
        .await;
    let compact_status = effective_status(&compact_resp);
    let compact_body: Value = compact_resp.take_json().await.unwrap();
    assert_eq!(compact_status, StatusCode::OK, "{compact_body}");
    let compact_bytes = serde_json::to_vec(&compact_body).unwrap().len();

    assert_eq!(compact_body["success"], true);
    assert_eq!(compact_body["output"]["compact"], true);
    let core = &compact_body["output"]["startup_brief"];
    let compact_session = core["session"]["session_id"]
        .as_str()
        .expect("Actions wrapper must retain core session identity");
    assert!(compact_session.starts_with("wc_sess_"));
    assert_eq!(core["detail"], "standard");
    assert!(core["instructions"].is_object());
    assert!(core["continuation"].is_object());
    assert!(core["workspace"].is_object());
    assert!(core["startup_verdict"]["suggested_next_actions"].is_array());
    assert!(core.get("runtime_status").is_none());
    for field in [
        "detail",
        "project",
        "workspace",
        "instructions",
        "continuation",
        "semantic_navigation",
        "blockers",
        "warnings",
        "startup_verdict",
        "deterministic",
        "llm_summary",
    ] {
        assert_eq!(
            core[field], standard_body["output"][field],
            "REST and GPT Actions must carry the same shared core field {field}"
        );
    }
    let mut rest_session = standard_body["output"]["session"].clone();
    let mut action_session = core["session"].clone();
    rest_session.as_object_mut().unwrap().remove("session_id");
    action_session.as_object_mut().unwrap().remove("session_id");
    assert_eq!(action_session, rest_session);
    assert!(compact_body["output"].get("tool_manifest").is_none());
    assert!(compact_body["output"].get("recommended_flow").is_none());
    assert!(
        compact_bytes < 32 * 1024,
        "Actions-wrapped standard startup must remain below 32 KiB: {compact_bytes}"
    );
    // Both modes create real sessions (execution unchanged).
    assert_ne!(compact_session, standard_session);
    executor.abort();
}

#[tokio::test]
async fn http_start_coding_task_compact_mode_preserves_error_payload() {
    let _compact = ActionCompactEnvGuard::enabled();

    let config = test_config(Some("secret"));
    let (_tmp, db) = test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let (runtime, _registry) = register_import_agent(tmp_proj.path()).await;
    let service = Service::new(build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({
            "tool": "start_coding_task",
            "project": "agent:importer:does-not-exist",
            "detail": "minimal",
        }))
        .send(&service)
        .await;

    assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "compact mode must not drop error text: {body}"
    );
    assert!(
        body.get("output").is_some(),
        "error ToolResult still carries output field"
    );
    assert!(
        body["output"].get("compact").is_none(),
        "errors must not be rewritten into compact success shape"
    );
}

#[tokio::test]
async fn http_tools_call_compact_mode_does_not_change_list_tools() {
    let _compact = ActionCompactEnvGuard::enabled();

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
    assert!(body["output"].get("compact").is_none());
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
        TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_recorder",
    }))
    .unwrap();

    assert_eq!(tool, "git_status");
    assert_eq!(
        params,
        json!({"project": "agent:oe:webcodex", "session_id": "wc_sess_tool_arg"})
    );
    assert_eq!(
        extract_recording_session_id(
            &json!({TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_recorder"})
        ),
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
        TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_recorder"
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
            .is_some_and(|m| !m.contains_key(TOOL_CALL_RECORDING_SESSION_ID_FIELD)),
        "recording_session_id must not leak into concrete params"
    );
    assert_eq!(
        extract_recording_session_id(&body),
        Some("wc_sess_recorder".to_string())
    );
}

#[test]
fn extract_tool_call_collects_flattened_write_project_file_fields() {
    let (tool, params) = extract_tool_call(&json!({
        "tool": "write_project_file",
        "project": "agent:oe:webcodex",
        "path": "x.tmp",
        "content": "BETA\n",
        "overwrite": true,
    }))
    .unwrap();

    assert_eq!(tool, "write_project_file");
    assert_eq!(params["project"], "agent:oe:webcodex");
    assert_eq!(params["path"], "x.tmp");
    assert_eq!(params["content"], "BETA\n");
    assert_eq!(params["overwrite"], true);
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
        TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_record"
    });
    let (tool, params) = extract_tool_call(&body).unwrap();

    assert_eq!(tool, "workspace_checkpoint_restore");
    assert_eq!(params["project"], "agent:special:test");
    assert_eq!(params["checkpoint_id"], "wc_ckpt_abc");
    assert_eq!(params["confirm"], true);
    assert!(
        params
            .as_object()
            .is_some_and(|m| !m.contains_key(TOOL_CALL_RECORDING_SESSION_ID_FIELD)),
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
    // GPT Action flattened call for apply_text_edits: nested `changes`
    // array and scalar flattened fields must be collected into params.
    let (tool, params) = extract_tool_call(&json!({
        "tool": "apply_text_edits",
        "project": "agent:special:test",
        "dry_run": true,
        "changes": [{
            "kind": "edit",
            "path": "a.txt",
            "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "edits": [{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]
        }]
    }))
    .unwrap();

    assert_eq!(tool, "apply_text_edits");
    assert_eq!(params["project"], "agent:special:test");
    assert_eq!(params["dry_run"], true);
    let changes = params["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["kind"], "edit");
    assert_eq!(changes[0]["path"], "a.txt");
    assert_eq!(changes[0]["edits"][0]["kind"], "replace_exact");
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
async fn http_tools_call_run_codex_is_unknown_without_creating_job() {
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
    assert_eq!(body["status"], 400);
    let err = body["error"].as_str().unwrap();
    assert!(err.contains("unknown tool 'run_codex'"), "{err}");
    assert_eq!(
        err.matches("run_codex").count(),
        1,
        "unknown-tool error must not advertise removed run_codex: {err}"
    );

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
async fn http_tools_call_accepts_omitted_null_and_alias_params() {
    // `params` may be omitted or null, and `arguments` is accepted as a
    // compatibility alias for `params`.
    let (_tmp, service) = phase2_service();
    for body in [
        json!({"tool": "list_tools"}),
        json!({"tool": "list_tools", "params": null}),
        json!({"tool": "list_tools", "arguments": null}),
    ] {
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK, "body: {body}");
        let out: Value = resp.take_json().await.unwrap();
        assert_eq!(out["success"], true, "body: {body}");
        assert!(out["output"]["tools"].is_array(), "body: {body}");
    }
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
            TOOL_CALL_RECORDING_SESSION_ID_FIELD: session_id,
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
            TOOL_CALL_RECORDING_SESSION_ID_FIELD: session_id,
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
            TOOL_CALL_RECORDING_SESSION_ID_FIELD: tracking_session_id
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
            TOOL_CALL_RECORDING_SESSION_ID_FIELD: tracking_session_id,
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
                TOOL_CALL_RECORDING_SESSION_ID_FIELD: session_id,
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
async fn http_tools_call_generic_path_dispatches_git_tools() {
    // callRuntimeTool routes these tools to the runtime. With an unknown
    // agent project the runtime returns a structured error (not a 401/404),
    // proving the generic path deserializes + dispatches each tool.
    let (_tmp, service) = phase2_service();
    for (tool, params) in [
        ("git_diff_summary", json!({"project": "agent:nope:nope"})),
        (
            "git_log",
            json!({"project": "agent:nope:nope", "limit": 5, "skip": 1}),
        ),
        (
            "show_changes",
            json!({"project": "agent:nope:nope", "include_diff": false}),
        ),
    ] {
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": tool, "params": params}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST, "{tool}");
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false, "{tool}");
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{tool} should return a structured runtime error"
        );
    }
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
        let stdout = "## main\n?? README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nstatus_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=1\nfiles_returned=1\nfiles_truncated=0\nfiles_limit=200\nmodified=0\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=1\nconflicted=0\nstaged=0\nunstaged=0\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=20\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ncommit=abc123\nshort=abc123\nsummary=test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nhead_exit=0\nhead_truncated=0\nhead_bytes=44\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=0\n";
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
async fn oauth2_tools_call_requires_project_write_for_edit_tools() {
    let (_tmp, service, token) = phase2_oauth_service("project:write");
    let (status, body, _) = oauth_tools_call(
        &service,
        &token,
        "write_project_file",
        json!({"project": "demo", "path": "README.md", "content": "new"}),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

    let (_tmp, service, token) = phase2_oauth_service("project:read");
    let (status, body, challenge) = oauth_tools_call(
        &service,
        &token,
        "write_project_file",
        json!({"project": "demo", "path": "README.md", "content": "new"}),
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
    // `replace_in_file` was removed and `start_session` is ModelHidden: both
    // are withheld from the model-facing tools/list surface. Only the visible
    // canonical tools appear here.
    assert!(!names.iter().any(|n| n == "replace_in_file"));
    assert!(!names.iter().any(|n| n == "start_session"));
    assert!(!names.iter().any(|n| n == "bind_current_session"));
    assert!(names.iter().any(|n| n == "write_project_file"));
    assert_eq!(body["count"], names.len());
    let tools = body["tools"].as_array().unwrap();
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
    // callRuntimeTool routes write_project_file / apply_text_edits to the
    // runtime. With a non-agent project the runtime returns a structured
    // error (not a 401/404), proving the generic path dispatches them.
    let (_tmp, service) = phase2_service();
    for (tool, params) in [
        (
            "write_project_file",
            json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
        ),
        (
            "apply_text_edits",
            json!({
                "project": "agent:nope:nope",
                "changes": [{
                    "kind": "edit",
                    "path": "x.txt",
                    "expected_sha256": "a".repeat(64),
                    "edits": [{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]
                }]
            }),
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
