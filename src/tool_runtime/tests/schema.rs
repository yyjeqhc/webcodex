//! Schema tests for tool_runtime.

mod definitions;
mod discovery;
mod flattened_args;

use super::super::*;
use super::support::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[test]
fn tool_definitions_drive_session_and_permission_policy() {
    use crate::tool_runtime::metadata::ToolRisk;
    use crate::tool_runtime::tool_definition::{
        runtime_tool_allows_current_session_fallback, runtime_tool_captures_validation_output,
        runtime_tool_creates_or_binds_session, runtime_tool_disabled_message,
        runtime_tool_extra_accepted_flattened_args, runtime_tool_is_change_summary_like,
        runtime_tool_is_current_session_control, runtime_tool_is_git_like,
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_permission_risk, runtime_tool_requires_explicit_business_session,
        runtime_tool_requires_permission, runtime_tool_requires_session_project_escape,
        runtime_tool_session_risk_class, tool_definitions, PERMISSION_RISK_ARTIFACT_WRITE,
        PERMISSION_RISK_DESTRUCTIVE, PERMISSION_RISK_JOB, PERMISSION_RISK_PATCH,
        PERMISSION_RISK_SHELL, PERMISSION_RISK_VALIDATION, PERMISSION_RISK_WRITE,
        TOOL_DISCOVERY_GROUPS, TOOL_DISCOVERY_GROUP_GIT,
    };

    let git_group = TOOL_DISCOVERY_GROUPS
        .iter()
        .find(|group| group.name == TOOL_DISCOVERY_GROUP_GIT)
        .expect("git discovery group")
        .tools
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for definition in tool_definitions() {
        let metadata = definition.metadata();
        assert_eq!(
            definition.session_risk_class(),
            metadata.risk.session_risk_class(),
            "{} session risk class must derive from metadata risk",
            definition.name
        );
        assert_eq!(
            definition.is_read_like(),
            metadata.read_only,
            "{} read-like policy must derive from metadata",
            definition.name
        );
        assert_eq!(
            definition.is_write_like(),
            metadata.risk == ToolRisk::ProjectWrite,
            "{} write-like policy must derive from metadata",
            definition.name
        );
        assert_eq!(
            definition.is_shell_like(),
            metadata.shell_like || metadata.risk == ToolRisk::JobRun,
            "{} shell-like guard policy must include job-run tools",
            definition.name
        );
        assert_eq!(
            definition.is_git_like(),
            git_group.contains(definition.name),
            "{} git-like ledger policy must mirror the git discovery group",
            definition.name
        );
        assert_eq!(
            definition.requires_session_project_escape(),
            !metadata.read_only || metadata.destructive || metadata.shell_like,
            "{} cross-project session policy must derive from metadata risk flags",
            definition.name
        );
        assert_eq!(
            definition.requires_permission(),
            !metadata.read_only || metadata.destructive || metadata.shell_like,
            "{} permission requirement must derive from metadata risk flags",
            definition.name
        );
        assert_eq!(
            runtime_tool_session_risk_class(definition.name),
            definition.session_risk_class(),
            "{} session risk facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_read_like(definition.name),
            definition.is_read_like(),
            "{} read-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_write_like(definition.name),
            definition.is_write_like(),
            "{} write-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_shell_like(definition.name),
            definition.is_shell_like(),
            "{} shell-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_git_like(definition.name),
            definition.is_git_like(),
            "{} git-like facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_change_summary_like(definition.name),
            definition.is_change_summary_like(),
            "{} change-summary facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_captures_validation_output(definition.name),
            definition.captures_validation_output(),
            "{} validation-output facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_current_session_control(definition.name),
            definition.is_current_session_control(),
            "{} current-session control facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_explicit_business_session(definition.name),
            definition.requires_explicit_business_session(),
            "{} business-session facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_creates_or_binds_session(definition.name),
            definition.creates_or_binds_session(),
            "{} session creation/bind facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_disabled_message(definition.name),
            definition.disabled_message(),
            "{} disabled facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_extra_accepted_flattened_args(definition.name),
            definition.extra_accepted_flattened_args(),
            "{} extra accepted flattened args facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_allows_current_session_fallback(definition.name),
            definition.allows_current_session_fallback(),
            "{} current-session fallback facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_session_project_escape(definition.name),
            definition.requires_session_project_escape(),
            "{} session-project escape facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_permission(definition.name),
            definition.requires_permission(),
            "{} permission facade must use ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_permission_risk(definition.name),
            definition.permission_risk(),
            "{} permission risk facade must use ToolDefinition",
            definition.name
        );
    }

    let change_summary_tools = tool_definitions()
        .filter(|definition| definition.is_change_summary_like())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        change_summary_tools,
        vec!["git_diff_summary", "show_changes", "git_diff_hunks"]
    );

    let validation_output_tools = tool_definitions()
        .filter(|definition| definition.captures_validation_output())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        validation_output_tools,
        vec!["cargo_fmt", "cargo_check", "cargo_test"]
    );

    let current_session_control_tools = tool_definitions()
        .filter(|definition| definition.is_current_session_control())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        current_session_control_tools,
        vec![
            "bind_current_session",
            "current_session",
            "unbind_current_session"
        ]
    );

    let explicit_business_session_tools = tool_definitions()
        .filter(|definition| definition.requires_explicit_business_session())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        explicit_business_session_tools,
        vec![
            "finish_coding_task",
            "session_summary",
            "post_session_message",
            "list_session_messages",
            "resolve_session_message",
            "session_discussion_summary",
            "session_handoff_summary"
        ]
    );

    let creates_or_binds_session_tools = tool_definitions()
        .filter(|definition| definition.creates_or_binds_session())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        creates_or_binds_session_tools,
        vec!["start_session", "start_coding_task", "bind_current_session"]
    );

    let disabled_tools = tool_definitions()
        .filter(|definition| definition.disabled_message().is_some())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(disabled_tools, vec!["run_codex"]);

    let extra_accepted_flattened_arg_tools = tool_definitions()
        .filter(|definition| !definition.extra_accepted_flattened_args().is_empty())
        .map(|definition| {
            (
                definition.name,
                definition.extra_accepted_flattened_args().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        extra_accepted_flattened_arg_tools,
        vec![("start_coding_task", vec!["session_id"])]
    );

    let unit_argument_tools = tool_definitions()
        .filter(|definition| definition.uses_unit_arguments())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        unit_argument_tools,
        vec!["list_projects", "list_agents", "runtime_status"]
    );

    let artifact_upload_path_binding_tools = tool_definitions()
        .filter(|definition| definition.requires_artifact_upload_path_binding())
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        artifact_upload_path_binding_tools,
        vec![
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort"
        ]
    );

    let current_session_fallback_tools = tool_definitions()
        .filter(|definition| definition.allows_current_session_fallback())
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert!(current_session_fallback_tools.contains("read_file"));
    assert!(current_session_fallback_tools.contains("run_shell"));
    assert!(current_session_fallback_tools.contains("workspace_hygiene_check"));
    for name in [
        "start_session",
        "start_coding_task",
        "finish_coding_task",
        "session_summary",
        "session_handoff_summary",
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            !current_session_fallback_tools.contains(name),
            "{name} must not implicitly use the current-session binding"
        );
    }

    for (tool, risk) in [
        ("cargo_check", PERMISSION_RISK_VALIDATION),
        ("run_shell", PERMISSION_RISK_SHELL),
        ("run_job", PERMISSION_RISK_JOB),
        ("stop_job", PERMISSION_RISK_JOB),
        ("run_codex", PERMISSION_RISK_JOB),
        ("delete_project_files", PERMISSION_RISK_DESTRUCTIVE),
        ("save_project_artifact", PERMISSION_RISK_ARTIFACT_WRITE),
        ("apply_patch", PERMISSION_RISK_PATCH),
        ("write_project_file", PERMISSION_RISK_WRITE),
    ] {
        assert_eq!(runtime_tool_permission_risk(tool), risk, "{tool}");
    }

    assert_eq!(
        runtime_tool_session_risk_class("__unknown__"),
        ToolRisk::Unknown.session_risk_class()
    );
    assert!(!runtime_tool_is_write_like("__unknown__"));
    assert!(!runtime_tool_is_shell_like("__unknown__"));
    assert!(!runtime_tool_allows_current_session_fallback("__unknown__"));
    assert!(runtime_tool_requires_permission("__unknown__"));
    assert!(runtime_tool_requires_session_project_escape("__unknown__"));
    assert_eq!(
        runtime_tool_permission_risk("__unknown__"),
        PERMISSION_RISK_WRITE
    );
    assert_eq!(
        runtime_tool_permission_risk("compat_patch_like"),
        PERMISSION_RISK_PATCH,
        "unknown compatibility names keep the legacy path/name fallback"
    );
    assert_ne!(
        runtime_tool_permission_risk("compat_patch_like"),
        runtime_tool_permission_risk("unknown_artifact"),
        "name-based patch fallback must not classify unrelated unknown names"
    );
}

