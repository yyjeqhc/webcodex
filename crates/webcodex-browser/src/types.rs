use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

pub const MAX_BROWSERS: usize = 4;
pub const MAX_PAGES_PER_BROWSER: usize = 16;
pub const MAX_PAGE_SUMMARIES: usize = 32;
pub const MAX_SNAPSHOT_NODES: usize = 256;
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

#[derive(Debug, Clone, Serialize)]
pub struct BrowserSummary {
    pub browser_id: String,
    pub page_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageSummary {
    pub browser_id: String,
    pub page_id: String,
    pub title: String,
    pub url: String,
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
    pub element_id: Option<String>,
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
