use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

pub const MAX_BROWSERS: usize = 4;
pub const MAX_PAGES_PER_BROWSER: usize = 16;
pub const MAX_PAGE_SUMMARIES: usize = 32;
pub const MAX_SNAPSHOT_NODES: usize = 256;
pub const MAX_SNAPSHOT_OFFSET: usize = 4096;
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
pub const MAX_NODE_TEXT_BYTES: usize = 512;
pub const MAX_IMAGE_BYTES: usize = 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
pub const MAX_INPUT_TEXT_BYTES: usize = 4096;
pub const MAX_URL_BYTES: usize = 8192;
pub const MAX_CONSOLE_ENTRIES: usize = 200;
pub const MAX_NETWORK_ENTRIES: usize = 300;
pub const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 2048;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
pub const BROWSER_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const MAX_BROWSER_LIFETIME: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    NotStarted,
    Completed,
    OutcomeUnknown,
}

impl ExecutionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Completed => "completed",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserError {
    pub kind: &'static str,
    pub message: String,
    pub execution_state: ExecutionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<&'static str>,
}

impl BrowserError {
    pub fn not_started(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            execution_state: ExecutionState::NotStarted,
            recovery_action: None,
        }
    }

    pub fn uncertain(
        kind: &'static str,
        message: impl Into<String>,
        recovery_action: &'static str,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            execution_state: ExecutionState::OutcomeUnknown,
            recovery_action: Some(recovery_action),
        }
    }

    pub fn observed(
        kind: &'static str,
        message: impl Into<String>,
        recovery_action: Option<&'static str>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            execution_state: ExecutionState::Completed,
            recovery_action,
        }
    }
}

pub type BrowserResult<T> = Result<T, BrowserError>;

pub const MAX_BATCH_OPERATIONS: usize = 32;

/// Already resolved local upload paths are supplied only by the Runner adapter.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BatchOperation {
    Click {
        element_id: String,
    },
    InputText {
        element_id: String,
        text: String,
    },
    SelectOption {
        element_id: String,
        option: String,
    },
    SetValue {
        element_id: String,
        value: String,
    },
    UploadFile {
        element_id: String,
        path: std::path::PathBuf,
    },
}

impl BatchOperation {
    pub(crate) fn authority(&self) -> (&str, AdmittedBrowserAction) {
        match self {
            Self::Click { element_id } => (element_id, AdmittedBrowserAction::Click),
            Self::InputText { element_id, .. } => (element_id, AdmittedBrowserAction::InputText),
            Self::SelectOption { element_id, .. } => {
                (element_id, AdmittedBrowserAction::SelectOption)
            }
            Self::SetValue { element_id, .. } => (element_id, AdmittedBrowserAction::SetValue),
            Self::UploadFile { element_id, .. } => (element_id, AdmittedBrowserAction::UploadFile),
        }
    }

    pub(crate) fn validate(&self) -> BrowserResult<()> {
        let value = match self {
            Self::InputText { text, .. } => Some(text),
            Self::SelectOption { option, .. } => Some(option),
            Self::SetValue { value, .. } => Some(value),
            _ => None,
        };
        if value.is_some_and(|v| v.is_empty() || v.contains('\0') || v.len() > MAX_INPUT_TEXT_BYTES)
        {
            return Err(BrowserError::not_started(
                "invalid_batch_value",
                "field value must be non-empty, NUL-free, and within the Browser UTF-8 byte bound",
            ));
        }
        Ok(())
    }
}

/// Sparse receipt. Aggregate certainty never erases an earlier completed effect.
/// stopped_at_index is zero-based; remaining_count counts definitely unstarted
/// operations (including a rejected operation, excluding an uncertain operation).
#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub execution_state: ExecutionState,
    pub requested_count: usize,
    pub completed_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_at_index: Option<usize>,
    pub remaining_count: usize,
    pub needs_snapshot: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stability: Option<BrowserStability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BrowserError>,
}

/// Ownership controls cleanup. External attachments never own a Chrome process.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserOwnership {
    OwnedEphemeral,
    OwnedManagedPersistent,
    AttachedExternal,
}

