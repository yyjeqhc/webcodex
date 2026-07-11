use super::support::*;
use crate::lsp_bridge::{
    parse_agent_lsp_result_envelope, AgentLspPayload, AgentLspResultEnvelope,
    DocumentDiagnosticsResult, DocumentSymbolsResult, HoverResult, LocationsResult,
    LspAvailabilityStatus, LspStatusResult, PublicDiagnostic, PublicHover, PublicLocation,
    PublicPosition, PublicRange, PublicSymbol, PublicWorkspaceSymbol, WorkspaceSymbolsResult,
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
        "document_diagnostics",
        "hover",
        "workspace_symbols",
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
        "document_diagnostics",
        "hover",
        "workspace_symbols",
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

    let diagnostics = &by_name["document_diagnostics"].input_schema;
    assert_eq!(diagnostics["required"], json!(["project", "path"]));
    assert_eq!(diagnostics["properties"]["limit"]["minimum"], 1);
    assert_eq!(diagnostics["properties"]["limit"]["maximum"], 200);
    assert_eq!(diagnostics["properties"]["limit"]["default"], 100);
    assert_eq!(diagnostics["additionalProperties"], false);
    let diagnostics_output = &by_name["document_diagnostics"].output_schema;
    let output_properties = &diagnostics_output["properties"]["output"]["properties"];
    for field in [
        "project",
        "path",
        "language",
        "diagnostics",
        "total_count",
        "returned_count",
        "truncated",
        "fresh",
        "timed_out",
        "published_version",
        "invalid_results_omitted",
        "related_information_omitted",
    ] {
        assert!(
            output_properties.get(field).is_some(),
            "diagnostics output schema missing {field}"
        );
    }
    let diagnostic_item = &output_properties["diagnostics"]["items"];
    assert_eq!(diagnostic_item["additionalProperties"], false);
    assert_eq!(diagnostic_item["properties"]["message"]["maxLength"], 4096);
    assert!(diagnostic_item["properties"].get("data").is_none());
    assert!(diagnostic_item["properties"]
        .get("relatedInformation")
        .is_none());

    let hover = &by_name["hover"].input_schema;
    assert_eq!(
        hover["required"],
        json!(["project", "path", "line", "column"])
    );
    assert_eq!(hover["properties"]["line"]["minimum"], 1);
    assert_eq!(hover["properties"]["column"]["minimum"], 1);
    assert_eq!(hover["additionalProperties"], false);
    let hover_output = &by_name["hover"].output_schema["properties"]["output"]["properties"];
    assert_eq!(
        hover_output["hover"]["anyOf"][0]["properties"]["value"]["maxLength"],
        16384
    );

    let workspace = &by_name["workspace_symbols"].input_schema;
    assert_eq!(workspace["required"], json!(["project", "query"]));
    assert_eq!(workspace["properties"]["query"]["minLength"], 1);
    assert_eq!(workspace["properties"]["query"]["maxLength"], 200);
    assert_eq!(workspace["properties"]["limit"]["default"], 50);
    assert_eq!(workspace["properties"]["limit"]["maximum"], 200);
    assert_eq!(workspace["additionalProperties"], false);
    let workspace_item = &by_name["workspace_symbols"].output_schema["properties"]["output"]
        ["properties"]["symbols"]["items"];
    assert_eq!(workspace_item["additionalProperties"], false);
    assert!(workspace_item["properties"].get("uri").is_none());
    assert!(workspace_item["properties"].get("data").is_none());

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
    let flat_diagnostics = accepted_flattened_args_for_spec(&by_name["document_diagnostics"]);
    for field in ["project", "path", "limit", "session_id"] {
        assert!(
            flat_diagnostics.iter().any(|item| item == field),
            "diagnostics missing flattened {field}: {flat_diagnostics:?}"
        );
    }
    let flat_hover = accepted_flattened_args_for_spec(&by_name["hover"]);
    for field in ["project", "path", "line", "column", "session_id"] {
        assert!(flat_hover.iter().any(|item| item == field));
    }
    let flat_workspace = accepted_flattened_args_for_spec(&by_name["workspace_symbols"]);
    for field in ["project", "query", "limit", "session_id"] {
        assert!(flat_workspace.iter().any(|item| item == field));
    }
}

