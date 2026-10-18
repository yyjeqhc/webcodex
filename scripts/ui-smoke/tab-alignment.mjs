import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { webResponse, key } from './fixtures.mjs';

// --baseline records current defects without treating expected failures as a
// passing regression run. No real credentials, backend, or clipboard is used.
const baseline = process.argv.includes('--baseline');
const output = new URL(`../../artifacts/web-page-consistency/tabs/${baseline ? 'baseline' : 'verified'}/`, import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true, ignoreDefaultArgs: ['--hide-scrollbars'], args: ['--disable-features=OverlayScrollbar,OverlayScrollbars,FluentOverlayScrollbar'] });
const report = { baseline, fixture: true, nativeBackend: false, checks: [], snapshots: [], failures: [], errors: [], limitations: [
  'Chromium production bundle with synthetic fixtures; no native WebView or live backend evidence.',
  'Native scrollbar width is styled to 17px; a layout probe records whether classic scrollbar space is actually reserved.',
  'Chinese is sampled at 1920 and 390 pixels; English covers every width and sidebar collapse is sampled at 1440.',
  'Mobile Window switches are centered above fixed navigation before expanded-reference capture; viewport and document y are then both checked. Runtime viewport positions remain strict.',
] };
const runner = 'fixture-runner-' + 'canonical-runner-'.repeat(9);
const project = `agent:${runner}:` + 'canonical-project-'.repeat(8);
const directory = '/fixture/' + 'long-directory-name/'.repeat(13) + 'workspace';
const values = [key, runner, directory, project];
const labels = { en: ['Window ID', 'Runner ID', 'Directory', 'Project ID'], 'zh-CN': ['窗口 ID', '机器 ID', '目录', '项目 ID'] };
const agent = { agent_id: 'fixture-agent', handle: 'fixture', display_name: 'Fixture reviewer', description: 'Short available Agent workspace.', profile_revision: 1, active_endpoint_count: 0, queued_delivery_count: 0 };
const replacements = new Map([['fixture-runner', runner], ['agent:fixture-runner:alpha', project], ['/fixture/alpha', directory]]);
function replace(value) {
  if (typeof value === 'string') return replacements.get(value) || value;
  if (Array.isArray(value)) return value.map(replace);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, replace(v)]));
  return value;
}
function check(passed, description, context = {}) {
  const result = { passed: Boolean(passed), description, ...context };
  report.checks.push(result); if (!passed) report.failures.push(result);
}
function save() { fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2)); }
async function settle(page) { await page.waitForTimeout(120); }
async function measure(page, scope) {
  return page.evaluate(scope => {
    const box = element => { const r = element.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom }; };
    const root = document.querySelector(scope === 'runtime' ? '.runtime-page' : '.window-work-main');
    const tabs = root.querySelector(scope === 'runtime' ? ':scope > .runtime-tabs' : '.window-center-tabs');
    const heading = root.querySelector(scope === 'runtime' ? '.page-heading h1' : '.window-work-header h2');
    const page = document.querySelector('.runtime-page');
    return { scrollY: window.scrollY, rootScrollTop: root.scrollTop, headerScrollTop: root.querySelector('.window-work-header')?.scrollTop ?? null, documentHeadingY: heading.getBoundingClientRect().y + window.scrollY, documentTabsY: tabs.getBoundingClientRect().y + window.scrollY, heading: box(heading), tabs: box(tabs), buttons: [...tabs.querySelectorAll('[role="tab"]')].map(element => ({ ...box(element), label: element.textContent.trim(), selected: element.getAttribute('aria-selected') })),
      overflow: Math.max(document.body.scrollWidth, document.documentElement.scrollWidth) > innerWidth + 1,
      page: page ? { clientWidth: page.clientWidth, offsetWidth: page.offsetWidth, scrollHeight: page.scrollHeight, clientHeight: page.clientHeight, overflowY: getComputedStyle(page).overflowY, gutter: getComputedStyle(page).scrollbarGutter } : null };
  }, scope);
}
function compare(snapshot, reference, context) {
  check(snapshot.buttons.length === 2, 'Two-tab switch has exactly two tracks', context);
  check(Math.abs(snapshot.buttons[0].width - snapshot.buttons[1].width) <= .5, 'Two tab tracks have equal widths', { ...context, widths: snapshot.buttons.map(b => b.width) });
  check(!snapshot.overflow, 'No document horizontal overflow', context);
  if (reference) {
    check(Math.abs(snapshot.heading.x - reference.heading.x) <= 1, 'Heading horizontal position stays stable', { ...context, before: reference.heading.x, after: snapshot.heading.x });
    if (context.scope === 'runtime' || ['collaboration', 'window-return'].includes(context.phase)) {
      check(Math.abs(snapshot.heading.y - reference.heading.y) <= 1, 'Heading vertical position stays stable across tab changes', { ...context, before: reference.heading.y, after: snapshot.heading.y });
      check(Math.abs(snapshot.tabs.y - reference.tabs.y) <= 1, 'Tab switch vertical position stays stable across tab changes', { ...context, before: reference.tabs.y, after: snapshot.tabs.y });
      if (context.scope === 'window') {
        check(Math.abs(snapshot.documentHeadingY - reference.documentHeadingY) <= 1, 'Window heading document position does not reflow', { ...context, before: reference.documentHeadingY, after: snapshot.documentHeadingY });
        check(Math.abs(snapshot.documentTabsY - reference.documentTabsY) <= 1, 'Window switch document position does not reflow', { ...context, before: reference.documentTabsY, after: snapshot.documentTabsY });
      }
    }
    check(Math.abs(snapshot.tabs.x - reference.tabs.x) <= 1 && Math.abs(snapshot.tabs.width - reference.tabs.width) <= 1, 'Tab switch bounds stay stable', { ...context, before: reference.tabs, after: snapshot.tabs });
    snapshot.buttons.forEach((button, i) => check(Math.abs(button.width - reference.buttons[i].width) <= 1, 'Tab track width stays stable', { ...context, tab: i, before: reference.buttons[i].width, after: button.width }));
  }
}
async function snapshot(page, scope, phase, context, reference) {
  await settle(page);
  const geometry = await measure(page, scope);
  compare(geometry, reference, { ...context, scope, phase });
  report.snapshots.push({ ...context, scope, phase, geometry });
  await page.screenshot({ path: new URL(`${context.width}-${context.theme}-${context.language}${context.collapsed ? '-collapsed' : ''}-${scope}-${phase}.png`, output).pathname, animations: 'disabled' });
  return geometry;
}
async function metadata(page, context) {
  const facts = page.locator('.window-header-facts');
  for (let i = 0; i < values.length; i++) {
    const code = facts.locator('code').filter({ hasText: values[i] }).first();
    await code.waitFor();
    const field = await code.evaluate(element => {
      const dd = element.closest('dd'), dt = dd?.previousElementSibling;
      const owner = dd?.parentElement || element.closest('.window-header-fact');
      const box = element.getBoundingClientRect(), bounds = owner.getBoundingClientRect();
      const range = document.createRange(); range.selectNodeContents(element);
      return { value: element.textContent, label: dt?.tagName === 'DT' ? dt.textContent.trim() : null, semantic: Boolean(dd && dt?.tagName === 'DT' && dd.closest('dl')), x: box.x, right: box.right, valueTop: box.top, labelBottom: dt?.getBoundingClientRect().bottom ?? null, clientHeight: element.clientHeight, scrollHeight: element.scrollHeight, ownerX: bounds.x, ownerRight: bounds.right, client: element.clientWidth, scroll: element.scrollWidth, lines: range.getClientRects().length, copyLabel: element.parentElement.querySelector('button')?.getAttribute('aria-label') };
    });
    check(field.value === values[i], 'Metadata preserves exact canonical value', { ...context, field: i });
    check(field.labelBottom !== null && field.labelBottom <= field.valueTop + 1, 'Metadata label sits above its own full value', { ...context, field: i, labelBottom: field.labelBottom, valueTop: field.valueTop });
    check(field.semantic && field.label === labels[context.language][i], 'Metadata label is semantically paired and names the field accurately', { ...context, field: i, expected: labels[context.language][i], actual: field.label });
    check(field.x >= field.ownerX - 1 && field.right <= field.ownerRight + 1 && field.scroll <= field.client + 1 && field.scrollHeight <= field.clientHeight + 1, 'Long metadata stays within its own field', { ...context, field: i, geometry: field });
    if (i === 1 || i === 3) check(field.lines > 1, 'Long canonical ID wraps without shortening', { ...context, field: i, lines: field.lines });
    check(Boolean(field.copyLabel && field.copyLabel.includes(labels[context.language][i])), 'Copy access names the exact metadata field', { ...context, field: i, copyLabel: field.copyLabel });
    await code.locator('..').getByRole('button').click();
    check(await page.evaluate(value => window.__fixtureCopied.at(-1) === value, values[i]), 'Copy action emits exact canonical value', { ...context, field: i });
  }
}
async function run(context) {
  const page = await browser.newPage({ viewport: { width: context.width, height: 900 }, reducedMotion: 'reduce' });
  page.setDefaultTimeout(7000);
  page.on('pageerror', error => report.errors.push({ ...context, error: error.message }));
  let denied = true;
  try {
    await page.addInitScript(({ theme }) => {
      localStorage.setItem('webcodex.runtime.appearance.v1', theme);
      localStorage.setItem('webcodex.runtime.v2.view.v1', 'runtime');
      window.__fixtureCopied = [];
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async value => { window.__fixtureCopied.push(value); } } });
    }, context);
    // Change only the fixture's explicit language initializer before the app boots.
    await page.route('**/runtime/', async route => {
      const response = await route.fetch();
      await route.fulfill({ response, body: (await response.text()).replace('language.v1","en"', `language.v1","${context.language}"`) });
    });
    await page.route('**/api/runtime-console/**', route => {
      const endpoint = route.request().url().split('/api/runtime-console/')[1];
      if (endpoint === 'communication/agents') return denied ? route.fulfill({ status: 403, json: { error: 'fixture-denied' } }) : route.fulfill({ json: { agents: [agent] } });
      if (endpoint === 'communication/conversations') return route.fulfill({ json: { conversations: [] } });
      if (endpoint.startsWith('communication/')) return route.fulfill({ json: {} });
      if (endpoint === 'window-collaboration') return route.fulfill({ json: { available: true, can_send: true, messages: [], truncated: false } });
      const data = replace(webResponse(endpoint, route.request().postDataJSON()));
      if (endpoint === 'overview') data.runners = Array.from({ length: 20 }, (_, i) => ({ ...data.runners[0], client_id: `${runner}-${i}`, hostname: `fixture-${i}` }));
      if (endpoint === 'window') { data.active_count = 12; data.activity_returned = 37; }
      return route.fulfill({ json: data });
    });
    await page.goto(fixture.url + '/runtime/');
    // Styling the native scrollbar makes classic layout reproducible in headless
    // Chromium; unlike overflow:scroll this preserves the auto-scroll transition.
    await page.addStyleTag({ content: '::-webkit-scrollbar{width:17px;height:17px}' });
    const probe = await page.evaluate(() => { const element = document.createElement('div'); element.style.cssText = 'position:absolute;width:100px;height:100px;overflow:scroll'; document.body.append(element); const width = element.offsetWidth - element.clientWidth; element.remove(); return width; });
    check(probe >= 15, 'Classic scrollbar reserves measurable layout space', { ...context, probe });
    if (context.collapsed) await page.locator('.sidebar-collapse').click();
    await page.getByRole('heading', { name: context.language === 'en' ? 'Runner fleet' : 'Runner 集群', exact: true }).waitFor();
    const overview = await snapshot(page, 'runtime', 'overview-before', context);
    check(overview.page.scrollHeight > overview.page.clientHeight || context.width <= 700, 'Overview fixture requires vertical scrolling', context);
    const tabs = page.locator('.runtime-page > .runtime-tabs [role="tab"]');
    await tabs.nth(1).click();
    await page.locator('.agents-denied').waitFor();
    await snapshot(page, 'runtime', 'agents-denied', context, overview);
    await tabs.nth(0).click();
    await snapshot(page, 'runtime', 'overview-return', context, overview);
    denied = false;
    await tabs.nth(1).click();
    await page.getByRole('heading', { name: agent.display_name, exact: true }).waitFor();
    await snapshot(page, 'runtime', 'agents-available', context, overview);
    await tabs.nth(0).click();
    await snapshot(page, 'runtime', 'overview-final', context, overview);
    const nav = page.locator(context.width <= 700 ? '.mobile-primary-nav' : '.app-nav');
    await nav.getByRole('button', { name: context.language === 'en' ? /^Work/ : /^工作/ }).click();
    await page.locator('.window-work-header').waitFor();
    const window = await snapshot(page, 'window', 'collapsed', context);
    await page.locator('.window-header-details > summary').click();
    await metadata(page, context);
    // Copying a long mobile field can leave the switch behind the fixed bottom
    // navigation. Establish an unobscured click position before comparing y;
    // Playwright's automatic scroll-to-click is not a layout shift.
    if (context.width <= 700) {
      await page.locator('.window-center-tabs').evaluate(element => element.scrollIntoView({ block: 'center', inline: 'nearest' }));
      await settle(page);
    }
    const expanded = await snapshot(page, 'window', 'expanded', context, window);
    const windowTabs = page.locator('.window-center-tabs [role="tab"]');
    await windowTabs.nth(1).click();
    await snapshot(page, 'window', 'collaboration', context, expanded);
    await windowTabs.nth(0).click();
    await snapshot(page, 'window', 'window-return', context, expanded);
    await page.locator('.window-header-details > summary').click();
    await snapshot(page, 'window', 'collapsed-return', context, window);
  } catch (error) { report.failures.push({ ...context, error: error.message }); console.log(JSON.stringify({ ...context, error: error.message })); }
  finally { await page.close(); save(); }
}
try {
  for (const width of [2560, 1920, 1440, 1280, 1024, 768, 390]) for (const theme of ['light', 'dark']) await run({ width, theme, language: 'en', collapsed: false });
  for (const width of [1920, 390]) for (const theme of ['light', 'dark']) await run({ width, theme, language: 'zh-CN', collapsed: false });
  for (const theme of ['light', 'dark']) await run({ width: 1440, theme, language: 'en', collapsed: true });
} finally { save(); await browser.close(); await new Promise(resolve => fixture.server.close(resolve)); }
console.log(JSON.stringify({ baseline, assertions: report.checks.length, snapshots: report.snapshots.length, failures: report.failures.length, errors: report.errors.length, output: output.pathname }));
if (!baseline) { assert.deepEqual(report.failures, [], 'Tab/metadata regressions; see report.json'); assert.deepEqual(report.errors, [], 'Renderer errors; see report.json'); }
