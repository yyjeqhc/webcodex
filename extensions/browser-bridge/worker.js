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
// A later explicit Revoke cancels every earlier in-flight Share, even if its
// Chrome tab lookup or native handshake has not completed yet.
let consentRevision = 0;
const offers = new Map();
const leases = new Map();
const tabLease = new Map();
// Chrome callbacks identify only a tab, not an attach generation. Keep it busy
// until pending attach and detach completions can no longer affect a new lease.
const attaching = new Set();
const detaching = new Set();
const diagnosticEvents = new Set(['Runtime.consoleAPICalled', 'Runtime.exceptionThrown', 'Log.entryAdded',
  'Network.requestWillBeSent', 'Network.responseReceived', 'Network.loadingFailed', 'Network.loadingFinished']);
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
  // Invalidate every tab synchronously before the first asynchronous detach.
  for (const tab of lease.tabs) {
    tabLease.delete(tab);
    detaching.add(tab);
  }
  await Promise.all([...lease.tabs].map(async tab => {
    try { await chrome.debugger.detach({tabId: tab}); } catch { /* Already detached. */ }
    finally { detaching.delete(tab); }
  }));
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
      if (native === port) { console.info('browser_bridge: native_disconnected'); void disconnect(); }
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
  if (sender.id !== chrome.runtime.id || !['status', 'share', 'revoke'].includes(message?.action)) return false;
  const revision = consentRevision;
  if (message.action === 'revoke') consentRevision += 1;
  (async () => {
    const [tab] = await chrome.tabs.query({active: true, currentWindow: true});
    if (!tab?.id) throw new Error('no_active_tab');
    if (message.action === 'status') return tabStatus(tab.id);
    if (message.action === 'share' && revision !== consentRevision) throw new Error('share_superseded');
    if (message.action === 'share' && native && (tabLease.has(tab.id) || offers.get(tab.id)?.expires > Date.now())) return tabStatus(tab.id);
    if (message.action === 'revoke') {
      // Invalidate consent before awaiting cleanup, including attach operations
      // that have reserved a lease but have not obtained the debugger yet.
      console.info('browser_bridge: explicit_revoke');
      const revoked = [...leases.values()].filter(lease => lease.originalTab === tab.id || lease.tabs.has(tab.id));
      const sharedTabs = new Set([tab.id, ...revoked.map(lease => lease.originalTab)]);
      for (const sharedTab of sharedTabs) offers.delete(sharedTab);
      const cleanup = Promise.all(revoked.map(lease => detachLease(lease.id)));
      for (const sharedTab of sharedTabs) {
        if (native) { try { send({kind: 'revoke', tab: sharedTab}); } catch { await disconnect(); } }
      }
      await cleanup;
      return {ok: true, message: 'Tab revoked. Chrome and its login data remain unchanged.'};
    }
    if (!/^https?:\/\//i.test(tab.url ?? '') || offers.size >= 16) throw new Error('tab_not_shareable');
    const port = await connect();
    if (native !== port || revision !== consentRevision) throw new Error('share_superseded');
    if (tabLease.has(tab.id) || offers.get(tab.id)?.expires > Date.now()) return tabStatus(tab.id);
    if (offers.size >= 16) throw new Error('tab_not_shareable');
    offers.set(tab.id, {window: tab.windowId, expires: Date.now() + 600000});
    send({kind: 'offer', tab: tab.id, window: tab.windowId, title: (tab.title ?? '').slice(0, 256), url: tab.url});
    return tabStatus(tab.id);
  })().then(respond, () => respond({ok: false}));
  return true;
});

// Read only: opening the popup never creates consent or connects a native host.
function tabStatus(tab) {
  if (native && tabLease.has(tab)) {
    return attaching.has(tab)
      ? {ok:true, state:'offered', message:'Tab authorized; Attach is in progress.'}
      : {ok:true, state:'attached', message:'Tab attached. Use WebCodex browsers/pages; sharing again is unnecessary.'};
  }
  if (native && offers.get(tab)?.expires > Date.now())
    return {ok:true, state:'offered', message:'Tab authorized and waiting for Attach. Use WebCodex discover, then attach.'};
  if (!native && generation > 0)
    return {ok:true, state:'disconnected', message:'Bridge connection lost. Check the Runner, then Share this tab again.'};
  return {ok:true, state:'unshared', message:'Tab is not shared. Share authorizes only this tab; new website tabs require their own Share.'};
}

