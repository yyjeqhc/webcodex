use super::*;

use crate::tool_runtime::metadata::{
    ToolPathHint, ToolRisk, PROJECT_WRITE, TOOL_PROVIDER_AGENT, TOOL_PROVIDER_UNKNOWN,
};

struct LegacyMetadataFallback {
    name: &'static str,
    reason: &'static str,
}

const NON_RUNTIME_METADATA_COMPATIBILITY_NAMES: &[&str] = &["delete_files"];

// TODO(tool-definition): delete this allowlist when the legacy dedicated
// delete-files HTTP route is removed or is represented outside the runtime tool
// metadata facade.
const KNOWN_LEGACY_METADATA_FALLBACKS: &[LegacyMetadataFallback] = &[LegacyMetadataFallback {
    name: "delete_files",
    reason: "legacy dedicated HTTP route metadata; not accepted by ToolCall and not a runtime tool",
}];

#[test]
fn tool_definition_runtime_tool_policy_inventory_is_stable() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, runtime_tool_allows_current_session_fallback,
        runtime_tool_creates_or_binds_session, runtime_tool_is_current_session_control,
        runtime_tool_requires_explicit_business_session, tool_definitions,
    };

    // Compact inventory: (name, category, session policy label). The other
    // columns of the old twelve-line-per-tool golden table (risk class,
    // read/write/shell/git classification, permission requirement, agent
    // capability) are derived from metadata by the invariants in `policy.rs`
    // and pinned per tool by
    // `required_agent_capability_matches_metadata_risk_table`. The session
    // policy label stays: it is hand-set per definition and pinned nowhere
    // else.
    let expected: &[(&str, &str, &str)] = &[
        ("list_tools", "runtime", "none"),
        ("start_session", "session", "creates_or_binds"),
        ("start_coding_task", "workflow", "creates_or_binds"),
        ("work_on_project", "workflow", "creates_or_binds"),
        (
            "finish_coding_task",
            "workflow",
            "explicit_business_session",
        ),
        ("session_summary", "session", "explicit_business_session"),
        (
            "update_session_context",
            "session",
            "explicit_business_session",
        ),
        ("close_session", "session", "explicit_business_session"),
        (
            "validation_summary",
            "validation",
            "explicit_business_session",
        ),
        (
            "post_session_message",
            "session",
            "explicit_business_session",
        ),
        (
            "list_session_messages",
            "session",
            "explicit_business_session",
        ),
        (
            "resolve_session_message",
            "session",
            "explicit_business_session",
        ),
        (
            "session_discussion_summary",
            "session",
            "explicit_business_session",
        ),
        (
            "session_handoff_summary",
            "session",
            "explicit_business_session",
        ),
        (
            "bind_current_session",
            "session",
            "creates_or_binds+current_session_control",
        ),
        ("current_session", "session", "current_session_control"),
        (
            "unbind_current_session",
            "session",
            "current_session_control",
        ),
        (
            "workspace_checkpoint_create",
            "checkpoint",
            "current_session_fallback",
        ),
        (
            "workspace_checkpoint_list",
            "checkpoint",
            "current_session_fallback",
        ),
        (
            "workspace_checkpoint_show",
            "checkpoint",
            "current_session_fallback",
        ),
        (
            "workspace_checkpoint_restore",
            "checkpoint",
            "current_session_fallback",
        ),
        (
            "workspace_checkpoint_delete",
            "checkpoint",
            "current_session_fallback",
        ),
        ("run_process", "job", "current_session_fallback"),
        ("run_script", "job", "current_session_fallback"),
        ("run_shell", "job", "current_session_fallback"),
        ("open_session_shell", "job", "explicit_business_session"),
        ("session_shell_exec", "job", "explicit_business_session"),
        ("session_shell_status", "job", "explicit_business_session"),
        ("close_session_shell", "job", "explicit_business_session"),
        ("apply_patch", "patch", "current_session_fallback"),
        ("apply_patch_checked", "patch", "current_session_fallback"),
        (
            "delete_project_files",
            "cleanup",
            "current_session_fallback",
        ),
        ("git_restore_paths", "cleanup", "current_session_fallback"),
        ("discard_untracked", "cleanup", "current_session_fallback"),
        ("validate_patch", "patch", "current_session_fallback"),
        ("git_status", "git", "current_session_fallback"),
        ("git_diff", "git", "current_session_fallback"),
        ("git_diff_hunks", "git", "current_session_fallback"),
        ("git_log", "git", "current_session_fallback"),
        ("cargo_fmt", "validation", "current_session_fallback"),
        ("cargo_check", "validation", "current_session_fallback"),
        ("cargo_test", "validation", "current_session_fallback"),
        ("read_file", "file", "current_session_fallback"),
        ("read_files", "file", "current_session_fallback"),
        ("lsp_status", "lsp", "current_session_fallback"),
        ("document_symbols", "lsp", "current_session_fallback"),
        ("document_diagnostics", "lsp", "current_session_fallback"),
        ("hover", "lsp", "current_session_fallback"),
        ("workspace_symbols", "lsp", "current_session_fallback"),
        ("goto_definition", "lsp", "current_session_fallback"),
        ("find_references", "lsp", "current_session_fallback"),
        ("run_job", "job", "current_session_fallback"),
        ("stop_job", "job", "current_session_fallback"),
        ("job_status", "job", "none"),
        ("job_log", "job", "none"),
        ("project_overview", "project", "current_session_fallback"),
        ("list_project_files", "file", "current_session_fallback"),
        (
            "list_project_tracked_files",
            "file",
            "current_session_fallback",
        ),
        ("search_project_text", "file", "current_session_fallback"),
        ("search_project_texts", "file", "current_session_fallback"),
        ("git_diff_summary", "git", "current_session_fallback"),
        ("show_changes", "git", "current_session_fallback"),
        (
            "workspace_hygiene_check",
            "cleanup",
            "current_session_fallback",
        ),
        ("list_jobs", "job", "none"),
        ("job_tail", "job", "none"),
        ("write_project_file", "edit", "current_session_fallback"),
        (
            "save_project_artifact",
            "artifact",
            "current_session_fallback",
        ),
        (
            "read_project_artifact_metadata",
            "artifact",
            "current_session_fallback",
        ),
        (
            "read_project_artifact",
            "artifact",
            "current_session_fallback",
        ),
        (
            "artifact_upload_begin",
            "artifact",
            "current_session_fallback",
        ),
        (
            "artifact_upload_chunk",
            "artifact",
            "current_session_fallback",
        ),
        (
            "artifact_upload_finish",
            "artifact",
            "current_session_fallback",
        ),
        (
            "artifact_upload_abort",
            "artifact",
            "current_session_fallback",
        ),
        ("apply_text_edits", "edit", "current_session_fallback"),
        ("list_projects", "project", "none"),
        ("register_project", "project", "none"),
        ("create_project", "project", "none"),
        ("list_agents", "runtime", "none"),
        ("runtime_status", "runtime", "none"),
        ("tool_manifest", "runtime", "none"),
    ];

    let expected_names = expected
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<BTreeSet<_>>();
    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    let definition_name_set = definition_names.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(definition_name_set, expected_names);
    assert_eq!(definition_names, known_tool_names().collect::<Vec<_>>());

    for (name, category, session_policy) in expected {
        let definition =
            lookup_tool_definition(name).unwrap_or_else(|| panic!("{name} missing ToolDefinition"));
        assert_eq!(definition.category, *category, "{name} category");
        assert_eq!(
            session_policy_label(
                runtime_tool_creates_or_binds_session(name),
                runtime_tool_is_current_session_control(name),
                runtime_tool_requires_explicit_business_session(name),
                runtime_tool_allows_current_session_fallback(name),
            ),
            *session_policy,
            "{name} session policy"
        );
    }
}

