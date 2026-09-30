import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = path.join(root, 'artifacts/desktop-workflow');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const report = { fixture: true, nativeBackend: false, checks: [], screenshots: [], errors: [] };
let browser;

async function bounded(page, label) {
  const dimensions = await page.evaluate(() => {
    const main = document.querySelector('.main-content');
    return { viewport: innerWidth, document: document.documentElement.scrollWidth,
      main: main.clientWidth, content: main.scrollWidth };
  });
  assert(dimensions.document <= dimensions.viewport + 1, `${label}: document overflow ${JSON.stringify(dimensions)}`);
  assert(dimensions.content <= dimensions.main + 1, `${label}: content overflow ${JSON.stringify(dimensions)}`);
  report.checks.push(label);
}

async function capture(page, name) {
  const filename = `${name}.png`;
  await page.screenshot({ path: path.join(output, filename), animations: 'disabled', caret: 'hide' });
  report.screenshots.push(filename);
}

try {
  const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  browser = await chromium.launch({ headless: true, ...(fs.existsSync(chrome) ? { executablePath: chrome } : {}) });
  for (const width of [1440, 1280, 1024, 768, 390]) {
    for (const theme of ['light', 'dark']) {
      const context = await browser.newContext({ viewport: { width, height: 900 }, colorScheme: theme, reducedMotion: 'reduce' });
      const page = await context.newPage();
      page.on('pageerror', error => report.errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') report.errors.push(message.text()); });
      await page.goto(fixture.url + '/desktop/');
      await page.locator('[data-webcodex-page="home"]').waitFor();
      await page.waitForFunction(expected => document.documentElement.dataset.theme === expected, theme);
      if (width <= 768) {
        assert.equal(await page.locator('.sidebar-preferences').isVisible(), false, 'compact navigation keeps preferences in Settings');
        assert.equal(await page.locator('.brand > div:last-child').isVisible(), false, 'compact navigation keeps room for page content');
      }
      // The shim supplies an English default on load. Change through the real UI
      // for Chinese coverage, including narrow labels and native form controls.
      await page.locator('[data-webcodex-action="navigate-settings"]').click();
      if (width === 1280 || width === 390) await page.locator('#desktop-settings-locale').selectOption('zh-CN');
      await page.locator('[data-webcodex-action="navigate-home"]').click();
      assert.equal(await page.locator('.readiness-banner').count(), 0, 'healthy Home avoids duplicate readiness facts');
      assert(await page.locator('.dashboard-handoff').isVisible());
      await bounded(page, `Home ${width} ${theme}`);
      await capture(page, `home-${width}-${theme}`);

      await page.locator('[data-webcodex-action="navigate-settings"]').click();
      for (const section of ['general', 'access', 'network', 'runtime', 'diagnostics', 'about']) {
        const tab = page.locator(`#settings-tab-${section}`);
        await tab.click();
        assert.equal(await tab.getAttribute('aria-selected'), 'true');
        assert(await page.locator(`#desktop-settings-${section}`).isVisible());
        assert.equal(await page.locator('.settings-content > div:visible').count(), 1);
        if (section === 'access') await page.locator('.settings-path-list code').first().waitFor();
        if (section === 'runtime') await page.locator('[data-webcodex-panel="runtime"] .runtime-facts').first().waitFor();
        if (section === 'diagnostics') await page.locator('#desktop-trace-mode').waitFor();
        await bounded(page, `Settings ${section} ${width} ${theme}`);
        await capture(page, `settings-${section}-${width}-${theme}`);
      }
      const mutations = await page.evaluate(() => window.__fixtureCalls.filter(call =>
        ['update_tunnel_proxy', 'update_runner_allowed_roots', 'restart_owned_runner', 'switch_runtime',
          'set_tool_request_tracing', 'request_computer_permission', 'environment_service_action'].includes(call.cmd)));
      assert.deepEqual(mutations, [], 'browsing settings never applies configuration or service effects');

      await page.locator('#settings-tab-network').click();
      await page.locator('#desktop-tunnel-proxy-mode').selectOption('custom');
      await page.locator('#desktop-tunnel-proxy-url').fill('http://127.0.0.1:7890');
      await page.locator('#settings-tab-diagnostics').click();
      await page.locator('#desktop-trace-mode').selectOption('metadata');
      await page.locator('#settings-tab-general').click();
      const control = await page.locator('#desktop-settings-locale').evaluate(element => ({
        height: element.getBoundingClientRect().height, fontSize: parseFloat(getComputedStyle(element).fontSize),
      }));
      assert(control.height >= 44 && control.fontSize >= 14, `readable controls: ${JSON.stringify(control)}`);
      const checkbox = await page.locator('#desktop-launch-at-login').boundingBox();
      assert(checkbox.width <= 24 && checkbox.height <= 24, 'checkbox remains a choice control');
      await page.locator('#settings-tab-network').click();
      assert.equal(await page.locator('#desktop-tunnel-proxy-url').inputValue(), 'http://127.0.0.1:7890');
      await page.locator('#settings-tab-diagnostics').click();
      assert.equal(await page.locator('#desktop-trace-mode').inputValue(), 'metadata');
      await page.locator('#settings-tab-access').click();
      await page.locator('.settings-path-list code').first().evaluate(element => { element.textContent = '/long-work-folder'.repeat(30); });
      await bounded(page, `Long allowed folder ${width} ${theme}`);
      report.checks.push(`Readable controls and preserved drafts ${width} ${theme}`);

      for (const destination of ['projects', 'activity', 'connection', 'extensions']) {
        await page.locator(`[data-webcodex-action="navigate-${destination}"]`).click();
        await page.locator(`[data-webcodex-page="${destination}"]`).waitFor();
        await bounded(page, `${destination} ${width} ${theme}`);
        if (destination === 'connection') {
          await page.locator('[data-webcodex-page="connection"] .page-heading-row .primary-button').click();
          const dialog = page.getByRole('dialog');
          await dialog.locator('#connection-profile-api-key').fill('fixture-only-write-only-key');
          const bounds = await dialog.boundingBox();
          assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1, 'connection editor fits the viewport');
          assert.equal(await dialog.locator('#connection-profile-api-key').getAttribute('type'), 'password');
          assert(!await dialog.innerText().then(text => text.includes('fixture-only-write-only-key')));
          await capture(page, `connection-editor-${width}-${theme}`);
          await page.keyboard.press('Escape');
          await dialog.waitFor({ state: 'hidden' });
          assert.equal(await page.evaluate(() => window.__fixtureCalls.some(call => call.cmd === 'save_tunnel_profile')), false);
          report.checks.push(`Connection editor containment ${width} ${theme}`);
        }
      }
      await page.locator('#extension-tab-instructions').click();
      await page.locator('#managed-global-instructions').fill('Keep this unsaved instruction draft.');
      await page.locator('#instruction-files').waitFor();
      assert(await page.locator('#skill-roots').isVisible(), 'path configuration is visible without a disclosure');
      await bounded(page, `Instructions ${width} ${theme}`);
      await capture(page, `instructions-${width}-${theme}`);
      await page.locator('#extension-tab-skills').click();
      await page.locator('#extension-tab-instructions').click();
      assert.equal(await page.locator('#managed-global-instructions').inputValue(), 'Keep this unsaved instruction draft.');
      await context.close();
    }
  }
  const context = await browser.newContext({ viewport: { width: 1024, height: 900 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  page.on('pageerror', error => report.errors.push(error.message));
  await page.goto(fixture.url + '/desktop/');
  await page.locator('[data-webcodex-action="navigate-settings"]').click();
  for (const locale of ['zh-CN', 'en-US', 'de-DE', 'fr-FR', 'ja-JP', 'ko-KR']) {
    await page.locator('#settings-tab-general').click();
    await page.locator('#desktop-settings-locale').selectOption(locale);
    for (const section of ['general', 'access', 'network', 'runtime', 'diagnostics', 'about']) {
      await page.locator(`#settings-tab-${section}`).click();
      await bounded(page, `Localized Settings ${locale} ${section}`);
    }
  }
  await context.close();
  assert.deepEqual(report.errors, [], 'renderer errors');
  console.log(JSON.stringify({ fixture: true, nativeBackend: false, checks: report.checks.length, screenshots: report.screenshots.length }));
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  if (browser) await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
