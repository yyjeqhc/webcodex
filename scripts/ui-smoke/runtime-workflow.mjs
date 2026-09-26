import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Production WebUI bundle, isolated observation fixtures, no native runtime.
const output = new URL('../../artifacts/liquid-glass-ui/runtime-workflow/', import.meta.url);
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
      let revealWindow = false;
      if (width === 1440 && theme === 'light') {
        await page.route('**/api/runtime-console/windows', async route => {
          const response = await route.fetch();
          const data = await response.json();
          if (revealWindow) {
            data.windows.push({ ...data.windows[0], client_window_key: 'b'.repeat(64) });
            data.total = data.windows.length;
          }
          await route.fulfill({ response, json: data });
        });
      }
      await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
      await page.goto(fixture.url + '/runtime/');
      await page.getByRole('searchbox', { name: 'Search Sessions' }).waitFor();
      await page.locator('.tool-cluster[open]').first().waitFor();
      assert(await page.getByRole('combobox', { name: 'Project filter' }).isVisible());
      await page.screenshot({ path: new URL(`work-${width}-${theme}.png`, output).pathname });
      const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
      await nav.getByRole('button', { name: /^Runtime/ }).click();
      await page.getByRole('button', { name: /View activity/ }).click();
      await page.getByText('run_process', { exact: true }).waitFor();
      assert(await page.getByText('500ms', { exact: true }).isVisible());
      assert(await page.locator('.window-call-card').filter({ hasText: 'read_files' }).getByText('Fixture alpha', { exact: true }).isVisible());
      assert(await page.locator('.window-relations-disclosure').getAttribute('open') !== null);
      if (width === 1440 && theme === 'light') {
        revealWindow = true;
        await page.getByTestId('work-window-row-' + 'b'.repeat(64)).waitFor({ timeout: 10_000 });
        assert.equal(await page.locator('.window-work-row.selected').getAttribute('data-testid'), 'work-window-row-' + '4700'.repeat(16));
      }
      const bounds = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
      assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
      await page.screenshot({ path: new URL(`windows-${width}-${theme}.png`, output).pathname, fullPage: true });
      checks.push({ width, theme, overflow: false, visibleRunningCall: true, visibleProject: true });
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
