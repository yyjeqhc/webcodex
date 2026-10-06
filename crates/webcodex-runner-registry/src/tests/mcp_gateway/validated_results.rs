use super::*;
use crate::{RunnerRegistryTelemetry, ValidatedMcpToolResult};
use std::sync::Mutex;
use webcodex_core::mcp_gateway::McpGatewayToolResult;
use webcodex_core::runner_operation::RunnerOperation;

#[derive(Debug, PartialEq, Eq)]
struct Observed {
    request: String,
    client: String,
    runner: String,
    provider: String,
    provider_instance: String,
    tool: String,
    outer_reference: Option<String>,
    is_error: bool,
}

#[derive(Debug, Default)]
struct Observer {
    accepted: Mutex<Vec<Observed>>,
    generic_accepted: Mutex<usize>,
}

impl RunnerRegistryTelemetry for Observer {
    fn request_enqueued(
        &self,
        _: &RunnerRequest,
        _: &RunnerOperation,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&str>,
    ) {
    }
    fn runner_result_accepted(&self, _: &str, _: &RunnerResultPayload) {
        *self.generic_accepted.lock().unwrap() += 1;
    }
    fn mcp_tool_result_validated(&self, o: ValidatedMcpToolResult<'_>) {
        self.accepted.lock().unwrap().push(Observed {
            request: o.request_id.to_owned(),
            client: o.client_id.to_owned(),
            runner: o.runner_instance_id.to_owned(),
            provider: o.provider_id.to_owned(),
            provider_instance: o.provider_instance_id.to_owned(),
            tool: o.tool_name.to_owned(),
            outer_reference: o
                .result
                .structured_content
                .as_ref()
                .and_then(|v| v.get("operation_ref"))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            is_error: o.result.is_error,
        });
    }
    fn runner_result_finalized(&self, _: &str) {}
    fn runner_job_update_accepted(&self, _: Option<&str>, _: &str, _: &RunnerJobUpdateRequest) {}
    fn runner_job_finalized(&self, _: Option<&str>, _: &str) {}
}

fn tool_request() -> McpGatewayRequest {
    McpGatewayRequest::ToolsCall {
        provider_id: "provider".into(),
        provider_instance_id: "provider-instance".into(),
        name: "read".into(),
        arguments: serde_json::json!({"private_argument": "never_projected"}),
        expected_schema: crate::mcp_gateway::McpGatewaySchemaObservation {
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
        },
    }
}

fn result(request: &str, outer: &str) -> RunnerResultPayload {
    RunnerResultPayload {
        result: RunnerResultRequest {
            client_id: "bridge-runner".into(),
            runner_instance_id: "bridge-instance".into(),
            request_id: request.into(),
            exit_code: None,
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: None,
            error: None,
        },
        command_execution_state: None,
        mcp_gateway: Some(McpGatewayResponse::success(
            McpGatewayResponsePayload::ToolResult {
                result: McpGatewayToolResult {
                    content: vec![],
                    structured_content: Some(serde_json::json!({
                        "operation_ref": outer, "result": {"operation_ref": "nested-old-result"}
                    })),
                    is_error: true,
                },
            },
        )),
        plugin_gateway: None,
        coding_agent: None,
    }
}

async fn enqueue(
    registry: &RunnerRegistry,
) -> (String, tokio::sync::oneshot::Receiver<McpGatewayResponse>) {
    let alice = auth_context(Some("alice"), false);
    registry
        .enqueue_mcp_gateway(
            "bridge-runner",
            "bridge-instance",
            tool_request(),
            Some(&alice),
            "test".into(),
        )
        .await
        .unwrap()
}

