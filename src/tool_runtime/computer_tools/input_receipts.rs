//! Request-bound receipts for clipboard and native input effects.

use super::accessibility::is_native_accessibility_platform;
use super::effects::{computer_error, ClipboardWriteContext, PointerRequestContext};
use super::*;
pub(super) fn validate_computer_read_clipboard(output: Value) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read result is not an object",
        );
    };
    if !matches!(
        output.get("platform").and_then(Value::as_str),
        Some("windows" | "macos")
    ) {
        return computer_error(
            "invalid_runner_response",
            "clipboard read platform is inconsistent",
        );
    }
    let Some(available) = output.get("available").and_then(Value::as_bool) else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read availability is missing",
        );
    };
    let Some(text_bytes) = output.get("text_bytes").and_then(Value::as_u64) else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read byte count is missing",
        );
    };
    if text_bytes > MAX_CLIPBOARD_TEXT_BYTES as u64 {
        return computer_error(
            "invalid_runner_response",
            "clipboard read byte count exceeds bound",
        );
    }
    if available {
        let allowed = ["platform", "available", "text", "text_bytes"];
        let Some(text) = output.get("text").and_then(Value::as_str) else {
            return computer_error(
                "invalid_runner_response",
                "available clipboard text is missing",
            );
        };
        if object.len() != allowed.len()
            || object.keys().any(|key| !allowed.contains(&key.as_str()))
            || text.len() != text_bytes as usize
            || text.len() > MAX_CLIPBOARD_TEXT_BYTES
            || text.contains('\0')
        {
            return computer_error(
                "invalid_runner_response",
                "clipboard read success metadata is inconsistent",
            );
        }
    } else {
        let allowed = ["platform", "available", "text_bytes"];
        if object.len() != allowed.len()
            || object.keys().any(|key| !allowed.contains(&key.as_str()))
            || text_bytes != 0
            || object.contains_key("text")
        {
            return computer_error(
                "invalid_runner_response",
                "unavailable clipboard result is inconsistent",
            );
        }
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_write_clipboard(
    output: Value,
    context: &ClipboardWriteContext,
) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "clipboard write result is not an object",
        );
    };
    let allowed = ["platform", "text_bytes", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !matches!(
            output.get("platform").and_then(Value::as_str),
            Some("windows" | "macos")
        )
        || output.get("success").and_then(Value::as_bool) != Some(true)
        || output.get("text_bytes").and_then(Value::as_u64)
            != context.text_bytes.map(|value| value as u64)
    {
        return computer_error(
            "invalid_runner_response",
            "clipboard write success metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_launch_application(
    output: Value,
    expected_application_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "application launch result is not an object",
            )
        }
    };
    let allowed = ["platform", "application_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !matches!(
            output.get("platform").and_then(Value::as_str),
            Some("windows" | "macos")
        )
        || output.get("application_id").and_then(Value::as_str) != Some(expected_application_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "application launch success metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn is_native_window_activation_platform(value: Option<&str>) -> bool {
    matches!(value, Some("macos" | "windows"))
}

pub(super) fn validate_computer_activate_window(
    output: Value,
    expected_surface_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer window activation result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer window activation result metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_control(
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
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
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

pub(super) fn validate_computer_scroll_to_element(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer scroll result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "element_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer scroll result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_pointer(
    mut output: Value,
    context: &PointerRequestContext,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer pointer result is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "display_id",
        "snapshot_generation",
        "x",
        "y",
        "success",
    ];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !matches!(
            output.get("platform").and_then(Value::as_str),
            Some("windows" | "macos")
        )
        || output.get("display_id").and_then(Value::as_str) != Some(context.display_id.as_str())
        || output.get("snapshot_generation").and_then(Value::as_u64)
            != Some(u64::from(context.snapshot_generation))
        || output.get("x").and_then(Value::as_u64) != Some(u64::from(context.x))
        || output.get("y").and_then(Value::as_u64) != Some(u64::from(context.y))
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer pointer result is inconsistent",
        );
    }
    let object = output
        .as_object_mut()
        .expect("computer pointer output was validated as an object");
    object.insert("execution_state".to_string(), json!("completed"));
    object.insert("state_changed".to_string(), json!(true));
    ToolResult::ok(output)
}

pub(super) fn validate_computer_key_input(
    output: Value,
    expected_surface_id: &str,
    expected_key: &str,
    expected_modifiers: &Value,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer key input result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "key", "modifiers", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("key").and_then(Value::as_str) != Some(expected_key)
        || output.get("modifiers") != Some(expected_modifiers)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer key input result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_computer_input_text(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
    expected_text_bytes: usize,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer text input result is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "element_id",
        "text_bytes",
        "success",
    ];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("text_bytes").and_then(Value::as_u64) != Some(expected_text_bytes as u64)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer text input result is inconsistent",
        );
    }
    ToolResult::ok(output)
}
