use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "workspace_hygiene_check" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Project input from the request.")),
            (
                "resolved_project",
                nullable_schema("string", "Canonical runtime project id, when resolved."),
            ),
            (
                "git_available",
                schema_type("boolean", "True when the project is a git repository."),
            ),
            (
                "clean",
                schema_type(
                    "boolean",
                    "True when git is available and no findings were reported.",
                ),
            ),
            (
                "counts",
                open_object_schema("Sparse non-zero finding counts: findings, critical, high, medium, low, untracked, tracked, large_files, secret_like_paths, cache_paths. Omitted count fields mean zero; the counts object is omitted when every count is zero."),
            ),
            (
                "findings",
                array_schema(
                    open_object_schema("Hygiene finding: path, kind, severity, tracked_status, reason, recommendation. Never includes file contents."),
                    "Bounded hygiene findings. Path is project-relative. Secret-like files are identified by name only. Omitted when there are no findings.",
                ),
            ),
            (
                "truncated",
                schema_type("boolean", "Present as true when findings were truncated to max_findings; omitted otherwise."),
            ),
            (
                "warnings",
                array_schema(
                    schema_type("string", "Warning code."),
                    "Warning codes such as non_git_project. Omitted when there are no warnings.",
                ),
            ),
            (
                "verdict",
                open_object_schema("Operator-friendly hygiene verdict: status pass/warn/fail, blocking, blocking_reasons, warning_reasons, and suggested_next_actions. The action list is empty for a clean pass and contains only actionable guidance otherwise. Does not change safety semantics."),
            ),
        ])),
        "git_restore_paths" => Some(wrapped_output_schema(vec![
            (
                "restored_paths",
                array_schema(
                    schema_type("string", "Project-relative path restored by git restore."),
                    "Requested project-relative paths restored from the git index/worktree. Result metadata only; does not grant new path, permission, or session authority.",
                ),
            ),
            (
                "command_result",
                open_object_schema("Fixed git cleanup command result metadata from git restore. This describes the requested cleanup result only, not a general shell-execution interface."),
            ),
        ])),
        "discard_untracked" => Some(wrapped_output_schema(vec![
            (
                "discarded_untracked_paths",
                array_schema(
                    schema_type("string", "Project-relative untracked path discarded by git clean."),
                    "Requested project-relative untracked paths discarded by git clean. Result metadata only; does not grant new path, permission, or session authority.",
                ),
            ),
            (
                "command_result",
                open_object_schema("Fixed git cleanup command result metadata from git clean. This describes the requested cleanup result only, not a general shell-execution interface."),
            ),
        ])),
        "delete_project_files" => Some(wrapped_output_schema(vec![
            (
                "ok",
                schema_type(
                    "boolean",
                    "True when the bounded file cleanup completed successfully.",
                ),
            ),
            (
                "state_changed",
                schema_type("boolean", "True only when successful cleanup evidence proves at least one requested project file was deleted."),
            ),
            (
                "deleted_paths",
                array_schema(
                    schema_type("string", "Deleted project-relative path."),
                    "Requested project-relative file paths removed by the selected cleanup path.",
                ),
            ),
            (
                "stdout_present",
                schema_type(
                    "boolean",
                    "Whether the legacy cleanup command produced stdout; false for structured/direct cleanup. Raw output is never exposed.",
                ),
            ),
            (
                "stderr_present",
                schema_type(
                    "boolean",
                    "Whether the legacy cleanup command produced stderr; false for structured/direct cleanup. Raw output is never exposed.",
                ),
            ),
        ])),
        _ => None,
    }
}
