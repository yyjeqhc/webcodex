use super::super::import_http::{set_import_test_download_base_url, MAX_IMPORT_FILE_BYTES};
use base64::{engine::general_purpose, Engine as _};
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

static IMPORT_HTTP_TEST_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

async fn lock_import_http_test() -> tokio::sync::MutexGuard<'static, ()> {
    IMPORT_HTTP_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

struct ImportDownloadBaseUrlGuard;

impl ImportDownloadBaseUrlGuard {
    fn set(base_url: String) -> Self {
        set_import_test_download_base_url(Some(base_url));
        Self
    }
}

impl Drop for ImportDownloadBaseUrlGuard {
    fn drop(&mut self) {
        set_import_test_download_base_url(None);
    }
}

struct MockHttpServer {
    base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn start_mock_http_server(responses: Vec<Vec<u8>>) -> MockHttpServer {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let mut responses = std::collections::VecDeque::from(responses);
        while let Some(response) = responses.pop_front() {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0_u8; 4096];
            let _ = stream.read(&mut buf).await;
            let _ = stream.write_all(&response).await;
            let _ = stream.shutdown().await;
        }
    });
    MockHttpServer {
        base_url: format!("http://{}", addr),
        handle,
    }
}

fn http_response(status: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {}\r\n", status).into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

fn import_body(download_link: &str, mime_type: &str, name: &str) -> Value {
    json!({"project":"agent:importer:demo","output_dir":"docs/assets","openaiFileIdRefs":[{"name":name,"id":"file_mock","mime_type":mime_type,"download_link":download_link}]})
}

async fn import_test_service_with_local_runtime() -> Service {
    let config = super::test_config(Some("secret"));
    let (_tmp, db) = super::test_db();
    let tmp_proj = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(tmp_proj.path(), "demo"));
    Service::new(super::build_projects_router(config, db, runtime))
}

async fn complete_one_save_artifact_request(
    registry: Arc<crate::shell_client::ShellClientRegistry>,
) {
    use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
    use sha2::{Digest, Sha256};
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
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    let path = payload["path"].as_str().unwrap().to_string();
    let mime_type = payload["mime_type"].as_str().unwrap().to_string();
    let bytes = general_purpose::STANDARD
        .decode(payload["content_base64"].as_str().unwrap())
        .unwrap();
    let full_path = std::path::Path::new(request.cwd.as_deref().unwrap()).join(&path);
    std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    std::fs::write(&full_path, &bytes).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let stdout =
        json!({"path":path,"bytes_written":bytes.len(),"sha256":sha256,"mime_type":mime_type})
            .to_string();
    registry
        .complete(ShellAgentResultRequest {
            client_id: "importer".to_string(),
            agent_instance_id: "inst-import".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: None,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn import_http_rejects_http_download_link() {
    let service = import_test_service_with_local_runtime().await;
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&import_body(
            "http://files.oaiusercontent.com/a.png",
            "image/png",
            "a.png",
        ))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        salvo::http::StatusCode::BAD_REQUEST
    );
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("https"));
}

#[tokio::test]
async fn import_http_rejects_non_openai_file_host() {
    let service = import_test_service_with_local_runtime().await;
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&import_body(
            "https://example.com/a.png",
            "image/png",
            "a.png",
        ))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        salvo::http::StatusCode::BAD_REQUEST
    );
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("OpenAI file host"));
}

// These ignored tests are a serial, loopback-only integration lane for import
// downloader behavior. They remain manual/slow and are not part of default
// runtime_http test execution.

#[tokio::test]
#[ignore]
async fn import_http_does_not_follow_302_redirect() {
    let _guard = lock_import_http_test().await;
    let server = start_mock_http_server(vec![http_response(
        "302 Found",
        &[(
            "Location",
            "https://files.oaiusercontent.com/other.png".to_string(),
        )],
        b"",
    )])
    .await;
    let _download_base = ImportDownloadBaseUrlGuard::set(server.base_url.clone());
    let service = import_test_service_with_local_runtime().await;
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&import_body(
            "https://files.oaiusercontent.com/a.png",
            "image/png",
            "a.png",
        ))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        salvo::http::StatusCode::BAD_REQUEST
    );
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("HTTP 302"));
}

