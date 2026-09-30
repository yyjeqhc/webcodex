//! Bounded application, display and window observation receipts.

use super::effects::computer_error;
use super::inputs::{valid_application_id, valid_display_id};
use super::*;
pub(super) fn validate_surface(value: &Value, expected_id: Option<&str>) -> Result<(), String> {
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

pub(super) fn validate_application_list(output: Value, limit: usize) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner application list is not an object",
            )
        }
    };
    let allowed = ["applications", "count", "truncated"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner application list fields are inconsistent",
        );
    }
    let applications = match output.get("applications").and_then(Value::as_array) {
        Some(applications)
            if applications.len() <= limit && applications.len() <= MAX_APPLICATIONS =>
        {
            applications
        }
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Runner application list exceeds bound or is missing",
            )
        }
    };
    let mut seen = std::collections::HashSet::with_capacity(applications.len());
    for application in applications {
        let Some(entry) = application.as_object() else {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry is not an object",
            );
        };
        let allowed_entry = ["application_id", "display_name"];
        if entry.len() != allowed_entry.len()
            || entry
                .keys()
                .any(|key| !allowed_entry.contains(&key.as_str()))
        {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry fields are inconsistent",
            );
        }
        let application_id = application
            .get("application_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let display_name = application
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !valid_application_id(application_id)
            || !seen.insert(application_id)
            || display_name.is_empty()
            || display_name.len() > MAX_TEXT_BYTES
            || display_name.contains('\0')
        {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry is invalid",
            );
        }
    }
    if output.get("count").and_then(Value::as_u64) != Some(applications.len() as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Runner application list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_display_list(output: Value, limit: usize) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner display list is not an object",
        );
    };
    let allowed = ["displays", "count", "truncated"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner display list fields are inconsistent",
        );
    }
    let Some(displays) = output.get("displays").and_then(Value::as_array) else {
        return computer_error("invalid_runner_response", "Runner display list is missing");
    };
    if displays.len() > limit || displays.len() > MAX_DISPLAYS {
        return computer_error(
            "invalid_runner_response",
            "Runner display list exceeds bound",
        );
    }
    let mut seen = std::collections::HashSet::with_capacity(displays.len());
    for display in displays {
        let Some(entry) = display.as_object() else {
            return computer_error(
                "invalid_runner_response",
                "Runner display entry is not an object",
            );
        };
        let allowed_entry = ["display_id", "width", "height", "primary"];
        if entry.len() != allowed_entry.len()
            || entry
                .keys()
                .any(|key| !allowed_entry.contains(&key.as_str()))
        {
            return computer_error(
                "invalid_runner_response",
                "Runner display entry fields are inconsistent",
            );
        }
        let display_id = display
            .get("display_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let width = display
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let height = display
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if !valid_display_id(display_id)
            || !seen.insert(display_id)
            || width == 0
            || width > u32::MAX as u64
            || height == 0
            || height > u32::MAX as u64
            || display.get("primary").and_then(Value::as_bool).is_none()
        {
            return computer_error("invalid_runner_response", "Runner display entry is invalid");
        }
    }
    if output.get("count").and_then(Value::as_u64) != Some(displays.len() as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

pub(super) fn validate_window_list(output: Value, limit: usize) -> ToolResult {
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
