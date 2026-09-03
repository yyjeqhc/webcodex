use super::*;
use std::collections::BTreeSet;

fn registered_tool_categories() -> Value {
    Value::Object(
        TOOL_DISCOVERY_GROUPS
            .iter()
            .map(|group| {
                (
                    group.name.to_string(),
                    Value::Array(
                        group
                            .tools
                            .iter()
                            .map(|tool| Value::String((*tool).to_string()))
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}

fn recommended_flows() -> Vec<&'static str> {
    TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| flow.summary)
        .collect()
}

#[test]
fn list_tools_schema_exposes_bounded_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "list_tools");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "list_tools input schema",
        present: ["category", "features", "summary_only", "limit"]
    );
    assert!(spec.input_schema["required"].as_array().unwrap().is_empty());
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "list_tools output schema",
        present: [
            "category", "features", "limit", "returned_count", "total_count",
            "filtered_count", "limit_applied", "requested_limit", "truncation_reason",
            "truncated", "categories", "recommended_flows",
        ]
    );
}

#[test]
fn tool_manifest_schema_exposes_compact_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "tool_manifest");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "tool_manifest input schema",
        present: ["category", "intent", "include_recommended_flows", "include_risk_summary"]
    );
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "tool_manifest output schema",
        present: [
            "schema_version", "count", "tool_count", "filtered_count", "category", "intent",
            "available_intents", "filtered", "categories_requested", "limit", "returned_count",
            "total_count", "limit_applied", "requested_limit", "truncation_reason", "truncated",
            "categories", "tools", "risk_summary", "recommended_flows",
        ]
    );
}

#[test]
fn tool_recommended_flows_reference_visible_defined_tools() {
    let expected_summaries = TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| {
            assert!(!flow.name.trim().is_empty());
            assert!(!flow.manifest_purpose.trim().is_empty(), "{}", flow.name);
            assert!(flow.summary.chars().count() <= 300, "{}", flow.name);
            assert!(!flow.tools.is_empty(), "{}", flow.name);
            for tool in flow.tools {
                let definition = lookup_tool_definition(tool)
                    .unwrap_or_else(|| panic!("{} references unknown tool {tool}", flow.name));
                assert!(
                    definition.visibility.is_model_visible(),
                    "{}: {tool}",
                    flow.name
                );
                assert!(is_model_visible_tool_name(tool), "{}: {tool}", flow.name);
            }
            flow.summary
        })
        .collect::<Vec<_>>();
    assert_eq!(recommended_flows(), expected_summaries);
}

#[test]
fn edit_recommended_flow_prefers_apply_patch_before_exact_edits() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "edit")
        .expect("edit recommended flow");
    assert_eq!(flow.tools.first().copied(), Some("apply_patch"));
    assert_eq!(flow.tools.get(1).copied(), Some("apply_text_edits"));
    assert!(flow.summary.starts_with("Edit: prefer apply_patch"));
}

#[test]
fn tool_categories_and_recommended_flows_are_well_formed() {
    let categories = registered_tool_categories();
    let names = registered_tool_names();
    for (cat, members) in categories.as_object().unwrap() {
        let arr = members.as_array().unwrap();
        assert!(!arr.is_empty(), "category '{cat}' must not be empty");
        for member in arr {
            let name = member.as_str().unwrap();
            assert!(
                names.iter().any(|candidate| candidate == name),
                "{cat}: {name}"
            );
        }
    }
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
            "missing category {cat}"
        );
    }
    let validation = categories[TOOL_DISCOVERY_GROUP_VALIDATION]
        .as_array()
        .unwrap();
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(validation.iter().any(|value| value == name));
    }
    let review = categories[TOOL_DISCOVERY_GROUP_REVIEW].as_array().unwrap();
    assert!(review.iter().any(|value| value == "git_diff_hunks"));
    assert!(review
        .iter()
        .any(|value| value == "workspace_hygiene_check"));
    assert!(review.iter().any(|value| value == "git_log"));
    let inspect = categories[TOOL_DISCOVERY_GROUP_INSPECT].as_array().unwrap();
    for name in [
        "read_file",
        "run_shell",
        "search_project_text",
        "show_changes",
    ] {
        assert!(
            inspect.iter().any(|value| value == name),
            "inspect category: {name}"
        );
    }
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT].as_array().unwrap();
    let edit_prefix = edit
        .iter()
        .take(5)
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        edit_prefix,
        vec![
            "apply_patch",
            "apply_text_edits",
            "apply_unified_diff",
            "write_project_file",
            "save_project_artifact"
        ]
    );
    let flows = recommended_flows();
    assert!(!flows.is_empty());
    for flow in &flows {
        assert!(flow.chars().count() <= 300, "flow too long: {flow}");
    }
    let joined_flows = flows.join("\n").to_lowercase();
    for phrase in [
        "use search_project_text for bounded code search",
        "run_shell with rg or git grep remains the diagnostic escape hatch",
        "inspect: use search_project_text and read_file before editing",
        "run_shell with rg or git grep is the diagnostic escape hatch",
        "edit: prefer apply_patch for model-generated contextual",
        "use apply_text_edits for small exact guarded edits",
        "apply_unified_diff only for external raw diffs",
        "write_project_file only for intentional whole-file rewrites",
        "validate: use cargo_check / cargo_test / go_test",
        "raw run_shell is a bounded escape hatch",
        "not the primary validation path",
        "review: start with show_changes for the bounded worktree overview",
        "if hunks truncate, continue/focus with git_diff_hunks",
        "handoff: use session_summary / session_handoff_summary",
    ] {
        assert!(
            joined_flows.contains(phrase),
            "recommended flows should mention {phrase}"
        );
    }
}

