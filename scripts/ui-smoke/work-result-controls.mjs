import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { chromium } from 'playwright';
import { baseState } from '../../src/mcp_tests/work_result_app_fixture.mjs';

const output = new URL('../../artifacts/work-result-controls/', import.meta.url);
await mkdir(output, { recursive: true });
const html = await readFile(new URL('../../src/mcp_work_result_app.html', import.meta.url), 'utf8');
const server = createServer((request, response) => {
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end('<!doctype html><style>body{margin:0}iframe{display:block;width:100%;height:720px;border:0}</style>');
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const browser = await chromium.launch({ headless: true,
  ...(process.env.WEBCODEX_SMOKE_BROWSER ? { executablePath: process.env.WEBCODEX_SMOKE_BROWSER } : {}) });
const report = { fixture: 'synthetic loopback MCP Host', nativeHost: false, browser: browser.version(), cases: [] };

async function check(name, action) {
  const entry = { name, passed: false };
  report.cases.push(entry);
  try { await action(); entry.passed = true; console.log(`PASS ${name}`); }
  catch (error) { entry.error = error.message; console.error(`FAIL ${name}: ${error.message}`); }
}

async function mount(width, theme, empty = false, touch = false, displayMode = 'fullscreen') {
  const page = await browser.newPage({ viewport: { width, height: 720 }, hasTouch: touch, colorScheme: theme === 'light' ? 'dark' : 'light', reducedMotion: 'reduce' });
  page.setDefaultTimeout(5000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(({ html, state, theme, empty, displayMode }) => {
    const frame = document.createElement('iframe');
    frame.setAttribute('sandbox', 'allow-scripts allow-same-origin');
    window.fixture = { state, calls: [], unexpected: [] };
    const notify = (method, params) => frame.contentWindow.postMessage({ jsonrpc: '2.0', method, params }, '*');
    const result = output => ({ structuredContent: { success: true, output } });
    window.publishState = () => notify('ui/notifications/tool-result', result({ work_result: fixture.state }));
    addEventListener('message', event => {
      if (event.source !== frame.contentWindow || event.data?.jsonrpc !== '2.0') return;
      const message = event.data;
      const reply = value => frame.contentWindow.postMessage({ jsonrpc: '2.0', id: message.id, result: value }, '*');
      if (message.method === 'ui/initialize') reply({ protocolVersion: '2026-01-26', hostCapabilities: {}, hostContext: { theme, displayMode } });
      else if (message.method === 'ui/notifications/initialized') {
        notify('ui/notifications/tool-input', { arguments: empty ? {} : { project: state.project } });
        const value = result({ work_result: empty ? null : state });
        if (empty) value._meta = { 'webcodex/workResultThread': { empty: true, session_id: null } };
        notify('ui/notifications/tool-result', value);
      } else if (message.method === 'tools/call') {
        fixture.calls.push(message.params);
        if (message.params.name === 'get_work_result_state') {
          const files = message.params.arguments.files;
          if (!files) reply(result({ work_result: fixture.state }));
          else {
            const snapshot_id = 'wc_changes_snapshot_' + (fixture.state.workspace.files_total ? 'b' : 'c').repeat(32);
            reply(result({ work_result_files: files.path
              ? { project: state.project, snapshot_id, path: files.path, diff: '@@ -1 +1 @@\n-old\n+new\n', truncated: false }
              : { project: state.project, snapshot_id, offset: 0, next_offset: null, files_total: fixture.state.workspace.files_total, source_truncated: false,
                files: fixture.state.workspace.files.map(file => ({ path: file.path, kind: 'modified', additions: 4, deletions: 1 })) } }));
          }
        } else { fixture.unexpected.push(message.params.name); reply({ structuredContent: { success: false, error: 'Unexpected synthetic Host call' } }); }
      }
    });
    frame.srcdoc = html;
    document.body.append(frame);
  }, { html, state: structuredClone(baseState), theme, empty, displayMode });
  const frame = page.frames().find(candidate => candidate.parentFrame());
  await frame.locator(empty ? '#emptyState' : '#refresh:not(:disabled)').waitFor();
  return { page, frame, errors };
}

async function assertLayout(frame) {
  assert.ok(await frame.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'Page must fit horizontally');
}

try {
  for (const theme of ['light', 'dark']) for (const width of [320, 390, 800, 1440]) {
    const label = `${theme} ${width}px`;
    const { page, frame, errors } = await mount(width, theme);
    try {
      await check(`Header geometry (${label})`, async () => {
        assert.equal(await frame.evaluate(() => getComputedStyle(document.documentElement).colorScheme), theme);
        assert.equal(await frame.locator('#badge').innerText(), 'Waiting');
        // Appearance is a quiet footer preference; the header keeps only Refresh and status.
        assert.equal(await frame.locator('.app-header .card-appearance').count(), 0);
        assert.equal(await frame.locator('.app-footer > .card-appearance').count(), 1);
        const sizes = await frame.locator('#refresh, #badge').evaluateAll(nodes => nodes.map(node => ({ height: node.getBoundingClientRect().height, fontSize: getComputedStyle(node).fontSize })));
        assert.equal(sizes.length, 2);
        assert.ok(Math.max(...sizes.map(size => size.height)) - Math.min(...sizes.map(size => size.height)) <= 1, `Header heights must agree: ${JSON.stringify(sizes)}`);
        assert.equal(new Set(sizes.map(size => size.fontSize)).size, 1, 'Header control text sizes must agree');
        await assertLayout(frame);
      });
      await check(`Appearance pointer and keyboard (${label})`, async () => {
        const trigger = frame.locator('.card-appearance > summary');
        await trigger.click();
        const popup = frame.locator('.card-selection-preference');
        await popup.waitFor();
        const bounds = await popup.evaluate(node => {
          const box = node.getBoundingClientRect();
          return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, width: innerWidth, height: innerHeight };
        });
        await page.screenshot({ path: new URL(`appearance-open-${theme}-${width}.png`, output).pathname });
        assert.ok(bounds.left >= 0 && bounds.right <= bounds.width + 1 && bounds.top >= 0 && bounds.bottom <= bounds.height + 1, `Appearance must fit viewport: ${JSON.stringify(bounds)}`);
        const picker = frame.getByRole('combobox', { name: 'Selected controls' });
        await picker.click();
        const dropdown = await picker.evaluate(node => {
          const list = document.getElementById(node.getAttribute('aria-controls'));
          const box = list.getBoundingClientRect();
          return { visible: !list.hidden, fits: box.left >= 0 && box.right <= innerWidth + 1 && box.top >= 0 && box.bottom <= innerHeight + 1 };
        });
        assert.deepEqual(dropdown, { visible: true, fits: true }, 'Appearance dropdown also fits the viewport');
        await page.screenshot({ path: new URL(`appearance-dropdown-${theme}-${width}.png`, output).pathname });
        await picker.press('End');
        await picker.press('Enter');
        assert.equal(await frame.locator('#selectionPreference').inputValue(), 'monochrome');
        await page.screenshot({ path: new URL(`appearance-${theme}-${width}.png`, output).pathname });
        await trigger.click();
        await trigger.focus();
        await page.keyboard.press('Enter');
        await popup.waitFor();
        await page.keyboard.press('Tab');
        assert.equal(await picker.evaluate(node => node === document.activeElement), true, 'Appearance setting must be keyboard reachable');
        await picker.press('Enter');
        await picker.press('Home');
        await picker.press('Enter');
        assert.equal(await frame.locator('#selectionPreference').inputValue(), 'accent');
        await page.keyboard.press('Escape');
        assert.equal(await popup.isVisible(), false, 'Escape closes Appearance');
        assert.equal(await trigger.evaluate(node => node === document.activeElement), true, 'Escape restores Appearance focus');
        await trigger.click();
        await page.setViewportSize({ width: width === 320 ? 390 : 320, height: 720 });
        await frame.waitForFunction(() => {
          const box = document.querySelector('.card-selection-preference').getBoundingClientRect();
          return box.left >= 0 && box.right <= innerWidth + 1 && box.top >= 0 && box.bottom <= innerHeight + 1;
        });
        await page.setViewportSize({ width, height: 720 });
        await frame.locator('h1').click();
        assert.equal(await popup.isVisible(), false, 'Outside pointer closes Appearance');
        await assertLayout(frame);
      });
      if (await frame.locator('.card-appearance').evaluate(node => node.open)) await frame.locator('.card-appearance > summary').click();
      await check(`Pointer selection and keyboard focus (${label})`, async () => {
        const results = frame.getByRole('tab', { name: 'Results', exact: true });
        await results.click();
        assert.equal(await results.getAttribute('aria-selected'), 'true');
        const pointer = await results.evaluate(node => ({ focusVisible: node.matches(':focus-visible'), outline: getComputedStyle(node).outlineStyle }));
        assert.equal(pointer.focusVisible, false, 'Pointer tab activation must not show the keyboard focus ring');
        assert.equal(pointer.outline, 'none', 'Pointer-selected tab uses the selected underline');
        const plate = frame.locator('#viewTabs [data-selection-motion-plate]');
        assert.ok(await plate.isVisible(), 'Selected underline remains visible');
        await page.screenshot({ path: new URL(`results-pointer-${theme}-${width}.png`, output).pathname });
        await page.keyboard.press('Tab');
        await page.keyboard.press('Shift+Tab');
        assert.equal(await results.evaluate(node => node === document.activeElement && node.matches(':focus-visible')), true);
        assert.equal(await results.evaluate(node => getComputedStyle(node).outlineStyle), 'dashed', 'Keyboard focus has a distinct dashed shape from the selected underline');
        await page.screenshot({ path: new URL(`results-keyboard-${theme}-${width}.png`, output).pathname });
      });
      await check(`Populated and empty Results actions (${label})`, async () => {
        await frame.getByRole('tab', { name: 'Results', exact: true }).click();
        assert.equal(await frame.locator('#workspaceCollapse').isVisible(), false, 'Collapsed files need no collapse action');
        assert.equal(await frame.locator('#workspacePin').isEnabled(), true, 'Changed files retain pin action');
        await frame.locator('#workspaceFiles .file-toggle').first().click();
        await frame.locator('#workspaceFiles .diff').first().waitFor();
        assert.equal(await frame.locator('#workspaceCollapse').isVisible(), true, 'Expanded files retain collapse action');
        const actionBoxes = await frame.locator('#workspaceCollapse, #workspacePin').evaluateAll(nodes => nodes.map(node => {
          const box = node.getBoundingClientRect();
          return { width: box.width, height: box.height, rowWidth: node.parentElement.getBoundingClientRect().width };
        }));
        for (const box of actionBoxes) assert.ok(box.width < box.rowWidth - 8, `Result actions must keep content width: ${JSON.stringify(box)}`);
        await page.screenshot({ path: new URL(`results-populated-${theme}-${width}.png`, output).pathname });
        await frame.locator('#workspaceCollapse').click();
        assert.equal(await frame.locator('#workspaceFiles .file-toggle').first().getAttribute('aria-expanded'), 'false');
        assert.equal(await frame.locator('#workspaceFiles .file-toggle').first().evaluate(node => node === document.activeElement), true, 'Hiding Collapse returns focus to the file');
        await page.evaluate(() => {
          fixture.state.state_version = 'wr2_' + 'c'.repeat(64);
          Object.assign(fixture.state.workspace, { clean: true, files_total: 0, files: [], additions: 0, deletions: 0 });
          publishState();
        });
        await frame.locator('#workspaceStatus').filter({ hasText: 'No uncommitted changes' }).waitFor();
        await frame.locator('#workspaceCollapse').waitFor({ state: 'hidden' });
        await frame.locator('#workspacePin').waitFor({ state: 'hidden' });
        assert.equal(await frame.locator('#workspaceCollapse').isVisible(), false, 'Empty Results must hide collapse action');
        assert.equal(await frame.locator('#workspacePin').isVisible(), false, 'Empty Results must hide pin action');
        await assertLayout(frame);
        await page.screenshot({ path: new URL(`results-empty-${theme}-${width}.png`, output).pathname });
      });
      await check(`Synthetic Host integrity (${label})`, async () => {
        assert.deepEqual(errors, []);
        assert.deepEqual(await page.evaluate(() => fixture.unexpected), []);
      });
    } finally { await page.close(); }
    const empty = await mount(width, theme, true);
    try {
      await check(`First-open empty card (${label})`, async () => {
        assert.equal(await empty.frame.locator('#viewTabs').isVisible(), false);
        assert.equal(await empty.frame.locator('#workspaceCollapse').isVisible(), false);
        assert.equal(await empty.frame.locator('#workspacePin').isVisible(), false);
        assert.equal(await empty.frame.locator('#refresh').isDisabled(), true);
        assert.deepEqual(await empty.page.evaluate(() => fixture.calls), []);
        assert.deepEqual(empty.errors, []);
        await assertLayout(empty.frame);
        await empty.page.screenshot({ path: new URL(`first-open-${theme}-${width}.png`, output).pathname });
      });
    } finally { await empty.page.close(); }
  }
  for (const theme of ['light', 'dark']) for (const width of [320, 390, 800, 1440]) {
    const { page, frame, errors } = await mount(width, theme, false, false, 'inline');
    try {
      await check(`Inline card controls (${theme} ${width}px)`, async () => {
        await frame.locator('.card-appearance > summary').click();
        await frame.waitForFunction(() => {
          const box = document.querySelector('.card-selection-preference').getBoundingClientRect();
          return box.left >= 0 && box.right <= innerWidth + 1 && box.top >= 0 && box.bottom <= innerHeight + 1;
        });
        await frame.locator('h1').click();
        await frame.getByRole('tab', { name: 'Results', exact: true }).click();
        assert.equal(await frame.getByRole('tab', { name: 'Results', exact: true }).getAttribute('aria-selected'), 'true');
        assert.equal(await frame.locator('#workspaceCollapse').isVisible(), false);
        assert.equal(await frame.locator('#workspacePin').isVisible(), true);
        await assertLayout(frame);
        assert.deepEqual(errors, []);
        await page.screenshot({ path: new URL(`results-inline-${theme}-${width}.png`, output).pathname });
      });
    } finally { await page.close(); }
  }
  for (const theme of ['light', 'dark']) {
    const { page, frame, errors } = await mount(390, theme, false, true);
    try {
      await check(`Coarse pointer controls (${theme} 390px)`, async () => {
        assert.equal(await frame.evaluate(() => matchMedia('(pointer:coarse)').matches), true);
        const heights = await frame.locator('.app-footer > .card-appearance > summary, #refresh, #badge').evaluateAll(nodes => nodes.map(node => node.getBoundingClientRect().height));
        assert.ok(heights.every(height => height >= 44), `Touch controls need 44px height: ${JSON.stringify(heights)}`);
        assert.ok(Math.max(...heights) - Math.min(...heights) <= 1, 'Touch control heights must agree');
        await frame.locator('.card-appearance > summary').tap();
        const fits = await frame.locator('.card-selection-preference').evaluate(node => {
          const box = node.getBoundingClientRect();
          return box.left >= 0 && box.right <= innerWidth + 1 && box.bottom <= innerHeight + 1;
        });
        assert.equal(fits, true);
        await assertLayout(frame);
        assert.deepEqual(errors, []);
        await page.screenshot({ path: new URL(`appearance-touch-${theme}-390.png`, output).pathname });
      });
    } finally { await page.close(); }
  }
} finally {
  await writeFile(new URL('report.json', output), JSON.stringify(report, null, 2) + '\n');
  await browser.close();
  server.close();
}
if (report.cases.some(entry => !entry.passed)) process.exitCode = 1;
