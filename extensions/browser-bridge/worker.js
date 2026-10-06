// No content scripts, external messaging, remote endpoint or browser-page token.
// Consent is an explicit extension action; the native host binds authentication
// and exact Chrome process provenance to one live Runner instance.
const HOST = 'com.webcodex.browser_bridge';
const MAX_TABS = 32;
const MAX_LEASES = 4;
const MAX_COMMAND_BYTES = 256 * 1024;
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
const MAX_EVENT_BYTES = 64 * 1024;
const allowedDomains = new Set(['Accessibility', 'DOM', 'DOMSnapshot', 'Input', 'Log', 'Network', 'Page', 'Runtime']);
let native = null;
let connecting = null;
let generation = 0;
const offers = new Map();
const leases = new Map();
const tabLease = new Map();
const encoder = new TextEncoder();
let admitted = 0;

function size(value) { return encoder.encode(JSON.stringify(value)).length; }
function send(value) {
  if (!native || size(value) > MAX_RESPONSE_BYTES) throw new Error('bridge_closed_or_oversized');
  native.postMessage(value);
}
async function detachLease(id) {
  const lease = leases.get(id);
  if (!lease) return;
  leases.delete(id);
  // Detach only. There is intentionally no Browser.close/window-close/process API.
  for (const tab of lease.tabs) {
    tabLease.delete(tab);
    try { await chrome.debugger.detach({tabId: tab}); } catch { /* Already detached. */ }
  }
}
async function disconnect() {
  const current = native;
  native = null;
  connecting = null;
  generation += 1;
  offers.clear();
  await Promise.all([...leases.keys()].map(detachLease));
  try { current?.disconnect(); } catch { /* Already disconnected. */ }
}
function connect() {
  if (connecting) return connecting;
  connecting = new Promise((resolve, reject) => {
    const port = chrome.runtime.connectNative(HOST);
    native = port;
    const timer = setTimeout(() => { reject(new Error('bridge_not_ready')); void disconnect(); }, 4000);
    port.onDisconnect.addListener(() => {
      void chrome.runtime.lastError;
      clearTimeout(timer);
      reject(new Error('bridge_disconnected'));
      if (native === port) void disconnect();
    });
    port.onMessage.addListener(message => {
      if (native !== port) return;
      if (message?.kind === 'ready' && message.version === 1) { clearTimeout(timer); resolve(port); return; }
      void command(message);
    });
  });
  return connecting;
}

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (sender.id !== chrome.runtime.id || !['share', 'revoke'].includes(message?.action)) return false;
  (async () => {
    const [tab] = await chrome.tabs.query({active: true, currentWindow: true});
    if (!tab?.id) throw new Error('no_active_tab');
    if (message.action === 'revoke') {
      // Invalidate consent before awaiting cleanup, including attach operations
      // that have reserved a lease but have not obtained the debugger yet.
      offers.delete(tab.id);
      const revoked = [...leases.values()].filter(lease => lease.originalTab === tab.id || lease.tabs.has(tab.id));
      await Promise.all(revoked.map(lease => detachLease(lease.id)));
      if (native) send({kind: 'revoke', tab: tab.id});
      return {ok: true, message: 'Tab revoked. Chrome and its login data remain unchanged.'};
    }
    if (!/^https?:\/\//i.test(tab.url ?? '') || offers.size >= 16) throw new Error('tab_not_shareable');
    await connect();
    offers.set(tab.id, {window: tab.windowId, expires: Date.now() + 600000});
    send({kind: 'offer', tab: tab.id, window: tab.windowId, title: (tab.title ?? '').slice(0, 256), url: tab.url});
    return {ok: true, message: 'Tab offered. WebCodex can now discover and attach it.'};
  })().then(respond, () => respond({ok: false}));
  return true;
});

