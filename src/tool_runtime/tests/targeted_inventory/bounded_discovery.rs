use super::*;

#[test]
fn bounded_project_discovery_canonical_schema_and_parser_agree() {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "list_projects")
        .unwrap();
    assert_eq!(spec.input_schema["properties"]["offset"]["default"], 0);
    assert_eq!(
        spec.input_schema["properties"]["summary_only"]["default"],
        false
    );
    let call =
        ToolCall::from_tool_name("list_projects", json!({"offset":100,"limit":100})).unwrap();
    assert!(matches!(
        call,
        ToolCall::ListProjects {
            offset: 100,
            limit: Some(100),
            summary_only: false,
            ..
        }
    ));
    assert!(ToolCall::from_tool_name("list_projects", json!({"offset":-1})).is_err());
}

#[tokio::test]
async fn bounded_project_discovery_default_page_sizes() {
    let schema = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "list_projects")
        .unwrap()
        .output_schema;
    for count in [0usize, 1, 30, 100, 101, 211] {
        let runtime = test_runtime();
        register_target_agent(&runtime, "bounded", large_fixture_projects(count), None).await;
        let result = runtime
            .dispatch(list_projects_call(None, None, None, None, false))
            .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["matched_count"], count);
        assert_eq!(
            result.output["count"],
            count.min(30),
            "inventory size {count}"
        );
        assert_eq!(result.output["truncated"], count > 30);
        webcodex_tool_contracts::test_support::validate_schema_instance(
            &serde_json::to_value(&result).unwrap(),
            &schema,
        )
        .unwrap();
    }
}

#[tokio::test]
async fn bounded_project_discovery_payload_measurement() {
    let runtime = test_runtime();
    let mut projects = large_fixture_projects(210);
    projects.push(registered_project("webcodex", "/tmp/webcodex"));
    register_target_agent(&runtime, "bounded", projects, None).await;
    let full = runtime
        .dispatch(list_projects_call(None, None, None, None, false))
        .await;
    let summary = runtime
        .dispatch(list_projects_call(None, None, None, None, true))
        .await;
    let error = runtime
        .resolve_project_input_for_auth("webcodx", None)
        .await
        .unwrap_err()
        .into_tool_result();
    println!(
        "PROJECT_BYTES canonical={} summary={} typo_error={}",
        serde_json::to_vec(&full).unwrap().len(),
        serde_json::to_vec(&summary).unwrap().len(),
        serde_json::to_vec(&error).unwrap().len()
    );
    assert_eq!(summary.output["count"], 30);
    assert_eq!(error.output["candidates"].as_array().unwrap().len(), 5);
    assert_eq!(
        error.output["candidates"][0]["id"],
        "agent:bounded:webcodex"
    );
}
fn paged_call(arguments: serde_json::Value) -> ToolCall {
    ToolCall::from_tool_name("list_projects", arguments).unwrap()
}

