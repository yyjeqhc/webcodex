use super::super::runtime_info::ListRunnersOptions;
use super::super::{ToolCall, ToolRuntime};
use super::support::*;
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest};
use serde_json::{json, Value};

async fn register(
    runtime: &ToolRuntime,
    client_id: &str,
    project_count: usize,
    auth: Option<&AuthContext>,
) {
    runtime
        .runner_registry
        .register_with_auth(
            RunnerRegisterRequest {
                owner: auth.and_then(|auth| auth.username.clone()),
                display_name: Some(format!("Runner {client_id}")),
                job_concurrency_limit: Some(4),
                ..crate::test_support::runner_registration(
                    client_id,
                    &format!("inst-{client_id}"),
                    RunnerCapabilities {
                        git: true,
                        async_shell_jobs: true,
                        ..Default::default()
                    },
                )
            },
            auth.map(crate::test_support::runner_access).as_ref(),
        )
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        &format!("inst-{client_id}"),
        (0..project_count)
            .map(|index| {
                registered_project(
                    &format!("project-{index:03}"),
                    &format!("/fixture/project-{index:03}"),
                )
            })
            .collect(),
    )
    .await;
}

fn call(arguments: Value) -> ToolCall {
    ToolCall::from_tool_name("list_runners", arguments).unwrap()
}

#[tokio::test]
async fn runner_discovery_default_is_sparse_with_211_projects() {
    let runtime = test_runtime();
    register(&runtime, "large", 211, None).await;
    let default = runtime.dispatch(call(json!({}))).await;
    let explicit_summary = runtime.dispatch(call(json!({"summary_only": true}))).await;
    let full = runtime
        .dispatch(call(
            json!({"summary_only": false, "include_projects": true}),
        ))
        .await;
    assert!(default.success && explicit_summary.success && full.success);
    eprintln!(
        "runner_discovery_211_bytes default={} explicit_summary={} detailed_projects={}",
        serde_json::to_vec(&default).unwrap().len(),
        serde_json::to_vec(&explicit_summary).unwrap().len(),
        serde_json::to_vec(&full).unwrap().len(),
    );
    let runner = &default.output["runners"][0];
    assert_eq!(runner["projects_count"], 211);
    for field in [
        "projects",
        "project_inventory",
        "capabilities",
        "build",
        "host_context",
        "policy",
        "shell_profiles",
    ] {
        assert!(runner.get(field).is_none(), "default exposed {field}");
    }
    assert_eq!(default.output["count"], 1);
}

#[tokio::test]
async fn runner_discovery_default_bounds_fleet_and_keeps_operator_inventory() {
    let runtime = test_runtime();
    for index in (0..131).rev() {
        register(&runtime, &format!("runner-{index:03}"), 0, None).await;
    }
    let result = runtime.dispatch(call(json!({}))).await;
    assert!(result.success);
    assert_eq!(result.output["count"], 30);
    assert_eq!(result.output["matched_count"], 131);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["next_offset"], 30);
    let operator = runtime
        .query_runner_inventory(
            None,
            &ListRunnersOptions {
                summary_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        operator.runners.len(),
        131,
        "Console's shared query remains explicitly complete"
    );
}
#[tokio::test]
async fn runner_discovery_pages_all_107_matches_after_filtering_and_clamps_limits() {
    let runtime = test_runtime();
    for index in (0..107).rev() {
        register(&runtime, &format!("match-{index:03}"), 0, None).await;
    }
    register(&runtime, "other", 0, None).await;
    let mut ids = Vec::new();
    let mut offset = 0;
    loop {
        let page = runtime
            .dispatch(call(json!({"query":"MATCH-", "offset":offset})))
            .await;
        assert!(page.success, "{:?}", page.error);
        assert_eq!(page.output["matched_count"], 107);
        assert_eq!(page.output["limit"], 30);
        assert_eq!(page.output["offset"], offset);
        assert_eq!(page.output["truncated"], page.output["has_more"]);
        let rows = page.output["runners"].as_array().unwrap();
        ids.extend(
            rows.iter()
                .map(|row| row["client_id"].as_str().unwrap().to_string()),
        );
        match page.output["next_offset"].as_u64() {
            Some(next) => {
                assert_eq!(page.output["has_more"], true);
                offset = next;
            }
            None => {
                assert_eq!(page.output["has_more"], false);
                break;
            }
        }
    }
    assert_eq!(
        ids,
        (0..107)
            .map(|index| format!("match-{index:03}"))
            .collect::<Vec<_>>()
    );
    for (limit, expected) in [
        (0, 1),
        (1, 1),
        (30, 30),
        (100, 100),
        (101, 100),
        (usize::MAX, 100),
    ] {
        let page = runtime.dispatch(call(json!({"limit":limit}))).await;
        assert!(page.success);
        assert_eq!(page.output["count"], expected);
        assert_eq!(page.output["limit"], expected);
    }
    for offset in [108, usize::MAX] {
        let page = runtime.dispatch(call(json!({"offset":offset}))).await;
        assert!(page.success);
        assert_eq!(page.output["count"], 0);
        assert_eq!(page.output["matched_count"], 108);
        assert_eq!(page.output["has_more"], false);
        assert!(page.output["next_offset"].is_null());
    }
    let exact = runtime
        .dispatch(call(json!({"client_id":"match-106"})))
        .await;
    assert_eq!(exact.output["count"], 1);
    assert_eq!(exact.output["runners"][0]["client_id"], "match-106");
}