#[test]
fn tool_specs_and_metadata_are_synchronized() {
    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();

    for spec in &specs {
        assert!(
            crate::tool_runtime::metadata::lookup_tool_metadata(&spec.name).is_some(),
            "{} missing metadata",
            spec.name
        );
    }

    for metadata in crate::tool_runtime::metadata::iter_tool_metadata() {
        if metadata.name == "delete_files" {
            // Legacy dedicated HTTP route metadata; not a public runtime
            // ToolSpec name and intentionally not accepted by ToolCall.
            continue;
        }
        if is_model_hidden_tool_name(metadata.name) {
            // Hidden implemented tools keep parser/metadata coverage without
            // being advertised through model-facing specs.
            continue;
        }
        assert!(
            spec_names.contains(metadata.name),
            "{} metadata is not exposed by registry specs",
            metadata.name
        );
    }
}

#[test]
fn required_agent_capability_matches_metadata_risk_table() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk, TOOL_PROVIDER_AGENT};

    let cases = [
        ("run_shell", ToolRisk::JobRun, AgentCapability::Shell),
        (
            "apply_patch",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "apply_patch_checked",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "delete_project_files",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "git_restore_paths",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        (
            "discard_untracked",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
        ),
        // Read-only dry run, but implemented through the agent shell path.
        ("validate_patch", ToolRisk::ReadOnly, AgentCapability::Shell),
        (
            "replace_in_file",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "replace_exact_block",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_before_pattern",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_after_pattern",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "write_project_file",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "save_project_artifact",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "read_project_artifact_metadata",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "read_project_artifact",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "artifact_upload_begin",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_chunk",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_finish",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "artifact_upload_abort",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "replace_line_range",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "insert_at_line",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "delete_line_range",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "apply_text_edits",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "git_status",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        ("git_diff", ToolRisk::ReadOnly, AgentCapability::GitOrShell),
        (
            "git_diff_hunks",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        ("git_log", ToolRisk::ReadOnly, AgentCapability::GitOrShell),
        ("cargo_fmt", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_check", ToolRisk::JobRun, AgentCapability::Shell),
        ("cargo_test", ToolRisk::JobRun, AgentCapability::Shell),
        ("read_file", ToolRisk::ReadOnly, AgentCapability::FileRead),
        ("run_job", ToolRisk::JobRun, AgentCapability::AsyncJobs),
        (
            "list_project_files",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "search_project_text",
            ToolRisk::ReadOnly,
            AgentCapability::Shell,
        ),
        (
            "git_diff_summary",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "show_changes",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "workspace_hygiene_check",
            ToolRisk::ReadOnly,
            AgentCapability::GitOrShell,
        ),
        (
            "workspace_checkpoint_create",
            ToolRisk::ReadOnly,
            AgentCapability::FileRead,
        ),
        (
            "workspace_checkpoint_restore",
            ToolRisk::ProjectWrite,
            AgentCapability::FileWrite,
        ),
        (
            "workspace_checkpoint_list",
            ToolRisk::ReadOnly,
            AgentCapability::OwnerOnly,
        ),
        (
            "workspace_checkpoint_show",
            ToolRisk::ReadOnly,
            AgentCapability::OwnerOnly,
        ),
        (
            "workspace_checkpoint_delete",
            ToolRisk::ProjectWrite,
            AgentCapability::OwnerOnly,
        ),
    ];

    let specs = registered_tool_specs();
    let expected_project_tools = specs
        .iter()
        .filter_map(|spec| {
            let metadata = lookup_tool_metadata(&spec.name).unwrap();
            ((metadata.provider_id == TOOL_PROVIDER_AGENT
                || spec.name.starts_with("workspace_checkpoint_"))
                && metadata.requires_project)
                .then_some(spec.name.as_str())
        })
        .collect::<BTreeSet<_>>();
    let table_project_tools = cases
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<BTreeSet<_>>();
    assert_eq!(table_project_tools, expected_project_tools);

    for (name, risk, capability) in cases {
        let metadata = lookup_tool_metadata(name).unwrap();
        assert_eq!(metadata.risk, risk, "{name} metadata risk");
        let call = ToolCall::from_tool_name(name, sample_tool_args(name))
            .unwrap_or_else(|e| panic!("{name} should deserialize: {e}"));
        assert_eq!(
            required_agent_capability(&call),
            Some(capability),
            "{name} capability"
        );
    }
}

#[test]
fn tool_specs_names_are_unique() {
    let specs = registered_tool_specs();
    let mut names = specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    names.sort();
    let mut deduped = names.clone();
    deduped.dedup();
    assert_eq!(names, deduped, "tool names must be unique");
}

#[test]
fn tool_specs_names_are_snake_case() {
    for spec in registered_tool_specs() {
        assert!(
            !spec.name.contains('-'),
            "tool name '{}' should use snake_case (no hyphens)",
            spec.name
        );
        assert!(
            spec.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "tool name '{}' should be snake_case",
            spec.name
        );
    }
}

#[test]
fn tool_specs_derive_contract_fields_from_name() {
    for spec in registered_tool_specs() {
        assert_eq!(
            spec.output_schema,
            crate::tool_runtime::registry::output_schema_for_tool(&spec.name),
            "{} output schema must derive from ToolSpec.name",
            spec.name
        );
        assert_eq!(
            spec.annotations,
            crate::tool_runtime::registry::tool_annotations(&spec.name),
            "{} MCP annotations must derive from ToolSpec.name",
            spec.name
        );
    }
}

#[test]
fn tool_specs_input_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.input_schema;
        assert_eq!(
            schema["type"].as_str(),
            Some("object"),
            "tool '{}' input schema must be type object",
            spec.name
        );
        assert!(
            schema["properties"].is_object(),
            "tool '{}' input schema must have properties object",
            spec.name
        );
        assert!(
            schema["required"].is_array(),
            "tool '{}' input schema must have required array",
            spec.name
        );
    }
}

#[test]
fn tool_specs_expose_common_testing_metadata() {
    use crate::tool_runtime::sessions::TOOL_CALL_EXPECTATION_METADATA_FIELDS;

    for spec in registered_tool_specs() {
        let props = spec.input_schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} input schema properties", spec.name));
        for &field in TOOL_CALL_EXPECTATION_METADATA_FIELDS {
            assert!(
                props.contains_key(field),
                "{} input schema should expose common testing metadata field {field}",
                spec.name
            );
            let desc = props[field]["description"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            assert!(
                desc.contains("testing") || desc.contains("smoke"),
                "{}.{field} should be documented as testing/smoke metadata: {desc}",
                spec.name
            );
            assert!(
                desc.contains("does not change"),
                "{}.{field} should document that behavior is unchanged: {desc}",
                spec.name
            );
        }
    }
}

#[test]
fn list_tools_schema_exposes_bounded_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "list_tools");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in ["category", "features", "summary_only", "limit"] {
        assert!(
            props.contains_key(field),
            "list_tools input schema must expose {field}"
        );
    }
    assert!(spec.input_schema["required"].as_array().unwrap().is_empty());
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "category",
        "features",
        "limit",
        "categories",
        "recommended_flows",
    ] {
        assert!(
            output.contains_key(field),
            "list_tools output schema must expose {field}"
        );
    }
}

