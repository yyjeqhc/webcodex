import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Real production renderer, synthetic native observations only. No Runtime,
// credentials or installed services are started or changed by this fixture.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = path.join(root, 'artifacts/desktop-runtime-simple');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
let browser;
try {
  const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  browser = await chromium.launch({ headless: true, ...(fs.existsSync(chrome) ? { executablePath: chrome } : {}) });
  for (const width of [1280, 390]) for (const locale of ['en-US', 'zh-CN']) {
    const context = await browser.newContext({ viewport: { width, height: 960 }, reducedMotion: 'reduce' });
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch();
      let source = await response.text();
      assert(source.includes('const diagnostic='));
      source = source.replace('const diagnostic=', `
        runtime.source={kind:'custom',directory:'C:\\\\workspace\\\\webcodex\\\\target\\\\dogfood'};
        runtime.selected.source=runtime.source;runtime.selected.directory=runtime.source.directory;
        runtime.selected.binaries=['webcodex','webcodex-server','webcodex-runner'].map(name=>({
          name,present:true,startup_check:'passed',metadata:{...build,binary:name},sha256:'fixture',error_code:null,diagnostics:null
        })); const diagnostic=`);
      await route.fulfill({ response, body: source });
    });
    await page.goto(fixture.url + '/desktop/');
    await page.locator('[data-webcodex-action="navigate-settings"]').click();
    await page.locator('#desktop-settings-locale').selectOption(locale);
    await page.locator('#settings-tab-runtime').click();
    const reload = page.getByRole('button', { name: locale === 'en-US' ? 'Reload selected folder' : '重新加载此目录', exact: true });
    await reload.waitFor();
    assert(await reload.isEnabled());
    assert(!await page.locator('.runtime-binary-list').first().isVisible(), 'healthy binary detail is collapsed');
    assert.equal(await page.getByRole('dialog').count(), 0);
    assert.equal(await page.locator('.runtime-candidate').count(), 0, 'no redundant approval card on entry');
    const dimensions = await page.evaluate(() => {
      const main = document.querySelector('.main-content');
      return { viewport: innerWidth, document: document.documentElement.scrollWidth, main: main.clientWidth, content: main.scrollWidth };
    });
    assert(dimensions.document <= dimensions.viewport + 1 && dimensions.content <= dimensions.main + 1, JSON.stringify(dimensions));
    const effects = await page.evaluate(() => window.__fixtureCalls.filter(call => ['switch_runtime', 'environment_service_action', 'restart_runtime', 'probe_runtime_candidate'].includes(call.cmd)));
    assert.deepEqual(effects, []);
    await page.screenshot({ path: path.join(output, `${width}-${locale}.png`), animations: 'disabled', caret: 'hide' });
    const details = page.locator('.runtime-binary-list').first().locator('..');
    await details.locator(':scope > summary').click();
    assert(await page.locator('.runtime-binary-list').first().isVisible(), 'inspection remains available');
    assert.equal(await page.locator('.runtime-binary-list article').count(), 3);
    report.checks.push(`${width}-${locale}`);
    await context.close();
  }
  assert.deepEqual(report.errors, []);
} finally {
  await browser?.close();
  await new Promise(resolve => fixture.server.close(resolve));
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
}
console.log(JSON.stringify(report));
