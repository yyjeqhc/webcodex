import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Production assets with isolated read-only fixtures; no live Runtime access.
const output = new URL('../../artifacts/work-header/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  for (const width of [1440, 800, 390]) {
    for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
      page.on('pageerror', error => errors.push(error.message));
      await page.addInitScript(theme => {
        localStorage.setItem('webcodex.runtime.appearance.v1', theme);
        Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
          writeText: async value => { window.__copied = value; },
        } });
      }, theme);
      await page.goto(fixture.url + '/runtime/');
      const header = page.locator('.window-work-header');
      await header.getByRole('button', { name: 'Copy Directory', exact: true }).waitFor();
      await page.screenshot({ path: new URL(`work-${width}-${theme}.png`, output).pathname, fullPage: true });
      for (const label of ['Window', 'Machine', 'Directory', 'Project address']) {
        const button = header.getByRole('button', { name: 'Copy ' + label, exact: true });
        const value = await button.locator('..').locator('code').textContent();
        await button.click();
        assert.equal(await page.evaluate(() => window.__copied), value);
      }
      await page.evaluate(() => { navigator.clipboard.writeText = async () => { throw new Error('denied'); }; });
      await header.getByRole('button', { name: 'Copy Directory', exact: true }).click();
      await header.getByText('Copy unavailable; select the text to copy.', { exact: true }).waitFor();
      const bounds = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
      assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
      for (const fact of await header.locator('.window-header-fact').all()) {
        assert(await fact.evaluate(element => element.scrollWidth <= element.clientWidth + 1));
      }
      await page.close();
    }
  }
  assert.deepEqual(errors, []);
  console.log('Work header: 6 viewport/theme combinations passed; exact clipboard values, denied clipboard fallback, no overflow or browser errors.');
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
