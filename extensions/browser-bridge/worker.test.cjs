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
for (const revoke of [true, false]) {
  test(`consent loss during tab lookup prevents debugger attach (${revoke ? 'revoke' : 'disconnect'})`,async()=>{
    const f=fixture();try {
      await f.share();let release;f.setGetHook(()=>new Promise(resolve=>{release=resolve;}));
      const attaching=f.command('attach');
      for(let n=0;n<10&&!release;n++) await Promise.resolve();
      assert(release);
      if(revoke) await f.revoke(); else await f.cleanup();
      release();await attaching;
      assert(!f.calls.some(call=>call[0]==='attach'),'revoked lookup must not dispatch an attach');
    } finally {await f.cleanup();}
  });
}
test('revocation during pending debugger attach detaches its late completion',async()=>{
  const f=fixture();try {
    await f.share();let release;f.setAttachHook(()=>new Promise(resolve=>{release=resolve;}));
    const attaching=f.command('attach');
    for(let n=0;n<10&&!release;n++) await Promise.resolve();
    assert(release);await f.revoke();release();await attaching;
    assert(f.calls.some(call=>call[0]==='detach'));
    assert.equal(vm.runInContext('leases.size',f.context),0);
  } finally {await f.cleanup();}
});
test('disconnect during new-page lookup cannot create a later unowned tab',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');
    let release;f.setGetHook(()=>new Promise(resolve=>{release=resolve;}));
    const creating=f.command('new_page',{url:'about:blank'});
    for(let n=0;n<10&&!release;n++) await Promise.resolve();
    assert(release);await f.cleanup();release();await creating;
    assert(!f.calls.some(call=>call[0]==='create'));
  } finally {await f.cleanup();}
});
function event() {
  const listeners = [];
  return {addListener(fn) { listeners.push(fn); }, emit(...args) { for (const fn of listeners) fn(...args); }, listeners};
}
function fixture() {
  const calls = [], sent = [], tabs = new Map([[7, {id:7, windowId:9, title:'Signed in', url:'https://example.test/account'}]]);
  let activeTab = 7, nextTab = 8, attachHook = null, getHook = null, queryHook = null, readyHook = null, commandHook = null, detachHook = null;
  const port = {onMessage:event(), onDisconnect:event(), postMessage(value) { sent.push(value); }, disconnect() { calls.push(['disconnect']); }};
  const chrome = {
    runtime:{id:ID, onMessage:event(), connectNative(host) { assert.equal(host, 'com.webcodex.browser_bridge'); queueMicrotask(() => { if(readyHook) readyHook(port); else port.onMessage.emit({kind:'ready',version:1}); }); return port; }},
    windows:{async get(id) { assert.equal(id,9); return {id, left:40, top:60, width:800, height:600}; }},
    tabs:{onRemoved:event(), async query() { if(queryHook) await queryHook(); return [tabs.get(activeTab)]; }, async get(id) { if(getHook) await getHook(); if (!tabs.has(id)) throw Error('missing'); return tabs.get(id); },
      async create(options) { const tab={id:nextTab++,windowId:options.windowId,url:options.url,title:'New'};tabs.set(tab.id,tab);calls.push(['create',tab.id]);return tab; },
      async remove(id) { calls.push(['remove',id]);tabs.delete(id);chrome.tabs.onRemoved.emit(id); }},
    debugger:{onEvent:event(),onDetach:event(),async attach(debuggee) { calls.push(['attach',debuggee.tabId]);if(attachHook) await attachHook(); },
      async detach(debuggee) { calls.push(['detach',debuggee.tabId]);if(detachHook) await detachHook();chrome.debugger.onDetach.emit(debuggee,'canceled_by_user'); },
      async sendCommand(debuggee,method,params) { calls.push(['cdp',debuggee.tabId,method]);if(commandHook) await commandHook();return {fixture:true}; }}
  };
  const context=vm.createContext({chrome,setTimeout,clearTimeout,TextEncoder,console});
  vm.runInContext(source,context,{filename:'worker.js'});
  const command = (method, params={}, kind='control', target=null) => {
    context.message={kind,channel:1,lease:LEASE,tab:7,target,request:{id:1,method,params}};
    return vm.runInContext('command(message)',context);
  };
  const request=action => new Promise(resolve => chrome.runtime.onMessage.listeners[0]({action},{id:ID},resolve));
  const share=() => request('share');
  return {calls,sent,tabs,chrome,port,context,command,share,status:()=>request('status'),setActiveTab(id){activeTab=id;},revoke:()=>request('revoke'),setCommandHook(fn){commandHook=fn;},setDetachHook(fn){detachHook=fn;},setGetHook(fn){getHook=fn;},setQueryHook(fn){queryHook=fn;},setReadyHook(fn){readyHook=fn;},setAttachHook(fn){attachHook=fn;},async cleanup(){await vm.runInContext('disconnect()',context);}};
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
    await f.command('window_bounds');
    assert.equal(JSON.stringify(f.sent.at(-1).message.result),JSON.stringify({x:40,y:60,width:800,height:600}));
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

test('large navigation diagnostics do not disconnect or revoke a shared tab',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');
    f.chrome.debugger.onEvent.emit({tabId:7},'Runtime.consoleAPICalled',{args:[{value:'x'.repeat(70000)}]});
    assert.equal(vm.runInContext('leases.size',f.context),1);
    assert(!f.calls.some(call=>call[0]==='disconnect'));
    assert.equal(f.sent.at(-1).message.method,'Runtime.consoleAPICalled');
    assert.equal(f.sent.at(-1).message.params.args[0].value.length,2048);
    await f.command('Page.getFrameTree',{},'cdp','tab_7');
    assert(f.sent.at(-1).message.result);
  } finally {await f.cleanup();}
});