#[test]
fn tool_manifest_schema_exposes_compact_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "tool_manifest");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "category",
        "include_recommended_flows",
        "include_risk_summary",
    ] {
        assert!(
            props.contains_key(field),
            "tool_manifest input schema must expose {field}"
        );
    }
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "schema_version",
        "count",
        "tool_count",
        "filtered_count",
        "category",
        "filtered",
        "categories_requested",
        "limit",
        "truncated",
        "categories",
        "tools",
        "risk_summary",
        "recommended_flows",
    ] {
        assert!(
            output.contains_key(field),
            "tool_manifest output schema must expose {field}"
        );
    }
}

#[test]
fn discovery_output_schemas_cover_runtime_payload_keys() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_GIT;

    let runtime = test_runtime();
    let specs = registered_tool_specs();

    let list_tools_spec = spec_named(&specs, "list_tools");
    let list_tools_payload = runtime.list_tools_payload(ListToolsOptions {
        category: Some(TOOL_CATEGORY_GIT.to_string()),
        features: Some("read".to_string()),
        summary_only: true,
        limit: Some(3),
    });
    assert_payload_keys_declared(
        "list_tools",
        &list_tools_payload,
        output_schema_properties(list_tools_spec),
    );

    let tool_manifest_spec = spec_named(&specs, "tool_manifest");
    let tool_manifest_payload = runtime
        .compact_tool_manifest_payload_bounded(Some(vec![TOOL_CATEGORY_GIT.to_string()]), Some(2));
    assert_payload_keys_declared(
        "tool_manifest",
        &tool_manifest_payload,
        output_schema_properties(tool_manifest_spec),
    );
}

fn output_schema_properties(spec: &ToolSpec) -> &serde_json::Map<String, Value> {
    spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} output schema properties", spec.name))
}

fn assert_payload_keys_declared(
    tool_name: &str,
    payload: &Value,
    output_schema_properties: &serde_json::Map<String, Value>,
) {
    let payload = payload
        .as_object()
        .unwrap_or_else(|| panic!("{tool_name} payload object"));
    for key in payload.keys() {
        assert!(
            output_schema_properties.contains_key(key),
            "{tool_name} runtime output key {key} is missing from output_schema properties"
        );
    }
}

#[test]
fn read_project_artifact_metadata_schema_exposes_allow_missing() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_project_artifact_metadata");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(
        props.contains_key("allow_missing"),
        "read_project_artifact_metadata input schema must expose allow_missing"
    );
    assert!(
        spec.description.contains("allow_missing=true")
            && spec.description.contains("exists=false"),
        "description should explain successful missing assertions: {}",
        spec.description
    );
}

#[test]
fn artifact_upload_followup_descriptions_explain_required_path_binding() {
    let specs = registered_tool_specs();
    for name in [
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.description.contains("path is required")
                && spec.description.contains("artifact_upload_begin")
                && spec.description.contains("binds upload_id"),
            "{name}: {}",
            spec.description
        );
        let path_desc = spec.input_schema["properties"]["path"]["description"]
            .as_str()
            .unwrap();
        assert!(
            path_desc.contains("Required")
                && path_desc.contains("must exactly match the path used in artifact_upload_begin")
                && path_desc.contains("bind upload_id"),
            "{name}: {path_desc}"
        );
    }
}

#[test]
fn tool_specs_output_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.output_schema;
        assert_eq!(
            schema["type"].as_str(),
            Some("object"),
            "tool '{}' output schema must be type object",
            spec.name
        );
        assert!(
            schema["properties"].is_object(),
            "tool '{}' output schema must have properties object",
            spec.name
        );
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|v| v == "success")),
            "tool '{}' output schema must require success",
            spec.name
        );
    }
}

