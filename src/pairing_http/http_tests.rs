use super::*;
use salvo::test::{ResponseExt, TestClient};

#[tokio::test]
async fn pairing_create_preserves_body_auth_and_validation_order() {
    let missing_auth = Service::new(Router::new().post(pairing_create));
    let mut auth = AuthContext::new(crate::auth::AuthKind::Bootstrap);
    auth.is_bootstrap = true;
    auth.role = Some("admin".to_string());
    let missing_db = Service::new(
        Router::new()
            .hoop(affix_state::inject(auth))
            .post(pairing_create),
    );
    for service in [&missing_auth, &missing_db] {
        let mut response = TestClient::post("http://localhost/")
            .json(&json!({}))
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
        let body: Value = response.take_json().await.unwrap();
        assert_eq!(body["status"], 400);
        assert!(body["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid request body: "));
        assert_eq!(body.as_object().unwrap().len(), 2);
    }
    for (service, body, status, message) in [
        (
            &missing_auth,
            json!({"username": "alice"}),
            StatusCode::INTERNAL_SERVER_ERROR,
            "no auth context",
        ),
        (
            &missing_db,
            json!({"username": "alice", "ttl_secs": 1}),
            StatusCode::BAD_REQUEST,
            "ttl_secs must be between 60 and 3600",
        ),
        (
            &missing_db,
            json!({"username": "alice"}),
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ),
    ] {
        let mut response = TestClient::post("http://localhost/")
            .json(&body)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(status));
        assert_eq!(
            response.take_json::<Value>().await.unwrap(),
            json!({
                "status": status.as_u16(), "error": message,
            })
        );
    }
}

#[tokio::test]
async fn pairing_enroll_keeps_input_validation_before_database_access() {
    let service = Service::new(Router::new().post(pairing_enroll));
    for (body, status, message) in [
        (
            json!({"pairing_code": " ", "client_id": "runner-one"}),
            StatusCode::BAD_REQUEST,
            "pairing_code is required",
        ),
        (
            json!({"pairing_code": "wc_pair_test", "client_id": "runner-one", "transport": "invalid"}),
            StatusCode::BAD_REQUEST,
            "transport must be websocket, polling, quic, or auto",
        ),
        (
            json!({"pairing_code": "wc_pair_test", "client_id": "runner-one"}),
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB not available",
        ),
    ] {
        let mut response = TestClient::post("http://localhost/")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(status));
        assert_eq!(
            response.take_json::<Value>().await.unwrap(),
            json!({
                "status": status.as_u16(), "error": message,
            })
        );
    }
}
