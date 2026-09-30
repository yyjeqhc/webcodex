//! Bounded accessibility observations and semantic element filtering.

use super::effects::computer_error;
use super::*;
pub(super) fn node_matches_find_query(
    node: &Value,
    role: Option<&str>,
    subrole: Option<&str>,
    label: Option<&str>,
    focused: Option<bool>,
    enabled: Option<bool>,
) -> bool {
    if role.is_some_and(|expected| node.get("role").and_then(Value::as_str) != Some(expected)) {
        return false;
    }
    if subrole.is_some_and(|expected| node.get("subrole").and_then(Value::as_str) != Some(expected))
    {
        return false;
    }
    if label.is_some_and(|expected| {
        !["title", "description", "placeholder"]
            .into_iter()
            .filter_map(|field| node.get(field).and_then(Value::as_str))
            .any(|value| value.contains(expected))
    }) {
        return false;
    }
    if focused
        .is_some_and(|expected| node.get("focused").and_then(Value::as_bool) != Some(expected))
    {
        return false;
    }
    if enabled
        .is_some_and(|expected| node.get("enabled").and_then(Value::as_bool) != Some(expected))
    {
        return false;
    }
    true
}

pub(super) fn filter_accessibility_tree(
    tree: Value,
    expected_surface_id: &str,
    role: Option<&str>,
    subrole: Option<&str>,
    label: Option<&str>,
    focused: Option<bool>,
    enabled: Option<bool>,
    limit: usize,
) -> ToolResult {
    let platform = tree.get("platform").cloned().unwrap_or(Value::Null);
    let nodes = match tree.get("nodes").and_then(Value::as_array) {
        Some(nodes) => nodes,
        None => {
            return computer_error(
                "invalid_runner_response",
                "validated Accessibility tree is missing nodes",
            )
        }
    };
    let source_truncated = tree
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let observation_generation = match tree.get("observation_generation").and_then(Value::as_u64) {
        Some(value) if value > 0 && value <= u32::MAX as u64 => value,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "validated Accessibility tree is missing observation_generation",
            )
        }
    };
    let mut total_matches = 0usize;
    let mut elements = Vec::with_capacity(limit.min(nodes.len()));
    for node in nodes {
        if !node_matches_find_query(node, role, subrole, label, focused, enabled) {
            continue;
        }
        total_matches = total_matches.saturating_add(1);
        if elements.len() >= limit {
            continue;
        }
        elements.push(json!({
            "element_id": node.get("element_id").cloned().unwrap_or(Value::Null),
            "role": node.get("role").cloned().unwrap_or(Value::Null),
            "subrole": node.get("subrole").cloned().unwrap_or(Value::Null),
            "title": node.get("title").cloned().unwrap_or(Value::Null),
            "description": node.get("description").cloned().unwrap_or(Value::Null),
            "placeholder": node.get("placeholder").cloned().unwrap_or(Value::Null),
            "enabled": node.get("enabled").cloned().unwrap_or(Value::Null),
            "focused": node.get("focused").cloned().unwrap_or(Value::Null),
        }));
    }
    let count = elements.len();
    ToolResult::ok(json!({
        "platform": platform,
        "surface_id": expected_surface_id,
        "observation_generation": observation_generation,
        "elements": elements,
        "count": count,
        "scanned_nodes": nodes.len(),
        "truncated": source_truncated || total_matches > count,
    }))
}

pub(super) fn is_native_accessibility_platform(value: Option<&str>) -> bool {
    matches!(value, Some("macos" | "windows"))
}

pub(super) fn validate_accessibility_status(output: Value) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility status is not an object",
            )
        }
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "platform" | "trusted"))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("trusted").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Accessibility status is malformed",
        );
    }
    ToolResult::ok(output)
}

