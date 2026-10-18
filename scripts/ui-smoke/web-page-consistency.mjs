import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { adminDashboard } from './fixtures.mjs';

// Production Web bundles, isolated loopback data. All apparent mutations below
// are intercepted synthetic replies; this harness never proxies a live backend.
const output = new URL('../../artifacts/web-page-consistency/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], failures: [], errors: [], limitations: [
  'English Chromium fixtures only; no native WebView, live backend, real model or credentials.',
  'Synthetic lifecycle dialogs are inspected without executing real project operations.',
  'State scenarios cover representative request boundaries, not every backend error code.',
  'Full Session record workflow/collaboration/inspector and trace payload details require separate focused checks.',
] };
const agent = { agent_id: 'consistency-reviewer', handle: 'reviewer', display_name: 'Fixture reviewer', description: 'Inspect workspace layout and retained evidence.', specialty_labels: ['Layout', 'Review'], profile_revision: 2, active_endpoint_count: 1, queued_delivery_count: 1 };
const conversation = { conversation_id: 'consistency-conversation', title: 'Fixture layout review', message_count: 1, last_seq: 1 };
const pause = page => page.waitForTimeout(250); // Allow Mantine portal transitions to finish before evidence capture.
function check(condition, description, context) {
  report.checks.push({ description, ...context, passed: Boolean(condition) });
  if (!condition) report.failures.push({ description, ...context });
}
async function scenario(context, action) {
  try { await action(); } catch (error) { report.failures.push({ ...context, error: error.message }); console.log(JSON.stringify({ ...context, error: error.message })); }
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
}
async function makePage(width, theme, height = 1000) {
  const page = await browser.newPage({ viewport: { width, height }, reducedMotion: 'reduce' });
  page.setDefaultTimeout(8000);
  page.on('pageerror', error => report.errors.push({ width, theme, error: error.message }));
  await page.addInitScript(theme => {
    localStorage.setItem('webcodex.runtime.appearance.v1', theme);
    localStorage.setItem('webcodex.admin.appearance.v1', theme);
  }, theme);
  await page.route('**/api/runtime-console/communication/**', route => {
    const name = route.request().url().split('/communication/')[1];
    const responses = {
      agents: { agents: [agent] }, conversations: { conversations: [conversation] },
      conversation: { conversation, messages: [{ message_id: 'fixture-message', seq: 1, body: 'Check layout at desktop and mobile sizes.', author: { participant_kind: 'human' } }] },
      'endpoint/attach': { endpoint: { agent_id: agent.agent_id, endpoint_id: 'fixture-endpoint', controller_generation: 7, lifecycle: 'attached', wake_capable: false } },
      inbox: { deliveries: [{ delivery_id: 'fixture-delivery', conversation_id: conversation.conversation_id, conversation_title: conversation.title, message: { body: 'Inspect the shared spacing rules.' } }] },
    };
    return route.fulfill({ json: responses[name] || {} });
  });
  await page.route('**/api/runtime-console/window-collaboration', route => route.fulfill({ json: { available: true, can_send: true, messages: [{ message_id: 'fixture-operator-message', source: 'operator', direction: 'inbound', message: 'Review the current layout.', created_at_ms: Date.now(), requires_ack: false }], truncated: false } }));
  return page;
}
async function capture(page, surface, width, theme) {
  await pause(page);
  const geometry = await page.evaluate(() => {
    const box = element => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom };
    };
    const visible = element => { const r = element.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
    const selectors = ['.app-content', '.page-heading', '.project-browser', '.runtime-page > .runtime-tabs', '.runtime-metrics', '.runtime-page > .runtime-section', '.agents-workbench', '.window-work-header', '.goal-detail-header', '.admin-workspace', '.workspace-toolbar', '.admin-section', '.table-wrap', '[role="dialog"]'];
    return {
      contentMax: Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--web-content-max')), viewport: innerWidth, rootWidth: document.documentElement.scrollWidth, bodyWidth: document.body.scrollWidth,
      boxes: Object.fromEntries(selectors.map(selector => [selector, [...document.querySelectorAll(selector)].filter(visible).map(element => ({ ...box(element), maxWidth: getComputedStyle(element).maxWidth, paddingInline: Number.parseFloat(getComputedStyle(element).paddingLeft) + Number.parseFloat(getComputedStyle(element).paddingRight) }))])),
      selections: [...document.querySelectorAll('.window-work-row[aria-current="true"], .goal-list-row.selected, .agent-row.selected, .project-card[aria-pressed="true"], .conversation-list button.selected')].filter(visible).map(element => ({ className: element.className, shadow: getComputedStyle(element).boxShadow, before: getComputedStyle(element, '::before').content, beforeShadow: getComputedStyle(element, '::before').boxShadow })),
      decorativeRails: document.querySelectorAll('.ui-selection-rail').length,
      goalHeader: (() => {
        const header = document.querySelector('.goal-detail-header');
        if (!header) return null;
        return { header: box(header), panel: box(header.parentElement), children: [...header.children].map(box) };
      })(),
    };
  });
  const context = { surface, width, theme };
  check(Math.max(geometry.rootWidth, geometry.bodyWidth) <= width + 1, 'No document horizontal overflow', context);
  check(geometry.decorativeRails === 0, 'No decorative selection rails', context);
  if (geometry.goalHeader) {
    const { header, panel, children } = geometry.goalHeader;
    check(header.x >= panel.x && header.right <= panel.right + 1, 'Goal header fits its content panel', context);
    check(children.every(child => child.x >= header.x && child.right <= header.right + 1 && child.y >= header.y && child.bottom <= header.bottom + 1), 'Goal title and metadata remain inside the header', context);
  }
  for (const selection of geometry.selections) check(!selection.shadow.includes('inset') && !selection.beforeShadow.includes('inset'), 'Selected entity has no inset stripe', { ...context, selection: selection.className });
  for (const [selector, boxes] of Object.entries(geometry.boxes)) for (const box of boxes) {
    check(box.x >= -1 && box.right <= width + 1, 'Surface stays within viewport', { ...context, selector });
    const maxWidth = Number.parseFloat(box.maxWidth);
    if (box.maxWidth.endsWith('px')) check(box.width <= maxWidth + 1, 'Declared content max-width respected', { ...context, selector, maxWidth });
  }
  if (geometry.contentMax) for (const selector of ['.page-heading', '.project-browser', '.runtime-page > .runtime-tabs', '.agents-workbench', '.admin-workspace']) for (const box of geometry.boxes[selector] || []) check(box.width - (selector === '.admin-workspace' ? box.paddingInline : 0) <= geometry.contentMax + 1, 'Shared Web content measure respected', { ...context, selector, contentMax: geometry.contentMax });
  const aligned = surface === 'projects' ? ['.page-heading', '.project-browser'] : surface.startsWith('runtime-') ? ['.page-heading', '.runtime-page > .runtime-tabs'] : null;
  if (aligned) {
    const [a, b] = aligned.map(selector => geometry.boxes[selector]?.[0]);
    if (a && b) check(Math.abs(a.x - b.x) <= 1 && Math.abs(a.right - b.right) <= 1, 'Page header and primary content share edges', context);
  }
  const admin = geometry.boxes['.workspace-toolbar']?.[0];
  if (admin) for (const section of geometry.boxes['.admin-section'].filter(box => !geometry.boxes['.admin-section'].some(other => other !== box && other.x < box.x && other.right > box.right))) {
    // Diagnostics/activity are a two-column detail grid; compare full-width sections only.
    if (section.width > admin.width * .8) check(Math.abs(section.x - admin.x) <= 1 && Math.abs(section.right - admin.right) <= 1, 'Admin toolbar and full-width sections share edges', context);
  }
  report.checks.push({ ...context, geometry });
  await page.screenshot({ path: new URL(`${surface}-${width}-${theme}.png`, output).pathname, fullPage: true, animations: 'disabled' });
}
async function dialogFocus(page, context) {
  const dialog = page.getByRole('dialog');
  await dialog.waitFor(); await pause(page);
  const field = dialog.locator('input:not([readonly]),textarea').last();
  if (await field.count()) await field.focus(); else await dialog.getByRole('button').first().focus();
  await page.keyboard.press('Tab');
  check(await dialog.evaluate(element => element.contains(document.activeElement)), 'Dialog contains keyboard focus', context);
}
async function closeDialog(page, context, trigger) {
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  if (trigger) {
    await page.waitForFunction(() => !document.querySelector('[role="dialog"]'));
    await pause(page);
    check(await trigger.evaluate(element => element === document.activeElement), 'Dialog restores trigger focus', context);
  }
}
async function runtimePages(page, width, theme) {
  await page.goto(fixture.url + '/runtime/');
  await page.getByTestId('work-window-row-' + '4700'.repeat(16)).waitFor();
  await capture(page, 'work-window', width, theme);
  await page.locator('.window-header-details > summary').click();
  await capture(page, 'work-window-details', width, theme);
  await page.locator('.window-center-tabs').getByRole('tab', { name: 'Collaboration', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message this Window', exact: true }).waitFor();
  await capture(page, 'work-collaboration', width, theme);
  await page.locator('.window-center-tabs').getByRole('tab', { name: /^Window/ }).click();
  await page.locator('.work-surface-control label').filter({ hasText: 'Goals' }).click();
  await page.getByRole('progressbar', { name: 'Plan', exact: true }).waitFor();
  await capture(page, 'work-goal', width, theme);
  const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
  await nav.getByRole('button', { name: /^Projects/ }).click();
  const beta = page.getByTestId('project-card-agent:fixture-runner:beta');
  await beta.click(); await page.getByRole('heading', { name: 'Fixture beta', exact: true }).waitFor();
  if (width <= 900) {
    await page.waitForFunction(() => document.activeElement === document.querySelector('.project-family-details'));
    check(await page.locator('.project-family-details').evaluate(element => element === document.activeElement), 'Mobile project detail receives focus', { width, theme });
  }
  await page.getByRole('button', { name: 'Check branch', exact: true }).click();
  await page.getByText('codex/fixture-ui', { exact: true }).waitFor();
  await capture(page, 'projects', width, theme);
  const add = page.getByRole('button', { name: 'Add Project', exact: true });
  await add.click(); await dialogFocus(page, { surface: 'project-add', width, theme });
  await capture(page, 'project-add', width, theme);
  await closeDialog(page, { surface: 'project-add', width, theme }, add);
  await nav.getByRole('button', { name: /^Runtime/ }).click();
  await page.getByRole('heading', { name: 'Runner fleet', exact: true }).waitFor();
  await capture(page, 'runtime-overview', width, theme);
  await page.locator('.runtime-diagnostics > summary').click();
  await capture(page, 'runtime-diagnostics', width, theme);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await page.getByRole('heading', { name: agent.display_name, exact: true }).waitFor();
  await page.getByRole('button', { name: 'Connect as this Agent', exact: true }).click();
  await page.getByRole('button', { name: 'Acknowledge', exact: true }).waitFor();
  await capture(page, 'runtime-agent-inbox', width, theme);
  await page.getByRole('tab', { name: 'All conversations', exact: true }).click();
  await page.getByText('Check layout at desktop and mobile sizes.', { exact: true }).waitFor();
  await capture(page, 'runtime-agent-conversations', width, theme);
  await page.getByRole('tab', { name: 'Profile', exact: true }).click();
  await page.getByText('Technical details', { exact: true }).click();
  await capture(page, 'runtime-agent-profile', width, theme);
  await page.locator('.agent-create-disclosure > summary').click();
  await capture(page, 'runtime-agent-create', width, theme);
  if (width <= 700) await page.getByRole('button', { name: 'Preferences', exact: true }).click();
  await page.getByRole('button', { name: 'Lock', exact: true }).click();
  await page.getByRole('heading', { name: 'Connect to your workspace', exact: true }).waitFor();
  await page.locator('.auth-advanced > summary').click();
  await capture(page, 'runtime-auth', width, theme);
}
async function adminPages(page, width, theme) {
  await page.goto(fixture.url + '/admin/');
  await page.getByRole('heading', { name: 'Administrator access', exact: true }).waitFor();
  await capture(page, 'admin-auth', width, theme);
  await page.getByLabel('Admin token', { exact: true }).fill('fixture-only-not-a-secret');
  await page.getByRole('button', { name: 'Unlock console', exact: true }).click();
  await page.getByRole('heading', { name: 'Operations', exact: true }).waitFor();
  for (const [section, id] of [['Overview', 'overview-section'], ['Runners', 'devices-section'], ['Projects', 'projects-section'], ['Diagnostics', 'diagnostics-section']]) {
    await page.getByRole('link', { name: section, exact: true }).click();
    await pause(page);
    check(await page.locator(`#${id}`).isVisible(), 'Admin section is reachable', { width, theme, section });
    await capture(page, 'admin-' + section.toLowerCase(), width, theme);
  }
  await page.evaluate(() => scrollTo(0, 0)); await pause(page);
  check(await page.locator('.section-nav a[aria-current]').getAttribute('href') === '#overview-section', 'Admin selection follows visible location', { width, theme });
  for (const name of ['Register', 'Create project']) {
    const trigger = page.getByRole('button', { name, exact: true });
    await trigger.click(); await dialogFocus(page, { surface: 'admin-' + name, width, theme });
    await capture(page, 'admin-' + name.toLowerCase().replaceAll(' ', '-'), width, theme);
    await closeDialog(page, { surface: 'admin-' + name, width, theme }, trigger);
  }
  for (const name of ['Enable', 'Disable', 'Unregister']) {
    await page.getByRole('button', { name: 'Actions for Fixture alpha', exact: true }).click();
    await page.getByRole('menuitem', { name, exact: true }).click();
    await dialogFocus(page, { surface: 'admin-' + name, width, theme });
    await capture(page, 'admin-' + name.toLowerCase(), width, theme);
    await closeDialog(page, { surface: 'admin-' + name, width, theme });
  }
}
async function shortModal(theme) {
  const page = await makePage(390, theme, 600);
  try {
    await page.goto(fixture.url + '/admin/');
    await page.getByLabel('Admin token', { exact: true }).fill('fixture-only-not-a-secret');
    await page.getByRole('button', { name: 'Unlock console', exact: true }).click();
    await page.getByRole('heading', { name: 'Operations', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Create project', exact: true }).click();
    await page.getByRole('dialog').waitFor(); await pause(page);
    const dialog = page.getByRole('dialog');
    await dialog.hover(); await page.mouse.wheel(0, 1400); await pause(page);
    const reachable = await page.getByRole('button', { name: 'Continue', exact: true }).evaluate(element => {
      const r = element.getBoundingClientRect();
      let clipTop = 0, clipBottom = innerHeight;
      for (let parent = element.parentElement; parent; parent = parent.parentElement) {
        if (/(auto|scroll|hidden|clip)/.test(getComputedStyle(parent).overflowY)) {
          const box = parent.getBoundingClientRect(); clipTop = Math.max(clipTop, box.top); clipBottom = Math.min(clipBottom, box.bottom);
        }
      }
      return { top: r.top, bottom: r.bottom, clipTop, clipBottom, reachable: r.top >= clipTop && r.bottom <= clipBottom + 1 };
    });
    check(reachable.reachable, 'Short mobile modal actions reachable by ordinary wheel', { width: 390, height: 600, theme, ...reachable });
    await capture(page, 'admin-create-short', 390, theme);
  } finally { await page.close(); }
}
async function stateScenario(width, theme, state) {
  const page = await makePage(width, theme);
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  try {
    if (state.startsWith('admin-')) {
      const mode = state.slice(6);
      let calls = 0;
      await page.route('**/api/admin/dashboard', async route => {
        if (mode === 'loading') await pending;
        if (mode === 'failure') return route.fulfill({ status: 500, json: { error: 'fixture-unavailable' } });
        const data = mode === 'empty' ? {} : structuredClone(adminDashboard);
        if (mode === 'stale' && calls++ > 0) data.section_status = Object.fromEntries(['overview', 'devices', 'projects', 'activity'].map(section => [section, { status: 'error', error: 'Fixture section unavailable; previous data retained.' }]));
        return route.fulfill({ json: data });
      });
      await page.goto(fixture.url + '/admin/');
      await page.getByLabel('Admin token', { exact: true }).fill('fixture-only-not-a-secret');
      await page.getByRole('button', { name: 'Unlock console', exact: true }).click();
      if (mode === 'loading') {
        await pause(page);
        check(await page.getByRole('button', { name: /Unlock|Unlocking|Connecting/ }).evaluate(element => element.getAttribute('aria-busy') === 'true' || element.hasAttribute('data-loading') || element.disabled), 'Admin unlock visibly pending', { state, width, theme });
      } else if (mode === 'failure') {
        await page.getByRole('alert').waitFor();
        check(await page.getByRole('alert').isVisible(), 'Initial Admin failure visible at gate', { state, width, theme });
      } else {
        await page.getByRole('heading', { name: 'Operations', exact: true }).waitFor();
        if (mode === 'stale') {
          await page.getByRole('button', { name: 'Refresh', exact: true }).click();
          await page.getByRole('alert').first().waitFor();
          check(await page.getByText('Fixture Runner', { exact: true }).isVisible(), 'Stale Admin preserves previous fleet data', { state, width, theme });
        } else check(await page.getByText('No projects registered.', { exact: true }).isVisible(), 'Admin empty state explains missing projects', { state, width, theme });
      }
    } else if (state.startsWith('agents-')) {
      const mode = state.slice(7);
      await page.addInitScript(() => localStorage.setItem('webcodex.runtime.v2.view.v1', 'runtime'));
      await page.route('**/api/runtime-console/communication/agents', async route => {
        if (mode === 'loading') await pending;
        return mode === 'failure' ? route.fulfill({ status: 500, json: { error: 'fixture-unavailable' } }) : route.fulfill({ json: { agents: [], returned: 0, total: 0, truncated: false } });
      });
      await page.goto(fixture.url + '/runtime/');
      await page.getByRole('tab', { name: 'Agents', exact: true }).click();
      await page.getByTestId('agents-workbench').waitFor();
      if (mode === 'empty') await page.getByText('No durable Agents are visible.', { exact: true }).waitFor();
      else if (mode === 'loading') {
        await pause(page);
        check(await page.getByText(/Loading.*Agent|Loading…/).count() > 0, 'Agent inventory visibly loading', { state, width, theme });
        check(await page.getByText('No durable Agents are visible.', { exact: true }).count() === 0, 'Pending Agent inventory does not claim empty', { state, width, theme });
      } else {
        await page.getByText(/unavailable|failed|retry/i).first().waitFor();
        check(await page.getByText('No durable Agents are visible.', { exact: true }).count() === 0, 'Failed Agent inventory does not claim empty', { state, width, theme });
      }
    } else {
      await page.addInitScript(() => localStorage.setItem('webcodex.runtime.v2.view.v1', 'projects'));
      await page.route('**/api/runtime-console/projects', route => route.fulfill({ json: { projects: [], total: 0, truncated: false } }));
      await page.goto(fixture.url + '/runtime/');
      await page.getByRole('button', { name: 'Add Project', exact: true }).waitFor();
      await pause(page);
      check(await page.locator('[data-testid^="project-card-"]').count() === 0, 'Empty project inventory has no stale rows', { state, width, theme });
    }
    await capture(page, 'state-' + state, width, theme);
  } finally { release(); await page.unrouteAll({ behavior: 'ignoreErrors' }); await page.close(); }
}
try {
  for (const width of [1440, 1280, 1024, 768, 390, 1920]) for (const theme of ['light', 'dark']) {
    const page = await makePage(width, theme);
    // Make every confirmation variant reachable using synthetic admitted actions.
    await page.route('**/api/admin/dashboard', route => {
      const data = structuredClone(adminDashboard);
      data.projects[0].actions = { enable: true, disable: true, unregister: true };
      return route.fulfill({ json: data });
    });
    try {
      await scenario({ renderer: 'runtime', width, theme }, () => runtimePages(page, width, theme));
      await scenario({ renderer: 'admin', width, theme }, () => adminPages(page, width, theme));
    } finally { await page.close(); }
    console.log(JSON.stringify({ width, theme, completed: true, failures: report.failures.length }));
  }
  for (const theme of ['light', 'dark']) {
    await scenario({ surface: 'admin-create-short', width: 390, height: 600, theme }, () => shortModal(theme));
    for (const width of [1440, 390]) for (const state of ['admin-loading', 'admin-failure', 'admin-empty', 'admin-stale', 'agents-loading', 'agents-failure', 'agents-empty', 'projects-empty']) {
      await scenario({ state, width, theme }, () => stateScenario(width, theme, state));
    }
  }
} finally {
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
console.log(JSON.stringify({ checks: report.checks.length, failures: report.failures.length, errors: report.errors.length, output: output.pathname }));
assert.deepEqual(report.failures, [], 'Page consistency failures (see report.json)');
assert.deepEqual(report.errors, [], 'Renderer errors (see report.json)');
