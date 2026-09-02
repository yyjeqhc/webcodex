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
        pattern_mode: None,
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
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
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
    assert_eq!(
        schema["properties"]["max_result_bytes"]["default"],
        64 * 1024
    );
    assert_eq!(
        schema["properties"]["max_result_bytes"]["maximum"],
        256 * 1024
    );
    let budget_description = schema["properties"]["max_result_bytes"]["description"]
        .as_str()
        .unwrap();
    assert!(budget_description.contains("whole-query"));
    assert!(budget_description.contains("narrow"));
    let removed_input_cursor = ["match", "offset"].join("_");
    assert!(schema["properties"]["queries"]["items"]["properties"]
        .get(&removed_input_cursor)
        .is_none());
    let mut removed_cursor_input = json!({
        "project": "demo",
        "queries": [{"pattern": "needle"}]
    });
    removed_cursor_input["queries"][0]
        .as_object_mut()
        .unwrap()
        .insert(removed_input_cursor, json!(1));
    assert!(!validates(&removed_cursor_input));
    assert!(!validates(&json!({
        "project": "demo",
        "queries": [{"pattern": "needle"}],
        "max_result_bytes": 256 * 1024 + 1
    })));
    assert!(
        schema["properties"].get("session_id").is_some(),
        "missing outer business session_id field"
    );
    for metadata in [
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
        "allow_cross_project_session",
    ] {
        assert!(
            schema["properties"].get(metadata).is_none(),
            "model-facing schema must not publish non-business metadata field {metadata}"
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
        json!({"project": "demo", "queries": [{"pattern": "needle", "pattern_mode": "glob"}]}),
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
    let success_full = &batch.output_schema["properties"]["output"]["anyOf"][0]["anyOf"][0]
        ["properties"]["items"]["items"]["properties"]["output"]["anyOf"][0]["anyOf"][0];
    let removed_output_cursor = ["next", "match", "offset"].join("_");
    assert!(success_full["properties"]
        .get(&removed_output_cursor)
        .is_none());
    assert!(success_full["properties"].get("budget_truncated").is_none());
    let producer_reasons = success_full["properties"]["truncation_reason"]["anyOf"][0]["enum"]
        .as_array()
        .unwrap();
    assert!(!producer_reasons.contains(&json!("batch_response_budget")));
    assert!(!producer_reasons.contains(&json!("hard_result_cap")));
    let failure = &batch.output_schema["properties"]["output"]["anyOf"][0]["anyOf"][0]
        ["properties"]["items"]["items"]["properties"]["output"]["anyOf"][1];
    assert_eq!(
        failure["required"],
        json!([
            "error_kind",
            "reason_code",
            "failure_stage",
            "detail_code",
            "state_changed"
        ])
    );
    assert_eq!(failure["additionalProperties"], false);
    for property in [
        "backend",
        "exit_code",
        "result_mode",
        "effective_timeout_secs",
        "provider_code",
    ] {
        assert!(
            failure["properties"].get(property).is_some(),
            "batch failure schema omitted safe provenance field {property}"
        );
    }
}

#[tokio::test]
async fn search_project_text_default_success_is_sparse_after_session_recording() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-sparse-single";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "needle".to_string(),
                        session_id: Some(session_id),
                        pattern_mode: None,
                        path: None,
                        limit: Some(20),
                        context_before: None,
                        context_after: None,
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    complete_search_success(&runtime, client_id, &request, "src/a.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"][0]["path"], "src/a.rs");
    assert!(result.output["session_event_id"].as_str().is_some());
    for omitted in [
        "project",
        "pattern",
        "path",
        "backend",
        "result_mode",
        "pattern_mode",
        "effective_timeout_secs",
        "exit_code",
        "context_before",
        "context_after",
        "count",
        "truncated",
        "truncation_reason",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "boring search field {omitted} should be omitted: {}",
            result.output
        );
    }

    let sparse_bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(
        sparse_bytes <= 512,
        "default sparse single search regressed above the model-facing context budget: {sparse_bytes} bytes"
    );
    eprintln!("search_project_text_sparse_default_bytes={sparse_bytes}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_text");
    let instance = json!({
        "success": true,
        "output": result.output.clone(),
        "error": null,
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("sparse search success must match schema: {error}"));

    let summary = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    let finished = summary
        .events
        .iter()
        .rev()
        .find(|event| {
            event.kind == "tool_call_finished" && event.tool_name == "search_project_text"
        })
        .expect("recorded search completion");
    assert!(
        finished
            .observed_paths
            .iter()
            .any(|path| path == "src/a.rs"),
        "Session observation extraction must run before model sparsification"
    );
}