#[tokio::test]
async fn bounded_project_discovery_pages_all_107_matches_after_filtering() {
    let runtime = test_runtime();
    let mut all = large_fixture_projects(211);
    for (index, project) in all.iter_mut().enumerate() {
        project.description = Some(if index < 107 { "webcodex" } else { "other" }.into());
    }
    register_target_agent(&runtime, "z-last", all[..100].to_vec(), None).await;
    register_target_agent(&runtime, "a-first", all[100..].to_vec(), None).await;
    let mut ids = Vec::new();
    let mut offset = 0;
    loop {
        let result = runtime
            .dispatch(paged_call(json!({
                "query":"WEBCODEX", "limit":40, "offset":offset, "summary_only":true
            })))
            .await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["matched_count"], 107);
        assert_eq!(result.output["offset"], offset);
        assert!(result.output["count"].as_u64().unwrap() <= 40);
        ids.extend(
            result.output["projects"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["id"].as_str().unwrap().to_owned()),
        );
        match result.output["next_offset"].as_u64() {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    let mut expected = (0..107)
        .map(|i| {
            format!(
                "agent:{}:project-{i:04}",
                if i < 100 { "z-last" } else { "a-first" }
            )
        })
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(ids, expected);
    for id in ids {
        assert_eq!(
            runtime
                .resolve_project_input_for_auth(&id, None)
                .await
                .unwrap()
                .resolved_id,
            id
        );
    }
    let maxed = runtime.dispatch(paged_call(json!({"limit":100_000}))).await;
    assert_eq!(maxed.output["count"], 100);
    assert_eq!(maxed.output["limit"], 100);
    let beyond = runtime.dispatch(paged_call(json!({"offset":1000}))).await;
    assert_eq!(beyond.output["count"], 0);
    assert_eq!(beyond.output["matched_count"], 211);
    assert_eq!(beyond.output["next_offset"], serde_json::Value::Null);
    assert_eq!(beyond.output["truncated"], false);
    let console = runtime
        .query_projects_for_runtime_console(None, None, None, None, 211)
        .await
        .unwrap();
    assert_eq!(console.projects.len(), 211);
}

#[tokio::test]
async fn bounded_project_discovery_offset_is_live_and_restartable() {
    let runtime = test_runtime();
    let projects = ["b", "c", "d", "e"]
        .into_iter()
        .map(|id| registered_project(id, &format!("/tmp/{id}")))
        .collect::<Vec<_>>();
    register_target_agent(&runtime, "live", projects.clone(), None).await;
    let first = runtime.dispatch(paged_call(json!({"limit":2}))).await;
    assert_eq!(first.output["projects"][1]["id"], "agent:live:c");
    let mut inserted = projects.clone();
    inserted.push(registered_project("a", "/tmp/a"));
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "live",
        "inst-live",
        inserted,
    )
    .await;
    let after_insert = runtime
        .dispatch(paged_call(json!({"limit":2,"offset":2})))
        .await;
    assert_eq!(
        after_insert.output["projects"][0]["id"], "agent:live:c",
        "insertion before a live offset can repeat a row"
    );
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "live",
        "inst-live",
        projects[1..].to_vec(),
    )
    .await;
    let after_delete = runtime
        .dispatch(paged_call(json!({"limit":2,"offset":2})))
        .await;
    assert_eq!(
        after_delete.output["projects"][0]["id"], "agent:live:e",
        "deletion before a live offset can omit a row"
    );
    let restart = runtime
        .dispatch(paged_call(json!({"limit":100,"offset":0})))
        .await;
    assert_eq!(restart.output["count"], 3);
    assert_eq!(restart.output["projects"][1]["id"], "agent:live:d");
}

