use super::support::*;
use crate::lsp_bridge::{
    parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspResultEnvelope,
    DocumentSymbolsResult, LocationsResult, LspAvailabilityStatus, LspCommandSource,
    LspStatusResult, PublicLocation, PublicPosition, PublicRange, PublicSymbol,
    AGENT_LSP_REQUEST_KIND,
};
use crate::shell_protocol::{
    ShellClientCapabilities, ShellClientRegisterRequest,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
};
use crate::tool_runtime::tool_definition::{
    lookup_tool_definition, model_visible_tool_definitions, AgentCapability, TOOL_CATEGORY_LSP,
};
use crate::tool_runtime::{ToolCall, ToolResult};
use serde_json::json;

#[test]
fn lsp_tools_are_registered_read_only_and_not_shell_like() {
    for name in [
        "lsp_status",
        "document_symbols",
        "goto_definition",
        "find_references",
    ] {
        let def = lookup_tool_definition(name).expect(name);
        assert_eq!(def.category, TOOL_CATEGORY_LSP, "{name}");
        assert!(def.metadata.read_only, "{name}");
        assert!(!def.metadata.destructive, "{name}");
        assert!(!def.metadata.shell_like, "{name}");
        assert_eq!(
            def.agent_capability,
            Some(AgentCapability::LspReadOnlyNavigation),
            "{name}"
        );
        assert_eq!(
            def.metadata.oauth_scope,
            Some(crate::tool_runtime::metadata::PROJECT_READ),
            "{name}"
        );
    }
    let names: Vec<_> = model_visible_tool_definitions().map(|d| d.name).collect();
    for name in [
        "lsp_status",
        "document_symbols",
        "goto_definition",
        "find_references",
    ] {
        assert!(names.contains(&name), "missing {name} in known tools");
    }
}

#[test]
fn lsp_input_schemas_have_required_bounds() {
    use crate::tool_runtime::registry::registered_tool_specs;
    let specs = registered_tool_specs();
    let by_name: std::collections::HashMap<_, _> =
        specs.into_iter().map(|s| (s.name.clone(), s)).collect();

    let status = &by_name["lsp_status"].input_schema;
    assert_eq!(status["required"], json!(["project"]));
    assert_eq!(status["additionalProperties"], false);

    let symbols = &by_name["document_symbols"].input_schema;
    assert_eq!(symbols["required"], json!(["project", "path"]));
    assert_eq!(symbols["properties"]["limit"]["maximum"], 500);
    assert_eq!(symbols["additionalProperties"], false);

    let goto = &by_name["goto_definition"].input_schema;
    assert_eq!(
        goto["required"],
        json!(["project", "path", "line", "column"])
    );
    assert_eq!(goto["properties"]["line"]["minimum"], 1);
    assert_eq!(goto["properties"]["column"]["minimum"], 1);
    assert_eq!(goto["properties"]["limit"]["maximum"], 100);

    let refs = &by_name["find_references"].input_schema;
    assert_eq!(
        refs["required"],
        json!(["project", "path", "line", "column"])
    );
    assert_eq!(refs["properties"]["include_declaration"]["default"], true);
    assert_eq!(refs["properties"]["limit"]["maximum"], 200);
    assert_eq!(refs["additionalProperties"], false);

    // Flattened Action fields must list path/line/column/include_declaration/limit.
    use crate::tool_runtime::accepted_flattened_args_for_spec;
    let flat_goto = accepted_flattened_args_for_spec(&by_name["goto_definition"]);
    for field in ["project", "path", "line", "column", "limit", "session_id"] {
        assert!(
            flat_goto.iter().any(|f| f == field),
            "goto missing flattened {field}: {flat_goto:?}"
        );
    }
    let flat_refs = accepted_flattened_args_for_spec(&by_name["find_references"]);
    for field in [
        "project",
        "path",
        "line",
        "column",
        "include_declaration",
        "limit",
        "session_id",
    ] {
        assert!(
            flat_refs.iter().any(|f| f == field),
            "refs missing flattened {field}: {flat_refs:?}"
        );
    }
}

async fn register_lsp_agent(
    runtime: &crate::tool_runtime::ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &std::path::Path,
    lsp_capable: bool,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write: true,
                lsp_read_only_navigation: lsp_capable,
                ..Default::default()
            }),
            projects: Some(vec![registered_project(project_id, &project_path)]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    crate::tool_runtime::agent_project_runtime_id(client_id, project_id)
}

