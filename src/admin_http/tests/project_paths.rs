use super::*;

async fn project_path_call(service: &Service, operation: &str, path: &str) -> (StatusCode, Value) {
    let mut response = TestClient::post(format!("http://127.0.0.1/admin/projects/{operation}"))
        .add_header("host", "127.0.0.1", true)
        .add_header("origin", "http://127.0.0.1", true)
        .add_header("content-type", "application/json", true)
        .body(
            json!({
                "client_id": "runner",
                "project_id": "demo",
                "name": "Demo",
                "path": path,
                "idempotency_key": "path-validation"
            })
            .to_string(),
        )
        .send(service)
        .await;
    let status = response.status_code.unwrap();
    (status, response.take_json().await.unwrap())
}

#[tokio::test]
async fn admin_project_paths_accept_cross_platform_absolute_shapes() {
    let mut admin = AuthContext::new(AuthKind::ApiToken);
    admin.scopes.push(SCOPE_ADMIN.to_string());
    let service = service(Some(admin));
    let max_length_path = format!("D:/{}", "a".repeat(4093));
    for operation in ["register", "create"] {
        for path in [
            "/srv/projects/demo",
            r"D:\projects\demo",
            "d:/projects/demo",
            r"\\server\share\demo",
            r"\\?\D:\projects\demo",
            r"\\?\UNC\server\share\demo",
            "//?/D:/projects/demo",
            &max_length_path,
        ] {
            let (status, body) = project_path_call(&service, operation, path).await;
            // With no Runner connected, a valid request must reach Runner lookup.
            // Acceptance here does not assert that its filesystem policy allows it.
            assert_eq!(
                status,
                StatusCode::SERVICE_UNAVAILABLE,
                "{operation}: {path:?}: {body}"
            );
            assert_eq!(body, json!({"error": {"code": "agent_unavailable"}}));
        }
    }
}

#[tokio::test]
async fn admin_project_paths_reject_relative_nul_and_oversized_inputs() {
    let mut admin = AuthContext::new(AuthKind::ApiToken);
    admin.scopes.push(SCOPE_ADMIN.to_string());
    let service = service(Some(admin));
    let oversized_path = format!("D:/{}", "a".repeat(4094));
    for operation in ["register", "create"] {
        for path in [
            "",
            "relative/path",
            r"relative\path",
            "D:",
            r"D:projects\demo",
            r"\projects\demo",
            "/srv/projects/\0demo",
            "D:\\projects\\\0demo",
            &oversized_path,
        ] {
            let (status, body) = project_path_call(&service, operation, path).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{operation}: {path:?}: {body}"
            );
            assert_eq!(body, json!({"error": {"code": "invalid_request"}}));
        }
    }
}
