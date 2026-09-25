//! JSON input schemas derived from the canonical typed [`ToolCall`] request contract.
//!
//! The cache is a projection, not a registry: tool identity remains owned by
//! `ToolDefinition`, while the accepted request shape remains owned by `ToolCall`.

use crate::schema_generation::{normalize_host_schema, openapi_schema_generator};
use crate::ToolCall;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use webcodex_core::workflow_session_contract::{
    tool_supports_model_facing_accepted_exit_codes, tool_supports_model_facing_assertion_name,
    tool_supports_model_facing_result_expectation, MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS,
    TOOL_ACCEPTED_EXIT_CODES_FIELD, TOOL_ASSERTION_NAME_FIELD, TOOL_RESULT_EXPECTATION_FIELD,
};

static TOOL_INPUT_SCHEMAS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();

/// Return the model-facing input schema derived from the canonical typed request.
/// Unknown tool names are programmer errors because every exposed ToolSpec must
/// already have a matching `ToolDefinition` and `ToolCall` variant.
pub fn input_schema_for_tool(tool_name: &str) -> Value {
    tool_input_schemas()
        .get(tool_name)
        .cloned()
        .unwrap_or_else(|| panic!("{tool_name} is missing from canonical ToolCall request schema"))
}

/// Reuse one property from the canonical derived request schema in an output
/// contract without reviving a hand-written input-schema registry.
pub(crate) fn input_property_schema_for_tool(
    tool_name: &str,
    property: &str,
    description: &str,
) -> Value {
    let mut schema = input_schema_for_tool(tool_name)
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(property))
        .cloned()
        .unwrap_or_else(|| {
            panic!("{tool_name}.{property} is missing from canonical ToolCall request schema")
        });
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    schema
}

pub fn canonical_tool_input_schema_count() -> usize {
    tool_input_schemas().len()
}

fn tool_input_schemas() -> &'static BTreeMap<String, Value> {
    TOOL_INPUT_SCHEMAS.get_or_init(derive_tool_input_schemas)
}

fn derive_tool_input_schemas() -> BTreeMap<String, Value> {
    // OpenAPI3 plus inline subschemas deliberately avoids $ref/$defs and nullable
    // unions that have historically been brittle across MCP/GPT Action hosts.
    // The small normalization below is global presentation cleanup only; it
    // never defines a field, requiredness rule, enum, or structural bound.
    let generator = openapi_schema_generator();
    let root = serde_json::to_value(generator.into_root_schema_for::<ToolCall>())
        .expect("ToolCall JsonSchema must serialize");
    let variants = root
        .get("oneOf")
        .and_then(Value::as_array)
        .expect("tagged ToolCall JsonSchema must contain oneOf variants");

    let mut schemas = BTreeMap::new();
    for variant in variants {
        let properties = variant
            .get("properties")
            .and_then(Value::as_object)
            .expect("ToolCall variant schema must contain properties");
        let tool_name = properties
            .get("tool")
            .and_then(|tool| tool.get("enum"))
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .expect("ToolCall variant schema must contain one tool enum value");

        let mut input_schema = properties
            .get("params")
            .cloned()
            .unwrap_or_else(empty_object_schema);
        normalize_host_schema(&mut input_schema);
        normalize_closed_object_unions(&mut input_schema);
        decorate_model_wrapper_schema(tool_name, &mut input_schema);
        let previous = schemas.insert(tool_name.to_string(), input_schema);
        assert!(
            previous.is_none(),
            "duplicate ToolCall schema for {tool_name}"
        );
    }
    schemas
}

fn decorate_model_wrapper_schema(tool_name: &str, schema: &mut Value) {
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    if matches!(tool_name, "run_process" | "run_detached_process") {
        let mut alias = properties.get("args").expect("process args schema").clone();
        alias["description"] = Value::String(
            "Compatibility spelling for args. Prefer args; if both are sent, their values must be identical."
                .to_string(),
        );
        properties.insert("argv".to_string(), alias);
    }
    if tool_supports_model_facing_assertion_name(tool_name) {
        properties.insert(
            TOOL_ASSERTION_NAME_FIELD.to_string(),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS,
                "description": "Optional stable human-readable validation assertion label. Reuse the same value after a fix to correlate later validation evidence even when the command changes. It never grants execution authority, changes success, or bypasses structured validation proof. It is inert unless the existing purpose/evidence rules classify the execution as validation-like."
            }),
        );
    }
    if tool_supports_model_facing_result_expectation(tool_name) {
        properties.insert(
            TOOL_RESULT_EXPECTATION_FIELD.to_string(),
            json!({
                "type": "string",
                "enum": ["success", "failure", "observe"],
                "description": "Optional pre-execution result expectation. Omit (or use success) for the normal success-required path; failure means a completed known business failure is the expected negative-test result; observe means either a completed known success or completed known business failure is acceptable. It changes only Session ledger expectation classification, never authorization, the real ToolResult, exit code, timeout, guard, transport, or outcome-unknown semantics."
            }),
        );
    }
    if tool_supports_model_facing_accepted_exit_codes(tool_name) {
        properties.insert(
            TOOL_ACCEPTED_EXIT_CODES_FIELD.to_string(),
            json!({
                "type": "array",
                "minItems": 1,
                "maxItems": 32,
                "items": {"type": "integer"},
                "description": "Optional exact set of completed process exit codes that are valid observations (for example [0,1] for boolean probe commands). A completed exit code outside the set is an expectation mismatch. This is more precise than observe and may be combined only with result_expectation=observe or with result_expectation omitted. Timeout, cancellation, transport failure, guard/permission rejection, malformed results, and outcome-unknown remain failures."
            }),
        );
    }
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": Map::<String, Value>::new(),
        "additionalProperties": false,
    })
}

