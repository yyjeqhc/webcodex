import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { webResponse, windowSummary } from './fixtures.mjs';

const output = new URL('../../artifacts/window-board/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
try {
  for (const [width, theme] of [[1440, 'light'], [1024, 'light'], [390, 'light'], [390, 'dark']]) {
    const page = await browser.newPage({ viewport: { width, height: 960 }, reducedMotion: 'reduce' });
    let label = null;
    const rows = Array.from({ length: 16 }, (_, index) => ({ ...windowSummary,
      client_window_key: index === 6 ? windowSummary.client_window_key : index.toString(16).padStart(4, '0').padEnd(64, '0'),
      source: index % 3 === 0 ? 'codex-thread' : index % 3 === 1 ? 'openai-session' : 'client-window-header',
      active_count: index < 7 ? 1 : 0,
      last_activity_name: index % 3 === 0 ? 'read_file' : 'run_process',
      first_seen_at_ms: windowSummary.last_seen_at_ms - 2 * 86_400_000,
      last_meaningful_activity_at_ms: windowSummary.last_seen_at_ms - (index < 10 ? index * 60_000 : index < 13 ? 3_600_000 : 86_400_000),
      label: index === 2 ? 'Documentation review' : null,
    }));
    await page.route('**/api/runtime-console/*', async route => {
      const name = new URL(route.request().url()).pathname.split('/').pop();
      const body = route.request().postDataJSON();
      let response = name === 'window-label' ? null : webResponse(name, body);
      if (name === 'window-label') { label = body.label; response = { client_window_key: body.client_window_key, label }; }
      if (name === 'window') response = { ...response, label };
      if (name === 'windows') response = { ...response, total: rows.length, returned: rows.length,
        windows: rows.map(row => {
          const updated = { ...row, label: row.client_window_key === windowSummary.client_window_key ? label : row.label };
          if (body.projection === 'liveness') delete updated.label;
          return updated;
        }) };
      await route.fulfill({ json: response });
    });
    page.on('pageerror', error => report.errors.push(error.message));
    await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
    await page.goto(fixture.url + '/runtime/');
    const row = page.getByTestId('work-window-row-' + '4700'.repeat(16));
    await row.waitFor();
    assert.equal(await page.locator('.window-board-detail').count(), 0);
    assert.equal(await page.locator('.window-board-chips > button').evaluateAll(buttons => buttons.every(button => button.scrollWidth <= button.clientWidth + 1)), true);
    await page.screenshot({ path: new URL(`${width}-${theme}-overview.png`, output).pathname });
    await row.scrollIntoViewIfNeeded();
    const listScroll = await page.locator('.app-content').evaluate(element => element.scrollTop);
    await row.click();
    const detail = page.locator('.window-board-detail');
    await detail.waitFor();
    assert.equal(await detail.getAttribute('data-layout'), width <= 700 ? 'page' : width < 1280 ? 'drawer' : 'split');
    await page.getByTestId('window-workflow-step').first().waitFor();
    assert.equal(await page.locator('.window-board-status').isVisible(), true);
    const geometry = await page.evaluate(() => ({
      overflow: document.documentElement.scrollWidth > innerWidth,
      detail: document.querySelector('.window-board-detail').getBoundingClientRect().toJSON(),
      navHidden: getComputedStyle(document.querySelector('.mobile-primary-nav')).display === 'none',
      scrollContainers: [...document.querySelectorAll('.window-board-list, .window-board-panel, .window-activity-results')]
        .filter(e => e.offsetHeight && ['auto', 'scroll'].includes(getComputedStyle(e).overflowY)).map(e => e.className),
    }));
    assert.equal(geometry.overflow, false);
    if (width <= 700) { assert.equal(geometry.navHidden, true); assert.deepEqual(geometry.scrollContainers, []); }
    if (width > 700 && width < 1280) {
      for (let index = 0; index < 12; index++) {
        await page.keyboard.press('Tab');
        assert.equal(await detail.evaluate(element => element.contains(document.activeElement)), true);
      }
    }
    await page.getByRole('button', { name: 'Rename Window', exact: true }).click();
    const input = page.getByRole('textbox', { name: 'Window name', exact: true });
    await input.fill('Research — review architecture and follow-up work');
    if (width <= 700) await page.getByRole('button', { name: 'Save', exact: true }).click();
    else await input.press('Enter');
    await page.getByRole('heading', { name: label || 'Research — review architecture and follow-up work', exact: true }).waitFor();
    await page.getByRole('dialog', { name: 'Rename Window', exact: true }).waitFor({ state: 'detached' });
    await page.screenshot({ path: new URL(`${width}-${theme}-detail.png`, output).pathname });
    await page.getByRole('button', { name: 'Rename Window', exact: true }).click();
    if (width <= 700) await page.getByRole('button', { name: 'Clear name', exact: true }).click();
    else { await input.fill(''); await input.press('Enter'); }
    await page.locator('.window-board-title h2.window-board-default-name').waitFor();
    await page.getByRole('dialog', { name: 'Rename Window', exact: true }).waitFor({ state: 'detached' });
    if (width <= 700) {
      assert.deepEqual(await page.evaluate(() => history.state), { webcodexWindowDetail: true });
      await page.getByRole('button', { name: 'Back', exact: true }).click();
    } else {
      await detail.focus(); await page.keyboard.press('Escape');
    }
    await detail.waitFor({ state: 'detached' });
    if (width <= 700) {
      assert.equal(await page.locator('.mobile-primary-nav').isVisible(), true);
      const restored = await page.locator('.app-content').evaluate(element => element.scrollTop);
      assert.ok(Math.abs(restored - listScroll) < 2, `List scroll ${listScroll} restored to ${restored}`);
    }
    report.checks.push({ width, theme, geometry, renamedAndCleared: true, returnedToOverview: true });
    await page.close();
  }
  assert.deepEqual(report.errors, []);
} finally {
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  await browser.close(); await new Promise(resolve => fixture.server.close(resolve));
}
console.log(JSON.stringify({ passed: report.checks.length, output: output.pathname }));
