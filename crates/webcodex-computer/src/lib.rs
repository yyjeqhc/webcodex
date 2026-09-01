use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
#[cfg(any(test, target_os = "macos"))]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const MAX_WINDOWS: usize = 64;
pub const MAX_APPLICATIONS: usize = 64;
const MAX_APPLICATION_SCAN: usize = 1024;
pub const MAX_DISPLAYS: usize = 16;
const MAX_APPLICATION_ID_BYTES: usize = 128;
const MAX_DISPLAY_ID_BYTES: usize = 128;
const MAX_DISPLAY_SNAPSHOT_BINDINGS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_ELEMENT_REGISTRY: usize = 1024;
const MAX_INPUT_TEXT_BYTES: usize = 2048;
const MAX_CLIPBOARD_TEXT_BYTES: usize = 16 * 1024;
const MAX_CLIPBOARD_NATIVE_STORAGE_BYTES: usize = 64 * 1024;
const COMPUTER_KEY_INPUT_KEYS: &[&str] = &[
    "enter",
    "escape",
    "tab",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "page_up",
    "page_down",
    "home",
    "end",
];
const COMPUTER_KEY_INPUT_MODIFIERS: &[&str] = &["shift", "control", "option", "command"];

fn valid_application_id(application_id: &str) -> bool {
    let Some(suffix) = application_id.strip_prefix("application_") else {
        return false;
    };
    application_id.len() <= MAX_APPLICATION_ID_BYTES
        && suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_display_id(display_id: &str) -> bool {
    let Some(suffix) = display_id.strip_prefix("display_") else {
        return false;
    };
    display_id.len() <= MAX_DISPLAY_ID_BYTES
        && suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_key_modifiers(modifiers: &[String]) -> Result<(), String> {
    if modifiers.len() > COMPUTER_KEY_INPUT_MODIFIERS.len() {
        return Err("invalid_request: computer key input has too many modifiers".to_string());
    }
    for (index, modifier) in modifiers.iter().enumerate() {
        if !COMPUTER_KEY_INPUT_MODIFIERS.contains(&modifier.as_str())
            || modifiers[..index].contains(modifier)
        {
            return Err(
                "invalid_request: computer key input modifiers are invalid or duplicated"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_key_input(key: &str, modifiers: &[String]) -> Result<(), String> {
    if !COMPUTER_KEY_INPUT_KEYS.contains(&key) {
        return Err("invalid_request: computer key is outside the closed vocabulary".to_string());
    }
    validate_key_modifiers(modifiers)
}
const MAX_ACCESSIBILITY_DEPTH: usize = 8;
const MAX_ACCESSIBILITY_NODES: usize = 256;
pub const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
pub const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
const RGBA_BYTES_PER_PIXEL: u64 = 4;
/// Pre-capture ceiling for the expected complete raw RGBA frame. Standard
/// 8K UHD (7680x4320x4) fits while malformed/extreme dimensions fail closed
/// before xcap is allowed to allocate the native capture image.
const MAX_RAW_CAPTURE_BYTES: u64 = 128 * 1024 * 1024;
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
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    pid: u32,
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    identity_hash: [u8; 32],
    application: String,
    title: String,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerAction {
    Press,
    Focus,
}

impl ComputerAction {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "press" => Ok(Self::Press),
            "focus" => Ok(Self::Focus),
            _ => Err("invalid_request: computer control action must be press or focus".to_string()),
        }
    }

    #[cfg(any(target_os = "macos", windows))]
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
    #[cfg(windows)]
    native_runtime_id: Vec<i32>,
}

impl ElementFingerprint {
    #[cfg(any(test, target_os = "macos", windows))]
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
    #[cfg(any(test, target_os = "macos", windows))]
    fn target_fingerprint(&self) -> Option<&ElementFingerprint> {
        (self.lineage.len() == self.path.len() + 1)
            .then(|| self.lineage.last())
            .flatten()
    }

    #[cfg(any(test, target_os = "macos", windows))]
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedClipboardText {
    utf16: Vec<u16>,
    text_bytes: usize,
    storage_bytes: usize,
}

fn prepare_clipboard_write_text(text: &str) -> Result<PreparedClipboardText, String> {
    let text_bytes = text.len();
    if text_bytes == 0 || text_bytes > MAX_CLIPBOARD_TEXT_BYTES || text.contains('\0') {
        return Err(
            "invalid_request: clipboard text must be non-empty, NUL-free, and within the 16 KiB UTF-8 byte limit"
                .to_string(),
        );
    }
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    let units_with_nul = utf16
        .len()
        .checked_add(1)
        .ok_or_else(|| "invalid_request: clipboard UTF-16 length overflow".to_string())?;
    let storage_bytes = units_with_nul
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| "invalid_request: clipboard native storage size overflow".to_string())?;
    if storage_bytes > MAX_CLIPBOARD_NATIVE_STORAGE_BYTES {
        return Err("invalid_request: clipboard native storage exceeds bound".to_string());
    }
    utf16.push(0);
    Ok(PreparedClipboardText {
        utf16,
        text_bytes,
        storage_bytes,
    })
}

#[cfg(any(test, windows))]
fn clipboard_read_result_from_utf16(
    storage: Option<&[u16]>,
    native_storage_bytes: usize,
) -> Result<Value, String> {
    let Some(storage) = storage else {
        return clipboard_read_result("windows", None);
    };
    if native_storage_bytes == 0 {
        return Err("clipboard_malformed: clipboard Unicode storage is empty".to_string());
    }
    if native_storage_bytes > MAX_CLIPBOARD_NATIVE_STORAGE_BYTES {
        return Err(
            "clipboard_too_large: clipboard Unicode storage exceeds the bounded native range"
                .to_string(),
        );
    }
    if native_storage_bytes % std::mem::size_of::<u16>() != 0 {
        return Err(
            "clipboard_malformed: clipboard Unicode storage has odd byte length".to_string(),
        );
    }
    let expected_units = native_storage_bytes / std::mem::size_of::<u16>();
    if storage.len() != expected_units {
        return Err(
            "clipboard_malformed: clipboard Unicode storage length is inconsistent".to_string(),
        );
    }
    let end = storage.iter().position(|unit| *unit == 0).ok_or_else(|| {
        "clipboard_malformed: clipboard Unicode text is not NUL terminated within bounded storage"
            .to_string()
    })?;
    let text = String::from_utf16(&storage[..end])
        .map_err(|_| "clipboard_malformed: clipboard Unicode text is invalid UTF-16".to_string())?;
    clipboard_read_result("windows", Some(&text))
}

#[cfg(any(test, target_os = "macos", windows))]
fn clipboard_read_result(platform: &str, text: Option<&str>) -> Result<Value, String> {
    let Some(text) = text else {
        return Ok(json!({
            "platform": platform,
            "available": false,
            "text_bytes": 0,
        }));
    };
    let text_bytes = text.len();
    if text_bytes > MAX_CLIPBOARD_TEXT_BYTES {
        return Err(
            "clipboard_too_large: clipboard UTF-8 text exceeds the 16 KiB bound".to_string(),
        );
    }
    if text.contains('\0') {
        return Err("clipboard_malformed: clipboard text contains NUL".to_string());
    }
    Ok(json!({
        "platform": platform,
        "available": true,
        "text": text,
        "text_bytes": text_bytes,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardWriteEffectState {
    #[cfg(any(test, windows))]
    NotStarted,
    OutcomeUnknown,
    Success,
}

#[cfg(any(test, windows))]
fn run_clipboard_write_effect_steps(
    empty_clipboard: impl FnOnce() -> bool,
    set_clipboard_text: impl FnOnce() -> bool,
    close_clipboard: impl FnOnce() -> bool,
) -> ClipboardWriteEffectState {
    if !empty_clipboard() {
        let _ = close_clipboard();
        return ClipboardWriteEffectState::NotStarted;
    }
    let set_succeeded = set_clipboard_text();
    let close_succeeded = close_clipboard();
    if !set_succeeded || !close_succeeded {
        ClipboardWriteEffectState::OutcomeUnknown
    } else {
        ClipboardWriteEffectState::Success
    }
}

#[cfg(any(test, target_os = "macos"))]
fn run_macos_clipboard_write_effect_steps(
    clear_contents: impl FnOnce() -> isize,
    set_clipboard_text: impl FnOnce() -> bool,
    current_change_count: impl FnOnce() -> isize,
) -> ClipboardWriteEffectState {
    let ownership_change_count = clear_contents();
    let set_succeeded = set_clipboard_text();
    let ownership_retained = current_change_count() == ownership_change_count;
    if set_succeeded && ownership_retained {
        ClipboardWriteEffectState::Success
    } else {
        ClipboardWriteEffectState::OutcomeUnknown
    }
}

#[cfg(any(test, windows))]
fn finish_clipboard_read<T>(
    read_result: Result<T, String>,
    close_clipboard: impl FnOnce() -> bool,
) -> Result<T, String> {
    if !close_clipboard() {
        Err("clipboard_failed: CloseClipboard failed after bounded read".to_string())
    } else {
        read_result
    }
}

#[cfg(any(test, target_os = "macos"))]
fn is_secure_text_fingerprint(fingerprint: &ElementFingerprint) -> bool {
    fingerprint.role == "AXSecureTextField"
        || fingerprint
            .subrole
            .as_deref()
            .is_some_and(|subrole| subrole.contains("Secure"))
}

#[cfg(any(test, target_os = "macos", windows))]
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
fn validate_element_state_target(element: &ElementRecord) -> Result<&ElementFingerprint, String> {
    let target = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for state observation"
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
    surface_generations: HashMap<String, u32>,
}

impl ElementRegistry {
    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.surface_generations.clear();
    }

    fn get(&self, element_id: &str) -> Option<ElementRecord> {
        self.entries.get(element_id).cloned()
    }

    fn get_with_generation(&self, element_id: &str) -> Option<(ElementRecord, u32)> {
        let record = self.entries.get(element_id)?.clone();
        let generation = *self.surface_generations.get(&record.surface_id)?;
        Some((record, generation))
    }

    fn replace_surface(
        &mut self,
        surface_id: &str,
        elements: Vec<(String, ElementRecord)>,
    ) -> Result<u32, String> {
        // Compute the next generation before mutating the registry so an exhausted
        // counter fails without invalidating the currently usable handles.
        let generation = self
            .surface_generations
            .get(surface_id)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "computer_state_error: observation generation exhausted".to_string())?;
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
        self.surface_generations
            .insert(surface_id.to_string(), generation);
        Ok(generation)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlatformApplication {
    display_name: String,
    native_identity: Vec<u8>,
}

fn application_candidate_order(
    left: &PlatformApplication,
    right: &PlatformApplication,
) -> std::cmp::Ordering {
    left.display_name
        .to_lowercase()
        .cmp(&right.display_name.to_lowercase())
        .then_with(|| left.display_name.cmp(&right.display_name))
        .then_with(|| left.native_identity.cmp(&right.native_identity))
}

fn sort_application_candidates(applications: &mut [PlatformApplication]) {
    applications.sort_by(application_candidate_order);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApplicationRecord {
    display_name: String,
    native_identity: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlatformDisplay {
    native_identity: Vec<u8>,
    width: u32,
    height: u32,
    primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayRecord {
    native_identity: Vec<u8>,
    width: u32,
    height: u32,
    primary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    Move,
    Click,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointerPlan {
    global_x: i32,
    global_y: i32,
    normalized_x: i32,
    normalized_y: i32,
}

#[cfg(target_os = "macos")]
struct PointerPlan {
    display: DisplayRecord,
    native_display_id: u32,
    bounds_origin_x: f64,
    bounds_origin_y: f64,
    bounds_width: f64,
    bounds_height: f64,
    rotation_degrees: f64,
    target_x: f64,
    target_y: f64,
    _source: objc2_core_foundation::CFRetained<objc2_core_graphics::CGEventSource>,
    move_event: objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>,
    click_down_event: Option<objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>>,
    click_up_event: Option<objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>>,
}

#[cfg(not(any(target_os = "macos", windows)))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointerPlan;

fn pointer_output_platform() -> &'static str {
    #[cfg(windows)]
    {
        return "windows";
    }
    #[cfg(target_os = "macos")]
    {
        return "macos";
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "unsupported"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplaySnapshotBinding {
    generation: u32,
    display_id: String,
    native_identity: Vec<u8>,
    source_width: u32,
    source_height: u32,
    spent: bool,
}

#[derive(Default)]
struct DisplaySnapshotRegistry {
    next_generation: u32,
    bindings: VecDeque<DisplaySnapshotBinding>,
}

impl DisplaySnapshotRegistry {
    fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    fn bind(&mut self, display_id: &str, display: &DisplayRecord) -> Result<u32, String> {
        let generation = self.next_generation.checked_add(1).ok_or_else(|| {
            "computer_state_error: display snapshot generation exhausted".to_string()
        })?;
        self.next_generation = generation;
        self.bindings.push_back(DisplaySnapshotBinding {
            generation,
            display_id: display_id.to_string(),
            native_identity: display.native_identity.clone(),
            source_width: display.width,
            source_height: display.height,
            spent: false,
        });
        while self.bindings.len() > MAX_DISPLAY_SNAPSHOT_BINDINGS {
            self.bindings.pop_front();
        }
        Ok(generation)
    }

    fn pointer_binding_index(
        &self,
        display_id: &str,
        generation: u32,
        display: &DisplayRecord,
    ) -> Result<usize, String> {
        let latest_generation = self
            .bindings
            .iter()
            .rev()
            .find(|binding| binding.display_id == display_id)
            .map(|binding| binding.generation)
            .ok_or_else(|| {
                "stale_snapshot_generation: no successful full-display snapshot is bound to display_id"
                    .to_string()
            })?;
        if latest_generation != generation {
            return Err(
                "stale_snapshot_generation: snapshot_generation is not the latest successful snapshot for display_id"
                    .to_string(),
            );
        }
        let index = self
            .bindings
            .iter()
            .position(|binding| binding.generation == generation)
            .ok_or_else(|| {
                "stale_snapshot_generation: snapshot_generation is unknown or evicted".to_string()
            })?;
        let binding = &self.bindings[index];
        if binding.display_id != display_id {
            return Err(
                "stale_snapshot_generation: snapshot_generation belongs to a different display_id"
                    .to_string(),
            );
        }
        if binding.native_identity != display.native_identity
            || binding.source_width != display.width
            || binding.source_height != display.height
        {
            return Err(
                "stale_display: snapshot generation display identity or source geometry changed"
                    .to_string(),
            );
        }
        if binding.spent {
            return Err(
                "stale_snapshot_generation: snapshot_generation was already consumed by a pointer effect"
                    .to_string(),
            );
        }
        Ok(index)
    }

    fn validate_pointer(
        &self,
        display_id: &str,
        generation: u32,
        display: &DisplayRecord,
    ) -> Result<(), String> {
        self.pointer_binding_index(display_id, generation, display)
            .map(|_| ())
    }

    fn spend_pointer(
        &mut self,
        display_id: &str,
        generation: u32,
        display: &DisplayRecord,
    ) -> Result<(), String> {
        let index = self.pointer_binding_index(display_id, generation, display)?;
        self.bindings[index].spent = true;
        Ok(())
    }
}
fn dispatch_after_spending_pointer_generation(
    snapshots: &mut DisplaySnapshotRegistry,
    display_id: &str,
    generation: u32,
    display: &DisplayRecord,
    dispatch: impl FnOnce(&DisplaySnapshotRegistry) -> Result<bool, String>,
) -> Result<bool, String> {
    snapshots.spend_pointer(display_id, generation, display)?;
    dispatch(snapshots)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputerConfig {
    pub max_encoded_image_bytes: usize,
}

pub struct ComputerRuntime {
    config: ComputerConfig,
    surfaces: Mutex<HashMap<String, SurfaceRecord>>,
    elements: Mutex<ElementRegistry>,
    applications: Mutex<HashMap<String, ApplicationRecord>>,
    displays: Mutex<HashMap<String, DisplayRecord>>,
    display_snapshots: Mutex<DisplaySnapshotRegistry>,
}

impl ComputerRuntime {
    pub fn new(config: ComputerConfig) -> Self {
        Self {
            config,
            surfaces: Mutex::new(HashMap::new()),
            elements: Mutex::new(ElementRegistry::default()),
            applications: Mutex::new(HashMap::new()),
            displays: Mutex::new(HashMap::new()),
            display_snapshots: Mutex::new(DisplaySnapshotRegistry::default()),
        }
    }

    pub fn read_clipboard(&self) -> Result<Value, String> {
        platform::read_clipboard()
    }

    pub fn write_clipboard(&self, text: &str) -> Result<Value, String> {
        platform::write_clipboard(text)
    }

    pub fn list_windows(&self, limit: usize) -> Result<Value, String> {
        if !(1..=MAX_WINDOWS).contains(&limit) {
            return Err("invalid_request: window discovery limit is invalid".to_string());
        }
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

    fn replace_application_candidates(
        &self,
        candidates: Vec<PlatformApplication>,
        limit: usize,
    ) -> Result<Value, String> {
        if !(1..=MAX_APPLICATIONS).contains(&limit) {
            return Err("invalid_request: application discovery limit is invalid".to_string());
        }
        let truncated = candidates.len() > limit;
        let mut applications = HashMap::new();
        let mut output = Vec::with_capacity(limit.min(candidates.len()));
        for candidate in candidates.into_iter().take(limit) {
            let display_name = bounded_text(&candidate.display_name);
            if display_name.is_empty()
                || display_name.contains('\0')
                || candidate.native_identity.is_empty()
            {
                return Err(
                    "application_failed: native application metadata is invalid".to_string()
                );
            }
            let application_id = format!("application_{}", Uuid::new_v4().simple());
            applications.insert(
                application_id.clone(),
                ApplicationRecord {
                    display_name: display_name.clone(),
                    native_identity: candidate.native_identity,
                },
            );
            output.push(json!({
                "application_id": application_id,
                "display_name": display_name,
            }));
        }
        let count = output.len();
        let mut registry = self
            .applications
            .lock()
            .map_err(|_| "computer_state_error: application registry lock poisoned".to_string())?;
        *registry = applications;
        Ok(json!({"applications": output, "count": count, "truncated": truncated}))
    }

    pub fn list_applications(&self, limit: usize) -> Result<Value, String> {
        if !(1..=MAX_APPLICATIONS).contains(&limit) {
            return Err("invalid_request: application discovery limit is invalid".to_string());
        }
        let candidates = platform::list_applications(MAX_APPLICATION_SCAN)?;
        self.replace_application_candidates(candidates, limit)
    }

    fn replace_display_candidates(
        &self,
        candidates: Vec<PlatformDisplay>,
        limit: usize,
    ) -> Result<Value, String> {
        if !(1..=MAX_DISPLAYS).contains(&limit) {
            return Err("invalid_request: display discovery limit is invalid".to_string());
        }
        let truncated = candidates.len() > limit;
        let mut displays = HashMap::new();
        let mut output = Vec::with_capacity(limit.min(candidates.len()));
        for candidate in candidates.into_iter().take(limit) {
            if candidate.native_identity.is_empty() || candidate.width == 0 || candidate.height == 0
            {
                return Err("display_failed: native display metadata is invalid".to_string());
            }
            let display_id = format!("display_{}", Uuid::new_v4().simple());
            let record = DisplayRecord {
                native_identity: candidate.native_identity,
                width: candidate.width,
                height: candidate.height,
                primary: candidate.primary,
            };
            output.push(json!({
                "display_id": display_id,
                "width": record.width,
                "height": record.height,
                "primary": record.primary,
            }));
            displays.insert(display_id, record);
        }
        let count = output.len();
        let mut display_registry = self
            .displays
            .lock()
            .map_err(|_| "computer_state_error: display registry lock poisoned".to_string())?;
        let mut snapshot_registry = self.display_snapshots.lock().map_err(|_| {
            "computer_state_error: display snapshot registry lock poisoned".to_string()
        })?;
        *display_registry = displays;
        snapshot_registry.clear_bindings();
        Ok(json!({"displays": output, "count": count, "truncated": truncated}))
    }

    pub fn list_displays(&self, limit: usize) -> Result<Value, String> {
        if !(1..=MAX_DISPLAYS).contains(&limit) {
            return Err("invalid_request: display discovery limit is invalid".to_string());
        }
        let candidates = platform::list_displays(MAX_DISPLAYS + 1)?;
        self.replace_display_candidates(candidates, limit)
    }

    pub fn snapshot_display(
        &self,
        display_id: &str,
        max_width: Option<u32>,
        max_height: Option<u32>,
    ) -> Result<Value, String> {
        if !valid_display_id(display_id) {
            return Err("invalid_request: display_id is invalid".to_string());
        }
        if max_width.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
            || max_height.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
        {
            return Err(
                "invalid_request: display snapshot output dimension bound is invalid".to_string(),
            );
        }
        let display_registry = self
            .displays
            .lock()
            .map_err(|_| "computer_state_error: display registry lock poisoned".to_string())?;
        let record = display_registry
            .get(display_id)
            .cloned()
            .ok_or_else(|| "stale_display: unknown or stale display_id".to_string())?;
        ensure_raw_capture_bound(record.width, record.height)?;
        let image = platform::capture_display(&record)?;
        let captured_at_unix_ms = current_unix_ms()?;
        let (image, _full_region) = transform_snapshot_image(
            image,
            record.width,
            record.height,
            None,
            max_width,
            max_height,
        )?;
        let encoded = encode_bounded_jpeg(image, self.config.max_encoded_image_bytes)?;
        let file_bytes = encoded.bytes.len();
        let sha256 = sha256_hex(&encoded.bytes);
        let generation = self
            .display_snapshots
            .lock()
            .map_err(|_| {
                "computer_state_error: display snapshot registry lock poisoned".to_string()
            })?
            .bind(display_id, &record)?;
        drop(display_registry);
        Ok(json!({
            "display_id": display_id,
            "snapshot_generation": generation,
            "source_width": record.width,
            "source_height": record.height,
            "width": encoded.width,
            "height": encoded.height,
            "mime_type": "image/jpeg",
            "file_bytes": file_bytes,
            "sha256": sha256,
            "captured_at_unix_ms": captured_at_unix_ms,
            "content_base64": general_purpose::STANDARD.encode(encoded.bytes),
        }))
    }

    pub fn pointer_effect(
        &self,
        action: PointerAction,
        display_id: &str,
        snapshot_generation: u32,
        x: u32,
        y: u32,
    ) -> Result<Value, String> {
        if !valid_display_id(display_id) || snapshot_generation == 0 {
            return Err(
                "invalid_request: pointer display_id or snapshot_generation is invalid".to_string(),
            );
        }
        let display_registry = self
            .displays
            .lock()
            .map_err(|_| "computer_state_error: display registry lock poisoned".to_string())?;
        let display = display_registry
            .get(display_id)
            .cloned()
            .ok_or_else(|| "stale_display: unknown or stale display_id".to_string())?;
        if x >= display.width || y >= display.height {
            return Err(
                "invalid_request: pointer coordinates are outside snapshot source geometry"
                    .to_string(),
            );
        }
        let mut snapshot_registry = self.display_snapshots.lock().map_err(|_| {
            "computer_state_error: display snapshot registry lock poisoned".to_string()
        })?;
        snapshot_registry.validate_pointer(display_id, snapshot_generation, &display)?;

        // Keep Windows topology metrics, exact native mapping, SendInput, and cursor
        // reconciliation in one per-monitor-v2 physical coordinate context. The guard
        // is established before pointer preflight and remains live across the effect.
        #[cfg(windows)]
        let _pointer_coordinate_context = platform::enter_pointer_coordinate_context()?;

        // All native identity/mapping/shared-input checks occur before the effect boundary.
        let plan = platform::prepare_pointer(&display, x, y, action)?;

        // Crossing this boundary consumes the snapshot generation before the first native
        // pointer effect, even if dispatch subsequently reports definite not_started or an uncertain outcome.
        let result = dispatch_after_spending_pointer_generation(
            &mut snapshot_registry,
            display_id,
            snapshot_generation,
            &display,
            |_| platform::dispatch_pointer(plan, action),
        )?;
        drop(snapshot_registry);
        drop(display_registry);
        Ok(json!({
            "platform": pointer_output_platform(),
            "display_id": display_id,
            "snapshot_generation": snapshot_generation,
            "x": x,
            "y": y,
            "success": result,
        }))
    }
    pub fn launch_application(&self, application_id: &str) -> Result<Value, String> {
        self.with_current_application(application_id, |record| {
            platform::launch_application(application_id, record)
        })
    }

    fn with_current_application<T>(
        &self,
        application_id: &str,
        effect: impl FnOnce(&ApplicationRecord) -> Result<T, String>,
    ) -> Result<T, String> {
        if !valid_application_id(application_id) {
            return Err("invalid_request: application_id is invalid".to_string());
        }
        // Keep the process-local discovery generation fenced through exact native
        // revalidation and dispatch. A concurrent fresh list cannot retire this id
        // between lookup and the native launch attempt.
        let registry = self
            .applications
            .lock()
            .map_err(|_| "computer_state_error: application registry lock poisoned".to_string())?;
        let record = registry
            .get(application_id)
            .ok_or_else(|| "stale_application: unknown or stale application_id".to_string())?;
        let result = effect(record);
        drop(registry);
        result
    }

    pub fn accessibility_status(&self) -> Result<Value, String> {
        platform::accessibility_status()
    }

    pub fn accessibility_tree(
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
        let AccessibilityTreeResult {
            mut output,
            elements,
        } = platform::accessibility_tree(surface_id, &record, max_depth, max_nodes)?;
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
        let Some(object) = output.as_object_mut() else {
            return Err(
                "computer_state_error: Accessibility tree output is not an object".to_string(),
            );
        };
        let mut element_registry = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?;
        let observation_generation = element_registry.replace_surface(surface_id, elements)?;
        object.insert(
            "observation_generation".to_string(),
            json!(observation_generation),
        );
        Ok(output)
    }

    pub fn element_state(&self, surface_id: &str, element_id: &str) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        if !element_id.starts_with("element_")
            || element_id.len() <= "element_".len()
            || element_id.len() > MAX_ELEMENT_ID_BYTES
        {
            return Err("invalid_request: element_id is invalid".to_string());
        }
        // Hold the surface registry guard through native re-resolution. Tree/list
        // observations take the same guard before replacing element generations,
        // so this state read cannot return a handle that was concurrently retired.
        let surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        let record = surface_registry
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        let (element, observation_generation) = self
            .elements
            .lock()
            .map_err(|_| "computer_state_error: element registry lock poisoned".to_string())?
            .get_with_generation(element_id)
            .ok_or_else(|| "stale_element: unknown, evicted, or stale element_id".to_string())?;
        if element.surface_id != surface_id {
            return Err("stale_element: element_id belongs to a different surface".to_string());
        }
        platform::element_state(
            surface_id,
            element_id,
            observation_generation,
            &record,
            &element,
        )
    }

    pub fn activate_window(&self, surface_id: &str) -> Result<Value, String> {
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

    pub fn control(
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

    pub fn scroll_to_element(&self, surface_id: &str, element_id: &str) -> Result<Value, String> {
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
        platform::scroll_to_element(surface_id, element_id, &record, &element)
    }

    pub fn key_input(
        &self,
        surface_id: &str,
        key: &str,
        modifiers: &[String],
    ) -> Result<Value, String> {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return Err("invalid_request: surface_id is invalid".to_string());
        }
        validate_key_input(key, modifiers)?;
        let surface_registry = self
            .surfaces
            .lock()
            .map_err(|_| "computer_state_error: surface registry lock poisoned".to_string())?;
        let record = surface_registry
            .get(surface_id)
            .cloned()
            .ok_or_else(|| "stale_surface: unknown or stale surface_id".to_string())?;
        platform::key_input(surface_id, &record, key, modifiers)
    }

    pub fn input_text(
        &self,
        surface_id: &str,
        element_id: &str,
        text: &str,
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

    pub fn snapshot(
        &self,
        surface_id: &str,
        region: Option<SnapshotRegion>,
        max_width: Option<u32>,
        max_height: Option<u32>,
    ) -> Result<Value, String> {
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
        if max_width.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
            || max_height.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
        {
            return Err("invalid_request: snapshot output dimension bound is invalid".to_string());
        }
        let region = resolve_snapshot_region(record.width, record.height, region)?;
        let image = platform::capture_window(&record)?;
        let captured_at_unix_ms = current_unix_ms()?;
        let (image, region) = transform_snapshot_image(
            image,
            record.width,
            record.height,
            Some(region),
            max_width,
            max_height,
        )?;
        let encoded = encode_bounded_jpeg(image, self.config.max_encoded_image_bytes)?;
        let file_bytes = encoded.bytes.len();
        let sha256 = sha256_hex(&encoded.bytes);
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
            "source_width": record.width,
            "source_height": record.height,
            "region": region,
            "width": encoded.width,
            "height": encoded.height,
            "mime_type": "image/jpeg",
            "file_bytes": file_bytes,
            "sha256": sha256,
            "captured_at_unix_ms": captured_at_unix_ms,
            "content_base64": general_purpose::STANDARD.encode(encoded.bytes),
        }))
    }
}

#[cfg(test)]
mod public_runtime_bounds_tests {
    use super::*;

    fn runtime() -> ComputerRuntime {
        ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: usize::MAX,
        })
    }

    #[test]
    fn discovery_limits_fail_closed_before_native_observation() {
        let runtime = runtime();
        for error in [
            runtime.list_windows(0).unwrap_err(),
            runtime.list_windows(MAX_WINDOWS + 1).unwrap_err(),
        ] {
            assert_eq!(error, "invalid_request: window discovery limit is invalid");
        }
        for error in [
            runtime.list_applications(0).unwrap_err(),
            runtime.list_applications(MAX_APPLICATIONS + 1).unwrap_err(),
        ] {
            assert_eq!(
                error,
                "invalid_request: application discovery limit is invalid"
            );
        }
        for error in [
            runtime.list_displays(0).unwrap_err(),
            runtime.list_displays(MAX_DISPLAYS + 1).unwrap_err(),
        ] {
            assert_eq!(error, "invalid_request: display discovery limit is invalid");
        }
    }
}

struct EncodedImage {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

fn current_unix_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "computer_state_error: system clock is before Unix epoch".to_string())?
        .as_millis();
    let millis = u64::try_from(millis)
        .map_err(|_| "computer_state_error: capture timestamp overflow".to_string())?;
    if millis > 9_007_199_254_740_991 {
        return Err(
            "computer_state_error: capture timestamp exceeds exact JSON integer range".to_string(),
        );
    }
    Ok(millis)
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn resolve_snapshot_region(
    source_width: u32,
    source_height: u32,
    region: Option<SnapshotRegion>,
) -> Result<SnapshotRegion, String> {
    if source_width == 0 || source_height == 0 {
        return Err("capture_failed: revalidated surface has zero dimensions".to_string());
    }
    let region = region.unwrap_or(SnapshotRegion {
        x: 0,
        y: 0,
        width: source_width,
        height: source_height,
    });
    if region.width == 0 || region.height == 0 {
        return Err("invalid_request: snapshot region must have positive dimensions".to_string());
    }
    let right = region
        .x
        .checked_add(region.width)
        .ok_or_else(|| "invalid_request: snapshot region horizontal bound overflow".to_string())?;
    let bottom = region
        .y
        .checked_add(region.height)
        .ok_or_else(|| "invalid_request: snapshot region vertical bound overflow".to_string())?;
    if right > source_width || bottom > source_height {
        return Err(
            "invalid_request: snapshot region must fit fully inside the revalidated surface"
                .to_string(),
        );
    }
    Ok(region)
}

#[cfg(any(test, target_os = "macos", windows))]
fn mapped_crop_bounds(
    region: SnapshotRegion,
    source_width: u32,
    source_height: u32,
    captured_width: u32,
    captured_height: u32,
) -> Result<(u32, u32, u32, u32), String> {
    if captured_width == 0 || captured_height == 0 {
        return Err("capture_failed: captured image has zero dimensions".to_string());
    }
    let floor_scaled = |value: u32, captured: u32, source: u32| -> u32 {
        ((u64::from(value) * u64::from(captured)) / u64::from(source)) as u32
    };
    let ceil_scaled = |value: u32, captured: u32, source: u32| -> u32 {
        let numerator = u64::from(value) * u64::from(captured);
        let denominator = u64::from(source);
        (numerator / denominator + u64::from(numerator % denominator != 0)) as u32
    };
    let right_source = region.x + region.width;
    let bottom_source = region.y + region.height;
    let left = floor_scaled(region.x, captured_width, source_width);
    let top = floor_scaled(region.y, captured_height, source_height);
    let right = ceil_scaled(right_source, captured_width, source_width).min(captured_width);
    let bottom = ceil_scaled(bottom_source, captured_height, source_height).min(captured_height);
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width == 0 || height == 0 {
        return Err("capture_failed: snapshot region maps to an empty captured image".to_string());
    }
    Ok((left, top, width, height))
}

#[cfg(any(target_os = "macos", windows))]
fn transform_snapshot_image(
    image: image::RgbaImage,
    source_width: u32,
    source_height: u32,
    region: Option<SnapshotRegion>,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<(image::RgbaImage, SnapshotRegion), String> {
    use image::imageops::FilterType;

    if max_width.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
        || max_height.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION)
    {
        return Err("invalid_request: snapshot output dimension bound is invalid".to_string());
    }
    let region = resolve_snapshot_region(source_width, source_height, region)?;
    let (x, y, width, height) = mapped_crop_bounds(
        region,
        source_width,
        source_height,
        image.width(),
        image.height(),
    )?;
    let mut image = if x == 0 && y == 0 && width == image.width() && height == image.height() {
        image
    } else {
        image::imageops::crop_imm(&image, x, y, width, height).to_image()
    };

    let width_scale = max_width
        .map(|bound| bound as f64 / image.width() as f64)
        .unwrap_or(1.0);
    let height_scale = max_height
        .map(|bound| bound as f64 / image.height() as f64)
        .unwrap_or(1.0);
    let scale = 1.0f64.min(width_scale).min(height_scale);
    if scale < 1.0 {
        let target_width = ((image.width() as f64 * scale).floor() as u32)
            .max(1)
            .min(max_width.unwrap_or(u32::MAX));
        let target_height = ((image.height() as f64 * scale).floor() as u32)
            .max(1)
            .min(max_height.unwrap_or(u32::MAX));
        image = image::imageops::resize(&image, target_width, target_height, FilterType::Triangle);
    }
    Ok((image, region))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn transform_snapshot_image(
    _image: (),
    _source_width: u32,
    _source_height: u32,
    _region: Option<SnapshotRegion>,
    _max_width: Option<u32>,
    _max_height: Option<u32>,
) -> Result<((), SnapshotRegion), String> {
    Err("unsupported_platform: computer observation is unavailable on this platform".to_string())
}

#[cfg(test)]
mod snapshot_region_tests {
    use super::*;

    #[test]
    fn computer_snapshot_region_is_surface_relative_and_fully_bounded() {
        assert_eq!(
            resolve_snapshot_region(100, 50, None).unwrap(),
            SnapshotRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            }
        );
        assert!(resolve_snapshot_region(
            100,
            50,
            Some(SnapshotRegion {
                x: 90,
                y: 0,
                width: 11,
                height: 10,
            })
        )
        .is_err());
        assert!(resolve_snapshot_region(
            100,
            50,
            Some(SnapshotRegion {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 10,
            })
        )
        .is_err());
    }

    #[test]
    fn computer_snapshot_region_maps_surface_coordinates_to_capture_pixels() {
        let mapped = mapped_crop_bounds(
            SnapshotRegion {
                x: 10,
                y: 5,
                width: 20,
                height: 10,
            },
            100,
            50,
            200,
            100,
        )
        .unwrap();
        assert_eq!(mapped, (20, 10, 40, 20));
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn computer_snapshot_region_downscale_preserves_aspect_and_never_upscales() {
        let image = image::RgbaImage::new(200, 100);
        let (image, region) = transform_snapshot_image(
            image,
            100,
            50,
            Some(SnapshotRegion {
                x: 10,
                y: 5,
                width: 20,
                height: 10,
            }),
            Some(20),
            Some(20),
        )
        .unwrap();
        assert_eq!((image.width(), image.height()), (20, 10));
        assert_eq!(
            region,
            SnapshotRegion {
                x: 10,
                y: 5,
                width: 20,
                height: 10,
            }
        );

        let image = image::RgbaImage::new(10, 5);
        let (image, _) =
            transform_snapshot_image(image, 10, 5, None, Some(100), Some(100)).unwrap();
        assert_eq!((image.width(), image.height()), (10, 5));
    }
}

#[cfg(any(target_os = "macos", windows))]
fn encode_bounded_jpeg(
    mut image: image::RgbaImage,
    max_encoded_image_bytes: usize,
) -> Result<EncodedImage, String> {
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
            if bytes.len() <= max_encoded_image_bytes {
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
        "image_too_large: screenshot could not be encoded within {max_encoded_image_bytes} bytes"
    ))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn encode_bounded_jpeg(
    _image: (),
    _max_encoded_image_bytes: usize,
) -> Result<EncodedImage, String> {
    Err("unsupported_platform: computer observation is unavailable on this platform".to_string())
}

fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn raw_rgba_bytes(width: u32, height: u32) -> Result<u64, String> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(RGBA_BYTES_PER_PIXEL))
        .ok_or_else(|| "image_too_large: raw RGBA capture size overflow".to_string())
}

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
            #[cfg(windows)]
            native_runtime_id: Vec::new(),
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
    fn computer_element_state_requires_positive_correlation_evidence() {
        let element = ElementRecord {
            surface_id: "surface_test".to_string(),
            path: Vec::new(),
            lineage: vec![fingerprint("")],
        };
        assert_eq!(
            validate_element_state_target(&element).unwrap_err(),
            "stale_element: AX element lacks positive correlation evidence for state observation"
        );
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
        assert_eq!(
            registry.replace_surface("surface_test", elements).unwrap(),
            1
        );
        assert_eq!(registry.entries.len(), MAX_ELEMENT_REGISTRY);
        assert!(registry.get("element_0").is_none());
        assert!(registry
            .get(&format!("element_{MAX_ELEMENT_REGISTRY}"))
            .is_some());
    }

    #[test]
    fn computer_element_registry_replaces_same_surface_generation() {
        let mut registry = ElementRegistry::default();
        let first = registry
            .replace_surface(
                "surface_test",
                vec![(
                    "element_old".to_string(),
                    record("surface_test", "old", vec![0]),
                )],
            )
            .unwrap();
        let second = registry
            .replace_surface(
                "surface_test",
                vec![(
                    "element_new".to_string(),
                    record("surface_test", "new", vec![1]),
                )],
            )
            .unwrap();
        assert_eq!((first, second), (1, 2));
        assert!(registry.get("element_old").is_none());
        assert_eq!(registry.get_with_generation("element_new").unwrap().1, 2);
    }

    #[test]
    fn computer_element_registry_generation_exhaustion_preserves_current_handles() {
        let mut registry = ElementRegistry::default();
        registry
            .replace_surface(
                "surface_test",
                vec![(
                    "element_old".to_string(),
                    record("surface_test", "old", vec![0]),
                )],
            )
            .unwrap();
        registry
            .surface_generations
            .insert("surface_test".to_string(), u32::MAX);

        let error = registry
            .replace_surface(
                "surface_test",
                vec![(
                    "element_new".to_string(),
                    record("surface_test", "new", vec![1]),
                )],
            )
            .unwrap_err();
        assert_eq!(
            error,
            "computer_state_error: observation generation exhausted"
        );
        assert!(registry.get("element_old").is_some());
        assert!(registry.get("element_new").is_none());
        assert_eq!(
            registry.surface_generations.get("surface_test"),
            Some(&u32::MAX)
        );
    }

    #[test]
    fn computer_element_registry_clear_invalidates_all_handles() {
        let mut registry = ElementRegistry::default();
        registry
            .replace_surface(
                "surface_test",
                vec![(
                    "element_test".to_string(),
                    record("surface_test", "test", vec![]),
                )],
            )
            .unwrap();
        registry.clear();
        assert!(registry.entries.is_empty());
        assert!(registry.order.is_empty());
        assert!(registry.surface_generations.is_empty());
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
    fn computer_text_input_utf8_bounds_are_closed() {
        assert_eq!(validate_input_text("你好🙂").unwrap(), "你好🙂".len());
        assert!(validate_input_text("").is_err());
        assert!(validate_input_text("a\0b").is_err());
        assert_eq!(
            validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES)).unwrap(),
            MAX_INPUT_TEXT_BYTES
        );
        assert!(validate_input_text(&"a".repeat(MAX_INPUT_TEXT_BYTES + 1)).is_err());
        assert!(validate_input_text(&"🙂".repeat((MAX_INPUT_TEXT_BYTES / 4) + 1)).is_err());
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

#[cfg(test)]
mod application_runtime_tests {
    use super::*;
    use std::sync::{mpsc, Arc};

    fn candidate(name: &str, marker: u8) -> PlatformApplication {
        PlatformApplication {
            display_name: name.to_string(),
            native_identity: vec![marker],
        }
    }

    fn observer() -> ComputerRuntime {
        ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: usize::MAX,
        })
    }

    #[test]
    fn application_candidates_have_stable_bounded_order() {
        assert!(MAX_APPLICATION_SCAN > MAX_APPLICATIONS);
        let mut applications = vec![
            candidate("zeta", 4),
            candidate("alpha", 3),
            candidate("Beta", 2),
            candidate("Alpha", 1),
        ];
        sort_application_candidates(&mut applications);
        assert_eq!(
            applications
                .iter()
                .map(|application| application.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "alpha", "Beta", "zeta"]
        );
    }

    #[test]
    fn application_ids_remain_closed() {
        for invalid in [
            "",
            "application_",
            "application_0123456789abcdef0123456789abcdeg",
            "surface_0123456789abcdef0123456789abcdef",
        ] {
            assert!(!valid_application_id(invalid), "{invalid}");
        }
        assert!(valid_application_id(
            "application_0123456789abcdef0123456789abcdef"
        ));
    }

    #[test]
    fn bounded_discovery_replaces_generation_and_stales_old_ids() {
        let observer = observer();
        let first = observer
            .replace_application_candidates(
                vec![
                    candidate("One", 1),
                    candidate("Two", 2),
                    candidate("Three", 3),
                ],
                2,
            )
            .unwrap();
        assert_eq!(first["count"], 2);
        assert_eq!(first["truncated"], true);
        let old_id = first["applications"][0]["application_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(first["applications"][0].get("native_identity").is_none());

        let second = observer
            .replace_application_candidates(vec![candidate("Four", 4)], 1)
            .unwrap();
        assert_eq!(second["count"], 1);
        let error = observer.launch_application(&old_id).unwrap_err();
        assert!(error.starts_with("stale_application:"), "{error}");
    }

    #[test]
    fn launch_admission_fences_concurrent_fresh_list_retirement() {
        let observer = Arc::new(observer());
        let first = observer
            .replace_application_candidates(vec![candidate("One", 1)], 1)
            .unwrap();
        let application_id = first["applications"][0]["application_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (effect_entered_tx, effect_entered_rx) = mpsc::channel();
        let (release_effect_tx, release_effect_rx) = mpsc::channel();
        let launch_observer = Arc::clone(&observer);
        let launch_id = application_id.clone();
        let launch = std::thread::spawn(move || {
            launch_observer.with_current_application(&launch_id, |_| {
                effect_entered_tx.send(()).unwrap();
                release_effect_rx.recv().unwrap();
                Ok(())
            })
        });
        effect_entered_rx.recv().unwrap();

        let (list_started_tx, list_started_rx) = mpsc::channel();
        let (list_completed_tx, list_completed_rx) = mpsc::channel();
        let list_observer = Arc::clone(&observer);
        let fresh_list = std::thread::spawn(move || {
            list_started_tx.send(()).unwrap();
            let output = list_observer
                .replace_application_candidates(vec![candidate("Two", 2)], 1)
                .unwrap();
            list_completed_tx.send(output).unwrap();
        });
        list_started_rx.recv().unwrap();
        assert!(matches!(
            list_completed_rx.recv_timeout(Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        release_effect_tx.send(()).unwrap();
        launch.join().unwrap().unwrap();
        let fresh = list_completed_rx.recv().unwrap();
        fresh_list.join().unwrap();
        assert_eq!(fresh["applications"][0]["display_name"], "Two");
        let error = observer
            .with_current_application(&application_id, |_| Ok(()))
            .unwrap_err();
        assert!(error.starts_with("stale_application:"), "{error}");
    }
}

#[cfg(test)]
mod display_runtime_tests {
    use super::*;

    fn display(marker: u8, width: u32, height: u32, primary: bool) -> PlatformDisplay {
        PlatformDisplay {
            native_identity: vec![marker],
            width,
            height,
            primary,
        }
    }

    fn observer() -> ComputerRuntime {
        ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: usize::MAX,
        })
    }

    #[test]
    fn display_ids_remain_closed() {
        assert!(valid_display_id("display_0123456789abcdef0123456789abcdef"));
        for invalid in [
            "",
            "display_",
            "display_0123456789abcdef0123456789abcdeg",
            "surface_0123456789abcdef0123456789abcdef",
        ] {
            assert!(!valid_display_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn display_discovery_replaces_ids_and_snapshot_generation_is_bounded_and_monotonic() {
        let observer = observer();
        let first = observer
            .replace_display_candidates(
                vec![display(1, 1920, 1080, true), display(2, 1280, 720, false)],
                1,
            )
            .unwrap();
        assert_eq!(first["count"], 1);
        assert_eq!(first["truncated"], true);
        assert!(first["displays"][0].get("native_identity").is_none());
        let old_id = first["displays"][0]["display_id"]
            .as_str()
            .unwrap()
            .to_string();
        let record = observer
            .displays
            .lock()
            .unwrap()
            .get(&old_id)
            .unwrap()
            .clone();
        {
            let mut snapshots = observer.display_snapshots.lock().unwrap();
            assert_eq!(snapshots.bind(&old_id, &record).unwrap(), 1);
            assert_eq!(snapshots.bind(&old_id, &record).unwrap(), 2);
            assert_eq!(snapshots.bindings.back().unwrap().native_identity, vec![1]);
            assert_eq!(snapshots.bindings.back().unwrap().source_width, 1920);
            assert_eq!(snapshots.bindings.back().unwrap().source_height, 1080);
        }
        observer
            .replace_display_candidates(vec![display(3, 2560, 1440, true)], 1)
            .unwrap();
        assert!(observer
            .display_snapshots
            .lock()
            .unwrap()
            .bindings
            .is_empty());
        let error = observer
            .snapshot_display(&old_id, Some(640), Some(480))
            .unwrap_err();
        assert!(error.starts_with("stale_display:"), "{error}");
        let restarted = self::observer();
        let restart_error = restarted
            .snapshot_display(&old_id, Some(640), Some(480))
            .unwrap_err();
        assert!(
            restart_error.starts_with("stale_display:"),
            "{restart_error}"
        );
        let new_id = observer
            .displays
            .lock()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let new_record = observer
            .displays
            .lock()
            .unwrap()
            .get(&new_id)
            .unwrap()
            .clone();
        assert_eq!(
            observer
                .display_snapshots
                .lock()
                .unwrap()
                .bind(&new_id, &new_record)
                .unwrap(),
            3
        );
        let mut snapshots = DisplaySnapshotRegistry::default();
        for expected in 1..=(MAX_DISPLAY_SNAPSHOT_BINDINGS as u32 + 1) {
            assert_eq!(snapshots.bind(&new_id, &new_record).unwrap(), expected);
        }
        assert_eq!(snapshots.bindings.len(), MAX_DISPLAY_SNAPSHOT_BINDINGS);
    }
}

#[cfg(test)]
mod pointer_runtime_tests {
    use super::*;

    #[test]
    fn pointer_output_platform_matches_native_backend() {
        #[cfg(target_os = "macos")]
        assert_eq!(pointer_output_platform(), "macos");
        #[cfg(windows)]
        assert_eq!(pointer_output_platform(), "windows");
        #[cfg(not(any(target_os = "macos", windows)))]
        assert_eq!(pointer_output_platform(), "unsupported");
    }

    #[test]
    fn pointer_generation_is_latest_exact_and_single_use() {
        let display = DisplayRecord {
            native_identity: vec![9],
            width: 1920,
            height: 1080,
            primary: false,
        };
        let mut snapshots = DisplaySnapshotRegistry::default();
        let display_id = "display_0123456789abcdef0123456789abcdef";
        let first = snapshots.bind(display_id, &display).unwrap();
        let second = snapshots.bind(display_id, &display).unwrap();
        assert!(snapshots
            .validate_pointer(display_id, first, &display)
            .unwrap_err()
            .starts_with("stale_snapshot_generation:"));
        snapshots
            .validate_pointer(display_id, second, &display)
            .unwrap();
        let dispatched = dispatch_after_spending_pointer_generation(
            &mut snapshots,
            display_id,
            second,
            &display,
            |spent| {
                assert!(spent
                    .validate_pointer(display_id, second, &display)
                    .unwrap_err()
                    .contains("already consumed"));
                Ok(true)
            },
        )
        .unwrap();
        assert!(dispatched);
        assert!(snapshots
            .validate_pointer(display_id, second, &display)
            .unwrap_err()
            .contains("already consumed"));

        let mut spent_not_started = DisplaySnapshotRegistry::default();
        let spent_generation = spent_not_started.bind(display_id, &display).unwrap();
        let error = dispatch_after_spending_pointer_generation(
            &mut spent_not_started,
            display_id,
            spent_generation,
            &display,
            |_| {
                Err(
                    "not_started: final native preflight failed after generation spend before post"
                        .to_string(),
                )
            },
        )
        .expect_err("spent final-preflight failure must remain a definite no-post result");
        assert!(error.starts_with("not_started:"), "{error}");
        assert!(spent_not_started
            .validate_pointer(display_id, spent_generation, &display)
            .unwrap_err()
            .contains("already consumed"));

        let mut changed_identity = display.clone();
        changed_identity.native_identity = vec![10];
        assert!(snapshots
            .validate_pointer(display_id, second, &changed_identity)
            .unwrap_err()
            .starts_with("stale_display:"));

        let mut fresh = DisplaySnapshotRegistry::default();
        let generation = fresh.bind(display_id, &display).unwrap();
        let mut changed_geometry = display.clone();
        changed_geometry.width += 1;
        assert!(fresh
            .validate_pointer(display_id, generation, &changed_geometry)
            .unwrap_err()
            .starts_with("stale_display:"));
        fresh.clear_bindings();
        assert!(fresh
            .validate_pointer(display_id, generation, &display)
            .unwrap_err()
            .starts_with("stale_snapshot_generation:"));
        let restarted = DisplaySnapshotRegistry::default();
        assert!(restarted
            .validate_pointer(display_id, generation, &display)
            .unwrap_err()
            .starts_with("stale_snapshot_generation:"));
    }
}

#[cfg(test)]
mod key_input_runtime_tests {
    use super::*;

    #[test]
    fn key_input_vocabulary_and_modifiers_are_closed() {
        assert!(validate_key_input("tab", &["shift".to_string()]).is_ok());
        assert!(validate_key_input("a", &[]).is_err());
        assert!(validate_key_input("enter", &["shift".to_string(), "shift".to_string()]).is_err());
    }
}

#[cfg(test)]
mod clipboard_contract_tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn utf16_storage(value: &str) -> Vec<u16> {
        let mut units: Vec<u16> = value.encode_utf16().collect();
        units.push(0);
        units
    }

    fn unicode_fixture() -> String {
        String::from_utf16(&[0x0041, 0x4E2D, 0xD83D, 0xDE00]).unwrap()
    }

    #[test]
    fn clipboard_write_preparation_is_bounded() {
        for invalid in [
            "".to_string(),
            "nul\0text".to_string(),
            "a".repeat(16 * 1024 + 1),
        ] {
            assert!(prepare_clipboard_write_text(&invalid).is_err());
        }
        let text = unicode_fixture();
        let prepared = prepare_clipboard_write_text(&text).unwrap();
        assert_eq!(prepared.text_bytes, text.len());
        assert_eq!(prepared.utf16.last(), Some(&0));
        assert_eq!(
            prepared.storage_bytes,
            prepared.utf16.len() * std::mem::size_of::<u16>()
        );
        assert!(prepared.storage_bytes <= MAX_CLIPBOARD_NATIVE_STORAGE_BYTES);
    }

    #[test]
    fn clipboard_read_decodes_unicode_empty_and_unavailable_without_truncation() {
        let unavailable = clipboard_read_result_from_utf16(None, 0).unwrap();
        assert_eq!(
            unavailable,
            json!({"platform":"windows","available":false,"text_bytes":0})
        );

        let empty = [0u16];
        assert_eq!(
            clipboard_read_result_from_utf16(Some(&empty), 2).unwrap(),
            json!({"platform":"windows","available":true,"text":"","text_bytes":0})
        );

        let text = unicode_fixture();
        let units = utf16_storage(&text);
        assert_eq!(
            clipboard_read_result_from_utf16(Some(&units), units.len() * 2).unwrap(),
            json!({"platform":"windows","available":true,"text":text,"text_bytes":text.len()})
        );

        let unterminated = [b'A' as u16, b'B' as u16];
        assert!(clipboard_read_result_from_utf16(Some(&unterminated), 4)
            .unwrap_err()
            .starts_with("clipboard_malformed:"));
        let malformed = [0xD800u16, 0];
        assert!(clipboard_read_result_from_utf16(Some(&malformed), 4)
            .unwrap_err()
            .starts_with("clipboard_malformed:"));
        assert!(clipboard_read_result_from_utf16(
            Some(&[0]),
            MAX_CLIPBOARD_NATIVE_STORAGE_BYTES + 2
        )
        .unwrap_err()
        .starts_with("clipboard_too_large:"));

        assert_eq!(
            clipboard_read_result("macos", None).unwrap(),
            json!({"platform":"macos","available":false,"text_bytes":0})
        );
        assert_eq!(
            clipboard_read_result("macos", Some("")).unwrap(),
            json!({"platform":"macos","available":true,"text":"","text_bytes":0})
        );
        assert!(
            clipboard_read_result("macos", Some(&"a".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1)))
                .unwrap_err()
                .starts_with("clipboard_too_large:")
        );
        assert!(clipboard_read_result("macos", Some("bad\0text"))
            .unwrap_err()
            .starts_with("clipboard_malformed:"));

        let two_byte = String::from_utf16(&[0x00E9]).unwrap();
        let oversized_text = two_byte.repeat((MAX_CLIPBOARD_TEXT_BYTES / 2) + 1);
        let oversized_units = utf16_storage(&oversized_text);
        assert!(oversized_units.len() * 2 <= MAX_CLIPBOARD_NATIVE_STORAGE_BYTES);
        assert!(clipboard_read_result_from_utf16(
            Some(&oversized_units),
            oversized_units.len() * 2
        )
        .unwrap_err()
        .starts_with("clipboard_too_large:"));
    }

    #[test]
    fn clipboard_read_cleanup_runs_once_on_success_and_error() {
        let closes = Cell::new(0usize);
        let result = finish_clipboard_read(Ok::<_, String>(7u8), || {
            closes.set(closes.get() + 1);
            true
        });
        assert_eq!(result.unwrap(), 7);
        assert_eq!(closes.get(), 1);

        let closes = Cell::new(0usize);
        let result =
            finish_clipboard_read::<u8>(Err("clipboard_malformed: bad".to_string()), || {
                closes.set(closes.get() + 1);
                true
            });
        assert!(result.unwrap_err().starts_with("clipboard_malformed:"));
        assert_eq!(closes.get(), 1);

        let closes = Cell::new(0usize);
        let result = finish_clipboard_read::<u8>(Ok(1), || {
            closes.set(closes.get() + 1);
            false
        });
        assert!(result.unwrap_err().contains("CloseClipboard"));
        assert_eq!(closes.get(), 1);
    }

    #[test]
    fn clipboard_write_effect_boundary_is_one_shot_and_conservative() {
        fn run(
            empty: bool,
            set: bool,
            close: bool,
        ) -> (ClipboardWriteEffectState, Vec<&'static str>) {
            let calls = RefCell::new(Vec::new());
            let state = run_clipboard_write_effect_steps(
                || {
                    calls.borrow_mut().push("empty");
                    empty
                },
                || {
                    calls.borrow_mut().push("set");
                    set
                },
                || {
                    calls.borrow_mut().push("close");
                    close
                },
            );
            (state, calls.into_inner())
        }

        assert_eq!(
            run(false, true, true),
            (
                ClipboardWriteEffectState::NotStarted,
                vec!["empty", "close"]
            )
        );
        assert_eq!(
            run(true, false, true),
            (
                ClipboardWriteEffectState::OutcomeUnknown,
                vec!["empty", "set", "close"]
            )
        );
        assert_eq!(
            run(true, true, false),
            (
                ClipboardWriteEffectState::OutcomeUnknown,
                vec!["empty", "set", "close"]
            )
        );
        assert_eq!(
            run(true, true, true),
            (
                ClipboardWriteEffectState::Success,
                vec!["empty", "set", "close"]
            )
        );
    }

    #[test]
    fn macos_clipboard_write_proves_set_and_retained_ownership_after_clear() {
        fn run(
            clear_count: isize,
            set: bool,
            final_count: isize,
        ) -> (ClipboardWriteEffectState, Vec<&'static str>) {
            let calls = RefCell::new(Vec::new());
            let state = run_macos_clipboard_write_effect_steps(
                || {
                    calls.borrow_mut().push("clear");
                    clear_count
                },
                || {
                    calls.borrow_mut().push("set");
                    set
                },
                || {
                    calls.borrow_mut().push("change_count");
                    final_count
                },
            );
            (state, calls.into_inner())
        }

        assert_eq!(
            run(7, true, 7),
            (
                ClipboardWriteEffectState::Success,
                vec!["clear", "set", "change_count"]
            )
        );
        assert_eq!(
            run(7, false, 7),
            (
                ClipboardWriteEffectState::OutcomeUnknown,
                vec!["clear", "set", "change_count"]
            )
        );
        assert_eq!(
            run(7, true, 8),
            (
                ClipboardWriteEffectState::OutcomeUnknown,
                vec!["clear", "set", "change_count"]
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_write_requires_non_null_runner_owned_hwnd_contract() {
        assert!(!platform::clipboard_owner_hwnd_contract_for_test(false));
        assert!(platform::clipboard_owner_hwnd_contract_for_test(true));
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_close_failure_keeps_best_effort_cleanup_armed() {
        assert!(!platform::clipboard_close_cleanup_armed_for_test(true));
        assert!(platform::clipboard_close_cleanup_armed_for_test(false));
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
        AccessibilityTreeResult, ApplicationRecord, ComputerAction, DisplayRecord, ElementRecord,
        PlatformApplication, PlatformDisplay, PlatformWindow, PointerAction, PointerPlan,
        SurfaceRecord,
    };

    pub(super) fn read_clipboard() -> Result<serde_json::Value, String> {
        Err("unsupported_platform: clipboard read is unavailable on this platform".to_string())
    }

    pub(super) fn write_clipboard(_text: &str) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: clipboard write is unavailable on this platform".to_string())
    }

    pub(super) fn prepare_pointer(
        _display: &DisplayRecord,
        _x: u32,
        _y: u32,
        _action: PointerAction,
    ) -> Result<PointerPlan, String> {
        Err(
            "unsupported_platform: coordinate pointer control is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn dispatch_pointer(
        _plan: PointerPlan,
        _action: PointerAction,
    ) -> Result<bool, String> {
        Err(
            "unsupported_platform: coordinate pointer control is unavailable on this platform"
                .to_string(),
        )
    }
    pub(super) fn list_applications(_limit: usize) -> Result<Vec<PlatformApplication>, String> {
        Err(
            "unsupported_platform: application discovery is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn launch_application(
        _application_id: &str,
        _application: &ApplicationRecord,
    ) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: application launch is unavailable on this platform".to_string())
    }

    pub(super) fn list_displays(_limit: usize) -> Result<Vec<PlatformDisplay>, String> {
        Err(
            "unsupported_platform: full-display observation is unavailable on this platform"
                .to_string(),
        )
    }

    pub(super) fn capture_display(_display: &DisplayRecord) -> Result<(), String> {
        Err(
            "unsupported_platform: full-display observation is unavailable on this platform"
                .to_string(),
        )
    }

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

    pub(super) fn element_state(
        _surface_id: &str,
        _element_id: &str,
        _observation_generation: u32,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
    ) -> Result<serde_json::Value, String> {
        Err(
            "unsupported_platform: computer element state is unavailable on this platform"
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

    pub(super) fn scroll_to_element(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
    ) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: computer scroll is unavailable on this platform".to_string())
    }

    pub(super) fn key_input(
        _surface_id: &str,
        _surface: &SurfaceRecord,
        _key: &str,
        _modifiers: &[String],
    ) -> Result<serde_json::Value, String> {
        Err("unsupported_platform: computer key input is unavailable on this platform".to_string())
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
mod unsupported_platform_tests {
    use super::*;

    fn runtime() -> ComputerRuntime {
        ComputerRuntime::new(ComputerConfig {
            max_encoded_image_bytes: usize::MAX,
        })
    }

    #[test]
    fn list_windows_fails_closed_as_unsupported() {
        let error = runtime().list_windows(1).unwrap_err();
        assert!(error.starts_with("unsupported_platform:"), "{error}");
    }

    #[test]
    fn unknown_surface_is_stale_before_platform_capture() {
        let error = runtime()
            .snapshot("surface_missing", None, None, None)
            .unwrap_err();
        assert!(error.starts_with("stale_surface:"), "{error}");
    }
    #[test]
    fn text_input_platform_is_unsupported_off_macos_and_windows() {
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
#[path = "platform.rs"]
mod platform;

#[cfg(all(test, windows))]
#[path = "windows_uia_tests.rs"]
mod windows_uia_tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "macos_live_tests.rs"]
mod macos_live_tests;
