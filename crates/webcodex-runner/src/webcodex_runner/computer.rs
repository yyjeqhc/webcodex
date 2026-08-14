use super::{err_cmd, ok_cmd, CommandResult};
#[cfg(any(target_os = "macos", windows))]
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::shell_protocol::ShellAgentShellRequest;
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
#[cfg(any(test, target_os = "macos"))]
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;

const MAX_WINDOWS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ACCESSIBILITY_DEPTH: usize = 8;
const MAX_ACCESSIBILITY_NODES: usize = 256;
const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
#[cfg(any(test, target_os = "macos", windows))]
const RGBA_BYTES_PER_PIXEL: u64 = 4;
#[cfg(any(test, target_os = "macos", windows))]
/// Pre-capture ceiling for the expected complete raw RGBA frame. Standard
/// 8K UHD (7680x4320x4) fits while malformed/extreme dimensions fail closed
/// before xcap is allowed to allocate the native capture image.
const MAX_RAW_CAPTURE_BYTES: u64 = 128 * 1024 * 1024;
#[cfg(any(target_os = "macos", windows))]
const MAX_IMAGE_DIMENSION: u32 = 4096;

#[cfg(any(test, target_os = "macos"))]
const AX_MESSAGING_TIMEOUT_SECS: f32 = 2.0;
#[cfg(any(test, target_os = "macos"))]
const AX_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(any(test, target_os = "macos"))]
const AX_OBSERVATION_TIMEOUT_ERROR: &str =
    "accessibility_failed: macOS Accessibility observation deadline exceeded";

#[cfg(any(test, target_os = "macos"))]
struct AxObservationDeadline {
    expires_at: Instant,
}

#[cfg(any(test, target_os = "macos"))]
impl AxObservationDeadline {
    fn new() -> Self {
        Self::from_now(Instant::now(), AX_OBSERVATION_TIMEOUT)
    }

    fn from_now(now: Instant, budget: Duration) -> Self {
        Self {
            expires_at: now + budget,
        }
    }

    fn ensure_remaining_at(&self, now: Instant) -> Result<(), String> {
        self.expires_at
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
            .map(|_| ())
            .ok_or_else(|| AX_OBSERVATION_TIMEOUT_ERROR.to_string())
    }

    fn ensure_remaining(&self) -> Result<(), String> {
        self.ensure_remaining_at(Instant::now())
    }

    fn remaining_timeout_secs_at(&self, now: Instant) -> Result<f32, String> {
        let remaining = self
            .expires_at
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| AX_OBSERVATION_TIMEOUT_ERROR.to_string())?;
        let timeout = remaining.as_secs_f32().min(AX_MESSAGING_TIMEOUT_SECS);
        if timeout.is_finite() && timeout > 0.0 {
            Ok(timeout)
        } else {
            Err(AX_OBSERVATION_TIMEOUT_ERROR.to_string())
        }
    }

    fn remaining_timeout_secs(&self) -> Result<f32, String> {
        self.remaining_timeout_secs_at(Instant::now())
    }
}

#[cfg(any(test, target_os = "macos"))]
fn select_exact_ax_window_index(
    geometry_matches: &[usize],
    title_matches: &[usize],
    window_count: usize,
) -> Result<usize, String> {
    let resolved = match geometry_matches {
        [index] => Some(*index),
        [] => match title_matches {
            [index] => Some(*index),
            _ => None,
        },
        _ => None,
    }
    .filter(|index| *index < window_count);

    resolved.ok_or_else(|| {
        format!(
            "accessibility_failed: exact AX window could not be resolved uniquely (geometry={}, title={}, windows={window_count})",
            geometry_matches.len(),
            title_matches.len(),
        )
    })
}

#[derive(Clone)]
struct SurfaceRecord {
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    native_id: u32,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pid: u32,
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    identity_hash: [u8; 32],
    application: String,
    title: String,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct SurfaceOutput<'a> {
    surface_id: &'a str,
    application: &'a str,
    title: &'a str,
    width: u32,
    height: u32,
    focused: Option<bool>,
    active: Option<bool>,
}

struct ComputerObserver {
    surfaces: Mutex<HashMap<String, SurfaceRecord>>,
}

impl ComputerObserver {
    fn global() -> &'static Self {
        static OBSERVER: OnceLock<ComputerObserver> = OnceLock::new();
        OBSERVER.get_or_init(|| ComputerObserver {
            surfaces: Mutex::new(HashMap::new()),
        })
    }

    fn list_windows(&self, limit: usize) -> Result<Value, String> {
        let candidates = platform::list_windows(MAX_WINDOWS + 1)?;
        let truncated = candidates.len() > limit;
        let mut surfaces = HashMap::new();
        let mut windows = Vec::new();
        for candidate in candidates.into_iter().take(limit) {
            let surface_id = format!("surface_{}", Uuid::new_v4().simple());
            let record = SurfaceRecord {
                native_id: candidate.native_id,
                pid: candidate.pid,
                identity_hash: candidate.identity_hash,
                application: bounded_text(&candidate.application),
                title: bounded_text(&candidate.title),
                width: candidate.width,
                height: candidate.height,
            };
            windows.push(json!(SurfaceOutput {
                surface_id: &surface_id,
                application: &record.application,
                title: &record.title,
                width: record.width,
                height: record.height,
                focused: candidate.focused,
                active: candidate.active,
            }));
            surfaces.insert(surface_id, record);
        }
        let count = windows.len();
        *self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())? =
            surfaces;
        Ok(json!({"windows": windows, "count": count, "truncated": truncated}))
    }

    fn accessibility_status(&self) -> Result<Value, String> {
        platform::accessibility_status()
    }

    fn accessibility_tree(
        &self,
        surface_id: &str,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        if max_depth > MAX_ACCESSIBILITY_DEPTH
            || !(1..=MAX_ACCESSIBILITY_NODES).contains(&max_nodes)
        {
            return Err("invalid_request: accessibility bounds are invalid".to_string());
        }
        let record = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        platform::accessibility_tree(surface_id, &record, max_depth, max_nodes)
    }

    fn snapshot(&self, surface_id: &str) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        let record = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        let image = platform::capture_window(&record)?;
        let encoded = encode_bounded_jpeg(image)?;
        let file_bytes = encoded.bytes.len();
        Ok(json!({
            "surface": SurfaceOutput {
                surface_id,
                application: &record.application,
                title: &record.title,
                width: record.width,
                height: record.height,
                focused: None,
                active: None,
            },
            "width": encoded.width,
            "height": encoded.height,
            "mime_type": "image/jpeg",
            "file_bytes": file_bytes,
            "content_base64": general_purpose::STANDARD.encode(encoded.bytes),
        }))
    }
}

