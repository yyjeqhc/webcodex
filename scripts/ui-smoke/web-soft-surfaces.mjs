import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Real built renderers against isolated data. No live credentials or mutations.
const output = new URL('../../artifacts/web-soft-surfaces/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
async function inspect(page, renderer, theme, width) {
  await page.waitForFunction(({ renderer, theme }) => {
    const nav = document.querySelector(renderer === 'runtime'
      ? innerWidth <= 700 ? '.mobile-primary-nav button.active' : '.app-nav .nav-button.active'
      : '.section-nav a[aria-current]');
    return nav && getComputedStyle(nav).backgroundColor === (theme === 'light' ? 'rgb(25, 25, 25)' : 'rgb(245, 245, 245)');
  }, { renderer, theme });
  const result = await page.evaluate(({ renderer }) => {
    const root = document.documentElement;
    const nav = document.querySelector(renderer === 'runtime'
      ? innerWidth <= 700 ? '.mobile-primary-nav button.active' : '.app-nav .nav-button.active'
      : '.section-nav a[aria-current]');
    const active = getComputedStyle(nav);
    return {
      renderer: root.dataset.uiRenderer,
      selection: active.backgroundColor,
      foreground: active.color,
      overflow: Math.max(document.body.scrollWidth, root.scrollWidth) > innerWidth + 1,
      blur: [...document.querySelectorAll('body *')].filter(element => {
        const bounds = element.getBoundingClientRect();
        return bounds.width && bounds.height && getComputedStyle(element).backdropFilter !== 'none';
      }).map(element => element.className),
    };
  }, { renderer });
  assert.equal(result.renderer, renderer);
  assert.equal(result.selection, theme === 'light' ? 'rgb(25, 25, 25)' : 'rgb(245, 245, 245)');
  assert.equal(result.foreground, theme === 'light' ? 'rgb(255, 255, 255)' : 'rgb(25, 25, 25)');
  assert.equal(result.overflow, false, `${renderer}/${theme}/${width}: overflow`);
  assert.deepEqual(result.blur, [], `${renderer}/${theme}/${width}: Web blur`);
  report.checks.push({ renderer, theme, width, monochromeSelection: true, opaqueChrome: true, overflow: false });
}
async function capture(page, name) {
  await page.screenshot({ path: new URL(`${name}.png`, output).pathname, fullPage: false, animations: 'disabled' });
}
try {
  for (const width of [1440, 1280, 1024, 768, 390]) {
    for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
      page.on('pageerror', error => report.errors.push(error.message));
      await page.addInitScript(theme => {
        localStorage.setItem('webcodex.runtime.appearance.v1', theme);
        localStorage.setItem('webcodex.admin.appearance.v1', theme);
      }, theme);
      await page.goto(fixture.url + '/runtime/');
      await page.getByTestId('work-window-row-' + '4700'.repeat(16)).waitFor();
      await inspect(page, 'runtime', theme, width);
      await capture(page, `runtime-work-${width}-${theme}`);
      await page.locator('.work-surface-control label').filter({ hasText: 'Goals' }).click();
      await page.getByRole('progressbar', { name: 'Plan', exact: true }).waitFor();
      await inspect(page, 'runtime', theme, width);
      await capture(page, `runtime-goal-${width}-${theme}`);
      const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
      await nav.getByRole('button', { name: /^Projects/ }).click();
      await page.getByTestId('project-card-agent:fixture-runner:alpha').waitFor();
      await inspect(page, 'runtime', theme, width);
      await capture(page, `runtime-projects-${width}-${theme}`);
      const search = page.getByRole('textbox', { name: 'Search projects', exact: true });
      await search.fill('Fixture beta');
      await page.getByTestId('project-card-agent:fixture-runner:beta').waitFor();
      await search.fill('');
      await page.getByTestId('project-card-agent:fixture-runner:alpha').waitFor();
      const addProject = page.getByRole('button', { name: 'Add Project', exact: true });
      await addProject.click();
      const dialog = page.getByRole('dialog', { name: 'Add Project', exact: true });
      await dialog.waitFor();
      await dialog.getByLabel(/Project folder/).focus();
      await page.keyboard.press('Tab');
      assert(await dialog.evaluate(element => element.contains(document.activeElement)), 'project dialog contains keyboard focus');
      await capture(page, `runtime-dialog-${width}-${theme}`);
      await page.keyboard.press('Escape');
      await dialog.waitFor({ state: 'hidden' });
      await page.waitForFunction(() => document.activeElement?.textContent?.includes('Add Project'));
      assert(await addProject.evaluate(element => element === document.activeElement), 'project dialog restores focus');
      await nav.getByRole('button', { name: /^Runtime/ }).click();
      await page.getByRole('heading', { name: 'Runner fleet', exact: true }).waitFor();
      await inspect(page, 'runtime', theme, width);
      await capture(page, `runtime-overview-${width}-${theme}`);
      await page.goto(fixture.url + '/admin/');
      await page.getByLabel('Admin token').fill('fixture-only-not-a-secret');
      await page.getByRole('button', { name: 'Unlock console' }).click();
      await page.getByRole('heading', { name: 'Operations', exact: true }).waitFor();
      await inspect(page, 'admin', theme, width);
      const lockInk = await page.getByRole('button', { name: 'Lock', exact: true }).evaluate(element =>
        getComputedStyle(element).color.match(/\d+/g).slice(0, 3).map(Number));
      assert(theme === 'dark' ? Math.min(...lockInk) >= 170 : Math.max(...lockInk) <= 110,
        `Admin ${theme}: Lock must retain readable contrast`);
      const badge = page.locator('.admin-status-badge[data-tone="good"]').first();
      const colors = await badge.evaluate(element => {
        const style = getComputedStyle(element);
        return [style.backgroundColor, style.color];
      });
      for (const color of colors) {
        const channels = color.match(/\d+/g).slice(0, 3).map(Number);
        assert.equal(Math.max(...channels) - Math.min(...channels), 0, `normal status must be neutral: ${color}`);
      }
      await capture(page, `admin-overview-${width}-${theme}`);
      await page.getByRole('button', { name: 'Create project', exact: true }).click();
      const adminDialog = page.getByRole('dialog');
      await adminDialog.waitFor();
      await capture(page, `admin-dialog-${width}-${theme}`);
      await page.keyboard.press('Escape');
      await adminDialog.waitFor({ state: 'hidden' });
      // An accent change persists, but never recolors primary navigation.
      await page.getByRole('button', { name: 'Accent color', exact: true }).click();
      await page.getByRole('button', { name: 'Violet', exact: true }).click();
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1')), '#7c3aed');
      await inspect(page, 'admin', theme, width);
      await page.getByRole('button', { name: 'Lock', exact: true }).click();
      await page.getByRole('heading', { name: 'Administrator access', exact: true }).waitFor();
      await capture(page, `admin-auth-${width}-${theme}`);
      await page.goto(fixture.url + '/runtime/');
      await page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav').getByRole('button', { name: /^Work/ }).click();
      await page.getByTestId('work-window-row-' + '4700'.repeat(16)).waitFor();
      if (width <= 700) await page.getByRole('button', { name: 'Preferences', exact: true }).click();
      await page.getByRole('button', { name: 'Lock', exact: true }).click();
      await page.getByRole('heading', { name: 'Connect to your workspace' }).waitFor();
      await capture(page, `runtime-auth-${width}-${theme}`);
      await page.close();
    }
  }
  // The shared foundation must retain its previous Desktop material and palette.
  const desktop = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await desktop.goto(fixture.url + '/desktop/');
  await desktop.locator('[data-webcodex-page="home"]').waitFor();
  const desktopStyle = await desktop.evaluate(() => ({
    renderer: document.documentElement.dataset.uiRenderer,
    background: getComputedStyle(document.documentElement).getPropertyValue('--ui-background').trim(),
  }));
  assert.equal(desktopStyle.renderer, undefined);
  assert.equal(desktopStyle.background, '#f5f5f7');
  report.checks.push({ renderer: 'desktop', unchangedFoundation: true });
  await desktop.close();
  assert.deepEqual(report.errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