#[tokio::test]
async fn search_project_text_nondefault_success_keeps_effective_metadata() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-noteworthy-single";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "needle".to_string(),
                        session_id: None,
                        path: Some("src".to_string()),
                        pattern_mode: None,
                        limit: Some(20),
                        context_before: Some(1),
                        context_after: None,
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: Some(5),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    complete_search_success(&runtime, client_id, &request, "src/a.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["path"], "src");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["pattern_mode"], "regex");
    assert_eq!(result.output["effective_timeout_secs"], 5);
    assert_eq!(result.output["context_before"], 1);
    assert_eq!(result.output["context_after"], 0);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["truncated"], false);
    assert!(result.output["truncation_reason"].is_null());
}

#[tokio::test]
async fn search_project_text_literal_mode_keeps_effective_metadata() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-literal-single";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "RuntimeInfo {".to_string(),
                        pattern_mode: Some(SearchPatternMode::Literal),
                        session_id: None,
                        path: None,
                        limit: Some(20),
                        context_before: None,
                        context_after: None,
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let payload: Value = serde_json::from_str(request.stdin.as_deref().expect("search payload"))
        .expect("search payload json");
    assert!(payload.get("pattern_mode").is_none());
    assert_eq!(payload["pattern"], r"RuntimeInfo \{");
    complete_search_success(&runtime, client_id, &request, "src/a.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["pattern_mode"], "literal");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["matches"][0]["path"], "src/a.rs");
}

#[tokio::test]
async fn search_project_texts_literal_query_preserves_mode_to_runner_and_output() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-literal-batch";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let mut literal = query("RuntimeInfo {", None);
    literal.pattern_mode = Some(SearchPatternMode::Literal);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![literal],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let payload: Value = serde_json::from_str(request.stdin.as_deref().expect("search payload"))
        .expect("search payload json");
    assert!(payload.get("pattern_mode").is_none());
    assert_eq!(payload["pattern"], r"RuntimeInfo \{");
    complete_search_success(&runtime, client_id, &request, "src/a.rs").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["items"][0]["success"], true);
    assert_eq!(
        result.output["items"][0]["output"]["pattern_mode"],
        "literal"
    );
    assert_eq!(result.output["items"][0]["output"]["backend"], "rg");
}

#[tokio::test]
async fn search_project_text_grep_fallback_keeps_backend_metadata() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-grep-visible";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "needle".to_string(),
                        session_id: None,
                        path: None,
                        limit: Some(20),
                        pattern_mode: None,
                        context_before: None,
                        context_after: None,
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\n",
        "src/a.rs:1:needle\n"
    );
    complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, stdout, "").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 30);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["truncated"], false);
    assert!(result.output["truncation_reason"].is_null());
}

#[tokio::test]
async fn search_project_texts_default_matches_items_are_sparse_and_schema_valid() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-sparse-batch";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![query("first", None), query("second", None)],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let requests = [
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
    ];
    for request in &requests {
        let pattern = request_pattern(request);
        let path = format!("src/{pattern}.rs");
        complete_search_success(&runtime, client_id, request, &path).await;
    }

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    for omitted in [
        "project",
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "output_truncated",
        "next_index",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "complete all-success batch field {omitted} should be omitted: {}",
            result.output
        );
    }
    let items = result.output["items"].as_array().unwrap();
    for item in items {
        assert_eq!(item["success"], true);
        assert!(item["error"].is_null());
        assert!(item["output"]["matches"].as_array().is_some());
        for omitted in [
            "path",
            "backend",
            "result_mode",
            "effective_timeout_secs",
            "exit_code",
            "context_before",
            "context_after",
            "count",
            "truncated",
            "truncation_reason",
        ] {
            assert!(
                item["output"].get(omitted).is_none(),
                "boring batch search field {omitted} should be omitted: {item}"
            );
        }
    }

    let sparse_bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(
        sparse_bytes <= 800,
        "default two-query sparse batch regressed above the model-facing context budget: {sparse_bytes} bytes"
    );
    eprintln!("search_project_texts_sparse_default_bytes={sparse_bytes}");
    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_texts");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("sparse batch search success must match schema: {error}"));
}

