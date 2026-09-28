//! Test-only support for validating instances against the bounded ToolSpec schema subset.
//!
//! This deliberately implements only the schema vocabulary authored by WebCodex contracts. It is
//! not a general JSON Schema engine and is compiled only for crate tests or the root test-support
//! feature.

use serde_json::Value;

pub fn validate_schema_instance(instance: &Value, schema: &Value) -> Result<(), String> {
    validate_schema_instance_at(instance, schema, "$")
}

pub fn validate_generated_tool_call_against_registered_input_schema(
    call: &Value,
) -> Result<(), String> {
    let object = call
        .as_object()
        .ok_or_else(|| "$: generated follow-up must be an object".to_string())?;
    let follow_up_kind = object
        .get("follow_up_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "$.follow_up_kind: missing generated follow-up posture".to_string())?;
    if !webcodex_core::runtime_contract::GENERATED_FOLLOW_UP_KIND_VALUES.contains(&follow_up_kind) {
        return Err(format!(
            "$.follow_up_kind: unsupported generated follow-up posture {follow_up_kind}"
        ));
    }
    let tool = object
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| "$.tool: missing generated follow-up target".to_string())?;
    let arguments = object
        .get("arguments")
        .ok_or_else(|| "$.arguments: missing generated follow-up arguments".to_string())?;
    let schema = crate::input_schema_for_tool(tool);
    validate_schema_instance(arguments, &schema)
        .map_err(|error| format!("{tool} generated arguments fail registered inputSchema: {error}"))
}

pub fn validate_generated_tool_calls_in_value(value: &Value) -> Result<usize, String> {
    fn visit(value: &Value, path: &str, count: &mut usize) -> Result<(), String> {
        if let Some(object) = value.as_object() {
            let looks_like_generated_call = object.contains_key("follow_up_kind")
                && object.contains_key("tool")
                && object.contains_key("arguments");
            if looks_like_generated_call {
                validate_generated_tool_call_against_registered_input_schema(value)
                    .map_err(|error| format!("{path}: {error}"))?;
                *count += 1;
            }
            for (name, child) in object {
                visit(child, &format!("{path}.{}", name), count)?;
            }
        } else if let Some(array) = value.as_array() {
            for (index, child) in array.iter().enumerate() {
                visit(child, &format!("{path}[{index}]"), count)?;
            }
        }
        Ok(())
    }

    let mut count = 0;
    visit(value, "$", &mut count)?;
    Ok(count)
}

