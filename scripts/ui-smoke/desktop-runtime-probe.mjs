import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Issue #780: exercise the native projection without starting real programs or
// changing an installed Runtime, credentials, permissions or security policy.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = path.join(root, 'artifacts/desktop-runtime-probe');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
const binaries = [
  { name: 'webcodex', present: true, startup_check: 'failed', error_code: 'webcodex_command_failed', diagnostics: { exit_code: -1073741790, io_kind: null } },
  { name: 'webcodex-server', present: false, startup_check: 'not_checked', error_code: 'binary_missing', diagnostics: null },
  { name: 'webcodex-runner', present: null, startup_check: 'not_checked', error_code: 'runtime_file_unreadable', diagnostics: { exit_code: null, io_kind: 'PermissionDenied' } },
].map(binary => ({ ...binary, metadata: null, sha256: null }));
let browser;
try {
  const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  browser = await chromium.launch({ headless: true, ...(fs.existsSync(chrome) ? { executablePath: chrome } : {}) });
  for (const width of [1280, 390]) for (const theme of ['light', 'dark']) for (const locale of ['en-US', 'zh-TW']) {
    const label = `${width}-${theme}-${locale}`;
    const context = await browser.newContext({ viewport: { width, height: 960 }, colorScheme: theme, reducedMotion: 'reduce' });
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch(); let source = await response.text();
      assert(source.includes('const diagnostic='));
      source = source.replace('const diagnostic=', `Object.assign(runtime.selected, ${JSON.stringify({ binaries, compatibility: 'unknown', error_code: 'webcodex_command_failed' })}); runtime.candidate=structuredClone(runtime.selected); const diagnostic=`);
      await route.fulfill({ response, body: source });
    });
    await page.goto(fixture.url + '/desktop/');
    await page.locator('[data-webcodex-action="navigate-settings"]').click();
    await page.locator('#desktop-settings-locale').selectOption(locale);
    await page.locator('#settings-tab-runtime').click();
    const current = page.locator('.runtime-binary-list').first();
    await current.locator('article').first().waitFor();
    const rows = current.locator('article');
    assert.equal(await rows.count(), 3);
    for (const row of await rows.all()) {
      assert(await row.locator('h4').isVisible());
      assert(await row.locator('.runtime-facts').isVisible(), 'file/startup observations stay outside folded details');
      assert(await row.locator('.field-help').isVisible(), 'failures have visible recovery guidance');
    }
    const facts = await current.locator('.runtime-facts').allTextContents();
    assert(facts[0].includes(locale === 'en-US' ? 'Present' : '存在'));
    assert(facts[1].includes(locale === 'en-US' ? 'Missing' : '缺失'));
    assert(facts[2].includes(locale === 'en-US' ? 'Unconfirmed' : '未確認'));
    assert(await page.locator('.runtime-candidate > button').isDisabled());
    await rows.first().locator('summary').click();
    assert(await rows.first().getByText(/0xC0000022/).isVisible());
    const dimensions = await page.evaluate(() => {
      const main = document.querySelector('.main-content');
      return { viewport: innerWidth, document: document.documentElement.scrollWidth, main: main.clientWidth, content: main.scrollWidth };
    });
    assert(dimensions.document <= dimensions.viewport + 1 && dimensions.content <= dimensions.main + 1, JSON.stringify(dimensions));
    assert.deepEqual(await page.evaluate(() => window.__fixtureCalls.filter(call => ['switch_runtime', 'environment_service_action', 'restart_runtime', 'probe_runtime_candidate'].includes(call.cmd))), []);
    await page.locator('.main-content').evaluate(element => { element.scrollTop = 0; });
    await page.screenshot({ path: path.join(output, `${label}.png`), animations: 'disabled', caret: 'hide' });
    report.checks.push(label);
    await context.close();
  }
  assert.deepEqual(report.errors, []);
} finally {
  await browser?.close();
  await new Promise(resolve => fixture.server.close(resolve));
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify(report));
