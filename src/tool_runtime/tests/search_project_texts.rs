//! Focused contract, Runner-boundary, deadline, and Session tests for batch search.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellAgentShellRequest,
};
use serde_json::{json, Value};
use std::time::Duration;

fn query(pattern: &str, mode: Option<SearchResultMode>) -> SearchProjectTextsQuery {
    SearchProjectTextsQuery {
        pattern: pattern.to_string(),
        path: None,
        limit: Some(20),
        context_before: None,
        context_after: None,
        include_globs: None,
        exclude_globs: None,
        result_mode: mode,
        timeout_secs: None,
    }
}

fn request_pattern(request: &ShellAgentShellRequest) -> String {
    request
        .stdin
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| {
            payload
                .get("pattern")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .expect("search request pattern")
}

fn search_stdout(mode: &str, path: &str, preview: &str) -> String {
    let marker = r#"{"webcodex_search":{"backend":"rg","feature_unavailable":false}}"#;
    match mode {
        "matches" => format!("{marker}\n{path}:1:{preview}\n"),
        "files_with_matches" => format!("{marker}\n{path}\n"),
        "count" => format!("{marker}\n{path}:2\n"),
        other => panic!("unexpected result mode: {other}"),
    }
}

async fn complete_search_success(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &ShellAgentShellRequest,
    path: &str,
) {
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    let mode = payload["result_mode"].as_str().unwrap();
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &search_stdout(mode, path, &request_pattern(request)),
        "",
    )
    .await;
}

async fn poll_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> Option<ShellAgentShellRequest> {
    runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
}

async fn assert_no_agent_request(runtime: &ToolRuntime, client_id: &str) {
    assert!(
        poll_agent_request(runtime, client_id).await.is_none(),
        "unexpected additional Runner request for {client_id}"
    );
}

async fn run_single_agent_batch_response(
    client_id: &str,
    batch_query: SearchProjectTextsQuery,
    exit_code: i32,
    stdout: String,
    stderr: &str,
) -> ToolResult {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts("demo".to_string(), vec![batch_query])
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("single batch search request");
    complete_patch_agent_request(
        &runtime,
        client_id,
        &request.request_id,
        exit_code,
        &stdout,
        stderr,
    )
    .await;
    let result = task.await.unwrap();
    assert_no_agent_request(&runtime, client_id).await;
    result
}

#[test]
fn search_project_texts_schema_and_parser_enforce_strict_batch_contract() {
    let specs = registered_tool_specs();
    let batch = spec_named(&specs, "search_project_texts");
    let schema = &batch.input_schema;
    let validates = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, schema).is_ok()
    };

    assert!(validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle"}]
    })));
    assert!(validates(&json!({
        "project": "demo",
        "queries": (0..8).map(|index| json!({"pattern": format!("needle-{index}")})).collect::<Vec<_>>()
    })));
    assert_eq!(schema["properties"]["queries"]["minItems"], 1);
    assert_eq!(schema["properties"]["queries"]["maxItems"], 8);
    assert!(!validates(&json!({
        "project": "demo",
        "queries": (0..9).map(|index| json!({"pattern": format!("needle-{index}")})).collect::<Vec<_>>()
    })));
    assert_eq!(
        schema["properties"]["queries"]["items"]["properties"]["pattern"]["minLength"],
        1
    );
    assert!(!validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle", "unexpected": true}]
    })));
    assert!(!validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle"}],
        "unexpected": true
    })));
    assert!(!validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle", "result_mode": "paths"}]
    })));
    assert_eq!(
        schema["properties"]["queries"]["items"]["properties"]["include_globs"]["items"]
            ["minLength"],
        1
    );
    assert!(!validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle", "context_before": "2"}]
    })));
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["queries"]["items"]["additionalProperties"],
        false
    );
    for metadata in ["session_id", "allow_cross_project_session"] {
        assert!(
            schema["properties"].get(metadata).is_some(),
            "missing outer business metadata field {metadata}"
        );
    }
    for metadata in [
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
    ] {
        assert!(
            schema["properties"].get(metadata).is_none(),
            "model-facing schema must not publish recorder metadata field {metadata}"
        );
    }

    for count in [1, 8] {
        let parsed = ToolCall::from_tool_name(
            "search_project_texts",
            json!({
                "project": "demo",
                "queries": (0..count)
                    .map(|index| json!({"pattern": format!("needle-{index}")}))
                    .collect::<Vec<_>>()
            }),
        )
        .unwrap();
        assert!(
            matches!(parsed, ToolCall::SearchProjectTexts { queries, .. } if queries.len() == count)
        );
    }
    for invalid in [
        json!({"project": "demo", "queries": []}),
        json!({
            "project": "demo",
            "queries": (0..9).map(|index| json!({"pattern": format!("needle-{index}")})).collect::<Vec<_>>()
        }),
        json!({"project": "demo", "queries": [{"pattern": "needle", "unexpected": true}]}),
        json!({"project": "demo", "queries": [{"pattern": "needle"}], "unexpected": true}),
        json!({"project": "demo", "queries": [{"pattern": "needle", "result_mode": "paths"}]}),
    ] {
        assert!(ToolCall::from_tool_name("search_project_texts", invalid).is_err());
    }

    let single = spec_named(&specs, "search_project_text");
    assert!(single.input_schema["properties"].get("queries").is_none());
    assert_eq!(
        single.input_schema["required"],
        json!(["project", "pattern"]),
        "single-query schema remains unchanged"
    );
}