function requireLease(lease) {
  if (!native || lease.generation !== generation || leases.get(lease.id) !== lease) throw new Error('lease_lost');
}
async function attachTab(lease, tab) {
  requireLease(lease);
  if (tabLease.has(tab) || attaching.has(tab) || detaching.has(tab)) throw new Error('tab_already_attached');
  attaching.add(tab);
  lease.tabs.add(tab);
  tabLease.set(tab, lease.id);
  try {
    await chrome.debugger.attach({tabId: tab}, '1.3');
    // Reservation makes onDetach visible even before attach's promise resolves.
    try { requireLease(lease); }
    catch (error) {
      try { await chrome.debugger.detach({tabId: tab}); } catch { /* Already detached. */ }
      throw error;
    }
  } catch (error) {
    lease.tabs.delete(tab);
    if (tabLease.get(tab) === lease.id) tabLease.delete(tab);
    throw error;
  } finally { attaching.delete(tab); }
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
// This projection mirrors record_cdp_event in webcodex-browser. Never forward
// request bodies, headers, cookies, stack traces or remote object previews.
// Keep identities exact: an oversized identity must produce a loss marker.
// Match the Rust display budgets without truncating request identities. Display
// text was already clipped by Rust; clipping before transport preserves evidence.
function diagnosticText(value, limit) {
  if (typeof value !== 'string') return undefined;
  let text = '', bytes = 0;
  for (const character of value) {
    const length = encoder.encode(character).length;
    if (bytes + length > limit) break;
    text += character; bytes += length;
  }
  return text;
}
function consoleText(args = []) {
  let text = '', seen = false;
  for (const arg of args) {
    // Never stringify page-controlled object graphs just to clip them later:
    // their nested data can be unbounded and include remote object previews.
    const primitive = arg.value;
    const value = primitive !== undefined
      ? (primitive !== null && typeof primitive === 'object' ? '[complex value omitted]' : String(primitive))
      : (arg.type === 'object' || arg.type === 'function' ? '[complex value omitted]' : arg.description);
    if (typeof value !== 'string') continue;
    const combined = text + (seen ? ' ' : '') + value;
    text = diagnosticText(combined, 2048);
    seen = true;
    if (text.length < combined.length || encoder.encode(text).length === 2048) break;
  }
  return text;
}
function diagnosticParams(method, p = {}) {
  const text = diagnosticText;
  switch (method) {
    case 'Network.requestWillBeSent':
      return {requestId:p.requestId, type:text(p.type,64), timestamp:p.timestamp,
        request:{method:text(p.request?.method,128), url:text(p.request?.url,8192)}};
    case 'Network.responseReceived': return {requestId:p.requestId, response:{status:p.response?.status}};
    case 'Network.loadingFailed': return {requestId:p.requestId, errorText:text(p.errorText,512)};
    case 'Network.loadingFinished': return {requestId:p.requestId};
    case 'Runtime.consoleAPICalled':
      return {type:text(p.type,64), timestamp:p.timestamp, args:[{value:consoleText(p.args)}]};
    case 'Runtime.exceptionThrown':
      return {timestamp:p.timestamp, exceptionDetails:{text:text(p.exceptionDetails?.text,2048), url:text(p.exceptionDetails?.url,8192)}};
    case 'Log.entryAdded': return {entry:{level:text(p.entry?.level,64), text:text(p.entry?.text,2048),
      url:text(p.entry?.url,8192), timestamp:p.entry?.timestamp}};
  }
}
chrome.debugger.onEvent.addListener((source, method, params) => {
  const lease = tabLease.get(source.tabId);
  if (!lease || source.sessionId || !diagnosticEvents.has(method)) return;
  let projected;
  try { projected = diagnosticParams(method, params); } catch { projected = null; }
  let event = {kind: 'event', lease, target: `tab_${source.tabId}`, message: {method, params: projected}};
  // Page-controlled diagnostic payloads are not loss of user consent. Preserve
  // the wire bound and explicitly mark missing evidence; never forward the body.
  if (projected === null || (method.startsWith('Network.') && (typeof params?.requestId !== 'string' || !params.requestId))
      || size(event) > MAX_EVENT_BYTES) {
    event = {kind: 'event', lease, target: `tab_${source.tabId}`,
      message: {method: 'WebCodex.eventsDiscarded', params: {domain: method.split('.')[0]}}};
  }
  try { send(event); } catch { void disconnect(); }
});
function revokeTab(tabId, reason) {
  const leaseId = tabLease.get(tabId);
  const lease = leases.get(leaseId);
  const originalTab = lease?.originalTab ?? tabId;
  if (lease && originalTab !== tabId && ['tab_closed', 'debugger_target_closed'].includes(reason)) {
    lease.tabs.delete(tabId);
    tabLease.delete(tabId);
    return;
  }
  const offered = offers.delete(originalTab);
  if (!lease && !offered) return;
  console.info(`browser_bridge: ${reason}`);
  // Rust indexes consent by the original shared tab, including detach of a
  // tool-created child. Invalidate the same lease on both sides.
  if (lease) void detachLease(leaseId);
  if (native) { try { send({kind: 'revoke', tab: originalTab}); } catch { void disconnect(); } }
}
chrome.debugger.onDetach.addListener((source, reason) => {
  if (source.sessionId || detaching.has(source.tabId)) return;
  const diagnostic = reason === 'target_closed' ? 'debugger_target_closed'
    : reason === 'canceled_by_user' ? 'debugger_canceled_by_user' : 'debugger_detached';
  revokeTab(source.tabId, diagnostic);
});
chrome.tabs.onRemoved.addListener(tabId => revokeTab(tabId, 'tab_closed'));