async fn complete_lsp_agent_request(
    runtime: &crate::tool_runtime::ToolRuntime,
    client_id: &str,
    result: impl serde::Serialize,
) {
    let mut req = None;
    for _ in 0..200 {
        req = next_patch_agent_request(runtime, client_id).await;
        if req.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let req = req.expect("expected LSP agent request");
    assert_eq!(req.kind, AGENT_LSP_REQUEST_KIND);
    assert!(req.lsp.is_some());
    assert!(req.command.is_empty());
    let envelope = AgentLspResultEnvelope::ok(result);
    complete_patch_agent_request(
        runtime,
        client_id,
        &req.request_id,
        0,
        &envelope.to_stdout_json(),
        "",
    )
    .await;
}

fn document_symbols_result(path: &str) -> DocumentSymbolsResult {
    DocumentSymbolsResult {
        project: "demo".into(),
        path: path.into(),
        language: "rust".into(),
        symbols: vec![],
        total_count: 0,
        returned_count: 0,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
    }
}

async fn dispatch_document_symbols_with_result_path(client_id: &str, path: &str) -> ToolResult {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, client_id, "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::DocumentSymbols {
                        project,
                        path: "src/main.rs".into(),
                        limit: Some(10),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(&runtime, client_id, document_symbols_result(path)).await;
    task.await.unwrap()
}

#[tokio::test]
async fn capability_missing_blocks_dispatch() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "old-agent", "demo", tmp.path(), false).await;
    let auth = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::LspStatus {
                project,
                session_id: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap_or_default();
    assert!(
        err.contains("agent_capability_unavailable")
            || err.contains(SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION),
        "{err}"
    );
}

#[tokio::test]
async fn lsp_status_unavailable_still_succeeds() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-agent", "demo", tmp.path(), true).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::LspStatus {
                        project,
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(
        &runtime,
        "lsp-agent",
        LspStatusResult {
            project: "demo".into(),
            detected_languages: vec![],
            servers: vec![crate::lsp_bridge::LspServerStatusEntry {
                language: "rust".into(),
                server: "rust-analyzer".into(),
                available: false,
                running: false,
                status: LspAvailabilityStatus::Unavailable,
                source: None,
                position_encoding: None,
            }],
            warnings: vec![],
        },
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["servers"][0]["available"], false);
    assert_eq!(result.output["servers"][0]["status"], "unavailable");
    assert!(!result.output.to_string().contains("file://"));
    let _ = auth;
}

#[tokio::test]
async fn document_symbols_and_locations_are_normalized() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-nav", "demo", tmp.path(), true).await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::DocumentSymbols {
                        project,
                        path: "src/main.rs".into(),
                        limit: Some(10),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(
        &runtime,
        "lsp-nav",
        DocumentSymbolsResult {
            project: "demo".into(),
            path: "src/main.rs".into(),
            language: "rust".into(),
            symbols: vec![PublicSymbol {
                name: "main".into(),
                kind: "function".into(),
                kind_code: 12,
                detail: None,
                range: PublicRange {
                    start: PublicPosition { line: 1, column: 1 },
                    end: PublicPosition { line: 1, column: 4 },
                },
                selection_range: PublicRange {
                    start: PublicPosition { line: 1, column: 1 },
                    end: PublicPosition { line: 1, column: 4 },
                },
                children: vec![],
            }],
            total_count: 1,
            returned_count: 1,
            truncated: false,
            external_results_omitted: 0,
            invalid_results_omitted: 0,
        },
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["symbols"][0]["name"], "main");
    assert_eq!(result.output["path"], "src/main.rs");
    assert!(result.output["project"]
        .as_str()
        .unwrap()
        .starts_with("agent:"));
}

