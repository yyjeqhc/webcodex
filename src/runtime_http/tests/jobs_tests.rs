use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use serde_json::{json, Value};
use std::sync::Arc;

// =========================================================================
// runProjectShellCommand
// =========================================================================

#[tokio::test]
async fn http_projects_run_shell_requires_bearer_auth() {
    let (_tmp, service) = super::phase2_service();
    let resp = TestClient::post("http://localhost/api/projects/run_shell")
        .json(&json!({"project": "demo", "command": "echo hi"}))
        .send(&service)
        .await;

    assert_eq!(super::effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_projects_run_shell_rejects_server_configured_project() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/run_shell")
        .bearer_auth("secret")
        .json(&json!({"project": "demo", "command": "echo hi"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("unknown_project"));
}

#[tokio::test]
async fn dedicated_run_shell_with_session_id_records_event() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let caps = crate::shell_protocol::ShellClientCapabilities::default();
    let (runtime, registry) =
        super::register_import_agent_with_capabilities(tmp_proj.path(), Some(caps)).await;
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({"tool": "start_session", "params": {"project": "agent:importer:demo"}}))
        .send(&service)
        .await;
    let start_body: Value = resp.take_json().await.unwrap();
    let session_id = start_body["output"]["session_id"].as_str().unwrap();

    let request = async {
        TestClient::post("http://localhost/api/projects/run_shell")
            .bearer_auth("secret")
            .json(&json!({
                "project": "agent:importer:demo",
                "command": "echo hi",
                "session_id": session_id
            }))
            .send(&service)
            .await
    };
    let complete = super::complete_one_agent_request(registry.clone(), "hi\n", "", 0);
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
    assert_eq!(summary["output"]["counts"]["shell_like"], 1);
    assert!(summary["output"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["tool_name"] == "run_shell"
            && event["status"] == "succeeded"
            && event["exit_code"] == 0));
}

// =========================================================================
// startProjectShellJob
// =========================================================================

#[tokio::test]
async fn http_projects_run_job_requires_bearer_auth() {
    let (_tmp, service) = super::phase2_service();
    let resp = TestClient::post("http://localhost/api/projects/run_job")
        .json(&json!({"project": "demo", "command": "echo hi"}))
        .send(&service)
        .await;

    assert_eq!(super::effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_projects_run_job_dispatches_to_runtime() {
    let (_tmp, service) = super::phase2_service();
    let mut resp = TestClient::post("http://localhost/api/projects/run_job")
        .bearer_auth("secret")
        .json(&json!({"project": "agent:nope:nope", "command": "echo hi"}))
        .send(&service)
        .await;

    assert_eq!(
        super::effective_status(&resp),
        StatusCode::BAD_REQUEST,
        "run_job should reach runtime and return structured error"
    );
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "run_job should return a structured runtime error"
    );
}

// =========================================================================
// Runtime job list/tail routes
// =========================================================================

#[tokio::test]
async fn http_jobs_routes_require_bearer_auth() {
    let (_tmp, service) = super::phase2_service();

    for (path, body) in [
        ("/api/jobs/list", json!({})),
        ("/api/jobs/tail", json!({"job_id": "abc"})),
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
async fn http_jobs_list_accepts_correct_bearer_and_routes_to_runtime() {
    let (_tmp, service) = super::phase2_service();
    let mut resp = TestClient::post("http://localhost/api/jobs/list")
        .bearer_auth("secret")
        .json(&json!({}))
        .send(&service)
        .await;

    assert_eq!(super::effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["output"]["jobs"].is_array());
}