#[test]
fn tool_definition_explains_all_tool_call_runtime_names() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, tool_definitions,
    };

    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    assert_eq!(
        definition_names, known_names,
        "Every ToolCall-reachable runtime name must be explained by ToolDefinition"
    );

    for name in known_tool_names() {
        // ModelHidden tools have no model-facing ToolSpec, so sample_tool_args
        // (which reads a spec's required fields) cannot build args for them.
        // They still must be parser-known: confirm the name is accepted (the
        // only allowed failure is a missing-field argument error, never
        // "unknown tool"). Visible tools get full arg validation.
        if !is_model_visible_tool_name(name) {
            match ToolCall::from_tool_name(name, Value::Null) {
                Ok(_) => {}
                Err(err) => assert!(
                    !err.contains("unknown tool"),
                    "{name} (hidden) must be parser-known, not unknown: {err}"
                ),
            }
            assert!(
                lookup_tool_definition(name).is_some(),
                "{name} (hidden) must resolve to a ToolDefinition"
            );
            continue;
        }
        let args = if name == "run_codex" {
            json!({"project": SAMPLE_PROJECT, "prompt": "summarize"})
        } else {
            sample_tool_args(name)
        };
        let call = ToolCall::from_tool_name(name, args)
            .unwrap_or_else(|err| panic!("{name} should parse through ToolDefinition: {err}"));
        assert_eq!(call.tool_name(), name);
        assert!(
            lookup_tool_definition(call.tool_name()).is_some(),
            "{} ToolCall::tool_name must resolve to ToolDefinition",
            call.tool_name()
        );
    }

    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        assert!(
            ToolCall::from_tool_name(fallback.name, json!({})).is_err(),
            "{} is a legacy metadata fallback only: {}",
            fallback.name,
            fallback.reason
        );
    }
}

