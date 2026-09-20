//! Compound inspection must preserve canonical reads without duplicate model output.

use super::super::*;
use super::support::*;
use serde_json::{json, Value};

fn start_inspection(
    runtime: &ToolRuntime,
    project: &str,
    session_id: &str,
    read_before: usize,
    read_after: usize,
) -> tokio::task::JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    let call = ToolCall::from_tool_name(
        "search_and_read",
        json!({
            "project": project,
            "session_id": session_id,
            "query": {"pattern": "needle", "pattern_mode": "literal", "path": "src/lib.rs"},
            "read_before": read_before,
            "read_after": read_after,
            "with_line_numbers": true
        }),
    )
    .unwrap();
    tokio::spawn(async move {
        let auth = auth_context(None, true);
        runtime.dispatch_with_auth(call, Some(&auth)).await
    })
}

fn start_batched_inspection_with_one_invalid_query(
    runtime: &ToolRuntime,
    project: &str,
    session_id: &str,
) -> tokio::task::JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    let call = ToolCall::from_tool_name(
        "search_and_read",
        json!({
            "project": project,
            "session_id": session_id,
            "queries": [
                {"pattern": "", "pattern_mode": "literal", "path": "src/lib.rs"},
                {"pattern": "needle", "pattern_mode": "literal", "path": "src/lib.rs"}
            ],
            "read_before": 0,
            "read_after": 0,
            "max_reads": 4,
            "with_line_numbers": true
        }),
    )
    .unwrap();
    tokio::spawn(async move {
        let auth = auth_context(None, true);
        runtime.dispatch_with_auth(call, Some(&auth)).await
    })
}

async fn complete_search(runtime: &ToolRuntime, client_id: &str, lines: &[usize]) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(payload["result_mode"], "matches");
    assert_eq!(payload["context_before"], 0);
    assert_eq!(payload["context_after"], 0);
    let mut stdout =
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n".to_string();
    for line in lines {
        stdout.push_str(&format!("src/lib.rs\0{line}:needle\n"));
    }
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        if lines.is_empty() { 1 } else { 0 },
        &stdout,
        "",
    )
    .await;
}

fn validate_compound_schema(result: &ToolResult) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_and_read");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &schema,
    )
    .unwrap();
}

#[tokio::test]
async fn zero_match_include_glob_reports_nonleaking_exclusion_hint() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-glob-exclusion-hint";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let call = ToolCall::from_tool_name(
        "search_and_read",
        json!({
            "project": project,
            "session_id": session.session_id,
            "query": {
                "pattern": "needle",
                "pattern_mode": "literal",
                "include_globs": ["docs/**/*.md"]
            }
        }),
    )
    .unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let auth = auth_context(None, true);
            runtime.dispatch_with_auth(call, Some(&auth)).await
        }
    });

    let scoped = wait_for_patch_agent_request(&runtime, client_id).await;
    let scoped_payload: Value = serde_json::from_str(scoped.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(scoped_payload["include_globs"], json!(["docs/**/*.md"]));
    complete_patch_agent_request(
        &runtime,
        client_id,
        &scoped.request_id,
        1,
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "",
    )
    .await;

    let diagnostic = wait_for_patch_agent_request(&runtime, client_id).await;
    let diagnostic_payload: Value =
        serde_json::from_str(diagnostic.stdin.as_deref().unwrap()).unwrap();
    assert!(diagnostic_payload["include_globs"]
        .as_array()
        .is_some_and(Vec::is_empty));
    complete_patch_agent_request(
        &runtime,
        client_id,
        &diagnostic.request_id,
        0,
        concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
            "src/private.rs\0",
            "1:needle\n"
        ),
        "",
    )
    .await;

    // The diagnostic proves only that the caller's include filter excluded a
    // match. It must not expose the diagnostic match path or schedule a read.
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["read_request_count"], 0);
    assert_eq!(result.output["reads"], json!([]));
    assert_eq!(
        result.output["search"]["zero_match_hint"],
        "include_globs_excluded_matches"
    );
    assert!(result.output.get("zero_match_hints").is_none());
    assert!(!serde_json::to_string(&result.output)
        .unwrap()
        .contains("src/private.rs"));
    validate_compound_schema(&result);
}

