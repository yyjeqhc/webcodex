use super::{err_cmd, ok_cmd, CommandResult};
#[cfg(any(target_os = "macos", windows))]
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::shell_protocol::{shell_computer_request_payload_max_bytes, ShellAgentShellRequest};
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
#[cfg(any(test, target_os = "macos"))]
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;

const MAX_WINDOWS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_ELEMENT_REGISTRY: usize = 1024;
const MAX_INPUT_TEXT_BYTES: usize = 2048;
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
#[cfg(target_os = "macos")]
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
    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
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

#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComputerAction {
    Press,
    Focus,
}

impl ComputerAction {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "press" => Ok(Self::Press),
            "focus" => Ok(Self::Focus),
            _ => Err("invalid_request: computer control action must be press or focus".to_string()),
        }
    }

    #[cfg(target_os = "macos")]
    fn as_str(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Focus => "focus",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ElementFingerprint {
    role: String,
    subrole: Option<String>,
    identifier: Option<String>,
    title: Option<String>,
    description: Option<String>,
    placeholder: Option<String>,
    protected: bool,
}

impl ElementFingerprint {
    #[cfg(any(test, target_os = "macos"))]
    fn has_positive_evidence(&self) -> bool {
        [
            self.identifier.as_deref(),
            self.title.as_deref(),
            self.description.as_deref(),
            self.placeholder.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| !value.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ElementRecord {
    surface_id: String,
    path: Vec<usize>,
    lineage: Vec<ElementFingerprint>,
}

impl ElementRecord {
    #[cfg(any(test, target_os = "macos"))]
    fn target_fingerprint(&self) -> Option<&ElementFingerprint> {
        (self.lineage.len() == self.path.len() + 1)
            .then(|| self.lineage.last())
            .flatten()
    }

    #[cfg(any(test, target_os = "macos"))]
    fn contains_protected_content(&self) -> bool {
        self.lineage.iter().any(|fingerprint| fingerprint.protected)
    }
}

fn validate_input_text(text: &str) -> Result<usize, String> {
    let text_bytes = text.len();
    if text_bytes == 0 || text_bytes > MAX_INPUT_TEXT_BYTES || text.contains('\0') {
        return Err("invalid_request: computer text input must be non-empty, NUL-free, and within the UTF-8 byte limit".to_string());
    }
    Ok(text_bytes)
}

#[cfg(any(test, target_os = "macos"))]
fn is_secure_text_fingerprint(fingerprint: &ElementFingerprint) -> bool {
    fingerprint.role == "AXSecureTextField"
        || fingerprint
            .subrole
            .as_deref()
            .is_some_and(|subrole| subrole.contains("Secure"))
}

#[cfg(any(test, target_os = "macos"))]
fn is_supported_text_input_fingerprint(fingerprint: &ElementFingerprint) -> bool {
    match fingerprint.role.as_str() {
        "AXTextField" => matches!(fingerprint.subrole.as_deref(), None | Some("AXSearchField")),
        "AXTextArea" => fingerprint.subrole.is_none(),
        _ => false,
    }
}

#[cfg(any(test, target_os = "macos"))]
fn validate_text_input_target(element: &ElementRecord) -> Result<&ElementFingerprint, String> {
    let target = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot receive text input"
                .to_string(),
        );
    }
    if element.lineage.iter().any(is_secure_text_fingerprint) {
        return Err(
            "permission_denied: secure Accessibility text elements cannot receive text input"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for text input"
                .to_string(),
        );
    }
    if !is_supported_text_input_fingerprint(target) {
        return Err(
            "input_failed: AX element is not a supported bounded text-entry role/subrole"
                .to_string(),
        );
    }
    Ok(target)
}

#[cfg(any(test, target_os = "macos"))]
fn validate_text_input_preflight(
    enabled: Option<bool>,
    focused: Option<bool>,
    value_settable: bool,
    current_value: Option<&str>,
) -> Result<(), String> {
    if enabled == Some(false) {
        return Err("input_failed: AX text element is disabled".to_string());
    }
    if focused != Some(true) {
        return Err("input_failed: AX text element must already be focused".to_string());
    }
    if !value_settable {
        return Err("input_failed: AXValue is not settable for this text element".to_string());
    }
    match current_value {
        Some("") => Ok(()),
        Some(_) => Err(
            "input_failed: AXValue must be empty before bounded text input; observe and reconcile before retrying"
                .to_string(),
        ),
        None => Err("input_failed: AXValue is unavailable for bounded text input".to_string()),
    }
}

#[cfg(any(test, target_os = "macos"))]
fn ensure_correlated_fingerprint(
    expected: &ElementFingerprint,
    current: &ElementFingerprint,
    ancestor: bool,
) -> Result<(), String> {
    if current == expected {
        Ok(())
    } else if ancestor {
        Err("stale_element: AX element ancestor identity changed since observation".to_string())
    } else {
        Err("stale_element: AX element lineage changed since observation".to_string())
    }
}

struct AccessibilityTreeResult {
    output: Value,
    elements: Vec<(String, ElementRecord)>,
}

#[derive(Default)]
struct ElementRegistry {
    entries: HashMap<String, ElementRecord>,
    order: VecDeque<String>,
}

impl ElementRegistry {
    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn get(&self, element_id: &str) -> Option<ElementRecord> {
        self.entries.get(element_id).cloned()
    }

    fn replace_surface(&mut self, surface_id: &str, elements: Vec<(String, ElementRecord)>) {
        let stale_ids: Vec<String> = self
            .entries
            .iter()
            .filter_map(|(element_id, record)| {
                (record.surface_id == surface_id).then(|| element_id.clone())
            })
            .collect();
        for element_id in &stale_ids {
            self.entries.remove(element_id);
        }
        self.order
            .retain(|element_id| !stale_ids.iter().any(|stale| stale == element_id));

        for (element_id, record) in elements {
            debug_assert_eq!(record.surface_id, surface_id);
            self.order.push_back(element_id.clone());
            self.entries.insert(element_id, record);
        }
        while self.entries.len() > MAX_ELEMENT_REGISTRY {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
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
    elements: Mutex<ElementRegistry>,
}

impl ComputerObserver {
    fn global() -> &'static Self {
        static OBSERVER: OnceLock<ComputerObserver> = OnceLock::new();
        OBSERVER.get_or_init(|| ComputerObserver {
            surfaces: Mutex::new(HashMap::new()),
            elements: Mutex::new(ElementRegistry::default()),
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
        let mut surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        let mut element_registry = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?;
        *surface_registry = surfaces;
        element_registry.clear();
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
        let tree = platform::accessibility_tree(surface_id, &record, max_depth, max_nodes)?;
        let surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        if surface_registry.get(surface_id) != Some(&record) {
            return Err(
                "stale_surface: surface registry changed during accessibility observation"
                    .to_string(),
            );
        }
        let mut element_registry = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?;
        element_registry.replace_surface(surface_id, tree.elements);
        Ok(tree.output)
    }

    fn activate_window(&self, surface_id: &str) -> Result<Value, String> {
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
        platform::activate_window(surface_id, &record)
    }

    fn control(
        &self,
        surface_id: &str,
        element_id: &str,
        action: ComputerAction,
    ) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        if !element_id.starts_with("element_")
            || element_id.len() <= "element_".len()
            || element_id.len() > MAX_ELEMENT_ID_BYTES
        {
            return Err("invalid_request: element_id is invalid".to_string());
        }
        let surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        let record = surface_registry
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        let element = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?
            .get(element_id)
            .ok_or_else(|| "stale_element: unknown, evicted, or stale element_id".to_string())?;
        if element.surface_id != surface_id {
            return Err("stale_element: element_id belongs to a different surface".to_string());
        }
        platform::control(surface_id, element_id, &record, &element, action)
    }

    fn input_text(&self, surface_id: &str, element_id: &str, text: &str) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        if !element_id.starts_with("element_")
            || element_id.len() <= "element_".len()
            || element_id.len() > MAX_ELEMENT_ID_BYTES
        {
            return Err("invalid_request: element_id is invalid".to_string());
        }
        validate_input_text(text)?;
        let surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        let record = surface_registry
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        let element = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?
            .get(element_id)
            .ok_or_else(|| "stale_element: unknown, evicted, or stale element_id".to_string())?;
        if element.surface_id != surface_id {
            return Err("stale_element: element_id belongs to a different surface".to_string());
        }
        platform::input_text(surface_id, element_id, &record, &element, text)
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

#[cfg(test)]
mod element_registry_tests {
    use super::*;

    fn fingerprint(label: &str) -> ElementFingerprint {
        ElementFingerprint {
            role: "AXButton".to_string(),
            subrole: None,
            identifier: None,
            title: Some(label.to_string()),
            description: None,
            placeholder: None,
            protected: false,
        }
    }

    fn record(surface_id: &str, label: &str, path: Vec<usize>) -> ElementRecord {
        let fingerprint = fingerprint(label);
        ElementRecord {
            surface_id: surface_id.to_string(),
            lineage: vec![fingerprint; path.len() + 1],
            path,
        }
    }

    #[test]
    fn computer_element_registry_is_bounded_and_evicts_oldest() {
        let mut registry = ElementRegistry::default();
        let elements = (0..=MAX_ELEMENT_REGISTRY)
            .map(|index| {
                let element_id = format!("element_{index}");
                (
                    element_id,
                    record("surface_test", &format!("button-{index}"), vec![index]),
                )
            })
            .collect();
        registry.replace_surface("surface_test", elements);
        assert_eq!(registry.entries.len(), MAX_ELEMENT_REGISTRY);
        assert!(registry.get("element_0").is_none());
        assert!(registry
            .get(&format!("element_{MAX_ELEMENT_REGISTRY}"))
            .is_some());
    }

    #[test]
    fn computer_element_registry_replaces_same_surface_generation() {
        let mut registry = ElementRegistry::default();
        registry.replace_surface(
            "surface_test",
            vec![(
                "element_old".to_string(),
                record("surface_test", "old", vec![0]),
            )],
        );
        registry.replace_surface(
            "surface_test",
            vec![(
                "element_new".to_string(),
                record("surface_test", "new", vec![1]),
            )],
        );
        assert!(registry.get("element_old").is_none());
        assert!(registry.get("element_new").is_some());
    }

    #[test]
    fn computer_element_registry_clear_invalidates_all_handles() {
        let mut registry = ElementRegistry::default();
        registry.replace_surface(
            "surface_test",
            vec![(
                "element_test".to_string(),
                record("surface_test", "test", vec![]),
            )],
        );
        registry.clear();
        assert!(registry.entries.is_empty());
        assert!(registry.order.is_empty());
    }

    #[test]
    fn computer_control_actions_are_closed_to_press_and_focus() {
        assert_eq!(
            ComputerAction::parse("press").unwrap(),
            ComputerAction::Press
        );
        assert_eq!(
            ComputerAction::parse("focus").unwrap(),
            ComputerAction::Focus
        );
        for action in ["type", "scroll", "click", "", "PRESS"] {
            assert!(ComputerAction::parse(action).is_err(), "{action}");
        }
    }

    #[test]
    fn computer_element_fingerprint_requires_positive_correlation_evidence() {
        let mut fingerprint = fingerprint("");
        assert!(!fingerprint.has_positive_evidence());
        fingerprint.identifier = Some("stable-id".to_string());
        assert!(fingerprint.has_positive_evidence());
    }

    #[test]
    fn computer_element_record_requires_complete_lineage_and_tracks_protected_content() {
        let mut element = record("surface_test", "target", vec![0, 1]);
        assert!(element.target_fingerprint().is_some());
        assert!(!element.contains_protected_content());
        element.lineage[1].protected = true;
        assert!(element.contains_protected_content());
        element.lineage.pop();
        assert!(element.target_fingerprint().is_none());
    }

    #[test]
    fn computer_activate_window_payload_is_exact_surface_only() {
        let exact = json!({"surface_id": "surface_test"});
        assert!(ensure_exact_payload_fields(&exact, &["surface_id"]).is_ok());
        for extra in [
            json!({"surface_id": "surface_test", "application": "Finder"}),
            json!({"surface_id": "surface_test", "pid": 42}),
            json!({"surface_id": "surface_test", "path": "/Applications/Finder.app"}),
            json!({"surface_id": "surface_test", "command": "open -a Finder"}),
        ] {
            assert!(ensure_exact_payload_fields(&extra, &["surface_id"]).is_err());
        }
    }

    #[test]
    fn computer_control_payload_rejects_semantic_extra_fields() {
        let exact =
            json!({"surface_id": "surface_test", "element_id": "element_test", "action": "press"});
        assert!(
            ensure_exact_payload_fields(&exact, &["surface_id", "element_id", "action"]).is_ok()
        );
        let extra = json!({"surface_id": "surface_test", "element_id": "element_test", "action": "press", "script": "ignored"});
        assert!(
            ensure_exact_payload_fields(&extra, &["surface_id", "element_id", "action"]).is_err()
        );
    }

    #[test]
    fn computer_text_input_payload_and_utf8_bounds_are_closed() {
        let exact =
            json!({"surface_id": "surface_test", "element_id": "element_test", "text": "你好🙂"});
        assert!(ensure_exact_payload_fields(&exact, &["surface_id", "element_id", "text"]).is_ok());
        let extra = json!({"surface_id": "surface_test", "element_id": "element_test", "text": "hello", "action": "focus"});
        assert!(
            ensure_exact_payload_fields(&extra, &["surface_id", "element_id", "text"]).is_err()
        );

        assert_eq!(validate_input_text("你好🙂").unwrap(), "你好🙂".len());
        assert!(validate_input_text("").is_err());
        assert!(validate_input_text("a\0b").is_err());
        assert_eq!(
            validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES)).unwrap(),
            MAX_INPUT_TEXT_BYTES
        );
        assert!(validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES + 1)).is_err());
        assert!(validate_input_text(&"🙂".repeat((MAX_INPUT_TEXT_BYTES / 4) + 1)).is_err());

        let escaped_text = "\u{1}".repeat(MAX_INPUT_TEXT_BYTES);
        let escaped_payload = json!({
            "surface_id": "surface_test",
            "element_id": "element_test",
            "text": escaped_text,
        })
        .to_string();
        assert!(
            escaped_payload.len() > crate::shell_protocol::SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES
        );
        assert!(
            escaped_payload.len()
                <= crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES
        );
        assert_eq!(
            shell_computer_request_payload_max_bytes("computer_input_text"),
            crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES
        );
    }

    #[test]
    fn computer_text_input_target_preflight_fails_closed() {
        let mut text_target = record("surface_test", "target", vec![0]);
        text_target.lineage[1].role = "AXTextArea".to_string();
        assert!(validate_text_input_target(&text_target).is_ok());

        let mut protected = text_target.clone();
        protected.lineage[0].protected = true;
        assert!(validate_text_input_target(&protected)
            .unwrap_err()
            .starts_with("permission_denied:"));

        let mut secure = text_target.clone();
        secure.lineage[1].role = "AXSecureTextField".to_string();
        assert!(validate_text_input_target(&secure)
            .unwrap_err()
            .starts_with("permission_denied:"));

        let mut secure_subrole = text_target.clone();
        secure_subrole.lineage[1].subrole = Some("AXSecureTextField".to_string());
        assert!(validate_text_input_target(&secure_subrole)
            .unwrap_err()
            .starts_with("permission_denied:"));

        let mut secure_ancestor = text_target.clone();
        secure_ancestor.lineage[0].role = "AXSecureTextField".to_string();
        assert!(validate_text_input_target(&secure_ancestor)
            .unwrap_err()
            .starts_with("permission_denied:"));

        let mut search_field = text_target.clone();
        search_field.lineage[1].role = "AXTextField".to_string();
        search_field.lineage[1].subrole = Some("AXSearchField".to_string());
        assert!(validate_text_input_target(&search_field).is_ok());

        let mut unsupported_subrole = text_target.clone();
        unsupported_subrole.lineage[1].role = "AXTextField".to_string();
        unsupported_subrole.lineage[1].subrole = Some("AXUnknownTextSubrole".to_string());
        assert!(validate_text_input_target(&unsupported_subrole)
            .unwrap_err()
            .starts_with("input_failed:"));

        let mut non_text = text_target.clone();
        non_text.lineage[1].role = "AXButton".to_string();
        assert!(validate_text_input_target(&non_text)
            .unwrap_err()
            .starts_with("input_failed:"));

        let mut incomplete = text_target.clone();
        incomplete.lineage.pop();
        assert!(validate_text_input_target(&incomplete)
            .unwrap_err()
            .starts_with("stale_element:"));

        assert!(validate_text_input_preflight(Some(true), Some(true), true, Some("")).is_ok());
        assert!(validate_text_input_preflight(Some(false), Some(true), true, Some("")).is_err());
        assert!(validate_text_input_preflight(Some(true), Some(false), true, Some("")).is_err());
        assert!(validate_text_input_preflight(Some(true), None, true, Some("")).is_err());
        assert!(validate_text_input_preflight(Some(true), Some(true), false, Some("")).is_err());
        assert!(
            validate_text_input_preflight(Some(true), Some(true), true, Some("existing")).is_err()
        );
        assert!(validate_text_input_preflight(Some(true), Some(true), true, None).is_err());
    }

    #[test]
    fn computer_text_input_rejects_stale_lineage_fingerprint() {
        let expected = fingerprint("target");
        let mut changed = expected.clone();
        changed.title = Some("changed".to_string());
        assert!(ensure_correlated_fingerprint(&expected, &changed, false)
            .unwrap_err()
            .starts_with("stale_element:"));
        assert!(ensure_correlated_fingerprint(&expected, &changed, true)
            .unwrap_err()
            .contains("ancestor"));
    }
}

pub(crate) fn is_computer_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "computer_list_windows"
            | "computer_snapshot"
            | "computer_accessibility_status"
            | "computer_accessibility_tree"
            | "computer_activate_window"
            | "computer_control"
            | "computer_input_text"
    )
}