#[tokio::test]
async fn runner_discovery_details_keep_sync_status_and_bound_explicit_projects() {
    let runtime = test_runtime();
    register(&runtime, "large", 211, None).await;
    let details = runtime.dispatch(call(json!({"summary_only":false}))).await;
    assert!(details.success);
    let runner = &details.output["runners"][0];
    assert!(runner.get("projects").is_none());
    assert!(runner["project_inventory"].get("sync_state").is_some());
    assert_eq!(runner["project_inventory"]["total_synced"], 211);
    assert!(runner["project_inventory"].get("projects").is_none());
    assert!(runner.get("build").is_some());
    assert!(runner.get("capabilities").is_some());
    let expanded = runtime
        .dispatch(call(json!({"summary_only":false, "include_projects":true})))
        .await;
    let runner = &expanded.output["runners"][0];
    assert_eq!(runner["projects"].as_array().unwrap().len(), 100);
    assert_eq!(runner["projects_count"], 211);
    assert_eq!(runner["projects_truncated"], true);
    assert_eq!(runner["projects"][0]["id"], "project-000");
    assert_eq!(runner["projects"][99]["id"], "project-099");
    assert!(runner["projects_hint"]
        .as_str()
        .unwrap()
        .contains("list_runners(client_id=\"large\", include_projects=true, projects_offset=100)"));
}

#[tokio::test]
async fn runner_discovery_pages_reauthorize_and_hidden_matches_equal_missing() {
    let runtime = test_runtime();
    let mut alice = auth_context(Some("alice"), false);
    alice.scopes = vec![
        crate::auth::SCOPE_RUNTIME_READ.into(),
        crate::auth::SCOPE_PROJECT_READ.into(),
    ];
    let mut bob = auth_context(Some("bob"), false);
    bob.scopes = alice.scopes.clone();
    for index in 0..31 {
        register(&runtime, &format!("alice-{index:03}"), 0, Some(&alice)).await;
    }
    register(&runtime, "bob-secret", 201, Some(&bob)).await;
    let first = runtime
        .dispatch_with_auth(call(json!({})), Some(&alice))
        .await;
    assert!(first.success);
    assert_eq!(first.output["matched_count"], 31);
    assert!(!first.output.to_string().contains("bob-secret"));
    let second = runtime
        .dispatch_with_auth(call(json!({"offset":30})), Some(&alice))
        .await;
    assert_eq!(second.output["count"], 1);
    assert_eq!(second.output["runners"][0]["client_id"], "alice-030");
    let changed_identity = runtime
        .dispatch_with_auth(call(json!({"offset":30})), Some(&bob))
        .await;
    assert_eq!(changed_identity.output["matched_count"], 1);
    assert_eq!(changed_identity.output["count"], 0);
    assert!(!changed_identity.output.to_string().contains("alice"));
    for summary_only in [true, false] {
        let hidden = runtime.dispatch_with_auth(call(json!({"client_id":"bob-secret","summary_only":summary_only,"include_projects":true})), Some(&alice)).await;
        let missing = runtime.dispatch_with_auth(call(json!({"client_id":"missing","summary_only":summary_only,"include_projects":true})), Some(&alice)).await;
        assert_eq!(hidden.output, missing.output);
    }
}

