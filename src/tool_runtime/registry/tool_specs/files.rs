use super::super::input_schemas::{
    list_project_files_input_schema, list_project_tracked_files_input_schema,
    project_overview_input_schema, read_file_input_schema, read_files_input_schema,
    search_project_text_input_schema, search_project_texts_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "project_overview",
            "Deterministic, bounded, metadata-only overview of an unfamiliar project: conventional project types, manifests, key files, roots, and direct children. Reads no file contents, uses no LLM, and is not semantic/LSP analysis; use read_file for contents.",
            project_overview_input_schema(),
        ),
        tool_spec(
            "list_project_tracked_files",
            "Default discovery tool: what files does this project contain? Lists Git-tracked paths in one bounded call, so ignored directories like .venv and target never appear. Supports globs, a scope, and paging; a project too large to list file by file rolls up to the deepest directory depth that fits.",
            list_project_tracked_files_input_schema(),
        ),
        tool_spec(
            "list_project_files",
            "List files in an agent-registered project directory (bounded, "
                .to_string()
                + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                + "to the owning registered agent; the server never reads the agent project "
                + "path directly.",
            list_project_files_input_schema(),
        ),
        tool_spec(
            "search_project_text",
            "Default inspect/search tool for project text. Uses rg-first with grep fallback. Supports matches/files_with_matches/count modes and context. Returns structured output with backend/truncated metadata; failure_stage/reason_code distinguish failure from a proven empty result.",
            search_project_text_input_schema(),
        ),
        tool_spec(
            "search_project_texts",
            "Run 1 to 8 bounded searches in request order. Item failures stay isolated with failure_stage/detail_code, distinct from successful empty results. Uses two Runner requests concurrently, one 30-second batch deadline, and bounded output with next_index; never reads files automatically.",
            search_project_texts_input_schema(),
        ),
        tool_spec(
            "read_file",
            "Default inspect tool for targeted source reading. Bounded UTF-8 range read with full-file sha256 and a continuation cursor (next_start_line); line numbers only change text. Oversized ranges fail range_too_large: shrink limit or narrow the range.",
            read_file_input_schema(),
        ),
        tool_spec(
            "read_files",
            "Read 1 to 8 targeted UTF-8 project files or ranges in request order. Item failures are isolated. Runner reads have true four-request concurrency, one 30-second batch deadline, and a 256 KiB serialized-result cap with next_index continuation.",
            read_files_input_schema(),
        ),
    ]
}
