use crate::cdp::{
    BackendFactory, BackendNode, BackendPage, BackendScreenshot, BrowserBackend, ChromiumFactory,
};
use crate::types::{
    clip_bytes, clip_chars, validate_navigation_url, BrowserError, BrowserKey, BrowserResult,
    BrowserShutdownReport, BrowserStability, BrowserSummary, PageSummary, Screenshot, SemanticNode,
    SemanticSnapshot, SnapshotMode, BROWSER_IDLE_TIMEOUT, MAX_BROWSERS, MAX_BROWSER_LIFETIME,
    MAX_IMAGE_BYTES, MAX_IMAGE_DIMENSION, MAX_INPUT_TEXT_BYTES, MAX_NODE_TEXT_BYTES,
    MAX_PAGES_PER_BROWSER, MAX_PAGE_SUMMARIES, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_NODES,
    SHUTDOWN_TIMEOUT,
};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

// Keep non-image Browser observations below the Server's ordinary 256 KiB
// Runner-result retention boundary, with explicit room for the Runner envelope.
const MAX_BROWSER_OBSERVATION_RESULT_BYTES: usize = 192 * 1024;
const BROWSER_OBSERVATION_ENVELOPE_RESERVE_BYTES: usize = 8 * 1024;
const BROWSER_OBSERVATION_ENTRIES_BYTES: usize =
    MAX_BROWSER_OBSERVATION_RESULT_BYTES - BROWSER_OBSERVATION_ENVELOPE_RESERVE_BYTES;
const DIAGNOSTIC_SECTION_ENTRIES_BYTES: usize = BROWSER_OBSERVATION_ENTRIES_BYTES / 2;
const AUTO_SNAPSHOT_COMPACT_NODES: usize = 128;
const DEFAULT_SNAPSHOT_DEPTH: u32 = 32;
const ACTION_STABILITY_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Clone)]
pub struct BrowserSupervisor {
    inner: Arc<Mutex<SupervisorState>>,
    factory: Arc<dyn BackendFactory>,
    shutting_down: Arc<AtomicBool>,
}

struct SupervisorState {
    browsers: HashMap<String, BrowserRuntime>,
}

#[derive(Debug, Clone)]
struct PageIdentity {
    target_id: String,
    document_id: String,
    snapshot_generation: u64,
}

#[derive(Debug, Clone)]
struct ElementIdentity {
    page_id: String,
    document_id: String,
    snapshot_generation: u64,
    backend_node_id: i64,
    actionable: bool,
}

struct BrowserRuntime {
    backend: Box<dyn BrowserBackend>,
    created_at: Instant,
    last_activity_at: Instant,
    pages: HashMap<String, PageIdentity>,
    target_to_page: HashMap<String, String>,
    elements: HashMap<String, ElementIdentity>,
    generation: u64,
    page_count_floor: usize,
}

impl std::fmt::Debug for BrowserSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserSupervisor").finish_non_exhaustive()
    }
}

impl Default for BrowserSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserSupervisor {
    pub fn new() -> Self {
        Self::with_factory(Arc::new(ChromiumFactory))
    }

