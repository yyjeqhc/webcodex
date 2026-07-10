use serde_json::Value;

use super::common::{array_schema, open_object_schema, schema_type, wrapped_output_schema};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "lsp_status" => {
            Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved runtime project id.")),
            (
                "detected_languages",
                array_schema(
                    schema_type("string", "Detected language id."),
                    "Languages detected for the project (for example rust when Cargo.toml exists).",
                ),
            ),
            (
                "servers",
                array_schema(
                    open_object_schema(
                        "Language server status entry without absolute executable paths.",
                    ),
                    "Per-language server availability and running state.",
                ),
            ),
            (
                "warnings",
                array_schema(
                    schema_type("string", "Bounded non-fatal warning."),
                    "Optional warnings.",
                ),
            ),
        ]))
        }
        "document_symbols" => Some(wrapped_output_schema(vec![
            (
                "project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "path",
                schema_type("string", "Project-relative source path."),
            ),
            ("language", schema_type("string", "Language id (rust).")),
            (
                "symbols",
                array_schema(
                    open_object_schema("Document symbol node with name/kind/range/children."),
                    "Bounded hierarchical symbol tree.",
                ),
            ),
            (
                "total_count",
                schema_type(
                    "integer",
                    "In-project valid symbol node count before truncation.",
                ),
            ),
            (
                "returned_count",
                schema_type("integer", "Symbol nodes actually returned."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether the symbol budget truncated results."),
            ),
            (
                "external_results_omitted",
                schema_type("integer", "External symbol results omitted."),
            ),
            (
                "invalid_results_omitted",
                schema_type("integer", "Invalid symbol results omitted."),
            ),
        ])),
        "goto_definition" | "find_references" => Some(wrapped_output_schema(vec![
            (
                "project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "path",
                schema_type("string", "Project-relative query path."),
            ),
            (
                "query_position",
                open_object_schema("1-based Unicode scalar query position."),
            ),
            (
                "locations",
                array_schema(
                    open_object_schema("Project-relative location with range."),
                    "Bounded project-relative locations.",
                ),
            ),
            (
                "total_results",
                schema_type("integer", "Raw server result count before filtering."),
            ),
            (
                "returned_count",
                schema_type("integer", "Locations returned after dedup/truncation."),
            ),
            (
                "truncated",
                schema_type(
                    "boolean",
                    "Whether in-project valid results exceeded the limit.",
                ),
            ),
            (
                "external_results_omitted",
                schema_type("integer", "External locations omitted."),
            ),
            (
                "invalid_results_omitted",
                schema_type("integer", "Invalid locations omitted."),
            ),
        ])),
        _ => None,
    }
}
