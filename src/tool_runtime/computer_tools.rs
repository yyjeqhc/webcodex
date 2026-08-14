use super::{ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::auth::AuthContext;
use crate::shell_protocol::{
    SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL, SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const MAX_WINDOWS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_ACCESSIBILITY_DEPTH: usize = 8;
const MAX_ACCESSIBILITY_NODES: usize = 256;
const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
const MAX_ACCESSIBILITY_CHILD_COUNT: u64 = 1_000_000;
const MAX_IMAGE_DIMENSION: u64 = 4096;
const COMPUTER_WAIT_SECS: u64 = 30;

impl ToolRuntime {
    pub(super) async fn dispatch_computer_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ComputerListWindows { client_id, limit } => {
                let limit = limit.unwrap_or(MAX_WINDOWS).clamp(1, MAX_WINDOWS);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_list_windows",
                    json!({"limit": limit}),
                    auth,
                    Some(limit),
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerAccessibilityStatus { client_id } => {
                self.dispatch_computer_request(
                    &client_id,
                    "computer_accessibility_status",
                    json!({}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerAccessibilityTree {
                client_id,
                surface_id,
                max_depth,
                max_nodes,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                let max_depth = max_depth
                    .unwrap_or(DEFAULT_ACCESSIBILITY_DEPTH)
                    .min(MAX_ACCESSIBILITY_DEPTH);
                let max_nodes = max_nodes
                    .unwrap_or(DEFAULT_ACCESSIBILITY_NODES)
                    .clamp(1, MAX_ACCESSIBILITY_NODES);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_accessibility_tree",
                    json!({
                        "surface_id": surface_id,
                        "max_depth": max_depth,
                        "max_nodes": max_nodes,
                    }),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    Some((max_depth, max_nodes)),
                )
                .await
            }
            ToolCall::ComputerControl {
                client_id,
                surface_id,
                element_id,
                action,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                if !element_id.starts_with("element_")
                    || element_id.len() <= "element_".len()
                    || element_id.len() > MAX_ELEMENT_ID_BYTES
                {
                    return computer_error("invalid_element", "element_id is invalid");
                }
                if !matches!(action.as_str(), "press" | "focus") {
                    return computer_error("invalid_request", "computer control action is invalid");
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_control",
                    json!({
                        "surface_id": surface_id,
                        "element_id": element_id,
                        "action": action,
                    }),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            ToolCall::ComputerSnapshot {
                client_id,
                surface_id,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_snapshot",
                    json!({"surface_id": surface_id}),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            _ => ToolResult::err("invalid computer tool dispatch".to_string()),
        }
    }

    async fn dispatch_computer_request(
        &self,
        client_id: &str,
        kind: &'static str,
        payload: Value,
        auth: Option<&AuthContext>,
        list_limit: Option<usize>,
        expected_surface_id: Option<&str>,
        accessibility_bounds: Option<(usize, usize)>,
    ) -> ToolResult {
        if client_id.is_empty() || client_id.len() > 128 {
            return computer_error("invalid_client", "client_id is invalid");
        }
        let required_capability = match kind {
            "computer_list_windows" | "computer_snapshot" => {
                SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE
            }
            "computer_accessibility_status" | "computer_accessibility_tree" => {
                crate::shell_protocol::SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE
            }
            "computer_control" => SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL,
            _ => return computer_error("invalid_request", "unsupported computer request kind"),
        };
        match self
            .shell_clients
            .client_supports_for_auth(client_id, required_capability, auth)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                return computer_error(
                    "capability_unavailable",
                    &format!("target Runner does not support {required_capability}"),
                )
            }
            Err(error) => return computer_error("client_access_denied", &error),
        }
        let expected_element_id = payload
            .get("element_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_action = payload
            .get("action")
            .and_then(Value::as_str)
            .map(str::to_string);
        let payload = match serde_json::to_string(&payload) {
            Ok(payload) => payload,
            Err(_) => {
                return computer_error("invalid_request", "could not encode computer request")
            }
        };
        let requested_by = crate::shell_client::requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .shell_clients
            .enqueue_computer(
                client_id.to_string(),
                kind,
                payload,
                requested_by,
                auth,
                COMPUTER_WAIT_SECS,
            )
            .await
        {
            Ok(value) => value,
            Err(error) => return computer_error("dispatch_denied", &error),
        };
        let is_control = kind == "computer_control";
        let response = match tokio::time::timeout(
            Duration::from_secs(COMPUTER_WAIT_SECS + 2),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) if is_control => {
                let request_dispatched = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                return computer_control_delivery_failure(
                        "Runner response channel closed before a terminal computer control result was received",
                        request_dispatched,
                    );
            }
            Ok(Err(_)) => {
                return computer_error("runner_disconnected", "Runner response channel closed")
            }
            Err(_) if is_control => {
                let request_dispatched = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                return computer_control_delivery_failure(
                    "Runner did not return a terminal computer control result in time",
                    request_dispatched,
                );
            }
            Err(_) => {
                return computer_error(
                    "runner_timeout",
                    "Runner did not return computer request in time",
                )
            }
        };
        if let Some(error) = response.error.as_deref() {
            let error_kind = classify_runner_error(error);
            if is_control && error_kind == "outcome_unknown" {
                return computer_control_outcome_unknown(error);
            }
            if is_control && error_kind == "runner_error" {
                return computer_control_delivery_failure(error, response.request_dispatched);
            }
            return computer_error(error_kind, error);
        }
        if response.exit_code != Some(0) {
            if is_control {
                return computer_control_delivery_failure(
                    "Runner computer control ended without a structured terminal result",
                    response.request_dispatched,
                );
            }
            return computer_error("runner_error", "Runner computer request failed");
        }
        let output: Value = match response
            .stdout
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(output)) => output,
            _ if is_control => {
                return computer_control_delivery_failure(
                    "Runner returned invalid JSON after computer control execution",
                    response.request_dispatched,
                )
            }
            _ => {
                return computer_error(
                    "invalid_runner_response",
                    "Runner returned invalid computer JSON",
                )
            }
        };
        match kind {
            "computer_list_windows" => {
                validate_window_list(output, list_limit.unwrap_or(MAX_WINDOWS))
            }
            "computer_snapshot" => {
                validate_snapshot(output, expected_surface_id.unwrap_or_default(), client_id)
            }
            "computer_accessibility_status" => validate_accessibility_status(output),
            "computer_accessibility_tree" => {
                let (max_depth, max_nodes) = accessibility_bounds.unwrap_or((0, 0));
                validate_accessibility_tree(
                    output,
                    expected_surface_id.unwrap_or_default(),
                    max_depth,
                    max_nodes,
                )
            }
            "computer_control" => {
                let result = validate_computer_control(
                    output,
                    expected_surface_id.unwrap_or_default(),
                    expected_element_id.as_deref().unwrap_or_default(),
                    expected_action.as_deref().unwrap_or_default(),
                );
                if result.success {
                    result
                } else {
                    computer_control_outcome_unknown(
                        "Runner reported successful computer control but returned inconsistent metadata; inspect current UI state before retrying",
                    )
                }
            }
            _ => computer_error("invalid_request", "unsupported computer request kind"),
        }
    }
}

fn computer_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({"error_kind": kind, "message": bounded_text(message)}),
    )
}