test('external detach during pending attach cannot resurrect consent',async()=>{
  const f=fixture();try {
    await f.share();let release;f.setAttachHook(()=>new Promise(resolve=>{release=resolve;}));
    const attaching=f.command('attach');
    for(let n=0;n<10&&!release;n++) await Promise.resolve();
    assert(release);
    f.chrome.debugger.onDetach.emit({tabId:7},'canceled_by_user');
    release();await attaching;
    assert.equal(vm.runInContext('leases.size',f.context),0);
    await f.command('Page.navigate',{url:'https://example.test/next'},'cdp','tab_7');
    assert(!f.calls.some(call=>call[0]==='cdp'));
  } finally {await f.cleanup();}
});

for (const loss of ['revoke','target_closed','canceled_by_user','unknown_private_reason','tab_close','native_disconnect']) {
  test(`navigation racing ${loss} loses consent and never repeats the write`,async()=>{
    const f=fixture();try {
      await f.share();await f.command('attach');
      let release;f.setCommandHook(()=>new Promise(resolve=>{release=resolve;}));
      const navigating=f.command('Page.navigate',{url:'https://example.test/next'},'cdp','tab_7');
      assert(release);
      if(loss==='revoke') await f.revoke();
      else if(loss==='tab_close') await f.chrome.tabs.remove(7);
      else if(loss==='native_disconnect') f.port.onDisconnect.emit();
      else f.chrome.debugger.onDetach.emit({tabId:7},loss);
      assert.equal(vm.runInContext('leases.size',f.context),0);
      release();await navigating;f.setCommandHook(null);
      await f.command('Page.getFrameTree',{},'cdp','tab_7');
      assert.deepEqual(f.calls.filter(call=>call[0]==='cdp'),[['cdp',7,'Page.navigate']]);
      assert.equal(vm.runInContext('offers.size',f.context),0);
    } finally {await f.cleanup();}
  });
}

for (const method of ['Page.navigate','Page.reload']) {
  test(`${method} preserves exact shared tab and does not adopt a popup`,async()=>{
    const f=fixture();try {
      await f.share();await f.command('attach');
      await f.command(method,{url:'https://example.test/next'},'cdp','tab_7');
      f.chrome.debugger.onEvent.emit({tabId:7},'Runtime.executionContextsCleared',{});
      f.chrome.debugger.onEvent.emit({tabId:7},'Page.frameNavigated',{frame:{loaderId:'new-document'}});
      f.tabs.set(8,{id:8,windowId:9,openerTabId:7,url:'https://example.test/job'});
      await f.command('targets');
      assert.equal(f.sent.at(-1).message.result.length,1);
      await f.command('Page.getFrameTree',{},'cdp','tab_7');
      assert(f.sent.at(-1).message.result);
      await f.command('Page.getFrameTree',{},'cdp','tab_8');
      assert(f.sent.at(-1).message.error);
      assert.equal(f.calls.filter(call=>call[0]==='attach').length,1);
      assert(!f.calls.some(call=>call[0]==='disconnect'));
    } finally {await f.cleanup();}
  });
}

