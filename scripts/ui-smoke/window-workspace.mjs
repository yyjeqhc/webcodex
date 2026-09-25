import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Render the production bundle against loopback fixtures; no real messages are sent.
const output = new URL('../../artifacts/runtime-work-review/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
const checks = [];
try {
  for (const width of [1440, 1024, 390]) {
    for (const theme of ['light', 'dark']) {
      for (const language of ['en', 'zh-CN']) {
        const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
        page.on('pageerror', error => errors.push(error.message));
        await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
        // The fixture normally initializes English; replace that one preference for Chinese checks.
        await page.route('**/runtime/', async route => {
          const response = await route.fetch();
          await route.fulfill({ response, body: (await response.text()).replace('language.v1","en"', `language.v1","${language}"`) });
        });
        await page.goto(fixture.url + '/runtime/');
        const zh = language === 'zh-CN';
        const activityTab = page.getByRole('tab', { name: zh ? '窗口活动' : 'Window activity', exact: true });
        const collaborationTab = page.getByRole('tab', { name: zh ? '窗口协作' : 'Window collaboration', exact: true });
        await activityTab.waitFor();
        assert.equal(await page.locator('.window-work-inspector').count(), 0);
        await page.locator('.window-call-card.running').waitFor();
        const assertBounds = async () => {
          const bounds = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
          assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
        };
        await assertBounds();
        await page.screenshot({ path: new URL(`activity-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
        await collaborationTab.click();
        const panel = page.getByRole('tabpanel', { name: zh ? '窗口协作' : 'Window collaboration' });
        await panel.getByRole('combobox').selectOption({ index: 1 });
        const composer = panel.locator('textarea');
        await composer.fill('Draft retained across tabs');
        await activityTab.click();
        assert.equal(await composer.isVisible(), false);
        await collaborationTab.click();
        assert.equal(await composer.inputValue(), 'Draft retained across tabs');
        await assertBounds();
        await page.screenshot({ path: new URL(`collaboration-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
        checks.push({ width, theme, language, overflow: false, draftRetained: true });
        await page.close();
      }
    }
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify({ fixture: true, nativeBackend: false, checks, errors }, null, 2));
  console.log(JSON.stringify({ checks: checks.length, errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