#[test]
fn tool_categories_include_edit_group() {
    let categories = registered_tool_categories();
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT]
        .as_array()
        .expect("edit category present");
    for present in [
        "apply_text_edits",
        "apply_patch",
        "write_project_file",
        "apply_unified_diff",
    ] {
        assert!(edit.iter().any(|value| value == present));
    }
    for removed in ["replace_in_file", "replace_line_range", "insert_at_line"] {
        assert!(!edit.iter().any(|value| value == removed));
    }
}

#[test]
fn tool_categories_include_projects_with_management_tools() {
    let categories = registered_tool_categories();
    let projects = categories[TOOL_DISCOVERY_GROUP_PROJECTS]
        .as_array()
        .expect("projects category present");
    assert!(projects.iter().any(|value| value == "register_project"));
    assert!(projects.iter().any(|value| value == "create_project"));
}

#[test]
fn tool_manifest_intents_reference_only_known_model_visible_tools() {
    let expected = ["coding", "audit", "exploration", "release", "discovery"];
    let names = TOOL_MANIFEST_INTENTS
        .iter()
        .map(|intent| intent.name)
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    assert_eq!(available_tool_manifest_intent_names(), names);
    for name in &names {
        let resolved = resolve_tool_manifest_intent(name)
            .unwrap_or_else(|unknown| panic!("available intent {unknown} must resolve"))
            .unwrap_or_else(|| panic!("available intent {name} must not resolve as empty"));
        assert_eq!(resolved.name, *name);
    }

    let mut seen = BTreeSet::new();
    for intent in TOOL_MANIFEST_INTENTS {
        assert!(!intent.tools.is_empty(), "{}", intent.name);
        assert!(seen.insert(intent.name), "duplicate intent {}", intent.name);
        for tool in intent.tools {
            assert!(is_known_tool_name(tool), "{}: {tool}", intent.name);
            assert!(is_model_visible_tool_name(tool), "{}: {tool}", intent.name);
            if matches!(intent.name, "audit" | "exploration" | "release") {
                assert_ne!(*tool, "run_shell", "{}", intent.name);
                assert_ne!(*tool, "run_job", "{}", intent.name);
            }
        }
    }
}

#[test]
fn project_overview_manifest_profiles_match_intended_workflows() {
    for intent in ["coding", "audit", "exploration", "discovery"] {
        let profile = TOOL_MANIFEST_INTENTS
            .iter()
            .find(|profile| profile.name == intent)
            .unwrap_or_else(|| panic!("missing {intent} intent"));
        assert!(profile.tools.contains(&"project_overview"), "{intent}");
    }
    let release = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|profile| profile.name == "release")
        .expect("release intent");
    assert!(!release.tools.contains(&"project_overview"));
}

#[test]
fn coding_intent_matches_local_coding_canonical_tools() {
    let coding = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .expect("coding intent");
    assert_eq!(coding.tools, LOCAL_CODING_TOOL_NAMES);
    assert_eq!(coding.tools.first().copied(), Some("work_on_project"));
    assert_eq!(coding.tools.last().copied(), Some("finish_coding_task"));
    assert!(!coding.tools.contains(&"start_coding_task"));
    let apply_patch_position = coding
        .tools
        .iter()
        .position(|tool| *tool == "apply_patch")
        .unwrap();
    let apply_text_edits_position = coding
        .tools
        .iter()
        .position(|tool| *tool == "apply_text_edits")
        .unwrap();
    assert!(apply_patch_position < apply_text_edits_position);
    for middle in [
        "project_overview",
        "apply_patch",
        "apply_text_edits",
        "apply_unified_diff",
        "cargo_test",
        "show_changes",
    ] {
        let position = coding
            .tools
            .iter()
            .position(|tool| *tool == middle)
            .unwrap();
        assert!(position > 0 && position + 1 < coding.tools.len());
    }
    assert!(coding.tools.contains(&"run_shell"));
    assert!(coding.tools.contains(&"run_job"));
    assert!(!coding.tools.contains(&"git_restore_paths"));
    assert!(!coding.tools.contains(&"discard_untracked"));
}
