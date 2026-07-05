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
