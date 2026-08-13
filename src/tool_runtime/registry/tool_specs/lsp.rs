use super::super::input_schemas::{
    call_hierarchy_input_schema, document_diagnostics_input_schema, document_symbols_input_schema,
    find_references_input_schema, goto_definition_input_schema, hover_input_schema,
    lsp_status_input_schema, workspace_symbols_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "lsp_status",
            "Read-only probe of configured agent-side language-server availability for a project. Does not start a language server, run checks, or execute project code. Returns detected languages and availability/running status without absolute executable paths.",
            lsp_status_input_schema(),
        ),
        tool_spec(
            "document_symbols",
            "Read-only hierarchical document symbols for a project-relative supported source file via its configured agent-side language server. Returns project-relative paths, 1-based Unicode scalar columns, and bounded pre-order results. External or invalid ranges are omitted.",
            document_symbols_input_schema(),
        ),
        tool_spec(
            "document_diagnostics",
            "Read-only bounded diagnostics for a project-relative supported source file via agent-side publishDiagnostics. Returns normalized 1-based Unicode scalar ranges and explicit freshness/timeout state; it does not run a project check.",
            document_diagnostics_input_schema(),
        ),
        tool_spec(
            "hover",
            "Read-only hover for a project-relative supported source file at a 1-based Unicode scalar position via its configured agent-side language server. MarkupContent and MarkedString forms are normalized to bounded markdown/plaintext; invalid optional ranges are omitted.",
            hover_input_schema(),
        ),
        tool_spec(
            "workspace_symbols",
            "Read-only bounded workspace/symbol query via configured agent-side language servers. Requires a non-empty 1..200 character query; results are workspace-filtered, sorted, deduplicated, and use project-relative paths only.",
            workspace_symbols_input_schema(),
        ),
        tool_spec(
            "goto_definition",
            "Read-only goto-definition for a project-relative supported source file at a 1-based Unicode scalar position via its configured agent-side language server. Supports Location, Location[], and LocationLink[]; external dependency results are omitted.",
            goto_definition_input_schema(),
        ),
        tool_spec(
            "find_references",
            "Read-only find-references for a project-relative supported source file at a 1-based Unicode scalar position via its configured agent-side language server. Results are deduplicated and truncated on the agent; external/invalid locations are counted separately.",
            find_references_input_schema(),
        ),
        tool_spec(
            "call_hierarchy",
            "Read-only bounded call hierarchy for a project-relative supported source position. The Runner performs prepare plus incoming/outgoing breadth-first traversal to depth 1 or 2, returning only normalized project-local symbols and globally bounded edges.",
            call_hierarchy_input_schema(),
        ),
    ]
}