#[test]
fn document_diagnostics_tool_call_parser_produces_only_typed_fields() {
    let call = ToolCall::from_tool_name(
        "document_diagnostics",
        json!({
            "project": "agent:oe:demo",
            "path": "src/main.rs",
            "limit": 25,
            "session_id": "wc_sess_demo"
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::DocumentDiagnostics {
            project,
            path,
            limit: Some(25),
            session_id: Some(session_id),
        } if project == "agent:oe:demo"
            && path == "src/main.rs"
            && session_id == "wc_sess_demo"
    ));
    let call_with_ignored_internal_extra = ToolCall::from_tool_name(
        "document_diagnostics",
        json!({"project": "agent:oe:demo", "path": "src/main.rs", "timeout": 30}),
    )
    .unwrap();
    assert!(matches!(
        call_with_ignored_internal_extra,
        ToolCall::DocumentDiagnostics {
            limit: None,
            session_id: None,
            ..
        }
    ));
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

fn document_diagnostics_result(path: &str) -> DocumentDiagnosticsResult {
    DocumentDiagnosticsResult {
        project: "demo".into(),
        path: path.into(),
        language: "rust".into(),
        diagnostics: vec![PublicDiagnostic {
            range: PublicRange {
                start: PublicPosition { line: 1, column: 1 },
                end: PublicPosition { line: 1, column: 2 },
            },
            severity: "warning".into(),
            severity_code: Some(2),
            code: Some("unused".into()),
            source: Some("rust-analyzer".into()),
            message: "unused item".into(),
            tags: vec!["unnecessary".into()],
        }],
        total_count: 1,
        returned_count: 1,
        truncated: false,
        fresh: true,
        timed_out: false,
        published_version: Some(2),
        invalid_results_omitted: 0,
        related_information_omitted: 0,
    }
}

fn hover_result(path: &str) -> HoverResult {
    HoverResult {
        project: "demo".into(),
        path: path.into(),
        position: PublicPosition { line: 1, column: 1 },
        hover: Some(PublicHover {
            kind: "markdown".into(),
            value: "`main`".into(),
            range: None,
        }),
        truncated: false,
        range_omitted: false,
    }
}

fn workspace_symbols_result() -> WorkspaceSymbolsResult {
    WorkspaceSymbolsResult {
        project: "demo".into(),
        query: "ToolRuntime".into(),
        symbols: vec![PublicWorkspaceSymbol {
            name: "ToolRuntime".into(),
            kind: "struct".into(),
            kind_code: 23,
            container_name: None,
            path: "src/tool_runtime/mod.rs".into(),
            range: None,
        }],
        total_results: 1,
        returned_count: 1,
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
            ToolCall::DocumentDiagnostics {
                project,
                path: "src/main.rs".into(),
                limit: None,
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
async fn disconnected_agent_blocks_document_diagnostics_dispatch() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "offline-lsp", "demo", tmp.path(), true).await;
    runtime
        .shell_clients
        .reconcile_disconnect("offline-lsp", "inst")
        .await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::DocumentDiagnostics {
                project,
                path: "src/main.rs".into(),
                limit: None,
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success, "{result:?}");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("not connected"),
        "{result:?}"
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
async fn document_diagnostics_dispatches_typed_result_without_process_output() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-diagnostics", "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::DocumentDiagnostics {
                        project,
                        path: "src/main.rs".into(),
                        limit: Some(100),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(
        &runtime,
        "lsp-diagnostics",
        document_diagnostics_result("src/main.rs"),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["diagnostics"][0]["severity"], "warning");
    assert_eq!(result.output["fresh"], true);
    assert_eq!(result.output["timed_out"], false);
    let serialized = result.output.to_string();
    assert!(!serialized.contains("stdout"));
    assert!(!serialized.contains("stderr"));
    assert!(!serialized.contains("file://"));
}

#[tokio::test]
async fn document_diagnostics_result_boundary_rejects_embedded_absolute_paths() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project =
        register_lsp_agent(&runtime, "lsp-diagnostic-path", "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::DocumentDiagnostics {
                        project,
                        path: "src/main.rs".into(),
                        limit: Some(100),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let mut result = document_diagnostics_result("src/main.rs");
    result.diagnostics[0].message = "compiler opened /tmp/private.rs".into();
    complete_lsp_agent_request(&runtime, "lsp-diagnostic-path", result).await;
    let result = task.await.unwrap();
    assert!(!result.success, "{result:?}");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("malformed_agent_lsp_result"),
        "{result:?}"
    );
}

#[tokio::test]
async fn hover_dispatches_typed_normalized_result() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-hover", "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::Hover {
                        project,
                        path: "src/main.rs".into(),
                        line: 1,
                        column: 1,
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(&runtime, "lsp-hover", hover_result("src/main.rs")).await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["hover"]["kind"], "markdown");
    assert_eq!(result.output["path"], "src/main.rs");
    assert!(!result.output.to_string().contains("file://"));
}

#[tokio::test]
async fn workspace_symbols_dispatches_typed_bounded_result() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-workspace", "demo", tmp.path(), true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::WorkspaceSymbols {
                        project,
                        query: "  ToolRuntime  ".into(),
                        limit: Some(50),
                        session_id: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    complete_lsp_agent_request(&runtime, "lsp-workspace", workspace_symbols_result()).await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["query"], "ToolRuntime");
    assert_eq!(
        result.output["symbols"][0]["path"],
        "src/tool_runtime/mod.rs"
    );
    assert!(!result.output.to_string().contains("file://"));
}

#[tokio::test]
async fn hover_and_workspace_symbols_validate_arguments_before_agent_enqueue() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project = register_lsp_agent(&runtime, "lsp-invalid", "demo", tmp.path(), true).await;
    for call in [
        ToolCall::Hover {
            project: project.clone(),
            path: "src/main.rs".into(),
            line: 0,
            column: 1,
            session_id: None,
        },
        ToolCall::WorkspaceSymbols {
            project: project.clone(),
            query: "   ".into(),
            limit: None,
            session_id: None,
        },
        ToolCall::WorkspaceSymbols {
            project: project.clone(),
            query: "x".repeat(201),
            limit: None,
            session_id: None,
        },
    ] {
        let result = runtime
            .dispatch_with_auth(call, Some(&auth_context(None, true)))
            .await;
        assert!(!result.success, "{result:?}");
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("invalid_arguments"),
            "{result:?}"
        );
    }
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
                    ToolCall::DocumentDiagnostics {
                        project,
                        path: "src/main.rs".into(),
                        limit: Some(100),
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
        document_diagnostics_result("src/main.rs"),
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
    let bad = r#"{"project_id":"p","request":{"operation":"arbitrary_passthrough","method":"workspace/symbol"}}"#;
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