#[tokio::test]
async fn search_project_texts_dispatch_large_default_batch_uses_sparse_fit_before_budget() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-sparse-budget-order";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let patterns = (0..8)
        .map(|index| format!("needle-{index}"))
        .collect::<Vec<_>>();
    let expected_preview = "x".repeat(7_400);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let patterns = patterns.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: patterns
                            .iter()
                            .map(|pattern| query(pattern, None))
                            .collect(),
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..8 {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        let pattern = request_pattern(&request);
        let path = format!("src/{pattern}.rs");
        let stdout = search_stdout("matches", &path, &expected_preview);
        complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "")
            .await;
    }

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("output_truncated").is_none(),
        "{}",
        result.output
    );
    assert!(
        result.output.get("next_index").is_none(),
        "{}",
        result.output
    );
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 8);
    for item in items {
        assert_eq!(item["output"]["matches"][0]["preview"], expected_preview);
        assert!(item["output"].get("backend").is_none());
        assert!(item["output"].get("truncation_reason").is_none());
    }
}

#[tokio::test]
async fn search_project_texts_dispatch_mixed_batch_only_sparsifies_success_item() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-sparse-mixed-batch";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let expected_project = project.clone();
    let auth = auth_context(None, true);
    let mut invalid = query("invalid", None);
    invalid.path = Some("../outside".to_string());

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![query("steady", None), invalid],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request_pattern(&request), "steady");
    complete_search_success(&runtime, client_id, &request, "src/steady.rs").await;
    assert_no_agent_request(&runtime, client_id).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], expected_project);
    assert_eq!(result.output["requested_count"], 2);
    assert_eq!(result.output["returned_count"], 2);
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 1);
    assert_eq!(result.output["output_truncated"], false);
    assert!(result.output["next_index"].is_null());

    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["success"], true);
    assert!(items[0]["error"].is_null());
    assert_eq!(items[0]["output"]["matches"][0]["path"], "src/steady.rs");
    for omitted in [
        "path",
        "backend",
        "result_mode",
        "effective_timeout_secs",
        "exit_code",
        "context_before",
        "context_after",
        "count",
        "truncated",
        "truncation_reason",
    ] {
        assert!(
            items[0]["output"].get(omitted).is_none(),
            "successful default item field {omitted} should be sparse in a mixed batch: {}",
            items[0]
        );
    }

    assert_eq!(items[1]["index"], 1);
    assert_eq!(items[1]["success"], false);
    assert_eq!(
        items[1]["output"]["error_kind"],
        "search_project_text_failed"
    );
    assert_eq!(items[1]["output"]["reason_code"], "invalid_path");
    assert_eq!(items[1]["output"]["failure_stage"], "request_validation");
    assert_eq!(items[1]["output"]["detail_code"], "invalid_path");
    assert_eq!(items[1]["output"]["state_changed"], false);
    assert!(items[1]["error"].as_str().is_some());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_texts");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("mixed sparse/full batch must match schema: {error}"));
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
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
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

    let retry = wait_for_patch_agent_request(&runtime, client_id).await;
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

    let first = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request_pattern(&first), "drop-twice");
    runtime
        .shell_clients
        .cancel_request(&first.request_id)
        .await;
    let second = wait_for_patch_agent_request(&runtime, client_id).await;
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
    assert_eq!(items[0]["output"]["failure_stage"], "agent_transport");
    assert_eq!(items[0]["output"]["detail_code"], "search_request_dropped");
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
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
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

    let retry = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request_pattern(&retry), "retry-slot");
    assert!(
        poll_agent_request(&runtime, client_id).await.is_none(),
        "third query reached Runner while blocker plus retry still occupied both slots"
    );

    complete_search_success(&runtime, client_id, &retry, "src/retry.rs").await;
    let third = wait_for_patch_agent_request(&runtime, client_id).await;
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

    let first = wait_for_patch_agent_request(&runtime, client_id).await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    runtime
        .shell_clients
        .cancel_request(&first.request_id)
        .await;
    let retry = wait_for_patch_agent_request(&runtime, client_id).await;
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
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("absolute batch deadline should end the query")
        .unwrap();
    assert_eq!(
        result.output["items"][0]["output"]["reason_code"],
        "timeout"
    );
    assert_eq!(
        result.output["items"][0]["output"]["failure_stage"],
        "batch_deadline"
    );
    assert_eq!(
        result.output["items"][0]["output"]["detail_code"],
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
    assert_eq!(
        invalid_result.output["items"][0]["output"]["failure_stage"],
        "request_validation"
    );
    assert_eq!(
        invalid_result.output["items"][0]["output"]["detail_code"],
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
    assert_eq!(
        timeout_result.output["items"][0]["output"]["failure_stage"],
        "backend_execution"
    );
    assert_eq!(
        timeout_result.output["items"][0]["output"]["detail_code"],
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
    assert_eq!(
        backend_result.output["items"][0]["output"]["failure_stage"],
        "backend_execution"
    );
    assert_eq!(
        backend_result.output["items"][0]["output"]["detail_code"],
        "backend_process_failed"
    );
    assert_eq!(backend_result.output["items"][0]["output"]["backend"], "rg");
    assert_eq!(backend_result.output["items"][0]["output"]["exit_code"], 2);

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
    assert_eq!(
        feature_result.output["items"][0]["output"]["failure_stage"],
        "backend_selection"
    );
    assert_eq!(
        feature_result.output["items"][0]["output"]["detail_code"],
        "backend_feature_unavailable"
    );

    let provider_result = run_single_agent_batch_response(
        "batch-search-no-retry-provider",
        query("provider", None),
        0,
        json!({
            "format": "webcodex.external_provider_error.v1",
            "provider": "claude_code",
            "capability": "search_project_text",
            "code": "rate_limited",
            "message": "private provider diagnostic at /private/provider/NEVER_RETURN"
        })
        .to_string(),
        "",
    )
    .await;
    assert_eq!(
        provider_result.output["items"][0]["output"]["reason_code"],
        "external_provider_error"
    );
    assert_eq!(
        provider_result.output["items"][0]["output"]["failure_stage"],
        "provider"
    );
    assert_eq!(
        provider_result.output["items"][0]["output"]["detail_code"],
        "provider_execution_failed"
    );
    assert_eq!(
        provider_result.output["items"][0]["output"]["provider_code"],
        "rate_limited"
    );
    let rendered = serde_json::to_string(&provider_result).unwrap();
    assert!(!rendered.contains("private provider"));
    assert!(!rendered.contains("/private/"));
    assert!(!rendered.contains("NEVER_RETURN"));

    let invalid_provider_result = run_single_agent_batch_response(
        "batch-search-invalid-provider-envelope",
        query("invalid-provider", None),
        0,
        json!({
            "format": "webcodex.external_provider_error.v1",
            "provider": "unexpected_provider",
            "capability": "search_project_text",
            "code": "rate_limited",
            "message": "untrusted provider prose"
        })
        .to_string(),
        "",
    )
    .await;
    assert_eq!(
        invalid_provider_result.output["items"][0]["output"]["reason_code"],
        "search_execution_failed"
    );
    assert_eq!(
        invalid_provider_result.output["items"][0]["output"]["failure_stage"],
        "provider"
    );
    assert_eq!(
        invalid_provider_result.output["items"][0]["output"]["detail_code"],
        "provider_protocol_invalid"
    );
    assert!(invalid_provider_result.output["items"][0]["output"]
        .get("provider_code")
        .is_none());
    assert!(!serde_json::to_string(&invalid_provider_result)
        .unwrap()
        .contains("untrusted provider prose"));
}

#[tokio::test]
async fn search_project_texts_rejects_nonleading_backend_markers() {
    for (client_id, stdout) in [
        (
            "batch-search-bare-marker",
            "{\"backend\":\"rg\"}\nsrc/a.rs:1:needle\n".to_string(),
        ),
        (
            "batch-search-late-marker",
            "src/z.rs:1:needle\n{\"webcodex_search\":{\"backend\":\"rg\"}}\n".to_string(),
        ),
        (
            "batch-search-truncated-marker",
            "[output truncated to last 12000 bytes]\nsrc/z.rs:1:needle\n{\"webcodex_search\":{\"backend\":\"rg\"}}\n"
                .to_string(),
        ),
    ] {
        let result = run_single_agent_batch_response(
            client_id,
            query("needle", None),
            0,
            stdout,
            "",
        )
        .await;
        let item = &result.output["items"][0];
        assert_eq!(item["success"], false, "client {client_id}");
        assert_eq!(
            item["output"]["reason_code"], "search_execution_failed",
            "client {client_id}"
        );
        assert_eq!(
            item["output"]["failure_stage"], "backend_protocol",
            "client {client_id}"
        );
        assert_eq!(
            item["output"]["detail_code"], "backend_identity_missing",
            "client {client_id}"
        );
        assert!(item["output"]["backend"].is_null(), "client {client_id}");
    }
}

#[tokio::test]
async fn search_project_texts_timeout_tail_cannot_promote_late_marker_records() {
    let mut timeout_query = query("needle", None);
    timeout_query.timeout_secs = Some(1);
    let result = run_single_agent_batch_response(
        "batch-search-timeout-tail-marker",
        timeout_query,
        -1,
        "[output truncated to last 12000 bytes]\nsrc/z.rs:1:needle\n{\"webcodex_search\":{\"backend\":\"rg\"}}\n"
            .to_string(),
        "command timed out after 1 seconds",
    )
    .await;

    let output = &result.output["items"][0]["output"];
    assert_eq!(result.output["items"][0]["success"], false);
    assert_eq!(output["reason_code"], "timeout");
    assert_eq!(output["failure_stage"], "agent_execution");
    assert_eq!(output["detail_code"], "timeout");
    assert!(output["backend"].is_null());
    assert!(output.get("matches").is_none());
    assert!(!serde_json::to_string(&result).unwrap().contains("src/z.rs"));
}

#[tokio::test]
async fn search_project_texts_mixed_batch_preserves_failure_and_empty_result_fidelity() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-search-provenance-mixed";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .search_project_texts(
                    "demo".to_string(),
                    vec![
                        query("normal-match", None),
                        query("backend-failure", None),
                        query("legitimate-empty", None),
                    ],
                )
                .await
        }
    });

    let first_two = [
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
    ];
    let failure = first_two
        .iter()
        .find(|request| request_pattern(request) == "backend-failure")
        .unwrap();
    complete_patch_agent_request(
        &runtime,
        client_id,
        &failure.request_id,
        2,
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "private rg stderr at /private/runner/NEVER_RETURN",
    )
    .await;
    let empty = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request_pattern(&empty), "legitimate-empty");
    let normal = first_two
        .iter()
        .find(|request| request_pattern(request) == "normal-match")
        .unwrap();
    complete_search_success(&runtime, client_id, normal, "src/found.rs").await;
    complete_patch_agent_request(
        &runtime,
        client_id,
        &empty.request_id,
        1,
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 2);
    assert_eq!(result.output["failed_count"], 1);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[0]["output"]["matches"][0]["path"], "src/found.rs");
    assert_eq!(items[1]["success"], false);
    assert_eq!(items[1]["output"]["reason_code"], "search_execution_failed");
    assert_eq!(items[1]["output"]["failure_stage"], "backend_execution");
    assert_eq!(items[1]["output"]["detail_code"], "backend_process_failed");
    assert_eq!(items[1]["output"]["backend"], "rg");
    assert_eq!(items[1]["output"]["exit_code"], 2);
    assert_eq!(items[2]["success"], true);
    assert_eq!(items[2]["output"]["matches"], json!([]));
    assert_eq!(items[2]["output"]["count"], 0);
    assert_eq!(items[2]["output"]["exit_code"], 1);
    assert_no_agent_request(&runtime, client_id).await;

    let rendered = serde_json::to_string(&result).unwrap();
    assert!(!rendered.contains("private rg stderr"));
    assert!(!rendered.contains("/private/"));
    assert!(!rendered.contains("NEVER_RETURN"));
    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_texts");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("mixed provenance batch must match schema: {error}"));
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
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
    ];
    let second = first_two
        .iter()
        .find(|request| request_pattern(request) == "second")
        .unwrap();
    complete_search_success(&runtime, client_id, second, "src/second.rs").await;
    let third = wait_for_patch_agent_request(&runtime, client_id).await;
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
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
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
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
    ];
    let third_before_completion = runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
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
        active.push(wait_for_patch_agent_request(&runtime, client_id).await);
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
        wait_for_patch_agent_request(&runtime, client_id).await,
        wait_for_patch_agent_request(&runtime, client_id).await,
    ];
    let fast = first_two
        .iter()
        .find(|request| request_pattern(request) == "fast")
        .unwrap();
    complete_search_success(&runtime, client_id, fast, "src/fast.rs").await;
    let third = wait_for_patch_agent_request(&runtime, client_id).await;
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
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..2 {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
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

#[tokio::test]
async fn search_project_texts_outer_recording_session_preserves_complete_sparse_shape() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
    };
    use crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD;

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-outer-sparse";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("outer sparse search".to_string()),
    );
    let auth = auth_context(None, true);
    let mut arguments = json!({
        "project": project,
        "queries": [{"pattern": "needle"}]
    });
    arguments[TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD] = json!(0);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_context_protocol_capability(
                    ToolCallRequest {
                        tool_name: "search_project_texts".to_string(),
                        arguments,
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: Some(&session_id),
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    true,
                    true,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    complete_search_success(&runtime, client_id, &request, "src/a.rs").await;

    let result = task.await.unwrap().result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert_eq!(result.output["session_context_revision"], 0);
    assert!(result.output.get("session_continuity").is_none());
    assert!(result.output.get("session_recovery").is_none());
    for omitted in [
        "project",
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "output_truncated",
        "next_index",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "outer recording changed complete sparse field {omitted}: {}",
            result.output
        );
    }
    assert_eq!(result.output["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        result.output["items"][0]["output"]["matches"][0]["path"],
        "src/a.rs"
    );
    assert!(result.output["items"][0]["output"].get("backend").is_none());
}

#[tokio::test]
async fn search_project_texts_outer_recording_session_keeps_final_response_under_hard_cap() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
    };
    use crate::tool_runtime::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD;
    use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "search-final-cap";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("search final response cap".to_string()),
    );
    assert_eq!(
        seed_model_facing_recovery_events(&runtime, &session.session_id, &project, 20),
        20
    );
    let auth = auth_context(None, true);
    let mut arguments = json!({
        "project": project,
        "queries": (0..8)
            .map(|index| json!({"pattern": format!("needle-{index}")}))
            .collect::<Vec<_>>(),
        "max_result_bytes": MAX_SERIALIZED_OUTPUT_BYTES
    });
    arguments[TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD] = json!(0);
    arguments[crate::tool_runtime::context_projection::TOOL_CALL_CONTEXT_REQUEST_INTERNAL_FIELD] =
        json!(["webcodex.workflow"]);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_context_protocol_capability(
                    ToolCallRequest {
                        tool_name: "search_project_texts".to_string(),
                        arguments,
                    },
                    ToolCallContext {
                        transport: ToolTransport::Mcp,
                        session_id: Some(&session_id),
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    true,
                    true,
                )
                .await
        }
    });
    let preview = "z".repeat(30 * 1024);
    for _ in 0..8 {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        let pattern = request_pattern(&request);
        let index = pattern
            .strip_prefix("needle-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let stdout = search_stdout("matches", &format!("src/{index}.rs"), &preview);
        complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "")
            .await;
    }

    let outcome = task.await.unwrap();
    assert!(outcome.success);
    let result = outcome.result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_continuity"]["status"], "behind");
    assert_eq!(result.output["context_projection"]["timing"], "post_tool");
    assert_eq!(
        result.output["context_projection"]["materials"][0]["key"],
        "webcodex.workflow"
    );
    assert_eq!(
        result.output["session_recovery"]["model_facing_events"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(result.output["output_truncated"], true);
    assert_eq!(result.output["truncation_reason"], "hard_result_cap");
    let returned_count = result.output["returned_count"].as_u64().unwrap();
    let next_index = result.output["next_index"].as_u64().unwrap();
    assert!(returned_count < 8);
    assert_eq!(next_index, returned_count);
    let serialized_len = serde_json::to_vec(&result).unwrap().len();
    assert!(
        serialized_len <= MAX_SERIALIZED_OUTPUT_BYTES,
        "outer Session overlays pushed search_project_texts final response above the 256 KiB hard cap: {serialized_len} bytes"
    );
}
