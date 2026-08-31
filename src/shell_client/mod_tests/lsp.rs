use super::*;
use crate::lsp_bridge::{AgentLspPayload, AgentLspRequest};

fn lsp_status_payload() -> AgentLspPayload {
    AgentLspPayload {
        project_id: "demo".to_string(),
        request: AgentLspRequest::Status,
    }
}

async fn register_lsp_test_client(
    registry: &ShellClientRegistry,
    client_id: &str,
    lsp_capable: bool,
) {
    register_lsp_test_client_capabilities(registry, client_id, lsp_capable, lsp_capable).await;
}

async fn register_lsp_test_client_capabilities(
    registry: &ShellClientRegistry,
    client_id: &str,
    lsp_capable: bool,
    call_hierarchy_capable: bool,
) {
    registry
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                lsp_read_only_navigation: lsp_capable,
                lsp_call_hierarchy: call_hierarchy_capable,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn enqueue_call_hierarchy_uses_only_its_distinct_capability() {
    let registry = ShellClientRegistry::default();
    register_lsp_test_client_capabilities(&registry, "hierarchy", false, true).await;
    let payload = AgentLspPayload {
        project_id: "demo".to_string(),
        request: AgentLspRequest::CallHierarchy {
            path: "src/main.rs".to_string(),
            line: 1,
            column: 1,
            direction: crate::lsp_bridge::CallHierarchyDirection::Both,
            depth: 1,
            limit: 50,
        },
    };
    registry
        .enqueue_lsp("hierarchy".to_string(), payload, "test".to_string(), 5)
        .await
        .expect("distinct call hierarchy capability should authorize enqueue");
}

#[tokio::test]
async fn enqueue_lsp_prunes_expired_shared_key_registration_before_admission() {
    let ttl_secs = 10;
    let registry = ShellClientRegistry::with_shared_key_limits_for_test(1, 4, ttl_secs);
    let auth = crate::auth::shared_key::shared_key_context("ttl-lsp");
    let mut registration = runner_registration("ttl-lsp", "inst", Vec::new());
    registration.capabilities = Some(crate::test_support::current_runner_capabilities(
        ShellClientCapabilities {
            lsp_call_hierarchy: true,
            ..Default::default()
        },
    ));
    registry
        .register_with_auth(registration, Some(&auth))
        .await
        .unwrap();
    registry
        .set_last_seen_for_test(
            "ttl-lsp",
            now_ts() - ttl_secs - CLIENT_ONLINE_WINDOW_SECS - 10,
        )
        .await;

    let error = registry
        .enqueue_lsp(
            "ttl-lsp".to_string(),
            AgentLspPayload {
                project_id: "demo".to_string(),
                request: AgentLspRequest::CallHierarchy {
                    path: "src/main.rs".to_string(),
                    line: 1,
                    column: 1,
                    direction: crate::lsp_bridge::CallHierarchyDirection::Both,
                    depth: 1,
                    limit: 50,
                },
            },
            "test".to_string(),
            5,
        )
        .await
        .unwrap_err();
    assert_eq!(
        error,
        EnqueueLspError::UnknownClient {
            client_id: "ttl-lsp".to_string(),
        }
    );
    let inner = registry.inner.lock().await;
    assert!(!inner.clients.contains_key("ttl-lsp"));
}

#[tokio::test]
async fn enqueue_lsp_returns_structured_unknown_client_error() {
    let registry = ShellClientRegistry::default();
    let error = registry
        .enqueue_lsp(
            "missing".to_string(),
            lsp_status_payload(),
            "test".to_string(),
            5,
        )
        .await
        .unwrap_err();

    assert_eq!(
        error,
        EnqueueLspError::UnknownClient {
            client_id: "missing".to_string()
        }
    );
    assert_eq!(error.to_string(), "unknown shell client: missing");
}

#[tokio::test]
async fn enqueue_lsp_returns_structured_offline_client_error() {
    let registry = ShellClientRegistry::default();
    register_lsp_test_client(&registry, "stale-lsp", true).await;
    registry
        .set_last_seen_for_test("stale-lsp", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    let error = registry
        .enqueue_lsp(
            "stale-lsp".to_string(),
            lsp_status_payload(),
            "test".to_string(),
            5,
        )
        .await
        .unwrap_err();

    assert_eq!(
        error,
        EnqueueLspError::ClientOffline {
            client_id: "stale-lsp".to_string()
        }
    );
}

#[tokio::test]
async fn enqueue_lsp_returns_structured_queue_full_error() {
    let registry = ShellClientRegistry::default();
    register_lsp_test_client(&registry, "full-lsp", true).await;
    {
        let mut inner = registry.inner.lock().await;
        inner.queues_by_client.insert(
            "full-lsp".to_string(),
            (0..MAX_QUEUED_REQUESTS_PER_CLIENT)
                .map(|index| format!("queued-{index}"))
                .collect(),
        );
    }
    let error = registry
        .enqueue_lsp(
            "full-lsp".to_string(),
            lsp_status_payload(),
            "test".to_string(),
            5,
        )
        .await
        .unwrap_err();

    assert_eq!(
        error,
        EnqueueLspError::QueueFull {
            client_id: "full-lsp".to_string(),
            limit: MAX_QUEUED_REQUESTS_PER_CLIENT,
        }
    );
}