/// Runner-internal native window correlation. This is never model-visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserWindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserWindowHint {
    pub process_id: u32,
    pub bounds: Option<BrowserWindowBounds>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserSummary {
    pub browser_id: String,
    pub ownership: BrowserOwnership,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub page_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageSummary {
    pub browser_id: String,
    pub page_id: String,
    pub title: String,
    pub url: String,
}

/// Admitted Browser effects for one resolved control.
///
/// `actions` on a snapshot node is this set in canonical order. AX role alone
/// does not populate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ControlCapability {
    pub(crate) pointer_click: bool,
    pub(crate) text_input: bool,
    pub(crate) select_option: bool,
    pub(crate) exact_value: bool,
    pub(crate) file_upload: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmittedBrowserAction {
    Click,
    InputText,
    SelectOption,
    SetValue,
    UploadFile,
}

impl ControlCapability {
    pub(crate) const fn click() -> Self {
        Self {
            pointer_click: true,
            text_input: false,
            select_option: false,
            exact_value: false,
            file_upload: false,
        }
    }

    pub(crate) const fn text_input() -> Self {
        Self {
            pointer_click: true,
            text_input: true,
            select_option: false,
            exact_value: false,
            file_upload: false,
        }
    }

    pub(crate) const fn native_text_input() -> Self {
        Self {
            exact_value: true,
            ..Self::text_input()
        }
    }

    pub(crate) const fn select_option() -> Self {
        Self {
            pointer_click: false,
            text_input: false,
            select_option: true,
            exact_value: false,
            file_upload: false,
        }
    }

    pub(crate) const fn exact_value() -> Self {
        Self {
            pointer_click: false,
            text_input: false,
            select_option: false,
            exact_value: true,
            file_upload: false,
        }
    }

    pub(crate) const fn file_upload() -> Self {
        Self {
            pointer_click: false,
            text_input: false,
            select_option: false,
            exact_value: false,
            file_upload: true,
        }
    }

    pub(crate) const fn admits_any(self) -> bool {
        self.pointer_click
            || self.text_input
            || self.select_option
            || self.exact_value
            || self.file_upload
    }

    pub(crate) const fn admits(self, action: AdmittedBrowserAction) -> bool {
        match action {
            AdmittedBrowserAction::Click => self.pointer_click,
            AdmittedBrowserAction::InputText => self.text_input,
            AdmittedBrowserAction::SelectOption => self.select_option,
            AdmittedBrowserAction::SetValue => self.exact_value,
            AdmittedBrowserAction::UploadFile => self.file_upload,
        }
    }

    pub(crate) fn action_names(self) -> Vec<String> {
        let mut names = Vec::new();
        if self.pointer_click {
            names.push("click".to_string());
        }
        if self.text_input {
            names.push("input_text".to_string());
        }
        if self.select_option {
            names.push("select_option".to_string());
        }
        if self.exact_value {
            names.push("set_value".to_string());
        }
        if self.file_upload {
            names.push("upload_file".to_string());
        }
        names
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FormContext {
    /// Stable DOM semantic fingerprint for a rendered field. It excludes current
    /// values and opaque Browser element ids; structurally identical repeated fields
    /// may intentionally share the same fingerprint.
    pub field_signature: String,
    pub dom_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aria_invalid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticNode {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_context: Option<FormContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// Canonical `browser_act` effects this node admits. Empty for semantic-only nodes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    pub actionable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotMode {
    #[default]
    Auto,
    Full,
    Interactive,
}

impl SnapshotMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Full => "full",
            Self::Interactive => "interactive",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserStability {
    pub stable: bool,
    pub waited_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticSnapshot {
    pub browser_id: String,
    pub page_id: String,
    pub snapshot_generation: u64,
    pub snapshot_mode: String,
    pub auto_compacted: bool,
    pub max_nodes: usize,
    pub max_depth: u32,
    pub node_count: usize,
    /// Effective post-filter semantic-node offset for this bounded window.
    pub node_offset: usize,
    /// Next recoverable post-filter offset, if more projected source nodes remain.
    /// This is independent of `truncated`, which may also report unrecoverable
    /// source/field truncation.
    pub next_node_offset: Option<usize>,
    pub truncated: bool,
    pub nodes: Vec<SemanticNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Screenshot {
    pub browser_id: String,
    pub page_id: String,
    pub content_base64: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub file_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
}

impl BrowserKey {
    pub(crate) const fn cdp(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Enter => ("Enter", "Enter", "\r"),
            Self::Tab => ("Tab", "Tab", "\t"),
            Self::Escape => ("Escape", "Escape", ""),
            Self::Backspace => ("Backspace", "Backspace", ""),
            Self::Delete => ("Delete", "Delete", ""),
            Self::ArrowUp => ("ArrowUp", "ArrowUp", ""),
            Self::ArrowDown => ("ArrowDown", "ArrowDown", ""),
            Self::ArrowLeft => ("ArrowLeft", "ArrowLeft", ""),
            Self::ArrowRight => ("ArrowRight", "ArrowRight", ""),
            Self::Home => ("Home", "Home", ""),
            Self::End => ("End", "End", ""),
            Self::PageUp => ("PageUp", "PageUp", ""),
            Self::PageDown => ("PageDown", "PageDown", ""),
            Self::Space => (" ", "Space", " "),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BrowserShutdownReport {
    pub browsers: usize,
    pub timed_out: usize,
    pub failures: usize,
}

pub fn validate_navigation_url(value: &str) -> BrowserResult<Url> {
    if value.len() > MAX_URL_BYTES {
        return Err(BrowserError::not_started(
            "invalid_url",
            "URL exceeds the Browser navigation bound",
        ));
    }
    let url = Url::parse(value).map_err(|_| {
        BrowserError::not_started(
            "invalid_url",
            "navigation requires an absolute http/https URL",
        )
    })?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(BrowserError::not_started(
            "unsupported_url_scheme",
            "Browser navigation permits only http and https",
        )),
    }
}

pub(crate) fn clip_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

pub(crate) fn clip_bytes(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_observation_json_publishes_model_facing_fields_only() {
        let snapshot = SemanticSnapshot {
            browser_id: "browser_abcdefghijklmnop".to_string(),
            page_id: "page_abcdefghijklmnop".to_string(),
            snapshot_generation: 4,
            snapshot_mode: SnapshotMode::Interactive.as_str().to_string(),
            auto_compacted: true,
            max_nodes: 256,
            max_depth: 32,
            node_count: 2,
            node_offset: 16,
            next_node_offset: Some(18),
            truncated: false,
            nodes: vec![
                SemanticNode {
                    role: "combobox".to_string(),
                    name: Some("Fruit".to_string()),
                    description: None,
                    value: None,
                    group_id: None,
                    group_role: None,
                    group_label: None,
                    checked: None,
                    selected: None,
                    required: Some(true),
                    disabled: Some(false),
                    read_only: None,
                    form_context: Some(FormContext {
                        field_signature: "0123456789abcdef01234567".to_string(),
                        dom_tag: "select".to_string(),
                        input_type: None,
                        html_name: Some("fruit".to_string()),
                        placeholder: None,
                        autocomplete: None,
                        nearby_label: None,
                        group_label: None,
                        group_index: None,
                        group_size: None,
                        section_label: Some("Preferences".to_string()),
                        component_hint: Some("native-select".to_string()),
                        aria_invalid: Some(false),
                        validation_hint: None,
                        option_count: Some(2),
                    }),
                    element_id: Some("element_abcdefghijklmnop".to_string()),
                    actions: vec!["select_option".to_string()],
                    actionable: true,
                },
                SemanticNode {
                    role: "option".to_string(),
                    name: Some("Apple".to_string()),
                    description: None,
                    value: Some("a".to_string()),
                    group_id: Some("group_1".to_string()),
                    group_role: Some("combobox".to_string()),
                    group_label: Some("Fruit".to_string()),
                    checked: None,
                    selected: Some(true),
                    required: None,
                    disabled: Some(false),
                    read_only: None,
                    form_context: None,
                    element_id: None,
                    actions: Vec::new(),
                    actionable: false,
                },
            ],
        };
        let value = serde_json::to_value(&snapshot).unwrap();
        let object = value.as_object().unwrap();
        for field in [
            "snapshot_mode",
            "auto_compacted",
            "max_nodes",
            "max_depth",
            "node_offset",
            "next_node_offset",
            "nodes",
        ] {
            assert!(object.contains_key(field), "missing {field}");
        }
        for forbidden in ["target_id", "backend_node_id", "document_id"] {
            assert!(!object.contains_key(forbidden), "leaked {forbidden}");
        }
        assert_eq!(value["snapshot_mode"], "interactive");
        assert_eq!(value["auto_compacted"], true);
        assert_eq!(value["node_offset"], 16);
        assert_eq!(value["next_node_offset"], 18);
        let option = &value["nodes"][1];
        assert_eq!(option["name"], "Apple");
        assert_eq!(option["value"], "a");
        assert_eq!(option["group_label"], "Fruit");
        assert_eq!(option["selected"], true);
        assert_eq!(option["disabled"], false);
        let control = &value["nodes"][0];
        assert_eq!(
            control["form_context"]["field_signature"],
            "0123456789abcdef01234567"
        );
        assert_eq!(control["form_context"]["section_label"], "Preferences");
        assert_eq!(control["form_context"]["option_count"], 2);
        assert_eq!(option["actionable"], false);
        assert!(option.get("actions").is_none());
        assert!(option.get("element_id").is_none());
        assert!(option.get("backend_node_id").is_none());

        let stability = serde_json::to_value(BrowserStability {
            stable: false,
            waited_ms: 250,
            reason: "dom_quiet_with_long_lived_network".to_string(),
        })
        .unwrap();
        let mut stability_fields = stability
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        stability_fields.sort_unstable();
        assert_eq!(stability_fields, ["reason", "stable", "waited_ms"]);
    }

    #[test]
    fn navigation_policy_is_http_https_only() {
        for denied in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "data:text/plain,x",
            "chrome://settings",
            "edge://settings",
            "devtools://devtools/bundled/inspector.html",
        ] {
            let error = validate_navigation_url(denied).unwrap_err();
            assert_eq!(error.execution_state, ExecutionState::NotStarted);
        }
        assert!(validate_navigation_url("https://example.test/a?q=1").is_ok());
        assert!(validate_navigation_url("http://127.0.0.1:8000/").is_ok());
    }
}