test('unused oversized events are not sent; diagnostic loss markers stay bounded',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');
    const before=f.sent.length;
    f.chrome.debugger.onEvent.emit({tabId:7},'Network.requestWillBeSentExtraInfo',{headers:{private:'x'.repeat(70000)}});
    assert.equal(f.sent.length,before);
    f.chrome.debugger.onEvent.emit({tabId:7},'Network.requestWillBeSent',{request:{postData:'private'.repeat(10000)}});
    assert.equal(f.sent.at(-1).message.params.domain,'Network');
    assert(Buffer.byteLength(JSON.stringify(f.sent.at(-1)))<65536);
    assert(!JSON.stringify(f.sent.at(-1)).includes('private'));
    assert.equal(vm.runInContext('leases.size',f.context),1);
  } finally {await f.cleanup();}
});

test('slow detach invalidates all tabs immediately and blocks replacement attach',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');await f.command('new_page',{url:'about:blank'});
    const releases=[];f.setDetachHook(()=>new Promise(resolve=>releases.push(resolve)));
    const revoking=f.revoke();
    for(let n=0;n<10&&releases.length<2;n++) await Promise.resolve();
    assert.equal(releases.length,2);
    await f.command('Page.navigate',{},'cdp','tab_8');
    assert(!f.calls.some(call=>call[0]==='cdp'));
    await f.share();await f.command('attach');
    assert.equal(f.calls.filter(call=>call[0]==='attach'&&call[1]===7).length,1);
    releases.forEach(resolve=>resolve());await revoking;
    f.setDetachHook(null);
  } finally {await f.cleanup();}
});

test('revoking a tool-created child invalidates original shared consent before detach finishes',async()=>{
  const f=fixture();try {
    await f.share();await f.command('attach');await f.command('new_page',{url:'about:blank'});
    f.setActiveTab(8);
    const releases=[];f.setDetachHook(()=>new Promise(resolve=>releases.push(resolve)));
    const revoking=f.revoke();
    for(let n=0;n<10&&releases.length<2;n++) await Promise.resolve();
    assert.equal(releases.length,2);
    assert(f.sent.some(message=>message.kind==='revoke'&&message.tab===7));
    assert.equal(vm.runInContext('offers.size',f.context),0);
    assert.equal(vm.runInContext('leases.size',f.context),0);
    releases.forEach(resolve=>resolve());await revoking;f.setDetachHook(null);
  } finally {await f.cleanup();}
});

test('failed debugger attach releases reservations without detaching another debugger',async()=>{
  const f=fixture();try {
    await f.share();f.setAttachHook(()=>{throw Error('already attached by another debugger');});
    await f.command('attach');
    assert.equal(vm.runInContext('leases.size + tabLease.size + attaching.size',f.context),0);
    assert(!f.calls.some(call=>call[0]==='detach'));
  } finally {await f.cleanup();}
});

for (const method of ['Network.requestWillBeSent', 'Network.responseReceived', 'Runtime.consoleAPICalled']) {
  test(`projects oversized unused fields for ${method} without losing evidence`, async () => {
    const f = fixture(); try {
      await f.share(); await f.command('attach');
      const secret = 'private'.repeat(20000);
      f.chrome.debugger.onEvent.emit({tabId:7}, method, {
        requestId:'exact-id', type:'Document', timestamp:12,
        request:{method:'GET', url:'https://example.test/', headers:{cookie:secret}, postData:secret},
        response:{status:200, headers:{cookie:secret}},
        args:[{value:'hello', preview:{description:secret}, objectId:secret}], stackTrace:secret
      });
      const event = f.sent.at(-1);
      assert.equal(event.message.method, method);
      assert(!JSON.stringify(event).includes('private'));
      assert(Buffer.byteLength(JSON.stringify(event)) < 65536);
      await f.command('Page.navigate',{url:'https://example.test/next'},'cdp','tab_7');
      assert(f.sent.at(-1).message.result);
      assert.equal(vm.runInContext('leases.size',f.context),1);
    } finally { await f.cleanup(); }
  });
}

test('popup status is read only and repeated Share is idempotent', async () => {
  const f = fixture(); try {
    assert.equal((await f.status()).state, 'unshared');
    assert.equal(f.sent.length, 0);
    assert.equal((await f.share()).state, 'offered');
    const offered = f.sent.length;
    assert.equal((await f.share()).state, 'offered');
    assert.equal(f.sent.length, offered);
    await f.command('attach');
    assert.equal((await f.status()).state, 'attached');
    assert.equal((await f.share()).state, 'attached');
    assert.equal(f.sent.filter(m => m.kind === 'offer').length, 1);
    f.tabs.set(8,{id:8,windowId:9,openerTabId:7,url:'https://example.test/child'});
    f.setActiveTab(8);
    assert.equal((await f.status()).state, 'unshared');
    f.setActiveTab(7);
    await f.cleanup();
    assert.equal((await f.status()).state, 'disconnected');
    assert.equal((await f.share()).state, 'offered');
    await f.revoke();
    assert.equal((await f.status()).state, 'unshared');
  } finally { await f.cleanup(); }
});