#[test]
fn key_tool_output_schemas_include_expected_fields() {
    let specs = registered_tool_specs();
    let has_output_field = |name: &str, field: &str| {
        let spec = spec_named(&specs, name);
        spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .is_some_and(|props| props.contains_key(field))
    };

    for field in [
        "duration_ms",
        "exit_code",
        "stdout",
        "stderr",
        "command_started",
        "command_completed",
        "command_ok",
        "failure_kind",
        "tool_failure",
    ] {
        assert!(
            has_output_field("run_shell", field),
            "run_shell missing {field}"
        );
    }
    for field in [
        "content",
        "start_line",
        "limit",
        "total_lines",
        "numbered_text",
        "lines",
    ] {
        assert!(
            has_output_field("read_file", field),
            "read_file missing {field}"
        );
    }
    for field in [
        "backend",
        "matches",
        "count",
        "truncated",
        "context_before",
        "context_after",
    ] {
        assert!(
            has_output_field("search_project_text", field),
            "search_project_text missing {field}"
        );
    }
    for field in ["job_id", "kind", "status", "project"] {
        assert!(
            has_output_field("run_job", field),
            "run_job missing {field}"
        );
    }
    for field in [
        "stopped",
        "already_finished",
        "already_stop_requested",
        "stop_request_accepted",
        "target_was_active_at_request",
        "terminal",
        "terminal_pending",
        "final_status",
        "stop_effect",
        "job_id",
        "project",
        "status_before",
        "status_after",
        "command_started",
        "ownership_basis",
    ] {
        assert!(
            has_output_field("stop_job", field),
            "stop_job missing {field}"
        );
    }
    for field in [
        "job_id",
        "project",
        "status",
        "exit_code",
        "started_at",
        "ended_at",
        "error",
        "command_preview_included",
        "active",
        "blocking_active",
        "terminal",
        "terminal_pending",
        "command_preview",
        "command_preview_truncated",
        "command_preview_max_chars",
        "command_preview_bounded",
    ] {
        assert!(
            has_output_field("job_status", field),
            "job_status missing {field}"
        );
    }
    for field in [
        "job_id",
        "stdout",
        "stderr",
        "next_stdout_line",
        "next_stderr_line",
        "status",
    ] {
        assert!(
            has_output_field("job_log", field),
            "job_log missing {field}"
        );
    }
    for field in [
        "path",
        "exists",
        "missing",
        "bytes",
        "sha256",
        "mime_type",
        "modified_at",
    ] {
        assert!(
            has_output_field("read_project_artifact_metadata", field),
            "read_project_artifact_metadata missing {field}"
        );
    }
    for field in [
        "path",
        "file_bytes",
        "offset",
        "bytes_returned",
        "content_base64",
        "next_offset",
        "truncated",
        "eof",
    ] {
        assert!(
            has_output_field("read_project_artifact", field),
            "read_project_artifact missing {field}"
        );
    }
    let upload_progress_fields = [
        "path",
        "upload_id",
        "received_bytes",
        "next_offset",
        "expected_bytes",
        "expected_sha256",
        "committed",
    ];
    for field in upload_progress_fields {
        assert!(
            has_output_field("artifact_upload_begin", field),
            "artifact_upload_begin missing {field}"
        );
        assert!(
            has_output_field("artifact_upload_chunk", field),
            "artifact_upload_chunk missing {field}"
        );
    }
    for field in [
        "path",
        "upload_id",
        "bytes",
        "received_bytes",
        "expected_bytes",
        "expected_sha256",
        "sha256",
        "committed",
    ] {
        assert!(
            has_output_field("artifact_upload_finish", field),
            "artifact_upload_finish missing {field}"
        );
    }
    for field in [
        "path",
        "upload_id",
        "received_bytes",
        "aborted",
        "temp_file_removed",
        "sidecar_removed",
        "final_file_touched",
        "final_file_exists",
        "changed_path_details",
    ] {
        assert!(
            has_output_field("artifact_upload_abort", field),
            "artifact_upload_abort missing {field}"
        );
    }
    for field in [
        "service",
        "version",
        "build",
        "auth_enabled",
        "configured_public_url",
        "agents",
        "projects",
        "jobs",
        "tools",
        "permissions",
        "quic",
    ] {
        assert!(
            has_output_field("runtime_status", field),
            "runtime_status missing {field}"
        );
    }
    for field in ["projects", "count", "recommended_for_smoke"] {
        assert!(
            has_output_field("list_projects", field),
            "list_projects missing {field}"
        );
    }
}

#[test]
fn tool_specs_required_fields_match_deserialization() {
    // For every tool spec, building arguments with only the required
    // fields must deserialize successfully, and omitting any required
    // field must fail.
    for spec in registered_tool_specs() {
        let required: Vec<String> = spec.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        // Build a minimal valid args object using a placeholder for each
        // required field based on its declared type.
        let mut minimal = serde_json::Map::new();
        let properties = spec.input_schema["properties"].as_object().unwrap();
        for field in &required {
            let prop = &properties[field.as_str()];
            let placeholder = placeholder_from_prop(prop);
            minimal.insert(field.clone(), placeholder);
        }
        let args = Value::Object(minimal);
        ToolCall::from_tool_name(&spec.name, args)
            .unwrap_or_else(|e| panic!("tool '{}' minimal args failed: {}", spec.name, e));

        // Omitting each required field should fail.
        for field in &required {
            let mut partial = serde_json::Map::new();
            for f in &required {
                if f != field {
                    let prop = &properties[f.as_str()];
                    let placeholder = placeholder_from_prop(prop);
                    partial.insert(f.clone(), placeholder);
                }
            }
            let err = ToolCall::from_tool_name(&spec.name, Value::Object(partial))
                .err()
                .unwrap_or_else(|| {
                    panic!(
                        "tool '{}' should reject missing required field '{}'",
                        spec.name, field
                    )
                });
            assert!(
                err.contains(field),
                "tool '{}' error for missing '{}' should mention field: {}",
                spec.name,
                field,
                err
            );
        }
    }
}

