use serde_json::Value;

mod checkpoints;
mod coding_tasks;
mod discovery;
mod files;
mod hygiene;
mod jobs;
mod sessions;

use super::super::tool_definition::{lookup_tool_definition, model_visible_tool_definitions};
use super::super::tool_spec::ToolSpec;
use super::super::ToolRuntime;
use super::input_schemas::{
    apply_patch_checked_input_schema, apply_patch_input_schema, apply_text_edits_input_schema,
    artifact_upload_abort_input_schema, artifact_upload_begin_input_schema,
    artifact_upload_chunk_input_schema, artifact_upload_finish_input_schema,
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema,
    delete_line_range_input_schema, git_diff_hunks_input_schema, git_diff_input_schema,
    git_diff_summary_input_schema, git_log_input_schema, git_status_input_schema,
    insert_after_pattern_input_schema, insert_at_line_input_schema,
    insert_before_pattern_input_schema, read_project_artifact_input_schema,
    read_project_artifact_metadata_input_schema, replace_exact_block_input_schema,
    replace_in_file_input_schema, replace_line_range_input_schema,
    save_project_artifact_input_schema, show_changes_input_schema, validate_patch_input_schema,
    with_common_testing_metadata, write_project_file_input_schema,
};
use super::{output_schema_for_tool, tool_annotations};

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let mut declarations = discovery::tool_specs();
        declarations.extend(sessions::tool_specs());
        declarations.extend(jobs::tool_specs());
        declarations.extend(checkpoints::tool_specs());
        declarations.extend(coding_tasks::tool_specs());
        declarations.extend(hygiene::tool_specs());
        declarations.extend(files::tool_specs());
        declarations.extend(vec![
            tool_spec(
                "git_diff_summary",
                "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                git_diff_summary_input_schema(),
            ),
            tool_spec(
                "show_changes",
                "Default inspect/review tool before final response. Read-only worktree plus optional session summary; reports status, warnings, next actions, and bounded hunks without modifying files.",
                show_changes_input_schema(),
            ),
            tool_spec(
                "git_status",
                "Run git status --porcelain for a project.",
                git_status_input_schema(),
            ),
            tool_spec(
                "git_diff",
                "Run git diff for a project, optionally scoped to paths.",
                git_diff_input_schema(),
            ),
            tool_spec(
                "git_diff_hunks",
                "Return bounded structured git diff hunks for review. Supports optional paths and cached diff; does not modify the worktree.",
                git_diff_hunks_input_schema(),
            ),
            tool_spec(
                "git_log",
                "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.",
                git_log_input_schema(),
            ),
            tool_spec(
                "cargo_fmt",
                "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation.",
                cargo_fmt_input_schema(),
            ),
            tool_spec(
                "cargo_check",
                "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation; use before raw run_shell when applicable.",
                cargo_check_input_schema(),
            ),
            tool_spec(
                "cargo_test",
                "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails; use before raw run_shell when applicable.",
                cargo_test_input_schema(),
            ),
            tool_spec(
                "apply_patch",
                "Apply a unified diff patch to an agent-registered project.".to_string(),
                apply_patch_input_schema(),
            ),
            tool_spec(
                "apply_patch_checked",
                "Validated unified-diff edit tool for broad or multi-file patches. Returns a diff summary; for local line edits prefer replace_line_range, insert_at_line, delete_line_range, or apply_text_edits.",
                apply_patch_checked_input_schema(),
            ),
            tool_spec(
                "validate_patch",
                "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.",
                validate_patch_input_schema(),
            ),
            tool_spec(
                "replace_in_file",
                "Literal pattern compatibility path for short exact replacements. Prefer replace_line_range, insert_at_line, or delete_line_range when line numbers are available; fails without writing when old text is missing or ambiguous.",
                replace_in_file_input_schema(),
            ),
            tool_spec(
                "replace_exact_block",
                "Replace literal UTF-8 text that matches exactly once; no regex or auto-format. Use line edit tools when line numbers are known.",
                replace_exact_block_input_schema(),
            ),
            tool_spec(
                "insert_before_pattern",
                "Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                insert_before_pattern_input_schema(),
            ),
            tool_spec(
                "insert_after_pattern",
                "Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.",
                insert_after_pattern_input_schema(),
            ),
            tool_spec(
                "write_project_file",
                "Whole-file write compatibility path for new files or deliberate small overwrites. Prefer structured line edits or apply_text_edits for source changes; requires overwrite/guards for existing files.",
                write_project_file_input_schema(),
            ),
            tool_spec(
                "save_project_artifact",
                "Write a bounded binary project artifact from base64. Use for imported session files, generated images, PDFs, and zip files; not for UTF-8 source edits.",
                save_project_artifact_input_schema(),
            ),
            tool_spec(
                "read_project_artifact_metadata",
                "Read bounded metadata for a binary artifact; images include dimensions and zip archives are counted but never extracted. Set allow_missing=true to make a missing artifact a successful exists=false negative assertion.",
                read_project_artifact_metadata_input_schema(),
            ),
            tool_spec(
                "read_project_artifact",
                "Chunked content read for a project artifact. Returns base64 for one small segment plus full-file sha256/MIME metadata; not a large-file transfer tool.",
                read_project_artifact_input_schema(),
            ),
            tool_spec(
                "artifact_upload_begin",
                "Begin a bounded chunked binary artifact upload. Creates a project-local temporary upload session; finish commits atomically to the target path. For smoke octet-stream uploads, use artifacts/smoke/<name>.artifact or omit mime_type when appropriate.",
                artifact_upload_begin_input_schema(),
            ),
            tool_spec(
                "artifact_upload_chunk",
                "Append one base64 chunk to an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id to the target path.",
                artifact_upload_chunk_input_schema(),
            ),
            tool_spec(
                "artifact_upload_finish",
                "Finish an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before atomic commit.",
                artifact_upload_finish_input_schema(),
            ),
            tool_spec(
                "artifact_upload_abort",
                "Abort an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before cleanup and reports final_file_exists without touching the final target.",
                artifact_upload_abort_input_schema(),
            ),
            tool_spec(
                "replace_line_range",
                "Preferred source-code edit tool for local line changes with clear line numbers. Replaces a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                replace_line_range_input_schema(),
            ),
            tool_spec(
                "insert_at_line",
                "Preferred source-code edit tool for local line changes with clear line numbers. Inserts before a specified 1-based line; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                insert_at_line_input_schema(),
            ),
            tool_spec(
                "delete_line_range",
                "Preferred source-code edit tool for local line changes with clear line numbers. Deletes a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
                delete_line_range_input_schema(),
            ),
            tool_spec(
                "apply_text_edits",
                "Preferred batch text edit tool for coordinated source changes in one UTF-8 file. Applies bounded exact replace/insert/delete edits atomically only when all matches validate as unique/non-overlapping. Supports dry_run and sha256 guard.",
                apply_text_edits_input_schema(),
            ),
        ]);
        model_visible_tool_definitions()
            .map(|definition| {
                declarations
                    .iter()
                    .find(|spec| spec.name == definition.name)
                    .unwrap_or_else(|| {
                        panic!(
                            "{} public ToolDefinition is missing a ToolSpec declaration",
                            definition.name
                        )
                    })
                    .clone()
            })
            .map(with_common_testing_metadata)
            .collect()
    }

    /// The sorted list of accepted runtime tool names (mirrors `tool_specs`).
    #[cfg(test)]
    pub fn tool_names(&self) -> Vec<String> {
        model_visible_tool_definitions()
            .map(|definition| definition.name.to_string())
            .collect()
    }
}

pub(super) fn tool_spec(
    name: &'static str,
    description: impl Into<String>,
    input_schema: Value,
) -> ToolSpec {
    debug_assert!(
        lookup_tool_definition(name).is_some(),
        "{name} ToolSpec is missing a ToolDefinition"
    );
    ToolSpec {
        name: name.to_string(),
        description: description.into(),
        input_schema,
        output_schema: output_schema_for_tool(name),
        annotations: tool_annotations(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use crate::tool_runtime::RuntimeInfo;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "projects not configured for test".to_string(),
                "test".to_string(),
            )),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    #[test]
    fn tool_specs_patch_fields_reject_codex_wrapper() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for tool in ["apply_patch", "apply_patch_checked", "validate_patch"] {
            let spec = specs
                .iter()
                .find(|spec| spec.name == tool)
                .unwrap_or_else(|| panic!("missing tool spec: {tool}"));
            let description = spec.input_schema["properties"]["patch"]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("missing patch description for {tool}"));
            assert!(
                description.contains("raw standard unified diff"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("Codex apply_patch wrapper"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("*** Begin Patch"),
                "{tool}: {description}"
            );
        }
    }
}
