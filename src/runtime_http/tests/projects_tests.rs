use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use serde_json::{json, Value};
use std::sync::Arc;

// =========================================================================
// listProjects
// =========================================================================

#[tokio::test]
async fn http_projects_list_rejects_wrong_bearer() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let resp = TestClient::post("http://localhost/api/projects/list")
        .bearer_auth("wrong")
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn http_projects_list_ignores_server_configured_projects() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/list")
        .bearer_auth("secret")
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    let list = body["output"]["projects"]
        .as_array()
        .expect("output.projects is a project array");
    assert_eq!(body["output"]["count"], 0);
    assert!(
        list.is_empty(),
        "runtime project discovery is agent-registered only"
    );
}

// =========================================================================
// register_project / create_project REST endpoints
// =========================================================================

#[tokio::test]
async fn http_projects_register_rejects_unknown_client_id() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/register")
        .bearer_auth("secret")
        .json(&json!({
            "client_id": "no-such-agent",
            "id": "my-project",
            "name": "My Project",
            "path": "/root/git/my-project"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown agent")),
        "register_project should reject unknown client_id: {:?}",
        body["error"]
    );
}

#[tokio::test]
async fn http_projects_create_rejects_unknown_client_id() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/create")
        .bearer_auth("secret")
        .json(&json!({
            "client_id": "no-such-agent",
            "id": "hello",
            "name": "Hello",
            "path": "/root/git/hello"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown agent")),
        "create_project should reject unknown client_id: {:?}",
        body["error"]
    );
}

#[tokio::test]
async fn http_projects_register_rejects_unsafe_id() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/register")
        .bearer_auth("secret")
        .json(&json!({
            "client_id": "oe",
            "id": "a/b",
            "name": "Test",
            "path": "/root/git/test"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn http_projects_create_rejects_relative_path() {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    let service = Service::new(super::build_projects_router(config, db, runtime));

    let mut resp = TestClient::post("http://localhost/api/projects/create")
        .bearer_auth("secret")
        .json(&json!({
            "client_id": "oe",
            "id": "hello",
            "name": "Hello",
            "path": "relative/path"
        }))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&resp), StatusCode::BAD_REQUEST);
    let body: Value = resp.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("absolute")),
        "create_project should reject relative path: {:?}",
        body["error"]
    );
}