    fn with_factory(factory: Arc<dyn BackendFactory>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SupervisorState {
                browsers: HashMap::new(),
            })),
            factory,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn available(&self) -> bool {
        self.factory.available()
    }

    pub fn begin_shutdown(&self) {
        // Shutdown admission must never wait behind a Browser operation that is
        // currently holding the runtime mutex while bounded CDP I/O completes.
        self.shutting_down.store(true, Ordering::Release);
    }

    pub fn list_browsers(&self) -> Vec<BrowserSummary> {
        self.reap_expired();
        let state = self.state();
        state
            .browsers
            .iter()
            .take(MAX_BROWSERS)
            .map(|(id, runtime)| BrowserSummary {
                browser_id: id.clone(),
                page_count: runtime.page_count(),
            })
            .collect()
    }

    pub fn launch(&self) -> BrowserResult<BrowserSummary> {
        self.reject_if_shutting_down()?;
        self.reap_expired();
        let mut state = self.operation_state()?;
        if state.browsers.len() >= MAX_BROWSERS {
            return Err(BrowserError::not_started(
                "browser_limit",
                "maximum owned Browser runtimes reached",
            ));
        }
        let backend = self.factory.launch()?;
        let browser_id = opaque_id("browser");
        let runtime = BrowserRuntime::new(backend);
        let summary = BrowserSummary {
            browser_id: browser_id.clone(),
            // Chromium is launched with one explicit about:blank target. Keep a
            // conservative count floor until pages observation assigns opaque IDs.
            page_count: runtime.page_count(),
        };
        state.browsers.insert(browser_id, runtime);
        Ok(summary)
    }

    pub fn pages(&self, browser_id: &str, limit: usize) -> BrowserResult<Vec<PageSummary>> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let observed = runtime.backend.pages()?;
        runtime.reconcile_pages(observed.clone());
        Ok(observed
            .into_iter()
            .take(limit.clamp(1, MAX_PAGE_SUMMARIES))
            .filter_map(|page| {
                runtime
                    .target_to_page
                    .get(&page.target_id)
                    .map(|page_id| PageSummary {
                        browser_id: browser_id.to_string(),
                        page_id: page_id.clone(),
                        title: clip_chars(&page.title, 256),
                        url: clip_bytes(&page.url, 2048),
                    })
            })
            .collect())
    }

    pub fn new_page(&self, browser_id: &str) -> BrowserResult<PageSummary> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        if runtime.page_count() >= MAX_PAGES_PER_BROWSER {
            return Err(BrowserError::not_started(
                "page_limit",
                "maximum pages per Browser reached",
            ));
        }
        let target_id = match runtime.backend.new_page() {
            Ok(target_id) => {
                runtime.page_count_floor = runtime
                    .page_count()
                    .saturating_add(1)
                    .min(MAX_PAGES_PER_BROWSER);
                target_id
            }
            Err(error) if error.execution_state == crate::types::ExecutionState::OutcomeUnknown => {
                // Creation may have happened. Block another create until a pages
                // observation reconciles the authoritative target inventory.
                runtime.page_count_floor = MAX_PAGES_PER_BROWSER;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        runtime.elements.clear();
        // Target.createTarget already returned an exact target id, so creation is
        // known to have completed. A later observation failure must never make a
        // retry look safe; preserve completed certainty and direct the caller to
        // pages reconciliation instead.
        let pages = runtime.backend.pages().map_err(|error| {
            BrowserError::observed(
                "page_create_reconcile_failed",
                format!(
                    "new page was created but page reconciliation failed: {}",
                    error.message
                ),
                Some("pages"),
            )
        })?;
        runtime.reconcile_pages(pages.clone());
        let page = pages
            .into_iter()
            .find(|page| page.target_id == target_id)
            .ok_or_else(|| {
                BrowserError::observed(
                    "page_create_unobserved",
                    "new page was created but is not present in the reconciled page list",
                    Some("pages"),
                )
            })?;
        let page_id = runtime
            .target_to_page
            .get(&target_id)
            .cloned()
            .ok_or_else(|| {
                BrowserError::observed(
                    "page_create_unobserved",
                    "new page was created but its opaque page identity could not be reconciled",
                    Some("pages"),
                )
            })?;
        Ok(PageSummary {
            browser_id: browser_id.to_string(),
            page_id,
            title: clip_chars(&page.title, 256),
            url: clip_bytes(&page.url, 2048),
        })
    }

    pub fn snapshot(
        &self,
        browser_id: &str,
        page_id: &str,
        mode: SnapshotMode,
        max_nodes: usize,
        max_depth: u32,
    ) -> BrowserResult<SemanticSnapshot> {
        self.touch_current(browser_id)?;
        let max_nodes = max_nodes.clamp(1, MAX_SNAPSHOT_NODES);
        let max_depth = max_depth.clamp(1, DEFAULT_SNAPSHOT_DEPTH);
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        let snapshot = runtime.backend.snapshot(&target_id, max_depth)?;
        let auto_compacted = mode == SnapshotMode::Auto
            && should_auto_compact_snapshot(&snapshot.nodes, snapshot.truncated, max_nodes);
        let effective_mode = match mode {
            SnapshotMode::Auto if auto_compacted => SnapshotMode::Interactive,
            SnapshotMode::Auto => SnapshotMode::Full,
            other => other,
        };

        runtime.generation = runtime.generation.saturating_add(1);
        let generation = runtime.generation;
        runtime.elements.clear();
        if let Some(page) = runtime.pages.get_mut(page_id) {
            page.document_id = snapshot.document_id.clone();
            page.snapshot_generation = generation;
        }

        let source_nodes = snapshot
            .nodes
            .into_iter()
            .filter(|node| effective_mode != SnapshotMode::Interactive || node.actionable)
            .collect::<Vec<_>>();
        let mut nodes = Vec::new();
        let mut aggregate_bytes = 0usize;
        let mut truncated = if effective_mode == SnapshotMode::Interactive {
            source_nodes.len() > max_nodes
        } else {
            snapshot.truncated || source_nodes.len() > max_nodes
        };
        let mut group_ids = HashMap::<String, String>::new();
        let mut next_group_id = 1usize;
        for node in source_nodes.into_iter().take(max_nodes) {
            let group_id = node.group_key.as_ref().map(|key| {
                group_ids
                    .entry(key.clone())
                    .or_insert_with(|| {
                        let id = format!("group_{next_group_id}");
                        next_group_id = next_group_id.saturating_add(1);
                        id
                    })
                    .clone()
            });
            let projected =
                runtime.project_node(page_id, &snapshot.document_id, generation, node, group_id);
            let projected_bytes = serde_json::to_vec(&projected)
                .map(|value| value.len())
                .unwrap_or(MAX_SNAPSHOT_BYTES);
            if aggregate_bytes.saturating_add(projected_bytes) > MAX_SNAPSHOT_BYTES {
                truncated = true;
                break;
            }
            aggregate_bytes += projected_bytes;
            nodes.push(projected);
        }
        Ok(SemanticSnapshot {
            browser_id: browser_id.to_string(),
            page_id: page_id.to_string(),
            snapshot_generation: generation,
            snapshot_mode: effective_mode.as_str().to_string(),
            auto_compacted,
            max_nodes,
            max_depth,
            node_count: nodes.len(),
            truncated,
            nodes,
        })
    }

    pub fn screenshot(&self, browser_id: &str, page_id: &str) -> BrowserResult<Screenshot> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        let shot = runtime.backend.screenshot(&target_id)?;
        screenshot_result(browser_id, page_id, shot)
    }

    pub fn console(&self, browser_id: &str, page_id: &str) -> BrowserResult<serde_json::Value> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        let snapshot = runtime.backend.console(&target_id)?;
        let retained_count = snapshot.entries.len();
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| serde_json::json!({
                "level": entry.level, "text": entry.text, "source": entry.source, "timestamp": entry.timestamp
            }))
            .collect::<Vec<_>>();
        let (entries, projection_truncated) =
            bounded_recent_json_entries(entries, BROWSER_OBSERVATION_ENTRIES_BYTES);
        Ok(serde_json::json!({
            "cursor": snapshot.cursor,
            "retained_count": retained_count,
            "count": entries.len(),
            "truncated": snapshot.truncated || projection_truncated,
            "entries": entries,
        }))
    }

    pub fn network(&self, browser_id: &str, page_id: &str) -> BrowserResult<serde_json::Value> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        let snapshot = runtime.backend.network(&target_id)?;
        let retained_count = snapshot.entries.len();
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| serde_json::json!({
                "method": entry.method, "url": entry.url, "resource_type": entry.resource_type,
                "status": entry.status, "failed_reason": entry.failed_reason, "timestamp": entry.timestamp
            }))
            .collect::<Vec<_>>();
        let (entries, projection_truncated) =
            bounded_recent_json_entries(entries, BROWSER_OBSERVATION_ENTRIES_BYTES);
        Ok(serde_json::json!({
            "cursor": snapshot.cursor,
            "retained_count": retained_count,
            "count": entries.len(),
            "truncated": snapshot.truncated || projection_truncated,
            "entries": entries,
        }))
    }

    pub fn diagnostics(
        &self,
        browser_id: &str,
        page_id: &str,
        include_all_console: bool,
        include_all_network: bool,
        since_cursor: Option<u64>,
    ) -> BrowserResult<serde_json::Value> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        let snapshot = runtime.backend.diagnostics(&target_id)?;
        let since_cursor = since_cursor.unwrap_or(0);
        if since_cursor > snapshot.cursor {
            return Err(BrowserError::not_started(
                "invalid_diagnostics_cursor",
                "diagnostics cursor is newer than the current page event stream",
            ));
        }

        let console_retained = snapshot.console.entries.len();
        let network_retained = snapshot.network.entries.len();
        let backend_delta_truncated = since_cursor > 0
            && (since_cursor < snapshot.cleared_through_cursor
                || (snapshot.console.truncated
                    && snapshot
                        .console
                        .oldest_sequence
                        .is_some_and(|oldest| since_cursor.saturating_add(1) < oldest))
                || (snapshot.network.truncated
                    && snapshot
                        .network
                        .oldest_sequence
                        .is_some_and(|oldest| since_cursor.saturating_add(1) < oldest)));

        let new_console_entries = snapshot
            .console
            .entries
            .into_iter()
            .filter(|entry| entry.sequence > since_cursor)
            .collect::<Vec<_>>();
        let new_network_entries = snapshot
            .network
            .entries
            .into_iter()
            .filter(|entry| entry.sequence > since_cursor)
            .collect::<Vec<_>>();

        let new_console_errors = new_console_entries
            .iter()
            .filter(|entry| matches!(entry.level.as_str(), "error" | "exception"))
            .count();
        let new_console_warnings = new_console_entries
            .iter()
            .filter(|entry| matches!(entry.level.as_str(), "warning" | "warn"))
            .count();
        let new_failed_requests = new_network_entries
            .iter()
            .filter(|entry| entry.failed_reason.is_some())
            .count();
        let new_4xx = new_network_entries
            .iter()
            .filter(|entry| {
                entry
                    .status
                    .is_some_and(|status| (400..500).contains(&status))
            })
            .count();
        let new_5xx = new_network_entries
            .iter()
            .filter(|entry| entry.status.is_some_and(|status| status >= 500))
            .count();

        let console = new_console_entries
            .into_iter()
            .filter(|entry| {
                include_all_console
                    || matches!(
                        entry.level.as_str(),
                        "error" | "warning" | "warn" | "exception"
                    )
            })
            .map(|entry| serde_json::json!({
                "level": entry.level, "text": entry.text, "source": entry.source, "timestamp": entry.timestamp
            }))
            .collect::<Vec<_>>();
        let network = new_network_entries
            .into_iter()
            .filter(|entry| {
                include_all_network
                    || entry.failed_reason.is_some()
                    || entry.status.is_some_and(|status| status >= 400)
                    || matches!(entry.resource_type.as_deref(), Some("XHR") | Some("Fetch"))
            })
            .map(|entry| serde_json::json!({
                "method": entry.method, "url": entry.url, "resource_type": entry.resource_type,
                "status": entry.status, "failed_reason": entry.failed_reason, "timestamp": entry.timestamp
            }))
            .collect::<Vec<_>>();
        let (console, console_truncated) =
            bounded_recent_json_entries(console, DIAGNOSTIC_SECTION_ENTRIES_BYTES);
        let (network, network_truncated) =
            bounded_recent_json_entries(network, DIAGNOSTIC_SECTION_ENTRIES_BYTES);
        let delta_truncated = backend_delta_truncated || console_truncated || network_truncated;
        Ok(serde_json::json!({
            "cursor": snapshot.cursor,
            "since_cursor": since_cursor,
            "delta_truncated": delta_truncated,
            "new_console_errors": new_console_errors,
            "new_console_warnings": new_console_warnings,
            "new_failed_requests": new_failed_requests,
            "new_4xx": new_4xx,
            "new_5xx": new_5xx,
            "console_retained": console_retained,
            "console_count": console.len(),
            "console_truncated": snapshot.console.truncated || console_truncated,
            "console": console,
            "network_retained": network_retained,
            "network_count": network.len(),
            "network_truncated": snapshot.network.truncated || network_truncated,
            "network": network,
        }))
    }

    pub fn clear_diagnostics(&self, browser_id: &str, page_id: &str) -> BrowserResult<()> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        runtime.backend.clear_diagnostics(&target_id)
    }

    pub fn navigate(
        &self,
        browser_id: &str,
        page_id: &str,
        url: &str,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        validate_navigation_url(url)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        // Navigation can replace the document. Fence prior element authority before
        // dispatch; uncertain outcomes remain stale rather than silently retargeting.
        runtime.invalidate_elements_for_page(page_id);
        runtime.backend.navigate(&target_id, url)?;
        Ok(runtime.wait_after_effect(&target_id))
    }

    pub fn reload(&self, browser_id: &str, page_id: &str) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        runtime.invalidate_elements_for_page(page_id);
        runtime.backend.reload(&target_id)?;
        Ok(runtime.wait_after_effect(&target_id))
    }

    pub fn click(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        self.element_effect(browser_id, page_id, element_id, |backend, target, node| {
            backend.click(target, node)
        })
    }

    pub fn input_text(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
        text: &str,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        if text.is_empty() || text.contains('\0') || text.len() > MAX_INPUT_TEXT_BYTES {
            return Err(BrowserError::not_started(
                "invalid_text",
                "input text must be non-empty, NUL-free, and within the Browser UTF-8 byte bound",
            ));
        }
        self.element_effect(browser_id, page_id, element_id, |backend, target, node| {
            backend.input_text(target, node, text)
        })
    }

    pub fn select_option(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
        option: &str,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        if option.is_empty() || option.contains('\0') || option.len() > MAX_INPUT_TEXT_BYTES {
            return Err(BrowserError::not_started(
                "invalid_option",
                "select option must be non-empty, NUL-free, and within the Browser UTF-8 byte bound",
            ));
        }
        self.element_effect(browser_id, page_id, element_id, |backend, target, node| {
            backend.select_option(target, node, option)
        })
    }

    pub fn set_value(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
        value: &str,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        if value.is_empty() || value.contains('\0') || value.len() > MAX_INPUT_TEXT_BYTES {
            return Err(BrowserError::not_started(
                "invalid_value",
                "form value must be non-empty, NUL-free, and within the Browser UTF-8 byte bound",
            ));
        }
        self.element_effect(browser_id, page_id, element_id, |backend, target, node| {
            backend.set_value(target, node, value)
        })
    }

    pub fn upload_file(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
        path: &std::path::Path,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        self.element_effect(browser_id, page_id, element_id, |backend, target, node| {
            backend.upload_file(target, node, path)
        })
    }

    pub fn key(
        &self,
        browser_id: &str,
        page_id: &str,
        key: BrowserKey,
    ) -> BrowserResult<BrowserStability> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        runtime.refresh_document_fence(page_id, &target_id)?;
        runtime.backend.key(&target_id, key)?;
        Ok(runtime.wait_after_effect(&target_id))
    }

    pub fn close_page(&self, browser_id: &str, page_id: &str) -> BrowserResult<()> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        runtime.invalidate_elements_for_page(page_id);
        let result = runtime.backend.close_page(&target_id);
        if result.is_ok() {
            runtime.pages.remove(page_id);
            runtime.target_to_page.remove(&target_id);
            runtime.page_count_floor = runtime
                .page_count_floor
                .saturating_sub(1)
                .max(runtime.pages.len());
        }
        result
    }

    pub fn close_browser(&self, browser_id: &str) -> BrowserResult<()> {
        self.touch_current(browser_id)?;
        let mut state = self.operation_state()?;
        let mut runtime = state
            .browsers
            .remove(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        runtime.backend.shutdown(SHUTDOWN_TIMEOUT)
    }

    pub fn shutdown_until(&self, deadline: Instant) -> BrowserShutdownReport {
        self.begin_shutdown();
        let mut state = self.state();
        let mut report = BrowserShutdownReport {
            browsers: state.browsers.len(),
            ..BrowserShutdownReport::default()
        };
        let browsers = std::mem::take(&mut state.browsers);
        drop(state);
        for (_, mut runtime) in browsers {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .min(SHUTDOWN_TIMEOUT);
            if remaining.is_zero() {
                report.timed_out += 1;
            }
            // Even with no graceful budget left, explicitly ask the backend to
            // terminate/reap its owned process tree. ManagedChild::Drop remains a
            // final fallback, not the normal zero-budget shutdown path.
            if runtime.backend.shutdown(remaining).is_err() {
                report.failures += 1;
            }
        }
        report
    }

    fn touch_current(&self, browser_id: &str) -> BrowserResult<()> {
        self.reject_if_shutting_down()?;
        let now = Instant::now();
        let mut state = self.operation_state()?;
        let expired = state
            .browsers
            .get(browser_id)
            .is_some_and(|runtime| runtime.expired(now));
        if expired {
            let mut runtime = state
                .browsers
                .remove(browser_id)
                .expect("expired Browser existed under the same lock");
            drop(state);
            let _ = runtime.backend.shutdown(SHUTDOWN_TIMEOUT);
            return Err(stale_browser(browser_id));
        }
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        runtime.last_activity_at = now;
        Ok(())
    }

    fn reap_expired(&self) {
        let now = Instant::now();
        let mut state = self.state();
        let expired_ids = state
            .browsers
            .iter()
            .filter(|(_, runtime)| runtime.expired(now))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let expired = expired_ids
            .into_iter()
            .filter_map(|id| state.browsers.remove(&id))
            .collect::<Vec<_>>();
        drop(state);
        for mut runtime in expired {
            let _ = runtime.backend.shutdown(SHUTDOWN_TIMEOUT);
        }
    }

    fn element_effect<F>(
        &self,
        browser_id: &str,
        page_id: &str,
        element_id: &str,
        effect: F,
    ) -> BrowserResult<BrowserStability>
    where
        F: FnOnce(&mut dyn BrowserBackend, &str, i64) -> BrowserResult<()>,
    {
        let mut state = self.operation_state()?;
        let runtime = state
            .browsers
            .get_mut(browser_id)
            .ok_or_else(|| stale_browser(browser_id))?;
        let target_id = runtime.page_target(page_id)?;
        runtime.refresh_document_fence(page_id, &target_id)?;
        let element = runtime
            .elements
            .get(element_id)
            .cloned()
            .ok_or_else(stale_element)?;
        let page = runtime
            .pages
            .get(page_id)
            .ok_or_else(|| stale_page(page_id))?;
        if element.page_id != page_id
            || element.document_id != page.document_id
            || element.snapshot_generation != page.snapshot_generation
            || !element.actionable
        {
            return Err(stale_element());
        }
        effect(
            runtime.backend.as_mut(),
            &target_id,
            element.backend_node_id,
        )?;
        Ok(runtime.wait_after_effect(&target_id))
    }

    fn reject_if_shutting_down(&self) -> BrowserResult<()> {
        if self.shutting_down.load(Ordering::Acquire) {
            Err(BrowserError::not_started(
                "runner_shutting_down",
                "Browser operation rejected during Runner shutdown",
            ))
        } else {
            Ok(())
        }
    }

    fn operation_state(&self) -> BrowserResult<std::sync::MutexGuard<'_, SupervisorState>> {
        self.reject_if_shutting_down()?;
        let state = self.state();
        if self.shutting_down.load(Ordering::Acquire) {
            drop(state);
            return Err(BrowserError::not_started(
                "runner_shutting_down",
                "Browser operation rejected during Runner shutdown",
            ));
        }
        Ok(state)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SupervisorState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl BrowserRuntime {
    fn new(backend: Box<dyn BrowserBackend>) -> Self {
        let now = Instant::now();
        Self {
            backend,
            created_at: now,
            last_activity_at: now,
            pages: HashMap::new(),
            target_to_page: HashMap::new(),
            elements: HashMap::new(),
            generation: 0,
            page_count_floor: 1,
        }
    }

    fn page_count(&self) -> usize {
        self.pages.len().max(self.page_count_floor)
    }

    fn wait_after_effect(&mut self, target_id: &str) -> BrowserStability {
        self.backend
            .wait_for_stable(target_id, ACTION_STABILITY_TIMEOUT)
            .unwrap_or_else(|_| BrowserStability {
                stable: false,
                waited_ms: 0,
                reason: "observation_failed".to_string(),
            })
    }

    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_activity_at) >= BROWSER_IDLE_TIMEOUT
            || now.saturating_duration_since(self.created_at) >= MAX_BROWSER_LIFETIME
    }

    fn reconcile_pages(&mut self, pages: Vec<BackendPage>) {
        self.page_count_floor = pages.len().min(MAX_PAGES_PER_BROWSER);
        let live_targets = pages
            .iter()
            .map(|page| page.target_id.clone())
            .collect::<HashSet<_>>();
        let removed = self
            .pages
            .iter()
            .filter(|(_, page)| !live_targets.contains(&page.target_id))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for page_id in removed {
            if let Some(page) = self.pages.remove(&page_id) {
                self.target_to_page.remove(&page.target_id);
            }
            self.invalidate_elements_for_page(&page_id);
        }
        for page in pages {
            if let Some(page_id) = self.target_to_page.get(&page.target_id).cloned() {
                let changed_document = self
                    .pages
                    .get(&page_id)
                    .is_some_and(|current| current.document_id != page.document_id);
                if changed_document {
                    if let Some(current) = self.pages.get_mut(&page_id) {
                        current.document_id = page.document_id;
                        current.snapshot_generation = 0;
                    }
                    self.invalidate_elements_for_page(&page_id);
                }
            } else {
                let page_id = opaque_id("page");
                self.target_to_page
                    .insert(page.target_id.clone(), page_id.clone());
                self.pages.insert(
                    page_id,
                    PageIdentity {
                        target_id: page.target_id,
                        document_id: page.document_id,
                        snapshot_generation: 0,
                    },
                );
            }
        }
    }

    fn page_target(&self, page_id: &str) -> BrowserResult<String> {
        self.pages
            .get(page_id)
            .map(|page| page.target_id.clone())
            .ok_or_else(|| stale_page(page_id))
    }

    fn project_node(
        &mut self,
        page_id: &str,
        document_id: &str,
        snapshot_generation: u64,
        node: BackendNode,
        group_id: Option<String>,
    ) -> SemanticNode {
        let mut element_id = None;
        if node.actionable {
            if let Some(backend_node_id) = node.backend_node_id {
                let id = opaque_id("element");
                self.elements.insert(
                    id.clone(),
                    ElementIdentity {
                        page_id: page_id.to_string(),
                        document_id: document_id.to_string(),
                        snapshot_generation,
                        backend_node_id,
                        actionable: true,
                    },
                );
                element_id = Some(id);
            }
        }
        SemanticNode {
            role: clip_chars(&node.role, 64),
            name: node
                .name
                .map(|value| clip_bytes(&value, MAX_NODE_TEXT_BYTES)),
            description: node
                .description
                .map(|value| clip_bytes(&value, MAX_NODE_TEXT_BYTES)),
            value: node
                .value
                .map(|value| clip_bytes(&value, MAX_NODE_TEXT_BYTES)),
            group_id,
            group_role: node.group_role.map(|value| clip_chars(&value, 64)),
            group_label: node
                .group_label
                .map(|value| clip_bytes(&value, MAX_NODE_TEXT_BYTES)),
            checked: node.checked.map(|value| clip_chars(&value, 32)),
            selected: node.selected,
            required: node.required,
            disabled: node.disabled,
            read_only: node.read_only,
            element_id,
            actionable: node.actionable,
        }
    }

    fn invalidate_elements_for_page(&mut self, page_id: &str) {
        self.elements
            .retain(|_, element| element.page_id != page_id);
    }

    fn refresh_document_fence(&mut self, page_id: &str, target_id: &str) -> BrowserResult<()> {
        let pages = self
            .backend
            .pages()
            .map_err(pre_effect_revalidation_error)?;
        let current = pages
            .into_iter()
            .find(|page| page.target_id == target_id)
            .ok_or_else(|| stale_page(page_id))?;
        let stored = self
            .pages
            .get_mut(page_id)
            .ok_or_else(|| stale_page(page_id))?;
        if stored.document_id != current.document_id {
            stored.document_id = current.document_id;
            stored.snapshot_generation = 0;
            self.invalidate_elements_for_page(page_id);
            return Err(stale_element());
        }
        Ok(())
    }
}