#[test]
fn tool_policy_helpers_match_tool_definitions_for_known_runtime_names() {
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, runtime_tool_agent_capability,
        runtime_tool_allows_current_session_fallback, runtime_tool_category,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_metadata, runtime_tool_permission_risk, runtime_tool_requires_permission,
        runtime_tool_requires_session_project_escape, runtime_tool_session_risk_class,
        tool_definitions,
    };

    for definition in tool_definitions() {
        assert_eq!(
            lookup_tool_definition(definition.name).map(|definition| definition.name),
            Some(definition.name),
            "{} must resolve through ToolDefinition before any policy fallback",
            definition.name
        );
        assert_eq!(
            lookup_tool_metadata(definition.name).copied(),
            Some(definition.metadata()),
            "{} lookup_tool_metadata must return ToolDefinition metadata",
            definition.name
        );
        assert_eq!(
            runtime_tool_metadata(definition.name),
            definition.metadata(),
            "{} metadata policy helper must read the ToolDefinition metadata",
            definition.name
        );
        assert_eq!(
            runtime_tool_session_risk_class(definition.name),
            definition.session_risk_class(),
            "{} session risk helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_read_like(definition.name),
            definition.is_read_like(),
            "{} read-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_write_like(definition.name),
            definition.is_write_like(),
            "{} write-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_shell_like(definition.name),
            definition.is_shell_like(),
            "{} shell-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_category(definition.name),
            definition.category,
            "{} category helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_allows_current_session_fallback(definition.name),
            definition.allows_current_session_fallback(),
            "{} current-session fallback helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_permission(definition.name),
            definition.requires_permission(),
            "{} permission helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_session_project_escape(definition.name),
            definition.requires_session_project_escape(),
            "{} session-project escape helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_permission_risk(definition.name),
            definition.permission_risk(),
            "{} permission risk helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_agent_capability(definition.name),
            definition.agent_capability,
            "{} agent capability helper must match ToolDefinition",
            definition.name
        );
    }
}

#[test]
fn tool_definition_strict_agent_capability_lookup_has_no_metadata_fallback() {
    use crate::tool_runtime::tool_definition::runtime_tool_agent_capability;

    for name in known_tool_names() {
        let _ = runtime_tool_agent_capability(name);
    }
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        let result = std::panic::catch_unwind(|| runtime_tool_agent_capability(fallback.name));
        assert!(
            result.is_err(),
            "{} must not resolve agent capability through legacy metadata fallback: {}",
            fallback.name,
            fallback.reason
        );
    }
    std::panic::set_hook(previous_hook);
}

