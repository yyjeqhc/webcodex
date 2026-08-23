//! Focused contract and Runner-boundary tests for `read_files`.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

fn item(path: &str, start_line: Option<usize>, limit: Option<usize>) -> ReadFilesItem {
    ReadFilesItem {
        path: path.to_string(),
        start_line,
        limit,
    }
}

async fn next_read_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> crate::shell_protocol::ShellAgentShellRequest {
    wait_for_patch_agent_request(runtime, client_id).await
}

async fn complete_read(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    content: &str,
) {
    complete_agent_ranged_file_read_request(runtime, client_id, request, content).await;
}

#[test]
fn read_files_input_schema_enforces_batch_and_item_bounds() {
    let specs = registered_tool_specs();
    let read_files = spec_named(&specs, "read_files");
    let schema = &read_files.input_schema;
    let validates = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, schema).is_ok()
    };

    assert!(validates(
        &json!({"project": "demo", "items": [{"path": "a.rs"}]})
    ));
    assert!(validates(&json!({
        "project": "demo",
        "items": (0..8).map(|index| json!({"path": format!("{index}.rs")})).collect::<Vec<_>>()
    })));
    assert_eq!(schema["properties"]["items"]["minItems"], 1);
    assert_eq!(schema["properties"]["items"]["maxItems"], 8);
    assert!(!validates(&json!({
        "project": "demo",
        "items": (0..9).map(|index| json!({"path": format!("{index}.rs")})).collect::<Vec<_>>()
    })));
    assert_eq!(
        schema["properties"]["items"]["items"]["properties"]["path"]["minLength"],
        1
    );
    assert!(!validates(&json!({
        "project": "demo",
        "items": [{"path": "a.rs", "unexpected": true}]
    })));
    assert!(!validates(&json!({
        "project": "demo",
        "items": [{"path": "a.rs"}],
        "unexpected": true
    })));
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["items"]["items"]["additionalProperties"],
        false
    );

    for count in [1, 8] {
        let parsed = ToolCall::from_tool_name(
            "read_files",
            json!({
                "project": "demo",
                "items": (0..count)
                    .map(|index| json!({"path": format!("{index}.rs")}))
                    .collect::<Vec<_>>(),
                "with_line_numbers": true
            }),
        )
        .unwrap();
        assert!(matches!(parsed, ToolCall::ReadFiles { items, .. } if items.len() == count));
    }

    for invalid in [
        json!({"project": "demo", "items": []}),
        json!({
            "project": "demo",
            "items": (0..9).map(|index| json!({"path": format!("{index}.rs")})).collect::<Vec<_>>()
        }),
        json!({"project": "demo", "items": [{"path": " "}]}),
        json!({"project": "demo", "items": [{"path": "a.rs", "unexpected": true}]}),
        json!({"project": "demo", "items": [{"path": "a.rs"}], "unexpected": true}),
    ] {
        assert!(ToolCall::from_tool_name("read_files", invalid).is_err());
    }

    let read_file = spec_named(&specs, "read_file");
    assert!(read_file.input_schema["properties"].get("items").is_none());
    assert_eq!(
        read_file.input_schema["required"],
        json!(["project", "path"]),
        "read_file remains the single-path contract"
    );
}

