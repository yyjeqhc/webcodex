import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Production renderers and isolated fixtures; never contact a live Runtime.
const fixture = await startFixtureServer();
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const executablePath = process.env.UI_SMOKE_BROWSER || (fs.existsSync(chrome) ? chrome : undefined);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
const output = new URL('../../artifacts/localized-layout/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const languages = ['en', 'zh-CN', 'zh-TW', 'ja-JP', 'ko-KR', 'de-DE', 'fr-FR'];
try {
  const page = await browser.newPage({ reducedMotion: "reduce" });
  page.setDefaultTimeout(10_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  let desktopLocale = 'de-DE';
  await page.route('**/desktop-shim.js', async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: (await response.text()).replace("'webcodex.desktop.locale','en-US'", `'webcodex.desktop.locale','${desktopLocale}'`) });
  });
  const rows = Array.from({ length: 30 }, (_, i) => ({
    message_id: 'wc_msg_' + i, source: 'window', direction: 'outbound',
    message: 'Original message 原文 — ' + 'Review this change carefully. '.repeat(8),
    created_at_ms: Date.now() + i, kind: 'answer', priority: 'normal', requires_ack: false,
    first_projected_at_ms: null, first_ack_observed_at_ms: null,
  }));
  await page.route('**/api/runtime-console/window', async route => {
    const response = await route.fetch();
    const data = await response.json();
    data.activity = Array.from({ length: 60 }, (_, index) => ({ ...data.activity[0],
      server_trace_id: `fixture-call-${index}`, tool_name: `fixture_tool_${index}`,
      started_at_ms: Date.now() - (60 - index) * 1000,
    }));
    data.activity_returned = data.activity.length;
    await route.fulfill({ response, json: data });
  });
  await page.route('**/api/runtime-console/window-collaboration', route => route.fulfill({ json: { available: true, can_send: true, messages: rows, truncated: false } }));
  const results = [];
  for (const width of [1024, 1440, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(fixture.url + '/runtime/');
    await page.locator('#window-collaboration-tab').click();
    for (const language of languages) {
      // The same preference control is used by desktop and mobile settings.
      if (!await page.locator('.runtime-language-control select:visible').count()) {
        await page.getByRole('button', { name: /Preferences|偏好|Préférences|Einstellungen|設定|환경 설정/ }).click();
      }
      await page.locator('.runtime-language-control select:visible').selectOption(language);
      if (await page.locator('#mobile-preferences').isVisible()) await page.locator('[aria-controls="mobile-preferences"]').click();
      await page.locator('.window-collaboration-message').first().waitFor();
      const metrics = await page.evaluate(() => {
        const thread = document.querySelector('.window-collaboration-thread');
        const pane = document.querySelector('.window-work-main');
        const composer = document.querySelector('.window-collaboration-composer');
        const a = thread.getBoundingClientRect(), b = composer.getBoundingClientRect();
        return { language: document.documentElement.lang, documentWidth: document.documentElement.scrollWidth,
          threadHeight: a.height, paneHeight: pane.getBoundingClientRect().height, threadBottom: a.bottom,
          composerTop: b.top, composerBottom: b.bottom, paneBottom: pane.getBoundingClientRect().bottom };
      });
      results.push({ surface: 'runtime', width, language, ...metrics });
      assert.equal(metrics.language, language);
      assert(metrics.documentWidth <= width + 1, JSON.stringify(metrics));
      assert(metrics.threadHeight >= metrics.paneHeight * .4, JSON.stringify(metrics));
      assert(metrics.threadBottom <= metrics.composerTop + 1, JSON.stringify(metrics));
      assert(metrics.composerBottom <= metrics.paneBottom + 1, JSON.stringify(metrics));
      await page.locator('.window-collaboration-composer textarea').fill('Draft survives 原文');
    }
    await page.locator('.window-collaboration-composer').scrollIntoViewIfNeeded();
    await page.screenshot({ path: new URL(`collaboration-${width}.png`, output).pathname, fullPage: false, animations: "disabled" });
    await page.locator('#window-activity-tab').click();
    const toolbar = page.locator('.window-activity-toolbar');
    const before = await toolbar.boundingBox();
    await page.locator('.window-call-card').last().waitFor();
    const scroll = await page.locator('.window-activity-results').evaluate(node => { node.scrollTop = node.scrollHeight; return node.scrollTop; });
    assert(scroll > 0, 'The call list must really scroll');
    assert.deepEqual(await toolbar.boundingBox(), before);
    await page.locator('input[value="oldest"]').check();
    assert(await page.locator('input[value="oldest"]').isChecked());
    await page.locator('.window-activity-results').evaluate(node => { node.scrollTop = 0; });
    await page.screenshot({ path: new URL(`activity-${width}.png`, output).pathname, animations: 'disabled' });
  }
  for (const width of [1024, 1440, 768, 390]) {
    await page.setViewportSize({ width, height: 900 });
    for (const language of languages) {
      desktopLocale = language === 'en' ? 'en-US' : language;
      await page.goto(fixture.url + '/desktop/');
      await page.locator('[data-webcodex-page]').waitFor();
      for (const view of ['home', 'settings', 'extensions']) {
        await page.locator(`[data-webcodex-action="navigate-${view}"]`).click();
        await page.locator(`[data-webcodex-page="${view}"]`).waitFor();
        const metrics = await page.evaluate(() => {
          const overlaps = [];
          for (const selector of ['.sidebar .nav-label', '.mantine-Button-label']) {
            for (const label of document.querySelectorAll(selector)) {
              const rect = label.getBoundingClientRect();
              if (!rect.width || !rect.height) continue;
              const button = label.closest('button').getBoundingClientRect();
              if (rect.right > button.right + 1 || rect.bottom > button.bottom + 1 || rect.left < button.left - 1) overlaps.push(label.textContent);
            }
          }
          return { documentWidth: document.documentElement.scrollWidth, overlaps };
        });
        results.push({ surface: 'desktop', width, language, view, ...metrics });
        assert(metrics.documentWidth <= width + 1, JSON.stringify({ width, language, view, ...metrics }));
        assert.deepEqual(metrics.overlaps, [], JSON.stringify({ width, language, view, ...metrics }));
      }
      if (language === 'de-DE') await page.screenshot({ path: new URL(`desktop-${width}.png`, output).pathname, fullPage: false, animations: "disabled" });
    }
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(new URL('metrics.json', output), JSON.stringify(results, null, 2));
  console.log(`Localized layout: ${results.length} viewport/language/page checks passed.`);
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