#[tokio::test]
#[ignore]
async fn import_http_rejects_content_length_over_limit() {
    let _guard = lock_import_http_test().await;
    let server = start_mock_http_server(vec![http_response(
        "200 OK",
        &[("Content-Length", (MAX_IMPORT_FILE_BYTES + 1).to_string())],
        b"",
    )])
    .await;
    let _download_base = ImportDownloadBaseUrlGuard::set(server.base_url.clone());
    let service = import_test_service_with_local_runtime().await;
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&import_body(
            "https://files.oaiusercontent.com/a.png",
            "image/png",
            "a.png",
        ))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        salvo::http::StatusCode::BAD_REQUEST
    );
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("exceeds"));
}

#[tokio::test]
#[ignore]
async fn import_http_rejects_chunked_body_after_limit_without_content_length() {
    let _guard = lock_import_http_test().await;
    let body = vec![b'x'; MAX_IMPORT_FILE_BYTES + 1];
    let server = start_mock_http_server(vec![http_response("200 OK", &[], &body)]).await;
    let _download_base = ImportDownloadBaseUrlGuard::set(server.base_url.clone());
    let service = import_test_service_with_local_runtime().await;
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&import_body(
            "https://files.oaiusercontent.com/a.png",
            "image/png",
            "a.png",
        ))
        .send(&service)
        .await;
    assert_eq!(
        super::effective_status(&resp),
        salvo::http::StatusCode::BAD_REQUEST
    );
    let body: Value = resp.take_json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("exceeds"));
}

#[tokio::test]
#[ignore]
async fn import_http_success_uses_source_name_fallback_for_missing_target() {
    let _guard = lock_import_http_test().await;
    let png = vec![0x89, b'P', b'N', b'G'];
    let webp = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
    let server = start_mock_http_server(vec![
        http_response("200 OK", &[("Content-Length", png.len().to_string())], &png),
        http_response(
            "200 OK",
            &[("Content-Length", webp.len().to_string())],
            &webp,
        ),
    ])
    .await;
    let _download_base = ImportDownloadBaseUrlGuard::set(server.base_url.clone());
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = super::register_import_agent(tmp.path()).await;
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let service = Service::new(super::build_projects_router(config, db, runtime));
    let agent1 = tokio::spawn(complete_one_save_artifact_request(registry.clone()));
    let agent2 = tokio::spawn(complete_one_save_artifact_request(registry));
    let mut resp = TestClient::post("http://localhost/api/artifacts/import")
        .bearer_auth("secret")
        .json(&json!({
            "project":"agent:importer:demo",
            "output_dir":"docs/assets",
            "targets":["custom.png"],
            "openaiFileIdRefs":[
                {"name":"generated.png","id":"file_png","mime_type":"image/png","download_link":"https://files.oaiusercontent.com/generated.png"},
                {"name":"fallback.webp","id":"file_webp","mime_type":"image/webp","download_link":"https://files.oaiusercontent.com/fallback.webp"}
            ]
        }))
        .send(&service)
        .await;
    agent1.await.unwrap();
    agent2.await.unwrap();
    assert_eq!(super::effective_status(&resp), salvo::http::StatusCode::OK);
    let body: Value = resp.take_json().await.unwrap();
    let imported = body["output"]["imported"].as_array().unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(imported[0]["path"], "docs/assets/custom.png");
    assert_eq!(imported[0]["bytes_written"], png.len());
    assert_eq!(imported[0]["mime_type"], "image/png");
    assert_eq!(imported[0]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(imported[1]["path"], "docs/assets/fallback.webp");
    assert_eq!(imported[1]["bytes_written"], webp.len());
    assert_eq!(imported[1]["mime_type"], "image/webp");
    assert_eq!(imported[1]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        std::fs::read(tmp.path().join("docs/assets/custom.png")).unwrap(),
        png
    );
    assert_eq!(
        std::fs::read(tmp.path().join("docs/assets/fallback.webp")).unwrap(),
        webp
    );
}
