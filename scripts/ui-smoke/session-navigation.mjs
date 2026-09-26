import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';
import { webResponse } from './fixtures.mjs';

const output = new URL('../../artifacts/projects-runtime/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
const checks = [];
try {
  for (const width of [1440, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
    page.on('pageerror', error => errors.push(error.message));
    const targetKey = 'b'.repeat(64);
    let sessionId;
    await page.route('**/api/runtime-console/workflow-session', async route => {
      const data = structuredClone(webResponse('workflow-session', route.request().postDataJSON()));
      sessionId = data.session_id;
      data.linked_windows = [{ client_window_key: targetKey, source: 'openai-session', first_linked_at_ms: 1, last_linked_at_ms: 2, last_seen_at_ms: 2, relations: ['recording'], relation_count: 1, recorder_gap_count: 0 }];
      await route.fulfill({ json: data });
    });
    await page.route('**/api/runtime-console/window', async route => {
      const data = structuredClone(webResponse('window', route.request().postDataJSON()));
      data.client_window_key = route.request().postDataJSON().client_window_key;
      if (sessionId) data.activity[0].workflow_sessions = [{ workflow_session_id: sessionId, project: 'agent:fixture-runner:alpha', relation: 'recording' }];
      await route.fulfill({ json: data });
    });
    await page.addInitScript(() => {
      if (!localStorage.getItem('webcodex.runtime.v2.view.v1')) localStorage.setItem('webcodex.runtime.v2.view.v1', 'projects');
    });
    await page.goto(fixture.url + '/runtime/');
    await page.getByRole('button', { name: /Fixture workspace review/ }).click();
    await page.getByTestId('window-primary-workbench').waitFor();
    await page.getByRole('combobox', { name: 'Session filter' }).waitFor();
    assert.equal(await page.getByRole('combobox', { name: 'Session filter' }).inputValue(), sessionId);
    await page.getByText('read_files', { exact: true }).waitFor();
    const selectedRow = page.getByTestId('work-window-row-' + targetKey);
    assert.equal(await selectedRow.getAttribute('aria-current'), 'true');
    assert.equal(await page.locator('.window-work-row[aria-current="true"]').count(), 1);
    assert.equal(await page.getByTestId('current-window-identity').getAttribute('title'), targetKey);
    assert.equal(await selectedRow.count(), 1);
    assert.equal(await page.getByRole('searchbox', { name: 'Search Sessions' }).count(), 0);
    const bounds = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth }));
    assert(bounds.body <= bounds.width + 1, JSON.stringify(bounds));
    await page.screenshot({ path: new URL(`session-navigation-${width}.png`, output).pathname, fullPage: true });
    if (width === 1440) {
      // A later inventory includes the target below a long list of active Windows.
      await page.route('**/api/runtime-console/windows', async route => {
        const data = structuredClone(webResponse('windows', {}));
        const row = data.windows[0];
        data.windows = Array.from({ length: 50 }, (_, index) => ({ ...row, client_window_key: index.toString(16).padStart(64, '0'), active_count: 1 }));
        data.windows.push({ ...row, client_window_key: targetKey, active_count: 0 });
        data.total = data.returned = data.windows.length;
        await route.fulfill({ json: data });
      });
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await page.waitForFunction(key => {
        const row = document.querySelector('[data-testid="work-window-row-' + key + '"]');
        return row && !row.closest('.window-current-selection');
      }, targetKey);
      assert.equal(await selectedRow.count(), 1);
      assert.equal(await selectedRow.getAttribute('aria-current'), 'true');
      const position = await selectedRow.evaluate(row => {
        const bounds = row.getBoundingClientRect();
        const list = row.closest('.work-list-scroll').getBoundingClientRect();
        return { top: bounds.top, bottom: bounds.bottom, listTop: list.top, listBottom: list.bottom };
      });
      assert(position.top >= position.listTop - 1 && position.bottom <= position.listBottom + 1, JSON.stringify(position));
    }
    // Revisiting Work and refreshing must keep the same Window-based interface.
    const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
    await nav.getByRole('button', { name: /^Work/ }).click();
    await page.getByTestId('window-primary-workbench').waitFor();
    await page.reload();
    await page.getByTestId('window-primary-workbench').waitFor();
    assert.equal(await page.getByRole('searchbox', { name: 'Search Sessions' }).count(), 0);
    checks.push({ width, linkedWindowOutsideInventory: true, sessionFocused: true, sidebarMatchesDetail: true, sameWorkbenchAfterReload: true, overflow: false });
    await page.unrouteAll({ behavior: 'wait' });
    await page.close();
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(new URL('session-navigation-report.json', output), JSON.stringify({ fixture: true, nativeBackend: false, checks, errors }, null, 2));
  console.log(JSON.stringify({ checks, errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