fn should_auto_compact_snapshot(
    nodes: &[BackendNode],
    backend_truncated: bool,
    max_nodes: usize,
) -> bool {
    if backend_truncated || nodes.len() > max_nodes || nodes.len() > AUTO_SNAPSHOT_COMPACT_NODES {
        return true;
    }
    let estimated = nodes.iter().fold(0usize, |total, node| {
        total
            .saturating_add(node.role.len())
            .saturating_add(node.name.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(node.description.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(node.value.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(node.group_label.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(160)
    });
    estimated > (MAX_SNAPSHOT_BYTES * 3 / 4)
}

fn screenshot_result(
    browser_id: &str,
    page_id: &str,
    screenshot: BackendScreenshot,
) -> BrowserResult<Screenshot> {
    let decoded = general_purpose::STANDARD
        .decode(&screenshot.data)
        .map_err(|_| {
            BrowserError::observed("invalid_image", "CDP returned invalid image base64", None)
        })?;
    if decoded.is_empty() || decoded.len() > MAX_IMAGE_BYTES {
        return Err(BrowserError::observed(
            "image_too_large",
            "browser screenshot exceeded bounded image bytes",
            None,
        ));
    }
    if screenshot.width == 0
        || screenshot.height == 0
        || screenshot.width > MAX_IMAGE_DIMENSION
        || screenshot.height > MAX_IMAGE_DIMENSION
    {
        return Err(BrowserError::observed(
            "invalid_image_dimensions",
            "browser screenshot dimensions are outside the bounded image contract",
            None,
        ));
    }
    let sha256 = format!("{:x}", Sha256::digest(&decoded));
    Ok(Screenshot {
        browser_id: browser_id.to_string(),
        page_id: page_id.to_string(),
        content_base64: screenshot.data,
        mime_type: "image/png".to_string(),
        width: screenshot.width,
        height: screenshot.height,
        file_bytes: decoded.len() as u64,
        sha256,
    })
}

fn bounded_recent_json_entries(
    entries: Vec<serde_json::Value>,
    max_serialized_bytes: usize,
) -> (Vec<serde_json::Value>, bool) {
    let total = entries.len();
    let mut kept = Vec::new();
    // Exact JSON-array accounting: brackets plus one comma between entries.
    let mut used = 2usize;
    for entry in entries.into_iter().rev() {
        let entry_bytes = serde_json::to_vec(&entry)
            .map(|encoded| encoded.len())
            .unwrap_or(max_serialized_bytes.saturating_add(1));
        let separator = usize::from(!kept.is_empty());
        if used.saturating_add(separator).saturating_add(entry_bytes) > max_serialized_bytes {
            break;
        }
        used = used.saturating_add(separator).saturating_add(entry_bytes);
        kept.push(entry);
    }
    kept.reverse();
    let truncated = kept.len() < total;
    (kept, truncated)
}

fn pre_effect_revalidation_error(mut error: BrowserError) -> BrowserError {
    error.execution_state = crate::types::ExecutionState::NotStarted;
    if error.recovery_action.is_none() {
        error.recovery_action = Some("pages");
    }
    error
}

fn stale_browser(browser_id: &str) -> BrowserError {
    BrowserError::not_started(
        "stale_browser",
        format!("browser_id '{browser_id}' is no longer current"),
    )
}

fn stale_page(page_id: &str) -> BrowserError {
    BrowserError::not_started(
        "stale_page",
        format!("page_id '{page_id}' is no longer current"),
    )
}

fn stale_element() -> BrowserError {
    let mut error = BrowserError::not_started(
        "stale_element",
        "element_id is stale; re-observe the semantic snapshot before acting",
    );
    error.recovery_action = Some("snapshot");
    error
}

fn opaque_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdp::{
        BackendConsoleEntry, BackendDiagnosticsSnapshot, BackendEventSnapshot, BackendFactory,
        BackendNetworkEntry, BackendSnapshot, BrowserBackend,
    };
    use crate::types::ExecutionState;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct FakeFactory {
        launches: AtomicUsize,
    }

    impl BackendFactory for FakeFactory {
        fn available(&self) -> bool {
            true
        }

        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            self.launches.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeBackend::new()))
        }
    }

    struct FakeBackend {
        pages: Vec<BackendPage>,
        document_generation: u64,
        snapshot_node_count: usize,
        mixed_snapshot: bool,
        diagnostic_calls: usize,
        diagnostics_cleared_through_cursor: u64,
        large_diagnostics: bool,
        wait_fails: bool,
        fail_pages_after_create: bool,
        page_created: bool,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self::with_snapshot_nodes(1)
        }

        fn with_snapshot_nodes(snapshot_node_count: usize) -> Self {
            Self {
                pages: vec![BackendPage {
                    target_id: "private-target".to_string(),
                    title: "Fixture".to_string(),
                    url: "https://example.test/?private=query".to_string(),
                    document_id: "doc-1".to_string(),
                }],
                document_generation: 1,
                snapshot_node_count,
                mixed_snapshot: false,
                diagnostic_calls: 0,
                diagnostics_cleared_through_cursor: 0,
                large_diagnostics: false,
                wait_fails: false,
                fail_pages_after_create: false,
                page_created: false,
            }
        }

        fn with_new_page_reconcile_failure() -> Self {
            let mut backend = Self::new();
            backend.fail_pages_after_create = true;
            backend
        }

        fn with_mixed_snapshot() -> Self {
            let mut backend = Self::with_snapshot_nodes(6);
            backend.mixed_snapshot = true;
            backend
        }

        fn with_wait_failure() -> Self {
            let mut backend = Self::new();
            backend.wait_fails = true;
            backend
        }

        fn with_large_diagnostics() -> Self {
            let mut backend = Self::new();
            backend.large_diagnostics = true;
            backend
        }
    }

    impl BrowserBackend for FakeBackend {
        fn pages(&mut self) -> BrowserResult<Vec<BackendPage>> {
            if self.fail_pages_after_create && self.page_created {
                return Err(BrowserError::observed(
                    "fixture_pages_failed",
                    "fixture page reconciliation failed",
                    None,
                ));
            }
            Ok(self.pages.clone())
        }
        fn new_page(&mut self) -> BrowserResult<String> {
            let target_id = format!("private-target-{}", self.pages.len() + 1);
            self.pages.push(BackendPage {
                target_id: target_id.clone(),
                title: String::new(),
                url: "about:blank".to_string(),
                document_id: format!("doc-{}", self.document_generation),
            });
            self.page_created = true;
            Ok(target_id)
        }
        fn snapshot(
            &mut self,
            _target_id: &str,
            _max_depth: u32,
        ) -> BrowserResult<BackendSnapshot> {
            let nodes = (0..self.snapshot_node_count)
                .map(|index| {
                    let actionable = !self.mixed_snapshot || index % 2 == 0;
                    BackendNode {
                        role: if actionable { "button" } else { "paragraph" }.to_string(),
                        name: Some(if actionable {
                            format!("Go {index}")
                        } else {
                            format!("Static {index}")
                        }),
                        description: None,
                        value: Some("x".repeat(MAX_NODE_TEXT_BYTES * 2)),
                        group_key: None,
                        group_role: None,
                        group_label: None,
                        checked: None,
                        selected: None,
                        required: None,
                        disabled: None,
                        read_only: None,
                        backend_node_id: actionable.then_some(index as i64 + 7),
                        actionable,
                    }
                })
                .collect::<Vec<_>>();
            Ok(BackendSnapshot {
                document_id: format!("doc-{}", self.document_generation),
                nodes,
                truncated: self.snapshot_node_count > MAX_SNAPSHOT_NODES,
            })
        }
        fn screenshot(&mut self, _target_id: &str) -> BrowserResult<BackendScreenshot> {
            Ok(BackendScreenshot {
                data: general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nFAKE"),
                width: 800,
                height: 600,
            })
        }
        fn console(
            &mut self,
            _target_id: &str,
        ) -> BrowserResult<BackendEventSnapshot<BackendConsoleEntry>> {
            Ok(BackendEventSnapshot {
                entries: Vec::new(),
                truncated: false,
                cursor: 0,
                oldest_sequence: None,
            })
        }
        fn network(
            &mut self,
            _target_id: &str,
        ) -> BrowserResult<BackendEventSnapshot<BackendNetworkEntry>> {
            Ok(BackendEventSnapshot {
                entries: Vec::new(),
                truncated: false,
                cursor: 0,
                oldest_sequence: None,
            })
        }
        fn diagnostics(&mut self, _target_id: &str) -> BrowserResult<BackendDiagnosticsSnapshot> {
            if self.large_diagnostics {
                let console = (1..=200)
                    .map(|sequence| BackendConsoleEntry {
                        sequence,
                        level: "error".into(),
                        text: format!(
                            "error-{sequence}-{}",
                            "x".repeat(crate::types::MAX_DIAGNOSTIC_TEXT_BYTES)
                        ),
                        source: None,
                        timestamp: Some(sequence as f64),
                    })
                    .collect::<Vec<_>>();
                return Ok(BackendDiagnosticsSnapshot {
                    console: BackendEventSnapshot {
                        entries: console,
                        truncated: false,
                        cursor: 200,
                        oldest_sequence: Some(1),
                    },
                    network: BackendEventSnapshot {
                        entries: Vec::new(),
                        truncated: false,
                        cursor: 200,
                        oldest_sequence: None,
                    },
                    cursor: 200,
                    cleared_through_cursor: self.diagnostics_cleared_through_cursor,
                });
            }
            self.diagnostic_calls += 1;
            let cursor = if self.diagnostic_calls == 1 { 3 } else { 5 };
            let mut console = vec![BackendConsoleEntry {
                sequence: 1,
                level: "warning".into(),
                text: "first warning".into(),
                source: None,
                timestamp: Some(1.0),
            }];
            let mut network = vec![
                BackendNetworkEntry {
                    sequence: 2,
                    method: "GET".into(),
                    url: "https://example.test/api/first".into(),
                    resource_type: Some("Fetch".into()),
                    status: Some(200),
                    failed_reason: None,
                    timestamp: Some(2.0),
                },
                BackendNetworkEntry {
                    sequence: 3,
                    method: "GET".into(),
                    url: "https://example.test/api/fail".into(),
                    resource_type: Some("XHR".into()),
                    status: Some(500),
                    failed_reason: None,
                    timestamp: Some(3.0),
                },
            ];
            if self.diagnostic_calls > 1 {
                console.push(BackendConsoleEntry {
                    sequence: 4,
                    level: "error".into(),
                    text: "new error".into(),
                    source: None,
                    timestamp: Some(4.0),
                });
                network.push(BackendNetworkEntry {
                    sequence: 5,
                    method: "POST".into(),
                    url: "https://example.test/api/new".into(),
                    resource_type: Some("Fetch".into()),
                    status: Some(404),
                    failed_reason: None,
                    timestamp: Some(5.0),
                });
            }
            Ok(BackendDiagnosticsSnapshot {
                console: BackendEventSnapshot {
                    entries: console,
                    truncated: false,
                    cursor,
                    oldest_sequence: Some(1),
                },
                network: BackendEventSnapshot {
                    entries: network,
                    truncated: false,
                    cursor,
                    oldest_sequence: Some(2),
                },
                cursor,
                cleared_through_cursor: self.diagnostics_cleared_through_cursor,
            })
        }
        fn clear_diagnostics(&mut self, _target_id: &str) -> BrowserResult<()> {
            self.diagnostics_cleared_through_cursor = match self.diagnostic_calls {
                0 => 0,
                1 => 3,
                _ => 5,
            };
            Ok(())
        }
        fn navigate(&mut self, _target_id: &str, _url: &str) -> BrowserResult<()> {
            self.document_generation += 1;
            for page in &mut self.pages {
                page.document_id = format!("doc-{}", self.document_generation);
            }
            Ok(())
        }
        fn reload(&mut self, _target_id: &str) -> BrowserResult<()> {
            self.document_generation += 1;
            for page in &mut self.pages {
                page.document_id = format!("doc-{}", self.document_generation);
            }
            Ok(())
        }
        fn click(&mut self, _target_id: &str, _backend_node_id: i64) -> BrowserResult<()> {
            Ok(())
        }
        fn input_text(
            &mut self,
            _target_id: &str,
            _backend_node_id: i64,
            _text: &str,
        ) -> BrowserResult<()> {
            Ok(())
        }
        fn select_option(
            &mut self,
            _target_id: &str,
            _backend_node_id: i64,
            _option: &str,
        ) -> BrowserResult<()> {
            Ok(())
        }
        fn set_value(
            &mut self,
            _target_id: &str,
            _backend_node_id: i64,
            _value: &str,
        ) -> BrowserResult<()> {
            Ok(())
        }
        fn upload_file(
            &mut self,
            _target_id: &str,
            _backend_node_id: i64,
            _path: &std::path::Path,
        ) -> BrowserResult<()> {
            Ok(())
        }
        fn key(&mut self, _target_id: &str, _key: BrowserKey) -> BrowserResult<()> {
            Ok(())
        }
        fn wait_for_stable(
            &mut self,
            _target_id: &str,
            _timeout: Duration,
        ) -> BrowserResult<BrowserStability> {
            if self.wait_fails {
                return Err(BrowserError::observed(
                    "fixture_wait_failed",
                    "fixture stability observation failed",
                    None,
                ));
            }
            Ok(BrowserStability {
                stable: true,
                waited_ms: 0,
                reason: "fixture_stable".to_string(),
            })
        }
        fn close_page(&mut self, target_id: &str) -> BrowserResult<()> {
            self.pages.retain(|page| page.target_id != target_id);
            Ok(())
        }
        fn shutdown(&mut self, _timeout: Duration) -> BrowserResult<()> {
            Ok(())
        }
    }

    struct ManyNodesFactory;
    impl BackendFactory for ManyNodesFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Ok(Box::new(FakeBackend::with_snapshot_nodes(
                MAX_SNAPSHOT_NODES + 64,
            )))
        }
    }

    struct MixedSnapshotFactory;
    impl BackendFactory for MixedSnapshotFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Ok(Box::new(FakeBackend::with_mixed_snapshot()))
        }
    }

    struct LargeDiagnosticsFactory;
    impl BackendFactory for LargeDiagnosticsFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Ok(Box::new(FakeBackend::with_large_diagnostics()))
        }
    }

    struct WaitFailureFactory;
    impl BackendFactory for WaitFailureFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Ok(Box::new(FakeBackend::with_wait_failure()))
        }
    }

    struct NewPageReconcileFailureFactory;
    impl BackendFactory for NewPageReconcileFailureFactory {
        fn available(&self) -> bool {
            true
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Ok(Box::new(FakeBackend::with_new_page_reconcile_failure()))
        }
    }

    struct UnavailableFactory;
    impl BackendFactory for UnavailableFactory {
        fn available(&self) -> bool {
            false
        }
        fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
            Err(BrowserError::not_started(
                "browser_unavailable",
                "no supported Chromium-family browser is installed",
            ))
        }
    }

    fn fixture() -> BrowserSupervisor {
        BrowserSupervisor::with_factory(Arc::new(FakeFactory::default()))
    }

    #[test]
    fn opaque_ids_never_expose_cdp_target_identity() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        assert!(browser.browser_id.starts_with("browser_"));
        assert!(page.page_id.starts_with("page_"));
        assert!(!page.page_id.contains("private-target"));
        let snapshot = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap();
        let element = snapshot.nodes[0].element_id.as_ref().unwrap();
        assert!(element.starts_with("element_"));
    }

    #[test]
    fn navigation_invalidates_prior_element_authority() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();
        supervisor
            .navigate(
                &browser.browser_id,
                &page.page_id,
                "https://example.test/next",
            )
            .unwrap();
        let error = supervisor
            .click(&browser.browser_id, &page.page_id, &element)
            .unwrap_err();
        assert_eq!(error.kind, "stale_element");
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
        assert_eq!(error.recovery_action, Some("snapshot"));
    }

    #[test]
    fn reload_invalidates_prior_element_authority() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();
        supervisor
            .reload(&browser.browser_id, &page.page_id)
            .unwrap();
        let error = supervisor
            .click(&browser.browser_id, &page.page_id, &element)
            .unwrap_err();
        assert_eq!(error.kind, "stale_element");
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
        assert_eq!(error.recovery_action, Some("snapshot"));
    }

    #[test]
    fn newer_snapshot_invalidates_prior_snapshot_elements() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();
        supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap();
        assert_eq!(
            supervisor
                .click(&browser.browser_id, &page.page_id, &element)
                .unwrap_err()
                .kind,
            "stale_element"
        );
    }

    #[test]
    fn launch_tracks_the_implicit_about_blank_page_without_extra_cdp_observation() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        assert_eq!(browser.page_count, 1);
        assert_eq!(supervisor.list_browsers()[0].page_count, 1);
        assert_eq!(supervisor.pages(&browser.browser_id, 8).unwrap().len(), 1);
    }

    #[test]
    fn pre_effect_revalidation_failure_is_not_started() {
        let error = pre_effect_revalidation_error(BrowserError::observed(
            "fixture_observation_failed",
            "fixture",
            None,
        ));
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
        assert_eq!(error.recovery_action, Some("pages"));
    }

    #[test]
    fn unavailable_browser_is_truthful_and_pre_dispatch() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(UnavailableFactory));
        assert!(!supervisor.available());
        let error = supervisor.launch().unwrap_err();
        assert_eq!(error.kind, "browser_unavailable");
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
    }

    #[test]
    fn new_page_reconciliation_failure_preserves_completed_effect_certainty() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(NewPageReconcileFailureFactory));
        let browser = supervisor.launch().unwrap();
        let error = supervisor.new_page(&browser.browser_id).unwrap_err();
        assert_eq!(error.kind, "page_create_reconcile_failed");
        assert_eq!(error.execution_state, ExecutionState::Completed);
        assert_eq!(error.recovery_action, Some("pages"));
        assert_eq!(
            supervisor
                .list_browsers()
                .into_iter()
                .find(|summary| summary.browser_id == browser.browser_id)
                .unwrap()
                .page_count,
            2,
            "completed create must reserve page capacity until pages reconciliation"
        );
    }

    #[test]
    fn adaptive_snapshot_compacts_large_pages_and_respects_explicit_modes() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(ManyNodesFactory));
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let snapshot = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Auto,
                64,
                12,
            )
            .unwrap();
        assert_eq!(snapshot.snapshot_mode, "interactive");
        assert!(snapshot.auto_compacted);
        assert_eq!(snapshot.max_nodes, 64);
        assert_eq!(snapshot.max_depth, 12);
        assert!(snapshot.node_count <= 64);

        let mixed = BrowserSupervisor::with_factory(Arc::new(MixedSnapshotFactory));
        let browser = mixed.launch().unwrap();
        let page = mixed.pages(&browser.browser_id, 8).unwrap().remove(0);
        let full = mixed
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                4,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap();
        assert_eq!(full.snapshot_mode, "full");
        assert!(!full.auto_compacted);
        assert_eq!(full.node_count, 4);
        assert!(full.truncated);
        assert!(full.nodes.iter().any(|node| !node.actionable));

        let interactive = mixed
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Interactive,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap();
        assert_eq!(interactive.snapshot_mode, "interactive");
        assert_eq!(interactive.node_count, 3);
        assert!(interactive.nodes.iter().all(|node| node.actionable));
    }

    #[test]
    fn diagnostics_cursor_returns_only_new_events_and_summary() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);

        let first = supervisor
            .diagnostics(&browser.browser_id, &page.page_id, false, false, None)
            .unwrap();
        assert_eq!(first["cursor"], 3);
        assert_eq!(first["new_console_warnings"], 1);
        assert_eq!(first["new_5xx"], 1);

        let second = supervisor
            .diagnostics(&browser.browser_id, &page.page_id, false, false, Some(3))
            .unwrap();
        assert_eq!(second["cursor"], 5);
        assert_eq!(second["since_cursor"], 3);
        assert_eq!(second["new_console_errors"], 1);
        assert_eq!(second["new_4xx"], 1);
        assert_eq!(second["console_count"], 1);
        assert_eq!(second["network_count"], 1);
        assert_eq!(second["delta_truncated"], false);

        supervisor
            .clear_diagnostics(&browser.browser_id, &page.page_id)
            .unwrap();
        let after_clear = supervisor
            .diagnostics(&browser.browser_id, &page.page_id, false, false, Some(3))
            .unwrap();
        assert_eq!(after_clear["delta_truncated"], true);

        let error = supervisor
            .diagnostics(&browser.browser_id, &page.page_id, false, false, Some(99))
            .unwrap_err();
        assert_eq!(error.kind, "invalid_diagnostics_cursor");
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
    }

    #[test]
    fn diagnostics_delta_reports_projection_truncation_before_advancing_cursor() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(LargeDiagnosticsFactory));
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);

        let diagnostics = supervisor
            .diagnostics(&browser.browser_id, &page.page_id, true, false, Some(1))
            .unwrap();
        assert_eq!(diagnostics["cursor"], 200);
        assert_eq!(diagnostics["since_cursor"], 1);
        assert_eq!(diagnostics["new_console_errors"], 199);
        assert_eq!(diagnostics["console_truncated"], true);
        assert_eq!(diagnostics["delta_truncated"], true);
        assert!(diagnostics["console_count"].as_u64().unwrap() < 199);
    }

    #[test]
    fn post_effect_stability_observation_never_changes_effect_certainty() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(WaitFailureFactory));
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();

        let stability = supervisor
            .click(&browser.browser_id, &page.page_id, &element)
            .unwrap();
        assert!(!stability.stable);
        assert_eq!(stability.reason, "observation_failed");
    }

    #[test]
    fn semantic_snapshot_and_text_are_bounded() {
        let supervisor = BrowserSupervisor::with_factory(Arc::new(ManyNodesFactory));
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let snapshot = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap();
        assert!(snapshot.truncated);
        assert!(snapshot.node_count <= MAX_SNAPSHOT_NODES);
        let encoded = serde_json::to_vec(&snapshot.nodes).unwrap();
        assert!(encoded.len() <= MAX_SNAPSHOT_BYTES + MAX_NODE_TEXT_BYTES * 2);
        assert!(snapshot.nodes.iter().all(|node| {
            node.value
                .as_ref()
                .is_none_or(|value| value.len() <= MAX_NODE_TEXT_BYTES)
        }));
    }

    #[test]
    fn input_text_bound_fails_before_effect_dispatch() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();
        let error = supervisor
            .input_text(
                &browser.browser_id,
                &page.page_id,
                &element,
                &"x".repeat(MAX_INPUT_TEXT_BYTES + 1),
            )
            .unwrap_err();
        assert_eq!(error.kind, "invalid_text");
        assert_eq!(error.execution_state, ExecutionState::NotStarted);
        for invalid in ["", "nul\0text"] {
            let error = supervisor
                .input_text(&browser.browser_id, &page.page_id, &element, invalid)
                .unwrap_err();
            assert_eq!(error.kind, "invalid_text");
            assert_eq!(error.execution_state, ExecutionState::NotStarted);
        }
    }

    #[test]
    fn form_value_bounds_fail_before_effect_dispatch() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        let page = supervisor.pages(&browser.browser_id, 8).unwrap().remove(0);
        let element = supervisor
            .snapshot(
                &browser.browser_id,
                &page.page_id,
                SnapshotMode::Full,
                MAX_SNAPSHOT_NODES,
                DEFAULT_SNAPSHOT_DEPTH,
            )
            .unwrap()
            .nodes[0]
            .element_id
            .clone()
            .unwrap();

        for invalid in ["", "nul\0value"] {
            let option_error = supervisor
                .select_option(&browser.browser_id, &page.page_id, &element, invalid)
                .unwrap_err();
            assert_eq!(option_error.kind, "invalid_option");
            assert_eq!(option_error.execution_state, ExecutionState::NotStarted);

            let value_error = supervisor
                .set_value(&browser.browser_id, &page.page_id, &element, invalid)
                .unwrap_err();
            assert_eq!(value_error.kind, "invalid_value");
            assert_eq!(value_error.execution_state, ExecutionState::NotStarted);
        }
        assert_eq!(
            supervisor
                .select_option(
                    &browser.browser_id,
                    &page.page_id,
                    &element,
                    &"x".repeat(MAX_INPUT_TEXT_BYTES + 1),
                )
                .unwrap_err()
                .kind,
            "invalid_option"
        );
        assert_eq!(
            supervisor
                .set_value(
                    &browser.browser_id,
                    &page.page_id,
                    &element,
                    &"x".repeat(MAX_INPUT_TEXT_BYTES + 1),
                )
                .unwrap_err()
                .kind,
            "invalid_value"
        );
    }

    #[test]
    fn screenshot_bytes_are_bounded_before_projection() {
        let error = screenshot_result(
            "browser_test",
            "page_test",
            BackendScreenshot {
                data: general_purpose::STANDARD.encode(vec![0u8; MAX_IMAGE_BYTES + 1]),
                width: 1,
                height: 1,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, "image_too_large");
        assert_eq!(error.execution_state, ExecutionState::Completed);
    }

    #[test]
    fn screenshot_dimensions_are_bounded_before_projection() {
        let error = screenshot_result(
            "browser_test",
            "page_test",
            BackendScreenshot {
                data: general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nFAKE"),
                width: MAX_IMAGE_DIMENSION + 1,
                height: 1,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, "invalid_image_dimensions");
        assert_eq!(error.execution_state, ExecutionState::Completed);
    }

    #[test]
    fn diagnostic_projection_is_serialized_bounded_and_keeps_newest_entries() {
        let entries = (0..200)
            .map(|index| {
                serde_json::json!({
                    "index": index,
                    "text": format!("{index}-{}", "\"".repeat(2048)),
                })
            })
            .collect::<Vec<_>>();
        let (console, console_truncated) =
            bounded_recent_json_entries(entries.clone(), DIAGNOSTIC_SECTION_ENTRIES_BYTES);
        let (network, network_truncated) =
            bounded_recent_json_entries(entries, DIAGNOSTIC_SECTION_ENTRIES_BYTES);
        assert!(console_truncated);
        assert!(network_truncated);
        assert_eq!(console.last().unwrap()["index"], 199);
        assert_eq!(network.last().unwrap()["index"], 199);
        let output = serde_json::json!({
            "console_retained": 200,
            "console_count": console.len(),
            "console_truncated": console_truncated,
            "console": console,
            "network_retained": 200,
            "network_count": network.len(),
            "network_truncated": network_truncated,
            "network": network,
        });
        assert!(
            serde_json::to_vec(&output).unwrap().len() <= MAX_BROWSER_OBSERVATION_RESULT_BYTES,
            "diagnostics result exceeded the ordinary Runner retention-safe budget"
        );
    }

    #[test]
    fn idle_and_absolute_lifetime_expire_owned_browser_runtimes() {
        let supervisor = fixture();
        let browser = supervisor.launch().unwrap();
        {
            let mut state = supervisor.state();
            let runtime = state.browsers.get_mut(&browser.browser_id).unwrap();
            runtime.last_activity_at = Instant::now() - BROWSER_IDLE_TIMEOUT;
        }
        assert!(supervisor.list_browsers().is_empty());

        let browser = supervisor.launch().unwrap();
        {
            let mut state = supervisor.state();
            let runtime = state.browsers.get_mut(&browser.browser_id).unwrap();
            runtime.created_at = Instant::now() - MAX_BROWSER_LIFETIME;
        }
        assert!(supervisor.list_browsers().is_empty());
    }

    #[test]
    fn launch_and_shutdown_are_bounded_and_owned() {
        let supervisor = fixture();
        supervisor.launch().unwrap();
        supervisor.begin_shutdown();
        assert_eq!(
            supervisor.launch().unwrap_err().kind,
            "runner_shutting_down"
        );
        let report = supervisor.shutdown_until(Instant::now() + Duration::from_secs(1));
        assert_eq!(report.browsers, 1);
        assert_eq!(report.failures, 0);
    }
}
