//! Focused contract and Runner-boundary tests for `read_files`.

use super::super::*;
use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerPollRequest, RunnerRegisterRequest,
    RunnerResultRequest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

async fn scoped_read(
    runtime: &ToolRuntime,
    scope: super::super::read_cache::ReadScope,
    items: Vec<ReadFilesItem>,
) -> ToolResult {
    super::super::read_cache::READ_SCOPE
        .scope(scope, runtime.read_files("demo".into(), items, None))
        .await
}

#[tokio::test]
async fn read_files_canonical_session_cache_keeps_each_invocation_recorded() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-cache-canonical";
    let project = register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let auth = auth_context(None, true);
    for expected_end in [2, 1] {
        let read = runtime.dispatch_with_auth(
            ToolCall::ReadFiles {
                project: project.clone(),
                items: vec![item("a.rs", Some(1), Some(2))],
                session_id: Some(session.session_id.clone()),
                with_line_numbers: None,
                max_result_bytes: None,
            },
            Some(&auth),
        );
        tokio::pin!(read);
        assert!(futures_util::poll!(&mut read).is_pending());
        let request = next_read_request(&runtime, client).await;
        assert_eq!(request.end_line, Some(expected_end));
        complete_read(&runtime, client, &request, "one\ntwo\nthree\n").await;
        let result = read.await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["items"][0]["output"]["text"], "one\ntwo");
    }
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 2);
    assert_eq!(summary.counts.read_like, 2);
}

#[tokio::test]
async fn read_files_session_cache_validates_sha_and_slices_partial_ranges() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-cache-hit";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let content = "one\ntwo\nthree\nfour\nfive\n";
    let first = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(4))],
    );
    tokio::pin!(first);
    assert!(futures_util::poll!(&mut first).is_pending());
    let request = next_read_request(&runtime, client).await;
    assert_eq!(request.end_line, Some(4));
    complete_read(&runtime, client, &request, content).await;
    let first = first.await;
    let revision = first.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .unwrap();

    for expected in [None, Some(revision)] {
        let mut requested = item("a.rs", Some(2), Some(2));
        requested.expected_read_revision = expected;
        let hit = scoped_read(&runtime, scope.clone(), vec![requested]);
        tokio::pin!(hit);
        assert!(futures_util::poll!(&mut hit).is_pending());
        let probe = next_read_request(&runtime, client).await;
        assert_eq!((probe.start_line, probe.end_line), (Some(2), Some(2)));
        complete_read(&runtime, client, &probe, content).await;
        let result = hit.await;
        assert_eq!(result.output["items"][0]["output"]["text"], "two\nthree");
        assert_eq!(
            result.output["items"][0]["output"]["read_revision"],
            revision
        );
    }
    // A partially overlapping range must read the requested range, never
    // pretend that the cached prefix covers the missing suffix.
    let miss = scoped_read(&runtime, scope, vec![item("a.rs", Some(4), Some(2))]);
    tokio::pin!(miss);
    assert!(futures_util::poll!(&mut miss).is_pending());
    let request = next_read_request(&runtime, client).await;
    assert_eq!((request.start_line, request.end_line), (Some(4), Some(5)));
    complete_read(&runtime, client, &request, content).await;
    assert_eq!(
        miss.await.output["items"][0]["output"]["text"],
        "four\nfive"
    );
}

#[tokio::test]
async fn read_files_session_cache_external_change_stales_revision_and_refreshes_unfenced_read() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-cache-change";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let old = "one\ntwo\nthree\n";
    let first = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(2))],
    );
    tokio::pin!(first);
    assert!(futures_util::poll!(&mut first).is_pending());
    let request = next_read_request(&runtime, client).await;
    complete_read(&runtime, client, &request, old).await;
    let revision = first.await.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .unwrap();
    // Only the unreturned third line changes: validating range bytes alone
    // would incorrectly accept this old full-file revision.
    let changed = "one\ntwo\nCHANGED\n";
    let stale = scoped_read(
        &runtime,
        scope.clone(),
        vec![fenced_item("a.rs", Some(1), Some(2), revision)],
    );
    tokio::pin!(stale);
    assert!(futures_util::poll!(&mut stale).is_pending());
    let probe = next_read_request(&runtime, client).await;
    assert_eq!(probe.end_line, Some(1));
    complete_read(&runtime, client, &probe, changed).await;
    assert_stale_read_item(&stale.await, 0, "a.rs");

    let unfenced = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(2))],
    );
    tokio::pin!(unfenced);
    assert!(futures_util::poll!(&mut unfenced).is_pending());
    let probe = next_read_request(&runtime, client).await;
    assert_eq!(probe.end_line, Some(2));
    complete_read(&runtime, client, &probe, changed).await;
    let result = unfenced.await;
    assert_eq!(result.output["items"][0]["success"], true);
    assert_ne!(
        result.output["items"][0]["output"]["read_revision"],
        revision
    );

    let external = scoped_read(&runtime, scope, vec![item("a.rs", Some(1), Some(2))]);
    tokio::pin!(external);
    assert!(futures_util::poll!(&mut external).is_pending());
    let probe = next_read_request(&runtime, client).await;
    assert_eq!(probe.end_line, Some(1));
    complete_read(&runtime, client, &probe, "new\ncontent\n").await;
    assert!(futures_util::poll!(&mut external).is_pending());
    let fresh = next_read_request(&runtime, client).await;
    assert_eq!(fresh.end_line, Some(2));
    complete_read(&runtime, client, &fresh, "new\ncontent\n").await;
    assert_eq!(
        external.await.output["items"][0]["output"]["text"],
        "new\ncontent"
    );
}

#[tokio::test]
async fn read_files_singleflight_merges_pending_reads_but_not_completed_reads() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-flight";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let call = |start| ToolCall::ReadFiles {
        project: "demo".into(),
        items: vec![item("a.rs", start, Some(2))],
        session_id: None,
        with_line_numbers: None,
        max_result_bytes: None,
    };
    let first = runtime.dispatch_with_auth(call(None), Some(&auth));
    let second = runtime.dispatch_with_auth(call(Some(1)), Some(&auth));
    tokio::pin!(first, second);
    assert!(futures_util::poll!(&mut first).is_pending());
    assert!(futures_util::poll!(&mut second).is_pending());
    let request = next_read_request(&runtime, client).await;
    let instance = runtime
        .runner_registry
        .get_runner_view(client)
        .await
        .unwrap()
        .runner_instance_id;
    assert_no_pending_read(&runtime, client, &instance).await;
    complete_read(&runtime, client, &request, "one\ntwo\n").await;
    let a = first.await;
    let b = second.await;
    assert_eq!(a.output, b.output);
    let third = runtime.dispatch_with_auth(call(None), Some(&auth));
    tokio::pin!(third);
    assert!(futures_util::poll!(&mut third).is_pending());
    let request = next_read_request(&runtime, client).await;
    assert_eq!(request.end_line, Some(2));
    complete_read(&runtime, client, &request, "changed\ntwo\n").await;
    assert_eq!(
        third.await.output["items"][0]["output"]["text"],
        "changed\ntwo"
    );
}