fn computer_control_not_started(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "not_started",
            "message": bounded_text(message),
            "state_changed": false,
            "execution_state": "not_started"
        }),
    )
}

fn computer_control_outcome_unknown(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(message),
            "execution_state": "outcome_unknown"
        }),
    )
}

fn computer_control_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_control_not_started(message)
    } else {
        computer_control_outcome_unknown(&format!(
            "{message}; the action may have taken effect, so inspect current UI state before retrying"
        ))
    }
}

fn classify_runner_error(error: &str) -> &'static str {
    for kind in [
        "permission_denied",
        "stale_surface",
        "stale_element",
        "unsupported_platform",
        "capture_failed",
        "accessibility_failed",
        "control_failed",
        "outcome_unknown",
        "image_too_large",
        "invalid_request",
    ] {
        if error.starts_with(kind) {
            return kind;
        }
    }
    "runner_error"
}

fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn validate_surface(value: &Value, expected_id: Option<&str>) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "surface must be an object".to_string())?;
    let allowed = [
        "surface_id",
        "application",
        "title",
        "width",
        "height",
        "focused",
        "active",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("surface contains unsupported fields".to_string());
    }
    let surface_id = value
        .get("surface_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "surface_id missing".to_string())?;
    if surface_id.is_empty()
        || surface_id.len() > MAX_SURFACE_ID_BYTES
        || expected_id.is_some_and(|expected| expected != surface_id)
    {
        return Err("surface_id mismatch or invalid".to_string());
    }
    for field in ["application", "title"] {
        let text = value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{field} missing"))?;
        if text.len() > MAX_TEXT_BYTES {
            return Err(format!("{field} exceeds bound"));
        }
    }
    for field in ["width", "height"] {
        let dimension = value
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{field} missing"))?;
        if dimension == 0 || dimension > 32768 {
            return Err(format!("{field} is out of range"));
        }
    }
    for field in ["focused", "active"] {
        if !value
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
        {
            return Err(format!("{field} must be boolean or null"));
        }
    }
    Ok(())
}

