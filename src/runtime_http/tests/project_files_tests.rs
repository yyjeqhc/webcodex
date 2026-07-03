use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use serde_json::{json, Value};
use std::sync::Arc;

// =========================================================================
// readProjectFile
// =========================================================================

#[tokio::test]
async fn http_projects_read_file_rejects_server_configured_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    std::fs::write(tmp_proj.path().join("README.md"), "line1\nline2\n").unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/read_file")
        .bearer_auth("secret")
        .json(&json!({"project": "demo", "path": "README.md"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("unknown_project"));
}

#[tokio::test]
async fn http_projects_read_file_rejects_unknown_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/read_file")
        .bearer_auth("secret")
        .json(&json!({"project": "nope", "path": "README.md"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("nope"));
}

#[tokio::test]
async fn dedicated_read_project_file_with_session_id_records_event() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let mut caps = crate::shell_protocol::ShellClientCapabilities::default();
    caps.file_read = true;
    let (runtime, registry) =
        super::register_import_agent_with_capabilities(tmp_proj.path(), Some(caps)).await;
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({
            "tool": "start_session",
            "params": {"project": "agent:importer:demo", "title": "dedicated read"}
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::OK);
    let start_body: Value = resp.take_json().await.unwrap();
    let session_id = start_body["output"]["session_id"].as_str().unwrap();

    let request = async {
        TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({
                "project": "agent:importer:demo",
                "path": "README.md",
                "session_id": session_id,
                "limit": 1
            }))
            .send(&service)
            .await
    };
    let complete = super::complete_one_agent_request(registry.clone(), "secret read body\n", "", 0);
    let (mut resp, _) = tokio::join!(request, complete);
    assert_eq!(super::effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["output"]["session_recorded"], true);

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({"tool": "session_summary", "params": {"session_id": session_id}}))
        .send(&service)
        .await;
    let summary: Value = resp.take_json().await.unwrap();
    assert_eq!(summary["output"]["counts"]["tool_calls"], 1);
    assert!(summary["output"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["tool_name"] == "read_file" && event["status"] == "succeeded"));
    let serialized = serde_json::to_string(&summary["output"]["events"]).unwrap();
    assert!(
        !serialized.contains("secret read body"),
        "session event leaked read_file content: {serialized}"
    );
}

#[tokio::test]
async fn dedicated_read_project_file_without_session_id_remains_compatible() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let mut caps = crate::shell_protocol::ShellClientCapabilities::default();
    caps.file_read = true;
    let (runtime, registry) =
        super::register_import_agent_with_capabilities(tmp_proj.path(), Some(caps)).await;
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let request = async {
        TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({"project": "agent:importer:demo", "path": "README.md"}))
            .send(&service)
            .await
    };
    let complete = super::complete_one_agent_request(registry.clone(), "hello\n", "", 0);
    let (mut resp, _) = tokio::join!(request, complete);
    assert_eq!(super::effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["output"]["content"], "hello");
    assert!(body["output"].get("session_recorded").is_none());
}

// =========================================================================
// getProjectGitStatus
// =========================================================================

