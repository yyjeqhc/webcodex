import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Render the production bundle against loopback fixtures; no real messages are sent.
const output = new URL('../../artifacts/runtime-call-stream/', import.meta.url);
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
        await page.route('**/api/runtime-console/window', async route => {
          const response = await route.fetch();
          const data = await response.json();
          const base = data.activity[0];
          data.activity = [
            { ...base, server_trace_id: 'failed-call', tool_name: 'apply_text_edits', status: 'error', started_at_ms: base.started_at_ms + 200 },
            { ...base, server_trace_id: 'observe-2', tool_name: 'runtime_status', meaningful: false, project: undefined, started_at_ms: base.started_at_ms + 100 },
            { ...base, server_trace_id: 'observe-1', tool_name: 'runtime_status', meaningful: false, project: undefined },
          ];
          data.activity_returned = 3;
          await route.fulfill({ response, json: data });
        });
        await page.goto(fixture.url + '/runtime/');
        const zh = language === 'zh-CN';
        await page.locator('.window-call-card.running').waitFor();
        const calls = page.getByTestId('window-workflow-step');
        assert.equal(await calls.count(), 4);
        assert.deepEqual(await calls.locator('header strong').allTextContents(), ['runtime_status', 'runtime_status', 'apply_text_edits', 'run_process']);
        assert.equal(await calls.nth(0).getByTestId('window-project-tag').count(), 0);
        assert.equal(await calls.nth(1).getByTestId('window-project-tag').count(), 0);
        assert.equal(await calls.nth(2).getByTestId('window-project-tag').textContent(), '/fixture/alpha');
        assert(await calls.nth(2).getByText(zh ? '失败' : 'Failed', { exact: true }).isVisible());
        assert(await calls.nth(0).getByText(zh ? '成功' : 'Succeeded', { exact: true }).isVisible());
        assert.equal(await page.locator('.window-work-main details, .window-work-main select, .window-work-inspector').count(), 0);
        assert.equal(await calls.locator('time[datetime]').count(), 4);
        assert.equal(await calls.locator('.window-call-timing strong').count(), 4);
        const bounds = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
        assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
        await page.screenshot({ path: new URL(`calls-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
        checks.push({ width, theme, language, overflow: false, individualCalls: 4, chronological: true });
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