#[tokio::test]
async fn search_project_texts_retries_one_dropped_agent_request_and_restores_order() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-retry-once";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![query("retry-me", None), query("steady", None)],
                )
                .await
        }
    });

    let first_two = vec![
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
    ];
    let dropped = first_two
        .iter()
        .find(|request| request_pattern(request) == "retry-me")
        .unwrap();
    let steady = first_two
        .iter()
        .find(|request| request_pattern(request) == "steady")
        .unwrap();
    runtime
        .shell_clients
        .cancel_request(&dropped.request_id)
        .await;
    complete_search_success(&runtime, client_id, steady, "src/steady.rs").await;

    let retry = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("dropped query retry");
    assert_eq!(request_pattern(&retry), "retry-me");
    assert_ne!(retry.request_id, dropped.request_id);
    complete_search_success(&runtime, client_id, &retry, "src/retried.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[0]["output"]["matches"][0]["path"], "src/retried.rs");
    assert_eq!(items[1]["index"], 1);
    assert_eq!(items[1]["success"], true);
    assert_eq!(items[1]["output"]["matches"][0]["path"], "src/steady.rs");
    assert_no_agent_request(&runtime, client_id).await;
}

#[tokio::test]
async fn search_project_texts_stops_after_two_dropped_agent_attempts() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-retry-dropped-twice";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts("demo".to_string(), vec![query("drop-twice", None)])
                .await
        }
    });

    let first = next_patch_agent_request(&runtime, client_id).await.unwrap();
    assert_eq!(request_pattern(&first), "drop-twice");
    runtime
        .shell_clients
        .cancel_request(&first.request_id)
        .await;
    let second = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("one retry after first drop");
    assert_eq!(request_pattern(&second), "drop-twice");
    assert_ne!(second.request_id, first.request_id);
    runtime
        .shell_clients
        .cancel_request(&second.request_id)
        .await;

    let result = task.await.unwrap();
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["success"], false);
    assert_eq!(items[0]["output"]["reason_code"], "search_request_dropped");
    assert_no_agent_request(&runtime, client_id).await;
}

#[tokio::test]
async fn search_project_texts_retry_stays_inside_existing_concurrency_slot() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-retry-slot";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![
                        query("retry-slot", None),
                        query("blocker", None),
                        query("third", None),
                    ],
                )
                .await
        }
    });

    let first_two = vec![
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
    ];
    let retry_slot = first_two
        .iter()
        .find(|request| request_pattern(request) == "retry-slot")
        .unwrap();
    let blocker = first_two
        .iter()
        .find(|request| request_pattern(request) == "blocker")
        .unwrap();
    runtime
        .shell_clients
        .cancel_request(&retry_slot.request_id)
        .await;

    let retry = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("retry must replace work inside the occupied query slot");
    assert_eq!(request_pattern(&retry), "retry-slot");
    assert!(
        poll_agent_request(&runtime, client_id).await.is_none(),
        "third query reached Runner while blocker plus retry still occupied both slots"
    );

    complete_search_success(&runtime, client_id, &retry, "src/retry.rs").await;
    let third = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("third query after retry slot completes");
    assert_eq!(request_pattern(&third), "third");
    complete_search_success(&runtime, client_id, blocker, "src/blocker.rs").await;
    complete_search_success(&runtime, client_id, &third, "src/third.rs").await;

    let result = task.await.unwrap();
    assert_eq!(result.output["succeeded_count"], 3);
    assert_no_agent_request(&runtime, client_id).await;
}