#[tokio::test]
async fn read_files_singleflight_isolates_authority_and_survives_one_cancelled_waiter() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-flight-authority";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let mut first = Box::pin(scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", None, Some(2))],
    ));
    let mut second = Box::pin(scoped_read(
        &runtime,
        scope,
        vec![item("a.rs", None, Some(2))],
    ));
    let other = scoped_read(
        &runtime,
        super::super::read_cache::ReadScope::new(
            Some(&crate::auth::AuthContext::new(
                crate::auth::AuthKind::Bootstrap,
            )),
            Some("session-a"),
        ),
        vec![item("a.rs", None, Some(2))],
    );
    tokio::pin!(other);
    assert!(futures_util::poll!(&mut first).is_pending());
    assert!(futures_util::poll!(&mut second).is_pending());
    assert!(futures_util::poll!(&mut other).is_pending());
    let request = next_read_request(&runtime, client).await;
    let other_request = next_read_request(&runtime, client).await;
    assert_ne!(request.request_id, other_request.request_id);
    drop(first);
    complete_read(&runtime, client, &request, "one\ntwo\n").await;
    complete_read(&runtime, client, &other_request, "one\ntwo\n").await;
    assert_eq!(second.await.output["succeeded_count"], 1);
    assert_eq!(other.await.output["succeeded_count"], 1);
}

#[tokio::test]
async fn read_files_singleflight_late_waiter_keeps_its_own_deadline_budget() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-flight-deadline";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let resolved = runtime.resolve_project_input("demo").await.unwrap();
    let runner_instance_id = runtime
        .runner_registry
        .get_runner_view(client)
        .await
        .unwrap()
        .runner_instance_id;
    let runner_project_id = crate::tool_runtime::runner_local_project_id(&resolved.resolved_id)
        .unwrap()
        .to_string();
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let base = tokio::time::Instant::now();
    let first_deadline = base + Duration::from_secs(5);
    let first = super::super::read_cache::READ_SCOPE.scope(
        scope.clone(),
        runtime.read_project_snapshot(
            &resolved,
            &runner_project_id,
            &runner_instance_id,
            "a.rs".into(),
            Some(1),
            Some(2),
            None,
            first_deadline,
        ),
    );
    tokio::pin!(first);
    assert!(futures_util::poll!(&mut first).is_pending());
    let first_request = next_read_request(&runtime, client).await;

    // This caller's deadline extends beyond the first flight's bounded physical
    // lifetime, so joining that flight would shorten its original read budget.
    let second_deadline = base + Duration::from_secs(20);
    let second = super::super::read_cache::READ_SCOPE.scope(
        scope,
        runtime.read_project_snapshot(
            &resolved,
            &runner_project_id,
            &runner_instance_id,
            "a.rs".into(),
            Some(1),
            Some(2),
            None,
            second_deadline,
        ),
    );
    tokio::pin!(second);
    assert!(futures_util::poll!(&mut second).is_pending());
    let second_request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client.into(),
            runner_instance_id: runner_instance_id.clone(),
        })
        .await
        .unwrap()
        .expect("late waiter must bypass a flight that expires before its caller deadline");
    assert_ne!(first_request.request_id, second_request.request_id);
    assert_no_pending_read(&runtime, client, &runner_instance_id).await;

    complete_read(&runtime, client, &first_request, "one\ntwo\n").await;
    complete_read(&runtime, client, &second_request, "one\ntwo\n").await;
    assert!(first.await.success);
    assert!(second.await.success);
}

#[tokio::test]
async fn read_files_singleflight_last_waiter_drop_cancels_runner_registry_request() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-flight-cancel";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let mut first = Box::pin(scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", None, Some(2))],
    ));
    let mut second = Box::pin(scoped_read(
        &runtime,
        scope,
        vec![item("a.rs", None, Some(2))],
    ));
    assert!(futures_util::poll!(&mut first).is_pending());
    assert!(futures_util::poll!(&mut second).is_pending());
    let request = next_read_request(&runtime, client).await;

    drop(first);
    tokio::task::yield_now().await;
    drop(second);
    // PendingReadGuard performs async registry cleanup from Drop. Yielding lets
    // that already-spawned cancellation run without relying on wall-clock sleeps.
    for _ in 0..3 {
        tokio::task::yield_now().await;
    }

    let late = runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client.into(),
            runner_instance_id: "inst".into(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(canonical_agent_file_read_output("one\ntwo\n", 1)),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await;
    assert!(
        late.is_err(),
        "dropping the last singleflight waiter left an orphan RunnerRegistry request"
    );
}

#[test]
fn read_files_planner_deduplicates_default_ranges_without_extending_union_cap() {
    use super::super::read_files::coalesce_read_files_items;
    let planned = coalesce_read_files_items(vec![
        item("a.rs", None, None),
        item("a.rs", Some(1), Some(2000)),
        item("a.rs", Some(30), Some(10)),
    ]);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].limit, Some(2000));
    let planned = coalesce_read_files_items(vec![
        item("a.rs", Some(1), Some(400)),
        item("a.rs", Some(401), Some(1)),
    ]);
    assert_eq!(planned.len(), 2);
}

fn item(path: &str, start_line: Option<usize>, limit: Option<usize>) -> ReadFilesItem {
    ReadFilesItem {
        path: path.to_string(),
        start_line,
        limit,
        expected_read_revision: None,
    }
}

fn fenced_item(
    path: &str,
    start_line: Option<usize>,
    limit: Option<usize>,
    expected_read_revision: u64,
) -> ReadFilesItem {
    ReadFilesItem {
        path: path.to_string(),
        start_line,
        limit,
        expected_read_revision: Some(expected_read_revision),
    }
}

async fn read_revision_for(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    path: &str,
    content: &str,
) -> u64 {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        let path = path.to_string();
        async move {
            runtime
                .read_files(project, vec![item(&path, Some(1), Some(1))], None)
                .await
        }
    });
    let request = next_read_request(runtime, client_id).await;
    assert_eq!(request.path.as_deref(), Some(path));
    complete_read(runtime, client_id, &request, content).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    result.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .expect("successful read must expose read_revision")
}

async fn assert_no_pending_read(runtime: &ToolRuntime, client_id: &str, runner_instance_id: &str) {
    let pending = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client_id.to_string(),
            runner_instance_id: runner_instance_id.to_string(),
        })
        .await
        .unwrap();
    assert!(
        pending.is_none(),
        "stale read must fail before Runner dispatch"
    );
}

fn assert_stale_read_item(result: &ToolResult, index: usize, path: &str) {
    assert!(
        result.success,
        "item failure must not fail the batch transport"
    );
    let item = &result.output["items"][index];
    assert_eq!(item["success"], false);
    assert_eq!(item["path"], path);
    assert_eq!(item["output"]["error_kind"], "read_file_failed");
    assert_eq!(item["output"]["reason_code"], "stale_read_revision");
    assert_eq!(item["output"]["path"], path);
    assert_eq!(item["output"]["state_changed"], false);
    for field in [
        "expected_read_revision",
        "actual_read_revision",
        "sha256",
        "text",
        "mismatch_kind",
        "safe_retry",
        "snapshot_stable",
    ] {
        assert!(item["output"].get(field).is_none(), "{item}");
    }
}