struct EncodedImage {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

#[cfg(any(target_os = "macos", windows))]
fn encode_bounded_jpeg(mut image: image::RgbaImage) -> Result<EncodedImage, String> {
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::FilterType;

    if image.width() == 0 || image.height() == 0 {
        return Err("capture_failed: captured image has zero dimensions".to_string());
    }
    if image.width() > MAX_IMAGE_DIMENSION || image.height() > MAX_IMAGE_DIMENSION {
        let scale = (MAX_IMAGE_DIMENSION as f64 / image.width() as f64)
            .min(MAX_IMAGE_DIMENSION as f64 / image.height() as f64);
        image = image::imageops::resize(
            &image,
            ((image.width() as f64 * scale).floor() as u32).max(1),
            ((image.height() as f64 * scale).floor() as u32).max(1),
            FilterType::Triangle,
        );
    }
    for _ in 0..5 {
        for quality in [82u8, 72, 62, 52, 42] {
            let mut bytes = Vec::new();
            JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode_image(&image)
                .map_err(|error| format!("capture_failed: JPEG encoding failed: {error}"))?;
            if bytes.len() <= MAX_MCP_IMAGE_BYTES {
                return Ok(EncodedImage {
                    bytes,
                    width: image.width(),
                    height: image.height(),
                });
            }
        }
        if image.width() <= 320 || image.height() <= 240 {
            break;
        }
        image = image::imageops::resize(
            &image,
            (image.width() * 3 / 4).max(1),
            (image.height() * 3 / 4).max(1),
            FilterType::Triangle,
        );
    }
    Err(format!(
        "image_too_large: screenshot could not be encoded within {MAX_MCP_IMAGE_BYTES} bytes"
    ))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn encode_bounded_jpeg(_image: ()) -> Result<EncodedImage, String> {
    Err("unsupported_platform: computer observation is unavailable on this platform".to_string())
}

fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(any(test, target_os = "macos", windows))]
fn raw_rgba_bytes(width: u32, height: u32) -> Result<u64, String> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(RGBA_BYTES_PER_PIXEL))
        .ok_or_else(|| "image_too_large: raw RGBA capture size overflow".to_string())
}

#[cfg(any(test, target_os = "macos", windows))]
fn ensure_raw_capture_bound(width: u32, height: u32) -> Result<u64, String> {
    let bytes = raw_rgba_bytes(width, height)?;
    if bytes > MAX_RAW_CAPTURE_BYTES {
        Err(format!(
            "image_too_large: raw RGBA capture {width}x{height} requires {bytes} bytes, limit {MAX_RAW_CAPTURE_BYTES}"
        ))
    } else {
        Ok(bytes)
    }
}

#[cfg(any(test, windows))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawCapturePlan {
    native_width: i32,
    native_height: i32,
    width: u32,
    height: u32,
    byte_len: usize,
}

#[cfg(any(test, windows))]
fn raw_capture_plan(native_width: i32, native_height: i32) -> Result<RawCapturePlan, String> {
    if native_width <= 0 || native_height <= 0 {
        return Err("image_too_large: native bitmap dimensions must be positive".to_string());
    }
    let width = u32::try_from(native_width)
        .map_err(|_| "image_too_large: native bitmap width is invalid".to_string())?;
    let height = u32::try_from(native_height)
        .map_err(|_| "image_too_large: native bitmap height is invalid".to_string())?;
    let raw_bytes = ensure_raw_capture_bound(width, height)?;
    let byte_len = usize::try_from(raw_bytes).map_err(|_| {
        "image_too_large: raw RGBA capture size does not fit the address space".to_string()
    })?;
    Ok(RawCapturePlan {
        native_width,
        native_height,
        width,
        height,
        byte_len,
    })
}

#[cfg(any(test, windows))]
fn native_capture_dimension(value: u32, axis: &str) -> Result<i32, String> {
    i32::try_from(value)
        .map_err(|_| format!("image_too_large: {axis} does not fit a Win32 bitmap dimension"))
}

#[cfg(test)]
mod raw_capture_bound_tests {
    use super::*;

    #[test]
    fn computer_raw_capture_bound_allows_modern_8k_uhd() {
        assert!(ensure_raw_capture_bound(7680, 4320).is_ok());
        assert_eq!(
            ensure_raw_capture_bound(8192, 4096).unwrap(),
            MAX_RAW_CAPTURE_BYTES
        );
    }

    #[test]
    fn computer_raw_capture_bound_rejects_over_limit_before_capture() {
        let error = ensure_raw_capture_bound(8192, 4097).unwrap_err();
        assert!(error.starts_with("image_too_large:"), "{error}");
    }

    #[test]
    fn computer_raw_capture_plan_preserves_checked_native_dimensions() {
        let modern_8k = raw_capture_plan(7680, 4320).unwrap();
        assert_eq!(
            (modern_8k.native_width, modern_8k.native_height),
            (7680, 4320)
        );
        let boundary = raw_capture_plan(8192, 4096).unwrap();
        assert_eq!(boundary.byte_len as u64, MAX_RAW_CAPTURE_BYTES);
    }