#[tokio::test]
async fn search_and_read_returns_one_source_block_for_overlapping_matches() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-unique-source";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = start_inspection(&runtime, &project, &session.session_id, 20, 59);
    complete_search(&runtime, client_id, &[26, 43, 60, 77]).await;

    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.start_line, Some(6));
    assert_eq!(request.end_line, Some(136));
    let content = (1..=180)
        .map(|line| format!("source {line}\n"))
        .collect::<String>();
    complete_agent_ranged_file_read_request(&runtime, client_id, &request, &content).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let matches = result.output["search"]["matches"].as_array().unwrap();
    assert_eq!(
        matches
            .iter()
            .map(|entry| entry["line"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![26, 43, 60, 77]
    );
    let reads = result.output["reads"]["items"].as_array().unwrap();
    assert_eq!(
        reads.len(),
        1,
        "compound output must not re-expand overlapping member ranges"
    );
    assert_eq!(reads[0]["output"]["start_line"], 6);
    assert_eq!(reads[0]["output"]["returned_lines"], 131);
    let expected = (6..=136)
        .map(|line| format!("{line} | source {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(reads[0]["output"]["text"], expected);
    assert!(reads[0]["output"]["read_revision"].as_u64().is_some());
    assert!(reads[0]["output"].get("sha256").is_none());
    assert_eq!(result.output["read_request_count"], 4);
    assert_eq!(result.output["coalesced_read_count"], 1);
    validate_compound_schema(&result);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "search_and_read")
        .expect("compound inspection completion event");
    assert_eq!(finished.observed_paths, vec!["src/lib.rs"]);
}

#[tokio::test]
async fn search_and_read_budget_continuation_preserves_session_and_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-budget-continuation";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = start_inspection(&runtime, &project, &session.session_id, 59, 60);
    complete_search(&runtime, client_id, &[60]).await;
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let content = format!("{}\n", "x".repeat(1024)).repeat(120);
    complete_agent_ranged_file_read_request(&runtime, client_id, &request, &content).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let reads = &result.output["reads"];
    assert_eq!(reads["output_truncated"], true);
    let returned = &reads["items"][0]["output"];
    let next = &reads["suggested_call"];
    assert_eq!(
        next["tool"], "read_files",
        "budget truncation must provide a parser-ready continuation"
    );
    assert_eq!(next["arguments"]["project"], project);
    assert_eq!(next["arguments"]["session_id"], session.session_id);
    assert_eq!(
        next["arguments"]["items"][0]["expected_read_revision"],
        returned["read_revision"]
    );
    assert_eq!(
        next["arguments"]["items"][0]["start_line"].as_u64(),
        returned["end_line"].as_u64().map(|line| line + 1)
    );
    let call = ToolCall::from_tool_name("read_files", next["arguments"].clone()).unwrap();
    validate_compound_schema(&result);

    let continuation = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let auth = auth_context(None, true);
            runtime.dispatch_with_auth(call, Some(&auth)).await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    complete_agent_ranged_file_read_request(
        &runtime,
        client_id,
        &request,
        &format!("{content}changed\n"),
    )
    .await;
    let stale = continuation.await.unwrap();
    assert_eq!(stale.output["items"][0]["success"], false);
    assert_eq!(
        stale.output["items"][0]["output"]["reason_code"],
        "stale_read_revision"
    );
    assert!(stale.output["items"][0]["output"].get("text").is_none());
}

#[tokio::test]
async fn search_and_read_byte_ceiling_fallback_keeps_original_member_continuation() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-byte-fallback";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = start_inspection(&runtime, &project, &session.session_id, 100, 100);
    complete_search(&runtime, client_id, &[100, 250]).await;
    let merged = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(merged.start_line, Some(1));
    assert_eq!(merged.end_line, Some(350));
    complete_patch_agent_request(
        &runtime,
        client_id,
        &merged.request_id,
        1,
        "",
        "read_file failed: range_too_large",
    )
    .await;
    let content = format!("{}\n", "x".repeat(600)).repeat(360);
    for (start, end) in [(1, 200), (150, 350)] {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        assert_eq!(request.start_line, Some(start));
        assert_eq!(request.end_line, Some(end));
        complete_agent_ranged_file_read_request(&runtime, client_id, &request, &content).await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["read_success"], true);
    assert_eq!(
        result.output["coalesced_read_count"], 2,
        "report actual output ranges after byte-ceiling fallback"
    );
    let reads = &result.output["reads"];
    assert_eq!(reads["output_truncated"], true);
    assert_eq!(reads["failed_count"], 0);
    let next = &reads["suggested_call"];
    let call = ToolCall::from_tool_name("read_files", next["arguments"].clone()).unwrap();
    let ToolCall::ReadFiles {
        items, session_id, ..
    } = call
    else {
        panic!("read continuation")
    };
    assert_eq!(session_id.as_deref(), Some(session.session_id.as_str()));
    assert_eq!(items.len(), 2);
    assert!(items[0].expected_read_revision.is_some());
    assert_eq!(items[1].path, "src/lib.rs");
    assert_eq!(items[1].start_line, Some(150));
    assert_eq!(items[1].limit, Some(201));
    validate_compound_schema(&result);
}

