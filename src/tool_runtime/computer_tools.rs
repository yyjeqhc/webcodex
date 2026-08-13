use super::{ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::auth::AuthContext;
use crate::shell_protocol::SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::time::Duration;

const MAX_WINDOWS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
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
    ) -> ToolResult {
        if client_id.is_empty() || client_id.len() > 128 {
            return computer_error("invalid_client", "client_id is invalid");
        }
        match self
            .shell_clients
            .client_supports_for_auth(client_id, SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE, auth)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                return computer_error(
                    "capability_unavailable",
                    "target Runner does not support computer_observe",
                )
            }
            Err(error) => return computer_error("client_access_denied", &error),
        }
        let payload = match serde_json::to_string(&payload) {
            Ok(payload) => payload,
            Err(_) => {
                return computer_error("invalid_request", "could not encode computer request")
            }
        };
        let requested_by = crate::shell_client::requested_by_from_auth(auth);
        let (_, receiver) = match self
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
        let response =
            match tokio::time::timeout(Duration::from_secs(COMPUTER_WAIT_SECS + 2), receiver).await
            {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => {
                    return computer_error("runner_disconnected", "Runner response channel closed")
                }
                Err(_) => {
                    return computer_error(
                        "runner_timeout",
                        "Runner did not return computer observation in time",
                    )
                }
            };
        if let Some(error) = response.error.as_deref() {
            return computer_error(classify_runner_error(error), error);
        }
        if response.exit_code != Some(0) {
            return computer_error("runner_error", "Runner computer observation failed");
        }
        let output: Value = match response
            .stdout
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(output)) => output,
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
                validate_snapshot(output, expected_surface_id.unwrap_or_default())
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

fn classify_runner_error(error: &str) -> &'static str {
    for kind in [
        "permission_denied",
        "stale_surface",
        "unsupported_platform",
        "capture_failed",
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

fn validate_snapshot(output: Value, expected_surface_id: &str) -> ToolResult {
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
        );
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "image_too_large");
    }
}