fn normalize_closed_object_unions(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                for nested in properties.values_mut() {
                    normalize_closed_object_unions(nested);
                }
            }
            if let Some(items) = object.get_mut("items") {
                normalize_closed_object_unions(items);
            }
            for keyword in ["oneOf", "anyOf", "allOf"] {
                if let Some(branches) = object.get_mut(keyword).and_then(Value::as_array_mut) {
                    for branch in branches {
                        normalize_closed_object_unions(branch);
                    }
                }
            }
            if let Some(additional) = object.get_mut("additionalProperties") {
                if additional.is_object() {
                    normalize_closed_object_unions(additional);
                }
            }
            normalize_closed_object_union(object);
        }
        Value::Array(values) => {
            for nested in values {
                normalize_closed_object_unions(nested);
            }
        }
        _ => {}
    }
}

fn normalize_closed_object_union(object: &mut Map<String, Value>) {
    let Some(branches) = object.get("oneOf").and_then(Value::as_array) else {
        return;
    };
    if branches.is_empty()
        || !branches.iter().all(|branch| {
            branch.get("type").and_then(Value::as_str) == Some("object")
                && branch
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some()
        })
    {
        return;
    }

    let mut properties = Map::new();
    let mut common_required: Option<Vec<String>> = None;
    for branch in branches {
        for field in branch["properties"]
            .as_object()
            .expect("checked above")
            .keys()
        {
            properties.entry(field.clone()).or_insert_with(|| json!({}));
        }
        let required = branch
            .get("required")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        common_required = Some(match common_required.take() {
            None => required,
            Some(existing) => existing
                .into_iter()
                .filter(|field| required.contains(field))
                .collect(),
        });
    }
    object.insert("type".to_string(), Value::String("object".to_string()));
    object.insert("additionalProperties".to_string(), Value::Bool(false));
    object.insert("properties".to_string(), Value::Object(properties));
    object.insert(
        "required".to_string(),
        Value::Array(
            common_required
                .unwrap_or_default()
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_closed_schema(name: &str, schema: &Value) {
        if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
            assert!(!branches.is_empty(), "{name}");
            for branch in branches {
                assert_closed_schema(name, branch);
            }
            return;
        }
        assert_eq!(
            schema.get("type"),
            Some(&Value::String("object".to_string())),
            "{name}"
        );
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "{name}"
        );
    }

    #[test]
    fn all_canonical_tool_schemas_are_closed_objects_or_closed_unions() {
        let schemas = tool_input_schemas();
        assert!(schemas.len() > 100);
        for (name, schema) in schemas {
            assert_closed_schema(name, schema);
            let encoded = serde_json::to_string(schema).unwrap();
            assert!(!encoded.contains("$defs"), "{name}");
            assert!(!encoded.contains("$ref"), "{name}");
            assert!(!encoded.contains("nullable"), "{name}");
        }
    }

    #[test]
    fn canonical_business_schemas_exclude_adapter_wrapper_metadata() {
        for (name, schema) in tool_input_schemas() {
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{name} business schema must expose object properties"));
            for wrapper in [
                "recording_session_id",
                "ack_session_message_ids",
                "context_request",
                "session_message_resolution",
                "_control",
            ] {
                assert!(
                    !properties.contains_key(wrapper),
                    "{name} leaked adapter wrapper field {wrapper} into canonical business schema"
                );
            }
        }
    }

    #[test]
    fn business_session_schema_documents_short_ref_without_leaking_recorder_wrapper() {
        let summary = input_schema_for_tool("session_summary");
        let session = &summary["properties"]["session_id"];
        assert_eq!(session["type"], "string");
        assert!(
            session.get("pattern").is_none(),
            "model-facing business session_id must remain an unconstrained string selector; Runtime owns canonical/session_ref validation"
        );
        assert!(summary["properties"].get("recording_session_id").is_none());
    }

    #[test]
    fn shared_host_normalization_preserves_request_optional_and_closed_shape() {
        let run = input_schema_for_tool("run_process");
        let cwd = &run["properties"]["cwd"];
        assert_eq!(cwd["type"], "string");
        assert!(cwd.get("anyOf").is_none());
        assert!(cwd.get("nullable").is_none());
        assert!(!run["required"].as_array().unwrap().contains(&json!("cwd")));
        assert_eq!(run["additionalProperties"], false);

        let artifact = input_schema_for_tool("project_artifact");
        assert_eq!(artifact["additionalProperties"], false);
        assert_eq!(
            artifact["properties"]["action"]["enum"],
            json!(["metadata", "inspect", "image", "export"])
        );
    }

    #[test]
    fn representative_schemas_come_from_typed_shapes() {
        let run = input_schema_for_tool("run_process");
        assert!(run["required"]
            .as_array()
            .unwrap()
            .contains(&json!("project")));
        assert!(run["required"]
            .as_array()
            .unwrap()
            .contains(&json!("executable")));
        assert_eq!(run["additionalProperties"], false);

        let read = input_schema_for_tool("read_files");
        assert!(read["required"]
            .as_array()
            .unwrap()
            .contains(&json!("items")));
        assert_eq!(
            read["properties"]["items"]["items"]["additionalProperties"],
            false
        );

        let artifact = input_schema_for_tool("project_artifact");
        assert_eq!(
            artifact["properties"]["action"]["enum"],
            json!(["metadata", "inspect", "image", "export"])
        );
    }
}
