//! Authenticated Runner-local Native Messaging bridge. Only explicitly offered
//! tabs are attachable. A peer/lease/channel fence survives neither reconnect nor
//! Runner restart; no effect is replayed and no external process is ever owned.
pub(crate) mod native;
mod protocol;
#[cfg(test)]
mod tests;

use crate::{
    profiles, BrowserError, BrowserResult, BrowserWindowBounds, BrowserWindowHint, REQUEST_TIMEOUT,
};
use fs2::FileExt;
use protocol::{FrameReader, EXTENSION_ID, MAX_COMMAND, MAX_RESPONSE, VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex, Weak,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use uuid::Uuid;

const MAX_PEERS: usize = 4;
const MAX_OFFERS: usize = 16;
const MAX_ROUTES: usize = 64;
const MAX_QUEUED_BYTES: usize = 16 * 1024 * 1024;
const OFFER_TTL: Duration = Duration::from_secs(600);
static RUNNING: AtomicUsize = AtomicUsize::new(0);
pub(crate) fn available() -> bool {
    RUNNING.load(Ordering::Acquire) > 0
}
fn unavailable() -> BrowserError {
    BrowserError::not_started(
        "browser_bridge_unavailable",
        "No live, authenticated Browser extension bridge is available",
    )
}
fn stale() -> BrowserError {
    BrowserError::not_started(
        "stale_attachment",
        "This tab offer or attachment is no longer valid; share the tab again in the extension",
    )
}
fn capacity() -> BrowserError {
    BrowserError::not_started(
        "browser_bridge_capacity",
        "Browser bridge capacity is exhausted",
    )
}
fn disconnected() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "Browser bridge disconnected")
}
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rendezvous {
    version: u32,
    address: SocketAddr,
    instance: String,
    token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    version: u32,
    instance: String,
    token: String,
    extension_id: String,
    parent_pid: u32,
}
impl Rendezvous {
    fn accepts(&self, hello: &Hello) -> bool {
        hello.version == VERSION
            && hello.instance == self.instance
            && hello.extension_id == EXTENSION_ID
            && hello.parent_pid > 0
            && secret_equal(&self.token, &hello.token)
    }
}
fn secret_equal(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentSummary {
    pub attachment_id: String,
    pub title: String,
    pub url: String,
}
struct Offer {
    peer: String,
    tab: i64,
    _window: i64,
    title: String,
    url: String,
    expires: Instant,
    lease: Option<String>,
}
struct Peer {
    outgoing: SyncSender<Value>,
    parent_pid: u32,
}
struct Queued {
    text: String,
    bytes: Arc<AtomicUsize>,
    size: usize,
}
impl Drop for Queued {
    fn drop(&mut self) {
        self.bytes.fetch_sub(self.size, Ordering::AcqRel);
    }
}
struct Route {
    peer: String,
    lease: String,
    target: Option<String>,
    pending: Option<u64>,
    tx: SyncSender<Queued>,
}
#[derive(Default)]
struct Registry {
    peers: HashMap<String, Peer>,
    offers: HashMap<String, Offer>,
    routes: HashMap<u64, Route>,
}
struct State {
    registry: Mutex<Registry>,
    stop: AtomicBool,
    next_channel: AtomicU64,
    queued: Arc<AtomicUsize>,
}
impl State {
    fn drop_peer(&self, peer: &str) {
        let mut registry = lock(&self.registry);
        registry.peers.remove(peer);
        registry.offers.retain(|_, offer| offer.peer != peer);
        registry.routes.retain(|_, route| route.peer != peer);
    }
    fn invalidate_lease(&self, lease: &str) {
        let mut registry = lock(&self.registry);
        registry
            .offers
            .retain(|_, offer| offer.lease.as_deref() != Some(lease));
        registry.routes.retain(|_, route| route.lease != lease);
    }
    fn queue(&self, route: &Route, value: &Value) -> bool {
        let Ok(text) = serde_json::to_string(value) else {
            return false;
        };
        let size = text.len();
        if size > MAX_RESPONSE
            || self
                .queued
                .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current
                        .checked_add(size)
                        .filter(|next| *next <= MAX_QUEUED_BYTES)
                })
                .is_err()
        {
            return false;
        }
        route
            .tx
            .try_send(Queued {
                text,
                size,
                bytes: self.queued.clone(),
            })
            .is_ok()
    }
    fn receive(&self, peer: &str, value: Value) -> bool {
        let mut registry = lock(&self.registry);
        match value.get("kind").and_then(Value::as_str) {
            Some("offer") => {
                let Some(tab) = value["tab"].as_i64().filter(|id| *id > 0) else {
                    return false;
                };
                let Some(window) = value["window"].as_i64().filter(|id| *id > 0) else {
                    return false;
                };
                let Some(url) = value["url"].as_str().filter(|url| url.len() <= 8192) else {
                    return false;
                };
                if crate::types::validate_navigation_url(url).is_err() {
                    return false;
                }
                let title =
                    crate::types::clip_chars(value["title"].as_str().unwrap_or_default(), 256);
                // A repeated offer cannot silently replace a live lease.
                if registry
                    .offers
                    .values()
                    .any(|offer| offer.peer == peer && offer.tab == tab)
                {
                    return true;
                }
                registry
                    .offers
                    .retain(|_, offer| offer.lease.is_some() || offer.expires > Instant::now());
                if registry.offers.len() >= MAX_OFFERS {
                    return false;
                }
                registry.offers.insert(
                    format!("attachment_{}", Uuid::new_v4().simple()),
                    Offer {
                        peer: peer.into(),
                        tab,
                        _window: window,
                        title,
                        url: crate::types::clip_bytes(url, 2048),
                        expires: Instant::now() + OFFER_TTL,
                        lease: None,
                    },
                );
                true
            }
            Some("revoke") => {
                let Some(tab) = value["tab"].as_i64() else {
                    return false;
                };
                let leases: HashSet<_> = registry
                    .offers
                    .values()
                    .filter(|offer| offer.peer == peer && offer.tab == tab)
                    .filter_map(|offer| offer.lease.clone())
                    .collect();
                registry
                    .offers
                    .retain(|_, offer| !(offer.peer == peer && offer.tab == tab));
                registry
                    .routes
                    .retain(|_, route| !leases.contains(&route.lease));
                true
            }
            Some("message") => {
                let Some(channel) = value["channel"].as_u64() else {
                    return false;
                };
                let Some(route) = registry.routes.get_mut(&channel) else {
                    return true;
                };
                if route.peer != peer || value["lease"].as_str() != Some(&route.lease) {
                    return false;
                }
                let message = &value["message"];
                let Some(id) = message["id"].as_u64() else {
                    return false;
                };
                if route.pending != Some(id) {
                    return true;
                }
                route.pending = None;
                self.queue(route, message)
            }
            Some("event") => {
                if value.to_string().len() > 64 * 1024 {
                    return false;
                }
                let Some(lease) = value["lease"].as_str() else {
                    return false;
                };
                let Some(target) = value["target"].as_str() else {
                    return false;
                };
                let event = &value["message"];
                if event["method"].as_str().is_none() || event.get("id").is_some() {
                    return false;
                }
                // Diagnostic backpressure invalidates only the affected route.
                // Losing an effect response remains outcome_unknown; no command
                // is replayed. A collector can reopen under the exact live lease.
                registry.routes.retain(|_, route| {
                    route.peer != peer
                        || route.lease != lease
                        || route.target.as_deref() != Some(target)
                        || self.queue(route, event)
                });
                true
            }
            Some("pong") => true,
            _ => false,
        }
    }
}

