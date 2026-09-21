import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { navigationUrl, localConnection, displayUrl, loadConfig } from '../dist/config.js';
import { BrowserFault, UncertainAction } from '../dist/errors.js';
import { boundedProcess, NativeBackend, childEnvironment, decodeBatch } from '../dist/backend.js';
import { BrowserController } from '../dist/browser.js';
import { FakeBackend } from './fake-backend.mjs';

const code = name => e => e instanceof BrowserFault && e.code === name;
async function setup(root) { const backend = new FakeBackend(); const browser = new BrowserController(backend, root); const connected = await browser.connect(); return { backend, browser, page: connected.page.page_id }; }

for (const url of ['file:///etc/passwd', 'javascript:alert(1)', 'data:text/html,x', 'chrome://settings', 'chrome-extension://id/', 'https://u:p@example.com', 'https://example.com\n', 'https://a.test/ a']) {
  test('reject unsafe URL: ' + JSON.stringify(url), () => assert.throws(() => navigationUrl(url), code('invalid_url')));
}
test('normalizes absolute HTTP(S) URLs', () => assert.equal(navigationUrl('https://example.com'), 'https://example.com/'));
test('rejects UTF-8 oversized URLs', () => assert.throws(() => navigationUrl('https://a.test/' + '中'.repeat(3000)), code('invalid_url')));
test('metadata removes query/fragment/credentials', () => assert.equal(displayUrl('https://u:p@example.test/a?q=secret#token'), 'https://example.test/a'));
test('connection supports explicit local targets only', () => {
  for (const value of ['auto', '9222', 'ws://127.0.0.1:9222/devtools/browser/abc']) assert.equal(localConnection(value), value);
  for (const value of ['0', '65536', 'https://remote.test', 'ws://remote.test:9222/devtools/browser/a', 'ws://localhost:9222/devtools/browser/a', 'ws://127.0.0.1:9222/devtools/browser/a?token=x']) assert.throws(() => localConnection(value), code('invalid_config'));
});
test('default config inherits natively and unknown argv fails', () => { assert.deepEqual(loadConfig([]), { executable: 'agent-browser', connection: 'native' }); assert.throws(() => loadConfig(['--cdp', '9222']), code('invalid_config')); });
test('child environment never inherits credentials, proxies, browser overrides or init scripts', () => {
  const env = childEnvironment({ HOME: '/home/test', PATH: '/bin', OPENAI_API_KEY: 'secret', WEBCODEX_TOKEN: 'secret', AGENT_BROWSER_PROVIDER: 'cloud', AGENT_BROWSER_INIT_SCRIPTS: 'evil.js', HTTPS_PROXY: 'evil' });
  assert.deepEqual(Object.keys(env).sort(), ['AGENT_BROWSER_DEFAULT_TIMEOUT', 'HOME', 'NO_PROXY', 'PATH'].sort());
});
test('strict one-command batch decoding', () => {
  assert.equal(decodeBatch('[{"success":true,"result":{}}]').success, true);
  for (const raw of ['not json', '[]', '[{},{}]', '[{"result":{}}]']) assert.throws(() => decodeBatch(raw), code('backend_protocol'));
});
test('bounded subprocess preserves exact stdin', async () => {
  const p = await boundedProcess(process.execPath, ['-e', 'process.stdin.pipe(process.stdout)'], '--config=literal 中文', childEnvironment(process.env), 2000, 4096);
  assert.equal(p.stdout, '--config=literal 中文'); assert.equal(p.exitCode, 0);
});
test('bounded subprocess enforces timeout', async () => assert.rejects(boundedProcess(process.execPath, ['-e', 'setTimeout(()=>{},10000)'], '', childEnvironment(process.env), 100, 4096), code('backend_timeout')));
test('bounded subprocess enforces output limit', async () => assert.rejects(boundedProcess(process.execPath, ['-e', 'process.stdout.write("x".repeat(100000))'], '', childEnvironment(process.env), 2000, 100), code('backend_output_limit')));
test('transport sends literal values in stdin, never argv, and has isolated sessions', async () => {
  let observed;
  const execute = async (...args) => { observed = args; return { stdout: '[{"success":true,"result":{}}]', exitCode: 0 }; };
  const a = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, execute);
  const b = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, execute);
  await a.run(['fill', '@e1', '--config=/evil 中文'], true);
  assert.notEqual(a.session, b.session); assert(a.session.length <= 16, 'macOS UNIX socket path components must stay short'); assert(!observed[1].includes('--config=/evil 中文'));
  assert.deepEqual(JSON.parse(observed[2]), [['fill', '@e1', '--config=/evil 中文']]);
  assert(observed[1].includes('--auto-connect'));
});
test('unknown effect response poisons provider and never retries', async () => {
  let calls = 0;
  const backend = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, async () => { calls++; return { stdout: 'broken', exitCode: 0 }; });
  await assert.rejects(backend.run(['click', '@e1'], true), UncertainAction);
  await assert.rejects(backend.run(['click', '@e1'], true), UncertainAction);
  await backend.shutdown(); assert.equal(calls, 1);
});
test('effectful page disappearance remains outcome-unknown while observation is actionable', async () => {
  const response = { stdout: '[{"success":false,"error":"tab not found"}]', exitCode: 1 };
  let effectCalls = 0;
  const effect = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, async () => { effectCalls++; return response; });
  await assert.rejects(effect.run(['click', '@e1'], true), UncertainAction);
  await assert.rejects(effect.run(['click', '@e1'], true), UncertainAction);
  assert.equal(effectCalls, 1, 'uncertain effect must poison the provider and never retry');

  const observation = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, async () => response);
  await assert.rejects(observation.run(['tab', 'list']), code('page_gone'));
});
test('known missing Chrome is actionable, not silent fallback', async () => {
  const backend = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, async () => ({ stdout: '{"success":false,"error":"No running Chrome instance found. Launch Chrome with --remote-debugging-port or use --cdp."}', exitCode: 1 }));
  await assert.rejects(backend.run(['tab', 'list'], true), code('chrome_not_authorized'));
});
test('backend enforces tested version', async () => {
  const backend = new NativeBackend({ executable: 'agent-browser', connection: 'auto' }, async () => ({ stdout: 'agent-browser 0.1.0', exitCode: 0 }));
  await assert.rejects(backend.version(), code('unsupported_backend_version'));
});
test('status does not attach and tabs require explicit connection', async () => {
  const backend = new FakeBackend(), browser = new BrowserController(backend);
  assert.equal((await browser.status()).connection_state, 'not_connected'); assert.equal(backend.calls.length, 0);
  await assert.rejects(browser.tabs(), code('not_connected')); assert.equal(backend.calls.length, 0);
});
test('connect owns only its new blank tab; existing metadata sanitized', async () => {
  const { browser, page } = await setup(); const result = await browser.tabs();
  assert(result.pages.find(p => p.page_id === page).created_by_plugin);
  const existing = result.pages.find(p => p.title === 'Existing tab'); assert(!existing.created_by_plugin); assert.equal(existing.url, 'https://example.test/private');
});
test('open creates a new tab and rejects unsafe navigation before dispatch', async () => {
  const { backend, browser } = await setup(); const before = backend.calls.length;
  await assert.rejects(browser.open('file:///private'), code('invalid_url')); assert.equal(backend.calls.length, before);
  const opened = await browser.open('https://new.test'); assert(opened.page.created_by_plugin); assert.equal(backend.tabs[1].url, 'https://example.test/private?q=secret#fragment');
});
test('snapshot has opaque IDs, not native refs or targets', async () => {
  const { browser, page } = await setup(); const snap = await browser.snapshot(page);
  assert(!snap.text.includes('ref=e')); assert(snap.text.includes('element_id=element_')); assert(!JSON.stringify(snap).includes('AAAA'));
});
test('full snapshots do not use compact mode that hides result text', async () => {
  const { backend, browser, page } = await setup();
  await browser.snapshot(page);
  const full = backend.calls.find(c => c.tokens[0] === 'snapshot').tokens;
  assert(!full.includes('-c')); assert(!full.includes('-i'));
  await browser.snapshot(page, true);
  const interactive = backend.calls.filter(c => c.tokens[0] === 'snapshot').at(-1).tokens;
  assert(interactive.includes('-i')); assert(interactive.includes('-c'));
});
test('adaptive snapshots compact large pages but explicit false still forces full text', async () => {
  const { backend, browser, page } = await setup();
  backend.tree = 'Static context '.repeat(700) + '\n- button "Do thing" [ref=e2]';
  backend.refs = { e2: { role: 'button', name: 'Do thing' } };
  backend.interactiveTree = '- button "Do thing" [ref=e2]';
  backend.interactiveRefs = { e2: { role: 'button', name: 'Do thing' } };
  const adaptive = await browser.snapshot(page);
  assert.equal(adaptive.snapshot_mode, 'interactive_auto'); assert.equal(adaptive.auto_compacted, true);
  assert(adaptive.text.length < 200); assert.match(adaptive.note, /automatically reduced/);
  const calls = backend.calls.filter(c => c.tokens[0] === 'snapshot');
  assert.equal(calls.length, 2); assert(!calls[0].tokens.includes('-i')); assert(calls[1].tokens.includes('-i'));
  const forced = await browser.snapshot(page, false);
  assert.equal(forced.snapshot_mode, 'full'); assert.equal(forced.auto_compacted, false); assert(forced.text.length > 8000);
});
test('fill consumes snapshot and values remain literal', async () => {
  const { backend, browser, page } = await setup(); const snap = await browser.snapshot(page);
  const textbox = snap.elements.find(e => e.role === 'textbox');
  await browser.elementAction('fill', page, snap.snapshot_id, textbox.element_id, '--config=x 中文');
  assert.deepEqual(backend.calls.at(-1).tokens, ['fill', '@e1', '--config=x 中文']);
  await assert.rejects(browser.elementAction('fill', page, snap.snapshot_id, textbox.element_id, 'again'), code('unknown_element'));
});
test('refuses filling non-input nodes even if backend would claim success', async () => {
  const { backend, browser, page } = await setup(); const snap = await browser.snapshot(page); const n = backend.calls.length;
  await assert.rejects(browser.elementAction('fill', page, snap.snapshot_id, snap.elements.find(e => e.role === 'heading').element_id, 'x'), code('not_text_input'));
  assert.equal(backend.calls.length, n);
});
test('select option requires an observed select and consumes the snapshot', async () => {
  const { backend, browser, page } = await setup(); let snap = await browser.snapshot(page);
  const select = snap.elements.find(e => e.role === 'combobox');
  await browser.selectOption(page, snap.snapshot_id, select.element_id, 'pro');
  assert.deepEqual(backend.calls.at(-1).tokens, ['select', '@e5', 'pro']);
  await assert.rejects(browser.selectOption(page, snap.snapshot_id, select.element_id, 'starter'), code('unknown_element'));
  snap = await browser.snapshot(page); const heading = snap.elements.find(e => e.role === 'heading'); const n = backend.calls.length;
  await assert.rejects(browser.selectOption(page, snap.snapshot_id, heading.element_id, 'x'), code('not_select')); assert.equal(backend.calls.length, n);
});
test('set checked is boolean, checkbox-only, and uses explicit check/uncheck commands', async () => {
  const { backend, browser, page } = await setup(); let snap = await browser.snapshot(page);
  const box = snap.elements.find(e => e.role === 'checkbox');
  await browser.setChecked(page, snap.snapshot_id, box.element_id, true);
  assert.deepEqual(backend.calls.at(-1).tokens, ['check', '@e4']);
  snap = await browser.snapshot(page); const nextBox = snap.elements.find(e => e.role === 'checkbox');
  await browser.setChecked(page, snap.snapshot_id, nextBox.element_id, false);
  assert.deepEqual(backend.calls.at(-1).tokens, ['uncheck', '@e4']);
  snap = await browser.snapshot(page); const text = snap.elements.find(e => e.role === 'textbox'); const n = backend.calls.length;
  await assert.rejects(browser.setChecked(page, snap.snapshot_id, text.element_id, true), code('not_checkbox')); assert.equal(backend.calls.length, n);
  await assert.rejects(browser.setChecked(page, snap.snapshot_id, nextBox.element_id, 'true'), code('invalid_argument'));
});
test('reload targets the selected page and invalidates old snapshot handles', async () => {
  const { backend, browser, page } = await setup(); const snap = await browser.snapshot(page);
  await browser.reload(page); assert.deepEqual(backend.calls.at(-1).tokens, ['reload']);
  await assert.rejects(browser.elementAction('click', page, snap.snapshot_id, snap.elements.find(e => e.role === 'button').element_id), code('unknown_element'));
});
test('same-URL document reload fences old elements', async () => {
  const { backend, browser, page } = await setup(); const snap = await browser.snapshot(page); backend.timeOrigin++;
  await assert.rejects(browser.elementAction('click', page, snap.snapshot_id, snap.elements[1].element_id), code('stale_snapshot'));
  assert(!backend.calls.some(c => c.tokens[0] === 'click'));
});
test('semantic changes fence old elements', async () => {
  const { backend, browser, page } = await setup(); const snap = await browser.snapshot(page); backend.tree += '\n- text "Changed"';
  await assert.rejects(browser.elementAction('click', page, snap.snapshot_id, snap.elements[1].element_id), code('stale_snapshot'));
  assert(!backend.calls.some(c => c.tokens[0] === 'click'));
});
test('newer snapshot invalidates previous snapshot', async () => {
  const { browser, page } = await setup(); const old = await browser.snapshot(page); const fresh = await browser.snapshot(page);
  await assert.rejects(browser.press(page, old.snapshot_id, 'Enter'), code('stale_snapshot')); assert.notEqual(old.snapshot_id, fresh.snapshot_id);
});
test('closed page never silently retargets another tab', async () => {
  const { backend, browser, page } = await setup(); backend.tabs = backend.tabs.filter(t => t.targetId !== 'AAAA');
  await assert.rejects(browser.snapshot(page), code('page_gone')); assert(!backend.calls.some(c => c.tokens[0] === 'snapshot'));
});
test('cannot close pre-existing user tabs', async () => {
  const { backend, browser } = await setup(); const existing = (await browser.tabs()).pages.find(p => p.title === 'Existing tab'); const count = backend.calls.length;
  await assert.rejects(browser.closePage(existing.page_id), code('not_owned')); assert.equal(backend.calls.length, count);
});
test('depth, scroll and keys have enforced bounds', async () => {
  const { backend, browser, page } = await setup(); const n = backend.calls.length;
  await assert.rejects(browser.snapshot(page, false, 21), code('invalid_argument'));
  await assert.rejects(browser.scroll(page, 'down', 5000), code('invalid_argument'));
  await assert.rejects(browser.press(page, 'x', 'Meta+q'), code('invalid_key'));
  assert.equal(backend.calls.length, n);
});
test('snapshot output is bounded without silent ref truncation', async () => {
  const { backend, browser, page } = await setup(); backend.tree = 'x'.repeat(17000);
  await assert.rejects(browser.snapshot(page), code('snapshot_limit'));
});
test('disconnect detaches without auto-connecting or closing Chrome', async () => {
  const { backend, browser } = await setup(); const result = await browser.disconnect();
  assert.equal(result.chrome_closed, false); assert.deepEqual(backend.calls.at(-1), { tokens: ['close'], effect: true, attach: false });
});
test('screenshot is a bounded owner-only artifact with digest', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'wc-artifact-test-'));
  try { const { browser, page } = await setup(dir); const shot = await browser.screenshot(page); assert.equal(shot.width, 800); assert.equal(shot.height, 600); assert.equal(shot.sha256.length, 64); assert.equal(shot.delivery, 'provider_local_artifact'); assert(!shot.artifact_path.includes('..')); assert.match(shot.note, /not automatically readable through WebCodex Project artifact tools/); const stat = await fs.stat(path.join(dir, shot.artifact_path)); if (process.platform !== 'win32') assert.equal(stat.mode & 0o777, 0o600); }
  finally { await fs.rm(dir, { recursive: true, force: true }); }
});
test('screenshot refuses symlink artifact directory', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'wc-symlink-test-'));
  try { await fs.mkdir(path.join(dir, 'outside')); await fs.symlink(path.join(dir, 'outside'), path.join(dir, 'artifacts'), 'dir'); const { browser, page } = await setup(dir); await assert.rejects(browser.screenshot(page), code('artifact_path')); assert.deepEqual(await fs.readdir(path.join(dir, 'outside')), []); }
  finally { await fs.rm(dir, { recursive: true, force: true }); }
});
