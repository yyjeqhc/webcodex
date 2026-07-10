use super::*;

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
        "project_overview",
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