#[tokio::test]
async fn bounded_project_discovery_authorization_and_provenance() {
    let runtime = test_runtime();
    let alice = shared_key_auth_context("bounded-alice");
    let bob = shared_key_auth_context("bounded-bob");
    register_target_agent_for_auth(&runtime, "alice", "public-name", &alice).await;
    register_target_agent_for_auth(&runtime, "bob", "hidden-canary", &bob).await;
    let visible = runtime
        .dispatch_with_auth(paged_call(json!({"offset":0})), Some(&alice))
        .await;
    assert_eq!(visible.output["matched_count"], 1);
    assert!(!serde_json::to_string(&visible)
        .unwrap()
        .contains("hidden-canary"));
    let past = runtime
        .dispatch_with_auth(paged_call(json!({"offset":1})), Some(&alice))
        .await;
    assert_eq!(past.output["count"], 0);
    let changed_identity = runtime
        .dispatch_with_auth(paged_call(json!({"offset":0})), Some(&bob))
        .await;
    assert_eq!(changed_identity.output["matched_count"], 1);
    assert_eq!(
        changed_identity.output["projects"][0]["id"],
        "agent:bob:hidden-canary"
    );
    let error = runtime
        .resolve_project_input_for_auth("webcodx", Some(&alice))
        .await
        .unwrap_err()
        .into_tool_result();
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("hidden-canary"));
    assert_eq!(error.output["candidate_count"], 1);
    let hidden = runtime
        .resolve_project_input_for_auth("agent:bob:hidden-canary", Some(&alice))
        .await
        .unwrap_err()
        .into_tool_result();
    let missing = runtime
        .resolve_project_input_for_auth("agent:missing:missing", Some(&alice))
        .await
        .unwrap_err()
        .into_tool_result();
    assert_eq!(hidden.output["candidates"], missing.output["candidates"]);
    assert_eq!(hidden.output["candidate_count"], 0);

    let runtime = test_runtime();
    let mut generated = registered_project("generated", "/tmp/ordinary-path");
    generated.registration_source = Some("auto_registered".into());
    generated.kind = Some("managed_worktree".into());
    let manual = registered_project("manual", "/tmp/.webcodex-worktrees/manual");
    register_target_agent(&runtime, "sources", vec![generated, manual], None).await;
    for query in ["auto_registered", "managed_worktree"] {
        let result = runtime
            .dispatch(paged_call(json!({"query":query,"summary_only":true})))
            .await;
        assert_eq!(result.output["matched_count"], 1);
        assert_eq!(
            result.output["projects"][0]["id"],
            "agent:sources:generated"
        );
        assert_eq!(result.output["projects"][0]["source"], "auto_registered");
    }
    let ordinary = runtime
        .dispatch(paged_call(
            json!({"project":"agent:sources:manual","summary_only":true}),
        ))
        .await;
    assert_eq!(ordinary.output["projects"][0]["source"], "agent_registered");
}

#[tokio::test]
async fn bounded_project_discovery_error_fields_and_ranking_are_bounded() {
    let runtime = test_runtime();
    let mut projects = large_fixture_projects(20);
    let mut target = registered_project("webcodex", "/tmp/webcodex");
    target.name = Some("WebCodex".into());
    projects.push(target);
    register_target_agent(&runtime, "ranking", projects, None).await;
    for spelling in ["WEBCODEX", "webcod", "webcodx"] {
        let result = runtime
            .resolve_project_input_for_auth(spelling, None)
            .await
            .unwrap_err()
            .into_tool_result();
        assert_eq!(result.output["candidates"].as_array().unwrap().len(), 5);
        assert_eq!(
            result.output["candidates"][0]["id"],
            "agent:ranking:webcodex"
        );
        assert_eq!(result.output["candidate_count"], 21);
        assert_eq!(result.output["candidates_truncated"], true);
        assert!(
            !result.error.unwrap().contains("agent:ranking:webcodex"),
            "candidate identities belong in the structured list, never repeated in error prose"
        );
    }
    let input = "恶意\n\u{202e}".repeat(10_000);
    let mut error = runtime
        .resolve_project_input_for_auth(&input, None)
        .await
        .unwrap_err();
    for candidate in &mut error.candidates {
        candidate.name = Some("长名称".repeat(10_000));
        candidate.path = "/".repeat(100_000);
        candidate.status = "异常".repeat(10_000);
    }
    error.candidates[0].id = "agent:".to_string() + &"x".repeat(100_000);
    let result = error.into_tool_result();
    let bytes = serde_json::to_vec(&result).unwrap();
    assert!(bytes.len() < 12_000, "{}", bytes.len());
    assert_eq!(result.output["project_truncated"], true);
    assert!(result.output["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| { c["id"].as_str().unwrap().len() < 512 && c.get("path").is_none() }));
    let empty = test_runtime()
        .resolve_project_input_for_auth("webcodx", None)
        .await
        .unwrap_err()
        .into_tool_result();
    let stale_ref = runtime
        .resolve_project_input_for_auth("~p999", None)
        .await
        .unwrap_err()
        .into_tool_result();
    assert!(stale_ref.output["suggested_call"]["arguments"]
        .get("query")
        .is_none());
    assert_eq!(empty.output["candidate_count"], 0);
    assert_eq!(empty.output["candidates"], json!([]));
}
