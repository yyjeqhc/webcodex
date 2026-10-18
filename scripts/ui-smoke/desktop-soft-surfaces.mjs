import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Actual production Desktop bundle, isolated loopback observations only. These
// fixtures never contact a native backend or exercise service/config effects.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = path.join(root, 'artifacts/desktop-soft-surfaces');
fs.mkdirSync(output, { recursive: true });
for (const name of fs.readdirSync(output)) if (name.endsWith('.png') || name === 'report.json') fs.unlinkSync(path.join(output, name));
const fixture = await startFixtureServer();
const report = { fixture: true, nativeBackend: false, widths: [1440, 1280, 1024, 780, 768, 390], checks: [], screenshots: [], geometry: [], errors: [], failures: [] };
const now = Date.now();
const longPath = '/fixture/团队项目/repository-with-a-long-readable-directory-name/'.repeat(3);
const projects = ['alpha', 'beta'].map(name => ({ id: `agent:fixture-runner:${name}`, name: `${name} — Desktop layout verification 团队项目`, path: `${longPath}${name}`, client_id: 'fixture-runner', connected: true, sessions: { active_sessions: 2, running_sessions: 1, latest_updated_at: Math.floor(now / 1000) } }));
const sessions = Array.from({ length: 4 }, (_, index) => ({ session_id: `fixture-session-${index}`, project_id: projects[index % 2].id, title: `Review Desktop preferences and verify long project metadata — 检查布局 ${index}`, updated_at: Math.floor(now / 1000) - index * 100, lifecycle: index === 2 ? 'closed' : 'active', running_call: index === 0, running_jobs: index === 1 ? 1 : 0, running_jobs_complete: true, overview: { attention: { open_todos: index === 0 ? 2 : 0, open_questions: index === 1 ? 1 : 0, open_risks: 0 }, reported_progress: { text: 'Checking the renderer with long but representative project metadata and multiple lines of context. 正在检查项目界面。', reported_at: Math.floor(now / 1000) }, validation: { state: 'unavailable', unresolved_failure_count: 0 } } }));
const windows = Array.from({ length: 10 }, (_, index) => ({ client_window_key: `fixture-window-${index}`, source: 'chatgpt', last_project: projects[index % 2].id, last_seen_at_ms: now - index * 30000, last_meaningful_activity_at_ms: now - index * 30000, active_count: index === 0 ? 1 : 0, linked_session_count: 1 }));
const details = Object.fromEntries(windows.map((row, index) => [row.client_window_key, { ...row, linked_sessions: [{ project: row.last_project, workflow_session_id: sessions[index % 4].session_id, title: sessions[index % 4].title }], active_requests: [], activity: [{ meaningful: true, tool_name: 'read_files', project: row.last_project, status: 'succeeded', started_at_ms: now - index * 30000 - 1000, ended_at_ms: now - index * 30000, request_observed_at_ms: now - index * 30000 - 1000, response_handed_at_ms: now - index * 30000, service_ms: 1000 }], sessions_truncated: false, activity_truncated: false }]));
let browser;
let currentScenario = 'setup';
function check(condition, label, detail = '') {
  if (!condition) report.failures.push({ scenario: currentScenario, message: `${label}${detail ? `: ${detail}` : ''}` });
  else report.checks.push(label);
}
async function capture(page, name) {
  await page.screenshot({ path: path.join(output, `${name}.png`), animations: 'disabled', caret: 'hide' });
  report.screenshots.push(`${name}.png`);
}
async function inspect(page, label) {
  const data = await page.evaluate(() => {
    const main = document.querySelector('.main-content');
    const content = main.querySelector('[data-webcodex-page]');
    const rect = content.getBoundingClientRect();
    const largeBlur = [...document.querySelectorAll('body *')].filter(el => {
      const r = el.getBoundingClientRect(), s = getComputedStyle(el);
      return r.width > 200 && r.height > 120 && s.backdropFilter !== 'none' && s.backdropFilter !== '';
    }).map(el => el.className);
    const active = document.querySelector('.sidebar nav [aria-current="page"]');
    const s = getComputedStyle(active);
    const pseudo = ['::before', '::after'].map(p => { const v = getComputedStyle(active, p); return { content: v.content, width: v.width, height: v.height, background: v.backgroundColor }; });
    const monochrome = value => { const n = value.match(/[\d.]+/g)?.map(Number); return !n || n.length < 3 || Math.max(...n.slice(0, 3)) - Math.min(...n.slice(0, 3)) <= 4; };
    const contrast = (first, second) => {
      const luminance = value => { const rgb = value.match(/[\d.]+/g).slice(0, 3).map(v => { const n = Number(v) / 255; return n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4; }); return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722; };
      const a = luminance(first), b = luminance(second); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
    return { viewport: innerWidth, document: document.documentElement.scrollWidth, main: main.clientWidth, scroll: main.scrollWidth, x: rect.x, width: rect.width, largeBlur, navigationIcons: [...document.querySelectorAll('.sidebar nav button')].map(el => ({ button: getComputedStyle(el).color, glyph: getComputedStyle(el.querySelector('.navigation-icon')).color })), selectedRows: [...document.querySelectorAll('[role=tab][aria-selected=true], .project-picker-option[aria-pressed=true]')].map(el => { const v = getComputedStyle(el); const labelColor = getComputedStyle(el.querySelector('strong') || el).color; return { className: el.className, labelColor, labelContrast: contrast(labelColor, v.backgroundColor), background: v.backgroundColor, color: v.color, neutral: monochrome(v.backgroundColor) && monochrome(v.color), entity: el.classList.contains('project-picker-option'), left: v.borderLeftWidth, right: v.borderRightWidth, top: v.borderTopWidth, bottom: v.borderBottomWidth }; }), selected: { bg: s.backgroundColor, color: s.color, radius: parseFloat(s.borderRadius), left: s.borderLeftWidth, right: s.borderRightWidth, pseudo }, neutral: monochrome(s.backgroundColor) && monochrome(s.color), dialogs: [...document.querySelectorAll('[role="dialog"]')].filter(el => el.getBoundingClientRect().width).map(el => { const r = el.getBoundingClientRect(); return { x: r.x, width: r.width, scroll: el.scrollWidth, client: el.clientWidth }; }) };
  });
  report.geometry.push({ label, ...data });
  check(data.document <= data.viewport + 1 && data.scroll <= data.main + 1, `${label} horizontal containment`, JSON.stringify(data));
  check(data.width <= 1440 + 1, `${label} bounded reading measure`, String(data.width));
  check(data.largeBlur.length === 0, `${label} no large blurred surfaces`, JSON.stringify(data.largeBlur));
  for (const icon of data.navigationIcons) check(icon.button === icon.glyph, `${label} navigation glyph inherits neutral label color`, JSON.stringify(icon));
  check(data.neutral, `${label} monochrome selected navigation`, JSON.stringify(data.selected));
  check(data.selected.radius >= 6 && data.selected.left === data.selected.right, `${label} rounded selection without edge rail`, JSON.stringify(data.selected));
  for (const p of data.selected.pseudo) check(p.content === 'none' || p.content === 'normal' || p.width === '0px' || p.height === '0px', `${label} no selected pseudo rail`, JSON.stringify(p));
  for (const row of data.selectedRows) check((row.entity || row.neutral) && row.left === row.right && row.top === row.bottom, `${label} selected category/entity treatment without rail`, JSON.stringify(row));
  for (const row of data.selectedRows) if (row.entity) check(row.labelContrast >= 4.5, `${label} selected entity actual label contrast >=4.5`, JSON.stringify(row));
  for (const dialog of data.dialogs) check(dialog.x >= -1 && dialog.x + dialog.width <= data.viewport + 1 && dialog.scroll <= dialog.client + 1, `${label} dialog containment`, JSON.stringify(dialog));
  return data;
}
async function navigate(page, destination) {
  await page.locator(`[data-webcodex-action="navigate-${destination}"]`).click();
  await page.locator(`[data-webcodex-page="${destination}"]`).waitFor();
}
async function tabs(page, ids, label, suffix) {
  let baseline;
  for (const id of ids) {
    const tab = page.locator(`#${id}`);
    await tab.click();
    check(await tab.getAttribute('aria-selected') === 'true', `${label} ${id} selected state`);
    const data = await inspect(page, `${label} ${id}`);
    const geometry = await tab.evaluate(el => {
      const list = el.parentElement, r = list.getBoundingClientRect();
      const values = [...list.querySelectorAll('[role="tab"]')].map(node => { const s = getComputedStyle(node); return { padding: s.padding, radius: s.borderRadius, size: s.fontSize, weight: s.fontWeight, height: node.getBoundingClientRect().height, width: node.getBoundingClientRect().width }; });
      return { x: r.x, width: r.width, overflow: list.scrollWidth > list.clientWidth + 1, values };
    });
    if (baseline) check(Math.abs(geometry.x - baseline.x) < 1 && Math.abs(geometry.width - baseline.width) < 1 && Math.abs(data.x - baseline.contentX) < 1, `${label} stable category geometry`, JSON.stringify({ baseline, geometry }));
    else baseline = { ...geometry, contentX: data.x };
    check(JSON.stringify(geometry.values.map(v => v.height)) === JSON.stringify(baseline.values.map(v => v.height)), `${label} tab heights remain stable across selection`, JSON.stringify(geometry.values));
    if (ids[0].startsWith('activity-')) {
      check(new Set(geometry.values.map(v => v.height)).size === 1, `${label} horizontal tabs share height`, JSON.stringify(geometry.values));
      check(Math.max(...geometry.values.map(v => v.width)) - Math.min(...geometry.values.map(v => v.width)) <= 1, `${label} horizontal tabs share width`, JSON.stringify(geometry.values));
    }
    check(!geometry.overflow, `${label} tabs have no horizontal scrollbar`);
    check(new Set(geometry.values.map(({ height, width, ...style }) => JSON.stringify(style))).size === 1, `${label} equal tab typography and padding`, JSON.stringify(geometry.values));
    await capture(page, `${id}-${suffix}`);
  }
}
async function dismissDialog(page, label, suffix) {
  const dialog = page.getByRole('dialog');
  await dialog.waitFor();
  await page.waitForFunction(() => [...document.querySelectorAll('[role=dialog]')].some(el => el.contains(document.activeElement)));
  check(await dialog.evaluate(el => el.contains(document.activeElement)), `${label} receives focus`);
  await inspect(page, label);
  await capture(page, `${label.replaceAll(' ', '-')}-${suffix}`);
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'hidden' });
}
async function preferences(page, suffix, theme) {
  const group = page.locator('.sidebar-preferences');
  const buttons = group.locator('button');
  check(await buttons.count() === 3 && await group.isVisible(), `${suffix} three visible preferences`);
  const sizes = await buttons.evaluateAll(nodes => nodes.map(node => { const r = node.getBoundingClientRect(); return [r.width, r.height]; }));
  check(sizes.every(([w, h]) => Math.abs(w - 36) <= 1 && Math.abs(h - 36) <= 1), `${suffix} compact 36px preferences`, JSON.stringify(sizes));
  const accent = group.locator('.appearance-preference-button').nth(0);
  await accent.click();
  let popup = page.locator('.appearance-preference-popover:visible');
  await popup.waitFor();
  await capture(page, `accent-palette-${suffix}`);
  const palette = await popup.boundingBox();
  check(palette.x >= -1 && palette.x + palette.width <= page.viewportSize().width + 1, `${suffix} accent palette containment`, JSON.stringify(palette));
  await popup.locator('button[aria-pressed]').nth(2).click();
  await popup.waitFor({ state: 'hidden' });
  await page.waitForFunction(() => document.activeElement === document.querySelector('.sidebar-preferences .appearance-preference-button'));
  check(await accent.evaluate(el => el === document.activeElement), `${suffix} accent preset returns focus`);
  const preset = await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1'));
  check(preset === '#0f766e', `${suffix} preset persisted`);
  await accent.click();
  popup = page.locator('.appearance-preference-popover:visible');
  const custom = popup.locator('input');
  await custom.fill('#a124bd');
  check(await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1')) === '#a124bd', `${suffix} custom accent persisted`);
  await custom.fill('invalid');
  await page.keyboard.press('Tab');
  check(await page.evaluate(() => localStorage.getItem('webcodex.ui.accent.v1')) === '#a124bd', `${suffix} invalid accent retains saved preference`);
  await page.keyboard.press('Escape');
  await popup.waitFor({ state: 'hidden' });
  await inspect(page, `${suffix} custom accent keeps navigation neutral`);
  const appearance = group.locator('.appearance-preference-button').nth(1);
  await appearance.focus();
  await page.keyboard.press('Enter');
  const nextTheme = await page.evaluate(() => document.documentElement.dataset.appearance);
  check(nextTheme === (theme === 'light' ? 'dark' : 'system'), `${suffix} appearance cycles`);
  const language = group.locator('[data-webcodex-control="locale"]');
  await language.click();
  const menu = page.getByRole('menu');
  await menu.waitFor();
  await capture(page, `language-menu-${suffix}`);
  const menuBounds = await menu.boundingBox();
  check(menuBounds.x >= -1 && menuBounds.x + menuBounds.width <= page.viewportSize().width + 1, `${suffix} language menu containment`, JSON.stringify(menuBounds));
  await menu.getByRole('menuitemradio', { name: '简体中文', exact: true }).click();
  await page.waitForFunction(() => document.documentElement.lang === 'zh-CN');
  await page.waitForFunction(() => document.activeElement === document.querySelector('.sidebar-preferences [data-webcodex-control=locale]'));
  check(await language.evaluate(el => el === document.activeElement), `${suffix} language returns focus`);
  await capture(page, `preferences-${suffix}`);
  // The fixture normally supplies English on every document load. Honor the
  // persisted locale in this isolated context so reload tests the real hook.
  await page.reload();
  await page.locator('[data-webcodex-page="home"]').waitFor();
  await page.waitForFunction(value => document.documentElement.dataset.appearance === value && document.documentElement.dataset.accent === 'custom' && document.documentElement.lang === 'zh-CN', nextTheme);
  check(await page.evaluate(() => document.documentElement.dataset.appearance) === nextTheme, `${suffix} appearance survives reload`);
  check(await page.evaluate(() => document.documentElement.dataset.accent) === 'custom', `${suffix} custom accent survives reload`);
  check(await page.evaluate(() => document.documentElement.lang) === 'zh-CN', `${suffix} locale survives reload`);
}
try {
  const executablePath = process.env.WEBCODEX_SMOKE_BROWSER;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  for (const width of report.widths) for (const theme of ['light', 'dark']) for (const locale of ['en-US', 'zh-CN']) {
    const suffix = `${width}-${theme}-${locale}`;
    currentScenario = suffix;
    const context = await browser.newContext({ viewport: { width, height: 960 }, colorScheme: theme, reducedMotion: 'reduce' });
    await context.addInitScript(theme => { if (!localStorage.getItem('webcodex.desktop.appearance.v1')) localStorage.setItem('webcodex.desktop.appearance.v1', theme); }, theme);
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => report.errors.push({ scenario: suffix, message: error.message }));
    page.on('console', msg => { if (msg.type() === 'error') report.errors.push({ scenario: suffix, message: msg.text() }); });
    await page.route('**/__fixture/desktop-state*', async route => {
      const response = await route.fetch(); const state = await response.json();
      state.workspace_runner = { client_id: 'fixture-runner', config_path: `${longPath}runner.toml`, server_url: 'http://127.0.0.1:1' };
      await route.fulfill({ response, json: state });
    });
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch(); let source = await response.text();
      const marker = "if(request.kind==='overview')return";
      assert(source.includes(marker), 'fixture workspace marker');
      const override = `if(request.kind==='overview')return ${JSON.stringify({ client_id: 'fixture-runner', connected: true, status: 'online', runners: [{ client_id: 'fixture-runner', connected: true, status: 'online' }], projects_available: true, visible_projects: projects.length, projects, projects_truncated: false, recent_sessions: { sessions, truncated: false, scan_truncated: false } })};if(request.kind==='projects')return ${JSON.stringify({ projects, total: projects.length, truncated: false })};if(request.kind==='windows')return ${JSON.stringify({ windows })};if(request.kind==='window')return ${JSON.stringify(details)}[request.client_window_key];`;
      source = source.replace(marker, override + marker);
      const inventoryEntry = (id, kind = 'file') => ({ id, component: id.split('.')[0], purpose: id.replaceAll('.', '_'), source: 'configured', configured_path: longPath + id, canonical_path: longPath + id, directory_to_open: longPath, status: 'present', kind, category: 'mixed_configuration', log_source: null });
      const inventory = { schema_version: 1, observed_at_ms: now, environment_id: null, local_server: true, local_runner: true, service_scope: 'user', revision: 'fixture-inventory-1', roots: [inventoryEntry('environment.root', 'directory'), inventoryEntry('desktop.root', 'directory')], entries: ['server.configuration', 'runner.configuration', 'desktop.locale', 'tunnel.profiles'].map(id => inventoryEntry(id)), issues: [], builds: [], identities: [] };
      source = source.replace("case 'get_runner_settings':", `case 'get_path_inventory':return ${JSON.stringify(inventory)};case 'get_runner_settings':`);
      source = source.replace("case 'get_launch_at_login':return false;", "case 'desktop_shell_restore_only':return false;case 'read_desktop_navigation':return null;case 'set_desktop_locale':case 'write_desktop_navigation':return null;case 'get_local_update_status':return {environment_id:null,selection_revision:1,installed_observed:false,installed_checked_at_ms:null,observation_error:false,confirmation:null,running:[],view:{schema_version:1,download:update.download,installed:[],candidate:null,candidate_components:[],upgrade:null,blockers:['unsupported_installation'],restart_required:false}};case 'get_launch_at_login':return false;");
      source = source.replace("localStorage.setItem('webcodex.desktop.locale','en-US');", `if(!localStorage.getItem('webcodex.desktop.locale'))localStorage.setItem('webcodex.desktop.locale',${JSON.stringify(locale)});`);
      source = source.replace("effective_roots:['/fixture/home']", `effective_roots:[${JSON.stringify(longPath)}]`);
      await route.fulfill({ response, body: source });
    });
    try {
      await page.goto(fixture.url + '/desktop/');
      await page.locator('[data-webcodex-page="home"]').waitFor();
      await page.waitForFunction(({ theme, locale }) => document.documentElement.dataset.theme === theme && document.documentElement.lang === locale, { theme, locale });
      let contentX;
      for (const destination of ['home', 'projects', 'activity', 'connection', 'console', 'extensions', 'settings']) {
        await navigate(page, destination);
        const data = await inspect(page, `${suffix} ${destination}`);
        if (contentX === undefined) contentX = data.x;
        check(Math.abs(data.x - contentX) <= 1, `${suffix} ${destination} shared content left`, JSON.stringify({ expected: contentX, actual: data.x }));
        await capture(page, `${destination}-${suffix}`);
        if (destination === 'activity') {
          await page.locator('.activity-work-row').first().waitFor();
          await page.locator('.activity-work-row').first().click();
          await dismissDialog(page, 'activity-details', suffix);
          await tabs(page, ['windows', 'sessions', 'system'].map(id => `activity-tab-${id}`), `${suffix} activity`, suffix);
        }
        if (destination === 'connection') {
          await page.locator('[data-webcodex-page="connection"] .page-heading-row .primary-button').click();
          const field = page.locator('#connection-profile-api-key');
          await field.fill('fixture-write-only-key');
          check(await field.getAttribute('type') === 'password', `${suffix} connection credential masked`);
          await dismissDialog(page, 'connection-editor', suffix);
        }
        if (destination === 'extensions') {
          await tabs(page, ['codingAgents', 'sshResources', 'mcpProviders', 'nativePlugins', 'skills', 'instructions'].map(id => `extension-tab-${id}`), `${suffix} extensions`, suffix);
          await page.locator('.extension-project-preview .project-picker-trigger').click();
          const selected = page.locator('.project-picker-option[data-value="agent:fixture-runner:beta"]');
          await selected.click();
          await page.waitForFunction(() => window.__fixtureCalls.some(call => call.cmd === 'workspace_query' && call.args.request.kind === 'extensions' && call.args.request.project === 'agent:fixture-runner:beta'));
          await page.locator('.extension-project-preview .project-picker-trigger').click();
          check(await selected.getAttribute('aria-pressed') === 'true', `${suffix} selected project row`);
          await inspect(page, `${suffix} selected project popup`);
          await capture(page, `selected-project-${suffix}`);
          await page.keyboard.press('Escape');
          await page.locator('#extension-tab-nativePlugins').click();
          await page.locator('.plugin-registration > button').click();
          await dismissDialog(page, 'plugin-editor', suffix);
        }
        if (destination === 'settings') await tabs(page, ['general', 'access', 'network', 'runtime', 'configuration', 'diagnostics', 'about'].map(id => `settings-tab-${id}`), `${suffix} settings`, suffix);
      }
      const mutations = await page.evaluate(() => window.__fixtureCalls.filter(call => /^(save_|update_|start_|stop_|restart_|remove_|register_|authorize_|request_computer_permission|environment_service_action|repair_environment_user_credential|copy_runtime_console_credential|open_diagnostic_resource|export_inventory_document|open_inventory_location)/.test(call.cmd)));
      check(mutations.length === 0, `${suffix} browsing preserves effect boundaries`, JSON.stringify(mutations));
      await preferences(page, suffix, theme);
    } catch (error) {
      report.failures.push({ scenario: suffix, message: error.message, stack: error.stack });
      await capture(page, `failure-${suffix}`);
    } finally {
      fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
      await context.close();
    }
  }
  assert.equal(report.errors.length, 0, `renderer errors: ${JSON.stringify(report.errors)}`);
  assert.equal(report.failures.length, 0, `Desktop failures (${report.failures.length}); see artifacts/desktop-soft-surfaces/report.json: ${JSON.stringify(report.failures.slice(0, 3))}`);
  console.log(JSON.stringify({ fixture: true, nativeBackend: false, scenarios: 24, checks: report.checks.length, screenshots: report.screenshots.length }));
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  if (browser) await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
