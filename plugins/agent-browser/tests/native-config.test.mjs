import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { inspectNativeConfig, inheritedEnvironment } from '../dist/native-config.js';
import { NativeBackend } from '../dist/backend.js';
import { BrowserController } from '../dist/browser.js';
import { FakeBackend } from './fake-backend.mjs';
import { BrowserFault } from '../dist/errors.js';
const code = name => e => e instanceof BrowserFault && e.code === name;
function fixture(t) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'wc-config-'));
  const cwd = path.join(home, 'project'); fs.mkdirSync(cwd); fs.mkdirSync(path.join(home, '.agent-browser'));
  const user = path.join(home, '.agent-browser/config.json'), project = path.join(cwd, 'agent-browser.json');
  t.after(() => fs.rmSync(home, {recursive:true, force:true}));
  return {home, cwd, user, project, env:{HOME:home, PATH:process.env.PATH}, config:{executable:'agent-browser', connection:'native', workingDirectory:cwd}};
}
const put = (file, value) => fs.writeFileSync(file, JSON.stringify(value));
const batch = result => ({exitCode:0, stdout:JSON.stringify([{success:true,result}])});
const info = (active = true) => ({exitCode:0, stdout:JSON.stringify({success:true,data:{active,runtime:{launchHash:42}}})});