#[test]
fn apply_text_edits_input_schema_matches_runtime_edit_objects() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    let edits = &spec.input_schema["properties"]["edits"];

    assert_eq!(edits["type"], "array");
    assert_eq!(edits["items"]["type"], "object");
    assert_eq!(edits["items"]["required"], json!(["kind"]));

    let kind_enum = edits["items"]["properties"]["kind"]["enum"]
        .as_array()
        .expect("apply_text_edits kind enum should be listed")
        .iter()
        .map(|value| value.as_str().expect("kind enum value should be a string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        kind_enum,
        BTreeSet::from([
            "replace_exact",
            "insert_after",
            "insert_before",
            "delete_exact"
        ])
    );

    let object_args = json!({
        "project": "agent:oe:private-drop",
        "path": "src/lib.rs",
        "edits": [
            {
                "kind": "insert_after",
                "anchor_text": "fn main() {}",
                "new_text": "\n"
            }
        ]
    });
    ToolCall::from_tool_name("apply_text_edits", object_args)
        .expect("apply_text_edits should deserialize object edit inputs");

    let string_args = json!({
        "project": "agent:oe:private-drop",
        "path": "src/lib.rs",
        "edits": [
            "{\"kind\":\"insert_after\",\"anchor_text\":\"fn main() {}\",\"new_text\":\"\\n\"}"
        ]
    });
    assert!(
        ToolCall::from_tool_name("apply_text_edits", string_args).is_err(),
        "apply_text_edits should reject stringified edit objects"
    );
}

#[test]
fn tool_specs_optional_fields_are_not_required() {
    // Optional fields (e.g. timeout_secs, cwd) must not appear in required.
    let specs = registered_tool_specs();
    let run_shell = specs.iter().find(|s| s.name == "run_shell").unwrap();
    let required: Vec<String> = run_shell.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"command".to_string()));
    assert!(!required.contains(&"timeout_secs".to_string()));
    assert!(!required.contains(&"cwd".to_string()));

    let read_file = specs.iter().find(|s| s.name == "read_file").unwrap();
    let required = required_fields(read_file);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"path".to_string()));
    assert!(!required.contains(&"with_line_numbers".to_string()));

    let search_project_text = specs
        .iter()
        .find(|s| s.name == "search_project_text")
        .unwrap();
    let required = required_fields(search_project_text);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"pattern".to_string()));
    assert!(!required.contains(&"context_before".to_string()));
    assert!(!required.contains(&"context_after".to_string()));
}

#[test]
fn tool_specs_covers_expected_tool_set() {
    let names = registered_tool_names();
    assert_eq!(
        names.len(),
        66,
        "model-facing runtime/MCP tool count should be 66 after exposing stop_job"
    );
    for expected in [
        "list_tools",
        "list_projects",
        "list_agents",
        "runtime_status",
        "run_shell",
        "run_job",
        "stop_job",
        "job_status",
        "job_log",
        "read_file",
        "git_status",
        "git_diff",
        "git_diff_summary",
        "git_diff_hunks",
        "git_log",
        "show_changes",
        "workspace_hygiene_check",
        "workspace_checkpoint_create",
        "workspace_checkpoint_list",
        "workspace_checkpoint_show",
        "workspace_checkpoint_restore",
        "workspace_checkpoint_delete",
        "apply_patch",
        "apply_patch_checked",
        "validate_patch",
        "delete_project_files",
        "git_restore_paths",
        "discard_untracked",
        // Phase A: read-only console tools
        "list_project_files",
        "search_project_text",
        "list_jobs",
        "job_tail",
        // Phase 4: file edit tools
        "replace_in_file",
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "write_project_file",
        "save_project_artifact",
        "read_project_artifact_metadata",
        "read_project_artifact",
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        // Project management
        "register_project",
        "create_project",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "expected tool '{}' in specs: {:?}",
            expected,
            names
        );
    }
    assert!(
        !names.iter().any(|n| n == "run_codex"),
        "run_codex must stay hidden from model-facing tool_specs: {:?}",
        names
    );
}

#[test]
fn tool_specs_descriptions_fit_gpt_action_limit() {
    for spec in registered_tool_specs() {
        assert!(
            spec.description.chars().count() <= 300,
            "{} description is too long: {} chars",
            spec.name,
            spec.description.chars().count()
        );
    }
}

#[test]
fn tool_specs_schema_spot_checks() {
    // Table-driven: each entry is (tool_name, required_fields, forbidden_fields).
    // Required fields are checked via exact equality to catch unexpected additions.
    let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
        (
            "apply_patch_checked",
            vec!["project", "patch"],
            vec!["deny_sensitive_paths"],
        ),
        (
            "validate_patch",
            vec!["project", "patch"],
            vec!["deny_sensitive_paths"],
        ),
        ("git_diff_summary", vec!["project"], vec![]),
    ];
    let specs = registered_tool_specs();
    for (name, expected_required, expected_forbidden) in &cases {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        let mut expected_sorted: Vec<String> =
            expected_required.iter().map(|s| s.to_string()).collect();
        expected_sorted.sort();
        let mut actual_sorted = required.clone();
        actual_sorted.sort();
        assert_eq!(
                actual_sorted, expected_sorted,
                "{name}: required fields mismatch (expected exactly {expected_sorted:?}, got {required:?})"
            );
        for field in expected_forbidden {
            assert!(
                !required.contains(&field.to_string()),
                "{name}: field '{field}' should not be required"
            );
        }
        assert!(
            spec.description.chars().count() <= 300,
            "{name}: description too long"
        );
    }
}

#[test]
fn tool_specs_git_log_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "git_log");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in ["project", "limit", "skip", "session_id"] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in ["project", "limit", "skip", "count", "truncated", "commits"] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_show_changes_schema() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "show_changes");
    let required = required_fields(spec);
    assert_eq!(required, vec!["project".to_string()]);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "project",
        "session_id",
        "include_diff",
        "max_hunks",
        "max_hunk_lines",
        "session_event_limit",
    ] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "project",
        "branch",
        "head",
        "clean",
        "counts",
        "files",
        "diff_stat",
        "untracked_previews",
        "untracked_previews_truncated",
        "warnings",
        "suggested_next_actions",
        "session",
    ] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
    assert!(spec.description.chars().count() <= 300);
}

#[test]
fn tool_specs_cargo_tools_schema_and_output() {
    let specs = registered_tool_specs();
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        assert!(spec.input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("cwd"));
        for field in [
            "exit_code",
            "duration_ms",
            "stdout_tail",
            "stderr_tail",
            "passed",
        ] {
            assert!(
                spec.output_schema["properties"]["output"]["properties"]
                    .as_object()
                    .unwrap()
                    .contains_key(field),
                "{} missing output field {}",
                name,
                field
            );
        }
    }
}