#[tokio::test]
async fn runner_discovery_offset_is_a_fresh_inventory_not_a_snapshot_lease() {
    let runtime = test_runtime();
    for index in 0..31 {
        register(&runtime, &format!("runner-{index:03}"), 0, None).await;
    }
    let first = runtime.dispatch(call(json!({}))).await;
    assert_eq!(first.output["runners"][29]["client_id"], "runner-029");
    register(&runtime, "runner-000a", 0, None).await;
    let second = runtime
        .dispatch(call(json!({"offset":first.output["next_offset"]})))
        .await;
    assert_eq!(second.output["matched_count"], 32);
    assert_eq!(
        second.output["runners"][0]["client_id"], "runner-029",
        "an insertion before the offset can repeat an entry; callers must restart to reconcile"
    );
}

#[test]
fn runner_discovery_schema_parser_and_audit_agree_on_defaults_and_offset() {
    let schema = webcodex_tool_contracts::input_schema_for_tool("list_runners");
    assert_eq!(schema["properties"]["summary_only"]["default"], true);
    assert_eq!(schema["properties"]["offset"]["type"], "integer");
    assert_eq!(schema["properties"]["projects_offset"]["type"], "integer");
    assert!(matches!(
        call(json!({})),
        ToolCall::ListRunners {
            summary_only: true,
            offset: 0,
            ..
        }
    ));
    assert!(matches!(
        call(json!({"offset":100,"summary_only":false})),
        ToolCall::ListRunners {
            summary_only: false,
            offset: 100,
            ..
        }
    ));
    assert!(ToolCall::from_tool_name("list_runners", json!({"offset":-1})).is_err());
    assert!(ToolCall::from_tool_name("list_runners", json!({"offest":1})).is_err());
    let audit = super::super::tool_audit::session_log_arguments_for_tool_request(
        "list_runners",
        &json!({"offset":100,"query":"private-query","client_id":"private-runner"}),
    );
    assert_eq!(audit["offset"], 100);
    assert!(!audit.to_string().contains("private-query"));
    assert!(!audit.to_string().contains("private-runner"));
}
#[tokio::test]
async fn runner_discovery_project_pages_are_complete_without_resource_root_identity_or_source_cap()
{
    for count in [201, 2101] {
        let runtime = test_runtime();
        register(&runtime, "large", count, None).await;
        let mut expected = (0..count)
            .map(|index| format!("project-{index:03}"))
            .collect::<Vec<_>>();
        expected.sort();
        let offsets = if count == 201 {
            vec![0, 100, 200]
        } else {
            vec![2000, 2100]
        };
        let mut collected = Vec::new();
        for offset in offsets {
            let result = runtime
                .dispatch(call(json!({
                    "client_id":"large", "include_projects":true, "projects_offset":offset
                })))
                .await;
            assert!(result.success, "{:?}", result.error);
            let runner = &result.output["runners"][0];
            assert!(
                runner.get("capabilities").is_none(),
                "Project preview does not require full diagnostics"
            );
            assert_eq!(runner["projects_count"], count);
            assert_eq!(runner["projects_offset"], offset);
            let projects = runner["projects"].as_array().unwrap();
            let end = (offset + 100).min(count);
            assert_eq!(runner["projects_returned_count"], end - offset);
            let ids = projects
                .iter()
                .map(|project| {
                    assert!(project.get("root_fingerprint").is_none_or(Value::is_null));
                    project["id"].as_str().unwrap().to_string()
                })
                .collect::<Vec<_>>();
            assert_eq!(ids, expected[offset..end]);
            collected.extend(ids);
            assert_eq!(
                runner["projects_next_offset"],
                if end < count { json!(end) } else { Value::Null }
            );
            assert_eq!(runner["projects_truncated"], end < count);
        }
        if count == 201 {
            assert_eq!(collected, expected);
        }
        let beyond = runtime
            .dispatch(call(json!({
                "client_id":"large","include_projects":true,"projects_offset":usize::MAX
            })))
            .await;
        assert!(beyond.success);
        assert_eq!(beyond.output["runners"][0]["projects_returned_count"], 0);
        assert!(beyond.output["runners"][0]["projects_next_offset"].is_null());
        assert_eq!(beyond.output["runners"][0]["projects_truncated"], false);
    }
}