#[tokio::test]
async fn search_and_read_budget_continuation_reindexes_coalesced_groups_without_gaps() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-reindexed-continuation";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = start_inspection(&runtime, &project, &session.session_id, 59, 60);
    complete_search(&runtime, client_id, &[60, 61, 400]).await;
    let content = format!("{}\n", "x".repeat(600)).repeat(500);
    let mut ranges = Vec::new();
    for _ in 0..2 {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        ranges.push((request.start_line.unwrap(), request.end_line.unwrap()));
        complete_agent_ranged_file_read_request(&runtime, client_id, &request, &content).await;
    }
    ranges.sort_unstable();
    assert_eq!(ranges, vec![(1, 121), (341, 460)]);
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["read_request_count"], 3);
    assert_eq!(result.output["coalesced_read_count"], 2);
    let reads = &result.output["reads"];
    assert_eq!(reads["output_truncated"], true);
    let next = &reads["suggested_call"];
    let call = ToolCall::from_tool_name("read_files", next["arguments"].clone()).unwrap();
    let ToolCall::ReadFiles { items, .. } = call else {
        panic!("read continuation")
    };
    assert_eq!(
        items.len(),
        2,
        "deferred group must survive removal of duplicate members"
    );
    assert!(items[0].expected_read_revision.is_some());
    assert_eq!(items[1].start_line, Some(341));
    assert_eq!(items[1].limit, Some(120));
    validate_compound_schema(&result);
}

#[tokio::test]
async fn batched_search_and_read_preserves_per_query_failure_identity() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-partial-failure";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task =
        start_batched_inspection_with_one_invalid_query(&runtime, &project, &session.session_id);

    // The empty first pattern fails normalization before reaching the Runner;
    // only the valid second query is dispatched.
    complete_search(&runtime, client_id, &[12]).await;
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.start_line, Some(12));
    assert_eq!(request.end_line, Some(12));
    let content = (1..=20)
        .map(|line| format!("source {line}\n"))
        .collect::<String>();
    complete_agent_ranged_file_read_request(&runtime, client_id, &request, &content).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["read_request_count"], 1);
    let search = &result.output["search"];
    assert_eq!(search["requested_count"], 2);
    assert_eq!(search["succeeded_count"], 1);
    assert_eq!(search["failed_count"], 1);
    let items = search["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["success"], false);
    assert_eq!(items[0]["output"]["reason_code"], "invalid_pattern");
    assert_eq!(items[1]["index"], 1);
    assert_eq!(items[1]["success"], true);
    assert!(items[1]["output"]["matches"][0].get("read_hint").is_none());
    validate_compound_schema(&result);
}

#[tokio::test]
async fn search_and_read_no_matches_does_not_dispatch_a_read() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "compound-no-matches";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = start_inspection(&runtime, &project, &session.session_id, 20, 20);
    complete_search(&runtime, client_id, &[]).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["search"]["matches"], json!([]));
    assert_eq!(result.output["read_request_count"], 0);
    validate_compound_schema(&result);
}
