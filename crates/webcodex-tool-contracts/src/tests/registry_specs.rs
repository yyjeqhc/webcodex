use super::*;

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
        "diff_review_handoff",
        "git_diff_hunks",
    ] {
        assert!(
            show_changes_desc.contains(phrase),
            "show_changes description should mention {phrase}: {show_changes_desc}"
        );
    }

    let git_diff_hunks = spec_named(&specs, "git_diff_hunks");
    let git_diff_hunks_desc = git_diff_hunks.description.to_lowercase();
    for phrase in [
        "targeted/paged",
        "scope-bound",
        "replay",
        "scope",
        "paging inputs",
        "later records",
        "hunk_line_limit",
        "larger max_hunk_lines",
        "narrower paths",
    ] {
        assert!(
            git_diff_hunks_desc.contains(phrase),
            "git_diff_hunks description should mention {phrase}: {git_diff_hunks_desc}"
        );
    }
    let continuation_desc = git_diff_hunks.input_schema["properties"]["continuation"]
        ["description"]
        .as_str()
        .expect("git_diff_hunks continuation description")
        .to_lowercase();
    for phrase in [
        "repeat the exact original",
        "base_commit/head_commit",
        "cached/worktree mode",
        "paths",
        "max_hunks",
        "max_hunk_lines",
        "scope-bound",
        "does not reconstruct",
    ] {
        assert!(
            continuation_desc.contains(phrase),
            "git_diff_hunks continuation description should mention {phrase}: {continuation_desc}"
        );
    }

    // Primary model-generated edit path.
    let apply_patch_desc = desc("apply_patch");
    for phrase in [
        "primary model edit path",
        "contextual/multi-file codex patches",
        "transactional",
        "sha rechecks",
        "rollback",
        "dry_run",
        "strict_match",
        "strict_matching=true",
        "exact-unique positioning",
        "apply_text_edits",
        "small exact edits",
        "external diffs",
    ] {
        assert!(
            apply_patch_desc.contains(phrase),
            "apply_patch description should mention {phrase}: {apply_patch_desc}"
        );
    }

    // Small exact guarded-edit fallback.
    let apply_text_edits_desc = desc("apply_text_edits");
    for phrase in [
        "precision fallback",
        "small exact guarded file changes",
        "current worktree",
        "transactional",
        "sha-guarded",
        "unique by default",
        "occurrence",
        "line_scope",
        "prefer apply_patch",
        "contextual",
        "multi-hunk",
        "multi-file",
    ] {
        assert!(
            apply_text_edits_desc.contains(phrase),
            "apply_text_edits description should mention {phrase}: {apply_text_edits_desc}"
        );
    }

    // Raw/external unified-diff path owns its own preflight and recovery semantics.
    let unified_diff_desc = desc("apply_unified_diff");
    for phrase in [
        "external raw unified-diff mutation path",
        "prefer apply_patch",
        "model-generated changes",
        "apply_text_edits",
        "small exact guarded edits",
        "bounded preflight",
        "never needs a separate validation call",
        "standard unified diff",
    ] {
        assert!(
            unified_diff_desc.contains(phrase),
            "apply_unified_diff description should mention {phrase}: {unified_diff_desc}"
        );
    }

    // Whole-file write is not the ordinary local-edit default.
    let write_file_desc = desc("write_project_file");
    for phrase in [
        "create new files",
        "whole-file rewrites",
        "prefer apply_patch",
        "model-generated changes",
        "apply_text_edits",
        "small exact guarded edits",
        "inspect current content",
        "expected_sha256",
    ] {
        assert!(
            write_file_desc.contains(phrase),
            "write_project_file description should mention {phrase}: {write_file_desc}"
        );
    }

    // The legacy single-purpose edit tools (replace_line_range, insert_at_line,
    // delete_line_range, replace_in_file, replace_exact_block,
    // insert_before_pattern, insert_after_pattern) were removed; they no longer
    // carry a model-facing ToolSpec/description. Their absence from the known
    // tool set is asserted by
    // `removed_legacy_edit_tools_are_not_known_tools` and the parser-name gate
    // in `tool_call_parser_name_gate_matches_tool_definitions`.

    for name in ["cargo_check", "cargo_test"] {
        let validation_desc = desc(name);
        assert!(
            validation_desc.contains("preferred structured"),
            "{name} should be described as preferred structured validation: {validation_desc}"
        );
        assert!(
            !validation_desc.contains("run_shell"),
            "{name} should express structured preference without sibling-tool name pollution: {validation_desc}"
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
        "bounded shell command",
        "escape hatch",
        "structured validation",
        "edit tools",
        "longer work",
        "open_session_shell + session_shell_exec",
    ] {
        assert!(
            run_shell_desc.contains(phrase),
            "run_shell description should mention {phrase}: {run_shell_desc}"
        );
    }

    let run_process_desc = desc("run_process");
    for phrase in [
        "isolated one-shot native executable",
        "literal argv",
        "open_session_shell + session_shell_exec",
    ] {
        assert!(
            run_process_desc.contains(phrase),
            "run_process description should mention {phrase}: {run_process_desc}"
        );
    }

    let open_shell_desc = desc("open_session_shell");
    for phrase in [
        "execution_context.resource",
        "update_session_context",
        "no per-shell host/resource parameter",
        "does not need webcodex runner",
    ] {
        assert!(
            open_shell_desc.contains(phrase),
            "open_session_shell description should mention {phrase}: {open_shell_desc}"
        );
    }

    let update_context_desc = desc("update_session_context");
    for phrase in ["runner-local named ssh resource", "open_session_shell"] {
        assert!(
            update_context_desc.contains(phrase),
            "update_session_context description should mention {phrase}: {update_context_desc}"
        );
    }

    let persistent_exec = spec_named(&specs, "session_shell_exec");
    assert_eq!(
        persistent_exec.input_schema["properties"]["result_expectation"]["enum"],
        json!(["success", "failure", "observe"])
    );
    assert!(persistent_exec.input_schema["properties"]
        .get("accepted_exit_codes")
        .is_none());

    let resource_desc = spec_named(&specs, "update_session_context").input_schema["properties"]
        ["execution_context"]["properties"]["resource"]["description"]
        .as_str()
        .expect("update_session_context resource description")
        .to_lowercase();
    for phrase in [
        "logical name",
        "runner-owned resource",
        "open_session_shell",
    ] {
        assert!(
            resource_desc.contains(phrase),
            "execution_context.resource should mention {phrase}: {resource_desc}"
        );
    }
}

