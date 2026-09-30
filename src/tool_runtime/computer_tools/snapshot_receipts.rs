//! Untrusted snapshot bytes, dimensions and identity validation.

use super::effects::computer_error;
use super::inputs::valid_display_id;
use super::screens::validate_surface;
use super::*;
fn expected_display_snapshot_dimensions(
    source_width: u64,
    source_height: u64,
    max_width: Option<u64>,
    max_height: Option<u64>,
) -> (u64, u64) {
    let width_scale = max_width
        .map(|bound| bound as f64 / source_width as f64)
        .unwrap_or(1.0);
    let height_scale = max_height
        .map(|bound| bound as f64 / source_height as f64)
        .unwrap_or(1.0);
    let scale = 1.0f64.min(width_scale).min(height_scale);
    if scale < 1.0 {
        let width = ((source_width as f64 * scale).floor() as u64)
            .max(1)
            .min(max_width.unwrap_or(u64::MAX));
        let height = ((source_height as f64 * scale).floor() as u64)
            .max(1)
            .min(max_height.unwrap_or(u64::MAX));
        (width, height)
    } else {
        (source_width, source_height)
    }
}

pub(super) fn validate_display_snapshot(
    mut output: Value,
    expected_display_id: &str,
    client_id: &str,
    expected_max_width: Option<u64>,
    expected_max_height: Option<u64>,
) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot is not an object",
        );
    };
    let allowed = [
        "display_id",
        "snapshot_generation",
        "source_width",
        "source_height",
        "width",
        "height",
        "mime_type",
        "file_bytes",
        "sha256",
        "captured_at_unix_ms",
        "content_base64",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot fields are inconsistent",
        );
    }
    if !valid_display_id(expected_display_id)
        || output.get("display_id").and_then(Value::as_str) != Some(expected_display_id)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot identity is inconsistent",
        );
    }
    let generation = output
        .get("snapshot_generation")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let source_width = output
        .get("source_width")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let source_height = output
        .get("source_height")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if generation == 0
        || generation > u32::MAX as u64
        || source_width == 0
        || source_width > u32::MAX as u64
        || source_height == 0
        || source_height > u32::MAX as u64
        || source_width
            .checked_mul(source_height)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_RAW_CAPTURE_BYTES)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot source geometry is invalid",
        );
    }
    let width = output
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let height = output
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let expected_dimensions = expected_display_snapshot_dimensions(
        source_width,
        source_height,
        expected_max_width,
        expected_max_height,
    );
    if (width, height) != expected_dimensions
        || width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot dimensions are inconsistent",
        );
    }
    if output.get("mime_type").and_then(Value::as_str) != Some("image/jpeg") {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot MIME is invalid",
        );
    }
    let Some(encoded) = output.get("content_base64").and_then(Value::as_str) else {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot content is missing",
        );
    };
    let decoded = match general_purpose::STANDARD.decode(encoded) {
        Ok(decoded) if !decoded.is_empty() && decoded.len() <= MAX_MCP_IMAGE_BYTES => decoded,
        _ => {
            return computer_error(
                "image_too_large",
                "Runner display snapshot image is invalid or too large",
            )
        }
    };
    if sniff_mime(&decoded) != Some("image/jpeg")
        || output.get("file_bytes").and_then(Value::as_u64) != Some(decoded.len() as u64)
        || output.get("sha256").and_then(Value::as_str) != Some(sha256_hex(&decoded).as_str())
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot image metadata is inconsistent",
        );
    }
    const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
    if !matches!(
        output.get("captured_at_unix_ms").and_then(Value::as_u64),
        Some(value) if value > 0 && value <= MAX_SAFE_JSON_INTEGER
    ) {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot timestamp is invalid",
        );
    }
    output
        .as_object_mut()
        .expect("display snapshot object shape checked above")
        .insert("client_id".to_string(), json!(client_id));
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

fn snapshot_region_values(region: &Value) -> Option<(u64, u64, u64, u64)> {
    let object = region.as_object()?;
    let allowed = ["x", "y", "width", "height"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return None;
    }
    Some((
        region.get("x")?.as_u64()?,
        region.get("y")?.as_u64()?,
        region.get("width")?.as_u64()?,
        region.get("height")?.as_u64()?,
    ))
}

pub(super) fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn validate_snapshot(
    mut output: Value,
    expected_surface_id: &str,
    client_id: &str,
    advanced: bool,
    expected_region: Option<&Value>,
    expected_max_width: Option<u64>,
    expected_max_height: Option<u64>,
) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot output is not an object",
        );
    };
    let metadata_fields = [
        "source_width",
        "source_height",
        "region",
        "sha256",
        "captured_at_unix_ms",
    ];
    let metadata_present = metadata_fields
        .iter()
        .any(|field| object.contains_key(*field));
    if advanced && !metadata_present {
        return computer_error(
            "invalid_runner_response",
            "Runner advanced snapshot metadata is missing",
        );
    }
    if metadata_present
        && metadata_fields
            .iter()
            .any(|field| !object.contains_key(*field))
    {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot metadata is incomplete",
        );
    }

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
    let surface_width = surface.get("width").and_then(Value::as_u64).unwrap_or(0);
    let surface_height = surface.get("height").and_then(Value::as_u64).unwrap_or(0);
    let width = output.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = output.get("height").and_then(Value::as_u64).unwrap_or(0);
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot dimensions exceed bound",
        );
    }
    if expected_max_width.is_some_and(|bound| width > bound)
        || expected_max_height.is_some_and(|bound| height > bound)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot exceeds requested output dimensions",
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

    if metadata_present {
        if output.get("source_width").and_then(Value::as_u64) != Some(surface_width)
            || output.get("source_height").and_then(Value::as_u64) != Some(surface_height)
        {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot source dimensions are inconsistent",
            );
        }
        let actual_region = output.get("region").and_then(snapshot_region_values);
        let expected_region = expected_region.and_then(snapshot_region_values).or(Some((
            0,
            0,
            surface_width,
            surface_height,
        )));
        let Some((x, y, region_width, region_height)) = actual_region else {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot region metadata is invalid",
            );
        };
        if region_width == 0
            || region_height == 0
            || x.checked_add(region_width)
                .is_none_or(|right| right > surface_width)
            || y.checked_add(region_height)
                .is_none_or(|bottom| bottom > surface_height)
            || actual_region != expected_region
        {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot region metadata is inconsistent",
            );
        }
        if output.get("sha256").and_then(Value::as_str) != Some(sha256_hex(&decoded).as_str()) {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot SHA-256 is inconsistent",
            );
        }
        const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
        if !matches!(
            output.get("captured_at_unix_ms").and_then(Value::as_u64),
            Some(value) if value > 0 && value <= MAX_SAFE_JSON_INTEGER
        ) {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot capture timestamp is invalid",
            );
        }
    }

    let Some(object) = output.as_object_mut() else {
        unreachable!("snapshot object shape checked above")
    };
    object.insert("client_id".to_string(), json!(client_id));
    ToolResult::ok(output)
}
