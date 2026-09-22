use crate::types::{
    clip_bytes, BrowserError, BrowserKey, BrowserResult, BrowserStability, LAUNCH_TIMEOUT,
    MAX_NODE_TEXT_BYTES, MAX_PAGES_PER_BROWSER, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_NODES,
    REQUEST_TIMEOUT,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tungstenite::{client::client_with_config, protocol::WebSocketConfig, Message, WebSocket};
use url::Url;
use webcodex_process::ManagedChild;

const MAX_CDP_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CDP_LIST_BYTES: usize = 256 * 1024;

pub(crate) trait BackendFactory: Send + Sync {
    fn available(&self) -> bool;
    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>>;
}

pub(crate) trait BrowserBackend: Send {
    fn pages(&mut self) -> BrowserResult<Vec<BackendPage>>;
    fn new_page(&mut self) -> BrowserResult<String>;
    fn snapshot(&mut self, target_id: &str, max_depth: u32) -> BrowserResult<BackendSnapshot>;
    fn screenshot(&mut self, target_id: &str) -> BrowserResult<BackendScreenshot>;
    fn console(
        &mut self,
        target_id: &str,
    ) -> BrowserResult<BackendEventSnapshot<BackendConsoleEntry>>;
    fn network(
        &mut self,
        target_id: &str,
    ) -> BrowserResult<BackendEventSnapshot<BackendNetworkEntry>>;
    fn diagnostics(&mut self, target_id: &str) -> BrowserResult<BackendDiagnosticsSnapshot>;
    fn clear_diagnostics(&mut self, target_id: &str) -> BrowserResult<()>;
    fn navigate(&mut self, target_id: &str, url: &str) -> BrowserResult<()>;
    fn reload(&mut self, target_id: &str) -> BrowserResult<()>;
    fn click(&mut self, target_id: &str, backend_node_id: i64) -> BrowserResult<()>;
    fn input_text(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        text: &str,
    ) -> BrowserResult<()>;
    fn select_option(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        option: &str,
    ) -> BrowserResult<()>;
    fn set_value(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        value: &str,
    ) -> BrowserResult<()>;
    fn upload_file(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        path: &Path,
    ) -> BrowserResult<()>;
    fn key(&mut self, target_id: &str, key: BrowserKey) -> BrowserResult<()>;
    fn wait_for_stable(
        &mut self,
        target_id: &str,
        timeout: Duration,
    ) -> BrowserResult<BrowserStability>;
    fn close_page(&mut self, target_id: &str) -> BrowserResult<()>;
    fn shutdown(&mut self, timeout: Duration) -> BrowserResult<()>;
}

#[derive(Debug, Clone)]
pub(crate) struct BackendPage {
    pub(crate) target_id: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) document_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BackendNode {
    pub(crate) role: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) group_key: Option<String>,
    pub(crate) group_role: Option<String>,
    pub(crate) group_label: Option<String>,
    pub(crate) checked: Option<String>,
    pub(crate) selected: Option<bool>,
    pub(crate) required: Option<bool>,
    pub(crate) disabled: Option<bool>,
    pub(crate) read_only: Option<bool>,
    pub(crate) backend_node_id: Option<i64>,
    pub(crate) actionable: bool,
}

#[derive(Debug)]
pub(crate) struct BackendSnapshot {
    pub(crate) document_id: String,
    pub(crate) nodes: Vec<BackendNode>,
    pub(crate) truncated: bool,
}

#[derive(Debug)]
pub(crate) struct BackendScreenshot {
    pub(crate) data: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct BackendConsoleEntry {
    pub(crate) sequence: u64,
    pub(crate) level: String,
    pub(crate) text: String,
    pub(crate) source: Option<String>,
    pub(crate) timestamp: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct BackendNetworkEntry {
    pub(crate) sequence: u64,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) resource_type: Option<String>,
    pub(crate) status: Option<u16>,
    pub(crate) failed_reason: Option<String>,
    pub(crate) timestamp: Option<f64>,
}

#[derive(Debug)]
pub(crate) struct BackendEventSnapshot<T> {
    pub(crate) entries: Vec<T>,
    pub(crate) truncated: bool,
    pub(crate) cursor: u64,
    pub(crate) oldest_sequence: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct BackendDiagnosticsSnapshot {
    pub(crate) console: BackendEventSnapshot<BackendConsoleEntry>,
    pub(crate) network: BackendEventSnapshot<BackendNetworkEntry>,
    pub(crate) cursor: u64,
    pub(crate) cleared_through_cursor: u64,
}

#[derive(Default)]
struct CdpEventBuffer {
    console: VecDeque<BackendConsoleEntry>,
    console_truncated: bool,
    network: HashMap<String, BackendNetworkEntry>,
    network_order: VecDeque<String>,
    network_truncated: bool,
    pending_network: HashSet<String>,
    next_sequence: u64,
    cleared_through_sequence: u64,
}

impl CdpEventBuffer {
    fn next_sequence(&mut self) -> u64 {
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.next_sequence
    }

    fn push_console(&mut self, mut entry: BackendConsoleEntry) {
        entry.sequence = self.next_sequence();
        if self.console.len() >= crate::types::MAX_CONSOLE_ENTRIES {
            self.console.pop_front();
            self.console_truncated = true;
        }
        self.console.push_back(entry);
    }

    fn insert_network(&mut self, request_id: String, mut entry: BackendNetworkEntry) {
        entry.sequence = self.next_sequence();
        self.pending_network.insert(request_id.clone());
        if self.network.contains_key(&request_id) {
            self.network_order
                .retain(|existing| existing != &request_id);
        } else if self.network.len() >= crate::types::MAX_NETWORK_ENTRIES {
            if let Some(oldest) = self.network_order.pop_front() {
                self.network.remove(&oldest);
                self.network_truncated = true;
            }
        }
        self.network.insert(request_id.clone(), entry);
        self.network_order.push_back(request_id);
    }

    fn update_network<F>(&mut self, request_id: &str, update: F)
    where
        F: FnOnce(&mut BackendNetworkEntry),
    {
        let sequence = self.next_sequence();
        if let Some(entry) = self.network.get_mut(request_id) {
            update(entry);
            entry.sequence = sequence;
        }
    }

    fn finish_network(&mut self, request_id: &str) {
        self.pending_network.remove(request_id);
        self.update_network(request_id, |_| {});
    }

    fn clear(&mut self) {
        self.cleared_through_sequence = self.next_sequence;
        self.console.clear();
        self.console_truncated = false;
        self.network.clear();
        self.network_order.clear();
        self.network_truncated = false;
        self.pending_network.clear();
    }

    fn cursor(&self) -> u64 {
        self.next_sequence
    }

    fn network_cursor(&self) -> u64 {
        self.network
            .values()
            .map(|entry| entry.sequence)
            .max()
            .unwrap_or(0)
    }

    fn pending_count(&self) -> usize {
        self.pending_network.len()
    }

    fn console_oldest_sequence(&self) -> Option<u64> {
        self.console.front().map(|entry| entry.sequence)
    }

    fn network_oldest_sequence(&self) -> Option<u64> {
        self.network.values().map(|entry| entry.sequence).min()
    }
}

struct CdpEventCollector {
    websocket: WebSocket<TcpStream>,
    events: CdpEventBuffer,
}

pub(crate) struct ChromiumFactory;

impl BackendFactory for ChromiumFactory {
    fn available(&self) -> bool {
        discover_chromium_executable().is_some()
    }

    fn launch(&self) -> BrowserResult<Box<dyn BrowserBackend>> {
        let executable = discover_chromium_executable().ok_or_else(|| {
            BrowserError::not_started(
                "browser_unavailable",
                "no supported Chromium-family browser is installed",
            )
        })?;
        CdpBackend::launch(&executable).map(|backend| Box::new(backend) as Box<dyn BrowserBackend>)
    }
}

pub fn discover_chromium_executable() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        for variable in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(variable) {
                let base = PathBuf::from(base);
                candidates.push(base.join("Microsoft/Edge/Application/msedge.exe"));
                candidates.push(base.join("Google/Chrome/Application/chrome.exe"));
            }
        }
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        for executable in ["msedge.exe", "chrome.exe"] {
            if let Some(path) = find_in_path(executable) {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for candidate in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ] {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for executable in [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
        ] {
            if let Some(path) = find_in_path(executable) {
                return Some(path);
            }
        }
    }

    None
}

fn parse_devtools_active_port(contents: &str) -> Option<(u16, String)> {
    let mut lines = contents.lines();
    let port = lines.next()?.parse::<u16>().ok()?;
    let path = lines.next()?;
    if port == 0 || !path.starts_with("/devtools/browser/") || path.contains(['\r', '\n']) {
        return None;
    }
    Some((port, path.to_string()))
}

fn find_in_path(executable: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|path| path.join(executable))
        .find(|path| path.is_file())
}