#[test]
fn tool_specs_schema_spot_checks_extended() {
    // Table-driven: (tool_name, required_fields, forbidden_fields).
    // Required fields are checked via exact equality to catch unexpected additions.
    let cases: Vec<(&str, Vec<&str>, Vec<&str>)> = vec![
        ("delete_project_files", vec!["project", "paths"], vec![]),
        ("git_restore_paths", vec!["project", "paths"], vec![]),
        ("discard_untracked", vec!["project", "paths"], vec![]),
        ("list_project_files", vec!["project"], vec!["path", "limit"]),
        (
            "search_project_text",
            vec!["project", "pattern"],
            vec!["path", "limit", "context_before", "context_after"],
        ),
        (
            "read_file",
            vec!["project", "path"],
            vec!["with_line_numbers"],
        ),
        ("list_jobs", vec![], vec![]),
        (
            "stop_job",
            vec!["project", "job_id"],
            vec!["confirm", "session_id"],
        ),
        (
            "job_status",
            vec!["job_id"],
            vec!["include_command_preview"],
        ),
        ("job_tail", vec!["job_id"], vec!["tail_lines"]),
    ];
    let specs = registered_tool_specs();
    for (name, expected_required, expected_forbidden) in &cases {
        let spec = spec_named(&specs, name);
        let required = required_fields(spec);
        let mut expected_sorted: Vec<String> =
            expected_required.iter().map(|s| s.to_string()).collect();
        expected_sorted.sort();
        let mut actual_sorted = required.clone();
        actual_sorted.sort();
        assert_eq!(
                actual_sorted, expected_sorted,
                "{name}: required fields mismatch (expected exactly {expected_sorted:?}, got {required:?})"
            );
        for field in expected_forbidden {
            assert!(
                !required.contains(&field.to_string()),
                "{name}: field '{field}' should not be required"
            );
        }
        assert!(
            spec.description.chars().count() <= 300,
            "{name}: description too long"
        );
    }

    // Extra property checks for tools with richer schemas.
    let spec = spec_named(&specs, "search_project_text");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("context_before"));
    assert!(props.contains_key("context_after"));

    let spec = spec_named(&specs, "job_status");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("include_command_preview"));

    let spec = spec_named(&specs, "read_file");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("with_line_numbers"));
}

#[test]
fn tool_categories_and_recommended_flows_are_well_formed() {
    use crate::tool_runtime::tool_definition::{
        TOOL_DISCOVERY_GROUP_CHECKPOINT, TOOL_DISCOVERY_GROUP_CLEANUP, TOOL_DISCOVERY_GROUP_EDIT,
        TOOL_DISCOVERY_GROUP_GIT, TOOL_DISCOVERY_GROUP_INSPECT, TOOL_DISCOVERY_GROUP_JOBS,
        TOOL_DISCOVERY_GROUP_PATCH, TOOL_DISCOVERY_GROUP_REVIEW, TOOL_DISCOVERY_GROUP_RUNTIME,
        TOOL_DISCOVERY_GROUP_SHELL, TOOL_DISCOVERY_GROUP_VALIDATION,
    };

    let categories = registered_tool_categories();
    // Every declared category is a non-empty array of known tool names.
    let names = registered_tool_names();
    for (cat, members) in categories.as_object().unwrap() {
        let arr = members.as_array().unwrap();
        assert!(!arr.is_empty(), "category '{}' must not be empty", cat);
        for m in arr {
            let name = m.as_str().unwrap();
            assert!(
                names.iter().any(|n| n == name),
                "category '{}' lists unknown tool '{}'",
                cat,
                name
            );
        }
    }
    // Each expected category is present.
    for cat in [
        TOOL_DISCOVERY_GROUP_INSPECT,
        TOOL_DISCOVERY_GROUP_GIT,
        TOOL_DISCOVERY_GROUP_REVIEW,
        TOOL_DISCOVERY_GROUP_VALIDATION,
        TOOL_DISCOVERY_GROUP_PATCH,
        TOOL_DISCOVERY_GROUP_SHELL,
        TOOL_DISCOVERY_GROUP_JOBS,
        TOOL_DISCOVERY_GROUP_RUNTIME,
        TOOL_DISCOVERY_GROUP_CLEANUP,
        TOOL_DISCOVERY_GROUP_CHECKPOINT,
    ] {
        assert!(
            categories.as_object().unwrap().contains_key(cat),
            "missing category {}",
            cat
        );
    }
    let validation = categories[TOOL_DISCOVERY_GROUP_VALIDATION]
        .as_array()
        .unwrap();
    for name in [
        "cargo_fmt",
        "cargo_check",
        "cargo_test",
        "validate_patch",
        "apply_patch_checked",
    ] {
        assert!(validation.iter().any(|v| v == name));
    }
    let review = categories[TOOL_DISCOVERY_GROUP_REVIEW].as_array().unwrap();
    assert!(review.iter().any(|v| v == "git_diff_hunks"));
    assert!(review.iter().any(|v| v == "workspace_hygiene_check"));
    assert!(review.iter().any(|v| v == "git_log"));
    let inspect = categories[TOOL_DISCOVERY_GROUP_INSPECT].as_array().unwrap();
    for name in ["read_file", "search_project_text", "show_changes"] {
        assert!(
            inspect.iter().any(|v| v == name),
            "inspect category should include default inspect tool {name}"
        );
    }
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT].as_array().unwrap();
    let edit_prefix: Vec<&str> = edit
        .iter()
        .take(5)
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        edit_prefix,
        vec![
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
        ],
        "preferred edit tools should lead the edit category"
    );
    // recommended_flows are short and non-empty.
    let flows = recommended_flows();
    assert!(!flows.is_empty());
    for flow in &flows {
        assert!(flow.chars().count() <= 300, "flow too long: {}", flow);
    }
    let joined_flows = flows.join("\n").to_lowercase();
    for phrase in [
        "inspect: use read_file, search_project_text, and show_changes before editing",
        "edit: prefer replace_line_range / insert_at_line / delete_line_range",
        "apply_text_edits for batches",
        "apply_patch_checked for broad diffs",
        "validate: use cargo_check / cargo_test / validate_patch",
        "raw run_shell is a bounded escape hatch",
        "not the primary editing or validation path",
        "review: use show_changes / git_diff_hunks / workspace_hygiene_check",
        "handoff: use session_summary / session_handoff_summary",
    ] {
        assert!(
            joined_flows.contains(phrase),
            "recommended flows should mention {phrase}: {joined_flows}"
        );
    }
}

#[test]
fn finish_coding_task_output_schema_describes_ledger_validation_summary() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("finish_coding_task");
    let output_props = schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("permissions"),
        "finish_coding_task output schema should include permissions"
    );
    assert!(
        output_props.contains_key("tool_failures"),
        "finish_coding_task output schema should include classified tool failures"
    );
    assert!(
        output_props.contains_key("summary_only"),
        "finish_coding_task output schema should include summary_only for compact output"
    );
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    let description = schema["properties"]["output"]["properties"]["validation"]["description"]
        .as_str()
        .unwrap();
    let description = description.to_lowercase();
    for phrase in [
        "ledger-based",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "minimal diagnostics",
        "bounded tails",
        "safe result metadata",
        "never infer root cause",
    ] {
        assert!(
            description.contains(phrase),
            "validation output schema should mention {phrase}: {description}"
        );
    }
}

