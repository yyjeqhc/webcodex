//! Deterministic samples for the bounded input-schema vocabulary, never execution fixtures.
use super::validate_schema_instance;
use crate::{registered_tool_specs, ToolSpec};
use serde_json::{json, Map, Value};

/// Generate a small sample or reject unsupported/unsatisfiable schema constraints.
/// Limits bound both recursion and total generated nodes; no references are followed.
pub fn sample_schema_value(schema: &Value) -> Result<Value, String> {
    generate(schema, 0, &mut 256, false)
}

fn generate(
    schema: &Value,
    depth: usize,
    budget: &mut usize,
    fixtures: bool,
) -> Result<Value, String> {
    if depth > 16 || *budget == 0 {
        return Err("sample recursion/node budget exceeded".into());
    }
    *budget -= 1;
    let object = schema
        .as_object()
        .ok_or("sample requires an object schema")?;
    check_keywords(object, false)?;
    let valid = |value: &Value| validate_candidate(value, schema, fixtures).is_ok();
    for key in ["const", "default"] {
        if let Some(value) = schema.get(key) {
            let mut candidate_budget = *budget;
            if charge_value(value, depth, &mut candidate_budget).is_ok() && valid(value) {
                *budget = candidate_budget;
                return Ok(value.clone());
            }
            if key == "const" {
                return Err("invalid const sample".into());
            }
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        for value in values.iter().take(256) {
            let mut candidate_budget = *budget;
            if charge_value(value, depth, &mut candidate_budget).is_ok() && valid(value) {
                *budget = candidate_budget;
                return Ok(value.clone());
            }
        }
        return Err("no valid bounded enum sample".into());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(variants) = schema.get(key).and_then(Value::as_array) {
            for variant in variants.iter().take(256) {
                let mut candidate_budget = *budget;
                if let Ok(value) = generate(variant, depth + 1, &mut candidate_budget, fixtures) {
                    if valid(&value) {
                        *budget = candidate_budget;
                        return Ok(value);
                    }
                }
            }
            return Err(format!("no valid bounded {key} sample"));
        }
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        for kind in types.iter().take(8) {
            let mut variant = schema.clone();
            variant["type"] = kind.clone();
            let mut candidate_budget = *budget;
            if let Ok(value) = generate(&variant, depth + 1, &mut candidate_budget, fixtures) {
                if valid(&value) {
                    *budget = candidate_budget;
                    return Ok(value);
                }
            }
        }
        return Err("no supported sample type".into());
    }
    let value = match schema.get("type").and_then(Value::as_str) {
        Some("string") => {
            let length = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0);
            if length > 1024 {
                return Err("sample string budget exceeded".into());
            }
            json!("a".repeat(length as usize))
        }
        Some("integer") | Some("number") => {
            let mut number = schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0);
            if schema["type"] == "integer" {
                number = number.ceil();
                if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
                    number = number.min(maximum.floor());
                }
                if number.abs() > 9_007_199_254_740_991.0 {
                    return Err("integer sample outside exact numeric range".into());
                }
                json!(number as i64)
            } else {
                if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
                    number = number.min(maximum);
                }
                json!(number)
            }
        }
        Some("boolean") => json!(false),
        Some("null") => Value::Null,
        Some("object") => {
            let mut args = Map::new();
            if let Some(required) = schema.get("required") {
                for field in required.as_array().ok_or("required must be an array")? {
                    let field = field.as_str().ok_or("required name must be a string")?;
                    if field.len() > 1024 {
                        return Err("sample key budget exceeded".into());
                    }
                    let child = schema
                        .get("properties")
                        .and_then(|p| p.get(field))
                        .ok_or_else(|| format!("missing schema for {field}"))?;
                    args.insert(
                        field.to_string(),
                        generate_field(field, child, depth + 1, budget, fixtures)
                            .map_err(|error| format!("{field}: {error}"))?,
                    );
                }
            }
            Value::Object(args)
        }
        Some("array") => {
            let length = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            if length > 32 {
                return Err("sample array budget exceeded".into());
            }
            let mut items = Vec::new();
            for _ in 0..length {
                items.push(generate(
                    schema.get("items").ok_or("missing items schema")?,
                    depth + 1,
                    budget,
                    fixtures,
                )?);
            }
            Value::Array(items)
        }
        _ => return Err("unsupported or absent sample type".into()),
    };
    validate_candidate(&value, schema, fixtures)?;
    Ok(value)
}

