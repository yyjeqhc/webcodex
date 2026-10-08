use super::*;
use std::collections::HashSet;

#[test]
fn bundled_app_registry_has_unique_exact_identities_and_stable_discovery() {
    let expected = [
        "ui://webcodex/pdf/v6",
        "ui://webcodex/spreadsheet/v1",
        "ui://webcodex/computer/v12",
        "ui://webcodex/workbench/v2",
        "ui://webcodex/work-result/v30",
        "ui://webcodex/goal-plan/v7",
        "ui://webcodex/agent-continuation/v18",
        "ui://webcodex/job-terminal-continuation/v2",
        "ui://webcodex/docx/v2",
    ];
    let listed = resources_list(None);
    let uris: Vec<_> = listed["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap())
        .collect();
    assert_eq!(uris, expected);
    assert_eq!(listed, resources_list(None));
    assert_eq!(BUILTIN_MCP_APPS.len(), expected.len() + 1);
    let mut identities = HashSet::new();
    let mut tools = HashSet::new();
    for app in BUILTIN_MCP_APPS {
        assert!(identities.insert(app.uri), "duplicate URI: {}", app.uri);
        assert!(std::ptr::eq(for_uri(app.uri).unwrap(), app));
        for tool in app.tools {
            assert!(tools.insert(*tool), "ambiguous tool binding: {tool}");
            assert!(std::ptr::eq(for_tool(tool).unwrap(), app));
        }
        for extra in ["/", "?version=1", "#cached", " "] {
            assert!(for_uri(&format!("{}{extra}", app.uri)).is_none());
        }
    }
    assert_eq!(tools.len(), 9);
    for retired in [
        "ui://webcodex/docx/v1",
        "ui://webcodex/spreadsheet/v7",
        "ui://webcodex/pdf/v1",
        "ui://webcodex/pdf/v2",
        "ui://webcodex/computer/v11",
        "ui://webcodex/changes/v3",
        "ui://webcodex/workbench/v1",
        "ui://webcodex/work-result/v27",
        "ui://webcodex/work-result/v26",
        "ui://webcodex/work-result/v25",
        "ui://webcodex/work-result/v24",
        "ui://webcodex/work-result/v28",
        "ui://webcodex/work-result/v29",
        "ui://webcodex/work-result/v23",
        "ui://webcodex/work-result/v22",
        "ui://webcodex/work-result/v21",
        "ui://webcodex/goal-plan/v6",
        "ui://webcodex/agent-continuation/v17",
        "ui://webcodex/job-terminal-continuation/v1",
    ] {
        assert!(
            for_uri(retired).is_none(),
            "retired alias admitted: {retired}"
        );
    }
}

#[test]
fn bundled_app_registry_preserves_read_only_cached_resource_without_new_bindings() {
    let cached = for_uri(MCP_RESULT_UI_RESOURCE_URI).unwrap();
    assert!(cached.listing.is_none());
    assert!(cached.tools.is_empty());
    assert_eq!(
        cached.read(None)["contents"][0]["text"],
        MCP_RESULT_APP_HTML
    );
    for ordinary in [
        "work_on_project",
        "start_agent_task_attempt",
        "run_process",
        "run_shell",
        "edit_project_files",
        "read_files",
        "search_project_texts",
        "review_changes",
        "cargo_test",
        "observe_jobs",
        "wait_for_job_readiness",
        "finish_coding_task",
        "plugin_tool",
        "observe_computer",
        "get_work_result_state",
        "sync_goal_plan",
        "work_result_thread_panel",
        "future_tool",
    ] {
        assert!(
            for_tool(ordinary).is_none(),
            "unexpected binding: {ordinary}"
        );
    }
    for (tool, uri) in [
        ("open_webcodex_workbench", MCP_WORKBENCH_UI_RESOURCE_URI),
        ("present_pdf", MCP_PDF_UI_RESOURCE_URI),
        ("present_work_result", MCP_WORK_RESULT_UI_RESOURCE_URI),
        ("present_spreadsheet", MCP_SPREADSHEET_UI_RESOURCE_URI),
        ("present_goal_plan", MCP_GOAL_PLAN_UI_RESOURCE_URI),
        ("present_docx", MCP_DOCX_UI_RESOURCE_URI),
        (
            "present_agent_continuation",
            MCP_AGENT_CONTINUATION_UI_RESOURCE_URI,
        ),
        (
            "wait_for_agent_events",
            MCP_AGENT_CONTINUATION_UI_RESOURCE_URI,
        ),
        (
            "present_job_terminal_continuation",
            MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI,
        ),
    ] {
        assert_eq!(for_tool(tool).unwrap().uri, uri);
    }
}

#[test]
fn bundled_app_registry_keeps_resource_csp_display_modes_and_cache_policy_separate() {
    for domain in [None, Some("https://self-host.example")] {
        let meta = resource_meta(domain);
        assert_eq!(
            meta["ui"]["csp"],
            json!({"connectDomains": [], "resourceDomains": []})
        );
        assert_eq!(meta["ui"]["prefersBorder"], true);
        assert_eq!(meta["ui"].get("domain"), domain.map(Value::from).as_ref());
        for resource in resources_list(domain)["resources"].as_array().unwrap() {
            let mut expected = meta.clone();
            if resource["uri"] == MCP_SPREADSHEET_UI_RESOURCE_URI {
                expected["ui"]["csp"]["resourceDomains"] = json!(["blob:"]);
            }
            assert_eq!(resource["_meta"], expected);
            assert_eq!(resource["mimeType"], MCP_UI_RESOURCE_MIME_TYPE);
        }
        for app in BUILTIN_MCP_APPS {
            let read = app.read(domain);
            let contents = read["contents"].as_array().unwrap();
            assert_eq!(contents.len(), 1);
            assert_eq!(contents[0]["uri"], app.uri);
            assert_eq!(contents[0]["text"], app.html);
            assert_eq!(contents[0]["mimeType"], MCP_UI_RESOURCE_MIME_TYPE);
            let mut expected = meta.clone();
            if app.uri == MCP_SPREADSHEET_UI_RESOURCE_URI {
                expected["ui"]["csp"]["resourceDomains"] = json!(["blob:"]);
            }
            assert_eq!(contents[0]["_meta"]["ui"], expected["ui"]);
            if matches!(
                app.uri,
                MCP_WORKBENCH_UI_RESOURCE_URI
                    | MCP_DOCX_UI_RESOURCE_URI
                    | MCP_SPREADSHEET_UI_RESOURCE_URI
            ) {
                assert_eq!(
                    contents[0]["_meta"]["openai/ui"],
                    json!({
                        "availableDisplayModes": ["inline", "fullscreen"], "preferredDisplayMode": "inline",
                    })
                );
            } else if app.uri == MCP_PDF_UI_RESOURCE_URI {
                assert_eq!(
                    contents[0]["_meta"]["openai/ui"],
                    json!({"availableDisplayModes": ["fullscreen", "inline"], "preferredDisplayMode": "fullscreen"})
                );
            } else {
                assert_eq!(contents[0]["_meta"], meta);
            }
            assert_eq!(
                app.cache_ttl_ms,
                (app.uri == MCP_COMPUTER_UI_RESOURCE_URI).then_some(0)
            );
        }
    }
}

#[test]
fn bundled_app_registry_metadata_preserves_existing_descriptor_fields() {
    for app in BUILTIN_MCP_APPS {
        let before = json!({
            "name": "fixture", "title": "Existing", "inputSchema": {"type":"object"},
            "annotations": {"readOnlyHint": true},
            "_meta": {"ui":{"other":true},"openai/fileParams":["file"]},
        });
        let mut descriptor = before.clone();
        super::super::tools::attach_app_metadata(&mut descriptor, app.uri);
        app.add_tool_metadata(&mut descriptor);
        assert_eq!(descriptor["inputSchema"], before["inputSchema"]);
        assert_eq!(descriptor["annotations"], before["annotations"]);
        assert_eq!(
            descriptor["_meta"]["openai/fileParams"],
            before["_meta"]["openai/fileParams"]
        );
        assert_eq!(descriptor["_meta"]["ui"]["other"], true);
        assert_eq!(descriptor["_meta"]["ui"]["resourceUri"], app.uri);
        assert_eq!(
            descriptor["title"],
            app.tool_title
                .map(Value::from)
                .unwrap_or_else(|| before["title"].clone())
        );
        if app.uri == MCP_WORKBENCH_UI_RESOURCE_URI {
            assert_eq!(
                descriptor["_meta"]["openai/ui"],
                json!({"entrypoints":[{"type":"global"},{"type":"thread"}]})
            );
        } else {
            assert!(descriptor["_meta"].get("openai/ui").is_none());
        }
    }
}

#[test]
fn bundled_app_registry_composition_does_not_bypass_capability_or_model_visibility() {
    for compact in [false, true] {
        for enabled in [false, true] {
            let payload =
                super::super::tools::mcp_tools_list_payload_with_compact_and_app(compact, enabled);
            let tools = payload["tools"].as_array().unwrap();
            assert!(!tools
                .iter()
                .any(|tool| tool["name"] == "present_agent_continuation"));
            for descriptor in tools {
                let name = descriptor["name"].as_str().unwrap();
                let expected = enabled.then(|| for_tool(name)).flatten().map(|app| app.uri);
                assert_eq!(
                    descriptor
                        .pointer("/_meta/ui/resourceUri")
                        .and_then(Value::as_str),
                    expected,
                    "{name}: enabled={enabled}"
                );
            }
        }
    }
}