#[test]
fn removed_legacy_edit_tools_are_not_known_tools() {
    let specs = registered_tool_specs();
    let spec_names: std::collections::BTreeSet<&str> =
        specs.iter().map(|s| s.name.as_str()).collect();
    for removed in [
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "replace_in_file",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        "apply_patch_checked",
        "validate_patch",
    ] {
        assert!(
            !is_known_tool_name(removed),
            "{removed} must no longer be a known tool definition"
        );
        assert!(
            !is_model_hidden_tool_name(removed),
            "{removed} must not be a hidden ToolDefinition"
        );
        assert!(
            !spec_names.contains(removed),
            "{removed} must not keep a model-facing ToolSpec"
        );
    }
}

#[test]
fn edit_tool_surface_keeps_canonical_tools_visible_and_schemas_stable() {
    let specs = registered_tool_specs();
    let names: std::collections::BTreeSet<&str> =
        specs.iter().map(|spec| spec.name.as_str()).collect();

    for required in [
        "apply_text_edits",
        "apply_patch",
        "apply_unified_diff",
        "write_project_file",
    ] {
        assert!(
            names.contains(required),
            "edit surface must keep {required} model-visible"
        );
        let spec = spec_named(&specs, required);
        assert!(
            spec.input_schema.is_object(),
            "{required} must keep an object input schema"
        );
        assert!(
            !spec.input_schema.as_object().unwrap().is_empty(),
            "{required} input schema must not be empty"
        );
    }

    // Parameter surface smoke checks (names only; not full-schema snapshots).
    let text_edits = &spec_named(&specs, "apply_text_edits").input_schema["properties"];
    for field in ["project", "changes", "dry_run"] {
        assert!(
            text_edits.get(field).is_some(),
            "apply_text_edits must keep field {field}"
        );
    }
    let codex_patch = &spec_named(&specs, "apply_patch").input_schema["properties"];
    for field in ["project", "patch", "dry_run", "strict_matching"] {
        assert!(
            codex_patch.get(field).is_some(),
            "apply_patch must keep field {field}"
        );
    }
    let patch_spec = spec_named(&specs, "apply_patch");
    let patch_output = &patch_spec.output_schema["properties"]["output"]["properties"];
    assert!(
        patch_output.get("match_diagnostic").is_some(),
        "apply_patch failures must expose body-free match diagnostics"
    );
    let recovery = patch_output
        .get("recovery")
        .expect("apply_patch must publish bounded context-mismatch recovery");
    assert_eq!(recovery["additionalProperties"], false);
    assert_eq!(
        recovery["properties"]["action"]["enum"],
        json!(["read_files"])
    );
    assert_eq!(recovery["properties"]["items"]["maxItems"], 1);
    assert_eq!(
        recovery["properties"]["items"]["items"]["properties"]["limit"]["maximum"],
        webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES
    );
    let patch_files = &patch_output["files"];
    assert_eq!(patch_files["type"], "array");
    let file_properties = patch_files["items"]["properties"]
        .as_object()
        .expect("apply_patch file summary properties");
    for field in [
        "index",
        "kind",
        "path",
        "to_path",
        "old_sha256",
        "new_sha256",
        "changed",
        "would_change",
        "edits",
    ] {
        assert!(
            file_properties.contains_key(field),
            "apply_patch file summary must expose {field}"
        );
    }
    let edit_properties = file_properties["edits"]["items"]["properties"]
        .as_object()
        .expect("apply_patch edit summary properties");
    for field in [
        "chunk_index",
        "change_context_present",
        "old_line_count",
        "new_line_count",
        "end_of_file",
        "match_mode",
        "match_source",
        "matched_start_line",
        "candidate_count",
        "strict_match",
    ] {
        assert!(
            edit_properties.contains_key(field),
            "apply_patch edit summary must expose {field}"
        );
    }
    let unified_diff = &spec_named(&specs, "apply_unified_diff").input_schema["properties"];
    for field in ["project", "diff", "deny_sensitive_paths"] {
        assert!(
            unified_diff.get(field).is_some(),
            "apply_unified_diff must keep field {field}"
        );
    }
    assert!(unified_diff.get("patch").is_none());
    let write_file = &spec_named(&specs, "write_project_file").input_schema["properties"];
    for field in ["project", "path", "content"] {
        assert!(
            write_file.get(field).is_some(),
            "write_project_file must keep field {field}"
        );
    }
}