#[tokio::test]
async fn read_files_returns_ordered_normalized_successes_after_out_of_order_completion() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-success";
    let runtime_project =
        register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let first_content = "one\ntwo\nthree\nfour\n";
    let second_content = "main\n";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![
                        item("src/lib.rs", Some(2), Some(2)),
                        item("src/main.rs", None, Some(1)),
                    ],
                    Some(true),
                )
                .await
        }
    });
    let request_a = next_read_request(&runtime, client_id).await;
    let request_b = next_read_request(&runtime, client_id).await;
    assert_eq!(request_a.kind, "file_read");
    assert_eq!(request_b.kind, "file_read");

    for request in [&request_b, &request_a] {
        let content = match request.path.as_deref() {
            Some("src/lib.rs") => first_content,
            Some("src/main.rs") => second_content,
            other => panic!("unexpected read path: {other:?}"),
        };
        complete_read(&runtime, client_id, request, content).await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], runtime_project);
    assert_eq!(result.output["requested_count"], 2);
    assert_eq!(result.output["returned_count"], 2);
    assert_eq!(result.output["succeeded_count"], 2);
    assert_eq!(result.output["failed_count"], 0);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["path"], "src/lib.rs");
    assert_eq!(items[0]["output"]["text"], "2 | two\n3 | three");
    assert_eq!(items[0]["output"]["format"], "numbered");
    assert_eq!(items[0]["output"]["start_line"], 2);
    assert_eq!(items[0]["output"]["returned_lines"], 2);
    assert_eq!(items[0]["output"]["end_line"], 3);
    assert_eq!(items[0]["output"]["has_more"], true);
    assert_eq!(items[0]["output"]["next_start_line"], 4);
    assert_eq!(
        items[0]["output"]["sha256"],
        format!("{:x}", Sha256::digest(first_content.as_bytes()))
    );
    assert_eq!(items[1]["index"], 1);
    assert_eq!(items[1]["path"], "src/main.rs");
    assert_eq!(items[1]["output"]["text"], "1 | main");
    assert_eq!(result.output["output_truncated"], false);
    assert!(result.output["next_index"].is_null());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap();
}

#[tokio::test]
async fn read_file_dispatch_complete_success_is_sparse_after_session_recording() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-single";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("sparse read".to_string()));
    let session_id = session.session_id.clone();
    let auth = auth_context(None, true);
    let content = "one\ntwo";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "src/lib.rs".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: None,
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    assert_eq!(request.path.as_deref(), Some("src/lib.rs"));
    complete_read(&runtime, client_id, &request, content).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["text"], "one\ntwo");
    assert_eq!(result.output["path"], "src/lib.rs");
    assert_eq!(
        result.output["sha256"],
        format!("{:x}", Sha256::digest(content.as_bytes()))
    );
    assert_eq!(result.output["total_lines"], 2);
    for omitted in [
        "format",
        "start_line",
        "limit",
        "returned_lines",
        "end_line",
        "has_more",
        "next_start_line",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "complete full-file read field {omitted} should be omitted: {}",
            result.output
        );
    }
    let sparse_bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(
        sparse_bytes <= 400,
        "complete sparse read_file regressed above model-facing budget: {sparse_bytes} bytes"
    );
    eprintln!("read_file_sparse_complete_bytes={sparse_bytes}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_file");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("sparse read_file success must match schema: {error}"));

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let finished = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "read_file")
        .expect("recorded read_file completion");
    assert!(
        finished
            .observed_paths
            .iter()
            .any(|path| path == "src/lib.rs"),
        "Session observation extraction must see the full read result before sparsification"
    );
}

#[tokio::test]
async fn read_file_dispatch_partial_success_keeps_full_range_cursor() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-partial-visible";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let content = "one\ntwo\nthree";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "src/lib.rs".to_string(),
                        session_id: None,
                        start_line: Some(2),
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, content).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["text"], "two");
    assert_eq!(result.output["format"], "plain");
    assert_eq!(result.output["path"], "src/lib.rs");
    assert_eq!(result.output["start_line"], 2);
    assert_eq!(result.output["limit"], 1);
    assert_eq!(result.output["total_lines"], 3);
    assert_eq!(result.output["returned_lines"], 1);
    assert_eq!(result.output["end_line"], 2);
    assert_eq!(result.output["has_more"], true);
    assert_eq!(result.output["next_start_line"], 3);

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_file");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("partial read_file success must match schema: {error}"));
}

#[tokio::test]
async fn read_file_dispatch_complete_explicit_range_keeps_full_range_metadata() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-explicit-range-visible";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let content = "one\ntwo";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "src/lib.rs".to_string(),
                        session_id: None,
                        start_line: Some(1),
                        limit: Some(2),
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, content).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["text"], "one\ntwo");
    assert_eq!(result.output["format"], "plain");
    assert_eq!(result.output["path"], "src/lib.rs");
    assert_eq!(result.output["start_line"], 1);
    assert_eq!(result.output["limit"], 2);
    assert_eq!(result.output["total_lines"], 2);
    assert_eq!(result.output["returned_lines"], 2);
    assert_eq!(result.output["end_line"], 2);
    assert_eq!(result.output["has_more"], false);
    assert!(result.output["next_start_line"].is_null());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_file");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| {
            panic!("explicit-range read_file success must match schema: {error}")
        });
}