#[tokio::test]
async fn search_project_texts_retry_uses_only_remaining_absolute_deadline() {
    let root = tempfile::tempdir().unwrap();
    let runtime =
        ToolRuntime::new_for_tests().with_search_project_texts_deadline(Duration::from_secs(6));
    let client_id = "batch-search-retry-deadline";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts("demo".to_string(), vec![query("deadline-retry", None)])
                .await
        }
    });

    let first = next_patch_agent_request(&runtime, client_id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    runtime
        .shell_clients
        .cancel_request(&first.request_id)
        .await;
    let retry = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("retry before batch deadline");
    assert!(
        retry.timeout_secs < first.timeout_secs,
        "retry reset the command timeout instead of using remaining batch budget: first={} retry={}",
        first.timeout_secs,
        retry.timeout_secs
    );
    complete_search_success(&runtime, client_id, &retry, "src/deadline.rs").await;
    let result = task.await.unwrap();
    assert_eq!(result.output["succeeded_count"], 1);
    assert_no_agent_request(&runtime, client_id).await;

    let root = tempfile::tempdir().unwrap();
    let runtime =
        ToolRuntime::new_for_tests().with_search_project_texts_deadline(Duration::from_millis(150));
    let client_id = "batch-search-expired-deadline";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts("demo".to_string(), vec![query("deadline-expired", None)])
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, client_id).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("absolute batch deadline should end the query")
        .unwrap();
    assert_eq!(
        result.output["items"][0]["output"]["reason_code"],
        "timeout"
    );
    assert_no_agent_request(&runtime, client_id).await;
    let late = runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(search_stdout("matches", "src/late.rs", "late")),
            stderr: Some(String::new()),
            duration_ms: Some(200),
            error: None,
        })
        .await;
    assert!(
        late.is_err(),
        "expired request remained pending after batch timeout"
    );
}

#[tokio::test]
async fn search_project_texts_does_not_retry_nontransient_agent_failures() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-no-retry-invalid";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let mut invalid = query("invalid", None);
    invalid.path = Some("../outside".to_string());
    let invalid_result = runtime
        .search_project_texts("demo".to_string(), vec![invalid])
        .await;
    assert_eq!(
        invalid_result.output["items"][0]["output"]["reason_code"],
        "invalid_path"
    );
    assert_no_agent_request(&runtime, client_id).await;

    let mut timeout_query = query("timeout", None);
    timeout_query.timeout_secs = Some(1);
    let timeout_result = run_single_agent_batch_response(
        "batch-search-no-retry-timeout",
        timeout_query,
        -1,
        r#"{"webcodex_search":{"backend":"rg","feature_unavailable":false}}
"#
        .to_string(),
        "command timed out after 1 seconds",
    )
    .await;
    assert_eq!(
        timeout_result.output["items"][0]["output"]["reason_code"],
        "timeout"
    );

    let backend_result = run_single_agent_batch_response(
        "batch-search-no-retry-backend",
        query("backend", None),
        2,
        r#"{"webcodex_search":{"backend":"rg","feature_unavailable":false}}
"#
        .to_string(),
        "rg failed",
    )
    .await;
    assert_eq!(
        backend_result.output["items"][0]["output"]["reason_code"],
        "search_execution_failed"
    );

    let feature_result = run_single_agent_batch_response(
        "batch-search-no-retry-feature",
        query("feature", Some(SearchResultMode::Count)),
        1,
        r#"{"webcodex_search":{"backend":"grep","feature_unavailable":true}}
"#
        .to_string(),
        "",
    )
    .await;
    assert_eq!(
        feature_result.output["items"][0]["output"]["reason_code"],
        "search_backend_feature_unavailable"
    );

    let provider_result = run_single_agent_batch_response(
        "batch-search-no-retry-provider",
        query("provider", None),
        0,
        json!({
            "format": "webcodex.external_provider_error.v1",
            "message": "provider failed"
        })
        .to_string(),
        "",
    )
    .await;
    assert_eq!(
        provider_result.output["items"][0]["output"]["reason_code"],
        "external_provider_error"
    );
}