fn validate_accessibility_node(
    value: &Value,
    max_depth: usize,
    seen: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "accessibility node must be an object".to_string())?;
    let allowed = [
        "element_id",
        "parent_element_id",
        "depth",
        "role",
        "subrole",
        "title",
        "description",
        "value",
        "placeholder",
        "enabled",
        "focused",
        "child_count",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("accessibility node fields are inconsistent".to_string());
    }
    let element_id = value
        .get("element_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "element_id missing".to_string())?;
    if !element_id.starts_with("element_")
        || element_id.len() > MAX_ELEMENT_ID_BYTES
        || element_id.len() <= "element_".len()
        || seen.contains_key(element_id)
    {
        return Err("element_id is invalid or duplicated".to_string());
    }
    let depth = value
        .get("depth")
        .and_then(Value::as_u64)
        .and_then(|depth| usize::try_from(depth).ok())
        .ok_or_else(|| "accessibility node depth missing".to_string())?;
    if depth > max_depth {
        return Err("accessibility node depth exceeds requested bound".to_string());
    }
    match value.get("parent_element_id") {
        Some(Value::Null) if depth == 0 => {}
        Some(parent) if depth > 0 => {
            let parent = parent
                .as_str()
                .ok_or_else(|| "parent_element_id must be string or null".to_string())?;
            if parent.len() > MAX_ELEMENT_ID_BYTES || seen.get(parent).copied() != Some(depth - 1) {
                return Err("parent_element_id is stale or structurally invalid".to_string());
            }
        }
        _ => return Err("root accessibility parent/depth is inconsistent".to_string()),
    }
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| "accessibility role missing".to_string())?;
    if role.is_empty() || role.len() > MAX_TEXT_BYTES {
        return Err("accessibility role exceeds bound or is empty".to_string());
    }
    for field in ["subrole", "title", "description", "value", "placeholder"] {
        match value.get(field) {
            Some(Value::Null) => {}
            Some(text)
                if text
                    .as_str()
                    .is_some_and(|text| text.len() <= MAX_TEXT_BYTES) => {}
            _ => {
                return Err(format!(
                    "accessibility {field} is malformed or exceeds bound"
                ))
            }
        }
    }
    for field in ["enabled", "focused"] {
        if !value
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
        {
            return Err(format!("accessibility {field} must be boolean or null"));
        }
    }
    let child_count = value
        .get("child_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| "accessibility child_count missing".to_string())?;
    if child_count > MAX_ACCESSIBILITY_CHILD_COUNT {
        return Err("accessibility child_count exceeds bound".to_string());
    }
    seen.insert(element_id.to_string(), depth);
    Ok(())
}

pub(super) fn validate_accessibility_tree(
    output: Value,
    expected_surface_id: &str,
    max_depth: usize,
    max_nodes: usize,
) -> ToolResult {
    if max_depth > MAX_ACCESSIBILITY_DEPTH || !(1..=MAX_ACCESSIBILITY_NODES).contains(&max_nodes) {
        return computer_error(
            "invalid_request",
            "Accessibility validation bounds are invalid",
        );
    }
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility tree is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "nodes",
        "node_count",
        "truncated",
        "max_depth",
        "max_nodes",
        "observation_generation",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Accessibility tree fields are inconsistent",
        );
    }
    if !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("max_depth").and_then(Value::as_u64) != Some(max_depth as u64)
        || output.get("max_nodes").and_then(Value::as_u64) != Some(max_nodes as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
        || !output
            .get("observation_generation")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0 && value <= u32::MAX as u64)
    {
        return computer_error(
            "invalid_runner_response",
            "Accessibility tree metadata is inconsistent",
        );
    }
    let nodes = match output.get("nodes").and_then(Value::as_array) {
        Some(nodes) if !nodes.is_empty() && nodes.len() <= max_nodes => nodes,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility node list is missing or exceeds bound",
            )
        }
    };
    if output.get("node_count").and_then(Value::as_u64) != Some(nodes.len() as u64) {
        return computer_error(
            "invalid_runner_response",
            "Accessibility node count is inconsistent",
        );
    }
    let mut seen = HashMap::with_capacity(nodes.len());
    if let Some(error) = nodes
        .iter()
        .find_map(|node| validate_accessibility_node(node, max_depth, &mut seen).err())
    {
        return computer_error("invalid_runner_response", &error);
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_element_state(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer element state is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "element_id",
        "observation_generation",
        "enabled",
        "focused",
        "protected",
        "value_empty",
        "can_press",
        "can_focus",
        "can_input_text",
    ];
    let bool_or_null = |field: &str| {
        output
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
    };
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || !output
            .get("observation_generation")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0 && value <= u32::MAX as u64)
        || !bool_or_null("enabled")
        || !bool_or_null("focused")
        || output.get("protected").and_then(Value::as_bool).is_none()
        || !bool_or_null("value_empty")
        || output.get("can_press").and_then(Value::as_bool).is_none()
        || output.get("can_focus").and_then(Value::as_bool).is_none()
        || output
            .get("can_input_text")
            .and_then(Value::as_bool)
            .is_none()
        || (output.get("protected").and_then(Value::as_bool) == Some(true)
            && (output.get("value_empty") != Some(&Value::Null)
                || output.get("can_press").and_then(Value::as_bool) != Some(false)
                || output.get("can_focus").and_then(Value::as_bool) != Some(false)
                || output.get("can_input_text").and_then(Value::as_bool) != Some(false)))
        || (output.get("enabled").and_then(Value::as_bool) == Some(false)
            && (output.get("can_press").and_then(Value::as_bool) != Some(false)
                || output.get("can_focus").and_then(Value::as_bool) != Some(false)
                || output.get("can_input_text").and_then(Value::as_bool) != Some(false)))
        || (output.get("can_input_text").and_then(Value::as_bool) == Some(true)
            && (output.get("focused").and_then(Value::as_bool) != Some(true)
                || output.get("value_empty").and_then(Value::as_bool) != Some(true)))
    {
        return computer_error(
            "invalid_runner_response",
            "computer element state metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}