#[tokio::test]
async fn read_files_dispatch_complete_batch_is_sparse_and_schema_valid() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-batch";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![
                            item("src/lib.rs", None, None),
                            item("src/main.rs", None, None),
                        ],
                        session_id: None,
                        with_line_numbers: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let requests = [
        next_read_request(&runtime, client_id).await,
        next_read_request(&runtime, client_id).await,
    ];
    for request in &requests {
        let content = match request.path.as_deref() {
            Some("src/lib.rs") => "lib",
            Some("src/main.rs") => "main",
            other => panic!("unexpected batch read path: {other:?}"),
        };
        complete_read(&runtime, client_id, request, content).await;
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
            "complete read_files batch field {omitted} should be omitted: {}",
            result.output
        );
    }
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    for item in items {
        assert_eq!(item["success"], true);
        assert!(item["error"].is_null());
        assert_eq!(item["output"]["format"], "numbered");
        assert!(item["output"].get("path").is_none());
        assert!(item["output"]["sha256"].as_str().is_some());
        assert_eq!(item["output"]["total_lines"], 1);
        for omitted in [
            "start_line",
            "limit",
            "returned_lines",
            "end_line",
            "has_more",
            "next_start_line",
        ] {
            assert!(
                item["output"].get(omitted).is_none(),
                "complete batch item field {omitted} should be omitted: {item}"
            );
        }
    }
    let sparse_bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(
        sparse_bytes <= 600,
        "complete two-file sparse batch regressed above model-facing budget: {sparse_bytes} bytes"
    );
    eprintln!("read_files_sparse_complete_bytes={sparse_bytes}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("sparse read_files batch must match schema: {error}"));
}

#[tokio::test]
async fn read_files_dispatch_mixed_batch_keeps_outer_and_failure_semantics() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-mixed";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let expected_project = project.clone();
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![item("good.txt", None, None), item(".env", None, None)],
                        session_id: None,
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    assert_eq!(request.path.as_deref(), Some("good.txt"));
    complete_read(&runtime, client_id, &request, "ok").await;

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
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[0]["path"], "good.txt");
    assert_eq!(items[0]["output"]["text"], "ok");
    assert!(items[0]["output"].get("path").is_none());
    assert!(items[0]["output"].get("format").is_none());
    assert!(items[0]["output"].get("has_more").is_none());
    assert_eq!(items[1]["success"], false);
    assert_eq!(items[1]["path"], ".env");
    assert_eq!(items[1]["output"]["error_kind"], "read_file_failed");
    assert_eq!(items[1]["output"]["reason_code"], "sensitive_path");
    assert_eq!(items[1]["output"]["state_changed"], false);
    assert!(items[1]["error"].as_str().is_some());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("mixed sparse/full read batch must match schema: {error}"));
}

#[tokio::test]
async fn read_files_isolates_mixed_failures_without_leaking_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-mixed";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![
                        item("good.txt", None, None),
                        item("missing.txt", None, None),
                        item(".env", None, None),
                        item("invalid.bin", None, None),
                    ],
                    None,
                )
                .await
        }
    });

    for _ in 0..3 {
        let request = next_read_request(&runtime, client_id).await;
        match request.path.as_deref() {
            Some("good.txt") => complete_read(&runtime, client_id, &request, "ok\n").await,
            Some("missing.txt") => {
                complete_patch_agent_request(
                    &runtime,
                    client_id,
                    &request.request_id,
                    -1,
                    "",
                    "read_file failed: not_found",
                )
                .await;
            }
            Some("invalid.bin") => {
                complete_patch_agent_request(
                    &runtime,
                    client_id,
                    &request.request_id,
                    -1,
                    "",
                    "read_file failed: invalid_utf8",
                )
                .await;
            }
            other => panic!("sensitive or unexpected path reached Runner: {other:?}"),
        }
    }

    let result = task.await.unwrap();
    assert!(result.success);
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 3);
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[1]["output"]["reason_code"], "not_found");
    assert_eq!(items[2]["output"]["reason_code"], "sensitive_path");
    assert_eq!(items[3]["output"]["reason_code"], "invalid_utf8");
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains(&root.path().to_string_lossy().to_string()));
    assert!(!serialized.contains("os error"));
}

