import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Exercise production assets with delayed read-only fixtures, never a live runtime.
const output = new URL('../../artifacts/projects-runtime/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
const checks = [];
try {
  for (const width of [1440, 1024, 390]) {
    for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
      page.on('pageerror', error => errors.push(error.message));
      const requests = [];
      page.on('request', request => {
        if (request.url().includes('/api/runtime-console/')) requests.push(request.url().split('/api/runtime-console/')[1]);
      });
      let releaseWindows;
      const delayedWindows = new Promise(resolve => { releaseWindows = resolve; });
      await page.route('**/api/runtime-console/windows', async route => {
        await delayedWindows;
        await route.continue();
      });
      await page.addInitScript(theme => {
        localStorage.setItem('webcodex.runtime.v2.view.v1', 'projects');
        localStorage.setItem('webcodex.runtime.appearance.v1', theme);
      }, theme);
      await page.goto(fixture.url + '/runtime/');
      await page.getByTestId('project-card-agent:fixture-runner:alpha').waitFor();
      // Selection must work even while the Window inventory has not arrived.
      await page.getByTestId('project-card-agent:fixture-runner:beta').click();
      assert.equal(await page.getByTestId('project-card-agent:fixture-runner:beta').getAttribute('aria-pressed'), 'true');
      await page.getByRole('heading', { name: 'Fixture beta', exact: true }).waitFor();
      assert(!requests.includes('project-git') && !requests.includes('workflow-session'), requests.join(','));
      releaseWindows();
      await page.getByRole('button', { name: 'Check branch', exact: true }).click();
      await page.getByText('codex/fixture-ui', { exact: true }).waitFor();
      assert.equal(requests.filter(route => route === 'project-git').length, 1);
      await page.getByTestId('project-card-agent:fixture-runner:alpha').click();
      await page.getByRole('button', { name: 'Check branch', exact: true }).waitFor();
      const noOverflow = async () => {
        const bounds = await page.evaluate(() => ({ viewport: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
        assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
      };
      await noOverflow();
      if (width > 900) {
        const list = await page.locator('.project-browser-list').boundingBox();
        const details = await page.locator('.project-family-details').boundingBox();
        assert(details.x >= list.x + list.width, 'Details must sit beside the project list');
      } else {
        assert.equal(await page.evaluate(() => document.activeElement?.className), 'project-family-details');
      }
      await page.screenshot({ path: new URL(`projects-${width}-${theme}.png`, output).pathname, fullPage: true });
      const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
      const marker = requests.length;
      await nav.getByRole('button', { name: /^Runtime/ }).click();
      await page.getByRole('heading', { name: 'Runner fleet', exact: true }).waitFor();
      assert.equal(await page.locator('.runtime-diagnostics').getAttribute('open'), null);
      assert(!requests.slice(marker).some(route => ['windows', 'window', 'communication/agents'].includes(route)));
      await noOverflow();
      await page.screenshot({ path: new URL(`runtime-${width}-${theme}.png`, output).pathname, fullPage: true });
      await page.getByText('Build diagnostics', { exact: true }).click();
      assert.notEqual(await page.locator('.runtime-diagnostics').getAttribute('open'), null);
      await noOverflow();
      await page.getByRole('button', { name: /View activity/ }).click();
      await page.getByTestId('work-window-row-' + '4700'.repeat(16)).waitFor();
      await nav.getByRole('button', { name: /^Runtime/ }).click();
      await Promise.all([
        page.waitForResponse(response => response.url().endsWith('/communication/agents')),
        page.getByRole('tab', { name: 'Agents', exact: true }).click(),
      ]);
      checks.push({ width, theme, delayedInventoryInteractive: true, onDemandGit: true, deferredRuntimeInventories: true, overflow: false });
      await page.close();
    }
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify({ fixture: true, nativeBackend: false, checks, errors }, null, 2));
  console.log(JSON.stringify({ checks: checks.length, errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