    #[test]
    fn computer_raw_capture_plan_rejects_invalid_native_dimensions() {
        for (width, height) in [(0, 1), (1, 0), (-1, 1), (1, -1)] {
            let error = raw_capture_plan(width, height).unwrap_err();
            assert!(error.starts_with("image_too_large:"), "{error}");
        }
    }

    #[test]
    fn computer_raw_capture_plan_rejects_over_bound_native_dimensions() {
        let error = raw_capture_plan(8192, 4097).unwrap_err();
        assert!(error.starts_with("image_too_large:"), "{error}");
    }

    #[test]
    fn computer_native_capture_dimension_rejects_conversion_overflow() {
        let error = native_capture_dimension(u32::MAX, "width").unwrap_err();
        assert!(error.starts_with("image_too_large:"), "{error}");
    }

    #[test]
    fn computer_raw_capture_bound_fails_closed_on_multiplication_overflow() {
        let error = ensure_raw_capture_bound(u32::MAX, u32::MAX).unwrap_err();
        assert!(error.starts_with("image_too_large:"), "{error}");
    }
}

#[cfg(test)]
mod ax_observation_bound_tests {
    use super::*;

    #[test]
    fn computer_ax_window_selection_accepts_unique_geometry() {
        assert_eq!(select_exact_ax_window_index(&[1], &[0, 1], 2).unwrap(), 1);
    }

    #[test]
    fn computer_ax_window_selection_rejects_ambiguous_geometry() {
        let error = select_exact_ax_window_index(&[0, 1], &[0], 2).unwrap_err();
        assert!(error.starts_with("accessibility_failed:"), "{error}");
    }

    #[test]
    fn computer_ax_window_selection_accepts_unique_title_only_without_geometry_match() {
        assert_eq!(select_exact_ax_window_index(&[], &[1], 2).unwrap(), 1);
        assert!(select_exact_ax_window_index(&[0, 1], &[1], 2).is_err());
    }

    #[test]
    fn computer_ax_window_selection_rejects_uncorrelated_single_window() {
        let error = select_exact_ax_window_index(&[], &[], 1).unwrap_err();
        assert!(error.starts_with("accessibility_failed:"), "{error}");
    }

    #[test]
    fn computer_ax_window_selection_rejects_ambiguous_title() {
        let error = select_exact_ax_window_index(&[], &[0, 1], 2).unwrap_err();
        assert!(error.starts_with("accessibility_failed:"), "{error}");
    }

    #[test]
    fn computer_ax_observation_deadline_expired_fails_closed() {
        let now = Instant::now();
        let deadline = AxObservationDeadline::from_now(now, Duration::ZERO);
        assert_eq!(
            deadline.remaining_timeout_secs_at(now).unwrap_err(),
            AX_OBSERVATION_TIMEOUT_ERROR
        );
        assert_eq!(
            deadline.ensure_remaining_at(now).unwrap_err(),
            AX_OBSERVATION_TIMEOUT_ERROR
        );
    }

    #[test]
    fn computer_ax_observation_call_timeout_never_exceeds_remaining_budget() {
        let now = Instant::now();
        let short_budget = Duration::from_millis(250);
        let short_deadline = AxObservationDeadline::from_now(now, short_budget);
        let short_timeout = short_deadline.remaining_timeout_secs_at(now).unwrap();
        assert!(short_timeout > 0.0);
        assert!(short_timeout <= short_budget.as_secs_f32());

        let long_budget = Duration::from_secs(5);
        let long_deadline = AxObservationDeadline::from_now(now, long_budget);
        assert_eq!(
            long_deadline.remaining_timeout_secs_at(now).unwrap(),
            AX_MESSAGING_TIMEOUT_SECS
        );
    }
}

pub(crate) fn is_computer_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "computer_list_windows"
            | "computer_snapshot"
            | "computer_accessibility_status"
            | "computer_accessibility_tree"
    )
}

pub(crate) fn handle_computer_request(request: &ShellAgentShellRequest) -> CommandResult {
    let start = Instant::now();
    let payload = match request.stdin.as_deref() {
        Some(payload) if payload.len() <= 4096 && !payload.contains('\0') => {
            match serde_json::from_str::<Value>(payload) {
                Ok(value) => value,
                Err(_) => {
                    return err_cmd(
                        start,
                        "invalid_request: computer payload is not valid JSON".to_string(),
                    )
                }
            }
        }
        _ => {
            return err_cmd(
                start,
                "invalid_request: computer payload is required and bounded".to_string(),
            )
        }
    };
    if !request.command.is_empty()
        || request.cwd.is_some()
        || request.path.is_some()
        || request.content.is_some()
        || request.process.is_some()
        || request.script.is_some()
        || request.job_id.is_some()
        || request.lsp.is_some()
        || request.job_context.is_some()
        || request.persistent_shell.is_some()
    {
        return err_cmd(
            start,
            "invalid_request: computer request contains unrelated execution fields".to_string(),
        );
    }
    let result = match request.kind.as_str() {
        "computer_list_windows" => {
            let limit = payload
                .get("limit")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(MAX_WINDOWS)
                .clamp(1, MAX_WINDOWS);
            ComputerObserver::global().list_windows(limit)
        }
        "computer_accessibility_status" => ComputerObserver::global().accessibility_status(),
        "computer_accessibility_tree" => {
            let surface_id = payload
                .get("surface_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "invalid_request: surface_id is required".to_string());
            let max_depth = payload
                .get("max_depth")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(DEFAULT_ACCESSIBILITY_DEPTH);
            let max_nodes = payload
                .get("max_nodes")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(DEFAULT_ACCESSIBILITY_NODES);
            surface_id.and_then(|surface_id| {
                ComputerObserver::global().accessibility_tree(surface_id, max_depth, max_nodes)
            })
        }
        "computer_snapshot" => payload
            .get("surface_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "invalid_request: surface_id is required".to_string())
            .and_then(|surface_id| ComputerObserver::global().snapshot(surface_id)),
        _ => Err("invalid_request: unsupported computer request kind".to_string()),
    };
    match result {
        Ok(result) => ok_cmd(start, result),
        Err(error) => err_cmd(start, error),
    }
}

#[derive(Clone)]
struct PlatformWindow {
    native_id: u32,
    pid: u32,
    identity_hash: [u8; 32],
    application: String,
    title: String,
    width: u32,
    height: u32,
    focused: Option<bool>,
    active: Option<bool>,
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::{PlatformWindow, SurfaceRecord};

    pub(super) fn list_windows(_limit: usize) -> Result<Vec<PlatformWindow>, String> {
        Err(
            "unsupported_platform: computer observation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn accessibility_status() -> Result<serde_json::Value, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn accessibility_tree(
        _surface_id: &str,
        _surface: &SurfaceRecord,
        _max_depth: usize,
        _max_nodes: usize,
    ) -> Result<serde_json::Value, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn capture_window(_surface: &SurfaceRecord) -> Result<(), String> {
        Err(
            "unsupported_platform: computer observation is unavailable on this platform"
                .to_string(),
        )
    }
}

#[cfg(all(test, not(any(target_os = "macos", windows))))]
mod tests {
    use super::*;

    fn request(kind: &str, payload: &str) -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "computer-test".to_string(),
            client_id: "runner".to_string(),
            kind: kind.to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: Some(payload.to_string()),
            timeout_secs: 5,
            requested_by: "test".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        }
    }

    #[test]
    fn computer_unsupported_platform_fails_closed_without_shell_fallback() {
        let result = handle_computer_request(&request("computer_list_windows", r#"{"limit":1}"#));
        assert_eq!(result.exit_code, None);
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("unsupported_platform:")));
    }

    #[test]
    fn computer_unknown_surface_is_stale_before_platform_capture() {
        let result = handle_computer_request(&request(
            "computer_snapshot",
            r#"{"surface_id":"surface_missing"}"#,
        ));
        assert_eq!(result.exit_code, None);
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("stale_surface:")));
    }
}

