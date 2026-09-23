use super::*;
use serde::Serialize;
use serde_json::{json, Value};
use webcodex_core::lsp_bridge::{
    CallHierarchyDirection, CallHierarchyResult, DocumentDiagnosticsResult,
    DocumentDiagnosticsStatus, DocumentSymbolsResult, HoverResult, LocationsResult,
    LspAvailabilityStatus, LspCommandSource, LspServerStatusEntry, LspStatusResult,
    PublicCallHierarchySymbol, PublicDiagnostic, PublicDiagnosticSeverity, PublicDiagnosticTag,
    PublicHover, PublicHoverKind, PublicLocation, PublicPosition, PublicRange, PublicSymbol,
    PublicWorkspaceSymbol, WorkspaceSymbolsResult,
};
use webcodex_core::runner_protocol::{
    RunnerConfigAction, RunnerConfigErrorCode, RunnerConfigErrorField, RunnerConfigErrorReason,
    RunnerConfigExecutionState, RunnerConfigOperationResponse,
};

fn tool_result_instance<T: Serialize>(success: bool, payload: &T) -> Value {
    let mut instance = json!({
        "success": success,
        "output": serde_json::to_value(payload).expect("typed payload serializes")
    });
    if !success {
        instance["error"] = json!("expected test failure");
    }
    instance
}

fn assert_registered_schema_accepts<T: Serialize>(tool: &str, success: bool, payload: &T) {
    let schema = output_schema_for_tool(tool);
    let instance = tool_result_instance(success, payload);
    test_support::validate_schema_instance(&instance, &schema)
        .unwrap_or_else(|error| panic!("{tool} rejected canonical typed payload: {error}"));
}

fn position(line: usize, column: usize) -> PublicPosition {
    PublicPosition { line, column }
}

fn range() -> PublicRange {
    PublicRange {
        start: position(1, 1),
        end: position(1, 4),
    }
}

fn runner_config_response(
    action: RunnerConfigAction,
    execution_state: RunnerConfigExecutionState,
    valid: Option<bool>,
    current_generation: Option<u64>,
    error_code: Option<RunnerConfigErrorCode>,
    error_field: Option<RunnerConfigErrorField>,
    error_reason: Option<RunnerConfigErrorReason>,
    restart_required: bool,
    restart_required_fields: Vec<String>,
) -> RunnerConfigOperationResponse {
    RunnerConfigOperationResponse {
        action,
        execution_state,
        valid,
        current_generation,
        error_code,
        error_field,
        error_reason,
        restart_required,
        restart_required_fields,
    }
}

#[test]
fn runner_config_registered_output_schema_accepts_canonical_runtime_states() {
    let cases = [
        runner_config_response(
            RunnerConfigAction::Check,
            RunnerConfigExecutionState::Completed,
            Some(true),
            Some(7),
            None,
            None,
            None,
            false,
            vec![],
        ),
        runner_config_response(
            RunnerConfigAction::Check,
            RunnerConfigExecutionState::Completed,
            Some(false),
            Some(7),
            Some(RunnerConfigErrorCode::ConfigValidationFailed),
            Some(RunnerConfigErrorField::MaxConcurrentJobs),
            Some(RunnerConfigErrorReason::OutOfRange),
            false,
            vec![],
        ),
        runner_config_response(
            RunnerConfigAction::Check,
            RunnerConfigExecutionState::Completed,
            Some(false),
            Some(7),
            Some(RunnerConfigErrorCode::ConfigValidationFailed),
            Some(RunnerConfigErrorField::SkillsRoots),
            Some(RunnerConfigErrorReason::InvalidPath),
            false,
            vec![],
        ),
        runner_config_response(
            RunnerConfigAction::Reload,
            RunnerConfigExecutionState::NotStarted,
            None,
            Some(7),
            Some(RunnerConfigErrorCode::ConfigGenerationConflict),
            None,
            None,
            false,
            vec![],
        ),
        runner_config_response(
            RunnerConfigAction::Reload,
            RunnerConfigExecutionState::OutcomeUnknown,
            None,
            None,
            Some(RunnerConfigErrorCode::OutcomeUnknown),
            None,
            None,
            false,
            vec![],
        ),
        runner_config_response(
            RunnerConfigAction::Reload,
            RunnerConfigExecutionState::Completed,
            Some(true),
            Some(8),
            None,
            None,
            None,
            true,
            vec!["capabilities".to_string()],
        ),
    ];

    for response in &cases {
        response
            .validate()
            .expect("representative Runner result is valid");
        let tool = match response.action {
            RunnerConfigAction::Check => "runner_config_check",
            RunnerConfigAction::Reload => "runner_config_reload",
        };
        assert_registered_schema_accepts(
            tool,
            response.execution_state == RunnerConfigExecutionState::Completed
                && response.valid == Some(true),
            response,
        );
    }
}

