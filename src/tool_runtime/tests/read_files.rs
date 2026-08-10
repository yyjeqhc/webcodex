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
    next_patch_agent_request(runtime, client_id)
        .await
        .expect("read_files should enqueue a file_read request")
}

async fn complete_read(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    content: &str,
) {
    let start = request.start_line.unwrap_or(1);
    let end = request.end_line.unwrap_or(start);
    let limit = end.saturating_sub(start).saturating_add(1);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &canonical_agent_file_read_range(content, start, limit),
        "",
    )
    .await;
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