#[tokio::test]
async fn search_project_texts_restores_input_order_after_out_of_order_runner_completion() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-order";
    let runtime_project =
        register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![
                        query("first", Some(SearchResultMode::Matches)),
                        query("second", Some(SearchResultMode::FilesWithMatches)),
                        query("third", Some(SearchResultMode::Count)),
                    ],
                )
                .await
        }
    });

    let first_two = [
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
    ];
    let second = first_two
        .iter()
        .find(|request| request_pattern(request) == "second")
        .unwrap();
    complete_search_success(&runtime, client_id, second, "src/second.rs").await;
    let third = next_patch_agent_request(&runtime, client_id).await.unwrap();
    assert_eq!(request_pattern(&third), "third");
    complete_search_success(&runtime, client_id, &third, "src/third.rs").await;
    let first = first_two
        .iter()
        .find(|request| request_pattern(request) == "first")
        .unwrap();
    complete_search_success(&runtime, client_id, first, "src/first.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], runtime_project);
    assert_eq!(result.output["requested_count"], 3);
    assert_eq!(result.output["returned_count"], 3);
    assert_eq!(result.output["succeeded_count"], 3);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["output"]["result_mode"], "matches");
    assert_eq!(items[0]["output"]["matches"][0]["path"], "src/first.rs");
    assert_eq!(items[1]["index"], 1);
    assert_eq!(items[1]["output"]["result_mode"], "files_with_matches");
    assert_eq!(items[1]["output"]["files"][0]["path"], "src/second.rs");
    assert_eq!(items[2]["index"], 2);
    assert_eq!(items[2]["output"]["result_mode"], "count");
    assert_eq!(items[2]["output"]["returned_match_count"], 2);
    for item in items {
        assert!(item["output"].get("pattern").is_none());
        assert!(item["output"].get("project").is_none());
        assert_eq!(item["output"]["backend"], "rg");
        assert_eq!(item["output"]["truncated"], false);
    }

    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_texts");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap();
}

#[tokio::test]
async fn search_project_texts_isolates_validation_no_match_and_protected_path_results() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-mixed";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let mut invalid_path = query("invalid-path", None);
    invalid_path.path = Some("../outside".to_string());
    let mut invalid_glob = query("invalid-glob", None);
    invalid_glob.include_globs = Some(vec!["!**/*.rs".to_string()]);
    let mut protected = query("protected", None);
    protected.path = Some("secrets".to_string());
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![
                        query("found", None),
                        query("absent", None),
                        invalid_path,
                        invalid_glob,
                        protected,
                        query("   ", None),
                    ],
                )
                .await
        }
    });

    for _ in 0..2 {
        let request = next_patch_agent_request(&runtime, client_id).await.unwrap();
        match request_pattern(&request).as_str() {
            "found" => complete_search_success(&runtime, client_id, &request, "src/found.rs").await,
            "absent" => {
                let marker = r#"{"webcodex_search":{"backend":"rg","feature_unavailable":false}}
"#;
                complete_patch_agent_request(
                    &runtime,
                    client_id,
                    &request.request_id,
                    1,
                    marker,
                    "",
                )
                .await;
            }
            other => panic!("unexpected Runner query: {other}"),
        }
    }

    let result = task.await.unwrap();
    assert!(result.success);
    assert_eq!(result.output["succeeded_count"], 3);
    assert_eq!(result.output["failed_count"], 3);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[1]["success"], true);
    assert_eq!(items[1]["output"]["count"], 0);
    assert_eq!(items[2]["output"]["reason_code"], "invalid_path");
    assert_eq!(items[3]["output"]["reason_code"], "invalid_glob");
    assert_eq!(items[4]["success"], true);
    assert_eq!(items[4]["output"]["count"], 0);
    assert_eq!(items[5]["output"]["reason_code"], "invalid_pattern");
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains(&root.path().to_string_lossy().to_string()));
    assert!(!serialized.contains("../outside"));
    assert!(!serialized.contains("os error"));
}

#[tokio::test]
async fn search_project_texts_runner_in_flight_is_concurrent_and_never_exceeds_two() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-concurrency";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    (0..8)
                        .map(|index| query(&format!("query-{index}"), None))
                        .collect(),
                )
                .await
        }
    });

    let mut active = vec![
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
    ];
    let third_before_completion = runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        third_before_completion.is_none(),
        "third search reached Runner before a concurrency slot opened"
    );

    let mut max_in_flight = active.len();
    let mut dispatched = 2;
    while dispatched < 8 {
        let finished = active.remove(0);
        complete_search_success(&runtime, client_id, &finished, "src/result.rs").await;
        active.push(next_patch_agent_request(&runtime, client_id).await.unwrap());
        dispatched += 1;
        max_in_flight = max_in_flight.max(active.len());
        assert!(active.len() <= 2);
    }
    for request in active {
        complete_search_success(&runtime, client_id, &request, "src/result.rs").await;
    }
    let result = task.await.unwrap();
    assert!(result.success);
    assert_eq!(result.output["succeeded_count"], 8);
    assert!(max_in_flight >= 2);
    assert!(max_in_flight <= 2);
}