async fn replace_runner_project_at_path(
    runtime: &ToolRuntime,
    client_id: &str,
    runner_instance_id: &str,
    project_id: &str,
    root: &std::path::Path,
) {
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: client_id.to_string(),
                runner_instance_id: runner_instance_id.to_string(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities {
                    shell: true,
                    git: true,
                    file_read: true,
                    file_write: true,
                    internal_posix_script: true,
                    ..Default::default()
                },
                policy: None,
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        runner_instance_id,
        vec![named_registered_project(
            client_id,
            project_id,
            project_id,
            &root.to_string_lossy(),
            1,
        )],
    )
    .await;
}

async fn next_read_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> crate::runner_protocol::RunnerRequest {
    wait_for_patch_agent_request(runtime, client_id).await
}

async fn complete_read(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &crate::runner_protocol::RunnerRequest,
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
    assert_eq!(
        schema["properties"]["max_result_bytes"]["default"],
        64 * 1024
    );
    assert_eq!(schema["properties"]["max_result_bytes"]["minimum"], 0);
    assert!(schema["properties"]["max_result_bytes"]
        .get("maximum")
        .is_none());
    let budget_description = schema["properties"]["max_result_bytes"]["description"]
        .as_str()
        .unwrap();
    assert!(budget_description.contains("runtime-clamped"));
    assert!(budget_description.contains("protocol overlays"));
    assert!(validates(&json!({
        "project": "demo",
        "items": [{"path": "a.rs"}],
        "max_result_bytes": 128 * 1024
    })));
    for max_result_bytes in [0, 1, 512 * 1024 + 1, 1024 * 1024] {
        assert!(validates(&json!({
            "project": "demo",
            "items": [{"path": "a.rs"}],
            "max_result_bytes": max_result_bytes
        })));
    }
    assert!(!validates(&json!({
        "project": "demo",
        "items": [{"path": "a.rs"}],
        "max_result_bytes": -1
    })));
    assert!(!validates(&json!({
        "project": "demo",
        "items": [{"path": "a.rs"}],
        "max_result_bytes": "65536"
    })));
    for revision in [1_u64, 9_007_199_254_740_991] {
        assert!(validates(&json!({
            "project": "demo",
            "items": [{"path": "a.rs", "expected_read_revision": revision}]
        })));
        let parsed = ToolCall::from_tool_name(
            "read_files",
            json!({
                "project": "demo",
                "items": [{"path": "a.rs", "expected_read_revision": revision}]
            }),
        )
        .expect("valid expected_read_revision must parse");
        assert!(matches!(
            parsed,
            ToolCall::ReadFiles { items, .. }
                if items[0].expected_read_revision == Some(revision)
        ));
    }
    for invalid_revision in [
        json!(0),
        json!(9_007_199_254_740_992_u64),
        json!("1"),
        Value::Null,
    ] {
        let value = json!({
            "project": "demo",
            "items": [{"path": "a.rs", "expected_read_revision": invalid_revision}]
        });
        assert!(!validates(&value));
        assert!(ToolCall::from_tool_name("read_files", value).is_err());
    }
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
}

#[tokio::test]
async fn read_files_returns_ordered_normalized_successes_after_out_of_order_completion() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-success";
    let runtime_project =
        register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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

    let mut result = result;
    let projection =
        super::super::dispatch::ModelFacingProjectionPlan::capture(&ToolCall::ReadFiles {
            project: runtime_project,
            items: vec![
                item("src/lib.rs", Some(2), Some(2)),
                item("src/main.rs", None, Some(1)),
            ],
            session_id: None,
            with_line_numbers: Some(true),
            max_result_bytes: None,
        });
    projection.project(&mut result);
    assert_eq!(
        result.output["suggested_call"]["arguments"]["items"][0]["start_line"],
        4
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap();
}

#[tokio::test]
async fn read_files_coalescing_falls_back_when_merged_range_crosses_byte_ceiling() {
    use webcodex_workspace::file_read_range::{read_range_from, EffectiveRange, ReadFileReason};

    let line = format!("{}\n", "x".repeat(600));
    let content = line.repeat(360);
    assert!(read_range_from(content.as_bytes(), EffectiveRange::new(Some(1), Some(180))).is_ok());
    assert!(read_range_from(
        content.as_bytes(),
        EffectiveRange::new(Some(181), Some(180))
    )
    .is_ok());
    let merged_error =
        read_range_from(content.as_bytes(), EffectiveRange::new(Some(1), Some(360))).unwrap_err();
    assert_eq!(merged_error.reason, ReadFileReason::RangeTooLarge);

    assert_merged_byte_fallback(content).await;
}

#[tokio::test]
async fn read_files_coalescing_falls_back_when_json_escaping_crosses_byte_ceiling() {
    use webcodex_workspace::file_read_normalize::{serialized_fits, success_output};
    use webcodex_workspace::file_read_range::{read_range_from, EffectiveRange};
    let content = format!("{}\n", "\"".repeat(400)).repeat(360);
    let merged =
        read_range_from(content.as_bytes(), EffectiveRange::new(Some(1), Some(360))).unwrap();
    assert!(!serialized_fits(&success_output(&merged, false)));
    for start in [1, 181] {
        let member = read_range_from(
            content.as_bytes(),
            EffectiveRange::new(Some(start), Some(180)),
        )
        .unwrap();
        assert!(serialized_fits(&success_output(&member, true)));
    }
    assert_merged_byte_fallback(content).await;
}

async fn assert_merged_byte_fallback(content: String) {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "coalesced-range-byte-fallback";
    register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![
                        item("src/lib.rs", Some(1), Some(180)),
                        item("src/lib.rs", Some(181), Some(180)),
                    ],
                    Some(false),
                )
                .await
        }
    });

    let merged = next_read_request(&runtime, client_id).await;
    assert_eq!(merged.start_line, Some(1));
    assert_eq!(merged.end_line, Some(360));
    complete_patch_agent_request(
        &runtime,
        client_id,
        &merged.request_id,
        1,
        "",
        "read_file failed: range_too_large",
    )
    .await;

    let first = next_read_request(&runtime, client_id).await;
    assert_eq!(first.start_line, Some(1));
    assert_eq!(first.end_line, Some(180));
    complete_read(&runtime, client_id, &first, &content).await;

    let second = next_read_request(&runtime, client_id).await;
    assert_eq!(second.start_line, Some(181));
    assert_eq!(second.end_line, Some(360));
    complete_read(&runtime, client_id, &second, &content).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 2);
    assert_eq!(result.output["failed_count"], 0);
    assert_eq!(result.output["items"][0]["output"]["returned_lines"], 180);
    assert_eq!(result.output["items"][1]["output"]["returned_lines"], 180);
    assert_eq!(
        result.output["items"][0]["output"]["sha256"],
        result.output["items"][1]["output"]["sha256"]
    );
}