function requireLease(lease) {
  if (!native || lease.generation !== generation || leases.get(lease.id) !== lease) throw new Error('lease_lost');
}
async function attachTab(lease, tab) {
  requireLease(lease);
  if (tabLease.has(tab)) throw new Error('tab_already_attached');
  await chrome.debugger.attach({tabId: tab}, '1.3');
  // The connection may disappear while attach was in flight. Do not retain an
  // unowned debugger after disconnect or reattach on a replacement native port.
  if (!native || lease.generation !== generation || leases.get(lease.id) !== lease) {
    try { await chrome.debugger.detach({tabId: tab}); } catch { /* Detached already. */ }
    throw new Error('lease_lost');
  }
  lease.tabs.add(tab);
  tabLease.set(tab, lease.id);
}
async function control(message, epoch) {
  if (!native || epoch !== generation) throw new Error('connection_lost');
  const request = message.request;
  const method = request.method;
  const id = message.lease;
  if (method === 'attach') {
    const offer = offers.get(message.tab);
    if (!offer || offer.expires < Date.now() || leases.has(id) || leases.size >= MAX_LEASES) throw new Error('consent_missing');
    const tab = await chrome.tabs.get(message.tab);
    if (!native || epoch !== generation || offers.get(message.tab) !== offer || offer.expires < Date.now()
        || leases.has(id) || leases.size >= MAX_LEASES) throw new Error('consent_missing');
    if (tab.windowId !== offer.window || !/^https?:\/\//i.test(tab.url ?? '')) throw new Error('tab_changed');
    const lease = {id, originalTab: tab.id, generation: epoch, tabs: new Set()};
    leases.set(id, lease);
    try { await attachTab(lease, tab.id); } catch (error) { await detachLease(id); throw error; }
    return {};
  }
  const lease = leases.get(id);
  if (!lease) {
    if (method === 'detach') return {};
    throw new Error('stale_lease');
  }
  if (method === 'detach') { await detachLease(id); offers.delete(message.tab); return {}; }
  if (method === 'targets') {
    const tabs = [];
    for (const tabId of lease.tabs) {
      try {
        const tab = await chrome.tabs.get(tabId);
        tabs.push({id: `tab_${tabId}`, type: 'page', title: (tab.title ?? '').slice(0, 256), url: (tab.url ?? '').slice(0, 2048)});
      } catch { lease.tabs.delete(tabId); tabLease.delete(tabId); }
    }
    return tabs;
  }
  if (method === 'window_bounds') {
    const tab = await chrome.tabs.get(lease.originalTab);
    requireLease(lease);
    const browserWindow = await chrome.windows.get(tab.windowId);
    const {left: x, top: y, width, height} = browserWindow;
    if (![x, y, width, height].every(Number.isInteger) || width <= 0 || height <= 0) throw new Error('window_bounds_unavailable');
    return {x, y, width, height};
  }
  if (method === 'new_page') {
    if (lease.tabs.size >= MAX_TABS || request.params?.url !== 'about:blank') throw new Error('page_limit_or_url');
    const original = await chrome.tabs.get(message.tab);
    requireLease(lease);
    const tab = await chrome.tabs.create({windowId: original.windowId, url: 'about:blank', active: false});
    await attachTab(lease, tab.id);
    return {targetId: `tab_${tab.id}`};
  }
  if (method === 'close_page') {
    const tab = parseTarget(request.params?.targetId);
    if (!lease.tabs.has(tab)) throw new Error('stale_target');
    await chrome.tabs.remove(tab);
    return {success: true};
  }
  throw new Error('unsupported_control');
}
function parseTarget(target) {
  if (typeof target !== 'string' || !/^tab_[1-9][0-9]{0,15}$/.test(target)) throw new Error('invalid_target');
  const tab = Number(target.slice(4));
  if (!Number.isSafeInteger(tab)) throw new Error('invalid_target');
  return tab;
}
async function command(message) {
  const port = native;
  const epoch = generation;
  if (!message || size(message) > MAX_COMMAND_BYTES || !['control', 'cdp'].includes(message.kind)
      || typeof message.lease !== 'string' || !/^[a-f0-9]{32}$/.test(message.lease)
      || !Number.isSafeInteger(message.channel) || !Number.isSafeInteger(message.request?.id)) {
    await disconnect(); return;
  }
  if (admitted >= 32) { await disconnect(); return; }
  admitted += 1;
  let response;
  try {
    if (message.kind === 'control') {
      response = {id: message.request.id, result: await control(message, epoch)};
    } else {
      const tab = parseTarget(message.target);
      if (!leases.get(message.lease)?.tabs.has(tab) || tabLease.get(tab) !== message.lease) throw new Error('stale_target');
      const method = message.request.method;
      if (typeof method !== 'string' || !allowedDomains.has(method.split('.')[0]) || method.length > 128) throw new Error('unsupported_domain');
      response = {id: message.request.id, result: await chrome.debugger.sendCommand({tabId: tab}, method, message.request.params ?? {})};
    }
  } catch {
    // Chrome error strings may contain URLs or other page data. Do not use them
    // as bridge diagnostics; the Runner preserves post-dispatch uncertainty.
    response = {id: message.request.id, error: {code: 'external_operation_failed'}};
  } finally { admitted -= 1; }
  if (native !== port || epoch !== generation) return;
  try { send({kind: 'message', channel: message.channel, lease: message.lease, message: response}); }
  catch { await disconnect(); }
}
chrome.debugger.onEvent.addListener((source, method, params) => {
  const lease = tabLease.get(source.tabId);
  if (!lease || source.sessionId) return;
  const event = {kind: 'event', lease, target: `tab_${source.tabId}`, message: {method, params}};
  if (size(event) > MAX_EVENT_BYTES) { void disconnect(); return; }
  try { send(event); } catch { void disconnect(); }
});
chrome.debugger.onDetach.addListener(source => {
  const leaseId = tabLease.get(source.tabId);
  if (!leaseId) return;
  void detachLease(leaseId);
  offers.delete(source.tabId);
  if (native) { try { send({kind: 'revoke', tab: source.tabId}); } catch { void disconnect(); } }
});
chrome.tabs.onRemoved.addListener(tabId => {
  const leaseId = tabLease.get(tabId);
  const lease = leases.get(leaseId);
  lease?.tabs.delete(tabId);
  tabLease.delete(tabId);
  if (offers.delete(tabId)) {
    if (leaseId) void detachLease(leaseId);
    if (native) { try { send({kind: 'revoke', tab: tabId}); } catch { void disconnect(); } }
  }
});