fn cleanup_failed_launch(child: &mut ManagedChild) {
    const FAILED_LAUNCH_CLEANUP_TIMEOUT: Duration = Duration::from_millis(500);
    let _ = child.terminate_tree();
    let _ = child.wait_tree_exit(FAILED_LAUNCH_CLEANUP_TIMEOUT);
    let _ = child.try_wait();
}

struct CdpBackend {
    child: ManagedChild,
    _profile: TempDir,
    endpoint: Url,
    next_id: u64,
    collectors: HashMap<String, CdpEventCollector>,
}

impl CdpBackend {
    fn launch(executable: &Path) -> BrowserResult<Self> {
        let profile = tempfile::Builder::new()
            .prefix("webcodex-browser-")
            .tempdir()
            .map_err(|error| {
                BrowserError::not_started(
                    "profile_create_failed",
                    format!("could not create ephemeral Browser profile: {error}"),
                )
            })?;
        let mut command = Command::new(executable);
        command
            .arg("--headless=new")
            .arg("--window-size=1440,900")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg("--remote-debugging-port=0")
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = ManagedChild::spawn(&mut command).map_err(|error| {
            BrowserError::not_started(
                "launch_failed",
                format!("could not start Browser runtime: {error}"),
            )
        })?;
        let active_port = profile.path().join("DevToolsActivePort");
        let deadline = Instant::now() + LAUNCH_TIMEOUT;
        let (port, websocket_path) = loop {
            if let Ok(contents) = std::fs::read_to_string(&active_port) {
                if let Some(endpoint) = parse_devtools_active_port(&contents) {
                    break endpoint;
                }
            }
            if child.try_wait().ok().flatten().is_some() {
                cleanup_failed_launch(&mut child);
                return Err(BrowserError::not_started(
                    "launch_failed",
                    "Browser exited before loopback CDP became ready",
                ));
            }
            if Instant::now() >= deadline {
                cleanup_failed_launch(&mut child);
                return Err(BrowserError::not_started(
                    "launch_timeout",
                    "Browser did not expose loopback CDP before launch timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let endpoint = match Url::parse(&format!("ws://127.0.0.1:{port}{websocket_path}")) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                cleanup_failed_launch(&mut child);
                return Err(BrowserError::not_started(
                    "cdp_endpoint_invalid",
                    "Browser returned an invalid loopback CDP endpoint",
                ));
            }
        };
        if endpoint.host_str() != Some("127.0.0.1") {
            cleanup_failed_launch(&mut child);
            return Err(BrowserError::not_started(
                "cdp_endpoint_invalid",
                "Browser CDP endpoint was not loopback",
            ));
        }
        Ok(Self {
            child,
            _profile: profile,
            endpoint,
            next_id: 1,
            collectors: HashMap::new(),
        })
    }

    fn browser_call(&mut self, method: &str, params: Value, effect: bool) -> BrowserResult<Value> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.browser_call_until(method, params, effect, deadline)
    }

    fn browser_call_until(
        &mut self,
        method: &str,
        params: Value,
        effect: bool,
        deadline: Instant,
    ) -> BrowserResult<Value> {
        cdp_call_until(
            &self.endpoint,
            &mut self.next_id,
            method,
            params,
            effect,
            deadline,
        )
    }

    fn page_descriptors_until(&self, deadline: Instant) -> BrowserResult<Vec<Value>> {
        fetch_page_descriptors(self.endpoint.port().unwrap_or_default(), deadline)
    }

    fn page_endpoint_until(&self, target_id: &str, deadline: Instant) -> BrowserResult<Url> {
        let list = self.page_descriptors_until(deadline)?;
        let websocket_url = list
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(target_id))
            .and_then(|value| {
                value
                    .get("webSocketDebuggerUrl")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| {
                BrowserError::not_started("stale_page", "page target is no longer available")
            })?;
        let endpoint = Url::parse(&websocket_url).map_err(|_| {
            BrowserError::observed("cdp_endpoint_invalid", "page CDP endpoint is invalid", None)
        })?;
        if !matches!(endpoint.host_str(), Some("127.0.0.1") | Some("localhost"))
            || endpoint.scheme() != "ws"
            || endpoint.port().is_none()
        {
            return Err(BrowserError::observed(
                "cdp_endpoint_invalid",
                "page CDP endpoint is not a bounded loopback websocket",
                None,
            ));
        }
        Ok(endpoint)
    }

    fn page_call_until(
        &mut self,
        target_id: &str,
        method: &str,
        params: Value,
        effect: bool,
        deadline: Instant,
    ) -> BrowserResult<Value> {
        let endpoint = self
            .page_endpoint_until(target_id, deadline)
            .map_err(|error| {
                if effect {
                    pre_dispatch_error(error)
                } else {
                    error
                }
            })?;
        cdp_call_until(
            &endpoint,
            &mut self.next_id,
            method,
            params,
            effect,
            deadline,
        )
    }

    fn ensure_event_collector(&mut self, target_id: &str) -> BrowserResult<()> {
        if self.collectors.contains_key(target_id) {
            return Ok(());
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let endpoint = self.page_endpoint_until(target_id, deadline)?;
        let mut websocket = open_loopback_websocket(&endpoint, deadline)?;
        for method in ["Runtime.enable", "Log.enable", "Network.enable"] {
            cdp_call_on_websocket_until(
                &mut websocket,
                &mut self.next_id,
                method,
                json!({}),
                false,
                deadline,
            )?;
        }
        self.collectors.insert(
            target_id.to_string(),
            CdpEventCollector {
                websocket,
                events: CdpEventBuffer::default(),
            },
        );
        Ok(())
    }

    fn drain_target_collector(&mut self, target_id: &str) -> BrowserResult<()> {
        let result = {
            let collector = self
                .collectors
                .get_mut(target_id)
                .expect("collector exists");
            drain_event_collector(collector)
        };
        if result.is_err() {
            self.collectors.remove(target_id);
        }
        result
    }

    fn page_list_until(&self, deadline: Instant) -> BrowserResult<Vec<BackendPage>> {
        let list = self.page_descriptors_until(deadline)?;
        let mut pages = Vec::new();
        for value in list {
            if value.get("type").and_then(Value::as_str) != Some("page") {
                continue;
            }
            let Some(target_id) = value.get("id").and_then(Value::as_str) else {
                continue;
            };
            pages.push(BackendPage {
                target_id: target_id.to_string(),
                title: value
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                url: value
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                document_id: value
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("about:blank")
                    .to_string(),
            });
            if pages.len() >= MAX_PAGES_PER_BROWSER {
                break;
            }
        }
        Ok(pages)
    }

    fn call_element_function_until(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        function_declaration: &'static str,
        argument: &str,
        deadline: Instant,
    ) -> BrowserResult<()> {
        // Remote object ids are scoped to the DevTools session that created
        // them. Keep resolveNode and callFunctionOn on one page websocket.
        let endpoint = self
            .page_endpoint_until(target_id, deadline)
            .map_err(pre_dispatch_error)?;
        let mut websocket =
            open_loopback_websocket(&endpoint, deadline).map_err(pre_dispatch_error)?;
        let resolved = cdp_call_on_websocket_until(
            &mut websocket,
            &mut self.next_id,
            "DOM.resolveNode",
            json!({ "backendNodeId": backend_node_id }),
            false,
            deadline,
        )
        .map_err(pre_dispatch_error)?;
        let object_id = resolved
            .pointer("/object/objectId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BrowserError::not_started(
                    "element_not_actionable",
                    "CDP could not resolve the current form control",
                )
            })?;
        let result = cdp_call_on_websocket_until(
            &mut websocket,
            &mut self.next_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": function_declaration,
                "arguments": [{ "value": argument }],
                "returnByValue": true,
                "userGesture": true,
            }),
            true,
            deadline,
        )?;
        if result.get("exceptionDetails").is_some() {
            return Err(BrowserError::uncertain(
                "form_control_script_failed",
                "form-control effect raised after dispatch",
                "snapshot",
            ));
        }
        let outcome = result.pointer("/result/value").ok_or_else(|| {
            BrowserError::uncertain(
                "form_control_result_invalid",
                "form-control effect returned no bounded result",
                "snapshot",
            )
        })?;
        if outcome.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        if outcome.get("mutated").and_then(Value::as_bool) == Some(true) {
            return Err(BrowserError::uncertain(
                "form_control_outcome_unknown",
                "form-control state changed before its postcondition failed",
                "snapshot",
            ));
        }
        let kind = outcome
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("form_control_rejected");
        let (kind, message) = match kind {
            "element_not_select" => (
                "element_not_select",
                "target element is not a native select control",
            ),
            "option_not_found" => (
                "option_not_found",
                "no native option matched the exact value or visible label",
            ),
            "option_ambiguous" => (
                "option_ambiguous",
                "more than one native option matched the requested visible label",
            ),
            "option_disabled" => ("option_disabled", "the requested native option is disabled"),
            "control_disabled" => (
                "control_disabled",
                "target native form control is disabled or read-only",
            ),
            "element_not_value_control" => (
                "element_not_value_control",
                "target element does not support exact Browser value assignment",
            ),
            "unsupported_value_control" => (
                "unsupported_value_control",
                "target input type is not supported by exact structured value assignment",
            ),
            "invalid_control_value" => (
                "invalid_control_value",
                "Browser rejected, normalized, or constraint-invalidated the requested native control value",
            ),
            _ => (
                "form_control_rejected",
                "Browser rejected the requested form-control effect before mutation",
            ),
        };
        Err(BrowserError::not_started(kind, message))
    }
}