#[test]
fn session_handoff_summary_schema_exposes_ledger_validation_summary() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "session_handoff_summary");
    let input_props = spec.input_schema["properties"].as_object().unwrap();
    assert!(
        input_props.contains_key("include_validation"),
        "session_handoff_summary input schema should include include_validation"
    );
    assert!(
        input_props.contains_key("summary_only"),
        "session_handoff_summary input schema should include summary_only"
    );

    let schema = crate::tool_runtime::registry::output_schema_for_tool("session_handoff_summary");
    let output_props = schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("validation"),
        "session_handoff_summary output schema should include validation"
    );
    assert!(
        output_props.contains_key("permissions"),
        "session_handoff_summary output schema should include permissions"
    );
    assert!(
        output_props.contains_key("tool_failures"),
        "session_handoff_summary output schema should include classified tool failures"
    );
    assert!(
        output_props.contains_key("expected_failed_tool_calls"),
        "session_handoff_summary output schema should include expected failed tool calls"
    );
    assert!(
        output_props.contains_key("unexpected_failed_tool_calls"),
        "session_handoff_summary output schema should include unexpected failed tool calls"
    );
    assert!(
        output_props.contains_key("expectation_mismatches"),
        "session_handoff_summary output schema should include expectation mismatches"
    );
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    let description = output_props["validation"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "ledger-derived",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "minimal diagnostics",
        "bounded tails",
        "safe result metadata",
        "never infer root cause",
        "parser.available remains false when session ledger events lack those fields",
    ] {
        assert!(
            description.contains(phrase),
            "handoff validation output schema should mention {phrase}: {description}"
        );
    }
}

fn assert_permission_summary_schema_fields(schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "approved_count",
        "manual_approved_count",
        "auto_approved_count",
        "total_approved_count",
    ] {
        assert!(props.contains_key(field), "permissions missing {field}");
    }
}

fn assert_job_lifecycle_summary_schema_fields(schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "active_count",
        "running_count",
        "stop_requested_count",
        "terminal_pending_count",
        "blocking_active_count",
        "nonblocking_active_count",
        "warnings",
    ] {
        assert!(props.contains_key(field), "jobs summary missing {field}");
    }
}

#[test]
fn session_tool_specs_describe_ledger_vs_current_binding() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let start_desc = desc("start_session");
    for phrase in [
        "explicit wc_sess_* session_id",
        "session ledger",
        "does not by itself bind future calls as current",
    ] {
        assert!(
            start_desc.contains(phrase),
            "start_session description should mention {phrase}: {start_desc}"
        );
    }

    let summary_desc = desc("session_summary");
    for phrase in [
        "session ledger",
        "explicit session_id",
        "does not rely on current-session binding",
    ] {
        assert!(
            summary_desc.contains(phrase),
            "session_summary description should mention {phrase}: {summary_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in [
        "session ledger",
        "explicit session_id",
        "ledger-derived validation",
        "bounded tails",
        "safe result metadata",
        "validation.parser.available",
        "does not depend on current-session binding",
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    for name in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        let current_desc = desc(name);
        for phrase in ["process-local in-memory", "not the durable session ledger"] {
            assert!(
                current_desc.contains(phrase),
                "{name} description should mention {phrase}: {current_desc}"
            );
        }
    }

    for name in ["bind_current_session", "current_session"] {
        let current_desc = desc(name);
        assert!(
            current_desc.contains("may be lost on restart"),
            "{name} description should mention restart loss: {current_desc}"
        );
    }
}

#[test]
fn tool_specs_describe_default_coding_loop_preferences() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let read_file_desc = desc("read_file");
    for phrase in [
        "default inspect tool",
        "targeted source reading",
        "line numbers",
    ] {
        assert!(
            read_file_desc.contains(phrase),
            "read_file description should mention {phrase}: {read_file_desc}"
        );
    }

    let search_desc = desc("search_project_text");
    for phrase in [
        "default inspect/search tool",
        "rg-first",
        "grep fallback",
        "structured output",
        "matches",
        "context",
        "backend",
        "truncated",
    ] {
        assert!(
            search_desc.contains(phrase),
            "search_project_text description should mention {phrase}: {search_desc}"
        );
    }

    let show_changes_desc = desc("show_changes");
    for phrase in [
        "default inspect/review tool",
        "before final response",
        "bounded hunks",
    ] {
        assert!(
            show_changes_desc.contains(phrase),
            "show_changes description should mention {phrase}: {show_changes_desc}"
        );
    }

    for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
        let edit_desc = desc(name);
        for phrase in ["preferred source-code edit tool", "line", "source edits"] {
            assert!(
                edit_desc.contains(phrase),
                "{name} description should mention {phrase}: {edit_desc}"
            );
        }
    }

    let apply_text_edits_desc = desc("apply_text_edits");
    for phrase in ["preferred batch text edit tool", "atomically", "dry_run"] {
        assert!(
            apply_text_edits_desc.contains(phrase),
            "apply_text_edits description should mention {phrase}: {apply_text_edits_desc}"
        );
    }

    let apply_patch_checked_desc = desc("apply_patch_checked");
    for phrase in [
        "validated unified-diff",
        "broad or multi-file",
        "local line edits prefer",
    ] {
        assert!(
            apply_patch_checked_desc.contains(phrase),
            "apply_patch_checked description should mention {phrase}: {apply_patch_checked_desc}"
        );
    }

    for name in ["cargo_check", "cargo_test"] {
        let validation_desc = desc(name);
        assert!(
            validation_desc.contains("preferred structured"),
            "{name} should be described as preferred structured validation: {validation_desc}"
        );
        assert!(
            validation_desc.contains("before raw run_shell"),
            "{name} should steer callers away from raw run_shell first: {validation_desc}"
        );
    }

    let workspace_hygiene_desc = desc("workspace_hygiene_check");
    for phrase in ["pre-final", "workspace hygiene", "read-only"] {
        assert!(
            workspace_hygiene_desc.contains(phrase),
            "workspace_hygiene_check description should mention {phrase}: {workspace_hygiene_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in ["handoff", "multi-step tasks", "read-only"] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    let run_shell_desc = desc("run_shell");
    for phrase in [
        "bounded command escape hatch",
        "validation",
        "diagnostics",
        "do not use as the primary file editing path",
    ] {
        assert!(
            run_shell_desc.contains(phrase),
            "run_shell description should mention {phrase}: {run_shell_desc}"
        );
    }

    let write_file_desc = desc("write_project_file");
    for phrase in [
        "whole-file write compatibility path",
        "prefer structured line edits",
        "apply_text_edits",
    ] {
        assert!(
            write_file_desc.contains(phrase),
            "write_project_file description should mention {phrase}: {write_file_desc}"
        );
    }

    let replace_in_file_desc = desc("replace_in_file");
    for phrase in [
        "literal pattern compatibility path",
        "prefer replace_line_range",
        "insert_at_line",
        "delete_line_range",
    ] {
        assert!(
            replace_in_file_desc.contains(phrase),
            "replace_in_file description should mention {phrase}: {replace_in_file_desc}"
        );
    }
}

#[test]
fn tool_specs_annotations_cover_safety_hints() {
    let specs = registered_tool_specs();
    for spec in &specs {
        let annotations = spec
            .annotations
            .as_object()
            .unwrap_or_else(|| panic!("{} annotations must be an object", spec.name));
        for field in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(
                annotations.contains_key(field),
                "{} missing annotation {}",
                spec.name,
                field
            );
        }
    }

    for name in [
        "read_file",
        "git_status",
        "git_diff_summary",
        "git_diff_hunks",
        "git_log",
        "show_changes",
    ] {
        assert_eq!(spec_named(&specs, name).annotations["readOnlyHint"], true);
    }
    for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
        let annotations = &spec_named(&specs, name).annotations;
        assert_eq!(annotations["readOnlyHint"], false);
        assert_eq!(annotations["openWorldHint"], false);
    }
    for name in ["run_shell", "run_job"] {
        assert_eq!(spec_named(&specs, name).annotations["openWorldHint"], true);
    }
    for name in [
        "delete_project_files",
        "discard_untracked",
        "git_restore_paths",
    ] {
        assert_eq!(
            spec_named(&specs, name).annotations["destructiveHint"],
            true
        );
    }
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        let annotations = &spec_named(&specs, name).annotations;
        assert_eq!(annotations["readOnlyHint"], false);
        assert_eq!(annotations["destructiveHint"], false);
        assert_eq!(annotations["openWorldHint"], false);
    }
}