#[tokio::test]
async fn read_files_coalesced_numbering_overflow_keeps_members_without_rereading() {
    use webcodex_workspace::file_read_normalize::success_output;
    use webcodex_workspace::file_read_range::{read_range_from, EffectiveRange};
    let content = format!("{}\n", "\"".repeat(356)).repeat(360);
    let range =
        read_range_from(content.as_bytes(), EffectiveRange::new(Some(1), Some(360))).unwrap();
    let parent = success_output(&range, false);
    let plain =
        super::super::files::slice_read_file_result(&parent, Some(1), Some(360), false, "a.rs");
    assert!(plain.success);
    let numbered =
        super::super::files::slice_read_file_result(&parent, Some(1), Some(360), true, "a.rs");
    assert!(!numbered.success);
    assert_eq!(numbered.output["reason_code"], "range_too_large");

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-plan-numbered";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let resolved = runtime.resolve_project_input("demo").await.unwrap();
    let read = runtime.read_files_coalesced_resolved(
        &resolved,
        vec![
            item("a.rs", Some(1), Some(180)),
            item("a.rs", Some(181), Some(180)),
        ],
        Some(true),
    );
    tokio::pin!(read);
    assert!(futures_util::poll!(&mut read).is_pending());
    let request = next_read_request(&runtime, client).await;
    assert_eq!(request.end_line, Some(360));
    complete_read(&runtime, client, &request, &content).await;
    let (result, items) = read.await;
    assert_eq!(result.output["succeeded_count"], 2);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].limit, Some(180));
    assert_eq!(items[1].start_line, Some(181));
}

#[tokio::test]
async fn read_files_session_cache_fails_closed_on_delete_and_runner_replacement() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client = "read-cache-replacement";
    register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let scope = super::super::read_cache::ReadScope::new(None, Some("session-a"));
    let first = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(2))],
    );
    tokio::pin!(first);
    assert!(futures_util::poll!(&mut first).is_pending());
    let request = next_read_request(&runtime, client).await;
    complete_read(&runtime, client, &request, "a\nb\n").await;
    let revision = first.await.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .unwrap();
    let deleted = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(2))],
    );
    tokio::pin!(deleted);
    assert!(futures_util::poll!(&mut deleted).is_pending());
    let probe = next_read_request(&runtime, client).await;
    assert_eq!(probe.end_line, Some(1));
    complete_patch_agent_request(
        &runtime,
        client,
        &probe.request_id,
        1,
        "",
        "read_file failed: not_found",
    )
    .await;
    assert_eq!(
        deleted.await.output["items"][0]["output"]["reason_code"],
        "not_found"
    );
    // Restore the old content to repopulate the cache before replacement.
    let restored = scoped_read(
        &runtime,
        scope.clone(),
        vec![item("a.rs", Some(1), Some(2))],
    );
    tokio::pin!(restored);
    assert!(futures_util::poll!(&mut restored).is_pending());
    let request = next_read_request(&runtime, client).await;
    assert_eq!(request.end_line, Some(2));
    complete_read(&runtime, client, &request, "a\nb\n").await;
    assert_eq!(restored.await.output["succeeded_count"], 1);
    replace_runner_project_at_path(&runtime, client, "replacement", "demo", root.path()).await;
    let stale = scoped_read(
        &runtime,
        scope.clone(),
        vec![fenced_item("a.rs", Some(1), Some(2), revision)],
    )
    .await;
    assert_stale_read_item(&stale, 0, "a.rs");
    assert_no_pending_read(&runtime, client, "replacement").await;
    let replacement = scoped_read(&runtime, scope, vec![item("a.rs", Some(1), Some(2))]);
    tokio::pin!(replacement);
    assert!(futures_util::poll!(&mut replacement).is_pending());
    let request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client.into(),
            runner_instance_id: "replacement".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.end_line, Some(2));
    complete_patch_agent_request_for_instance(
        &runtime,
        client,
        "replacement",
        &request.request_id,
        0,
        &canonical_agent_file_read_range("new\nrunner\n", 1, 2),
        "",
    )
    .await;
    assert_eq!(
        replacement.await.output["items"][0]["output"]["text"],
        "new\nrunner"
    );
}

#[tokio::test]
async fn read_files_reuses_read_revision_for_same_full_file_snapshot_across_ranges() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-revision-reuse";
    register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let content = "one\ntwo\nthree\nfour\n";

    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![item("src/lib.rs", Some(1), Some(2))],
                    Some(false),
                )
                .await
        }
    });
    let first_request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &first_request, content).await;
    let first = first.await.unwrap();
    assert!(first.success, "{:?}", first.error);
    let first_revision = first.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .expect("first read revision");

    let second = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_files(
                    "demo".to_string(),
                    vec![item("src/lib.rs", Some(3), Some(2))],
                    Some(false),
                )
                .await
        }
    });
    let second_request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &second_request, content).await;
    let second = second.await.unwrap();
    assert!(second.success, "{:?}", second.error);
    let second_revision = second.output["items"][0]["output"]["read_revision"]
        .as_u64()
        .expect("second read revision");

    assert_eq!(first_revision, second_revision);
    assert_eq!(
        first.output["items"][0]["output"]["sha256"],
        second.output["items"][0]["output"]["sha256"]
    );
}

#[tokio::test]
async fn read_file_dispatch_complete_success_is_sparse_after_session_recording() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-single";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
                    ToolCall::ReadFiles {
                        project: project,
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "src/lib.rs".to_string(),
                            start_line: None,
                            limit: None,
                            expected_read_revision: None,
                        }],
                        session_id: Some(session_id),
                        with_line_numbers: None,
                        max_result_bytes: None,
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
    let item = &result.output["items"][0];
    assert_eq!(item["output"]["text"], "one\ntwo");
    assert_eq!(item["path"], "src/lib.rs");
    assert!(item["output"].get("sha256").is_none());
    assert!(item["output"]["read_revision"].as_u64().is_some());
    assert_eq!(item["output"]["total_lines"], 2);
    for omitted in [
        "format",
        "start_line",
        "limit",
        "returned_lines",
        "end_line",
        "has_more",
        "next_start_line",
        "continuation",
    ] {
        assert!(
            item["output"].get(omitted).is_none(),
            "complete full-file read item field {omitted} should be omitted: {item}"
        );
    }
    let sparse_bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(
        sparse_bytes <= 600,
        "complete sparse one-item read_files regressed above model-facing budget: {sparse_bytes} bytes"
    );
    eprintln!("read_files_sparse_complete_one_item_bytes={sparse_bytes}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| {
            panic!("sparse one-item read_files success must match schema: {error}")
        });

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let finished = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "read_files")
        .expect("recorded read_files completion");
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
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("read continuation session".to_string()),
    );
    let session_id = session.session_id.clone();
    let auth = auth_context(None, true);
    let content = "one\ntwo\nthree";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project: project,
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "src/lib.rs".to_string(),
                            start_line: Some(2),
                            limit: Some(1),
                            expected_read_revision: None,
                        }],
                        session_id: Some(session_id),
                        with_line_numbers: None,
                        max_result_bytes: None,
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
    let item = &result.output["items"][0];
    assert_eq!(item["output"]["text"], "two");
    assert_eq!(item["output"]["format"], "plain");
    assert_eq!(item["path"], "src/lib.rs");
    assert_eq!(item["output"]["start_line"], 2);
    assert_eq!(item["output"]["limit"], 1);
    assert_eq!(item["output"]["total_lines"], 3);
    assert_eq!(item["output"]["returned_lines"], 1);
    assert_eq!(item["output"]["end_line"], 2);
    assert_eq!(item["output"]["has_more"], true);
    assert_eq!(
        result.output["suggested_call"]["arguments"]["items"][0]["start_line"],
        3
    );
    let suggested = &result.output["suggested_call"];
    let read_revision = item["output"]["read_revision"]
        .as_u64()
        .expect("successful read must expose read_revision");
    assert!(item["output"].get("sha256").is_none());
    assert!((1..=9_007_199_254_740_991).contains(&read_revision));
    assert_eq!(suggested["tool"], "read_files");
    assert_eq!(suggested["arguments"]["session_id"], session_id);
    let next_call = ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .expect("read_files continuation suggested_call must parse");
    assert!(matches!(
        &next_call,
        ToolCall::ReadFiles {
            project: next_project,
            items,
            session_id: Some(next_session_id),
            with_line_numbers: None,
            max_result_bytes: None,
        } if next_project == &project
            && items.len() == 1
            && items[0].path == "src/lib.rs"
            && items[0].start_line == Some(3)
            && items[0].limit == Some(1)
            && items[0].expected_read_revision == Some(read_revision)
            && next_session_id == &session_id
    ));

    let second = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(next_call, Some(&auth)).await }
    });
    let second_request = next_read_request(&runtime, client_id).await;
    assert_eq!(second_request.start_line, Some(3));
    complete_read(&runtime, client_id, &second_request, content).await;
    let second = second.await.unwrap();
    assert!(second.success, "{:?}", second.error);
    let second_item = &second.output["items"][0];
    assert_eq!(second_item["success"], true);
    assert_eq!(second_item["output"]["text"], "three");
    assert_eq!(second_item["output"]["read_revision"], read_revision);

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| {
            panic!("partial one-item read_files success must match schema: {error}")
        });
}

