import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Real production renderer, isolated loopback fixtures; no native backend or tokens.
const output = new URL('../../artifacts/web-soft-surfaces/admin-preferences/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, language: 'en', checks: [], errors: [], limitations: ['Chromium fixture renderer only; no live backend or native WebView.'] };
const appearanceKey = 'webcodex.admin.appearance.v1';
const accentKey = 'webcodex.ui.accent.v1';
const presets = [['Blue', '#2563eb'], ['Indigo', '#4f46e5'], ['Teal', '#0f766e'], ['Violet', '#7c3aed'], ['Orange', '#c2410c']];

async function unlock(page) {
  await page.getByLabel('Admin token', { exact: true }).fill('fixture-only-not-a-secret');
  await page.getByRole('button', { name: 'Unlock console', exact: true }).click();
  await page.getByRole('heading', { name: 'Operations', exact: true }).waitFor();
}

async function inspect(page, width, theme, state) {
  const group = page.getByRole('group', { name: 'Preferences', exact: true });
  await group.waitFor();
  assert(await group.evaluate(element => element.classList.contains('admin-preference-buttons')));
  const controls = group.locator(':scope > button');
  assert.equal(await controls.count(), 2, 'Admin exposes exactly Accent color and Appearance');
  const accent = group.getByRole('button', { name: 'Accent color', exact: true });
  const appearance = group.getByRole('button', { name: 'Appearance', exact: true });
  assert.equal(await controls.nth(0).getAttribute('aria-label'), 'Accent color');
  assert.equal(await controls.nth(1).getAttribute('aria-label'), 'Appearance');
  assert.equal(await group.locator('.appearance-preference-button').count(), 2);
  const geometry = await controls.evaluateAll(elements => elements.map(element => {
    const box = node => { const rect = node.getBoundingClientRect(); return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom, cx: rect.x + rect.width / 2, cy: rect.y + rect.height / 2 }; };
    return { ...box(element), text: element.innerText, icon: box(element.querySelector('svg, .mantine-ColorSwatch-root')) };
  }));
  const vertical = width > 700 && width <= 1100;
  for (const control of geometry) {
    assert.equal(control.text, '', 'Preference controls have no visible text');
    assert.equal(control.width, 36); assert.equal(control.height, 36);
    assert(Math.abs(control.icon.cx - control.cx) <= 1 && Math.abs(control.icon.cy - control.cy) <= 1, 'Icons centered in targets');
    assert(control.x >= 0 && control.right <= width && control.y >= 0 && control.bottom <= 1000, 'Target within viewport');
  }
  const [first, second] = geometry;
  const expectedGap = width > 1100 ? 12 : 8;
  assert(Math.abs((vertical ? second.y - first.bottom : second.x - first.right) - expectedGap) <= 1, 'Responsive target gap');
  assert(Math.abs(vertical ? second.cx - first.cx : second.cy - first.cy) <= 1, 'Responsive target axis');
  await page.mouse.move(width - 1, 1);
  await page.screenshot({ path: new URL(`${state}-${width}-${theme}.png`, output).pathname, animations: 'disabled' });
  if (width === 1440 && theme === 'dark' && state === 'dashboard') {
    await page.locator('.rail-footer').screenshot({ path: new URL('admin-preferences-1440-dark.png', output).pathname, animations: 'disabled' });
  }

  await accent.focus();
  await page.keyboard.press('Enter');
  const palette = page.getByRole('dialog', { name: 'Accent color', exact: true });
  await palette.waitFor();
  assert(await palette.evaluate(element => element.classList.contains('appearance-preference-popover')));
  const bounds = await palette.boundingBox();
  assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1 && bounds.y >= 0 && bounds.y + bounds.height <= 1001, 'Popover stays within viewport');
  assert.equal(await palette.locator('button[aria-pressed]').count(), presets.length);
  for (const [name] of presets) assert.equal(await palette.getByRole('button', { name, exact: true }).count(), 1);
  await palette.getByRole('textbox', { name: 'Custom color', exact: true }).focus();
  for (const key of ['Tab', 'Shift+Tab']) {
    for (let step = 0; step < 8; step++) {
      await page.keyboard.press(key);
      assert(await palette.evaluate(element => element.contains(document.activeElement)), 'Popover traps keyboard focus');
    }
  }
  await palette.screenshot({ path: new URL(`palette-${state}-${width}-${theme}.png`, output).pathname, animations: 'disabled' });
  if (width === 1440 && theme === 'dark' && state === 'dashboard') {
    await palette.screenshot({ path: new URL('admin-palette-1440-dark.png', output).pathname, animations: 'disabled' });
  }
  await page.keyboard.press('Escape');
  await palette.waitFor({ state: 'hidden' });
  await page.waitForFunction(() => document.activeElement === document.querySelector('.admin-preference-buttons > button'));

  for (const [name, color] of presets) {
    await accent.click();
    await palette.getByRole('button', { name, exact: true }).click();
    await palette.waitFor({ state: 'hidden' });
    await page.waitForFunction(({ key, color }) => localStorage.getItem(key) === color, { key: accentKey, color });
  }
  await accent.click();
  await palette.getByRole('textbox', { name: 'Custom color', exact: true }).fill('#135abc');
  await page.waitForFunction(key => localStorage.getItem(key) === '#135abc', accentKey);
  await page.getByRole('heading', { name: state === 'gate' ? 'Administrator access' : 'Operations', exact: true }).click();
  await palette.waitFor({ state: 'hidden' });

  const cycle = theme === 'light' ? ['dark', 'system', 'light'] : ['system', 'light', 'dark'];
  for (const value of cycle) {
    await appearance.click();
    await page.waitForFunction(({ key, value }) => localStorage.getItem(key) === value, { key: appearanceKey, value });
    assert.equal(await page.locator('html').getAttribute('data-theme'), value);
    assert.equal(await appearance.getAttribute('aria-description'), `Appearance · ${value[0].toUpperCase()}${value.slice(1)}`);
  }
  await page.reload();
  await page.getByRole('heading', { name: 'Administrator access', exact: true }).waitFor();
  assert.equal(await page.evaluate(key => localStorage.getItem(key), accentKey), '#135abc', 'Custom accent persists across reload');
  assert.equal(await page.evaluate(key => localStorage.getItem(key), appearanceKey), theme, 'Admin appearance persists across reload');
  assert.equal(await page.locator('html').getAttribute('data-theme'), theme);
  assert.equal(await page.locator('html').getAttribute('data-accent'), 'custom');
  if (state === 'dashboard') await unlock(page);
  assert(await page.evaluate(() => Math.max(document.body.scrollWidth, document.documentElement.scrollWidth) <= innerWidth + 1), 'No page horizontal overflow');
  report.checks.push({ width, theme, state, geometry, gap: expectedGap, layout: vertical ? 'column' : 'row', presets: true, customAccent: true, appearanceCycle: true, persistedReload: true, keyboardFocusTrap: true, escapeFocusRestoration: true, outsideClick: true, popupBounds: bounds, overflow: false });
}

try {
  for (const width of [1440, 1280, 1024, 768, 390]) {
    for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
      page.on('pageerror', error => report.errors.push(error.message));
      await page.addInitScript(({ appearanceKey, accentKey, theme }) => {
        if (!localStorage.getItem(appearanceKey)) localStorage.setItem(appearanceKey, theme);
        if (!localStorage.getItem(accentKey)) localStorage.setItem(accentKey, '#2563eb');
      }, { appearanceKey, accentKey, theme });
      try {
        await page.goto(fixture.url + '/admin/');
        await page.getByRole('heading', { name: 'Administrator access', exact: true }).waitFor();
        await inspect(page, width, theme, 'gate');
        await unlock(page);
        await inspect(page, width, theme, 'dashboard');
      } finally { await page.close(); }
    }
  }
  assert.deepEqual(report.errors, [], 'No browser page errors');
  report.passed = true;
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors, output: output.pathname }));
} catch (error) {
  report.passed = false;
  report.failure = error.stack;
  throw error;
} finally {
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