#[test]
fn tool_definition_metadata_fallback_facade_is_legacy_or_unknown_only() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, tool_metadata};
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition,
        runtime_tool_allows_current_session_fallback, runtime_tool_category,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_metadata, runtime_tool_permission_risk, runtime_tool_requires_permission,
        runtime_tool_requires_session_project_escape, runtime_tool_session_risk_class,
        PERMISSION_RISK_DESTRUCTIVE, PERMISSION_RISK_WRITE,
    };

    let delete_files = lookup_tool_metadata("delete_files")
        .copied()
        .expect("delete_files legacy route metadata");
    assert_eq!(delete_files.name, "delete_files");
    assert_eq!(delete_files.provider_id, TOOL_PROVIDER_AGENT);
    assert_eq!(delete_files.risk, ToolRisk::ProjectWrite);
    assert_eq!(delete_files.oauth_scope, Some(PROJECT_WRITE));
    assert!(delete_files.requires_project);
    assert_eq!(delete_files.path_hint, ToolPathHint::PathList);
    assert!(!delete_files.read_only);
    assert!(delete_files.destructive);
    assert!(!delete_files.shell_like);
    assert!(
        lookup_tool_definition("delete_files").is_none(),
        "delete_files must remain metadata-only legacy route metadata"
    );
    assert!(!is_known_tool_name("delete_files"));
    assert!(
        ToolCall::from_tool_name(
            "delete_files",
            json!({"project": SAMPLE_PROJECT, "paths": ["old.txt"]})
        )
        .is_err(),
        "delete_files metadata fallback must not make it ToolCall-parseable"
    );
    assert_eq!(runtime_tool_metadata("delete_files"), delete_files);
    assert_eq!(runtime_tool_category("delete_files"), "other");
    assert_eq!(
        runtime_tool_session_risk_class("delete_files"),
        ToolRisk::ProjectWrite.session_risk_class()
    );
    assert!(!runtime_tool_is_read_like("delete_files"));
    assert!(runtime_tool_is_write_like("delete_files"));
    assert!(!runtime_tool_is_shell_like("delete_files"));
    assert!(!runtime_tool_allows_current_session_fallback(
        "delete_files"
    ));
    assert!(runtime_tool_requires_permission("delete_files"));
    assert!(runtime_tool_requires_session_project_escape("delete_files"));
    assert_eq!(
        runtime_tool_permission_risk("delete_files"),
        PERMISSION_RISK_DESTRUCTIVE
    );
    assert_model_facing_surfaces_do_not_list_name("delete_files");
    assert_agent_capability_lookup_rejects_non_runtime_name("delete_files");

    for name in [
        "__unknown_non_runtime__",
        "__unknown_tool_for_metadata_test__",
        "not_a_tool",
        "delete_files_v2",
    ] {
        let unknown = tool_metadata(name);
        assert!(lookup_tool_metadata(name).is_none(), "{name}");
        assert!(
            lookup_tool_definition(name).is_none(),
            "{name} must not resolve to ToolDefinition"
        );
        assert!(!is_known_tool_name(name), "{name}");
        assert!(!is_model_visible_tool_name(name), "{name}");
        assert_eq!(unknown.name, "<unknown>", "{name}");
        assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN, "{name}");
        assert_eq!(unknown.risk, ToolRisk::Unknown, "{name}");
        assert_eq!(unknown.oauth_scope, None, "{name}");
        assert!(!unknown.requires_project, "{name}");
        assert_eq!(unknown.path_hint, ToolPathHint::None, "{name}");
        assert!(!unknown.read_only, "{name}");
        assert!(!unknown.destructive, "{name}");
        assert!(!unknown.shell_like, "{name}");
        assert_eq!(runtime_tool_metadata(name), unknown, "{name}");
        assert_eq!(runtime_tool_category(name), "other", "{name}");
        assert_eq!(
            runtime_tool_session_risk_class(name),
            ToolRisk::Unknown.session_risk_class(),
            "{name}"
        );
        assert!(!runtime_tool_is_read_like(name), "{name}");
        assert!(!runtime_tool_is_write_like(name), "{name}");
        assert!(!runtime_tool_is_shell_like(name), "{name}");
        assert!(
            !runtime_tool_allows_current_session_fallback(name),
            "{name}"
        );
        assert!(runtime_tool_requires_permission(name), "{name}");
        assert!(runtime_tool_requires_session_project_escape(name), "{name}");
        assert_eq!(
            runtime_tool_permission_risk(name),
            PERMISSION_RISK_WRITE,
            "{name}"
        );
        assert!(
            ToolCall::from_tool_name(name, json!({})).is_err(),
            "{name} must remain non-callable"
        );
        assert_model_facing_surfaces_do_not_list_name(name);
        assert_agent_capability_lookup_rejects_non_runtime_name(name);
    }
}