#[tokio::test]
async fn read_files_continuation_rejects_changed_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-source-change";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first_content = "one\ntwo\nthree";

    let first = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project: project,
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "src/lib.rs".to_string(),
                            start_line: Some(1),
                            limit: Some(1),
                            expected_read_revision: None,
                        }],
                        session_id: None,
                        with_line_numbers: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, first_content).await;
    let first = first.await.unwrap();
    let first_item = &first.output["items"][0];
    let first_revision = first_item["output"]["read_revision"]
        .as_u64()
        .expect("first read revision");
    let suggested = &first.output["suggested_call"];
    let next_call = ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .expect("snapshot-fenced continuation must parse");
    assert!(matches!(
        &next_call,
        ToolCall::ReadFiles { items, .. }
            if items[0].expected_read_revision == Some(first_revision)
    ));

    // Insert a line before the cursor between calls. The range request still
    // reaches the exact Runner, but Runtime must reject its changed full-file
    // snapshot rather than expose shifted continuation content.
    let changed_content = "zero\none\ntwo\nthree";
    let second = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(next_call, Some(&auth)).await }
    });
    let request = next_read_request(&runtime, client_id).await;
    assert_eq!(request.start_line, Some(2));
    complete_read(&runtime, client_id, &request, changed_content).await;
    let second = second.await.unwrap();
    assert!(second.success, "batch transport should remain successful");
    let second_item = &second.output["items"][0];
    assert_eq!(second_item["success"], false);
    assert_eq!(second_item["output"]["error_kind"], "read_file_failed");
    assert_eq!(second_item["output"]["reason_code"], "stale_read_revision");
    assert_eq!(second_item["output"]["path"], "src/lib.rs");
    assert_eq!(second_item["output"]["state_changed"], false);
    for field in ["text", "read_revision", "sha256", "actual_read_revision"] {
        assert!(second_item["output"].get(field).is_none(), "{second_item}");
    }
    assert!(second.output.get("suggested_call").is_none());
}

#[tokio::test]
async fn read_files_rejects_revision_from_previous_runtime_before_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let client_id = "read-restart-revision";
    let runtime_a = ToolRuntime::new_for_tests();
    let project_a =
        register_runner_project_at_path(&runtime_a, client_id, "demo", root.path()).await;
    let revision = read_revision_for(
        &runtime_a,
        client_id,
        &project_a,
        "src/lib.rs",
        "one\ntwo\n",
    )
    .await;

    let runtime_b = ToolRuntime::new_for_tests();
    let project_b =
        register_runner_project_at_path(&runtime_b, client_id, "demo", root.path()).await;
    let result = runtime_b
        .read_files(
            project_b,
            vec![fenced_item("src/lib.rs", Some(2), Some(1), revision)],
            None,
        )
        .await;
    assert_stale_read_item(&result, 0, "src/lib.rs");
    assert_no_pending_read(&runtime_b, client_id, "inst").await;
}

#[tokio::test]
async fn read_files_rejects_revision_for_wrong_path_before_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-wrong-path-revision";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let revision = read_revision_for(&runtime, client_id, &project, "src/a.rs", "one\ntwo\n").await;

    let result = runtime
        .read_files(
            project,
            vec![fenced_item("src/b.rs", Some(2), Some(1), revision)],
            None,
        )
        .await;
    assert_stale_read_item(&result, 0, "src/b.rs");
    assert_no_pending_read(&runtime, client_id, "inst").await;
}

#[tokio::test]
async fn read_files_rejects_revision_for_wrong_project_before_dispatch() {
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let project_a =
        register_runner_project_at_path(&runtime, "read-project-a", "demo-a", root_a.path()).await;
    let project_b =
        register_runner_project_at_path(&runtime, "read-project-b", "demo-b", root_b.path()).await;
    let revision = read_revision_for(
        &runtime,
        "read-project-a",
        &project_a,
        "src/lib.rs",
        "one\ntwo\n",
    )
    .await;

    let result = runtime
        .read_files(
            project_b,
            vec![fenced_item("src/lib.rs", Some(2), Some(1), revision)],
            None,
        )
        .await;
    assert_stale_read_item(&result, 0, "src/lib.rs");
    assert_no_pending_read(&runtime, "read-project-b", "inst").await;
}

#[tokio::test]
async fn read_files_rejects_revision_after_runner_replacement_before_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-runner-replacement";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let revision =
        read_revision_for(&runtime, client_id, &project, "src/lib.rs", "one\ntwo\n").await;

    runtime
        .runner_registry
        .set_last_seen_for_test(client_id, chrono::Utc::now().timestamp() - 120)
        .await;
    replace_runner_project_at_path(&runtime, client_id, "inst-b", "demo", root.path()).await;

    let result = runtime
        .read_files(
            project,
            vec![fenced_item("src/lib.rs", Some(2), Some(1), revision)],
            None,
        )
        .await;
    assert_stale_read_item(&result, 0, "src/lib.rs");
    assert_no_pending_read(&runtime, client_id, "inst-b").await;
}

#[tokio::test]
async fn read_files_stale_revision_isolated_from_normal_batch_item() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-stale-batch-isolation";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let revision = read_revision_for(&runtime, client_id, &project, "src/a.rs", "one\ntwo\n").await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .read_files(
                    project,
                    vec![
                        fenced_item("src/stale.rs", Some(2), Some(1), revision),
                        item("src/good.rs", None, None),
                    ],
                    None,
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    assert_eq!(request.path.as_deref(), Some("src/good.rs"));
    complete_read(&runtime, client_id, &request, "ok\n").await;
    let result = task.await.unwrap();

    assert_stale_read_item(&result, 0, "src/stale.rs");
    assert_eq!(result.output["items"][1]["success"], true);
    assert_eq!(result.output["items"][1]["output"]["text"], "ok");
    assert_no_pending_read(&runtime, client_id, "inst").await;
}

