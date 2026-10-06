//! Runner-owned first-class Chromium/CDP runtime.
//!
//! This crate deliberately has no knowledge of OAuth, Workflow Sessions, MCP,
//! Project authority, or model-facing ToolDefinitions. It owns only bounded
//! Browser process/CDP semantics and opaque process-local identities.

mod bridge;
mod cdp;
mod profiles;
mod supervisor;
mod types;

pub use bridge::native::run as run_native_host;
pub use bridge::AttachmentSummary;
/// Whether this process owns an authenticated extension bridge listener.
pub fn extension_bridge_available() -> bool {
    bridge::available()
}

pub use cdp::discover_chromium_executable;
pub use supervisor::BrowserSupervisor;
pub use types::{
    validate_navigation_url, BatchOperation, BatchResult, BrowserError, BrowserKey,
    BrowserOwnership, BrowserResult, BrowserShutdownReport, BrowserStability, BrowserSummary,
    BrowserWindowBounds, BrowserWindowHint, ExecutionState, PageSummary, Screenshot, SemanticNode,
    SemanticSnapshot, SnapshotMode, BROWSER_IDLE_TIMEOUT, LAUNCH_TIMEOUT, MAX_BATCH_OPERATIONS,
    MAX_BROWSERS, MAX_BROWSER_LIFETIME, MAX_IMAGE_BYTES, MAX_INPUT_TEXT_BYTES,
    MAX_PAGES_PER_BROWSER, MAX_PAGE_SUMMARIES, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_NODES,
    MAX_URL_BYTES, REQUEST_TIMEOUT, SHUTDOWN_TIMEOUT,
};
