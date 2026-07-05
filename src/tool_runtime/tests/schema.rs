//! Schema tests for tool_runtime.

mod artifacts;
mod definitions;
mod discovery;
mod flattened_args;
mod outputs;
mod policy;
mod sessions;
mod specs;

use super::super::*;
use super::support::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;

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
