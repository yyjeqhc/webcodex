use super::*;

#[test]
fn request_policy_metadata_receipts_record_selection_not_client_brand_or_history() {
    let root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        root.path().to_str().unwrap(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let (_db_dir, db) = test_db();
        let runtime = Arc::new(test_runtime().with_mcp_host_policy(deployment()));
        let service = Service::new(build_test_router(test_config(Some("secret")), db, runtime));
        for (profile, budget) in [(None,None),(Some("direct"),Some("9")),(Some("host_code_mode"),Some("999999")),(None,Some("8"))] {
            let params = json!({"name":"read_tool_manifest","arguments":{"tool_name":"read_files"},
                "_meta":{"io.modelcontextprotocol/clientInfo":{"name":"openai-mcp","version":"fixture"}}});
            let response = post(&service,"tools/call",params,profile,budget).await;
            assert_eq!(response.0,StatusCode::OK,"{}",response.1);
        }
        let rejected = post(&service,"tools/call",context_call(false),Some("PRIVATE_BAD_PROFILE"),None).await;
        assert_eq!(rejected.0,StatusCode::BAD_REQUEST);
    });
    crate::tool_request_trace::flush_full_trace_writer();
    let mut selections = Vec::new();
    let mut rejected = false;
    for directory in std::fs::read_dir(root.path()).unwrap().flatten() {
        if !directory.file_type().unwrap().is_dir() {
            continue;
        }
        assert!(!directory.path().join("payloads").exists());
        let text = std::fs::read_to_string(directory.path().join("events.jsonl")).unwrap();
        assert!(!text.contains("PRIVATE_BAD_PROFILE"));
        for line in text.lines() {
            let event: Value = serde_json::from_str(line).unwrap();
            if event["event"] == "mcp_request_policy_selected" {
                assert_eq!(event["method"], "tools/call");
                assert!(event["duration_ms"].is_u64());
                selections.push(event["selection"].clone());
            }
            rejected |= event["category"] == "request_policy_error";
        }
    }
    assert!(rejected);
    assert_eq!(selections.len(), 4);
    for (requested, source, profile, effective) in [
        (Value::Null, "deployment", "host_code_mode", 55),
        (json!(9), "request_header", "direct", 9),
        (json!(999999), "request_header", "host_code_mode", 55),
        (json!(8), "deployment", "host_code_mode", 8),
    ] {
        let found = selections
            .iter()
            .find(|s| s["requested_budget_secs"] == requested)
            .unwrap();
        assert_eq!(found["profile_source"], source);
        assert_eq!(
            found["budget_source"],
            if requested.is_null() {
                "deployment"
            } else {
                "request_header"
            }
        );
        assert_eq!(found["effective"]["profile"], profile);
        assert_eq!(found["effective"]["host_budget_secs"], effective);
        assert_eq!(found["deployment_budget_secs"], 55);
    }
}