#[tokio::test]
async fn goto_definition_multiple_locations() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-def", "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GotoDefinition {
                        project,
                        path: "src/main.rs".into(),
                        line: 1,
                        column: 1,
                        limit: Some(20),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(
        &runtime,
        "lsp-def",
        LocationsResult {
            project: "demo".into(),
            path: "src/main.rs".into(),
            query_position: PublicPosition { line: 1, column: 1 },
            locations: vec![
                PublicLocation {
                    path: "src/main.rs".into(),
                    range: PublicRange {
                        start: PublicPosition { line: 1, column: 1 },
                        end: PublicPosition { line: 1, column: 4 },
                    },
                    target_range: None,
                },
                PublicLocation {
                    path: "src/lib.rs".into(),
                    range: PublicRange {
                        start: PublicPosition { line: 2, column: 1 },
                        end: PublicPosition { line: 2, column: 3 },
                    },
                    target_range: None,
                },
            ],
            total_results: 2,
            returned_count: 2,
            truncated: false,
            external_results_omitted: 1,
            invalid_results_omitted: 0,
        },
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["returned_count"], 2);
    assert_eq!(result.output["external_results_omitted"], 1);
    assert!(!result.output.to_string().contains("file://"));
}

#[tokio::test]
async fn lsp_result_boundary_rejects_absolute_paths_and_file_uris() {
    for (index, path) in [
        "/tmp/main.rs",
        r"C:\repo\src\main.rs",
        r"\\server\share\main.rs",
        "file:///tmp/main.rs",
    ]
    .into_iter()
    .enumerate()
    {
        let result =
            dispatch_document_symbols_with_result_path(&format!("lsp-path-reject-{index}"), path)
                .await;
        assert!(!result.success, "path must be rejected: {path:?}");
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("malformed_agent_lsp_result"),
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn lsp_result_boundary_accepts_project_relative_paths() {
    let result =
        dispatch_document_symbols_with_result_path("lsp-path-relative", "src/main.rs").await;
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["path"], "src/main.rs");
}

#[tokio::test]
async fn read_only_session_allows_lsp_tools() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-ro", "demo", tmp.path(), true).await;
    let auth = auth_context(None, true);
    let start = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some(project.clone()),
                title: Some("lsp-ro".into()),
                mode: crate::tool_runtime::SessionMode::ReadOnly,
                deny_write_tools: true,
                deny_shell_tools: true,
            },
            Some(&auth),
        )
        .await;
    assert!(start.success, "{start:?}");
    let session_id = start.output["session_id"].as_str().unwrap().to_string();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::LspStatus {
                        project,
                        session_id: Some(session_id),
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(
        &runtime,
        "lsp-ro",
        LspStatusResult {
            project: "demo".into(),
            detected_languages: vec!["rust".into()],
            servers: vec![crate::lsp_bridge::LspServerStatusEntry {
                language: "rust".into(),
                server: "rust-analyzer".into(),
                available: true,
                running: false,
                status: LspAvailabilityStatus::Available,
                source: Some(LspCommandSource::Path),
                position_encoding: None,
            }],
            warnings: vec![],
        },
    )
    .await;
    let result = task.await.unwrap();
    assert!(
        result.success,
        "read_only session must allow lsp tools: {result:?}"
    );
}

#[test]
fn malformed_agent_envelope_is_rejected() {
    assert!(parse_agent_lsp_result_envelope("hello").is_err());
    assert!(
        parse_agent_lsp_result_envelope(r#"{"format":"nope","success":true,"result":{}}"#).is_err()
    );
    let ok = AgentLspResultEnvelope::ok(json!({"ok": true}));
    let parsed = parse_agent_lsp_result_envelope(&ok.to_stdout_json()).unwrap();
    assert!(parsed.success);
}

#[test]
fn typed_payload_rejects_arbitrary_operation() {
    let bad = r#"{"project_id":"p","request":{"operation":"workspace_symbols"}}"#;
    assert!(serde_json::from_str::<AgentLspPayload>(bad).is_err());
    let old_request = r#"{"request_id":"r","client_id":"c","command":"echo","timeout_secs":1,"requested_by":"t","created_at":1}"#;
    let req: crate::shell_protocol::ShellAgentShellRequest =
        serde_json::from_str(old_request).unwrap();
    assert!(req.lsp.is_none());
}

#[tokio::test]
async fn capability_default_false_on_old_registration() {
    let runtime = test_runtime();
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: "legacy".into(),
            agent_instance_id: "inst".into(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: Some(serde_json::from_value(json!({"shell": true})).unwrap()),
            projects: None,
            agent_protocol_version: Some("polling-v1".into()),
            policy: None,
        })
        .await
        .unwrap();
    assert!(!runtime
        .shell_clients
        .client_supports("legacy", SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION)
        .await
        .unwrap());
}
