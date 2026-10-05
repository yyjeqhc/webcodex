'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {readFileSync} = require('node:fs');
const {createHash} = require('node:crypto');
const vm = require('node:vm');
const path = require('node:path');
const manifest = JSON.parse(readFileSync(path.join(__dirname, 'manifest.json'), 'utf8'));
const source = readFileSync(path.join(__dirname, 'worker.js'), 'utf8');
const ID = 'pjhnlafbcbgcgjpkfiomhjnhpnaohgcg';
const LEASE = 'a'.repeat(32);
function event() {
  const listeners = [];
  return {addListener(fn) { listeners.push(fn); }, emit(...args) { for (const fn of listeners) fn(...args); }, listeners};
}
function fixture() {
  const calls = [], sent = [], tabs = new Map([[7, {id:7, windowId:9, title:'Signed in', url:'https://example.test/account'}]]);
  let nextTab = 8, attachHook = null;
  const port = {onMessage:event(), onDisconnect:event(), postMessage(value) { sent.push(value); }, disconnect() { calls.push(['disconnect']); }};
  const chrome = {
    runtime:{id:ID, onMessage:event(), connectNative(host) { assert.equal(host, 'com.webcodex.browser_bridge'); queueMicrotask(() => port.onMessage.emit({kind:'ready',version:1})); return port; }},
    tabs:{onRemoved:event(), async query() { return [tabs.get(7)]; }, async get(id) { if (!tabs.has(id)) throw Error('missing'); return tabs.get(id); },
      async create(options) { const tab={id:nextTab++,windowId:options.windowId,url:options.url,title:'New'};tabs.set(tab.id,tab);calls.push(['create',tab.id]);return tab; },
      async remove(id) { calls.push(['remove',id]);tabs.delete(id);chrome.tabs.onRemoved.emit(id); }},
    debugger:{onEvent:event(),onDetach:event(),async attach(debuggee) { calls.push(['attach',debuggee.tabId]);if(attachHook) await attachHook(); },
      async detach(debuggee) { calls.push(['detach',debuggee.tabId]);chrome.debugger.onDetach.emit(debuggee,'canceled_by_user'); },
      async sendCommand(debuggee,method,params) { calls.push(['cdp',debuggee.tabId,method]);return {fixture:true}; }}
  };
  const context=vm.createContext({chrome,setTimeout,clearTimeout,TextEncoder,console});
  vm.runInContext(source,context,{filename:'worker.js'});
  const command = (method, params={}, kind='control', target=null) => {
    context.message={kind,channel:1,lease:LEASE,tab:7,target,request:{id:1,method,params}};
    return vm.runInContext('command(message)',context);
  };
  const share=() => new Promise(resolve => chrome.runtime.onMessage.listeners[0]({action:'share'},{id:ID},resolve));
  return {calls,sent,tabs,chrome,port,context,command,share,setAttachHook(fn){attachHook=fn;},async cleanup(){await vm.runInContext('disconnect()',context);}};
}
test('manifest public key fixes the exact native caller identity',()=>{
  const digest=createHash('sha256').update(Buffer.from(manifest.key,'base64')).digest('hex').slice(0,32);
  const identity=[...digest].map(n=>String.fromCharCode(97+parseInt(n,16))).join('');
  assert.equal(identity,ID);
  assert.deepEqual(manifest.permissions,['activeTab','debugger','nativeMessaging']);
  assert.equal(manifest.externally_connectable,undefined);
});
test('sharing requires explicit attach; detach leaves the external tab and login context',async()=>{
  const f=fixture();try {
    assert.equal((await f.share()).ok,true);
    assert.equal(f.sent[0].kind,'offer');assert.equal(f.calls.length,0);
    await f.command('attach');assert.deepEqual(f.calls,[['attach',7]]);
    await f.command('Page.getFrameTree',{},'cdp','tab_7');
    assert.deepEqual(f.calls[1],['cdp',7,'Page.getFrameTree']);
    await f.command('detach');
    assert(f.calls.some(call=>call[0]==='detach'&&call[1]===7));
    assert(!f.calls.some(call=>call[0]==='remove'));assert(f.tabs.has(7));
  } finally { await f.cleanup(); }
});
test('unoffered attachment, wrong target and process-level commands cannot dispatch',async()=>{
  const f=fixture();try {
    await f.share();await f.command('Page.getFrameTree',{},'cdp','tab_7');
    assert.equal(f.calls.length,0);
    await f.command('attach');
    await f.command('Page.getFrameTree',{},'cdp','tab_99');
    await f.command('Browser.close',{},'cdp','tab_7');
    assert(!f.calls.some(call=>call[0]==='cdp'));
    assert(f.sent.at(-1).message.error);assert(f.tabs.has(7));
  } finally { await f.cleanup(); }
});
test('connection loss while debugger attach is pending immediately detaches late completion',async()=>{
  const f=fixture();try {
    await f.share();let release;f.setAttachHook(()=>new Promise(resolve=>{release=resolve;}));
    const attaching=f.command('attach');
    for(let n=0;n<10&&!release;n++) await Promise.resolve();
    assert(release);await f.cleanup();release();await attaching;
    assert(f.calls.some(call=>call[0]==='detach'&&call[1]===7));
    assert(!f.calls.some(call=>call[0]==='remove'));assert(f.tabs.has(7));
  } finally { await f.cleanup(); }
});
test('explicit close_page only removes the exact tab in the current lease',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');
    await f.command('close_page',{targetId:'tab_99'});assert(f.tabs.has(7));
    await f.command('new_page',{url:'about:blank'});assert(f.tabs.has(8));
    await f.command('close_page',{targetId:'tab_8'});assert(!f.tabs.has(8));assert(f.tabs.has(7));
    assert.deepEqual(f.calls.filter(call=>call[0]==='remove'),[['remove',8]]);
  } finally { await f.cleanup(); }
});
test('runner disconnect detaches all leased tabs without closing Chrome tabs',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');await f.command('new_page',{url:'about:blank'});
    await f.cleanup();assert(f.tabs.has(7)&&f.tabs.has(8));
    assert.deepEqual(f.calls.filter(call=>call[0]==='detach'),[['detach',7],['detach',8]]);
    assert(!f.calls.some(call=>call[0]==='remove'));
  } finally { await f.cleanup(); }
});
