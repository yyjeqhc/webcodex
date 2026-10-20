use crate::runner_tokens_http::*;
use crate::users_http::*;
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use serde_json::{json, Value};

fn routes() -> Router {
    Router::new()
        .push(Router::with_path("api/users/create").post(users_create))
        .push(Router::with_path("api/users/list").post(users_list))
        .push(Router::with_path("api/users/me").post(users_me))
        .push(Router::with_path("api/tokens/create").post(tokens_create))
        .push(Router::with_path("api/tokens/register_hash").post(tokens_register_hash))
        .push(Router::with_path("api/tokens/list").post(tokens_list))
        .push(Router::with_path("api/tokens/revoke").post(tokens_revoke))
        .push(Router::with_path("api/agent-tokens/create").post(runner_tokens_create))
        .push(Router::with_path("api/agent-tokens/register_hash").post(runner_tokens_register_hash))
        .push(Router::with_path("api/agent-tokens/list").post(runner_tokens_list))
        .push(Router::with_path("api/agent-tokens/revoke").post(runner_tokens_revoke))
}

#[tokio::test]
async fn management_routes_keep_json_errors_and_validation_order() {
    let missing_auth = Service::new(routes());
    let missing_db = Service::new(
        Router::new()
            .hoop(affix_state::inject(crate::test_support::test_config(Some(
                "secret",
            ))))
            .hoop(crate::AuthMiddleware)
            .push(routes()),
    );
    for (path, body) in [
        ("users/create", json!({"username": "alice"})),
        ("users/list", json!({})),
        ("users/me", json!({})),
        ("tokens/create", json!({"username": "alice"})),
        (
            "tokens/register_hash",
            json!({
                "username": "alice", "token_hash": "a".repeat(64), "token_prefix": "wc_pat_1234567890",
            }),
        ),
        ("tokens/list", json!({"username": "alice"})),
        (
            "tokens/revoke",
            json!({"username": "alice", "token_id": "test-token"}),
        ),
        (
            "agent-tokens/create",
            json!({"username": "alice", "client_id": "runner-one"}),
        ),
        (
            "agent-tokens/register_hash",
            json!({
                "username": "alice", "client_id": "runner-one", "name": "runner-one",
                "token_hash": "b".repeat(64), "token_prefix": "wc_agent_1234567",
            }),
        ),
        ("agent-tokens/list", json!({"username": "alice"})),
        (
            "agent-tokens/revoke",
            json!({"username": "alice", "token_id": "test-token"}),
        ),
    ] {
        let url = format!("http://localhost/api/{path}");
        for (service, message) in [
            (&missing_auth, "no auth context"),
            (&missing_db, "DB not available"),
        ] {
            let mut response = TestClient::post(&url)
                .bearer_auth("secret")
                .json(&body)
                .send(service)
                .await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::INTERNAL_SERVER_ERROR),
                "{path}"
            );
            assert!(
                response.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with("application/json"),
                "{path}"
            );
            assert_eq!(
                response.take_json::<Value>().await.unwrap(),
                json!({"status": 500, "error": message}),
                "{path}"
            );
        }
        if !body.as_object().unwrap().is_empty() {
            // Body validation precedes handler context lookup on all JSON-input routes.
            let mut response = TestClient::post(&url)
                .json(&json!({}))
                .send(&missing_auth)
                .await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::BAD_REQUEST),
                "{path}"
            );
            let error: Value = response.take_json().await.unwrap();
            assert_eq!(error["status"], 400, "{path}");
            assert!(
                error["error"]
                    .as_str()
                    .unwrap()
                    .starts_with("invalid request body: "),
                "{path}"
            );
            assert_eq!(error.as_object().unwrap().len(), 2, "{path}");
        }
    }
}
