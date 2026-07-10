use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn lsp_status_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![(
        "project",
        "string",
        "Full agent runtime project id (agent:<client_id>:<project_id>).",
        true,
    )]))
}

pub(crate) fn document_symbols_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        (
            "project",
            "string",
            "Full agent runtime project id (agent:<client_id>:<project_id>).",
            true,
        ),
        (
            "path",
            "string",
            "Project-relative UTF-8 path to a .rs file.",
            true,
        ),
        (
            "limit",
            "integer",
            "Maximum symbol nodes to return (default 100, clamped to 1..500).",
            false,
        ),
    ]));
    schema["properties"]["limit"]["minimum"] = json!(1);
    schema["properties"]["limit"]["maximum"] = json!(500);
    schema["properties"]["limit"]["default"] = json!(100);
    schema
}

pub(crate) fn goto_definition_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        (
            "project",
            "string",
            "Full agent runtime project id (agent:<client_id>:<project_id>).",
            true,
        ),
        (
            "path",
            "string",
            "Project-relative UTF-8 path to a .rs file.",
            true,
        ),
        ("line", "integer", "1-based line number.", true),
        (
            "column",
            "integer",
            "1-based Unicode scalar column (end-of-line caret allowed at length+1).",
            true,
        ),
        (
            "limit",
            "integer",
            "Maximum locations to return (default 20, clamped to 1..100).",
            false,
        ),
    ]));
    schema["properties"]["line"]["minimum"] = json!(1);
    schema["properties"]["column"]["minimum"] = json!(1);
    schema["properties"]["limit"]["minimum"] = json!(1);
    schema["properties"]["limit"]["maximum"] = json!(100);
    schema["properties"]["limit"]["default"] = json!(20);
    schema
}

pub(crate) fn find_references_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        (
            "project",
            "string",
            "Full agent runtime project id (agent:<client_id>:<project_id>).",
            true,
        ),
        (
            "path",
            "string",
            "Project-relative UTF-8 path to a .rs file.",
            true,
        ),
        ("line", "integer", "1-based line number.", true),
        (
            "column",
            "integer",
            "1-based Unicode scalar column (end-of-line caret allowed at length+1).",
            true,
        ),
        (
            "include_declaration",
            "boolean",
            "Include the declaration in results (default true).",
            false,
        ),
        (
            "limit",
            "integer",
            "Maximum locations to return (default 50, clamped to 1..200).",
            false,
        ),
    ]));
    schema["properties"]["line"]["minimum"] = json!(1);
    schema["properties"]["column"]["minimum"] = json!(1);
    schema["properties"]["include_declaration"]["default"] = json!(true);
    schema["properties"]["limit"]["minimum"] = json!(1);
    schema["properties"]["limit"]["maximum"] = json!(200);
    schema["properties"]["limit"]["default"] = json!(50);
    schema
}
