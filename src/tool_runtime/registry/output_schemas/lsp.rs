use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

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
        "document_diagnostics" => Some(wrapped_output_schema(vec![
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
                "diagnostics",
                array_schema(
                    diagnostic_schema(),
                    "Bounded, sorted, deduplicated diagnostics without raw data or related-information locations.",
                ),
            ),
            (
                "total_count",
                schema_type("integer", "Raw server diagnostic count before filtering."),
            ),
            (
                "returned_count",
                schema_type("integer", "Normalized diagnostics actually returned."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether cache or caller limits truncated diagnostics."),
            ),
            (
                "fresh",
                schema_type("boolean", "Whether a current-version or post-prepare publication was observed."),
            ),
            (
                "timed_out",
                schema_type("boolean", "Whether no fresh publication arrived within the shared two-second wait budget."),
            ),
            (
                "published_version",
                nullable_schema("integer", "Optional LSP document version from the publication."),
            ),
            (
                "invalid_results_omitted",
                schema_type("integer", "Malformed diagnostics or invalid ranges omitted."),
            ),
            (
                "related_information_omitted",
                schema_type("integer", "Related-information entries intentionally not expanded."),
            ),
        ])),
        "hover" => Some(wrapped_output_schema(vec![
            (
                "project",
                schema_type("string", "Resolved runtime project id."),
            ),
            (
                "path",
                schema_type("string", "Project-relative source path."),
            ),
            ("position", position_schema()),
            ("hover", hover_schema()),
            (
                "truncated",
                schema_type("boolean", "Whether normalized hover text exceeded 16 KiB."),
            ),
            (
                "range_omitted",
                schema_type("boolean", "Whether a malformed optional hover range was omitted."),
            ),
        ])),
        "workspace_symbols" => Some(wrapped_output_schema(vec![
            (
                "project",
                schema_type("string", "Resolved runtime project id."),
            ),
            ("query", schema_type("string", "Trimmed symbol query.")),
            (
                "symbols",
                array_schema(
                    workspace_symbol_schema(),
                    "Bounded, sorted, deduplicated workspace-only symbols.",
                ),
            ),
            (
                "total_results",
                schema_type("integer", "Raw server result count before filtering."),
            ),
            (
                "returned_count",
                schema_type("integer", "Workspace symbols returned after deduplication and truncation."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether valid workspace symbols exceeded the caller limit."),
            ),
            (
                "external_results_omitted",
                schema_type("integer", "External/dependency symbol locations omitted."),
            ),
            (
                "invalid_results_omitted",
                schema_type("integer", "Malformed symbol results omitted."),
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

fn diagnostic_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["range", "severity", "severity_code", "code", "source", "message", "tags"],
        "properties": {
            "range": {
                "type": "object",
                "additionalProperties": false,
                "required": ["start", "end"],
                "properties": {
                    "start": position_schema(),
                    "end": position_schema()
                }
            },
            "severity": {
                "type": "string",
                "enum": ["error", "warning", "information", "hint", "unknown"]
            },
            "severity_code": nullable_schema("integer", "Original numeric LSP severity, when present."),
            "code": nullable_schema("string", "Bounded string-normalized diagnostic code."),
            "source": nullable_schema("string", "Bounded diagnostic source."),
            "message": {"type": "string", "maxLength": 4096},
            "tags": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": 3,
                "items": {"type": "string", "enum": ["unnecessary", "deprecated", "unknown"]}
            }
        }
    })
}

fn hover_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "value", "range"],
                "properties": {
                    "kind": {"type": "string", "enum": ["markdown", "plaintext"]},
                    "value": {"type": "string", "maxLength": 16384},
                    "range": {
                        "anyOf": [range_schema(), {"type": "null"}]
                    }
                }
            },
            {"type": "null"}
        ]
    })
}

fn workspace_symbol_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "kind", "kind_code", "container_name", "path", "range"],
        "properties": {
            "name": {"type": "string", "maxLength": 256},
            "kind": {"type": "string"},
            "kind_code": {"type": "integer"},
            "container_name": nullable_schema("string", "Bounded container name, when provided."),
            "path": {"type": "string", "description": "Project-relative source path."},
            "range": {
                "anyOf": [range_schema(), {"type": "null"}]
            }
        }
    })
}

fn range_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["start", "end"],
        "properties": {
            "start": position_schema(),
            "end": position_schema()
        }
    })
}

fn position_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["line", "column"],
        "properties": {
            "line": {"type": "integer", "minimum": 1},
            "column": {"type": "integer", "minimum": 1}
        }
    })
}
