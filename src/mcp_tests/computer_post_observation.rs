//! Real HTTP adapters and runtime governance; only Runner/native I/O is simulated.
use super::*;

const CLIENT: &str = "post-observation-http";
const INSTANCE: &str = "instance";

fn registration(read: bool) -> RunnerRegisterRequest {
    let mut request = crate::test_support::runner_registration(
        CLIENT,
        INSTANCE,
        RunnerCapabilities {
            computer_control: true,
            computer_text_input: true,
            computer_accessibility_observe: read,
            ..Default::default()
        },
    );
    request.owner = Some("alice".into());
    request
}

async fn setup(scopes: &str, read: bool) -> (tempfile::TempDir, Service, Arc<ToolRuntime>, String) {
    let config = test_config_oauth2(Some("secret"));
    let (tmp, db) = test_db();
    let user = seed_user(&db, "alice");
    let client = seed_oauth_client(&db, &user);
    let token = seed_oauth_access_token(&db, &client, &user, scopes);
    let runtime = Arc::new(test_runtime_with_mcp_settings(false, false));
    runtime
        .runner_registry
        .register(registration(read))
        .await
        .unwrap();
    let router = build_test_router(config, db, runtime.clone()).push(
        Router::with_path("api/tools/call")
            .hoop(crate::AuthMiddleware)
            .post(crate::runtime_http::tools_call),
    );
    (tmp, Service::new(router), runtime, token)
}

fn arguments(action: &str) -> Value {
    let mut value = json!({"action":action,"client_id":CLIENT,
        "surface_id":"surface_test","element_id":"element_test"});
    if action == "input_text" {
        value["text"] = json!("hello");
    }
    value
}

async fn invoke(service: &Service, token: &str, mcp: bool, args: Value) -> (StatusCode, Value) {
    if mcp {
        let (status, body, _) = oauth_mcp_request(
            service,
            token,
            "tools/call",
            mcp_2026_params(adaptive_runtime_gateway_params("control_computer", args)),
        )
        .await;
        (status, body)
    } else {
        let mut response = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth(token)
            .json(&json!({"tool":"control_computer","params":args}))
            .send(service)
            .await;
        (
            effective_status(&response),
            response.take_json().await.unwrap(),
        )
    }
}

async fn poll(runtime: &ToolRuntime) -> Option<crate::runner_protocol::RunnerRequest> {
    runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: CLIENT.into(),
            runner_instance_id: INSTANCE.into(),
        })
        .await
        .unwrap()
}

/// Admission happens after the asynchronous auth hop, so the runner's first
/// poll may legitimately race it. Wait (bounded) for the single admitted
/// action instead of assuming it is queued synchronously.
async fn wait_for_admitted_action(runtime: &ToolRuntime) -> crate::runner_protocol::RunnerRequest {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(request) = poll(runtime).await {
                return request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("timed out waiting for the admitted computer action")
}

async fn finish_action(runtime: &ToolRuntime, action: &str, request_id: String) {
    let mut output = json!({"platform":"windows","surface_id":"surface_test",
        "element_id":"element_test","success":true});
    if action == "press" {
        output["action"] = json!("press");
    } else {
        output["text_bytes"] = json!(5);
    }
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: CLIENT.into(),
            runner_instance_id: INSTANCE.into(),
            request_id,
            exit_code: Some(0),
            stdout: Some(output.to_string()),
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn computer_post_observation_public_adapters_require_both_scopes_before_dispatch() {
    for mcp in [false, true] {
        for (scopes, missing) in [
            ("runtime:read computer:control", "computer:read"),
            ("runtime:read computer:read", "computer:control"),
        ] {
            let (_tmp, service, runtime, token) = setup(scopes, true).await;
            for action in ["press", "input_text"] {
                let mut args = arguments(action);
                args["observe_after"] = json!({"action":"accessibility_tree"});
                let (status, body) = invoke(&service, &token, mcp, args).await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
                assert_eq!(body["error"], "insufficient_scope", "{body}");
                assert!(
                    body["error_description"]
                        .as_str()
                        .unwrap()
                        .contains(missing),
                    "{body}"
                );
                assert!(poll(&runtime).await.is_none());
            }
        }
    }
}

#[tokio::test]
async fn computer_post_observation_public_adapters_preserve_legacy_and_completed_action() {
    for mcp in [false, true] {
        for action in ["press", "input_text"] {
            // Omitted/null retains the old control-only authority and one request.
            // A composed call starts with read authority, then loses read capability
            // after dispatch; the confirmed action remains successful.
            for mode in ["omitted", "null", "read_revoked"] {
                let composed = mode == "read_revoked";
                let scopes = if composed {
                    "runtime:read computer:control computer:read"
                } else {
                    "runtime:read computer:control"
                };
                let (_tmp, service, runtime, token) = setup(scopes, composed).await;
                let mut args = arguments(action);
                if mode == "null" {
                    args["observe_after"] = Value::Null;
                } else if composed {
                    args["observe_after"] = json!({"action":"accessibility_tree"});
                }
                let mut call = Box::pin(invoke(&service, &token, mcp, args));
                // Drive the call (including its asynchronous auth hop) until
                // the single action is admitted; the call then parks waiting
                // for the Runner result.
                let request = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        let _ = futures_util::poll!(call.as_mut());
                        if let Some(request) = poll(&runtime).await {
                            break request;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("timed out waiting for the admitted computer action");
                assert!(futures_util::poll!(call.as_mut()).is_pending());
                assert_eq!(
                    request.kind,
                    if action == "press" {
                        "computer_control"
                    } else {
                        "computer_input_text"
                    }
                );
                assert!(poll(&runtime).await.is_none());
                if composed {
                    runtime
                        .runner_registry
                        .register(registration(false))
                        .await
                        .unwrap();
                }
                finish_action(&runtime, action, request.request_id).await;
                let (status, body) = call.await;
                assert_eq!(status, StatusCode::OK, "{body}");
                let result = if mcp {
                    &body["result"]["structuredContent"]
                } else {
                    &body
                };
                assert_eq!(result["success"], true, "{body}");
                assert_eq!(result["output"]["success"], true, "{body}");
                if composed {
                    let read = &result["output"]["post_observation"];
                    assert_eq!(read["success"], false, "{body}");
                    assert_eq!(
                        read["output"]["error_kind"], "capability_unavailable",
                        "{body}"
                    );
                } else {
                    assert!(result["output"].get("post_observation").is_none(), "{body}");
                }
                assert!(poll(&runtime).await.is_none());
            }
        }
    }
}