#[tokio::test]
async fn search_project_texts_deadline_preserves_fast_result_and_cancels_unfinished_requests() {
    let root = tempfile::tempdir().unwrap();
    let runtime =
        ToolRuntime::new_for_tests().with_search_project_texts_deadline(Duration::from_millis(100));
    let client_id = "batch-search-deadline";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![
                        query("fast", None),
                        query("slow-a", None),
                        query("slow-b", None),
                    ],
                )
                .await
        }
    });
    let first_two = vec![
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
        next_patch_agent_request(&runtime, client_id).await.unwrap(),
    ];
    let fast = first_two
        .iter()
        .find(|request| request_pattern(request) == "fast")
        .unwrap();
    complete_search_success(&runtime, client_id, fast, "src/fast.rs").await;
    let third = next_patch_agent_request(&runtime, client_id).await.unwrap();
    assert_eq!(request_pattern(&third), "slow-b");
    let unfinished = first_two
        .into_iter()
        .filter(|request| request_pattern(request) != "fast")
        .chain(std::iter::once(third))
        .collect::<Vec<_>>();

    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("batch deadline should return promptly")
        .unwrap();
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[0]["output"]["matches"][0]["path"], "src/fast.rs");
    assert_eq!(items[1]["output"]["reason_code"], "timeout");
    assert_eq!(items[2]["output"]["reason_code"], "timeout");

    for request in unfinished {
        let late = runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(search_stdout("matches", "src/late.rs", "late")),
                stderr: Some(String::new()),
                duration_ms: Some(200),
                error: None,
            })
            .await;
        assert!(late.is_err(), "timed-out Runner request was not cancelled");
    }
}

#[tokio::test]
async fn search_project_texts_records_one_event_without_patterns_and_aggregates_paths() {
    let root = tempfile::tempdir().unwrap();
    let ledger_dir = tempfile::tempdir().unwrap();
    let ledger = ledger_dir.path().join("sessions.json");
    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let client_id = "batch-search-session";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("batch search".to_string()));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let auth = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![
                            query("RAW_BATCH_PATTERN_ALPHA", Some(SearchResultMode::Matches)),
                            query(
                                "RAW_BATCH_PATTERN_BETA",
                                Some(SearchResultMode::FilesWithMatches),
                            ),
                        ],
                        session_id: Some(session_id),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..2 {
        let request = next_patch_agent_request(&runtime, client_id).await.unwrap();
        let stdout = match request_pattern(&request).as_str() {
            "RAW_BATCH_PATTERN_ALPHA" => format!(
                "{}{}",
                search_stdout("matches", "src/a.rs", "alpha"),
                "src/shared.rs:2:shared\n/root/private.rs:3:absolute\n"
            ),
            "RAW_BATCH_PATTERN_BETA" => format!(
                "{}{}",
                search_stdout("files_with_matches", "src/shared.rs", "beta"),
                "src/b.rs\n"
            ),
            other => panic!("unexpected pattern: {other}"),
        };
        complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "")
            .await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    for item in result.output["items"].as_array().unwrap() {
        assert!(item["output"].get("session_recorded").is_none());
        assert!(item["output"].get("permission").is_none());
    }

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    let event = finished_event(&summary, "search_project_texts");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    assert_eq!(
        event.observed_paths,
        vec![
            "src/a.rs".to_string(),
            "src/shared.rs".to_string(),
            "src/b.rs".to_string()
        ]
    );
    assert_eq!(
        super::super::handoff::review_evidence_summary_for_session(&summary)["search_count"],
        1
    );
    let memory_ledger = serde_json::to_string(&summary.events).unwrap();
    assert!(!memory_ledger.contains("RAW_BATCH_PATTERN_ALPHA"));
    assert!(!memory_ledger.contains("RAW_BATCH_PATTERN_BETA"));
    assert!(!memory_ledger.contains(&root.path().to_string_lossy().to_string()));

    runtime.sessions.flush_persistence();
    let persisted = std::fs::read_to_string(&ledger).unwrap();
    assert!(!persisted.contains("RAW_BATCH_PATTERN_ALPHA"));
    assert!(!persisted.contains("RAW_BATCH_PATTERN_BETA"));
    assert!(!persisted.contains(&root.path().to_string_lossy().to_string()));
    assert!(persisted.contains("src/a.rs"));
    assert!(persisted.contains("src/shared.rs"));
    assert!(persisted.contains("src/b.rs"));
}