#[tokio::test]
async fn read_file_dispatch_complete_explicit_range_keeps_full_range_metadata() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-explicit-range-visible";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let content = "one\ntwo";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project: project,
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "src/lib.rs".to_string(),
                            start_line: Some(1),
                            limit: Some(2),
                            expected_read_revision: None,
                        }],
                        session_id: None,
                        with_line_numbers: None,
                        max_result_bytes: None,
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
    let item = &result.output["items"][0];
    assert_eq!(item["output"]["text"], "one\ntwo");
    assert_eq!(item["output"]["format"], "plain");
    assert_eq!(item["path"], "src/lib.rs");
    assert_eq!(item["output"]["start_line"], 1);
    assert_eq!(item["output"]["limit"], 2);
    assert_eq!(item["output"]["total_lines"], 2);
    assert_eq!(item["output"]["returned_lines"], 2);
    assert_eq!(item["output"]["end_line"], 2);
    assert_eq!(item["output"]["has_more"], false);
    assert!(item["output"]["next_start_line"].is_null());
    assert!(item.get("continuation").is_none());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| {
            panic!("explicit-range one-item read_files success must match schema: {error}")
        });
}

#[tokio::test]
async fn read_files_dispatch_complete_batch_is_sparse_and_schema_valid() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-batch";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
                        max_result_bytes: None,
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
        assert!(item["output"].get("sha256").is_none());
        assert!(item["output"]["read_revision"].as_u64().is_some());
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
async fn read_files_partial_item_has_one_invocation_follow_up() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-batch-item-continuation";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("batch read continuation session".to_string()),
    );
    let session_id = session.session_id.clone();
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![
                            item("src/lib.rs", Some(2), Some(1)),
                            item("src/main.rs", None, None),
                        ],
                        session_id: Some(session_id),
                        with_line_numbers: Some(true),
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..2 {
        let request = next_read_request(&runtime, client_id).await;
        let content = match request.path.as_deref() {
            Some("src/lib.rs") => "one\ntwo\nthree",
            Some("src/main.rs") => "main",
            other => panic!("unexpected read path: {other:?}"),
        };
        complete_read(&runtime, client_id, &request, content).await;
    }

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["output_truncated"], false);
    assert!(result.output.get("continuation").is_none());
    let items = result.output["items"].as_array().unwrap();
    let suggested = &result.output["suggested_call"];
    assert_eq!(suggested["arguments"]["session_id"], session_id);
    assert_eq!(
        suggested["arguments"]["items"][0]["expected_read_revision"],
        items[0]["output"]["read_revision"]
    );
    let next_call = ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .expect("item continuation must parse");
    assert!(matches!(
        next_call,
        ToolCall::ReadFiles {
            project: ref next_project,
            ref items,
            session_id: Some(ref next_session_id),
            with_line_numbers: Some(true),
            max_result_bytes: None,
        } if next_project == &project
            && items.len() == 1
            && items[0].path == "src/lib.rs"
            && items[0].start_line == Some(3)
            && items[0].limit == Some(1)
            && next_session_id == &session_id
    ));
    assert!(items[1].get("continuation").is_none());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&serialized, &schema)
        .unwrap_or_else(|error| panic!("partial read_files item must match schema: {error}"));
}

#[tokio::test]
async fn read_files_dispatch_large_default_batch_uses_sparse_fit_before_budget() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-budget-order";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let auth = auth_context(None, true);
    let paths = (0..8)
        .map(|index| format!("src/{index}.rs"))
        .collect::<Vec<_>>();
    let expected_text = "x".repeat(7_400);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let paths = paths.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: paths.iter().map(|path| item(path, None, None)).collect(),
                        session_id: None,
                        with_line_numbers: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..8 {
        let request = next_read_request(&runtime, client_id).await;
        complete_read(&runtime, client_id, &request, &expected_text).await;
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
        assert_eq!(item["output"]["text"], expected_text);
        assert!(item["output"].get("start_line").is_none());
        assert!(item["output"].get("next_start_line").is_none());
    }
}

#[tokio::test]
async fn read_files_dispatch_mixed_batch_keeps_outer_and_failure_semantics() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "read-sparse-mixed";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
                        items: vec![item("good.txt", None, Some(1)), item(".env", None, None)],
                        session_id: None,
                        with_line_numbers: None,
                        max_result_bytes: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    assert_eq!(request.path.as_deref(), Some("good.txt"));
    complete_read(&runtime, client_id, &request, "ok\nmore").await;

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
    assert_eq!(items[0]["output"]["has_more"], true);
    assert_eq!(
        result.output["suggested_call"]["arguments"]["items"][0]["start_line"],
        2
    );
    let suggested = &result.output["suggested_call"];
    ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .expect("successful mixed-batch item continuation must remain parseable");
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
    register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;

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
async fn read_files_max_batch_can_enqueue_all_eight_independent_reads() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-concurrency";
    register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
    for _ in 0..8 {
        active.push(next_read_request(&runtime, client_id).await);
    }
    assert_eq!(active.len(), 8);
    let extra_before_completion = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap();
    assert!(
        extra_before_completion.is_none(),
        "max-size read batch enqueued work beyond its eight-item bound"
    );

    for request in active.into_iter().rev() {
        complete_read(&runtime, client_id, &request, "value\n").await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 8);
    assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
}