#[test]
fn tool_definition_legacy_metadata_fallbacks_are_explicit_and_reasoned() {
    let metadata_only_names = crate::tool_runtime::metadata::iter_tool_metadata()
        .filter(|metadata| !is_known_tool_name(metadata.name))
        .map(|metadata| metadata.name)
        .collect::<Vec<_>>();
    let expected_names = KNOWN_LEGACY_METADATA_FALLBACKS
        .iter()
        .map(|fallback| fallback.name)
        .collect::<Vec<_>>();
    let fallback_reasons = KNOWN_LEGACY_METADATA_FALLBACKS
        .iter()
        .map(|fallback| format!("{}: {}", fallback.name, fallback.reason))
        .collect::<Vec<_>>();

    assert_eq!(
        expected_names, NON_RUNTIME_METADATA_COMPATIBILITY_NAMES,
        "non-runtime metadata compatibility allowlist must stay explicitly named"
    );
    assert_eq!(
        metadata_only_names, expected_names,
        "remaining metadata fallbacks must stay explicit and reasoned: {fallback_reasons:?}"
    );
    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        eprintln!(
            "legacy metadata fallback retained: {} - {}",
            fallback.name, fallback.reason
        );
        assert!(
            !fallback.reason.trim().is_empty(),
            "{} fallback must explain why it remains",
            fallback.name
        );
    }

    let unknown = crate::tool_runtime::tool_definition::runtime_tool_metadata("__unknown__");
    eprintln!(
        "unknown metadata fallback retained: non-runtime unknown names return provider={} risk={:?}",
        unknown.provider_id, unknown.risk
    );
    assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN);
    assert_eq!(unknown.risk, ToolRisk::Unknown);
    assert!(!is_known_tool_name("__unknown__"));
    assert!(ToolCall::from_tool_name("__unknown__", json!({})).is_err());
    assert_model_facing_surfaces_do_not_list_name("__unknown__");
}

#[test]
fn tool_definition_surface_counts_stay_fixed_during_fallback_migration() {
    use crate::tool_runtime::tool_definition::{lookup_tool_definition, model_hidden_tool_names};

    let openapi = crate::openapi::build_openapi_spec();
    let openapi_operation_count: usize = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|methods| methods.as_object().unwrap().len())
        .sum();
    assert_eq!(openapi_operation_count, 25, "OpenAPI operation count");

    let operation_ids = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|methods| methods.as_object().unwrap().values())
        .map(|operation| operation["operationId"].as_str().unwrap())
        .collect::<Vec<_>>();
    for forbidden in [
        "runCodex",
        "RunCodex",
        "sessionHandoffSummary",
        "SessionHandoff",
        "applyTextEdits",
        "ApplyTextEdits",
        "artifactUpload",
        "ArtifactUpload",
    ] {
        assert!(
            !operation_ids
                .iter()
                .any(|operation_id| operation_id.contains(forbidden)),
            "{forbidden} must remain hidden/runtime-only and not become a dedicated GPT Action: {operation_ids:?}"
        );
    }

    let tool_call_properties = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .expect("ToolCallRequest properties");
    for field in [
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
        "summary_only",
        "include_command_preview",
        "detail",
        "compact",
    ] {
        assert!(
            tool_call_properties.contains_key(field),
            "callRuntimeTool must keep flattened GPT Action field {field}"
        );
    }
    assert!(!tool_call_properties.contains_key("test_expect_failure_kind"));
    let tool_description = tool_call_properties["tool"]["description"]
        .as_str()
        .unwrap();
    assert!(
        !tool_description.contains("run_codex"),
        "callRuntimeTool model-facing accepted-name description must not advertise run_codex"
    );

    let model_facing_names = registered_tool_names();
    assert!(
        lookup_tool_definition("run_codex").is_none(),
        "run_codex must not keep an explicit ToolDefinition"
    );
    // `ModelHidden` is a stable, documented back-compat surface: dispatched
    // but withheld from the model. `run_codex` is different — it must be fully
    // gone (no ToolDefinition at all). The hidden set is asserted to a fixed
    // batch elsewhere; here we only confirm run_codex is not hiding in it.
    assert!(
        !model_hidden_tool_names().any(|name| name == "run_codex"),
        "run_codex must remain fully removed, not hidden"
    );
    assert!(
        ToolCall::from_tool_name(
            "run_codex",
            json!({"project": SAMPLE_PROJECT, "prompt": "summarize"})
        )
        .is_err(),
        "run_codex must not remain parser-known"
    );
    assert!(
        !model_facing_names.iter().any(|name| name == "run_codex"),
        "run_codex must remain removed from model-facing tools: {model_facing_names:?}"
    );
    assert_eq!(
        crate::tool_runtime::tool_definition::model_visible_tool_definitions().count(),
        model_facing_names.len(),
        "model-visible ToolDefinition count must match model-facing tool count (ModelHidden tools are dispatched but not listed)"
    );
    assert_model_facing_surfaces_do_not_list_name("run_codex");
}

