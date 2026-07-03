//! Schema tests for tool_runtime.

use super::super::types::*;
use super::super::*;
use super::support::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[test]
fn tool_specs_and_metadata_are_synchronized() {
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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

    for metadata in crate::tool_runtime::metadata::TOOL_METADATA {
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
    use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk};

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
            AgentCapability::Shell,
        ),
        (
            "workspace_checkpoint_restore",
            ToolRisk::ProjectWrite,
            AgentCapability::Shell,
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

    let runtime = test_runtime();
    let specs = runtime.tool_specs();
    let expected_project_tools = specs
        .iter()
        .filter_map(|spec| {
            let metadata = lookup_tool_metadata(&spec.name).unwrap();
            ((metadata.provider_id == "agent" || spec.name.starts_with("workspace_checkpoint_"))
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
            ToolRuntime::required_agent_capability(&call),
            Some(capability),
            "{name} capability"
        );
    }
}

#[test]
fn tool_specs_names_are_unique() {
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
    let mut names = specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    names.sort();
    let mut deduped = names.clone();
    deduped.dedup();
    assert_eq!(names, deduped, "tool names must be unique");
}

#[test]
fn tool_specs_names_are_snake_case() {
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
fn tool_specs_input_schemas_are_objects() {
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
fn tool_specs_output_schemas_are_objects() {
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
        "job_id",
        "status",
        "exit_code",
        "started_at",
        "ended_at",
        "error",
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
        "offset",
        "next_offset",
        "tail_lines",
    ] {
        assert!(
            has_output_field("job_log", field),
            "job_log missing {field}"
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
        "quic",
    ] {
        assert!(
            has_output_field("runtime_status", field),
            "runtime_status missing {field}"
        );
    }
}

#[test]
fn tool_specs_required_fields_match_deserialization() {
    // For every tool spec, building arguments with only the required
    // fields must deserialize successfully, and omitting any required
    // field must fail.
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let names: Vec<String> = runtime
        .tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    for expected in [
        "list_tools",
        "list_projects",
        "list_agents",
        "runtime_status",
        "run_shell",
        "run_job",
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
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
        ("job_tail", vec!["job_id"], vec!["tail_lines"]),
    ];
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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

    let spec = spec_named(&specs, "read_file");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("with_line_numbers"));
}

#[test]
fn tool_categories_and_recommended_flows_are_well_formed() {
    let runtime = test_runtime();
    let categories = runtime.tool_categories();
    // Every declared category is a non-empty array of known tool names.
    let names = runtime.tool_names();
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
        "inspect",
        "git",
        "review",
        "validation",
        "patch",
        "shell",
        "jobs",
        "runtime",
        "cleanup",
    ] {
        assert!(
            categories.as_object().unwrap().contains_key(cat),
            "missing category {}",
            cat
        );
    }
    let validation = categories["validation"].as_array().unwrap();
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(validation.iter().any(|v| v == name));
    }
    let review = categories["review"].as_array().unwrap();
    assert!(review.iter().any(|v| v == "git_diff_hunks"));
    assert!(review.iter().any(|v| v == "git_log"));
    // recommended_flows are short and non-empty.
    let flows = ToolRuntime::recommended_flows();
    assert!(!flows.is_empty());
    for flow in &flows {
        assert!(flow.chars().count() <= 300, "flow too long: {}", flow);
    }
    let joined_flows = flows.join("\n").to_lowercase();
    assert!(joined_flows.contains("source code edit"));
    for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
        assert!(
            joined_flows.contains(name),
            "recommended flows should mention {}",
            name
        );
    }
    assert!(joined_flows.contains("run_shell"));
    assert!(
        joined_flows.contains("validation") || joined_flows.contains("checks"),
        "run_shell guidance should mention validation/checks"
    );
    assert!(
        joined_flows.contains("not primary"),
        "run_shell should not be the primary edit path"
    );
    for name in ["cargo_fmt", "cargo_check", "cargo_test", "git_diff_hunks"] {
        assert!(
            joined_flows.contains(name),
            "recommended flows should mention {}",
            name
        );
    }
    let specs = runtime.tool_specs();
    for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
        let desc = spec_named(&specs, name).description.to_lowercase();
        assert!(desc.contains("preferred"), "{} should be preferred", name);
        assert!(desc.contains("source"), "{} should mention source", name);
        assert!(desc.contains("line"), "{} should mention line", name);
    }

    let run_shell_desc = spec_named(&specs, "run_shell").description.to_lowercase();
    assert!(run_shell_desc.contains("file editing path"));
    assert!(run_shell_desc.contains("not"));
}

#[test]
fn tool_specs_annotations_cover_safety_hints() {
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
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
    let runtime = test_runtime();
    let cats = runtime.tool_categories();
    let edit = cats["edit"].as_array().expect("edit category present");
    assert!(edit.iter().any(|v| v == "replace_in_file"));
    assert!(edit.iter().any(|v| v == "write_project_file"));
    assert!(edit.iter().any(|v| v == "replace_line_range"));
    assert!(edit.iter().any(|v| v == "insert_at_line"));
    assert!(edit.iter().any(|v| v == "delete_line_range"));
    assert!(edit.iter().any(|v| v == "apply_text_edits"));
}

#[test]
fn tool_categories_include_projects_with_management_tools() {
    let runtime = test_runtime();
    let cats = runtime.tool_categories();
    let projects = cats["projects"]
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
    let runtime = test_runtime();
    // Known name + spec + metadata coverage. tool_specs() backs both the
    // list_tools runtime tool and MCP tools/list (parity is enforced by
    // mcp_tools_list_parity_with_rest_tools_list), so checking specs covers
    // both surfaces.
    assert!(KNOWN_TOOL_NAMES.contains(&"apply_text_edits"));
    let specs = runtime.tool_specs();
    assert!(
        specs.iter().any(|s| s.name == "apply_text_edits"),
        "apply_text_edits must appear in tool_specs (list_tools + MCP tools/list)"
    );
    for spec in &specs {
        assert!(
            KNOWN_TOOL_NAMES.contains(&spec.name.as_str()),
            "{} must be recognized by ToolCall",
            spec.name
        );
    }
    assert!(
        specs.len() < KNOWN_TOOL_NAMES.len(),
        "hidden implemented tools should make public specs a strict subset"
    );
    assert!(crate::tool_runtime::metadata::lookup_tool_metadata("apply_text_edits").is_some());
    // The edit category includes the new tool.
    let cats = runtime.tool_categories();
    let edit = cats["edit"].as_array().expect("edit category present");
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
    assert_eq!(count, 27, "OpenAPI operation count must remain 27");
}