impl BrowserBackend for CdpBackend {
    fn pages(&mut self) -> BrowserResult<Vec<BackendPage>> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let mut pages = self.page_list_until(deadline)?;
        for page in &mut pages {
            if Instant::now() >= deadline {
                break;
            }
            if let Ok(tree) = self.page_call_until(
                &page.target_id,
                "Page.getFrameTree",
                json!({}),
                false,
                deadline,
            ) {
                if let Some(loader_id) = tree
                    .pointer("/frameTree/frame/loaderId")
                    .and_then(Value::as_str)
                {
                    page.document_id = loader_id.to_string();
                }
            }
        }
        Ok(pages)
    }

    fn new_page(&mut self) -> BrowserResult<String> {
        let result =
            self.browser_call("Target.createTarget", json!({ "url": "about:blank" }), true)?;
        result
            .get("targetId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                BrowserError::uncertain(
                    "create_page_result_invalid",
                    "CDP did not return targetId after page creation",
                    "pages",
                )
            })
    }

    fn snapshot(&mut self, target_id: &str, max_depth: u32) -> BrowserResult<BackendSnapshot> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let frame_tree =
            self.page_call_until(target_id, "Page.getFrameTree", json!({}), false, deadline)?;
        let document_id = frame_tree
            .pointer("/frameTree/frame/loaderId")
            .and_then(Value::as_str)
            .unwrap_or("unknown-document")
            .to_string();
        let accessibility = self.page_call_until(
            target_id,
            "Accessibility.getFullAXTree",
            json!({ "depth": max_depth.clamp(1, 32) }),
            false,
            deadline,
        )?;
        let raw_nodes = accessibility
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (nodes, truncated) = parse_ax_snapshot_nodes(&raw_nodes);
        Ok(BackendSnapshot {
            document_id,
            nodes,
            truncated,
        })
    }

    fn screenshot(&mut self, target_id: &str) -> BrowserResult<BackendScreenshot> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let metrics = self
            .page_call_until(
                target_id,
                "Page.getLayoutMetrics",
                json!({}),
                false,
                deadline,
            )
            .unwrap_or_else(|_| json!({}));
        let width = viewport_dimension(&metrics, "/cssVisualViewport/clientWidth");
        let height = viewport_dimension(&metrics, "/cssVisualViewport/clientHeight");
        let result = self.page_call_until(
            target_id,
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "fromSurface": true,
                "captureBeyondViewport": false
            }),
            false,
            deadline,
        )?;
        let data = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BrowserError::observed("invalid_image", "CDP screenshot response had no data", None)
            })?
            .to_string();
        Ok(BackendScreenshot {
            data,
            width,
            height,
        })
    }

    fn console(
        &mut self,
        target_id: &str,
    ) -> BrowserResult<BackendEventSnapshot<BackendConsoleEntry>> {
        self.ensure_event_collector(target_id)?;
        self.drain_target_collector(target_id)?;
        let collector = self
            .collectors
            .get(target_id)
            .expect("collector survives successful drain");
        Ok(BackendEventSnapshot {
            entries: collector.events.console.iter().cloned().collect(),
            truncated: collector.events.console_truncated,
            cursor: collector.events.cursor(),
            oldest_sequence: collector.events.console_oldest_sequence(),
        })
    }

    fn network(
        &mut self,
        target_id: &str,
    ) -> BrowserResult<BackendEventSnapshot<BackendNetworkEntry>> {
        self.ensure_event_collector(target_id)?;
        self.drain_target_collector(target_id)?;
        let collector = self
            .collectors
            .get(target_id)
            .expect("collector survives successful drain");
        let mut values = collector
            .events
            .network
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|a, b| {
            a.timestamp
                .partial_cmp(&b.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(BackendEventSnapshot {
            entries: values,
            truncated: collector.events.network_truncated,
            cursor: collector.events.cursor(),
            oldest_sequence: collector.events.network_oldest_sequence(),
        })
    }

    fn diagnostics(&mut self, target_id: &str) -> BrowserResult<BackendDiagnosticsSnapshot> {
        self.ensure_event_collector(target_id)?;
        self.drain_target_collector(target_id)?;
        let collector = self
            .collectors
            .get(target_id)
            .expect("collector survives successful drain");
        let cursor = collector.events.cursor();
        let mut network = collector
            .events
            .network
            .values()
            .cloned()
            .collect::<Vec<_>>();
        network.sort_by_key(|entry| entry.sequence);
        Ok(BackendDiagnosticsSnapshot {
            console: BackendEventSnapshot {
                entries: collector.events.console.iter().cloned().collect(),
                truncated: collector.events.console_truncated,
                cursor,
                oldest_sequence: collector.events.console_oldest_sequence(),
            },
            network: BackendEventSnapshot {
                entries: network,
                truncated: collector.events.network_truncated,
                cursor,
                oldest_sequence: collector.events.network_oldest_sequence(),
            },
            cursor,
            cleared_through_cursor: collector.events.cleared_through_sequence,
        })
    }

    fn clear_diagnostics(&mut self, target_id: &str) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let next_id = &mut self.next_id;
        let collector = self
            .collectors
            .get_mut(target_id)
            .expect("collector exists");
        clear_event_collector(collector, next_id)
    }

    fn navigate(&mut self, target_id: &str, url: &str) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.page_call_until(
            target_id,
            "Page.navigate",
            json!({ "url": url }),
            true,
            deadline,
        )
        .map(|_| ())
    }

    fn reload(&mut self, target_id: &str) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.page_call_until(target_id, "Page.reload", json!({}), true, deadline)
            .map(|_| ())
    }

    fn click(&mut self, target_id: &str, backend_node_id: i64) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        // CDP mouse coordinates are viewport-relative. Ensure off-screen
        // controls are visible before deriving the box-model click point.
        self.page_call_until(
            target_id,
            "DOM.scrollIntoViewIfNeeded",
            json!({ "backendNodeId": backend_node_id }),
            false,
            deadline,
        )
        .map_err(pre_dispatch_error)?;
        let model = self
            .page_call_until(
                target_id,
                "DOM.getBoxModel",
                json!({ "backendNodeId": backend_node_id }),
                false,
                deadline,
            )
            .map_err(pre_dispatch_error)?;
        let quad = model
            .pointer("/model/content")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                BrowserError::not_started(
                    "element_not_actionable",
                    "element has no current box model",
                )
            })?;
        let coordinates = quad.iter().filter_map(Value::as_f64).collect::<Vec<_>>();
        if coordinates.len() < 8 {
            return Err(BrowserError::not_started(
                "element_not_actionable",
                "element box model is invalid",
            ));
        }
        let x = (coordinates[0] + coordinates[2] + coordinates[4] + coordinates[6]) / 4.0;
        let y = (coordinates[1] + coordinates[3] + coordinates[5] + coordinates[7]) / 4.0;
        self.page_call_until(
            target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
            true,
            deadline,
        )?;
        self.page_call_until(
            target_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
            true,
            deadline,
        )
        .map(|_| ())
        .map_err(|error| post_effect_error(error, "snapshot"))
    }

    fn input_text(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        text: &str,
    ) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.page_call_until(
            target_id,
            "DOM.focus",
            json!({ "backendNodeId": backend_node_id }),
            true,
            deadline,
        )?;
        self.page_call_until(
            target_id,
            "Input.insertText",
            json!({ "text": text }),
            true,
            deadline,
        )
        .map(|_| ())
        .map_err(|error| post_effect_error(error, "snapshot"))
    }

    fn select_option(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        option: &str,
    ) -> BrowserResult<()> {
        const SELECT_OPTION: &str = r#"function(requested) {
            if (!(this instanceof HTMLSelectElement)) {
                return { ok: false, kind: "element_not_select" };
            }
            if (this.disabled) {
                return { ok: false, kind: "control_disabled" };
            }
            const options = Array.from(this.options);
            let matches = options.filter((item) => item.value === requested);
            if (matches.length === 0) {
                matches = options.filter((item) => item.text.trim() === requested);
            }
            if (matches.length === 0) {
                return { ok: false, kind: "option_not_found" };
            }
            if (matches.length !== 1) {
                return { ok: false, kind: "option_ambiguous" };
            }
            if (matches[0].disabled) {
                return { ok: false, kind: "option_disabled" };
            }
            const selectedValue = matches[0].value;
            this.value = selectedValue;
            this.dispatchEvent(new Event("input", { bubbles: true }));
            this.dispatchEvent(new Event("change", { bubbles: true }));
            if (this.value !== selectedValue) {
                return { ok: false, kind: "select_postcondition_failed", mutated: true };
            }
            return { ok: true };
        }"#;
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.call_element_function_until(
            target_id,
            backend_node_id,
            SELECT_OPTION,
            option,
            deadline,
        )
    }

    fn set_value(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        value: &str,
    ) -> BrowserResult<()> {
        const SET_VALUE: &str = r#"function(requested) {
            if (!(this instanceof HTMLInputElement)) {
                return { ok: false, kind: "element_not_value_control" };
            }
            if (this.disabled || this.readOnly) {
                return { ok: false, kind: "control_disabled" };
            }
            const type = (this.type || "text").toLowerCase();
            const structuredTypes = new Set([
                "date", "datetime-local", "month", "week", "time", "number", "range", "color"
            ]);
            if (!structuredTypes.has(type)) {
                return { ok: false, kind: "unsupported_value_control" };
            }
            const probe = document.createElement("input");
            probe.type = type;
            for (const attribute of ["min", "max", "step"]) {
                if (this.hasAttribute(attribute)) {
                    probe.setAttribute(attribute, this.getAttribute(attribute));
                }
            }
            probe.value = requested;
            if (probe.value !== requested || !probe.checkValidity()) {
                return { ok: false, kind: "invalid_control_value" };
            }
            this.value = requested;
            if (this.value !== requested) {
                return { ok: false, kind: "value_postcondition_failed", mutated: true };
            }
            this.dispatchEvent(new Event("input", { bubbles: true }));
            this.dispatchEvent(new Event("change", { bubbles: true }));
            if (this.value !== requested) {
                return { ok: false, kind: "value_postcondition_failed", mutated: true };
            }
            return { ok: true };
        }"#;
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.call_element_function_until(target_id, backend_node_id, SET_VALUE, value, deadline)
    }

    fn upload_file(
        &mut self,
        target_id: &str,
        backend_node_id: i64,
        path: &Path,
    ) -> BrowserResult<()> {
        let upload_path = path.to_str().ok_or_else(|| {
            BrowserError::not_started(
                "upload_path_unrepresentable",
                "Browser upload path is not representable as a CDP UTF-8 path",
            )
        })?;
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        self.page_call_until(
            target_id,
            "DOM.setFileInputFiles",
            json!({
                "files": [upload_path],
                "backendNodeId": backend_node_id,
            }),
            true,
            deadline,
        )
        .map(|_| ())
        .map_err(|error| post_effect_error(error, "snapshot"))
    }

    fn key(&mut self, target_id: &str, key: BrowserKey) -> BrowserResult<()> {
        self.ensure_event_collector(target_id)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let (key_name, code, text) = key.cdp();
        self.page_call_until(
            target_id,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyDown",
                "key": key_name,
                "code": code,
                "text": text
            }),
            true,
            deadline,
        )?;
        self.page_call_until(
            target_id,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "key": key_name,
                "code": code
            }),
            true,
            deadline,
        )
        .map(|_| ())
        .map_err(|error| post_effect_error(error, "snapshot"))
    }

    fn wait_for_stable(
        &mut self,
        target_id: &str,
        timeout: Duration,
    ) -> BrowserResult<BrowserStability> {
        const QUIET_PERIOD: Duration = Duration::from_millis(250);
        const LONG_LIVED_NETWORK_QUIET_PERIOD: Duration = Duration::from_millis(750);
        const POLL_INTERVAL: Duration = Duration::from_millis(75);
        const STABILITY_EXPRESSION: &str = r#"JSON.stringify({
            ready: document.readyState,
            href: location.href,
            elements: document.getElementsByTagName("*").length,
            text: document.body ? document.body.innerText.length : 0
        })"#;

        let started = Instant::now();
        let deadline = started + timeout;
        let mut last_signature: Option<String> = None;
        let mut last_network_cursor: Option<u64> = None;
        let mut quiet_since: Option<Instant> = None;
        let mut last_reason = "deadline".to_string();

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(BrowserStability {
                    stable: false,
                    waited_ms: started.elapsed().as_millis() as u64,
                    reason: last_reason,
                });
            }

            if self.ensure_event_collector(target_id).is_err()
                || self.drain_target_collector(target_id).is_err()
            {
                last_reason = "collector_recovering".to_string();
                std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
                continue;
            }

            let (pending, network_cursor) = self
                .collectors
                .get(target_id)
                .map(|collector| {
                    (
                        collector.events.pending_count(),
                        collector.events.network_cursor(),
                    )
                })
                .unwrap_or((0, 0));
            let observed = self.page_call_until(
                target_id,
                "Runtime.evaluate",
                json!({
                    "expression": STABILITY_EXPRESSION,
                    "returnByValue": true,
                    "awaitPromise": false
                }),
                false,
                deadline,
            );

            let Ok(observed) = observed else {
                last_reason = "document_recovering".to_string();
                std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
                continue;
            };
            let Some(signature) = observed
                .pointer("/result/value")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                last_reason = "document_unavailable".to_string();
                std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
                continue;
            };
            let ready = serde_json::from_str::<Value>(&signature)
                .ok()
                .and_then(|value| {
                    value
                        .get("ready")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .is_some_and(|state| state != "loading");

            let dom_unchanged = last_signature.as_deref() == Some(signature.as_str());
            let network_unchanged = last_network_cursor == Some(network_cursor);
            if ready && dom_unchanged && network_unchanged {
                let since = quiet_since.get_or_insert(now);
                let required_quiet = if pending == 0 {
                    QUIET_PERIOD
                } else {
                    LONG_LIVED_NETWORK_QUIET_PERIOD
                };
                if since.elapsed() >= required_quiet {
                    return Ok(BrowserStability {
                        stable: true,
                        waited_ms: started.elapsed().as_millis() as u64,
                        reason: if pending == 0 {
                            "dom_and_network_quiet".to_string()
                        } else {
                            "dom_quiet_with_long_lived_network".to_string()
                        },
                    });
                }
                last_reason = if pending == 0 {
                    "quiet_period".to_string()
                } else {
                    "long_lived_network_quiet_period".to_string()
                };
            } else {
                quiet_since = ready.then_some(now);
                last_reason = if !ready {
                    "document_loading".to_string()
                } else if !dom_unchanged {
                    "dom_changed".to_string()
                } else {
                    "network_changed".to_string()
                };
            }
            last_signature = Some(signature);
            last_network_cursor = Some(network_cursor);
            std::thread::sleep(
                POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn close_page(&mut self, target_id: &str) -> BrowserResult<()> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let result = self
            .browser_call_until(
                "Target.closeTarget",
                json!({ "targetId": target_id }),
                true,
                deadline,
            )
            .map(|_| ());
        if result.is_ok() {
            self.collectors.remove(target_id);
        }
        result
    }

    fn shutdown(&mut self, timeout: Duration) -> BrowserResult<()> {
        let deadline = Instant::now() + timeout;
        if !timeout.is_zero() {
            let _ = self.browser_call_until("Browser.close", json!({}), true, deadline);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let graceful = self.child.wait_tree_exit(remaining).unwrap_or(false);
        if !graceful {
            self.child.terminate_tree().map_err(|error| {
                BrowserError::uncertain(
                    "browser_shutdown_failed",
                    format!("could not terminate owned Browser process tree: {error}"),
                    "browsers",
                )
            })?;
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(1));
            let _ = self.child.wait_tree_exit(remaining);
        }
        let _ = self.child.try_wait();
        Ok(())
    }
}

fn pre_dispatch_error(mut error: BrowserError) -> BrowserError {
    error.execution_state = crate::types::ExecutionState::NotStarted;
    error.recovery_action = None;
    error
}

fn post_effect_error(error: BrowserError, recovery_action: &'static str) -> BrowserError {
    if error.execution_state == crate::types::ExecutionState::OutcomeUnknown {
        error
    } else {
        BrowserError::uncertain(
            "partial_effect_outcome_unknown",
            format!(
                "Browser effect partially completed before a later step failed: {}",
                error.message
            ),
            recovery_action,
        )
    }
}

fn viewport_dimension(metrics: &Value, pointer: &str) -> u32 {
    metrics
        .pointer(pointer)
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .round()
        .clamp(1.0, 4096.0) as u32
}

fn parse_ax_snapshot_nodes(raw_nodes: &[Value]) -> (Vec<BackendNode>, bool) {
    let raw_by_id = raw_nodes
        .iter()
        .filter_map(|node| {
            node.get("nodeId")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), node))
        })
        .collect::<HashMap<_, _>>();
    let mut nodes = Vec::new();
    let mut estimated_bytes = 0usize;
    for raw in raw_nodes {
        let role = ax_value(raw, "role").unwrap_or_else(|| "generic".to_string());
        if role == "RootWebArea" {
            continue;
        }
        let name = ax_value(raw, "name");
        let description = ax_value(raw, "description");
        let value = ax_value(raw, "value");
        let group = ax_group_context(raw, &raw_by_id);
        let checked = ax_property_string(raw, "checked");
        let selected = ax_property_bool(raw, "selected");
        let required = ax_property_bool(raw, "required");
        let disabled = ax_property_bool(raw, "disabled");
        let read_only = ax_property_bool(raw, "readonly");
        let backend_node_id = raw.get("backendDOMNodeId").and_then(Value::as_i64);
        let actionable = is_actionable(&role) && backend_node_id.is_some();
        estimated_bytes = estimated_bytes
            .saturating_add(role.len())
            .saturating_add(name.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(description.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(value.as_deref().map(str::len).unwrap_or(0))
            .saturating_add(
                group
                    .as_ref()
                    .map(|(_, role, label)| role.len() + label.len())
                    .unwrap_or(0),
            )
            .saturating_add(128);
        nodes.push(BackendNode {
            role,
            name,
            description,
            value,
            group_key: group.as_ref().map(|(key, _, _)| key.clone()),
            group_role: group.as_ref().map(|(_, role, _)| role.clone()),
            group_label: group.map(|(_, _, label)| label),
            checked,
            selected,
            required,
            disabled,
            read_only,
            backend_node_id,
            actionable,
        });
    }
    let truncated = nodes.len() > MAX_SNAPSHOT_NODES || estimated_bytes > MAX_SNAPSHOT_BYTES;
    (nodes, truncated)
}

fn ax_value(node: &Value, key: &str) -> Option<String> {
    node.get(key)?
        .get("value")?
        .as_str()
        .map(|value| clip_bytes(value, MAX_NODE_TEXT_BYTES))
}

fn ax_property<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node.get("properties")?
        .as_array()?
        .iter()
        .find(|property| property.get("name").and_then(Value::as_str) == Some(name))?
        .get("value")?
        .get("value")
}

