use super::super::input_schemas::{
    document_symbols_input_schema, find_references_input_schema, goto_definition_input_schema,
    lsp_status_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "lsp_status",
            "Read-only probe of agent-side rust-analyzer availability for a project. Does not start the language server, run cargo, or execute project code. Returns detected languages and availability/running status without absolute executable paths.",
            lsp_status_input_schema(),
        ),
        tool_spec(
            "document_symbols",
            "Read-only hierarchical document symbols for a project-relative .rs file via agent-side rust-analyzer. Returns project-relative paths, 1-based Unicode scalar columns, and bounded pre-order results. External or invalid ranges are omitted.",
            document_symbols_input_schema(),
        ),
        tool_spec(
            "goto_definition",
            "Read-only goto-definition for a project-relative .rs file at a 1-based Unicode scalar position via agent-side rust-analyzer. Supports Location, Location[], and LocationLink[]; external registry/sysroot results are omitted.",
            goto_definition_input_schema(),
        ),
        tool_spec(
            "find_references",
            "Read-only find-references for a project-relative .rs file at a 1-based Unicode scalar position via agent-side rust-analyzer. Results are deduplicated and truncated on the agent; external/invalid locations are counted separately.",
            find_references_input_schema(),
        ),
    ]
}