#[test]
fn session_tool_specs_describe_explicit_targeting() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let summary_desc = desc("session_summary");
    for phrase in ["session ledger", "explicit session_id"] {
        assert!(
            summary_desc.contains(phrase),
            "session_summary description should mention {phrase}: {summary_desc}"
        );
    }

    let update = spec_named(&specs, "update_session_context");
    assert_eq!(
        update.input_schema["required"],
        json!(["project", "session_id", "execution_context"])
    );
    assert_eq!(update.input_schema["additionalProperties"], false);
    assert_eq!(
        update.input_schema["properties"]["execution_context"]["additionalProperties"],
        false
    );
    assert!(
        update.input_schema["properties"]["execution_context"]["properties"]
            .get("resource")
            .is_some(),
        "update_session_context must expose the named SSH resource field"
    );
    let update_desc = update.description.to_lowercase();
    for phrase in [
        "authorized project",
        "exact session project",
        "cross-project escape is not supported",
        "store lock",
        "background writer",
        "success does not mean",
        "never falls back",
        "never creates",
    ] {
        assert!(
            update_desc.contains(phrase),
            "update_session_context description should mention {phrase}: {update_desc}"
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
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    for removed in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            !names.contains(&removed),
            "removed Session tool leaked into specs: {removed}"
        );
    }
}