fn ax_property_bool(node: &Value, name: &str) -> Option<bool> {
    match ax_property(node, name)? {
        Value::Bool(value) => Some(*value),
        Value::String(value) if value == "true" => Some(true),
        Value::String(value) if value == "false" => Some(false),
        _ => None,
    }
}

fn ax_property_string(node: &Value, name: &str) -> Option<String> {
    match ax_property(node, name)? {
        Value::String(value) => Some(clip_bytes(value, MAX_NODE_TEXT_BYTES)),
        Value::Bool(value) => Some(value.to_string()),
        value if value.is_number() => Some(value.to_string()),
        _ => None,
    }
}

fn ax_group_context(
    node: &Value,
    by_id: &HashMap<String, &Value>,
) -> Option<(String, String, String)> {
    let mut parent_id = node.get("parentId").and_then(Value::as_str);
    for _ in 0..8 {
        let id = parent_id?;
        let parent = by_id.get(id)?;
        let role = ax_value(parent, "role").unwrap_or_default();
        let name = ax_value(parent, "name").unwrap_or_default();
        if matches!(
            role.as_str(),
            "group" | "radiogroup" | "combobox" | "listbox"
        ) && !name.trim().is_empty()
        {
            return Some((id.to_string(), role, name));
        }
        parent_id = parent.get("parentId").and_then(Value::as_str);
    }
    None
}

