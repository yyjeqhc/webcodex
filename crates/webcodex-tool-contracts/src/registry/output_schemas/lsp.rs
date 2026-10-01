use serde_json::Value;
use webcodex_core::lsp_bridge::{
    CallHierarchyResult, DocumentDiagnosticsResult, DocumentSymbolsResult, HoverResult,
    LocationsResult, LspStatusResult, WorkspaceSymbolsResult,
};

use super::common::{array_schema, open_object_schema, wrapped_typed_output_schema};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "get_lsp_status" => Some(wrapped_typed_output_schema::<LspStatusResult>(vec![(
            "servers",
            array_schema(
                open_object_schema(
                    "Language server status entry without absolute executable paths.",
                ),
                "Per-language server availability and running state.",
            ),
        )])),
        "list_document_symbols" => {
            Some(wrapped_typed_output_schema::<DocumentSymbolsResult>(vec![
                (
                    "symbols",
                    array_schema(
                        open_object_schema("Document symbol node with name/kind/range/children."),
                        "Bounded hierarchical symbol tree.",
                    ),
                ),
            ]))
        }
        "read_document_diagnostics" => Some(
            wrapped_typed_output_schema::<DocumentDiagnosticsResult>(Vec::new()),
        ),
        "read_symbol_hover" => Some(wrapped_typed_output_schema::<HoverResult>(Vec::new())),
        "list_workspace_symbols" => Some(wrapped_typed_output_schema::<WorkspaceSymbolsResult>(
            Vec::new(),
        )),
        "find_definition" | "find_references" => {
            Some(wrapped_typed_output_schema::<LocationsResult>(vec![(
                "locations",
                array_schema(
                    open_object_schema("Project-relative location with range."),
                    "Bounded project-relative locations.",
                ),
            )]))
        }
        "read_call_hierarchy" => Some(wrapped_typed_output_schema::<CallHierarchyResult>(vec![
            (
                "roots",
                array_schema(
                    open_object_schema("Normalized project-local call-hierarchy root symbol."),
                    "Bounded normalized prepareCallHierarchy roots.",
                ),
            ),
            (
                "edges",
                array_schema(
                    open_object_schema("Normalized project-local call edge."),
                    "Breadth-first flattened call edges.",
                ),
            ),
        ])),
        _ => None,
    }
}