fn ensure_exact_payload_fields(payload: &Value, expected: &[&str]) -> Result<(), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "invalid_request: computer payload must be an object".to_string())?;
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        return Err("invalid_request: computer payload contains unsupported fields".to_string());
    }
    Ok(())
}

pub(crate) fn handle_computer_request(request: &ShellAgentShellRequest) -> CommandResult {
    let start = Instant::now();
    let payload_max_bytes = shell_computer_request_payload_max_bytes(&request.kind);
    let payload = match request.stdin.as_deref() {
        Some(payload) if payload.len() <= payload_max_bytes && !payload.contains('\0') => {
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
        "computer_activate_window" => ensure_exact_payload_fields(&payload, &["surface_id"])
            .and_then(|()| {
                payload
                    .get("surface_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: surface_id is required".to_string())
            })
            .and_then(|surface_id| ComputerObserver::global().activate_window(surface_id)),
        "computer_control" => {
            ensure_exact_payload_fields(&payload, &["surface_id", "element_id", "action"]).and_then(
                |()| {
                    let surface_id = payload
                        .get("surface_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: surface_id is required".to_string());
                    let element_id = payload
                        .get("element_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: element_id is required".to_string());
                    let action = payload
                        .get("action")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: action is required".to_string())
                        .and_then(ComputerAction::parse);
                    surface_id.and_then(|surface_id| {
                        element_id.and_then(|element_id| {
                            action.and_then(|action| {
                                ComputerObserver::global().control(surface_id, element_id, action)
                            })
                        })
                    })
                },
            )
        }
        "computer_input_text" => {
            ensure_exact_payload_fields(&payload, &["surface_id", "element_id", "text"]).and_then(
                |()| {
                    let surface_id = payload
                        .get("surface_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: surface_id is required".to_string());
                    let element_id = payload
                        .get("element_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: element_id is required".to_string());
                    let text = payload
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: text is required".to_string());
                    surface_id.and_then(|surface_id| {
                        element_id.and_then(|element_id| {
                            text.and_then(|text| {
                                ComputerObserver::global().input_text(surface_id, element_id, text)
                            })
                        })
                    })
                },
            )
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
    use super::{
        AccessibilityTreeResult, ComputerAction, ElementRecord, PlatformWindow, SurfaceRecord,
    };

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
    ) -> Result<AccessibilityTreeResult, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn activate_window(
        _surface_id: &str,
        _surface: &SurfaceRecord,
    ) -> Result<serde_json::Value, String> {
        Err(
            "unsupported_platform: computer window activation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn control(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
        _action: ComputerAction,
    ) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: computer control is unavailable on this platform".to_string())
    }

    pub(super) fn input_text(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
        _text: &str,
    ) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: computer text input is unavailable on this platform".to_string())
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

    #[test]
    fn computer_text_input_platform_is_unsupported_off_macos() {
        let surface = SurfaceRecord {
            native_id: 1,
            pid: 1,
            identity_hash: [0; 32],
            application: "test".to_string(),
            title: "test".to_string(),
            width: 1,
            height: 1,
        };
        let fingerprint = ElementFingerprint {
            role: "AXTextField".to_string(),
            subrole: None,
            identifier: Some("field".to_string()),
            title: None,
            description: None,
            placeholder: None,
            protected: false,
        };
        let element = ElementRecord {
            surface_id: "surface_test".to_string(),
            path: Vec::new(),
            lineage: vec![fingerprint],
        };
        let error =
            platform::input_text("surface_test", "element_test", &surface, &element, "hello")
                .unwrap_err();
        assert!(error.starts_with("unsupported_platform:"), "{error}");
    }
}

#[cfg(any(target_os = "macos", windows))]
mod platform {
    use super::{
        bounded_text, ensure_raw_capture_bound, AccessibilityTreeResult, ComputerAction,
        ElementRecord, PlatformWindow, SurfaceRecord,
    };
    #[cfg(target_os = "macos")]
    use super::{
        ensure_correlated_fingerprint, select_exact_ax_window_index, validate_input_text,
        validate_text_input_preflight, validate_text_input_target, AxObservationDeadline,
        ElementFingerprint,
    };
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
    const MAX_AX_ACTION_NAMES: usize = 64;

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
    fn control_attempt_error(operation: &str, error: AXError) -> String {
        if error == AXError::APIDisabled {
            "permission_denied: macOS Accessibility permission is not granted".to_string()
        } else if matches!(
            error,
            AXError::IllegalArgument
                | AXError::InvalidUIElement
                | AXError::AttributeUnsupported
                | AXError::ActionUnsupported
                | AXError::NotImplemented
        ) {
            format!(
                "control_failed: {operation} was rejected with AXError({})",
                error.0
            )
        } else {
            format!(
                "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
                error.0
            )
        }
    }

    #[cfg(target_os = "macos")]
    fn window_activation_attempt_error(
        operation: &str,
        error: AXError,
        prior_effect_succeeded: bool,
    ) -> String {
        if prior_effect_succeeded {
            format!(
                "outcome_unknown: {operation} returned AXError({}) after application activation had already succeeded",
                error.0
            )
        } else {
            control_attempt_error(operation, error)
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    mod window_activation_tests {
        use super::*;

        #[test]
        fn partial_window_activation_failure_is_always_outcome_unknown() {
            let partial = window_activation_attempt_error(
                "AXUIElementPerformAction(AXRaise)",
                AXError::ActionUnsupported,
                true,
            );
            assert!(partial.starts_with("outcome_unknown:"), "{partial}");

            let not_started = window_activation_attempt_error(
                "AXUIElementPerformAction(AXRaise)",
                AXError::ActionUnsupported,
                false,
            );
            assert!(not_started.starts_with("control_failed:"), "{not_started}");
        }
    }

    #[cfg(target_os = "macos")]
    fn text_input_attempt_error(error: AXError) -> String {
        if error == AXError::APIDisabled {
            "permission_denied: macOS Accessibility permission is not granted".to_string()
        } else if matches!(
            error,
            AXError::IllegalArgument
                | AXError::InvalidUIElement
                | AXError::AttributeUnsupported
                | AXError::NotImplemented
        ) {
            format!(
                "input_failed: AXUIElementSetAttributeValue(AXValue) was rejected with AXError({})",
                error.0
            )
        } else {
            format!(
                "outcome_unknown: AXUIElementSetAttributeValue(AXValue) returned AXError({}) after the native text write was attempted",
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
    fn element_fingerprint(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        inherited_protected: bool,
    ) -> Result<ElementFingerprint, String> {
        let role = optional_ax_string(deadline, element, "AXRole")?.ok_or_else(|| {
            "accessibility_failed: AX element is missing a string role".to_string()
        })?;
        let protected = inherited_protected
            || optional_ax_bool(deadline, element, "AXProtectedContent")?.unwrap_or(false);
        Ok(ElementFingerprint {
            role,
            subrole: optional_ax_string(deadline, element, "AXSubrole")?,
            identifier: optional_ax_string(deadline, element, "AXIdentifier")?,
            title: if protected {
                None
            } else {
                optional_ax_string(deadline, element, "AXTitle")?
            },
            description: if protected {
                None
            } else {
                optional_ax_string(deadline, element, "AXDescription")?
            },
            placeholder: if protected {
                None
            } else {
                optional_ax_string(deadline, element, "AXPlaceholderValue")?
            },
            protected,
        })
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
    fn ax_supports_action(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        expected_action: &'static str,
    ) -> Result<bool, String> {
        let mut raw: *const CFArray = std::ptr::null();
        prepare_ax_call(deadline, element)?;
        let error = unsafe { element.copy_action_names(NonNull::from(&mut raw)) };
        deadline.ensure_remaining()?;
        if error != AXError::Success {
            return Err(accessibility_error("AXUIElementCopyActionNames", error));
        }
        let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
            "accessibility_failed: AX action-name copy succeeded with null value".to_string()
        })?;
        let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
        let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
        if array.len() > MAX_AX_ACTION_NAMES {
            return Err(
                "accessibility_failed: AX action-name list exceeds bounded inspection limit"
                    .to_string(),
            );
        }
        let expected_utf16_len = expected_action.encode_utf16().count();
        for value in array.iter() {
            let action = value.downcast::<CFString>().map_err(|_| {
                "accessibility_failed: AX action-name array contained a non-string value"
                    .to_string()
            })?;
            let action_len = usize::try_from(action.length()).map_err(|_| {
                "accessibility_failed: AX action name has invalid string length".to_string()
            })?;
            if action_len == expected_utf16_len && action.to_string() == expected_action {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(target_os = "macos")]
    fn ax_attribute_settable(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute_name: &'static str,
    ) -> Result<bool, String> {
        let attribute = CFString::from_static_str(attribute_name);
        let mut settable = 0u8;
        prepare_ax_call(deadline, element)?;
        let error =
            unsafe { element.is_attribute_settable(&attribute, NonNull::from(&mut settable)) };
        deadline.ensure_remaining()?;
        match error {
            AXError::Success => Ok(settable != 0),
            AXError::AttributeUnsupported => Ok(false),
            error => Err(accessibility_error("AXUIElementIsAttributeSettable", error)),
        }
    }

    #[cfg(target_os = "macos")]
    fn ax_element_at(
        deadline: &AxObservationDeadline,
        element: &AXUIElement,
        attribute_name: &'static str,
        index: usize,
    ) -> Result<CFRetained<AXUIElement>, String> {
        let attribute = CFString::from_static_str(attribute_name);
        let index = CFIndex::try_from(index)
            .map_err(|_| "stale_element: AX child index exceeds CFIndex".to_string())?;
        let mut raw: *const CFArray = std::ptr::null();
        prepare_ax_call(deadline, element)?;
        let error =
            unsafe { element.copy_attribute_values(&attribute, index, 1, NonNull::from(&mut raw)) };
        deadline.ensure_remaining()?;
        if error != AXError::Success {
            return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
        }
        let raw = NonNull::new(raw.cast_mut())
            .ok_or_else(|| "stale_element: AX child lookup returned null".to_string())?;
        let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
        let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
        if array.len() != 1 {
            return Err("stale_element: AX child path no longer resolves exactly".to_string());
        }
        array
            .iter()
            .next()
            .expect("single AX child")
            .downcast::<AXUIElement>()
            .map_err(|_| "stale_element: AX child path resolved to a non-element value".to_string())
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
    ) -> Result<AccessibilityTreeResult, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        let deadline = AxObservationDeadline::new();
        let root = exact_ax_window(surface, &deadline)?;
        let mut queue = VecDeque::from([(
            root,
            None::<String>,
            0usize,
            Vec::<usize>::new(),
            Vec::<ElementFingerprint>::new(),
            false,
        )]);
        let mut nodes = Vec::with_capacity(max_nodes.min(64));
        let mut elements = Vec::with_capacity(max_nodes.min(64));
        let mut truncated = false;
        while let Some((
            element,
            parent_element_id,
            depth,
            path,
            mut lineage,
            inherited_protected,
        )) = queue.pop_front()
        {
            deadline.ensure_remaining()?;
            if nodes.len() >= max_nodes {
                truncated = true;
                break;
            }
            let element_id = format!("element_{}", Uuid::new_v4().simple());
            let fingerprint = element_fingerprint(&deadline, &element, inherited_protected)?;
            let role = fingerprint.role.clone();
            let subrole = fingerprint.subrole.clone();
            let title = fingerprint.title.clone();
            let description = fingerprint.description.clone();
            let placeholder = fingerprint.placeholder.clone();
            let protected = fingerprint.protected;
            lineage.push(fingerprint);
            let sensitive = role == "AXSecureTextField"
                || subrole
                    .as_deref()
                    .is_some_and(|value| value.contains("Secure"));
            let value = if sensitive || protected {
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
                for (index, child) in ax_elements(&deadline, &element, "AXChildren", take)?
                    .into_iter()
                    .enumerate()
                {
                    let mut child_path = path.clone();
                    child_path.push(index);
                    queue.push_back((
                        child,
                        Some(element_id.clone()),
                        depth + 1,
                        child_path,
                        lineage.clone(),
                        protected,
                    ));
                }
            } else if child_count > 0 {
                truncated = true;
            }
            elements.push((
                element_id.clone(),
                ElementRecord {
                    surface_id: surface_id.to_string(),
                    path,
                    lineage,
                },
            ));
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
        Ok(AccessibilityTreeResult {
            output: json!({
                "platform": "macos",
                "surface_id": surface_id,
                "nodes": nodes,
                "node_count": node_count,
                "truncated": truncated,
                "max_depth": max_depth,
                "max_nodes": max_nodes,
            }),
            elements,
        })
    }

    #[cfg(target_os = "macos")]
    fn resolve_correlated_element(
        surface: &SurfaceRecord,
        element: &ElementRecord,
        deadline: &AxObservationDeadline,
    ) -> Result<CFRetained<AXUIElement>, String> {
        if element.lineage.len() != element.path.len() + 1 {
            return Err("stale_element: AX element correlation lineage is incomplete".to_string());
        }
        let mut current = exact_ax_window(surface, deadline)?;
        let current_root_fingerprint = element_fingerprint(deadline, &current, false)?;
        ensure_correlated_fingerprint(&element.lineage[0], &current_root_fingerprint, true)?;
        for (depth, &index) in element.path.iter().enumerate() {
            let child_count = ax_array_count(deadline, &current, "AXChildren")?;
            if index >= child_count {
                return Err("stale_element: AX child path no longer exists".to_string());
            }
            current = ax_element_at(deadline, &current, "AXChildren", index)?;
            let current_fingerprint =
                element_fingerprint(deadline, &current, element.lineage[depth].protected)?;
            ensure_correlated_fingerprint(
                &element.lineage[depth + 1],
                &current_fingerprint,
                false,
            )?;
        }
        Ok(current)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn activate_window(
        surface_id: &str,
        surface: &SurfaceRecord,
    ) -> Result<Value, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }

        // Re-resolve the native surface and exact AX window immediately before
        // any effect so an opaque stale surface cannot drift to another window.
        let deadline = AxObservationDeadline::new();
        let window = exact_ax_window(surface, &deadline)?;
        let application = unsafe { AXUIElement::new_application(surface.pid as _) };
        let frontmost = optional_ax_bool(&deadline, &application, "AXFrontmost")?;
        if frontmost != Some(true)
            && !ax_attribute_settable(&deadline, &application, "AXFrontmost")?
        {
            return Err(
                "control_failed: AX application does not allow AXFrontmost to be set".to_string(),
            );
        }
        if !ax_supports_action(&deadline, &window, "AXRaise")? {
            return Err("control_failed: exact AX window does not support AXRaise".to_string());
        }

        // Prepare both native call sites before the first mutation. After the
        // application becomes frontmost, any later failure is a partial effect.
        prepare_ax_call(&deadline, &application)?;
        prepare_ax_call(&deadline, &window)?;
        let mut application_activated = false;
        if frontmost != Some(true) {
            let error = unsafe {
                application.set_attribute_value(
                    &CFString::from_static_str("AXFrontmost"),
                    CFBoolean::new(true),
                )
            };
            if error != AXError::Success {
                return Err(window_activation_attempt_error(
                    "AXUIElementSetAttributeValue(AXFrontmost)",
                    error,
                    false,
                ));
            }
            application_activated = true;
        }

        let error = unsafe { window.perform_action(&CFString::from_static_str("AXRaise")) };
        if error != AXError::Success {
            return Err(window_activation_attempt_error(
                "AXUIElementPerformAction(AXRaise)",
                error,
                application_activated,
            ));
        }
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "success": true,
        }))
    }

    #[cfg(target_os = "macos")]
    pub(super) fn control(
        surface_id: &str,
        element_id: &str,
        surface: &SurfaceRecord,
        element: &ElementRecord,
        action: ComputerAction,
    ) -> Result<Value, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        let target_fingerprint = element.target_fingerprint().ok_or_else(|| {
            "stale_element: AX element correlation lineage is incomplete".to_string()
        })?;
        if element.contains_protected_content() {
            return Err(
                "permission_denied: macOS Accessibility protected content cannot be controlled"
                    .to_string(),
            );
        }
        if !target_fingerprint.has_positive_evidence() {
            return Err(
                "stale_element: AX element lacks positive correlation evidence for control"
                    .to_string(),
            );
        }
        let deadline = AxObservationDeadline::new();
        let current = resolve_correlated_element(surface, element, &deadline)?;

        match action {
            ComputerAction::Press if !ax_supports_action(&deadline, &current, "AXPress")? => {
                return Err(
                    "control_failed: AX element does not support the AXPress action".to_string(),
                );
            }
            ComputerAction::Focus if !ax_attribute_settable(&deadline, &current, "AXFocused")? => {
                return Err(
                    "control_failed: AX element does not allow AXFocused to be set".to_string(),
                );
            }
            _ => {}
        }

        prepare_ax_call(&deadline, &current)?;
        let error = match action {
            ComputerAction::Press => unsafe {
                current.perform_action(&CFString::from_static_str("AXPress"))
            },
            ComputerAction::Focus => unsafe {
                current.set_attribute_value(
                    &CFString::from_static_str("AXFocused"),
                    CFBoolean::new(true),
                )
            },
        };
        if error != AXError::Success {
            return Err(control_attempt_error(
                match action {
                    ComputerAction::Press => "AXUIElementPerformAction(AXPress)",
                    ComputerAction::Focus => "AXUIElementSetAttributeValue(AXFocused)",
                },
                error,
            ));
        }
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "element_id": element_id,
            "action": action.as_str(),
            "success": true,
        }))
    }

    #[cfg(target_os = "macos")]
    pub(super) fn input_text(
        surface_id: &str,
        element_id: &str,
        surface: &SurfaceRecord,
        element: &ElementRecord,
        text: &str,
    ) -> Result<Value, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        let text_bytes = validate_input_text(text)?;
        validate_text_input_target(element)?;

        let deadline = AxObservationDeadline::new();
        let current = resolve_correlated_element(surface, element, &deadline)?;
        let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
        let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
        let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
        // This is deliberately the final read before the effect. The helper may
        // truncate a non-empty string for bounded observation, but emptiness is
        // preserved exactly and caller text is never transformed or normalized.
        let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
        validate_text_input_preflight(enabled, focused, value_settable, current_value.as_deref())?;

        let text_value = CFString::from_str(text);
        prepare_ax_call(&deadline, &current)?;
        let error = unsafe {
            current.set_attribute_value(&CFString::from_static_str("AXValue"), &text_value)
        };
        if error != AXError::Success {
            return Err(text_input_attempt_error(error));
        }
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "element_id": element_id,
            "text_bytes": text_bytes,
            "success": true,
        }))
    }

    #[cfg(windows)]
    pub(super) fn accessibility_tree(
        _surface_id: &str,
        _surface: &SurfaceRecord,
        _max_depth: usize,
        _max_nodes: usize,
    ) -> Result<AccessibilityTreeResult, String> {
        Err(
            "unsupported_platform: computer accessibility observation is unavailable on this platform"
                .to_string(),
        )
    }

    #[cfg(windows)]
    pub(super) fn activate_window(
        _surface_id: &str,
        _surface: &SurfaceRecord,
    ) -> Result<Value, String> {
        Err(
            "unsupported_platform: computer window activation is unavailable on this platform"
                .to_string(),
        )
    }

    #[cfg(windows)]
    pub(super) fn control(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
        _action: ComputerAction,
    ) -> Result<Value, String> {
        Err("unsupported_platform: computer control is unavailable on this platform".to_string())
    }

    #[cfg(windows)]
    pub(super) fn input_text(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
        _text: &str,
    ) -> Result<Value, String> {
        Err("unsupported_platform: computer text input is unavailable on this platform".to_string())
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
        let tree = platform::accessibility_tree("surface_live", &record, 3, 64)
            .expect("read bounded live accessibility tree");
        let output = tree.output;
        assert_eq!(output["platform"], "macos");
        assert!(output["node_count"].as_u64().unwrap_or(0) > 0);
        for node in output["nodes"].as_array().expect("nodes array") {
            assert!(node["role"].as_str().is_some_and(|role| !role.is_empty()));
        }
        true
    }

    fn live_focus_control_smoke(application_matches: impl Fn(&str) -> bool) -> bool {
        let candidates = platform::list_windows(MAX_WINDOWS).expect("list live macOS windows");
        let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| application_matches(&candidate.application))
        else {
            return false;
        };
        let record = surface_record(candidate);
        let surface_id = "surface_control_live";
        let tree = platform::accessibility_tree(surface_id, &record, 6, 128)
            .expect("read bounded accessibility tree for live focus control");
        let candidate_roles = [
            "AXTextField",
            "AXTextArea",
            "AXComboBox",
            "AXButton",
            "AXCheckBox",
            "AXRadioButton",
            "AXLink",
        ];
        for (element_id, element) in tree
            .elements
            .into_iter()
            .filter(|(_, element)| {
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.has_positive_evidence()
                        && !fingerprint.protected
                        && candidate_roles.contains(&fingerprint.role.as_str())
                })
            })
            .take(16)
        {
            match platform::control(
                surface_id,
                &element_id,
                &record,
                &element,
                ComputerAction::Focus,
            ) {
                Ok(output) => {
                    assert_eq!(output["action"], "focus");
                    assert_eq!(output["success"], true);
                    return true;
                }
                Err(error) if error.starts_with("control_failed:") => continue,
                Err(error) => {
                    panic!("live focus control failed with uncertain/error state: {error}")
                }
            }
        }
        false
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
    #[ignore = "requires live Microsoft Edge window and macOS Accessibility permission"]
    fn computer_macos_control_focus_edge_live_smoke() {
        assert!(
            live_focus_control_smoke(|application| {
                application.to_ascii_lowercase().contains("microsoft edge")
                    || application.to_ascii_lowercase() == "edge"
            }),
            "Microsoft Edge must expose a bounded focusable AX element for this live smoke"
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