#[tokio::test]
async fn http_projects_git_status_rejects_server_configured_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    // Initialize a real git repo so `git status --porcelain` succeeds.
    let root = tmp_proj.path();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .expect("git init");
    std::fs::write(root.join("tracked.txt"), "a").unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(root, "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/git_status")
        .bearer_auth("secret")
        .json(&json!({"project": "demo"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("unknown_project"));
}

// =========================================================================
// getProjectGitDiff
// =========================================================================

#[tokio::test]
async fn http_projects_git_diff_rejects_server_configured_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let root = tmp_proj.path();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .expect("git init");
    let runtime = Arc::new(super::runtime_with_local_project(root, "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/git_diff")
        .bearer_auth("secret")
        .json(&json!({"project": "demo"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("unknown_project"));
}

// =========================================================================
// applyProjectPatch
// =========================================================================

#[tokio::test]
async fn http_projects_apply_patch_rejects_server_configured_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/apply_patch")
        .bearer_auth("secret")
        .json(&json!({"project": "demo", "patch": "diff"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("unknown_project"));
}

// =========================================================================
// Phase A read-only console REST wrappers (wiring + auth gate)
// =========================================================================

#[tokio::test]
async fn http_console_routes_require_bearer_auth() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    for (path, body) in [
        ("/api/projects/list_files", json!({"project": "demo"})),
        (
            "/api/projects/search_text",
            json!({"project": "demo", "pattern": "fn"}),
        ),
        ("/api/projects/git_diff_summary", json!({"project": "demo"})),
    ] {
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            super::effective_status(&resp),
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
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/list_files")
        .bearer_auth("secret")
        .json(&json!({"project": "agent:nope:nope"}))
        .send(&service)
        .await;
    // Authenticated and dispatched to ToolRuntime: a structured error
    // (BAD_REQUEST + success=false), not a 401/404.
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "list_files should return a structured runtime error"
    );
}

// =========================================================================
// validateProjectPatch (POST /api/projects/validate_patch)
// =========================================================================

#[tokio::test]
async fn http_projects_validate_patch_dispatches_to_runtime() {
    // With a correct bearer token the route reaches the runtime. The
    // project id below is not agent-registered, so the runtime returns a
    // structured error (not a 401/404) — proving the request was
    // authenticated, deserialized, and dispatched to ToolRuntime.
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/validate_patch")
        .bearer_auth("secret")
        .json(&json!({
            "project": "agent:nope:nope",
            "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "validate_patch should return a structured runtime error"
    );
}

#[tokio::test]
async fn http_projects_validate_patch_rejects_empty_patch_via_runtime() {
    // An empty patch is rejected by the runtime with a structured error
    // (BAD_REQUEST + success=false), not a 401/404. This proves the
    // wrapper deserializes and dispatches even for invalid patches.
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/validate_patch")
        .bearer_auth("secret")
        .json(&json!({"project": "agent:nope:nope", "patch": ""}))
        .send(&service)
        .await;
    // Empty patch is rejected; because the project is not agent-registered
    // authorize_agent_tool fails first, but the request is still
    // authenticated + dispatched (structured error, not 401/404).
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
}

// =========================================================================
// Phase 3: dedicated mutation actions (apply_patch_checked, delete_files,
// git_restore_paths, discard_untracked) — auth gate + dispatch wiring
// =========================================================================

#[tokio::test]
async fn http_phase3_mutation_actions_require_bearer_auth() {
    let (_tmp, service) = super::phase2_service();
    for (path, body) in [
        (
            "/api/projects/apply_patch_checked",
            json!({"project": "demo", "patch": "diff"}),
        ),
        (
            "/api/projects/delete_files",
            json!({"project": "demo", "paths": ["x.txt"]}),
        ),
        (
            "/api/projects/git_restore_paths",
            json!({"project": "demo", "paths": ["x.txt"]}),
        ),
        (
            "/api/projects/discard_untracked",
            json!({"project": "demo", "paths": ["x.txt"]}),
        ),
    ] {
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            super::effective_status(&resp),
            StatusCode::UNAUTHORIZED,
            "{} should require auth",
            path
        );
    }
}

#[tokio::test]
async fn http_phase3_mutation_actions_dispatch_to_runtime() {
    // With a correct bearer token the mutation routes reach the runtime.
    // The project id is not agent-registered, so the runtime returns a
    // structured error (not a 401/404) — proving the request was
    // authenticated, deserialized, and dispatched to ToolRuntime.
    let (_tmp, service) = super::phase2_service();
    for (path, body) in [
        (
            "/api/projects/apply_patch_checked",
            json!({"project": "agent:nope:nope", "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"}),
        ),
        (
            "/api/projects/delete_files",
            json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
        ),
        (
            "/api/projects/git_restore_paths",
            json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
        ),
        (
            "/api/projects/discard_untracked",
            json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
        ),
    ] {
        let mut resp = TestClient::post(&format!("http://localhost{}", path))
            .bearer_auth("secret")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            super::effective_status(&resp),
            StatusCode::BAD_REQUEST,
            "{} should reach runtime and return structured error",
            path
        );
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{} should return a structured runtime error",
            path
        );
    }
}

// =========================================================================
// Phase 4/5: structured-edit endpoints — auth gate + dispatch wiring.
// replace_in_file is now also a dedicated GPT Action; write_file remains
// runtime-only. Both are still reachable via callRuntimeTool / MCP.
// =========================================================================

#[tokio::test]
async fn http_phase4_edit_endpoints_require_bearer_auth() {
    let (_tmp, service) = super::phase2_service();
    for (path, body) in [
        (
            "/api/projects/replace_in_file",
            json!({"project": "demo", "path": "x.txt", "old": "a", "new": "b"}),
        ),
        (
            "/api/projects/write_file",
            json!({"project": "demo", "path": "x.txt", "content": "a"}),
        ),
    ] {
        let resp = TestClient::post(&format!("http://localhost{}", path))
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            super::effective_status(&resp),
            StatusCode::UNAUTHORIZED,
            "{} should require auth",
            path
        );
    }
}

#[tokio::test]
async fn http_phase4_edit_endpoints_dispatch_to_runtime() {
    // With a correct bearer token the edit routes reach the runtime. The
    // project id is not agent-registered, so the runtime returns a
    // structured error (not a 401/404) — proving the request was
    // authenticated, deserialized, and dispatched to ToolRuntime.
    let (_tmp, service) = super::phase2_service();
    for (path, body, tool) in [
        (
            "/api/projects/replace_in_file",
            json!({"project": "agent:nope:nope", "path": "x.txt", "old": "a", "new": "b"}),
            "replace_in_file",
        ),
        (
            "/api/projects/write_file",
            json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
            "write_project_file",
        ),
    ] {
        let mut resp = TestClient::post(&format!("http://localhost{}", path))
            .bearer_auth("secret")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(
            super::effective_status(&resp),
            StatusCode::BAD_REQUEST,
            "{} should reach runtime and return structured error",
            path
        );
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{} should return a structured runtime error",
            tool
        );
    }
}

