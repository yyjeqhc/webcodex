import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startFixtureServer } from './server.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = path.join(root, 'artifacts/liquid-glass-ui');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({ headless: true, ...(fs.existsSync(chrome) ? { executablePath: chrome } : {}) });
const report = { fixture: true, nativeBackend: false, checks: [], screenshots: [], errors: [] };
let page;
async function screenshot(name) {
  const filename = name + '.png';
  const viewport = page.viewportSize();
  await page.mouse.move(viewport.width - 2, viewport.height - 2);
  // Let finite entry transitions settle before recording a visual baseline.
  await page.waitForTimeout(360);
  await page.screenshot({ path: path.join(output, filename), fullPage: false, animations: 'disabled', caret: 'hide' });
  report.screenshots.push(filename);
}
async function noOverflow(label) {
  const sizes = await page.evaluate(() => ({ width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
  assert(sizes.body <= sizes.width + 1 && sizes.root <= sizes.width + 1, `${label}: horizontal overflow ${JSON.stringify(sizes)}`);
  report.checks.push(label);
}
async function themeSurfaceAudit(label, theme) {
  const mismatches = await page.evaluate(expectedTheme => [...document.querySelectorAll('body *')].flatMap(element => {
    const rect = element.getBoundingClientRect();
    if (rect.width < 180 || rect.height < 40 || rect.width * rect.height < 18000) return [];
    const color = getComputedStyle(element).backgroundColor;
    const channels = color.match(/[\d.]+/g)?.map(Number);
    if (!channels || (channels.length > 3 && channels[3] < .99)) return [];
    const rgb = channels.slice(0, 3);
    const mismatched = expectedTheme === 'light' ? Math.max(...rgb) < 105 : Math.min(...rgb) > 185;
    return mismatched ? [{ element: `${element.tagName.toLowerCase()}.${String(element.className).trim().replaceAll(' ', '.')}`, color,
      width: Math.round(rect.width), height: Math.round(rect.height) }] : [];
  }).slice(0, 8), theme);
  assert.deepEqual(mismatches, [], `${label}: large surface conflicts with ${theme} theme`);
  report.checks.push(`${label} ${theme} surface parity`);
}
async function activeRail(selector, label) {
  assert.equal(await page.locator(selector).count(), 1, `${label}: selected navigation rail missing or duplicated`);
  report.checks.push(`${label} selected navigation rail`);
}
async function alignedRows(rows, anchor, label) {
  const positions = await page.locator(rows).evaluateAll((elements, child) => elements.slice(0, 4)
    .map(element => element.querySelector(child)?.getBoundingClientRect().left)
    .filter(value => typeof value === 'number'), anchor);
  if (positions.length > 1) assert(positions.every(value => Math.abs(value - positions[0]) <= 1), `${label}: row content starts differ ${positions}`);
  report.checks.push(`${label} row alignment`);
}
async function longTextBounded(selector, label, capture = false) {
  const target = page.locator(selector).first();
  const original = await target.textContent();
  await target.evaluate(element => { element.textContent = '/extremely-long-workspace-directory'.repeat(16); });
  await noOverflow(`${label} long text`);
  const textBounds = await target.boundingBox();
  assert(textBounds && textBounds.height <= 64, `${label}: long text grows the row vertically ${JSON.stringify(textBounds)}`);
  if (capture) {
    await target.scrollIntoViewIfNeeded();
    await screenshot(`${label.toLowerCase().replaceAll(' ', '-')}-long-text`);
  }
  await target.evaluate((element, value) => { element.textContent = value; }, original);
  if (capture) await page.evaluate(() => {
    window.scrollTo({ top: 0, behavior: 'instant' });
    document.querySelectorAll('.table-wrap, .workspace-project-table').forEach(element => { element.scrollLeft = 0; });
  });
}
async function visibleKeyboardFocus(locator, label) {
  await page.keyboard.press('Tab');
  await locator.focus();
  const focus = await locator.evaluate(el => ({
    visible: el.matches(':focus-visible'),
    outline: getComputedStyle(el).outlineStyle,
  }));
  assert(focus.visible && focus.outline !== 'none', `${label}: keyboard focus must be visible ${JSON.stringify(focus)}`);
  report.checks.push(`${label} keyboard focus`);
}
async function accentChoice(locator, label, color, capture = false) {
  await locator.locator('summary').click();
  const panel = page.getByRole('dialog', { name: 'Accent color', exact: true });
  await panel.waitFor();
  const bounds = await panel.boundingBox();
  assert(bounds && bounds.x >= -1 && bounds.x + bounds.width <= page.viewportSize().width + 1, `${label}: accent palette exceeds viewport ${JSON.stringify(bounds)}`);
  if (capture) await screenshot(`${label.toLowerCase().replaceAll(' ', '-')}-accent-picker`);
  await panel.getByRole('button', { name: color }).click();
  assert.equal(await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1')), ({ Blue: '#2563eb', Indigo: '#4f46e5', Teal: '#0f766e', Violet: '#7c3aed', Orange: '#c2410c' })[color]);
  assert.equal(await panel.getByRole('button', { name: color }).getAttribute('aria-pressed'), 'true');
  report.checks.push(`${label} preset, selection, and palette bounds`);
  await page.keyboard.press('Escape');
  if (await locator.count()) assert.equal(await locator.getAttribute('open'), null);
}
async function restoreDefaultAccent(locator) {
  await locator.locator('summary').click();
  await page.getByRole('dialog', { name: 'Accent color', exact: true }).getByRole('button', { name: 'Blue' }).click();
  await page.keyboard.press('Escape');
}
async function accessibilityPreferences(context, materialSelector, motionSelector, label) {
  const cdp = await context.newCDPSession(page);
  await cdp.send('Emulation.setEmulatedMedia', { features: [
    { name: 'prefers-reduced-transparency', value: 'reduce' },
    { name: 'prefers-reduced-motion', value: 'reduce' },
  ] });
  await page.waitForFunction(selector => getComputedStyle(document.querySelector(selector)).backdropFilter === 'none', materialSelector);
  const styles = await page.evaluate(({ materialSelector, motionSelector }) => ({
    blur: getComputedStyle(document.querySelector(materialSelector)).backdropFilter,
    transition: getComputedStyle(document.querySelector(motionSelector)).transitionDuration,
  }), { materialSelector, motionSelector });
  assert.equal(styles.blur, 'none', `${label}: reduced transparency should remove blur`);
  assert(parseFloat(styles.transition) <= .001, `${label}: reduced motion should remove transitions (${styles.transition})`);
  report.checks.push(`${label} reduced transparency and motion`);
  await cdp.detach();
}
async function desktopNav(name) {
  await page.locator(`[data-webcodex-action="navigate-${name}"]`).click();
  await page.locator(`[data-webcodex-page="${name}"]`).waitFor();
}
async function inspectProjectPicker(selector, label, capture = false) {
  const picker = page.locator(selector);
  assert.equal(await picker.locator('select').count(), 0, `${label}: project filter should not use a native select`);
  const trigger = picker.locator('.project-picker-trigger');
  await trigger.click();
  const menu = page.getByRole('dialog', { name: /Projects|Project/ });
  await menu.waitFor();
  const bounds = await menu.boundingBox();
  const viewport = page.viewportSize();
  assert(bounds && bounds.x >= -1 && bounds.x + bounds.width <= viewport.width + 1 && bounds.y >= -1 && bounds.y + bounds.height <= viewport.height + 1, `${label}: menu exceeds viewport ${JSON.stringify(bounds)}`);
  if (capture) await screenshot(`${label.toLowerCase().replaceAll(' ', '-')}-project-picker`);
  await menu.getByRole('button', { name: /beta/i }).click();
  assert.match(await trigger.innerText(), /beta/i);
  await trigger.click();
  await page.getByRole('dialog', { name: /Projects|Project/ }).getByRole('button', { name: 'All Projects' }).click();
  report.checks.push(`${label} project picker bounds, exact selection, and reset`);
}
async function inspectRunnerPicker(label, capture = false) {
  const picker = page.locator('.runner-picker');
  assert.equal(await picker.locator('select').count(), 0);
  const trigger = picker.locator('.project-picker-trigger');
  await trigger.click();
  const menu = page.getByRole('dialog', { name: 'Runner' });
  await menu.waitFor();
  const bounds = await menu.boundingBox();
  const viewport = page.viewportSize();
  assert(bounds && bounds.x >= -1 && bounds.x + bounds.width <= viewport.width + 1 && bounds.y >= -1 && bounds.y + bounds.height <= viewport.height + 1, `${label}: runner menu exceeds viewport`);
  if (capture) await screenshot(`${label.toLowerCase().replaceAll(' ', '-')}-runner-picker`);
  await menu.getByRole('button', { name: 'fixture-runner' }).click();
  assert.match(await trigger.innerText(), /fixture-runner/);
  await trigger.click();
  await page.getByRole('dialog', { name: 'Runner' }).getByRole('button', { name: 'All Runners' }).click();
  report.checks.push(`${label} runner picker bounds, exact selection, and reset`);
}
async function prepare(route, width = 1440, height = 900) {
  const context = await browser.newContext({ viewport: { width, height }, colorScheme: 'light' });
  page = await context.newPage(); page.setDefaultTimeout(10000);
  page.on('pageerror', error => report.errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') report.errors.push(message.text()); });
  await page.goto(fixture.url + route);
  return context;
}
try {
  for (const width of [1440, 1280, 1024, 768, 390]) {
    const context = await prepare('/desktop/', width, width === 390 ? 844 : 900);
    await page.locator('[data-webcodex-page="home"]').waitFor();
    await activeRail('.sidebar nav button.active .sidebar-selection', `Desktop ${width}`);
    if (width === 1440) {
      assert.equal(await page.locator('.sidebar-preferences select').count(), 0, 'Desktop sidebar preferences should use compact controls');
      const rail = await page.locator('.sidebar nav button.active .sidebar-selection').evaluate(el => ({
        markerWidth: getComputedStyle(el, '::before').width,
        markerTop: getComputedStyle(el, '::before').top,
      }));
      assert.deepEqual(rail, { markerWidth: '3px', markerTop: '8px' }, 'Desktop navigation rail should match Runtime');
      const language = page.locator('.sidebar-preferences [data-webcodex-control="locale"]');
      await language.click();
      await page.getByRole('menuitem', { name: 'English' }).waitFor();
      await page.keyboard.press('Escape');
      report.checks.push('Desktop sidebar compact language control and Runtime-matched rail');
      const picker = page.locator('.sidebar-preferences .accent-picker');
      await accentChoice(picker, 'Desktop 1440', 'Violet', true);
      await picker.locator('summary').click();
      await page.getByRole('dialog', { name: 'Accent color', exact: true }).getByLabel('Custom color').fill('#d946ef');
      assert.equal(await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1')), '#d946ef');
      await page.reload();
      await page.locator('[data-webcodex-page="home"]').waitFor();
      assert.equal(await page.locator('html').getAttribute('data-accent'), 'custom');
      report.checks.push('Desktop custom accent persists after reload');
      await restoreDefaultAccent(page.locator('.sidebar-preferences .accent-picker'));
    }
    await noOverflow(`Desktop home ${width}`);
    if (width === 1440) await themeSurfaceAudit('Desktop Home', 'light');
    if (width <= 600) assert.equal(await page.locator('.workspace-project-mobile-row').count(), 2, `Desktop ${width}: compact project list missing`);
    else assert.equal(await page.locator('.workspace-project-table table').count(), 1, `Desktop ${width}: recent projects table missing`);
    const projectBadge = await page.locator('.project-current-badge').first().evaluate(el => ({
      foreground: getComputedStyle(el).color, background: getComputedStyle(el).backgroundColor,
    }));
    assert.notEqual(projectBadge.foreground, projectBadge.background, `Desktop ${width}: current project badge text blends into its background`);
    report.checks.push(`Desktop ${width} responsive project component`);
    if (width === 1440) await visibleKeyboardFocus(page.locator('[data-webcodex-action="navigate-home"]'), 'Desktop');
    await screenshot(`desktop-home-${width}`);
    for (const name of ['projects', 'activity', 'connection', 'extensions', 'settings']) {
      await desktopNav(name);
      if (name === 'projects') {
        if (width <= 600) {
          await alignedRows('.workspace-project-mobile-row', '.mantine-NavLink-body', `Desktop ${width}`);
          await longTextBounded('.workspace-project-mobile-row [title="/fixture/alpha"]', 'Desktop 390', true);
        } else await alignedRows('.workspace-project-table tbody tr', '.project-table-name', `Desktop ${width}`);
      }
      if (name === 'activity' && (width === 1440 || width === 390)) await inspectProjectPicker('.activity-project-filter', `Desktop ${width}`, true);
      await noOverflow(`Desktop ${name} ${width}`);
      if (width === 1440) await themeSurfaceAudit(`Desktop ${name}`, 'light');
      await screenshot(`desktop-${name}-${width}`);
      if (name === 'projects' && width === 390) {
        await visibleKeyboardFocus(page.locator('.workspace-project-mobile-row').first(), 'Desktop compact project');
        await page.locator('.workspace-project-mobile-row').filter({ hasText: '/fixture/beta' }).click();
        await page.waitForFunction(() => window.__fixtureCalls.some(call => call.cmd === 'activate_local_project' && call.args.request.projectPath === '/fixture/beta'));
        report.checks.push('Desktop compact project opens exact path');
      }
    }
    await desktopNav('connection');
    const connectionTrigger = page.getByRole('button', { name: 'Add Connection' });
    await connectionTrigger.click();
    await page.getByRole('dialog').waitFor();
    assert.equal(await page.evaluate(() => Boolean(document.activeElement?.closest('[role="dialog"]'))), true);
    await screenshot(`desktop-connection-dialog-${width}`);
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await page.waitForFunction(() => document.activeElement instanceof HTMLButtonElement && document.activeElement.textContent?.includes('Add Connection'));
    report.checks.push(`Desktop connection dialog Escape ${width}`);
    await page.getByRole('button', { name: 'Add Connection' }).click();
    await page.getByRole('dialog').getByLabel('Name').fill('Fixture connection');
    await page.getByRole('dialog').getByLabel('Tunnel ID').fill('tunnel_ui_fixture');
    await page.getByRole('dialog').getByLabel('API Key').fill('fixture-only-api-key');
    await page.getByRole('dialog').getByRole('button', { name: 'Save & Apply' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    const saved = await page.evaluate(() => window.__fixtureCalls.find(call => call.cmd === 'save_tunnel_profile'));
    assert.equal(saved.args.request.tunnel_id, 'tunnel_ui_fixture');
    assert.equal(saved.args.request.api_key, 'fixture-only-api-key');
    await page.locator('[data-tunnel-profile-id="fixture-connection"]').waitFor();
    assert.equal((await page.locator('body').innerText()).includes('fixture-only-api-key'), false);
    report.checks.push(`Connection profile save keeps API key out of UI ${width}`);
    if (width === 1440) {
      await desktopNav('projects');
      await page.locator('.workspace-project-table tbody tr').filter({ hasText: '/fixture/beta' }).getByRole('button', { name: /Use project/ }).click();
      await page.waitForFunction(() => window.__fixtureCalls.some(call => call.cmd === 'activate_local_project' && call.args.request.projectPath === '/fixture/beta'));
      await page.locator('[data-webcodex-action="add-project"]').click();
      await page.locator('.workspace-project-table tbody tr').filter({ hasText: '/fixture/gamma' }).waitFor();
      assert.equal(await page.locator('.workspace-project-table tbody tr').count(), 3);
      report.checks.push('One Runner: select and add exact project roots');
      await desktopNav('extensions');
      assert.equal(await page.getByRole('tab', { name: 'Coding Agents' }).getAttribute('aria-selected'), 'true');
      assert.equal(await page.locator('.activity-project-filter').count(), 0);
      await page.getByRole('button', { name: 'Authorize Runner Capabilities', exact: true }).click();
      await page.getByRole('dialog').waitFor();
      assert.equal(await page.evaluate(() => window.__fixtureCalls.some(call => call.cmd === 'authorize_runner_capabilities')), false);
      await page.getByRole('button', { name: 'Confirm Authorize Runner Capabilities', exact: true }).click();
      await page.getByRole('dialog').waitFor({ state: 'hidden' });
      assert.equal(await page.evaluate(() => window.__fixtureCalls.some(call => call.cmd === 'ssh_resource_list')), false);
      await page.getByRole('button', { name: 'Add Coding Agent', exact: true }).click();
      const codingDialog = page.getByRole('dialog');
      await codingDialog.getByLabel('Name', { exact: true }).fill('UI ACP Fixture');
      await codingDialog.getByLabel('Provider ID', { exact: true }).fill('ui-acp-fixture');
      await codingDialog.getByLabel('Executable', { exact: true }).fill('/fixture/acp');
      await screenshot('desktop-acp-dialog-1440');
      await codingDialog.getByRole('button', { name: 'Save', exact: true }).click();
      await codingDialog.waitFor({ state: 'hidden' });
      await page.getByText('Saved · Restart Runner to apply', { exact: true }).waitFor();
      assert.equal(await page.evaluate(() => window.__fixtureCalls.some(call => call.cmd === 'restart_owned_runner')), false);
      await page.getByRole('button', { name: 'Restart Runner', exact: true }).click();
      await page.getByText('Configured · Active', { exact: true }).waitFor();
      await screenshot('desktop-acp-active-1440');
      await page.getByRole('tab', { name: 'SSH Resources', exact: true }).click();
      await page.getByRole('button', { name: 'Add SSH Resource', exact: true }).click();
      await page.getByRole('dialog').getByLabel('Resource Name', { exact: true }).waitFor();
      await screenshot('desktop-ssh-dialog-1440');
      await page.keyboard.press('Escape');
      await page.getByRole('dialog').waitFor({ state: 'hidden' });
      assert.equal(await page.locator('.activity-project-filter').count(), 0);
      report.checks.push('Merged Coding Agents-only explicit authorization, deferred activation, and SSH dialog');
      await page.getByRole('tab', { name: 'Instructions', exact: true }).click();
      await page.getByRole('button', { name: 'Manage' }).click();
      await page.getByLabel('Global instruction files').fill('/fixture/AGENTS.md');
      await page.getByLabel('Configured Skill roots').fill('/fixture/skills-next');
      await page.locator('[data-webcodex-action="save-runner-settings"]').click();
      await page.waitForFunction(() => window.__fixtureCalls.some(call => call.cmd === 'update_runner_settings'));
      const request = await page.evaluate(() => window.__fixtureCalls.find(call => call.cmd === 'update_runner_settings').args.request);
      assert.equal(request.target.client_id, 'fixture-runner');
      assert.deepEqual(request.paths, { instruction_files: ['/fixture/AGENTS.md'], skill_roots: ['/fixture/skills-next'] });
      await page.getByRole('tab', { name: 'MCP Providers' }).click();
      await page.getByText('Advanced: Native Tool Plugins', { exact: true }).click();
      await page.getByText('Add a native Tool Plugin', { exact: true }).click();
      await page.getByLabel('Plugin ID', { exact: true }).fill('ui-fixture-plugin');
      await page.getByLabel('Display name', { exact: true }).fill('UI fixture plugin');
      await page.getByLabel('Executable', { exact: true }).fill('node');
      await page.getByLabel('Arguments (JSON array)', { exact: true }).fill('["/fixture/plugin.js"]');
      await page.locator('[data-webcodex-action="add-plugin-registration"]').click();
      await page.waitForFunction(() => window.__fixtureCalls.some(call => call.cmd === 'add_runner_plugin'));
      const pluginRequest = await page.evaluate(() => window.__fixtureCalls.find(call => call.cmd === 'add_runner_plugin').args.request);
      assert.equal(pluginRequest.target.client_id, 'fixture-runner');
      assert.deepEqual(pluginRequest.provider, { id: 'ui-fixture-plugin', name: 'UI fixture plugin', command: 'node', args: ['/fixture/plugin.js'], cwd: null });
      report.checks.push('Exact Runner instruction/Skill save and native Plugin registration');
      await page.keyboard.press('Control+3');
      await page.locator('[data-webcodex-page="activity"]').waitFor();
      report.checks.push('Desktop keyboard navigation');
    }
    await desktopNav('home');
    if (width <= 768) {
      await desktopNav('settings');
      if (width === 390) {
        const picker = page.locator('.settings-page .accent-picker');
        await accentChoice(picker, 'Desktop 390', 'Orange', true);
        await restoreDefaultAccent(picker);
      }
    }
    if (width > 768) {
      const appearance = page.locator('.sidebar-preferences [data-webcodex-control="appearance"]');
      for (let attempt = 0; attempt < 3 && await page.locator('html').getAttribute('data-theme') !== 'dark'; attempt++) await appearance.click();
      report.checks.push(`Desktop ${width} compact appearance control`);
    } else await page.locator('[data-webcodex-control="appearance"]:visible').last().selectOption('dark');
    if (width <= 768) await desktopNav('home');
    if (width === 390) {
      await page.waitForFunction(() => window.scrollY === 0);
      report.checks.push('Desktop mobile navigation returns to page start');
    }
    assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
    await noOverflow(`Desktop dark ${width}`);
    await screenshot(`desktop-dark-${width}`);
    for (const name of ['projects', 'activity', 'connection', 'extensions', 'settings']) {
      await desktopNav(name);
      if (name === 'activity' && (width === 1440 || width === 390)) await inspectProjectPicker('.activity-project-filter', `Desktop dark ${width}`, true);
      await noOverflow(`Desktop dark ${name} ${width}`);
      await screenshot(`desktop-dark-${name}-${width}`);
    }
    await desktopNav('home');
    if (width === 1440) {
      await page.reload();
      await page.locator('[data-webcodex-page="home"]').waitFor();
      assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
      report.checks.push('Desktop dark appearance persists');
      await accessibilityPreferences(context, '.sidebar', '.sidebar nav button', 'Desktop');
    }
    await context.close();
  }
  {
    const context = await prepare('/desktop/?state=disconnected');
    await desktopNav('connection');
    await page.getByRole('button', { name: 'Add Connection' }).click();
    assert(await page.getByRole('dialog').getByLabel('Tunnel ID').isEditable());
    assert(await page.getByRole('dialog').getByLabel('API Key').isEditable());
    report.checks.push('Disconnected Tunnel credentials stay editable');
    await context.close();
  }
  {
    const context = await prepare('/desktop/?permissions');
    await page.getByRole('dialog').waitFor();
    await screenshot('desktop-permissions');
    const before = await page.evaluate(() => window.__fixtureCalls.filter(call => call.cmd === 'request_computer_permission').length);
    assert.equal(before, 0);
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    report.checks.push('Foreground permission explanation: native dialog and Escape, no automatic request');
    await context.close();
  }
  for (const width of [1440, 1280, 1024, 768, 390]) {
    const context = await prepare('/runtime/', width, width === 390 ? 844 : 900);
    await page.locator('.work-surface-control').waitFor();
    assert.equal(await page.getByRole('radio', { name: /Sessions/ }).isChecked(), true);
    await page.locator('.work-surface-control label').filter({ hasText: 'Goals' }).click();
    await page.getByRole('progressbar', { name: 'Plan' }).waitFor();
    assert.equal(await page.locator('.goal-plan-stepper .mantine-Stepper-step').count(), 3, `Runtime ${width}: Goal plan must use three Mantine steps`);
    assert.equal(await page.locator('.goal-plan-stepper .mantine-Stepper-step[data-progress]').count(), 1, `Runtime ${width}: current Goal step must be distinct`);
    report.checks.push(`Runtime ${width} segmented work switch and progress`);
    await activeRail('.app-nav .nav-button.active .ui-selection-rail', `Runtime ${width}`);
    if (width === 1440) {
      const picker = page.locator('.nav-utilities .accent-picker');
      await accentChoice(picker, 'Runtime 1440', 'Indigo', true);
      await restoreDefaultAccent(picker);
    }
    if (width === 1440) {
      await visibleKeyboardFocus(page.locator('.app-nav .nav-button').first(), 'Runtime');
      await page.locator('.sidebar-collapse').click();
      assert.equal(await page.locator('.app-shell').evaluate(el => el.classList.contains('sidebar-collapsed')), true);
      await page.waitForFunction(() => document.querySelector('.app-nav').getBoundingClientRect().width <= 71);
      assert.equal(await page.locator('.app-nav .brand-mark').isVisible(), true, 'Collapsed sidebar must retain the Logo');
      assert.equal(await page.locator('.app-nav .nav-button.active .nav-icon').isVisible(), true, 'Collapsed selection must retain its icon');
      assert.equal(await page.locator('.app-nav .nav-button.active .nav-label').isVisible(), false, 'Collapsed selection must not show text');
      await page.locator('.sidebar-collapse').click();
      report.checks.push('Runtime sidebar collapse and expand with Logo and selected icon');
    }
    if (width === 390) {
      await page.locator('.mobile-app-bar button').click();
      await page.locator('.mobile-preferences').waitFor();
      const picker = page.locator('.mobile-preferences .accent-picker');
      await accentChoice(picker, 'Runtime 390', 'Orange', true);
      await page.locator('.mobile-app-bar button').click();
      await page.locator('.mobile-preferences').waitFor();
      await restoreDefaultAccent(picker);
      await page.keyboard.press('Escape');
      await page.locator('.mobile-preferences').waitFor({ state: 'hidden' });
      report.checks.push('Runtime mobile preferences and Escape');
    }
    if (width === 1440 || width === 390) await inspectProjectPicker('.goal-list-filters', `Runtime ${width}`, true);
    await screenshot(`runtime-goals-${width}`);
    await page.locator('.work-surface-control label').filter({ hasText: 'Sessions' }).click();
    await page.getByTestId('work-row-' + 'wc_sess_fixture470_active').waitFor();
    await noOverflow(`Runtime Work ${width}`);
    await screenshot(`runtime-work-${width}`);
    await page.getByTestId('work-row-' + 'wc_sess_fixture470_active').click();
    await page.locator('.session-main').waitFor();
    if (width === 1440) await themeSurfaceAudit('Runtime Session', 'light');
    await screenshot(`runtime-session-${width}`);
    if (width === 1440) {
      await page.getByRole('tab', { name: /Collaboration/ }).click();
      const draft = page.getByRole('textbox', { name: 'Send a message to this work session…' });
      await draft.fill('Fixture unsent draft');
      await page.getByRole('tab', { name: 'Workflow' }).click();
      await page.getByRole('tab', { name: /Collaboration/ }).click();
      assert.equal(await draft.inputValue(), 'Fixture unsent draft');
      report.checks.push('Runtime Session draft survives view changes');
    }
    const nav = width <= 700 ? '.mobile-primary-nav button' : '.app-nav .nav-button';
    await page.locator(nav).filter({ hasText: 'Projects' }).click();
    await page.getByRole('heading', { name: 'Projects' }).waitFor();
    await page.locator('.project-card').first().waitFor();
    await alignedRows('.project-card', '.project-card-head > span:nth-child(2)', `Runtime ${width} projects`);
    await alignedRows('.project-session-row', '.project-session-main', `Runtime ${width} sessions`);
    if (width === 390) await longTextBounded('.project-card-head small', 'Runtime 390', true);
    if (width === 1440 || width === 390) await inspectRunnerPicker(`Runtime ${width}`, true);
    await noOverflow(`Runtime Projects ${width}`);
    if (width === 1440) await themeSurfaceAudit('Runtime Projects', 'light');
    await screenshot(`runtime-projects-${width}`);
    if (width === 1440 || width === 390) {
      const addProjectTrigger = page.getByRole('button', { name: 'Add Project' });
      await addProjectTrigger.click();
      const addProjectDialog = page.getByRole('dialog', { name: 'Add Project' });
      await addProjectDialog.waitFor();
      assert.equal(await page.evaluate(() => Boolean(document.activeElement?.closest('[role="dialog"]'))), true);
      await screenshot(`runtime-add-project-dialog-${width}`);
      await page.keyboard.press('Escape');
      await addProjectDialog.waitFor({ state: 'hidden' });
      await page.waitForFunction(() => document.activeElement instanceof HTMLButtonElement && document.activeElement.textContent?.includes('Add Project'));
      report.checks.push(`Runtime Add Project dialog focus and Escape ${width}`);
    }
    await page.locator(nav).filter({ hasText: 'Runtime' }).click();
    await page.getByRole('heading', { name: 'Runtime', exact: true }).waitFor();
    await noOverflow(`Runtime Overview ${width}`);
    if (width === 1440) await themeSurfaceAudit('Runtime Overview', 'light');
    await screenshot(`runtime-overview-${width}`);
    if (width === 1440) {
      await page.getByRole('button', { name: /View activity/ }).click();
      await page.locator('.window-work-row').first().waitFor();
      await page.locator('.window-work-row').first().click();
      await page.locator('.window-work-header').waitFor();
      await themeSurfaceAudit('Runtime Window', 'light');
      const windowCalls = fixture.requests.filter(request => request.route === 'window');
      assert(windowCalls.length > 0);
      assert.equal(windowCalls.at(-1).payload.client_window_key, '4700'.repeat(16));
      report.checks.push('Window observation uses the exact Window key');
      await screenshot('runtime-window-activity');
    }
    await page.evaluate(() => localStorage.setItem('webcodex.runtime.appearance.v1', 'dark'));
    await page.reload();
    await page.getByRole('heading', { name: 'Runtime', exact: true }).waitFor();
    assert.equal(await page.locator('html').getAttribute('data-resolved-theme'), 'dark');
    await noOverflow(`Runtime dark ${width}`);
    await screenshot(`runtime-dark-${width}`);
    await page.locator(nav).filter({ hasText: 'Work' }).click();
    await page.locator('.work-surface-control').waitFor();
    await page.locator('.work-surface-control label').filter({ hasText: 'Goals' }).click();
    if (width === 1440 || width === 390) await inspectProjectPicker('.goal-list-filters', `Runtime dark ${width}`, true);
    if (width === 1440) {
      const identityBackground = await page.locator('.goal-inspector .fact-list > div').first().evaluate(el => getComputedStyle(el).backgroundColor);
      assert.notEqual(identityBackground, 'rgb(255, 255, 255)', 'Runtime dark Goal identity must use a dark content surface');
      report.checks.push('Runtime dark Goal identity surface contrast');
    }
    await noOverflow(`Runtime dark Goals ${width}`);
    await screenshot(`runtime-goals-dark-${width}`);
    if (width === 1440) {
      await page.locator('.work-surface-control label').filter({ hasText: 'Sessions' }).click();
      await page.getByTestId('work-row-' + 'wc_sess_fixture470_active').click();
      const signalsSurface = await page.locator('.activity-signals-card').evaluate(el => {
        const [r, g, b] = getComputedStyle(el).backgroundColor.match(/\d+/g).slice(0, 3).map(Number);
        return { r, g, b };
      });
      assert(Math.max(signalsSurface.r, signalsSurface.g, signalsSurface.b) < 90,
        `Runtime dark Activity signals must use a dark surface: ${JSON.stringify(signalsSurface)}`);
      const signalText = await page.locator('.activity-signals-heading strong').evaluate(el => getComputedStyle(el).color.match(/\d+/g).slice(0, 3).map(Number));
      assert(Math.min(...signalText) > 175, `Runtime dark Activity signals heading must remain readable: ${signalText}`);
      report.checks.push('Runtime dark Activity signals surface contrast');
      await screenshot('runtime-session-dark-1440');
      await themeSurfaceAudit('Runtime Session', 'dark');
      await page.getByRole('tab', { name: /Collaboration/ }).click();
      await page.locator('.collaboration-workspace .composer-row').waitFor();
      const composerBackground = await page.locator('.collaboration-workspace .composer-row').evaluate(el => getComputedStyle(el).backgroundColor);
      assert.notEqual(composerBackground, 'rgb(255, 255, 255)', 'Runtime dark composer must use a dark content surface');
      const tabsBackground = await page.locator('.inspector .segmented').evaluate(el => getComputedStyle(el).backgroundColor);
      assert.notEqual(tabsBackground, 'rgb(255, 255, 255)', 'Runtime dark inspector tabs must use a dark surface');
      await screenshot('runtime-session-collaboration-dark-1440');
      report.checks.push('Runtime dark Session composer and inspector surface contrast');
    }
    await page.locator(nav).filter({ hasText: 'Projects' }).click();
    await page.getByRole('heading', { name: 'Projects' }).waitFor();
    await page.locator('.project-card').first().waitFor();
    if (width === 1440 || width === 390) await inspectRunnerPicker(`Runtime dark ${width}`, true);
    if (width === 1440) {
      const runnerBackground = await page.locator('.runner-picker .project-picker-trigger').evaluate(el => getComputedStyle(el).backgroundColor);
      assert.notEqual(runnerBackground, 'rgb(255, 255, 255)', 'Runtime dark Runner control must use a dark surface');
      report.checks.push('Runtime dark Runner control surface contrast');
    }
    await noOverflow(`Runtime dark Projects ${width}`);
    await screenshot(`runtime-projects-dark-${width}`);
    report.checks.push(`Runtime dark appearance persists ${width}`);
    if (width === 1440) await accessibilityPreferences(context, '.app-nav', '.nav-button', 'Runtime');
    await context.close();
  }
  {
    const context = await prepare('/runtime/', 2560, 1229);
    await page.locator('.work-surface-control label').filter({ hasText: 'Goals' }).click();
    await page.getByRole('progressbar', { name: 'Plan' }).waitFor();
    await screenshot('runtime-goals-2560');
    await page.locator('.work-surface-control label').filter({ hasText: 'Sessions' }).click();
    await page.getByTestId('work-row-wc_sess_fixture470_active').click();
    await page.locator('.activity-signals-card').waitFor();
    const sessionLayout = await page.evaluate(() => {
      const bounds = selector => {
        const { x, width } = document.querySelector(selector).getBoundingClientRect();
        return { x, width };
      };
      return {
        pane: bounds('.workflow-pane'),
        measure: bounds('.workflow-pane .timeline-measure'),
        signals: bounds('.workflow-pane .activity-signals-card'),
        support: bounds('.workflow-pane .workflow-support'),
      };
    });
    assert(sessionLayout.pane.width >= 1600 && sessionLayout.measure.width >= 1200,
      `Runtime 2560 Session should use the wide canvas: ${JSON.stringify(sessionLayout)}`);
    assert(sessionLayout.support.x >= sessionLayout.signals.x + sessionLayout.signals.width,
      `Runtime 2560 evidence and execution should sit side by side: ${JSON.stringify(sessionLayout)}`);
    await noOverflow('Runtime light Session 2560');
    await themeSurfaceAudit('Runtime Session 2560', 'light');
    await screenshot('runtime-session-light-2560');
    report.checks.push('Runtime light Session wide layout at 2560 px');
    await page.evaluate(() => localStorage.setItem('webcodex.runtime.appearance.v1', 'dark'));
    await page.reload();
    await page.locator('.work-surface-control label').filter({ hasText: 'Sessions' }).click();
    await page.getByTestId('work-row-wc_sess_fixture470_active').click();
    await page.locator('.activity-signals-card').waitFor();
    await noOverflow('Runtime dark Session 2560');
    await themeSurfaceAudit('Runtime Session 2560', 'dark');
    await screenshot('runtime-session-dark-2560');
    const surface = await page.locator('.activity-signals-card').evaluate(el => getComputedStyle(el).backgroundColor.match(/\d+/g).slice(0, 3).map(Number));
    assert(Math.max(...surface) < 90, `Runtime dark Activity signals must use a dark surface at 2560 px: ${surface}`);
    report.checks.push('Runtime dark Activity signals at 2560 px');
    await page.locator('.app-nav .nav-button').filter({ hasText: 'Runtime' }).click();
    await page.getByRole('button', { name: /View activity/ }).click();
    await page.locator('.window-work-row').first().click();
    const windowLayout = await page.evaluate(() => {
      const workbench = document.querySelector('.window-primary-workbench').getBoundingClientRect();
      const selected = document.querySelector('.window-work-row.selected');
      return {
        width: workbench.width,
        background: getComputedStyle(selected).backgroundColor.match(/\d+/g).slice(0, 3).map(Number),
        text: getComputedStyle(selected).color.match(/\d+/g).slice(0, 3).map(Number),
      };
    });
    assert(windowLayout.width >= 1600, `Runtime 2560 Window workbench should use the wide canvas: ${JSON.stringify(windowLayout)}`);
    assert(Math.max(...windowLayout.background) < 100 && Math.min(...windowLayout.text) > 170,
      `Runtime dark selected Window must retain contrast: ${JSON.stringify(windowLayout)}`);
    await noOverflow('Runtime dark Window 2560');
    await themeSurfaceAudit('Runtime Window 2560', 'dark');
    await screenshot('runtime-window-dark-2560');
    report.checks.push('Runtime dark Window selection and wide layout at 2560 px');
    await context.close();
  }
  {
    const context = await prepare('/runtime/', 1900, 815);
    await page.locator('.sidebar-collapse').click();
    await page.locator('.work-surface-control label').filter({ hasText: 'Sessions' }).click();
    await page.getByTestId('work-row-wc_sess_fixture470_active').click();
    await page.locator('.active-command').waitFor();
    const execution = await page.evaluate(() => {
      const card = document.querySelector('.active-command');
      const heading = document.querySelector('.active-command-head strong');
      const timeline = document.querySelector('.progress-heading span');
      return {
        surface: getComputedStyle(card).backgroundColor.match(/\d+/g).slice(0, 3).map(Number),
        heading: getComputedStyle(heading).color.match(/\d+/g).slice(0, 3).map(Number),
        timelineHeight: timeline.getBoundingClientRect().height,
        timelineLineHeight: parseFloat(getComputedStyle(timeline).lineHeight),
      };
    });
    assert(Math.min(...execution.surface) > 225 && Math.max(...execution.heading) < 100,
      `Runtime light Current execution must use a readable light surface: ${JSON.stringify(execution)}`);
    assert(execution.timelineHeight <= execution.timelineLineHeight + 1,
      `Runtime 1900 Activity timeline heading should stay on one line: ${JSON.stringify(execution)}`);
    await themeSurfaceAudit('Runtime collapsed Session 1900', 'light');
    await noOverflow('Runtime collapsed Session 1900');
    await screenshot('runtime-session-light-1900');
    report.checks.push('Runtime light Current execution contrast and timeline heading at 1900 px');
    await context.close();
  }
  {
    const context = await prepare('/desktop/', 2560, 1229);
    await page.locator('.dashboard-page').waitFor();
    const homeWidth = await page.locator('.dashboard-page').evaluate(el => el.getBoundingClientRect().width);
    assert(homeWidth >= 1500, `Desktop 2560 Home should use the wide canvas: ${homeWidth}`);
    assert.equal(await page.getByRole('button', { name: 'Open Project alpha' }).count(), 1,
      'Desktop project row should expose a labeled Open action');
    await noOverflow('Desktop light Home 2560');
    await themeSurfaceAudit('Desktop Home 2560', 'light');
    await screenshot('desktop-home-2560');
    report.checks.push('Desktop light Home wide layout and labeled project action at 2560 px');
    await page.evaluate(() => localStorage.setItem('webcodex.desktop.appearance.v1', 'dark'));
    await page.reload();
    await page.locator('[data-webcodex-page="home"]').waitFor();
    await activeRail('.sidebar nav button.active .sidebar-selection', 'Desktop 2560');
    assert.equal(await page.locator('.sidebar-preferences select').count(), 0);
    await noOverflow('Desktop dark Home 2560');
    await themeSurfaceAudit('Desktop Home 2560', 'dark');
    await screenshot('desktop-dark-2560');
    await context.close();
  }
  {
    const context = await prepare('/runtime/', 924, 974);
    await page.evaluate(() => localStorage.setItem('webcodex.runtime.appearance.v1', 'dark'));
    await page.reload();
    await page.getByRole('progressbar', { name: 'Plan' }).waitFor();
    assert.equal(await page.locator('.sidebar-collapse').isVisible(), false, 'Compact Runtime sidebar should not show a redundant collapse button');
    assert.equal(await page.locator('.nav-button.active .nav-icon').isVisible(), true, 'Selected compact navigation icon must be visible');
    assert.equal(await page.locator('.nav-button.active .nav-label').isVisible(), false, 'Compact navigation label must be hidden');
    const compact = await page.locator('.app-nav').evaluate(nav => {
      const bounds = nav.getBoundingClientRect();
      const children = [...nav.querySelector('.nav-utilities').children].map(el => el.getBoundingClientRect());
      const icon = nav.querySelector('.nav-button.active .nav-icon').getBoundingClientRect();
      const brand = nav.querySelector('.brand-mark').getBoundingClientRect();
      return { center: bounds.x + bounds.width / 2, bottom: bounds.bottom,
        iconCenter: icon.x + icon.width / 2, brandCenter: brand.x + brand.width / 2,
        utilities: children.map(child => ({ top: child.top, bottom: child.bottom })) };
    });
    assert(Math.abs(compact.iconCenter - compact.center) <= 1 && Math.abs(compact.brandCenter - compact.center) <= 1,
      `Compact Runtime sidebar icons must be centered: ${JSON.stringify(compact)}`);
    assert(compact.utilities.every((item, index) => item.bottom <= compact.bottom && (!index || item.top >= compact.utilities[index - 1].bottom)),
      `Compact Runtime preferences must not overlap or leave the sidebar: ${JSON.stringify(compact)}`);
    await noOverflow('Runtime compact sidebar 924');
    await screenshot('runtime-compact-sidebar-dark-924');
    await page.locator('.app-nav .nav-button[title="Projects"]').click();
    await activeRail('.app-nav .nav-button.active .ui-selection-rail', 'Runtime compact Projects');
    await page.locator('.app-nav .nav-button[title="Work"]').click();
    report.checks.push('Runtime compact sidebar icon, brand, preferences, and navigation at 924 px');
    await context.close();
  }
  for (const width of [1440, 1280, 1024, 768, 390]) {
    const context = await prepare('/admin/', width, width === 390 ? 844 : 900);
    await page.getByRole('heading', { name: 'Administrator access' }).waitFor();
    await screenshot(`admin-gate-${width}`);
    await page.getByLabel('Admin token').fill('fixture-only-admin-token');
    await page.getByRole('button', { name: 'Unlock console' }).click();
    await page.locator('#overview-section .overview-item').first().waitFor();
    assert.equal(await page.locator('#projects-section .admin-data-table').count(), 1);
    report.checks.push(`Admin ${width} component project table`);
    await activeRail('.section-nav a[aria-current] .admin-nav-rail', `Admin ${width}`);
    const sectionEdges = await page.locator('#overview-section, #devices-section, #projects-section').evaluateAll(elements => elements.map(element => {
      const rect = element.getBoundingClientRect(); return [rect.left, rect.right];
    }));
    assert(sectionEdges.every(([left, right]) => Math.abs(left - sectionEdges[0][0]) <= 1 && Math.abs(right - sectionEdges[0][1]) <= 1), `Admin ${width}: section edges differ ${JSON.stringify(sectionEdges)}`);
    report.checks.push(`Admin ${width} section alignment`);
    if (width === 390) await longTextBounded('#projects-section tbody td:nth-child(4) code', 'Admin 390', true);
    if (width === 1440 || width === 390) {
      const picker = page.locator('.rail-footer .accent-picker');
      await accentChoice(picker, `Admin ${width}`, width === 1440 ? 'Teal' : 'Violet', true);
      await restoreDefaultAccent(picker);
    }
    if (width === 1440) await visibleKeyboardFocus(page.locator('.section-nav a').first(), 'Admin');
    await noOverflow(`Admin dashboard ${width}`);
    if (width === 1440) await themeSurfaceAudit('Admin dashboard', 'light');
    await screenshot(`admin-dashboard-${width}`);
    await page.getByRole('button', { name: 'Create project' }).click();
    await page.getByRole('dialog').waitFor();
    assert.equal(await page.evaluate(() => Boolean(document.activeElement?.closest('[role="dialog"]'))), true);
    report.checks.push(`Admin dialog focus containment ${width}`);
    await screenshot(`admin-dialog-${width}`);
    if (width === 390) {
      await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).scrollIntoViewIfNeeded();
      assert.equal(await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).isVisible(), true);
      await screenshot('admin-dialog-actions-390');
      report.checks.push('Admin mobile dialog actions reachable');
    }
    await page.keyboard.press('Escape');
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    assert.equal(await page.getByRole('button', { name: 'Create project' }).evaluate(el => document.activeElement === el), true);
    report.checks.push(`Admin dialog focus restoration ${width}`);
    if (width === 1440) {
      const actions = page.getByRole('button', { name: 'Actions for Fixture alpha' });
      await actions.click();
      await page.getByRole('menuitem', { name: 'Disable' }).click();
      await page.getByRole('dialog', { name: 'Disable project' }).waitFor();
      await page.keyboard.press('Escape');
      await page.getByRole('dialog').waitFor({ state: 'hidden' });
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Actions for Fixture alpha');
      report.checks.push('Admin project action menu, confirmation, and focus return');
    }
    await page.getByRole('button', { name: /Appearance:/ }).click();
    await page.getByRole('button', { name: /Appearance:/ }).click();
    assert.equal(await page.locator('html').getAttribute('data-resolved-theme'), 'dark');
    await noOverflow(`Admin dark ${width}`);
    if (width === 1440) await themeSurfaceAudit('Admin dashboard', 'dark');
    await page.evaluate(() => window.scrollTo({ top: 0, behavior: 'instant' }));
    await screenshot(`admin-dark-${width}`);
    if (width === 1440 || width === 390) {
      await page.locator('#projects-section').scrollIntoViewIfNeeded();
      await screenshot(`admin-projects-dark-${width}`);
    }
    if (width === 1440) {
      await page.reload();
      assert.equal(await page.locator('html').getAttribute('data-resolved-theme'), 'dark');
      report.checks.push('Admin dark appearance persists');
      await accessibilityPreferences(context, '.admin-rail', 'button', 'Admin');
    }
    await context.close();
  }
  assert.deepEqual(report.errors, [], 'Browser errors');
  report.passed = true;
} catch (error) {
  report.passed = false; report.failure = error.stack;
  if (page && !page.isClosed()) {
    await screenshot('failure').catch(() => {});
    fs.writeFileSync(path.join(output, 'failure-text.txt'), (await page.locator('body').innerText()).slice(0, 24000));
  }
  process.exitCode = 1;
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
