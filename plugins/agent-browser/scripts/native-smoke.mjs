// Real native inheritance test. All profile, HOME, config, cookie/storage and
// page data are synthetic and temporary. Never uses the person's daily profile.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
import { PluginClient } from '../tests/rpc-client.mjs';
import { chromeExecutable } from '../tests/chrome-fixture.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));
// Short HOME also keeps agent-browser's UNIX socket path below macOS's limit.
const home = await fs.mkdtemp(path.join(process.platform === 'darwin' ? '/tmp' : os.tmpdir(), 'wcn-'));
const cwd = path.join(home, 'project'); await fs.mkdir(cwd); await fs.mkdir(path.join(home, '.agent-browser'));
const userFile = path.join(home, '.agent-browser/config.json');
const projectFile = path.join(cwd, 'agent-browser.json');
const operator = path.join(home, 'plugin.local.json');
const profile = path.join(home, 'profile');
await fs.writeFile(userFile, JSON.stringify({ profile, executablePath:chromeExecutable, userAgent:'WC-User', headed:false }));
await fs.writeFile(projectFile, JSON.stringify({ userAgent:'WC-Project' }));
await fs.writeFile(operator, JSON.stringify({ executable:process.env.WC_TEST_AGENT_BROWSER || 'agent-browser', connection:'native', workingDirectory:cwd }));
const env = { HOME:home, USERPROFILE:home, PATH:process.env.PATH, TMPDIR:home, SystemRoot:process.env.SystemRoot, AGENT_BROWSER_USER_AGENT:'WC-Environment' };
const checks=[]; const pass=label=>{checks.push(label);console.log('PASS '+label);};
const server=http.createServer((req,res)=>{
  res.setHeader('Content-Type','text/html;charset=utf-8');res.setHeader('Cache-Control','no-store');
  res.end(`<!doctype html><html><head><title>Native inheritance fixture</title></head><body>
    <h1>Native inheritance fixture</h1><p id="ua"></p><p id="saved"></p>
    <label>Name <input id="name"></label><button id="save">Save test value</button><p id="result">Ready</p>
    <script>document.querySelector('#ua').textContent='Agent '+navigator.userAgent;
    document.querySelector('#saved').textContent='Saved '+(localStorage.getItem('wc-native-test')||'none');
    document.querySelector('#save').onclick=()=>{const v=document.querySelector('#name').value;
    localStorage.setItem('wc-native-test',v);document.querySelector('#result').textContent='Stored '+v;};</script></body></html>`);
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const url=`http://127.0.0.1:${server.address().port}/`;
let client;
const create=async e=>{client=new PluginClient(operator,{env:e,cwd});await client.init();return client;};
try {
  await create(env);
  assert.equal((await client.request('tools/list',{})).tools.length,16);
  let status=await client.call('browser_status');
  assert.equal(status.connection_mode,'native');assert.equal(status.configuration.profile_kind,'persistent_directory');
  assert.equal(status.configuration.user_config_present,true);assert.equal(status.configuration.project_config_present,true);
  await assert.rejects(fs.access(profile));
  pass('Native status and catalog discover configuration without starting a browser');
  let connection=await client.call('browser_connect');assert.equal(connection.browser_ownership,'plugin');
  let opened=await client.call('browser_open',{url});let page=opened.page.page_id;
  let snap=await client.call('browser_snapshot',{page_id:page});assert(snap.text.includes('Agent WC-Environment'));
  pass('Real CLI inherits original user config, project cwd and environment precedence');
  await fs.access(profile);pass('Native profile directory is used by the real browser');
  const input=snap.elements.find(e=>e.role==='textbox');
  await client.call('browser_fill',{page_id:page,snapshot_id:snap.snapshot_id,element_id:input.element_id,text:'WebCodex native 中文'});
  snap=await client.call('browser_snapshot',{page_id:page});
  await client.call('browser_click',{page_id:page,snapshot_id:snap.snapshot_id,element_id:snap.elements.find(e=>e.name==='Save test value').element_id});
  snap=await client.call('browser_snapshot',{page_id:page});assert(snap.text.includes('Stored WebCodex native 中文'));
  pass('Native browser supports real fill/click and observable results');
  const shot=await client.call('browser_screenshot',{page_id:page});assert.equal(shot.mime_type,'image/png');assert(shot.bytes>1000);
  pass('Native screenshot remains a bounded PNG artifact');
  await fs.writeFile(projectFile,JSON.stringify({userAgent:'WC-Changed'}));
  const rejected=await client.raw('browser_tabs');assert.equal(rejected.structuredContent.code,'configuration_changed');
  pass('Mid-session config change is rejected before another browser command');
  const closed=await client.call('browser_disconnect');assert.equal(closed.plugin_owned_browser_closed,true);assert.equal(closed.user_browser_closed,false);
  pass('Disconnect closes only the plugin-owned native browser');
  await client.call('browser_connect');const reconnected=await client.call('browser_open',{url});
  assert.notEqual(reconnected.page.page_id,page);
  const again=await client.call('browser_snapshot',{page_id:reconnected.page.page_id});assert(again.text.includes('Saved WebCodex native 中文'));
  await client.call('browser_disconnect');await client.stop();client=undefined;
  pass('Same provider reconnect safely obtains fresh page identities and the configured profile');
  const fileEnv={...env};delete fileEnv.AGENT_BROWSER_USER_AGENT;
  await create(fileEnv);await client.call('browser_connect');opened=await client.call('browser_open',{url});
  snap=await client.call('browser_snapshot',{page_id:opened.page.page_id});
  assert(snap.text.includes('Agent WC-Changed'));assert(snap.text.includes('Saved WebCodex native 中文'));
  pass('Fresh isolated session uses project override and preserves native persistent profile state');
  await client.call('browser_disconnect');await client.stop();client=undefined;
  const explicit=path.join(home,'explicit.json');
  await fs.writeFile(explicit,JSON.stringify({profile,executablePath:chromeExecutable,userAgent:'WC-Explicit',headed:false}));
  await create({...fileEnv,AGENT_BROWSER_CONFIG:explicit});status=await client.call('browser_status');
  assert.equal(status.configuration.configuration_source,'explicit_native_config');
  await client.call('browser_connect');opened=await client.call('browser_open',{url});
  snap=await client.call('browser_snapshot',{page_id:opened.page.page_id});assert(snap.text.includes('Agent WC-Explicit'));
  pass('AGENT_BROWSER_CONFIG uses the original explicit file, replacing default discovery');
  await client.call('browser_disconnect');await client.stop();client=undefined;
  await fs.writeFile(explicit,'{invalid');await create({...fileEnv,AGENT_BROWSER_CONFIG:explicit});
  const invalid=await client.raw('browser_connect');assert.equal(invalid.structuredContent.code,'invalid_native_config');
  pass('Malformed inherited config fails without a fallback browser');
  await fs.mkdir(path.join(root,'.local'),{recursive:true,mode:0o700});
  await fs.writeFile(path.join(root,'.local/native-report.json'),JSON.stringify({passed:true,tested_at:new Date().toISOString(),checks,synthetic_profile:true,daily_profile_tested:false},null,2)+'\n',{mode:0o600});
  console.log('PASS '+checks.length+' real native-inheritance checks');
} finally {
  if(client){await client.call('browser_disconnect').catch(()=>{});await client.stop();}
  server.closeAllConnections();await new Promise(resolve=>server.close(resolve));
  await fs.rm(home,{recursive:true,force:true,maxRetries:8,retryDelay:250});
}
