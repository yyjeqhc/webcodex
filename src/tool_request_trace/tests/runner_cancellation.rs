use super::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerPollRequest};
use webcodex_runner_registry::{RunnerAccess, RunnerRegistry};

#[tokio::test]
async fn registry_cancellation_paths_record_terminal_state_and_finalize_exact_trace() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_str().unwrap(),
    );
    let registry =
        RunnerRegistry::with_telemetry(crate::runner_http::tool_request_trace_telemetry());
    let mut registration = crate::test_support::runner_registration(
        "computer",
        "inst",
        RunnerCapabilities {
            computer_observe: true,
            ..Default::default()
        },
    );
    registration.owner = Some("alice".into());
    registry.register(registration).await.unwrap();
    let access = RunnerAccess {
        global_visibility: false,
        owner_bypass: false,
        username: Some("alice".into()),
        group: None,
    };
    let trace_id = new_trace_id();
    let lifecycle = ToolRequestLifecycle::new(
        "mcp",
        trace_id.clone(),
        "registry-cancel",
        "tools/call",
        Some("observe_computer".into()),
    );
    let mut expected = Vec::new();
    for (path, dispatched) in [
        ("poll", false),
        ("sweep", false),
        ("sweep", true),
        ("explicit", false),
        ("explicit", true),
    ] {
        let (id, rx) = scope_active_trace(lifecycle.active_trace_id(), async {
            registry
                .enqueue_computer(
                    "computer".into(),
                    "computer_list_windows",
                    "{\"private_payload_sentinel\":\"never-persist-cancellation-body\"}".into(),
                    "alice".into(),
                    Some(&access),
                    5,
                )
                .await
                .unwrap()
        })
        .await;
        let poll = RunnerPollRequest {
            client_id: "computer".into(),
            runner_instance_id: "inst".into(),
        };
        if dispatched {
            assert_eq!(
                registry
                    .poll(poll.clone())
                    .await
                    .unwrap()
                    .unwrap()
                    .request_id,
                id
            );
        }
        // No enqueue task-local trace is present on this cancellation path.
        match path {
            "poll" => {
                drop(rx);
                assert!(registry.poll(poll).await.unwrap().is_none());
            }
            "sweep" => {
                drop(rx);
                assert_eq!(registry.cancel_abandoned_sync_requests().await, 1);
            }
            _ => {
                assert_eq!(
                    registry.cancel_request_dispatch_state(&id).await,
                    Some(dispatched)
                );
                drop(rx);
            }
        }
        let reason = if path == "explicit" {
            "explicit_cancel"
        } else {
            "waiter_closed"
        };
        expected.push((id.clone(), dispatched, reason));
        assert!(
            lookup_request_correlation(&id).is_none(),
            "{path} must finalize exact sync correlation"
        );
    }
    flush_full_trace_writer();
    let directory = temp.path().join(&trace_id);
    assert!(!directory.join("payloads").exists());
    let text = fs::read_to_string(directory.join("events.jsonl")).unwrap();
    assert!(!text.contains("never-persist-cancellation-body"));
    let events: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let cancelled: Vec<_> = events
        .iter()
        .filter(|event| event["event"] == "tool_runner_request_cancelled")
        .collect();
    assert_eq!(cancelled.len(), expected.len());
    for (id, dispatched, reason) in expected {
        let event = cancelled
            .iter()
            .find(|event| event["runner_request_id"] == id)
            .unwrap();
        assert_eq!(event["dispatched"], dispatched);
        assert_eq!(event["reason"], reason);
    }
}

#[tokio::test]
async fn runner_disconnect_returns_failure_and_finishes_the_original_handler_trace() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_str().unwrap(),
    );
    let registry =
        RunnerRegistry::with_telemetry(crate::runner_http::tool_request_trace_telemetry());
    let access = RunnerAccess {
        global_visibility: false,
        owner_bypass: false,
        username: Some("alice".into()),
        group: None,
    };
    for dispatched in [false, true] {
        // Reconnect the same instance for the next independently traced request.
        let mut registration = crate::test_support::runner_registration(
            "computer",
            "inst",
            RunnerCapabilities {
                computer_observe: true,
                ..Default::default()
            },
        );
        registration.owner = Some("alice".into());
        registry.register(registration).await.unwrap();
        let id = new_trace_id();
        let guard = ToolRequestLifecycle::new(
            "mcp",
            id.clone(),
            "1",
            "tools/call",
            Some("observe_computer".into()),
        );
        let (request_id, receiver) = scope_active_trace(
            guard.active_trace_id(),
            registry.enqueue_computer(
                "computer".into(),
                "computer_list_windows",
                "{}".into(),
                "alice".into(),
                Some(&access),
                5,
            ),
        )
        .await
        .unwrap();
        if dispatched {
            assert_eq!(
                registry
                    .poll(RunnerPollRequest {
                        client_id: "computer".into(),
                        runner_instance_id: "inst".into()
                    })
                    .await
                    .unwrap()
                    .unwrap()
                    .request_id,
                request_id
            );
        }
        registry.reconcile_disconnect("computer", "inst").await;
        let result = receiver.await.unwrap();
        assert!(!result.success);
        assert_eq!(result.request_dispatched, Some(dispatched));
        guard.dispatch_finished(true, Some(result.success), "runner_offline");
        guard.response_serialized(200, None, Some(true), Some(false), "runner_offline");
        guard.handler_returned(200, None, Some(true), Some(false), "runner_offline");
        drop(guard);
        flush_full_trace_writer();
        let page = read_trace(&id, None, None, None).unwrap();
        assert_eq!(page["handler_finished_observed"], true);
        assert!(page["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "mcp_tool_dispatch_finished"
                && event["tool_success"] == false));
        assert!(page["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["server_trace_id"] == id));
    }
}