fn is_actionable(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "link"
            | "textbox"
            | "searchbox"
            | "DateTime"
            | "combobox"
            | "checkbox"
            | "radio"
            | "switch"
            | "menuitem"
            | "tab"
    )
}

fn fetch_page_descriptors(port: u16, deadline: Instant) -> BrowserResult<Vec<Value>> {
    let remaining = remaining_before_dispatch(deadline)?;
    let http = format!("http://127.0.0.1:{port}/json/list");
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(remaining)
        .build()
        .map_err(|error| BrowserError::observed("cdp_connect_failed", error.to_string(), None))?;
    let mut response = client
        .get(http)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| {
            BrowserError::observed(
                "cdp_list_failed",
                format!("could not observe CDP pages: {error}"),
                None,
            )
        })?;
    let mut body = Vec::new();
    response
        .by_ref()
        .take((MAX_CDP_LIST_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| {
            BrowserError::observed(
                "cdp_list_failed",
                format!("could not read bounded CDP page list: {error}"),
                None,
            )
        })?;
    if body.len() > MAX_CDP_LIST_BYTES {
        return Err(BrowserError::observed(
            "cdp_list_too_large",
            "CDP page list exceeded the bounded response size",
            None,
        ));
    }
    serde_json::from_slice(&body).map_err(|error| {
        BrowserError::observed(
            "cdp_list_failed",
            format!("could not decode CDP page list: {error}"),
            None,
        )
    })
}

fn remaining_before_dispatch(deadline: Instant) -> BrowserResult<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(BrowserError::not_started(
            "cdp_timeout",
            "Browser CDP request exceeded its absolute deadline before dispatch",
        ))
    } else {
        Ok(remaining)
    }
}

fn configure_socket_timeout(
    websocket: &mut WebSocket<TcpStream>,
    timeout: Duration,
) -> BrowserResult<()> {
    websocket
        .get_mut()
        .set_read_timeout(Some(timeout))
        .map_err(|error| {
            BrowserError::not_started(
                "cdp_timeout_config_failed",
                format!("could not bound CDP read timeout: {error}"),
            )
        })?;
    websocket
        .get_mut()
        .set_write_timeout(Some(timeout))
        .map_err(|error| {
            BrowserError::not_started(
                "cdp_timeout_config_failed",
                format!("could not bound CDP write timeout: {error}"),
            )
        })?;
    Ok(())
}

fn open_loopback_websocket(
    endpoint: &Url,
    deadline: Instant,
) -> BrowserResult<WebSocket<TcpStream>> {
    if !matches!(endpoint.host_str(), Some("127.0.0.1") | Some("localhost"))
        || endpoint.scheme() != "ws"
    {
        return Err(BrowserError::not_started(
            "cdp_endpoint_invalid",
            "Browser CDP transport must be a plain loopback websocket",
        ));
    }
    let port = endpoint.port().ok_or_else(|| {
        BrowserError::not_started("cdp_endpoint_invalid", "Browser CDP endpoint has no port")
    })?;
    let remaining = remaining_before_dispatch(deadline)?;
    let stream =
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), remaining)
            .map_err(|error| {
                BrowserError::not_started(
                    "cdp_connect_failed",
                    format!("could not connect to loopback CDP: {error}"),
                )
            })?;
    let _ = stream.set_nodelay(true);
    stream.set_read_timeout(Some(remaining)).map_err(|error| {
        BrowserError::not_started(
            "cdp_timeout_config_failed",
            format!("could not bound CDP handshake read timeout: {error}"),
        )
    })?;
    stream.set_write_timeout(Some(remaining)).map_err(|error| {
        BrowserError::not_started(
            "cdp_timeout_config_failed",
            format!("could not bound CDP handshake write timeout: {error}"),
        )
    })?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_CDP_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_CDP_MESSAGE_BYTES));
    let (mut websocket, _) =
        client_with_config(endpoint.as_str(), stream, Some(config)).map_err(|error| {
            BrowserError::not_started(
                "cdp_connect_failed",
                format!("could not complete loopback CDP handshake: {error}"),
            )
        })?;
    configure_socket_timeout(&mut websocket, remaining_before_dispatch(deadline)?)?;
    Ok(websocket)
}

#[cfg(test)]
fn cdp_call(
    endpoint: &Url,
    next_id: &mut u64,
    method: &str,
    params: Value,
    effect: bool,
) -> BrowserResult<Value> {
    cdp_call_until(
        endpoint,
        next_id,
        method,
        params,
        effect,
        Instant::now() + REQUEST_TIMEOUT,
    )
}

fn clear_event_collector(
    collector: &mut CdpEventCollector,
    next_id: &mut u64,
) -> BrowserResult<()> {
    // A response on the same ordered CDP websocket is a bounded barrier: all
    // already-queued diagnostic messages are consumed before this response.
    // Clearing only after the barrier prevents pre-clear backlog from resurfacing
    // on the next observation without relying on a best-effort drain duration.
    cdp_call_on_websocket_until(
        &mut collector.websocket,
        next_id,
        "Page.getFrameTree",
        json!({}),
        false,
        Instant::now() + REQUEST_TIMEOUT,
    )
    .map_err(pre_dispatch_error)?;
    collector.events.clear();
    Ok(())
}