test('native environment inherits browser preferences and proxies but not WebCodex/account credentials', () => {
  const env = inheritedEnvironment({HOME:'/home/user', PATH:'/bin', AGENT_BROWSER_PROFILE:'default', AGENT_BROWSER_CONFIG:'/tmp/native.json',
    AGENT_BROWSER_PROXY_PASSWORD:'local-only', HTTPS_PROXY:'http://proxy', NO_PROXY:'internal',
    WEBCODEX_TOKEN:'secret', OPENAI_API_KEY:'secret', NODE_OPTIONS:'--import evil', AGENT_BROWSER_SESSION:'default',
    AGENT_BROWSER_NAMESPACE:'default', AGENT_BROWSER_SOCKET_DIR:'/tmp/other', AGENT_BROWSER_INIT_SCRIPTS:'/trusted/init.js'}, true);
  assert.equal(env.AGENT_BROWSER_PROFILE,'default'); assert.equal(env.AGENT_BROWSER_PROXY_PASSWORD,'local-only');
  assert.equal(env.HTTPS_PROXY,'http://proxy'); assert.equal(env.NO_PROXY,'internal');
  for(const key of ['WEBCODEX_TOKEN','OPENAI_API_KEY','NODE_OPTIONS','AGENT_BROWSER_SESSION','AGENT_BROWSER_NAMESPACE','AGENT_BROWSER_SOCKET_DIR']) assert.equal(env[key],undefined);
  assert.equal(env.AGENT_BROWSER_INIT_SCRIPTS,'/trusted/init.js');
});
test('native defaults use user/project discovery without inventing a profile', t => {
  const f=fixture(t), result=inspectNativeConfig(f.config,f.env,f.cwd);
  assert.equal(result.ownership,'plugin'); assert.equal(result.summary.profile_configured,false);
  assert.equal(result.summary.user_config_present,false); assert.equal(result.summary.project_config_present,false);
});
test('profile diagnostics respect native file then environment precedence', t => {
  const f=fixture(t); put(f.user,{profile:'Default',userAgent:'User'}); put(f.project,{profile:'Work'});
  assert.equal(inspectNativeConfig(f.config,f.env,f.cwd).summary.profile_name,'Work');
  f.env.AGENT_BROWSER_PROFILE='default'; const r=inspectNativeConfig(f.config,f.env,f.cwd);
  assert.equal(r.summary.profile_name,'default'); assert(r.summary.inherited_environment_keys.includes('AGENT_BROWSER_PROFILE'));
});
test('explicit config replaces both discovered files; never merges with them', t => {
  const f=fixture(t); put(f.user,{provider:'remote'}); put(f.project,{profile:'Work'});
  const file=path.join(f.cwd,'explicit.json'); put(file,{profile:'Default'});
  assert.equal(inspectNativeConfig({...f.config,agentBrowserConfig:file},f.env,f.cwd).summary.profile_name,'Default');
  f.env.AGENT_BROWSER_CONFIG='explicit.json'; assert.equal(inspectNativeConfig(f.config,f.env,f.cwd).summary.configuration_source,'explicit_native_config');
});
test('native profile paths remain private', t => {
  const f=fixture(t); put(f.user,{profile:'/private/profile-path'}); const r=inspectNativeConfig(f.config,f.env,f.cwd);
  assert.equal(r.summary.profile_kind,'persistent_directory'); assert(!JSON.stringify(r.summary).includes('/private'));
});
for(const [value, expected] of [[{autoConnect:true},'external'],[{cdp:'9222'},'external'],[{profile:'Default'},'plugin']]) {
  test('classify native lifecycle '+JSON.stringify(value), t => {const f=fixture(t);put(f.user,value);assert.equal(inspectNativeConfig(f.config,f.env,f.cwd).ownership,expected);});
}
for(const value of [{profile:'Default',autoConnect:true},{profile:'Default',cdp:'9222'},{autoConnect:true,cdp:'9222'}]) {
  test('reject conflicting native connection '+JSON.stringify(value), t=>{const f=fixture(t);put(f.user,value);assert.throws(()=>inspectNativeConfig(f.config,f.env,f.cwd),code('conflicting_native_config'));});
}
for(const value of [{provider:'browserbase'},{engine:'lightpanda'},{plugins:[{capabilities:['launch.mutate']}]}]) {
  test('reject unsupported backend rather than silently discard '+JSON.stringify(value),t=>{const f=fixture(t);put(f.user,value);assert.throws(()=>inspectNativeConfig(f.config,f.env,f.cwd),code('unsupported_native_backend'));});
}
for (const setting of [{restore:'default'},{sessionName:'shared'}]) {
  test('shared restore identity fails explicitly '+JSON.stringify(setting), t=>{const f=fixture(t);put(f.user,setting);assert.throws(()=>inspectNativeConfig(f.config,f.env,f.cwd),code('unsupported_native_restore'));});
}
test('reject remote debugging endpoint inherited from config', t=>{const f=fixture(t);put(f.user,{cdp:'ws://remote.example:9222/devtools/browser/id'});assert.throws(()=>inspectNativeConfig(f.config,f.env,f.cwd),code('invalid_config'));});
test('malformed discovered JSON fails closed instead of falling back', t=>{const f=fixture(t);fs.writeFileSync(f.user,'{broken');assert.throws(()=>inspectNativeConfig(f.config,f.env,f.cwd),code('invalid_native_config'));});
test('missing explicit config fails closed', t=>{const f=fixture(t);assert.throws(()=>inspectNativeConfig({...f.config,agentBrowserConfig:path.join(f.home,'missing')},f.env,f.cwd),code('invalid_native_config'));});
test('changed file changes the fingerprint', t=>{const f=fixture(t);put(f.user,{profile:'Default'});const a=inspectNativeConfig(f.config,f.env,f.cwd);put(f.user,{profile:'Work'});assert.notEqual(a.fingerprint,inspectNativeConfig(f.config,f.env,f.cwd).fingerprint);});
test('native CLI receives original config discovery/env/cwd, not a merged replacement',async t=>{
  const f=fixture(t);put(f.user,{profile:'default'});let observed;
  const b=new NativeBackend(f.config,async(...args)=>{observed=args;return batch({});},f.env);
  await b.run(['tab','list'],true);
  assert(!observed[1].includes('--config'));assert(!observed[1].includes('--auto-connect'));assert(!observed[1].includes('--profile'));
  assert.equal(observed[6],f.cwd);assert.equal(observed[3].HOME,f.home);assert.equal(b.ownership,'plugin');
});
test('explicit native file is passed unchanged to CLI',async t=>{
  const f=fixture(t), file=path.join(f.cwd,'exact.json');put(file,{profile:'Default'});let observed;
  const b=new NativeBackend({...f.config,agentBrowserConfig:file},async(...args)=>{observed=args;return batch({});},f.env);
  await b.run(['tab','list'],true);assert.equal(observed[1][observed[1].indexOf('--config')+1],file);
});
test('config changes block connected calls but never block scoped cleanup',async t=>{
  const f=fixture(t);put(f.user,{profile:'Default'});const calls=[];
  const b=new NativeBackend(f.config,async(...args)=>{calls.push(args);return args[1].includes('info')?info():batch({lifecycle:{effectiveLaunch:{launchHash:42}}});},f.env);
  await b.run(['tab','list'],true);put(f.user,{profile:'Work'});const n=calls.length;
  await assert.rejects(b.run(['click','@e1'],true),code('configuration_changed'));assert.equal(calls.length,n);
  await b.run(['close'],true,false);const last=calls.at(-1);assert(last[1].includes('--config'));assert.equal(last[3].AGENT_BROWSER_PROFILE,undefined);
});
test('lost session is observed before a command and never auto-relaunched',async t=>{
  const f=fixture(t);const calls=[];
  const b=new NativeBackend(f.config,async(...args)=>{calls.push(args);return args[1].includes('info')?info(false):batch({});},f.env);
  await b.run(['tab','list'],true);await assert.rejects(b.run(['tab','new','https://example.test'],true),code('browser_session_lost'));
  assert.equal(calls.length,2);assert(calls[1][1].includes('info'));
});
test('native disconnect reports closure only for plugin-owned browser',async()=>{
  const fake=new FakeBackend();fake.mode='native';fake.ownership='plugin';const b=new BrowserController(fake);await b.connect();
  const result=await b.disconnect();assert.equal(result.plugin_owned_browser_closed,true);assert.equal(result.user_browser_closed,false);
});
test('native startup tabs are preserved and a new blank is created',async()=>{
  const fake=new FakeBackend();fake.mode='native';fake.ownership='plugin';fake.tabs[0].url='https://existing.example/';
  const b=new BrowserController(fake);const c=await b.connect();assert.equal(c.page.url,'about:blank');assert.equal(fake.tabs[0].url,'https://existing.example/');
});