// =========================================================================
// Compatibility write_project_file REST wrapper - auth gate + dispatch wiring.
// write_file remains reachable directly for non-GPT clients and through
// callRuntimeTool / MCP.
// =========================================================================

#[tokio::test]
async fn http_compat_write_file_requires_bearer_auth() {
    let (_tmp, service) = super::phase2_service();
    let resp = TestClient::post("http://localhost/api/projects/write_file")
        .json(&json!({"project": "demo", "path": "x.txt", "content": "a"}))
        .send(&service)
        .await;

    assert_eq!(super::effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_compat_write_file_dispatches_to_runtime() {
    // With a correct bearer token the dedicated route reaches the runtime.
    // The project id is not agent-registered, so the runtime returns a
    // structured error (not a 401/404) — proving the request was
    // authenticated, deserialized, and dispatched to ToolRuntime.
    let (_tmp, service) = super::phase2_service();
    let mut resp = TestClient::post("http://localhost/api/projects/write_file")
        .bearer_auth("secret")
        .json(&json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        StatusCode::BAD_REQUEST,
        "write_file should reach runtime and return structured error",
    );
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "write_file should return a structured runtime error"
    );
}

#[tokio::test]
async fn compat_write_project_file_unknown_session_fails_before_mutation() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let caps = crate::shell_protocol::ShellClientCapabilities::default();
    let (runtime, registry) =
        super::register_import_agent_with_capabilities(tmp_proj.path(), Some(caps)).await;
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/write_file")
        .bearer_auth("secret")
        .json(&json!({
            "project": "agent:importer:demo",
            "path": "should-not-exist.txt",
            "content": "nope",
            "session_id": "wc_sess_missing"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert_eq!(body["output"]["error_kind"], "unknown_session_id");
    assert_eq!(body["output"]["session_id"], "wc_sess_missing");
    assert!(
        !tmp_proj.path().join("should-not-exist.txt").exists(),
        "write_file must fail before mutating for unknown session_id"
    );
    let polled = registry
        .poll(crate::shell_protocol::ShellAgentPollRequest {
            client_id: "importer".to_string(),
            agent_instance_id: "inst-import".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        polled.is_none(),
        "unknown session_id should fail before enqueueing an agent write"
    );
}