async fn dequeue(registry: &RunnerRegistry) {
    registry
        .poll(RunnerPollRequest {
            client_id: "bridge-runner".into(),
            runner_instance_id: "bridge-instance".into(),
        })
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn validated_tool_result_observes_exact_requests_before_delivery_once() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    let (one, first) = enqueue(&registry).await;
    let (two, second) = enqueue(&registry).await;
    dequeue(&registry).await;
    dequeue(&registry).await;
    // Opposite result arrival order must not use a global last-call reference.
    registry.complete(result(&two, "outer-two")).await.unwrap();
    assert_eq!(observer.accepted.lock().unwrap().len(), 1);
    registry.complete(result(&one, "outer-one")).await.unwrap();
    let observations = observer.accepted.lock().unwrap();
    assert_eq!(
        *observations,
        vec![
            Observed {
                request: two.clone(),
                client: "bridge-runner".into(),
                runner: "bridge-instance".into(),
                provider: "provider".into(),
                provider_instance: "provider-instance".into(),
                tool: "read".into(),
                outer_reference: Some("outer-two".into()),
                is_error: true
            },
            Observed {
                request: one.clone(),
                client: "bridge-runner".into(),
                runner: "bridge-instance".into(),
                provider: "provider".into(),
                provider_instance: "provider-instance".into(),
                tool: "read".into(),
                outer_reference: Some("outer-one".into()),
                is_error: true
            },
        ]
    );
    drop(observations);
    // A valid business error is still an accepted protocol result, not success.
    assert!(first.await.unwrap().payload.is_some());
    assert!(second.await.unwrap().payload.is_some());
    assert!(registry.complete(result(&one, "replay")).await.is_err());
    assert_eq!(observer.accepted.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn invalid_or_non_tool_results_never_reach_validated_observer() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    for kind in ["mixed", "oversized", "wrong_variant", "error"] {
        let (id, receiver) = enqueue(&registry).await;
        dequeue(&registry).await;
        let mut payload = result(&id, "must-not-observe");
        match kind {
            "mixed" => payload.result.stdout = Some("private payload".into()),
            "oversized" => {
                if let Some(McpGatewayResponsePayload::ToolResult { result }) =
                    payload.mcp_gateway.as_mut().unwrap().payload.as_mut()
                {
                    result.structured_content =
                        Some(serde_json::json!({"large": "x".repeat(4 * 1024 * 1024)}));
                }
            }
            "wrong_variant" => {
                payload.mcp_gateway = Some(McpGatewayResponse::success(
                    McpGatewayResponsePayload::Tools { tools: vec![] },
                ))
            }
            "error" => {
                payload.mcp_gateway = Some(McpGatewayResponse::error(
                    McpGatewayDispatchState::OutcomeUnknown,
                    "provider_failed",
                    "private error",
                ))
            }
            _ => unreachable!(),
        }
        registry.complete(payload).await.unwrap();
        receiver.await.unwrap();
    }
    // This explicitly shows why the older generic pre-validation callback is insufficient.
    assert_eq!(*observer.generic_accepted.lock().unwrap(), 4);
    assert!(observer.accepted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn closed_waiter_does_not_erase_observed_result_or_prove_client_receipt() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    let (id, receiver) = enqueue(&registry).await;
    dequeue(&registry).await;
    drop(receiver);
    registry
        .complete(result(&id, "returned-before-response-loss"))
        .await
        .unwrap();
    assert_eq!(observer.accepted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn not_dispatched_wrong_instance_and_late_results_do_not_observe() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    let (id, receiver) = enqueue(&registry).await;
    registry.complete(result(&id, "unsolicited")).await.unwrap();
    receiver.await.unwrap();
    assert!(observer.accepted.lock().unwrap().is_empty());
    let (id, receiver) = enqueue(&registry).await;
    dequeue(&registry).await;
    let mut wrong = result(&id, "wrong-instance");
    wrong.result.runner_instance_id = "different-instance".into();
    assert!(registry.complete(wrong).await.is_err());
    registry
        .reconcile_disconnect("bridge-runner", "bridge-instance")
        .await;
    receiver.await.unwrap();
    assert!(registry.complete(result(&id, "late")).await.is_err());
    assert!(observer.accepted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn validated_tool_result_keeps_two_clients_and_shared_operation_separate() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    registry
        .register(bridge_registration(
            "other-runner",
            "other-instance",
            Some(vec![bridge_provider("provider-instance")]),
        ))
        .await
        .unwrap();
    let (one, first) = enqueue(&registry).await;
    let alice = auth_context(Some("alice"), false);
    let (two, second) = registry
        .enqueue_mcp_gateway(
            "other-runner",
            "other-instance",
            tool_request(),
            Some(&alice),
            "test".into(),
        )
        .await
        .unwrap();
    dequeue(&registry).await;
    registry
        .poll(RunnerPollRequest {
            client_id: "other-runner".into(),
            runner_instance_id: "other-instance".into(),
        })
        .await
        .unwrap()
        .unwrap();
    let mut other = result(&two, "same-original-operation");
    other.result.client_id = "other-runner".into();
    other.result.runner_instance_id = "other-instance".into();
    registry.complete(other).await.unwrap();
    registry
        .complete(result(&one, "same-original-operation"))
        .await
        .unwrap();
    let seen = observer.accepted.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        (
            seen[0].request.as_str(),
            seen[0].client.as_str(),
            seen[0].runner.as_str()
        ),
        (two.as_str(), "other-runner", "other-instance")
    );
    assert_eq!(
        (
            seen[1].request.as_str(),
            seen[1].client.as_str(),
            seen[1].runner.as_str()
        ),
        (one.as_str(), "bridge-runner", "bridge-instance")
    );
    assert_eq!(seen[0].outer_reference, seen[1].outer_reference);
    assert_ne!(seen[0].request, seen[1].request);
    drop(seen);
    first.await.unwrap();
    second.await.unwrap();
}

#[tokio::test]
async fn incomplete_or_mismatched_saved_fences_do_not_certify_observation() {
    let observer = Arc::new(Observer::default());
    let registry = RunnerRegistry::with_telemetry(observer.clone());
    register_bridge_runner(&registry).await;
    for field in [
        "runner",
        "provider",
        "instance",
        "wrong_runner",
        "wrong_provider",
        "wrong_instance",
    ] {
        let (id, receiver) = enqueue(&registry).await;
        dequeue(&registry).await;
        {
            let mut inner = registry.inner.lock().await;
            let pending = inner.pending_by_id.get_mut(&id).unwrap();
            match field {
                "runner" => pending.expected_mcp_gateway_runner_instance_id = None,
                "provider" => pending.expected_mcp_gateway_provider_id = None,
                "instance" => pending.expected_mcp_gateway_provider_instance_id = None,
                "wrong_runner" => {
                    pending.expected_mcp_gateway_runner_instance_id = Some("unrelated".into())
                }
                "wrong_provider" => {
                    pending.expected_mcp_gateway_provider_id = Some("unrelated".into())
                }
                "wrong_instance" => {
                    pending.expected_mcp_gateway_provider_instance_id = Some("unrelated".into())
                }
                _ => unreachable!(),
            }
        }
        registry
            .complete(result(&id, "must-not-certify"))
            .await
            .unwrap();
        receiver.await.unwrap();
    }
    assert!(observer.accepted.lock().unwrap().is_empty());
}