#[tokio::test]
async fn read_files_runner_in_flight_is_concurrent_and_never_exceeds_four() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-concurrency";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    (0..8)
                        .map(|index| item(&format!("{index}.txt"), None, Some(1)))
                        .collect(),
                    None,
                )
                .await
        }
    });

    let mut active = Vec::new();
    for _ in 0..4 {
        active.push(next_read_request(&runtime, client_id).await);
    }
    let mut max_in_flight = active.len();
    let fifth_before_completion = runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        fifth_before_completion.is_none(),
        "fifth read was enqueued before a slot opened"
    );

    let mut dispatched = 4;
    while dispatched < 8 {
        let finished = active.remove(0);
        complete_read(&runtime, client_id, &finished, "value\n").await;
        active.push(next_read_request(&runtime, client_id).await);
        dispatched += 1;
        max_in_flight = max_in_flight.max(active.len());
        assert!(active.len() <= 4);
    }
    for request in active {
        complete_read(&runtime, client_id, &request, "value\n").await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 8);
    assert!(
        max_in_flight > 1,
        "batch unexpectedly degraded to serial reads"
    );
    assert!(max_in_flight <= 4);
}

#[tokio::test]
async fn read_files_deadline_preserves_completed_results_and_cancels_unfinished_reads() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests().with_read_files_deadline(Duration::from_millis(75));
    let client_id = "batch-deadline";
    register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![
                        item("fast.txt", None, None),
                        item("slow-a.txt", None, None),
                        item("slow-b.txt", None, None),
                    ],
                    None,
                )
                .await
        }
    });
    let requests = [
        next_read_request(&runtime, client_id).await,
        next_read_request(&runtime, client_id).await,
        next_read_request(&runtime, client_id).await,
    ];
    let fast = requests
        .iter()
        .find(|request| request.path.as_deref() == Some("fast.txt"))
        .unwrap();
    complete_read(&runtime, client_id, fast, "ready\n").await;

    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("batch deadline should finish promptly")
        .unwrap();
    let items = result.output["items"].as_array().unwrap();
    assert_eq!(items[0]["success"], true);
    assert_eq!(items[0]["output"]["text"], "ready");
    assert_eq!(items[1]["output"]["reason_code"], "timeout");
    assert_eq!(items[2]["output"]["reason_code"], "timeout");

    for request in requests
        .iter()
        .filter(|request| request.path.as_deref() != Some("fast.txt"))
    {
        let late = runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id.clone(),
                exit_code: Some(0),
                stdout: Some(canonical_agent_file_read_output("late\n", 1)),
                stderr: Some(String::new()),
                duration_ms: Some(100),
                error: None,
            })
            .await;
        assert!(late.is_err(), "timed-out Runner request was not cancelled");
    }
}

#[tokio::test]
async fn read_files_records_one_outer_session_event_and_keeps_metadata_outer_only() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-session";
    let project = register_agent_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("batch read".to_string()));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let auth = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![item("a.rs", None, None), item("b.rs", None, None)],
                        session_id: Some(session_id),
                        with_line_numbers: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..2 {
        let request = next_read_request(&runtime, client_id).await;
        complete_read(&runtime, client_id, &request, "session-private-text\n").await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    assert!(result.output["session_event_id"].as_str().is_some());
    assert!(result.output.get("permission").is_none());
    for item in result.output["items"].as_array().unwrap() {
        let serialized = serde_json::to_string(item).unwrap();
        assert!(!serialized.contains("session_recorded"));
        assert!(!serialized.contains("session_event_id"));
        assert!(!serialized.contains("permission"));
    }

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    let event = finished_event(&summary, "read_files");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    assert_eq!(
        event.observed_paths,
        vec!["a.rs".to_string(), "b.rs".to_string()]
    );
    let ledger = serde_json::to_string(&summary.events).unwrap();
    assert!(!ledger.contains("session-private-text"));
}