fn drain_event_collector(collector: &mut CdpEventCollector) -> BrowserResult<()> {
    let deadline = Instant::now() + Duration::from_millis(150);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        configure_socket_timeout(&mut collector.websocket, remaining)?;
        match collector.websocket.read() {
            Ok(Message::Text(text)) => {
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                record_cdp_event(&mut collector.events, &value);
            }
            Ok(Message::Close(_)) => {
                return Err(BrowserError::observed(
                    "cdp_receive_closed",
                    "diagnostic CDP stream closed before observation completed",
                    Some("pages"),
                ))
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break
            }
            Err(error) => {
                return Err(BrowserError::observed(
                    "cdp_receive_failed",
                    error.to_string(),
                    None,
                ))
            }
        }
    }
    Ok(())
}

fn record_cdp_event(buffer: &mut CdpEventBuffer, value: &Value) {
    let method = value.get("method").and_then(Value::as_str);
    let params = value.get("params").cloned().unwrap_or_default();
    match method {
        Some("Runtime.consoleAPICalled") => {
            let level = params.get("type").and_then(Value::as_str).unwrap_or("log");
            let text = params
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(console_arg_text)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            buffer.push_console(BackendConsoleEntry {
                sequence: 0,
                level: clip_bytes(level, 64),
                text: clip_bytes(&text, crate::types::MAX_DIAGNOSTIC_TEXT_BYTES),
                source: None,
                timestamp: params.get("timestamp").and_then(Value::as_f64),
            });
        }
        Some("Runtime.exceptionThrown") => {
            let details = params.get("exceptionDetails").cloned().unwrap_or_default();
            buffer.push_console(BackendConsoleEntry {
                sequence: 0,
                level: "exception".to_string(),
                text: clip_bytes(
                    details
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("Uncaught exception"),
                    crate::types::MAX_DIAGNOSTIC_TEXT_BYTES,
                ),
                source: details
                    .get("url")
                    .and_then(Value::as_str)
                    .map(|url| clip_bytes(url, crate::types::MAX_URL_BYTES)),
                timestamp: params.get("timestamp").and_then(Value::as_f64),
            });
        }
        Some("Log.entryAdded") => {
            let entry = value.pointer("/params/entry").cloned().unwrap_or_default();
            buffer.push_console(BackendConsoleEntry {
                sequence: 0,
                level: clip_bytes(
                    entry.get("level").and_then(Value::as_str).unwrap_or("info"),
                    64,
                ),
                text: clip_bytes(
                    entry
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    crate::types::MAX_DIAGNOSTIC_TEXT_BYTES,
                ),
                source: entry
                    .get("url")
                    .and_then(Value::as_str)
                    .map(|url| clip_bytes(url, crate::types::MAX_URL_BYTES)),
                timestamp: entry.get("timestamp").and_then(Value::as_f64),
            });
        }
        Some("Network.requestWillBeSent") => {
            let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
                return;
            };
            let request = params.get("request").cloned().unwrap_or_default();
            buffer.insert_network(
                request_id.to_string(),
                BackendNetworkEntry {
                    sequence: 0,
                    method: clip_bytes(
                        request
                            .get("method")
                            .and_then(Value::as_str)
                            .unwrap_or("GET"),
                        128,
                    ),
                    url: clip_bytes(
                        request
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        crate::types::MAX_URL_BYTES,
                    ),
                    resource_type: params
                        .get("type")
                        .and_then(Value::as_str)
                        .map(|resource_type| clip_bytes(resource_type, 64)),
                    status: None,
                    failed_reason: None,
                    timestamp: params.get("timestamp").and_then(Value::as_f64),
                },
            );
        }
        Some("Network.responseReceived") => {
            let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
                return;
            };
            let response = params.get("response").cloned().unwrap_or_default();
            let status = response
                .get("status")
                .and_then(Value::as_f64)
                .map(|value| value as u16);
            buffer.update_network(request_id, |entry| entry.status = status);
        }
        Some("Network.loadingFailed") => {
            let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
                return;
            };
            let failed_reason = params
                .get("errorText")
                .and_then(Value::as_str)
                .map(|value| clip_bytes(value, 512));
            buffer.update_network(request_id, |entry| entry.failed_reason = failed_reason);
            buffer.finish_network(request_id);
        }
        Some("Network.loadingFinished") => {
            let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
                return;
            };
            buffer.finish_network(request_id);
        }
        _ => {}
    }
}

