import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Geometry regression for the reported sidebar screenshots. Fixture data only.
const output = new URL('../../artifacts/web-soft-surfaces/sidebar/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
async function inspect(page, width, theme, language, collapsed = false) {
  if (width <= 700) await page.locator('.mobile-app-bar button').click();
  const root = width <= 700 ? '.mobile-preferences' : '.nav-utilities';
  const group = page.locator(`${root} .runtime-preference-buttons`);
  const controls = group.locator(':scope > button');
  assert.equal(await controls.count(), 3, 'Exactly three compact preference controls');
  await controls.nth(2).click();
  const languageMenu = page.getByRole('menu');
  await languageMenu.waitFor();
  assert.equal(await languageMenu.getByRole('menuitemradio').count(), 7);
  await languageMenu.getByRole('menuitemradio', { name: language === 'en' ? 'English' : '简体中文', exact: true }).click();
  await page.waitForFunction(language => document.documentElement.lang === language, language);
  assert.equal(await page.evaluate(() => localStorage.getItem('webcodex.runtime.language.v1')), language);
  const geometry = await controls.evaluateAll(elements => elements.map(control => {
    const box = node => { const b = node.getBoundingClientRect(); return { x: b.x, y: b.y, width: b.width, height: b.height, right: b.right, bottom: b.bottom, cx: b.x + b.width / 2, cy: b.y + b.height / 2 }; };
    return { ...box(control), text: control.innerText, icon: box(control.querySelector('svg, .mantine-ColorSwatch-root')) };
  }));
  const vertical = width > 700 && (width <= 1100 || collapsed);
  for (let i = 0; i < geometry.length; i++) {
    const control = geometry[i];
    assert.equal(control.text, '', 'Icon controls have no permanent text labels');
    assert.equal(control.width, 36); assert.equal(control.height, 36);
    assert(Math.abs(control.icon.cx - control.cx) <= 1 && Math.abs(control.icon.cy - control.cy) <= 1, 'Icon centered inside button');
    assert(control.x >= 0 && control.right <= width, 'Control within viewport');
    if (vertical) {
      assert(Math.abs(control.cx - geometry[0].cx) <= 1, 'Compact rail uses one icon axis');
      if (i) assert(control.y >= geometry[i - 1].bottom + 7, 'Compact controls have separate targets');
    } else {
      assert(Math.abs(control.cy - geometry[0].cy) <= 1, 'Expanded preferences share one row');
      if (i) assert(control.x >= geometry[i - 1].right + 7, 'Three distinct buttons with consistent gaps');
    }
  }
  await controls.nth(0).click();
  const palette = page.getByRole('dialog');
  await palette.waitFor();
  const swatches = palette.locator('button[aria-pressed]');
  assert.equal(await swatches.count(), 5, 'Five existing presets');
  const bounds = await palette.boundingBox();
  assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1 && bounds.y >= 0 && bounds.y + bounds.height <= 1001, 'Library popover remains in viewport');
  await palette.screenshot({ path: new URL(`palette-${width}-${theme}-${language}${collapsed ? '-collapsed' : ''}.png`, output).pathname });
  await swatches.nth(3).click();
  await palette.waitFor({ state: 'hidden' });
  await page.waitForFunction(() => localStorage.getItem('webcodex.ui.accent.v1') === '#7c3aed');
  await page.waitForFunction(selector => document.activeElement === document.querySelector(selector), `${root} .runtime-preference-buttons > button`);
  await controls.nth(0).click();
  await palette.getByRole('textbox').fill('#135abc');
  await page.waitForFunction(() => localStorage.getItem('webcodex.ui.accent.v1') === '#135abc');
  await page.keyboard.press('Escape');
  await palette.waitFor({ state: 'hidden' });
  if (width <= 700) assert(await page.locator(root).isVisible(), 'Escape closes child popup before mobile preferences');
  await controls.nth(0).click();
  await palette.locator('button[aria-pressed]').first().click();
  const cycle = theme === 'light' ? ['dark', 'system', 'light'] : ['system', 'light', 'dark'];
  for (const appearance of cycle) {
    await controls.nth(1).click();
    await page.waitForFunction(appearance => localStorage.getItem('webcodex.runtime.appearance.v1') === appearance, appearance);
    assert.equal(await page.locator('html').getAttribute('data-theme'), appearance);
  }
  await controls.nth(2).focus(); await page.keyboard.press('Enter');
  await languageMenu.waitFor();
  assert.equal(await languageMenu.locator('[aria-checked="true"]').innerText(), language === 'en' ? 'English' : '简体中文');
  await languageMenu.screenshot({ path: new URL(`language-${width}-${theme}-${language}${collapsed ? '-collapsed' : ''}.png`, output).pathname });
  await page.keyboard.press('ArrowDown'); await page.keyboard.press('Escape');
  await languageMenu.waitFor({ state: 'hidden' });
  assert(await controls.nth(2).evaluate(element => element === document.activeElement), 'Language menu restores focus');
  await page.mouse.move(width - 2, 2);
  await page.waitForTimeout(160); // Let the tooltip close before capture.
  if (width > 700) {
    assert(await page.locator('.profile .runtime-lock-button').isVisible(), 'Separate lock action accessible');
    if (width === 1440 && !collapsed) await page.locator(root).screenshot({ path: new URL(`preferences-${width}-${theme}-${language}.png`, output).pathname, animations: 'disabled' });
    await page.locator('.app-nav').screenshot({ path: new URL(`sidebar-${width}-${theme}-${language}${collapsed ? '-collapsed' : ''}.png`, output).pathname, animations: 'disabled' });
  } else {
    await page.locator('.mobile-preferences').screenshot({ path: new URL(`preferences-${width}-${theme}-${language}.png`, output).pathname, animations: 'disabled' });
    await page.keyboard.press('Escape');
  }
  assert.equal(await page.locator('.ui-selection-rail').count(), 0, 'primary selection has no leftover decorative rail');
  const selected = page.locator('.window-work-row[aria-current="true"]');
  assert.equal(await selected.count(), 1);
  const row = await selected.evaluate(element => {
    const style = getComputedStyle(element);
    const copy = element.querySelector('.window-work-row-main');
    return { shadow: style.boxShadow, border: style.borderTopWidth, background: style.backgroundColor,
      contentBackground: getComputedStyle(document.documentElement).getPropertyValue('--ui-content').trim(),
      metadataLines: copy.querySelectorAll(':scope > small').length,
      width: element.clientWidth, contentWidth: element.scrollWidth };
  });
  assert.equal(row.shadow, 'none', 'selected Window has no inset blue edge');
  assert.equal(row.border, '1px', 'selected Window uses a complete outline');
  assert.notEqual(row.background, 'rgba(0, 0, 0, 0)');
  assert.equal(row.metadataLines, 2, 'Window row has title and two supporting lines');
  assert(row.contentWidth <= row.width + 1, 'long row text stays contained');
  assert(await page.evaluate(() => Math.max(document.body.scrollWidth, document.documentElement.scrollWidth) <= innerWidth + 1));
  await selected.screenshot({ path: new URL(`selection-${width}-${theme}-${language}${collapsed ? '-collapsed' : ''}.png`, output).pathname, animations: 'disabled' });
  report.checks.push({ width, theme, language, collapsed, compactPreferences: true, libraryPopups: true, persistedChanges: true, selectionOutline: true, compactMetadata: true, overflow: false });
}
try {
  for (const width of [1440, 1280, 1024, 768, 390]) {
    for (const theme of ['light', 'dark']) {
      for (const language of ['en', 'zh-CN']) {
        const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
        page.on('pageerror', error => report.errors.push(error.message));
        await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
        await page.goto(fixture.url + '/runtime/');
        await page.getByTestId('work-window-row-' + '4700'.repeat(16)).waitFor();
        await inspect(page, width, theme, language);
        if (width === 1440) {
          await page.locator('.sidebar-collapse').click();
          await inspect(page, width, theme, language, true);

        }
        await page.close();
      }
    }
  }
  assert.deepEqual(report.errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