#[test]
fn tool_definition_dead_code_residue_is_narrow_and_documented() {
    let source = include_str!("../../tool_definition.rs");
    assert!(
        !source.contains("#![allow(dead_code)]"),
        "tool_definition.rs must not use a module-wide dead_code allowance"
    );
}

fn session_policy_label(
    creates_or_binds_session: bool,
    current_session_control: bool,
    requires_explicit_business_session: bool,
    allows_current_session_fallback: bool,
) -> String {
    let mut labels = Vec::new();
    if creates_or_binds_session {
        labels.push("creates_or_binds");
    }
    if current_session_control {
        labels.push("current_session_control");
    }
    if requires_explicit_business_session {
        labels.push("explicit_business_session");
    }
    if allows_current_session_fallback {
        labels.push("current_session_fallback");
    }
    if labels.is_empty() {
        "none".to_string()
    } else {
        labels.join("+")
    }
}

fn assert_model_facing_surfaces_do_not_list_name(name: &str) {
    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        !spec_names.contains(name),
        "{name} must not appear in registered ToolSpecs"
    );
    assert!(
        !registered_tool_names().iter().any(|tool| tool == name),
        "{name} must not appear in model-facing tool names"
    );

    let mcp_payload = json!({ "tools": specs });
    let mcp_names = mcp_payload["tools"]
        .as_array()
        .expect("MCP tools/list payload tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("MCP tool name"))
        .collect::<BTreeSet<_>>();
    assert!(
        !mcp_names.contains(name),
        "{name} must not appear in MCP tools/list names"
    );

    let openapi = crate::openapi::build_openapi_spec();
    let tool_description = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        [TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .expect("ToolCallRequest.tool description");
    assert!(
        !tool_description.contains(name),
        "{name} must not appear in callRuntimeTool accepted-name text"
    );

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    assert!(
        !serde_json::to_string(&manifest).unwrap().contains(name),
        "{name} must not appear in compact tool_manifest"
    );
    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: None,
    });
    assert!(
        !serde_json::to_string(&list_tools).unwrap().contains(name),
        "{name} must not appear in bounded list_tools discovery"
    );
    let full_list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: false,
        limit: None,
    });
    assert!(
        !serde_json::to_string(&full_list_tools)
            .unwrap()
            .contains(name),
        "{name} must not appear in full list_tools discovery"
    );

    // Static discovery surfaces: category groups and recommended flows are
    // compiled straight into the model-facing catalog.
    for group in crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUPS {
        assert!(
            !group.tools.contains(&name),
            "{name} must not appear in discovery group {}",
            group.name
        );
    }
    for flow in crate::tool_runtime::tool_catalog::TOOL_RECOMMENDED_FLOWS {
        assert!(
            !flow.tools.contains(&name),
            "{name} must not appear in recommended flow {}",
            flow.name
        );
        assert!(
            !flow.summary.contains(name) && !flow.manifest_purpose.contains(name),
            "{name} must not appear in recommended flow text for {}",
            flow.name
        );
    }
}

fn assert_agent_capability_lookup_rejects_non_runtime_name(name: &str) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        let _ = crate::tool_runtime::tool_definition::runtime_tool_agent_capability(name);
    });
    std::panic::set_hook(previous_hook);
    assert!(
        result.is_err(),
        "{name} must not resolve agent capability through metadata fallback"
    );
}
