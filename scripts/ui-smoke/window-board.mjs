import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

const output = new URL('../../artifacts/window-board/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
try {
  for (const [width, theme] of [[1440, 'light'], [1024, 'light'], [390, 'light'], [390, 'dark']]) {
    const page = await browser.newPage({ viewport: { width, height: 960 }, reducedMotion: 'reduce' });
    page.on('pageerror', error => report.errors.push(error.message));
    await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
    await page.goto(fixture.url + '/runtime/');
    const row = page.getByTestId('work-window-row-' + '4700'.repeat(16));
    await row.waitFor();
    assert.equal(await page.locator('.window-board-detail').count(), 0);
    await page.screenshot({ path: new URL(`${width}-${theme}-overview.png`, output).pathname });
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
    await page.screenshot({ path: new URL(`${width}-${theme}-detail.png`, output).pathname });
    if (width <= 700) {
      assert.deepEqual(await page.evaluate(() => history.state), { webcodexWindowDetail: true });
      await page.getByRole('button', { name: 'Back', exact: true }).click();
    } else {
      await detail.focus(); await page.keyboard.press('Escape');
    }
    await detail.waitFor({ state: 'detached' });
    if (width <= 700) assert.equal(await page.locator('.mobile-primary-nav').isVisible(), true);
    report.checks.push({ width, theme, geometry, returnedToOverview: true });
    await page.close();
  }
  assert.deepEqual(report.errors, []);
} finally {
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  await browser.close(); await new Promise(resolve => fixture.server.close(resolve));
}
console.log(JSON.stringify({ passed: report.checks.length, output: output.pathname }));