#[test]
fn mcp_tool_annotations_use_metadata_for_read_write_tools() {
    let specs = registered_tool_specs();
    for name in [
        "show_changes",
        "write_project_file",
        "delete_project_files",
        "run_shell",
        "cargo_test",
    ] {
        let metadata = crate::tool_runtime::metadata::lookup_tool_metadata(name).unwrap();
        let annotations = &spec_named(&specs, name).annotations;
        assert_eq!(annotations["readOnlyHint"], metadata.read_only, "{name}");
        assert_eq!(
            annotations["destructiveHint"], metadata.destructive,
            "{name}"
        );
        assert_eq!(annotations["openWorldHint"], metadata.shell_like, "{name}");
        assert_eq!(annotations["idempotentHint"], metadata.read_only, "{name}");
    }
}

#[test]
fn tool_specs_include_anchor_edit_tools() {
    let specs = registered_tool_specs();
    for required in [
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
    ] {
        let spec = specs
            .iter()
            .find(|s| s.name == required)
            .expect("anchor edit spec");
        assert!(spec.description.contains("literal"), "{}", spec.description);
        assert!(
            spec.description.contains("no regex"),
            "{}",
            spec.description
        );
    }
}

#[test]
fn tool_categories_include_edit_group() {
    use crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUP_EDIT;

    let cats = registered_tool_categories();
    let edit = cats[TOOL_DISCOVERY_GROUP_EDIT]
        .as_array()
        .expect("edit category present");
    assert!(edit.iter().any(|v| v == "replace_in_file"));
    assert!(edit.iter().any(|v| v == "write_project_file"));
    assert!(edit.iter().any(|v| v == "replace_line_range"));
    assert!(edit.iter().any(|v| v == "insert_at_line"));
    assert!(edit.iter().any(|v| v == "delete_line_range"));
    assert!(edit.iter().any(|v| v == "apply_text_edits"));
}

#[test]
fn tool_categories_include_projects_with_management_tools() {
    use crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUP_PROJECTS;

    let cats = registered_tool_categories();
    let projects = cats[TOOL_DISCOVERY_GROUP_PROJECTS]
        .as_array()
        .expect("projects category present");
    assert!(
        projects.iter().any(|v| v == "register_project"),
        "projects category must include register_project"
    );
    assert!(
        projects.iter().any(|v| v == "create_project"),
        "projects category must include create_project"
    );
}

#[test]
fn apply_text_edits_metadata_mcp_openapi_consistency() {
    use crate::tool_runtime::tool_definition::TOOL_DISCOVERY_GROUP_EDIT;

    // Known name + spec + metadata coverage. tool_specs() backs both the
    // list_tools runtime tool and MCP tools/list (parity is enforced by
    // mcp_tools_list_parity_with_rest_tools_list), so checking specs covers
    // both surfaces.
    assert!(is_known_tool_name("apply_text_edits"));
    let specs = registered_tool_specs();
    assert!(
        specs.iter().any(|s| s.name == "apply_text_edits"),
        "apply_text_edits must appear in tool_specs (list_tools + MCP tools/list)"
    );
    for spec in &specs {
        assert!(
            is_known_tool_name(&spec.name),
            "{} must be recognized by ToolCall",
            spec.name
        );
    }
    assert!(
        specs.len() < known_tool_names().count(),
        "hidden implemented tools should make public specs a strict subset"
    );
    assert!(crate::tool_runtime::metadata::lookup_tool_metadata("apply_text_edits").is_some());
    // The edit category includes the new tool.
    let cats = registered_tool_categories();
    let edit = cats[TOOL_DISCOVERY_GROUP_EDIT]
        .as_array()
        .expect("edit category present");
    assert!(edit.iter().any(|v| v == "apply_text_edits"));
    // OpenAPI ToolCallRequest description lists the name; operation count
    // stays 27 while Codex delegation is hidden (no dedicated op added).
    let spec = crate::openapi::build_openapi_spec();
    let tool_desc = &spec["components"]["schemas"]["ToolCallRequest"]["properties"]["tool"]
        ["description"]
        .as_str()
        .unwrap();
    assert!(
        tool_desc.contains("apply_text_edits"),
        "OpenAPI ToolCallRequest.tool should list apply_text_edits"
    );
    let count: usize = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|m| m.as_object().unwrap().len())
        .sum();
    assert_eq!(count, 25, "OpenAPI operation count must remain 25");
}