/// Owns the listener and all connection workers. Drop joins bounded workers and
/// closes native hosts, whose extension disconnect handler detaches debuggees.
pub(crate) struct BridgeServer {
    state: Arc<State>,
    worker: Option<JoinHandle<()>>,
    _lease: File,
    record: PathBuf,
}
impl BridgeServer {
    pub(crate) fn start() -> BrowserResult<Self> {
        let root = profiles::state_root()?;
        profiles::private_dir(&root)?;
        Self::start_at(&root.join("extension"))
    }
    fn start_at(root: &Path) -> BrowserResult<Self> {
        profiles::private_dir(root)?;
        let lease = profiles::private_file(&root.join("owner.lock"), true)?;
        lease.try_lock_exclusive().map_err(|_| unavailable())?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| unavailable())?;
        listener.set_nonblocking(true).map_err(|_| unavailable())?;
        let record = root.join("bridge.json");
        let config = Rendezvous {
            version: VERSION,
            address: listener.local_addr().map_err(|_| unavailable())?,
            instance: Uuid::new_v4().simple().to_string(),
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        };
        let mut file = profiles::private_file(&record, true)?;
        file.set_len(0)
            .and_then(|_| {
                file.write_all(&serde_json::to_vec(&config).map_err(|_| protocol::invalid())?)
            })
            .and_then(|_| file.sync_all())
            .map_err(|_| unavailable())?;
        let state = Arc::new(State {
            registry: Mutex::new(Registry::default()),
            stop: AtomicBool::new(false),
            next_channel: AtomicU64::new(1),
            queued: Arc::new(AtomicUsize::new(0)),
        });
        let weak = Arc::downgrade(&state);
        let worker = std::thread::Builder::new()
            .name("browser-bridge".into())
            .spawn(move || serve(listener, config, weak))
            .map_err(|_| unavailable())?;
        RUNNING.fetch_add(1, Ordering::AcqRel);
        Ok(Self {
            state,
            worker: Some(worker),
            _lease: lease,
            record,
        })
    }
    pub(crate) fn discover(&self) -> Vec<AttachmentSummary> {
        let mut registry = lock(&self.state.registry);
        registry
            .offers
            .retain(|_, offer| offer.lease.is_some() || offer.expires > Instant::now());
        registry
            .offers
            .iter()
            .filter(|(_, offer)| offer.lease.is_none())
            .take(MAX_OFFERS)
            .map(|(id, offer)| AttachmentSummary {
                attachment_id: id.clone(),
                title: offer.title.clone(),
                url: offer.url.clone(),
            })
            .collect()
    }
    pub(crate) fn attach(&self, id: &str, deadline: Instant) -> BrowserResult<ExternalLease> {
        let (peer, tab, lease) = {
            let mut registry = lock(&self.state.registry);
            let offer = registry.offers.get_mut(id).ok_or_else(stale)?;
            if offer.expires <= Instant::now() || offer.lease.is_some() {
                return Err(stale());
            }
            let lease = Uuid::new_v4().simple().to_string();
            offer.lease = Some(lease.clone());
            (offer.peer.clone(), offer.tab, lease)
        };
        let lease = ExternalLease(Arc::new(LeaseInner {
            state: self.state.clone(),
            peer,
            lease,
            tab,
            targets: Mutex::new(HashSet::from([format!("tab_{tab}")])),
            detached: AtomicBool::new(false),
        }));
        lease.control("attach", json!({}), true, deadline)?;
        Ok(lease)
    }
}
impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        lock(&self.state.registry).routes.clear();
        let _ = std::fs::remove_file(&self.record);
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}
fn serve(listener: TcpListener, config: Rendezvous, weak: Weak<State>) {
    let config = Arc::new(config);
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    loop {
        let Some(state) = weak.upgrade() else {
            break;
        };
        if state.stop.load(Ordering::Acquire) {
            break;
        }
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let _ = workers.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
        match listener.accept() {
            Ok((stream, address)) if address.ip().is_loopback() && workers.len() < MAX_PEERS => {
                let config = config.clone();
                if let Ok(worker) = std::thread::Builder::new()
                    .name("browser-native-peer".into())
                    .spawn(move || serve_peer(stream, &config, &state))
                {
                    workers.push(worker);
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(_) => break,
        }
    }
    if let Some(state) = weak.upgrade() {
        state.stop.store(true, Ordering::Release);
    }
    for worker in workers {
        let _ = worker.join();
    }
}
fn serve_peer(mut stream: TcpStream, config: &Rendezvous, state: &State) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_nodelay(true);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut frames = FrameReader::new(4096);
    let hello = loop {
        if state.stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return;
        }
        match frames.poll(&mut stream) {
            Ok(Some(value)) => break serde_json::from_value::<Hello>(value).ok(),
            Ok(None) => {}
            Err(_) => return,
        }
    };
    let Some(hello) = hello.filter(|hello| config.accepts(hello)) else {
        return;
    };
    let peer = Uuid::new_v4().simple().to_string();
    let (tx, rx) = mpsc::sync_channel(32);
    lock(&state.registry).peers.insert(
        peer.clone(),
        Peer {
            outgoing: tx,
            parent_pid: hello.parent_pid,
        },
    );
    let _ = protocol::write_frame(
        &mut stream,
        &json!({"kind":"ready","version":VERSION}),
        MAX_COMMAND,
        false,
    );
    let mut frames = FrameReader::new(MAX_RESPONSE);
    while !state.stop.load(Ordering::Acquire) && lock(&state.registry).peers.contains_key(&peer) {
        let mut failed = false;
        for value in rx.try_iter().take(32) {
            if protocol::write_frame(&mut stream, &value, MAX_COMMAND, false).is_err() {
                failed = true;
                break;
            }
        }
        if failed {
            break;
        }
        match frames.poll(&mut stream) {
            Ok(Some(value)) => {
                if !state.receive(&peer, value) {
                    break;
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
    }
    state.drop_peer(&peer);
}

struct LeaseInner {
    state: Arc<State>,
    peer: String,
    lease: String,
    tab: i64,
    targets: Mutex<HashSet<String>>,
    detached: AtomicBool,
}
#[derive(Clone)]
pub(crate) struct ExternalLease(Arc<LeaseInner>);
impl ExternalLease {
    fn live(&self) -> BrowserResult<()> {
        let registry = lock(&self.0.state.registry);
        if self.0.detached.load(Ordering::Acquire)
            || self.0.state.stop.load(Ordering::Acquire)
            || !registry.peers.contains_key(&self.0.peer)
            || !registry.offers.values().any(|offer| {
                offer.peer == self.0.peer && offer.lease.as_deref() == Some(&self.0.lease)
            })
        {
            return Err(stale());
        }
        Ok(())
    }
    pub(crate) fn live_process_id(&self) -> BrowserResult<u32> {
        self.live()?;
        lock(&self.0.state.registry)
            .peers
            .get(&self.0.peer)
            .map(|peer| peer.parent_pid)
            .ok_or_else(stale)
    }
    pub(crate) fn live_window_hint(&self) -> BrowserResult<BrowserWindowHint> {
        let process_id = self.live_process_id()?;
        let value = self.control(
            "window_bounds",
            json!({}),
            false,
            Instant::now() + REQUEST_TIMEOUT,
        )?;
        let integer = |name: &str| -> BrowserResult<i32> {
            value[name]
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(stale)
        };
        let dimension = |name: &str| -> BrowserResult<u32> {
            value[name]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(stale)
        };
        Ok(BrowserWindowHint {
            process_id,
            bounds: Some(BrowserWindowBounds {
                x: integer("x")?,
                y: integer("y")?,
                width: dimension("width")?,
                height: dimension("height")?,
            }),
        })
    }
    pub(crate) fn socket(&self, target: Option<&str>) -> BrowserResult<BridgeSocket> {
        self.live()?;
        if target.is_some_and(|target| !lock(&self.0.targets).contains(target)) {
            return Err(stale());
        }
        let mut registry = lock(&self.0.state.registry);
        if registry.routes.len() >= MAX_ROUTES {
            return Err(capacity());
        }
        let id = self
            .0
            .state
            .next_channel
            .try_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| capacity())?;
        let (tx, rx) = mpsc::sync_channel(128);
        registry.routes.insert(
            id,
            Route {
                peer: self.0.peer.clone(),
                lease: self.0.lease.clone(),
                target: target.map(str::to_owned),
                pending: None,
                tx,
            },
        );
        Ok(BridgeSocket {
            lease: self.clone(),
            id,
            rx,
            timeout: Duration::from_secs(10),
            target: target.map(str::to_owned),
        })
    }
    fn control(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        deadline: Instant,
    ) -> BrowserResult<Value> {
        let mut socket = self.socket(None)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(BrowserError::not_started(
                "browser_bridge_timeout",
                "Bridge operation expired before dispatch",
            ));
        }
        socket.timeout = remaining;
        socket
            .send(json!({"id":1,"method":method,"params":params}))
            .map_err(|_| {
                if effect {
                    BrowserError::uncertain(
                        "browser_bridge_delivery_uncertain",
                        "Bridge command delivery was not confirmed",
                        "browsers",
                    )
                } else {
                    unavailable()
                }
            })?;
        let text = socket.read().map_err(|_| {
            if effect {
                BrowserError::uncertain(
                    "browser_bridge_outcome_unknown",
                    "Bridge command outcome was not confirmed; do not replay it",
                    "browsers",
                )
            } else {
                unavailable()
            }
        })?;
        let value: Value = serde_json::from_str(&text).map_err(|_| unavailable())?;
        if value.get("error").is_some() {
            return Err(if effect {
                BrowserError::uncertain(
                    "browser_bridge_operation_failed",
                    "The external Browser operation did not complete cleanly",
                    "browsers",
                )
            } else {
                unavailable()
            });
        }
        Ok(value.get("result").cloned().unwrap_or_else(|| json!({})))
    }
    pub(crate) fn targets(&self, deadline: Instant) -> BrowserResult<Vec<Value>> {
        let result = self.control("targets", json!({}), false, deadline)?;
        if result.to_string().len() > 256 * 1024 {
            return Err(capacity());
        }
        let values = result
            .as_array()
            .filter(|values| values.len() <= crate::MAX_PAGES_PER_BROWSER)
            .ok_or_else(stale)?;
        let mut targets = HashSet::new();
        for value in values {
            let target = value["id"]
                .as_str()
                .filter(|id| valid_target(id))
                .ok_or_else(stale)?;
            if !targets.insert(target.to_string()) {
                return Err(stale());
            }
        }
        *lock(&self.0.targets) = targets;
        Ok(values.clone())
    }
    pub(crate) fn browser_call(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        deadline: Instant,
    ) -> BrowserResult<Value> {
        let operation = match method {
            "Target.createTarget" => "new_page",
            "Target.closeTarget" => "close_page",
            _ => {
                return Err(BrowserError::not_started(
                    "external_browser_ownership",
                    "An attached Browser does not grant process ownership",
                ))
            }
        };
        let result = self.control(operation, params, effect, deadline)?;
        if operation == "new_page" {
            let id = result["targetId"]
                .as_str()
                .filter(|id| valid_target(id))
                .ok_or_else(stale)?;
            lock(&self.0.targets).insert(id.into());
        }
        Ok(result)
    }
    pub(crate) fn detach(&self, timeout: Duration) -> BrowserResult<()> {
        if self.0.detached.load(Ordering::Acquire) {
            return Ok(());
        }
        let result = self.control("detach", json!({}), true, Instant::now() + timeout);
        if result.is_err() {
            // Closing this native peer forces extension-side detach even if the
            // acknowledgement was lost. Other leases become stale, never replayed.
            self.0.state.drop_peer(&self.0.peer);
        }
        self.0.detached.store(true, Ordering::Release);
        self.0.state.invalidate_lease(&self.0.lease);
        result.map(|_| ())
    }
}
fn valid_target(id: &str) -> bool {
    id.len() <= 32
        && id
            .strip_prefix("tab_")
            .and_then(|id| id.parse::<i64>().ok())
            .is_some_and(|id| id > 0)
}
impl Drop for LeaseInner {
    fn drop(&mut self) {
        if !self.detached.swap(true, Ordering::AcqRel) {
            let registry = lock(&self.state.registry);
            if let Some(peer) = registry.peers.get(&self.peer) {
                if peer
                    .outgoing
                    .try_send(
                        json!({"kind":"control","channel":0,"lease":self.lease,"tab":self.tab,
                    "request":{"id":0,"method":"detach","params":{}}}),
                    )
                    .is_err()
                {
                    drop(registry);
                    self.state.drop_peer(&self.peer);
                    return;
                }
            }
            drop(registry);
            self.state.invalidate_lease(&self.lease);
        }
    }
}

pub(crate) struct BridgeSocket {
    lease: ExternalLease,
    id: u64,
    rx: Receiver<Queued>,
    pub(crate) timeout: Duration,
    target: Option<String>,
}
impl BridgeSocket {
    pub(crate) fn send(&mut self, request: Value) -> io::Result<()> {
        self.lease.live().map_err(|_| disconnected())?;
        let method = request["method"].as_str().ok_or_else(protocol::invalid)?;
        if request.to_string().len() > MAX_COMMAND - 1024 || method == "Browser.close" {
            return Err(protocol::invalid());
        }
        let id = request["id"].as_u64().ok_or_else(protocol::invalid)?;
        let mut registry = lock(&self.lease.0.state.registry);
        let route = registry.routes.get_mut(&self.id).ok_or_else(disconnected)?;
        if route.pending.is_some() {
            return Err(protocol::invalid());
        }
        route.pending = Some(id);
        let value = json!({"kind":if self.target.is_some() { "cdp" } else { "control" },
            "channel":self.id,"lease":self.lease.0.lease,"tab":self.lease.0.tab,"target":self.target,"request":request});
        registry
            .peers
            .get(&self.lease.0.peer)
            .ok_or_else(disconnected)?
            .outgoing
            .try_send(value)
            .map_err(|_| disconnected())
    }
    pub(crate) fn read(&mut self) -> io::Result<String> {
        self.lease.live().map_err(|_| disconnected())?;
        match self.rx.recv_timeout(self.timeout) {
            Ok(mut value) => Ok(std::mem::take(&mut value.text)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::ErrorKind::TimedOut.into()),
            Err(_) => Err(disconnected()),
        }
    }
}
impl Drop for BridgeSocket {
    fn drop(&mut self) {
        lock(&self.lease.0.state.registry).routes.remove(&self.id);
    }
}