#[tokio::test]
async fn read_files_deadline_preserves_completed_results_and_cancels_unfinished_reads() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests().with_read_files_deadline(Duration::from_millis(75));
    let client_id = "batch-deadline";
    register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
            .runner_registry
            .complete(RunnerResultRequest {
                client_id: client_id.to_string(),
                runner_instance_id: "inst".to_string(),
                request_id: request.request_id.clone(),
                exit_code: Some(0),
                stdout: Some(canonical_agent_file_read_output("late\n", 1)),
                stderr: Some(String::new()),
                stdout_truncated: false,
                stderr_truncated: false,
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
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
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
                        max_result_bytes: None,
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
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert!(result.output.get("session_id").is_none());
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

#[tokio::test]
async fn read_files_direct_session_overlay_pressure_keeps_final_response_under_hard_cap() {
    use crate::tool_runtime::sessions::{SessionTransport, ToolCallRecorderMetadata};
    use webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES as MAX_SERIALIZED_OUTPUT_BYTES;

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-direct-final-cap";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("direct final cap".to_string()));
    seed_recovery_events(&runtime, &session.session_id, &project, 20);
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth_transport_options_and_metadata(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![
                            item("a.rs", None, None),
                            item("b.rs", None, None),
                            item("c.rs", None, None),
                            item("d.rs", None, None),
                        ],
                        session_id: Some(session_id),
                        with_line_numbers: None,
                        max_result_bytes: Some(MAX_SERIALIZED_OUTPUT_BYTES),
                    },
                    Some(&auth),
                    SessionTransport::Mcp,
                    ToolCallRecorderMetadata {
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let content = "x".repeat(150 * 1024);
    for _ in 0..4 {
        let request = next_read_request(&runtime, client_id).await;
        complete_read(&runtime, client_id, &request, &content).await;
    }

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_continuity").is_none());
    assert!(result.output.get("session_recovery").is_none());
    assert!(result.output.get("session_context_revision").is_none());
    let serialized_len = serde_json::to_vec(&result).unwrap().len();
    assert!(
        serialized_len <= MAX_SERIALIZED_OUTPUT_BYTES,
        "direct Session overlays pushed read_files final response above the 512 KiB inspection hard cap: {serialized_len} bytes"
    );
}

#[tokio::test]
async fn ordinary_read_delivers_terminal_attention_without_host_continuation_support() {
    use crate::client_window::ClientWindow;
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };

    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("read-passive-owner");
    let client_id = "read-passive-terminal";
    super::jobs::register_job_agent_for_auth(&runtime, client_id, "repo", &auth).await;
    let project = format!("agent:{client_id}:repo");
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("same-turn passive job attention".to_string()),
    );
    let window = ClientWindow::for_test("read-passive-window");

    let job_id = super::jobs::start_agent_runtime_job_in_session(
        &runtime,
        client_id,
        "repo",
        Some(&session.session_id),
        &auth,
    )
    .await;
    assert_eq!(
        super::jobs::mark_next_agent_job_running(&runtime, client_id).await,
        job_id
    );

    let mut initiating_handoff = ToolResult::ok(json!({
        "execution_state": "pending",
        "continuation": super::super::jobs::observe_job_continuation(&job_id, None),
    }));
    runtime
        .add_passive_job_attention(
            &mut initiating_handoff,
            "run_process",
            Some(&project),
            Some(&session.session_id),
            Some(&window),
            Some(&auth),
        )
        .await;
    assert!(initiating_handoff.output.get("job_attention").is_none());

    let job = runtime
        .runner_registry
        .get_job_for_auth(Some(&crate::test_support::runner_access(&auth)), &job_id)
        .await
        .unwrap();
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client_id.into(),
            runner_instance_id: "inst".into(),
            job_id: job_id.clone(),
            request_id: job.request_id,
            update_seq: Some(2),
            status: "completed".into(),
            stdout_chunk: Some("PRIVATE_JOB_OUTPUT".into()),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(50),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();

    let arguments = json!({
        "project": project,
        "items": [{"path": "a.rs"}],
        "session_id": session.session_id,
    });
    let read = runtime.call_tool_with_invocation_metadata(
        ToolCallRequest {
            tool_name: "read_files".to_string(),
            arguments,
        },
        ToolCallContext {
            transport: ToolTransport::Mcp,
            session_id: Some(&session.session_id),
            auth: Some(&auth),
            window: Some(&window),
            record_oauth_scope_denials: false,
            host_file_import_trust: HostFileImportTrust::Untrusted,
        },
        ToolInvocationMetadata::default(),
        ToolProtocolCapabilities::default(),
    );
    tokio::pin!(read);
    assert!(futures_util::poll!(&mut read).is_pending());
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, "ordinary read\n").await;

    let outcome = read.await;
    let result = outcome
        .result
        .as_ref()
        .expect("model-facing ordinary read result");
    let telemetry = outcome
        .model_ergonomics
        .as_ref()
        .unwrap()
        .record_for_tool_result(result)
        .unwrap();
    assert_eq!(
        telemetry
            .job_convergence
            .as_ref()
            .unwrap()
            .passive_terminal_delivery_count,
        1
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["items"][0]["output"]["text"], "ordinary read");
    let attention = &result.output["job_attention"]["items"][0];
    assert_eq!(attention["job_id"], job_id);
    assert_eq!(attention["state"], "terminal");
    assert_eq!(attention["outcome"], "passed");
    assert_eq!(attention["command_ok"], true);
    assert_eq!(attention["details"]["tool"], "observe_jobs");
    assert!(!result.output["job_attention"]
        .to_string()
        .contains("PRIVATE_JOB_OUTPUT"));

    let second_arguments = json!({
        "project": project,
        "items": [{"path": "b.rs"}],
        "session_id": session.session_id,
    });
    let second = runtime.call_tool_with_invocation_metadata(
        ToolCallRequest {
            tool_name: "read_files".to_string(),
            arguments: second_arguments,
        },
        ToolCallContext {
            transport: ToolTransport::Mcp,
            session_id: Some(&session.session_id),
            auth: Some(&auth),
            window: Some(&window),
            record_oauth_scope_denials: false,
            host_file_import_trust: HostFileImportTrust::Untrusted,
        },
        ToolInvocationMetadata::default(),
        ToolProtocolCapabilities::default(),
    );
    tokio::pin!(second);
    assert!(futures_util::poll!(&mut second).is_pending());
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, "next read\n").await;
    let second = second.await.result.expect("second ordinary read result");
    assert!(second.output.get("job_attention").is_none());
}

#[tokio::test]
async fn read_files_outer_recording_session_preserves_complete_sparse_shape() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-outer-sparse";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("outer sparse read".to_string()));
    let auth = auth_context(None, true);
    let arguments = json!({
        "project": project,
        "items": [{"path": "a.rs"}]
    });

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "read_files".to_string(),
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
                    ToolInvocationMetadata {
                        ..Default::default()
                    },
                    ToolProtocolCapabilities {
                        context_sidecar: true,
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, "small\n").await;

    let result = task.await.unwrap().result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert!(result.output.get("session_id").is_none());
    assert!(result.output.get("session_context_revision").is_none());
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
    assert_eq!(result.output["items"][0]["output"]["text"], "small");
    assert!(result.output["items"][0]["output"].get("path").is_none());
}

#[tokio::test]
async fn read_files_outer_recorder_observes_canonical_batch_before_primary_projection() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-canonical-before-projection";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let recording_session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("canonical batch evidence".to_string()),
    );
    let business_session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("inner business read".to_string()),
    );
    let auth = auth_context(None, true);
    let arguments = json!({
        "project": project,
        "items": [{"path": "a.rs"}, {"path": "b.rs"}],
        "session_id": business_session.session_id,
        "max_result_bytes": 8192
    });

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = recording_session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "read_files".to_string(),
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
                    ToolInvocationMetadata::default(),
                    ToolProtocolCapabilities::default(),
                )
                .await
        }
    });
    let content = format!("{}\n", "x".repeat(3_000));
    for _ in 0..2 {
        let request = next_read_request(&runtime, client_id).await;
        complete_read(&runtime, client_id, &request, &content).await;
    }

    let result = task.await.unwrap().result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["output_truncated"], true);
    assert_eq!(result.output["truncation_reason"], "batch_response_budget");
    assert_eq!(result.output["returned_count"], 1);
    assert!(result.output.get("next_index").is_none());
    assert_eq!(
        result.output["suggested_call"]["arguments"]["session_id"],
        business_session.session_id,
        "model continuation must preserve the concrete business Session rather than inherit the outer recorder"
    );

    let recording_summary = runtime
        .sessions
        .summary(&recording_session.session_id, Some(20))
        .unwrap();
    let recording_event = finished_event(&recording_summary, "read_files");
    assert_eq!(
        recording_event.observed_paths,
        vec!["a.rs".to_string(), "b.rs".to_string()],
        "outer recorder must consume canonical batch evidence before the terminal model budget"
    );
    let business_summary = runtime
        .sessions
        .summary(&business_session.session_id, Some(20))
        .unwrap();
    let business_event = finished_event(&business_summary, "read_files");
    assert_eq!(
        business_event.observed_paths,
        vec!["a.rs".to_string(), "b.rs".to_string()],
        "concrete business Session must remain independently recorded with canonical evidence"
    );
}