fn validate_schema_instance_at(instance: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(schemas) = schema.get("allOf").and_then(Value::as_array) {
        for child in schemas {
            validate_schema_instance_at(instance, child, path)?;
        }
    }
    if let Some(condition) = schema.get("if") {
        let branch = if validate_schema_instance_at(instance, condition, path).is_ok() {
            schema.get("then")
        } else {
            schema.get("else")
        };
        if let Some(branch) = branch {
            validate_schema_instance_at(instance, branch, path)?;
        }
    }
    if let Some(negated) = schema.get("not") {
        if validate_schema_instance_at(instance, negated, path).is_ok() {
            return Err(format!("{path}: negated schema matched"));
        }
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let results = variants
            .iter()
            .map(|variant| validate_schema_instance_at(instance, variant, path))
            .collect::<Vec<_>>();
        let successes = results.iter().filter(|result| result.is_ok()).count();
        (successes == 1).then_some(()).ok_or_else(|| {
            let errors = results
                .into_iter()
                .enumerate()
                .filter_map(|(index, result)| {
                    result
                        .err()
                        .map(|error| format!("variant {index}: {error}"))
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!("{path}: expected exactly one matching schema, got {successes}; {errors}")
        })?;
    }
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        variants
            .iter()
            .find_map(|variant| {
                validate_schema_instance_at(instance, variant, path)
                    .ok()
                    .map(|_| ())
            })
            .ok_or_else(|| format!("{path}: no anyOf variant matched"))?;
    }
    if let Some(expected) = schema.get("const") {
        if instance != expected {
            return Err(format!("{path}: const mismatch"));
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.iter().any(|value| value == instance) {
            return Err(format!("{path}: value is outside the declared enum"));
        }
    }
    if let Some(expected_type) = schema.get("type") {
        let matches_type = |kind: &str| match kind {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "string" => instance.is_string(),
            "boolean" => instance.is_boolean(),
            "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
            "number" => instance.is_number(),
            "null" => instance.is_null(),
            _ => false,
        };
        let matches = match expected_type {
            Value::String(kind) => matches_type(kind),
            Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).any(matches_type),
            _ => false,
        };
        if !matches {
            return Err(format!("{path}: expected {expected_type}"));
        }
    }
    if let Some(object) = instance.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Err(format!("{path}: missing required field {field}"));
                }
            }
        }
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            let properties = properties
                .ok_or_else(|| format!("{path}: strict object schema is missing properties"))?;
            for field in object.keys() {
                if !properties.contains_key(field) {
                    return Err(format!("{path}: unknown field {field}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (field, value) in object {
                if let Some(child_schema) = properties.get(field) {
                    validate_schema_instance_at(value, child_schema, &format!("{path}.{field}"))?;
                }
            }
        }
    }
    if let Some(array) = instance.as_array() {
        if let Some(min_items) = schema.get("minItems").and_then(Value::as_u64) {
            if array.len() < min_items as usize {
                return Err(format!("{path}: below minItems"));
            }
        }
        if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64) {
            if array.len() > max_items as usize {
                return Err(format!("{path}: maxItems exceeded"));
            }
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            for (index, item) in array.iter().enumerate() {
                if array[..index].iter().any(|earlier| earlier == item) {
                    return Err(format!("{path}: duplicate array item"));
                }
            }
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_schema_instance_at(item, item_schema, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(value) = instance.as_str() {
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| value.chars().count() < minimum as usize)
        {
            return Err(format!("{path}: below minLength"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|maximum| value.chars().count() > maximum as usize)
        {
            return Err(format!("{path}: maxLength exceeded"));
        }
    }
    if instance.is_number() {
        // Preserve exact integer comparisons for revision/fence-sized u64 values.
        let compare = |bound: &Value| {
            if let (Some(a), Some(b)) = (instance.as_i64(), bound.as_i64()) {
                Some(a.cmp(&b))
            } else if let (Some(a), Some(b)) = (instance.as_u64(), bound.as_u64()) {
                Some(a.cmp(&b))
            } else {
                instance.as_f64()?.partial_cmp(&bound.as_f64()?)
            }
        };
        if schema.get("minimum").and_then(compare) == Some(std::cmp::Ordering::Less) {
            return Err(format!("{path}: below minimum"));
        }
        if schema.get("maximum").and_then(compare) == Some(std::cmp::Ordering::Greater) {
            return Err(format!("{path}: above maximum"));
        }
    }
    if let (Some(value), Some(pattern)) = (
        instance.as_str(),
        schema.get("pattern").and_then(Value::as_str),
    ) {
        let matches = match pattern {
            "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$" => value
                .strip_prefix("wc_host_binding_")
                .and_then(webcodex_core::compact::decode::<16>)
                .is_some(),
            "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$" => {
                webcodex_core::workflow_session_contract::is_valid_session_id(value)
            }
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$" => {
                webcodex_core::workflow_session_contract::is_valid_session_message_id(value)
            }
            "^[0-9a-f]{64}$" => {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }
            "^repository:v1:[0-9a-f]{64}$" => {
                value.strip_prefix("repository:v1:").is_some_and(|digest| {
                    digest.len() == 64
                        && digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
            }
            _ => true,
        };
        if !matches {
            return Err(format!("{path}: pattern mismatch"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_not_inverts_child_validation() {
        let schema = json!({"not": {"required": ["forbidden"]}});
        validate_schema_instance(&json!({}), &schema).unwrap();
        assert!(validate_schema_instance(&json!({"forbidden": true}), &schema).is_err());
    }

    #[test]
    fn schema_nested_then_not_rejects_the_forbidden_sibling_shape() {
        let schema = json!({
            "if": {"required": ["suggested_call"]},
            "then": {"not": {"required": ["reconcile_with"]}}
        });
        validate_schema_instance(&json!({"suggested_call": {}}), &schema).unwrap();
        assert!(validate_schema_instance(
            &json!({"suggested_call": {}, "reconcile_with": "skill_versions"}),
            &schema,
        )
        .is_err());
    }

    #[test]
    fn schema_all_of_and_not_validate_as_sibling_constraints() {
        let schema = json!({
            "allOf": [
                {"required": ["present"]},
                {"properties": {"present": {"const": true}}}
            ],
            "not": {"required": ["forbidden"]}
        });
        validate_schema_instance(&json!({"present": true}), &schema).unwrap();
        assert!(
            validate_schema_instance(&json!({"present": true, "forbidden": true}), &schema,)
                .is_err()
        );
    }

    #[test]
    fn schema_not_is_not_bypassed_by_one_of_or_any_of_early_returns() {
        for keyword in ["oneOf", "anyOf"] {
            let mut schema = json!({"not": {"required": ["forbidden"]}});
            schema[keyword] = json!([
                {"required": ["allowed"]},
                {"required": ["alternate"]}
            ]);
            validate_schema_instance(&json!({"allowed": true}), &schema).unwrap();
            assert!(
                validate_schema_instance(&json!({"allowed": true, "forbidden": true}), &schema,)
                    .is_err(),
                "{keyword} returned before sibling not was enforced"
            );
        }
    }
}

mod samples;
pub use samples::{sample_schema_value, sample_tool_args, sample_tool_args_for_spec};