#[test]
fn runner_config_registered_output_schema_uses_canonical_closed_vocabulary() {
    let response = runner_config_response(
        RunnerConfigAction::Check,
        RunnerConfigExecutionState::Completed,
        Some(false),
        Some(3),
        Some(RunnerConfigErrorCode::ConfigValidationFailed),
        Some(RunnerConfigErrorField::SkillsRoots),
        Some(RunnerConfigErrorReason::InvalidPath),
        false,
        vec![],
    );
    response.validate().unwrap();
    let schema = output_schema_for_tool("runner_config_check");
    let mut instance = tool_result_instance(false, &response);
    test_support::validate_schema_instance(&instance, &schema).unwrap();

    instance["output"]["error_field"] = json!("future.config.field");
    assert!(test_support::validate_schema_instance(&instance, &schema).is_err());

    let mut instance = tool_result_instance(false, &response);
    instance["output"]["error_reason"] = json!("future_reason");
    assert!(test_support::validate_schema_instance(&instance, &schema).is_err());

    let encoded = serde_json::to_string(&schema).unwrap();
    assert!(encoded.contains("skills.roots"));
    assert!(encoded.contains("invalid_path"));
}

#[test]
fn typed_output_envelope_keeps_sparse_failure_and_runtime_decorations_legal() {
    let schema = output_schema_for_tool("runner_config_reload");
    let sparse_failure = json!({
        "success": false,
        "error": "Runner config operation was not started",
        "output": {
            "execution_state": "not_started",
            "error_code": "runner_unavailable",
            "recovery_kind": "reobserve",
            "trace_ref": "opaque-trace"
        }
    });
    test_support::validate_schema_instance(&sparse_failure, &schema).unwrap();
}

#[test]
fn lsp_registered_output_schemas_accept_canonical_typed_results() {
    let status = LspStatusResult {
        project: "agent:test:demo".to_string(),
        detected_languages: vec!["rust".to_string()],
        servers: vec![LspServerStatusEntry {
            language: "rust".to_string(),
            server: "rust-analyzer".to_string(),
            available: true,
            running: true,
            status: LspAvailabilityStatus::Running,
            source: Some(LspCommandSource::Path),
            position_encoding: Some("utf-16".to_string()),
        }],
        warnings: vec![],
    };
    assert_registered_schema_accepts("lsp_status", true, &status);

    let symbols = DocumentSymbolsResult {
        project: "agent:test:demo".to_string(),
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        symbols: vec![PublicSymbol {
            name: "demo".to_string(),
            kind: "function".to_string(),
            kind_code: 12,
            detail: Some("fn demo()".to_string()),
            range: range(),
            selection_range: range(),
            children: vec![],
        }],
        total_count: 1,
        returned_count: 1,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
    };
    assert_registered_schema_accepts("document_symbols", true, &symbols);

    let diagnostics = DocumentDiagnosticsResult {
        project: "agent:test:demo".to_string(),
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        diagnostics: vec![PublicDiagnostic {
            range: range(),
            severity: PublicDiagnosticSeverity::Error,
            severity_code: Some(1),
            code: Some("E0001".to_string()),
            source: Some("rust-analyzer".to_string()),
            message: "example diagnostic".to_string(),
            tags: vec![PublicDiagnosticTag::Deprecated],
        }],
        total_count: 1,
        returned_count: 1,
        truncated: false,
        status: DocumentDiagnosticsStatus::Complete,
        clean: Some(false),
        published_version: Some(1),
        invalid_results_omitted: 0,
        related_information_omitted: 0,
    };
    assert_registered_schema_accepts("document_diagnostics", true, &diagnostics);

    let hover = HoverResult {
        project: "agent:test:demo".to_string(),
        path: "src/lib.rs".to_string(),
        position: position(1, 2),
        hover: Some(PublicHover {
            kind: PublicHoverKind::Markdown,
            value: "`demo`".to_string(),
            range: Some(range()),
        }),
        truncated: false,
        range_omitted: false,
    };
    assert_registered_schema_accepts("hover", true, &hover);

    let workspace = WorkspaceSymbolsResult {
        project: "agent:test:demo".to_string(),
        query: "demo".to_string(),
        symbols: vec![PublicWorkspaceSymbol {
            name: "demo".to_string(),
            kind: "function".to_string(),
            kind_code: 12,
            container_name: Some("crate".to_string()),
            path: "src/lib.rs".to_string(),
            range: Some(range()),
        }],
        total_results: 1,
        returned_count: 1,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
    };
    assert_registered_schema_accepts("workspace_symbols", true, &workspace);

    let locations = LocationsResult {
        project: "agent:test:demo".to_string(),
        path: "src/lib.rs".to_string(),
        query_position: position(1, 2),
        locations: vec![PublicLocation {
            path: "src/lib.rs".to_string(),
            range: range(),
            target_range: None,
        }],
        total_results: 1,
        returned_count: 1,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
    };
    assert_registered_schema_accepts("goto_definition", true, &locations);
    assert_registered_schema_accepts("find_references", true, &locations);

    let call_hierarchy = CallHierarchyResult {
        project: "agent:test:demo".to_string(),
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        query_position: position(1, 2),
        direction: CallHierarchyDirection::Both,
        depth: 1,
        roots: vec![PublicCallHierarchySymbol {
            name: "demo".to_string(),
            kind: "function".to_string(),
            kind_code: 12,
            path: "src/lib.rs".to_string(),
            range: range(),
            selection_range: range(),
        }],
        root_total_count: 1,
        root_returned_count: 1,
        edges: vec![],
        returned_count: 0,
        truncated: false,
        external_results_omitted: 0,
        invalid_results_omitted: 0,
        call_site_ranges_omitted: 0,
    };
    assert_registered_schema_accepts("call_hierarchy", true, &call_hierarchy);
}