#[tokio::test]
async fn read_files_ignores_context_ack_and_preserves_bounded_attention() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };
    use crate::tool_runtime::sessions::{
        PostSessionMessageInput, SessionMessageKind, SessionMessagePriority,
    };
    use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-overlay-bound";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("bounded recovery overlays".to_string()),
    );
    seed_recovery_events(&runtime, &session.session_id, &project, 50);
    seed_large_changed_path_events(&runtime, &session.session_id, &project, 50);
    for kind in [
        SessionMessageKind::Guidance,
        SessionMessageKind::Question,
        SessionMessageKind::Risk,
        SessionMessageKind::Todo,
    ] {
        for index in 0..5 {
            runtime
                .sessions
                .post_message_with_ack(
                    PostSessionMessageInput {
                        session_id: session.session_id.clone(),
                        kind,
                        message: format!("overlay-{kind:?}-{index}-{}", "m".repeat(7_900)),
                        tags: vec!["bounded-overlay".to_string()],
                        reply_to: None,
                        priority: SessionMessagePriority::High,
                    },
                    kind == SessionMessageKind::Guidance,
                )
                .unwrap();
        }
    }
    // Seed an explicit retained attempt boundary after the deliberately large
    // historical overlay. Current validation evidence must not infer a complete
    // attempt from a truncated Session tail.
    runtime
        .sessions
        .ensure_coding_session(crate::tool_runtime::sessions::CodingSessionRequest {
            project: project.clone(),
            authority_fingerprint:
                crate::tool_runtime::sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                    .to_string(),
            resume_session_id: Some(session.session_id.clone()),
            instruction: Some("validate bounded recovery overlays".to_string()),
            mode: crate::tool_runtime::SessionMode::Normal,
            guards: crate::tool_runtime::sessions::SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: crate::tool_runtime::sessions::SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap();
    let assertion_name = "recovery websocket validation";
    let validation_request = json!({
        "project": project,
        "executable": "cargo",
        "args": ["test", "before-recovery"],
        "cwd": ".",
        "purpose": "test",
        "assertion_name": assertion_name,
    });
    let (_, recorder_metadata) = crate::tool_runtime::parse_tool_call_with_recorder_metadata(
        "run_process",
        validation_request.clone(),
    )
    .unwrap();
    let audited = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "run_process",
        &validation_request,
    );
    let validation_start = runtime.sessions.record_tool_call_started_with_metadata(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Mcp,
        "run_process",
        &audited,
        Some(project.clone()),
        recorder_metadata,
        crate::tool_runtime::sessions::session_tool_contract("run_process"),
    );
    runtime.sessions.record_tool_call_finished(
        validation_start,
        false,
        &json!({
            "exit_code": 101,
            "purpose": "test",
            "execution_state": "completed",
            "stdout_tail": "validation failed\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
        }),
        Some("validation failed"),
        None,
    );

    let auth = auth_context(None, true);
    let arguments = json!({
        "project": project,
        "items": [{"path": "a.rs"}]
    });

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "read_files".to_string(),
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
                    ToolInvocationMetadata {
                        context_request: vec!["webcodex.workflow".to_string()],

                        ..Default::default()
                    },
                    ToolProtocolCapabilities {
                        context_sidecar: true,
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let request = next_read_request(&runtime, client_id).await;
    complete_read(&runtime, client_id, &request, "small\n").await;

    let result = task.await.unwrap().result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    for field in [
        "session_context_revision",
        "session_continuity",
        "session_recovery",
    ] {
        assert!(result.output.get(field).is_none(), "{field}");
    }
    assert_eq!(result.output["session_attention"]["requires_ack"], true);
    let attention_messages = result.output["session_attention"]["messages"]
        .as_array()
        .unwrap();
    assert_eq!(attention_messages.len(), 1);
    assert_eq!(attention_messages[0]["message_truncated"], true);
    assert_eq!(result.output["session_attention"]["truncated"], true);
    assert_eq!(result.output["session_attention"]["omitted_count"], 4);
    assert_eq!(
        result.output["context_projection"]["materials"][0]["key"],
        "webcodex.workflow"
    );
    assert!(result.output.get("output_truncated").is_none());
    assert_eq!(result.output["items"].as_array().unwrap().len(), 1);
    let serialized_len = serde_json::to_vec(&result).unwrap().len();
    eprintln!("maximal_bounded_session_overlay_result_bytes={serialized_len}");
    assert!(
        serialized_len <= MAX_SERIALIZED_OUTPUT_BYTES,
        "bounded Session recovery/handoff/attention overlays alone exceeded the 256 KiB hard cap: {serialized_len} bytes"
    );
}

#[tokio::test]
async fn read_files_outer_recording_session_keeps_final_response_under_hard_cap() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolInvocationMetadata,
        ToolProtocolCapabilities, ToolTransport,
    };
    use webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES as MAX_SERIALIZED_OUTPUT_BYTES;

    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "batch-final-cap";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("final response cap".to_string()),
    );
    seed_recovery_events(&runtime, &session.session_id, &project, 20);
    let auth = auth_context(None, true);
    let arguments = json!({
        "project": project,
        "items": [
            {"path": "a.rs"},
            {"path": "b.rs"},
            {"path": "c.rs"},
            {"path": "d.rs"}
        ],
        "max_result_bytes": MAX_SERIALIZED_OUTPUT_BYTES
    });

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "read_files".to_string(),
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
                    ToolInvocationMetadata {
                        ..Default::default()
                    },
                    ToolProtocolCapabilities {
                        context_sidecar: true,
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let content = "x".repeat(150 * 1024);
    for _ in 0..4 {
        let request = next_read_request(&runtime, client_id).await;
        complete_read(&runtime, client_id, &request, &content).await;
    }

    let outcome = task.await.unwrap();
    assert!(outcome.success);
    let result = outcome.result.expect("model-facing result");
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_continuity").is_none());
    assert!(result.output.get("session_recovery").is_none());
    assert!(result.output.get("session_context_revision").is_none());

    assert_eq!(result.output["output_truncated"], true);
    assert_eq!(result.output["truncation_reason"], "hard_result_cap");
    let returned_count = result.output["returned_count"].as_u64().unwrap();
    assert!(returned_count < 4);
    assert_eq!(
        result.output["suggested_call"]["arguments"]["items"]
            .as_array()
            .unwrap()
            .len() as u64,
        4 - returned_count
    );
    let serialized_len = serde_json::to_vec(&result).unwrap().len();
    assert!(
        serialized_len <= MAX_SERIALIZED_OUTPUT_BYTES,
        "outer Session overlays pushed read_files final response above the 512 KiB inspection hard cap: {serialized_len} bytes"
    );
}