fn console_arg_text(value: &Value) -> Option<String> {
    value
        .get("value")
        .map(|value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .or_else(|| {
            value
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn cdp_call_until(
    endpoint: &Url,
    next_id: &mut u64,
    method: &str,
    params: Value,
    effect: bool,
    deadline: Instant,
) -> BrowserResult<Value> {
    let mut websocket = open_loopback_websocket(endpoint, deadline)?;
    cdp_call_on_websocket_until(&mut websocket, next_id, method, params, effect, deadline)
}

fn cdp_call_on_websocket_until(
    websocket: &mut WebSocket<TcpStream>,
    next_id: &mut u64,
    method: &str,
    params: Value,
    effect: bool,
    deadline: Instant,
) -> BrowserResult<Value> {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    let request = json!({ "id": id, "method": method, "params": params }).to_string();
    configure_socket_timeout(websocket, remaining_before_dispatch(deadline)?)?;
    websocket
        .send(Message::Text(request.into()))
        .map_err(|error| {
            if effect {
                BrowserError::uncertain(
                    "cdp_delivery_uncertain",
                    format!("effect delivery became uncertain: {error}"),
                    "snapshot",
                )
            } else {
                BrowserError::observed("cdp_send_failed", error.to_string(), None)
            }
        })?;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(if effect {
                BrowserError::uncertain(
                    "cdp_receive_timeout",
                    "effect result exceeded the Browser CDP absolute deadline",
                    "snapshot",
                )
            } else {
                BrowserError::observed(
                    "cdp_receive_timeout",
                    "Browser CDP observation exceeded its absolute deadline",
                    None,
                )
            });
        }
        configure_socket_timeout(websocket, remaining)?;
        match websocket.read() {
            Ok(Message::Text(text)) => {
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if value.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(error) = value.get("error") {
                    return Err(if effect {
                        BrowserError::uncertain(
                            "cdp_effect_error",
                            format!(
                                "CDP effect returned an error after dispatch: {}",
                                clip_bytes(&error.to_string(), 256)
                            ),
                            "snapshot",
                        )
                    } else {
                        BrowserError::observed(
                            "cdp_error",
                            format!(
                                "CDP observation returned an error: {}",
                                clip_bytes(&error.to_string(), 256)
                            ),
                            None,
                        )
                    });
                }
                return Ok(value.get("result").cloned().unwrap_or_else(|| json!({})));
            }
            Ok(Message::Close(_)) => {
                return Err(if effect {
                    BrowserError::uncertain(
                        "cdp_closed",
                        "CDP connection closed after effect dispatch",
                        "snapshot",
                    )
                } else {
                    BrowserError::observed("cdp_closed", "CDP connection closed", None)
                })
            }
            Ok(_) => {}
            Err(error) => {
                return Err(if effect {
                    BrowserError::uncertain(
                        "cdp_receive_uncertain",
                        format!("effect result became uncertain: {error}"),
                        "snapshot",
                    )
                } else {
                    BrowserError::observed("cdp_receive_failed", error.to_string(), None)
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::TcpListener;
    use std::thread;
    use tungstenite::{accept, Message};

    #[test]
    fn form_control_roles_needed_for_structured_fill_are_actionable() {
        for role in ["textbox", "combobox", "DateTime"] {
            assert!(
                is_actionable(role),
                "{role} should project an element identity"
            );
        }
        assert!(
            !is_actionable("option"),
            "native option nodes are semantic choices; the owning combobox carries select_option authority"
        );
    }

    fn fake_cdp_server(reply: Option<Value>) -> (Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            let request = websocket.read().unwrap();
            let Message::Text(text) = request else {
                panic!("expected CDP text request")
            };
            let parsed: Value = serde_json::from_str(&text).unwrap();
            if let Some(result) = reply {
                websocket
                    .send(Message::Text(
                        json!({"id": parsed["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .unwrap();
            } else {
                websocket.close(None).unwrap();
            }
        });
        (
            Url::parse(&format!(
                "ws://127.0.0.1:{}/devtools/page/fake",
                address.port()
            ))
            .unwrap(),
            handle,
        )
    }

    fn fake_cdp_event_flood(events: usize, interval: Duration) -> (Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            let request = websocket.read().unwrap();
            assert!(matches!(request, Message::Text(_)));
            for sequence in 0..events {
                thread::sleep(interval);
                if websocket
                    .send(Message::Text(
                        json!({"method": "Runtime.fixtureEvent", "params": {"sequence": sequence}})
                            .to_string()
                            .into(),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
        (
            Url::parse(&format!(
                "ws://127.0.0.1:{}/devtools/page/flood",
                address.port()
            ))
            .unwrap(),
            handle,
        )
    }

    fn fake_http_body(body_len: usize) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let header = format!(
                "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {body_len}
Connection: close

"
            );
            use std::io::Write;
            stream.write_all(header.as_bytes()).unwrap();
            let chunk = vec![b' '; body_len.min(8192)];
            let mut remaining = body_len;
            while remaining > 0 {
                let count = remaining.min(chunk.len());
                if stream.write_all(&chunk[..count]).is_err() {
                    break;
                }
                remaining -= count;
            }
        });
        (address.port(), handle)
    }

    #[test]
    fn devtools_active_port_is_parsed_only_as_loopback_browser_path_input() {
        assert_eq!(
            parse_devtools_active_port("9222\n/devtools/browser/opaque\n"),
            Some((9222, "/devtools/browser/opaque".to_string()))
        );
        assert!(parse_devtools_active_port("0\n/devtools/browser/x\n").is_none());
        assert!(
            parse_devtools_active_port("9222\nws://evil.example/devtools/browser/x\n").is_none()
        );
        assert!(parse_devtools_active_port("9222\n/devtools/page/x\n").is_none());
    }

    #[test]
    fn effect_composition_preserves_pre_dispatch_and_partial_effect_certainty() {
        let pre = pre_dispatch_error(BrowserError::observed("probe_failed", "probe", None));
        assert_eq!(
            pre.execution_state,
            crate::types::ExecutionState::NotStarted
        );
        assert_eq!(pre.recovery_action, None);

        let partial = post_effect_error(
            BrowserError::not_started("second_stage_not_started", "second stage"),
            "snapshot",
        );
        assert_eq!(
            partial.execution_state,
            crate::types::ExecutionState::OutcomeUnknown
        );
        assert_eq!(partial.recovery_action, Some("snapshot"));
    }

    #[test]
    fn ax_form_metadata_extracts_group_and_control_state() {
        let raw_nodes = vec![
            json!({
                "nodeId":"group-1",
                "role":{"value":"group"},
                "name":{"value":"是否接受岗位调剂"},
                "parentId":"form-1"
            }),
            json!({
                "nodeId":"wrapper-1",
                "role":{"value":"none"},
                "parentId":"group-1"
            }),
            json!({
                "nodeId":"radio-1",
                "role":{"value":"radio"},
                "name":{"value":"否"},
                "parentId":"wrapper-1",
                "properties":[
                    {"name":"checked","value":{"value":"false"}},
                    {"name":"required","value":{"value":true}},
                    {"name":"disabled","value":{"value":false}}
                ]
            }),
        ];
        let by_id = raw_nodes
            .iter()
            .filter_map(|node| {
                node.get("nodeId")
                    .and_then(Value::as_str)
                    .map(|id| (id.to_string(), node))
            })
            .collect::<HashMap<_, _>>();

        let group = ax_group_context(&raw_nodes[2], &by_id).unwrap();
        assert_eq!(group.0, "group-1");
        assert_eq!(group.1, "group");
        assert_eq!(group.2, "是否接受岗位调剂");
        assert_eq!(
            ax_property_string(&raw_nodes[2], "checked").as_deref(),
            Some("false")
        );
        assert_eq!(ax_property_bool(&raw_nodes[2], "required"), Some(true));
        assert_eq!(ax_property_bool(&raw_nodes[2], "disabled"), Some(false));
        assert_eq!(ax_property_bool(&raw_nodes[2], "readonly"), None);
    }

    #[test]
    fn ax_snapshot_parser_keeps_late_actionable_nodes_for_compaction() {
        let mut raw_nodes = (0..=MAX_SNAPSHOT_NODES)
            .map(|index| {
                json!({
                    "nodeId": format!("static-{index}"),
                    "role": {"value": "paragraph"},
                    "name": {"value": format!("Static {index}")},
                })
            })
            .collect::<Vec<_>>();
        raw_nodes.push(json!({
            "nodeId": "late-action",
            "role": {"value": "button"},
            "name": {"value": "Continue"},
            "backendDOMNodeId": 4242,
        }));

        let (nodes, truncated) = parse_ax_snapshot_nodes(&raw_nodes);
        assert!(truncated);
        let late = nodes.last().expect("late actionable node retained");
        assert_eq!(late.name.as_deref(), Some("Continue"));
        assert!(late.actionable);
        assert_eq!(late.backend_node_id, Some(4242));
    }

    #[test]
    fn remote_object_sequence_reuses_one_page_websocket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();

            let first = websocket.read().unwrap();
            let Message::Text(first) = first else {
                panic!("expected first CDP text request")
            };
            let first: Value = serde_json::from_str(&first).unwrap();
            assert_eq!(first["method"], "DOM.resolveNode");
            websocket
                .send(Message::Text(
                    json!({
                        "id": first["id"],
                        "result": {"object": {"objectId": "remote-object-1"}}
                    })
                    .to_string()
                    .into(),
                ))
                .unwrap();

            let second = websocket.read().unwrap();
            let Message::Text(second) = second else {
                panic!("expected second CDP text request")
            };
            let second: Value = serde_json::from_str(&second).unwrap();
            assert_eq!(second["method"], "Runtime.callFunctionOn");
            assert_eq!(second["params"]["objectId"], "remote-object-1");
            websocket
                .send(Message::Text(
                    json!({
                        "id": second["id"],
                        "result": {"result": {"value": {"ok": true}}}
                    })
                    .to_string()
                    .into(),
                ))
                .unwrap();
        });

        let endpoint = Url::parse(&format!(
            "ws://127.0.0.1:{}/devtools/page/sequence",
            address.port()
        ))
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut websocket = open_loopback_websocket(&endpoint, deadline).unwrap();
        let mut next_id = 1;
        let resolved = cdp_call_on_websocket_until(
            &mut websocket,
            &mut next_id,
            "DOM.resolveNode",
            json!({"backendNodeId": 42}),
            false,
            deadline,
        )
        .unwrap();
        let object_id = resolved["object"]["objectId"].as_str().unwrap();
        let result = cdp_call_on_websocket_until(
            &mut websocket,
            &mut next_id,
            "Runtime.callFunctionOn",
            json!({"objectId": object_id, "functionDeclaration": "function(){return {ok:true};}"}),
            true,
            deadline,
        )
        .unwrap();
        assert_eq!(result["result"]["value"]["ok"], true);
        assert_eq!(next_id, 3);
        handle.join().unwrap();
    }

    #[test]
    fn irrelevant_cdp_events_do_not_rearm_the_absolute_deadline() {
        let (endpoint, handle) = fake_cdp_event_flood(40, Duration::from_millis(20));
        let mut next_id = 1;
        let started = Instant::now();
        let error = cdp_call_until(
            &endpoint,
            &mut next_id,
            "Page.getFrameTree",
            json!({}),
            false,
            started + Duration::from_millis(120),
        )
        .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_millis(600),
            "non-matching CDP events must not renew the request deadline"
        );
        assert!(matches!(
            error.kind,
            "cdp_receive_timeout" | "cdp_receive_failed"
        ));
        handle.join().unwrap();
    }

    #[test]
    fn oversized_cdp_message_fails_before_unbounded_materialization() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            let _ = websocket.read().unwrap();
            let oversized = "x".repeat(MAX_CDP_MESSAGE_BYTES + 1);
            let _ = websocket.send(Message::Text(oversized.into()));
        });
        let endpoint = Url::parse(&format!(
            "ws://127.0.0.1:{}/devtools/page/oversized",
            address.port()
        ))
        .unwrap();
        let mut next_id = 1;
        let error = cdp_call_until(
            &endpoint,
            &mut next_id,
            "Page.getFrameTree",
            json!({}),
            false,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(
            error.execution_state,
            crate::types::ExecutionState::Completed
        );
        assert_eq!(error.kind, "cdp_receive_failed");
        handle.join().unwrap();
    }

    #[test]
    fn oversized_cdp_page_list_is_rejected_at_the_transport_bound() {
        let (port, handle) = fake_http_body(MAX_CDP_LIST_BYTES + 1);
        let error =
            fetch_page_descriptors(port, Instant::now() + Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.kind, "cdp_list_too_large");
        assert_eq!(
            error.execution_state,
            crate::types::ExecutionState::Completed
        );
        handle.join().unwrap();
    }

    #[test]
    fn fake_cdp_observation_round_trip_is_deterministic() {
        let (endpoint, handle) =
            fake_cdp_server(Some(json!({"frameTree": {"frame": {"loaderId": "doc"}}})));
        let mut next_id = 1;
        let result = cdp_call(
            &endpoint,
            &mut next_id,
            "Page.getFrameTree",
            json!({}),
            false,
        )
        .expect("fake CDP observation");
        assert_eq!(result["frameTree"]["frame"]["loaderId"], "doc");
        assert_eq!(next_id, 2);
        handle.join().unwrap();
    }

    #[test]
    fn closed_diagnostic_stream_is_not_reported_as_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            websocket.close(None).unwrap();
        });
        let endpoint = Url::parse(&format!(
            "ws://127.0.0.1:{}/devtools/page/closed-diagnostics",
            address.port()
        ))
        .unwrap();
        let websocket =
            open_loopback_websocket(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
        let mut collector = CdpEventCollector {
            websocket,
            events: CdpEventBuffer::default(),
        };
        collector.events.push_console(BackendConsoleEntry {
            sequence: 0,
            level: "error".into(),
            text: "stale-buffer".into(),
            source: None,
            timestamp: None,
        });
        let error = drain_event_collector(&mut collector).unwrap_err();
        assert_eq!(error.kind, "cdp_receive_closed");
        assert_eq!(
            error.execution_state,
            crate::types::ExecutionState::Completed
        );
        assert_eq!(error.recovery_action, Some("pages"));
        assert_eq!(collector.events.console.len(), 1);
        handle.join().unwrap();
    }

    #[test]
    fn diagnostic_event_metadata_is_byte_bounded() {
        let mut buffer = CdpEventBuffer::default();
        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Runtime.exceptionThrown",
                "params": {
                    "exceptionDetails": {
                        "text": "boom",
                        "url": format!("https://example.test/{}", "x".repeat(crate::types::MAX_URL_BYTES * 2)),
                    }
                }
            }),
        );
        let console = buffer.console.back().unwrap();
        assert!(console.source.as_ref().unwrap().len() <= crate::types::MAX_URL_BYTES);

        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Network.requestWillBeSent",
                "params": {
                    "requestId": "bounded-metadata",
                    "request": {
                        "method": "M".repeat(512),
                        "url": "https://example.test/",
                    },
                    "type": "T".repeat(512),
                }
            }),
        );
        let network = buffer.network.get("bounded-metadata").unwrap();
        assert!(network.method.len() <= 128);
        assert!(network.resource_type.as_ref().unwrap().len() <= 64);
    }

    #[test]
    fn clear_diagnostics_uses_cdp_barrier_before_resetting_buffer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            let request = websocket.read().unwrap();
            let Message::Text(request) = request else {
                panic!("expected clear barrier request");
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"], "Page.getFrameTree");
            websocket
                .send(Message::Text(
                    json!({
                        "method": "Runtime.consoleAPICalled",
                        "params": {"type": "error", "args": [{"value": "before-clear"}]}
                    })
                    .to_string()
                    .into(),
                ))
                .unwrap();
            websocket
                .send(Message::Text(
                    json!({"id": request["id"], "result": {"frameTree": {}}})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            websocket
                .send(Message::Text(
                    json!({
                        "method": "Runtime.consoleAPICalled",
                        "params": {"type": "error", "args": [{"value": "after-clear"}]}
                    })
                    .to_string()
                    .into(),
                ))
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        });
        let endpoint = Url::parse(&format!(
            "ws://127.0.0.1:{}/devtools/page/clear",
            address.port()
        ))
        .unwrap();
        let websocket =
            open_loopback_websocket(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
        let mut collector = CdpEventCollector {
            websocket,
            events: CdpEventBuffer::default(),
        };
        collector.events.push_console(BackendConsoleEntry {
            sequence: 0,
            level: "error".into(),
            text: "buffered-before-clear".into(),
            source: None,
            timestamp: None,
        });
        let mut next_id = 1;
        clear_event_collector(&mut collector, &mut next_id).unwrap();
        assert!(collector.events.console.is_empty());
        drain_event_collector(&mut collector).unwrap();
        assert_eq!(collector.events.console.len(), 1);
        assert_eq!(
            collector.events.console.front().unwrap().text,
            "after-clear"
        );
        handle.join().unwrap();
    }

    #[test]
    fn clear_diagnostics_barrier_failure_is_not_started_and_preserves_buffer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = accept(stream).unwrap();
            let _ = websocket.read().unwrap();
            // Close without acknowledging the read-only barrier. The diagnostic
            // reset has not happened, so the caller must see a pre-effect failure.
        });
        let endpoint = Url::parse(&format!(
            "ws://127.0.0.1:{}/devtools/page/clear-failure",
            address.port()
        ))
        .unwrap();
        let websocket =
            open_loopback_websocket(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
        let mut collector = CdpEventCollector {
            websocket,
            events: CdpEventBuffer::default(),
        };
        collector.events.push_console(BackendConsoleEntry {
            sequence: 0,
            level: "error".into(),
            text: "must-survive-failed-clear".into(),
            source: None,
            timestamp: None,
        });
        let mut next_id = 1;
        let error = clear_event_collector(&mut collector, &mut next_id).unwrap_err();
        assert_eq!(
            error.execution_state,
            crate::types::ExecutionState::NotStarted
        );
        assert_eq!(collector.events.console.len(), 1);
        assert_eq!(
            collector.events.console.front().unwrap().text,
            "must-survive-failed-clear"
        );
        handle.join().unwrap();
    }

    #[test]
    fn diagnostic_event_buffers_roll_forward_instead_of_freezing_at_capacity() {
        let mut buffer = CdpEventBuffer::default();
        for index in 0..crate::types::MAX_CONSOLE_ENTRIES + 2 {
            record_cdp_event(
                &mut buffer,
                &json!({
                    "method": "Runtime.consoleAPICalled",
                    "params": {
                        "type": if index + 1 == crate::types::MAX_CONSOLE_ENTRIES + 2 { "error" } else { "log" },
                        "args": [{"value": format!("console-{index}")}],
                        "timestamp": index as f64,
                    }
                }),
            );
        }
        assert_eq!(buffer.console.len(), crate::types::MAX_CONSOLE_ENTRIES);
        assert!(buffer.console_truncated);
        assert_eq!(buffer.console.front().unwrap().text, "console-2");
        assert_eq!(
            buffer.console.back().unwrap().level,
            "error",
            "a late error must not be starved by earlier benign console traffic"
        );

        for index in 0..crate::types::MAX_NETWORK_ENTRIES + 2 {
            record_cdp_event(
                &mut buffer,
                &json!({
                    "method": "Network.requestWillBeSent",
                    "params": {
                        "requestId": format!("request-{index}"),
                        "request": {"method": "GET", "url": format!("https://example.test/{index}")},
                        "type": "Fetch",
                        "timestamp": index as f64,
                    }
                }),
            );
        }
        assert_eq!(buffer.network.len(), crate::types::MAX_NETWORK_ENTRIES);
        assert!(buffer.network_truncated);
        assert!(!buffer.network.contains_key("request-0"));
        assert!(!buffer.network.contains_key("request-1"));
        let latest = format!("request-{}", crate::types::MAX_NETWORK_ENTRIES + 1);
        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Network.loadingFailed",
                "params": {"requestId": latest, "errorText": "late failure"}
            }),
        );
        assert_eq!(
            buffer
                .network
                .get(&format!(
                    "request-{}",
                    crate::types::MAX_NETWORK_ENTRIES + 1
                ))
                .and_then(|entry| entry.failed_reason.as_deref()),
            Some("late failure"),
            "a late failed request must remain diagnosable after the buffer fills"
        );
    }

    #[test]
    fn console_activity_does_not_reset_network_quiet_cursor() {
        let mut buffer = CdpEventBuffer::default();
        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Network.requestWillBeSent",
                "params": {
                    "requestId": "request-1",
                    "request": {"method": "GET", "url": "https://example.test/api"},
                    "type": "Fetch",
                    "timestamp": 1.0,
                }
            }),
        );
        let network_cursor = buffer.network_cursor();
        let event_cursor = buffer.cursor();

        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Runtime.consoleAPICalled",
                "params": {
                    "type": "log",
                    "args": [{"value": "chatty console"}],
                    "timestamp": 2.0,
                }
            }),
        );
        assert!(buffer.cursor() > event_cursor);
        assert_eq!(buffer.network_cursor(), network_cursor);

        record_cdp_event(
            &mut buffer,
            &json!({
                "method": "Network.responseReceived",
                "params": {
                    "requestId": "request-1",
                    "response": {"status": 200},
                }
            }),
        );
        assert!(buffer.network_cursor() > network_cursor);
    }

    #[test]
    fn fake_cdp_close_after_effect_dispatch_is_outcome_unknown() {
        let (endpoint, handle) = fake_cdp_server(None);
        let mut next_id = 7;
        let error = cdp_call(
            &endpoint,
            &mut next_id,
            "Page.navigate",
            json!({"url": "https://example.test/"}),
            true,
        )
        .unwrap_err();
        assert_eq!(
            error.execution_state,
            crate::types::ExecutionState::OutcomeUnknown
        );
        assert_eq!(error.recovery_action, Some("snapshot"));
        handle.join().unwrap();
    }
}