fn validate_window_list(output: Value, limit: usize) -> ToolResult {
    let windows = match output.get("windows").and_then(Value::as_array) {
        Some(windows) if windows.len() <= limit && windows.len() <= MAX_WINDOWS => windows,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Runner window list exceeds bound or is missing",
            )
        }
    };
    if let Some(error) = windows
        .iter()
        .find_map(|surface| validate_surface(surface, None).err())
    {
        return computer_error("invalid_runner_response", &error);
    }
    let count = output.get("count").and_then(Value::as_u64);
    let truncated = output.get("truncated").and_then(Value::as_bool);
    if count != Some(windows.len() as u64) || truncated.is_none() {
        return computer_error(
            "invalid_runner_response",
            "Runner window list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_accessibility_status(output: Value) -> ToolResult {
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
        || output.get("platform").and_then(Value::as_str) != Some("macos")
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

fn validate_accessibility_tree(
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
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Accessibility tree fields are inconsistent",
        );
    }
    if output.get("platform").and_then(Value::as_str) != Some("macos")
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("max_depth").and_then(Value::as_u64) != Some(max_depth as u64)
        || output.get("max_nodes").and_then(Value::as_u64) != Some(max_nodes as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
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

fn validate_computer_control(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
    expected_action: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer control result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "element_id", "action", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || output.get("platform").and_then(Value::as_str) != Some("macos")
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("action").and_then(Value::as_str) != Some(expected_action)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer control result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn validate_snapshot(mut output: Value, expected_surface_id: &str, client_id: &str) -> ToolResult {
    let surface = match output.get("surface") {
        Some(surface) => surface,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot surface is missing",
            )
        }
    };
    if let Err(error) = validate_surface(surface, Some(expected_surface_id)) {
        return computer_error("invalid_runner_response", &error);
    }
    let width = output.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = output.get("height").and_then(Value::as_u64).unwrap_or(0);
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot dimensions exceed bound",
        );
    }
    let mime = output
        .get("mime_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(mime, "image/png" | "image/jpeg" | "image/webp") {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot MIME is unsupported",
        );
    }
    let encoded = match output.get("content_base64").and_then(Value::as_str) {
        Some(encoded) => encoded,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot content is missing",
            )
        }
    };
    let decoded = match general_purpose::STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot base64 is invalid",
            )
        }
    };
    if decoded.is_empty() || decoded.len() > MAX_MCP_IMAGE_BYTES {
        return computer_error(
            "image_too_large",
            "Runner snapshot exceeds native MCP image bound",
        );
    }
    if sniff_mime(&decoded) != Some(mime) {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot MIME does not match image bytes",
        );
    }
    if output.get("file_bytes").and_then(Value::as_u64) != Some(decoded.len() as u64) {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot byte count is inconsistent",
        );
    }
    let Some(object) = output.as_object_mut() else {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot output is not an object",
        );
    };
    object.insert("client_id".to_string(), json!(client_id));
    ToolResult::ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(id: &str) -> Value {
        json!({
            "surface_id": id,
            "application": "Example",
            "title": "Window",
            "width": 1280,
            "height": 720,
            "focused": false,
            "active": false
        })
    }

    fn accessibility_tree() -> Value {
        json!({
            "platform": "macos",
            "surface_id": "surface_test",
            "nodes": [
                {
                    "element_id": "element_root",
                    "parent_element_id": null,
                    "depth": 0,
                    "role": "AXWindow",
                    "subrole": null,
                    "title": "Example",
                    "description": null,
                    "value": null,
                    "placeholder": null,
                    "enabled": true,
                    "focused": false,
                    "child_count": 1
                },
                {
                    "element_id": "element_child",
                    "parent_element_id": "element_root",
                    "depth": 1,
                    "role": "AXButton",
                    "subrole": null,
                    "title": "OK",
                    "description": null,
                    "value": null,
                    "placeholder": null,
                    "enabled": true,
                    "focused": false,
                    "child_count": 0
                }
            ],
            "node_count": 2,
            "truncated": false,
            "max_depth": 2,
            "max_nodes": 8
        })
    }

    #[test]
    fn computer_accessibility_tree_validator_accepts_bounded_parent_first_tree() {
        let result = validate_accessibility_tree(accessibility_tree(), "surface_test", 2, 8);
        assert!(result.success, "{:?}", result.output);
    }

    #[test]
    fn computer_accessibility_tree_validator_rejects_forward_parent_reference() {
        let mut tree = accessibility_tree();
        tree["nodes"][0]["parent_element_id"] = json!("element_child");
        tree["nodes"][0]["depth"] = json!(1);
        let result = validate_accessibility_tree(tree, "surface_test", 2, 8);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_runner_response");
    }

    #[test]
    fn computer_control_validator_accepts_exact_metadata_only_success() {
        let result = validate_computer_control(
            json!({
                "platform": "macos",
                "surface_id": "surface_test",
                "element_id": "element_child",
                "action": "focus",
                "success": true
            }),
            "surface_test",
            "element_child",
            "focus",
        );
        assert!(result.success, "{:?}", result.output);
    }

    #[test]
    fn computer_control_validator_rejects_mismatch_or_semantic_extra_fields() {
        let mismatched = validate_computer_control(
            json!({
                "platform": "macos",
                "surface_id": "surface_test",
                "element_id": "element_other",
                "action": "focus",
                "success": true
            }),
            "surface_test",
            "element_child",
            "focus",
        );
        assert!(!mismatched.success);
        assert_eq!(mismatched.output["error_kind"], "invalid_runner_response");

        let semantic_extra = validate_computer_control(
            json!({
                "platform": "macos",
                "surface_id": "surface_test",
                "element_id": "element_child",
                "action": "press",
                "success": true,
                "title": "SECRET BUTTON"
            }),
            "surface_test",
            "element_child",
            "press",
        );
        assert!(!semantic_extra.success);
        assert_eq!(
            semantic_extra.output["error_kind"],
            "invalid_runner_response"
        );
    }

    #[test]
    fn computer_control_transport_failure_is_retryable_only_when_undispatched() {
        let not_started = computer_control_delivery_failure("transport lost", Some(false));
        assert!(!not_started.success);
        assert_eq!(not_started.output["error_kind"], "not_started");
        assert_eq!(not_started.output["state_changed"], false);
        assert_eq!(not_started.output["execution_state"], "not_started");

        for dispatched in [Some(true), None] {
            let unknown = computer_control_delivery_failure("transport lost", dispatched);
            assert!(!unknown.success);
            assert_eq!(unknown.output["error_kind"], "outcome_unknown");
            assert_eq!(unknown.output["execution_state"], "outcome_unknown");
            assert!(unknown.output.get("state_changed").is_none());
        }
    }

    #[test]
    fn read_only_computer_transport_errors_keep_existing_classification() {
        let disconnected = computer_error("runner_disconnected", "Runner response channel closed");
        let timed_out = computer_error("runner_timeout", "Runner did not return computer request");
        assert_eq!(disconnected.output["error_kind"], "runner_disconnected");
        assert_eq!(timed_out.output["error_kind"], "runner_timeout");
        assert!(disconnected.output.get("execution_state").is_none());
        assert!(timed_out.output.get("execution_state").is_none());
    }

    #[test]
    fn computer_control_runner_errors_preserve_structured_error_kinds() {
        for error in [
            "stale_element: handle expired",
            "control_failed: AXPress was rejected",
        ] {
            let result = computer_error(classify_runner_error(error), error);
            assert!(!result.success);
            assert_eq!(result.output["error_kind"], classify_runner_error(error));
        }
        let unknown = "outcome_unknown: AXPress messaging failed after dispatch";
        assert_eq!(classify_runner_error(unknown), "outcome_unknown");
        let result = computer_control_outcome_unknown(unknown);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "outcome_unknown");
        assert_eq!(result.output["execution_state"], "outcome_unknown");
    }

    #[test]
    fn computer_window_list_validator_rejects_more_than_requested_limit() {
        let result = validate_window_list(
            json!({
                "windows": [surface("surface_1"), surface("surface_2")],
                "count": 2,
                "truncated": false
            }),
            1,
        );
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_runner_response");
    }

    #[test]
    fn computer_runner_image_too_large_preserves_structured_error_kind() {
        let error = "image_too_large: raw RGBA capture exceeds bound";
        let result = computer_error(classify_runner_error(error), error);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "image_too_large");
    }

    #[test]
    fn computer_snapshot_validator_rejects_decoded_image_over_mcp_bound() {
        let encoded = "AAAA".repeat((MAX_MCP_IMAGE_BYTES / 3) + 1);
        let result = validate_snapshot(
            json!({
                "surface": surface("surface_test"),
                "width": 1280,
                "height": 720,
                "mime_type": "image/jpeg",
                "file_bytes": MAX_MCP_IMAGE_BYTES + 2,
                "content_base64": encoded
            }),
            "surface_test",
            "msi",
        );
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "image_too_large");
    }

    #[test]
    fn computer_snapshot_validator_attaches_exact_client_id() {
        let image = [0xff, 0xd8, 0xff, 0xe0];
        let result = validate_snapshot(
            json!({
                "surface": surface("surface_test"),
                "width": 1280,
                "height": 720,
                "mime_type": "image/jpeg",
                "file_bytes": image.len(),
                "content_base64": general_purpose::STANDARD.encode(image)
            }),
            "surface_test",
            "msi",
        );
        assert!(result.success);
        assert_eq!(result.output["client_id"], "msi");
    }
}
