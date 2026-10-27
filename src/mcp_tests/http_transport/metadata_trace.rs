use super::*;

#[test]
fn metadata_trace_captures_direct_gateway_and_invalid_envelopes_without_payloads() {
    std::thread::Builder::new()
        .name("mcp-metadata-trace".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(exercise())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn exercise() {
    let root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        root.path().to_str().unwrap(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    let config = test_config(Some("ingress-secret-never-copy"));
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime());
    let service = Service::new(build_test_router(config, db, runtime));
    let cases = [
        (
            "read_files",
            json!({"project":"agent:missing:demo","items":[{"path":"src/example.rs","start_line":4,"limit":7}],"_wc":{"context":["project.instructions"]}}),
            StatusCode::OK,
        ),
        (
            "call_runtime_tool",
            json!({"tool":"run_script","arguments":{"project":"agent:missing:demo","language":"python","script":"body-only-private-sentinel".repeat(1_000)},"_wc":{"context":["project.instructions"]}}),
            StatusCode::OK,
        ),
        (
            "work_on_project",
            json!({"client_id":"missing","path":"/workspace/demo","instruction":"instruction-body-private-sentinel","_wc":{"context":"wrong-shape"}}),
            StatusCode::BAD_REQUEST,
        ),
    ];
    for (id, (name, arguments, expected)) in cases.into_iter().enumerate() {
        let (status, body) = stateless_2026_tool_call(
            &service,
            "ingress-secret-never-copy",
            101 + id as i64,
            name,
            arguments,
            None,
        )
        .await;
        assert_eq!(status, expected, "{name}: {body}");
    }
    crate::tool_request_trace::flush_full_trace_writer();
    let mut supplied = Vec::new();
    let mut phases = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(root.path()).unwrap().flatten() {
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        assert!(!entry.path().join("payloads").exists());
        let bytes = std::fs::read_to_string(entry.path().join("events.jsonl")).unwrap();
        for sentinel in [
            "ingress-secret-never-copy",
            "body-only-private-sentinel",
            "instruction-body-private-sentinel",
        ] {
            assert!(
                !bytes.contains(sentinel),
                "body or ingress credential escaped metadata projection"
            );
        }
        let events: Vec<Value> = bytes
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let position = |name: &str| {
            events
                .iter()
                .position(|event| event["event"] == name)
                .unwrap()
        };
        let handed = position("mcp_tool_handler_returned");
        let finished = position("mcp_tool_handler_finished");
        assert!(position("mcp_tool_response_serialized") < handed && handed < finished);
        if let Some(audit) = events
            .iter()
            .position(|event| event["phase"] == "audit_finalize")
        {
            let finalize = events
                .iter()
                .position(|event| event["phase"] == "request_finalize")
                .unwrap();
            assert!(handed < audit && audit < finalize && finalize < finished);
        }
        for line in bytes.lines() {
            let event: Value = serde_json::from_str(line).unwrap();
            if let Some(phase) = event["phase"].as_str() {
                phases.insert(phase.to_owned());
            }
            if event["phase"] == "supplied_arguments" {
                supplied.push(event["diagnostic"]["value"].clone());
            }
            assert_ne!(event["event"], "tool_trace_payload_captured");
        }
    }
    assert_eq!(supplied.len(), 3);
    let direct = supplied
        .iter()
        .find(|item| item["tool"] == "read_files")
        .unwrap();
    assert_eq!(direct["arguments"]["items"][0]["path"], "src/example.rs");
    assert!(direct["arguments"].get("_wc").is_none());
    assert_eq!(
        direct["invocation"]["context"],
        json!(["project.instructions"])
    );
    let gateway = supplied
        .iter()
        .find(|item| item["tool"] == "run_script")
        .unwrap();
    assert_eq!(gateway["entry_tool"], "call_runtime_tool");
    assert_eq!(gateway["arguments"]["script"]["omitted"], "body");
    assert_eq!(
        supplied
            .iter()
            .find(|item| item["tool"] == "work_on_project")
            .unwrap()["invocation"]["context"],
        "wrong-shape"
    );
    assert!(phases.contains("kernel_arguments"));
    assert!(phases.contains("response_summary"));
}

#[test]
fn saturated_metadata_writer_does_not_change_mcp_success() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let root = tempfile::tempdir().unwrap();
                    let mut env = crate::test_support::TestEnvGuard::new();
                    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
                    env.set(
                        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
                        root.path().to_str().unwrap(),
                    );
                    let (_tmp, db) = test_db();
                    let service = Service::new(build_test_router(
                        test_config(Some("fixture")),
                        db,
                        Arc::new(test_runtime()),
                    ));
                    let held = crate::tool_request_trace::hold_trace_writer_for_test();
                    let trace = crate::tool_request_trace::new_trace_id();
                    for _ in 0..256 {
                        crate::tool_request_trace::record_phase_latency_for(
                            Some(&trace),
                            "test_saturation",
                            std::time::Instant::now(),
                            "completed",
                        );
                    }
                    let (status, body) = stateless_2026_tool_call(
                        &service,
                        "fixture",
                        1,
                        "get_runtime_status",
                        json!({}),
                        None,
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK);
                    assert_ne!(body["result"]["isError"], true, "{body}");
                    assert_eq!(
                        body["result"]["structuredContent"]["success"], true,
                        "{body}"
                    );
                    drop(held);
                    crate::tool_request_trace::flush_full_trace_writer();
                    let page =
                        crate::tool_request_trace::read_trace(&trace, None, None, None).unwrap();
                    assert_eq!(page["coverage"], "incomplete");
                    assert!(page["capture_health"]["queue_drops"].as_u64().unwrap() > 0);
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
