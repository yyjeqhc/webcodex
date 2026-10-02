import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Read-only fixtures served locally; no production Runtime or credentials.
const output = new URL('../../artifacts/runtime-overview/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  for (const width of [1440, 800, 390]) {
    for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width, height: 1100 }, reducedMotion: 'reduce' });
      page.on('pageerror', error => errors.push(error.message));
      const requests = [];
      page.on('request', request => {
        if (request.url().includes('/api/runtime-console/')) requests.push(request.url().split('/api/runtime-console/')[1]);
      });
      await page.addInitScript(theme => {
        localStorage.setItem('webcodex.runtime.v2.view.v1', 'runtime');
        localStorage.setItem('webcodex.runtime.appearance.v1', theme);
      }, theme);
      await page.goto(fixture.url + '/runtime/');
      await page.getByRole('heading', { name: 'Authentication', exact: true }).waitFor();
      assert.equal(await page.locator('.runtime-metrics > div').count(), 4);
      assert.equal(await page.locator('.runtime-diagnostics').getAttribute('open'), null);
      assert(!requests.some(route => ['windows', 'window', 'communication/agents'].includes(route)));
      for (const language of ['en', 'zh']) {
        if (language === 'zh') {
          if (width <= 700) await page.getByRole('button', { name: 'Preferences', exact: true }).click();
          await page.getByRole('button', { name: 'Language', exact: true }).click();
          if (width <= 700) await page.locator('.mobile-app-bar button').click();
          await page.getByRole('heading', { name: '访问认证', exact: true }).waitFor();
        }
        for (const state of await page.locator('.runtime-config-group dd .quiet-pill').all()) {
          assert(await state.isVisible(), 'Authentication states must stay visible on mobile');
        }
        const bounds = await page.evaluate(() => ({ body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
        assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
        for (const item of await page.locator('.runtime-config-group, .runtime-row, .runtime-metrics > div').all()) {
          assert(await item.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
        }
        assert.equal(await page.locator('.runtime-config-group dt').first().evaluate(element => getComputedStyle(element).fontSize), '15px');
        await page.screenshot({ path: new URL(`runtime-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
      }
      await page.locator('.runtime-diagnostics summary').click();
      assert.notEqual(await page.locator('.runtime-diagnostics').getAttribute('open'), null);
      await page.close();
    }
  }
  assert.deepEqual(errors, []);
  console.log('Runtime overview: 12 viewport/theme/language combinations passed; readable font sizes, bounded layout, deferred diagnostics and no extra inventory requests.');
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