#[test]
fn lsp_typed_fields_are_closed_while_intentional_projection_boundaries_stay_open() {
    let diagnostics_schema = output_schema_for_tool("document_diagnostics");
    let diagnostic =
        &diagnostics_schema["properties"]["output"]["properties"]["diagnostics"]["items"];
    assert_eq!(diagnostic["additionalProperties"], false);
    assert_eq!(diagnostic["properties"]["message"]["maxLength"], 4096);

    let mut invalid = json!({
        "success": true,
        "output": {
            "diagnostics": [{
                "range": {"start":{"line":1,"column":1},"end":{"line":1,"column":2}},
                "severity": "future_severity",
                "severity_code": 99,
                "code": null,
                "source": null,
                "message": "x",
                "tags": []
            }]
        }
    });
    assert!(test_support::validate_schema_instance(&invalid, &diagnostics_schema).is_err());
    invalid["output"]["diagnostics"][0]["severity"] = json!("error");
    test_support::validate_schema_instance(&invalid, &diagnostics_schema).unwrap();

    let hover_schema = output_schema_for_tool("hover");
    let mut invalid_hover = json!({
        "success": true,
        "output": {
            "hover": {"kind":"html","value":"x","range":null}
        }
    });
    assert!(test_support::validate_schema_instance(&invalid_hover, &hover_schema).is_err());
    invalid_hover["output"]["hover"]["kind"] = json!("plaintext");
    test_support::validate_schema_instance(&invalid_hover, &hover_schema).unwrap();

    let call_schema = output_schema_for_tool("call_hierarchy");
    let bad_depth = json!({"success":true,"output":{"depth":3}});
    assert!(test_support::validate_schema_instance(&bad_depth, &call_schema).is_err());

    for (tool, field) in [
        ("lsp_status", "servers"),
        ("document_symbols", "symbols"),
        ("goto_definition", "locations"),
        ("find_references", "locations"),
        ("call_hierarchy", "roots"),
        ("call_hierarchy", "edges"),
    ] {
        let schema = output_schema_for_tool(tool);
        let item = &schema["properties"]["output"]["properties"][field]["items"];
        assert_eq!(item["type"], "object", "{tool}.{field}");
        assert_eq!(item["additionalProperties"], true, "{tool}.{field}");
    }
}

#[test]
fn apply_text_edits_success_match_ranges_require_occurrence_while_conflicts_keep_it_optional() {
    let schema = output_schema_for_tool("apply_text_edits");
    let success_range = &schema["properties"]["output"]["properties"]["files"]["items"]
        ["properties"]["edits"]["items"]["properties"]["match_ranges"]["items"];
    assert_eq!(
        success_range["required"],
        json!(["occurrence", "start_line", "end_line"])
    );

    let conflict_range = &schema["properties"]["output"]["properties"]["candidate_ranges"]["items"];
    assert_eq!(
        conflict_range["required"],
        json!(["start_line", "end_line"]),
        "unguarded conflict candidates intentionally omit occurrence"
    );
}

#[test]
fn typed_output_schemas_use_host_normalized_inline_shapes() {
    for name in [
        "runner_config_check",
        "runner_config_reload",
        "lsp_status",
        "document_symbols",
        "document_diagnostics",
        "hover",
        "workspace_symbols",
        "goto_definition",
        "find_references",
        "call_hierarchy",
    ] {
        let encoded = serde_json::to_string(&output_schema_for_tool(name)).unwrap();
        assert!(!encoded.contains("$defs"), "{name}");
        assert!(!encoded.contains("$ref"), "{name}");
        assert!(!encoded.contains("nullable"), "{name}");
    }
}