fn charge_value(value: &Value, depth: usize, budget: &mut usize) -> Result<(), String> {
    if depth > 16 || *budget == 0 {
        return Err("sample literal budget exceeded".into());
    }
    *budget -= 1;
    match value {
        Value::String(s) if s.len() > 1024 => return Err("sample string budget exceeded".into()),
        Value::Array(values) => {
            if values.len() > 32 {
                return Err("sample array budget exceeded".into());
            }
            for value in values {
                charge_value(value, depth + 1, budget)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > 1024 {
                    return Err("sample key budget exceeded".into());
                }
                charge_value(value, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn sample_tool_args(name: &str) -> Value {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("missing tool spec for {name}"));
    sample_tool_args_for_spec(&spec)
}

pub fn sample_tool_args_for_spec(spec: &ToolSpec) -> Value {
    let mut value = generate(&spec.input_schema, 0, &mut 256, true)
        .unwrap_or_else(|error| panic!("{}: {error}", spec.name));
    let args = value.as_object_mut().expect("tool object sample");
    // Conditional Project source: use the Project form for accessor tests.
    if spec.name == "work_on_project" {
        args.insert("project".into(), json!("agent:oe:private-drop"));
    }
    // SSH action is a free-form String with custom cross-field parser validation,
    // not a schema enum. List requires an exact Runner selector.
    if spec.name == "ssh_resource" {
        args.insert("action".into(), json!("list"));
        args.insert("runner".into(), json!("runner-a"));
    }
    value
}

fn generate_field(
    field: &str,
    schema: &Value,
    depth: usize,
    budget: &mut usize,
    fixtures: bool,
) -> Result<Value, String> {
    // Opaque identity formats and nonempty runtime selectors are semantic fixtures,
    // not alternate enum/type tables. Schema-declared values always take precedence.
    if fixtures
        && !["const", "default", "enum"]
            .iter()
            .any(|key| schema.get(key).is_some())
    {
        let value = match field {
            "project" | "source_project" | "destination_project" => {
                Some(json!("agent:oe:private-drop"))
            }
            "path" => Some(json!("src/lib.rs")),
            "handle" => Some(json!("reviewer")),
            "session_id" => Some(json!(format!("wc_sess_{}", "1".repeat(32)))),
            "message_id" => Some(json!(format!("wc_msg_{}", "1".repeat(32)))),
            "peer_id" => Some(json!(format!("wc_peer_{}", "a".repeat(32)))),
            "expected_assignment_fence" => Some(json!(format!("wsa2_{}", "A".repeat(22)))),
            "agent_id" | "assignee_agent_id" => Some(json!("wc_dagent_qqqqqqqqqqqqqqqq")),
            "agent_ids" => Some(json!(["wc_dagent_qqqqqqqqqqqqqqqq"])),
            "task_id" => Some(json!("wc_agent_task_ERERERERERERERER")),
            "wait_id" => Some(json!("wc_agent_wait_ZmZmZmZmZmZmZmZm")),
            "goal_id" => Some(json!("wc_goal_AAAAAAAAAAAAAAAA")),
            "attempt_id" => Some(json!("wc_agent_task_attempt_IiIiIiIiIiIiIiIi")),
            "attempt_fence" => Some(json!("wc_agent_task_fence_MzMzMzMzMzMzMzMzMzMzMw")),
            "endpoint_id" => Some(json!("wc_endpoint_u7u7u7u7u7u7u7u7")),
            "conversation_id" => Some(json!("wc_conv_zMzMzMzMzMzMzMzM")),
            "delivery_ids" => Some(json!(["wc_delivery_3d3d3d3d3d3d3d3d"])),
            "wake_id" => Some(json!("wc_wake_7u7u7u7u7u7u7u7u")),
            "consume_token" => Some(json!("wc_wake_consume______________________w")),
            "run_id" => Some(json!("wc_agent_run_sample_1234")),
            "skill_id" => Some(json!("wc_skill_EREREREREREREREREREREQ")),
            "expected_definition_revision" => Some(json!("a".repeat(64))),
            "base_commit" | "head_commit" | "expected_head" => Some(json!("a".repeat(40))),
            _ => None,
        };
        if let Some(value) = value {
            validate_schema_instance(&value, schema)?;
            charge_value(&value, depth, budget)?;
            return Ok(value);
        }
    }
    generate(schema, depth, budget, fixtures)
}

// Validate the actually sampled paths before using the shared subset validator.
// In particular, const/default containers must not bypass unsupported child constraints.
fn validate_candidate(value: &Value, schema: &Value, fixtures: bool) -> Result<(), String> {
    fn visit(
        value: &Value,
        schema: &Value,
        fixtures: bool,
        depth: usize,
        budget: &mut usize,
    ) -> Result<(), String> {
        if depth > 16 || *budget == 0 {
            return Err("sample validation budget exceeded".into());
        }
        *budget -= 1;
        let object = schema.as_object().ok_or("unsupported boolean schema")?;
        check_keywords(object, fixtures)?;
        for key in ["oneOf", "anyOf"] {
            if let Some(variants) = schema.get(key).and_then(Value::as_array) {
                for variant in variants {
                    visit(value, variant, fixtures, depth + 1, budget)?;
                }
            }
        }
        if let Some(values) = value.as_object() {
            for (key, child) in values {
                if let Some(child_schema) = schema.get("properties").and_then(|p| p.get(key)) {
                    visit(child, child_schema, fixtures, depth + 1, budget)?;
                }
            }
        }
        if let (Some(values), Some(items)) = (value.as_array(), schema.get("items")) {
            for child in values {
                visit(child, items, fixtures, depth + 1, budget)?;
            }
        }
        Ok(())
    }
    visit(value, schema, fixtures, 0, &mut 256)?;
    validate_schema_instance(value, schema)
}

fn check_keywords(object: &Map<String, Value>, fixtures: bool) -> Result<(), String> {
    for (key, value) in object {
        match key.as_str() {
            "type" | "const" | "default" | "properties" | "items" | "description" | "title" => {}
            "minimum" | "maximum" if value.is_number() => {}
            "minLength" | "maxLength" | "minItems" | "maxItems" if value.as_u64().is_some() => {}
            "additionalProperties" if value.is_boolean() => {}
            "required" | "enum" | "anyOf" | "oneOf"
                if value.as_array().is_some_and(|a| a.len() <= 256) => {}
            // Opaque formats are allowed only in the explicit semantic-fixture path.
            "pattern" if fixtures => {}
            _ => return Err(format!("unsupported sample keyword or bound {key}")),
        }
    }
    Ok(())
}