test('oversized network identities still mark loss; display-only payloads are UTF-8 bounded', async () => {
  const f=fixture(); try {
    await f.share(); await f.command('attach');
    f.chrome.debugger.onEvent.emit({tabId:7},'Network.requestWillBeSent',{requestId:'x'.repeat(70000),request:{url:'https://example.test/'}});
    assert.equal(f.sent.at(-1).message.method,'WebCodex.eventsDiscarded');
    assert.equal(f.sent.at(-1).message.params.domain,'Network');
    f.chrome.debugger.onEvent.emit({tabId:7},'Runtime.consoleAPICalled',{args:[{value:'😀'.repeat(20000)}]});
    assert.equal(Buffer.byteLength(f.sent.at(-1).message.params.args[0].value),2048);
    f.chrome.debugger.onEvent.emit({tabId:7},'Network.requestWillBeSent',{requestId:'exact',request:{url:'https://example.test/'+ 'x'.repeat(100000)}});
    assert.equal(f.sent.at(-1).message.params.requestId,'exact');
    assert.equal(Buffer.byteLength(f.sent.at(-1).message.params.request.url),8192);
    await f.cleanup(); await f.share(); await f.command('attach');
    assert.equal(vm.runInContext('leases.size',f.context),1);
  } finally {await f.cleanup();}
});

test('Revoke wins against an earlier Share awaiting Chrome tab lookup', async () => {
  const f=fixture(); try {
    let release, queries=0;
    f.setQueryHook(() => ++queries === 1 ? new Promise(resolve => { release=resolve; }) : Promise.resolve());
    const pendingShare=f.share();
    for(let n=0;n<10 && !release;n++) await Promise.resolve();
    assert(release);
    assert.equal((await f.revoke()).ok, true);
    release();
    await pendingShare;
    assert.equal(f.sent.filter(m=>m.kind==='offer').length,0);
    assert.equal((await f.status()).state,'unshared');
    assert.equal((await f.share()).state,'offered');
  } finally {await f.cleanup();}
});

test('Revoke wins against an earlier Share awaiting native readiness', async () => {
  const f=fixture(); try {
    let release;
    f.setReadyHook(port => { release=() => port.onMessage.emit({kind:'ready',version:1}); });
    const pendingShare=f.share();
    for(let n=0;n<10 && !release;n++) await Promise.resolve();
    assert(release);
    assert.equal((await f.revoke()).ok, true);
    release();
    await pendingShare;
    assert.equal(f.sent.filter(m=>m.kind==='offer').length,0);
    assert.equal((await f.status()).state,'unshared');
    assert.equal((await f.share()).state,'offered');
  } finally {await f.cleanup();}
});

test('large console object values are bounded before serialization', async () => {
  const f=fixture(); try {
    await f.share(); await f.command('attach');
    const object={nested:{private:'sensitive'.repeat(100000)}};
    f.chrome.debugger.onEvent.emit({tabId:7},'Runtime.consoleAPICalled',{args:[{value:object}]});
    const data=JSON.stringify(f.sent.at(-1));
    assert(!data.includes('sensitive'));
    assert(data.includes('omitted') || f.sent.at(-1).message.method==='WebCodex.eventsDiscarded');
    assert.equal(vm.runInContext('leases.size',f.context),1);
  } finally {await f.cleanup();}
});

test('concurrent Share requests issue only one offer after connection', async () => {
  const f=fixture(); try {
    await Promise.all([f.share(), f.share(), f.share()]);
    assert.equal(f.sent.filter(m => m.kind === 'offer').length, 1);
    f.chrome.debugger.onEvent.emit({tabId:7},'Network.loadingFinished',{requestId:'not-attached'});
    assert.equal(f.sent.length, 1);
    await f.command('attach');
    f.chrome.debugger.onEvent.emit({tabId:7},'Runtime.consoleAPICalled',{args:[{value:'x'.repeat(2045)},{value:'a'}]});
    assert.equal(f.sent.at(-1).message.params.args[0].value, 'x'.repeat(2045)+' a');
  } finally {await f.cleanup();}
});