#[cfg(any(target_os = "macos", windows))]
mod platform {
    use super::{bounded_text, ensure_raw_capture_bound, PlatformWindow, SurfaceRecord};
    #[cfg(target_os = "macos")]
    use super::{select_exact_ax_window_index, AxObservationDeadline};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use xcap::Window;

    #[cfg(target_os = "macos")]
    use objc2_application_services::{
        AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType,
    };
    #[cfg(target_os = "macos")]
    use objc2_core_foundation::{
        CFArray, CFBoolean, CFIndex, CFRetained, CFString, CFType, CGPoint, CGSize,
    };
    #[cfg(target_os = "macos")]
    use std::collections::VecDeque;
    #[cfg(target_os = "macos")]
    use std::ptr::NonNull;
    #[cfg(target_os = "macos")]
    use uuid::Uuid;

    #[cfg(windows)]
    use windows_sys::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
            GetCurrentObject, GetDIBits, GetObjectW, GetWindowDC, ReleaseDC, SelectObject, BITMAP,
            BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, OBJ_BITMAP,
            SRCCOPY,
        },
        Storage::Xps::PrintWindow,
    };

    #[cfg(target_os = "macos")]
    fn ensure_capture_permission() -> Result<(), String> {
        let granted = objc2_core_graphics::CGPreflightScreenCaptureAccess();
        if granted {
            Ok(())
        } else {
            Err("permission_denied: macOS Screen Recording permission is not granted".to_string())
        }
    }

    #[cfg(windows)]
    fn ensure_capture_permission() -> Result<(), String> {
        Ok(())
    }

    fn map_error(error: impl std::fmt::Display) -> String {
        let message = error.to_string();
        if message.to_ascii_lowercase().contains("permission") {
            format!("permission_denied: {message}")
        } else {
            format!("capture_failed: {message}")
        }
    }

    #[cfg(target_os = "macos")]
    const MAX_AX_WINDOWS: usize = 64;
    #[cfg(target_os = "macos")]
    const MAX_AX_CHILD_COUNT: usize = 1_000_000;

    #[cfg(target_os = "macos")]
    fn accessibility_error(operation: &str, error: AXError) -> String {
        if error == AXError::APIDisabled {
            "permission_denied: macOS Accessibility permission is not granted".to_string()
        } else {
            format!(
                "accessibility_failed: {operation} failed with AXError({})",
                error.0
            )
        }
    }

    #[cfg(target_os = "macos")]
    fn prepare_ax_call(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
    ) -> Result<(), String> {
        let timeout_secs = deadline.remaining_timeout_secs()?;
        let error = unsafe { element.set_messaging_timeout(timeout_secs) };
        if error != AXError::Success {
            return Err(accessibility_error("AXUIElementSetMessagingTimeout", error));
        }
        deadline.ensure_remaining()
    }

    #[cfg(target_os = "macos")]
    fn optional_ax_value(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<Option<CFRetained<CFType>>, String> {
        let attribute = CFString::from_static_str(attribute);
        let mut raw: *const CFType = std::ptr::null();
        prepare_ax_call(deadline, element)?;
        let error = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut raw)) };
        deadline.ensure_remaining()?;
        match error {
            AXError::Success => {
                let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
                    "accessibility_failed: AX attribute succeeded with null value".to_string()
                })?;
                Ok(Some(unsafe { CFRetained::from_raw(raw) }))
            }
            AXError::AttributeUnsupported | AXError::NoValue => Ok(None),
            error => Err(accessibility_error("AXUIElementCopyAttributeValue", error)),
        }
    }

    #[cfg(target_os = "macos")]
    fn optional_ax_string(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<Option<String>, String> {
        let Some(value) = optional_ax_value(deadline, element, attribute)? else {
            return Ok(None);
        };
        Ok(value
            .downcast::<CFString>()
            .ok()
            .map(|value| bounded_text(&value.to_string())))
    }

    #[cfg(target_os = "macos")]
    fn optional_ax_bool(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<Option<bool>, String> {
        let Some(value) = optional_ax_value(deadline, element, attribute)? else {
            return Ok(None);
        };
        Ok(value
            .downcast::<CFBoolean>()
            .ok()
            .map(|value| value.value()))
    }

    #[cfg(target_os = "macos")]
    fn optional_ax_point(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<Option<CGPoint>, String> {
        let Some(value) = optional_ax_value(deadline, element, attribute)? else {
            return Ok(None);
        };
        let Ok(value) = value.downcast::<AXValue>() else {
            return Ok(None);
        };
        if unsafe { value.r#type() } != AXValueType::CGPoint {
            return Ok(None);
        }
        let mut point = CGPoint::ZERO;
        let copied = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
        Ok(copied.then_some(point))
    }

    #[cfg(target_os = "macos")]
    fn optional_ax_size(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<Option<CGSize>, String> {
        let Some(value) = optional_ax_value(deadline, element, attribute)? else {
            return Ok(None);
        };
        let Ok(value) = value.downcast::<AXValue>() else {
            return Ok(None);
        };
        if unsafe { value.r#type() } != AXValueType::CGSize {
            return Ok(None);
        }
        let mut size = CGSize::ZERO;
        let copied = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
        Ok(copied.then_some(size))
    }

    #[cfg(target_os = "macos")]
    fn ax_window_geometry_matches(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<bool, String> {
        const TOLERANCE: f64 = 2.0;
        let Some(position) = optional_ax_point(deadline, element, "AXPosition")? else {
            return Ok(false);
        };
        let Some(size) = optional_ax_size(deadline, element, "AXSize")? else {
            return Ok(false);
        };
        Ok((position.x - f64::from(x)).abs() <= TOLERANCE
            && (position.y - f64::from(y)).abs() <= TOLERANCE
            && (size.width - f64::from(width)).abs() <= TOLERANCE
            && (size.height - f64::from(height)).abs() <= TOLERANCE)
    }

    #[cfg(target_os = "macos")]
    fn ax_array_count(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute: &'static str,
    ) -> Result<usize, String> {
        let attribute = CFString::from_static_str(attribute);
        let mut count: CFIndex = 0;
        prepare_ax_call(deadline, element)?;
        let error = unsafe { element.attribute_value_count(&attribute, NonNull::from(&mut count)) };
        deadline.ensure_remaining()?;
        match error {
            AXError::Success => {
                let count = usize::try_from(count).map_err(|_| {
                    "accessibility_failed: AX array count is negative or too large".to_string()
                })?;
                if count > MAX_AX_CHILD_COUNT {
                    return Err(
                        "accessibility_failed: AX child count exceeds bounded inspection limit"
                            .to_string(),
                    );
                }
                Ok(count)
            }
            AXError::AttributeUnsupported | AXError::NoValue => Ok(0),
            error => Err(accessibility_error(
                "AXUIElementGetAttributeValueCount",
                error,
            )),
        }
    }

    #[cfg(target_os = "macos")]
    fn ax_elements(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute_name: &'static str,
        count: usize,
    ) -> Result<Vec<CFRetained<AXUIElement>>, String> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let attribute = CFString::from_static_str(attribute_name);
        let max_values = CFIndex::try_from(count)
            .map_err(|_| "accessibility_failed: AX array request exceeds CFIndex".to_string())?;
        let mut raw: *const CFArray = std::ptr::null();
        prepare_ax_call(deadline, element)?;
        let error = unsafe {
            element.copy_attribute_values(&attribute, 0, max_values, NonNull::from(&mut raw))
        };
        deadline.ensure_remaining()?;
        if error != AXError::Success {
            return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
        }
        let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
            "accessibility_failed: AX array copy succeeded with null value".to_string()
        })?;
        let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
        let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
        let mut output = Vec::with_capacity(array.len());
        for value in array.iter() {
            let element = value.downcast::<AXUIElement>().map_err(|_| {
                "accessibility_failed: AX element array contained a non-element value".to_string()
            })?;
            output.push(element);
        }
        Ok(output)
    }

    #[cfg(target_os = "macos")]
    fn resolve_surface_window(surface: &SurfaceRecord) -> Result<Window, String> {
        let window = Window::all()
            .map_err(map_error)?
            .into_iter()
            .find(|window| window.id().ok() == Some(surface.native_id))
            .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
        let width = window.width().map_err(map_error)?;
        let height = window.height().map_err(map_error)?;
        let application = window.app_name().map_err(map_error)?;
        let title = window.title().map_err(map_error)?;
        let pid = window.pid().map_err(map_error)?;
        let current_identity = identity_hash(&window, &application, &title, width, height)?;
        if pid != surface.pid || current_identity != surface.identity_hash {
            return Err("stale_surface: window identity changed since enumeration".to_string());
        }
        if bounded_text(&application) != surface.application
            || bounded_text(&title) != surface.title
        {
            return Err("stale_surface: window metadata changed since enumeration".to_string());
        }
        Ok(window)
    }

    #[cfg(target_os = "macos")]
    fn exact_ax_window(
        surface: &SurfaceRecord,
        deadline: &AxObservationDeadline,
    ) -> Result<CFRetained<AXUIElement>, String> {
        let native_window = resolve_surface_window(surface)?;
        deadline.ensure_remaining()?;
        let x = native_window.x().map_err(map_error)?;
        let y = native_window.y().map_err(map_error)?;
        let width = native_window.width().map_err(map_error)?;
        let height = native_window.height().map_err(map_error)?;
        let application = unsafe { AXUIElement::new_application(surface.pid as _) };
        let window_count = ax_array_count(deadline, &application, "AXWindows")?;
        if window_count == 0 || window_count > MAX_AX_WINDOWS {
            return Err(
                "accessibility_failed: exact AX window cannot be resolved within the bounded window set"
                    .to_string(),
            );
        }
        let mut windows = ax_elements(deadline, &application, "AXWindows", window_count)?;
        let mut geometry_matches = Vec::new();
        for (index, window) in windows.iter().enumerate() {
            if ax_window_geometry_matches(deadline, window, x, y, width, height)? {
                geometry_matches.push(index);
            }
        }
        if !geometry_matches.is_empty() {
            let index = select_exact_ax_window_index(&geometry_matches, &[], windows.len())?;
            return Ok(windows.swap_remove(index));
        }

        let mut title_matches = Vec::new();
        if !surface.title.is_empty() {
            for (index, window) in windows.iter().enumerate() {
                if optional_ax_string(deadline, window, "AXTitle")?
                    .is_some_and(|title| bounded_text(&title) == surface.title)
                {
                    title_matches.push(index);
                }
            }
        }
        let index = select_exact_ax_window_index(&geometry_matches, &title_matches, windows.len())?;
        Ok(windows.swap_remove(index))
    }

    #[cfg(target_os = "macos")]
    pub(super) fn accessibility_status() -> Result<Value, String> {
        Ok(json!({
            "platform": "macos",
            "trusted": unsafe { AXIsProcessTrusted() },
        }))
    }

    #[cfg(windows)]
    pub(super) fn accessibility_status() -> Result<Value, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    #[cfg(target_os = "macos")]
    pub(super) fn accessibility_tree(
        surface_id: &str,
        surface: &SurfaceRecord,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Value, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        let deadline = AxObservationDeadline::new();
        let root = exact_ax_window(surface, &deadline)?;
        let mut queue = VecDeque::from([(root, None::<String>, 0usize)]);
        let mut nodes = Vec::with_capacity(max_nodes.min(64));
        let mut truncated = false;
        while let Some((element, parent_element_id, depth)) = queue.pop_front() {
            deadline.ensure_remaining()?;
            if nodes.len() >= max_nodes {
                truncated = true;
                break;
            }
            let element_id = format!("element_{}", Uuid::new_v4().simple());
            let role = optional_ax_string(&deadline, &element, "AXRole")?.ok_or_else(|| {
                "accessibility_failed: AX element is missing a string role".to_string()
            })?;
            let subrole = optional_ax_string(&deadline, &element, "AXSubrole")?;
            let title = optional_ax_string(&deadline, &element, "AXTitle")?;
            let description = optional_ax_string(&deadline, &element, "AXDescription")?;
            let placeholder = optional_ax_string(&deadline, &element, "AXPlaceholderValue")?;
            let sensitive = role == "AXSecureTextField"
                || subrole
                    .as_deref()
                    .is_some_and(|value| value.contains("Secure"));
            let value = if sensitive {
                None
            } else {
                optional_ax_string(&deadline, &element, "AXValue")?
            };
            let enabled = optional_ax_bool(&deadline, &element, "AXEnabled")?;
            let focused = optional_ax_bool(&deadline, &element, "AXFocused")?;
            let child_count = ax_array_count(&deadline, &element, "AXChildren")?;
            if depth < max_depth && child_count > 0 {
                let reserved = nodes.len() + queue.len() + 1;
                let remaining = max_nodes.saturating_sub(reserved);
                let take = child_count.min(remaining);
                if take < child_count {
                    truncated = true;
                }
                for child in ax_elements(&deadline, &element, "AXChildren", take)? {
                    queue.push_back((child, Some(element_id.clone()), depth + 1));
                }
            } else if child_count > 0 {
                truncated = true;
            }
            nodes.push(json!({
                "element_id": element_id,
                "parent_element_id": parent_element_id,
                "depth": depth,
                "role": role,
                "subrole": subrole,
                "title": title,
                "description": description,
                "value": value,
                "placeholder": placeholder,
                "enabled": enabled,
                "focused": focused,
                "child_count": child_count,
            }));
        }
        if !queue.is_empty() {
            truncated = true;
        }
        deadline.ensure_remaining()?;
        let node_count = nodes.len();
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "nodes": nodes,
            "node_count": node_count,
            "truncated": truncated,
            "max_depth": max_depth,
            "max_nodes": max_nodes,
        }))
    }

    #[cfg(windows)]
    pub(super) fn accessibility_tree(
        _surface_id: &str,
        _surface: &SurfaceRecord,
        _max_depth: usize,
        _max_nodes: usize,
    ) -> Result<Value, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    #[cfg(windows)]
    fn win32_capture_error(operation: &str) -> String {
        format!(
            "capture_failed: {operation} failed: {}",
            std::io::Error::last_os_error()
        )
    }

    #[cfg(windows)]
    struct WindowDc {
        hwnd: HWND,
        hdc: HDC,
    }

    #[cfg(windows)]
    impl WindowDc {
        fn acquire(hwnd: HWND) -> Result<Self, String> {
            let hdc = unsafe { GetWindowDC(hwnd) };
            if hdc.is_null() {
                Err(win32_capture_error("GetWindowDC"))
            } else {
                Ok(Self { hwnd, hdc })
            }
        }
    }

    #[cfg(windows)]
    impl Drop for WindowDc {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseDC(self.hwnd, self.hdc);
            }
        }
    }

    #[cfg(windows)]
    struct MemoryDc(HDC);

    #[cfg(windows)]
    impl MemoryDc {
        fn create(source: HDC) -> Result<Self, String> {
            let hdc = unsafe { CreateCompatibleDC(source) };
            if hdc.is_null() {
                Err(win32_capture_error("CreateCompatibleDC"))
            } else {
                Ok(Self(hdc))
            }
        }
    }

    #[cfg(windows)]
    impl Drop for MemoryDc {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    #[cfg(windows)]
    struct OwnedBitmap(HBITMAP);

    #[cfg(windows)]
    impl OwnedBitmap {
        fn create(source: HDC, plan: super::RawCapturePlan) -> Result<Self, String> {
            let bitmap =
                unsafe { CreateCompatibleBitmap(source, plan.native_width, plan.native_height) };
            if bitmap.is_null() {
                Err(win32_capture_error("CreateCompatibleBitmap"))
            } else {
                Ok(Self(bitmap))
            }
        }
    }

    #[cfg(windows)]
    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.0 as HGDIOBJ);
            }
        }
    }

    #[cfg(windows)]
    struct SelectedBitmap {
        hdc: HDC,
        previous: HGDIOBJ,
    }

    #[cfg(windows)]
    impl SelectedBitmap {
        fn select(hdc: HDC, bitmap: HBITMAP) -> Result<Self, String> {
            let previous = unsafe { SelectObject(hdc, bitmap as HGDIOBJ) };
            if previous.is_null() {
                Err(win32_capture_error("SelectObject"))
            } else {
                Ok(Self { hdc, previous })
            }
        }
    }

    #[cfg(windows)]
    impl Drop for SelectedBitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.hdc, self.previous);
            }
        }
    }

    #[cfg(windows)]
    fn create_bounded_bitmap(
        source: HDC,
        fallback: super::RawCapturePlan,
    ) -> Result<(OwnedBitmap, super::RawCapturePlan), String> {
        let mut native_width = fallback.native_width;
        let mut native_height = fallback.native_height;
        let current = unsafe { GetCurrentObject(source, OBJ_BITMAP as u32) };
        if !current.is_null() {
            let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
            let copied = unsafe {
                GetObjectW(
                    current,
                    std::mem::size_of::<BITMAP>() as i32,
                    (&mut bitmap as *mut BITMAP).cast(),
                )
            };
            if copied != 0 {
                native_width = bitmap.bmWidth;
                native_height = bitmap.bmHeight;
            }
        }

        let plan = super::raw_capture_plan(native_width, native_height)?;
        let bitmap = OwnedBitmap::create(source, plan)?;
        Ok((bitmap, plan))
    }

    #[cfg(windows)]
    fn capture_window_gdi(
        native_id: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<image::RgbaImage, String> {
        let output_plan = super::raw_capture_plan(
            super::native_capture_dimension(output_width, "window width")?,
            super::native_capture_dimension(output_height, "window height")?,
        )?;
        let hwnd = native_id as i32 as isize as HWND;
        if hwnd.is_null() {
            return Err("capture_failed: window handle is invalid".to_string());
        }

        let window_dc = WindowDc::acquire(hwnd)?;
        let (bitmap, capture_plan) = create_bounded_bitmap(window_dc.hdc, output_plan)?;
        let memory_dc = MemoryDc::create(window_dc.hdc)?;
        let selected = SelectedBitmap::select(memory_dc.0, bitmap.0)?;

        let mut captured = unsafe { PrintWindow(hwnd, memory_dc.0, 2) } != 0;
        if !captured {
            captured = unsafe { PrintWindow(hwnd, memory_dc.0, 0) } != 0;
        }
        if !captured {
            captured = unsafe { PrintWindow(hwnd, memory_dc.0, 4) } != 0;
        }
        if !captured {
            captured = unsafe {
                BitBlt(
                    memory_dc.0,
                    0,
                    0,
                    capture_plan.native_width,
                    capture_plan.native_height,
                    window_dc.hdc,
                    0,
                    0,
                    SRCCOPY,
                )
            } != 0;
        }
        if !captured {
            return Err(win32_capture_error("PrintWindow/BitBlt"));
        }
        drop(selected);

        let raw_size = u32::try_from(capture_plan.byte_len)
            .map_err(|_| "image_too_large: raw RGBA capture size exceeds Win32".to_string())?;
        let mut bitmap_info: BITMAPINFO = unsafe { std::mem::zeroed() };
        bitmap_info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: capture_plan.native_width,
            biHeight: -capture_plan.native_height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            biSizeImage: raw_size,
            ..unsafe { std::mem::zeroed() }
        };
        let mut pixels = vec![0u8; capture_plan.byte_len];
        let scan_lines = unsafe {
            GetDIBits(
                memory_dc.0,
                bitmap.0,
                0,
                capture_plan.height,
                pixels.as_mut_ptr().cast(),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            )
        };
        if scan_lines != capture_plan.native_height {
            return Err(win32_capture_error("GetDIBits"));
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let image = image::RgbaImage::from_raw(capture_plan.width, capture_plan.height, pixels)
            .ok_or_else(|| "capture_failed: RGBA image dimensions are inconsistent".to_string())?;
        if image.width() == output_plan.width && image.height() == output_plan.height {
            Ok(image)
        } else {
            Ok(image::imageops::resize(
                &image,
                output_plan.width,
                output_plan.height,
                image::imageops::FilterType::CatmullRom,
            ))
        }
    }

    #[cfg(target_os = "macos")]
    fn ensure_platform_capture_bound(
        _window: &Window,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // xcap's macOS window bounds are display-space dimensions while
        // CGWindowListCreateImage can produce backing-pixel images. Use the
        // largest live display scale as a conservative bound, including windows
        // spanning displays; missing/invalid scale information fails closed.
        let monitors = xcap::Monitor::all().map_err(map_error)?;
        let mut max_scale: Option<f32> = None;
        for monitor in monitors {
            let scale = monitor.scale_factor().map_err(map_error)?;
            if !scale.is_finite() || scale <= 0.0 {
                return Err(
                    "image_too_large: cannot establish a bounded macOS capture scale".to_string(),
                );
            }
            max_scale = Some(max_scale.map_or(scale, |current| current.max(scale)));
        }
        let scale = max_scale
            .ok_or_else(|| {
                "image_too_large: cannot establish a bounded macOS capture scale".to_string()
            })?
            .max(1.0);
        let scaled = |value: u32| -> Result<u32, String> {
            let pixels = (f64::from(value) * f64::from(scale)).ceil();
            if !pixels.is_finite() || pixels > f64::from(u32::MAX) {
                return Err("image_too_large: macOS capture dimensions overflow".to_string());
            }
            Ok(pixels as u32)
        };
        ensure_raw_capture_bound(scaled(width)?, scaled(height)?)?;
        Ok(())
    }

    #[cfg(windows)]
    fn ensure_platform_capture_bound(
        window: &Window,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // xcap 0.9.8 is built here without WGC, so its GDI path can allocate a
        // display-backed bitmap before cropping to the requested window. Bound
        // both the current window and current monitor; if monitor metrics cannot
        // be obtained we fail closed instead of pretending the window size alone
        // bounds the native allocation.
        ensure_raw_capture_bound(width, height)?;
        let monitor = window.current_monitor().map_err(map_error)?;
        let monitor_width = monitor.width().map_err(map_error)?;
        let monitor_height = monitor.height().map_err(map_error)?;
        ensure_raw_capture_bound(monitor_width, monitor_height)?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
        // xcap 0.9.8 reports frontmost-application state on macOS, not exact
        // window focus. Preserve that reliable signal as `active` only.
        (None, window.is_focused().ok())
    }

    #[cfg(windows)]
    fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
        // Windows xcap compares this HWND with GetForegroundWindow(), which is
        // an exact-window signal suitable for both CU-1 fields.
        let focused = window.is_focused().ok();
        (focused, focused)
    }

    fn identity_hash(
        window: &Window,
        application: &str,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<[u8; 32], String> {
        let pid = window.pid().map_err(map_error)?;
        let mut hash = Sha256::new();
        hash.update(pid.to_le_bytes());
        hash.update((application.len() as u64).to_le_bytes());
        hash.update(application.as_bytes());
        hash.update((title.len() as u64).to_le_bytes());
        hash.update(title.as_bytes());
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        Ok(hash.finalize().into())
    }

    pub(super) fn list_windows(limit: usize) -> Result<Vec<PlatformWindow>, String> {
        ensure_capture_permission()?;
        let mut output = Vec::new();
        for window in Window::all().map_err(map_error)? {
            if output.len() >= limit {
                break;
            }
            if window.is_minimized().unwrap_or(false) {
                continue;
            }
            let width = window.width().map_err(map_error)?;
            let height = window.height().map_err(map_error)?;
            if width == 0 || height == 0 {
                continue;
            }
            let application = window.app_name().map_err(map_error)?;
            let title = window.title().map_err(map_error)?;
            let identity_hash = identity_hash(&window, &application, &title, width, height)?;
            let (focused, active) = focus_state(&window);
            output.push(PlatformWindow {
                native_id: window.id().map_err(map_error)?,
                pid: window.pid().map_err(map_error)?,
                identity_hash,
                application,
                title,
                width,
                height,
                focused,
                active,
            });
        }
        Ok(output)
    }

    pub(super) fn capture_window(surface: &SurfaceRecord) -> Result<image::RgbaImage, String> {
        ensure_capture_permission()?;
        let window = Window::all()
            .map_err(map_error)?
            .into_iter()
            .find(|window| window.id().ok() == Some(surface.native_id))
            .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
        let width = window.width().map_err(map_error)?;
        let height = window.height().map_err(map_error)?;
        let application = window.app_name().map_err(map_error)?;
        let title = window.title().map_err(map_error)?;
        let current_identity = identity_hash(&window, &application, &title, width, height)?;
        if current_identity != surface.identity_hash {
            return Err("stale_surface: window identity changed since enumeration".to_string());
        }
        if bounded_text(&application) != surface.application
            || bounded_text(&title) != surface.title
        {
            return Err("stale_surface: window metadata changed since enumeration".to_string());
        }
        #[cfg(target_os = "macos")]
        {
            ensure_platform_capture_bound(&window, width, height)?;
            window.capture_image().map_err(map_error)
        }
        #[cfg(windows)]
        {
            capture_window_gdi(surface.native_id, width, height)
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_live_tests {
    use super::*;

    fn surface_record(candidate: PlatformWindow) -> SurfaceRecord {
        SurfaceRecord {
            native_id: candidate.native_id,
            pid: candidate.pid,
            identity_hash: candidate.identity_hash,
            application: bounded_text(&candidate.application),
            title: bounded_text(&candidate.title),
            width: candidate.width,
            height: candidate.height,
        }
    }

    fn live_accessibility_smoke(application_matches: impl Fn(&str) -> bool) -> bool {
        let candidates = platform::list_windows(MAX_WINDOWS).expect("list live macOS windows");
        let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| application_matches(&candidate.application))
        else {
            return false;
        };
        let record = surface_record(candidate);
        let output = platform::accessibility_tree("surface_live", &record, 3, 64)
            .expect("read bounded live accessibility tree");
        assert_eq!(output["platform"], "macos");
        assert!(output["node_count"].as_u64().unwrap_or(0) > 0);
        for node in output["nodes"].as_array().expect("nodes array") {
            assert!(node["role"].as_str().is_some_and(|role| !role.is_empty()));
        }
        true
    }

    #[test]
    #[ignore = "requires live macOS Accessibility permission and desktop"]
    fn computer_macos_accessibility_permission_live_smoke() {
        let status = platform::accessibility_status().expect("read accessibility status");
        assert_eq!(status["trusted"], true);
    }

    #[test]
    #[ignore = "requires live Microsoft Edge window and macOS Accessibility permission"]
    fn computer_macos_accessibility_edge_live_smoke() {
        assert!(
            live_accessibility_smoke(|application| {
                application.to_ascii_lowercase().contains("microsoft edge")
                    || application.to_ascii_lowercase() == "edge"
            }),
            "Microsoft Edge window must be open for this live smoke"
        );
    }

    #[test]
    #[ignore = "requires live WeChat window and macOS Accessibility permission"]
    fn computer_macos_accessibility_wechat_live_smoke() {
        assert!(
            live_accessibility_smoke(|application| {
                application == "微信" || application.to_ascii_lowercase().contains("wechat")
            }),
            "WeChat window must be open for this live smoke"
        );
    }
}
