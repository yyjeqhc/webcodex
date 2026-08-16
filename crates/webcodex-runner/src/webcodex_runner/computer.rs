use super::{err_cmd, ok_cmd, CommandResult};
#[cfg(any(target_os = "macos", windows))]
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::shell_protocol::{shell_computer_request_payload_max_bytes, ShellAgentShellRequest};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
#[cfg(any(test, target_os = "macos"))]
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MAX_WINDOWS: usize = 64;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_ELEMENT_REGISTRY: usize = 1024;
const MAX_INPUT_TEXT_BYTES: usize = 2048;
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
const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
#[cfg(any(test, target_os = "macos", windows))]
const RGBA_BYTES_PER_PIXEL: u64 = 4;
#[cfg(any(test, target_os = "macos", windows))]
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
struct SnapshotRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
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

    fn element_state(&self, surface_id: &str, element_id: &str) -> Result<Value, String> {
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

    fn scroll_to_element(&self, surface_id: &str, element_id: &str) -> Result<Value, String> {
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

    fn key_input(
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

    fn snapshot(
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
        let encoded = encode_bounded_jpeg(image)?;
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

    #[test]
    fn computer_snapshot_region_payload_is_closed_and_typed() {
        let exact = json!({
            "surface_id": "surface_test",
            "region": {"x": 1, "y": 2, "width": 3, "height": 4},
            "max_width": 100,
            "max_height": null
        });
        assert!(ensure_exact_payload_fields(
            &exact,
            &["surface_id", "region", "max_width", "max_height"]
        )
        .is_ok());
        assert_eq!(
            optional_snapshot_region(&exact).unwrap(),
            Some(SnapshotRegion {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            })
        );
        assert_eq!(
            optional_snapshot_dimension(&exact, "max_width").unwrap(),
            Some(100)
        );
        assert_eq!(
            optional_snapshot_dimension(&exact, "max_height").unwrap(),
            None
        );

        let extra = json!({
            "surface_id": "surface_test",
            "region": {"x": 1, "y": 2, "width": 3, "height": 4},
            "max_width": 100,
            "max_height": null,
            "quality": 99
        });
        assert!(ensure_exact_payload_fields(
            &extra,
            &["surface_id", "region", "max_width", "max_height"]
        )
        .is_err());
        let nested_extra = json!({
            "region": {"x": 1, "y": 2, "width": 3, "height": 4, "global": true}
        });
        assert!(optional_snapshot_region(&nested_extra).is_err());
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
    fn computer_element_state_payload_is_exact_surface_and_element_only() {
        let exact = json!({
            "surface_id": "surface_test",
            "element_id": "element_test"
        });
        assert!(ensure_exact_payload_fields(&exact, &["surface_id", "element_id"]).is_ok());
        for extra in [
            json!({"surface_id": "surface_test", "element_id": "element_test", "value": true}),
            json!({"surface_id": "surface_test", "element_id": "element_test", "action": "focus"}),
            json!({"surface_id": "surface_test", "element_id": "element_test", "refresh": true}),
        ] {
            assert!(ensure_exact_payload_fields(&extra, &["surface_id", "element_id"]).is_err());
        }
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
            | "computer_snapshot_region"
            | "computer_accessibility_status"
            | "computer_accessibility_tree"
            | "computer_element_state"
            | "computer_activate_window"
            | "computer_control"
            | "computer_scroll_to_element"
            | "computer_key_input"
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

#[cfg(test)]
mod scroll_wire_contract_tests {
    use super::*;

    #[test]
    fn scroll_to_element_is_a_distinct_strict_request_kind() {
        assert!(is_computer_request_kind("computer_scroll_to_element"));
        assert!(ensure_exact_payload_fields(
            &json!({"surface_id": "surface_test", "element_id": "element_test"}),
            &["surface_id", "element_id"],
        )
        .is_ok());
        let error = ensure_exact_payload_fields(
            &json!({
                "surface_id": "surface_test",
                "element_id": "element_test",
                "delta": 1
            }),
            &["surface_id", "element_id"],
        )
        .unwrap_err();
        assert!(error.contains("unsupported fields"));
    }
}

#[cfg(test)]
mod key_input_wire_contract_tests {
    use super::*;

    #[test]
    fn key_input_is_a_distinct_strict_closed_request_kind() {
        assert!(is_computer_request_kind("computer_key_input"));
        let exact = json!({
            "surface_id": "surface_test",
            "key": "tab",
            "modifiers": ["shift"]
        });
        assert!(ensure_exact_payload_fields(&exact, &["surface_id", "key", "modifiers"]).is_ok());
        assert!(validate_key_input("tab", &["shift".to_string()]).is_ok());
        assert!(validate_key_input("a", &[]).is_err());
        assert!(validate_key_input("enter", &["shift".to_string(), "shift".to_string()]).is_err());
        for extra in ["text", "keycode", "repeat", "held", "element_id"] {
            let mut extra_payload = exact.clone();
            extra_payload
                .as_object_mut()
                .unwrap()
                .insert(extra.to_string(), Value::from(1));
            assert!(
                ensure_exact_payload_fields(&extra_payload, &["surface_id", "key", "modifiers"])
                    .is_err(),
                "extra field {extra}"
            );
        }
    }
}

fn optional_snapshot_region(payload: &Value) -> Result<Option<SnapshotRegion>, String> {
    match payload.get("region") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|_| "invalid_request: snapshot region is invalid".to_string()),
    }
}

fn optional_snapshot_dimension(payload: &Value, field: &str) -> Result<Option<u32>, String> {
    match payload.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| format!("invalid_request: snapshot {field} is invalid")),
    }
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
        "computer_element_state" => {
            ensure_exact_payload_fields(&payload, &["surface_id", "element_id"]).and_then(|()| {
                let surface_id = payload
                    .get("surface_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: surface_id is required".to_string())?;
                let element_id = payload
                    .get("element_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: element_id is required".to_string())?;
                ComputerObserver::global().element_state(surface_id, element_id)
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
        "computer_scroll_to_element" => {
            ensure_exact_payload_fields(&payload, &["surface_id", "element_id"]).and_then(|()| {
                let surface_id = payload
                    .get("surface_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: surface_id is required".to_string())?;
                let element_id = payload
                    .get("element_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: element_id is required".to_string())?;
                ComputerObserver::global().scroll_to_element(surface_id, element_id)
            })
        }
        "computer_key_input" => {
            ensure_exact_payload_fields(&payload, &["surface_id", "key", "modifiers"]).and_then(
                |()| {
                    let surface_id = payload
                        .get("surface_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: surface_id is required".to_string())?;
                    let key = payload
                        .get("key")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "invalid_request: key is required".to_string())?;
                    let modifier_values = payload
                        .get("modifiers")
                        .and_then(Value::as_array)
                        .ok_or_else(|| "invalid_request: modifiers must be an array".to_string())?;
                    let modifiers = modifier_values
                        .iter()
                        .map(|value| {
                            value.as_str().map(str::to_string).ok_or_else(|| {
                                "invalid_request: each modifier must be a string".to_string()
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    ComputerObserver::global().key_input(surface_id, key, &modifiers)
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
        "computer_snapshot" => ensure_exact_payload_fields(&payload, &["surface_id"])
            .and_then(|()| {
                payload
                    .get("surface_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "invalid_request: surface_id is required".to_string())
            })
            .and_then(|surface_id| {
                ComputerObserver::global().snapshot(surface_id, None, None, None)
            }),
        "computer_snapshot_region" => ensure_exact_payload_fields(
            &payload,
            &["surface_id", "region", "max_width", "max_height"],
        )
        .and_then(|()| {
            let surface_id = payload
                .get("surface_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "invalid_request: surface_id is required".to_string())?;
            let region = optional_snapshot_region(&payload)?;
            let max_width = optional_snapshot_dimension(&payload, "max_width")?;
            let max_height = optional_snapshot_dimension(&payload, "max_height")?;
            if region.is_none() && max_width.is_none() && max_height.is_none() {
                return Err(
                    "invalid_request: region snapshot requires a region or output dimension bound"
                        .to_string(),
                );
            }
            ComputerObserver::global().snapshot(surface_id, region, max_width, max_height)
        }),
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
        bounded_text, ensure_raw_capture_bound, validate_input_text, AccessibilityTreeResult,
        ComputerAction, ElementRecord, PlatformWindow, SurfaceRecord,
    };
    #[cfg(target_os = "macos")]
    use super::{
        ensure_correlated_fingerprint, is_secure_text_fingerprint, select_exact_ax_window_index,
        validate_element_state_target, validate_key_input, validate_key_modifiers,
        validate_text_input_preflight, validate_text_input_target, AxObservationDeadline,
    };
    #[cfg(any(target_os = "macos", windows))]
    use super::{is_supported_text_input_fingerprint, ElementFingerprint};
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
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGKeyCode, CGPreflightPostEventAccess};
    #[cfg(any(target_os = "macos", windows))]
    use std::collections::VecDeque;
    #[cfg(any(target_os = "macos", windows))]
    use std::ptr::NonNull;
    #[cfg(windows)]
    use std::time::{Duration, Instant};
    #[cfg(any(target_os = "macos", windows))]
    use uuid::Uuid;

    #[cfg(windows)]
    use windows::Win32::System::{
        Com::SAFEARRAY,
        Ole::{
            SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
            SafeArrayGetLBound, SafeArrayGetUBound,
        },
    };
    #[cfg(windows)]
    use windows::{
        core::{IUnknown, Interface},
        Win32::{
            Foundation::{E_NOINTERFACE, E_POINTER, HWND as WinHwnd, RPC_E_CHANGED_MODE},
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            UI::Accessibility::{
                CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationInvokePattern,
                IUIAutomationTreeWalker, IUIAutomationValuePattern, UIA_ButtonControlTypeId,
                UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_CustomControlTypeId,
                UIA_DataGridControlTypeId, UIA_DataItemControlTypeId, UIA_DocumentControlTypeId,
                UIA_EditControlTypeId, UIA_GroupControlTypeId, UIA_HeaderControlTypeId,
                UIA_HeaderItemControlTypeId, UIA_HyperlinkControlTypeId, UIA_InvokePatternId,
                UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuControlTypeId,
                UIA_MenuItemControlTypeId, UIA_PaneControlTypeId, UIA_ProgressBarControlTypeId,
                UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
                UIA_SeparatorControlTypeId, UIA_SliderControlTypeId, UIA_SpinnerControlTypeId,
                UIA_StatusBarControlTypeId, UIA_TabControlTypeId, UIA_TabItemControlTypeId,
                UIA_TableControlTypeId, UIA_TextControlTypeId, UIA_ToolBarControlTypeId,
                UIA_ToolTipControlTypeId, UIA_TreeControlTypeId, UIA_TreeItemControlTypeId,
                UIA_ValuePatternId, UIA_WindowControlTypeId, UIA_CONTROLTYPE_ID,
                UIA_E_ELEMENTNOTAVAILABLE, UIA_E_NOTSUPPORTED, UIA_PATTERN_ID,
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, IsIconic, ShowWindowAsync, SW_RESTORE},
        },
    };
    #[cfg(windows)]
    use windows_sys::Win32::{
        Foundation::HWND as SysHwnd,
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
    fn checked_surface_pid(surface: &SurfaceRecord) -> Result<libc::pid_t, String> {
        libc::pid_t::try_from(surface.pid)
            .map_err(|_| "stale_surface: surface PID exceeds native range".to_string())
    }

    #[cfg(target_os = "macos")]
    fn key_code(key: &str) -> Result<CGKeyCode, String> {
        match key {
            "enter" => Ok(0x24),
            "tab" => Ok(0x30),
            "escape" => Ok(0x35),
            "home" => Ok(0x73),
            "page_up" => Ok(0x74),
            "end" => Ok(0x77),
            "page_down" => Ok(0x79),
            "arrow_left" => Ok(0x7b),
            "arrow_right" => Ok(0x7c),
            "arrow_down" => Ok(0x7d),
            "arrow_up" => Ok(0x7e),
            _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
        }
    }

    #[cfg(target_os = "macos")]
    fn key_modifier_flags(modifiers: &[String]) -> Result<CGEventFlags, String> {
        validate_key_modifiers(modifiers)?;
        let mut flags = CGEventFlags::empty();
        for modifier in modifiers {
            flags |= match modifier.as_str() {
                "shift" => CGEventFlags::MaskShift,
                "control" => CGEventFlags::MaskControl,
                "option" => CGEventFlags::MaskAlternate,
                "command" => CGEventFlags::MaskCommand,
                _ => unreachable!("validate_key_input closed modifier vocabulary"),
            };
        }
        Ok(flags)
    }

    #[cfg(all(test, target_os = "macos"))]
    mod key_input_native_contract_tests {
        use super::*;

        #[test]
        fn closed_key_codes_and_modifier_flags_are_stable() {
            for (key, expected) in [
                ("enter", 0x24),
                ("tab", 0x30),
                ("escape", 0x35),
                ("home", 0x73),
                ("page_up", 0x74),
                ("end", 0x77),
                ("page_down", 0x79),
                ("arrow_left", 0x7b),
                ("arrow_right", 0x7c),
                ("arrow_down", 0x7d),
                ("arrow_up", 0x7e),
            ] {
                assert_eq!(key_code(key).unwrap(), expected, "{key}");
            }
            assert!(key_code("a").is_err());

            let flags = key_modifier_flags(&["shift".to_string(), "command".to_string()]).unwrap();
            assert!(flags.contains(CGEventFlags::MaskShift));
            assert!(flags.contains(CGEventFlags::MaskCommand));
            assert!(!flags.contains(CGEventFlags::MaskAlternate));

            let mut surface = SurfaceRecord {
                native_id: 1,
                pid: 1,
                identity_hash: [0; 32],
                application: "test".to_string(),
                title: "test".to_string(),
                width: 1,
                height: 1,
            };
            assert_eq!(checked_surface_pid(&surface).unwrap(), 1);
            surface.pid = u32::MAX;
            assert!(checked_surface_pid(&surface)
                .unwrap_err()
                .starts_with("stale_surface:"));
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
    fn scroll_attempt_error(operation: &str, error: AXError) -> String {
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
                "scroll_failed: {operation} was rejected with AXError({})",
                error.0
            )
        } else {
            format!(
                "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
                error.0
            )
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    mod scroll_attempt_tests {
        use super::*;

        #[test]
        fn rejected_scroll_action_is_definite_but_unclassified_native_error_is_unknown() {
            let rejected = scroll_attempt_error(
                "AXUIElementPerformAction(AXScrollToVisible)",
                AXError::ActionUnsupported,
            );
            assert!(rejected.starts_with("scroll_failed:"), "{rejected}");

            let uncertain = scroll_attempt_error(
                "AXUIElementPerformAction(AXScrollToVisible)",
                AXError::NoValue,
            );
            assert!(uncertain.starts_with("outcome_unknown:"), "{uncertain}");
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

    #[cfg(any(target_os = "macos", windows))]
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
        let pid = checked_surface_pid(surface)?;
        let application = unsafe { AXUIElement::new_application(pid) };
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
    const UIA_CONNECTION_TIMEOUT_MS: u32 = 2_000;
    #[cfg(windows)]
    const UIA_TRANSACTION_TIMEOUT_MS: u32 = 2_000;
    #[cfg(windows)]
    const UIA_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
    #[cfg(windows)]
    const UIA_OBSERVATION_TIMEOUT_ERROR: &str =
        "accessibility_failed: Windows UI Automation observation deadline exceeded";
    #[cfg(windows)]
    const MAX_UIA_RUNTIME_ID_ELEMENTS: usize = 64;

    #[cfg(windows)]
    struct UiaObservationDeadline {
        expires_at: Instant,
    }

    #[cfg(windows)]
    impl UiaObservationDeadline {
        fn new() -> Self {
            Self {
                expires_at: Instant::now() + UIA_OBSERVATION_TIMEOUT,
            }
        }

        fn ensure_remaining(&self) -> Result<(), String> {
            self.expires_at
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .map(|_| ())
                .ok_or_else(|| UIA_OBSERVATION_TIMEOUT_ERROR.to_string())
        }
    }

    #[cfg(windows)]
    struct ComInitialization {
        uninitialize: bool,
    }

    #[cfg(windows)]
    impl ComInitialization {
        fn new() -> Result<Self, String> {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result.is_ok() {
                Ok(Self { uninitialize: true })
            } else if result == RPC_E_CHANGED_MODE {
                // The Runner thread is already initialized in another apartment.
                // UI Automation supports either apartment; do not uninitialize an
                // apartment established by another subsystem.
                Ok(Self {
                    uninitialize: false,
                })
            } else {
                Err(format!(
                    "accessibility_failed: CoInitializeEx failed with HRESULT(0x{:08X})",
                    result.0 as u32
                ))
            }
        }
    }

    #[cfg(windows)]
    impl Drop for ComInitialization {
        fn drop(&mut self) {
            if self.uninitialize {
                unsafe { CoUninitialize() };
            }
        }
    }

    #[cfg(windows)]
    struct UiaContext {
        // COM interfaces must be released before the matching CoUninitialize.
        // Rust drops struct fields in declaration order, so keep the guard last.
        automation: IUIAutomation2,
        walker: IUIAutomationTreeWalker,
        deadline: UiaObservationDeadline,
        _com: ComInitialization,
    }

    #[cfg(windows)]
    impl UiaContext {
        fn new() -> Result<Self, String> {
            let com = ComInitialization::new()?;
            let automation: IUIAutomation2 = unsafe {
                CoCreateInstance(&CUIAutomation8, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
            }
            .map_err(|error| uia_error("CoCreateInstance(CUIAutomation8)", &error))?;
            unsafe { automation.SetConnectionTimeout(UIA_CONNECTION_TIMEOUT_MS) }
                .map_err(|error| uia_error("IUIAutomation2::SetConnectionTimeout", &error))?;
            unsafe { automation.SetTransactionTimeout(UIA_TRANSACTION_TIMEOUT_MS) }
                .map_err(|error| uia_error("IUIAutomation2::SetTransactionTimeout", &error))?;
            let walker = unsafe { automation.ControlViewWalker() }
                .map_err(|error| uia_error("IUIAutomation::ControlViewWalker", &error))?;
            Ok(Self {
                automation,
                walker,
                deadline: UiaObservationDeadline::new(),
                _com: com,
            })
        }
    }

    #[cfg(windows)]
    struct OwnedSafeArray(NonNull<SAFEARRAY>);

    #[cfg(windows)]
    impl OwnedSafeArray {
        fn new(array: *mut SAFEARRAY) -> Result<Self, String> {
            NonNull::new(array)
                .map(Self)
                .ok_or_else(|| "stale_element: UI Automation runtime id is missing".to_string())
        }

        fn as_ptr(&self) -> *mut SAFEARRAY {
            self.0.as_ptr()
        }
    }

    #[cfg(windows)]
    impl Drop for OwnedSafeArray {
        fn drop(&mut self) {
            unsafe {
                let _ = SafeArrayDestroy(self.as_ptr());
            }
        }
    }

    #[cfg(windows)]
    fn uia_error(operation: &str, error: &windows::core::Error) -> String {
        format!(
            "accessibility_failed: {operation} failed with HRESULT(0x{:08X})",
            error.code().0 as u32
        )
    }

    #[cfg(windows)]
    fn uia_error_code(error: &windows::core::Error) -> u32 {
        error.code().0 as u32
    }

    #[cfg(windows)]
    fn optional_uia_element(
        result: windows::core::Result<IUIAutomationElement>,
        operation: &str,
    ) -> Result<Option<IUIAutomationElement>, String> {
        match result {
            Ok(element) => Ok(Some(element)),
            // UIA tree-walker APIs use S_OK + a null interface for end-of-list.
            // windows-rs represents that nullable success as Error::empty() (S_OK).
            Err(error) if error.code().is_ok() || error.code() == E_POINTER => Ok(None),
            Err(error) => Err(uia_error(operation, &error)),
        }
    }

    #[cfg(windows)]
    fn optional_uia_pattern<T: Interface>(
        context: &UiaContext,
        element: &IUIAutomationElement,
        pattern: UIA_PATTERN_ID,
    ) -> Result<Option<T>, String> {
        context.deadline.ensure_remaining()?;
        match unsafe { element.GetCurrentPatternAs::<T>(pattern) } {
            Ok(pattern) => Ok(Some(pattern)),
            Err(error)
                if error.code().is_ok()
                    || error.code() == E_NOINTERFACE
                    || error.code() == E_POINTER
                    || uia_error_code(&error) == UIA_E_NOTSUPPORTED =>
            {
                Ok(None)
            }
            Err(error) if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE => {
                Err("stale_element: UI Automation element is no longer available".to_string())
            }
            Err(error) => Err(uia_error(
                "IUIAutomationElement::GetCurrentPatternAs",
                &error,
            )),
        }
    }

    #[cfg(windows)]
    fn uia_element_has_exact_focus(
        context: &UiaContext,
        element: &IUIAutomationElement,
    ) -> Result<bool, String> {
        context.deadline.ensure_remaining()?;
        let Some(focused) = optional_uia_element(
            unsafe { context.automation.GetFocusedElement() },
            "IUIAutomation::GetFocusedElement",
        )?
        else {
            return Ok(false);
        };
        context.deadline.ensure_remaining()?;
        unsafe { context.automation.CompareElements(element, &focused) }
            .map(|same| same.as_bool())
            .map_err(|error| uia_error("IUIAutomation::CompareElements", &error))
    }

    #[cfg(windows)]
    fn uia_string(
        result: windows::core::Result<windows::core::BSTR>,
        operation: &str,
    ) -> Result<Option<String>, String> {
        let value = result.map_err(|error| uia_error(operation, &error))?;
        let value = bounded_text(&value.to_string());
        Ok((!value.is_empty()).then_some(value))
    }

    #[cfg(windows)]
    pub(super) fn uia_control_role(control_type: UIA_CONTROLTYPE_ID) -> String {
        let role = if control_type == UIA_WindowControlTypeId {
            Some("AXWindow")
        } else if control_type == UIA_ButtonControlTypeId {
            Some("AXButton")
        } else if control_type == UIA_EditControlTypeId {
            Some("AXTextField")
        } else if control_type == UIA_DocumentControlTypeId {
            Some("AXTextArea")
        } else if control_type == UIA_HyperlinkControlTypeId {
            Some("AXLink")
        } else if control_type == UIA_CheckBoxControlTypeId {
            Some("AXCheckBox")
        } else if control_type == UIA_RadioButtonControlTypeId {
            Some("AXRadioButton")
        } else if control_type == UIA_ComboBoxControlTypeId {
            Some("AXComboBox")
        } else if control_type == UIA_ListControlTypeId {
            Some("AXList")
        } else if control_type == UIA_ListItemControlTypeId
            || control_type == UIA_TreeItemControlTypeId
            || control_type == UIA_DataItemControlTypeId
        {
            Some("AXRow")
        } else if control_type == UIA_MenuControlTypeId {
            Some("AXMenu")
        } else if control_type == UIA_MenuItemControlTypeId {
            Some("AXMenuItem")
        } else if control_type == UIA_TreeControlTypeId {
            Some("AXOutline")
        } else if control_type == UIA_TabControlTypeId {
            Some("AXTabGroup")
        } else if control_type == UIA_TabItemControlTypeId {
            Some("AXRadioButton")
        } else if control_type == UIA_TextControlTypeId {
            Some("AXStaticText")
        } else if control_type == UIA_TableControlTypeId
            || control_type == UIA_DataGridControlTypeId
        {
            Some("AXTable")
        } else if control_type == UIA_ToolBarControlTypeId {
            Some("AXToolbar")
        } else if control_type == UIA_ScrollBarControlTypeId {
            Some("AXScrollBar")
        } else if control_type == UIA_SliderControlTypeId {
            Some("AXSlider")
        } else if control_type == UIA_SpinnerControlTypeId {
            Some("AXIncrementor")
        } else if control_type == UIA_ProgressBarControlTypeId {
            Some("AXProgressIndicator")
        } else if control_type == UIA_HeaderItemControlTypeId {
            Some("AXColumn")
        } else if control_type == UIA_PaneControlTypeId
            || control_type == UIA_GroupControlTypeId
            || control_type == UIA_CustomControlTypeId
            || control_type == UIA_HeaderControlTypeId
            || control_type == UIA_StatusBarControlTypeId
            || control_type == UIA_SeparatorControlTypeId
            || control_type == UIA_ToolTipControlTypeId
        {
            Some("AXGroup")
        } else {
            None
        };
        role.map(str::to_string)
            .unwrap_or_else(|| format!("UIAControlType({})", control_type.0))
    }

    #[cfg(windows)]
    pub(super) fn uia_semantic_focus_role(role: &str) -> bool {
        role == "AXTextField"
    }

    #[cfg(windows)]
    pub(super) fn uia_semantic_text_input_role(role: &str) -> bool {
        role == "AXTextField"
    }

    #[cfg(windows)]
    fn uia_runtime_id(
        context: &UiaContext,
        element: &IUIAutomationElement,
    ) -> Result<Vec<i32>, String> {
        context.deadline.ensure_remaining()?;
        let array = unsafe { element.GetRuntimeId() }.map_err(|error| {
            if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE {
                "stale_element: UI Automation element is no longer available".to_string()
            } else {
                uia_error("IUIAutomationElement::GetRuntimeId", &error)
            }
        })?;
        let array = OwnedSafeArray::new(array)?;
        if unsafe { SafeArrayGetDim(array.as_ptr()) } != 1
            || unsafe { SafeArrayGetElemsize(array.as_ptr()) } != std::mem::size_of::<i32>() as u32
        {
            return Err(
                "stale_element: UI Automation runtime id has an invalid SAFEARRAY shape"
                    .to_string(),
            );
        }
        let lower = unsafe { SafeArrayGetLBound(array.as_ptr(), 1) }
            .map_err(|error| uia_error("SafeArrayGetLBound(runtime id)", &error))?;
        let upper = unsafe { SafeArrayGetUBound(array.as_ptr(), 1) }
            .map_err(|error| uia_error("SafeArrayGetUBound(runtime id)", &error))?;
        let length = upper
            .checked_sub(lower)
            .and_then(|span| span.checked_add(1))
            .and_then(|length| usize::try_from(length).ok())
            .filter(|length| (1..=MAX_UIA_RUNTIME_ID_ELEMENTS).contains(length))
            .ok_or_else(|| {
                "stale_element: UI Automation runtime id length is invalid or exceeds the bound"
                    .to_string()
            })?;
        let mut runtime_id = Vec::with_capacity(length);
        for offset in 0..length {
            context.deadline.ensure_remaining()?;
            let index = lower
                .checked_add(i32::try_from(offset).map_err(|_| {
                    "stale_element: UI Automation runtime id index exceeds the bound".to_string()
                })?)
                .ok_or_else(|| {
                    "stale_element: UI Automation runtime id index overflow".to_string()
                })?;
            let mut value = 0i32;
            unsafe {
                SafeArrayGetElement(
                    array.as_ptr(),
                    &index,
                    (&mut value as *mut i32).cast::<std::ffi::c_void>(),
                )
            }
            .map_err(|error| uia_error("SafeArrayGetElement(runtime id)", &error))?;
            runtime_id.push(value);
        }
        Ok(runtime_id)
    }

    #[cfg(windows)]
    fn uia_fingerprint(
        context: &UiaContext,
        element: &IUIAutomationElement,
        inherited_protected: bool,
    ) -> Result<ElementFingerprint, String> {
        context.deadline.ensure_remaining()?;
        let control_type = unsafe { element.CurrentControlType() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
        context.deadline.ensure_remaining()?;
        let is_password = unsafe { element.CurrentIsPassword() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsPassword", &error))?
            .as_bool();
        let protected = inherited_protected || is_password;
        let native_runtime_id = uia_runtime_id(context, element)?;
        context.deadline.ensure_remaining()?;
        let identifier = uia_string(
            unsafe { element.CurrentAutomationId() },
            "IUIAutomationElement::CurrentAutomationId",
        )?;
        let title = if protected {
            None
        } else {
            context.deadline.ensure_remaining()?;
            uia_string(
                unsafe { element.CurrentName() },
                "IUIAutomationElement::CurrentName",
            )?
        };
        let description = if protected {
            None
        } else {
            context.deadline.ensure_remaining()?;
            uia_string(
                unsafe { element.CurrentHelpText() },
                "IUIAutomationElement::CurrentHelpText",
            )?
        };
        Ok(ElementFingerprint {
            role: uia_control_role(control_type),
            native_runtime_id,
            subrole: None,
            identifier,
            title,
            description,
            placeholder: None,
            protected,
        })
    }

    #[cfg(windows)]
    fn uia_text_pattern(
        context: &UiaContext,
        element: &IUIAutomationElement,
    ) -> Result<Option<IUIAutomationValuePattern>, String> {
        optional_uia_pattern::<IUIAutomationValuePattern>(context, element, UIA_ValuePatternId)
    }

    #[cfg(windows)]
    fn uia_value_pattern_current_value(
        context: &UiaContext,
        pattern: &IUIAutomationValuePattern,
    ) -> Result<String, String> {
        context.deadline.ensure_remaining()?;
        uia_string(
            unsafe { pattern.CurrentValue() },
            "IUIAutomationValuePattern::CurrentValue",
        )
        .map(|value| value.unwrap_or_default())
    }

    #[cfg(windows)]
    fn uia_value_pattern_writable(
        context: &UiaContext,
        pattern: &IUIAutomationValuePattern,
    ) -> Result<bool, String> {
        context.deadline.ensure_remaining()?;
        unsafe { pattern.CurrentIsReadOnly() }
            .map(|read_only| !read_only.as_bool())
            .map_err(|error| uia_error("IUIAutomationValuePattern::CurrentIsReadOnly", &error))
    }

    #[cfg(windows)]
    fn uia_text_value(
        context: &UiaContext,
        element: &IUIAutomationElement,
    ) -> Result<Option<String>, String> {
        let Some(pattern) = uia_text_pattern(context, element)? else {
            return Ok(None);
        };
        uia_value_pattern_current_value(context, &pattern).map(Some)
    }

    #[cfg(windows)]
    fn uia_children(
        context: &UiaContext,
        element: &IUIAutomationElement,
        limit: usize,
    ) -> Result<(Vec<IUIAutomationElement>, bool), String> {
        let mut output = Vec::with_capacity(limit.min(32));
        context.deadline.ensure_remaining()?;
        let mut current = optional_uia_element(
            unsafe { context.walker.GetFirstChildElement(element) },
            "IUIAutomationTreeWalker::GetFirstChildElement",
        )?;
        while let Some(element) = current {
            if output.len() >= limit {
                return Ok((output, true));
            }
            context.deadline.ensure_remaining()?;
            current = optional_uia_element(
                unsafe { context.walker.GetNextSiblingElement(&element) },
                "IUIAutomationTreeWalker::GetNextSiblingElement",
            )?;
            output.push(element);
        }
        Ok((output, false))
    }

    #[cfg(windows)]
    pub(super) fn win_hwnd(native_id: u32) -> Result<WinHwnd, String> {
        let hwnd = WinHwnd(native_id as i32 as isize as *mut std::ffi::c_void);
        if hwnd.0.is_null() {
            Err("stale_surface: window handle is invalid".to_string())
        } else {
            Ok(hwnd)
        }
    }

    #[cfg(windows)]
    fn exact_uia_window(
        context: &UiaContext,
        surface: &SurfaceRecord,
    ) -> Result<IUIAutomationElement, String> {
        let _window = resolve_surface_window(surface)?;
        let hwnd = win_hwnd(surface.native_id)?;
        context.deadline.ensure_remaining()?;
        let root = unsafe { context.automation.ElementFromHandle(hwnd) }
            .map_err(|error| uia_error("IUIAutomation::ElementFromHandle", &error))?;
        context.deadline.ensure_remaining()?;
        let current_hwnd = unsafe { root.CurrentNativeWindowHandle() }.map_err(|error| {
            uia_error("IUIAutomationElement::CurrentNativeWindowHandle", &error)
        })?;
        context.deadline.ensure_remaining()?;
        let current_pid = unsafe { root.CurrentProcessId() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentProcessId", &error))?;
        let current_pid = u32::try_from(current_pid)
            .map_err(|_| "stale_surface: UI Automation process id is invalid".to_string())?;
        if current_hwnd != hwnd || current_pid != surface.pid {
            return Err(
                "stale_surface: UI Automation root no longer matches the observed HWND/PID"
                    .to_string(),
            );
        }
        Ok(root)
    }

    #[cfg(windows)]
    fn resolve_uia_element(
        context: &UiaContext,
        surface: &SurfaceRecord,
        element: &ElementRecord,
    ) -> Result<IUIAutomationElement, String> {
        if element.lineage.len() != element.path.len() + 1 {
            return Err("stale_element: UIA element correlation lineage is incomplete".to_string());
        }
        let mut current = exact_uia_window(context, surface)?;
        let current_root = uia_fingerprint(context, &current, false)?;
        if current_root != element.lineage[0] {
            return Err("stale_element: UIA root identity changed since observation".to_string());
        }
        for (depth, &index) in element.path.iter().enumerate() {
            let (children, has_more) = uia_children(context, &current, index + 1)?;
            if children.len() <= index {
                return Err("stale_element: UIA child path no longer exists".to_string());
            }
            if has_more && index >= super::MAX_ACCESSIBILITY_NODES {
                return Err("stale_element: UIA child path exceeds bounded correlation".to_string());
            }
            current = children[index].clone();
            let current_fingerprint =
                uia_fingerprint(context, &current, element.lineage[depth].protected)?;
            if current_fingerprint != element.lineage[depth + 1] {
                return Err(
                    "stale_element: UIA element lineage changed since observation".to_string(),
                );
            }
        }
        Ok(current)
    }

    #[cfg(windows)]
    pub(super) fn accessibility_status() -> Result<Value, String> {
        let _context = UiaContext::new()?;
        Ok(json!({"platform": "windows", "trusted": true}))
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
    pub(super) fn element_state(
        surface_id: &str,
        element_id: &str,
        observation_generation: u32,
        surface: &SurfaceRecord,
        element: &ElementRecord,
    ) -> Result<Value, String> {
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        let target = validate_element_state_target(element)?;
        let deadline = AxObservationDeadline::new();
        let current = resolve_correlated_element(surface, element, &deadline)?;
        let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
        let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
        let protected = element.contains_protected_content()
            || element.lineage.iter().any(is_secure_text_fingerprint);
        let enabled_for_effect = enabled != Some(false);
        let can_press =
            !protected && enabled_for_effect && ax_supports_action(&deadline, &current, "AXPress")?;
        let can_focus = !protected
            && enabled_for_effect
            && ax_attribute_settable(&deadline, &current, "AXFocused")?;

        let supported_text = !protected && is_supported_text_input_fingerprint(target);
        let (value_empty, can_input_text) = if supported_text {
            let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
            let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
            let value_empty = current_value.as_deref().map(str::is_empty);
            let can_input_text = enabled != Some(false)
                && focused == Some(true)
                && value_settable
                && value_empty == Some(true);
            (value_empty, can_input_text)
        } else {
            (None, false)
        };
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "element_id": element_id,
            "observation_generation": observation_generation,
            "enabled": enabled,
            "focused": focused,
            "protected": protected,
            "value_empty": value_empty,
            "can_press": can_press,
            "can_focus": can_focus,
            "can_input_text": can_input_text,
        }))
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
    pub(super) fn scroll_to_element(
        surface_id: &str,
        element_id: &str,
        surface: &SurfaceRecord,
        element: &ElementRecord,
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
                "permission_denied: macOS Accessibility protected content cannot be scrolled"
                    .to_string(),
            );
        }
        if !target_fingerprint.has_positive_evidence() {
            return Err(
                "stale_element: AX element lacks positive correlation evidence for scrolling"
                    .to_string(),
            );
        }
        let deadline = AxObservationDeadline::new();
        let current = resolve_correlated_element(surface, element, &deadline)?;
        if !ax_supports_action(&deadline, &current, "AXScrollToVisible")? {
            return Err(
                "scroll_failed: AX element does not support the AXScrollToVisible action"
                    .to_string(),
            );
        }
        prepare_ax_call(&deadline, &current)?;
        let error =
            unsafe { current.perform_action(&CFString::from_static_str("AXScrollToVisible")) };
        if error != AXError::Success {
            return Err(scroll_attempt_error(
                "AXUIElementPerformAction(AXScrollToVisible)",
                error,
            ));
        }
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "element_id": element_id,
            "success": true,
        }))
    }

    #[cfg(target_os = "macos")]
    fn validate_key_input_target(
        deadline: &AxObservationDeadline,
        application: &AXUIElement,
        exact_window: &CFRetained<AXUIElement>,
    ) -> Result<(), String> {
        if optional_ax_bool(deadline, application, "AXFrontmost")? != Some(true) {
            return Err(
                "key_input_failed: exact surface application must already be frontmost".to_string(),
            );
        }
        let focused_window = optional_ax_value(deadline, application, "AXFocusedWindow")?
            .ok_or_else(|| {
                "key_input_failed: exact surface application has no focused window".to_string()
            })?
            .downcast::<AXUIElement>()
            .map_err(|_| {
                "accessibility_failed: AXFocusedWindow is not an AXUIElement".to_string()
            })?;
        if &focused_window != exact_window {
            return Err(
                "key_input_failed: exact surface must already be the focused window".to_string(),
            );
        }

        if let Some(focused_value) = optional_ax_value(deadline, application, "AXFocusedUIElement")?
        {
            let focused_element = focused_value.downcast::<AXUIElement>().map_err(|_| {
                "accessibility_failed: AXFocusedUIElement is not an AXUIElement".to_string()
            })?;
            let fingerprint = element_fingerprint(deadline, &focused_element, false)?;
            if fingerprint.protected || is_secure_text_fingerprint(&fingerprint) {
                return Err(
                    "permission_denied: protected or secure Accessibility content cannot receive key input"
                        .to_string(),
                );
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(super) fn key_input(
        surface_id: &str,
        surface: &SurfaceRecord,
        key: &str,
        modifiers: &[String],
    ) -> Result<Value, String> {
        validate_key_input(key, modifiers)?;
        if !unsafe { AXIsProcessTrusted() } {
            return Err(
                "permission_denied: macOS Accessibility permission is not granted".to_string(),
            );
        }
        if !CGPreflightPostEventAccess() {
            return Err(
                "permission_denied: macOS event-posting permission is not granted".to_string(),
            );
        }

        let pid = checked_surface_pid(surface)?;
        let deadline = AxObservationDeadline::new();
        let exact_window = exact_ax_window(surface, &deadline)?;
        let application = unsafe { AXUIElement::new_application(pid) };

        let key_code = key_code(key)?;
        let flags = key_modifier_flags(modifiers)?;
        let key_down = CGEvent::new_keyboard_event(None, key_code, true).ok_or_else(|| {
            "key_input_failed: could not create native key-down event".to_string()
        })?;
        let key_up = CGEvent::new_keyboard_event(None, key_code, false)
            .ok_or_else(|| "key_input_failed: could not create native key-up event".to_string())?;
        CGEvent::set_flags(Some(&key_down), flags);
        CGEvent::set_flags(Some(&key_up), flags);

        // This is the final authority/privacy check before the first effect. Quartz
        // posts keyboard events to a process rather than a specific window, so keep
        // the exact focused-window check as close to dispatch as possible.
        validate_key_input_target(&deadline, &application, &exact_window)?;
        deadline.ensure_remaining()?;

        CGEvent::post_to_pid(pid, Some(&key_down));
        CGEvent::post_to_pid(pid, Some(&key_up));
        Ok(json!({
            "platform": "macos",
            "surface_id": surface_id,
            "key": key,
            "modifiers": modifiers,
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
        surface_id: &str,
        surface: &SurfaceRecord,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<AccessibilityTreeResult, String> {
        let context = UiaContext::new()?;
        let root = exact_uia_window(&context, surface)?;
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
            current,
            parent_element_id,
            depth,
            path,
            mut lineage,
            inherited_protected,
        )) = queue.pop_front()
        {
            if nodes.len() >= max_nodes {
                truncated = true;
                break;
            }

            let element_id = format!("element_{}", Uuid::new_v4().simple());
            let fingerprint = uia_fingerprint(&context, &current, inherited_protected)?;
            let role = fingerprint.role.clone();
            let subrole = fingerprint.subrole.clone();
            let title = fingerprint.title.clone();
            let description = fingerprint.description.clone();
            let placeholder = fingerprint.placeholder.clone();
            let protected = fingerprint.protected;
            let value = if protected || !is_supported_text_input_fingerprint(&fingerprint) {
                None
            } else {
                uia_text_value(&context, &current)?
            };
            context.deadline.ensure_remaining()?;
            let enabled = unsafe { current.CurrentIsEnabled() }
                .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
                .as_bool();
            context.deadline.ensure_remaining()?;
            let focused = unsafe { current.CurrentHasKeyboardFocus() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentHasKeyboardFocus", &error)
                })?
                .as_bool();
            lineage.push(fingerprint);

            let reserved = nodes.len() + queue.len() + 1;
            let remaining = max_nodes.saturating_sub(reserved);
            let inspect_limit = if depth < max_depth {
                remaining.saturating_add(1).max(1)
            } else {
                1
            };
            let (children, has_more_children) = uia_children(&context, &current, inspect_limit)?;
            let child_count = children.len() + usize::from(has_more_children);

            if depth < max_depth {
                if children.len() > remaining || has_more_children {
                    truncated = true;
                }
                for (index, child) in children.into_iter().take(remaining).enumerate() {
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
        let node_count = nodes.len();
        Ok(AccessibilityTreeResult {
            output: json!({
                "platform": "windows",
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

    #[cfg(windows)]
    pub(super) fn element_state(
        surface_id: &str,
        element_id: &str,
        observation_generation: u32,
        surface: &SurfaceRecord,
        element: &ElementRecord,
    ) -> Result<Value, String> {
        let target = element.target_fingerprint().ok_or_else(|| {
            "stale_element: UIA element correlation lineage is incomplete".to_string()
        })?;
        if !target.has_positive_evidence() {
            return Err(
                "stale_element: UIA element lacks positive correlation evidence for state"
                    .to_string(),
            );
        }
        let context = UiaContext::new()?;
        let current = resolve_uia_element(&context, surface, element)?;
        context.deadline.ensure_remaining()?;
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
            .as_bool();
        let focused = uia_element_has_exact_focus(&context, &current)?;
        let protected = element.contains_protected_content();
        let (value_empty, value_writable) =
            if !protected && is_supported_text_input_fingerprint(target) {
                match uia_text_pattern(&context, &current)? {
                    Some(pattern) => {
                        let value = uia_value_pattern_current_value(&context, &pattern)?;
                        let writable = uia_value_pattern_writable(&context, &pattern)?;
                        (Some(value.is_empty()), writable)
                    }
                    None => (None, false),
                }
            } else {
                (None, false)
            };
        let hwnd = win_hwnd(surface.native_id)?;
        let surface_foreground = unsafe { GetForegroundWindow() == hwnd };
        let can_press = if protected || !enabled {
            false
        } else {
            optional_uia_pattern::<IUIAutomationInvokePattern>(
                &context,
                &current,
                UIA_InvokePatternId,
            )?
            .is_some()
        };
        let can_focus = if protected
            || !enabled
            || !surface_foreground
            || !uia_semantic_focus_role(&target.role)
        {
            false
        } else {
            context.deadline.ensure_remaining()?;
            unsafe { current.CurrentIsKeyboardFocusable() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                })?
                .as_bool()
        };
        let can_input_text = !protected
            && enabled
            && surface_foreground
            && focused
            && uia_semantic_text_input_role(&target.role)
            && value_writable
            && value_empty == Some(true);

        Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "element_id": element_id,
            "observation_generation": observation_generation,
            "enabled": enabled,
            "focused": focused,
            "protected": protected,
            "value_empty": value_empty,
            "can_press": can_press,
            "can_focus": can_focus,
            "can_input_text": can_input_text,
        }))
    }

    #[cfg(windows)]
    pub(super) fn windows_window_activation_attempt_error(operation: &str) -> String {
        format!(
            "outcome_unknown: {operation} did not establish the exact foreground-window postcondition after the native activation attempt"
        )
    }

    #[cfg(windows)]
    pub(super) fn windows_control_attempt_error(operation: &str) -> String {
        format!(
            "outcome_unknown: {operation} returned after the exact Windows UI Automation control effect was attempted"
        )
    }

    #[cfg(windows)]
    pub(super) fn windows_text_input_attempt_error(operation: &str) -> String {
        format!(
            "outcome_unknown: {operation} returned after the exact Windows UI Automation text write was attempted"
        )
    }

    #[cfg(windows)]
    pub(super) fn activate_window(
        surface_id: &str,
        surface: &SurfaceRecord,
    ) -> Result<Value, String> {
        // Resolve the exact xcap identity immediately before the first native
        // effect. This never falls back to an application name, PID, or title.
        let _window = resolve_surface_window(surface)?;
        let hwnd = win_hwnd(surface.native_id)?;
        let already_foreground = unsafe { GetForegroundWindow() == hwnd };
        let minimized = unsafe { IsIconic(hwnd).as_bool() };
        if already_foreground && !minimized {
            return Ok(json!({
                "platform": "windows",
                "surface_id": surface_id,
                "success": true,
            }));
        }

        // Obtain the exact UIA root before the first effect. This revalidates
        // the same HWND/PID lineage used by read-only Windows observation.
        let context = UiaContext::new()?;
        let root = exact_uia_window(&context, surface)?;
        context.deadline.ensure_remaining()?;
        let control_type = unsafe { root.CurrentControlType() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
        if control_type != UIA_WindowControlTypeId {
            return Err(
                "control_failed: exact Windows UIA root is not an activatable Window control"
                    .to_string(),
            );
        }
        let mut prior_effect = false;
        if minimized {
            // Restoring a foreign UI thread must not synchronously wait on a
            // stalled target. Queue the exact restore request asynchronously,
            // then observe only the local window-state predicate for a bounded
            // interval before proceeding.
            let restore_expires_at = Instant::now() + Duration::from_secs(2);
            let _ = unsafe { ShowWindowAsync(hwnd, SW_RESTORE) };
            prior_effect = true;
            while unsafe { IsIconic(hwnd).as_bool() } {
                if let Err(error) = context.deadline.ensure_remaining() {
                    return Err(windows_window_activation_attempt_error(&error));
                }
                if Instant::now() >= restore_expires_at {
                    return Err(windows_window_activation_attempt_error(
                        "ShowWindowAsync(SW_RESTORE) timeout",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        // A background Runner is normally denied by SetForegroundWindow.
        // UI Automation's exact SetFocus is the native automation primitive;
        // once attempted, any error or mismatched postcondition is uncertain.
        if let Err(error) = context.deadline.ensure_remaining() {
            if prior_effect {
                return Err(windows_window_activation_attempt_error(&error));
            }
            return Err(error);
        }
        if let Err(error) = unsafe { root.SetFocus() } {
            return Err(windows_window_activation_attempt_error(&format!(
                "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
                error.code().0 as u32
            )));
        }
        if unsafe { GetForegroundWindow() != hwnd } {
            return Err(windows_window_activation_attempt_error(
                "IUIAutomationElement::SetFocus postcondition",
            ));
        }

        Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "success": true,
        }))
    }

    #[cfg(windows)]
    pub(super) fn control(
        surface_id: &str,
        element_id: &str,
        surface: &SurfaceRecord,
        element: &ElementRecord,
        action: ComputerAction,
    ) -> Result<Value, String> {
        let target = element.target_fingerprint().ok_or_else(|| {
            "stale_element: UIA element correlation lineage is incomplete".to_string()
        })?;
        if element.contains_protected_content() {
            return Err(
                "permission_denied: Windows UI Automation protected content cannot be controlled"
                    .to_string(),
            );
        }
        if !target.has_positive_evidence() {
            return Err(
                "stale_element: UIA element lacks positive correlation evidence for control"
                    .to_string(),
            );
        }

        let context = UiaContext::new()?;
        let current = resolve_uia_element(&context, surface, element)?;
        context.deadline.ensure_remaining()?;
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
            .as_bool();
        if !enabled {
            return Err("control_failed: UI Automation element is disabled".to_string());
        }

        match action {
            ComputerAction::Press => {
                let pattern = optional_uia_pattern::<IUIAutomationInvokePattern>(
                    &context,
                    &current,
                    UIA_InvokePatternId,
                )?
                .ok_or_else(|| {
                    "control_failed: UI Automation element does not support InvokePattern"
                        .to_string()
                })?;
                context.deadline.ensure_remaining()?;
                if let Err(error) = unsafe { pattern.Invoke() } {
                    return Err(windows_control_attempt_error(&format!(
                        "IUIAutomationInvokePattern::Invoke HRESULT(0x{:08X})",
                        error.code().0 as u32
                    )));
                }
            }
            ComputerAction::Focus => {
                if !uia_semantic_focus_role(&target.role) {
                    return Err(
                        "control_failed: UI Automation element role is outside the bounded semantic focus set"
                            .to_string(),
                    );
                }
                let hwnd = win_hwnd(surface.native_id)?;
                if unsafe { GetForegroundWindow() != hwnd } {
                    return Err(
                        "control_failed: exact Windows surface must already be foreground before element focus"
                            .to_string(),
                    );
                }
                context.deadline.ensure_remaining()?;
                let focusable = unsafe { current.CurrentIsKeyboardFocusable() }
                    .map_err(|error| {
                        uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                    })?
                    .as_bool();
                if !focusable {
                    return Err(
                        "control_failed: UI Automation element is not keyboard-focusable"
                            .to_string(),
                    );
                }
                let already_focused = uia_element_has_exact_focus(&context, &current)?;
                if !already_focused {
                    context.deadline.ensure_remaining()?;
                    if let Err(error) = unsafe { current.SetFocus() } {
                        return Err(windows_control_attempt_error(&format!(
                            "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
                            error.code().0 as u32
                        )));
                    }
                    let focus_expires_at = Instant::now() + Duration::from_secs(1);
                    loop {
                        if let Err(error) = context.deadline.ensure_remaining() {
                            return Err(windows_control_attempt_error(&error));
                        }
                        let focused = match uia_element_has_exact_focus(&context, &current) {
                            Ok(focused) => focused,
                            Err(error) => return Err(windows_control_attempt_error(&error)),
                        };
                        if focused {
                            break;
                        }
                        if Instant::now() >= focus_expires_at {
                            return Err(windows_control_attempt_error(
                                "IUIAutomationElement::SetFocus postcondition timeout",
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }

        Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "element_id": element_id,
            "action": action.as_str(),
            "success": true,
        }))
    }

    #[cfg(windows)]
    pub(super) fn scroll_to_element(
        _surface_id: &str,
        _element_id: &str,
        _surface: &SurfaceRecord,
        _element: &ElementRecord,
    ) -> Result<Value, String> {
        Err("unsupported_platform: computer scroll is unavailable on this platform".to_string())
    }

    #[cfg(windows)]
    pub(super) fn key_input(
        _surface_id: &str,
        _surface: &SurfaceRecord,
        _key: &str,
        _modifiers: &[String],
    ) -> Result<Value, String> {
        Err("unsupported_platform: computer key input is unavailable on this platform".to_string())
    }

    #[cfg(windows)]
    pub(super) fn input_text(
        surface_id: &str,
        element_id: &str,
        surface: &SurfaceRecord,
        element: &ElementRecord,
        text: &str,
    ) -> Result<Value, String> {
        let text_bytes = validate_input_text(text)?;
        let target = element.target_fingerprint().ok_or_else(|| {
            "stale_element: UIA element correlation lineage is incomplete".to_string()
        })?;
        if element.contains_protected_content() {
            return Err(
                "permission_denied: Windows UI Automation protected content cannot receive text input"
                    .to_string(),
            );
        }
        if !target.has_positive_evidence() {
            return Err(
                "stale_element: UIA element lacks positive correlation evidence for text input"
                    .to_string(),
            );
        }
        if !uia_semantic_text_input_role(&target.role) {
            return Err(
                "input_failed: UI Automation element is outside the bounded Windows text-entry role set"
                    .to_string(),
            );
        }

        let context = UiaContext::new()?;
        let current = resolve_uia_element(&context, surface, element)?;
        context.deadline.ensure_remaining()?;
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
            .as_bool();
        if !enabled {
            return Err("input_failed: UI Automation text element is disabled".to_string());
        }
        let pattern = uia_text_pattern(&context, &current)?.ok_or_else(|| {
            "input_failed: UI Automation text element does not expose ValuePattern".to_string()
        })?;
        if !uia_value_pattern_writable(&context, &pattern)? {
            return Err("input_failed: UI Automation ValuePattern is read-only".to_string());
        }

        let hwnd = win_hwnd(surface.native_id)?;
        if unsafe { GetForegroundWindow() != hwnd } {
            return Err(
                "input_failed: exact Windows surface must already be foreground before text input"
                    .to_string(),
            );
        }
        if !uia_element_has_exact_focus(&context, &current)? {
            return Err(
                "input_failed: exact Windows text element must already have keyboard focus"
                    .to_string(),
            );
        }

        // Keep emptiness as the final state read before the native write. The
        // value never leaves the Runner; only the empty/non-empty affordance is
        // exposed through element_state.
        let current_value = uia_value_pattern_current_value(&context, &pattern)?;
        if !current_value.is_empty() {
            return Err(
                "input_failed: UI Automation ValuePattern must be empty before bounded text input; observe and reconcile before retrying"
                    .to_string(),
            );
        }

        let value = windows::core::BSTR::from(text);
        context.deadline.ensure_remaining()?;
        if let Err(error) = unsafe { pattern.SetValue(&value) } {
            return Err(windows_text_input_attempt_error(&format!(
                "IUIAutomationValuePattern::SetValue HRESULT(0x{:08X})",
                error.code().0 as u32
            )));
        }
        Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "element_id": element_id,
            "text_bytes": text_bytes,
            "success": true,
        }))
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
        hwnd: SysHwnd,
        hdc: HDC,
    }

    #[cfg(windows)]
    impl WindowDc {
        fn acquire(hwnd: SysHwnd) -> Result<Self, String> {
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
        let hwnd = native_id as i32 as isize as SysHwnd;
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
            ensure_platform_capture_bound(&window, width, height)?;
            capture_window_gdi(surface.native_id, width, height)
        }
    }
}

#[cfg(all(test, windows))]
mod windows_uia_tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use windows::Win32::UI::Accessibility::{
        UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_HyperlinkControlTypeId, UIA_WindowControlTypeId,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

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

    const WINDOWS_CONTROL_FIXTURE_TITLE: &str = "WebCodex Windows UIA Control Smoke";

    struct WindowsControlFixture {
        child: Child,
    }

    impl WindowsControlFixture {
        fn start() -> Self {
            let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
$form = New-Object System.Windows.Forms.Form
$form.Text = 'WebCodex Windows UIA Control Smoke'
$form.StartPosition = 'Manual'
$form.Location = New-Object System.Drawing.Point(120, 120)
$form.Size = New-Object System.Drawing.Size(700, 300)
$input = New-Object System.Windows.Forms.TextBox
$input.Name = 'SmokeInput'
$input.AccessibleName = 'Smoke input'
$input.Location = New-Object System.Drawing.Point(24, 44)
$input.Size = New-Object System.Drawing.Size(280, 28)
$button = New-Object System.Windows.Forms.Button
$button.Name = 'SmokePress'
$button.Text = 'Smoke press'
$button.Location = New-Object System.Drawing.Point(24, 96)
$button.Size = New-Object System.Drawing.Size(140, 32)
$status = New-Object System.Windows.Forms.Label
$status.Name = 'SmokeStatus'
$status.Text = 'ready'
$status.Location = New-Object System.Drawing.Point(24, 148)
$status.Size = New-Object System.Drawing.Size(140, 24)
$script:twinPrimary = New-Object System.Windows.Forms.Button
$script:twinPrimary.Name = 'TwinAction'
$script:twinPrimary.Text = 'Twin action'
$script:twinPrimary.AccessibleName = 'Twin action'
$script:twinPrimary.Location = New-Object System.Drawing.Point(210, 96)
$script:twinPrimary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinPrimary.TabIndex = 10
$script:twinPrimary.Add_Click({ param($sender, $eventArgs) $sender.Text = 'wrong target invoked' })
$script:twinSecondary = New-Object System.Windows.Forms.Button
$script:twinSecondary.Name = 'TwinAction'
$script:twinSecondary.Text = 'Twin action'
$script:twinSecondary.AccessibleName = 'Twin action'
$script:twinSecondary.Location = New-Object System.Drawing.Point(350, 96)
$script:twinSecondary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinSecondary.TabIndex = 11
$script:twinSecondary.Add_Click({ param($sender, $eventArgs) $sender.Text = 'wrong target invoked' })
$replaceTwins = New-Object System.Windows.Forms.Button
$replaceTwins.Name = 'ReplaceTwins'
$replaceTwins.Text = 'Replace twins'
$replaceTwins.AccessibleName = 'Replace twins'
$replaceTwins.Location = New-Object System.Drawing.Point(210, 148)
$replaceTwins.Size = New-Object System.Drawing.Size(140, 32)
$replaceTwins.TabIndex = 12
$replaceTwins.Add_Click({
    param($sender, $eventArgs)
    $primaryIndex = $form.Controls.GetChildIndex($script:twinPrimary)
    $secondaryIndex = $form.Controls.GetChildIndex($script:twinSecondary)
    $form.Controls.Remove($script:twinPrimary)
    $form.Controls.Remove($script:twinSecondary)
    $script:twinPrimary.Dispose()
    $script:twinSecondary.Dispose()
    $script:twinPrimary = New-Object System.Windows.Forms.Button
    $script:twinPrimary.Name = 'TwinAction'
    $script:twinPrimary.Text = 'Twin action'
    $script:twinPrimary.AccessibleName = 'Twin action'
    $script:twinPrimary.Location = New-Object System.Drawing.Point(210, 96)
    $script:twinPrimary.Size = New-Object System.Drawing.Size(120, 32)
    $script:twinPrimary.TabIndex = 10
    $script:twinPrimary.Add_Click({ param($replacementSender, $replacementEventArgs) $replacementSender.Text = 'wrong target invoked' })
    $script:twinSecondary = New-Object System.Windows.Forms.Button
    $script:twinSecondary.Name = 'TwinAction'
    $script:twinSecondary.Text = 'Twin action'
    $script:twinSecondary.AccessibleName = 'Twin action'
    $script:twinSecondary.Location = New-Object System.Drawing.Point(350, 96)
    $script:twinSecondary.Size = New-Object System.Drawing.Size(120, 32)
    $script:twinSecondary.TabIndex = 11
    $script:twinSecondary.Add_Click({ param($replacementSender, $replacementEventArgs) $replacementSender.Text = 'wrong target invoked' })
    $form.Controls.Add($script:twinPrimary)
    $form.Controls.Add($script:twinSecondary)
    $form.Controls.SetChildIndex($script:twinPrimary, $primaryIndex)
    $form.Controls.SetChildIndex($script:twinSecondary, $secondaryIndex)
})
$button.Add_Click({ param($sender, $eventArgs) $sender.Text = 'clicked' })
$form.Controls.Add($input)
$form.Controls.Add($button)
$form.Controls.Add($status)
$form.Controls.Add($script:twinPrimary)
$form.Controls.Add($script:twinSecondary)
$form.Controls.Add($replaceTwins)
[System.Windows.Forms.Application]::Run($form)
"#;
            let child = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-STA",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    script,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("launch private WinForms control fixture");
            Self { child }
        }
    }

    impl Drop for WindowsControlFixture {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn computer_windows_uia_control_types_use_existing_semantic_roles() {
        assert_eq!(
            platform::uia_control_role(UIA_WindowControlTypeId),
            "AXWindow"
        );
        assert_eq!(
            platform::uia_control_role(UIA_ButtonControlTypeId),
            "AXButton"
        );
        assert_eq!(
            platform::uia_control_role(UIA_EditControlTypeId),
            "AXTextField"
        );
        assert_eq!(
            platform::uia_control_role(UIA_DocumentControlTypeId),
            "AXTextArea"
        );
        assert_eq!(
            platform::uia_control_role(UIA_HyperlinkControlTypeId),
            "AXLink"
        );
        assert_eq!(
            platform::uia_control_role(UIA_CheckBoxControlTypeId),
            "AXCheckBox"
        );
        assert!(platform::uia_semantic_focus_role("AXTextField"));
        assert!(!platform::uia_semantic_focus_role("AXTextArea"));
        assert!(platform::uia_semantic_text_input_role("AXTextField"));
        assert!(!platform::uia_semantic_text_input_role("AXTextArea"));
        assert!(!platform::uia_semantic_focus_role("AXButton"));
        assert!(!platform::uia_semantic_focus_role("AXWindow"));
    }

    #[test]
    fn computer_windows_window_activation_attempt_failure_is_unknown() {
        let error =
            platform::windows_window_activation_attempt_error("IUIAutomationElement::SetFocus");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
    }

    #[test]
    fn computer_windows_control_attempt_failure_is_unknown() {
        let error = platform::windows_control_attempt_error("IUIAutomationInvokePattern::Invoke");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
    }

    #[test]
    fn computer_windows_text_input_attempt_failure_is_unknown() {
        let error =
            platform::windows_text_input_attempt_error("IUIAutomationValuePattern::SetValue");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop with two observable UIA-backed windows; leaves the activated test window foreground"]
    fn computer_windows_window_activation_live_smoke() {
        let candidates = platform::list_windows(MAX_WINDOWS).expect("list live Windows windows");
        let foreground = unsafe { GetForegroundWindow() };
        let original_native_id = candidates
            .iter()
            .find(|candidate| platform::win_hwnd(candidate.native_id).ok() == Some(foreground))
            .map(|candidate| candidate.native_id)
            .expect("current foreground window must have an exact xcap surface");
        let mut failures = Vec::new();

        for candidate in candidates {
            if candidate.native_id == original_native_id {
                continue;
            }
            let target_hwnd = match platform::win_hwnd(candidate.native_id) {
                Ok(hwnd) if hwnd != foreground => hwnd,
                _ => continue,
            };
            let target_record = surface_record(candidate);
            match platform::accessibility_tree(
                "surface_windows_activation_probe",
                &target_record,
                1,
                8,
            ) {
                Ok(tree)
                    if tree.output["nodes"]
                        .as_array()
                        .and_then(|nodes| nodes.first())
                        .and_then(|node| node["role"].as_str())
                        == Some("AXWindow") => {}
                Ok(_) => continue,
                Err(error)
                    if error.starts_with("stale_surface:")
                        || error.starts_with("accessibility_failed:") =>
                {
                    if failures.len() < 8 {
                        failures.push(error);
                    }
                    continue;
                }
                Err(error) => panic!("unexpected Windows activation preflight error: {error}"),
            }

            let output =
                platform::activate_window("surface_windows_activation_live", &target_record)
                    .expect("activate one exact Windows UIA-backed surface");
            assert_eq!(output["platform"], "windows");
            assert_eq!(output["surface_id"], "surface_windows_activation_live");
            assert_eq!(output["success"], true);
            assert_eq!(unsafe { GetForegroundWindow() }, target_hwnd);
            return;
        }

        panic!(
            "no alternate exact UIA-backed Windows surface was available; failures={failures:?}"
        );
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop; creates and closes a private WinForms control fixture"]
    fn computer_windows_control_fixture_live_smoke() {
        let mut fixture = WindowsControlFixture::start();
        let candidate = (0..500)
            .find_map(|_| {
                if let Some(status) = fixture
                    .child
                    .try_wait()
                    .expect("query private WinForms fixture process")
                {
                    panic!("private WinForms fixture exited before discovery: {status}");
                }
                let candidate = platform::list_windows(4096)
                    .expect("list Windows windows for control fixture")
                    .into_iter()
                    .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE);
                if candidate.is_none() {
                    thread::sleep(Duration::from_millis(20));
                }
                candidate
            })
            .expect("discover private WinForms control fixture");
        let record = surface_record(candidate);
        let activation =
            platform::activate_window("surface_windows_control_fixture_activate", &record)
                .expect("activate private WinForms control fixture");
        assert_eq!(activation["success"], true);

        let candidate = platform::list_windows(4096)
            .expect("re-list Windows windows after fixture activation")
            .into_iter()
            .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
            .expect("re-observe private WinForms control fixture");
        let record = surface_record(candidate);
        let surface_id = "surface_windows_control_fixture";
        let tree = platform::accessibility_tree(surface_id, &record, 4, 64)
            .expect("read private WinForms control fixture UIA tree");
        let (edit_id, edit) = tree
            .elements
            .iter()
            .find(|(_, element)| {
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.role == "AXTextField"
                        && fingerprint.has_positive_evidence()
                        && !fingerprint.protected
                })
            })
            .expect("fixture exposes a positively correlated UIA edit");
        let (button_id, button) = tree
            .elements
            .iter()
            .find(|(_, element)| {
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.role == "AXButton"
                        && fingerprint.has_positive_evidence()
                        && !fingerprint.protected
                })
            })
            .expect("fixture exposes a positively correlated UIA button");

        let edit_state = platform::element_state(surface_id, edit_id, 1, &record, edit)
            .expect("read private edit state");
        assert_eq!(edit_state["can_focus"], true);
        assert_eq!(edit_state["value_empty"], true);
        let focused = platform::control(surface_id, edit_id, &record, edit, ComputerAction::Focus)
            .expect("focus private UIA edit");
        assert_eq!(focused["platform"], "windows");
        assert_eq!(focused["action"], "focus");
        assert_eq!(focused["success"], true);
        let focused_state = platform::element_state(surface_id, edit_id, 1, &record, edit)
            .expect("re-read private edit state");
        assert_eq!(focused_state["focused"], true);
        assert_eq!(focused_state["value_empty"], true);
        assert_eq!(focused_state["can_input_text"], true);

        let text = "webcodex computer smoke";
        let input = platform::input_text(surface_id, edit_id, &record, edit, text)
            .expect("write bounded text through private UIA ValuePattern");
        assert_eq!(input["platform"], "windows");
        assert_eq!(input["surface_id"], surface_id);
        assert_eq!(input["element_id"], edit_id.as_str());
        assert_eq!(input["text_bytes"], text.len());
        assert_eq!(input["success"], true);

        let after_input = platform::element_state(surface_id, edit_id, 1, &record, edit)
            .expect("re-read private edit state after bounded text input");
        assert_eq!(after_input["focused"], true);
        assert_eq!(after_input["value_empty"], false);
        assert_eq!(after_input["can_input_text"], false);
        let second = platform::input_text(surface_id, edit_id, &record, edit, "again")
            .expect_err("bounded Windows text input must not overwrite a non-empty field");
        assert!(second.starts_with("input_failed:"), "{second}");

        let button_state = platform::element_state(surface_id, button_id, 1, &record, button)
            .expect("read private button state");
        assert_eq!(button_state["can_press"], true);
        let pressed = platform::control(
            surface_id,
            button_id,
            &record,
            button,
            ComputerAction::Press,
        )
        .expect("invoke private UIA button");
        assert_eq!(pressed["platform"], "windows");
        assert_eq!(pressed["action"], "press");
        assert_eq!(pressed["success"], true);
        let click_deadline = Instant::now() + Duration::from_secs(1);
        let mut clicked = false;
        while Instant::now() < click_deadline {
            let after_press = platform::accessibility_tree(
                "surface_windows_control_fixture_after_press",
                &record,
                4,
                64,
            )
            .expect("re-observe private WinForms fixture after InvokePattern");
            clicked = after_press.output["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes.iter().any(|node| node["title"] == "clicked"));
            if clicked {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            clicked,
            "UIA InvokePattern did not update the private fixture"
        );
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop; creates and replaces indistinguishable private WinForms controls"]
    fn computer_windows_uia_stale_identity_rejects_indistinguishable_replacement_live() {
        let mut fixture = WindowsControlFixture::start();
        let candidate = (0..500)
            .find_map(|_| {
                if let Some(status) = fixture
                    .child
                    .try_wait()
                    .expect("query private WinForms fixture process")
                {
                    panic!("private WinForms fixture exited before discovery: {status}");
                }
                let candidate = platform::list_windows(4096)
                    .expect("list Windows windows for identity fixture")
                    .into_iter()
                    .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE);
                if candidate.is_none() {
                    thread::sleep(Duration::from_millis(20));
                }
                candidate
            })
            .expect("discover private WinForms identity fixture");
        let record = surface_record(candidate);
        platform::activate_window("surface_windows_identity_fixture_activate", &record)
            .expect("activate private WinForms identity fixture");

        let candidate = platform::list_windows(4096)
            .expect("re-list Windows windows after identity fixture activation")
            .into_iter()
            .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
            .expect("re-observe private WinForms identity fixture");
        let record = surface_record(candidate);
        let surface_id = "surface_windows_identity_fixture";
        let tree = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("read private WinForms identity fixture UIA tree");
        let twins = tree
            .elements
            .iter()
            .filter(|(_, element)| {
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.role == "AXButton"
                        && fingerprint.identifier.as_deref() == Some("TwinAction")
                        && fingerprint.title.as_deref() == Some("Twin action")
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            twins.len(),
            2,
            "fixture must expose two indistinguishable twins"
        );
        let (old_twin_id, old_twin) = twins[0];
        let old_twin_id = old_twin_id.clone();
        let old_twin = old_twin.clone();
        let old_target = old_twin
            .target_fingerprint()
            .expect("old twin has complete lineage")
            .clone();
        let (replace_id, replace) = tree
            .elements
            .iter()
            .find(|(_, element)| {
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.role == "AXButton"
                        && fingerprint.identifier.as_deref() == Some("ReplaceTwins")
                })
            })
            .expect("fixture exposes the replace-twins trigger");
        platform::control(
            surface_id,
            replace_id,
            &record,
            replace,
            ComputerAction::Press,
        )
        .expect("replace both indistinguishable twins through the private fixture");

        let replacement_deadline = Instant::now() + Duration::from_secs(1);
        let mut replacement_observed = false;
        while Instant::now() < replacement_deadline {
            let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
                .expect("re-observe private fixture after twin replacement");
            replacement_observed = refreshed.elements.iter().any(|(_, element)| {
                if element.path != old_twin.path {
                    return false;
                }
                element.target_fingerprint().is_some_and(|fingerprint| {
                    fingerprint.role == old_target.role
                        && fingerprint.subrole == old_target.subrole
                        && fingerprint.identifier == old_target.identifier
                        && fingerprint.title == old_target.title
                        && fingerprint.description == old_target.description
                        && fingerprint.placeholder == old_target.placeholder
                        && fingerprint.protected == old_target.protected
                        && fingerprint.native_runtime_id != old_target.native_runtime_id
                })
            });
            if replacement_observed {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            replacement_observed,
            "replacement must preserve the old semantic path while changing native UIA identity"
        );

        let stale_state = platform::element_state(surface_id, &old_twin_id, 1, &record, &old_twin)
            .expect_err("old element state handle must not retarget the replacement twin");
        assert!(stale_state.starts_with("stale_element:"), "{stale_state}");
        let stale_control = platform::control(
            surface_id,
            &old_twin_id,
            &record,
            &old_twin,
            ComputerAction::Press,
        )
        .expect_err("old effect handle must fail before invoking the replacement twin");
        assert!(
            stale_control.starts_with("stale_element:"),
            "{stale_control}"
        );
        let after = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe fixture after rejected stale control");
        assert!(
            after.output["nodes"].as_array().is_some_and(|nodes| nodes
                .iter()
                .all(|node| node["title"] != "wrong target invoked")),
            "stale control must not invoke either indistinguishable replacement"
        );
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop with at least one UIA-accessible window"]
    fn computer_windows_uia_live_smoke() {
        let status = platform::accessibility_status().expect("initialize Windows UI Automation");
        assert_eq!(status["platform"], "windows");
        assert_eq!(status["trusted"], true);

        let candidates = platform::list_windows(MAX_WINDOWS).expect("list live Windows windows");
        let mut failures = Vec::new();
        for candidate in candidates {
            let record = surface_record(candidate);
            match platform::accessibility_tree("surface_windows_live", &record, 3, 64) {
                Ok(tree) => {
                    assert_eq!(tree.output["platform"], "windows");
                    assert_eq!(tree.output["surface_id"], "surface_windows_live");
                    assert!(tree.output["node_count"].as_u64().unwrap_or(0) > 0);
                    assert!(!tree.elements.is_empty());
                    let Some((element_id, element)) = tree.elements.iter().find(|(_, element)| {
                        element
                            .target_fingerprint()
                            .is_some_and(ElementFingerprint::has_positive_evidence)
                    }) else {
                        if failures.len() < 8 {
                            failures.push(
                                "accessibility_failed: live UIA tree had no positively correlated element"
                                    .to_string(),
                            );
                        }
                        continue;
                    };
                    match platform::element_state(
                        "surface_windows_live",
                        element_id,
                        1,
                        &record,
                        element,
                    ) {
                        Ok(state) => {
                            assert_eq!(state["platform"], "windows");
                            assert_eq!(state["surface_id"], "surface_windows_live");
                            assert_eq!(state["element_id"], element_id.as_str());
                            assert!(state["can_press"].is_boolean());
                            assert!(state["can_focus"].is_boolean());
                            assert!(state["can_input_text"].is_boolean());
                            return;
                        }
                        Err(error) => {
                            if failures.len() < 8 && !failures.contains(&error) {
                                failures.push(error);
                            }
                            continue;
                        }
                    }
                }
                Err(error) => {
                    if failures.len() < 8 && !failures.contains(&error) {
                        failures.push(error);
                    }
                    continue;
                }
            }
        }
        panic!(
            "no bounded observable window exposed a Windows UIA Control View root; errors={failures:?}"
        );
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
