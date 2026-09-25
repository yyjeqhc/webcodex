//! In-process HTTP/MCP transport coverage; not a live Host Hook acceptance test.
use super::*;

#[test]
fn external_observations_rest_to_mcp_roundtrip_preserves_untrusted_provenance() {
    // This test asserts the auth-required unknown-bearer path. Shared-key and
    // open-anonymous modes are process-global env switches used by other tests,
    // so pin them off under the canonical test env lock for the whole spawned
    // roundtrip.
    let mut env = crate::test_support::TestEnvGuard::new();
    env.remove("WEBCODEX_SHARED_KEY_ENABLED");
    env.remove("WEBCODEX_ALLOW_ANONYMOUS");
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(roundtrip());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn roundtrip() {
    let config = test_config(Some("fixture-token"));
    let (_temp, db) = test_db();
    let registry = stateless_observation_runner_registry().await;
    let runtime = Arc::new(
        ToolRuntime::new_for_tests_with_runner_registry(registry)
            .with_communication_database(db.clone()),
    );
    let project = crate::tool_runtime::runner_project_runtime_id("mcp-observation-agent", "shared");
    let session = start_mcp_fixture_session(&runtime, Some(&project), "External observations");
    let service = Service::new(
        build_test_router(config, db, runtime).push(
            Router::with_path("api/tools/call")
                .hoop(crate::AuthMiddleware)
                .post(crate::runtime_http::tools_call),
        ),
    );
    let event = json!({"project":project,"session_id":session,"adapter_id":"a".repeat(64),"event_id":"b".repeat(64),"observed_tool":"Bash","exit_code":null});
    let mut rejected = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("wrong-token")
        .json(&json!({"tool":"record_external_observation","params":event}))
        .send(&service)
        .await;
    assert_eq!(effective_status(&rejected), StatusCode::UNAUTHORIZED);
    let _ = rejected.take_string().await;
    for inserted in [true, false] {
        let mut response = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("fixture-token")
            .json(&json!({"tool":"record_external_observation","params":event}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&response), StatusCode::OK);
        let body = response.take_json::<Value>().await.unwrap();
        assert_eq!(body["success"], true, "{body}");
        assert_eq!(body["output"]["inserted"], inserted, "{body}");
    }
    let (status, body) = stateless_2026_tool_call(
        &service,
        "fixture-token",
        631,
        "list_external_observations",
        json!({"project":project,"session_id":session}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["isError"], false, "{body}");
    let output = &body["result"]["structuredContent"]["output"];
    assert_eq!(output["provenance"], "external_report", "{body}");
    let reports = output["observations"].as_array().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["event_id"], event["event_id"]);
    assert_eq!(reports[0]["status"], "unknown");
    assert_eq!(output["coverage"]["complete"], false);
    assert_eq!(output["coverage"]["reason"], "source_sequence_unavailable");
}
