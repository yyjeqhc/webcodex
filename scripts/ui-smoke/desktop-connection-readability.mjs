import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium, webkit } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { dialogFixture } from './desktop-dialog-fixtures.mjs';

// Read-only fixture interactions: hover, local selection, and opening/cancelling
// confirmations. Never run a native mutation, a tunnel, or a permission change.
const output = path.resolve(import.meta.dirname, '../../artifacts/desktop-clarity/connection-readability');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
const settle = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));

async function readableHover(button, page, label) {
  await button.hover();
  await settle(page);
  const colors = await button.evaluate(el => {
    const rgb = value => value.match(/[\d.]+/g).map(Number);
    const blend = (top, bottom) => top.slice(0, 3).map((value, index) => value * (top[3] ?? 1) + bottom[index] * (1 - (top[3] ?? 1)));
    const ancestors = []; for (let node = el; node; node = node.parentElement) ancestors.unshift(node);
    let background = [255, 255, 255];
    for (const node of ancestors) background = blend(rgb(getComputedStyle(node).backgroundColor), background);
    const foreground = blend(rgb(getComputedStyle(el).color), background);
    const luminance = color => color.map(value => { const n = value / 255; return n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4; })
      .reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
    const a = luminance(foreground), b = luminance(background);
    return { foreground, background, contrast: (Math.max(a, b) + .05) / (Math.min(a, b) + .05) };
  });
  assert(colors.contrast >= 4.5, `${label}: ${JSON.stringify(colors)}`);
  report.checks.push({ label, contrast: colors.contrast });
}

async function bounded(page, label) {
  await settle(page);
  const overflow = await page.evaluate(() => [...document.querySelectorAll('html, .main-content, .connection-profile, .connection-id, .connection-meta-grid, .mantine-Modal-content, .mantine-Button-label')]
    .filter(el => el.getClientRects().length && el.scrollWidth > el.clientWidth + 1)
    .map(el => ({ element: el.className || el.tagName, width: el.clientWidth, content: el.scrollWidth })));
  assert.deepEqual(overflow, [], label);
  report.checks.push({ label });
}

try {
  for (const engine of [chromium, webkit]) {
    const browser = await engine.launch({ headless: true });
    try {
      for (const locale of ['zh-CN', 'ja-JP', 'de-DE']) for (const width of [1280, 768, 390]) for (const theme of ['light', 'dark']) {
        const label = `${engine.name()}-${locale}-${width}-${theme}`;
        const context = await browser.newContext({ viewport: { width, height: 960 }, colorScheme: theme, reducedMotion: 'reduce' });
        const page = await context.newPage();
        page.setDefaultTimeout(10_000);
        page.on('pageerror', error => report.errors.push({ label, error: error.message }));
        await page.addInitScript(theme => localStorage.setItem('webcodex.desktop.appearance.v1', theme), theme);
        await page.route('**/desktop-shim.js', async route => {
          const response = await route.fetch();
          let source = dialogFixture(await response.text(), locale);
          source = source.replace('window.__fixtureCalls=[];', `
            const base=state.connections.profiles[0];
            Object.assign(base,{name:'OpenAI Tunnel',tunnel_id:'webcodex-personal-7fa9c82e-11a4-4b98-ae79-2f67719e9c72',host_mode:'embedded',lifecycle:'running',enabled:true,ready:true,process_started:true,process_ready:true,tunnel_ready:true,local_mcp_ready:true,autostart:true,runtime_directory:'/fixture/runtime/connections/personal',local_mcp_url:'http://127.0.0.1:8787/mcp',startup_timings_ms:{cli_total:238,initial_poll:180,poll_attempts:1,poll_request_ms:170}});
            state.connections.profiles=[base,{...base,id:'fixture-failed',name:'接続状態の確認が必要な長い名前の接続 — Team workspace',host_mode:'standalone',ready:false,lifecycle:'error',tunnel_ready:false,last_error:'tunnel_unavailable',reason_code:'tunnel_control_plane_unreachable'}];
            settings.file_access={configured_roots:['/fixture/home','/fixture/work/project'],default_roots:['/fixture/home'],effective_roots:['/fixture/home','/fixture/work/project'],using_default_roots:false,allow_cwd_anywhere:false};
            window.__fixtureCalls=[];`);
          await route.fulfill({ response, body: source });
        });
        await page.goto(fixture.url + '/desktop/');
        await page.waitForFunction(({ locale, theme }) => document.documentElement.lang === locale && document.documentElement.dataset.theme === theme, { locale, theme });
        await page.locator('[data-webcodex-action="navigate-connection"]').click();
        const card = page.locator('[data-tunnel-profile-id="fixture-connection"]');
        await card.waitFor();
        assert.equal(await page.locator('.error-card').count(), 0, 'fixture startup must not mask the page');
        assert.equal(await card.locator('.connection-details').getAttribute('open'), null);
        await bounded(page, `${label} connections`);
        await page.screenshot({ path: path.join(output, `${label}-connections.png`), animations: 'disabled' });
        await readableHover(card.locator('.connection-id button'), page, `${label} copy tunnel ID`);
        await readableHover(card.locator('.connection-actions > .text-button'), page, `${label} delete connection`);
        await card.locator('.connection-details summary').click();
        await bounded(page, `${label} diagnostics`);
        if (width === 1280) await page.screenshot({ path: path.join(output, `${label}-details.png`), animations: 'disabled' });
        await card.locator('.connection-actions > button').first().click();
        await page.getByRole('dialog').waitFor();
        await bounded(page, `${label} editor`);
        await page.keyboard.press('Escape');

        await page.locator('[data-webcodex-action="navigate-projects"]').click();
        const selection = page.locator('.project-selection-actions .mantine-Button-root').first();
        await readableHover(selection, page, `${label} select/deselect`);
        if (width === 1280) await page.screenshot({ path: path.join(output, `${label}-selection-hover.png`), animations: 'disabled' });
        await page.locator('.project-remove-action').first().click();
        const dialog = page.getByRole('dialog');
        await dialog.waitFor();
        await readableHover(dialog.locator('.mantine-Button-root').first(), page, `${label} cancel removal`);
        // The red confirmation retains Mantine's semantic palette; no click.
        await readableHover(dialog.locator('.mantine-Button-root').last(), page, `${label} confirm removal`);
        await bounded(page, `${label} removal confirmation`);
        await page.keyboard.press('Escape');
        if (width === 1280) for (const accent of ['#ffffff', '#000000', '#ffcc00', '#7c3aed']) {
          await page.evaluate(value => {
            localStorage.setItem('webcodex.ui.accent.v1', value);
            dispatchEvent(new CustomEvent('webcodex:accent-change', { detail: value }));
          }, accent);
          await readableHover(selection, page, `${label} ${accent} select/deselect`);
        }
        await page.locator('[data-webcodex-action="navigate-settings"]').click();
        await page.locator('#settings-tab-access').click();
        await readableHover(page.locator('.settings-path-list .text-button').first(), page, `${label} remove file access`);
        assert.deepEqual(await page.evaluate(() => window.__fixtureForbiddenCalls || []), [], 'no native mutation may run');
        await context.close();
      }
    } finally { await browser.close(); }
  }
  assert.deepEqual(report.errors, []);
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors }));
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  await new Promise(resolve => fixture.server.close(resolve));
}
